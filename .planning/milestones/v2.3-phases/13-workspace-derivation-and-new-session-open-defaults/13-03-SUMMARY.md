---
phase: 13
plan: 03
subsystem: workspace
tags: [workspace, daemon-parity, folder-derivation, remote-provenance, documentation]

requires:
  - phase: 13
    provides: [workspace derivation, shared startup helper, TDD test suite]

provides:
  - Daemon workspace resolution with shared startup helper
  - Daemon provenance (workspace_source) via /info endpoint
  - Remote header displaying daemon workspace with source label
  - Comprehensive README documentation of workspace selection and precedence chain
  - Tests covering daemon startup parity, /info endpoint, remote deserialization

affects:
  - Phase 14 (managed worktree path identity)
  - Phase 15 (startup performance)
  - Any phase that documents baude behavior

tech-stack:
  added: []
  patterns:
    - Daemon/TUI startup parity via shared launch::start_workspace helper
    - Workspace source tracking in daemon API responses
    - Background poller fetching both /sessions and /info endpoints
    - Remote snapshot caching workspace-level metadata

key-files:
  modified:
    - bauded/src/main.rs (call launch::start_workspace with "daemon-state" lock_base)
    - bauded/src/api.rs (extend /info response with workspace_source field)
    - baude/src/remote.rs (add workspace_source fields to RemoteInfo and RemoteSnapshot)
    - baude/src/remote.rs (poller fetches /info and stores daemon_workspace/daemon_workspace_source)
    - baude/src/ui.rs (remote_header displays daemon workspace with source label)
    - baude/src/app.rs (initialize RemoteSnapshot with daemon workspace fields in tests)
    - README.md (comprehensive documentation of workspace selection, precedence, derivation, new-session, title display)

key-decisions:
  - Daemon startup mirrors TUI: canonicalize current_dir, construct StartEnv, call start_workspace with daemon-specific lock_base
  - Lock claim happens before recording; refusal prevents partial writes (single-writer invariant preserved)
  - Workspace source computed via workspace::active().display_hint() and trimmed of parentheses for API
  - Remote snapshot caches daemon-level workspace metadata (not per-session) for header display
  - README documents full 7-level precedence chain and explains derivation boundaries
  - Daemon parity explicitly documented: both TUI and daemon use same resolver and lock semantics

requirements-completed:
  - WSPC-03 (daemon parity calling shared startup helper)
  - WSPC-04 (daemon recording derived bindings via shared code path)
  - WSPC-05 (workspace source display: TUI title + daemon-backed remote title)
  - OPEN-03 (repository deduplication test inherited from 13-02)
  - OPEN-04 (new-session prefill documented)

coverage:
  - id: D1
    description: Daemon startup calls shared launch::start_workspace helper with lock_base="daemon-state"
    requirement: WSPC-03
    verification:
      - kind: unit
        ref: baude-core/src/launch.rs#test_daemon_startup_parity_with_tui
        status: pass
    human_judgment: false
  - id: D2
    description: Daemon /info endpoint includes workspace_source field
    requirement: WSPC-05
    verification:
      - kind: unit
        ref: bauded/src/api.rs#info_endpoint_includes_workspace_source
        status: pass
    human_judgment: false
  - id: D3
    description: Remote header displays daemon workspace with source label
    requirement: WSPC-05
    verification:
      - kind: automated_ui
        ref: baude/src/ui.rs#remote_header
        status: pass
    human_judgment: false
  - id: D4
    description: README documents workspace selection precedence chain and derivation rule
    requirement: WSPC-03, OPEN-03, OPEN-04
    verification:
      - kind: other
        ref: README.md#workspace-selection, README.md#when-does-derivation-apply, README.md#new-session-defaults
        status: pass
    human_judgment: true
    rationale: Documentation accuracy and completeness requires human review to ensure clarity and correctness of precedence examples

duration: 47min
completed: 2026-09-20
status: complete
plan_head_before: 53bba6f
commits:
  - hash: 91b80e8
    message: "feat(13-03): add daemon workspace resolution with remote provenance display"
  - hash: 7fde0fc
    message: "docs(13-03): document workspace derivation, precedence chain, and new-session defaults"
actuals:
  tokens: 35000
  tasks: 2
  commits: 2

---

# Phase 13 Plan 03: Daemon Workspace Resolution and Documentation Summary

**Daemon calls shared startup helper with lock_base="daemon-state", exposes workspace_source via /info endpoint, displays remote workspace with source label, and comprehensive README documents full precedence chain and derivation rule with examples**

## Performance

- **Duration:** 47 min
- **Started:** 2026-09-20T06:24:00Z
- **Completed:** 2026-09-20T07:11:00Z
- **Tasks:** 2
- **Files modified:** 7

## Accomplishments

### Task 1: Daemon Workspace Resolution and Remote Provenance

- Daemon startup replaced inline initialization with call to shared `launch::start_workspace` helper
- Canonicalized daemon current directory before calling startup helper, matching TUI flow
- Added workspace_source field to bauded /info endpoint (explicit/bound/derived/blank)
- Extended RemotePoller to fetch both /sessions and /info endpoints in background poller
- Store daemon workspace and workspace_source in RemoteSnapshot for UI rendering
- Updated remote_header to display daemon workspace with source label: "remote: {name} ({source})"
- Added workspace_source field to RemoteInfo struct with #[serde(default)] for backward-compat
- Tests verify:
  - Daemon startup parity (test_daemon_startup_parity_with_tui exercises both TUI and daemon paths)
  - /info endpoint includes workspace_source field with correct values
  - RemoteInfo deserializes workspace_source from JSON (older daemons default to None)
  - RemoteSnapshot initialization includes daemon_workspace and daemon_workspace_source fields

### Task 2: README Documentation

- Rewrote Workspaces section with:
  - Full 7-level precedence order (BAUDE_WORKSPACE > binding > config > derived > BAUDE_BACKEND > config backend > default)
  - "When does derivation apply?" subsection explaining derivation only inside git repos
  - "Examples" subsection with 3 real-world cases
  - Daemon workspace resolution section documenting same rules as TUI
- Updated Folder context section with:
  - Ancestor walk mechanics: walks up from launch_dir, stops at $HOME
  - Recording: derives on first launch, records for future subfolder launches
  - Override/rebind: BAUDE_WORKSPACE, delete from folder-workspaces.json, config workspace (with binding priority note)
  - Kill switch: folder_context: false disables both reading and recording
- Added New-session defaults section explaining n prompt prefill (repo root when inside repo, config new_session_dir outside)
- Added TUI title section documenting workspace name and source label display
- Verified clone_base_dir section unchanged; clone destination logic unaffected

## Test Results

All tests pass with full workspace coverage:

- cargo build --all: ✓
- cargo fmt --all --check: ✓
- cargo clippy --all-targets -- -D warnings: ✓
- cargo test --all --lib: 677 tests passed

## Files Created/Modified

- `bauded/src/main.rs` - Daemon startup calls launch::start_workspace instead of initialize
- `bauded/src/api.rs` - /info handler includes workspace_source field; test added for endpoint
- `baude/src/remote.rs` - RemoteInfo and RemoteSnapshot extended with workspace source fields; poller fetches /info; deserialization test added
- `baude/src/ui.rs` - remote_header displays daemon workspace with source label
- `baude/src/app.rs` - RemoteSnapshot initializers updated with daemon_workspace and daemon_workspace_source fields
- `README.md` - Comprehensive documentation of workspace selection, precedence, derivation, new-session defaults, and title display

## Decisions Made

- Daemon lock_base is "daemon-state" (from manager::STATE_BASE constant), distinct from TUI "state"
- Workspace source computed via display_hint() with parentheses trimmed for JSON payload
- Daemon workspace metadata cached at RemoteSnapshot level (daemon-level, not per-session)
- Remote header displays workspace with same format as local title for consistency: "remote: {name} ({source})"
- README precedence order follows locked decision from 13-CONTEXT.md exactly
- Examples include repo subfolder, bound parent, and unbound non-git folder to cover all derivation cases

## Deviations from Plan

None - plan executed exactly as written. Implementation matches all acceptance criteria:
- Both TUI and daemon call launch::start_workspace from shared helper
- Lock claim happens before recording; refusal prevents partial writes
- Daemon startup parity verified via test_daemon_startup_parity_with_tui
- /info endpoint includes workspace_source field for all four source types
- Remote header displays daemon workspace with source label
- README documents full precedence chain with examples
- clone_base_dir documentation remains unchanged
- All tests pass; fmt, clippy, build all green; 677 tests passing

## Next Phase Readiness

Daemon workspace resolution complete with full parity with TUI. Remote views display daemon workspace source label matching local title format. README comprehensively documents workspace selection rules and boundaries. Ready for:
- Phase 14: Managed worktree path identity and collision handling
- Phase 15: Startup performance and timing facilities
- User understanding of workspace derivation and how to override or rebind folders

---

*Phase: 13 (Workspace Derivation and New-Session/Open Defaults)*
*Completed: 2026-09-20*
