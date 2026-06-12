use std::io::{Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, Child, CommandBuilder, MasterPty, PtySize};

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

        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".to_string());
        let mut cmd = CommandBuilder::new(&shell);
        match command {
            Some(c) => {
                cmd.args(["-il", "-c", c]);
            }
            None => {
                cmd.args(["-il"]);
            }
        }
        cmd.cwd(cwd);
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");

        let child = pair
            .slave
            .spawn_command(cmd)
            .context("failed to spawn command in pty")?;
        drop(pair.slave);

        let mut reader = pair
            .master
            .try_clone_reader()
            .context("failed to clone pty reader")?;
        let writer = pair
            .master
            .take_writer()
            .context("failed to take pty writer")?;

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

    pub fn pid(&self) -> Option<u32> {
        self.child.lock().ok().and_then(|c| c.process_id())
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

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
    fn output_timestamp_goes_idle() {
        let pty = Pty::spawn(Some("echo hi; sleep 5"), Path::new("/tmp"), 5, 40).unwrap();
        std::thread::sleep(Duration::from_millis(3500));
        let last = pty.last_output_ms.load(Ordering::Relaxed);
        let idle = now_ms().saturating_sub(last);
        assert!(
            idle >= 2000,
            "expected >=2000ms idle, got {idle}ms (output kept arriving)"
        );
        assert!(!pty.is_exited(), "child should still be sleeping");
    }
}
