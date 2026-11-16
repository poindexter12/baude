---
phase: 20-tui-remove-action
reviewed: 2026-11-05T00:00:00Z
depth: standard
files_reviewed: 4
files_reviewed_list:
  - baude-core/src/removal.rs
  - baude-core/src/testing.rs
  - baude/src/app.rs
  - baude/src/ui.rs
findings:
  critical: 0
  warning: 4
  info: 5
  total: 9
  necessity: 0
status: issues_found
---

# Phase 20: Code Review Report

**Reviewed:** 2026-11-05
**Depth:** standard
**Files Reviewed:** 4
**Status:** issues_found

## Summary

Reviewed the Phase 20 diff (`4ec8989..HEAD`) across the four listed files, concentrating on the changed hunks and the callers they wire into: the `D` dispatch in `sidebar_action`, `Modal::ConfirmRemove`, `open_remove_project_modal`, `confirm_remove_project` and its handle move/restore helpers, `select_after_removal`, the `draw_modal` arm and `remove_project_confirmation_lines` in `ui.rs`, and the live-runtime doubles hoisted from `removal.rs` tests into `baude_core::testing`.

The core wiring is sound. The reservation is held for the whole of `confirm_remove_project` (`_reservation` is a named binding, not `_`), the previewed set is recomputed under that reservation before any handle moves, `collect_row_runtimes` only drains sessions for rows the fresh report names, and on `Err` every handle goes back in id order while `runtime_checkouts` / `runtime_standalones` are left untouched so the mapping survives. On `Ok` the cleanup matches the existing close path at `app.rs:1667-1672` (drop from `sessions`, drop the runtime map entry), so there is no new per-session state left dangling. Tests use the cloned `AdmissionRepo` fixture, real process groups via `arm_unkillable_runtime`, and assert on state-file bytes and group extinction rather than source text. No source-grep assertions were added by this phase (the `include_str!("app.rs")` in `ui.rs` tests predates it).

The `removal.rs` change is a pure extraction of test doubles into `testing.rs` with one call-site rename (`holder.0` to `holder.pid()`); nothing in the production removal path moved. `testing.rs` remains gated on `any(test, feature = "test-support")`, so nothing here reaches a release binary.

Findings are confined to the TUI layer: the confirm-time recheck is narrower than the D-05 promise, the modal scroll offset is unbounded in the key handler, the modal height budget in `draw_modal` has drifted from the line generator it budgets for, and the retry path is never exercised across a `tick`. No security issues; no data-loss path found.

Necessity review: `$HOME/.poindexter/claude/gsd-core/references/necessity-review.md` was not read during this review, so no `necessity:` findings are classified; the frontmatter count is 0.

## Warnings

### WR-01: Confirm-time recheck compares only `rows_removed`, so a runtime that appears after preview is killed without being announced

**File:** `baude/src/app.rs:4014`
**Issue:** D-08 recomputes the report at confirm time, but the refresh trigger is `fresh.rows_removed != previewed.rows_removed`. The row set is not the only thing that changes while the modal is open: `tick()` keeps running under the modal, and Phase B restore gating (`restore_in_progress`, `stop_sessions_added_since`) can admit a session for a row in the set between preview and confirm. In that case `fresh.runtimes_killed` gains an entry, `rows_removed` is unchanged, and the removal proceeds against a modal whose "runtimes killed" section still reads `none`. That is exactly the "confirmation promises less than the real run performs" case D-05 rules out. The CONTEXT left the comparison breadth to discretion, but the discretion was between `rows_removed` and the whole report, and the cheap middle ground is missing. `RemovalReport` is `Eq`, so comparing the action-bearing fields costs one line.
**Fix:**
```rust
let action_changed = fresh.rows_removed != previewed.rows_removed
    || fresh.runtimes_killed != previewed.runtimes_killed
    || fresh.paths_deleted != previewed.paths_deleted;
if action_changed {
    return Ok(RemoveProjectOutcome::Recheck(fresh));
}
```
Leave `cleared` and `already_removed` out of the comparison: both come from filesystem probes and would make the recheck flap on advisory state. Add a test that launches a runtime on a row after `D` opens the modal and asserts Enter yields `Recheck` rather than `Removed`.

### WR-02: Modal scroll offset is unbounded in the key handler, so `k` after over-scrolling appears dead

**File:** `baude/src/app.rs:5826` (clamp lives at `baude/src/ui.rs:2503`)
**Issue:** `j`/`Down` does `*scroll = scroll.saturating_add(1)` with no upper bound; the only clamp is in the renderer, `scroll.min(rows.len().saturating_sub(max_rows))`. The renderer's clamp is display-only and never written back. Holding `j` on a six-row list for a second leaves `scroll` in the tens; each `k` then decrements an invisible surplus and the list does not move until the surplus is gone. The render test `remove_project_lines_repository_parent_lists_every_child_and_scrolls` passes `99` and sees the last page, which proves the clamp but masks the handler defect because it never presses `k` afterwards.
**Fix:** Clamp in the handler against the report, which the modal owns:
```rust
KeyCode::Char('j') | KeyCode::Down => {
    if let Modal::ConfirmRemove { scroll, report, .. } = &mut self.modal {
        let last = report.rows_removed.len().saturating_sub(1);
        *scroll = (*scroll + 1).min(last);
        self.dirty = true;
    }
}
```
`max_rows` is only known at draw time, so clamp to `len - 1` here and keep the renderer's tighter clamp for the window. Add an App-level test: `j` past the end, one `k`, assert the first visible row changed.

### WR-03: `draw_modal` height budget is out of sync with the line generator it budgets for

**File:** `baude/src/ui.rs:1790-1801`
**Issue:** `fixed_lines` is a hand-maintained duplicate of what `remove_project_confirmation_lines` emits, and it is already wrong in both directions. It counts three "blank" lines (comment at line 1790: "prompt, blank, 'rows dropped', blank, legend, blank") that the generator never pushes, and it does not count the `+N more` line the generator pushes whenever the list overflows (line 2509-2511). Today the three phantom blanks absorb the uncounted `+N more` line with two rows to spare, so nothing clips, and the 80% test only asserts `<= 24`, not `== 24`. The failure mode is one honest edit away: anyone who "fixes" the comment by dropping the blanks, or adds a section to the generator without touching this arithmetic, pushes `lines.len() + 2` past `max_height`, and `height.min(max_height)` clips from the bottom, which is where the legend and the error line live. The modal would then show a retry prompt with no legend.
**Fix:** Derive the budget from the generator instead of duplicating it. Either (a) call `remove_project_confirmation_lines` once with `max_rows = usize::MAX` and `scroll = 0`, measure `lines.len() - rows.len()` as the true fixed count (plus one for `+N more`), then call again with the derived `max_rows`; or (b) move the budgeting into a `pub(crate) fn remove_project_modal_layout(report, error, max_height) -> (max_rows, height)` beside the generator so the two are maintained together and unit-tested with the exact-height case (`assert_eq!(occupied.len(), 24)` for the 20-child fixture, not `<=`).

### WR-04: Retry after a failed removal is never exercised across a `tick`, so exit-detection interference is unproven

**File:** `baude/src/app.rs:12967`, `baude/src/app.rs:13094`
**Issue:** Both failure tests press Enter, observe the error, release the holder (or clear the injected save failure) and press Enter again with no `tick()` in between. In production the user reads the error and presses Enter seconds later, during which `tick()` runs exit detection over `self.sessions`. After a save failure the row's runtime has already been killed by the kill stage and is back in `sessions` dead; after a kill failure the group is intact but the error arm of `kill_and_wait` is documented as leaving the reader parked. `removal.rs` says "the existing exit detection reconciles" these handles, but the retry contract in `preflight` is strict: a persisted runtime with no handle is `RuntimeHandleMissing`, a handle for a row with no persisted runtime is `RuntimeIdentityMismatch`. If exit detection clears one side (drops the dead session from `sessions` and the runtime map) without the other (the persisted `owned_runtime` on the row, which `remove` reads from in-memory state), the retry fails preflight permanently and the row becomes unremovable via `D` until restart. Nothing in this phase proves which way it goes.
**Fix:** In `remove_project_failure_save_keeps_rows_then_retry_succeeds`, insert `app.tick()` (and whatever settles exit detection, e.g. the bounded wait the main loop uses) between the failed Enter and the retry Enter, and assert the retry still returns `Removed`. If it does not, `confirm_remove_project` should tolerate the reconciled shape: when `collect_row_runtimes` finds no session for a row whose persisted runtime is `Some`, and the persisted pid is not live, clear the persisted runtime on the candidate before calling `remove` (or route through the existing exit transition first). Do the same for the kill-failure test with the holder still alive, which is the shape a user actually sees while deciding whether to retry.

## Info

### IN-01: Row labels rebuild the hierarchy once per label on every frame

**File:** `baude/src/app.rs:3784`, `baude/src/app.rs:3814`
**Issue:** `draw_modal` calls `removal_kind_label` once and `removal_row_label` once per visible row per frame, and each call runs `self.hierarchy_rows()`, which reprojects the whole repository state. For a parent with twenty children that is eleven full hierarchy builds per redraw while the modal is open. Performance is out of v1 scope; the correctness angle is that labels are resolved against live state at draw time rather than captured with the previewed report, so a rename or re-projection between preview and confirm changes the names under the user without changing the report. Capture `Vec<(RowKey, String)>` labels into `Modal::ConfirmRemove` when it is built and reuse them in `draw_modal`.
**Fix:** Add `labels: Vec<String>` (parallel to `report.rows_removed`) and `kind_label: &'static str` to the variant, filled in `open_remove_project_modal` and on `Recheck`.

### IN-02: Reservation refusal surfaces a `Debug` dump in the footer

**File:** `baude/src/app.rs:3900`
**Issue:** `RemoveProjectError::Reservation(format!("{outcome:?}"))` puts the `Debug` rendering of the reservation outcome into a user-facing message ("repository is busy: Held { ... }"). `prepare_remove_worktree` and `removal_refusal` already produce prose for the same situation.
**Fix:** Reuse the existing refusal wording, or implement `Display` on the reservation outcome and format with `{outcome}`.

### IN-03: Truncation width is the nominal 76 even when `centered` shrinks the modal

**File:** `baude/src/ui.rs:1803`
**Issue:** `inner_width` is derived from the constant `width = 76`, but `centered(area, width, height)` clamps to `area.width` (line 1547). On the 59-column case the tests exercise, lines are truncated with an ellipsis at 74 cells and then hard-clipped by the border at 57, so long paths lose their tail without the ellipsis that signals truncation.
**Fix:** `let rect_width = width.min(area.width); let inner_width = usize::from(rect_width.saturating_sub(2));` and pass that to the generator.

### IN-04: Standalone removal jumps selection to the first repository rather than a neighbour

**File:** `baude/src/app.rs:4115-4132`
**Issue:** For a `RemovalTarget::Standalone`, `parent` is `None` and `removed_at` is `None`, so the `None` arm picks the first surviving repository in `rows_before`. D-11 says "the parent when a child was removed, otherwise the next repository"; for a standalone row at the bottom of the sidebar that reads as "the row after it", and the implementation sends the cursor to the top. Not wrong by the letter, but worth deciding deliberately and pinning in a test.
**Fix:** Compute the position of the removed row in `rows_before` for every target kind and prefer the next selectable surviving row at or after that index, falling back to the previous one, before the repository-only search.

### IN-05: New `debug_assertions`-gated doubles are consumed by ungated tests

**File:** `baude-core/src/testing.rs:532`, `baude-core/src/testing.rs:546`, `baude/src/app.rs:12967`
**Issue:** `arm_unkillable_runtime` and `spawn_unkillable_runtime` are `#[cfg(debug_assertions)]` because `Pty::fail_next_teardown_for_test` is, but `remove_project_failure_kill_keeps_rows_and_handles_then_retry_succeeds` calls them without mirroring the gate, so `cargo test --release -p baude` fails to compile the test target. The gate runs `cargo test` in debug, and `fail_next_teardown_for_test` is already used ungated elsewhere in `app.rs` (lines 11309, 12185), so this follows precedent rather than introducing a new hazard. Flagged because `testing.rs` is now a public surface shared across crates and the asymmetry will bite the first time someone profiles under `--release`.
**Fix:** Either add `#[cfg(debug_assertions)]` on the consuming test, or lift the `debug_assertions` gate on the injection hook to `cfg(any(test, feature = "test-support"))` to match the module it now lives beside.

---

_Reviewed: 2026-11-05_
_Reviewer: Claude (gsd-code-reviewer)_
_Depth: standard_
