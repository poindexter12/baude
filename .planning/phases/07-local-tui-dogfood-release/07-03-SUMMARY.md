---
phase: 07-local-tui-dogfood-release
plan: 03
subsystem: ui
tags: [rust, ratatui, responsive-layout, unicode, terminal-cells, tdd]

requires:
  - phase: 07-local-tui-dogfood-release
    plan: 01
    provides: Durable repository and checkout hierarchy with key-based selection
  - phase: 07-local-tui-dogfood-release
    plan: 02
    provides: Capability-gated lifecycle actions and durable interaction routing
provides:
  - Exact split and single-pane responsive layout for the contracted viewport matrix
  - Unicode-cell-safe hierarchy clipping, padding, scrolling, and selection bands
  - Visibility-aware PTY resize and immediate tiny-height shell focus transfer
  - Capability-derived hints, exact close/remove confirmations, and durable-context overlays
affects: [07-04, 07-05, local-tui, dogfood, release-certification]

tech-stack:
  added: []
  patterns:
    - Layout visibility is shared by rendering and PTY resize/focus routing
    - Ratatui Line width and local cell clusters drive all hierarchy clipping and padding
    - ActionView is the sole source for contextual hierarchy hint capability

key-files:
  created: []
  modified: [baude/src/app.rs, baude/src/ui.rs]

key-decisions:
  - "Responsive layout exposes explicit pane visibility so hidden single-pane content is never resized or used as an input target."
  - "Hierarchy strings are clipped by terminal cell clusters with branch/path tails preserved, without adding a Unicode or snapshot dependency."
  - "Full and narrow hint text is derived from ActionView capability rather than status glyphs, runtime absence, or cause copy."

patterns-established:
  - "Visibility authority: the same responsive LayoutRects controls render, PTY resize, and tiny-height focus transfer."
  - "Exact-copy helpers: modal bodies and hint matrices remain directly assertable at wide and tiny widths."

requirements-completed: [SURF-01, SURF-02]

duration: 30min
completed: 2026-08-31
---

# Phase 7 Plan 3: Responsive Local Hierarchy UI Summary

**Ratatui hierarchy rendering now honors the exact viewport, Unicode-cell, focus, hint, confirmation, and durable interaction contract without changing lifecycle authority.**

## Performance

- **Duration:** 30 min
- **Started:** 2026-08-31T08:50:17Z
- **Completed:** 2026-08-31T09:20:23Z
- **Tasks:** 2
- **Files modified:** 2

## Accomplishments

- Implemented exact 42-column, clamped medium, 26-column compact, and focus-selected single-pane layouts across 160×40 through 40×12.
- Made hierarchy truncation, suffix preservation, padding, scrolling, borders, and selected bands terminal-cell aware for wide and combining Unicode.
- Prevented hidden or zero-area panes from receiving PTY resizes and transferred tiny-height shell focus to a live agent or sidebar with exact messages.
- Added all eight literal full-width hints, both retained-child narrow variants, exact persistence/pending/result copy, and distinct cyan close versus red managed-remove confirmation anatomy.
- Kept repository and retained checkout Info, Activity, and GSD views usable from durable path context while preserving flat remote compatibility behavior.

## Viewport and Focus Evidence

| Contract | Automated evidence |
|---|---|
| 160×40, 100×30, 79×24, 59×20, 40×12 | `ui::tests::hierarchy_viewport_matrix_renders_without_panic_and_preserves_semantics` |
| Wide/combining cells, tail clipping, scroll, border-safe selected band | `ui::tests::hierarchy_unicode_width_scroll_and_selection_band_are_cell_correct` |
| Hidden-size retention, positive minimums, both tiny-height focus messages | `app::tests::hierarchy_resize_never_sends_zero_dimensions_and_transfers_hidden_shell_focus` |
| Exact close/remove target and safe-key modal anatomy | `ui::tests::hierarchy_modals_name_exact_targets_and_distinguish_close_from_remove` |
| Empty, persistence, pending, success, no-runtime, full and narrow hint copy | `ui::tests::hierarchy_copy_contract_matches_ui_spec_for_empty_pending_success_and_hints` |
| Existing durable local action routing | `app::tests::hierarchy_action_matrix_dispatches_only_authorized_local_actions` |

## Task Commits

Each TDD task was committed as a RED/GREEN pair:

1. **Task 1: Enforce the viewport matrix, cell width, and visible-pane focus contract**
   - `e0c9e99` — failing responsive hierarchy tests
   - `42527e7` — responsive viewport, Unicode cells, and focus/resize implementation
2. **Task 2: Finish exact rows, contextual hints, confirmations, help, and applicable actions**
   - `fe09417` — failing exact modal and copy tests
   - `2fa1769` — exact copy, modal, status, and durable overlay implementation

## Files Created/Modified

- `baude/src/ui.rs` — Responsive modes, cell-safe layout helpers, hierarchy scrolling/anatomy, contextual hints, help, overlays, and separate confirmation rendering.
- `baude/src/app.rs` — Visible-pane resize authority, tiny-height shell focus transfer, exact persistence/reopen messages, and durable target access for rendering.

## Decisions Made

- Single-pane mode follows focus: sidebar focus renders only hierarchy, while agent/shell focus renders only content; returning to a larger viewport never steals focus.
- Hidden PTYs retain their last valid dimensions. Resizes occur only for visible positive inner rectangles, with remote attach minimums still enforced at 2×10.
- No new Unicode crate was added. Ratatui `Line::width()` measures cells while a local cluster pass keeps zero-width combining marks attached during tail-preserving clipping.
- Repository and retained overlays resolve canonical durable paths rather than requiring a live runtime decoration.

## Deviations from Plan

### Auto-fixed Issues

**1. [Rule 1 - Bug] Restored milestone progress after state-handler miscalculation**
- **Found during:** Plan metadata finalization
- **Issue:** `state.update-progress` counted zero completed milestone phases because Phase 6 remains in certification, resetting the displayed milestone progress from 33% to 0%; its aggregate velocity rows also remained stale.
- **Fix:** Restored the completed Phase 5 count and 33% progress, then updated the observed 13-plan and Phase 7 execution metrics without marking Phase 6 or Phase 7 complete.
- **Files modified:** `.planning/STATE.md`
- **Verification:** State reports one of three milestone phases complete, 13 of 16 plans executed, and Phase 7 at 3/6 while certification remains pending.
- **Committed in:** Plan metadata commit

---

**Total deviations:** 1 auto-fixed bug
**Impact on plan:** The correction preserves accurate planning metadata only; implementation scope and certification status are unchanged.

## Issues Encountered

- Wide and combining glyphs occupy continuation cells in Ratatui buffers, so tests assert terminal cell width and preserved semantic suffixes rather than scalar-counted strings.
- Existing test fixtures print benign Git cleanup diagnostics for already-removed temporary worktrees; all authoritative tests still pass.

## Verification

- All six guarded exact tests passed with `cargo test -- --list | rg -Fx` presence guards.
- `cargo fmt --all -- --check` passed.
- `cargo clippy --workspace --all-targets -- -D warnings` passed.
- `cargo test --workspace` passed: 43 TUI/App, 213 core, and 78 daemon tests (334 total).
- `git diff --check` passed.
- No dependencies were added; no publish, push, or PR command was run.

## User Setup Required

None - no external service configuration required.

## Known Stubs

None found in files created or modified by this plan.

## Threat Flags

| Flag | File | Description |
|---|---|---|
| threat_flag: file-access | `baude/src/ui.rs` | Repository/retained GSD overlays read `.planning/STATE.md` through the existing parser using the canonical durable repository path. |

## TDD Gate Compliance

- RED commits: `e0c9e99`, `fe09417`
- GREEN commits: `42527e7`, `2fa1769`
- Both guarded RED commits failed on missing responsive/copy behavior before their corresponding GREEN implementation commits.

## Next Phase Readiness

- Automated SURF-01/SURF-02 viewport, copy, and routing evidence is green and ready for isolated local dogfood.
- Manual visual dogfood, screenshots, Linux/runtime certification, independent review, phase verification, and release certification remain pending. Nothing was published or pushed.

## Self-Check: PASSED

- Both implementation files and this summary exist.
- All four RED/GREEN task commits are present in git history.
- `git diff --check` passes.

---
*Phase: 07-local-tui-dogfood-release*
*Completed: 2026-08-31*
