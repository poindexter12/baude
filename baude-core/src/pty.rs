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

/// One embedded terminal: a PTY with a child process and a vt100 screen model.
pub struct Pty {
    pub parser: Arc<Mutex<vt100::Parser>>,
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
    child: Arc<Mutex<Box<dyn Child + Send + Sync>>>,
    identity: ProcessIdentity,
    pub last_output_ms: Arc<AtomicU64>,
    exited: Arc<AtomicBool>,
    size: (u16, u16), // (rows, cols)
    /// Live raw-output subscribers (remote attach). Pruned on send failure.
    subscribers: Subscribers,
}

/// Written to the paused child's stdin once its identity is durably recorded.
const GATE_TOKEN: &str = "baude-runtime-registered";

/// The paused gate, then the user's interactive LOGIN shell — production.
///
/// `-il` is what makes a session usable: PATH from `.zshrc`/`.zprofile` (mise,
/// homebrew) is how the configured `claude` command is found at all.
const GATE_SCRIPT: &str = "IFS= read -r gate || exit 125; [ \"$gate\" = \"$BAUDE_GATE_TOKEN\" ] || exit 126; if [ \"$BAUDE_GATE_MODE\" = command ]; then exec \"$BAUDE_GATE_SHELL\" -il -c \"$BAUDE_GATE_COMMAND\"; else exec \"$BAUDE_GATE_SHELL\" -il; fi";

/// Assemble the gate command, including (in support builds) the child's whole
/// environment. Separated from the spawn so it can run — and abort — before any
/// PTY exists, and so a test can inspect the finished map deterministically.
fn build_gate_command(
    command: Option<&str>,
    env: &[(String, String)],
    cwd: &Path,
) -> CommandBuilder {
    let mut cmd = CommandBuilder::new("/bin/sh");
    cmd.args(["-c", GATE_SCRIPT]);
    cmd.cwd(cwd);

    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
    cmd.env("TERM", "xterm-256color");
    cmd.env("COLORTERM", "truecolor");
    cmd.env("BAUDE_GATE_TOKEN", GATE_TOKEN);
    cmd.env("BAUDE_GATE_SHELL", &shell);
    cmd.env(
        "BAUDE_GATE_MODE",
        if command.is_some() {
            "command"
        } else {
            "interactive"
        },
    );
    cmd.env("BAUDE_GATE_COMMAND", command.unwrap_or_default());
    for (key, value) in env {
        cmd.env(key, value);
    }

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
        let rows = rows.max(2);
        let cols = cols.max(10);

        // Built BEFORE `openpty`, deliberately. In a support build this is
        // where the child's roots are resolved through the guarded resolvers,
        // so an unguarded fixture aborts with no pty, no file descriptor and no
        // process — the escape costs nothing and cannot be half-committed.
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

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone pty reader")?;
        let mut writer = pair
            .master
            .take_writer()
            .context("failed to take pty writer")?;
        if let Err(error) = register(&identity) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error.context("failed to register paused PTY identity"));
        }
        if let Err(error) = writer
            .write_all(format!("{GATE_TOKEN}\n").as_bytes())
            .and_then(|_| writer.flush())
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error).context("failed to release registered PTY gate");
        }

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 2000)));
        let last_output_ms = Arc::new(AtomicU64::new(now_ms()));
        let exited = Arc::new(AtomicBool::new(false));
        let subscribers: Subscribers = Arc::new(Mutex::new(Vec::new()));

        {
            let parser = Arc::clone(&parser);
            let last_output_ms = Arc::clone(&last_output_ms);
            let exited = Arc::clone(&exited);
            let subscribers = Arc::clone(&subscribers);
            std::thread::spawn(move || {
                let mut buf = [0u8; 8192];
                loop {
                    match reader.read(&mut buf) {
                        Ok(0) | Err(_) => {
                            exited.store(true, Ordering::Relaxed);
                            break;
                        }
                        Ok(n) => {
                            // Process and broadcast under the parser lock so
                            // subscribe() can register + snapshot atomically:
                            // a subscriber sees every byte exactly once —
                            // either inside its snapshot or on its channel.
                            if let Ok(mut p) = parser.lock() {
                                p.process(&buf[..n]);
                                if let Ok(mut subs) = subscribers.lock() {
                                    subs.retain(|s| s.send(buf[..n].to_vec()).is_ok());
                                }
                            }
                            last_output_ms.store(now_ms(), Ordering::Relaxed);
                        }
                    }
                }
            });
        }

        Ok(Pty {
            parser,
            master: pair.master,
            writer,
            child: Arc::new(Mutex::new(child)),
            identity,
            last_output_ms,
            exited,
            size: (rows, cols),
            subscribers,
        })
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
        let root =
            std::env::temp_dir().join(format!("baude-registration-gate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
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
        pty.kill_and_wait().unwrap();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn subscribe_snapshot_then_live_bytes() {
        let mut pty = Pty::spawn(
            Some("echo before; cat; echo after"),
            Path::new("/tmp"),
            6,
            60,
        )
        .unwrap();
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
        let mut pty = Pty::spawn(Some("sleep 30"), Path::new("/tmp"), 5, 40).unwrap();
        assert!(!pty.is_exited());
        pty.kill_and_wait().unwrap();
        assert!(pty.is_exited());
    }

    #[test]
    fn kill_and_wait_accepts_and_retries_naturally_exited_child() {
        let mut pty = Pty::spawn(Some("exit 0"), Path::new("/tmp"), 5, 40).unwrap();
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
        let pty = Pty::spawn(Some("echo hi; sleep 30"), Path::new("/tmp"), 5, 40).unwrap();
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
    }
}
