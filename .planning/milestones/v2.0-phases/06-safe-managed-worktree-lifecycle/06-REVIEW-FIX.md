---
phase: 06-safe-managed-worktree-lifecycle
fixed_at: 2026-08-31T02:41:34Z
review_path: .planning/phases/06-safe-managed-worktree-lifecycle/06-REVIEW.md
iteration: 1
findings_in_scope: 3
fixed: 0
skipped: 3
status: none_fixed
---

# Phase 6: Code Review Fix Report

**Fixed at:** 2026-08-31T02:41:34Z
**Source review:** `.planning/phases/06-safe-managed-worktree-lifecycle/06-REVIEW.md`
**Iteration:** 1

**Summary:**
- Findings in scope: 3
- Fixed: 0
- Skipped: 3

## Fixed Issues

None — all findings were skipped.

## Skipped Issues

### CR-01: Occupied reuse overwrites protected recovery state on the existing checkout

**File:** `baude-core/src/lifecycle.rs:1081-1127`
**Reason:** The shared compatibility guard, owner startup ordering, and mirrored recovery tests were applied, but the new core test failed to compile because the helper was omitted from the test module's explicit imports. Per-finding rollback restored all touched files; no partial activation changes remain.
**Original issue:** Occupied activation reuse can overwrite `TeardownPending`, `RemovalTombstone`, and other protected unavailable recovery state before the owning recovery path runs.

### CR-02: Manager close rollback still restarts only the agent and silently loses the shell

**File:** `bauded/src/manager.rs:1487-1524`
**Reason:** Manager shell snapshot/restoration and a shared typed restoration decision initially passed the targeted Rename rollback test and were committed as `1dec466`. Full-suite verification then exposed a remaining Manager serialization path that rewrote `shell_open` to false. A corrective attempt failed to compile, so the fix commit was reverted by `3e66c52`; the baseline tree was restored and the complete suite passes.
**Original issue:** Manager snapshots `shell_open: false` and restores only the agent after a pre-replacement close-save failure, losing an open shell.

### CR-03: App discards process ownership when failed rollback cleanup cannot stop the restarted runtime

**File:** `baude/src/app.rs:2890-2919`
**Reason:** App and Manager were routed through typed destructive teardown with mirrored agent/shell cleanup-refusal tests. Verification showed that the injected Rename failure remained active for the required recovery save, so the test correctly detected that `TeardownPending` was not yet durable. Per-finding rollback removed the incomplete cleanup implementation and tests.
**Original issue:** Failed App restoration ignores cleanup failure, drops runtime ownership, and persists intermediate rather than final process outcomes.

## Verification

After all rollbacks and the CR-02 revert:

- `cargo fmt --all -- --check` — passed
- `cargo clippy --all-targets -- -D warnings` — passed
- `cargo test` — passed (315 tests: 31 baude, 207 baude-core, 77 bauded)

---

_Fixed: 2026-08-31T02:41:34Z_
_Fixer: the agent (gsd-code-fixer)_
_Iteration: 1_
