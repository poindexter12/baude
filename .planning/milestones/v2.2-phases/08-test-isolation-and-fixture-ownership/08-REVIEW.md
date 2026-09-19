---
phase: 08-test-isolation-and-fixture-ownership
reviewed: 2026-09-15T21:12:00Z
depth: standard
iteration: 3
scope: convergence-check
files_reviewed: 3
files_reviewed_list:
  - baude-core/src/persist.rs
  - baude/src/main.rs
  - baude-core/src/worktree_scan.rs
findings:
  critical: 0
  warning: 0
  info: 0
  total: 0
status: clean
---

# Phase 8: Code Review Report (iteration 3 — convergence check)

**Reviewed:** 2026-09-15T21:12:00Z
**Depth:** standard
**Files Reviewed:** 3
**Status:** clean

## Summary

This is the final iteration of the `--auto` fix loop and a convergence check, not
a fresh review. Scope was exactly the four iteration-2 fixes (`7e6c5f1`,
`4f8537e`, `bcd12d6`, `911f111`) re-derived against current source, plus the
question of whether those fixes introduced anything new. `git diff --name-only
7e6c5f1~1..911f111` confirms the four commits touch exactly two files —
`baude-core/src/persist.rs` and `baude/src/main.rs`. `.github/workflows/ci.yml`
and `scripts/assert-real-roots-untouched.sh` are byte-identical to the tree
iteration 2 reviewed, so iteration-2's IN-01 and IN-02 carry forward unchanged
and are not re-raised per instruction. `baude-core/src/worktree_scan.rs` was read
as substrate — it is what the two edited files must agree with — not re-reviewed.

**All four fixes close their findings, and none introduces a new defect.**

**WR-01 / `7e6c5f1` — `persist::real_home_dir` is now private.** Confirmed at
`baude-core/src/persist.rs:900` (`fn real_home_dir()`, no `pub`). A
workspace-wide grep for `real_home_dir` across all `.rs` and `.toml` returns
exactly three hits: the definition, the one call from the guarded `home_dir()`
at `persist.rs:919`, and a mention inside the new doc paragraph. No `pub use`
re-export, no consumer in `baude` or `bauded`, and the item stays used so no
`dead_code` warning is suppressed or triggered. The claim the new doc makes about
its siblings is accurate rather than aspirational: `real_config_base` really does
have an external consumer (`baude/src/main.rs:283`), which is what distinguishes
it. The CR-01 invariant — every home resolution goes through the guarded
`home_dir()` — is now enforced by the module boundary, exactly as claimed. No
public item links to the now-private function, so rustdoc has no broken
intra-doc link.

**WR-02 / `4f8537e` — the exhaustive match is genuinely exhaustive.**
`PruneDisposition` (`worktree_scan.rs:1225-1241`) has five variants and
`RefusalReason` (`worktree_scan.rs:1198-1221`) has seven; the match at
`main.rs:1049-1067` names all five and all seven with no wildcard arm and no `_`
binding. Neither enum carries `#[non_exhaustive]` (grep across `baude-core/src`
returns no hits), which matters because `baude` is a downstream crate — without
that attribute the compiler's exhaustiveness check really does bind across the
crate boundary, so a new variant breaks the build rather than landing silently on
the exit-0 side. The fix report's correction of the review snippet is right:
`NotRemovableNow`, `ProofChanged` and `GitdirPresent` all carry fields and are
written `{ .. }`. Policy placement matches the stated line — only
`RemovalFailed` is a failure; `Removed`, `WouldRemove`, `NotApproved`,
`Unapproved` and the six safety refusals are exit-0.

**WR-03 / `bcd12d6` — the `confirmed` gate is correct and not over-broad.**
`PruneReport.confirmed` is propagated verbatim from `prune_at`'s `confirmed`
argument (`worktree_scan.rs:1381-1384`), which the CLI passes as `options.yes`
(`main.rs:1016`), so `report.confirmed` is exactly `--yes` and the gate cannot
drift from the flag it stands for. The preview path it protects is real:
`prune_one` at `worktree_scan.rs:1494-1506` runs `removal_gate` unconfirmed, and
`removal_gate:1523-1527` maps a non-`NotFound` `symlink_metadata` error to
`RemovalFailed` — the one way a read-only preview could have produced that
reason. The `&&` short-circuit skips a closure with no side effects. The
confirmed half is unchanged and still pinned, not vacuously: I ran
`a_removal_that_failed_exits_nonzero` with `--nocapture` and it passed without
emitting its root-bypass `skipped:` line, so the `assert_eq!(code,
WORKTREES_EXIT_FAILED)` genuinely executed. `a_safety_refusal_still_exits_zero`
passes alongside it. The unpinned preview-side exit-0 case is disclosed in the
fix report with a structural rationale (no deterministic fixture produces a
mid-run permission change between the scan and the gate) — a known documented
gap, not a defect.

**WR-04 / `911f111` — the doc string is now accurate.** Every claim it makes
checks out against the code: the two bail-early paths it names really do print
"Nothing was removed." (`main.rs:1000`, `main.rs:1012`), as does the
`prune_at`-refused arm (`main.rs:1074`), and `WORKTREES_EXIT_USAGE`'s stronger
"nothing was read or removed" is correctly held apart from it. The part-way
removal it describes is reachable, since `remove_verified` removes one candidate
and the loop continues. `worktrees_help_text()` (`main.rs:509-513`) agrees with
the new text rather than contradicting it.

**Gate evidence, re-run here rather than taken on report:** `cargo check
--workspace --all-targets` and `cargo clippy --workspace --all-targets -- -D
warnings` both exit 0 against the current tree (scratchpad `CARGO_TARGET_DIR`;
exit codes captured directly, not through a pipe). Targeted test runs pass:
6 worktrees/prune CLI cases, the 2 exit-code cases, and 13 `persist::` cases.
No `baude worktrees scan --prune` was run, no real root was read, written,
observed or removed, nothing was pushed, and no branch was switched.

One sub-finding-grade observation, recorded as a note rather than raised as a
finding per the convergence-check instruction: `WORKTREES_EXIT_FAILED`'s new doc
says "Everywhere else, the account printed above the exit is the record of what
actually happened", but the constant is also returned by two `run_worktrees_scan`
paths (`main.rs:947` on a `scan_at` error, `main.rs:967` on a serialization
error) that print no account. Both are read-only paths that remove nothing, so
the doc errs conservative and misleads nobody about deletion; the sentence is
simply scoped to prune in a comment attached to a shared constant. Not worth a
change on the final iteration. The dead `GitDisownsIt` clearing arm under CR-02's
Option A fix remains the known, accepted open question already escalated to the
user — unchanged by these four fixes, and deliberately not re-raised.

No new findings. The four iteration-2 findings are closed and the loop has
converged.

---

_Reviewed: 2026-09-15T21:12:00Z_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard (iteration 3, convergence check)_
