---
phase: 20-tui-remove-action
fixed_at: 2026-11-05T14:57:06Z
review_path: .planning/phases/20-tui-remove-action/20-REVIEW.md
iteration: 1
findings_in_scope: 4
fixed: 4
skipped: 0
status: all_fixed
---

# Phase 20: Code Review Fix Report

**Fixed at:** 2026-11-05T14:57:06Z
**Source review:** .planning/phases/20-tui-remove-action/20-REVIEW.md
**Iteration:** 1

**Summary:**
- Findings in scope: 4 (scope `critical_warning`: WR-01 to WR-04; IN-01 to IN-05 out of scope and left in REVIEW.md)
- Fixed: 4
- Skipped: 0

Worktrees: `workflow.use_worktrees` is `false`, so every edit and commit landed in the main checkout on `gsd/phase-18-shared-kill-then-remove-lifecycle`. No worktree, temp branch or recovery sentinel was created.

## Fixed Issues

### WR-01: Confirm-time recheck compares only `rows_removed`

**Files modified:** `baude/src/app.rs`
**Commit:** ca979fb
**Applied fix:** `confirm_remove_project` now compares `rows_removed`, `runtimes_killed` and `paths_deleted` against the previewed report and returns `Recheck` when any differ; `cleared` and `already_removed` stay out of the comparison as the review suggested. Added `remove_project_recheck_refreshes_modal_when_runtime_set_changes`, which opens the modal on the repository, reopens the dormant swept-in checkout behind it (so the row set is unchanged but a runtime appears), presses Enter, and asserts the refreshed modal lists the new runtime by pid, the process is still live, no save ran and the footer explains the refresh.
**Status:** fixed: requires human verification (the fix is a condition change; the test pins the intended behaviour but a human should confirm the field set is the one D-05 wants).

### WR-02: Modal scroll offset is unbounded in the key handler

**Files modified:** `baude/src/app.rs`, `baude/src/ui.rs`
**Commit:** f99e095
**Applied fix:** Adapted from the suggestion. Clamping to `len - 1` alone was not enough: the render test still failed because the renderer clamps to `len - max_rows`, so a `k` from `len - 1` stayed inside the renderer's dead zone. The handler now clamps to the same window the renderer draws, via a new `pub(crate) fn remove_project_modal_max_rows(report, error, area_height)` in `ui.rs` (shared with WR-03) and a new `App::terminal_area` field recorded by `sync_sizes` each frame. `handle_modal_key` became `pub(crate)` so the `ui.rs` render tests can drive it. Added `confirm_remove_modal_scroll_past_end_then_up_moves_one_row`: holds `j` three times the list length, presses `k` once, asserts the first visible row moved up by exactly one and `+1 more` is shown.

### WR-03: `draw_modal` height budget is out of sync with the line generator

**Files modified:** `baude/src/ui.rs`
**Commit:** 824a4b2
**Applied fix:** Option (a) from the review, packaged as the helper option (b) asked for. `remove_project_modal_max_rows` calls `remove_project_confirmation_lines` once with `max_rows = usize::MAX` to measure the true fixed line count, subtracts it from the 80% inner height, and reserves one line for `+N more` only when the list overflows. `draw_modal` uses it and `remove_project_modal_max_height`; the hand-maintained `fixed_lines` arithmetic is gone. The 80% render test now asserts `occupied.len() == 24` with the derived window (twelve of 21 rows, `+9 more`, `+4 more` after scrolling five). The narrow retry case used a two-child list that no longer overflows at the correct budget, so it now uses six children and still asserts the prompt, error, legend and `+4 more` are visible at 59x20.

### WR-04: Retry after a failed removal is never exercised across a `tick`

**Files modified:** `baude/src/app.rs`
**Commit:** 09fcd33
**Applied fix:** Test-only. `remove_project_failure_save_keeps_rows_then_retry_succeeds` now calls `app.tick()` over the dead restored handle before the retry; `remove_project_failure_kill_keeps_rows_and_handles_then_retry_succeeds` ticks with the holder still alive and again after it is released. Both retries still return `Removed`, so the production fallback the review described (clearing the persisted runtime when the handle is gone) was not needed: exit detection does not split the handle from the persisted runtime within a tick.

## Skipped Issues

None.

## Verification

Per-fix: each targeted test group was run with `cargo test -p baude <filter>` and the exit code captured directly (WR-01: 2 passed; WR-02/03: 4 passed; WR-04: 2 passed). WR-02's test was observed failing before the window-aware clamp and passing after.

Full gates, run in the main checkout (not a worktree), exit codes captured to files rather than read through a pipe:
- `scripts/gate.sh` (macOS): exit 0. 560 passed / 1 ignored in `baude-core`, 276 passed in `baude`, 107 in `bauded`, clippy `-D warnings` clean.
- `scripts/gate.sh --linux` (`rust:1-bookworm`, uid 1000): exit 0, same counts.

`scripts/check-commit-message.sh --range origin/main..HEAD` exits 0 for all four commits (no nested parentheses).

---

_Fixed: 2026-11-05T14:57:06Z_
_Fixer: Claude (gsd-code-fixer)_
_Iteration: 1_
