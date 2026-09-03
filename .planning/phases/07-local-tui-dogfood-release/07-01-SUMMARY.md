---
phase: 07-local-tui-dogfood-release
plan: 01
subsystem: ui
tags: [rust, ratatui, hierarchy, durable-identity, navigation]

requires:
  - phase: 05-persistent-local-session-lifecycle
    provides: Durable repository and checkout identity with persisted lifecycle state
provides:
  - Durable repository-parent and checkout-child hierarchy projection
  - Ratatui rendering for persistent local rows independent of runtime presence
  - RepositoryKey and CheckoutKey selection reconciliation across refresh and removal
affects: [07-02, 07-03, 07-04, local-tui, release-certification]

tech-stack:
  added: []
  patterns:
    - Pure projection from durable repository state with runtime-only decoration joins
    - Durable-key selection reconciled after volatile refreshes

key-files:
  created: [baude/src/hierarchy.rs]
  modified: [baude/src/main.rs, baude/src/app.rs, baude/src/ui.rs, .planning/phases/07-local-tui-dogfood-release/07-VALIDATION.md]

key-decisions:
  - "Structural hierarchy and ordering come only from persisted repository state; runtime, status, and archive facts decorate rows without determining identity or order."
  - "Normal navigation visits repository parents, while cycle navigation visits only actionable checkout and remote rows."
  - "Invalid local selection falls back within the repository before its parent; restart falls back to the first local parent, then the first remote row."

patterns-established:
  - "Durable hierarchy projection: project persisted parent/child identity first, then join volatile decoration by CheckoutKey."
  - "Selection reconciliation: preserve RepositoryKey or CheckoutKey when present and apply deterministic local fallback when absent."

requirements-completed: []

duration: 17min
completed: 2026-08-31
---

# Phase 7 Plan 1: Durable Hierarchy Projection and Navigation Summary

**Persistent repository parents and checkout children now render from durable state and retain key-based selection across refresh, removal, and focus changes.**

## Performance

- **Duration:** 17 min
- **Started:** 2026-08-31T07:40:21Z
- **Completed:** 2026-08-31T07:57:09Z
- **Tasks:** 2
- **Files modified:** 5

## Accomplishments

- Added deterministic parent/child projection ordered by repository path identity and persisted checkout order.
- Rendered runtime-less main/default/worktree children in the real Ratatui sidebar while preserving separate remote rows.
- Reconciled durable selection across refresh, local removal, restart fallback, and sidebar focus changes.
- Covered production rendering and navigation with exact-name guarded tests and passed the full workspace quality gate.

## Task Commits

Each TDD task was committed as a RED/GREEN pair:

1. **Task 1: Project and render the durable hierarchy**
   - `1646b58` — test RED
   - `02cdfe7` — feature GREEN
2. **Task 2: Preserve durable selection and navigation behavior**
   - `02aa85d` — test RED
   - `5252f4f` — feature GREEN

## Files Created/Modified

- `baude/src/hierarchy.rs` — Pure hierarchy projection, stable ordering, durable selection reconciliation, and tests.
- `baude/src/main.rs` — Registers the hierarchy module.
- `baude/src/app.rs` — Projects real repository state and applies key-based navigation, refresh, and removal behavior.
- `baude/src/ui.rs` — Renders repository parents and persistent checkout children in Ratatui.
- `.planning/phases/07-local-tui-dogfood-release/07-VALIDATION.md` — Records observed automated evidence without claiming UAT or certification.

## Decisions Made

- Structural ordering is based only on durable repository and checkout facts. Runtime, status, and archive state are decoration joins so process churn cannot reorder the hierarchy.
- `j`/`k` traverse repository parents and children; cycle navigation deliberately skips non-actionable parents.
- Local removal fallback stays inside the selected repository when possible, while unrelated remote selection remains unchanged.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Corrected cycle navigation when starting on a repository parent**
- **Found during:** Task 2 (Preserve durable selection and navigation behavior)
- **Issue:** A generic index fallback could skip the first checkout when cycle navigation started from a parent row.
- **Fix:** Made cycle fallback direction-aware so forward and reverse traversal land on the correct actionable child.
- **Files modified:** `baude/src/app.rs`
- **Verification:** The guarded hierarchy navigation test and full workspace suite pass.
- **Committed in:** `5252f4f`

**2. [Rule 1 - Bug] Preserved unrelated remote selection after local checkout removal**
- **Found during:** Task 2 (Preserve durable selection and navigation behavior)
- **Issue:** Applying local removal fallback indiscriminately could replace a still-valid remote selection.
- **Fix:** Reconcile removal fallback only when the removed checkout was selected; otherwise retain and reconcile the current selection normally.
- **Files modified:** `baude/src/app.rs`, `baude/src/hierarchy.rs`
- **Verification:** Durable selection tests, navigation tests, and the full workspace suite pass.
- **Committed in:** `5252f4f`

**3. [Rule 1 - Bug] Restored milestone progress after state-handler miscalculation**
- **Found during:** Plan metadata finalization
- **Issue:** `state.update-progress` counted zero completed milestone phases because Phase 6 remains in certification, even though Phase 5 is complete, and reset the displayed phase progress from 33% to 0%.
- **Fix:** Restored the completed-phase count and 33% phase progress while retaining the handler's accurate 11/16 plan totals and keeping Phase 6 certification pending.
- **Files modified:** `.planning/STATE.md`
- **Verification:** State now reports one of three milestone phases complete, 11 of 16 plans executed, and does not mark Phase 6 or Phase 7 complete.
- **Committed in:** Plan metadata commit

---

**Total deviations:** 3 auto-fixed bugs
**Impact on plan:** Two corrections enforce deterministic navigation semantics; one preserves accurate planning metadata. No scope expansion or dependency changes.

## Issues Encountered

- Editor diagnostics briefly reported stale hierarchy/import errors after edits; authoritative `cargo check`, Clippy, and workspace tests all passed.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None found in files created or modified by this plan.

## TDD Gate Compliance

- RED commits: `1646b58`, `02aa85d`
- GREEN commits: `02cdfe7`, `5252f4f`
- Both RED commits preceded their corresponding GREEN implementations.

## Next Phase Readiness

- Durable hierarchy and selection behavior are ready for later Phase 7 visual polish and dogfood plans.
- Human UAT and release certification intentionally remain pending in Plans 07-04 and 07-05.

## Self-Check: PASSED

- All implementation and evidence files exist.
- All four RED/GREEN task commits are present in git history.
- `git diff --check` passes.

---
*Phase: 07-local-tui-dogfood-release*
*Completed: 2026-08-31*
