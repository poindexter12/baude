# Phase 8: Test Isolation and Fixture Ownership - Pattern Map

**Mapped:** 2026-09-13
**Files analyzed:** 10 (2 new, 8 modified)
**Analogs found:** 9 / 10

All analog paths below are git-tracked source (`git ls-files` verified); this
repo has no gitignored install mirror.

## File Classification

| New/Modified File | Role | Data Flow | Closest Analog | Match Quality |
|-------------------|------|-----------|----------------|---------------|
| `baude-core/src/testing.rs` (NEW) | test-support utility / RAII guard | process-local state | `baude-core/src/git.rs:1727-1748` (`WORKTREES_BASE_OVERRIDE` + setter) + `git.rs:2708-2790,2904` (`GitFixture` Drop) | exact |
| `baude-core/src/worktree_scan.rs` (NEW) | service / classifier | file-I/O + batch | `baude-core/src/git.rs:2499` (`inspect_removal` → `RemovalSafety`/`RemovalBlocker`) | role-match (fail-closed verdict enum); enumeration is greenfield |
| `baude-core/src/git.rs` | resolver + path composition | request-response | itself, `worktrees_base()` at `git.rs:1750` | exact (in-place) |
| `baude-core/src/persist.rs` (`config_base`) | resolver | file-I/O | `git.rs:1750` `worktrees_base()` (the only resolver already carrying a redirect + guard) | exact |
| `baude-core/src/meta.rs` (`claude_config_dir`) | resolver | file-I/O | `git.rs:1750` `worktrees_base()` | exact |
| `baude-core/src/hook.rs` | resolver (override folded into `testing.rs`) | request-response | `git.rs:1727-1748` | exact |
| `baude-core/src/workspace.rs` (`initialize`/`active`) | provider / identity cache | request-response | `git.rs:1727-1748` redirect shape; current `ACTIVE: OnceLock` at `workspace.rs:199` | role-match (needs `Box::leak` for `&'static`) |
| `baude/src/main.rs` (scan subcommand arm) | CLI route | request-response | `baude/src/main.rs:225-251` (`statusline`/`hook`/`permission-mcp` arms) | exact |
| `bauded/src/push.rs` | store (VAPID + subscriptions) | file-I/O | `baude-core/src/persist.rs:840-852` (`config_base`/`config_dir`) | exact — it is a verbatim duplicate to delete |
| `bauded/src/manager.rs` fixture helper | test fixture | — | `baude-core/src/git.rs:2706-2790` `GitFixture` (seq counter + Drop) | exact |
| `baude/src/ui.rs` fixture ownership (revision 2) | test fixture | App constructor -> renderer | Returned owner pattern from 08-01 app/API fixtures; five direct App sites and six hierarchy_fixture callers | role-match |
| `baude/src/usage.rs` (revision 2) | test worker boundary | App -> detached thread -> ccusage | Existing UsageCosts snapshot; test start becomes inert, production start unchanged | new compile-time test policy |
| `baude-core/src/pty.rs` (revision 2) | subprocess environment boundary | shared spawn -> paused gate -> shell | Existing CommandBuilder::env wiring at 80-98 and registration gate at 145-155 | exact launch seam, additional test containment |
| `baude-core/Cargo.toml` / `baude`/`bauded` `Cargo.toml` | config (feature wiring) | — | none in repo (`[features]` absent from all three manifests) | **no analog** |

## Pattern Assignments

### `baude-core/src/testing.rs` (NEW — test-support utility)

**Analog:** `baude-core/src/git.rs:1727-1758`, plus `git.rs:2706-2718` / `git.rs:2904` for the RAII+seq shape.

**Thread-local redirect storage + its recorded rationale** (`git.rs:1727-1733`) — copy this doc-comment discipline, the parallel-test rationale is the repo's governing rule:
```rust
thread_local! {
    /// Test-only redirect for [`worktrees_base`]. Thread-local, not an env
    /// var: the test binary runs cases in parallel, and a process-wide
    /// `XDG_DATA_HOME` written by one case would decide where another one's
    /// worktrees land.
    static WORKTREES_BASE_OVERRIDE: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}
```
Note `const { RefCell::new(None) }` — the repo uses the const-init form in both
`git.rs:1732` and `hook.rs:94`. Keep it.

**Setter to generalize** (`git.rs:1742-1748`) — the new guard replaces this; the `AtomicBool` arm is deleted per RESEARCH §Pattern 4:
```rust
pub fn set_worktrees_base_for_test(base: impl Into<PathBuf>) {
    let base = base.into();
    WORKTREES_BASE_OVERRIDE.with(|cell| *cell.borrow_mut() = Some(base));
    REQUIRE_WORKTREES_OVERRIDE.store(true, Ordering::SeqCst);   // DELETE
}
```

**Escape-assert message shape to preserve** (`git.rs:1753-1758`):
```rust
assert!(
    !REQUIRE_WORKTREES_OVERRIDE.load(Ordering::SeqCst),
    "managed worktree root resolved to the real data dir during a test; \
     call baude_core::git::set_worktrees_base_for_test on this thread first"
);
```
The new `assert_contained(resolved, what)` keeps the "what happened + what to do" two-clause message, naming `TestRedirect` instead of the setter.

**RAII + unique-root shape** (`git.rs:2706-2718`, `git.rs:2903-2907`):
```rust
static NEXT_FIXTURE: AtomicU64 = AtomicU64::new(1);

impl GitFixture {
    fn new() -> Self {
        let sequence = NEXT_FIXTURE.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir()
            .join(format!("baude-git-test-{}-{sequence}", std::process::id()));
        std::fs::create_dir(&root).expect("create unique Git fixture root");
        Self { root }
    }
}

impl Drop for GitFixture {
    fn drop(&mut self) { let _ = std::fs::remove_dir_all(&self.root); }
}
```
`Drop` swallows the error (`let _ =`) — do the same in `TestRedirect::drop`; a
guard that panics during unwind aborts the process.

**Module registration** — `baude-core/src/lib.rs` lists modules alphabetically, all bare `pub mod`. Add `pub mod testing;` (cfg-gated) and `pub mod worktree_scan;` in alphabetical position.

---

### `baude-core/src/persist.rs` — `config_base()` (resolver, file-I/O)

**Analog:** `git.rs:1750-1766` (redirect-then-guard-then-real), applied to `persist.rs:840-852`.

**Current code to split** (`persist.rs:840-852`) — the body becomes `real_config_base()` verbatim:
```rust
fn config_base() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baude")
}

/// The config directory (`~/.config/baude`), for sibling stores that live
/// next to config.json and the state files (e.g. breadcrumbs).
pub fn config_dir() -> PathBuf { config_base() }
```

**Guard insertion shape to copy** (from `git.rs:1750-1758`): check override first and return early; only the fall-through real resolution is asserted. Do not assert on the override path.

**Also in this file:** `release_state_lock_for_test` (`persist.rs:551-553`) is `#[cfg(test)]` and not `pub`:
```rust
/// Drop this process's claim on a lock so a fixture root can be reused or
/// removed. Tests only — the real lock is held for the life of the process.
#[cfg(test)]
fn release_state_lock_for_test(destination: &std::path::Path) {
```
Change both: `pub` + `#[cfg(any(test, feature = "test-support"))]` (RESEARCH §Finding 7).

---

### `baude-core/src/meta.rs` — `claude_config_dir()` (resolver, file-I/O)

**Analog:** same as `persist.rs` — `git.rs:1750`.

**Current code** (`meta.rs:23-29`) — CLAUDE_CONFIG_DIR→home→fallback, not XDG; split the body into `real_claude_config_dir()`:
```rust
/// The config dir the spawned claude processes will use (inherited env).
pub fn claude_config_dir() -> PathBuf {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".claude")))
        .unwrap_or_else(|| PathBuf::from("."))
}
```
Callers `meta.rs:217` and `meta.rs:261` are untouched — that is the whole point of the thread-local decision.

---

### `baude-core/src/hook.rs` (resolver)

**Analog / source to move:** `hook.rs:91-111`, the second instance of the `git.rs` pattern:
```rust
thread_local! {
    /// Test-only override for [`baude_hook_command`]. Thread-local so parallel
    /// cases cannot decide each other's seeded command.
    static HOOK_COMMAND_OVERRIDE: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn set_hook_command_for_test(command: impl Into<String>) {
    let command = command.into();
    HOOK_COMMAND_OVERRIDE.with(|cell| *cell.borrow_mut() = Some(command));
}
```
Its long doc-comment (`hook.rs:98-105`) explains *why* fixtures must seed a
production-shaped `<abs>/baude hook` — carry that explanation into
`TestRedirect`'s `hook_command` field, or the reason is lost.

---

### `baude-core/src/workspace.rs` — `initialize` / `active` (provider)

**Analog:** `git.rs:1727-1748` for the override; the existing body is the fallback.

**Current code** (`workspace.rs:199-224`):
```rust
static ACTIVE: OnceLock<Workspace> = OnceLock::new();

pub fn initialize(hint: Option<&str>) -> &'static Workspace {
    ACTIVE.get_or_init(|| {
        resolve_with_hint(
            std::env::var("BAUDE_WORKSPACE").ok().as_deref(),
            std::env::var("BAUDE_BACKEND").ok().as_deref(),
            hint,
            &crate::persist::load_config(),     // <- the real-config read to inject away
            |msg| eprintln!("baude: {msg}"),
        )
    })
}

pub fn active() -> &'static Workspace { initialize(None) }
```
`active()`'s signature is `-> &'static Workspace`, so the thread-local stores a
`Box::leak`'d `&'static Workspace` in a `Cell` (Copy) — not the `RefCell<Option<PathBuf>>`
of the path redirects. This is the one place the `git.rs` shape must be adapted
rather than copied; the verified lifetime skeleton is in RESEARCH §Finding 3. The executable
08-03 contract supersedes the lazy initializer shown as current code above: active() is a
reader only, support builds require an override before cache lookup, and production
startup calls initialize(&config, hint) explicitly. Neither identity function reads config.
Test initialization updates a held fixture scope, never the production OnceLock.

---

### `baude-core/src/worktree_scan.rs` (NEW — service, file-I/O + batch)

**Analog:** `baude-core/src/git.rs:1904+` (`RemovalBlocker`) and `git.rs:2499` (`inspect_removal`) — the repo's established fail-closed verdict pattern.

**Blocker-enum pattern to mirror** (`git.rs:1904-1920`) — one variant per independent reason, payload carrying the evidence:
```rust
pub enum RemovalBlocker {
    NotManaged,
    MainWorktree,
    NotLinked,
    Detached,
    Locked,
    Prunable,
    IdentityChanged,
    PathChanged,
    BranchChanged,
    StagedAdd { path: Vec<u8> },
    UnstagedModification { path: Vec<u8> },
    Conflict { path: Vec<u8> },
    // …
}
```
`Evidence`/`Verdict` in RESEARCH §Pattern 5 should follow exactly this shape:
named variants, data-carrying where the report needs the detail.

**Fail-closed signature pattern** (`git.rs:2499-2503`) — errors are a distinct
type, never folded into the "safe" verdict:
```rust
pub fn inspect_removal(
    expected_common_dir: &Path,
    checkout: &SavedCheckout,
) -> std::result::Result<RemovalSafety, InspectionError> {
```
Note the fully-qualified `std::result::Result` — this file shadows `Result`
with an alias; keep the qualification when adding functions to `git.rs`.

**Reusable primitives (do not reimplement):**
- `git::discover_repository(path: &Path) -> Result<RepositorySnapshot, RepositoryDiscoveryError>` (`git.rs:294`) — canonicalizes input first (`git.rs:297`).
- `git::inspect_removal` (`git.rs:2499`), `remove_verified_worktree` (`git.rs:2660`) for `--prune`.
- `persist::load_named_at(root, file)` (`persist.rs:1007`) — strict non-locking state reader, widened to pub(crate) in 08-04. The workspace-aware loader at :338 acquires a lock and must not be used by the read-only scanner. Inventory every state file independently, including legacy filenames.
- `parse_worktree_porcelain` (`git.rs:211`) — currently private; needs `pub(crate)` or `pub` if the scanner calls it.

**Symlink precedent** (`git.rs:2594`): classification uses `std::fs::symlink_metadata`, never `metadata`/`exists()`.

**Path shape to match, verbatim source of truth** (`git.rs:1769-1775`):
```rust
pub fn managed_default_worktree_path(repository_key: u64, checkout_key: u64) -> PathBuf {
    worktrees_base()
        .join(&crate::workspace::active().name)
        .join(format!("repository-{repository_key}"))
        .join(format!("primary-{checkout_key}"))
}
```
and the branch variant (`git.rs:1805-1810`) whose child is `{label}-{checkout_key}`
with `label` sanitized (`git.rs:1777-1788`) and truncated to 48 bytes — lossy,
so the scanner must not reverse it to a branch name.

---

### `baude/src/main.rs` — scan subcommand arm (CLI route)

**Analog:** `baude/src/main.rs:225-251` — three existing hand-rolled arms.

**Dispatch pattern** (`main.rs:240-242`), with the comment convention (every arm
explains *why it must precede the TUI*):
```rust
    // `baude hook` — Claude Code lifecycle-event hook, no TUI. …
    if args.get(1).map(String::as_str) == Some("hook") {
        run_hook();
    }
```
and (`main.rs:225-233`) the flag-extraction idiom, when an arm takes options:
```rust
    if args.get(1).map(String::as_str) == Some("statusline") {
        let wrap = args
            .iter()
            .position(|a| a == "--wrap")
            .and_then(|i| args.get(i + 1))
            .cloned();
        std::process::exit(baude_core::bridge::run(wrap));
    }
```
`std::process::exit(<code>)` is the exit convention for arms that return a status.

**Help text to extend** (`main.rs:262-269`) — the new verb must appear here:
```rust
            println!(
                "baude {} — multiple AI coding sessions in one terminal\n\n\
                 usage: baude [<repo-dir>]\n\n\
                 subcommands: statusline, hook, permission-mcp\n\
                 options:     --version/-V, --help/-h",
                env!("CARGO_PKG_VERSION")
            );
```

---

### `bauded/src/push.rs` (store, file-I/O)

**Analog:** `baude-core/src/persist.rs:840-852` — `push.rs:27-33` is a byte-identical duplicate of it.

**Code to delete** (`push.rs:27-33`):
```rust
fn config_base() -> PathBuf {
    std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| dirs::home_dir().map(|h| h.join(".config")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("baude")
}
```

**Call site to rewrite** (`push.rs:51-52`, and the same shape at `:189`, `:207`):
```rust
    pub fn load_or_generate() -> Result<Vapid> {
        let path = config_base().join(VAPID_FILE);      // -> baude_core::persist::config_dir()
```
Filenames stay (`VAPID_FILE = "daemon-vapid.json"`, `SUBS_FILE = "daemon-push.json"`, `push.rs:24-25`), so the production path is unchanged.

---

### `bauded/src/manager.rs` — extracted fixture helper (test fixture)

**Analog:** `GitFixture` (`baude-core/src/git.rs:2706-2718`, Drop at `:2903-2907`, excerpted above).

Adopt its `AtomicU64` sequence counter — the 10 copied preambles in `manager.rs`
(`:2615, 2698, 2729, 2811, 2843, 2933, 3049, 3136, 3215, 3521`) use a pid-only
suffix, so two same-labelled tests in one process share a root.

---

### Cargo manifests (config) — no analog

`baude-core/Cargo.toml`, `baude/Cargo.toml`, `bauded/Cargo.toml` have **no
`[features]` section today** (verified: `baude-core/Cargo.toml` is
`[package]` + `[dependencies]` only), and `baude-core` is consumed as
`baude-core.workspace = true`. The feature wiring in RESEARCH §Pattern 2 is the
template; there is no in-repo precedent to copy.

## Shared Patterns

### Test isolation is thread-local, never env mutation
**Source:** `baude-core/src/git.rs:1728-1731` (the doc-comment rationale, PR #82 / `725c558`)
**Apply to:** every redirect added in this phase
```rust
/// Thread-local, not an env var: the test binary runs cases in parallel, and a
/// process-wide `XDG_DATA_HOME` written by one case would decide where another
/// one's worktrees land.
```

### Real-root resolution chain
**Source:** `persist.rs:840-846`, `git.rs:1760-1766`, `meta.rs:24-29`, `push.rs:27-33`
**Apply to:** real-root extractions and the external suite observer. Preserve each chain:

| Root | Precedence |
|------|------------|
| Config/state/push | XDG_CONFIG_HOME → home/.config → `.`; append baude |
| Claude | CLAUDE_CONFIG_DIR → home/.claude → `.`; no XDG lookup, no baude suffix |
| Worktrees | XDG_DATA_HOME → home/.local/share → `/tmp`; append baude/worktrees |

`var_os` treats an empty but present override as present. Unix `dirs::home_dir` uses a
nonempty HOME, then passwd-home fallback. 08-06 self-tests these distinctions with synthetic
child environments; a generic XDG chain would observe the wrong Claude root.

### Assertion messages state the fix, not just the fault
**Source:** `git.rs:1753-1757`
**Apply to:** the escape guard in all five resolvers — `"<what> resolved to the real … during a test; <do this> on this thread first"`.

### Doc-comments carry the rationale
**Source:** `hook.rs:98-105`, `git.rs:1735-1741`, `workspace.rs:201-205`
**Apply to:** every new public item. This codebase documents *why* a mechanism
was chosen over the obvious alternative; reviewers expect it.

### Drop impls never panic
**Source:** `git.rs:2904-2906` (`let _ = std::fs::remove_dir_all(…)`)
**Apply to:** `TestRedirect::drop`, any fixture helper.

## No Analog Found

| File | Role | Data Flow | Reason |
|------|------|-----------|--------|
| `baude-core/Cargo.toml` `[features]` + dev-dependency wiring | config | — | No manifest in the workspace declares a feature today; use RESEARCH §Pattern 2 |
| `worktree_scan::enumerate` (directory walk) | service | file-I/O | Greenfield — no code walks `~/.local/share/baude/worktrees`; only the *verdict* half has an analog (`inspect_removal`) |

## Revision 2: Returned UI Owners and Worker Boundaries

Source inspection found 39 test App::new calls in app.rs, five in ui.rs and one production
call in main.rs. ui.rs's hierarchy_fixture returns (App, RepositoryKey) at 2224/2301;
its guard owner must return with the App or be held by the caller. The other four direct
sites are 2439, 2551, 2799 and 2874. UiFixture in 08-03 follows the existing pid-plus-sequence
root and RAII restoration pattern; all six helper callers retain it through render/use.
08-03 authors ui_fixture_isolation_ regressions and 08-08 runs them after worker isolation.

App::new at 741 starts UsagePoller; usage.rs:33 launches a detached thread whose ccusage
command at 76 inherits environment. There is no core resolver call to intercept. Use an
inert cfg(test) start, not a dropped handle or an environment mutation. App's ambient remote
selection at 722-727 also runs before callers assign remote=None; gate it before launch.

The PTY reader thread does not resolve roots, but its child launcher at pty.rs:78-98 reads
SHELL and starts -il before the fixture command. 08-08 adds a support-only cleared child
map and explicit no-profile/no-rc test shell at this single shared launch seam, retaining
the existing registration/teardown and production branch. portable-pty 0.8.1 exposes
env_clear, env, env_remove and get_env; docs/source were checked in this revision, no
package installation. Worker inventory and synthetic regression contracts live in 08-08.
Thread-local guards remain the in-process rule, not a claim that subprocesses inherit them.

## Metadata

**Analog search scope:** `baude-core/src/{git,hook,persist,meta,workspace}.rs`, `baude/src/main.rs`, `bauded/src/{push,manager}.rs`, all three `Cargo.toml`
**Files scanned:** 9
**Pattern extraction date:** 2026-09-13
