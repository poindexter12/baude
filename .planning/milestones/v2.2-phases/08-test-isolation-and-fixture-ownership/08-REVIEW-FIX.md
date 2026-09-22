---
phase: 08
fixed_at: 2026-09-15T20:34:00Z
review_path: .planning/phases/08-test-isolation-and-fixture-ownership/08-REVIEW.md
iteration: 2
findings_in_scope: 4
fixed: 4
skipped: 0
status: all_fixed
---

# Phase 08: Code Review Fix Report

**Fixed at:** 2026-09-15
**Source review:** `.planning/phases/08-test-isolation-and-fixture-ownership/08-REVIEW.md` (iteration 2 re-review)
**Iteration:** 2

**Summary:**
- Findings in scope: 4 (WR-01 … WR-04 of the *current* review — numbering reset, these are not the iteration-1 warnings of the same names)
- Fixed: 4
- Skipped: 0
- Out of scope by instruction: IN-01, IN-02 (untouched)

Iteration 1's seven findings were confirmed closed by the re-review and were not
revisited. The `GitDisownsIt` open question was left exactly as escalated — no
`RoutedToGit` disposition, no new deletion path.

---

## Fixed Issues

### WR-01: `persist::real_home_dir()` was public and ungated

**Files modified:** `baude-core/src/persist.rs`
**Commit:** `7e6c5f1`

**Applied fix:** changed `pub fn real_home_dir()` to `fn real_home_dir()`. The
review offered "make it private" or "keep it public with a warning plus a
`real_home_dir_is_never_redirected` test"; private is the stronger of the two and
the one the fix's own `Cargo.toml` comments already claim, so it was taken. The
existing doc paragraph about the `/` fallback was preserved rather than replaced
by the review's snippet, and a second paragraph was added explaining why this
resolver is private while its `real_config_base` / `real_worktrees_base` siblings
are not: those two have a real external consumer (the leak scanner needs the real
tree), this one has only `home_dir()` two lines below it.

Verified as the only consumer before editing: `grep -rn real_home_dir` across the
workspace found exactly two source hits, the definition and `home_dir()`. The
module boundary now makes "every home resolution goes through the guarded
`home_dir`" a property `rustc` enforces rather than a convention.

### WR-02: the `matches!` guarding prune's exit code claimed an exhaustiveness it did not have

**Files modified:** `baude/src/main.rs`
**Commit:** `4f8537e`

**Applied fix:** replaced the `matches!` with a wildcard-free nested `match` over
`PruneDisposition` and all seven `RefusalReason` variants, and rewrote the comment
so it states *why* the longer form is used — `matches!` has an implicit wildcard
arm and does not participate in exhaustiveness checking, so a new variant would
compile silently onto the exit-0 side.

**Adapted from the review's snippet:** the suggested code wrote
`RefusalReason::NotRemovableNow`, `::ProofChanged` and `::GitdirPresent` as unit
patterns, but all three carry fields; they were written as `{ .. }` instead. The
suggestion also listed six variants in the false arm — the enum has seven.
`cargo check` accepting a wildcard-free match is itself the proof the list is now
complete.

### WR-03: a read-only preview could exit 1 while printing "Nothing was removed"

**Files modified:** `baude/src/main.rs`
**Commit:** `bcd12d6`

**Applied fix:** took the review's first option — `let failed = report.confirmed
&& …`. This is the minimal change that makes behaviour and the two written
contracts agree: `worktrees_help_text()`'s "Step one — preview. Reads only;
removes nothing" and "the run still exits 0". No new `RefusalReason` variant and
no new `WORKTREES_EXIT_*` code were introduced.

The rule is now exactly what `RemovalFailed` means — "a removal was attempted and
failed" — and only the confirmed path attempts one. The confirmed half is
unchanged and still pinned: `a_removal_that_failed_exits_nonzero` passes `--yes`,
so it exercises `confirmed == true` and still asserts exit 1 (confirmed passing in
the gate log, so the condition is not vacuous).

**Gap worth recording:** the preview-side exit-0 case is *not* pinned by a new
test, and this was a deliberate choice rather than an oversight. The only trigger
is `removal_gate`'s `symlink_metadata` failing with a non-`NotFound` error on the
unconfirmed path, which requires the fresh re-verification to classify the
candidate `Removable` *first* and the permissions to change *afterwards*, inside
the same run. Both the scan and the gate stat the same path in the same process,
so there is no deterministic fixture shape that produces it without a mid-run
hook. A test asserting it would have to fake the report, which the current inline
exit-code computation is not shaped to allow. See the note below.

### WR-04: `WORKTREES_EXIT_FAILED`'s doc comment still promised "Nothing was removed"

**Files modified:** `baude/src/main.rs`
**Commit:** `911f111`

**Applied fix:** rewrote the doc comment at the definition site. It now says the
constant does *not* promise an untouched tree, names the part-way removal as the
case that produces it, and explicitly separates the bail-early paths (unreadable
report, unparseable report, a report `prune_at` refuses) which genuinely do remove
nothing and say so in their own message. The review's point that the neighbouring
`WORKTREES_EXIT_USAGE` doc makes the stale claim read as intentional is addressed
directly — the new text warns against reading the two as the same guarantee.

`worktrees_help_text()` was checked and needed no change: it already reads "1
could not complete — a report that was rejected outright (nothing removed), or a
removal that failed part-way", which is accurate after WR-03.

---

## Verification

**Where it ran:** all gates ran inside an isolated git worktree
(`.claude/worktrees/rf-08-80445-…`, since removed) on a temporary branch, with a
scratchpad `CARGO_TARGET_DIR`. The project's own `target/` was never written. The
branch was fast-forwarded onto `gsd/phase-08-test-isolation-and-fixture-ownership`
afterwards, so the verified tree and the current tree are byte-identical — but the
gate runs themselves are not reproducible from the main checkout without re-running
them there.

All four gates were run after **each** of the four fixes (four full passes), each
gate's exit code captured directly rather than through a pipe:

| Gate | Result |
|------|--------|
| `cargo fmt --all --check` | pass ×4 |
| `cargo clippy --workspace --all-targets -- -D warnings` | pass ×4 |
| `cargo check --workspace` (shipped shape, no `test-support`) | pass ×4 |
| `cargo test --workspace` | pass ×4 — 528 tests, 0 failures |

A baseline `cargo check --workspace --all-targets` was run before the first edit
and was clean, so nothing above is a pre-existing failure being tolerated.

**Constraints honoured:** no real user root was read, written, observed or
deleted — the only home/config resolutions touched were made *less* reachable, and
the test suite's `assert_contained` guards would have panicked on an escape. No
`baude worktrees scan --prune` or any other removal command was run at any point;
the ~1433 known leaked candidates were never enumerated. Nothing was pushed, no
branch was switched, no PR was opened. The untracked `.gsd/`,
`.planning/milestone.lock`, `08-REVIEW.iter2.md` and `08-REVIEW-FIX.iter2.md` were
left untouched and uncommitted, as was the modified `08-REVIEW.md`.

---

## Note for a human (not a blocker)

WR-03's fix is correct but its preview half is unpinned, for the structural reason
given above. If that contract is worth a regression test, the cheap route is to
extract the exit-code computation out of `run_worktrees_prune` into a pure
`fn prune_exit_code(report: &PruneReport) -> i32` and unit-test it against two
hand-built reports (`confirmed: false` + `RemovalFailed` → 0, `confirmed: true` +
`RemovalFailed` → 1). That was **not** done here because the instruction was to
prefer the minimal change, and an extraction is a refactor rather than a fix.
Flagging it rather than deciding it.

---

_Fixed: 2026-09-15_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 2_
