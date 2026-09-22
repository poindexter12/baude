use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

use crate::repository::ProcessIdentity;

/// Milliseconds since program start. Monotonic clock shared by all sessions.
pub fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

type Subscribers = Arc<Mutex<Vec<std::sync::mpsc::Sender<Vec<u8>>>>>;

/// Intermediate state held during paused PTY spawn, before gate token is written.
struct PausedPtyParts {
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    reader: Box<dyn Read + Send>,
    writer: Box<dyn Write + Send>,
    master: Box<dyn MasterPty + Send>,
    identity: ProcessIdentity,
    rows: u16,
    cols: u16,
}

/// A paused PTY child, held until its identity is durably recorded.
/// Holds the child process behind the stdin gate; call release() to write the
/// gate token and activate the child, or abort() to kill it.
pub struct PausedPty {
    parts: Option<PausedPtyParts>,
    identity: ProcessIdentity,
}

impl PausedPty {
    /// Returns the exact ProcessIdentity of the paused child.
    pub fn identity(&self) -> &ProcessIdentity {
        &self.identity
    }

    /// Release the gate immediately: the command starts now.
    pub fn release(self) -> Result<Pty> {
        let mut pty = self.into_gated()?;
        pty.release_gate()?;
        Ok(pty)
    }

    /// Turn the paused handle into a live `Pty` WITHOUT releasing the gate. The
    /// reader thread starts (the gate shell prints nothing until the token
    /// arrives), so the session can be constructed and shown while its record
    /// is still being made durable; `Pty::release_gate` starts the command.
    pub fn into_gated(mut self) -> Result<Pty> {
        let parts = self
            .parts
            .take()
            .ok_or_else(|| anyhow::anyhow!("pty already released"))?;
        let writer = parts.writer;
        let parser = Arc::new(Mutex::new(vt100::Parser::new(parts.rows, parts.cols, 2000)));
        let last_output_ms = Arc::new(AtomicU64::new(now_ms()));
        let exited = Arc::new(AtomicBool::new(false));
        let subscribers: Subscribers = Arc::new(Mutex::new(Vec::new()));
        let screen_generation = Arc::new(AtomicU64::new(0));

        // Spawn the reader thread that will process output from the child
        {
            let parser = Arc::clone(&parser);
            let last_output_ms = Arc::clone(&last_output_ms);
            let screen_gen = Arc::clone(&screen_generation);
            let exited = Arc::clone(&exited);
            let subscribers = Arc::clone(&subscribers);
            let mut reader = parts.reader;

            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => {
                            exited.store(true, Ordering::Relaxed);
                            break;
                        }
                        Ok(n) => {
                            if let Ok(mut p) = parser.lock() {
                                p.process(&buf[..n]);
                                if let Ok(mut subs) = subscribers.lock() {
                                    subs.retain(|s| s.send(buf[..n].to_vec()).is_ok());
                                }
                            }
                            last_output_ms.store(now_ms(), Ordering::Relaxed);
                            screen_gen.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            });
        }

        Ok(Pty {
            parser,
            master: parts.master,
            writer,
            child: parts.child,
            identity: parts.identity,
            last_output_ms,
            screen_generation,
            exited,
            size: (parts.rows, parts.cols),
            subscribers,
            gated: true,
        })
    }

    /// Abort the paused child (kill and reap without releasing).
    pub fn abort(mut self) {
        if let Some(parts) = self.parts.take() {
            if let Ok(mut child) = parts.child.lock() {
                let _ = child.kill();
                let _ = child.wait();
            }
        }
    }
}

/// One embedded terminal: a PTY with a child process and a vt100 screen model.
pub struct Pty {
    pub parser: Arc<Mutex<vt100::Parser>>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    identity: ProcessIdentity,
    pub last_output_ms: Arc<AtomicU64>,
    pub screen_generation: Arc<AtomicU64>,
    exited: Arc<AtomicBool>,
    size: (u16, u16), // (rows, cols)
    /// Live raw-output subscribers (remote attach). Pruned on send failure.
    subscribers: Subscribers,
    /// True while the child still waits behind the registration gate. A gated
    /// Pty is live (reader thread running, writer held) but the command has
    /// not started; `release_gate` writes the token that starts it.
    gated: bool,
}

/// Written to the paused child's stdin once its identity is durably recorded.
const GATE_TOKEN: &str = "baude-runtime-registered";

/// The paused gate, then the user's interactive LOGIN shell — production.
///
/// `-il` is what makes a session usable: PATH from `.zshrc`/`.zprofile` (mise,
/// homebrew) is how the configured `claude` command is found at all.
#[cfg(not(any(test, feature = "test-support")))]
const GATE_SCRIPT: &str = "IFS= read -r gate || exit 125; [ \"$gate\" = \"$BAUDE_GATE_TOKEN\" ] || exit 126; if [ \"$BAUDE_GATE_MODE\" = command ]; then exec \"$BAUDE_GATE_SHELL\" -il -c \"$BAUDE_GATE_COMMAND\"; else exec \"$BAUDE_GATE_SHELL\" -il; fi";

/// The same paused gate, then an explicit shell with NO startup files.
///
/// `-il` is the escape this phase exists to close: a login shell sources the
/// developer's `.zprofile`/`.zshrc` — reading `~/.claude`, exporting API keys,
/// running `mise`, whatever the machine happens to do — BEFORE the fixture's
/// command runs, and no Rust-side redirect can intercept that. `--noprofile
/// --norc -i` keeps the interactive PTY semantics the registration handshake
/// and idle detection rely on while reading nothing.
#[cfg(any(test, feature = "test-support"))]
const GATE_SCRIPT: &str = "IFS= read -r gate || exit 125; [ \"$gate\" = \"$BAUDE_GATE_TOKEN\" ] || exit 126; if [ \"$BAUDE_GATE_MODE\" = command ]; then exec \"$BAUDE_GATE_SHELL\" --noprofile --norc -i -c \"$BAUDE_GATE_COMMAND\"; else exec \"$BAUDE_GATE_SHELL\" --noprofile --norc -i; fi";

/// The gate shell for fixtures: named explicitly rather than inherited, because
/// `$SHELL` is exactly the value a developer's environment supplies.
#[cfg(any(test, feature = "test-support"))]
const TEST_GATE_SHELL: &str = "/bin/bash";

/// Every root a support-build PTY child is allowed to see, resolved on the
/// CALLING thread — the only thread that holds the fixture's redirects.
#[cfg(any(test, feature = "test-support"))]
struct TestChildRoots {
    home: std::path::PathBuf,
    config: std::path::PathBuf,
    data: std::path::PathBuf,
    state: std::path::PathBuf,
    cache: std::path::PathBuf,
    claude: std::path::PathBuf,
}

#[cfg(any(test, feature = "test-support"))]
impl TestChildRoots {
    /// Resolve through the GUARDED resolvers, which panic when this thread
    /// holds neither a redirect nor a fixture root (D-09/D-10).
    ///
    /// The dogfood child satisfies this without a thread-local: it is launched
    /// with already-contained `HOME`/`XDG_*`/`CLAUDE_CONFIG_DIR`, so the real
    /// resolvers land inside `BAUDE_TEST_FIXTURE_ROOT` and the containment
    /// assertion passes (D-17). Declaring a root is not enough — it has to be
    /// contained.
    fn resolve() -> Self {
        let config_dir = crate::persist::config_dir();
        let claude = crate::meta::claude_config_dir();
        Self {
            home: config_dir.join("child-home"),
            config: config_dir.join("child-config"),
            data: config_dir.join("child-data"),
            state: config_dir.join("child-state"),
            cache: config_dir.join("child-cache"),
            claude,
        }
    }

    /// Create only these directories.
    ///
    /// A `HOME` that does not exist is not itself a containment failure:
    /// `portable_pty`'s `CommandBuilder::get_home_dir` returns the builder's own
    /// `HOME` whenever that key is *present* and consults the passwd database
    /// only when it is unset, and [`configure_test_child`] always sets it. So
    /// the child would get a `HOME` that merely does not resolve — no escape.
    ///
    /// It is still a broken fixture, and it aborts here for the same reason
    /// every other guard in this phase does: the alternative is a confusing
    /// child-side failure (a tool that cannot write its own config) several
    /// layers away from the call that could not build the directory (#72,
    /// WR-03).
    fn create(&self) {
        for dir in [
            &self.home,
            &self.config,
            &self.data,
            &self.state,
            &self.cache,
            &self.claude,
        ] {
            std::fs::create_dir_all(dir).unwrap_or_else(|error| {
                panic!(
                    "fixture child root {} could not be created: {error}",
                    dir.display()
                )
            });
        }
    }
}

/// Replace the child's environment wholesale with a fixture-owned one.
///
/// Every `Pty::spawn*` entry point funnels through `build_gate_command`, so
/// this is the single place child-environment policy lives: `App`, `Manager`
/// and direct PTY tests are all covered without any caller repeating it.
///
/// The ordering rule this follows is stated once, on [`build_gate_command`],
/// because both branches obey it.
#[cfg(any(test, feature = "test-support"))]
fn configure_test_child(cmd: &mut CommandBuilder, env: &[(String, String)], command: Option<&str>) {
    let roots = TestChildRoots::resolve();
    roots.create();

    // Not "override the interesting keys": the ambient environment is not
    // copied at all. An inherited `ANTHROPIC_*`, `CCUSAGE_*` or NODE config
    // would otherwise reach the child through a name this policy never listed.
    cmd.env_clear();

    for (key, value) in env {
        cmd.env(key, value);
    }

    cmd.env("HOME", &roots.home);
    cmd.env("XDG_CONFIG_HOME", &roots.config);
    cmd.env("XDG_DATA_HOME", &roots.data);
    cmd.env("XDG_STATE_HOME", &roots.state);
    cmd.env("XDG_CACHE_HOME", &roots.cache);
    cmd.env("CLAUDE_CONFIG_DIR", &roots.claude);
    cmd.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    // A redirected HOME alone is insufficient: `ZDOTDIR` overrides it for zsh,
    // and `ENV`/`BASH_ENV` are sourced by sh/bash regardless of HOME.
    cmd.env("ZDOTDIR", &roots.home);
    cmd.env("ENV", "/dev/null");
    cmd.env("BASH_ENV", "/dev/null");
    // `portable_pty` consults the builder's own `SHELL` when it materializes
    // the command, so pinning it here also pins what it resolves.
    cmd.env("SHELL", TEST_GATE_SHELL);
    cmd.env("BAUDE_GATE_TOKEN", GATE_TOKEN);
    cmd.env("BAUDE_GATE_SHELL", TEST_GATE_SHELL);
    cmd.env("BAUDE_GATE_MODE", gate_mode(command));
    cmd.env("BAUDE_GATE_COMMAND", command.unwrap_or_default());
}

fn gate_mode(command: Option<&str>) -> &'static str {
    if command.is_some() {
        "command"
    } else {
        "interactive"
    }
}

/// Assemble the gate command, including (in support builds) the child's whole
/// environment. Separated from the spawn so it can run — and abort — before any
/// PTY exists, and so a test can inspect the finished map deterministically.
///
/// **Order is the policy, in both branches.** The caller's explicit env goes in
/// FIRST so opaque launch-plan values (resume ids and the like) reach the child,
/// and the protected root/shell/gate keys go in LAST so no caller — and no
/// inherited value — can name a root or a startup file.
///
/// The production branch used to do the reverse, which mattered: `GATE_SCRIPT`
/// execs `"$BAUDE_GATE_SHELL" -il -c "$BAUDE_GATE_COMMAND"`, so a launch-plan
/// entry named `BAUDE_GATE_COMMAND` or `BAUDE_GATE_SHELL` replaced the command
/// the gate was built to run, and one named `BAUDE_GATE_TOKEN` broke the
/// handshake. `env` comes from backend launch plans derived from `config.json`
/// rather than from any request body, so that was defense in depth rather than a
/// remote vector — but it contradicted the invariant stated three lines away
/// from it, and the daemon's config is exactly the sort of thing that grows a
/// remote-write endpoint later (#72, WR-04).
fn build_gate_command(
    command: Option<&str>,
    env: &[(String, String)],
    cwd: &Path,
) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", GATE_SCRIPT]);
    cmd.cwd(cwd);

    #[cfg(not(any(test, feature = "test-support")))]
    {
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        for (key, value) in env {
            cmd.env(key, value);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("BAUDE_GATE_TOKEN", GATE_TOKEN);
        cmd.env("BAUDE_GATE_SHELL", &shell);
        cmd.env("BAUDE_GATE_MODE", gate_mode(command));
        cmd.env("BAUDE_GATE_COMMAND", command.unwrap_or_default());
    }
    #[cfg(any(test, feature = "test-support"))]
    configure_test_child(&mut cmd, env, command);

    cmd
}

impl Pty {
    /// Spawn `command` under the user's shell (interactive login, so PATH from
    /// .zshrc/.zprofile — mise, homebrew — is available) inside a new PTY.
    pub fn spawn(command: Option<&str>, cwd: &Path, rows: u16, cols: u16) -> Result<Pty> {
        Self::spawn_with_env(command, &[], cwd, rows, cols)
    }

    /// Spawn with opaque environment values attached directly to the child
    /// process rather than interpolated into the shell command text.
    pub fn spawn_with_env(
        command: Option<&str>,
        env: &[(String, String)],
        cwd: &Path,
        rows: u16,
        cols: u16,
    ) -> Result<Pty> {
        Self::spawn_registered_with(command, env, cwd, rows, cols, |_| Ok(()))
    }

    /// Spawn a PTY child in paused state, held at the stdin gate.
    /// The child's identity is available immediately via paused.identity(),
    /// allowing registration before the child is released.
    pub fn spawn_paused(
        command: Option<&str>,
        env: &[(String, String)],
        cwd: &Path,
        rows: u16,
        cols: u16,
    ) -> Result<PausedPty> {
        let rows = rows.max(2);
        let cols = cols.max(10);

        // Build gate command before openpty, as with spawn_registered_with
        let cmd = build_gate_command(command, env, cwd);

        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open pty")?;

        let mut child = pair
            .slave
            .spawn_command(cmd)
            .context("failed to spawn command in pty")?;
        drop(pair.slave);

        // Get the process identity
        let identity = child
            .process_id()
            .ok_or_else(|| anyhow::anyhow!("PTY child did not expose a process id"))
            .and_then(|pid| {
                crate::session::inspect_process_identity(pid)
                    .map_err(anyhow::Error::msg)?
                    .ok_or_else(|| anyhow::anyhow!("PTY child {pid} exited before identification"))
            });
        let identity = match identity {
            Ok(identity)
                if identity.process_group == identity.pid as i32
                    && identity.session == identity.pid as i32 =>
            {
                identity
            }
            Ok(identity) => {
                let _ = child.kill();
                let _ = child.wait();
                anyhow::bail!(
                    "PTY child {} does not own its process group/session ({}/{})",
                    identity.pid,
                    identity.process_group,
                    identity.session
                );
            }
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(error.context("failed to establish PTY process identity"));
            }
        };

        // Clone reader and take writer, but don't write gate token yet
        let reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone pty reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("failed to take pty writer")?;

        Ok(PausedPty {
            parts: Some(PausedPtyParts {
                child: Arc::new(Mutex::new(child)),
                reader,
                writer,
                master: pair.master,
                identity: identity.clone(),
                rows,
                cols,
            }),
            identity,
        })
    }

    /// Spawn a PTY session leader behind a private stdin registration gate.
    /// `register` observes the exact paused identity and must durably record it
    /// before the intended command is released. Any registration failure stops
    /// and reaps the gate, so no unowned command can escape.
    pub fn spawn_registered_with(
        command: Option<&str>,
        env: &[(String, String)],
        cwd: &Path,
        rows: u16,
        cols: u16,
        register: impl FnOnce(&ProcessIdentity) -> Result<()>,
    ) -> Result<Pty> {
        let paused = Self::spawn_paused(command, env, cwd, rows, cols)?;
        register(paused.identity())?;
        paused.release()
    }

    /// Subscribe to raw output for remote attach. Returns a redraw snapshot
    /// (clear + current screen + terminal modes) to apply first, and a
    /// receiver carrying every chunk after it. Registration and snapshot
    /// happen under the parser lock the reader holds while processing and
    /// broadcasting, so nothing is lost or duplicated in between.
    pub fn subscribe(&self) -> (Vec<u8>, std::sync::mpsc::Receiver<Vec<u8>>) {
        let (tx, rx) = std::sync::mpsc::channel();
        let snapshot = match self.parser.lock() {
            Ok(p) => {
                if let Ok(mut subs) = self.subscribers.lock() {
                    subs.push(tx);
                }
                let screen = p.screen();
                let mut bytes = Vec::new();
                // Kitty keyboard replay, full depth (BAUDE FORK accessors):
                // one push per stack entry, oldest first, so post-attach pops
                // from a nested-push child decrement the mirror identically
                // to the source — a single top-of-stack push would collapse
                // an N-deep stack to depth 1 and the next CSI < 1 u would
                // empty the mirror while the source stays kitty. Main-screen
                // pushes go out BEFORE the alternate-screen switch so each
                // stack lands on its own per-screen mirror stack (entering
                // the alternate screen empties its stack); the alternate
                // stack is replayed after the switch, below. An inactive
                // child (both stacks empty) adds zero bytes — the snapshot
                // stays byte-identical to the pre-phase construction.
                for flags in screen.kitty_main_stack() {
                    bytes.extend_from_slice(format!("\x1b[>{flags}u").as_bytes());
                }
                if screen.alternate_screen() {
                    bytes.extend_from_slice(b"\x1b[?1049h");
                }
                bytes.extend_from_slice(b"\x1b[2J\x1b[H");
                bytes.extend_from_slice(&screen.contents_formatted());
                // Terminal modes aren't part of contents_formatted; replay
                // the ones claude relies on.
                if screen.application_cursor() {
                    bytes.extend_from_slice(b"\x1b[?1h");
                }
                if screen.application_keypad() {
                    bytes.extend_from_slice(b"\x1b=");
                }
                if screen.bracketed_paste() {
                    bytes.extend_from_slice(b"\x1b[?2004h");
                }
                if screen.hide_cursor() {
                    bytes.extend_from_slice(b"\x1b[?25l");
                }
                // Alternate-screen kitty stack (TKEY-05 across attach): must
                // come after the `?1049h` above so the pushes land on the
                // mirror's alternate stack. Skipped on the main screen —
                // alternate-stack residue is unreachable there (only readable
                // while the alternate screen is active, and re-entry empties
                // it on both sides).
                if screen.alternate_screen() {
                    for flags in screen.kitty_alternate_stack() {
                        bytes.extend_from_slice(format!("\x1b[>{flags}u").as_bytes());
                    }
                }
                bytes
            }
            Err(_) => Vec::new(),
        };
        (snapshot, rx)
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows = rows.max(2);
        let cols = cols.max(10);
        if self.size == (rows, cols) {
            return;
        }
        self.size = (rows, cols);
        let _ = self.master.resize(PtySize {
            rows,
            cols,
            pixel_width: 0,
            pixel_height: 0,
        });
        if let Ok(mut p) = self.parser.lock() {
            p.set_size(rows, cols);
        }
    }

    pub fn write_input(&mut self, bytes: &[u8]) {
        if bytes.is_empty() || self.is_exited() {
            return;
        }
        let _ = self.writer.write_all(bytes);
        let _ = self.writer.flush();
    }

    /// Returns `(alternate_screen, mouse_enabled, mouse_sgr)` — enough for
    /// the TUI to decide whether to use vt100 scrollback or forward scroll
    /// events as PTY input to the inner application.
    pub fn scroll_info(&self) -> (bool, bool, bool) {
        self.parser
            .lock()
            .ok()
            .map(|p| {
                let screen = p.screen();
                let alt = screen.alternate_screen();
                let mouse_enabled = screen.mouse_protocol_mode() != vt100::MouseProtocolMode::None;
                let mouse_sgr =
                    screen.mouse_protocol_encoding() == vt100::MouseProtocolEncoding::Sgr;
                (alt, mouse_enabled, mouse_sgr)
            })
            .unwrap_or((false, false, false))
    }

    pub fn pid(&self) -> Option<u32> {
        self.child.lock().ok().and_then(|c| c.process_id())
    }

    pub fn process_identity(&self) -> &ProcessIdentity {
        &self.identity
    }

    pub fn is_exited(&self) -> bool {
        if self.exited.load(Ordering::Relaxed) {
            return true;
        }
        if let Ok(mut child) = self.child.lock() {
            if let Ok(Some(_)) = child.try_wait() {
                self.exited.store(true, Ordering::Relaxed);
                return true;
            }
        }
        false
    }

    pub fn kill(&mut self) {
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
        }
        self.exited.store(true, Ordering::Relaxed);
    }

    /// Send SIGSTOP to the child process group, pausing execution.
    /// Unix-only; no-op on other platforms.
    #[cfg(unix)]
    pub fn suspend(&self) {
        let pi = self.process_identity();
        let pgid = pi.process_group;
        unsafe {
            let _ = libc::kill(pgid, libc::SIGSTOP);
        }
    }

    #[cfg(not(unix))]
    pub fn suspend(&self) {
        // Non-Unix: no-op
    }

    /// Send SIGCONT to the child process group, resuming execution.
    /// Unix-only; no-op on other platforms.
    #[cfg(unix)]
    pub fn resume(&self) {
        let pi = self.process_identity();
        let pgid = pi.process_group;
        unsafe {
            let _ = libc::kill(pgid, libc::SIGCONT);
        }
    }

    #[cfg(not(unix))]
    pub fn resume(&self) {
        // Non-Unix: no-op
    }

    /// True while the child is still held behind the registration gate.
    pub fn is_gated(&self) -> bool {
        self.gated
    }

    /// Write the gate token so a gated child starts its command. No-op once
    /// released. Only call after the child's identity is durably recorded.
    pub fn release_gate(&mut self) -> Result<()> {
        if !self.gated {
            return Ok(());
        }
        self.writer
            .write_all(format!("{GATE_TOKEN}\n").as_bytes())
            .and_then(|_| self.writer.flush())
            .context("failed to write gate token")?;
        self.gated = false;
        Ok(())
    }

    /// Stop the child and confirm that it has exited before reporting success.
    /// Safety-sensitive callers must use this instead of the best-effort
    /// `kill`, because a signal attempt alone is not a process-stop boundary.
    pub fn kill_and_wait(&mut self) -> Result<()> {
        #[cfg(debug_assertions)]
        if let Some(pid) = self.pid() {
            if let Some(detail) = teardown_failures_for_test()
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .remove(&pid)
            {
                anyhow::bail!("injected PTY teardown failure: {detail}");
            }
        }
        let mut child = self
            .child
            .lock()
            .map_err(|_| anyhow::anyhow!("PTY child lock poisoned"))?;
        if child
            .try_wait()
            .context("failed to inspect PTY child before stop")?
            .is_some()
        {
            self.exited.store(true, Ordering::Release);
            return Ok(());
        }
        let observed = crate::session::inspect_process_identity(self.identity.pid)
            .map_err(anyhow::Error::msg)?;
        if observed.as_ref() != Some(&self.identity) {
            self.exited.store(true, Ordering::Release);
            return Ok(());
        }
        signal_group(self.identity.process_group, libc::SIGTERM)
            .context("failed to terminate exact PTY process group")?;
        if !wait_for_group_extinction(self.identity.process_group, child.as_mut()) {
            signal_group(self.identity.process_group, libc::SIGKILL)
                .context("failed to force exact PTY process group")?;
            if !wait_for_group_extinction(self.identity.process_group, child.as_mut()) {
                anyhow::bail!(
                    "PTY process group {} remained live after forced termination",
                    self.identity.process_group
                );
            }
        }
        if child
            .try_wait()
            .context("failed to inspect PTY child after group extinction")?
            .is_none()
        {
            child.wait().context("failed to wait for PTY child")?;
        }
        self.exited.store(true, Ordering::Release);
        Ok(())
    }

    #[cfg(debug_assertions)]
    #[doc(hidden)]
    pub fn fail_next_teardown_for_test(&self, detail: impl Into<String>) {
        if let Some(pid) = self.pid() {
            teardown_failures_for_test()
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .insert(pid, detail.into());
        }
    }
}

fn signal_group(process_group: i32, signal: i32) -> std::io::Result<()> {
    // SAFETY: negative pid selects the process group captured from the exact
    // PTY leader identity and rechecked immediately before the first signal.
    let result = unsafe { libc::kill(-process_group, signal) };
    if result == 0 {
        Ok(())
    } else {
        let error = std::io::Error::last_os_error();
        if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(())
        } else {
            Err(error)
        }
    }
}

fn process_group_exists(process_group: i32) -> bool {
    // SAFETY: signal zero performs a liveness/permission probe only.
    let result = unsafe { libc::kill(-process_group, 0) };
    result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
}

fn wait_for_group_extinction(process_group: i32, child: &mut dyn Child) -> bool {
    for _ in 0..50 {
        let _ = child.try_wait();
        if !process_group_exists(process_group) {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    !process_group_exists(process_group)
}

#[cfg(debug_assertions)]
fn teardown_failures_for_test() -> &'static std::sync::Mutex<std::collections::HashMap<u32, String>>
{
    static FAILURES: std::sync::OnceLock<std::sync::Mutex<std::collections::HashMap<u32, String>>> =
        std::sync::OnceLock::new();
    FAILURES.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::time::Duration;

    /// Selects the re-exec'd child branch of
    /// [`worker_isolation_pty_child_environment`].
    const PTY_CHILD_ENV_CHILD: &str = "BAUDE_WORKER_ISOLATION_PTY_CHILD";

    /// A synthetic root a single PTY test OWNS: created empty, redirected for
    /// the duration, removed with the test.
    ///
    /// Every PTY test needs one now, because [`configure_test_child`] resolves
    /// the child's roots through the guarded resolvers — an unredirected test
    /// aborts instead of launching. That is the point: it also gives the child
    /// a cwd the fixture owns, replacing the shared `/tmp` these tests used,
    /// where two concurrent runs (or two runs of the same test) wrote over each
    /// other.
    ///
    /// The name plus pid plus a per-process sequence is what keeps roots unique:
    /// pid alone collides between two tests in one binary, and a name alone
    /// collides between two binaries running at once.
    struct PtyFixture {
        root: PathBuf,
        _redirect: crate::testing::TestRedirect,
    }

    impl PtyFixture {
        fn new(name: &str) -> Self {
            use std::sync::atomic::AtomicUsize;
            static SEQ: AtomicUsize = AtomicUsize::new(0);
            let root = std::env::temp_dir().join(format!(
                "baude-pty-{name}-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::Relaxed)
            ));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            let _redirect = crate::testing::TestRedirect::new(&root);
            Self { root, _redirect }
        }

        /// The cwd handed to the child — inside the fixture, by construction.
        fn cwd(&self) -> &Path {
            &self.root
        }
    }

    impl Drop for PtyFixture {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    /// A sentinel "startup file" that records the fact it was sourced or
    /// executed, and nothing else. Every one of these lives inside the
    /// synthetic ambient tree, so a sentinel that DOES run writes somewhere
    /// observable but harmless.
    fn write_sentinel(path: &Path, marker: &Path) {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(
            path,
            format!("#!/bin/sh\nprintf ran >> {}\n", marker.display()),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
    }

    /// A test-launched PTY must not inherit the developer's roots, and the
    /// developer's login-shell startup files must not run before the fixture's
    /// command.
    ///
    /// This is the boundary a thread-local redirect cannot cross. The child is
    /// a separate process: it reads `HOME`, `XDG_*`, `CLAUDE_CONFIG_DIR`,
    /// `SHELL`, `ZDOTDIR`, `ENV` and `BASH_ENV` from the environment it is
    /// handed, and `zsh -il` sources `~/.zshrc` before it ever reaches the
    /// command baude asked for. A redirected `HOME` alone is not enough,
    /// because `ZDOTDIR` overrides it.
    ///
    /// The whole exercise runs in a re-exec'd child whose environment ADVERTISES
    /// hostile roots and executable startup sentinels — all synthetic — so a
    /// failing run writes into the synthetic ambient tree rather than anywhere
    /// real. One sentinel is executed directly as a POSITIVE CONTROL first, so a
    /// detector that stopped recording cannot let this pass.
    #[test]
    fn worker_isolation_pty_child_environment() {
        if std::env::var_os(PTY_CHILD_ENV_CHILD).is_some() {
            let root = PathBuf::from(
                std::env::var_os("BAUDE_TEST_FIXTURE_ROOT")
                    .expect("the child must receive a synthetic fixture root"),
            );
            let ambient = root.join("ambient");
            let fixture = root.join("fixture");
            let startup_marker = ambient.join("startup-ran");
            std::fs::create_dir_all(&fixture).unwrap();

            // Positive control: the sentinel recorder works.
            let control = std::process::Command::new(ambient.join("shell-sentinel"))
                .status()
                .expect("the synthetic shell sentinel must be executable");
            assert!(control.success(), "the sentinel must run cleanly");
            assert!(
                startup_marker.exists(),
                "positive control failed: executing a startup sentinel recorded nothing"
            );
            std::fs::remove_file(&startup_marker).unwrap();

            let _redirect = crate::testing::TestRedirect::new(&fixture);
            let config_dir = crate::persist::config_dir();
            let claude_dir = crate::meta::claude_config_dir();

            // Explicit launch-plan env: one opaque value that MUST survive, and
            // a full set of hostile root/startup keys that must not.
            let hostile = ambient.display().to_string();
            let sentinel = ambient.join("env-sentinel").display().to_string();
            let explicit: Vec<(String, String)> = vec![
                ("BAUDE_RESUME_ID".into(), "opaque-resume-7".into()),
                ("HOME".into(), hostile.clone()),
                ("XDG_CONFIG_HOME".into(), hostile.clone()),
                ("XDG_DATA_HOME".into(), hostile.clone()),
                ("CLAUDE_CONFIG_DIR".into(), hostile.clone()),
                ("SHELL".into(), sentinel.clone()),
                ("ZDOTDIR".into(), hostile.clone()),
                ("ENV".into(), sentinel.clone()),
                ("BASH_ENV".into(), sentinel.clone()),
                // The gate's own keys. `GATE_SCRIPT` execs
                // `"$BAUDE_GATE_SHELL" … -c "$BAUDE_GATE_COMMAND"`, so a caller
                // that could set these would choose what the child runs
                // outright, and one that could set the token would break the
                // handshake (#72, WR-04).
                ("BAUDE_GATE_COMMAND".into(), "touch /tmp/pwned".into()),
                ("BAUDE_GATE_SHELL".into(), sentinel.clone()),
                ("BAUDE_GATE_TOKEN".into(), "not-the-token".into()),
                ("BAUDE_GATE_MODE".into(), "interactive".into()),
            ];

            // Deterministic control: inspect the finished builder map rather
            // than inferring policy from one child's output.
            let builder = build_gate_command(Some("true"), &explicit, &fixture);
            let env_of = |key: &str| {
                builder
                    .get_env(key)
                    .map(|v| v.to_string_lossy().into_owned())
            };
            assert_eq!(
                env_of("BAUDE_RESUME_ID").as_deref(),
                Some("opaque-resume-7"),
                "an opaque launch-plan value must reach the child"
            );
            assert_eq!(
                env_of("HOME").map(PathBuf::from),
                Some(config_dir.join("child-home")),
                "explicit env overrode the protected HOME"
            );
            assert_eq!(
                env_of("XDG_CONFIG_HOME").map(PathBuf::from),
                Some(config_dir.join("child-config")),
                "explicit env overrode the protected XDG_CONFIG_HOME"
            );
            assert_eq!(
                env_of("XDG_DATA_HOME").map(PathBuf::from),
                Some(config_dir.join("child-data")),
                "explicit env overrode the protected XDG_DATA_HOME"
            );
            assert_eq!(
                env_of("CLAUDE_CONFIG_DIR").map(PathBuf::from),
                Some(claude_dir.clone()),
                "explicit env overrode the protected CLAUDE_CONFIG_DIR"
            );
            assert_eq!(
                env_of("ZDOTDIR").map(PathBuf::from),
                Some(config_dir.join("child-home")),
                "ZDOTDIR must be pinned under the child home, not the caller's value"
            );
            assert_eq!(
                env_of("ENV").as_deref(),
                Some("/dev/null"),
                "ENV must be neutralized"
            );
            assert_eq!(
                env_of("BASH_ENV").as_deref(),
                Some("/dev/null"),
                "BASH_ENV must be neutralized"
            );
            assert_eq!(
                env_of("PATH").as_deref(),
                Some("/usr/bin:/bin:/usr/sbin:/sbin"),
                "the child must get an explicit PATH"
            );
            assert_eq!(
                env_of("SHELL").as_deref(),
                Some("/bin/bash"),
                "the gate must use an explicit no-startup shell"
            );
            assert_eq!(
                env_of("BAUDE_GATE_SHELL").as_deref(),
                Some("/bin/bash"),
                "the gate shell must be the explicit executable, not the caller's SHELL"
            );
            assert_eq!(
                env_of("TERM").as_deref(),
                Some("xterm-256color"),
                "terminal type is unchanged"
            );
            assert_eq!(
                env_of("BAUDE_GATE_COMMAND").as_deref(),
                Some("true"),
                "the caller must not choose what the gate execs"
            );
            assert_eq!(
                env_of("BAUDE_GATE_MODE").as_deref(),
                Some("command"),
                "the caller must not flip the gate's mode"
            );
            assert_eq!(
                env_of("BAUDE_GATE_TOKEN").as_deref(),
                Some(GATE_TOKEN),
                "the caller must not break the handshake token"
            );

            // Now an ACTUAL PTY child, launched with the same hostile explicit
            // env, reporting what it really sees.
            let report = fixture.join("child-report");
            let command = format!(
                "{{ echo \"HOME=$HOME\"; echo \"XDG_CONFIG_HOME=$XDG_CONFIG_HOME\"; \
                 echo \"CLAUDE_CONFIG_DIR=$CLAUDE_CONFIG_DIR\"; echo \"ZDOTDIR=$ZDOTDIR\"; \
                 echo \"RESUME=$BAUDE_RESUME_ID\"; }} > {0}; cat {0}",
                report.display()
            );
            let mut pty = Pty::spawn_with_env(Some(&command), &explicit, &fixture, 24, 200)
                .expect("the guarded PTY must launch");

            let deadline = std::time::Instant::now() + Duration::from_secs(20);
            let text = loop {
                if let Ok(contents) = std::fs::read_to_string(&report) {
                    if contents.contains("RESUME=") {
                        break contents;
                    }
                }
                assert!(
                    std::time::Instant::now() < deadline,
                    "the PTY child never reported its environment"
                );
                std::thread::sleep(Duration::from_millis(50));
            };
            // Reap before any guard drops.
            pty.kill_and_wait().unwrap();

            let field = |key: &str| {
                text.lines()
                    .find_map(|line| line.strip_prefix(&format!("{key}=")))
                    .map(str::to_string)
                    .unwrap_or_else(|| panic!("the child reported no {key}:\n{text}"))
            };
            assert_eq!(
                PathBuf::from(field("HOME")),
                config_dir.join("child-home"),
                "the PTY child's HOME escaped the fixture"
            );
            assert_eq!(
                PathBuf::from(field("XDG_CONFIG_HOME")),
                config_dir.join("child-config"),
                "the PTY child's XDG_CONFIG_HOME escaped the fixture"
            );
            assert_eq!(
                PathBuf::from(field("CLAUDE_CONFIG_DIR")),
                claude_dir,
                "the PTY child's CLAUDE_CONFIG_DIR escaped the fixture"
            );
            assert_eq!(
                PathBuf::from(field("ZDOTDIR")),
                config_dir.join("child-home"),
                "the PTY child inherited the caller's ZDOTDIR"
            );
            assert_eq!(
                field("RESUME"),
                "opaque-resume-7",
                "the opaque launch-plan value was lost"
            );
            assert!(
                !startup_marker.exists(),
                "a developer-style startup sentinel executed before the fixture command"
            );
            return;
        }

        let root = std::env::temp_dir().join(format!(
            "baude-worker-isolation-pty-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let ambient = root.join("ambient");
        let startup_marker = ambient.join("startup-ran");
        for leaf in ["fixture", "ambient", "ambient/zdotdir", "ambient/claude"] {
            std::fs::create_dir_all(root.join(leaf)).expect("synthetic child tree");
        }
        // Everything a login shell would read, plus the two the plan's own
        // threat model names (ZDOTDIR and ENV/BASH_ENV) — all executable, all
        // recording into the synthetic tree.
        write_sentinel(&ambient.join("shell-sentinel"), &startup_marker);
        write_sentinel(&ambient.join("env-sentinel"), &startup_marker);
        write_sentinel(&ambient.join(".zshrc"), &startup_marker);
        write_sentinel(&ambient.join(".zprofile"), &startup_marker);
        write_sentinel(&ambient.join(".bash_profile"), &startup_marker);
        write_sentinel(&ambient.join("zdotdir").join(".zshrc"), &startup_marker);

        let mut command = std::process::Command::new(std::env::current_exe().unwrap());
        command.args([
            "--exact",
            "pty::tests::worker_isolation_pty_child_environment",
            "--nocapture",
            "--test-threads=1",
        ]);
        command.env_clear();
        command.env(PTY_CHILD_ENV_CHILD, "1");
        command.env("BAUDE_TEST_FIXTURE_ROOT", &root);
        command.env("HOME", &ambient);
        command.env("XDG_CONFIG_HOME", &ambient);
        command.env("XDG_DATA_HOME", &ambient);
        command.env("CLAUDE_CONFIG_DIR", ambient.join("claude"));
        command.env("SHELL", ambient.join("shell-sentinel"));
        command.env("ZDOTDIR", ambient.join("zdotdir"));
        command.env("ENV", ambient.join("env-sentinel"));
        command.env("BASH_ENV", ambient.join("env-sentinel"));
        command.env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin");
        let output = command.output().expect("re-exec the test binary");

        let status = output.status;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let leaked_startup = startup_marker.exists();
        if status.success() {
            let _ = std::fs::remove_dir_all(&root);
        }
        assert!(
            status.success(),
            "pty child-environment child failed ({status})\n--- stdout ---\n{stdout}\n--- stderr ---\n{stderr}"
        );
        assert!(
            stdout.contains("1 passed"),
            "the child must have RUN the case, not filtered it out:\n{stdout}"
        );
        assert!(
            !leaked_startup,
            "a startup sentinel ran somewhere in the child process tree"
        );
    }

    /// An unguarded test launch must abort BEFORE a process exists.
    ///
    /// Not merely "must not read the real roots": a PTY that has already been
    /// opened and spawned has run the developer's login shell, and no later
    /// check can take that back. The resolution therefore happens in
    /// `build_gate_command`, ahead of `openpty` — so even an explicitly
    /// harmless command like `true` never becomes a child.
    #[test]
    #[should_panic(expected = "resolved to the real user path")]
    fn worker_isolation_pty_requires_fixture() {
        let _no_root = crate::testing::NoFixtureRoot::new();
        // No TestRedirect on this thread, and the fixture-root exemption is
        // suppressed: the guarded resolver must panic.
        let _escaped = Pty::spawn(Some("true"), Path::new("/"), 5, 40);
    }

    #[test]
    fn pre_exec_registration_gate_owner_death_and_release() {
        let fixture = PtyFixture::new("registration-gate");
        let root = fixture.cwd().to_path_buf();
        let marker = root.join("released");
        let command = format!(
            "trap '' HUP; printf released > {}; sleep 30",
            marker.display()
        );

        let failed = Pty::spawn_registered_with(Some(&command), &[], &root, 5, 40, |_| {
            anyhow::bail!("persistence refused")
        });
        assert!(failed.is_err());
        std::thread::sleep(Duration::from_millis(100));
        assert!(!marker.exists(), "failed registration released command");

        let mut pty = Pty::spawn_registered_with(Some(&command), &[], &root, 5, 40, |identity| {
            assert_eq!(identity.pid as i32, identity.process_group);
            assert_eq!(identity.pid as i32, identity.session);
            Ok(())
        })
        .unwrap();
        let identity = pty.process_identity().clone();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !marker.exists() {
            assert!(
                std::time::Instant::now() < deadline,
                "gate was not released"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(pty.process_identity(), &identity);
        // Before the fixture drops: the child must be gone while its root, and
        // the redirects that contained it, still exist.
        pty.kill_and_wait().unwrap();
    }

    #[test]
    fn subscribe_snapshot_then_live_bytes() {
        let fixture = PtyFixture::new("subscribe");
        let mut pty =
            Pty::spawn(Some("echo before; cat; echo after"), fixture.cwd(), 6, 60).unwrap();
        // Let "before" land in the parser, then attach.
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let has = pty
                .parser
                .lock()
                .map(|p| p.screen().contents().contains("before"))
                .unwrap_or(false);
            if has {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "no initial output");
            std::thread::sleep(Duration::from_millis(50));
        }
        let (snapshot, rx) = pty.subscribe();
        let snap = String::from_utf8_lossy(&snapshot).to_string();
        assert!(snap.contains("before"), "snapshot misses prior output");

        pty.write_input(b"live-marker\n");
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        let mut seen = String::new();
        while !seen.contains("live-marker") {
            assert!(
                std::time::Instant::now() < deadline,
                "no live bytes: {seen}"
            );
            if let Ok(chunk) = rx.recv_timeout(Duration::from_millis(200)) {
                seen.push_str(&String::from_utf8_lossy(&chunk));
            }
        }
        // Live bytes must not replay what the snapshot already covered.
        assert!(
            !seen.contains("before"),
            "snapshot bytes duplicated on channel"
        );
        pty.kill();
    }

    #[test]
    fn kill_and_wait_confirms_child_exit() {
        let fixture = PtyFixture::new("kill-confirms");
        let mut pty = Pty::spawn(Some("sleep 30"), fixture.cwd(), 5, 40).unwrap();
        assert!(!pty.is_exited());
        pty.kill_and_wait().unwrap();
        assert!(pty.is_exited());
    }

    #[test]
    fn kill_and_wait_accepts_and_retries_naturally_exited_child() {
        let fixture = PtyFixture::new("kill-retries");
        let mut pty = Pty::spawn(Some("exit 0"), fixture.cwd(), 5, 40).unwrap();
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !pty.is_exited() {
            assert!(std::time::Instant::now() < deadline, "child did not exit");
            std::thread::sleep(Duration::from_millis(10));
        }
        pty.kill_and_wait().unwrap();
        pty.kill_and_wait().unwrap();
    }

    #[test]
    fn output_timestamp_goes_idle() {
        // The child speaks once (`echo hi`) then goes quiet for a long time.
        let fixture = PtyFixture::new("idle");
        let mut pty = Pty::spawn(Some("echo hi; sleep 30"), fixture.cwd(), 5, 40).unwrap();
        // Poll until the silence since the last output crosses the ~2s idle
        // threshold, rather than asserting a fixed sleep lines up with when the
        // reader thread happens to record the echo — that coupling made this
        // flaky on loaded CI (the echo could be timestamped late, squeezing the
        // measured idle below 2000ms). The 5s budget bounds a genuine hang while
        // staying well under the 30s child lifetime.
        let mut idle = 0;
        for _ in 0..50 {
            std::thread::sleep(Duration::from_millis(100));
            idle = now_ms().saturating_sub(pty.last_output_ms.load(Ordering::Relaxed));
            if idle >= 2000 {
                break;
            }
        }
        assert!(
            idle >= 2000,
            "expected idle to reach >=2000ms, got {idle}ms"
        );
        // It went idle because the child is silently sleeping, not because it
        // exited.
        assert!(
            !pty.is_exited(),
            "child should still be sleeping, not exited"
        );
        // The 30s sleeper outlives the test otherwise, and would still be
        // holding the fixture root as its cwd after the fixture removed it.
        pty.kill_and_wait().unwrap();
    }

    /// LINK-03 remote parity (Pitfall 3): a subscriber attaching AFTER links
    /// were printed must reconstruct identical link targets from the redraw
    /// snapshot. Pure — this builds the snapshot bytes exactly the way
    /// `subscribe()` does (alternate-screen preamble + clear + home +
    /// `contents_formatted` + mode replay) and feeds them to a fresh parser;
    /// no real PTY is involved.
    #[test]
    fn subscribe_snapshot_replays_pre_attach_links() {
        let mut parser = vt100::Parser::new(6, 60, 0);
        parser.process(
            b"\x1b]8;;https://pre.example/attach\x1b\\pre-attach link\x1b]8;;\x1b\\ plain tail",
        );
        let screen = parser.screen();

        // Mirror the snapshot construction in subscribe().
        let mut bytes = Vec::new();
        if screen.alternate_screen() {
            bytes.extend_from_slice(b"\x1b[?1049h");
        }
        bytes.extend_from_slice(b"\x1b[2J\x1b[H");
        bytes.extend_from_slice(&screen.contents_formatted());
        if screen.application_cursor() {
            bytes.extend_from_slice(b"\x1b[?1h");
        }
        if screen.application_keypad() {
            bytes.extend_from_slice(b"\x1b=");
        }
        if screen.bracketed_paste() {
            bytes.extend_from_slice(b"\x1b[?2004h");
        }
        if screen.hide_cursor() {
            bytes.extend_from_slice(b"\x1b[?25l");
        }

        let mut remote = vt100::Parser::new(6, 60, 0);
        remote.process(&bytes);
        let remote_screen = remote.screen();

        let target = |s: &vt100::Screen, row: u16, col: u16| -> Option<String> {
            s.cell(row, col)
                .and_then(|c| c.link_id())
                .and_then(|id| s.link_target(id))
                .map(str::to_string)
        };
        for row in 0..6 {
            for col in 0..60 {
                assert_eq!(
                    target(screen, row, col),
                    target(remote_screen, row, col),
                    "link target parity at ({row},{col})"
                );
            }
        }
        // Guard against vacuous parity: the pre-attach link actually links.
        let id = screen
            .cell(0, 0)
            .unwrap()
            .link_id()
            .expect("local pre-attach cell carries a link id");
        assert_eq!(screen.link_target(id), Some("https://pre.example/attach"));
    }

    /// Mirror the snapshot construction in subscribe() byte-for-byte (the
    /// pre-attach-links precedent above): drift between this mirror and the
    /// production construction is the failure mode being guarded. Pure — no
    /// real PTY.
    fn subscribe_snapshot_bytes(screen: &vt100::Screen) -> Vec<u8> {
        let mut bytes = Vec::new();
        for flags in screen.kitty_main_stack() {
            bytes.extend_from_slice(format!("\x1b[>{flags}u").as_bytes());
        }
        if screen.alternate_screen() {
            bytes.extend_from_slice(b"\x1b[?1049h");
        }
        bytes.extend_from_slice(b"\x1b[2J\x1b[H");
        bytes.extend_from_slice(&screen.contents_formatted());
        if screen.application_cursor() {
            bytes.extend_from_slice(b"\x1b[?1h");
        }
        if screen.application_keypad() {
            bytes.extend_from_slice(b"\x1b=");
        }
        if screen.bracketed_paste() {
            bytes.extend_from_slice(b"\x1b[?2004h");
        }
        if screen.hide_cursor() {
            bytes.extend_from_slice(b"\x1b[?25l");
        }
        if screen.alternate_screen() {
            for flags in screen.kitty_alternate_stack() {
                bytes.extend_from_slice(format!("\x1b[>{flags}u").as_bytes());
            }
        }
        bytes
    }

    /// TKEY-05 across attach: a child that pushed kitty flags before a remote
    /// attach must have that push replayed in the subscribe snapshot so the
    /// mirror parser converges on the same child kitty state.
    #[test]
    fn subscribe_snapshot_replays_active_kitty_push() {
        let mut parser = vt100::Parser::new(6, 60, 0);
        parser.process(b"\x1b[>1uordinary text before attach");
        let screen = parser.screen();
        assert_eq!(screen.kitty_keyboard(), 1, "source observed the push");

        let bytes = subscribe_snapshot_bytes(screen);
        let mut remote = vt100::Parser::new(6, 60, 0);
        remote.process(&bytes);
        assert_eq!(
            remote.screen().kitty_keyboard(),
            1,
            "mirror must converge on the child's active kitty flags"
        );
    }

    /// WR-03: an N-deep source stack replays at full depth. The next
    /// CSI < 1 u from a nested-push child (push, then nested push — e.g.
    /// Claude Code plus an inner tool) must leave both source and mirror
    /// at the prior nonzero entry instead of emptying the mirror and
    /// permanently degrading remote Shift+Enter.
    #[test]
    fn subscribe_snapshot_replays_full_stack_depth() {
        let mut parser = vt100::Parser::new(6, 60, 0);
        parser.process(b"\x1b[>1u\x1b[>5u");
        assert_eq!(parser.screen().kitty_keyboard(), 5);

        let bytes = subscribe_snapshot_bytes(parser.screen());
        let mut remote = vt100::Parser::new(6, 60, 0);
        remote.process(&bytes);
        assert_eq!(remote.screen().kitty_keyboard(), 5);

        // Post-attach pop from the child reaches both parsers.
        parser.process(b"\x1b[<1u");
        remote.process(b"\x1b[<1u");
        assert_eq!(parser.screen().kitty_keyboard(), 1);
        assert_eq!(
            remote.screen().kitty_keyboard(),
            1,
            "mirror stack must pop in lockstep with the source, not empty"
        );
    }

    /// WR-03 x WR-02: per-screen stacks replay onto their own mirror
    /// stacks — a child attached mid-alt-screen converges on both the
    /// alternate flags AND the main-screen flags it returns to on exit.
    #[test]
    fn subscribe_snapshot_replays_per_screen_stacks() {
        let mut parser = vt100::Parser::new(6, 60, 0);
        parser.process(b"\x1b[>1u\x1b[?1049h\x1b[>5u");
        assert_eq!(parser.screen().kitty_keyboard(), 5);

        let bytes = subscribe_snapshot_bytes(parser.screen());
        let mut remote = vt100::Parser::new(6, 60, 0);
        remote.process(&bytes);
        assert_eq!(remote.screen().kitty_keyboard(), 5);

        // Child exits the alternate screen post-attach on both sides: the
        // mirror must restore the same main-screen flags as the source.
        parser.process(b"\x1b[?1049l");
        remote.process(b"\x1b[?1049l");
        assert_eq!(parser.screen().kitty_keyboard(), 1);
        assert_eq!(
            remote.screen().kitty_keyboard(),
            1,
            "mirror must restore the source's main-screen flags on alt exit"
        );
    }

    /// Inactive child kitty state adds zero bytes: snapshots for non-kitty
    /// children stay byte-identical to the pre-phase construction.
    #[test]
    fn subscribe_snapshot_inactive_kitty_adds_no_bytes() {
        let mut parser = vt100::Parser::new(6, 60, 0);
        parser.process(b"plain child, no kitty push");
        let screen = parser.screen();
        assert_eq!(screen.kitty_keyboard(), 0);

        let bytes = subscribe_snapshot_bytes(screen);
        assert!(
            !bytes.windows(3).any(|w| w == b"\x1b[>"),
            "no CSI > u replay bytes may leak into a non-kitty snapshot"
        );
        let mut remote = vt100::Parser::new(6, 60, 0);
        remote.process(&bytes);
        assert_eq!(
            remote.screen().kitty_keyboard(),
            0,
            "mirror of a non-kitty child must stay legacy"
        );
    }

    #[test]
    fn spawn_paused_holds_child_until_release() {
        let fixture = PtyFixture::new("spawn_paused_holds_child");
        let paused = Pty::spawn_paused(Some("/bin/sh -c 'read x'"), &[], &fixture.root, 5, 40)
            .expect("spawn_paused");

        // Child is paused, identity is available
        let identity = paused.identity();
        assert!(identity.pid > 0, "paused child must have a valid pid");

        // Release the child
        let pty = paused.release().expect("release");
        assert!(!pty.is_exited(), "child should be running after release");
    }

    #[test]
    fn abort_reaps_without_release() {
        let fixture = PtyFixture::new("abort_reaps_without_release");
        let paused = Pty::spawn_paused(Some("/bin/sh -c 'sleep 100'"), &[], &fixture.root, 5, 40)
            .expect("spawn_paused");

        let pid = paused.identity().pid as i32;
        paused.abort();

        // After abort, the child should be reaped
        // Attempting to get its status should indicate it's gone
        std::thread::sleep(Duration::from_millis(10));
        let status = unsafe { libc::waitpid(pid, std::ptr::null_mut(), libc::WNOHANG) };
        // Either already reaped (-1) or doesn't exist (ECHILD returned as -1)
        assert!(status == -1 || status == pid, "child must be reaped");
    }

    #[test]
    fn spawn_registered_with_still_releases_after_register() {
        let fixture = PtyFixture::new("spawn_registered_with_compat");
        let mut registered = false;
        let pty = Pty::spawn_registered_with(
            Some("/bin/sh -c 'read x'"),
            &[],
            &fixture.root,
            5,
            40,
            |identity| {
                assert!(identity.pid > 0);
                registered = true;
                Ok(())
            },
        )
        .expect("spawn_registered_with");

        assert!(registered, "register callback must be called");
        assert!(!pty.is_exited(), "child should be running after register");
    }
}
