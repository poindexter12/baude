---
phase: 06-safe-managed-worktree-lifecycle
plan: 05
subsystem: repository-lifecycle
tags: [rust, retained-session, targeted-resume, pty, tdd, reconciliation]

requires:
  - phase: 06-04
    provides: Retained checkout children with opaque conversation IDs and commit-aware close
provides:
  - Typed Fresh, ContinueLatest, and ResumeId backend spawn modes
  - Opaque PTY environment transport for hostile conversation IDs
  - Fail-closed reconcile-save-dispatch reopen planning with checkout reservations
  - Shared App and Manager retained-checkout reopen adapters
affects: [06-06, phase-07-hierarchy, phase-08-daemon-parity]

tech-stack:
  added: []
  patterns: [opaque process environment, reconcile-before-intent, save-before-runtime, checkout-key reopen reservation]

key-files:
  created: []
  modified: [baude-core/src/backend/mod.rs, baude-core/src/backend/claude.rs, baude-core/src/backend/opencode.rs, baude-core/src/pty.rs, baude-core/src/lifecycle.rs, baude/src/app.rs, bauded/src/manager.rs]

key-decisions:
  - "Targeted resume IDs travel only as BAUDE_RESUME_ID process environment data while static backend command templates reference the fixed quoted variable."
  - "Reopen changes durable active intent only after fresh exact checkout reconciliation and persists that intent before focus, restart, or spawn."
  - "Same-checkout reopen reservations return a typed pending outcome, while conflicting repository mutations remain busy."

patterns-established:
  - "Retained resume selects ResumeId when observed and ContinueLatest only when no conversation ID exists."
  - "App and Manager restore, reopen, and restart through checkout-key ownership without path or display-name runtime identity."

requirements-completed: [WORK-04]

duration: 11min
completed: 2026-08-30
---

# Phase 6 Plan 5: Secure Retained Checkout Reopen Summary

**Retained main and managed checkouts now reconcile exact Git topology, durably activate, and resume the intended Claude Code or OpenCode conversation through one checkout-key runtime**

## Performance

- **Duration:** 11 min
- **Started:** 2026-08-30T21:04:56Z
- **Completed:** 2026-08-30T21:15:58Z
- **Tasks:** 3
- **Files modified:** 7

## Accomplishments

- Replaced boolean backend resume selection with typed fresh, directory-latest, and exact-ID modes for both supported backends.
- Kept hostile retained IDs outside shell syntax by attaching them directly to the PTY child environment.
- Added fail-closed reopen planning for missing, moved, branch-changed, detached, locked, prunable, and identity-changed checkouts.
- Generalized App and Manager restore/reopen paths to persist active intent before exactly one focus, restart, or spawn effect.
- Preserved retained names, shell state, archive state, branch context, and retryable active children across spawn and persistence failures.

## Task Commits

Each task followed RED then GREEN TDD, followed by one refactor:

1. **Task 1 RED: targeted resume transport contracts** - `b52c3bb` (test)
2. **Task 1 GREEN: opaque typed backend resume transport** - `dd68bd1` (feat)
3. **Task 2 RED: retained reopen transition contracts** - `6eb38b8` (test)
4. **Task 2 GREEN: fail-closed reopen planning and reservations** - `f41f675` (feat)
5. **Task 3 RED: App and Manager owner reopen contracts** - `3b7a046` (test)
6. **Task 3 GREEN: shared retained checkout reopen execution** - `0c0c918` (feat)
7. **REFACTOR: remove superseded primary-only dispatch seam** - `5f7fefe` (refactor)

## Files Created/Modified

- `baude-core/src/backend/mod.rs` - Defines typed spawn modes and opaque environment-bearing spawn plans.
- `baude-core/src/backend/claude.rs` - Maps exact IDs to Claude Code `--resume` through a fixed environment reference.
- `baude-core/src/backend/opencode.rs` - Maps exact IDs to OpenCode `--session` while preserving port and prompt configuration.
- `baude-core/src/pty.rs` - Applies process environment values directly at the PTY command boundary.
- `baude-core/src/lifecycle.rs` - Plans reconciliation-gated active intent, runtime dispatch, and reopen reservations.
- `baude/src/app.rs` - Reopens any retained local checkout and targets retained IDs on managed restart.
- `bauded/src/manager.rs` - Reuses the same reopen plan for daemon restore, restart, and explicit checkout reopen.

## Decisions Made

- Used one fixed `BAUDE_RESUME_ID` variable for both backends so persisted values never enter command construction, option parsing, or filesystem identity.
- Represented a duplicate in-flight request as `ReopenPending` rather than permitting a second spawn path; unrelated same-repository mutation remains `Busy`.
- Persisted unavailable health after failed reconciliation while leaving inactive intent and runtime ownership unchanged.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Removed the superseded primary-only dispatch seam**
- **Found during:** Overall Clippy verification
- **Issue:** Generalizing reopen through the shared lifecycle plan left `PrimaryDispatch`, `primary_dispatch`, and `commit_then_spawn` unused, causing the required `-D warnings` gate to fail.
- **Fix:** Removed the obsolete primary-only helpers and their implementation-specific tests; retained save-before-runtime behavior is covered by the new core and owner reopen contracts.
- **Files modified:** `baude/src/app.rs`
- **Verification:** Focused reopen tests, Clippy, and the complete workspace gate pass.
- **Committed in:** `5f7fefe`

**2. [Rule 3 - Blocking] Reconciled SDK-generated progress metadata**
- **Found during:** Plan metadata update
- **Issue:** `state.update-progress` found eight summaries but projected zero completed phases and 0%, leaving Phase 6 and milestone velocity stale.
- **Fix:** Reconciled milestone completion, Phase 6's 5/6 count, velocity, and execution totals against summaries and ROADMAP.
- **Files modified:** `.planning/STATE.md`
- **Verification:** STATE now reports 8/9 v2.0 plans, one completed phase, 5/6 Phase 6 plans, and 89% milestone progress.
- **Committed in:** plan metadata commit

---

**Total deviations:** 2 auto-fixed (2 Rule 3 blocking issues)
**Impact on plan:** The source refactor and metadata reconciliation were required for warning-free verification and accurate planning state; no product scope was added.

## Issues Encountered

- The first complete Clippy run identified the obsolete primary dispatch seam after Task 3 generalized the behavior; removing it restored the warning-free gate.

## User Setup Required

None - no dependencies, manifests, external services, or configuration changes were added.

## TDD Gate Compliance

- RED `b52c3bb` failed on missing `SpawnMode` and opaque environment transport before GREEN `dd68bd1` passed both backend suites.
- RED `6eb38b8` failed on missing reopen plan, dispatch, and reservation contracts before GREEN `f41f675` passed all core reopen vectors.
- RED `3b7a046` failed on missing App/Manager reopen entrypoints and outcome before GREEN `0c0c918` passed both owner adapters.
- Refactor `5f7fefe` preserved focused and complete workspace behavior.

## Verification

- `cargo test -p baude-core backend:: -- --nocapture` - 18 passed.
- `cargo test -p baude-core lifecycle::tests::reopen -- --nocapture` - 3 passed.
- `cargo test lifecycle_reopen -- --nocapture` - App and Manager contracts passed.
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` - 291 tests passed (24 baude, 193 baude-core, 74 bauded).
- `Cargo.toml` and `Cargo.lock` are unchanged.

## Known Stubs

None.

## Self-Check: PASSED

- All seven modified source files and this summary exist.
- RED/GREEN/refactor commits `b52c3bb`, `dd68bd1`, `6eb38b8`, `f41f675`, `3b7a046`, `0c0c918`, and `5f7fefe` are present in Git history.

## Next Phase Readiness

- Plan 06-06 can reuse targeted retained reopen as runtime compensation after a second-preflight or Git-removal refusal.
- Phase 7 can invoke checkout-key reopen for retained main and managed children without weakening topology checks.
- No new endpoint, authentication path, dependency, schema, or unplanned trust-boundary surface was introduced.

---
*Phase: 06-safe-managed-worktree-lifecycle*
*Completed: 2026-08-30*
