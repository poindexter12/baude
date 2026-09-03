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
        let pty_system = native_pty_system();
        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("failed to open pty")?;

        const GATE_TOKEN: &str = "baude-runtime-registered";
        const GATE_SCRIPT: &str = "IFS= read -r gate || exit 125; [ \"$gate\" = \"$BAUDE_GATE_TOKEN\" ] || exit 126; if [ \"$BAUDE_GATE_MODE\" = command ]; then exec \"$BAUDE_GATE_SHELL\" -il -c \"$BAUDE_GATE_COMMAND\"; else exec \"$BAUDE_GATE_SHELL\" -il; fi";
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let mut cmd = CommandBuilder::new("/bin/sh");
        cmd.args(["-c", GATE_SCRIPT]);
        cmd.cwd(cwd);
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
    use std::time::Duration;

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
