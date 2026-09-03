# Phase 5: Durable Repository Admission - Pattern Map

**Mapped:** 2026-08-30
**Files analyzed:** 6 new/modified files
**Analogs found:** 6 / 6

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude-core/src/repository.rs` (new) | model | transform | `baude-core/src/workspace.rs` | role-match |
| `baude-core/src/git.rs` | service / utility | request-response + transform + file-I/O | `baude-core/src/git.rs` | exact extension seam |
| `baude-core/src/persist.rs` | service / model | file-I/O + transform | `baude-core/src/persist.rs` | exact extension seam |
| `baude-core/src/lib.rs` | config / module registry | transform | `baude-core/src/lib.rs` | exact |
| `baude/src/app.rs` | controller / store | event-driven + request-response | `baude/src/app.rs` | exact extension seam |
| `bauded/src/manager.rs` | service / store | request-response + file-I/O | `bauded/src/manager.rs` | exact extension seam |

Tests should remain inline in each Rust module under `#[cfg(test)]`; the repository has no separate Rust test tree. Phase 5 adds substantial test coverage to `git.rs`, `persist.rs`, and `app.rs`, and adjusts manager tests only where the shared persistence API changes.

## Pattern Assignments

### `baude-core/src/repository.rs` (model, transform)

**Analog:** `baude-core/src/workspace.rs`

Use this file for UI-free core value types, pure state transitions, and inline unit tests. Do not put PTY/backend ownership in repository records.

**Imports and core-owned model pattern** (`baude-core/src/workspace.rs` lines 33-46):

```rust
use std::sync::OnceLock;

use crate::backend::{self, Backend};
use crate::persist::Config;

pub struct Workspace {
    pub name: String,
    pub backend: &'static dyn Backend,
    pub daemon_url: Option<String>,
    pub daemon_port: Option<u16>,
}
```

Copy the local `crate::...` import style and public-field value-object style. For the new module, substitute serde/path imports and define opaque `RepositoryKey` / `CheckoutKey` newtypes plus repository, checkout-role, health, observed-path, ordering, active-intent, and retained-session value types.

**Pure decision function pattern** (`baude-core/src/workspace.rs` lines 105-120):

```rust
/// Pure resolution from explicit inputs — the testable core of [`active`].
pub fn resolve(
    ws_env: Option<&str>,
    backend_env: Option<&str>,
    config: &Config,
    mut warn: impl FnMut(String),
) -> Workspace {
    let name = sanitize(
        ws_env
            .or(config.workspace.as_deref())
            .or(backend_env)
            .or(config.backend.as_deref())
            .unwrap_or(DEFAULT),
    );
```

Keep allocation, uniqueness validation, migration grouping, and health transitions pure over explicit inputs where possible. This allows primary-dispatch and migration behavior to be tested without a live PTY or real user state.

**Typed state enum pattern** (`baude-core/src/session.rs` lines 20-31, 38-51):

```rust
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum StateSource {
    Hook,
    SessionFile,
    Silence,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Status {
    Waiting,
    Completed,
    Busy,
    Exited,
}
```

Model unavailable default/repository/checkout states as enums with preserved causes, not booleans or deleted records. Persisted key newtypes should derive serde traits and equality/order traits needed for foreign-key and uniqueness checks.

**Testing pattern** (`baude-core/src/workspace.rs` lines 183-203, 205-213):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn no_warn(msg: String) {
        panic!("unexpected warning: {msg}");
    }

    #[test]
    fn default_is_claude_workspace() {
        let ws = resolve(None, None, &Config::default(), no_warn);
        assert_eq!(ws.name, "claude");
        assert_eq!(ws.backend.name(), "claude");
    }
}
```

Add focused tests for monotonic opaque-key allocation, repository/checkout uniqueness, deterministic first-seen order, foreign-key validation, and unavailable-state retention.

---

### `baude-core/src/git.rs` (service / utility, request-response + transform + file-I/O)

**Analog:** existing command adapter and parser in `baude-core/src/git.rs`

Retain the shell-free `Command` boundary and contextual `anyhow` application errors, but replace the lossy string helper on topology paths with a byte-returning typed boundary. Repository discovery/default errors should remain distinguishable until the app presentation boundary.

**Imports and command invocation pattern** (lines 1-20):

```rust
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{anyhow, Result};

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
    } else {
        Err(anyhow!(
            "git {}: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}
```

Copy `Command::new(...).arg(...).args(...)`, success-status checking, and stderr-only diagnostic decoding. **Do not copy** `String::from_utf8_lossy` for `worktree list --porcelain -z`; return `Output`/bytes and parse NUL-delimited records into typed snapshots. Accept `OsStr` arguments where paths or refs cross the process boundary.

**Safe argv terminator pattern** (lines 146-163):

```rust
pub fn clone_repo(url: &str, dest: &Path) -> Result<()> {
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let out = Command::new("git")
        .args(["clone", "--", url])
        .arg(dest)
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(anyhow!(
            "git clone: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}
```

Use argv arrays only and `--end-of-options`/`--` where supported. Never interpolate paths or branch names into a shell command.

**Current worktree seam to replace** (lines 50-73):

```rust
pub fn create_worktree(repo: &Path, branch: &str) -> Result<PathBuf> {
    // ...
    if dir.exists() {
        return Ok(dir);
    }
    std::fs::create_dir_all(dir.parent().unwrap())?;
    // ...
    let new_branch = git(repo, &["worktree", "add", &dir_str, "-b", branch]);
    if new_branch.is_ok() {
        return Ok(dir);
    }
    git(repo, &["worktree", "add", &dir_str, branch]).map_err(|e| anyhow!("{e}"))?;
    Ok(dir)
}
```

Keep worktree creation centralized here, but do **not** retain directory-exists reuse or speculative `-b` fallback. First discover inventory, compare full `refs/heads/<branch>`, reuse only a verified record, create from the exact verified local/remote ref, then rediscover and verify common-dir/path/branch before returning.

**Testing pattern** (lines 177-205):

```rust
#[cfg(test)]
mod tests {
    use super::parse_clone_target;

    fn parts(input: &str) -> (String, String, String, String) {
        let t = parse_clone_target(input).expect(input);
        (t.host, t.owner, t.repo, t.url)
    }

    #[test]
    fn https_url_keeps_https() {
        let (host, owner, repo, url) = parts("https://github.com/poindexter12/baude");
        assert_eq!(host, "github.com");
        assert_eq!(owner, "poindexter12");
        assert_eq!(repo, "baude");
        assert_eq!(url, "https://github.com/poindexter12/baude.git");
    }
}
```

Extend this inline module with a standard-library real-Git fixture. Give each test a unique temp root, configure local `user.name`/`user.email`, run Git via argv, and clean up. Required groups: `admission_identity`, `default_branch`, `default_worktree`, and `reconciliation`; include nested/symlink/linked aliases, spaces/newlines, slash branches, dangling remote HEAD, detached/unborn states, branch occupancy, and externally changed/missing topology.

---

### `baude-core/src/persist.rs` (service / model, file-I/O + transform)

**Analog:** existing serde state and workspace fallback in `baude-core/src/persist.rs`, plus filename ownership in `workspace.rs`

**Serde record pattern preserving all legacy fields** (`persist.rs` lines 6-23):

```rust
#[derive(Serialize, Deserialize, Default)]
pub struct State {
    pub sessions: Vec<SavedSession>,
}

#[derive(Serialize, Deserialize, Clone)]
pub struct SavedSession {
    pub name: String,
    pub cwd: PathBuf,
    pub repo_root: PathBuf,
    pub branch: Option<String>,
    pub is_worktree: bool,
    pub shell_open: bool,
    #[serde(default)]
    pub archived: bool,
    #[serde(default)]
    pub archived_by_user: bool,
}
```

Keep this exact legacy shape available for strict legacy decoding and preserve all eight fields during migration. Add an explicit versioned current envelope containing repository and checkout records. Use `#[serde(default)]` only where missing-field compatibility has a defined meaning; malformed current state must not become an empty default.

**Workspace-primary then legacy fallback pattern** (`persist.rs` lines 131-150; `workspace.rs` lines 52-63):

```rust
pub fn load_for_workspace(base: &str, ws: &crate::workspace::Workspace) -> State {
    let primary = ws.state_file(base);
    if state_path(&primary).exists() {
        return load_named(&primary);
    }
    match ws.legacy_state_file(base) {
        Some(legacy) => load_named(&legacy),
        None => State::default(),
    }
}

pub fn save_for_workspace(
    base: &str,
    ws: &crate::workspace::Workspace,
    state: &State,
) -> Result<()> {
    save_named(&ws.state_file(base), state)
}
```

Preserve this source-selection order exactly: if the workspace-specific primary exists, inspect only it; otherwise inspect only the allowed legacy fallback. Migration must never glob or merge dormant files, and saves always target the workspace primary.

**Current direct-write seam to replace** (`persist.rs` lines 153-168):

```rust
pub fn load_named(file: &str) -> State {
    std::fs::read_to_string(state_path(file))
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub fn save_named(file: &str, state: &State) -> Result<()> {
    let path = state_path(file);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, serde_json::to_string_pretty(state)?)?;
    Ok(())
}
```

Keep parent-directory creation and pretty JSON, but replace silent/default loading with `Result<LoadOutcome, LoadError>` (`Missing`, `Legacy`, `Current`) and replace direct writing with sibling temp `create_new` → `write_all` → `flush` → `sync_all` → close → `rename`. On pre-rename failure remove only the temp; preserve destination bytes. Unsupported versions and malformed/I/O failures are blocking errors.

**Testing pattern** (`persist.rs` lines 171-187):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_archive_ms_resolves_env_then_config_then_default() {
        std::env::remove_var("BAUDED_AUTO_ARCHIVE_MIN");
        let mut c = Config::default();
        assert_eq!(c.auto_archive_ms(), crate::session::AUTO_ARCHIVE_IDLE_MS);
        c.auto_archive_minutes = Some(5);
        assert_eq!(c.auto_archive_ms(), 5 * 60_000);
    }
}
```

Add an injectable state-root/path helper so tests use isolated directories rather than process-global `~/.config/baude`. Add current round-trip, all-field legacy migration, migration idempotence, primary-over-legacy precedence, missing-file, malformed/truncated JSON, unsupported version, foreign-key/uniqueness rejection, and atomic old-byte preservation tests. Prefer inline JSON constants or module-local fixture builders unless reusable checked-in fixtures materially simplify all-eight-field coverage.

---

### `baude-core/src/lib.rs` (config / module registry, transform)

**Analog:** `baude-core/src/lib.rs`

**Module export pattern** (lines 8-17):

```rust
pub mod backend;
pub mod bridge;
pub mod git;
pub mod hook;
pub mod meta;
pub mod permission;
pub mod persist;
pub mod pty;
pub mod session;
pub mod workspace;
```

Add `pub mod repository;` alphabetically with the existing flat module declarations. Keep repository types UI-free as required by the crate-level documentation (lines 1-4).

---

### `baude/src/app.rs` (controller / store, event-driven + request-response)

**Analog:** existing startup/open/clone and session lifecycle in `baude/src/app.rs`

App remains the local aggregate/runtime owner. Replace each direct admission route with one idempotent `admit_repository` / `ensure_primary` path; do not duplicate Git identity/default logic in event handlers.

**Imports pattern** (lines 1-20):

```rust
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use anyhow::Result;

use baude_core::backend;
use baude_core::git;
use baude_core::meta::{now_unix_ms, ClaudeMeta, RateWindow};
use baude_core::persist::{self, Config, SavedSession, State};
use baude_core::pty::{now_ms, Pty};
use baude_core::session::{Session, Status};
```

Import shared core modules through `baude_core::...`; keep TUI-only state and message formatting local. Add repository/persistence outcome types to this block rather than exposing ratatui types to core.

**Backend/session spawn pattern** (lines 546-615):

```rust
let be = backend::active();
let base = be.resolve_cmd(&self.claude_cmd()).cmd;
let plan = be.spawn_plan(&base, None, resume);
be.prepare_cwd(&cwd);

let (rows, cols) = self.claude_spawn_size(shell_open);
let claude = Pty::spawn(Some(&plan.cmd), &cwd, rows, cols)?;
let mut meta = ClaudeMeta::default();
meta.backend_port = plan.server_port;

let id = self.next_id;
self.next_id += 1;
// construct Session
self.sessions.push(session);
self.selected_id = Some(SelId::Local(id));
Ok(id)
```

Reuse `add_session` (or an extracted injectable equivalent) for active-workspace backend selection, `prepare_cwd`, spawn-plan composition, monotonic runtime IDs, metadata initialization, and selection. Persist the repository + primary child + active intent successfully **before** calling this spawn path.

**Restore seam to replace** (lines 367-402):

```rust
pub fn restore(&mut self) {
    let state = persist::load();
    for saved in &state.sessions {
        if !saved.cwd.exists() {
            continue;
        }
        match self.add_session(/* saved fields */, true, saved.shell_open) {
            Ok(id) => { /* restore flags */ }
            Err(e) => self.set_message(format!("restore {}: {e}", saved.name)),
        }
    }
    // launch-directory auto-add
    self.save();
}
```

Replace missing-path `continue` with retained unavailable records. Handle load errors before mutating runtime state, display the source path/cause/recovery action, and disable automatic saves while load is blocked. Restore the hierarchy first; ensure only primary children with prior `active_intent`; leave intentionally idle parents idle. Do not unconditionally save after a failed load.

**One open/clone completion seam** (lines 1317-1363):

```rust
fn open_repo_session(&mut self, path: PathBuf) {
    if let Some(remote) = &self.remote {
        match remote.create(&path.to_string_lossy(), None, None) {
            Ok(()) => self.set_message("session queued on daemon".into()),
            Err(e) => self.set_message(format!("daemon: {e}")),
        }
        return;
    }
    match self.add_session(path, None, None, false, false, false) {
        Ok(_) => {
            self.focus = Focus::Claude;
            self.save();
        }
        Err(e) => self.set_message(format!("spawn failed: {e}")),
    }
}
// clone completion calls self.open_repo_session(pc.dest)
```

Keep Open and clone completion converged through one method, but make local behavior repository admission rather than direct spawn. Phase 5 does not add daemon hierarchy APIs; preserve remote routing and defer its projection.

**Focus/restart patterns** (`app.rs` lines 1388-1403 and 1567-1597):

```rust
if let Some(a) = &self.attach {
    if a.remote_id == id && !a.is_closed() {
        self.focus = Focus::Claude;
        return;
    }
}

let Some(s) = self.session(id) else { return };
if !s.claude.is_exited() {
    self.set_message("claude is still running".into());
    return;
}
// rebuild backend spawn plan and replace the exited PTY
```

Apply the same dispatch shape to a primary child: live → select/focus; exited retained runtime → restart/resume; absent + active intent → spawn; absent + idle → no-op. Key lookup by `(RepositoryKey, PrimaryDefault)`, never display name or cwd. Closing the primary kills/removes runtime only and clears active intent while retaining repository/checkout records.

**Testing analog:** `bauded/src/manager.rs` lines 886-907 and 1016-1036:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn mgr() -> Manager {
        Manager::new("sleep 30".into(), false)
    }

    #[test]
    fn restart_requires_exited() {
        let mut m = mgr();
        let id = m.create("/tmp", None, None).unwrap().id;
        let err = m.restart(id).unwrap_err().to_string();
        assert!(err.contains("still running"), "got: {err}");
        m.kill_all();
    }
}
```

Extract a pure primary-dispatch decision or injectable spawn seam before adding App tests. Cover live focus, exited resume, absent-active spawn, absent-idle no-op, duplicate launch/Open/clone calls, save failure before spawn, spawn failure after durable intent, primary close/reopen, and startup intent filtering. Use harmless commands and cleanup as manager tests do; never launch Claude/OpenCode in automated tests.

---

### `bauded/src/manager.rs` (service / store, request-response + file-I/O)

**Analog:** existing persistence ownership and error reporting in `bauded/src/manager.rs`

Phase 5 changes this file only enough to consume the shared versioned/result-valued persistence safely. Do not add repository hierarchy endpoints or remote projection here.

**Imports and workspace-specific state ownership** (lines 14-19, 26-30):

```rust
use baude_core::backend;
use baude_core::git;
use baude_core::meta::{now_unix_ms, ClaudeMeta, HookEvent};
use baude_core::persist::{self, SavedSession, State};
use baude_core::pty::Pty;
use baude_core::session::{Session, StateSource, Status};

const STATE_BASE: &str = "daemon-state";
```

Continue routing daemon state through shared `persist` and `workspace::active()`. Update imported state/outcome/error types; do not hand-roll a daemon-only format.

**Restore and save error boundary** (lines 215-272):

```rust
pub fn restore(&mut self) -> usize {
    let state = persist::load_for_workspace(STATE_BASE, baude_core::workspace::active());
    // restore sessions
    self.save();
    restored
}

pub fn save(&self) {
    if !self.persist {
        return;
    }
    // construct State
    if let Err(e) =
        persist::save_for_workspace(STATE_BASE, baude_core::workspace::active(), &state)
    {
        eprintln!("save state: {e}");
    }
}
```

Keep `persist: false` as the test guard and contextual stderr reporting. Change restore to handle `Missing`/`Legacy`/`Current` explicitly and return/report a blocking error rather than silently continuing. On load failure set a persistence-disabled guard so the unconditional save shown above cannot overwrite malformed evidence. Missing/changed paths become retained unavailable records, not skipped rows.

**Backend spawn remains outside persisted repository records** (lines 330-379):

```rust
let be = backend::active();
be.prepare_cwd(&cwd);
let resolved = be.resolve_cmd(&self.claude_cmd);
let plan = be.spawn_plan(&resolved.cmd, Some(&event_url(id)), resume);
let claude = Pty::spawn(Some(&plan.cmd), &cwd, ROWS, COLS)?;
// initialize runtime Session and push it
```

Preserve active workspace backend resolution at spawn time. Repository records must not persist a backend.

**Test isolation pattern** (lines 886-905):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn mgr() -> Manager {
        Manager::new("sleep 30".into(), false)
    }

    #[test]
    fn create_list_info_remove() {
        let mut m = mgr();
        let info = m.create("/tmp", None, Some("t1")).unwrap();
        assert_eq!(info.name, "t1");
        m.remove(info.id).unwrap();
        assert!(m.list().is_empty());
    }
}
```

Adjust/add tests for result-valued daemon load, selected fallback migration, malformed-state save blocking, and preservation of all legacy fields. Keep `persist: false` for unrelated runtime tests; persistence tests must inject an isolated state root.

## Shared Patterns

### Workspace Isolation and Backend Selection

**Source:** `baude-core/src/workspace.rs` lines 52-63 and `baude/src/app.rs` lines 546-565  
**Apply to:** `persist.rs`, `app.rs`, `manager.rs`

```rust
pub fn state_file(&self, base: &str) -> String {
    format!("{base}-{}.json", self.name)
}

pub fn legacy_state_file(&self, base: &str) -> Option<String> {
    (self.name == DEFAULT).then(|| format!("{base}.json"))
}

let be = backend::active();
let plan = be.spawn_plan(&be.resolve_cmd(&self.claude_cmd()).cmd, None, resume);
be.prepare_cwd(&cwd);
```

Repository keys are stable only inside the selected workspace state file. The same filesystem repository may have an independent key in another workspace. Resolve backend from the active workspace when spawning, never from persisted repository data.

### Error Handling and User-Visible Recovery

**Source:** `baude-core/src/git.rs` lines 7-20; `baude/src/app.rs` lines 630-647; `bauded/src/manager.rs` lines 268-272  
**Apply to:** all admission, discovery, migration, load/save, and ensure paths

```rust
if out.status.success() {
    Ok(value)
} else {
    Err(anyhow!("git ...: {}", String::from_utf8_lossy(&out.stderr).trim()))
}

pub fn set_message(&mut self, msg: String) {
    self.message = Some((msg, now_ms() + MESSAGE_TTL_MS));
}

if let Err(e) = persist::save_for_workspace(/* ... */) {
    eprintln!("save state: {e}");
}
```

Core returns typed causes with command/path context. The TUI maps them to an actionable message; the daemon logs contextual errors. A malformed/unsupported state error is special: it blocks restore-triggered mutation and automatic save until resolved, preserving the source file.

### Persist Intent, Reconcile Facts

**Source:** `baude-core/src/persist.rs` lines 11-23 (durable session intent) and `baude-core/src/git.rs` lines 6-27 (Git queried at runtime)  
**Apply to:** `repository.rs`, `git.rs`, `persist.rs`, `app.rs`

Persist opaque IDs, ownership/role, first-seen ordering, managed flag, session settings, active intent, and last observations. Before launch or Git mutation, rediscover canonical common-dir, main-first worktree inventory, branch/ref state, and selected checkout. Update health/observations, but retain stale or unavailable intent for explicit recovery.

### Save Before Spawn

**Source:** current spawn construction at `baude/src/app.rs` lines 546-615, reordered by the Phase 5 durability contract  
**Apply to:** new admission and primary reopen paths in `app.rs`

The required sequence is: discover/reconcile → reserve repository and primary child → set active intent → validate aggregate → atomically save → spawn/focus/restart. Never call `Pty::spawn` before the first successful save for a newly admitted primary.

### Inline Tests and Pure Cores

**Source:** `baude-core/src/session.rs` lines 149-180 and 341-380; `baude-core/src/workspace.rs` lines 105-166 and 183-213  
**Apply to:** `repository.rs`, `git.rs`, `persist.rs`, `app.rs`

```rust
fn decide_status(/* raw inputs */) -> (Status, StateSource) {
    // side-effect-free decision
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_hook_busy_wins() {
        let (st, src) = decide_status(/* explicit inputs */);
        assert_eq!((st, src), (Status::Busy, StateSource::Hook));
    }
}
```

Extract pure parser/decision/migration/dispatch functions and test explicit inputs. Wrap real Git and filesystem behavior with isolated standard-library fixtures. App tests need an injectable spawn decision/seam because the current file has no inline test module and `add_session` directly spawns a PTY.

## Patterns That Must Not Be Copied Unchanged

| Existing Source | Existing Behavior | Phase 5 Replacement |
|-----------------|-------------------|---------------------|
| `git.rs:13` | Lossy stdout conversion | Byte-safe `Output` / NUL parser for topology paths |
| `git.rs:23-27` | `--show-toplevel` as repository identity | canonical common-dir + first main-worktree record |
| `git.rs:60-63` | existing directory means reusable worktree | reuse only verified inventory records |
| `persist.rs:155-159` | all load errors become empty state | only NotFound is empty; typed blocking errors otherwise |
| `persist.rs:162-168` | direct destination write | sibling temp, flush, sync, close, atomic rename |
| `app.rs:369-372`, `manager.rs:220-223` | missing cwd silently skipped | retain unavailable repository/checkout metadata |
| `app.rs:401-421`, `manager.rs:244-272` | restore followed by unconditional save | save only after successful load/migration; block after load error |
| `app.rs:502-513`, `manager.rs:383-395` | display-name suffix as duplicate defense | primary uniqueness by repository key + checkout role |

## No Analog Found

None. Every planned file has a local role or integration analog. The exact byte-safe worktree parser, versioned load outcome, atomic replacement implementation, and repository aggregate are new contracts; use `05-RESEARCH.md` for their detailed algorithms while retaining the codebase conventions above.

## Metadata

**Analog search scope:** `baude-core/src`, `baude/src`, `bauded/src`, inline `#[cfg(test)]` modules  
**Files scanned:** 29 Rust source files discovered; 7 closest analog files read in full  
**Primary analogs:** `baude-core/src/{git,persist,workspace,session,lib}.rs`, `baude/src/app.rs`, `bauded/src/manager.rs`  
**Pattern extraction date:** 2026-08-30
