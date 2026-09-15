---
phase: 08
fixed_at: 2026-09-15T00:00:00Z
review_path: .planning/phases/08-test-isolation-and-fixture-ownership/08-REVIEW.md
iteration: 1
findings_in_scope: 7
fixed: 7
skipped: 0
status: all_fixed
---

# Phase 08: Code Review Fix Report

**Fixed at:** 2026-09-15
**Source review:** `.planning/phases/08-test-isolation-and-fixture-ownership/08-REVIEW.md`
**Iteration:** 1

**Summary:**
- Findings in scope: 7 (CR-01, CR-02, WR-01 … WR-05)
- Fixed: 7
- Skipped: 0
- Out of scope by instruction: IN-01 … IN-04 (untouched)

---

## Decisions that need a human

### The `GitDisownsIt` clearing arm is unreachable by design, and that is now pinned by a test

CR-02 was fixed with **Option A only**, per explicit instruction. `remove_verified`'s
symlink / not-a-directory / gitdir pre-checks are factored into a `removal_gate`, and
`prune_one` now runs that same gate on the *unconfirmed* path, so `WouldRemove` is
printed only when `--yes` would actually remove.

The consequence, stated plainly: **a candidate cleared by `Evidence::GitDisownsIt`
can never be removed by this tool.** `GitDisownsIt` requires a gitdir to be present
(that is what git is disowning), and `removal_gate` refuses every gitdir-bearing path
with `GitdirPresent`. So the clearing arm classifies, reports, and then always
refuses. Before this fix the disagreement was hidden: the preview said "would
remove", and only a confirmed run revealed the refusal.

That is the correct conservative behaviour, and it is now honest rather than
misleading — but it does leave one arm of the clearing predicate with no removal
path behind it. **Option B was NOT implemented**: no `RoutedToGit` disposition was
added, and no new path that deletes gitdir-bearing candidates exists. Expanding what
the tool is willing to delete is a user decision that has not been made.

The open question for a human: should a git-disowned, gitdir-bearing candidate be
(a) routed to `git worktree remove`, (b) removed directly, or (c) left permanently as
a reported-but-never-removed class — in which case the `GitDisownsIt` arm might be
better demoted from a clearing signal to an informational one, so the report stops
promising something the tool will not do.

### Deviations from the review's suggested fix

Two, both deliberate:

1. **CR-01 used an explicit `home_dir` field on `Redirects`**, not the review's
   `config_dir.parent().join("home")` derivation. `testing::Redirects` is built on
   "every path a fixture redirects, held as one value"; a derived home would be the
   only redirect that is not visible in the struct, and `TestRedirect::new`'s doc
   table would no longer be the complete list.
2. **WR-01 deleted `worktree_scan::scan()` rather than wiring the CLI through it.**
   T-08-25 requires the acting process to resolve the tree it acts on, so a core-side
   convenience wrapper that resolves real roots works against the phase's own
   constraint. The review offered both; the deletion is the one that does not
   reintroduce a second place a real root is resolved.

### Two findings the review did not list, closed alongside CR-01

- **A fourth unguarded home resolver**: `App::context_dir_label` (`baude/src/app.rs`)
  read `dirs::home_dir()` directly. Same escape class, same fix.
- **`dirs` was removed from `baude/Cargo.toml` and `bauded/Cargo.toml`.** Both crates
  are now wholly free of the unguarded crate, so a *fifth* private `expand_tilde`
  cannot be written there by accident. Each manifest carries a comment saying so.

---

## Fixed Issues

### CR-01: real/guarded home-resolver pair

**Files modified:** `baude-core/src/testing.rs`, `baude-core/src/persist.rs`,
`bauded/src/manager.rs`, `baude/src/app.rs`, `baude/src/ui.rs`, `baude/Cargo.toml`,
`bauded/Cargo.toml`, `scripts/assert-real-roots-untouched.sh`, `Cargo.lock`
**Commit:** `aee56c5`

**Applied fix:** Added `persist::real_home_dir()` (ungated, verbatim fallback) and
`persist::home_dir()` (redirect-aware, `assert_contained` on the real path), plus one
shared `persist::expand_tilde()`. Deleted the two byte-identical private
`expand_tilde` helpers in `baude` and `bauded` — carrying the same escape in two
crates is how it survived. `testing::Redirects` gained a `home_dir` field;
`TestRedirect::new` derives `<root>/home`; `testing::home_dir_override()` reads it.

Four call sites rerouted: `manager::create_session` (a `POST /sessions` body naming
`~/repo`), `app::complete_dir_path`, `app::submit_input` (both `NewSessionPath` and
`CloneDest` — the last hands a destination to a real `git clone`), `ui::tilde_path`,
and `app::context_dir_label`.

Five `#[should_panic(expected = "resolved to the real user path")]` probes added
(`app`, `ui`, `manager`) plus a positive case asserting a redirected `~/Code` lands at
`<root>/home/Code`.

`scripts/assert-real-roots-untouched.sh` gained the clone destination root:
`SCHEMA_VERSION` 1 → 2, a fourth root named from `clone_base_dir` (default `~/Code`),
a pure `expand_tilde` mirroring the Rust helper, a two-pass
`resolve_roots_from_disk` (the setting lives *inside* the config root), and
`ROOT_DEPTH = {"clone": 3}` — the clone root is the developer's whole code tree, so
the walk is bounded at `<base>/<host>/<owner>/<repo>` and a self-test row pins that
trade-off explicitly rather than leaving it implied.

### CR-02: `WouldRemove` promised removals the confirmed run would refuse

**Files modified:** `baude-core/src/worktree_scan.rs`
**Commit:** `343d297`

**Applied fix:** Option A. Extracted `removal_gate(path) -> Result<(), RefusalReason>`
holding the symlink / not-a-directory / gitdir checks; `remove_verified` runs it then
removes; `prune_one` runs it on the unconfirmed path and returns `Refused` instead of
`WouldRemove` when it fails. The gate is read-only — nothing in it creates, opens, or
follows anything — which is what makes it safe on the preview path.

Test added: `a_gitdir_bearing_candidate_is_refused_identically_with_and_without_yes`.
Constructing it needed care and the construction is documented in the test: every
entry beneath the candidate is a *directory* (a `.git` **file** would be a
non-directory entry, land on `ContainsCheckout`, and never reach the gate), while the
unusable `.git` directory makes git walk up to the fixture root and report a
repository whose inventory does not list the path. A new `ScanFixture::git_init_root`
supplies that ancestor. The test asserts the verdict really is `Removable` and the
evidence really is `GitDisownsIt` first, so it cannot pass vacuously.

**Verified as a regression test:** with the gate removed from `prune_one` the case
fails with `left: WouldRemove, right: Refused { GitdirPresent { … } }`.

### WR-01: `ScanRoots` halves living in different universes

**Files modified:** `baude-core/src/persist.rs`, `baude-core/src/worktree_scan.rs`,
`baude/src/main.rs`
**Commit:** `a46536c`

**Applied fix:** `persist::real_config_base()` made `pub` with the "ungated on
purpose — the leak scanner is production code whose job is the real root" rationale
`git::real_worktrees_base` already carries, plus an explicit "nothing that allocates,
writes, or removes may call this". `main.rs` now pairs `real_worktrees_base()` with
`real_config_base()`. The dead `worktree_scan::scan()` is deleted (confirmed zero
references; its doc comment "the only place a real root is resolved" was false) and
replaced with a comment recording why there is no wrapper.

### WR-02: a failed prune removal exited 0

**Files modified:** `baude/src/main.rs`
**Commit:** `af2ea4e`

**Applied fix:** `run_worktrees_prune` returns `WORKTREES_EXIT_FAILED` when any
outcome is `Refused { reason: RemovalFailed { .. } }`. The match is written without a
wildcard on the reason, because the line between "the tool working" and "the tool
failing" is a policy call that should break the build if a new `RefusalReason`
appears. Safety refusals (`GitdirPresent`, `BecameSymlink`, `ProofChanged`) stay
exit-0. `worktrees_help_text()` now states the distinction and no longer implies exit
1 always means "nothing removed" — a part-way failure can leave some candidates gone.

Two tests: `a_removal_that_failed_exits_nonzero` (parent made read-only after the
preview, so the gate clears and the `remove_dir` itself is what fails) and
`a_safety_refusal_still_exits_zero` (the other side of the same line). The first
probes whether the mode bits are actually enforced and declines with a printed note
under root rather than asserting something false.

### WR-03: `TestChildRoots::create()` swallowed every error, with a wrong rationale

**Files modified:** `baude-core/src/pty.rs`
**Commit:** `1fce023`

**Applied fix:** `create_dir_all` failures now panic naming the directory and the
error. The comment is corrected: `portable_pty`'s `get_home_dir` returns the
builder's `HOME` whenever that key is *present* and consults the passwd database only
when it is unset, and `configure_test_child` always sets it — so a missing directory
is **not** a containment failure, it is a broken fixture, and it aborts for the same
reason every other guard in this phase does.

### WR-04: the shipped branch let callers override the gate's own keys

**Files modified:** `baude-core/src/pty.rs`
**Commit:** `a8b1076`

**Applied fix:** The production branch of `build_gate_command` now applies caller
`env` FIRST and `TERM`/`COLORTERM`/`BAUDE_GATE_*` LAST, matching the support branch.
The ordering-policy comment moved from `configure_test_child` up to
`build_gate_command`, where it now describes both branches, and records why it
mattered: `GATE_SCRIPT` execs `"$BAUDE_GATE_SHELL" … -c "$BAUDE_GATE_COMMAND"`.

The existing `worker_isolation_pty_child_environment` case was extended with hostile
`BAUDE_GATE_COMMAND` / `BAUDE_GATE_SHELL` / `BAUDE_GATE_TOKEN` / `BAUDE_GATE_MODE`
entries and assertions that none of them wins. **Note the limit:** that test can only
reach the `cfg(any(test, feature = "test-support"))` branch. The production branch is
compile-time excluded from every test binary, which is exactly why it drifted; it is
covered by `cargo check --workspace` (no `--all-targets`, so no dev-dependency
feature unification), which was run and is clean.

### WR-05: a symlinked snapshot directory defeated the own-state overlap check

**Files modified:** `scripts/assert-real-roots-untouched.sh`
**Commit:** `2c223b3`

**Applied fix:** `is_inside` resolves both operands with `os.path.realpath` before the
lexical comparison, with a docstring marking it as the deliberate single exception to
the script's never-follow rule (everything else uses `os.lstat`, because there the
subject is the link itself; here the question is the opposite one). `realpath` does
not raise on a non-existent path, so the check still works before the snapshot
directory exists.

Three self-test rows added: a snapshot directory named outside every root that
resolves through a link *into* one (must FATAL), the FATAL line named specifically
rather than a bare substring that the ordinary listing would also match, and — the
other side of the line — a symlinked snapshot directory genuinely outside every root
that must still be accepted, so "normalize both operands" cannot be satisfied by a
check that refuses everything.

**Verified as a regression test:** with the two `realpath` lines removed,
`a symlinked snapshot directory is refused` fails.

---

## Verification

**Where it ran:** an isolated git worktree at
`.claude/worktrees/rf-08-86184-1789497742` on temp branch `gsd-reviewfix/08-86184`,
fast-forwarded into `gsd/phase-08-test-isolation-and-fixture-ownership` on teardown.
Cargo used a scratchpad `CARGO_TARGET_DIR`, so the main checkout's `target/` was
never touched. A re-run from the main checkout will recompile from cold but should be
identical.

| Gate | Result |
|------|--------|
| `cargo check --workspace --all-targets` | clean (run after every edit batch) |
| `cargo check --workspace` (shipped shape, no `test-support`) | clean — this is the one that compiles WR-04's branch |
| `cargo test --workspace` | **528 passed, 0 failed**, exit 0 |
| `cargo clippy --workspace --all-targets -- -D warnings` | clean |
| `cargo fmt --all --check` | clean (one rustfmt diff in the CR-02 helper, fixed in `2e4ce15`) |
| `bash scripts/assert-real-roots-untouched.sh --self-test` | **40 checks, 0 failures**, exit 0 |

Two of the new tests were additionally proven to FAIL with their fix reverted
(CR-02's agreement case, WR-05's symlinked-snapshot row), so neither is a test that
would pass against the bug.

**Constraints honoured:** no real user root was read, written, or observed — the
observer script was run in `--self-test` mode only, which uses synthetic roots and an
explicit child environment. `baude worktrees scan --prune` was never run. Nothing was
pushed, no branch was switched, no PR opened. Untracked `.gsd/` and
`.planning/milestone.lock` are untouched.

**Commit hygiene:** `git status --porcelain` is empty; every fix is its own commit;
`08-REVIEW-FIX.md` is deliberately left uncommitted for the orchestrator.

One extra commit beyond the seven findings: `2e4ce15`, a rustfmt-only fix to the
CR-02 fixture helper. It is separate rather than amended into `343d297` because
amending a mid-stack commit means a rebase.

---

_Fixed: 2026-09-15_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
