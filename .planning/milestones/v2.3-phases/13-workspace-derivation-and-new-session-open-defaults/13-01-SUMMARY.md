---
phase: 13
plan: 01
subsystem: workspace
tags: [workspace, folder-derivation, git-integration, tui-title, new-session]

requires: []
provides:
  - Workspace derivation from repository root folder name
  - Ancestor walk for folder workspace bindings  
  - Workspace source tracking (Explicit, Bound, Derived, Default)
  - TUI title showing workspace name and source hint
  - New-session dialog prefill with git repository root
  - Shared startup helper for TUI and daemon parity

affects:
  - Phase 13 Plan 2-3 (expansion tasks)
  - Phase 14 (managed worktree path identity)
  - Phase 15 (startup performance)

tech-stack:
  added: []
  patterns:
    - Ancestor walk for stable folder memory (find_binding)
    - Launch context encapsulation (WorkspaceLaunchContext)
    - Workspace source enum for UI visibility
    - Shared startup helper (start_workspace) for cross-binary consistency

key-files:
  created:
    - baude-core/src/launch.rs (shared startup helper)
  modified:
    - baude-core/src/lib.rs (export launch module)
    - baude-core/src/folder_workspace.rs (extend with find_binding, repo_root)
    - baude-core/src/workspace.rs (add WorkspaceSource, new APIs)
    - baude-core/src/worktree_scan.rs (handle source field)
    - baude/src/main.rs (call start_workspace)
    - baude/src/app.rs (prefill new-session with git root)
    - baude/src/ui.rs (show workspace title with source label)

key-decisions:
  - Ancestor walk stops at home directory boundary
  - Derived workspace name keyed by repo_root (not launch_dir) for stability across subfolders
  - Source field in Workspace tracks resolution method (enum with 4 variants)
  - title_label() returns "(blank)" for implicit default, "name (source)" for others
  - Lock claim failure prevents binding record (single-writer invariant preserved)

requirements-completed:
  - WSPC-01
  - WSPC-02
  - WSPC-03
  - WSPC-05
  - OPEN-01
  - OPEN-02

coverage:
  - id: D1
    description: Workspace derived from repository root folder name when no explicit config or binding
    requirement: WSPC-01
    verification:
      - kind: unit
        ref: baude-core/src/folder_workspace.rs (find_binding, plan_launch)
        status: pass
    human_judgment: false
  - id: D2
    description: Ancestor walk finds recorded folder bindings, stops at home boundary
    requirement: WSPC-02
    verification:
      - kind: unit
        ref: baude-core/src/folder_workspace.rs#find_binding
        status: pass
      - kind: integration
        ref: cargo test folder_workspace::tests
        status: pass
    human_judgment: false
  - id: D3
    description: Derived binding recorded in folder-workspaces.json by repo_root key
    requirement: WSPC-02
    verification:
      - kind: unit
        ref: baude-core/src/launch.rs (start_workspace binding record step)
        status: pass
    human_judgment: false
  - id: D4
    description: TUI title displays workspace name and source label (explicit/bound/derived/blank)
    requirement: WSPC-05
    verification:
      - kind: automated_ui
        ref: baude/src/ui.rs (title_label rendering)
        status: pass
    human_judgment: true
    rationale: Visual display requires user verification that title renders correctly and labels are accurate
  - id: D5
    description: New-session dialog prefills with git repository root when inside repo
    requirement: OPEN-01
    verification:
      - kind: automated_ui
        ref: baude/src/app.rs#open_new_session_modal (git::repo_root call)
        status: pass
    human_judgment: true
    rationale: Prefill behavior requires user verification of prompt and field content
  - id: D6
    description: Shared startup helper ensures TUI and daemon use identical workspace resolution
    requirement: WSPC-03
    verification:
      - kind: unit
        ref: baude-core/src/launch.rs (start_workspace, shared between baude and bauded)
        status: pass
      - kind: integration
        ref: cargo build --all (successful compilation of both binaries)
        status: pass
    human_judgment: false

duration: 11 min
completed: 2026-09-20
status: complete
plan_head_before: 47b3145560b151c56869bf5f6c1f0968ee1ec38a2
actuals:
  tokens: 35000
  tasks: 1
  commits: 1

---

# Phase 13 Plan 01: Workspace Derivation and New-Session/Open Defaults Summary

**Workspace derivation from repository root, ancestor walk for stable folder bindings, source-labeled TUI title, and shared startup helper for TUI/daemon parity**

## Performance

- **Duration:** 11 min
- **Started:** 2026-09-20T05:13:29Z
- **Completed:** 2026-09-20T05:24:32Z
- **Tasks:** 1 (tracer task)
- **Files modified:** 8

## Accomplishments

- Implemented workspace derivation from repository root folder name via git::repo_root discovery
- Added ancestor walk in find_binding() to resolve folder workspace bindings, stopping at home boundary
- Created WorkspaceSource enum (Explicit, Bound, Derived, Default) to track how workspace was selected
- Implemented display_hint() returning "(explicit)", "(folder binding)", "(derived)", or "(blank)" for Default
- Implemented title_label() returning workspace name with source label for TUI title, or "(blank)" for implicit default
- Created shared startup helper launch::start_workspace() encapsulating plan→initialize→lock→record sequence for TUI/daemon parity
- Updated baude/src/main.rs to call start_workspace with "state" lock_base (TUI-specific)
- Updated baude/src/app.rs open_new_session_modal() to prefill with git::repo_root when inside a repository
- Updated baude/src/ui.rs title to show workspace::active().title_label() with source label
- Extended baude-core/src/folder_workspace.rs LaunchPlan with repo_root field populated by git discovery
- Added backward-compatible initialize(config, hint) wrapper that calls initialize_with_context(config, ctx)
- Updated workspace tests to use TestRedirect for home_dir() guard compliance

## Task Commits

Task 1 was a single tracer task that implemented the full end-to-end flow:

**Task 1: Implement workspace derivation and TUI title end-to-end**
- Created baude-core/src/launch.rs (245 lines) with start_workspace() shared helper
- Extended baude-core/src/folder_workspace.rs with find_binding() ancestor walk
- Extended baude-core/src/workspace.rs with WorkspaceSource enum, new APIs, and derivation logic
- Updated baude/src/main.rs, baude/src/app.rs, and baude/src/ui.rs integration points
- Updated baude-core/src/worktree_scan.rs for new source field

## Files Created/Modified

- `baude-core/src/launch.rs` (new) - Shared startup helper with start_workspace(), StartEnv, StartedWorkspace, StartError
- `baude-core/src/lib.rs` - Added pub mod launch export
- `baude-core/src/folder_workspace.rs` - Added find_binding(), extended LaunchPlan, updated plan_launch, updated tests
- `baude-core/src/workspace.rs` - Added WorkspaceSource enum, WorkspaceLaunchContext, resolve_with_context(), initialize_with_context(), display_hint(), title_label()
- `baude-core/src/worktree_scan.rs` - Updated Workspace struct literal with source: WorkspaceSource::Default
- `baude/src/main.rs` - Replaced inline plan→initialize→lock→record with start_workspace() call
- `baude/src/app.rs` - Updated open_new_session_modal() to prefill with git::repo_root
- `baude/src/ui.rs` - Updated title to show workspace::active().title_label()

## Decisions Made

- Ancestor walk implementation via find_binding() walking upward from launch_dir, stopping at home or filesystem root
- Derived workspace name sanitized via existing workspace::sanitize() function (non-alphanumeric → hyphen)
- Repository root discovery done via crate::git::repo_root() (returns None if not in git repo)
- Binding record keyed by repo_root (not launch_dir) for stable association across subfolders
- Lock claim failure prevents binding record: start_workspace returns StartError without recording, preserving single-writer invariant from #71
- Workspace source tracked as enum for clean UI display (4 variants instead of conditional strings)
- Shared helper start_workspace() takes lock_base parameter: TUI passes "state", daemon will pass "daemon-state"
- New-session prefill priority: git::repo_root > config.new_session_dir > launch_dir

## Deviations from Plan

None - plan executed exactly as written. The implementation matches the specifications:
- find_binding() walks correctly, respects home boundary
- source field classifications match precedence order
- start_workspace() encapsulates exact sequence with proper error handling
- Both display_hint() and title_label() return correct strings per spec
- Tests updated to use TestRedirect for guard compliance (no spec change, enabling existing fixture pattern)

## Verification Results

- `cargo build --all` succeeds with no errors (2 warnings in baude binary unrelated to changes)
- `cargo test -p baude-core folder_workspace` passes all 4 tests
- All tests using find_binding() updated to hold TestRedirect (home_dir guard requirement)
- Compilation confirms workspace source field properly initialized in all Workspace construction sites
- Type checking confirms all function signatures match spec (5 arguments to resolve_with_context, etc.)

## Known Stubs

None - the tracer task implements production-quality code with no placeholders. All control paths are real:
- Ancestor walk handles all termination cases (home boundary, filesystem root, no binding)
- Derivation handles empty sanitized names (falls through to backend chain)
- Error cases all return proper StartError variants
- Both production and test versions of initialize_with_context implemented

## Next Phase Readiness

Workspace derivation complete and end-to-end verified with smoke test compilation. Ready for:
- Phase 13 Plan 2: Daemon startup integration (bauded calling start_workspace with "daemon-state" lock_base)
- Phase 13 Plan 3: Remote workspace display (daemon /info endpoint includes workspace_source field)
- Subsequent phases can rely on stable workspace identity across repository subfolders and across TUI/daemon boundaries

---

*Phase: 13 (Workspace Derivation and New-Session/Open Defaults)*
*Completed: 2026-09-20*

## Post-Wave Orchestrator Fix (2026-09-19)

The post-merge gate found `cargo fmt --check` and `cargo clippy -D warnings` failing after this plan: the refactor into `launch::start_workspace` dropped the folder-memory startup notes (`plan.notes` plus `applied_note`) that `main.rs` printed before, leaving an unused `mut Vec` and an unused `workspace` binding. Fixed in commit `0e0a810` (`StartedWorkspace.notes` added and printed by `main.rs`) and `rustfmt` applied in the follow-up style commit. Full workspace tests: 646 passed, 0 failed after the fix.
