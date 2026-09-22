---
phase: 09-hook-seeding-safety
plan: 02
subsystem: testing
tags: [rust, file-locks, try_lock, regression-tests, persist, bauded]

requires:
  - phase: 08-test-isolation
    provides: "TestRedirect RAII isolation, isolated_root persist-test convention, ManagerFixture owner-struct fixture"
provides:
  - "persist::tests::reopen_claims_lock_when_leftover_file_outlives_released_os_lock — pins WLOCK-04's reopen-after-release sequence"
  - "manager::tests::held_lock_save_refuses_with_pid_diagnostic — pins bauded's held-lock StateLockError::Held pid/path diagnostic at first save"
affects: [10-terminal-links, 11-newline-input, 12-release-gate]

actuals:
  tokens: 1397
  tasks: 2
  commits: 2
plan_head_before: 750fece38760675c463108481b8c56d9d1d425ec

tech-stack:
  added: []
  patterns:
    - "External lock holder simulated with raw OpenOptions + try_lock, never hold_state_lock (re-entrant cache bypass, Pitfall 6)"
    - "Lock-path shape mirrored in bauded tests (.{file_name}.lock sibling) since persist::lock_path is private"

key-files:
  created: []
  modified:
    - baude-core/src/persist.rs
    - bauded/src/manager.rs

key-decisions:
  - "bauded held-lock test asserts the EXISTING first-save contention surface (RESEARCH Open Question 1 option (a)) — bauded gains no startup claim; WLOCK-01's startup residual stays documented"
  - "Both tests simulate the foreign owner with a raw file handle so hold_state_lock's re-entrant HELD_STATE_LOCKS cache is never the thing under test"

patterns-established:
  - "Leftover-lock regression vocabulary: stamp fake pid, drop handle, assert file+stale-stamp survive, assert reopen Ok + current-pid re-stamp"

requirements-completed: [WLOCK-01, WLOCK-03, WLOCK-04]

coverage:
  - id: D1
    description: "Reopening a workspace whose lock file remains on disk after the OS lock was released succeeds and re-stamps the current pid (WLOCK-04, D-12)"
    requirement: WLOCK-04
    verification:
      - kind: unit
        ref: "baude-core/src/persist.rs#reopen_claims_lock_when_leftover_file_outlives_released_os_lock (cargo test -p baude-core --lib persist::)"
        status: pass
    human_judgment: false
  - id: D2
    description: "A bauded save under a lock held by another owner surfaces StateLockError::Held's pid + lock-path diagnostic and never removes the other owner's lock file (WLOCK-01/03/02, D-12)"
    requirement: WLOCK-03
    verification:
      - kind: integration
        ref: "bauded/src/manager.rs#held_lock_save_refuses_with_pid_diagnostic (cargo test -p bauded --bins manager::tests::held_lock_)"
        status: pass
    human_judgment: false
  - id: D3
    description: "Both new tests run under Phase-8 isolation (isolated_root / ManagerFixture) and touch no real user root (D-13)"
    verification:
      - kind: unit
        ref: "isolated_root(\"leftover-lock\") in persist test; ManagerFixture::new(\"held-lock\") in manager test — both roots under temp_dir, RAII cleanup"
        status: pass
    human_judgment: false

duration: 5min
completed: 2026-09-16
status: complete
---

# Phase 09 Plan 02: Lock Regression Tests Summary

**Two v2.1.3 inspection-only WLOCK behaviors pinned by automated tests: leftover-lock-file reopen in persist (try_lock decides contention, never file existence) and bauded's held-lock pid diagnostic at first save**

## Performance

- **Duration:** 5 min
- **Started:** 2026-09-16T04:39:53Z
- **Completed:** 2026-09-16T04:45:07Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- `reopen_claims_lock_when_leftover_file_outlives_released_os_lock` (baude-core/src/persist.rs): prior owner simulated with a raw `OpenOptions` + `try_lock` handle stamped with fake pid 999999; after `drop`, the lock FILE and stale stamp survive, and `hold_state_lock` re-claims `Ok(())` and re-stamps the current pid. Pins WLOCK-04: a leftover lock file alone never blocks reopen.
- `held_lock_save_refuses_with_pid_diagnostic` (bauded/src/manager.rs): another owner holds a raw try_lock handle (stamped pid 424242) across `save_checked`; the returned `SaveError` has `replacement_committed() == false` and its Display carries "already owns this workspace", "pid 424242", and the lock file name. The refused save leaves the lock file and its stamp intact (WLOCK-02 contract).
- Both tests run under Phase-8 isolation (`isolated_root`, `ManagerFixture`) — no real user root touched; no production code changed.

## Task Commits

Each task was committed atomically:

1. **Task 1: Reopen-with-leftover-lock-file regression test** - `9b192cd` (test)
2. **Task 2: bauded held-lock pid-diagnostic regression test** - `fa7ceaa` (test)

## Files Created/Modified

- `baude-core/src/persist.rs` - new test in the existing tests mod pinning the leftover-lock reopen sequence (test-only, +46 lines)
- `bauded/src/manager.rs` - new test in the existing tests mod pinning the held-lock save refusal diagnostic (test-only, +70 lines)

## Decisions Made

- Adopted RESEARCH Open Question 1 option (a): the bauded test asserts the existing first-save contention surface via `save_checked`, not a new startup claim — adding one would change the WLOCK contract, which the phase boundary excludes. WLOCK-01's "bauded learns of contention at first save rather than startup" residual stays documented.
- Mirrored persist's private `lock_path` shape (`.{file_name}.lock` sibling) in the bauded test rather than exposing the helper — keeps the diff test-only.

## Deviations from Plan

None - plan executed exactly as written.

## Verification Results

- `cargo test -p baude-core --lib persist::` — 14 passed, 0 failed (new test listed passing)
- `cargo test -p bauded --bins manager::tests::held_lock_` — 1 passed, 0 failed (filter matched)
- Diff test-only: no non-test lines changed in persist.rs or manager.rs (both additions inside `#[cfg(test)] mod tests`)
- `cargo fmt --check` clean; `cargo clippy --workspace --all-targets` clean

## Issues Encountered

None

## User Setup Required

None - no external service configuration required.

## Next Phase Readiness

- Phases 10-12 can no longer silently regress the WLOCK single-writer contract: both inspection-only behaviors are now enforced by CI-runnable tests.
- Ready for 09-03 (remaining hook-seeding plans in this phase).

---
*Phase: 09-hook-seeding-safety*
*Completed: 2026-09-16*

## Self-Check: PASSED
