//! Fixture redirects and the real-path escape guard.
//!
//! This module exists because the suite leaked: 1433 orphaned
//! `repository-<n>` directories accumulated in the developer's real
//! `~/.local/share/baude/worktrees`, plus a real VAPID keypair in
//! `~/.config/baude` (#72). Every redirect a fixture needs now lives here, in
//! one guard, so there is exactly one place to arm and one place to reset.
//!
//! # Why a cargo feature and not `#[cfg(test)]`
//!
//! The `test` cfg is set **per crate** by `rustc --test`. When cargo builds
//! `baude`'s or `bauded`'s test harness, `baude-core` is compiled as an
//! ordinary library with `test` false — so a `#[cfg(test)]`-only guard in this
//! crate would be absent from exactly the two binaries that produced the leak.
//! The module is therefore gated on `cfg(any(test, feature = "test-support"))`:
//! `test` covers `baude-core`'s own harness, and the feature — enabled from the
//! downstream crates' `[dev-dependencies]` — covers theirs. Resolver v2 leaves
//! a dev-dependency feature unselected for `cargo build --release`, so nothing
//! here reaches a shipped binary.
//!
//! # Containment, not override-presence
//!
//! [`assert_contained`] asks "did this resolve inside the fixture root?", not
//! "was an override set?". That distinction is load-bearing in two directions.
//! It needs no arming state at all, so the guard is a property of the compiled
//! binary rather than of execution history and holds for the FIRST test in a
//! binary as much as the thousandth. And it lets the re-exec'd dogfood child
//! pass: that child is isolated by `HOME`/`XDG_*` rather than by thread-locals,
//! so every real path it resolves already lands inside
//! `BAUDE_TEST_FIXTURE_ROOT`.
//!
//! # Not a replacement for explicit-root APIs
//!
//! [`crate::persist`]'s `*_at` functions and `persistence_root_for_test` take an
//! explicit root and are deliberately NOT folded into this module. They serve a
//! different job: the redirect provides *ambient containment* for a whole
//! fixture, the explicit root provides *deliberate failure injection* for tests
//! that need to point one call at a path of their choosing. Do not unify them.

use std::cell::{Cell, RefCell};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::OnceLock;

/// Every path/identity a fixture redirects, held as one value so a guard can
/// restore the whole set wholesale.
#[derive(Default, Clone)]
pub struct Redirects {
    /// Base handed to [`crate::git`]'s managed-worktree resolver, which appends
    /// `baude/worktrees` exactly as it does for the real data dir.
    pub worktrees_base: Option<PathBuf>,
    /// Replaces `persist::config_base()` outright — config.json, the state
    /// files, and their locks all live in this one directory.
    pub config_dir: Option<PathBuf>,
    /// Replaces `meta::claude_config_dir()` (consumed from plan 02).
    pub claude_config_dir: Option<PathBuf>,
    /// Replaces the `current_exe()`-derived hook command.
    pub hook_command: Option<String>,
    /// Per-fixture workspace identity (consumed from plan 03).
    pub workspace: Option<&'static crate::workspace::Workspace>,
}

impl Redirects {
    const fn empty() -> Self {
        Self {
            worktrees_base: None,
            config_dir: None,
            claude_config_dir: None,
            hook_command: None,
            workspace: None,
        }
    }
}

thread_local! {
    /// Thread-local, not env vars: the harness runs cases in parallel, and a
    /// process-wide `XDG_DATA_HOME` written by one case would decide where
    /// another one's worktrees land (rationale recorded in commit `725c558`).
    static REDIRECTS: RefCell<Redirects> = const { RefCell::new(Redirects::empty()) };

    /// Set by [`NoFixtureRoot`]. Thread-local so an escape test can observe the
    /// "no fixture root" condition without changing what any concurrently
    /// running test sees.
    static SUPPRESS_FIXTURE_ROOT: Cell<bool> = const { Cell::new(false) };
}

fn current() -> Redirects {
    REDIRECTS.with(|cell| cell.borrow().clone())
}

/// Redirects every fixture-owned path for the CURRENT THREAD, restoring the
/// previous set on drop.
///
/// Scopes nest: an inner guard sees the outer values it did not change and the
/// outer set returns when it drops.
#[must_use = "a TestRedirect restores the previous redirects the moment it drops; bind it to a \
              named local (`let _redirect = TestRedirect::new(&root);`) so the fixture stays \
              redirected for the whole test body — an unbound construction arms and disarms in \
              the same statement and silently un-redirects the fixture"]
pub struct TestRedirect {
    previous: Redirects,
    /// Thread-affine: restoring on another thread would write the wrong
    /// thread's redirects.
    _not_send: PhantomData<Rc<()>>,
}

impl TestRedirect {
    /// Derive every redirect from one fixture root:
    ///
    /// | Field | Value |
    /// |---|---|
    /// | worktrees base | `<root>/data` (the resolver appends `baude/worktrees`) |
    /// | config dir | `<root>/config` |
    /// | claude config dir | `<root>/claude` |
    /// | hook command | `<root>/bin/baude hook` |
    ///
    /// The hook command keeps the production shape — an absolute path whose
    /// file stem is `baude` — so what a fixture seeds is what baude really
    /// writes (see [`crate::hook::is_seeded_hook_command`]).
    pub fn new(root: impl AsRef<Path>) -> Self {
        let root = root.as_ref();
        Self::swap(Redirects {
            worktrees_base: Some(root.join("data")),
            config_dir: Some(root.join("config")),
            claude_config_dir: Some(root.join("claude")),
            hook_command: Some(format!("{} hook", root.join("bin").join("baude").display())),
            workspace: None,
        })
    }

    /// Change only the hook command, preserving the enclosing scope's root and
    /// identity. Restores the whole enclosing set on drop.
    pub fn with_hook_command(command: impl Into<String>) -> Self {
        let mut next = current();
        next.hook_command = Some(command.into());
        Self::swap(next)
    }

    /// Change only the workspace identity, preserving the enclosing scope's
    /// paths. Plan 03's literal-config fixture returns this same guard type
    /// rather than a second redirect store.
    pub fn with_workspace(workspace: &'static crate::workspace::Workspace) -> Self {
        let mut next = current();
        next.workspace = Some(workspace);
        Self::swap(next)
    }

    fn swap(next: Redirects) -> Self {
        let previous = REDIRECTS.with(|cell| cell.replace(next));
        Self {
            previous,
            _not_send: PhantomData,
        }
    }
}

impl Drop for TestRedirect {
    fn drop(&mut self) {
        let previous = std::mem::take(&mut self.previous);
        REDIRECTS.with(|cell| *cell.borrow_mut() = previous);
    }
}

/// Replace ONLY the workspace identity inside an existing identity scope.
///
/// Requires a [`TestRedirect::with_workspace`] guard to be live on this thread:
/// injected initialization must never implicitly arm an override or write the
/// production cache.
///
/// Consumed by support-build [`crate::workspace::initialize`], which resolves a
/// literal config into the CURRENT fixture scope. Restoration stays with the
/// already-held guard, so a re-initializing fixture still hands the enclosing
/// identity back on drop.
pub(crate) fn replace_workspace_override(workspace: &'static crate::workspace::Workspace) {
    REDIRECTS.with(|cell| {
        let mut redirects = cell.borrow_mut();
        assert!(
            redirects.workspace.is_some(),
            "replace_workspace_override requires a live identity scope; hold a \
             baude_core::testing::TestRedirect::with_workspace guard on this thread first"
        );
        redirects.workspace = Some(workspace);
    });
}

/// The redirected managed-worktree base, if one is live on this thread.
pub fn worktrees_base_override() -> Option<PathBuf> {
    REDIRECTS.with(|cell| cell.borrow().worktrees_base.clone())
}

/// The redirected config directory, if one is live on this thread.
pub fn config_dir_override() -> Option<PathBuf> {
    REDIRECTS.with(|cell| cell.borrow().config_dir.clone())
}

/// The redirected `~/.claude` directory, if one is live on this thread.
pub fn claude_config_dir_override() -> Option<PathBuf> {
    REDIRECTS.with(|cell| cell.borrow().claude_config_dir.clone())
}

/// The redirected hook command, if one is live on this thread.
pub fn hook_command_override() -> Option<String> {
    REDIRECTS.with(|cell| cell.borrow().hook_command.clone())
}

/// The redirected workspace identity, if one is live on this thread.
pub fn workspace_override() -> Option<&'static crate::workspace::Workspace> {
    REDIRECTS.with(|cell| cell.borrow().workspace)
}

/// The process-wide fixture root, resolved from `BAUDE_TEST_FIXTURE_ROOT`
/// exactly once.
///
/// Caching is sound because nothing in this repo calls `set_var` on that
/// variable in-process: the only writer is the dogfood test, which sets it on a
/// child `Command`'s environment, and the child reads it at start-up. Caching
/// also removes any temptation to mutate it later.
fn fixture_root() -> Option<PathBuf> {
    static FIXTURE_ROOT: OnceLock<Option<PathBuf>> = OnceLock::new();
    if SUPPRESS_FIXTURE_ROOT.with(Cell::get) {
        return None;
    }
    FIXTURE_ROOT
        .get_or_init(|| std::env::var_os("BAUDE_TEST_FIXTURE_ROOT").map(PathBuf::from))
        .clone()
}

/// Makes [`fixture_root`] report `None` for the CURRENT THREAD only.
///
/// Its only legitimate use is a `#[should_panic]` test asserting that an
/// unguarded real-path resolution aborts. A fixture that holds one is defeating
/// the phase.
///
/// It is thread-local on purpose: `BAUDE_TEST_FIXTURE_ROOT` is process-wide, so
/// *clearing* it to observe the escape path would change what every
/// concurrently running test sees, and a mutex could not fix that because
/// non-participating tests never take the lock.
#[must_use = "a NoFixtureRoot restores the previous flag the moment it drops; bind it to a named \
              local so the escape condition holds for the resolution under test"]
pub struct NoFixtureRoot {
    previous: bool,
    _not_send: PhantomData<Rc<()>>,
}

impl NoFixtureRoot {
    /// Suppress the fixture-root exemption until this guard drops.
    pub fn new() -> Self {
        let previous = SUPPRESS_FIXTURE_ROOT.with(|flag| flag.replace(true));
        Self {
            previous,
            _not_send: PhantomData,
        }
    }
}

impl Default for NoFixtureRoot {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for NoFixtureRoot {
    fn drop(&mut self) {
        let previous = self.previous;
        SUPPRESS_FIXTURE_ROOT.with(|flag| flag.set(previous));
    }
}

/// Substring every escape panic carries, so `#[should_panic(expected = …)]`
/// tests pin the guard rather than any panic that happens to occur.
pub const ESCAPE_PANIC_MARKER: &str = "resolved to the real user path";

/// Abort the current test unless `resolved` lands inside the fixture root.
///
/// Panics rather than returning a `Result` (D-10): a test that reached the real
/// data dir has already failed, and a `Result` would let call sites swallow it.
pub(crate) fn assert_contained(resolved: &Path, what: &str) {
    match fixture_root() {
        None => panic!(
            "{what} {ESCAPE_PANIC_MARKER} {} during a test; hold a \
             baude_core::testing::TestRedirect on this thread, or set \
             BAUDE_TEST_FIXTURE_ROOT for a re-exec'd child process",
            resolved.display()
        ),
        Some(root) => assert!(
            resolved.starts_with(&root),
            "{what} resolved to {}, which escapes the fixture root {}",
            resolved.display(),
            root.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Declared FIRST in this module on purpose, and it constructs no
    /// [`TestRedirect`] at all: that is what proves the guard is armed from the
    /// first instruction of the binary rather than by some earlier fixture's
    /// side effect (D-09). It mutates no environment variable — the probe is
    /// thread-local, so this needs no mutex, no serial flag, and no
    /// `--test-threads=1`.
    #[test]
    #[should_panic(expected = "resolved to the real user path")]
    fn unguarded_resolution_panics() {
        let _no_root = NoFixtureRoot::new();
        let _escaped = crate::git::worktrees_base();
    }

    #[test]
    fn redirect_contains_the_managed_worktree_root() {
        let root = PathBuf::from("/nonexistent/baude-testing-worktrees");
        let _redirect = TestRedirect::new(&root);
        assert_eq!(
            crate::git::worktrees_base(),
            root.join("data").join("baude").join("worktrees")
        );
    }

    #[test]
    fn nested_redirects_restore_the_outer_root() {
        let outer = PathBuf::from("/nonexistent/baude-testing-outer");
        let inner = PathBuf::from("/nonexistent/baude-testing-inner");
        let _outer = TestRedirect::new(&outer);
        assert_eq!(config_dir_override(), Some(outer.join("config")));
        {
            let _inner = TestRedirect::new(&inner);
            assert_eq!(
                crate::git::worktrees_base(),
                inner.join("data").join("baude").join("worktrees")
            );
            assert_eq!(config_dir_override(), Some(inner.join("config")));
        }
        assert_eq!(
            crate::git::worktrees_base(),
            outer.join("data").join("baude").join("worktrees")
        );
        assert_eq!(config_dir_override(), Some(outer.join("config")));
    }

    /// A nested `with_hook_command` scope changes only the command and restores
    /// the whole enclosing set on drop — the shape the second-install
    /// reconciliation regression needs.
    #[test]
    fn nested_hook_command_restores_the_outer_command() {
        let root = PathBuf::from("/nonexistent/baude-testing-hook");
        let _redirect = TestRedirect::new(&root);
        let original = format!("{} hook", root.join("bin").join("baude").display());
        assert_eq!(crate::hook::baude_hook_command(), original);

        let newer = format!("{} hook", root.join("bin2").join("baude").display());
        {
            let _newer = TestRedirect::with_hook_command(&newer);
            assert_eq!(crate::hook::baude_hook_command(), newer);
            assert_ne!(newer, original, "the second install must be distinct");
            assert_eq!(
                config_dir_override(),
                Some(root.join("config")),
                "a hook-only scope must not disturb the config redirect"
            );
            assert_eq!(
                worktrees_base_override(),
                Some(root.join("data")),
                "a hook-only scope must not disturb the worktrees redirect"
            );
        }
        assert_eq!(crate::hook::baude_hook_command(), original);
    }

    /// The scanner plan 04 builds is production code that must reach the REAL
    /// root, so this resolver carries no guard and no redirect.
    #[test]
    fn real_worktrees_base_is_never_redirected() {
        let root = PathBuf::from("/nonexistent/baude-testing-real");
        let _redirect = TestRedirect::new(&root);
        let real = crate::git::real_worktrees_base();
        assert!(
            !real.starts_with(&root),
            "real_worktrees_base must ignore the redirect; got {}",
            real.display()
        );
        assert!(real.ends_with("baude/worktrees"), "got {}", real.display());
    }
}
