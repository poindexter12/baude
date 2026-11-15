# Phase 20: TUI Remove Action - Context

**Gathered:** 2026-11-04
**Status:** Ready for planning

<domain>
## Phase Boundary

The TUI gains one project-removal action over the shared `baude-core::removal` operation from Phases 18 and 19. The user presses a key on any local sidebar row, reads a confirmation built from `preview_removal`'s report that names the row kind, every row that drops, whether a live runtime is killed, which paths are deleted versus kept, and the Phase 19 clearings, and on confirm the rows leave the sidebar and the state file in one save. A failure at any stage leaves the rows visible with the typed error and a retry. REST and PWA removal are Phase 21; nothing in this phase touches the daemon.

</domain>

<decisions>
## Implementation Decisions

### Key Binding and Reach
- The action is bound to `D` (shift-d), unbound today. `X` keeps its meaning of verified-safe managed-worktree removal with the clean-tree preflight; the Phase 18 force path gets its own key rather than changing what `X` does.
- `D` is offered on repository, main checkout, managed worktree, unmanaged checkout, standalone and Unavailable rows. An Unavailable row with a missing worktree is the "already removed" case Phase 18 reports; recovery state does not block it. Remote rows refuse with a notice, since remote removal arrives with Phase 21.
- `D` works on a row with a live runtime. The confirmation states that the runtime will be killed; the user is not asked to `x` close first.
- The action is appended to every row kind's context footer line and to the `?` help overlay beside `x`, `w` and `X`, using the existing `*` marker for destructive keys.

### Confirmation Content
- The modal is built from `preview_removal`'s `RemovalReport`, the same data the real run produces. Title names the row kind. Sections: rows dropped (every child listed for a repository parent), runtimes killed (count plus row names), paths deleted, paths kept, and the Phase 19 clearings (bindings, markers, suppression entries). A path is never shown under deleted unless the report lists it there.
- Keys match the existing `ConfirmRemoveWorktree` modal: `Enter` or `y` confirms, `Esc` or `n` cancels. Cancel is non-mutating.
- The modal grows to at most 80% of the screen height. When the child list overflows, `j`/`k` scroll it and a hidden remainder is rendered as "+N more", so the total count is never hidden.
- A new `Modal::ConfirmRemove { target, report }` variant is created when `D` is pressed. On confirm the real run recomputes the row set; if it differs from the previewed report, the modal refreshes with the new report and asks again instead of removing.

### Failure, Retry and Aftermath
- A kill, disk or save failure returns `Err` from the core before any row drops. App returns the moved `Session` handles to its registry (the existing `restore_removed_runtime` path), keeps every row visible, and the modal stays open showing the typed error (surviving pids, failing path, save error) with `enter retry · esc cancel`.
- Clearing failures after the committed save do not fail the removal. The rows are gone; a non-blocking context-footer notice names what could not be cleared.
- On success the rows vanish from the hierarchy in the same tick the save returns. Selection moves to the next surviving row: the parent when a child was removed, otherwise the next repository. The footer confirms with counts of rows, runtimes killed and paths deleted.
- Tests are App-level with cloned-repo fixtures and a real spawned process group per Phase 18's doubles: `D` on each row kind opens a modal whose lines match the preview report; confirm removes the rows from the hierarchy and the state file in one save; a forced kill failure and a forced save failure each leave the rows visible with the error, and a retry then succeeds.

### Claude's Discretion
- Exact modal line layout, section headings and the help-overlay wording.
- How `App` moves `Session` handles into `RowRuntime` values and back on error, provided no handle is lost on `Err`.
- Whether the preview-versus-confirm recheck compares `rows_removed` only or the whole report.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `baude-core/src/removal.rs`: `RemovalTarget`, `preview_removal`, `remove`, `RemovalReport` (rows_removed, runtimes_killed, paths_deleted, paths_kept, already_removed, cleared, clearing_failures), `RemovalError`, `RowRuntime`.
- `baude/src/app.rs`: `sidebar_action` key dispatch, `SidebarAction` and `SidebarRefusal`, `Modal` enum with `ConfirmRemoveWorktree`, `prepare_remove_worktree` / `confirm_remove_worktree`, `restore_removed_runtime`, `refuse_sidebar_action`, `context_footer`.
- `baude/src/ui.rs`: `draw_modal` with the `ConfirmRemoveWorktree` arm and `remove_confirmation_lines`; context footer hint strings near line 1480.

### Established Patterns
- Destructive actions are confirmed through a `Modal` variant carrying a confirmation value computed up front; the key handler matches on the modal and calls a `confirm_*` method.
- Refusals surface through `refuse_sidebar_action` into the context footer rather than a modal.
- Removal core operations take a `save` closure so the TUI's atomic persist path is reused.

### Integration Points
- Key dispatch: a new `SidebarAction::RemoveProject` branch in `sidebar_action` for `D`.
- Modal handling in `handle_key` and rendering in `draw_modal`.
- Footer hint strings and the `?` help overlay in `ui.rs`.
- Hierarchy refresh and selection after `remove` returns.

</code_context>

<specifics>
## Specific Ideas

- The confirmation must reflect the Phase 19 report fields (cleared bindings, markers, suppression entries) as the Phase 19 deferred list anticipated.
- The `*` marker convention already used for `X remove*` in footer hints carries over to `D`.

</specifics>

<deferred>
## Deferred Ideas

- Removing remote rows from the TUI: Phase 21 with the REST and PWA removal.
- Phase 18 and 19 open items remain where they are logged: `scripts/gate.sh --linux` target dir, 18/19 VALIDATION drafts.

</deferred>
