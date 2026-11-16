# Phase 20: TUI Remove Action - Pattern Map

**Mapped:** 2026-11-04
**Files analyzed:** 2 modified (no new files required; `baude/src/app/remove_project.rs` optional)
**Analogs found:** 2 / 2 (both files are modified in place; analogs are sibling code in the same files)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude/src/app.rs` (new `SidebarAction::RemoveProject`, `SidebarRefusal::RemoteRemove`, `Modal::ConfirmRemove`, `prepare_remove_project`, `confirm_remove_project`, handle_key arm, tests) | controller / store | request-response (key event -> modal -> core op -> state save) | same file: `X` flow at `sidebar_action` 331-345, `Modal::ConfirmRemoveWorktree` 473-475, `prepare_remove_worktree` 3714-3734, `confirm_remove_worktree` 3736+, key arm 5292-5302 | exact (sibling flow) |
| `baude/src/ui.rs` (new `draw_modal` arm, `remove_project_confirmation_lines`, footer strings, help line, render tests) | component | transform (report -> lines) | same file: `draw_modal` `ConfirmRemoveWorktree` arm 1746-1762, `remove_confirmation_lines` 2393-2415, footer strings 1486-1518, help 2326-2332, exact-string tests 2706, 2919-3036, 3158 | exact |
| `baude-core/src/removal.rs` (read-only consumer; no change planned) | service | batch (select -> preflight -> kill -> disk -> persist -> clear) | n/a, this IS the API | consumed |

All three paths verified git-tracked (`git ls-files` prints each).

## Pattern Assignments

### `baude/src/app.rs` (controller, request-response)

**Analog:** the `X` managed-worktree flow in the same file. Copy its shape; do not extend it.

**Dispatch pattern** (`sidebar_action`, lines 331-345). Key test accepts both the shifted char and `'x'`+SHIFT; every `ActionKind` gets an explicit arm:
```rust
fn sidebar_action(view: ActionView, key: KeyEvent) -> SidebarAction {
    let remove_key = matches!(key.code, KeyCode::Char('X'))
        || matches!(key.code, KeyCode::Char('x')) && key.modifiers.contains(KeyModifiers::SHIFT);
    if remove_key {
        return match view.kind {
            ActionKind::Repository => SidebarAction::Refuse(SidebarRefusal::RepositoryRemove),
            ActionKind::Main => SidebarAction::Refuse(SidebarRefusal::MainRemove),
            ActionKind::Managed if view.can_remove => SidebarAction::Remove,
            ActionKind::External => SidebarAction::Refuse(SidebarRefusal::UnmanagedRemove),
            ActionKind::Standalone => SidebarAction::Refuse(SidebarRefusal::StandaloneRemove),
            ActionKind::Unavailable
                if view.capability == Some(lifecycle::LifecycleCapability::RetryRecovery) =>
            {
                SidebarAction::Refuse(SidebarRefusal::RecoveryRemove)
            }
            ...
```
For `D`: same `remove_key` idiom with `'D'`/`'d'`+SHIFT, placed BEFORE the `X` block; Repository, Main, Managed, External, Standalone, Unavailable all map to `SidebarAction::RemoveProject` (no `can_remove` or `RetryRecovery` gate per locked decision); remote kinds map to `SidebarAction::Refuse(SidebarRefusal::RemoteRemove)`. Enums to extend: `SidebarRefusal` (line 275, flat variant list) and `SidebarAction` (line 310). Refusal text is produced in `refuse_sidebar_action` (line 1156), which already builds target/repository/path labels; add a `RemoteRemove` arm naming Phase 21.

**Modal variant pattern** (lines 473-475): a variant carrying the precomputed confirmation value.
```rust
    ConfirmRemoveWorktree {
        confirmation: lifecycle::RemovalConfirmation,
    },
```
New: `ConfirmRemove { target: removal::RemovalTarget, report: removal::RemovalReport, scroll: usize, error: Option<String> }` so the modal can survive an `Err` and scroll the child list.

**Prepare pattern** (`prepare_remove_worktree`, lines 3714-3734): reserve the repository, then call the core preview.
```rust
        let _reservation = self
            .repository_reservations
            .reserve(saved.repository_key)
            .map_err(|outcome| lifecycle::RemovalFailure::Inspection(format!("{outcome:?}")))?;
        lifecycle::prepare_removal(&self.repository_state, checkout)
```
Copy the reservation; DROP the preceding `persistence_dirty || has_pending_activation()` refusal (lines 3718-3722), it would block the Unavailable "already removed" case. Replace `lifecycle::prepare_removal` with `removal::preview_removal(&self.repository_state, target)`.

**Confirm pattern** (`confirm_remove_worktree`, lines 3736-3760): snapshot rows before mutation, re-reserve, resolve runtime id via `self.runtime_checkouts.get(&checkout)`, and (test-only) honour an injected stop error.
```rust
        let rows_before_removal = self.visible_hierarchy_rows();
        let _reservation = self
            .repository_reservations
            .reserve(confirmation.repository())
            .map_err(|outcome| lifecycle::RemovalFailure::Inspection(format!("{outcome:?}")))?;
        let checkout = confirmation.checkout();
        let runtime_id = self.runtime_checkouts.get(&checkout).copied();
        ...
        #[cfg(test)]
        if let Some(error) = &self.remove_stop_error_for_test {
            return Err(lifecycle::RemovalFailure::Inspection(format!(
                "runtime stop failed: {error}"
            )));
        }
```
Use `rows_before_removal` the same way to pick the post-removal selection (parent of a removed child, else next repository).

**Key handler pattern** (lines 5292-5302): `y`/Enter confirms, modal is swapped out first.
```rust
            Modal::ConfirmRemoveWorktree { .. } => match key.code {
                KeyCode::Char('y') | KeyCode::Enter => {
                    let modal = std::mem::replace(&mut self.modal, Modal::None);
                    let Modal::ConfirmRemoveWorktree { confirmation } = modal else {
                        unreachable!();
                    };
                    match self.confirm_remove_worktree(confirmation) {
                        Ok(LifecycleOutcome::Removed { branch_ref, .. }) => self.set_message(
                            format!("worktree removed — local branch {branch_ref} retained"),
                        ),
```
For `D`: do NOT leave the modal as `Modal::None` on `Err`. Take it with `mem::replace`, and in the `Err` arm reinstall `Modal::ConfirmRemove { .., error: Some(text) }`. Add `j`/`k` arms adjusting `scroll`; `n`/Esc arm sets `Modal::None` without mutation. Success uses `self.set_message(...)` with counts.

**Save closure pattern** (lines 1948-1958): the atomic persist path consumes `self.atomic_failure_for_test`.
```rust
                &StateFile::new(state),
                self.atomic_failure_for_test,
                None,
            );
        }
        persist::save_for_workspace_status(
            "state",
            baude_core::workspace::active(),
            &StateFile::new(state),
        )
```
`remove` takes `save: impl FnOnce(&StateFile) -> Result<(), SaveError>` while `&mut self.repository_state` is borrowed. Copy `atomic_failure_for_test` into a local before the call and build the closure from locals (config/workspace, the copied failure) rather than `&self`; clear the one-shot failure after the call as `save_removal_revocation` does (lines 1961-1970). Existing tests set it with `app.atomic_failure_for_test = Some(persist::AtomicFailure::Rename)` (line 10438) and clear with `None` before retry (10452).

**Existing test precedent** (lines 11736-11760): press `X` through `app.handle_sidebar_key(KeyEvent::new(KeyCode::Char('X'), KeyModifiers::SHIFT))`, then `matches!(app.modal, Modal::ConfirmRemoveWorktree { .. })`. Mirror with `'D'` and `Modal::ConfirmRemove`.

---

### `baude/src/ui.rs` (component, transform)

**Analog:** `ConfirmRemoveWorktree` render arm and `remove_confirmation_lines`.

**Render arm pattern** (lines 1746-1762): resolve display name from `app.hierarchy_rows()`, size with `centered(area, w, h)`, compute `inner_width = rect.width - 2`, build lines through a pure function.
```rust
        Modal::ConfirmRemoveWorktree { confirmation } => {
            let target = app
                .hierarchy_rows()
                .into_iter()
                .find_map(|row| match row {
                    LocalRow::Checkout(child) if child.key == confirmation.checkout() => {
                        Some(child.name)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| app.selected_target_label());
            let rect = centered(area, 76, 8);
            let inner_width = rect.width.saturating_sub(2) as usize;
            let lines = remove_confirmation_lines(
```
For `D`: height is `min(lines.len() + 2, area.height * 80 / 100)`; pass `scroll` and the visible-row budget into the line builder so it emits "+N more".

**Pure line builder pattern** (lines 2393-2415): `labeled` closure plus `truncate`/`cell_width`, last line is the key legend.
```rust
fn remove_confirmation_lines(
    target: &str,
    path: &std::path::Path,
    branch_ref: &str,
    width: usize,
) -> Vec<String> {
    let labeled = |label: &str, value: &str| {
        let prefix = format!("{label}: ");
        format!(
            "{prefix}{}",
            truncate(value, width.saturating_sub(cell_width(&prefix)))
        )
    };
    vec![
        truncate("Remove this clean baude-managed linked worktree?", width),
        labeled("target", target),
        ...
        truncate("y/enter remove · n/esc keep worktree", width),
```
New `remove_project_confirmation_lines(report: &RemovalReport, kind_label: &str, row_name: &dyn Fn(RowKey) -> String, scroll: usize, max_rows: usize, width: usize) -> Vec<String>` with sections from report fields only: rows dropped (`rows_removed`), runtimes killed (`runtimes_killed`), paths deleted (`paths_deleted`), paths kept (`paths_kept`), already removed, clearings (`cleared` by `ClearingKind`), optional `error` line, legend `y/enter remove · n/esc cancel` (or `enter retry · esc cancel` when error is set).

**Footer hint strings** (lines 1486-1518): one literal per row-kind/state combination, `*` marks destructive keys.
```rust
                _ if view.has_runtime && view.can_remove => {
                    "enter attach · x close · X remove* · t shell · e edit · a archive · ? help"
                }
                _ if view.can_remove => "enter reopen · X remove* · e edit · i info · ? help",
```
Append `D remove*` (placed after `X remove*` where present, after `x close` otherwise) to the Repository, Unavailable (all three capability arms), Standalone (both), and the four `_` arms; also the compact "? more" variants near 1486-1490. Remote-row strings stay untouched.

**Help overlay line** (line 2330):
```rust
                Line::raw("  X removes verified clean managed worktree"),
                Line::raw("              local branch is retained"),
```
Add a sibling pair for `D` directly after.

**Exact-string tests to update in the same task:** line 2706 (`assert!(rendered.contains("X removes"))`, add `D` sibling), 2919-3036 (footer literals compared whole), 3158 (`assert!(!sidebar.contains("X remove"))` for remote rows; add the same negative for `D remove`).

---

### `baude-core/src/removal.rs` (service, consumed API)

Signatures the App code calls (lines 359, 371, 436-442):
```rust
pub fn preview_removal(state: &RepositoryState, target: RemovalTarget) -> Result<RemovalReport, RemovalError>
pub struct RowRuntime { pub row: RowKey, pub session: crate::session::Session }
pub fn remove(
    state: &mut RepositoryState,
    target: RemovalTarget,
    runtimes: &mut Vec<RowRuntime>,
    save: impl FnOnce(&crate::persist::StateFile) -> Result<(), crate::persist::SaveError>,
) -> Result<RemovalReport, RemovalError>
```

Stage order inside `remove` (lines 443-455): `select_rows` -> `planned_clearings` -> `preflight` -> clone candidate + `remove_rows` + `validate_for_save` -> `kill_stage` -> `disk_stage` -> `persist_stage` -> `clear_stage`. `state` is only mutated inside `persist_stage` after a successful save.

**Handle custody finding (answers RESEARCH open question 1 / A4):** `kill_stage` takes `&mut [RowRuntime]` (line 918) and only calls `handle.session.kill_and_wait()` in place; it does NOT drain the vector. `preflight` (840) borrows `&[RowRuntime]`. Only `persist_stage` receives `&mut Vec<RowRuntime>` (1032), so draining happens, if at all, there and only after the save succeeded. Therefore on any `Err` every `RowRuntime` the App supplied is still in the vector (killed ones included, now dead). The App's restore helper should put every remaining element back; killed-but-restored sessions will read as exited and the existing close path handles them.

## Shared Patterns

### Refusal into context footer
**Source:** `baude/src/app.rs` `refuse_sidebar_action` (line 1156) and `SidebarRefusal` (275)
**Apply to:** remote-row `D` press
Add `SidebarRefusal::RemoteRemove` and an arm producing a notice like the existing `UnavailableRemove` one; the function already supplies target, repository, path and cause labels.

### Failure injection
**Source:** `baude/src/app.rs` lines 604, 894, 1951, 1961-1970, tests 10438/10452
**Apply to:** save-failure test for `D`
`atomic_failure_for_test: Option<persist::AtomicFailure>` is read by the atomic save and cleared after one use; set `Rename`/`Write`/`DirectorySync`, press Enter, assert rows present and modal error, then press Enter again for the retry.

### Kill-failure double
**Source:** `baude-core/src/removal.rs` tests module from line 1169 (Phase 18 process-group double producing `RemovalError::RuntimeSurvived`). Not read this session due to budget; planner's Wave 0 task should confirm it is reachable via the `test-support` feature from `baude`, as RESEARCH A1 flags.

### Selection after mutation
**Source:** `confirm_remove_worktree` line 3740 `let rows_before_removal = self.visible_hierarchy_rows();`
**Apply to:** `confirm_remove_project` success arm; choose parent key or next repository from the snapshot, assign `self.selected_id` after the hierarchy rebuild.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| scrollable modal list with "+N more" | component | transform | No existing modal scrolls; `centered(area, 76, 8)` is fixed-size. Planner should add `scroll` to the variant and compute height from `area.height * 80 / 100`. |

## Metadata

**Analog search scope:** `baude/src/app.rs`, `baude/src/ui.rs`, `baude-core/src/removal.rs`
**Files scanned:** 3 (targeted grep + ranged reads; no full-file loads)
**Pattern extraction date:** 2026-11-04
