---
phase: 20
plan: 02
type: tdd
status: complete
subsystem: tui-remove-action
requirements_addressed: [TUI-01, TUI-02, TUI-03, TUI-04]
tags: [tui, removal, modal, tdd]
dependency_graph:
  requires:
    - 20-01 remove_project_confirmation_lines renderer in baude/src/ui.rs
    - baude_core::removal::{RemovalTarget, RemovalReport, RowRuntime, RowKey, preview, execute}
  provides:
    - SidebarAction::RemoveProject dispatched by D on every local row kind
    - Modal::ConfirmRemove { target, report, scroll, error } with y/n/Esc/j/k handling
    - App::confirm_remove_project kill-then-remove in one save
  affects:
    - 20-03 and 20-04 build on the same modal and outcome types
tech_stack:
  added: []
  patterns:
    - preview-report-backed confirmation modal
    - kill runtimes, execute removal, restore runtimes on failure
key_files:
  created: []
  modified:
    - baude/src/app.rs
    - baude/src/ui.rs
files_modified:
  - baude/src/app.rs
  - baude/src/ui.rs
commits:
  - sha: 722a1fb
    subject: "feat(20-02): add D remove action with report-backed confirmation"
  - sha: 04cd676
    subject: "test(20-02): prove repository parent removal in one save"
plan_head_before: 26d862bff482294d51fd482372df940b6d0c1d82
plan_head_after: 04cd676
decisions:
  - D maps to one SidebarAction::RemoveProject for every local row kind and a SidebarRefusal for remote rows; the target is derived from the selection at modal-open time via removal_target_for_selection, not stored on the action
  - Confirmation modal carries the preview RemovalReport computed at open, so y re-runs preview inside confirm_remove_project against fresh state rather than trusting the stale report
  - Runtimes are killed before execute and restored via restore_row_runtimes when removal returns Err, keeping Esc and failure paths non-mutating for session state
metrics:
  duration: "about 1h 10m of executor time across 2026-11-04 22:25 to 23:34 PST"
  completed: 2026-11-04
actuals:
  tokens: 12400
  tasks: 3
  commits: 2
---

# Phase 20 Plan 02: D Remove Action Summary

D on any local sidebar row opens a report-backed confirmation modal and y performs kill-then-remove through `baude_core::removal` in a single state save, with remote rows refused via the footer notice.

## What was built

**Task 1: sidebar dispatch** (`722a1fb`)

- `SidebarAction::RemoveProject` added. `sidebar_action` yields it for `D` on every local row kind: repository parent, main checkout, managed worktree, unmanaged worktree, standalone session.
- `SidebarRefusal::RemoteRemove` (footer text: remote rows cannot be removed from this side) returned for remote rows; `handle_sidebar_key` routes it through the existing refusal notice path.
- `App::removal_target_for_selection` maps the current `SelId` to a `baude_core::removal::RemovalTarget` (Repository / Checkout / Standalone), returning `None` for remote and header rows.

**Task 2: modal and confirm** (`722a1fb`, same commit per plan)

- `Modal::ConfirmRemove { target, report, scroll, error }`. `open_remove_project_modal` runs `removal::preview` on the current repository state and stores the report; a preview error lands in `error` so the modal still opens and the user sees why.
- Modal keys: `y` confirms, `n` and `Esc` close without mutation, `j`/`Down` and `k`/`Up` scroll the report.
- `pub(crate) fn confirm_remove_project(&mut self, target) -> Result<RemoveProjectOutcome, RemoveProjectError>`: collects live runtimes for the rows in the report via `collect_row_runtimes`, kills them, calls `removal::execute`, saves state once, then `select_after_removal` moves the cursor to the nearest surviving row. On `Err`, `restore_row_runtimes` reattaches the killed sessions' rows before the error is shown in the modal.
- `RemoveProjectOutcome::Removed(RemovalReport)` and `RemoveProjectError` (wrapping `RemovalError`, with `Display` and `From`) are the outcome types plan 20-03 consumes for the footer.
- `removal_notice(&RemovalReport) -> String` builds the footer message with removed row counts and the runtime-killed count.
- `baude/src/ui.rs`: new `draw_modal` arm for `Modal::ConfirmRemove` calling `remove_project_confirmation_lines(report, kind_label, row_name, error, scroll, max_rows, width)`; the handoff `#[cfg_attr(not(test), allow(dead_code))]` attribute and its comment are dropped. Two label helpers on `App` feed it: `removal_kind_label` and `removal_row_label`.

**Task 3: repository parent proof** (`04cd676`)

- `confirm_remove_repository_parent_drops_every_child_in_one_save` builds the relaunch fixture with a second managed worktree, opens `D` on the clone's parent row, confirms with `y`, and asserts: the report lists the parent plus all five checkouts (main, linked, managed, second managed, unmanaged), every child row key is gone from the saved state, exactly one save happened (state-file bytes compared before and after), and the footer notice names the counts.

## RED evidence

Tests for Tasks 1 and 2 were written first against `todo!("plan 20-02 task 2")` bodies in `open_remove_project_modal`, `collect_row_runtimes`, `restore_row_runtimes`, and `confirm_remove_project`. Single RED run of the plan's gate test:

```
cargo test -p baude app::tests::d_opens_confirm_remove_modal -- --test-threads=1
not yet implemented: plan 20-02 task 2
test result: FAILED. 0 passed; 1 failed; 0 ignored; 0 measured; 266 filtered out; finished in 22.39s
exit=101
```

`grep -c 'error\[E' /tmp/baude-20-02-red.log` returned 0 on that run, so the failure was the behavior gap, not a compile error. Two earlier attempts of the same run did hit `error[E`: `Modal` lacks `Debug` (test helper formatted it with `{:?}`) and `ActionSelection::Repository` has a different field shape than the test case table assumed. Both were test-side fixes made before the recorded RED above.

Task 3's test was written after the Task 2 implementation existed, so its first run was green after two fixture corrections (see Deviations); the plan prescribes this task as a proof over the delivered code rather than a RED gate.

## Verification results

All exits captured directly (never via a pipe).

| Command | Exit |
| --- | --- |
| Task 1 chain: `cargo test -p baude app::tests::sidebar_action_d -- --test-threads=1 && cargo test -p baude app::tests::d_on_remote_row_sets_footer_notice -- --test-threads=1` | 0 |
| Task 2 chain: `d_opens_confirm_remove_modal`, `d_on_row_with_live_runtime`, `confirm_remove_managed_worktree_drops_row_in_one_save`, `esc_on_confirm_remove_is_non_mutating`, each `cargo test -p baude app::tests::<name> -- --test-threads=1` chained with `&&` | 0 |
| Task 3 chain: `cargo test -p baude app::tests::confirm_remove_repository_parent_drops_every_child_in_one_save -- --test-threads=1 && cargo fmt --check && cargo clippy -p baude --all-targets -- -D warnings` | 0 |
| `cargo fmt --check` | 0 |
| `cargo clippy -p baude --all-targets -- -D warnings` | 0 |
| `cargo test -p baude app::tests -- --test-threads=1` (plan-level gate) | 0: 108 passed, 0 failed, 0 ignored, 978s |

New tests (8): `sidebar_action_d_yields_remove_project_on_every_local_kind`, `sidebar_action_d_refuses_remote_rows`, `d_on_remote_row_sets_footer_notice`, `d_opens_confirm_remove_modal_with_preview_report_on_each_row_kind`, `d_on_row_with_live_runtime_reports_runtime_killed`, `confirm_remove_managed_worktree_drops_row_in_one_save`, `esc_on_confirm_remove_is_non_mutating`, `confirm_remove_repository_parent_drops_every_child_in_one_save`.

Acceptance greps from the plan: `has_pending_activation` count unchanged from the HEAD baseline; no `todo!` remains in `baude/src/app.rs` or `baude/src/ui.rs`.

## Deviations

None in production behaviour. Test-side corrections, all inside this plan's own new tests:

1. **[Rule 1 - Bug] Clippy dead-field warnings in new test struct and outcome.** `cargo clippy --all-targets -D warnings` flagged never-read fields introduced by the tests (a tuple field on the test row struct and the `Removed` payload). Fixed by having the managed-worktree test assert on the report payload and the footer counts rather than discarding them. Commit `722a1fb`.
2. **[Rule 1 - Bug] Fixture realism in `d_opens_confirm_remove_modal_with_preview_report_on_each_row_kind`.** The first draft assumed the relaunch fixture's primary checkout carried `ActionKind::Main`; the fixture launches from the linked worktree, so the Main role belongs to the clone's main checkout. The unavailable-row case also assumed deleting the directory and relaunching would mark the row `Unavailable`; health is reconciled only when the checkout is opened, so the test opens it first. Commit `722a1fb`.
3. **[Rule 1 - Bug] Task 3 child set.** `RowKey` has no `Ord`, so the expected child set is compared as a set, and the fixture's `wt-linked` worktree was missing from the expected children (report correctly listed six rows). Commit `04cd676`.

No scope creep, no deferred items, no `.planning/STATE.md` or `.planning/ROADMAP.md` edits.

## Handoff notes for plans 20-03 and 20-04

Production names in `baude/src/app.rs`:

- `SidebarAction::RemoveProject`, `SidebarRefusal::RemoteRemove`
- `Modal::ConfirmRemove { target: RemovalTarget, report: RemovalReport, scroll: usize, error: Option<String> }`
- `App::removal_target_for_selection(&self) -> Option<RemovalTarget>`
- `App::open_remove_project_modal(&mut self)`
- `pub(crate) fn App::confirm_remove_project(&mut self, target: RemovalTarget) -> Result<RemoveProjectOutcome, RemoveProjectError>`
- `pub(crate) enum RemoveProjectOutcome { Removed(RemovalReport) }`
- `pub(crate) enum RemoveProjectError` (From<RemovalError>, Display)
- `App::collect_row_runtimes`, `App::restore_row_runtimes(Vec<RowRuntime>)`, `App::select_after_removal`, `App::removal_notice(&RemovalReport) -> String`
- `pub(crate) fn App::removal_kind_label(..)` and `App::removal_row_label(&RowKey) -> String` (consumed by the ui arm)

Test helpers in `app::tests` (reusable by 20-03 and 20-04):

- `d_key() -> KeyEvent`
- `remove_project_app(fx: &RelaunchFixture, branch: &str) -> (App, RemoveProjectRows)`: relaunches the fixture and returns row keys (`primary`, `main`, `managed`, `unmanaged`, `standalone`, plus the repository id) for driving `D` on each kind
- `confirm_remove_modal(&App) -> (RemovalTarget, RemovalReport)` panics with a message if the modal is not `ConfirmRemove`
- `checkout_row_by_key(&App, CheckoutKey) -> &SavedCheckout`
- `saved_state(&RelaunchFixture) -> RepositoryState` and `state_file_bytes(&RelaunchFixture) -> Vec<u8>` for one-save assertions

Known gaps for later plans: the footer notice after removal is produced by `removal_notice` but its exact wording is owned by 20-03; keyboard help text for `D` in the sidebar hint bar is not updated by this plan (20-04).

## Known Stubs

None.

## Self-Check: PASSED

- `baude/src/app.rs` and `baude/src/ui.rs` modified and present.
- Commits `722a1fb` and `04cd676` are ancestors of HEAD on `gsd/phase-18-shared-kill-then-remove-lifecycle`.
- `commits: 2` measured by `git rev-list --count 26d862b..HEAD`.
