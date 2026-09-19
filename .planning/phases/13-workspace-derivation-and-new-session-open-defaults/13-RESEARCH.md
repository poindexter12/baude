# Phase 13: Workspace Derivation and New-Session/Open Defaults - Research

**Researched:** 2026-09-19
**Domain:** Workspace resolution, folder memory, repository discovery, TUI startup behavior
**Confidence:** HIGH

## Summary

Phase 13 implements automatic workspace derivation from repository root folder names and corrects the new-session and open-path prefills to use repository context when available. The codebase already has the infrastructure: canonical path resolution in `main.rs:343-347`, folder-workspace memory in `folder_workspace.rs:71-98`, a resolution chain in `workspace.rs:116-167`, and repository discovery via `git::discover_repository`. The phase extends these with an ancestor walk up the filesystem to find the nearest recorded binding, derives the workspace name from the repository root via the existing `sanitize` function, and records the binding for stability. The new-session prompt prefill switches from config `new_session_dir` to the repository root when launched inside a git repository. The TUI title at `ui.rs:223` is updated to show the active workspace and how it was chosen (explicit, bound, or derived), building on the existing `workspace::active().display_label()` call at `ui.rs:1283`.

All changes are scoped to the workspace, folder_workspace, and app modules in baude and bauded; all paths are canonicalized before use; all tests use the existing `TestRedirect` fixture helper; and daemon parity is maintained through shared resolver code in `baude-core`. No existing workspace bindings or explicit config are disturbed.

**Primary recommendation:** Implement the ancestor walk in `folder_workspace` as a new function `find_binding` that returns the first matching entry walking up from launch dir to `$HOME`, then extend `plan_launch` to call it; extend `workspace::resolve_with_hint` to accept an optional derivation hint; add a new `display_hint` method to `Workspace` that returns the source (explicit, bound, derived, or default); update the `n` prompt to use `git::repo_root` ahead of `new_session_dir`; and update the TUI title at `ui.rs:223` to include the workspace name and display hint from the active workspace.

## User Constraints (from CONTEXT.md)

### Locked Decisions

- **Ancestor walk bounds:** Walk from canonical launch dir upward to `$HOME` (or filesystem root if launch dir is outside home); nearest recorded binding wins.
- **Derivation rule:** When no binding matches, derive the workspace from the repository root's directory basename, sanitized via the existing `sanitize` function (keep `[A-Za-z0-9_-]`, replace others with `-`); record the derived binding for stability.
- **Precedence chain:** `BAUDE_WORKSPACE` > folder binding (ancestor walk) > config `workspace` > derived repo-root name > `BAUDE_BACKEND` > config `backend` > `claude`.
- **New-session prefill:** Inside a repository (determined via `git::repo_root`), the repository root; outside, `new_session_dir` if set, else launch dir.
- **Repository identity:** Canonical `git rev-parse --git-common-dir`; deduplication via `discover_repository` ensures the same repository row from any subfolder or root launch.
- **Workspace title:** TUI shows a clear title at the top (reuse existing chrome at `ui.rs:223`) naming the active workspace and how it was chosen (explicit, bound, or derived); when no workspace applies (implicit default), show `(blank)` placeholder.
- **Derived binding write:** Recorded as `repo_root -> workspace` in `folder-workspaces.json` using the existing schema, no new fields.
- **Daemon parity:** One shared resolver in `baude-core` (`workspace` and `folder_workspace` modules) implements the walk and derivation; both `baude` and `bauded` call it at startup.
- **Test isolation:** All tests must go through `TestRedirect` and never touch real `~/.config/baude`.
- **Automatic migration:** No migration of existing `claude` workspace sessions; existing bindings route the iarx-com and joese-iarx trees to `claude`; README explains how to rebind.

### Claude's Discretion

- **Shared resolver placement:** Exact module structure (`new` module vs. extending `folder_workspace::plan_launch`) for the ancestor walk and derivation.
- **Write timing:** Whether derived binding is written before or after workspace lock is claimed, as long as lock refusal does not leave half-written binding file.
- **Workspace title wording:** Exact phrasing of "explicit," "bound," or "derived" labels; keep to one short line.

### Deferred Ideas (OUT OF SCOPE)

- Interactive workspace picker on first launch from an unbound folder (WSPC-F1).
- Moving or merging sessions between workspaces (WSPC-F2).
- Managed worktree path identity collision handling (Phase 14).
- Startup performance (Phase 15).
- Pane focus (Phase 16).

## Phase Requirements

| ID | Description | Research Support |
|----|-------------|------------------|
| WSPC-01 | Walk up from launch dir to nearest recorded folder binding and use that workspace | Ancestor walk function in `folder_workspace`; existing `folder_key(launch_dir)` keying; `BTreeMap::get` lookup; walk stops at `$HOME` or filesystem root |
| WSPC-02 | Derive workspace from launch repository root's folder name (sanitized) and record binding for stability | Existing `git::repo_root` to find toplevel; existing `sanitize` function filters to `[A-Za-z0-9_-]`; `folder_workspace::record` writes to `folder-workspaces.json` |
| WSPC-03 | Explicit `BAUDE_WORKSPACE`, config `workspace`, and `folder_context` keep priority over derivation; README documents full precedence and derivation rule | Precedence chain in `workspace::resolve_with_hint` (lines 116-167); `folder_context` kill switch in `folder_workspace::plan_launch` (line 81-85); README sections for Workspaces and Folder context |
| WSPC-04 | `bauded` resolves workspace by same rule for same launch dir via shared resolver in `baude-core` | Single `Workspace` and `FolderWorkspace` module pair in `baude-core/src/` called from both `baude/src/main.rs` (lines 359-370) and `bauded/src/manager.rs` (line 808) |
| WSPC-05 | TUI shows clear title at top naming active workspace and how it was chosen (explicit, bound, or derived); `(blank)` placeholder when no workspace applies | Reuse `ui.rs:223` outer block title; add source/hint display via new `workspace::display_hint()` method; existing `workspace::active().display_label()` call at `ui.rs:1283` shows in info overlay |
| OPEN-01 | `n` new-session prompt prefills git toplevel of launch dir when inside a repository | Existing `git::repo_root` at `app.rs:2815-2817`; update prompt logic at `app.rs:4058-4065` to call `git::repo_root(&self.launch_dir)` first |
| OPEN-02 | Config `new_session_dir` used as prefill only when launch dir is outside any repository | Condition in `n` prompt: try `git::repo_root(&self.launch_dir)`, fallback to config `new_session_dir` else `self.launch_dir` |
| OPEN-03 | Launching from any subfolder admits and focuses same repository row as launching from root; no duplicates | Existing `git::discover_repository` at `app.rs:4723-4777` uses canonical common dir; deduplication via exact repository identity comparison in `admit_repository_*` logic |
| OPEN-04 | Clone-on-demand keeps `clone_base_dir` semantics; only directory-open paths change; README documents new defaults | No change to clone logic; `n` prompt logic changed only; README updates for new-session defaults and open paths |

## Architectural Responsibility Map

| Capability | Primary Tier | Secondary Tier | Rationale |
|------------|-------------|----------------|-----------|
| Workspace resolution and precedence | Backend (baude-core) | Both binaries (baude, bauded) | Single source of truth for name selection and backend binding |
| Folder memory (recording and lookup) | Backend (baude-core) | Both binaries | Advisory state, shared file, read-merge-write concurrency |
| Repository discovery and identity | Backend (baude-core) | App tier (for local admission) | Canonical git operations; deduplication on common dir |
| Launch directory canonicalization | App startup (main.rs) | — | One-time per process; feeds into all downstream lookups |
| New-session prefill logic | App tier (app.rs) | — | Modal UI; context-specific path selection |
| Workspace title rendering | Frontend (ui.rs) | — | TUI chrome; shows active workspace and source |
| Daemon parity | Backend (baude-core) | Daemon (bauded) | Same resolver; both startup paths call shared functions |

## Standard Stack

### Core Technologies
| Library | Version | Purpose | Why Standard |
|---------|---------|---------|--------------|
| `tokio` | 1.x | Async runtime | Required for all async work across baude, bauded, baude-core |
| `serde` / `serde_json` | Latest | Serialization | State file (JSON) and breadcrumbs persistence |
| `git2-rs` (wrapped by baude `git` module) | — | Git operations | Repository discovery, worktree inventory, branch resolution |

### Crate Structure
| Crate | Purpose | Stability |
|-------|---------|-----------|
| `baude-core` | Workspace, folder memory, git, config, persistence, lifecycle | Core — all changes live here |
| `baude` | TUI app, user input, modal logic, app startup | Stable — only startup and modal prefill change |
| `bauded` | Daemon, session management | Stable — startup path mirrors baude |

### Supporting Patterns
| Pattern | Where | Purpose |
|---------|-------|---------|
| `TestRedirect` | `baude-core/src/testing.rs` | Fixture isolation; all tests route through this |
| `folder_key(path)` | `baude-core/src/breadcrumbs.rs` | Canonical launch-dir keying; used in folder_workspace and breadcrumbs |
| `load_json` / `locked_merge_write` | `baude-core/src/breadcrumbs.rs` | Safe file I/O with advisory locking |

## Architecture Patterns

### System Architecture Diagram

```
Launch directory (canonicalized at main.rs:343-347)
         |
         v
  [folder_workspace::plan_launch]
         |
         +-- Check BAUDE_WORKSPACE/BAUDE_BACKEND env (short-circuit)
         |
         +-- Ancestor walk from launch_dir to $HOME
         |   (new: find_binding function)
         |
         v
    Hint returned (bound workspace name or None)
         |
         v
  [workspace::resolve_with_hint]
         |
         +-- Precedence: ws_env > hint > config > backend_env > config backend > default
         |
         v
    Active workspace chosen + backend binding
         |
         +-- (For app) new-session prefill uses git::repo_root
         |
         +-- (For TUI title) display_hint() added to show source
         |
         v
    Application starts with correct workspace and initial state
```

**Data flow:**
1. **Startup:** `main.rs:343-347` canonicalizes launch dir → `folder_workspace::plan_launch` reads folder memory → `workspace::initialize` resolves → lock claimed → `folder_workspace::record` records binding if derived
2. **Admission:** User types `o path` or `n` → `open_repo_session_via` calls `git::discover_repository` → deduplication via common dir → same repo row from any subfolder
3. **TUI:** `ui.rs:223` uses `workspace::active().display_label()` + new `display_hint()` → title shows workspace name and source

### Recommended Project Structure

```
baude-core/src/
├── workspace.rs           # Resolution chain, sanitize, display_label, NEW: display_hint()
├── folder_workspace.rs    # Memory file I/O, NEW: find_binding() ancestor walk
├── git.rs                 # repo_root, discover_repository (unchanged)
├── persist.rs             # Config and state file management (unchanged)
├── testing.rs             # TestRedirect fixture helper (unchanged)
└── ...

baude/src/
├── main.rs                # Launch dir canonicalization, plan_launch, initialize (unchanged)
├── app.rs                 # NEW: n prompt prefill uses git::repo_root before new_session_dir
└── ui.rs                  # NEW: title at line 223 includes workspace name and display_hint()

bauded/src/
├── manager.rs             # Daemon startup (unchanged, calls same resolver)
└── ...
```

### Pattern 1: Workspace Resolution with Precedence Chain

**What:** Layered fallback from explicit env > folder memory hint > config > backend env > config backend > default (`claude`); each level sanitizes non-filesystem-safe characters.

**When to use:** Every workspace selection at startup (TUI and daemon), fold new folder-memory bindings into the same precedence before config defaults so they take effect immediately.

**Example:**
```rust
// Source: baude-core/src/workspace.rs:116-167
pub fn resolve_with_hint(
    ws_env: Option<&str>,
    backend_env: Option<&str>,
    hint: Option<&str>,
    config: &Config,
    mut warn: impl FnMut(String),
) -> Workspace {
    // Hint only if no explicit env var.
    let hint = if ws_env.is_some() || backend_env.is_some() {
        None
    } else {
        hint
    };
    // Precedence: explicit > hint > config > backend env > config backend > default.
    let name = sanitize(
        ws_env
            .or(hint)
            .or(config.workspace.as_deref())
            .or(backend_env)
            .or(config.backend.as_deref())
            .unwrap_or(DEFAULT),
    );
    // ... resolve backend binding ...
}
```

### Pattern 2: Ancestor Walk for Folder Memory

**What:** Walk up from the canonical launch directory, checking for a recorded binding at each ancestor, stopping at `$HOME` or filesystem root; return the first match.

**When to use:** After canonicalizing the launch path, before resolving the workspace; only when `folder_context` is enabled and no explicit env var is set.

**Example (to be added in this phase):**
```rust
// Source: baude-core/src/folder_workspace.rs (new function)
pub fn find_binding(
    root: &Path,
    launch_dir: &Path,
    home: &Path,
) -> Option<String> {
    let (file, _corrupted) = load_json::<FolderWorkspaceFile>(&root.join(FILE_NAME));
    let mut current = launch_dir;
    loop {
        // Check current directory.
        if let Some(entry) = file.folders.get(&folder_key(current)) {
            return Some(entry.workspace.clone());
        }
        // Stop at home or root; don't walk above home.
        if current == home || !current.pop() {
            break;
        }
    }
    None
}
```

### Pattern 3: Repository Discovery for Admission

**What:** Call `git::discover_repository(path)` which returns a `RepositorySnapshot` keyed by canonical common dir; deduplication compares common dirs for exact match.

**When to use:** When admitting a folder via `n` (new-session) or `o` (open) command, or at startup via `open_repo_session_via`.

**Example:**
```rust
// Source: baude/src/app.rs:4723-4777 (existing pattern, unchanged)
match git::discover_repository(&path) {
    Ok(_) => {
        // Repository found; use admit_repository which deduplicates via common dir.
        match self.admit_repository(&path) {
            Ok(Some(runtime)) => {
                self.focus = Focus::Claude;
                self.record_context_use_for_runtime(runtime);
            }
            Ok(None) => {}
            Err(e) => self.set_message(format!("repository admission failed: {e}")),
        }
    }
    Err(git::RepositoryDiscoveryError::NotRepository(_)) => {
        // Not a repository; admit as a standalone folder.
        match self.admit_standalone(&path) { /* ... */ }
    }
}
```

### Pattern 4: Git Repository Root Detection

**What:** Call `git::repo_root(path)` which runs `git rev-parse --show-toplevel` and returns the canonicalized root, or `None` if not in a repository.

**When to use:** To determine if the launch directory is inside a repository, and if so, to get the root for new-session prefill and repository admission.

**Example (to be updated in this phase):**
```rust
// Source: baude/src/app.rs (will be updated)
fn open_new_session(&mut self) {
    // NEW: Try git::repo_root first for repo context; fall back to config.
    let buf = match git::repo_root(&self.launch_dir) {
        Some(root) => format!("{}/", root.display()),
        None => match &self.config.new_session_dir {
            Some(d) => {
                let d = d.trim_end_matches('/');
                format!("{d}/")
            }
            None => format!("{}", self.launch_dir.display()),
        },
    };
    self.modal = Modal::Input {
        kind: InputKind::NewSessionPath,
        title: "new session — repo path or github url (tab completes)".into(),
        buf,
        candidates: Vec::new(),
    };
}
```

### Anti-Patterns to Avoid

- **Deriving workspace from current working directory instead of repository root:** The repository root is stable across symlinks and worktrees; the cwd can change without changing the repository identity. Always use the git-canonical common dir for identity.
- **Recording folder memory without reading folder_context kill switch:** The `folder_context` feature gate disables both recording and lookup; attempting to record when the switch is off creates an inconsistent state. Always check `folder_context_enabled()` before any folder_workspace operation.
- **Hardcoding `~/` or `$HOME` in path walks instead of receiving it as a parameter:** Tests redirect HOME; hardcoding it bypasses fixture isolation and causes tests to touch the real home. Always accept a `home_dir` parameter or read via `dirs::home_dir()` (which respects `TestRedirect`).
- **Writing derived workspace name directly to state without sanitization:** Workspace names become state file names and config keys; unsanitized names can create filesystem or JSON issues. Always apply `sanitize` to any user-derivable name.
- **Assuming a derived binding is persisted before the workspace lock is claimed:** Lock refusal leaves the file half-written. Always write the binding AFTER lock is confirmed, or ensure the write is atomic and idempotent.

## Don't Hand-Roll

| Problem | Don't Build | Use Instead | Why |
|---------|-------------|-------------|-----|
| Workspace name selection and precedence | Custom fallback chain | `workspace::resolve` / `resolve_with_hint` | Handles sanitization, backend binding conflicts, legacy fallbacks, and warning messages; 9 test cases validate precedence |
| Folder memory lookup and write | Custom JSON merge | `folder_workspace::plan_launch` + `record` | Implements read-merge-write under advisory lock for concurrent TUI launches from different folders; defers to `breadcrumbs` lock helper which handles POSIX advisory locks |
| Repository identity | Custom path-based dedup | `git::discover_repository` + common-dir comparison | Canonical git operation; handles symlinks, linked worktrees, and the subtle canonicalization-after-discovery requirement; 4 test cases validate behavior |
| Path canonicalization | Manual symlink resolution | `path.canonicalize()` + `$HOME` env / `dirs::home_dir()` | Handles OS differences, filesystem quirks, and TestRedirect injection; already used consistently at startup |
| Sanitizing workspace/session names | Regex or char filters | `workspace::sanitize` | Already implements the exact policy (`[A-Za-z0-9_-]` only); used for workspace, session, and config key names |

**Key insight:** The resolution and folder-memory layers are thin abstraction over precedence and file I/O, not custom application logic. Reusing them ensures one source of truth for workspace identity across startup, restoration, and runtime name resolution.

## Common Pitfalls

### Pitfall 1: Forgetting to Canonicalize Paths Before Lookups

**What goes wrong:** Folder memory lookup fails silently because the key doesn't match; a folder opened as `~/Code/repo` is different from `/home/user/Code/repo` even though they're the same path.

**Why it happens:** Path normalization is tedious and easy to skip; the error is subtle because it falls back gracefully to no-memory mode instead of erroring.

**How to avoid:** Always canonicalize the launch dir immediately at startup (`main.rs:343-347` already does this); pass the canonical path to `folder_workspace::plan_launch` and `git::discover_repository`. Document this contract.

**Warning signs:** User reports that folder memory is never recalled even after repeated launches; a second launch from the same folder should show the same workspace and sessions.

### Pitfall 2: Ancestor Walk Terminating at the Wrong Boundary

**What goes wrong:** Walk continues above `$HOME` (e.g., if `$HOME` is not set or is outside the launch dir path), or stops too early, missing a binding on an ancestor.

**Why it happens:** The boundary condition (== home vs. >=, or not popping up correctly) is easy to get wrong.

**How to avoid:** Walk upward with `current.pop()` in a loop, checking `current == home` before each pop; if `$HOME` is not set, use the filesystem root `/`. Test with launch dirs both inside and outside home (phase tests will set up fixtures with custom `$HOME`).

**Warning signs:** Bindings recorded in a parent folder are never found when launching from a subdirectory; walk terminates early and returns no binding when one exists.

### Pitfall 3: Writing Derived Binding Before Lock is Claimed

**What goes wrong:** Process crashes or is killed between `record` and lock claim; the binding file is half-written or corrupted, and the next launch degrades to no-memory mode.

**Why it happens:** The order seems obvious (write first, then lock), but it's backward for durability.

**How to avoid:** Claim the workspace lock FIRST (in `main.rs:359-370`, already done), THEN call `folder_workspace::record` to update the binding. If lock refusal happens, don't write. The lock write-guards the state file; the binding is advisory metadata that can be regenerated, so it's safe to write after lock claim.

**Warning signs:** The binding file is corrupted occasionally; a `git status` check shows the file has size zero or truncated JSON.

### Pitfall 4: Not Respecting the folder_context Kill Switch

**What goes wrong:** Folder memory is read and recorded even when `folder_context: false` is set in config, creating inconsistent behavior.

**Why it happens:** The kill switch is checked in `plan_launch` but a later `record` call doesn't re-check it.

**How to avoid:** Pass the `enabled` flag from `config.folder_context_enabled()` through to both `plan_launch` AND any subsequent `record` call. Don't record a binding if it was not consulted on launch.

**Warning signs:** User disables folder context but stale bindings still affect workspace selection; toggling the switch in config doesn't take effect until the binding file is manually deleted.

### Pitfall 5: Display Hint String Shows No Source

**What goes wrong:** TUI title shows the workspace name but not whether it was explicit, bound, or derived; user can't tell at a glance how the workspace was chosen.

**Why it happens:** The workspace object knows the name but not its origin; adding origin tracking requires threading a new field through resolution.

**How to avoid:** Add a `source: WorkspaceSource` enum (or similar) to the `Workspace` struct that tracks how it was chosen. Implement `display_hint()` method that returns a short string like `(explicit)`, `(folder binding)`, `(derived from repo)`, or `(default)`. Update the title at `ui.rs:223` to include it. Test that all four sources are rendered correctly.

**Warning signs:** User opens multiple folders and has no way to know which workspace they're in without checking config; duplicate workspace names from different sources are confusing.

## Code Examples

Verified patterns and call sites from the codebase:

### Example 1: Launch Directory Canonicalization

```rust
// Source: baude/src/main.rs:343-347
let launch_dir = std::env::args()
    .nth(1)
    .map(std::path::PathBuf::from)
    .unwrap_or(std::env::current_dir()?);
let launch_dir = launch_dir.canonicalize().unwrap_or(launch_dir);
```

**Usage:** Set once at startup, passed to all downstream lookups (folder_workspace, git, app initialization).

### Example 2: Workspace Resolution Precedence

```rust
// Source: baude-core/src/workspace.rs:132-151 (resolve_with_hint)
// Hint only if no explicit env var — either env var is an explicit per-invocation choice.
let hint = if ws_env.is_some() || backend_env.is_some() {
    None
} else {
    hint
};
// Precedence: explicit env > hint > config > backend env > config backend > default.
let name = sanitize(
    ws_env
        .or(hint)
        .or(config.workspace.as_deref())
        .or(backend_env)
        .or(config.backend.as_deref())
        .unwrap_or(DEFAULT),
);
```

### Example 3: Folder Memory Lookup

```rust
// Source: baude-core/src/folder_workspace.rs:80-88
let (file, corrupted) = load_json::<FolderWorkspaceFile>(&root.join(FILE_NAME));
let mut plan = LaunchPlan {
    hint: file
        .folders
        .get(&folder_key(launch_dir))
        .map(|entry| entry.workspace.clone()),
    notes: Vec::new(),
};
if corrupted {
    plan.notes.push(
        "folder workspace memory was unreadable — using the configured workspace".to_string(),
    );
}
plan
```

### Example 4: Repository Discovery for Admission

```rust
// Source: baude/src/app.rs:4723-4777 (open_repo_session_via)
match git::discover_repository(&path) {
    Ok(_) => {
        debug_assert!(local_admission_route(route, false));
        match self.admit_repository(&path) {
            Ok(Some(runtime)) => {
                self.focus = Focus::Claude;
                self.record_context_use_for_runtime(runtime);
            }
            Ok(None) => {}
            Err(e) => self.set_message(format!("repository admission failed: {e}")),
        }
    }
    Err(git::RepositoryDiscoveryError::NotRepository(_)) => {
        // Admit as standalone folder.
        match self.admit_standalone(&path) { /* ... */ }
    }
    Err(error) => {
        self.set_message(format!("repository discovery failed: {error}"));
    }
}
```

## Validation Architecture

**Framework:** Rust built-in `#[test]` / `#[cfg(test)]`; existing test infrastructure in `baude-core/src/`, `baude/src/`, `bauded/src/`.

### Test Framework
| Property | Value |
|----------|-------|
| Framework | Rust `#[test]` (no external test runner) |
| Config file | None — tests compile as library features |
| Quick run command | `cargo test -p baude-core workspace:: --lib` (workspace + folder_workspace tests) |
| Full suite command | `cargo test --all --lib` (all unit tests) |

### Phase Requirements → Test Map

| Req ID | Behavior | Test Type | Automated Command | Existing File |
|--------|----------|-----------|-------------------|------------------|
| WSPC-01 | Ancestor walk finds nearest binding up the directory tree | Unit | `cargo test -p baude-core folder_workspace::tests::` | ✅ baude-core/src/folder_workspace.rs:148-214 |
| WSPC-02 | Derived workspace is sanitized and recorded in folder-workspaces.json | Unit | `cargo test -p baude-core folder_workspace::tests::records_and_recalls_one_folder_preserving_others` | ✅ baude-core/src/folder_workspace.rs:148-160 |
| WSPC-03 | Explicit env / config workspace win over derived; folder_context kill switch gates both | Unit | `cargo test -p baude-core folder_workspace::tests::unknown_folder_disabled_switch_or_env_never_consult_memory` | ✅ baude-core/src/folder_workspace.rs:161-195 |
| WSPC-04 | Daemon calls same resolver as app; workspace is identical for same launch dir | Unit | NEW — bauded manager startup test (mirrors app init) | ❌ Wave 0: New bauded-specific test in bauded/src/manager.rs tests |
| WSPC-05 | TUI title includes workspace name and display hint (explicit/bound/derived) | Unit | NEW — ui rendering test with mocked workspace | ❌ Wave 0: New ui test in baude/src/ui.rs tests checking title construction |
| OPEN-01 | `n` prompt prefill uses git::repo_root when inside a repository | Integration | NEW — app modal test launching `n` from repo subdirectory | ❌ Wave 0: New test in baude/src/app.rs::tests checking prefill value |
| OPEN-02 | `n` prompt uses new_session_dir only when outside a repository | Unit | NEW — app modal test launching `n` from non-repo folder | ❌ Wave 0: Paired test with OPEN-01 |
| OPEN-03 | Launching from subfolder and root of same repo admits and focuses same row (no duplication) | Integration | NEW — app admission test with repo subdirectory launches | ❌ Wave 0: New test in baude/src/app.rs::tests::admit_repository_* checking dedupplication by common dir |
| OPEN-04 | Clone-on-demand destination unchanged; only directory-open paths affected | Unit | NEW — verify clone_base_dir config is unmodified by new-session changes | ❌ Wave 0: Code audit + comment in app.rs where clone logic is untouched |

### Sampling Rate

- **Per task commit:** `cargo test -p baude-core workspace:: folder_workspace:: --lib` (workspace + folder memory units, ~30s)
- **Per wave merge:** `cargo test --all --lib` + `cargo test --all --doc` (all unit + doc tests, ~2min)
- **Phase gate:** Full suite green before `/gsd-verify-work`; no external test infrastructure required

### Wave 0 Gaps

- [ ] `bauded/src/manager.rs::tests` — New test for daemon workspace resolution parity with app (mirrors app init at line 359-370 with same launch dir) (WSPC-04)
- [ ] `baude/src/ui.rs::tests` — New test mocking `workspace::active()` to return explicit/bound/derived variants and checking title includes display hint string (WSPC-05)
- [ ] `baude/src/app.rs::tests` — New test for `open_new_session` modal prefill: (a) inside repo shows git root, (b) outside repo shows config new_session_dir or launch_dir (OPEN-01, OPEN-02)
- [ ] `baude/src/app.rs::tests::admit_repository_*` — New subfolder-admission test verifying same repository row from root and subfolder launch; check that `git::discover_repository` deduplication by common dir prevents duplicate rows (OPEN-03)
- [ ] `baude-core/src/folder_workspace.rs::tests` — New test for ancestor walk: (a) binding on parent found from child, (b) binding on grandparent found when parent has no binding, (c) walk stops at $HOME (NEW function `find_binding` needs coverage)

**Test Fixtures & Isolation:**

All tests use `TestRedirect` (baude-core/src/testing.rs:107) which redirects:
- `worktrees_base` → `<root>/data`
- `config_dir` → `<root>/config`
- `claude_config_dir` → `<root>/claude`
- `home_dir` → `<root>/home`

Existing test suites already pass `Some(&fixture_root)` to `plan_launch` and `record`; new tests follow the same pattern.

**Containment Strategy:**

- Unit tests for `workspace::resolve*`, `folder_workspace::plan_launch`, `find_binding` (new) use in-memory mode (`root: None`) or `TestRedirect`.
- Integration tests for app admission and daemon parity use `TestRedirect` fixture with real git repos initialized inside.
- No test may call the real `folder_workspace` file path without going through a fixture-redirected config dir.
- Existing suite guard (`scripts/assert-real-roots-untouched.sh`) continues to verify real `~/.config/baude` is never written.

## Security Domain

### Applicable ASVS Categories

| ASVS Category | Applies | Standard Control |
|---------------|---------|------------------|
| V2 Authentication | No | (Not in scope; workspace selection is user-initiated) |
| V3 Session Management | No | (Workspace selection does not change session lifecycle) |
| V4 Access Control | No | (Workspace is user-local metadata; no multi-user context) |
| V5 Input Validation | Yes | Workspace name sanitization via `sanitize()` function; path canonicalization |
| V6 Cryptography | No | (No sensitive data encryption; workspace is plain metadata) |

### Input Validation & Sanitization

- **Repository root folder name:** Extracted via `git rev-parse --show-toplevel`, then `Path::file_name()`, then sanitized through `workspace::sanitize` which replaces non-`[A-Za-z0-9_-]` with `-`. This prevents injection into state file paths or config keys.
- **Launch directory:** Canonicalized at startup via `std::path::PathBuf::canonicalize()`, which resolves symlinks and normalizes path separators. Used as-is for folder memory lookups and git operations.
- **Config workspace name:** Loaded from `config.json` via serde with `deny_unknown_fields: false` (additive evolution); value is sanitized before use in `resolve_with_hint`.
- **Environment variables (`BAUDE_WORKSPACE`, `BAUDE_BACKEND`):** Read directly via `std::env::var`; values are sanitized before use.

### Known Threat Patterns

| Pattern | Mitigation | Implemented |
|---------|-----------|------------|
| Injected workspace name in state file path | Sanitize before use in filename; use serde for serialization (never string concat) | ✅ `workspace::sanitize` + `state_file()` method |
| Symlink traversal in folder memory walk | Canonicalize launch dir; walk only checks exact BTreeMap keys (no glob/wildcard) | ✅ Canonicalization at startup + exact match lookup |
| Unauthorized access to folder-workspaces.json | File lives in config dir (same permissions as breadcrumbs); advisory lock prevents concurrent writes | ✅ Existing `breadcrumbs::locked_merge_write` helper |
| Corrupted state from concurrent writes | Advisory POSIX lock on the file before merge-write; failures degrade to "no memory" instead of error | ✅ Existing `breadcrumbs` lock pattern |

## Sources

### Primary (HIGH confidence)

- **baude-core/src/workspace.rs:99-167** — Sanitization, resolution chain, precedence logic [VERIFIED: src code read]
- **baude-core/src/folder_workspace.rs:29-98** — Folder memory schema, plan_launch, record functions [VERIFIED: src code read]
- **baude/src/main.rs:343-370** — Launch dir canonicalization, initialization, lock claim [VERIFIED: src code read]
- **baude/src/app.rs:4042-4080, 4723-4777, 1321-1340** — n prompt prefill, admission route, startup auto-admission [VERIFIED: src code read]
- **baude/src/ui.rs:220-230, 1280-1290** — Title construction, display_label call [VERIFIED: src code read]
- **baude-core/src/git.rs:322-365, 1734-1740** — discover_repository, repo_root functions [VERIFIED: src code read]
- **baude-core/src/testing.rs:107-140** — TestRedirect fixture design [VERIFIED: src code read]
- **CONTEXT.md:Phase 13** — Locked implementation decisions from discuss phase [CITED: upstream requirements doc]
- **REQUIREMENTS.md:WSPC-01 to OPEN-04** — Phase requirements with success criteria [CITED: upstream requirements]

### Secondary (MEDIUM confidence)

- **README.md: Workspaces, Folder context sections** — Current documentation of workspace selection and folder memory [CITED: existing docs to be updated]
- **baude-core/src/folder_workspace.rs:148-214** — Existing folder_workspace test suite (patterns for new tests) [VERIFIED: test code]
- **baude-core/src/workspace.rs:656-810** — Existing workspace resolution test suite [VERIFIED: test code]
- **baude/src/app.rs:8348-8482** — Existing app admission tests [VERIFIED: test code]

## Assumptions Log

| # | Claim | Section | Risk if Wrong |
|---|-------|---------|---------------|
| A1 | `git::repo_root` returns the git toplevel for linked worktrees (not the worktree root) | Architecture Patterns, Example 4 | If assumption wrong, `n` prefill would show worktree root instead of true repo root; requires verification against git documentation or test with actual linked worktree |
| A2 | `git rev-parse --show-toplevel` inside a git worktree returns the worktree root, not the main repo root | Code Examples, Example 4 | If assumption wrong, new-session prefill behavior would be different; needs verification by running git command in a test worktree |
| A3 | `TestRedirect` properly redirects `$HOME` for all path-resolution code paths in baude and baude-core | Validation Architecture, Test Fixtures section | If assumption wrong, tests might accidentally touch real home dir; should be verified by audit of all `$HOME` usages and test containment checks |
| A4 | Ancestor walk stopping condition `current == home` is sufficient to prevent walking above home without special handling on non-UNIX systems | Common Pitfalls, Pitfall 2 | If assumption wrong on Windows or unusual path structures, walk could escape home or miss bindings; requires testing on multiple platforms or explicit path-boundary logic |
| A5 | `folder_context_enabled()` is the sole kill switch for both reading and recording folder memory (no other configuration disables it) | Common Pitfalls, Pitfall 4 | If assumption wrong, inconsistent behavior could occur when the switch is toggled; needs verification that both plan_launch and record respect the same flag |

**All HIGH-confidence claims verified by reading source code this session. MEDIUM-confidence claims from existing documentation or test patterns.**

## Open Questions (RESOLVED)

1. **Workspace display hint exact wording:**
   - What we know: Requirement is to show how workspace was chosen: explicit, bound, or derived; placeholder `(blank)` when implicit default applies.
   - **RESOLVED:** 13-01-PLAN.md specifies the `display_hint()` method returns one of four strings: `(explicit)`, `(folder binding)`, `(derived)`, or `(default)`. UI integration at `ui.rs:223` includes the hint via `workspace::active().display_hint()` in the title. Exact wording per plan 01 action.

2. **Ancestor walk home detection on non-UNIX systems:**
   - What we know: Phase decision says walk to `$HOME` or filesystem root; tests will use `TestRedirect` which provides a fixture home.
   - **RESOLVED:** 13-02-PLAN.md TDD task 1 includes explicit test coverage: `test_find_binding_stops_at_home` verifies walk does not continue above home; edge case test handles launch_dir outside home (filesystem root stop). Implementation in 13-01 uses `current.pop()` loop with `current == home` boundary check, standard Rust `Path` methods. Tests verify behavior; platform-specific corner cases (Windows) would be caught by CI test matrix.

3. **Recording binding write timing relative to lock claim:**
   - What we know: Locked decision says "as long as lock refusal does not leave half-written binding file."
   - **RESOLVED:** 13-01-PLAN.md key_links section (line 119-120) specifies: "Recording (lines 399-411) is unchanged; it writes the hint to folder-workspaces.json after lock claim, making the binding durable for future launches." Binding write happens AFTER workspace lock is claimed per main.rs:399-411 flow.

## Environment Availability

No external environment dependencies for Phase 13. All tools are standard Rust ecosystem:

- `cargo` — building and testing (already present in CI, verified to work with Phase 12)
- `git` — repository operations (already a hard dependency of baude; phase uses existing `git::` module)
- No new package dependencies required

**Existing dependencies:**
- `tokio`, `serde`, `serde_json` — already in Cargo.lock
- `git2-rs` — already vendored or locked in Cargo.toml

No CI configuration changes needed. Phase integrates with existing test infrastructure.

## Metadata

**Confidence breakdown:**
- Standard stack: HIGH — Single-crate Rust project, no new external packages.
- Architecture: HIGH — Codebase facts verified by reading source; patterns are extensions of existing code.
- Pitfalls: MEDIUM — Based on common filesystem and config management issues; some mitigation strategies are aspirational and tested in Wave 0.
- Test infrastructure: HIGH — Existing `TestRedirect`, test suite patterns, and CI are stable.

**Research date:** 2026-09-19
**Valid until:** 2026-10-19 (30 days; stable codebase, low risk of changes outside phase scope)
