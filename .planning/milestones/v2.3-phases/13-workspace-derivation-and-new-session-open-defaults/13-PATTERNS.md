# Phase 13: Workspace Derivation and New-Session/Open Defaults - Pattern Map

**Mapped:** 2026-09-19
**Files analyzed:** 8 (6 modified, 1 documentation, 1 test infrastructure)
**Analogs found:** 6 / 6 (100% match — all changes are localized extensions of existing code)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude-core/src/workspace.rs` | utility/service | configuration resolution | itself (existing resolution chain) | exact |
| `baude-core/src/folder_workspace.rs` | service/model | file I/O, JSON persistence | itself (existing folder memory) | exact |
| `baude/src/main.rs` | startup/initialization | initialization chain | itself (lines 343-412) | exact |
| `baude/src/app.rs` | controller/app logic | modal input handling | itself (lines 4057-4071) | exact |
| `baude/src/ui.rs` | view/UI renderer | rendering | itself (line 223) | exact |
| `bauded/src/main.rs` | daemon initialization | initialization chain | itself (line 182) | exact |
| `README.md` | documentation | static content | existing Workspaces section | exact |
| Test files (workspace.rs, folder_workspace.rs, app.rs, ui.rs) | test infrastructure | unit/integration | existing test patterns | exact |

## Pattern Assignments

### `baude-core/src/workspace.rs` (utility/service, configuration resolution)

**Analog:** `baude-core/src/workspace.rs` itself (existing resolution patterns)

**Current struct definition** (lines 46-54):
```rust
pub struct Workspace {
    pub name: String,
    pub backend: &'static dyn Backend,
    pub daemon_url: Option<String>,
    pub daemon_port: Option<u16>,
}
```

**Pattern: Sanitization function** (lines 99-111):
```rust
/// Keep workspace names filesystem- and URL-safe: anything outside
/// `[A-Za-z0-9_-]` becomes `-`.
fn sanitize(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '-'
            }
        })
        .collect()
}
```

**Pattern: Existing display_label method** (lines 78-84) — reuse this pattern for new `display_hint` method:
```rust
pub fn display_label(&self) -> String {
    if self.name == self.backend.name() {
        self.backend.display_name().to_string()
    } else {
        format!("{} · {}", self.name, self.backend.display_name())
    }
}
```

**Pattern: Resolution precedence chain** (lines 132-197, `resolve_with_hint`):
```rust
pub fn resolve_with_hint(
    ws_env: Option<&str>,
    backend_env: Option<&str>,
    hint: Option<&str>,
    config: &Config,
    mut warn: impl FnMut(String),
) -> Workspace {
    // Hint only if no explicit env var — either env var is an explicit choice.
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
    // ... backend binding logic ...
}
```

**NEW: Add `display_hint()` method** to track derivation source (explicit/bound/derived/default). Pattern follows `display_label` structure; return string like `"(explicit)"`, `"(folder binding)"`, `"(derived)"`, or `"(default)"`.

---

### `baude-core/src/folder_workspace.rs` (service/model, file I/O + JSON persistence)

**Analog:** `baude-core/src/folder_workspace.rs` itself (existing folder memory patterns)

**Pattern: Folder memory schema and file I/O** (lines 44-57):
```rust
#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct FolderWorkspaceFile {
    #[serde(default = "schema_version_default")]
    pub schema_version: u32,
    /// Keyed by the canonical launch directory (lossy UTF-8).
    #[serde(default)]
    pub folders: BTreeMap<String, FolderWorkspaceEntry>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct FolderWorkspaceEntry {
    pub workspace: String,
    #[serde(default)]
    pub updated_ms: u64,
}
```

**Pattern: LaunchPlan returned by plan_launch** (lines 62-65):
```rust
#[derive(Debug, Default, PartialEq)]
pub struct LaunchPlan {
    pub hint: Option<String>,
    pub notes: Vec<String>,
}
```

**Pattern: Folder memory lookup** (lines 71-98, `plan_launch`):
```rust
pub fn plan_launch(
    enabled: bool,
    ws_env: Option<&str>,
    backend_env: Option<&str>,
    root: Option<&Path>,
    launch_dir: &Path,
) -> LaunchPlan {
    if !enabled || ws_env.is_some() || backend_env.is_some() {
        return LaunchPlan::default();
    }
    let Some(root) = root else {
        return LaunchPlan::default();
    };
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
}
```

**Pattern: Recording folder memory with advisory lock** (lines 118-129, `record`):
```rust
pub fn record(root: Option<&Path>, launch_dir: &Path, workspace: &str, now_ms: u64) {
    let Some(root) = root else { return };
    let key = folder_key(launch_dir);
    let entry = FolderWorkspaceEntry {
        workspace: workspace.to_string(),
        updated_ms: now_ms,
    };
    let _ = locked_merge_write(root, FILE_NAME, |merged: &mut FolderWorkspaceFile| {
        merged.schema_version = SCHEMA_VERSION;
        merged.folders.insert(key, entry);
    });
}
```

**NEW: Add `find_binding()` function** for ancestor walk. Signature follows git path patterns:
```rust
/// Walk up from launch_dir to home, finding the first recorded folder binding.
pub fn find_binding(
    root: &Path,
    launch_dir: &Path,
    home: &Path,
) -> Option<String> {
    // Load folder-workspaces.json
    let (file, _corrupted) = load_json::<FolderWorkspaceFile>(&root.join(FILE_NAME));
    let mut current = launch_dir.to_path_buf();
    loop {
        // Check current directory for binding
        if let Some(entry) = file.folders.get(&folder_key(&current)) {
            return Some(entry.workspace.clone());
        }
        // Stop at home or filesystem root
        if current == home || !current.pop() {
            break;
        }
    }
    None
}
```

**Helpers to reuse:**
- `folder_key(path)` (imported from `breadcrumbs`) — canonicalize launch dir for consistent lookup key
- `load_json()` (imported from `breadcrumbs`) — parse folder-workspaces.json with corruption detection
- `locked_merge_write()` (imported from `breadcrumbs`) — advisory lock + merge-write pattern for concurrent safety

---

### `baude/src/main.rs` (startup/initialization, initialization chain)

**Analog:** `baude/src/main.rs` itself, lines 343-412 (launch dir canonicalization through workspace recording)

**Pattern: Launch directory canonicalization** (lines 343-347):
```rust
let launch_dir = std::env::args()
    .nth(1)
    .map(std::path::PathBuf::from)
    .unwrap_or(std::env::current_dir()?);
let launch_dir = launch_dir.canonicalize().unwrap_or(launch_dir);
```

**Pattern: Folder memory lookup with plan_launch** (lines 358-367):
```rust
let ws_env = std::env::var("BAUDE_WORKSPACE").ok();
let backend_env = std::env::var("BAUDE_BACKEND").ok();
let memory_root = baude_core::persist::config_dir();
let plan = baude_core::folder_workspace::plan_launch(
    config.folder_context_enabled(),
    ws_env.as_deref(),
    backend_env.as_deref(),
    Some(&memory_root),
    &launch_dir,
);
```

**Pattern: Workspace initialization with hint** (line 368):
```rust
let workspace = baude_core::workspace::initialize(&config, plan.hint.as_deref());
```

**Pattern: State lock claim BEFORE recording** (lines 377-396):
```rust
if let Err(baude_core::persist::StateLockError::Held { path, holder }) =
    baude_core::persist::claim_workspace_state_lock("state", workspace)
{
    // ... error handling ...
    std::process::exit(1);
}
```

**Pattern: Recording after lock is claimed** (lines 399-411):
```rust
let mut startup_notes = plan.notes;
if config.folder_context_enabled() {
    startup_notes.extend(baude_core::folder_workspace::applied_note(
        &workspace.name,
        ws_env.as_deref(),
        backend_env.as_deref(),
        &config,
    ));
    baude_core::folder_workspace::record(
        Some(&memory_root),
        &launch_dir,
        &workspace.name,
        baude_core::pty::now_ms(),
    );
}
```

**INTEGRATION POINT:** All this code exists; Phase 13 extends `plan_launch` to call the new `find_binding()` function (ancestor walk) before returning the hint, so the walk is transparently integrated into the existing startup flow.

---

### `baude/src/app.rs` (controller/app logic, modal input handling)

**Analog:** `baude/src/app.rs` itself, lines 4057-4071 (`open_new_session_modal`)

**Current pattern** (lines 4057-4071):
```rust
fn open_new_session_modal(&mut self) {
    let buf = match &self.config.new_session_dir {
        Some(d) => {
            let d = d.trim_end_matches('/');
            format!("{d}/")
        }
        None => format!("{}", self.launch_dir.display()),
    };
    self.modal = Modal::Input {
        kind: InputKind::NewSessionPath,
        title: "new session — repo path or github url (tab completes)".into(),
        buf,
        candidates: Vec::new(),
    };
}
```

**NEW: Update prefill logic to try git::repo_root first** — pattern follows the existing fallback structure:
```rust
fn open_new_session_modal(&mut self) {
    let buf = match git::repo_root(&self.launch_dir) {
        Some(root) => format!("{}/", root.display()),
        None => {
            match &self.config.new_session_dir {
                Some(d) => {
                    let d = d.trim_end_matches('/');
                    format!("{d}/")
                }
                None => format!("{}", self.launch_dir.display()),
            }
        }
    };
    self.modal = Modal::Input {
        kind: InputKind::NewSessionPath,
        title: "new session — repo path or github url (tab completes)".into(),
        buf,
        candidates: Vec::new(),
    };
}
```

**Existing repository discovery pattern** (for reference, used in admission logic, lines 4723-4777):
```rust
match git::discover_repository(&path) {
    Ok(_) => {
        match self.admit_repository(&path) {
            Ok(Some(runtime)) => {
                self.focus = Focus::Claude;
                self.record_context_use_for_runtime(runtime);
            }
            // ...
        }
    }
    Err(git::RepositoryDiscoveryError::NotRepository(_)) => {
        match self.admit_standalone(&path) { /* ... */ }
    }
}
```

**Helper to reuse:**
- `git::repo_root(path: &Path) -> Option<PathBuf>` — returns git toplevel or None if not in a repo (existing function at git.rs:1734)

---

### `baude/src/ui.rs` (view/UI renderer, rendering)

**Analog:** `baude/src/ui.rs` itself, line 223 (title rendering)

**Current title pattern** (line 223):
```rust
.title(concat!(" baude v", env!("CARGO_PKG_VERSION"), " "));
```

**Existing display_label usage pattern** (reference, line 1283 in overlay):
```rust
workspace::active().display_label()
```

**NEW: Update title to include workspace and display hint** (line 223 changed to):
```rust
.title(format!(
    " baude v{} — {} {} ",
    env!("CARGO_PKG_VERSION"),
    workspace::active().display_label(),
    workspace::active().display_hint()  // NEW method returning e.g. "(derived)"
))
```

**Note:** Exact wording of display hint and placement are Claude's discretion (per CONTEXT.md); update title construction to interpolate both label and new display_hint() method from workspace.

---

### `bauded/src/main.rs` (daemon initialization, initialization chain)

**Analog:** `bauded/src/main.rs` itself, line 182 (workspace initialization)

**Current daemon workspace initialization** (line 182):
```rust
let _ = baude_core::workspace::initialize(&config, None);
```

**Pattern note:** Daemon currently does NOT consult folder-context memory (hint is always `None`). Phase 13 decision leaves this as Claude's discretion (line 49 in CONTEXT.md: "Exact placement of the shared resolver..."). For daemon parity with TUI:
- Optionally extend daemon startup to also read folder memory via `plan_launch()` and pass hint to `initialize()`, same as TUI startup (lines 359-368 of baude/src/main.rs)
- Or leave daemon at default workspace (daemon receives explicit `BAUDE_WORKSPACE` via env var when spawned by TUI at line 72 of baude/src/main.rs: `.env("BAUDE_WORKSPACE", &ws.name)`)

**If folder-context added to daemon** (optional), follow same pattern as TUI:
```rust
let ws_env = std::env::var("BAUDE_WORKSPACE").ok();
let backend_env = std::env::var("BAUDE_BACKEND").ok();
let memory_root = baude_core::persist::config_dir();
let plan = baude_core::folder_workspace::plan_launch(
    config.folder_context_enabled(),
    ws_env.as_deref(),
    backend_env.as_deref(),
    Some(&memory_root),
    &launch_dir,  // daemon launch dir (cwd or arg, canonicalized)
);
let workspace = baude_core::workspace::initialize(&config, plan.hint.as_deref());
```

---

## Shared Patterns

### Workspace Resolution Precedence
**Source:** `baude-core/src/workspace.rs:132-197` (`resolve_with_hint`)
**Apply to:** All workspace selection at startup (TUI main.rs:368, daemon main.rs:182)

The precedence chain is:
1. `BAUDE_WORKSPACE` environment variable (explicit per-invocation choice)
2. Folder-memory hint from ancestor walk (NEW: find_binding result)
3. Config `workspace` key
4. `BAUDE_BACKEND` environment variable (resolves to implicit workspace named after backend)
5. Config `backend` key
6. Default `"claude"` workspace

**Implementation:** Already in `workspace::resolve_with_hint()` — pass the hint (from folder_workspace::plan_launch or find_binding result) directly to this function; no changes needed to the function itself, only to callers that gather the hint.

---

### Path Canonicalization at Startup
**Source:** `baude/src/main.rs:343-347`
**Apply to:** All new path-based lookups (folder_workspace, git operations)

```rust
let launch_dir = std::env::args()
    .nth(1)
    .map(std::path::PathBuf::from)
    .unwrap_or(std::env::current_dir()?);
let launch_dir = launch_dir.canonicalize().unwrap_or(launch_dir);
```

Always canonicalize the launch dir at startup, before any folder-memory or git lookups. Passing canonical paths ensures consistent BTreeMap keys and correct deduplication.

---

### Folder Memory Lookup and Recording Under Lock
**Source:** `baude-core/src/folder_workspace.rs:71-129`
**Apply to:** All folder-memory operations

**Lookup pattern** (`plan_launch`):
- Called BEFORE workspace is known
- Returns a LaunchPlan with optional hint and status notes
- Kill switch `folder_context_enabled()` gates both reading and recording
- Explicit env vars bypass memory lookup
- Corruption degrades gracefully ("no memory") with a note

**Recording pattern** (`record`):
- Called AFTER workspace lock is claimed (ensures atomicity)
- Writes via `locked_merge_write()` which holds an advisory lock during read-merge-write
- Best-effort: failures ignored (advisory memory)
- `root: None` (in-memory mode for tests) is a no-op

**Order in main.rs:**
1. Canonicalize launch_dir
2. Call plan_launch to get hint
3. Claim workspace lock (already done at lines 377-396)
4. Record derived binding (already done at lines 406-411)

---

### Repository Root Detection for New-Session Prefill
**Source:** `baude/src/app.rs:4057-4071` + `baude-core/src/git.rs:1734` (repo_root)
**Apply to:** Modal input prefill for `n` (new-session) command

Pattern in `open_new_session_modal`:
```rust
let buf = match git::repo_root(&self.launch_dir) {
    Some(root) => format!("{}/", root.display()),
    None => {
        // fallback to config new_session_dir, then launch_dir
    }
};
```

`git::repo_root()` returns `Some(PathBuf)` if launch_dir is inside a git repository (via `git rev-parse --show-toplevel`), or `None` if not. Fallback respects existing config `new_session_dir` precedence.

---

### TUI Title Rendering with Workspace Context
**Source:** `baude/src/ui.rs:220-225` (block title) + `baude/src/ui.rs:1283` (display_label usage)
**Apply to:** Outer chrome rendering

Update the title string (line 223) to include:
1. Current baude version (already present via `env!("CARGO_PKG_VERSION")`)
2. Active workspace display label (existing `workspace::active().display_label()`)
3. NEW: Workspace source hint via new `workspace::active().display_hint()` method

Example format: `" baude v0.42 — workspace · opencode (derived) "` or `" baude v0.42 — claude (explicit) "`

---

## Analog Summary Table

| Analog File | Purpose | Key Lines | Reuse Strategy |
|-------------|---------|-----------|-----------------|
| `baude-core/src/workspace.rs` | Workspace resolution, sanitization | 99-111 (sanitize), 132-197 (resolve_with_hint), 78-84 (display_label pattern) | Extend Workspace struct with new `display_hint()` method; reuse existing resolve_with_hint |
| `baude-core/src/folder_workspace.rs` | Folder memory I/O | 71-98 (plan_launch), 118-129 (record) | Add new `find_binding()` function; integrate into plan_launch return value |
| `baude/src/main.rs` | TUI startup | 343-347 (canonicalize), 358-368 (plan_launch), 377-412 (lock + record) | Code already integrates folder_workspace; phase extends plan_launch behavior via find_binding |
| `baude/src/app.rs` | Modal prefill | 4057-4071 (open_new_session_modal) | Update prefill fallback to try git::repo_root first |
| `baude/src/ui.rs` | Title rendering | 223 (title), 1283 (display_label usage) | Update title string to include display_hint from active workspace |
| `bauded/src/main.rs` | Daemon startup | 182 (initialize) | Optional: add folder_workspace lookup like TUI, or keep daemon at env-provided workspace |
| `baude-core/src/git.rs` | Git operations | 1734 (repo_root) | Existing function, no changes; used by app.rs prefill logic |

---

## No Analog Found

All files in Phase 13 are extensions or updates to existing files. No new files require patterns from unrelated modules:

| Feature | Status | Reason |
|---------|--------|--------|
| Ancestor walk (find_binding) | NEW in folder_workspace.rs | Extends existing folder memory logic; pattern is standard path-walking |
| Display hint (Workspace method) | NEW in workspace.rs | Extends existing display_label pattern; same struct, new method |
| Workspace title rendering | UPDATES ui.rs line 223 | Uses existing workspace methods and display patterns |
| Repository root detection | UPDATES app.rs lines 4057-4071 | Uses existing git::repo_root; call site changes only |

---

## Test Infrastructure

**Existing test patterns to extend:**

| File | Test Location | Pattern | Phase 13 Addition |
|------|---|---------|----------|
| `baude-core/src/workspace.rs` | lines 656-810 | Resolution chain with multiple sources | Add test for display_hint method covering explicit/bound/derived/default sources |
| `baude-core/src/folder_workspace.rs` | lines 148-214 | Folder memory record/recall under lock | Add test for find_binding ancestor walk (stops at home, finds nearest binding) |
| `baude/src/app.rs` | lines 8348-8482 | App admission and modal logic | Add test for open_new_session_modal prefill (inside repo vs. outside) |
| `baude/src/ui.rs` | (no existing app tests shown) | Rendering mocks | NEW: Test title construction with mocked workspace (display_label + display_hint) |
| `bauded/src/manager.rs` | (manager tests) | Daemon session management | NEW: Test daemon startup workspace resolution parity with TUI (optional) |

**Fixture pattern to reuse:**
- `TestRedirect` (baude-core/src/testing.rs) — all tests route through this for home/config dir isolation
- `scratch()` function (folder_workspace.rs tests, line 137) — creates isolated temp dir per test

---

## Metadata

**Analog search scope:** baude, baude-core, bauded source directories
**Files scanned:** 6 source files identified and read (100% tracked)
**Pattern extraction date:** 2026-09-19
**Confidence:** HIGH — All analogs are existing code in the same codebase; patterns are established and tested.

---

## Critical Integration Notes

1. **Ancestor walk integration:** The new `find_binding()` function should be called from within `plan_launch()` after loading the folder-workspaces.json file but before returning the hint. This makes the walk transparent to callers (main.rs:361-367 needs no changes).

2. **Display hint source tracking:** The `Workspace` struct needs a new field to track the source (explicit/bound/derived/default), populated during `resolve_with_hint()`. The `display_hint()` method returns a short string based on this field. This enables the TUI title update and helps users understand how their workspace was selected.

3. **Writing order:** Lock claim must happen BEFORE `folder_workspace::record()` to ensure atomicity. Existing code at main.rs:377-411 already does this correctly; Phase 13 just extends the logic transparently.

4. **Test isolation:** All new tests must use `TestRedirect` to avoid touching the real `~/.config/baude`. Existing test patterns in workspace.rs (lines 656-810) and folder_workspace.rs (lines 148-214) show how to set up fixtures properly.
