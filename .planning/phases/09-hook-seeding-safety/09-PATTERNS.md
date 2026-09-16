# Phase 9: Hook Seeding Safety - Pattern Map

**Mapped:** 2026-09-15
**Files analyzed:** 7 modified files (no new files)
**Analogs found:** 7 / 7 (all changes land inside existing files with in-file or sibling analogs; all analog paths verified git-tracked via `git ls-files`)

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude-core/src/hook.rs` (guarded read, `SeedWarning`, quoting, recognizer, unit tests) | service/utility | file-I/O + transform | itself: `seed_settings` :266-277, `baude_hook_command` :88-97, `is_seeded_hook_command` :111-121, `merge_hook_settings` :163-197 | exact |
| `baude-core/src/backend/mod.rs` (trait `prepare_cwd` signature) | config (trait contract) | request-response | itself: :110 | exact |
| `baude-core/src/backend/claude.rs` (`prepare_cwd` impl, `seed_mcp_config` guard) | service | file-I/O | itself: :74-79, :110-122; guard pattern mirrors new `hook.rs` helper | exact |
| `baude-core/src/backend/opencode.rs` (trivial signature update) | service | — | `claude.rs` impl; today `fn prepare_cwd(&self, _cwd: &Path) {}` at :173 | exact |
| `baude/src/app.rs` (2 call sites :2841/:5245, warning surface, app-level integration test) | component (TUI) | event-driven | `warn_prompt_mode_without_daemon` :3594-3602 | exact |
| `bauded/src/manager.rs` (2 call sites :1158/:2136, eprintln surface, held-lock manager test) | service (daemon) | event-driven | `save()` eprintln :652-656; `ManagerFixture` :2652-2696 | exact |
| `baude-core/src/persist.rs` (leftover-lock reopen test) | test | file-I/O | `second_workspace_owner_cannot_load_while_writer_lock_is_held` :1633-1656, `held_state_lock_records_holder_pid` :1660-1678 | exact |

## Pattern Assignments

### `baude-core/src/hook.rs` (service, file-I/O)

**Analog:** itself — the defect sites are the pattern sources; modify in place.

**Clobber site to replace — `seed_settings`** (lines 266-277, verbatim):
```rust
pub fn seed_settings(cwd: &std::path::Path) {
    let dir = cwd.join(".claude");
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("settings.local.json");
    let existing = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<Value>(&s).ok())
        .unwrap_or_else(|| json!({}));
    let command = baude_hook_command();
    let merged = merge_hook_settings(&existing, &command);
    let _ = std::fs::write(&path, merged.to_string());
}
```
Replace the `.ok()` chain with the four-way guarded read (RESEARCH.md Pattern 1 sketch): `NotFound` → fresh `json!({})`; other read error / parse error / non-object root → return `SeedWarning { file, reason }` and touch nothing; write error after merge → warning, spawn continues. Return type becomes `Vec<SeedWarning>` (or `Option<SeedWarning>`).

**Quoting defect — `baude_hook_command`** (lines 88-97):
```rust
pub fn baude_hook_command() -> String {
    #[cfg(any(test, feature = "test-support"))]
    if let Some(command) = crate::testing::hook_command_override() {
        return command;
    }
    match std::env::current_exe() {
        Ok(p) => format!("{} hook", p.display()),
        Err(_) => FALLBACK_HOOK_COMMAND.to_string(),
    }
}
```
Quote only the `Ok` arm: `format!("{} hook", quote_posix_single(&p.display().to_string()))` with `fn quote_posix_single(s: &str) -> String { format!("'{}'", s.replace('\'', r"'\''")) }`. `FALLBACK_HOOK_COMMAND: &str = "baude hook"` (line 71) stays bare. Note: the override at :90 returns verbatim — Phase-8 fixture override strings must migrate to the quoted canonical form.

**Recognizer — `is_seeded_hook_command`** (lines 111-121):
```rust
pub fn is_seeded_hook_command(command: &str) -> bool {
    let Some(path) = command.strip_suffix(" hook") else {
        return false;
    };
    let path = std::path::Path::new(path);
    path.is_absolute()
        && matches!(
            path.file_stem().and_then(|stem| stem.to_str()),
            Some("baude" | "bauded")
        )
}
```
Keep `strip_suffix(" hook")` first; then two-arm extraction: if remainder starts AND ends with `'`, unquote and require round-trip `quote_posix_single(unquoted) == remainder` (strict — rejects user look-alikes, #78 class); else treat as legacy raw path. Existing `is_absolute()` + stem check applies to the extracted path. Unquote BEFORE `Path::new` (Pitfall 1: `Path::new("'/x/baude'")` is not absolute).

**Consumers that inherit the fix for free** (lines 163-197, 237-240): `merge_hook_settings` prunes via `groups.retain(|g| !seeded_group_command(g).is_some_and(is_seeded_hook_command))` (line 182) and its idempotency sentinel is exact string equality (lines 185-189) — always-quote keeps it deterministic. Safety-critical purity gate (lines 237-240):
```rust
fn is_pure_seed_group(group: &Value) -> bool {
    seeded_group_command(group)
        .is_some_and(|command| is_seeded_hook_command(command) || command == FALLBACK_HOOK_COMMAND)
}
```
This governs worktree removal; a false positive deletes a user's `.claude` (#78). Do not modify these functions — only the recognizer they call.

**Unit-test pattern:** existing `hook.rs` tests module; add cases per RESEARCH.md (quoted forms of spaced/`$`/`;`/backtick/embedded-`'` paths recognized, legacy form still recognized, `NotFound` = fresh seed vs other errors = warning, `merge(merge(x)) == merge(x)`). Spaced-path E2E per RESEARCH.md "Code Examples" — hostile chars in the DIRECTORY name, stub named `baude`, `#!/bin/sh` marker script, run via `std::process::Command::new("sh").arg("-c")`, `#[cfg(unix)]`.

---

### `baude-core/src/backend/mod.rs` (trait contract)

**Analog:** itself, line 110: `fn prepare_cwd(&self, cwd: &Path);` → `fn prepare_cwd(&self, cwd: &Path) -> Vec<SeedWarning>;`. Implementors enumerated by grep this session: `claude.rs:74` (real) and `opencode.rs:173` (empty body → returns `Vec::new()`). No others exist.

---

### `baude-core/src/backend/claude.rs` (service, file-I/O)

**Analog:** itself.

**`prepare_cwd` impl** (lines 74-79, verbatim):
```rust
fn prepare_cwd(&self, cwd: &Path) {
    crate::hook::seed_settings(cwd);
    if crate::permission::is_prompt_mode() {
        seed_mcp_config(cwd);
    }
}
```
Collect and return warnings from both seeds.

**`seed_mcp_config`** (lines 110-122, verbatim):
```rust
fn seed_mcp_config(cwd: &Path) {
    let exe = match std::env::current_exe() {
        Ok(p) => p.display().to_string(),
        Err(_) => return, // can't resolve the bridge command — best-effort skip.
    };
    let path = crate::permission::mcp_config_path(cwd);
    let existing = std::fs::read_to_string(&path)
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}));
    let merged = crate::permission::merge_mcp_config(&existing, &exe);
    let _ = std::fs::write(&path, merged.to_string());
}
```
Same guarded-read replacement, identical `SeedWarning` shape. **Do NOT quote `exe` here** — `.mcp.json` `command` is argv data, spawned without a shell (RESEARCH.md Pattern 3, A2). Discretion: share the guarded-read helper with `hook.rs` (recommended — bodies are near-identical; only the pre-resolved `current_exe` differs).

---

### `baude/src/app.rs` (TUI component, event-driven)

**Analog for warning surface:** `warn_prompt_mode_without_daemon` (lines 3594-3602, verbatim):
```rust
fn warn_prompt_mode_without_daemon(&mut self) {
    const MSG: &str = "BAUDE_PERMISSION_MODE=prompt has no approval UI under the bare TUI \
         (no daemon) — every tool will be DENIED. Run via bauded + the PWA to approve.";
    static WARNED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    if !WARNED.swap(true, std::sync::atomic::Ordering::Relaxed) {
        eprintln!("baude: {MSG}");
    }
    self.set_message(MSG.into());
}
```
Copy this shape: `self.set_message(...)` naming the affected file (criterion 1), optionally the `static WARNED` once-flag + `eprintln!("baude: ...")`. Call sites to update: `be.prepare_cwd(&cwd)` at app.rs:2841 and :5245 — consume the returned `Vec<SeedWarning>` there.

**Analog for app-level test:** Phase-8 owner-struct fixture convention (see `ManagerFixture` excerpt below for the field-ordering pattern); seed `{not json` and `[1,2]` variants, drive the spawn path through :2841, assert byte-identical file + surfaced warning. `hook_command_override` keeps the command production-shaped.

---

### `bauded/src/manager.rs` (daemon service, event-driven)

**Analog for warning surface:** `save()` (lines 652-656, verbatim):
```rust
pub fn save(&mut self) {
    if let Err(error) = self.save_checked() {
        eprintln!("save state: {error}");
    }
}
```
Copy this shape for seed warnings at call sites :1158 and :2136 — prefixed `eprintln!` is bauded's only warning channel (no logging crate; CONTEXT's "warn-level logging" maps to this).

**Analog for held-lock test:** `ManagerFixture` (lines 2652-2696) — key pattern:
```rust
struct ManagerFixture {
    _identity: baude_core::testing::TestRedirect,   // drop order: declaration order
    _redirect: baude_core::testing::TestRedirect,   // held as FIELD, RAII (D-01)
    root: PathBuf,
    workspace: baude_core::workspace::Workspace,
}
// new(): unique root via NEXT_MANAGER_FIXTURE counter + pid; root FIRST, identity SECOND:
let redirect = baude_core::testing::TestRedirect::new(&root);
let identity = baude_core::workspace::override_for_test(&config, None);
```
Assertion target per RESEARCH.md Open Question 1, option (a): a manager mutation under a held lock surfaces an error chain containing `StateLockError::Held { holder: Some(pid) }` — bauded has no startup claim today (out of scope to add one). The `Held` Display carries pid + lock path; the recovery-guidance sentences live only in the TUI (`baude/src/main.rs:341-359`, the `claim_workspace_state_lock` refusal block with `eprintln!("       Quit that instance, ...")`).

---

### `baude-core/src/persist.rs` (test, file-I/O)

**Analog:** existing lock tests (lines 1633-1678). External-holder simulation (verbatim, :1638-1645):
```rust
let lock = OpenOptions::new()
    .read(true)
    .write(true)
    .create(true)
    .truncate(false)
    .open(&lock_path)
    .unwrap();
lock.try_lock().unwrap();
```
Pid-stamp assertions from `held_state_lock_records_holder_pid` (:1660-1678): `lock_holder_pid(&lock_path)`, `std::fs::read_to_string(&lock_path).trim() == pid`. Cleanup: `release_state_lock_for_test(&destination)` (:558-566). Test flow for reopen-with-leftover-lock (Pitfall 6): raw `OpenOptions` + `try_lock()` as fake prior owner, stamp a fake pid, `drop(lock)` (releases OS lock, file stays), assert `hold_state_lock(&destination)` is `Ok(())` and re-stamps the current pid — do NOT hold the first claim via `hold_state_lock` (re-entrant cache at :578-579 returns `Ok` early and would bypass the leftover-file path). `hold_state_lock` internals for reference: :568-609 (`std::fs::TryLockError::WouldBlock` → `StateLockError::Held { holder: lock_holder_pid(&path), path }`).

## Shared Patterns

### SeedWarning value type (new, baude-core)
**Source:** RESEARCH.md Pattern 1 sketch (design, locked reason taxonomy)
**Apply to:** `hook.rs::seed_settings`, `backend/claude.rs::seed_mcp_config`, `backend/mod.rs` trait, both binaries' call sites
```rust
pub struct SeedWarning { pub file: std::path::PathBuf, pub reason: SeedWarningReason }
pub enum SeedWarningReason { Unreadable(std::io::Error), Unparseable(serde_json::Error), NonObjectRoot, WriteFailed(std::io::Error) }
```
baude-core never prints; binaries own presentation (`app.rs` `set_message`+`eprintln` pattern, `manager.rs` `eprintln` pattern).

### Best-effort seeding contract
**Source:** doc comments on `seed_settings` / `seed_mcp_config` (both read this session)
**Apply to:** all seed changes — a seed failure never aborts the spawn; warnings inform, never block.

### Phase-8 test isolation
**Source:** `baude_core::testing::TestRedirect` + owner-struct fixtures (`manager.rs:2652-2696` excerpt above); `BAUDE_TEST_FIXTURE_ROOT` containment
**Apply to:** every new test — thread-local RAII redirect, never env mutation; hostile-path E2E stub and marker must live under the fixture root.

## No Analog Found

None — every change modifies an existing file whose current code is the direct pattern source. The only genuinely new construct (`SeedWarning`) has a locked design sketch in RESEARCH.md.

## Metadata

**Analog search scope:** `baude-core/src/` (hook.rs, backend/, persist.rs, permission.rs, testing.rs), `baude/src/` (app.rs, main.rs), `bauded/src/` (manager.rs)
**Files scanned:** 9 (all verified git-tracked; `prepare_cwd` implementors enumerated by grep: claude.rs, opencode.rs only)
**Pattern extraction date:** 2026-09-15
