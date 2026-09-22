# Phase 13: Workspace Derivation and New-Session/Open Defaults - Context

**Gathered:** 2026-09-19
**Status:** Ready for planning
**Mode:** Smart discuss, autonomous (`--auto`): recommended answers accepted and logged

<domain>
## Phase Boundary

Opening baude from any folder with no arguments uses the right workspace and repository. The workspace is derived from the launch folder when nothing explicit is set: nearest recorded folder binding walking up from the launch dir, else the repository root's folder name. New-session and open paths default to the launch repository's git root ahead of `new_session_dir`. Explicit `BAUDE_WORKSPACE`, config `workspace`, and `folder_context` keep priority. `bauded` applies the same rule. Requirements: WSPC-01..05, OPEN-01..04. WSPC-05 (added during discuss, 2026-09-19): the TUI shows a clear title at the top naming the active workspace. Managed worktree path identity (Phase 14), startup performance (Phase 15), pane focus (Phase 16) are out of this phase.

</domain>

<decisions>
## Implementation Decisions

### Derivation rule
- Ancestor walk runs from the canonical launch dir upward to `$HOME` (or the filesystem root when the launch dir is outside home); the nearest recorded binding wins.
- When no binding matches, derive the workspace from the repository root's directory basename, passed through the existing `workspace::sanitize` rules (`[A-Za-z0-9_-]`, else `-`).
- A launch dir that is not inside a repository and has no binding uses the existing chain unchanged (config `workspace`, `BAUDE_BACKEND`, config `backend`, `claude`); nothing is derived or recorded.
- A derived workspace is recorded as `repo_root -> workspace` in `folder-workspaces.json` using the existing schema (no new fields) so later launches from any subfolder resolve through the walk and stay stable.

### Precedence and overrides
- Resolution order: `BAUDE_WORKSPACE` > folder binding found by the ancestor walk > config `workspace` > derived repo-root name > `BAUDE_BACKEND` > config `backend` > `claude`.
- A derived workspace takes the default backend (`BAUDE_BACKEND`, then config `backend`, then claude); the config `workspaces` map may still declare a backend for a derived name.
- The `folder_context` flow is unchanged; its bindings live in the same file and are honored by the walk.
- README: rewrite the Workspaces precedence list and the folder-memory section with the derivation rule, examples (repo root, subfolder, bound parent, unbound non-git folder), and how to override or rebind.

### New-session and open defaults
- Repository root comes from the existing `git::repo_root` (git toplevel) of the launch dir; a linked worktree resolves to the worktree root.
- `n` prefill: inside a repository, the repository root (`new_session_dir` ignored); outside, `new_session_dir` when set, else the launch dir.
- Launch-dir auto-admission resolves the repository root and identifies the repository by canonical common dir (existing `discover_repository`), so launching from the root or any subfolder admits and focuses the same repository row with no duplicates.
- Clone-on-demand destination is unchanged (`clone_base_dir`, else `~/Code/<host>/<owner>/<repo>`); README documents both the open defaults and the clone default.

### Daemon parity and safety
- One shared resolver in `baude-core` (`workspace` and `folder_workspace` modules) implements the walk and derivation; both `baude` and `bauded` call it at startup with their launch dir.
- `bauded` records derived bindings through the same code path and file.
- Tests: unit tests for the ancestor walk, derivation, sanitization, and precedence under the existing TestRedirect fixtures; app-level tests for `n` prefill (inside and outside a repo) and subfolder admission; a daemon test for parity. No test may touch the real `~/.config/baude`.
- No automatic migration of sessions already in the `claude` workspace; existing bindings keep routing the iarx-com and joese-iarx trees to `claude`. README explains how to rebind a folder.

### Workspace title (WSPC-05, added by Joe during discuss)
- The TUI shows a clear title at the top of the whole window naming the active workspace, so it is obvious which workspace a launch landed in.
- The title also shows how the workspace was chosen: explicit (env or config), bound (folder binding), or derived (repo-root name). Wording is Claude's discretion; keep it to one short line.
- When no workspace applies (implicit default, nothing explicit, bound, or derived), show a placeholder such as `(blank)` rather than the literal default name, so an unconfigured launch is visibly unconfigured.
- Render in the existing top chrome (reuse the current header or sidebar title area rather than adding a new row when possible); remote-attach and daemon-backed views show the daemon's workspace the same way.

### Claude's Discretion
- Exact placement of the shared resolver (new module vs. extending `folder_workspace::plan_launch`) and the walk's stop condition implementation.
- Whether the derived binding is written before or after the workspace lock is claimed, as long as a lock refusal does not leave a half-written binding file.

</decisions>

<code_context>
## Existing Code Insights

### Reusable Assets
- `baude-core/src/folder_workspace.rs:71-98` `plan_launch` (exact-match lookup keyed by canonical launch dir) and `record` (main.rs:405-414): extend with the ancestor walk and derived-binding write.
- `baude-core/src/workspace.rs:144-151` resolution chain and `workspace::sanitize` (99-111); `DEFAULT = "claude"` (58); `initialize` (276).
- `baude/src/main.rs:343-347` launch dir (argv[1] or cwd, canonicalized); `main.rs:359-370` plan_launch plus initialize.
- `baude/src/app.rs:4042-4056` `n` prompt prefill (`config.new_session_dir` else launch dir); `app.rs:2815-2817` the only existing `git::repo_root` fallback; `app.rs:4723-4777` `open_repo_session_via` -> `git::discover_repository` -> admission; `app.rs:1321-1324` startup auto-admission.
- `baude/src/ui.rs:223` outer block title `" baude v<version> "` is the existing top-of-window chrome for WSPC-05; `ui.rs:1283` already calls `baude_core::workspace::active().display_label()` (info overlay), so a display label exists to reuse.
- `bauded/src/manager.rs:808` daemon admission equivalent; daemon state file `daemon-state-<ws>.json` (manager.rs:36).
- Tests to extend: `workspace.rs:656-810` (defaulting chain), `folder_workspace.rs:148-214` (folder memory), `app.rs:8348-8482` (`admit_repository_*`).

### Established Patterns
- Workspace is the state namespace: `state-<name>.json` / `daemon-state-<name>.json` beside `~/.config/baude/config.json`; one writer per workspace lock.
- Repository identity is the canonicalized `git rev-parse --git-common-dir`, never a path string or remote URL.
- Fixture realism: every path resolves through one `TestRedirect`; an escape guard aborts tests that reach the real home.

### Integration Points
- `baude/src/main.rs` startup (plan_launch -> initialize -> lock -> record) and `bauded` startup.
- README sections: Workspaces (precedence, state files), folder memory, config key list (`new_session_dir`, `clone_base_dir`, `workspace`/`workspaces`, `folder_context`).

</code_context>

<specifics>
## Specific Ideas

- Joe's live bindings: `~/Code/github.com/poindexter12 -> poindexter12`, `~/Code/github.com/iarx-com -> claude`, `~/Code/github.com/joese-iarx -> claude`, `.../baude/target/release -> smoke`. Launching from `~/Code/github.com/poindexter12/baude` must land in `poindexter12` via the walk, not `claude`.
- Joe's config sets `new_session_dir: "~/Code/"`; after this phase the `n` prompt inside a repo shows that repo's root instead.

</specifics>

<deferred>
## Deferred Ideas

- Interactive workspace picker on first launch from an unbound folder (WSPC-F1).
- Moving or merging sessions between workspaces (WSPC-F2).
- Managed worktree path identity and collision handling (Phase 14).

</deferred>
