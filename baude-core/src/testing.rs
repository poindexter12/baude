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
use std::time::{Duration, Instant};

use crate::pty::Pty;

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
    /// Replaces `persist::home_dir()`, the home a leading `~` expands to.
    /// `baude` and `bauded` expand `~` for a tab-completion `read_dir`, a
    /// `POST /sessions` repository path, and the destination a `git clone` is
    /// written into — all of them compiled into those crates' test harnesses.
    pub home_dir: Option<PathBuf>,
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
            home_dir: None,
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

/// The redirects live on the calling thread, as a value that can be moved to
/// another thread and installed there with [`TestRedirect::install`].
///
/// Redirects are thread-local: a thread the test starts sees none of them, so
/// a test that hands work to another thread must capture here, on its own
/// thread, and install on the other one before that thread resolves any path.
pub fn capture() -> Redirects {
    current()
}

/// `Redirects` must cross threads for [`capture`] and
/// [`TestRedirect::install`] to work.
const _: fn() = || {
    fn assert_send<T: Send>() {}
    assert_send::<Redirects>();
};

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
    /// | home dir | `<root>/home` |
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
            home_dir: Some(root.join("home")),
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

    /// Install `redirects`, taken with [`capture`] on another thread, on the
    /// calling thread, restoring the calling thread's previous set on drop.
    ///
    /// Redirects are thread-local, so a test that hands work to another
    /// thread, a `std::thread` or a `spawn_blocking` closure, must call
    /// [`capture`] on its own thread and `install` first thing on the other.
    /// A thread that skips the install resolves real user paths and panics
    /// with [`ESCAPE_PANIC_MARKER`]. Bind the guard to a named local, and let
    /// it drop on the thread that installed it.
    pub fn install(redirects: Redirects) -> Self {
        Self::swap(redirects)
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

/// The redirected home directory, if one is live on this thread.
pub fn home_dir_override() -> Option<PathBuf> {
    REDIRECTS.with(|cell| cell.borrow().home_dir.clone())
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

/// Run `git` with `args` in `dir`. Panics with git's stderr on a non-zero
/// status and returns trimmed stdout, so a fixture step that fails names the
/// command and the directory instead of leaving a half-built topology.
pub fn git_in(dir: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|error| panic!("run git {args:?} in {}: {error}", dir.display()));
    assert!(
        output.status.success(),
        "git {args:?} in {}: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

/// The topology real users have: an upstream, a clone whose main checkout
/// sits on a working branch, a managed default worktree on `main` and a
/// managed branch worktree, the two managed worktrees allocated under the
/// redirected managed root so the forced removal path accepts them.
///
/// Every path is the one [`crate::git::discover_repository`] reports, so a
/// test can compare it with what removal records without canonicalizing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClonedTopology {
    /// The repository the clone was made from.
    pub upstream: PathBuf,
    /// The clone's main checkout, switched to `chore/workspace`.
    pub clone: PathBuf,
    /// `<managed root>/default`, a linked worktree on `main`.
    pub default_worktree: PathBuf,
    /// `<managed root>/managed`, a linked worktree on `feature/kept`.
    pub managed_worktree: PathBuf,
    /// The clone's common dir, as discovery reports it.
    pub common_dir: PathBuf,
}

/// Build a [`ClonedTopology`] under `root`.
///
/// Requires an active [`TestRedirect`] on this thread and panics otherwise:
/// the managed worktrees are created under [`crate::git::worktrees_base`],
/// and without a redirect that would be the developer's real managed root,
/// which is the leak this module exists to prevent.
pub fn cloned_topology(root: &Path) -> ClonedTopology {
    assert!(
        worktrees_base_override().is_some(),
        "cloned_topology requires an active TestRedirect on this thread; without one the \
         managed worktrees would be created under the real managed root"
    );
    let upstream = root.join("upstream");
    std::fs::create_dir_all(&upstream).expect("create upstream");
    git_in(&upstream, &["init", "-q", "-b", "main"]);
    git_in(&upstream, &["config", "user.email", "test@example.com"]);
    git_in(&upstream, &["config", "user.name", "Test"]);
    std::fs::write(upstream.join("tracked"), b"one\n").expect("write tracked file");
    git_in(&upstream, &["add", "tracked"]);
    git_in(&upstream, &["commit", "-q", "-m", "initial"]);
    let clone = root.join("clone");
    git_in(
        root,
        &[
            "clone",
            "-q",
            upstream.to_str().expect("utf-8 upstream path"),
            clone.to_str().expect("utf-8 clone path"),
        ],
    );
    git_in(&clone, &["switch", "-q", "-c", "chore/workspace"]);
    let base = crate::git::worktrees_base();
    std::fs::create_dir_all(&base).expect("create managed root");
    let default = base.join("default");
    git_in(
        &clone,
        &[
            "worktree",
            "add",
            "-q",
            default.to_str().expect("utf-8 default path"),
            "main",
        ],
    );
    let managed = base.join("managed");
    git_in(
        &clone,
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature/kept",
            managed.to_str().expect("utf-8 managed path"),
        ],
    );
    let reported = |path: &Path| {
        crate::git::discover_repository(path)
            .expect("discover worktree")
            .selected_worktree
            .path
    };
    let snapshot = crate::git::discover_repository(&clone).expect("discover clone");
    ClonedTopology {
        upstream,
        clone: snapshot.main_worktree,
        default_worktree: reported(&default),
        managed_worktree: reported(&managed),
        common_dir: snapshot.common_dir,
    }
}

// ---- live-runtime doubles (phase 18 plan 08, shared with the TUI since phase 20) ----

/// Kills the out-of-group slave holder on drop so an assertion panic never
/// leaks a 30 s perl process.
pub struct HolderGuard(i32);

impl HolderGuard {
    /// The holder's pid, for a test that proves it exited after release.
    pub fn pid(&self) -> i32 {
        self.0
    }
}

impl Drop for HolderGuard {
    fn drop(&mut self) {
        // SAFETY: the pid was written by this fixture's own holder.
        unsafe {
            libc::kill(self.0, libc::SIGKILL);
        }
    }
}

/// The holder's process group as `ps` reports it, `None` once it has exited.
pub fn holder_pgid(holder: i32) -> Option<i32> {
    let out = std::process::Command::new("ps")
        .args(["-o", "pgid=", "-p", &holder.to_string()])
        .output()
        .expect("ps");
    String::from_utf8_lossy(&out.stdout).trim().parse().ok()
}

/// The shell command behind [`spawn_parked_reader_runtime`], exposed so a
/// caller that spawns its runtimes through its own path (the App's
/// `opencode_cmd`) produces the same shape: the leader backgrounds a perl
/// process that leaves the session with setsid, records its pid at
/// `holder_pid_path` and sleeps holding the inherited slave terminal, then
/// the leader exits.
///
/// The script is written beside the pid file and the command is `sh <script>`
/// rather than the script text itself, because a backend composes its spawn
/// as `exec <command> --port N ...`: an executable plus arguments survives
/// that, a bare `set +m; ...; exit 0` fragment does not.
pub fn parked_reader_command(holder_pid_path: &Path) -> String {
    let script_path = holder_pid_path.with_extension("sh");
    let script = format!(
        "set +m\ntrap '' HUP\nperl -MPOSIX -e 'my $sid = POSIX::setsid(); \
         exit 3 unless defined $sid && $sid != -1; \
         open(my $f, \">\", \"{}\"); print $f $$; close $f; sleep 30' &\nexit 0\n",
        holder_pid_path.display()
    );
    std::fs::write(&script_path, script).expect("write holder script");
    format!("sh {}", script_path.display())
}

/// Wait for the holder behind `pty` to record its pid and for the leader to
/// exit, then prove from `ps` that the holder sits outside the recorded
/// group. Returns the guard that kills the holder on drop. It reaps the
/// leader through `is_exited`, so a test of the exited, unreaped leader
/// window must not use it.
pub fn await_parked_holder(pty: &Pty, holder_pid_path: &Path) -> HolderGuard {
    let pgid = pty.process_identity().process_group;
    let deadline = Instant::now() + Duration::from_secs(5);
    let holder: i32 = loop {
        let written = std::fs::read_to_string(holder_pid_path)
            .ok()
            .and_then(|text| text.trim().parse::<i32>().ok());
        if let Some(holder) = written {
            if pty.is_exited() {
                break holder;
            }
        }
        assert!(
            Instant::now() < deadline,
            "holder.pid was not written or the leader did not exit \
             (pid file written: {}, leader exited: {}, pty output: {:?})",
            written.is_some(),
            pty.is_exited(),
            String::from_utf8_lossy(&pty.subscribe().0)
        );
        std::thread::sleep(Duration::from_millis(10));
    };
    let guard = HolderGuard(holder);
    let observed = holder_pgid(holder).expect("holder pgid");
    assert_ne!(
        observed, pgid,
        "setsid did not move the holder out of the recorded group"
    );
    guard
}

/// The plan 08 holder shape, spawned directly: see [`parked_reader_command`].
/// Returns once the holder is proven outside the recorded group and the
/// leader has exited.
pub fn spawn_parked_reader_runtime(cwd: &Path, holder_pid_path: &Path) -> (Pty, HolderGuard) {
    let _ = std::fs::remove_file(holder_pid_path);
    let command = parked_reader_command(holder_pid_path);
    let pty = Pty::spawn(Some(&command), cwd, 5, 40).expect("spawn parked-reader runtime");
    let guard = await_parked_holder(&pty, holder_pid_path);
    (pty, guard)
}

/// Arm a real parked-reader runtime so its next teardown fails the way a
/// group that outlived SIGKILL does, naming the surviving leader pid.
///
/// Nothing portable survives SIGKILL for real: every live member of the
/// group dies to it, and a zombie left in the group counts as a member on
/// Linux but is skipped by macOS's `killpg`, so a real survivor would read as
/// extinct on one platform and live on the other. The failure is therefore
/// the Phase 18 injection over a real process group: the first
/// `kill_and_wait` returns `Err` before any signal, and the retry after the
/// holder is released drives the real kill path over the same group.
#[cfg(debug_assertions)]
pub fn arm_unkillable_runtime(pty: &Pty, holder_pid_path: &Path) -> HolderGuard {
    let guard = await_parked_holder(pty, holder_pid_path);
    let identity = pty.process_identity();
    pty.fail_next_teardown_for_test(format!(
        "process group {} survived SIGKILL; leader pid {} still live",
        identity.process_group, identity.pid
    ));
    guard
}

/// A runtime whose first teardown fails naming its surviving pid, over a
/// real process group with a real out-of-group slave holder: see
/// [`arm_unkillable_runtime`] for why the survivor is injected.
#[cfg(debug_assertions)]
pub fn spawn_unkillable_runtime(cwd: &Path, holder_pid_path: &Path) -> (Pty, HolderGuard) {
    let _ = std::fs::remove_file(holder_pid_path);
    let command = parked_reader_command(holder_pid_path);
    let pty = Pty::spawn(Some(&command), cwd, 5, 40).expect("spawn unkillable runtime");
    let guard = arm_unkillable_runtime(&pty, holder_pid_path);
    (pty, guard)
}

// ---- fixture hygiene guards (phase 22.1) ----

/// A fixture directory under `temp_dir()`, removed when it drops, also when
/// the test panicked.
///
/// The path is `temp_dir()` verbatim, never canonicalized: tests rely on the
/// macOS `/var` symlink staying in the path they are handed.
pub struct FixtureRoot {
    path: PathBuf,
}

impl FixtureRoot {
    /// Create `temp_dir()/<prefix>-<pid>-<seq>`, removing a stale copy first.
    ///
    /// The pid keeps two test binaries apart and the process-wide sequence
    /// keeps two roots in one binary apart. `prefix` must start with `baude-`
    /// or `bauded-` and name a single path component; anything else panics
    /// here, so the drop can only ever remove a tree it created.
    pub fn new(prefix: &str) -> Self {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static SEQ: AtomicUsize = AtomicUsize::new(0);
        assert!(
            Self::is_fixture_name(prefix),
            "FixtureRoot prefix {prefix:?} must start with baude- or bauded- and name one \
             path component"
        );
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{}",
            std::process::id(),
            SEQ.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path)
            .unwrap_or_else(|error| panic!("create fixture root {}: {error}", path.display()));
        Self { path }
    }

    /// The root's path, under `temp_dir()` as returned.
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn is_fixture_name(name: &str) -> bool {
        (name.starts_with("baude-") || name.starts_with("bauded-"))
            && !name.contains('/')
            && !name.contains("..")
    }
}

impl std::ops::Deref for FixtureRoot {
    type Target = Path;

    fn deref(&self) -> &Path {
        &self.path
    }
}

impl AsRef<Path> for FixtureRoot {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

impl Drop for FixtureRoot {
    /// Remove the tree. `NotFound` is fine; on `PermissionDenied` the tree is
    /// made owner-writable and the removal retried once. Any other outcome is
    /// printed, never a panic: a drop that panics while the test is already
    /// unwinding aborts the whole suite.
    fn drop(&mut self) {
        let named = self
            .path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(Self::is_fixture_name);
        if !named {
            eprintln!(
                "FixtureRoot: refusing to remove {}, not a fixture name",
                self.path.display()
            );
            return;
        }
        let error = match std::fs::remove_dir_all(&self.path) {
            Ok(()) => return,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
            Err(error) if error.kind() == std::io::ErrorKind::PermissionDenied => {
                make_owner_writable(&self.path);
                match std::fs::remove_dir_all(&self.path) {
                    Ok(()) => return,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                    Err(error) => error,
                }
            }
            Err(error) => error,
        };
        eprintln!(
            "FixtureRoot: cannot remove {}: {error}",
            self.path.display()
        );
    }
}

/// Give the owner rwx on every directory under `path`, top down, so a tree a
/// test made read-only can be removed. Symlinks are never followed.
fn make_owner_writable(path: &Path) {
    use std::os::unix::fs::PermissionsExt;
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return;
    };
    if !meta.is_dir() {
        return;
    }
    let mut perms = meta.permissions();
    perms.set_mode(perms.mode() | 0o700);
    let _ = std::fs::set_permissions(path, perms);
    if let Ok(entries) = std::fs::read_dir(path) {
        for entry in entries.flatten() {
            make_owner_writable(&entry.path());
        }
    }
}

/// Kills what a fixture runtime left behind when it drops, also on panic.
///
/// The recorded process group alone is not enough: the PTY runs its command
/// under an interactive shell, whose job control puts the command in a
/// process group of its own, and a command that ignores HUP outlives its
/// leader there. What still names it is the session the PTY child created.
///
/// Pids are reused. Once the leader is reaped and its group is empty, the
/// kernel may give its pid to a stranger, and a stranger that calls setsid
/// then owns a group and a session with the recorded ids. So before it
/// signals anything the reaper checks the recorded leader's start time, as
/// `Pty::signal_verified` does, and leaves alone a pid that now names
/// another process.
pub struct RuntimeReaper {
    leader: i32,
    start_time: u64,
    process_group: i32,
    session: i32,
}

impl RuntimeReaper {
    /// A reaper for the runtime `identity` records.
    pub fn for_identity(identity: &crate::repository::ProcessIdentity) -> Self {
        Self {
            leader: identity.pid as i32,
            start_time: identity.start_time,
            process_group: identity.process_group,
            session: identity.session,
        }
    }

    /// Whether a live process holds pid `pid` and is not the recorded
    /// leader: a different pid than the leader's, or the leader's pid with
    /// another start time. A pid nobody holds is taken as not reused: a
    /// stranger's group or session with that id needs a process holding the
    /// pid to create it, so what the recorded ids name then is the runtime's
    /// own orphans. The one case this cannot tell apart is a stranger that
    /// took the pid, made the group or session and already exited. When the
    /// process cannot be inspected the answer is yes, so nothing is signalled
    /// on a guess.
    fn pid_names_a_stranger(&self, pid: i32) -> bool {
        match crate::session::inspect_process_identity(pid as u32) {
            Ok(None) => false,
            Ok(Some(live)) => pid != self.leader || live.start_time != self.start_time,
            Err(error) => {
                eprintln!("RuntimeReaper: cannot verify pid {pid}, signalling nothing: {error}");
                true
            }
        }
    }

    /// A reaper for `pty`'s recorded runtime. Build it right after the spawn,
    /// before the test kills the leader, and bind it after the fixture root so
    /// it drops first.
    pub fn for_pty(pty: &Pty) -> Self {
        Self::for_identity(pty.process_identity())
    }

    /// The recorded group may be signaled: never `kill(0)` or `kill(-1)`,
    /// which reach the caller's own group or every process, and never the
    /// harness's own group.
    fn group_is_signalable(&self) -> bool {
        // SAFETY: getpgrp only reads this process's own group id.
        self.process_group > 1 && self.process_group != unsafe { libc::getpgrp() }
    }

    /// The recorded session may be swept only when it is the recorded leader's
    /// own, which holds only for a PTY child that called setsid, and never
    /// when it is the session the harness itself runs in.
    fn sweeps_session(&self) -> bool {
        // SAFETY: getsid(0) only reads this process's own session id.
        self.leader > 1 && self.session == self.leader && self.session != unsafe { libc::getsid(0) }
    }

    /// Live pids of the recorded group and, when `sweep` holds, of the
    /// recorded session. Zombies count as gone; this process is never listed.
    /// `None` when `ps` could not be run.
    fn live_members(&self, group: bool, sweep: bool) -> Option<Vec<i32>> {
        let out = match std::process::Command::new("ps")
            .args(["-eo", "pid=,pgid=,stat="])
            .output()
        {
            Ok(out) => out,
            Err(error) => {
                eprintln!("RuntimeReaper: cannot run ps: {error}");
                return None;
            }
        };
        // SAFETY: getpid only reads this process's own id.
        let me = unsafe { libc::getpid() };
        let members = String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let pid: i32 = fields.next()?.parse().ok()?;
                let pgid: i32 = fields.next()?.parse().ok()?;
                let stat = fields.next().unwrap_or("");
                if pid <= 1 || pid == me || stat.starts_with('Z') {
                    return None;
                }
                let in_group = group && pgid == self.process_group;
                // SAFETY: getsid only reads the process table.
                let in_session = sweep && unsafe { libc::getsid(pid) } == self.session;
                (in_group || in_session).then_some(pid)
            })
            .collect();
        Some(members)
    }
}

impl Drop for RuntimeReaper {
    /// SIGKILL the recorded group, then every process in the recorded session
    /// when the leader owns it, then wait up to 5 s until none is left. It
    /// never calls `waitpid`, because the `Pty` owns its child, and it never
    /// panics: a survivor is printed and the drop returns.
    ///
    /// The group is signalled only while no stranger holds its id as a pid,
    /// and the session only while no stranger holds the leader's pid; both
    /// are checked again before every round of signals.
    fn drop(&mut self) {
        let group = self.group_is_signalable() && !self.pid_names_a_stranger(self.process_group);
        let sweep = self.sweeps_session() && !self.pid_names_a_stranger(self.session);
        if group {
            // SAFETY: the group id was recorded from this fixture's own leader
            // and is neither 0, 1 nor the harness's group. ESRCH and EPERM are
            // ignored: the group is already gone or no longer ours.
            unsafe {
                libc::kill(-self.process_group, libc::SIGKILL);
            }
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let Some(live) = self.live_members(group, sweep) else {
                return;
            };
            if live.is_empty() {
                return;
            }
            // The leader's pid can be handed out while this loop waits.
            if (group && self.pid_names_a_stranger(self.process_group))
                || (sweep && self.pid_names_a_stranger(self.session))
            {
                eprintln!(
                    "RuntimeReaper: pid {} now names another process; leaving group {} and \
                     session {} alone",
                    self.leader, self.process_group, self.session
                );
                return;
            }
            for pid in &live {
                // SAFETY: each pid is a live member of the recorded group or of
                // the recorded session its own leader created.
                unsafe {
                    libc::kill(*pid, libc::SIGKILL);
                }
            }
            if Instant::now() >= deadline {
                eprintln!(
                    "RuntimeReaper: pid(s) {live:?} of group {} or session {} still live after 5 s",
                    self.process_group, self.session
                );
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One `ps -eo pid=,pgid=,stat=` row per live process, zombies left out.
    fn live_rows() -> Vec<(i32, i32)> {
        let out = std::process::Command::new("ps")
            .args(["-eo", "pid=,pgid=,stat="])
            .output()
            .expect("ps");
        String::from_utf8_lossy(&out.stdout)
            .lines()
            .filter_map(|line| {
                let mut fields = line.split_whitespace();
                let pid = fields.next()?.parse().ok()?;
                let pgid = fields.next()?.parse().ok()?;
                let stat = fields.next().unwrap_or("");
                (!stat.starts_with('Z')).then_some((pid, pgid))
            })
            .collect()
    }

    /// Live processes whose session id is `session`, with their groups.
    fn live_session_members(session: i32) -> Vec<(i32, i32)> {
        live_rows()
            .into_iter()
            // SAFETY: getsid only reads the process table.
            .filter(|(pid, _)| unsafe { libc::getsid(*pid) } == session)
            .collect()
    }

    /// SIGKILLs each recorded pid when the test unwinds, so a failed
    /// assertion never leaks a 30 s process. A passing test signals nothing.
    struct KillOnPanic(Vec<i32>);

    impl Drop for KillOnPanic {
        fn drop(&mut self) {
            if std::thread::panicking() {
                for pid in &self.0 {
                    // SAFETY: each pid was recorded from a process this test spawned.
                    unsafe {
                        libc::kill(*pid, libc::SIGKILL);
                    }
                }
            }
        }
    }

    /// Remove a tree a failed assertion left, read-only parts included.
    fn force_remove(path: &Path) {
        make_owner_writable(path);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn fixture_root_is_removed_when_the_test_panics() {
        let seen = std::sync::Mutex::new(None::<PathBuf>);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let root = FixtureRoot::new("baude-testing-panic");
            std::fs::write(root.join("marker"), b"x").expect("write marker");
            *seen.lock().unwrap() = Some(root.path().to_path_buf());
            panic!("the fixture body failed");
        }));
        assert!(outcome.is_err(), "the body must have panicked");
        let path = seen.lock().unwrap().clone().expect("root path recorded");
        let remained = path.exists();
        if remained {
            force_remove(&path);
        }
        assert!(
            !remained,
            "fixture root {} still exists after a panicking test",
            path.display()
        );
    }

    #[test]
    fn fixture_root_paths_are_unique_and_prefixed() {
        let first = FixtureRoot::new("baude-testing-unique");
        let second = FixtureRoot::new("baude-testing-unique");
        assert_ne!(first.path(), second.path());
        for root in [&first, &second] {
            assert!(root.is_dir(), "{} was not created", root.display());
            assert_eq!(root.parent(), Some(std::env::temp_dir().as_path()));
            let name = root.file_name().and_then(|n| n.to_str()).unwrap_or("");
            assert!(
                name.starts_with("baude-testing-unique-"),
                "unexpected name {name}"
            );
        }
    }

    #[test]
    fn fixture_root_removes_a_read_only_subtree() {
        use std::os::unix::fs::PermissionsExt;
        let root = FixtureRoot::new("baude-testing-read-only");
        let path = root.path().to_path_buf();
        let locked = root.join("locked");
        std::fs::create_dir_all(&locked).expect("create locked dir");
        std::fs::write(locked.join("file"), b"x").expect("write file");
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o500))
            .expect("make read-only");
        drop(root);
        let remained = path.exists();
        if remained {
            force_remove(&path);
        }
        assert!(
            !remained,
            "fixture root {} with a read-only subtree still exists after drop",
            path.display()
        );
    }

    #[test]
    #[should_panic(expected = "must start with baude- or bauded-")]
    fn fixture_root_refuses_a_foreign_prefix() {
        let _root = FixtureRoot::new("other-x");
    }

    /// The real job-control shape: the interactive gate shell runs `sleep` in
    /// a process group of its own, `trap '' HUP` keeps it alive past its
    /// leader, and only the recorded session still names it.
    ///
    /// The trailing `true` is load-bearing: bash 5.2 execs the last command of
    /// a `-c` string in place, so a bare `...; sleep 30` makes `sleep` the
    /// session leader itself on Linux and the shape is never built. macOS's
    /// bash 3.2 forks it either way.
    #[test]
    fn runtime_reaper_kills_job_control_children_in_the_recorded_session() {
        let root = FixtureRoot::new("baude-testing-reaper-session");
        let _redirect = TestRedirect::new(&root);
        let mut pty =
            Pty::spawn(Some("trap '' HUP; sleep 30; true"), &root, 5, 40).expect("spawn runtime");
        let identity = pty.process_identity().clone();
        let reaper = RuntimeReaper::for_pty(&pty);
        assert_eq!(
            identity.pid as i32, identity.session,
            "the PTY child must lead its own session"
        );

        let deadline = Instant::now() + Duration::from_secs(5);
        let moved = loop {
            let outside = live_session_members(identity.session)
                .into_iter()
                .find(|(_, pgid)| *pgid != identity.process_group);
            if let Some((pid, _)) = outside {
                break pid;
            }
            assert!(
                Instant::now() < deadline,
                "no member of session {} outside group {} appeared within 5 s",
                identity.session,
                identity.process_group
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        let _cleanup = KillOnPanic(vec![moved]);
        // Let the trap arm before the leader dies.
        std::thread::sleep(Duration::from_millis(200));

        pty.kill();
        drop(reaper);

        let survivors = live_session_members(identity.session);
        assert!(
            survivors.is_empty(),
            "surviving pid(s) in recorded session {} after the reaper dropped: {survivors:?} \
             (job-control child was {moved})",
            identity.session
        );
    }

    /// The sweep is gated on the recorded session being the recorded leader:
    /// an identity naming somebody else's session must never sweep it.
    #[test]
    fn runtime_reaper_never_sweeps_a_session_it_does_not_lead() {
        use std::os::unix::process::CommandExt;

        let mut outsider = std::process::Command::new("perl")
            .args([
                "-MPOSIX",
                "-e",
                "my $sid = POSIX::setsid(); exit 3 unless defined $sid && $sid != -1; sleep 30",
            ])
            .spawn()
            .expect("spawn setsid sleeper");
        let outsider_pid = outsider.id() as i32;
        let mut named = std::process::Command::new("sleep")
            .arg("30")
            .process_group(0)
            .spawn()
            .expect("spawn named child");
        let named_pid = named.id() as i32;
        let _cleanup = KillOnPanic(vec![outsider_pid, named_pid]);

        let deadline = Instant::now() + Duration::from_secs(5);
        // SAFETY: getsid only reads the process table.
        while unsafe { libc::getsid(outsider_pid) } != outsider_pid {
            assert!(
                Instant::now() < deadline,
                "the perl sleeper never left the harness session"
            );
            std::thread::sleep(Duration::from_millis(10));
        }

        // The named child's real start time, so the group kill still reaches
        // it and only the session rule is under test here.
        let start_time = crate::session::inspect_process_identity(named_pid as u32)
            .expect("inspect the named child")
            .expect("the named child is live")
            .start_time;
        let identity = crate::repository::ProcessIdentity {
            pid: named_pid as u32,
            start_time,
            process_group: named_pid,
            session: outsider_pid,
        };
        drop(RuntimeReaper::for_identity(&identity));

        // SAFETY: signal zero is a liveness probe only.
        let alive = unsafe { libc::kill(outsider_pid, 0) } == 0
            && outsider.try_wait().expect("try_wait outsider").is_none();
        assert!(
            alive,
            "the reaper swept session {outsider_pid}, which its leader {named_pid} does not lead"
        );
        // SAFETY: the pid is this test's own perl child.
        unsafe {
            libc::kill(outsider_pid, libc::SIGKILL);
        }
        let _ = outsider.wait();
        let _ = named.wait();
    }

    /// WR-02: once the recorded leader is reaped, the kernel may hand its pid
    /// to a stranger, and a stranger that calls setsid, as every terminal tab
    /// and PTY child does, then owns a group and a session with the recorded
    /// ids. The reaper tells the two apart by start time, as
    /// `Pty::signal_verified` does, and signals neither. The stranger here is
    /// a live session leader with a job in a group of its own, under an
    /// identity whose start time is not its own.
    #[test]
    fn runtime_reaper_signals_nothing_when_the_leader_pid_was_reused() {
        let mut stranger = std::process::Command::new("perl")
            .args([
                "-MPOSIX",
                "-e",
                "my $sid = POSIX::setsid(); exit 3 unless defined $sid && $sid != -1; \
                 my $job = fork(); exit 4 unless defined $job; \
                 if ($job == 0) { POSIX::setpgid(0, 0); sleep 30; exit 0 } \
                 sleep 30",
            ])
            .spawn()
            .expect("spawn setsid stranger");
        let stranger_pid = stranger.id() as i32;
        let mut cleanup = KillOnPanic(vec![stranger_pid]);

        let deadline = Instant::now() + Duration::from_secs(5);
        let job = loop {
            // SAFETY: getsid only reads the process table.
            let led = unsafe { libc::getsid(stranger_pid) } == stranger_pid;
            let job = live_session_members(stranger_pid)
                .into_iter()
                .find(|(pid, pgid)| *pid != stranger_pid && *pgid == *pid);
            if let (true, Some((job, _))) = (led, job) {
                break job;
            }
            assert!(
                Instant::now() < deadline,
                "the stranger never led a session with a job in its own group"
            );
            std::thread::sleep(Duration::from_millis(10));
        };
        cleanup.0.push(job);

        let live = crate::session::inspect_process_identity(stranger_pid as u32)
            .expect("inspect the stranger")
            .expect("the stranger is live");
        assert_eq!(live.process_group, stranger_pid);
        assert_eq!(live.session, stranger_pid);
        let recorded = crate::repository::ProcessIdentity {
            pid: stranger_pid as u32,
            start_time: live.start_time.wrapping_add(1),
            process_group: stranger_pid,
            session: stranger_pid,
        };
        drop(RuntimeReaper::for_identity(&recorded));

        // SAFETY: signal zero is a liveness probe only.
        let leader_alive = unsafe { libc::kill(stranger_pid, 0) } == 0
            && stranger.try_wait().expect("try_wait stranger").is_none();
        let job_alive = live_session_members(stranger_pid)
            .iter()
            .any(|(pid, _)| *pid == job);
        assert!(
            leader_alive && job_alive,
            "WR-02 pin: the reaper signalled a reused pid's group or session: leader {stranger_pid} \
             alive {leader_alive}, job {job} alive {job_alive}"
        );
        for pid in [job, stranger_pid] {
            // SAFETY: each pid is this test's own perl child or its job.
            unsafe {
                libc::kill(pid, libc::SIGKILL);
            }
        }
        let _ = stranger.wait();
    }

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

    /// The text of a panic payload, for a `join` that returned `Err`.
    fn panic_text(payload: &(dyn std::any::Any + Send)) -> String {
        if let Some(text) = payload.downcast_ref::<String>() {
            return text.clone();
        }
        if let Some(text) = payload.downcast_ref::<&str>() {
            return (*text).to_string();
        }
        "a non-string panic payload".to_string()
    }

    fn is_empty(redirects: &Redirects) -> bool {
        redirects.worktrees_base.is_none()
            && redirects.config_dir.is_none()
            && redirects.claude_config_dir.is_none()
            && redirects.home_dir.is_none()
            && redirects.hook_command.is_none()
            && redirects.workspace.is_none()
    }

    /// Redirects are thread-local, so work handed to another thread escapes
    /// them unless that thread installs what this one captured. Both workers
    /// hold a `NoFixtureRoot`, so containment can only come from the install
    /// and the bare worker takes the escape path whatever the environment.
    #[test]
    fn installed_redirects_contain_a_worker_thread() {
        let root = FixtureRoot::new("baude-testing-install");
        let _redirect = TestRedirect::new(&root);
        let redirects = capture();

        let installed = std::thread::spawn(move || {
            let _no_root = NoFixtureRoot::new();
            let guard = TestRedirect::install(redirects);
            let resolved = (crate::git::worktrees_base(), crate::persist::config_dir());
            drop(guard);
            (resolved, is_empty(&current()))
        })
        .join();
        let ((worktrees, config), emptied) = installed.unwrap_or_else(|payload| {
            panic!(
                "the worker that installed the captured redirects escaped them: {}",
                panic_text(payload.as_ref())
            )
        });
        assert!(
            worktrees.starts_with(&*root),
            "worker worktrees base {} is outside the fixture root {}",
            worktrees.display(),
            root.display()
        );
        assert!(
            config.starts_with(&*root),
            "worker config dir {} is outside the fixture root {}",
            config.display(),
            root.display()
        );
        assert!(
            emptied,
            "the worker's redirects were not empty after its installed guard dropped"
        );

        let bare = std::thread::spawn(|| {
            let _no_root = NoFixtureRoot::new();
            crate::git::worktrees_base()
        })
        .join();
        let payload = match bare {
            Ok(escaped) => panic!(
                "a worker without the install resolved {} instead of panicking",
                escaped.display()
            ),
            Err(payload) => payload,
        };
        let text = panic_text(payload.as_ref());
        assert!(
            text.contains(ESCAPE_PANIC_MARKER),
            "the bare worker panicked without the escape marker: {text}"
        );
    }
}
