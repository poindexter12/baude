---
phase: 21-daemon-and-pwa-parity
plan: 03
subsystem: api
tags: [rust, axum, daemon, removal, status-matrix, drift, persistence, fixtures]

requires:
  - phase: 21-01
    provides: POST /removals/preview and POST /removals, Manager::remove_rows, RemoveRowsError, the removal fixtures
  - phase: 21-02
    provides: GET /rows, the only route that serves checkout_key
  - phase: 21-09
    provides: cloned_repo, removal_root, removal_daemon and the parent versus child race test
  - phase: 21-10
    provides: standalone rows with runtime custody, folder_fixture, restarted_daemon, removed_over_routes
provides:
  - RemovalError::RuntimeHandleMissing answered with 503 instead of 500
  - Fourteen matrix_ tests that drive every status in the wire contract by its cause over HTTP
  - assert_refused, world_snapshot, assert_world_unchanged, assert_drift, assert_executed, removal_response, runtime_pids, sibling_directory
  - Test seams forget_runtime_handle_for_test and reserve_repository_for_test
  - A named status on the refused side of the three inherited race tests
affects: [21-04, 21-05, 21-08, 21-11]

actuals:
  tokens: 11510
  tasks: 3
  commits: 3
plan_head_before: c3564b9e0ac071b4201424cf2597ac08cd697997
plan_head_after: 0691318731fefe22152b02b9369f09d7ef261b5c

tech-stack:
  added: []
  patterns:
    - "Every pinned status goes through assert_refused, assert_drift or assert_executed, whose failure message carries the status and body the daemon actually returned"
    - "A failure class is driven by its real cause: an armed teardown, an injected atomic failure, a held reservation, a state file the loader rejects. No test sets the outcome directly"
    - "A refusal is proven by what it left alone: /rows, the state file bytes, the agent pid and the directory on disk, read after the response"

key-files:
  created: []
  modified:
    - bauded/src/api.rs
    - bauded/src/manager.rs

key-decisions:
  - "A missing runtime handle is 503, not 500: core raises it in preflight before any stage runs, so nothing was signalled, deleted or saved"
  - "The same-row race loser is 404 on both checkout and standalone rows; the parent versus child loser is 404 when the repository removal won and 409 when the child removal won"
  - "An unchanged world compares /rows and the state file exactly, and /sessions without status, state_source and waiting_for_ms, which the daemon derives from the clock and PTY output"

requirements-completed: []
requirements-advanced: [API-01, API-02, API-03]

coverage:
  - deliverable: "A row whose runtime handle the daemon does not hold is refused with 503 and nothing is touched"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#matrix_missing_runtime_handle_is_503_and_touches_nothing"
        status: pass
  - deliverable: "Each of the three compared sets refuses with a 409 preview body, and the resent body executes"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#matrix_drift_row_set_change_is_409_then_resend_executes_then_replay_is_404"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#matrix_drift_runtime_only_change_is_409"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#matrix_drift_tampered_paths_deleted_is_409_and_touches_nothing"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#matrix_drift_holds_for_a_standalone_row"
        status: pass
  - deliverable: "Fields the recheck does not compare cannot name anything the daemon acts on"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#matrix_drift_uncompared_fields_cannot_steer_the_daemon"
        status: pass
  - deliverable: "A survived runtime is 500 with rows and handles held, on a checkout row, a standalone row and inside a repository removal; the retry succeeds"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#matrix_failure_runtime_survived_is_500_then_retry_succeeds"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#matrix_failure_standalone_runtime_survived_is_500_then_retry_succeeds"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#matrix_failure_parent_abort_keeps_every_handle_held"
        status: pass
  - deliverable: "An interrupted removal converges through 503, 409 and 200 with no re-arming"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#matrix_failure_persist_is_503_and_the_retry_converges"
        status: pass
  - deliverable: "A committed replacement is a 200 with saved_degraded; a busy repository and blocked persistence are 503 before anything is touched"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#matrix_failure_committed_replacement_is_a_200_saved_degraded"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#matrix_failure_busy_repository_is_503_before_any_kill"
        status: pass
      - kind: test
        ref: "bauded/src/api.rs#matrix_failure_blocked_persistence_is_503_before_the_target_is_looked_up"
        status: pass
  - deliverable: "Refused admissions are 400 plain text and leave /rows byte-equal"
    human_judgment: false
    verification:
      - kind: test
        ref: "bauded/src/api.rs#matrix_refused_admission_is_400_plain_text"
        status: pass

metrics:
  duration: "about 45 min, approximate"
  completed: 2026-11-12
status: complete
---

# Phase 21 Plan 03: The removal status matrix Summary

Every status `POST /removals` can return is now driven by its real cause over HTTP and pinned by
name, a missing runtime handle answers 503 instead of 500, and no test in the phase still asserts
a refusal as merely not 200.

## What was built

### Task 1: a missing runtime handle is 503 (`6fdf4eb`)

- `removal_error` in `bauded/src/api.rs` maps `RemovalError::RuntimeHandleMissing` beside
  `RemovalError::Persist` to 503. Core raises it in preflight, before any stage runs.
- `Manager::forget_runtime_handle_for_test` drops one `runtime_checkouts` entry and nothing else,
  which is the stale runtime row a failed restore leaves behind.
- Helpers `assert_refused` and `world_snapshot`.
- `matrix_missing_runtime_handle_is_503_and_touches_nothing`.

### Task 2: the drift matrix (`a152da3`)

- Five tests: a row-set change, a runtime-only change, a tampered `paths_deleted`, rewritten
  uncompared fields, and drift on a standalone row.
- Helpers `assert_drift`, `assert_world_unchanged`, `settled_sessions`, `runtime_pids` and
  `sibling_directory`.
- No production change.

### Task 3: the failure matrix and the pinned refusals (`0691318`)

- Eight tests: 500 on a survived runtime for a checkout row, a standalone row and a repository
  removal; 503 on a failed save, a busy repository and blocked persistence; 200 `saved_degraded`
  on a committed replacement; 400 on refused admissions.
- `Manager::reserve_repository_for_test` returns the real `RepositoryReservation` guard. It is the
  only production-file change in this task and is `#[cfg(test)]`.
- Helpers `assert_executed` and `removal_response`.
- Three inherited tests pinned on the refused side:
  `removals_concurrent_execute_of_the_same_row_commits_once` and
  `standalone_concurrent_removals_of_one_folder_commit_once` assert sorted statuses `[200, 404]`;
  `row_kinds_parent_removal_racing_a_child_removal_commits_one` asserts the loser is 404 when the
  repository removal won and 409 when the child removal won.

## The status matrix as observed

| Status | Cause | Body |
|---|---|---|
| 200 | Executed, save committed | report, `persistence.outcome` `saved` |
| 200 | Executed, replacement committed, directory sync failed | report, `saved_degraded` with `detail`; `persistence_status()` stays dirty with that detail |
| 400 | `POST /sessions` for a removed worktree path, a path that never existed, a regular file | plain text containing `not a directory` |
| 404 | Target not stored: an executed report sent again, a same-row race loser | plain text |
| 409 | Any of `rows_removed`, `runtimes_killed`, `paths_deleted` differs from a fresh preview | JSON `RemovalReport` with `planned: true`, the only JSON error body |
| 500 | A runtime survived the kill stage | plain text containing `survived the kill stage` and the `Debug` form of the row key |
| 503 | Save failed before the replacement committed | plain text containing `could not persist` |
| 503 | Repository reserved by another mutation | plain text beginning `repository busy:` then the repository path |
| 503 | Persistence blocked by a state file the loader rejected | plain text containing `persistence is blocked`, answered before the target is looked up |
| 503 | Row records a runtime the daemon holds no handle for | plain text |

## RED gates

- **Task 1.** `matrix_missing_runtime_handle_is_503_and_touches_nothing` was run before the
  production edit and failed on its own marker: `21-03 status pin: a row whose runtime handle the
  daemon does not hold: the daemon answered 500 Internal Server Error`. Log
  `/tmp/baude-21-03-red.log`, status `101` in `/tmp/baude-21-03-red.status`. GREEN after the
  `removal_error` arm moved.
- **Tasks 2 and 3.** These carry no production change, so there was no RED to record. Each test
  passed against the daemon as it stood. No status in the wire contract had to be changed to fit an
  observation.

## Verification

| Command | Exit | Result |
|---|---|---|
| `cargo test -p bauded api::tests::matrix_ -- --test-threads=1` | 0 | `matrix ok: 14` |
| `cargo test -p bauded -- --test-threads=1` | 0 | 163 passed, 0 failed |
| `cargo fmt --check` and `cargo clippy -p bauded --all-targets -- -D warnings` | 0, 0 | clean |
| `cargo fmt --all --check` | 0 | clean |
| `cargo clippy --workspace --all-targets -- -D warnings` | 0 | clean |
| `cargo test -p bauded` | 0 | 163 passed, 0 failed |

The baseline was 149 tests; this plan adds 14. The macOS teardown flake
`failed to terminate exact PTY process group` did not appear in either full crate run or in any
matrix run.

## Observations recorded, not required

- **Runtime-only drift.** A restart keeps the session id and changes the agent pid: previewed pid
  77812, fresh pid 78095, session id 1 before and after.
- **Parent abort.** With the teardown armed on the row listed last, the other child's agent pid was
  dead after the 500 while its session id, its handle and its row were all still served. A 500 from
  a repository removal can therefore leave a child whose agent has exited.
- **Convergence bodies.** 503 `could not persist the removed row set: injected rename failure`;
  then 409 with `paths_deleted: []`, the worktree path under `already_removed` and the same
  `runtimes_killed` entry; then 200 with `persistence.outcome` `saved`. The replay needed no
  re-arming, so the injected failure is cleared on `Err` as 21-01 step 7 says.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Test realism] `/sessions` is compared without three derived fields**
- **Found during:** Task 2
- **Issue:** `status`, `state_source` and `waiting_for_ms` are derived from the clock and from PTY
  output, so they move between two reads with no request in between. A byte comparison of
  `/sessions` would fail for a reason unrelated to the removal.
- **Fix:** `assert_world_unchanged` compares `/rows` and the state file exactly and `/sessions`
  through `settled_sessions`, which drops only those three fields. Whether a refusal stopped an
  agent is asserted separately and directly, through `pid_is_alive` on the previewed pid.
- **Files modified:** bauded/src/api.rs
- **Commit:** a152da3

**2. [Rule 3 - Blocking] `PersistenceStatus` has no `Debug`**
- **Found during:** Task 3
- **Issue:** Three assertion messages formatted the status with `{:?}` and did not compile.
- **Fix:** The messages are literal text. No derive was added, because the task allows no
  production change beyond the reservation seam.
- **Files modified:** bauded/src/api.rs
- **Commit:** 0691318

Otherwise the plan was executed as written. The `matrix_` count is exactly 14.

## Known issues

- The open macOS flake on `DELETE /sessions/{id}` is unchanged and was not seen here. No assertion
  was loosened for it.
- Requirements API-01, API-02 and API-03 stay pending, as instructed.

## What later plans need

**21-04, publishing one event per committed removal.**

- A removal is committed in exactly one place: the single `Ok(report)` return at the end of
  `Manager::remove_rows`, `bauded/src/manager.rs:1199`. Every other exit of that function is an
  `Err`: `Blocked` at `:1086`, `Drift` at `:1111`, and the core error at `:1163`, which restores
  the handles first.
- The handler sees the same fact in the `Ok(report)` arm of `execute_removal`,
  `bauded/src/api.rs:176`, with the manager lock still held. Publishing there, or at the manager
  return, gives one event per commit and orders it against any later mutation.
- Both `saved` and `saved_degraded` are commits: the rows are gone from memory, from `/rows` and
  from a fresh restore in both. Publish on both. `report.rows_removed` is the daemon's own list of
  what left.
- Nothing is committed on any non-200. In particular the 503 from a failed save leaves the row
  stored and its handle restored, although the worktree directory is already gone, and the 409 and
  the busy 503 touch nothing. Publish nothing on those.
- A 500 from a repository removal can leave a child session whose agent already exited. That is a
  session status change, not a removal, and no removal event belongs to it.
- A same-row race produces one 200 and one 404, so a publish at the commit point fires once.
- 21-04 asserts the `matrix_` count of 14.

**Any later plan writing a status test.** Use `assert_refused` for a plain-text refusal,
`assert_drift` for 409 and `assert_executed` for 200. `world_snapshot` with
`assert_world_unchanged` proves a refusal touched nothing.

## Commits

- `6fdf4eb` feat(21-03): answer a missing runtime handle with 503 and prove the refusal touches nothing
- `a152da3` test(21-03): prove each drift set refuses with 409 and echoed fields cannot steer removal
- `0691318` test(21-03): drive each removal failure class by its cause and pin the inherited refusals

## Self-Check: PASSED

- FOUND: `.planning/phases/21-daemon-and-pwa-parity/21-03-SUMMARY.md`
- FOUND: `6fdf4eb`, `a152da3`, `0691318` as ancestors of HEAD
- FOUND: the cited lines `bauded/src/manager.rs:1086`, `:1111`, `:1163`, `:1199` and
  `bauded/src/api.rs:176`, each read back after the last commit
