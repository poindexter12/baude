---
phase: 18-shared-kill-then-remove-lifecycle
plan: 08
subsystem: pty
tags: [rust, pty, signals, process-group, reaping, tdd]

# Dependency graph
requires:
  - phase: 06-shared-lifecycle-core-refactor
    provides: Pty, ProcessIdentity, Session, SessionTeardownError, inspect_process_identity
provides:
  - GroupDisposition { Signal, Extinct, ForeignOwner, Unverifiable } and group_disposition(recorded, observed_leader: Result<Option<&ProcessIdentity>, &str>, group_alive) in baude-core/src/pty.rs, comparing process_group explicitly
  - Pty::kill_and_wait restructured as a lock-free sequence (debug injection lookup by identity.pid, inspection, disposition, SIGTERM) ahead of any child-mutex lock, then one shared reaping block and a bounded reader join
  - Pty.reader: Option<JoinHandle<()>> retained from spawn; Pty::reader_finished; private extinguish_group and join_reader helpers; wait_for_group_extinction reaping through try_lock
  - fail_next_teardown_for_test keyed by identity.pid; test-only reader_holds_child_lock_for_test and hang_up_slave_for_test
  - Session::kill_and_wait error arm reads agent_pid and shell_pid from process_identity().pid, never Pty::pid, plus its post-success contract doc comment
affects: [18-04, 18-05, 18-06, 18-07]

# Actuals (#2632) - estimateTokens scale, chars/4 over the realized diff
actuals:
  tokens: 11219
  tasks: 3
  commits: 2
plan_head_before: 61af9fbe3bbf8a5fe6d6550742c0ea1af9698324
plan_head_after: 213e26e2423b8970edc8f28d9e28dcd520c0aa89

# Tech tracking
tech-stack:
  added: []
  patterns:
    - "Teardown decides from immutable recorded identity plus a kill(-pgid, 0) probe before taking any lock a reader thread can hold; the child mutex is locked only after the group is proven extinct"
    - "A JoinHandle that may never finish stays owned by the struct and is reported through an accessor, never detached silently"
    - "Forking PTY fixtures start with set +m and prove group membership from ps -eo pid=,pgid=,comm= before the call under test, so a job-control pgid split fails the seed instead of passing vacuously"

key-files:
  created: []
  modified:
    - baude-core/src/pty.rs
    - baude-core/src/session.rs

key-decisions:
  - "Task 3 extraction was done inside GREEN: kill_and_wait calls extinguish_group for the TERM, wait, KILL, wait sequence and join_reader for the bounded join, with the injection check and SIGTERM ordered ahead of the first child-mutex lock, so Task 3 found nothing to change and no refactor commit was made"
  - "macOS keeps the controlling tty open until the session leader exits, so the parked-reader fixture reaches the reader-in-child.wait shape through a test-only hang_up_slave_for_test that calls revoke(2) on the slave path from ptsname on the master; it is a no-op on Linux where the leader's last slave close already hangs the master up"

patterns-established:
  - "descendant_in_group(pgid, comm) as a private helper in each test module that owns a PTY fixture; plans 04 to 07 inherit it through spawn_trapping_runtime"

requirements-completed: []

coverage:
  - id: D1
    description: "A TERM- and HUP-trapping descendant whose PTY leader already exited is signalled and killed; kill(-pgid, 0) fails with ESRCH after kill_and_wait"
    requirement: KILL-01
    verification:
      - kind: unit
        ref: "baude-core/src/pty.rs#group_extinction_after_leader_exit"
        status: pass
    human_judgment: false
  - id: D2
    description: "group_disposition never selects Signal for a live stranger inside the recorded group, maps an inspection error to Unverifiable, and still signals the group's own survivors when the pid-reuser sits in another group"
    requirement: KILL-01
    verification:
      - kind: unit
        ref: "baude-core/src/pty.rs#group_extinction_disposition_never_signals_foreign_owner"
        status: pass
    human_judgment: false
  - id: D3
    description: "After Ok the reader thread is joined and reader_finished is true when nothing outside the group holds the slave"
    requirement: KILL-02
    verification:
      - kind: unit
        ref: "baude-core/src/pty.rs#group_extinction_joins_reader"
        status: pass
    human_judgment: false
  - id: D4
    description: "SIGTERM reaches the group before the child mutex is taken, so a reader parked in child.wait cannot stall teardown in a debug build"
    requirement: KILL-01
    verification:
      - kind: unit
        ref: "baude-core/src/pty.rs#group_extinction_signals_before_taking_child_lock"
        status: pass
    human_judgment: false
  - id: D5
    description: "A setsid holder of the slave outside the recorded group leaves the reader parked, the handle owned by the Pty and reader_finished false, while the group itself is extinct"
    requirement: KILL-02
    verification:
      - kind: unit
        ref: "baude-core/src/pty.rs#group_extinction_leaves_reader_parked_for_out_of_group_slave_holder"
        status: pass
    human_judgment: false
  - id: D6
    description: "Session::kill_and_wait drains both the agent and the shell PTY: groups extinct, leaders reaped, readers joined"
    requirement: KILL-02
    verification:
      - kind: unit
        ref: "baude-core/src/session.rs#group_extinction_drains_agent_and_shell_readers"
        status: pass
    human_judgment: false
  - id: D7
    description: "With both readers parked and a teardown failure injected on both PTYs, Session::kill_and_wait returns Err within 3 s with both pids populated and both leaders still alive, so the error arm never locks a child mutex"
    requirement: KILL-01
    verification:
      - kind: unit
        ref: "baude-core/src/session.rs#group_extinction_error_path_does_not_take_child_lock"
        status: pass
    human_judgment: false

# Metrics
duration: 28 min
completed: 2026-11-03
status: complete
---

# Phase 18 Plan 08: Process-Group Extinction Regardless of Leader State Summary

**`Pty::kill_and_wait` now decides from a four-way `GroupDisposition` and signals the recorded process group before it touches the child mutex, so a trapping descendant that outlived its exited leader is killed and proven extinct, the reader thread is joined afterwards or kept owned and reported through `reader_finished()`, and `Session::kill_and_wait` builds its error from the immutable identities so a refusal surfaces promptly even with both readers parked**

## Performance

- **Duration:** 28 min
- **Started:** 2026-11-03T22:44:42Z
- **Completed:** 2026-11-03T23:13:00Z
- **Tasks:** 3 (RED, GREEN, REFACTOR evaluated with nothing left to extract)
- **Files modified:** 2

## Accomplishments
- `group_disposition` takes the inspection `Result` and the `kill(-pgid, 0)` liveness probe: `Extinct` when the group is gone, `Unverifiable` on an inspection error (returned before any signal, `exited` untouched), `ForeignOwner` only when a live different `ProcessIdentity` sits in the same `process_group`, and `Signal` otherwise, including the pid-reuser-in-another-group row the POSIX fork() rule makes the group's own orphan case
- `kill_and_wait` runs the debug-only injection lookup by `self.identity.pid`, inspection, disposition and the first `signal_group(` ahead of any `self.child` lock (line 30 against line 40 of the function region, zero `self.pid()` calls); `extinguish_group` sends TERM, waits through `wait_for_group_extinction` which reaps via `try_lock` and otherwise lets the reader's own `wait()` reap, escalates to KILL and errors on a survivor; the shared reaping block locks only after extinction and tolerates `ECHILD` from a reader that reaped first
- `Pty` retains the reader `JoinHandle`; `join_reader` joins for a 2 s bound and leaves the handle in place on timeout so `reader_finished()` is false and the thread is never detached silently; `fail_next_teardown_for_test` keys on `identity.pid`
- `Session::kill_and_wait` reads `agent_pid` and `shell_pid` from `process_identity().pid` (two occurrences, zero `pid()` or `Pty::pid` in the wrapper) and documents the post-`Ok` contract including `RemovalReport.readers_parked` for plan 04
- Seven tests use `set +m` fixtures that prove group membership through `descendant_in_group` before the call, assert on `kill(-pgid, 0)` returning ESRCH, on `reader_finished()`, on the child `try_lock` state and on wall-clock bounds; the out-of-group holder uses `perl -MPOSIX` `setsid` and exits 3 without writing `holder.pid` when setsid fails

## TDD Gate Compliance

| Gate | Evidence |
| ---- | -------- |
| RED | Commit `3c58b40` (`test(18-08): add failing process-group extinction tests`). `timeout 600 cargo test -p baude-core group_extinction -- --test-threads=1` exit non-zero, zero `error[E` lines, 0 passed / 7 failed in 18.50 s. Each failure is the planned assertion against the pre-plan code: `group 25720 survived kill_and_wait` with `left: None` (leader-exited early return), `stranger inside the group` with `left: Signal` (stub disposition), `reader thread was not joined`, `reader handle must stay owned by the Pty, not detached`, `kill_and_wait blocked behind the parked reader for 7.94 s`, and the two session assertions on drained readers and on the error arm returning within the bound. Semantic assessment: every target test executed and failed on its intended assertion for the intended reason. |
| GREEN | Commit `213e26e` (`feat(18-08): extinguish PTY process groups after leader exit and join readers`). Target: 7 passed, 0 failed in 3.43 s. `pty::tests` 23 passed, `session::tests` 27 passed. `cargo clippy -p baude-core --all-targets -- -D warnings` exit 0 with no warnings on any run; `cargo fmt --check` exit 0. |
| REFACTOR | Conditional task; `kill_and_wait` is 62 lines and already delegates to `extinguish_group` and `join_reader`, so no file changed and no commit was made, as the plan directs. Task 3 acceptance re-run on HEAD: first `signal_group(\|extinguish_group(` at region line 30, first `self.child\|.child\|self.pid()` at line 40, `self.pid()` count 0, session wrapper `pid()\|Pty::pid` count 0 and `process_identity().pid` count 2, `cargo fmt --check` exit 0. |

`gsd_run check tdd-red-evidence` on the captured cargo output returned `INVALID_RED` with reason `unreadable_record` (`record is not valid JSON`): the classifier has no adapter for `cargo test` text, the same gap plans 18-01 to 18-03 recorded. The plan's own RED gate (exit non-zero, no `error[E`, `7 failed`, every target test named with `FAILED`) was applied instead and passed.

## Task Commits

1. **Task 1 (RED): group-extinction tests, descendant_in_group helpers, GroupDisposition and reader_finished stubs** - `3c58b40` (test)
2. **Task 2 (GREEN): route every leader state through group extinction and join the reader** - `213e26e` (feat)
3. **Task 3 (REFACTOR, conditional): nothing to extract** - no commit

**Plan metadata:** see the docs commit that lands this file

## Files Created/Modified
- `baude-core/src/pty.rs` - `GroupDisposition`, `group_disposition`, `signal_group`, `process_group_exists`, `wait_for_group_extinction(&Mutex<...>)`, `Pty.reader`, `reader_finished`, `reader_holds_child_lock_for_test`, `hang_up_slave_for_test`, restructured `kill_and_wait`, `extinguish_group`, `join_reader`, `fail_next_teardown_for_test` keyed by pid; tests `descendant_in_group` and the five `group_extinction_*` tests
- `baude-core/src/session.rs` - `Session::kill_and_wait` doc comment and lock-free error arm; tests `descendant_in_group`, `group_extinction_drains_agent_and_shell_readers`, `group_extinction_error_path_does_not_take_child_lock`

## Verification

- `cargo test -p baude-core group_extinction -- --test-threads=1`: 7 passed, 0 failed (host macOS, debug build)
- `cargo test -p baude-core pty::tests -- --test-threads=1`: 23 passed; `session::tests`: 27 passed
- Full `cargo test -p baude-core` on the host: 496 passed, 0 failed, 1 ignored
- Full `cargo test -p baude-core -- --test-threads=1` in `rust:1-bookworm` as uid 1000 with a fresh `CARGO_HOME` and target dir: 496 passed, 0 failed, 1 ignored; the seven target tests pass there in 4.31 s with `/usr/bin/ps` and `/usr/bin/perl` present
- `cargo clippy -p baude-core --all-targets -- -D warnings` and `cargo fmt --check` exit 0
- `git log --oneline -E --grep='^test\(18-0?8\):'` returns `3c58b40`; `--grep='^feat\(18-0?8\):'` returns `213e26e`
- No file deletions in either commit; no `Cargo.toml` change

## Decisions Made
- Extracted `extinguish_group` and `join_reader` during GREEN rather than as a separate refactor, keeping the Task 2 order invariant in `kill_and_wait` itself; Task 3 then had no diff to commit
- On macOS the parked-reader shape the plan describes (leader alive with stdio redirected, reader in `child.wait()`) is not reachable from the leader's redirection alone because the session keeps the controlling tty open until the leader exits; a `#[cfg(test)]` `hang_up_slave_for_test` revokes the slave so the master reads EOF while the leader keeps running. The out-of-group holder test documents the inverse: macOS revokes the tty at leader exit, so that reader finishes there while Linux keeps it parked, and the assertion branches per platform while the group-extinction and handle-ownership checks stay unconditional

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] macOS fixture could not park the reader in child.wait() from stdio redirection alone**
- **Found during:** Task 1, while proving the `group_extinction_signals_before_taking_child_lock` fixture shape with a throwaway probe
- **Issue:** With `exec sleep 8 </dev/null >/dev/null 2>&1` the master never reached EOF on macOS, so the reader parked in `read()` rather than `child.wait()` and the test would not have exercised the mutex ordering it exists to pin
- **Fix:** Added test-only `Pty::hang_up_slave_for_test`, which on macOS calls `revoke(2)` on the slave path obtained from `libc::ptsname` on the master fd through a local `extern "C"` declaration, since the `libc` crate has no `revoke` binding and this `portable_pty` has no slave-name accessor; the function is empty on Linux. No production code path and no dependency change
- **Files modified:** baude-core/src/pty.rs
- **Commit:** `3c58b40`

## Issues Encountered
- `scripts/gate.sh --linux` failed in its `isolated cargo test` step with `error[E0463]: can't find crate for http, rustls, rustls_pki_types, sha1, thiserror` after `fmt`, `clippy`, the commit-message check and the real-roots check had all passed in the container; the stale `target/linux-gate` cargo state, not this plan's code, was the cause. Re-running the full suite in the same `rust:1-bookworm` image as uid 1000 with a fresh `CARGO_HOME` and `CARGO_TARGET_DIR` gave 496 passed, 0 failed. The cached gate state under `target/linux-gate` should be cleared before the next push gate
- The scratchpad shared by the phase's executors still held plan 03's `red.log`; this plan's RED capture is `red.txt`, which is the file quoted above

## Known Stubs
None. The RED-phase stubs (`group_disposition` returning `Signal`, `reader_finished` returning `false`) were replaced in GREEN.

## Threat Flags
None beyond the plan's threat model. T-18-23, T-18-24, T-18-25, T-18-30, T-18-31, T-18-32, T-18-40 and T-18-42 are each covered by the named test; T-18-SC is kept (no `Cargo.toml` change; `revoke` is declared locally in a test-only function).

## Requirements
KILL-01 and KILL-02 are also declared by plans 18-04, 18-05, 18-06 and 18-07, none of which has a SUMMARY yet, so they are not marked complete here; the last declaring plan marks them.

## User Setup Required
None - no external service configuration required.

## Next Phase Readiness
- Plan 04's `removal::remove` kill stage can call `Session::kill_and_wait`, then check `reader_finished()` on every consumed Pty and list parked rows in `RemovalReport.readers_parked` before dropping the handles
- Plan 07's Linux log can name all seven tests; they pass in `rust:1-bookworm` today
- `descendant_in_group` exists as a private helper in both `pty::tests` and `session::tests` for plan 04's `spawn_trapping_runtime` to copy

---
*Phase: 18-shared-kill-then-remove-lifecycle*
*Completed: 2026-11-03*

## Self-Check: PASSED

- `baude-core/src/pty.rs` and `baude-core/src/session.rs` exist; `pty.rs` carries `pub fn kill_and_wait` and `pub fn reader_finished`.
- Commits 3c58b40 and 213e26e are ancestors of HEAD on `gsd/phase-18-shared-kill-then-remove-lifecycle`.
