---
phase: 06-safe-managed-worktree-lifecycle
reviewed: 2026-08-31T00:50:41Z
depth: deep
files_reviewed: 6
files_reviewed_list:
  - baude-core/src/lifecycle.rs
  - baude-core/src/pty.rs
  - baude-core/src/repository.rs
  - baude-core/src/session.rs
  - baude/src/app.rs
  - bauded/src/manager.rs
findings:
  critical: 3
  warning: 0
  info: 0
  total: 3
status: resolved
adjudicated: 2026-09-02
---

# Phase 6: Code Review Report

**Reviewed:** 2026-08-31T00:50:41Z
**Depth:** deep
**Files Reviewed:** 6
**Status:** issues_found

## Summary

Commits `ab85320`, `bca48b7`, `6628395`, and `881a7ef` fix the four findings from the prior review in their originally reported paths: unchanged occupied worktrees can now be rebound after a pending-save crash, recorded teardown verifies PID/start-time/process-group/session identity before signaling on macOS and Linux, App restores both agent and shell on the successful rollback path, and blocked activation retries preserve provenance.

The re-review found three Critical regressions or incomplete cross-owner paths. Occupied reuse can erase an existing checkout's teardown/tombstone recovery state and skip its recovery entirely. Manager still loses an open shell on the same close rollback that App now handles. App's new failed-restore cleanup discards teardown errors and process identity, potentially orphaning a live restarted runtime while persisting misleading outcomes.

Checks run: `cargo fmt --all -- --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`. All passed; 315 tests passed (31 baude, 207 baude-core, 77 bauded). Tests ran on macOS. No Linux Rust target is installed, so Linux behavior was inspected directly but not cross-compiled in this environment.

### Prior finding disposition

- **Prior CR-01:** Fixed for an unchanged, otherwise healthy occupied owner. New CR-01 covers the unsafe merge into an owner already carrying protected recovery state.
- **Prior CR-02:** Fixed. Durable process identity is captured and exact identity is checked immediately before TERM/KILL; PID reuse does not signal the new occupant.
- **Prior CR-03:** Fixed in App's successful agent-and-shell restoration path. New CR-02 is the missing Manager parity path; new CR-03 is App's failed-restoration cleanup path.
- **Prior WR-01:** Fixed. `created_branch`, preexisting owner, compensation, and prior verification evidence survive blocked retries and serialization.

## Narrative Findings (AI reviewer)

## Critical Issues

### CR-01 [BLOCKER]: Occupied reuse overwrites protected recovery state on the existing checkout

**File:** `/Users/joese/Code/github.com/poindexter12/baude/baude-core/src/lifecycle.rs:1081-1127`  
**Also:** `/Users/joese/Code/github.com/poindexter12/baude/baude/src/app.rs:535-541`, `/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:357-363`

**Issue:** The occupied-owner recovery branch merges into any saved checkout with the same repository and path. It checks only the branch string, then unconditionally sets `active_intent = true` and `health = Available`. It does not reject `RemovalTombstone`, `TeardownPending`, `ActivationRecovery`, or `StoppedActiveRecovery`. Startup runs activation reconciliation before teardown reconciliation in both owners. Therefore, if the occupied checkout was pending teardown, this merge deletes its recorded process identities before teardown recovery scans the state; startup then spawns a second runtime while the original process can remain live. A removal tombstone can likewise be made reopenable despite the explicit invariant that it must never reopen. The ordinary occupied-reuse path at `lifecycle.rs:1314-1332` has the same unchecked merge, so retrying without a crash is also unsafe.

**Fix:** Permit reuse of an existing durable checkout only when its health is compatible with activation (normally `Available`) and it is not carrying a protected recovery cause. Preserve/block on tombstone and activation recovery; finish `TeardownPending` before activation can merge or spawn. Apply the same guard to both recovery and `execute_activation`, and add App/Manager tests with an occupied owner in `TeardownPending` and `RemovalTombstone` states.

```rust
match &state.checkouts[existing].health {
    CheckoutHealth::Available => { /* merge and activate */ }
    CheckoutHealth::Unavailable(cause) => {
        return Ok(ActivationRecoveryResolution::Blocked {
            checkout: checkout_key,
            detail: format!("occupied owner has unresolved recovery: {cause:?}"),
        });
    }
}
```

### CR-02 [BLOCKER]: Manager close rollback still restarts only the agent and silently loses the shell

**File:** `/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:1487-1524`  
**Also:** `/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:1207-1230`, `/Users/joese/Code/github.com/poindexter12/baude/bauded/src/manager.rs:1885-1895`

**Issue:** Manager hardcodes `shell_open: false` when snapshotting a retained runtime, even though Manager can restore/open shells and destructive teardown stops `session.shell`. After a pre-replacement close-save failure, `restart_with_mode` replaces only `s.claude`; the exited shell remains attached and the runtime mapping is retained. The call then returns only the persistence error, presenting rollback as complete while shell state and liveness diverge. This is the daemon-owner version of the prior App CR-03 and violates owner parity.

**Fix:** Snapshot the actual `session.shell_open`, share an agent-and-shell restoration helper with equivalent semantics to App, verify both processes are live before retaining the mapping, and persist typed failed-restoration state if either cannot be restored. Add a Manager Rename-failure test with an open shell that asserts new agent and shell PIDs are live.

```rust
let snapshot = RetainedSessionState {
    shell_open: session.shell_open,
    // existing fields
};
restore_stopped_runtime(id, mode, snapshot.shell_open)?;
```

### CR-03 [BLOCKER]: App discards process ownership when failed rollback cleanup cannot stop the restarted runtime

**File:** `/Users/joese/Code/github.com/poindexter12/baude/baude/src/app.rs:2890-2919`  
**Also:** `/Users/joese/Code/github.com/poindexter12/baude/baude/src/app.rs:1799-1818`

**Issue:** If agent restart succeeds but shell restoration fails, `restore_stopped_runtime` calls `session.kill_and_wait()` and ignores its result. The caller then always removes the runtime mapping/session and persists `StoppedActiveRecovery`, which has no PID or process identity fields. A cleanup refusal can therefore leave the newly spawned agent or shell alive with no runtime owner and no durable authority to finish teardown after restart. Even when cleanup succeeds, the recorded `agent_restarted: true` describes the pre-cleanup observation although the agent was deliberately stopped, making durable recovery outcomes false.

**Fix:** Treat rollback cleanup as the same destructive teardown boundary used by close/removal. If cleanup is incomplete, retain the session mapping until `mark_teardown_pending` has captured exact identities and that state is durable; never ignore `kill_and_wait`. If cleanup succeeds, record final liveness (`false` for processes that were stopped), not the intermediate restart result. Add injected cleanup-refusal tests for both agent and shell.

```rust
if !(agent_restarted && shell_restarted) {
    lifecycle::destructive_teardown(&mut self.repository_state, checkout, session)?;
    // Only forget the mapping after confirmed teardown and durable state update.
}
```

---

_Reviewed: 2026-08-31T00:50:41Z_  
_Reviewer: the agent (gsd-code-reviewer)_  
_Depth: deep_

---

## Adjudication — 2026-09-02

Owner-directed adjudication of the three standing Criticals against the
current tree (`57b7c1c` plus the two guard tests added by this adjudication).
Each disposition below was verified by reading the current code and executing
the named tests in-process, not by trusting summaries. The 06-REVIEW-FIX.md
`none_fixed` record remains accurate for its own 2026-08-31T02:41Z attempt;
every fix below landed AFTER that attempt, chiefly through plan 06-07's
lifecycle-engine cutover.

### CR-01 — RESOLVED (fixed post-review, now test-pinned on both paths)

The unchecked occupied-owner merge no longer exists on either flagged path:

- `execute_activation` finalization refuses an occupied owner carrying any
  `CheckoutHealth::Unavailable` cause with the typed
  `LifecycleError::OccupiedProtected` (baude-core/src/lifecycle.rs:1867 area),
  introduced by `b661784` (2026-08-31 01:42, after the review).
- `reconcile_activation_recovery`'s preexisting-owner reuse arm returns
  `ActivationRecoveryResolution::Blocked` with "occupied checkout N has
  unresolved recovery" for the same condition
  (baude-core/src/lifecycle.rs:1600 area), introduced by `06c2076`
  (2026-08-30 22:21, after the review).
- The review's demanded tests now exist and pass:
  `occupied_protected_checkout_refuses_activation_overwrite` (`57b7c1c`) and
  `occupied_protected_checkout_blocks_activation_recovery_merge` (this
  adjudication), both asserting a RemovalTombstone occupant survives
  byte-intact and the pending child is not merged.

### CR-02 — RESOLVED (point-fix reverted, then superseded by the engine cutover)

Timeline: fix `1dec466` (19:35) was indeed reverted by `3e66c52` (19:40) —
the fix log is truthful for that attempt — but plan 06-07 then re-landed
agent-and-shell close-rollback parity through the shared `LifecycleEngine`
(`c97bbaf` 22:37 and `7dad443` 23:24, both after the review). In the current
tree Manager's retained snapshot carries the real `session.shell_open`
(bauded/src/manager.rs:1398), and the exact test the review prescribed —
`lifecycle_close_manager_persistence_failure_retains_child_and_parent`, a
pre-replacement close-save failure with an open shell — passes, asserting
both the agent and shell PIDs are live on the compensated runtime with
`shell_open` intact. The App mirror
(`lifecycle_close_local_obeys_persistence_commit_boundary`) passes with the
same both-process assertions, restoring the owner parity the finding
demanded.

### CR-03 — RESOLVED (path redesigned at the typed teardown boundary)

The flagged `restore_stopped_runtime` cleanup path no longer exists; the
engine cutover replaced it with the single typed stop boundary
`lifecycle::destructive_teardown` (baude-core/src/lifecycle.rs:855 area),
which never ignores `kill_and_wait`: a partial stop is transferred into
`UnavailableCause::TeardownPending` carrying exact agent/shell PIDs,
identities, and per-process stopped flags before the error returns to either
owner. `StoppedActiveRecovery` deliberately remains identity-free because it
is now recorded only for processes that were verifiably stopped with restart
compensation also failed — restart outcomes, not liveness claims about
unowned processes; its reopen/recovery dispatch is pinned by the lifecycle
capability tests. The remaining `let _ = kill_and_wait()` sites in
baude/src/app.rs (2530/2545/4652/4733 area) are all fresh-spawn abort paths —
best-effort kills of a replacement PTY spawned by the very call that is
erroring, before any durable ownership was granted — which is a different
category from the retained-runtime orphaning the finding described and
cannot strand durable state.

**Disposition:** all three Criticals closed as fixed-in-tree. Review status
updated to `resolved`; SC5's clean-review requirement is satisfied by this
adjudication record.

_Adjudicated: 2026-09-02 by Claude, directed by the owner._
