# Requirements: baude

**Defined:** 2026-09-19
**Milestone:** v2.3 Launch Defaults and Startup Speed
**Core Value:** You can see at a glance which of your many coding-agent sessions needs you next and act on it from the terminal or phone.

## v2.3 Requirements

Approved scope: opening baude from any folder with no arguments lands in the right workspace, offers the right repository, never collides with another repository's managed worktrees, and reaches the first frame fast. Pane-focus bug #89 is pulled in as a small UX item.

**Baseline facts (2026-09-19, v2.2.0):** the workspace resolution chain is `BAUDE_WORKSPACE` -> folder-memory hint (exact canonical launch-dir match in `~/.config/baude/folder-workspaces.json`) -> config `workspace` -> `BAUDE_BACKEND` -> config `backend` -> `claude`; the launch folder never names the workspace. The `n` prompt prefills config `new_session_dir` before the launch dir, and the launch dir is argv[1] or cwd canonicalized, not the git toplevel. Managed worktrees live at `~/.local/share/baude/worktrees/<workspace>/repository-<key>/<primary|branch>-<key>` where keys are per-state-file counters (TUI and daemon each start at 1) and the path carries no repository identity, so a collision surfaces only as a hard `PathCollision` error. Startup eagerly restores every Active checkout (three git subprocesses, two PTYs, and 3-5 fsync'd full state rewrites each), runs a synchronous kitty keyboard probe with a 2 s deadline before the first frame, and has no logging or timing facility.

### Workspace derivation

- [x] **WSPC-01**: With no explicit workspace, baude walks up from the launch dir to the nearest recorded folder binding and uses that workspace
- [x] **WSPC-02**: With no binding on any ancestor, baude derives the workspace from the launch repository root's folder name (sanitized) and records a binding for that root so later launches are stable
- [x] **WSPC-03**: Explicit `BAUDE_WORKSPACE`, config `workspace`, and the `folder_context` flow keep priority over derivation, and the README documents the full precedence and the derivation rule
- [x] **WSPC-04**: `bauded` resolves the workspace by the same rule for the same launch dir, so TUI and daemon agree on the workspace
- [x] **WSPC-05**: The TUI shows a clear title at the top naming the active workspace (and whether it was explicit, bound, or derived); when no workspace applies it shows a placeholder such as `(blank)`

### New-session and open defaults

- [x] **OPEN-01**: The `n` new-session prompt prefills the git toplevel of the launch dir when baude was launched inside a repository
- [x] **OPEN-02**: Config `new_session_dir` is used as the prefill only when the launch dir is outside any repository
- [x] **OPEN-03**: Launching baude from a subfolder of a repository admits and focuses the same repository row as launching from its root
- [x] **OPEN-04**: Clone-on-demand keeps its `clone_base_dir` destination semantics; only directory-open paths change, and the README documents the new defaults

### Managed worktree identity

- [x] **WTID-01**: Two repositories can never resolve to the same managed worktree path, across TUI and daemon state files and after a state file reset
- [x] **WTID-02**: Existing managed checkouts are recognized under the new identity scheme in place, with nothing moved, re-cloned, or deleted
- [x] **WTID-03**: When a collision is still detected, baude names the repository that owns the path and offers a non-destructive resolution instead of a hard error
- [x] **WTID-04**: `baude worktrees scan` reports the owning repository for each managed checkout

### Startup and idle performance

- [x] **PERF-01**: A timing facility (env var or flag) records each startup stage's duration so a slow launch can be diagnosed without a debugger
- [x] **PERF-02**: The first frame renders before session restore and the first metadata poll complete; restore runs lazily or off the render path
- [x] **PERF-03**: The kitty keyboard probe never delays the first frame beyond a short bound and degrades to legacy encoding when the terminal does not answer
- [x] **PERF-04**: Session restore writes the durable state file once, batched, instead of several fsync'd full rewrites per restored session
- [x] **PERF-05**: The TUI redraws only when something changed (dirty flag); working and waiting rows show a static busy/thinking glyph and a static needs-input marker instead of a wall-clock spinner or flash, so an idle baude sends no terminal writes and a working baude repaints only on child output, status transitions, input, or resize
- [x] **PERF-06**: Per-session metadata polling runs only for live, non-archived rows and skips unchanged files by mtime, so idle polling cost no longer scales with the number of sessions
- [x] **PERF-07**: An opt-in setting suspends or stops idle Claude children after the auto-archive timeout, and archiving a row can stop its child rather than only hiding the row
- [x] **PERF-08**: The usage poller can be disabled or slowed via config, and it never scans transcripts more often than the configured interval

### Session UX

- [x] **UX-01**: Per-session pane focus (Claude pane or expanded shell pane) is remembered across session switches, and a key moves focus between the two panes while both are visible (GitHub #89)
- [x] **UX-02**: Session and checkout rows show a static single-character status code that reads without a key, replacing the filled/empty circle and spinner glyphs: `?` waiting for your input, `B` busy, `✓` completed, `✗` exited, `-` closed, `A` archived, `!` unavailable; each code keeps the existing per-state color (yellow waiting, blue busy, green completed, dark gray exited and archived, gray closed, yellow unavailable) so the colored letter is the icon, and a one-line legend is reachable in-app (sidebar footer or help view). Exact characters are confirmed in Phase 15 discuss; this set is the default

### Release

- [ ] **SHIP-05**: A maintainer can publish v2.3.0 through the existing release-please workflow only after tests, CI, and a terminal smoke pass succeed, with README and release notes reflecting the new defaults

## Future Requirements

Deferred to a later milestone. Tracked but not in the current roadmap.

### Workspace derivation

- **WSPC-F1**: Interactive workspace picker on first launch from an unbound folder (derivation covers the default; a picker is a later refinement)
- **WSPC-F2**: Merge or move sessions between workspaces (for users who want to fold the historical `claude` workspace into derived ones)

### Startup performance

- **PERF-F1**: Parallel git reconcile across checkouts during restore
- **PERF-F2**: Incremental metadata poll (transcript tail from the last offset on first tick)

### Carried from v2.2

- Proxy-monitor integration: deferred until the external telemetry contract is resolved
- Dormant local branch rows, dormant-branch activation UI, and safe branch deletion
- Daemon-backed remote TUI and PWA repository hierarchy/action parity

## Out of Scope

Explicitly excluded. Documented to prevent scope creep.

| Feature | Reason |
|---------|--------|
| Configurable worktree base directory (`worktree_dir`) | Orthogonal to identity; the collision is in path composition, not location |
| Automatic deletion or relocation of existing managed worktrees during migration | Safety constraint: cleanup stays preview-first and manual (v2.2 leak-scan decision) |
| Renaming existing workspaces or rewriting historical state files | Derivation only applies when nothing is passed; existing bindings and explicit config keep working unchanged |
| Multi-user or auth layer | Single-user, VPN-bound by design |
| Coding agents other than Claude Code and OpenCode | Backend support is intentionally explicit |

## Traceability

Which phases cover which requirements. Updated during roadmap creation.

| Requirement | Phase | Status |
|-------------|-------|--------|
| WSPC-01 | Phase 13 | Complete |
| WSPC-02 | Phase 13 | Complete |
| WSPC-03 | Phase 13 | Complete |
| WSPC-04 | Phase 13 | Complete |
| WSPC-05 | Phase 13 | Complete |
| OPEN-01 | Phase 13 | Complete |
| OPEN-02 | Phase 13 | Complete |
| OPEN-03 | Phase 13 | Complete |
| OPEN-04 | Phase 13 | Complete |
| WTID-01 | Phase 14 | Complete |
| WTID-02 | Phase 14 | Complete |
| WTID-03 | Phase 14 | Complete |
| WTID-04 | Phase 14 | Complete |
| PERF-01 | Phase 15 | Complete |
| PERF-02 | Phase 15 | Complete |
| PERF-03 | Phase 15 | Complete |
| PERF-04 | Phase 15 | Complete |
| PERF-05 | Phase 15 | Complete |
| PERF-06 | Phase 15 | Complete |
| PERF-07 | Phase 15 | Complete |
| PERF-08 | Phase 15 | Complete |
| UX-01 | Phase 16 | Complete |
| UX-02 | Phase 15 | Complete |
| SHIP-05 | Phase 17 | Pending |

**Coverage:**

- v2.3 requirements: 23 total
- Mapped to phases: 23
- Unmapped: 0 ✓

---
*Requirements defined: 2026-09-19*
*Last updated: 2026-09-19 after roadmap creation*
