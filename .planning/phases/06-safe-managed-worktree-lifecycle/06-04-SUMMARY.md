---
phase: 06-safe-managed-worktree-lifecycle
plan: 04
subsystem: repository-lifecycle
tags: [rust, retained-session, tdd, durable-state, commit-boundary]

requires:
  - phase: 06-02
    provides: Commit-aware persistence stages and durable checkout-key runtime ownership
provides:
  - Backward-compatible opaque conversation resume metadata on retained children
  - Shared snapshot-save-stop retained-close transition without hierarchy deletion
  - App and Manager close adapters that honor atomic replacement commit stages
affects: [06-05, phase-07-hierarchy, phase-08-daemon-parity]

tech-stack:
  added: []
  patterns: [opaque retained metadata, explicit ordered close effects, save-before-stop, commit-aware runtime teardown]

key-files:
  created: []
  modified: [baude-core/src/repository.rs, baude-core/src/lifecycle.rs, baude-core/src/persist.rs, baude/src/app.rs, bauded/src/manager.rs, bauded/src/api.rs]

key-decisions:
  - "Retained conversation IDs are optional opaque strings with an explicit serde default and never participate in path or ownership identity."
  - "Close plans snapshot runtime context, save inactive intent, then stop exactly one checkout-key runtime while retaining checkout and repository membership."
  - "Pre-replacement close failures restore memory and leave the runtime live; post-replacement directory-sync failures keep inactive memory, stop the runtime, and mark persistence dirty."

patterns-established:
  - "Runtime owners consume one shared close transition and may detach a runtime only after the durable replacement boundary authorizes inactive intent."
  - "Compatibility DELETE means retained close, not checkout or repository deletion."

requirements-completed: [WORK-03]

duration: 13min
completed: 2026-08-30
---

# Phase 6 Plan 4: Retained Session Close Summary

**Managed session close now preserves exact checkout and conversation context while committing inactive intent before stopping one App or daemon runtime**

## Performance

- **Duration:** 13 min
- **Started:** 2026-08-30T20:47:22Z
- **Completed:** 2026-08-30T21:00:22Z
- **Tasks:** 2
- **Files modified:** 6

## Accomplishments

- Added compatible optional `resume_id` persistence with exact hostile-looking opaque string round trips and unchanged strict unknown-field rejection.
- Added a shared close plan whose observable effects are snapshot runtime metadata, save inactive intent, then stop runtime, with no child, branch, path, parent, key, role, ordering, ownership, health, or setting deletion.
- Routed local keep-worktree close and daemon compatibility DELETE through the same transition and checkout-key runtime association.
- Proved rename-stage failures leave the process and aggregate untouched, while committed directory-sync failures retain inactive memory, stop the runtime, surface degradation, and mark persistence dirty.

## Task Commits

Both tasks followed RED then GREEN TDD, with one validation-contract fix:

1. **Task 1 RED: retained schema and close transition contracts** - `0bf428a` (test)
2. **Task 1 GREEN: compatible retained close state and ordered plan** - `e0cc63e` (feat)
3. **Task 2 RED: App and Manager retained-close contracts** - `0b646f4` (test)
4. **Task 2 GREEN: commit-aware owner close adapters** - `a6cb2a3` (feat)
5. **Validation fix: API atomic-failure retained-state expectation** - `96cc2d4` (test)

## Files Created/Modified

- `baude-core/src/repository.rs` - Adds optional opaque `resume_id` with backward-compatible deserialization.
- `baude-core/src/lifecycle.rs` - Defines the complete close request, explicit ordered effects, aggregate transition, and closed outcome.
- `baude-core/src/persist.rs` - Initializes legacy migration metadata and exercises retained resume IDs in persistence fixtures.
- `baude/src/app.rs` - Snapshots live local metadata, saves inactive intent, and only then detaches and stops the checkout-key runtime.
- `bauded/src/manager.rs` - Makes compatibility DELETE retain child and parent while applying identical commit-stage behavior.
- `bauded/src/api.rs` - Updates the existing atomic-failure regression to assert retained inactive checkout evidence.

## Decisions Made

- Used the complete `RetainedSessionState` as the close runtime snapshot so every user setting and observed backend session ID crosses one typed boundary together.
- Kept explicit close effects inspectable in core while App and Manager continue owning persistence and PTY teardown.
- Preserved the daemon's existing response/status behavior: a directory-sync degradation remains an error even though the replacement committed and the runtime was safely stopped.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 3 - Blocking] Updated persistence migration and fixtures for the compatible schema field**
- **Found during:** Task 1 GREEN
- **Issue:** Adding `resume_id` made core migration and persistence struct literals incomplete even though `persist.rs` was omitted from the plan's file list.
- **Fix:** Initialized migrated legacy records with `None` and exercised a present opaque value in the current-state fixture.
- **Files modified:** `baude-core/src/persist.rs`
- **Verification:** Core close tests and all 188 core tests pass.
- **Committed in:** `e0cc63e`

**2. [Rule 3 - Blocking] Aligned the existing API atomic-failure test with retained DELETE semantics**
- **Found during:** Overall workspace verification
- **Issue:** The API regression still expected a committed daemon DELETE to erase the checkout, contradicting WORK-03's new compatibility-close contract.
- **Fix:** Kept the 503 status assertion and changed durable assertions to require one repository, one child, and commit-stage-appropriate inactive intent.
- **Files modified:** `bauded/src/api.rs`
- **Verification:** The focused API test and complete CI triad pass.
- **Committed in:** `96cc2d4`

**3. [Rule 3 - Blocking] Reconciled SDK progress output during parallel plan completion**
- **Found during:** Plan metadata update
- **Issue:** `state.update-progress` counted seven completed plans but projected zero completed phases and 0%, while concurrent Plan 06-03 had already advanced the correct plan position and phase counts.
- **Fix:** Preserved the concurrency-correct Plan 5 position and reconciled milestone and Phase 6 progress with the four summaries and ROADMAP counts.
- **Files modified:** `.planning/STATE.md`
- **Verification:** STATE records 7/9 v2.0 plans, 4/6 Phase 6 plans, and the 06-04 metric while ROADMAP reports 4/6.
- **Committed in:** plan metadata commit

---

**Total deviations:** 3 auto-fixed (3 Rule 3 blocking issues)
**Impact on plan:** Both changes were required to compile and verify the planned compatible retained-close behavior; no endpoint or physical-removal scope was added.

## Issues Encountered

- Concurrent Plan 06-03 RED commits temporarily prevented core unit-test compilation; focused owner tests ran independently, then all requested filters and the full gate passed after 06-03 reached GREEN.

## User Setup Required

None - no dependencies, manifests, external services, or configuration changes were added.

## TDD Gate Compliance

- RED `0bf428a` failed on the absent schema field and close transition before GREEN `e0cc63e` passed both core contracts.
- RED `0b646f4` failed on the absent App close adapter and old Manager deletion behavior before GREEN `a6cb2a3` passed owner success and commit-stage vectors.
- No refactor commit was needed; the implementation remained minimal and the complete formatting, Clippy, and test gate passed.

## Verification

- `cargo test -p baude-core lifecycle::tests::close -- --nocapture` - 2 passed.
- `cargo test lifecycle_close -- --nocapture` - 4 App/Manager tests passed.
- `cargo test -p bauded --bin bauded real_atomic_persistence_failures_are_503_for_every_mutation -- --nocapture` - passed.
- `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test` - 286 tests passed (25 baude, 188 baude-core, 73 bauded).
- `Cargo.toml` and `Cargo.lock` are unchanged.

## Known Stubs

None.

## Self-Check: PASSED

- All six modified source files and this summary exist.
- RED/GREEN and validation commits `0bf428a`, `e0cc63e`, `0b646f4`, `a6cb2a3`, and `96cc2d4` are present in Git history.

## Next Phase Readiness

- Plan 06-05 can consume the retained opaque ID for targeted backend reopen without changing close semantics.
- Plans 06-06 and Phase 7 can distinguish retained inactive children from physical worktree removal and hierarchy deletion.
- No new network endpoint, authentication path, dependency, schema trust boundary beyond the planned compatible field, or physical removal behavior was introduced.

---
*Phase: 06-safe-managed-worktree-lifecycle*
*Completed: 2026-08-30*
