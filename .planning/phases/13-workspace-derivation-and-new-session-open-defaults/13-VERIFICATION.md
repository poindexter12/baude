---
phase: 13-workspace-derivation-and-new-session-open-defaults
verified: 2026-09-20T09:56:00Z
status: passed
score: 9/9 must-haves verified
covered_files:
  - baude-core/src/launch.rs
  - baude-core/src/folder_workspace.rs
  - baude-core/src/workspace.rs
  - baude-core/src/lib.rs
  - baude-core/src/worktree_scan.rs
  - baude/src/main.rs
  - baude/src/app.rs
  - baude/src/ui.rs
  - baude/src/remote.rs
  - bauded/src/main.rs
  - bauded/src/api.rs
  - README.md
covered_digest: "v1:sha256:6eeceeeeebbf3fdf6a5e43ce926441ea2904be130659abfd8a8e312ac4d4d3e2"
behavior_unverified: 0
overrides_applied: 0
---

# Phase 13: Workspace Derivation and New-Session/Open Defaults Verification Report

**Phase Goal:** Opening baude from any folder automatically uses the right workspace and repository, with recorded defaults and explicit config still winning.

**Verified:** 2026-09-20T09:56:00Z
**Status:** PASSED
**Re-verification:** No — initial verification

## Goal Achievement

### Observable Truths

| # | Truth | Status | Evidence |
|---|-------|--------|----------|
| 1 | Ancestor walk finds recorded folder bindings and stops at home boundary | ✓ VERIFIED | baude-core/src/folder_workspace.rs:find_binding() implements tree walk from launch_dir upward; canonicalizes home for boundary check; 7 unit tests pass (parent, grandparent, nearest, home-boundary, no-binding, at-home, symlink-boundary cases) |
| 2 | Workspace derived from repository root basename and recorded in folder-workspaces.json | ✓ VERIFIED | plan_launch() calls git::repo_root(); workspace::resolve_with_context derives name via sanitize(); launch::start_workspace records binding only after lock claim succeeds (lines 105-118); WSPC-02 end-to-end test passes (derives → persists → recalls) |
| 3 | Workspace precedence order enforced: BAUDE_WORKSPACE > bound > config > derived > BAUDE_BACKEND > config backend > default | ✓ VERIFIED | workspace::resolve_with_context() implements 7-level precedence; WorkspaceSource enum tracks source correctly; precedence matrix test validates all 7 levels in order; BAUDE_BACKEND gate removed from plan_launch (no suppression of binding/derivation) |
| 4 | TUI and daemon apply same workspace resolution for same launch directory | ✓ VERIFIED | Both baude/src/main.rs and bauded/src/main.rs call launch::start_workspace with shared logic; daemon parity test exercises both paths with same inputs and asserts identical results (name, source, repo_root) |
| 5 | TUI title displays workspace name and source label; implicit default shows (blank) placeholder | ✓ VERIFIED | ui.rs:223 uses workspace::active().title_label(); title_label() returns "(blank)" for Default or "name (source)" for others; 4 UI title rendering tests pass for all source types; remote UI renders "remote: {name} ({source})" matching format |
| 6 | New-session prompt prefills git repository root when launched inside a repository | ✓ VERIFIED | app.rs:open_new_session_modal calls git::repo_root() first; if Some, prefills repo root with trailing /; test_open_new_session_prefill_inside_repo_returns_git_root passes |
| 7 | New-session prefill uses config.new_session_dir when outside repository; launch_dir as fallback | ✓ VERIFIED | app.rs implements fallback chain: git::repo_root (if inside repo) → config.new_session_dir (if set) → launch_dir; all three cases tested and pass with consistent trailing / |
| 8 | Launching from repository root or subfolder admits and focuses same repository row | ✓ VERIFIED | app.rs admits via git::discover_repository using canonical common dir; 2 deduplication tests pass: same repo admitted from root and subfolder creates one row; no duplicate repository rows |
| 9 | Clone-on-demand destination semantics unchanged; README documents new defaults | ✓ VERIFIED | No modifications to clone_base_dir logic; README documents new-session defaults (section "New-session defaults") and full workspace precedence (section "Workspace selection: precedence order"); daemon parity documented in "Daemon workspace resolution" section |

**Score:** 9/9 truths verified

## Required Artifacts

| Artifact | Expected | Status | Details |
|----------|----------|--------|---------|
| baude-core/src/launch.rs | Shared startup helper with start_workspace function | ✓ VERIFIED | File exists; pub fn start_workspace(launch_dir, config, env, lock_base) → Result<StartedWorkspace, StartError> encapsulates canonicalize → plan_launch → initialize_with_context → claim lock → conditional record sequence |
| baude-core/src/folder_workspace.rs | find_binding() for ancestor walk; plan_launch returns repo_root | ✓ VERIFIED | find_binding(root, launch_dir, home) → Option<String> walks upward; canonicalizes home for boundary; plan_launch returns LaunchPlan with repo_root: Option<PathBuf> field from git::repo_root() call |
| baude-core/src/workspace.rs | WorkspaceSource enum; WorkspaceLaunchContext struct; initialize_with_context; display_hint; title_label | ✓ VERIFIED | All 5 components exist: enum with (Explicit, Bound, Derived, Default); struct with (hint, repo_root); functions with correct signatures and behavior |
| baude/src/main.rs | TUI startup calls launch::start_workspace with "state" lock_base | ✓ VERIFIED | Lines ~350-370 call launch::start_workspace(&launch_dir, &config, env, "state"); handles LockHeld and LockIo errors with appropriate exit behavior |
| bauded/src/main.rs | Daemon startup calls launch::start_workspace with "daemon-state" lock_base | ✓ VERIFIED | Lines ~180-195 canonicalize current_dir and call launch::start_workspace(&launch_dir, &config, env, "daemon-state"); mirrors TUI startup flow |
| baude/src/ui.rs | Title uses workspace::active().title_label() | ✓ VERIFIED | Line 223 formats title with title_label() output; UI title tests verify all 4 source labels render correctly |
| baude/src/app.rs | open_new_session_modal prefills git repo root first, then config, then launch_dir | ✓ VERIFIED | Lines ~4065-4074 implement: git::repo_root → config.new_session_dir → launch_dir; all cases tested; trailing / maintained |
| bauded/src/api.rs | /info handler includes workspace_source field | ✓ VERIFIED | /info endpoint computes workspace_source from workspace::active().display_hint() trimmed of parentheses; test_info_endpoint_includes_workspace_source passes |
| baude/src/remote.rs | Parses and stores workspace_source from daemon /info response | ✓ VERIFIED | RemoteInfo struct has workspace_source: Option<String> field with #[serde(default)]; parser extracts from JSON /info response; stored in RemoteSnapshot.daemon_workspace_source |
| README.md | Documents precedence order, derivation rule, new-session defaults, title display, daemon parity | ✓ VERIFIED | All 5 sections present: "Workspace selection: precedence order" (7 levels), "When does derivation apply" (rules + examples), "New-session defaults" (prefill logic), "TUI title display" (source labels), "Daemon workspace resolution" (parity statement) |

## Key Links Verification

| From | To | Via | Status | Details |
|------|----|----|--------|---------|
| TUI startup | Workspace cache | launch::start_workspace → initialize_with_context | ✓ WIRED | launch.rs:99-101 calls initialize_with_context; baude/src/main.rs:~351 calls start_workspace |
| TUI startup | State lock claim | launch::start_workspace → persist::claim_workspace_state_lock | ✓ WIRED | launch.rs:105; lock claim before recording (lines 105-118) |
| Daemon startup | Workspace cache | launch::start_workspace → initialize_with_context | ✓ WIRED | bauded/src/main.rs:~188 calls start_workspace identically to TUI |
| plan_launch | Ancestor walk | folder_workspace::find_binding | ✓ WIRED | plan_launch calls find_binding at lines ~99-100; walk stops at home |
| Ancestor walk | Repository root discovery | git::repo_root via plan_launch | ✓ WIRED | plan_launch calls git::repo_root at line ~95; returns repo_root in LaunchPlan |
| Workspace resolution | Source tracking | workspace::resolve_with_context sets source field | ✓ WIRED | workspace.rs:380-420; source assigned per precedence (explicit/bound/derived/default) |
| UI title | Workspace display | workspace::active().title_label() | ✓ WIRED | ui.rs:223; title_label formats "name (source)" or "(blank)" for default |
| New-session prefill | Git root discovery | app.rs calls git::repo_root | ✓ WIRED | app.rs:~4066; repo_root checked before config fallback |
| Daemon /info | Workspace source | workspace::active().display_hint() | ✓ WIRED | bauded/src/api.rs:~844; display_hint trimmed for JSON response |
| Remote UI | Daemon workspace source | Parses and renders from daemon_workspace_source field | ✓ WIRED | baude/src/ui.rs:~656; fetches daemon_workspace_source from RemoteSnapshot |

## Behavioral Spot-Checks

| Behavior | Test Case | Expected | Status | Evidence |
|----------|-----------|----------|--------|----------|
| Ancestor walk stops at home | find_binding_stops_at_home_boundary | Walk terminates at canonical home, returns None if no binding above | ✓ PASS | Test exists and passes; walk uses while loop with home boundary check (lines 102-112) |
| Ancestor walk stops at filesystem root | walk outside home boundary | When launch_dir outside home, stops at FS root not above | ✓ PASS | Test handles /tmp outside /home scenario; loop uses current.pop() which returns false at root |
| Derived workspace name sanitized | sanitize_empty_input | Converts non-[A-Za-z0-9_-] to -, empty input returns "" | ✓ PASS | Test exists; workspace.rs sanitize function applies character rules |
| Workspace derivation only in repos | plan_launch_with_no_repo | Non-git folder returns None repo_root, skips derivation | ✓ PASS | Test verifies non-repo path returns plan with repo_root: None |
| Binding recorded after lock succeeds | test_wspc02_start_workspace_derives_persists_recalls | Binding persisted in folder-workspaces.json after lock claim | ✓ PASS | Test creates repo, calls start_workspace, verifies binding written at repo_root |
| Lock refusal prevents binding write | Lock failure scenario | If lock claim fails (Held/Io), binding is NOT recorded | ✓ PASS | Code review verified (launch.rs:105 lock claim must succeed before line 112 record call); Err paths (lines 120-145) return without recording |
| TUI title shows (blank) for default | title_rendering_with_default_source_shows_blank | When source=Default, title shows "(blank)" not workspace name | ✓ PASS | Test exists; title_label returns "(blank)" for Default enum variant |
| New-session prefill includes trailing / | test_open_new_session_prefill_all_cases_end_with_slash | All three prefill cases (repo, config, fallback) end with "/" | ✓ PASS | Test verifies: buf.ends_with('/') for all cases |

## Requirements Coverage

| Requirement | Source Plan | Description | Status | Evidence |
|-------------|------------|-------------|--------|----------|
| WSPC-01 | 13-01, 13-02 | Ancestor walk for folder bindings | ✓ SATISFIED | find_binding() walks from launch_dir to home; 7 unit tests; plan_launch integration test |
| WSPC-02 | 13-01, 13-02, 13-03 | Derive workspace from repo root and record binding | ✓ SATISFIED | plan_launch discovers repo_root; resolve_with_context derives name; binding recorded after lock claim succeeds; end-to-end test exercises flow |
| WSPC-03 | 13-01, 13-02 | Precedence chain and README documentation | ✓ SATISFIED | 7-level precedence in resolve_with_context; README has 5 documentation sections covering precedence, derivation rule, examples, new-session defaults, daemon parity |
| WSPC-04 | 13-03 | Daemon parity with TUI workspace resolution | ✓ SATISFIED | Both call start_workspace; daemon parity test verifies identical results; daemon /info exposes workspace_source |
| WSPC-05 | 13-01, 13-02, 13-03 | TUI title shows workspace name and source label | ✓ SATISFIED | title_label() in workspace.rs; ui.rs renders it; display_hint() returns correct labels; 4 UI title tests pass; daemon workspace source displayed in remote UI |
| OPEN-01 | 13-02, 13-03 | New-session prefill uses git repo root inside repo | ✓ SATISFIED | app.rs calls git::repo_root() first; test_open_new_session_prefill_inside_repo_returns_git_root passes |
| OPEN-02 | 13-02, 13-03 | New-session prefill fallback outside repo | ✓ SATISFIED | Fallback chain: git::repo_root → config.new_session_dir → launch_dir; 2 tests verify fallback cases |
| OPEN-03 | 13-02, 13-03 | Repository deduplication by canonical common dir | ✓ SATISFIED | discover_repository uses canonical common dir; 2 tests verify same repo from root and subfolder creates one row |
| OPEN-04 | 13-03 | Clone-on-demand unchanged; README documents | ✓ SATISFIED | clone_base_dir logic untouched; README "Cloning" section and "New-session defaults" document clone and new-session paths separately |

## Anti-Patterns Found

| File | Line | Pattern | Severity | Disposition |
|------|------|---------|----------|-------------|
| (all modified files) | - | Code review clean; no TBD/FIXME/XXX debt markers in phase 13 code | - | CLEAN |
| (all modified files) | - | No stubs or placeholder implementations | - | CLEAN |
| (all modified files) | - | All tests use TestRedirect fixture; no real home touched | - | CLEAN |

## Test Summary

- **Total tests passing:** 383 (all lib tests)
- **Workspace-specific tests:** 35 tests across 5 categories
  - find_binding tests: 7
  - workspace resolution tests: 11  
  - app prefill and dedup tests: 6
  - UI rendering tests: 4
  - daemon/parity tests: 2
  - launch helper tests: 5
- **Test isolation:** All tests use TestRedirect fixture; no real filesystem touched
- **Code gates passed:**
  - `cargo fmt --all -- --check`: ✓ PASS
  - `cargo clippy --all-targets -- -D warnings`: ✓ PASS
  - `cargo build --workspace`: ✓ PASS
  - `cargo test --workspace`: ✓ PASS (383 passed, 0 failed)

## Code Review Status

- **Depth:** standard (fix-loop iteration 2)
- **Files reviewed:** 12
- **Findings:** 0 critical, 0 warning, 0 info (CLEAN after fixes)
- **Fixes applied:** 2 (CR-01: symlink boundary canonicalization; WR-01: canonicalization doc contract)
- **New test added:** find_binding_respects_symlinked_home_boundary (validates symlink fix)
- **Status:** CLEAN

## Summary

All 9 observable truths are verified, all artifacts exist and are properly wired, all key links are connected, all requirements are satisfied, and the codebase compiles with all tests passing.

**Phase 13 goal achieved:** Opening baude from any folder automatically uses the right workspace and repository, with recorded defaults and explicit config still winning.

- TUI and daemon both derive workspace from repository root when no explicit binding/config exists
- Ancestor walk finds recorded bindings and stops at home boundary
- Workspace source (explicit/bound/derived/default) is tracked and displayed in TUI title
- New-session prefill defaults to git repo root inside repos, config/launch_dir outside
- Repository deduplication works: same repo from root or subfolder is identified as one row
- Lock claim precedes binding record; lock refusal prevents partial writes (single-writer invariant preserved)
- Full documentation in README covers precedence, derivation rules, new-session defaults, daemon parity
- Code review clean; no security or correctness issues found

---

_Verified: 2026-09-20T09:56:00Z_
_Verifier: Claude (gsd-verifier)_
_Mode: Goal-backward verification_
