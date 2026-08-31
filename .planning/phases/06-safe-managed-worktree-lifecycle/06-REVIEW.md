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
status: issues_found
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
