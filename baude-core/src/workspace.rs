//! Workspaces: named, hard-separated session contexts, each BOUND to one
//! backend so claude and opencode session pools can never mix — not on
//! restore, not through a shared daemon.
//!
//! A workspace owns its persisted session state (`state-<name>.json` /
//! `daemon-state-<name>.json` under the baude config dir) and pins the
//! backend every session in it uses. Two implicit workspaces exist with zero
//! config — `claude` and `opencode`, each bound to the backend of the same
//! name — and custom ones are declared in config.json:
//!
//! ```json
//! {
//!   "workspace": "oss",
//!   "workspaces": {
//!     "oss":  { "backend": "opencode", "daemon_port": 8650 },
//!     "work": { "backend": "claude", "daemon_url": "http://bauded:8642" }
//!   }
//! }
//! ```
//!
//! Selection: `BAUDE_WORKSPACE` env, then config `workspace`, then the
//! backend name (`BAUDE_BACKEND` / config `backend` / `claude`) — so
//! `BAUDE_BACKEND=opencode` alone lands in the `opencode` workspace and gets
//! separated state automatically. A workspace's backend BINDING WINS over
//! `BAUDE_BACKEND` (that is the whole point: the env var can't cross-wire a
//! workspace onto the wrong backend; a conflict warns and is ignored).
//!
//! The TUI may additionally pass a folder-memory HINT (the workspace last
//! used from the launch folder, see [`crate::folder_workspace`]) through
//! [`initialize`]. The hint fills in only when NEITHER env var is set — an
//! explicit `BAUDE_WORKSPACE`/`BAUDE_BACKEND` invocation always wins — and
//! then outranks the config defaults, so a folder that last ran `opencode`
//! comes back up there even when config names another default. With no hint,
//! resolution is byte-identical to the chain above.
//!
//! Back-compat: the `claude` workspace falls back to reading the legacy
//! un-suffixed `state.json` / `daemon-state.json` when its own file does not
//! exist yet, so pre-workspace session lists survive the upgrade (saves go to
//! the new name).

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::backend::{self, Backend};
use crate::persist::Config;

/// How the workspace was selected during resolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkspaceSource {
    /// Explicit BAUDE_WORKSPACE env var or config workspace key
    Explicit,
    /// Ancestor walk found a recorded folder binding
    Bound,
    /// Derived from repository root folder name
    Derived,
    /// Default (no env, no binding, no derived, no config)
    Default,
}

/// Launch context passed to workspace initialization.
pub struct WorkspaceLaunchContext {
    /// Workspace hint from ancestor walk (folder binding).
    pub hint: Option<String>,
    /// Repository root discovered during startup (for derivation).
    pub repo_root: Option<PathBuf>,
}

pub struct Workspace {
    pub name: String,
    pub backend: &'static dyn Backend,
    /// Per-workspace remote daemon URL (config `workspaces.<n>.daemon_url`);
    /// falls back to the global `daemon_url` at the call site.
    pub daemon_url: Option<String>,
    /// Explicit auto-daemon port (config `workspaces.<n>.daemon_port`).
    pub daemon_port: Option<u16>,
    /// How this workspace was selected.
    pub source: WorkspaceSource,
}

/// The default workspace/backend name, and the only one whose state files
/// have a legacy un-suffixed form.
pub const DEFAULT: &str = "claude";

impl Workspace {
    /// Session-state filename for one of the two state kinds (`"state"` for
    /// the TUI, `"daemon-state"` for bauded).
    pub fn state_file(&self, base: &str) -> String {
        format!("{base}-{}.json", self.name)
    }

    /// The pre-workspace filename this workspace may fall back to READING
    /// (never writing): only the `claude` workspace has one.
    pub fn legacy_state_file(&self, base: &str) -> Option<String> {
        (self.name == DEFAULT).then(|| format!("{base}.json"))
    }

    /// Human-facing label: the platform's product name, prefixed by the
    /// workspace name when it adds information. An implicit workspace named
    /// after its backend reads as just the platform ("Claude Code" /
    /// "opencode"); a custom one reads as "oss · opencode" so both the pool
    /// and the platform it operates on are visible at a glance.
    pub fn display_label(&self) -> String {
        if self.name == self.backend.name() {
            self.backend.display_name().to_string()
        } else {
            format!("{} · {}", self.name, self.backend.display_name())
        }
    }

    /// The loopback port `auto_daemon` uses for this workspace. Implicit
    /// workspaces get stable defaults (claude keeps the historical 8642);
    /// custom workspaces must set `daemon_port` in config — `None` here means
    /// auto_daemon cannot safely pick one.
    pub fn auto_daemon_port(&self) -> Option<u16> {
        self.daemon_port.or(match self.name.as_str() {
            "claude" => Some(8642),
            "opencode" => Some(8643),
            _ => None,
        })
    }

    /// Return the source label indicating how this workspace was chosen.
    /// Returns only the label: "(explicit)", "(folder binding)", "(derived)", or "(blank)" for Default.
    pub fn display_hint(&self) -> String {
        match self.source {
            WorkspaceSource::Explicit => "(explicit)".to_string(),
            WorkspaceSource::Bound => "(folder binding)".to_string(),
            WorkspaceSource::Derived => "(derived)".to_string(),
            WorkspaceSource::Default => "(blank)".to_string(),
        }
    }

    /// Return a title-compatible label showing workspace name and source.
    /// For Default source, returns "(blank)"; for others, returns "name (source-label)".
    pub fn title_label(&self) -> String {
        match self.source {
            WorkspaceSource::Default => "(blank)".to_string(),
            _ => {
                let source_label = self
                    .display_hint()
                    .trim_matches(|c| c == '(' || c == ')')
                    .to_string();
                format!("{} ({})", self.name, source_label)
            }
        }
    }
}

/// Keep workspace names filesystem- and URL-safe: anything outside
/// `[A-Za-z0-9_-]` becomes `-`.
pub fn sanitize(name: &str) -> String {
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

/// Pure resolution with launch context: env vars, launch hint/repo_root, config, and warnings.
/// Sets the source field based on precedence: BAUDE_WORKSPACE (Explicit) > hint (Bound) >
/// config workspace (Explicit) > derived from repo_root (Derived) > BAUDE_BACKEND (Explicit) >
/// config backend > DEFAULT (Default).
pub fn resolve_with_context(
    ws_env: Option<&str>,
    backend_env: Option<&str>,
    ctx: &WorkspaceLaunchContext,
    config: &Config,
    mut warn: impl FnMut(String),
) -> Workspace {
    // Determine source and derive workspace name.
    let (name, source) = if let Some(ws) = ws_env {
        (ws.to_string(), WorkspaceSource::Explicit)
    } else if let Some(hint) = ctx.hint.as_deref() {
        (hint.to_string(), WorkspaceSource::Bound)
    } else if let Some(ws) = config.workspace.as_deref() {
        (ws.to_string(), WorkspaceSource::Explicit)
    } else if let Some(repo_root) = &ctx.repo_root {
        // Derive from repository root folder name
        if let Some(folder_name) = repo_root.file_name() {
            let folder_str = folder_name.to_string_lossy();
            let derived = sanitize(&folder_str);
            if !derived.is_empty() {
                (derived, WorkspaceSource::Derived)
            } else {
                // Empty after sanitization, fall through to next rung
                let fallback_name = sanitize(
                    backend_env
                        .or(config.backend.as_deref())
                        .unwrap_or(DEFAULT),
                );
                (fallback_name, WorkspaceSource::Default)
            }
        } else {
            let fallback_name = sanitize(
                backend_env
                    .or(config.backend.as_deref())
                    .unwrap_or(DEFAULT),
            );
            (fallback_name, WorkspaceSource::Default)
        }
    } else if let Some(be) = backend_env {
        (be.to_string(), WorkspaceSource::Explicit)
    } else if let Some(be) = config.backend.as_deref() {
        (be.to_string(), WorkspaceSource::Explicit)
    } else {
        (DEFAULT.to_string(), WorkspaceSource::Default)
    };

    let name = sanitize(&name);
    let entry = config
        .workspaces
        .as_ref()
        .and_then(|m| m.get(&name))
        .cloned()
        .unwrap_or_default();

    // Backend: the workspace's explicit binding wins; an implicit workspace
    // named after a backend binds to it; otherwise fall through the plain
    // backend chain. Unknown names fall back to claude (with a warning).
    let bound = entry
        .backend
        .clone()
        .or_else(|| match name.as_str() {
            "claude" | "opencode" => Some(name.clone()),
            _ => None,
        })
        .or_else(|| backend_env.map(str::to_string))
        .or_else(|| config.backend.clone());
    let be = backend::backend_for(bound.as_deref());
    if let Some(b) = bound.as_deref() {
        if b != be.name() {
            warn(format!(
                "unknown backend {b:?} for workspace {name:?} — using {}",
                be.name()
            ));
        }
    }
    // The binding is authoritative: a conflicting BAUDE_BACKEND is ignored,
    // loudly — silently honoring it would re-open the cross-wiring hole
    // workspaces exist to close.
    if let (Some(bound), Some(env)) = (entry.backend.as_deref(), backend_env) {
        if bound != env {
            warn(format!(
                "workspace {name:?} is bound to backend {bound:?} — ignoring BAUDE_BACKEND={env:?}"
            ));
        }
    }

    Workspace {
        name,
        backend: be,
        daemon_url: entry.daemon_url,
        daemon_port: entry.daemon_port,
        source,
    }
}

/// Pure resolution from explicit inputs — the testable core of [`active`].
/// `warn` receives human-readable conflict/fallback messages (the binaries
/// route it to stderr; tests capture it).
pub fn resolve(
    ws_env: Option<&str>,
    backend_env: Option<&str>,
    config: &Config,
    warn: impl FnMut(String),
) -> Workspace {
    resolve_with_hint(ws_env, backend_env, None, config, warn)
}

/// [`resolve`] plus a folder-memory hint. The hint is consulted ONLY when
/// neither `BAUDE_WORKSPACE` nor `BAUDE_BACKEND` is set — either env var is
/// an explicit per-invocation choice that wins over remembered history — and
/// then slots ABOVE the config defaults. Any hinted name resolves the same
/// way an explicit `BAUDE_WORKSPACE` of that name would (undeclared names get
/// their own namespace and the default backend chain), so a stale hint fails
/// open instead of erroring.
pub fn resolve_with_hint(
    ws_env: Option<&str>,
    backend_env: Option<&str>,
    hint: Option<&str>,
    config: &Config,
    warn: impl FnMut(String),
) -> Workspace {
    let context = WorkspaceLaunchContext {
        hint: if ws_env.is_some() || backend_env.is_some() {
            None
        } else {
            hint.map(str::to_string)
        },
        repo_root: None,
    };
    resolve_with_context(ws_env, backend_env, &context, config, warn)
}

/// The production identity cache, written exactly once by production
/// [`initialize`] at start-up.
///
/// Support builds never write it and never read it: a cached `Workspace`
/// carries no fixture provenance, so "someone already initialized it" must not
/// count as "this fixture owns it" (D-08). It is dead in a support build that
/// is not `baude-core`'s own harness — `baude-core`'s tests seed it directly to
/// prove a populated cache STILL cannot be observed without an override.
#[cfg_attr(all(not(test), feature = "test-support"), allow(dead_code))]
static ACTIVE: OnceLock<Workspace> = OnceLock::new();

/// Substring every override-free identity panic carries, so a
/// `#[should_panic(expected = …)]` test pins THIS guard rather than any panic
/// that happens to occur.
pub const IDENTITY_ESCAPE_PANIC_MARKER: &str =
    "workspace identity was resolved with no fixture override";

/// Leak one resolved identity so [`active`] can keep handing out
/// `&'static Workspace` (D-07).
///
/// A `thread_local!` cannot produce a `'static` reference to its own storage,
/// and the return type is locked, so the per-fixture identity is leaked
/// instead. The leak is bounded by fixture-construction count rather than by
/// call count — repeated `active()` reads copy the reference and allocate
/// nothing — and it is compiled only under the test-support gate, so it never
/// reaches a shipped binary (T-08-07, accepted).
///
/// Environment inputs are explicitly ABSENT: a fixture identity comes from its
/// literal config and hint, never from the developer's ambient
/// `BAUDE_WORKSPACE`/`BAUDE_BACKEND` (D-06).
#[cfg(any(test, feature = "test-support"))]
fn leak_identity(config: &Config, hint: Option<&str>) -> &'static Workspace {
    Box::leak(Box::new(resolve_with_hint(
        None,
        None,
        hint,
        config,
        |msg| {
            eprintln!("baude: {msg}");
        },
    )))
}

/// Install a per-fixture workspace identity resolved from a LITERAL config,
/// for the current thread only, until the returned guard drops.
///
/// This is the explicit fixture constructor: no config file has to exist
/// anywhere, no environment variable is consulted, and no other thread's
/// identity changes. Scopes nest — the enclosing identity comes back on drop —
/// because the guard is the same unified [`crate::testing::TestRedirect`] the
/// rest of the fixture roots live in, not a second store.
///
/// Fixture owners hold the ROOT guard first and this identity guard second, so
/// identity is restored before the root it was resolved against.
#[cfg(any(test, feature = "test-support"))]
#[must_use = "the returned guard restores the enclosing identity the moment it drops; bind it to a \
              named local (or a fixture owner the caller retains) so the fixture keeps its own \
              workspace for the whole test body"]
pub fn override_for_test(config: &Config, hint: Option<&str>) -> crate::testing::TestRedirect {
    crate::testing::TestRedirect::with_workspace(leak_identity(config, hint))
}

/// Resolve and cache the process-wide workspace from an EXPLICITLY SUPPLIED
/// config with launch context (env vars, hint, repo_root).
///
/// The FIRST caller wins the cache. The TUI calls this once from `main`
/// (before `ensure_daemon` or any `active()` reader) so the launch context
/// participates; every other binary and subcommand resolves without context.
///
/// Reads NO configuration of its own — the config is a parameter.
#[cfg(not(any(test, feature = "test-support")))]
pub fn initialize_with_context(config: &Config, ctx: WorkspaceLaunchContext) -> &'static Workspace {
    ACTIVE.get_or_init(|| {
        resolve_with_context(
            std::env::var("BAUDE_WORKSPACE").ok().as_deref(),
            std::env::var("BAUDE_BACKEND").ok().as_deref(),
            &ctx,
            config,
            |msg| eprintln!("baude: {msg}"),
        )
    })
}

/// Resolve and cache the process-wide workspace from an EXPLICITLY SUPPLIED
/// config, optionally with a folder-memory hint (D-06).
///
/// The config is a parameter rather than a `load_config()` call inside this
/// function: production start-up has already loaded it, and a fixture must be
/// able to supply a literal so the identity path never imports the developer's
/// real configured workspace name into a test's path composition.
///
/// The FIRST caller wins the cache: the TUI calls this once from `main`
/// (before `ensure_daemon` or any `active()` reader) so the hint participates;
/// every other binary and subcommand never passes a hint and resolves exactly
/// as before.
///
/// Reads NO configuration of its own — see the parameter above.
#[cfg(not(any(test, feature = "test-support")))]
pub fn initialize(config: &Config, hint: Option<&str>) -> &'static Workspace {
    let ctx = WorkspaceLaunchContext {
        hint: hint.map(str::to_string),
        repo_root: None,
    };
    initialize_with_context(config, ctx)
}

/// Support-build [`initialize_with_context`]: resolves with launch context.
#[cfg(any(test, feature = "test-support"))]
pub fn initialize_with_context(config: &Config, ctx: WorkspaceLaunchContext) -> &'static Workspace {
    let resolved = Box::leak(Box::new(resolve_with_context(
        None,
        None,
        &ctx,
        config,
        |msg| {
            eprintln!("baude: {msg}");
        },
    )));
    crate::testing::replace_workspace_override(resolved);
    resolved
}

/// Support-build [`initialize`]: resolves the supplied literal into the
/// CURRENT FIXTURE'S identity scope and nothing else.
///
/// Distinct storage from production on purpose (D-05, D-08). It never writes
/// `ACTIVE`, so re-initializing one fixture cannot reach another one running
/// concurrently, and it never arms an override of its own — the enclosing
/// [`override_for_test`] guard must already be held, and that guard is what
/// restores the previous identity on drop. It reads no configuration: the
/// literal and hint are the whole input (D-06).
#[cfg(any(test, feature = "test-support"))]
pub fn initialize(config: &Config, hint: Option<&str>) -> &'static Workspace {
    let ctx = WorkspaceLaunchContext {
        hint: hint.map(str::to_string),
        repo_root: None,
    };
    initialize_with_context(config, ctx)
}

/// The active workspace for this process: resolved once from
/// `BAUDE_WORKSPACE`/`BAUDE_BACKEND`/config and cached (the poll loop reads
/// it every tick via [`backend::active`]).
///
/// READER ONLY. It resolves nothing, loads no config, and seeds no identity:
/// a missing production identity is a start-up bug to fix at the entry point,
/// never something to paper over with a lazy config read here (D-06, D-07).
#[cfg(not(any(test, feature = "test-support")))]
pub fn active() -> &'static Workspace {
    ACTIVE.get().expect(
        "workspace::initialize(&config, hint) must run at start-up before any workspace::active() \
         reader — this binary reached identity resolution without initializing it",
    )
}

/// Support-build [`active`]: the fixture's own identity, or an abort.
///
/// The escape assertion runs BEFORE any cache lookup, because containment is
/// not provenance: a workspace pinned by whichever fixture ran first is still
/// the wrong identity for this one, and the filesystem paths it composes would
/// all be correct-looking and shared (D-08, T-08-24).
#[cfg(any(test, feature = "test-support"))]
pub fn active() -> &'static Workspace {
    crate::testing::workspace_override().unwrap_or_else(|| {
        panic!(
            "{IDENTITY_ESCAPE_PANIC_MARKER} during a test; hold a fixture identity \
             (`baude_core::workspace::override_for_test(&literal_config, None)`) on this thread \
             before anything reads the active workspace"
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persist::WorkspaceConfig;
    use crate::testing::TestRedirect;
    use std::collections::HashMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    /// A root that exists only as a string. Identity resolution touches no
    /// filesystem, so nothing here is ever created — which also makes it
    /// impossible for one of these cases to leak a directory.
    fn synthetic_root(label: &str) -> PathBuf {
        static SEQ: AtomicU64 = AtomicU64::new(0);
        PathBuf::from(format!(
            "/nonexistent/baude-workspace-{label}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ))
    }

    /// A literal config naming the workspace this fixture wants — never the
    /// developer's configured one.
    fn literal(workspace: &str) -> Config {
        Config {
            workspace: Some(workspace.to_string()),
            ..Config::default()
        }
    }

    /// Two fixtures alive AT THE SAME TIME on different threads each resolve
    /// their own identity and never the other's (D-05).
    ///
    /// Both overrides are constructed INSIDE their own thread bodies. A
    /// thread-local does not cross a `std::thread::spawn` boundary, so an
    /// override built on the parent thread would be invisible in both children
    /// and this test would certify the opposite of what it claims. The barrier
    /// makes the overlap real rather than incidental: neither thread reads its
    /// identity until the other's is installed.
    ///
    /// Nor is this "an override set by one `#[test]` is invisible to the next"
    /// — libtest allocates a fresh thread per test, so that assertion passes on
    /// a process-wide `OnceLock` and certifies nothing. THIS property is the one
    /// a process-wide cache fails: with `OnceLock`, whichever thread wins the
    /// race pins the identity both threads then observe.
    #[test]
    fn concurrent_fixtures_resolve_independent_identities() {
        use std::sync::{Arc, Barrier};

        let barrier = Arc::new(Barrier::new(2));
        let fixture = |name: &'static str, barrier: Arc<Barrier>| {
            std::thread::spawn(move || {
                let _root = TestRedirect::new(synthetic_root(name));
                let _identity = override_for_test(&literal(name), None);
                // Both identities are now live at once.
                barrier.wait();
                let first = active().name.clone();
                let managed = crate::git::managed_default_worktree_path(7, 11);
                // Hold both through the second read.
                barrier.wait();
                (first, active().name.clone(), managed)
            })
        };

        let alpha = fixture("alpha-ws", Arc::clone(&barrier));
        let beta = fixture("beta-ws", Arc::clone(&barrier));
        let (a_first, a_second, a_managed) = alpha.join().expect("alpha fixture panicked");
        let (b_first, b_second, b_managed) = beta.join().expect("beta fixture panicked");

        assert_eq!(
            a_first, "alpha-ws",
            "alpha observed another fixture's identity"
        );
        assert_eq!(
            a_second, "alpha-ws",
            "alpha's identity did not survive the overlap"
        );
        assert_eq!(
            b_first, "beta-ws",
            "beta observed another fixture's identity"
        );
        assert_eq!(
            b_second, "beta-ws",
            "beta's identity did not survive the overlap"
        );
        assert!(
            a_managed.to_string_lossy().contains("alpha-ws"),
            "managed path followed the wrong identity: {}",
            a_managed.display()
        );
        assert!(
            b_managed.to_string_lossy().contains("beta-ws"),
            "managed path followed the wrong identity: {}",
            b_managed.display()
        );
    }

    /// An override-free reader must ABORT, not fall back to a config-derived
    /// identity (D-08). A silent fallback is what pins the whole test binary to
    /// the developer's configured workspace name today.
    ///
    /// Note the root redirect IS held: containment alone does not make an
    /// identity fixture-owned, so this must panic even though every path this
    /// test could resolve is already inside a fixture root.
    #[test]
    #[should_panic(expected = "workspace identity was resolved with no fixture override")]
    fn active_without_an_override_panics_even_when_contained() {
        let _root = TestRedirect::new(synthetic_root("no-override"));
        let _escaped = active();
    }

    /// An inner identity scope shadows the outer one, and the outer value comes
    /// back when the inner scope ends.
    #[test]
    fn nested_identity_scopes_restore_the_outer_workspace() {
        let _root = TestRedirect::new(synthetic_root("nested"));
        let _outer = override_for_test(&literal("outer-ws"), None);
        assert_eq!(active().name, "outer-ws");
        {
            let _inner = override_for_test(&literal("inner-ws"), None);
            assert_eq!(active().name, "inner-ws");
        }
        assert_eq!(
            active().name,
            "outer-ws",
            "dropping the inner identity must restore the outer one"
        );
    }

    /// Dropping the LAST identity scope restores the required panic — a fixture
    /// cannot leave an identity armed for whatever runs next on this thread.
    #[test]
    #[should_panic(expected = "workspace identity was resolved with no fixture override")]
    fn dropping_the_last_identity_scope_restores_the_override_free_panic() {
        let _root = TestRedirect::new(synthetic_root("drop-restores"));
        {
            let _identity = override_for_test(&literal("transient-ws"), None);
            assert_eq!(active().name, "transient-ws");
        }
        let _escaped = active();
    }

    /// `initialize` resolves from the config it is HANDED, updates only this
    /// fixture's scope, and neither it nor any reader touches config on disk
    /// (D-06, D-07, T-08-24).
    #[test]
    fn initialize_uses_injected_config() {
        let root = synthetic_root("injected");
        let _root = TestRedirect::new(&root);
        let _identity = override_for_test(&literal("seed-ws"), None);
        assert_eq!(active().name, "seed-ws");

        // Positive control: the counter DOES move when config is really read,
        // so a zero delta below means "no read", not "broken instrument".
        let before = crate::persist::config_read_count_for_test();
        let _redirected = crate::persist::load_config();
        assert_eq!(
            crate::persist::config_read_count_for_test(),
            before + 1,
            "the config-read counter must advance on a real (redirected) read"
        );

        // The measured window: literal initialization plus repeated readers.
        let baseline = crate::persist::config_read_count_for_test();
        let installed = initialize(&literal("injected-ws"), None);
        assert_eq!(installed.name, "injected-ws");
        assert_eq!(
            active().name,
            "injected-ws",
            "initialize must update this scope"
        );
        assert_eq!(active().name, "injected-ws", "repeated reads stay stable");
        assert_eq!(active().backend.name(), "claude");
        assert_eq!(
            crate::persist::config_read_count_for_test(),
            baseline,
            "identity resolution must perform zero config reads"
        );

        // No config file exists anywhere under this fixture root — the literal
        // is the only source.
        assert!(!root.join("config").join("config.json").exists());
        assert!(
            ACTIVE.get().is_none(),
            "a support-build initialize must never write the production cache"
        );
    }

    /// The identity feeds `<base>/<workspace>/repository-<key>/…`, so a change
    /// in resolution order that altered this composition would silently
    /// relocate every managed worktree (T-08-11).
    #[test]
    fn managed_worktree_path_composition_is_unchanged() {
        let root = synthetic_root("composition");
        let _root = TestRedirect::new(&root);
        let _identity = override_for_test(&literal("compose-ws"), None);
        let base = root.join("data").join("baude").join("worktrees");
        assert_eq!(
            crate::git::managed_default_worktree_path(7, 11),
            base.join("compose-ws")
                .join("repository-7")
                .join("primary-11")
        );
        assert_eq!(
            crate::git::managed_branch_worktree_path(7, 12, "feature/a"),
            base.join("compose-ws")
                .join("repository-7")
                .join("feature-a-12")
        );
    }

    /// Env var selecting the child branch of the re-exec regression below.
    const SEEDED_CACHE_CHILD: &str = "BAUDE_TEST_SEEDED_WORKSPACE_CACHE_CHILD";

    /// A POPULATED production cache still cannot satisfy an override-free read
    /// (D-08). This is the bypass the thread-local override closes: a cached
    /// `Workspace` carries no fixture provenance, so "someone already
    /// initialized it" must not count as "this fixture owns it".
    ///
    /// It runs in an exact re-exec child with its own synthetic
    /// `HOME`/`XDG_*`/`CLAUDE_CONFIG_DIR` because seeding `ACTIVE` is
    /// irreversible: doing it in-process would hand every later case in this
    /// binary a cache state no other test asked for. The child seeds, probes
    /// under `catch_unwind`, and exits. The ordinary fixture constructor
    /// ([`override_for_test`]) never seeds `ACTIVE` at all.
    #[test]
    fn a_seeded_cache_is_not_observable_without_an_override() {
        if std::env::var_os(SEEDED_CACHE_CHILD).is_some() {
            let sentinel = resolve(
                Some("seeded-cache-sentinel"),
                None,
                &Config::default(),
                |_| {},
            );
            assert!(
                ACTIVE.set(sentinel).is_ok(),
                "this child must be the only writer of the cache"
            );
            assert_eq!(
                ACTIVE.get().map(|ws| ws.name.as_str()),
                Some("seeded-cache-sentinel"),
                "the regression needs the cache actually populated"
            );

            let hook = std::panic::take_hook();
            std::panic::set_hook(Box::new(|_| {}));
            let observed = std::panic::catch_unwind(|| active().name.clone());
            std::panic::set_hook(hook);

            let payload = observed
                .expect_err("a populated cache must not satisfy an override-free identity read");
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or("<non-string panic payload>");
            assert!(
                message.contains(IDENTITY_ESCAPE_PANIC_MARKER),
                "the escape must name workspace identity; got {message:?}"
            );
            return;
        }

        let root = std::env::temp_dir().join(format!(
            "baude-seeded-cache-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&root).expect("child fixture root");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "workspace::tests::a_seeded_cache_is_not_observable_without_an_override",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(SEEDED_CACHE_CHILD, "1")
            .env("BAUDE_TEST_FIXTURE_ROOT", &root)
            .env("HOME", &root)
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("CLAUDE_CONFIG_DIR", root.join("claude"))
            .env_remove("BAUDE_WORKSPACE")
            .env_remove("BAUDE_BACKEND")
            .output()
            .expect("re-exec the test binary");
        let status = output.status;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            status.success(),
            "seeded-cache child failed ({status})\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
        );
        assert!(
            stdout.contains("1 passed"),
            "the child must have RUN the case, not filtered it out:\n{stdout}"
        );
    }

    fn cfg(workspaces: &[(&str, WorkspaceConfig)]) -> Config {
        Config {
            workspaces: Some(
                workspaces
                    .iter()
                    .map(|(n, w)| (n.to_string(), w.clone()))
                    .collect::<HashMap<_, _>>(),
            ),
            ..Config::default()
        }
    }

    fn no_warn(msg: String) {
        panic!("unexpected warning: {msg}");
    }

    #[test]
    fn default_is_claude_workspace() {
        let ws = resolve(None, None, &Config::default(), no_warn);
        assert_eq!(ws.name, "claude");
        assert_eq!(ws.backend.name(), "claude");
        assert_eq!(ws.state_file("state"), "state-claude.json");
        assert_eq!(ws.legacy_state_file("state").as_deref(), Some("state.json"));
        assert_eq!(ws.auto_daemon_port(), Some(8642));
    }

    #[test]
    fn backend_env_lands_in_implicit_workspace() {
        // BAUDE_BACKEND=opencode alone → the `opencode` workspace: separated
        // state with zero new configuration.
        let ws = resolve(None, Some("opencode"), &Config::default(), no_warn);
        assert_eq!(ws.name, "opencode");
        assert_eq!(ws.backend.name(), "opencode");
        assert_eq!(ws.state_file("daemon-state"), "daemon-state-opencode.json");
        // Only claude has a legacy fallback.
        assert_eq!(ws.legacy_state_file("daemon-state"), None);
        assert_eq!(ws.auto_daemon_port(), Some(8643));
    }

    #[test]
    fn bound_workspace_wins_over_backend_env() {
        let cfg = cfg(&[(
            "oss",
            WorkspaceConfig {
                backend: Some("opencode".into()),
                daemon_port: Some(8650),
                ..Default::default()
            },
        )]);
        let mut warned = Vec::new();
        let ws = resolve(Some("oss"), Some("claude"), &cfg, |m| warned.push(m));
        assert_eq!(ws.name, "oss");
        assert_eq!(ws.backend.name(), "opencode", "binding must win");
        assert_eq!(ws.auto_daemon_port(), Some(8650));
        assert_eq!(warned.len(), 1, "conflict must warn: {warned:?}");
        assert!(warned[0].contains("ignoring BAUDE_BACKEND"));
    }

    #[test]
    fn custom_workspace_without_binding_follows_backend_chain() {
        let cfg = cfg(&[("scratch", WorkspaceConfig::default())]);
        let ws = resolve(Some("scratch"), Some("opencode"), &cfg, no_warn);
        assert_eq!(ws.name, "scratch");
        assert_eq!(ws.backend.name(), "opencode");
        // No implicit port for custom workspaces — auto_daemon must be
        // configured explicitly, never guessed.
        assert_eq!(ws.auto_daemon_port(), None);
    }

    #[test]
    fn unknown_names_sanitize_and_fail_safe() {
        let mut warned = Vec::new();
        let cfg = cfg(&[(
            "weird",
            WorkspaceConfig {
                backend: Some("codex".into()),
                ..Default::default()
            },
        )]);
        let ws = resolve(Some("weird"), None, &cfg, |m| warned.push(m));
        assert_eq!(ws.backend.name(), "claude", "unknown backend → claude");
        assert!(warned[0].contains("unknown backend"));
        // Path-hostile names can't escape the config dir.
        let ws = resolve(Some("../evil name"), None, &Config::default(), no_warn);
        assert_eq!(ws.name, "---evil-name");
        assert_eq!(ws.state_file("state"), "state----evil-name.json");
    }

    #[test]
    fn display_label_shows_platform_and_custom_name() {
        // Implicit workspaces read as just the platform product name —
        // "claude" the id renders as "Claude Code" the product.
        let ws = resolve(None, None, &Config::default(), no_warn);
        assert_eq!(ws.display_label(), "Claude Code");
        let ws = resolve(None, Some("opencode"), &Config::default(), no_warn);
        assert_eq!(ws.display_label(), "opencode");
        // Custom names show pool AND platform.
        let oss_cfg = cfg(&[(
            "oss",
            WorkspaceConfig {
                backend: Some("opencode".into()),
                ..Default::default()
            },
        )]);
        let ws = resolve(Some("oss"), None, &oss_cfg, no_warn);
        assert_eq!(ws.display_label(), "oss · opencode");
        let work_cfg = cfg(&[(
            "work",
            WorkspaceConfig {
                backend: Some("claude".into()),
                ..Default::default()
            },
        )]);
        let ws = resolve(Some("work"), None, &work_cfg, no_warn);
        assert_eq!(ws.display_label(), "work · Claude Code");
    }

    #[test]
    fn hint_outranks_config_defaults_but_never_env() {
        let config = Config {
            workspace: Some("work".into()),
            backend: Some("claude".into()),
            ..Config::default()
        };
        // Hint beats config `workspace` and config `backend`.
        let ws = resolve_with_hint(None, None, Some("opencode"), &config, no_warn);
        assert_eq!(ws.name, "opencode");
        assert_eq!(ws.backend.name(), "opencode");
        // BAUDE_WORKSPACE beats the hint.
        let ws = resolve_with_hint(Some("work"), None, Some("opencode"), &config, no_warn);
        assert_eq!(ws.name, "work");
        // BAUDE_BACKEND alone suppresses the hint entirely — the chain then
        // runs exactly as without one (config `workspace` outranks the env
        // backend name, as today).
        let ws = resolve_with_hint(None, Some("claude"), Some("opencode"), &config, no_warn);
        assert_eq!(ws.name, "work");
        let ws = resolve_with_hint(
            None,
            Some("opencode"),
            Some("scratch"),
            &Config::default(),
            no_warn,
        );
        assert_eq!(ws.name, "opencode");
    }

    #[test]
    fn no_hint_resolution_is_unchanged() {
        let config = Config {
            workspace: Some("work".into()),
            ..Config::default()
        };
        let with = resolve_with_hint(None, Some("opencode"), None, &config, |_| {});
        let without = resolve(None, Some("opencode"), &config, |_| {});
        assert_eq!(with.name, without.name);
        assert_eq!(with.backend.name(), without.backend.name());
    }

    #[test]
    fn stale_hint_fails_open_like_an_explicit_workspace() {
        // A hinted workspace whose config entry vanished still resolves: its
        // own namespace, default backend chain — same as BAUDE_WORKSPACE.
        let ws = resolve_with_hint(None, None, Some("gone"), &Config::default(), no_warn);
        assert_eq!(ws.name, "gone");
        assert_eq!(ws.backend.name(), "claude");
        // Hostile names sanitize instead of escaping the config dir.
        let ws = resolve_with_hint(None, None, Some("../evil"), &Config::default(), no_warn);
        assert_eq!(ws.name, "---evil");
    }

    #[test]
    fn undeclared_workspace_name_still_resolves() {
        // Using a workspace name with no config entry is fine — it gets its
        // own state namespace and the default backend chain.
        let ws = resolve(Some("side"), None, &Config::default(), no_warn);
        assert_eq!(ws.name, "side");
        assert_eq!(ws.backend.name(), "claude");
        assert_eq!(ws.legacy_state_file("state"), None);
    }
}
