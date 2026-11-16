# Requirements: baude

**Defined:** 2026-11-02
**Milestone:** v2.8 Project Removal
**Core Value:** You can see at a glance which of your many coding-agent sessions needs you next and act on it from the terminal or phone.

## v2.8 Requirements

Approved scope: remove a repository, checkout, worktree or standalone row from baude in one action, even while a runtime is live, so the project is gone from the sidebar, the state file and every rediscovery path. The same removal is available over the daemon REST API and from the PWA.

**Baseline facts (2026-11-01, v2.7.3):** `X` in `baude/src/app.rs` `sidebar_action` is refused on Repository, main, unmanaged and standalone rows and runs only on a clean managed worktree; `x` closes the runtime and keeps the row; no forget action exists. `baude-core/src/repository.rs` `remove_standalone_session` is the only row-removal primitive outside the managed-worktree path. Re-admission paths are the session-start sweep, `worktree_scan.rs`, `marker.rs` recovery, the `folder_workspace.rs` binding walk and `launch.rs`. The daemon's `DELETE /sessions/{id}` kills and removes a flat session row only.

**Decisions taken during questioning (2026-11-01):**

- Removal kills the owned runtime first; never detach-and-forget.
- Removal deletes baude-managed worktrees from disk; it never deletes a main checkout, a standalone folder, or a local branch.
- Scope covers a repository parent with all its children, a single checkout or worktree child, and a standalone row.
- Removed projects stay gone across reconcile and relaunch. Removal clears the folder-workspace binding and marker ownership. Re-admission happens only through `n`, `c`, or launching baude inside the folder.
- Daemon parity over REST and the PWA.

### Runtime shutdown

- [x] **KILL-01**: Removing a row whose runtime is live terminates the owned child process group before any row or file is dropped, sending SIGTERM first and escalating to SIGKILL after a bounded grace period, on macOS and Linux
- [x] **KILL-02**: After the kill, baude-owned PTYs and reader threads for that row are closed and no zombie or orphaned child remains attributable to the removed row
  - Scope (Phase 18): "attributable to the removed row" means the row's own agent and shell process groups; a process that left the group but still holds the PTY slave lies outside the whole-session kill unit (18-CONTEXT.md Kill Semantics, D-KILL-01) and is not signalled and the row is reported in `readers_parked` rather than hidden. Details in 18-VALIDATION.md Requirement Scope Notes.
- [x] **KILL-03**: A runtime that fails to exit after escalation leaves the row in place with a visible error instead of dropping the row and leaking the process

### Row removal

- [x] **RMV-01**: User can remove a single managed worktree child, including one with a live runtime or an unclean working tree, and its worktree directory is removed from disk
- [x] **RMV-02**: User can remove a main checkout child, which drops the row and its runtime without deleting the checkout directory or any local branch
- [x] **RMV-03**: User can remove a standalone folder row, which drops the row and its runtime without deleting the folder
- [x] **RMV-04**: User can remove a repository parent, which removes every child by the per-child rules above and then drops the parent row
- [x] **RMV-05**: Removal is a shared `baude-core` lifecycle operation that the TUI and the daemon both call, composed from the existing kill, close and managed-worktree removal stages rather than a parallel code path
- [x] **RMV-06**: Removal persists as an atomic state-file write per removed row set, so a crash mid-removal leaves either the prior state or the fully removed state, never a parent without children or a child without its parent
- [x] **RMV-07**: Removal never deletes a main checkout, a standalone folder or a local branch, and a test asserts each of these on fixtures shaped like real cloned and `git init` repositories

### Re-admission suppression

- [x] **READM-01**: Removing a row clears its folder-workspace binding so the launch-dir binding walk no longer resolves the removed folder to a workspace
- [x] **READM-02**: Removing a row clears baude's marker ownership for its paths so marker recovery does not restore the row
- [x] **READM-03**: After removal, the session-start sweep, worktree scan, marker recovery and reconcile do not re-admit the removed repository, checkout, worktree or standalone row, verified across a relaunch of the TUI
- [x] **READM-04**: A removed project can be admitted again only by `n`, by `c`, or by launching baude inside the folder, and that re-admission produces a fresh row with no stale identity carried over
- [x] **READM-05**: Removal does not alter other rows' bindings, markers or worktrees, including other repositories that share a workspace

### TUI removal action

- [x] **TUI-01**: The remove action is available on repository, main checkout, managed worktree, unmanaged checkout and standalone rows, and the sidebar footer or help lists it alongside `x`, `w` and `X`
- [x] **TUI-02**: Before removing, the TUI shows a confirmation naming the row kind, the exact rows that will be dropped, whether a live runtime will be killed, and which paths will be deleted from disk versus left in place
- [x] **TUI-03**: Removing a repository parent lists every child that will be removed in the confirmation and removes them all on confirm
- [x] **TUI-04**: Removal failure at any stage leaves the affected rows visible with an error notice rather than a half-removed sidebar, and the user can retry

### Daemon and PWA parity

- [x] **API-01**: The daemon exposes a REST removal endpoint for repository, checkout, worktree and standalone rows that calls the same `baude-core` operation as the TUI and returns a structured result naming the rows removed, the runtimes killed and the paths deleted
- [x] **API-02**: Removal over REST emits SSE events so a connected TUI or PWA drops the rows without a refresh
- [x] **API-03**: The PWA offers remove on the same row kinds with a confirmation equivalent to TUI-02, and completes the removal through API-01

### Validation and release

- [x] **REL-01**: Integration tests cover each row kind with a live runtime and with no runtime, using fixtures shaped like real cloned and `git init` repositories, and assert on fixture output that process, worktree, binding and marker state are gone
- [x] **REL-02**: v2.8.0 ships through release-please with README and PWA docs describing the remove action, its confirmation, and what is and is not deleted

## Future Requirements

Deferred to a later milestone. Tracked but not in the current roadmap.

### Undo and bulk operations

- **UNDO-01**: User can undo a removal within a short window, restoring rows and bindings
- **BULK-01**: User can multi-select rows and remove them in one confirmation

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Deleting a main checkout, standalone folder or local branch from disk | Removal forgets baude's knowledge of a project; destroying user data is a different, riskier operation |
| Detach-and-forget that leaves the runtime running | Decided against during questioning; an orphaned Claude process with no owner is the failure mode this milestone exists to prevent |
| Archiving as an alternative to removal | Idle archiving already exists; this milestone is about rows the user wants gone |
| Removing remote rows from the TUI by any path other than the daemon's removal endpoints | Remote removal from the TUI is in scope since Phase 21 per D-12 and goes through the daemon's preview and execute endpoints; the TUI never acts on the daemon host's files or processes itself |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| KILL-01 | Phase 18 | Complete |
| KILL-02 | Phase 18 | Complete |
| KILL-03 | Phase 18 | Complete |
| RMV-01 | Phase 18 | Complete |
| RMV-02 | Phase 18 | Complete |
| RMV-03 | Phase 18 | Complete |
| RMV-04 | Phase 18 | Complete |
| RMV-05 | Phase 18 | Complete |
| RMV-06 | Phase 18 | Complete |
| RMV-07 | Phase 18 | Complete |
| READM-01 | Phase 19 | Complete |
| READM-02 | Phase 19 | Complete |
| READM-03 | Phase 19 | Complete |
| READM-04 | Phase 19 | Complete |
| READM-05 | Phase 19 | Complete |
| TUI-01 | Phase 20 | Complete |
| TUI-02 | Phase 20 | Complete |
| TUI-03 | Phase 20 | Complete |
| TUI-04 | Phase 20 | Complete |
| API-01 | Phase 21 | Complete |
| API-02 | Phase 21 | Complete |
| API-03 | Phase 21 | Complete |
| REL-01 | Phase 22 | Complete |
| REL-02 | Phase 22 | Complete |

**Coverage:**
- v2.8 requirements: 24 total
- Mapped to phases: 24
- Unmapped: 0

---
*Requirements defined: 2026-11-02*
*Last updated: 2026-11-02 after roadmap creation*
