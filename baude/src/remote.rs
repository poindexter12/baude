//! bauded client: poll a daemon's session list over REST and attach to its
//! sessions over the raw PTY websocket. Everything is synchronous — a poll
//! thread and an attach IO thread feed shared state the draw loop reads,
//! mirroring how local PTYs work.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use serde::Deserialize;

use baude_core::meta::HookEvent;
use baude_core::pty::now_ms;
use baude_core::vt100;

const POLL_SECS: u64 = 3;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(3);

/// One row of the daemon's `GET /sessions`.
#[derive(Deserialize, Clone, Default)]
pub struct RemoteInfo {
    pub id: u64,
    pub name: String,
    pub title: Option<String>,
    pub status: String,
    pub waiting_for_ms: Option<u64>,
    pub model: Option<String>,
    pub permission_mode: Option<String>,
    pub context_used_pct: Option<u8>,
    /// The session's active 5h rate-limit window (used % + reset), mirrored
    /// from `SessionInfo`. `#[serde(default)]` so an older daemon that omits
    /// these deserializes to `None` (back-compat, like the fields below).
    #[serde(default)]
    pub rate_5h_used_pct: Option<u8>,
    #[serde(default)]
    pub rate_5h_resets_at_unix_s: Option<u64>,
    pub branch: Option<String>,
    pub session_cost_usd: Option<f64>,
    /// Which source decided `status` ("hook"/"session-file"/"silence"); shown
    /// in the remote info overlay so a regression to silence is observable.
    #[serde(default)]
    pub state_source: Option<String>,
    /// The last tool the daemon-managed session ran, if any.
    #[serde(default)]
    pub last_tool: Option<String>,
    /// PERM-04: why the session is waiting — `"permission"` / `"input"` /
    /// absent. `#[serde(default)]` keeps an older daemon (which omits the
    /// field) deserializing to `None` (backward-compat, mirroring the fields
    /// above).
    #[serde(default)]
    pub waiting_reason: Option<String>,
    #[serde(default)]
    pub archived: bool,
    /// Recent (~30) tool-activity events bundled into the `/sessions` poll so
    /// the remote activity overlay needs no extra round-trip. Defaults to an
    /// empty Vec against an older daemon that omits the field (backward-compat,
    /// mirroring the `#[serde(default)]` fields above).
    #[serde(default)]
    pub activity: Vec<HookEvent>,
    /// BL-03: compact GSD active phase (e.g. `4`), surfaced in the remote line
    /// so daemon sessions show GSD state like local ones. `#[serde(default)]`
    /// for back-compat with a daemon that omits it.
    #[serde(default)]
    pub gsd_active_phase: Option<String>,
}

#[derive(Clone, Default)]
pub struct RemoteSnapshot {
    pub sessions: Vec<RemoteInfo>,
    /// `now_ms()` at fetch time — waiting timers tick client-side from here.
    pub fetched_ms: u64,
    pub ok: bool,
}

/// Background poller for one daemon.
pub struct RemotePoller {
    pub base: String,
    data: Arc<Mutex<RemoteSnapshot>>,
}

impl RemotePoller {
    pub fn start(base: String) -> RemotePoller {
        let base = base.trim_end_matches('/').to_string();
        let data = Arc::new(Mutex::new(RemoteSnapshot::default()));
        let shared = Arc::clone(&data);
        let url = format!("{base}/sessions");
        std::thread::spawn(move || loop {
            let fetched = ureq::get(&url)
                .timeout(REQUEST_TIMEOUT)
                .call()
                .ok()
                .and_then(|r| r.into_json::<Vec<RemoteInfo>>().ok());
            if let Ok(mut d) = shared.lock() {
                match fetched {
                    Some(sessions) => {
                        *d = RemoteSnapshot {
                            sessions,
                            fetched_ms: now_ms(),
                            ok: true,
                        }
                    }
                    // Keep the stale list visible, just mark it offline.
                    None => d.ok = false,
                }
            }
            std::thread::sleep(Duration::from_secs(POLL_SECS));
        });
        RemotePoller { base, data }
    }

    pub fn snapshot(&self) -> RemoteSnapshot {
        self.data.lock().map(|d| d.clone()).unwrap_or_default()
    }

    pub fn delete(&self, id: u64) -> Result<(), String> {
        ureq::delete(&format!("{}/sessions/{id}", self.base))
            .timeout(REQUEST_TIMEOUT)
            .call()
            .map(|_| ())
            .map_err(|e| short_err(&e))
    }

    pub fn restart(&self, id: u64) -> Result<(), String> {
        ureq::post(&format!("{}/sessions/{id}/restart", self.base))
            .timeout(REQUEST_TIMEOUT)
            .call()
            .map(|_| ())
            .map_err(|e| short_err(&e))
    }

    pub fn set_archived(&self, id: u64, archived: bool) -> Result<(), String> {
        let action = if archived { "archive" } else { "unarchive" };
        ureq::post(&format!("{}/sessions/{id}/{action}", self.base))
            .timeout(REQUEST_TIMEOUT)
            .call()
            .map(|_| ())
            .map_err(|e| short_err(&e))
    }
}

fn short_err(e: &ureq::Error) -> String {
    match e {
        ureq::Error::Status(code, _) => format!("daemon said {code}"),
        ureq::Error::Transport(t) => t
            .message()
            .map(str::to_string)
            .unwrap_or_else(|| "connection failed".into()),
    }
}

enum AttachInput {
    Bytes(Vec<u8>),
    Resize(u16, u16),
}

/// A live raw attach to one remote session: a websocket IO thread feeds a
/// local vt100 parser (rendered exactly like a local PTY) and drains queued
/// input/resizes back to the daemon.
pub struct RemoteAttach {
    pub remote_id: u64,
    pub parser: Arc<Mutex<vt100::Parser>>,
    closed: Arc<AtomicBool>,
    tx: mpsc::Sender<AttachInput>,
    size: (u16, u16),
}

impl RemoteAttach {
    pub fn connect(
        base: &str,
        remote_id: u64,
        rows: u16,
        cols: u16,
    ) -> Result<RemoteAttach, String> {
        let rows = rows.max(2);
        let cols = cols.max(10);
        let ws_base = if let Some(rest) = base.strip_prefix("https://") {
            format!("wss://{rest}")
        } else if let Some(rest) = base.strip_prefix("http://") {
            format!("ws://{rest}")
        } else {
            format!("ws://{base}")
        };
        let url = format!("{ws_base}/sessions/{remote_id}/pty");
        let (mut socket, _) = tungstenite::connect(&url).map_err(|e| e.to_string())?;
        // Short read timeout so the IO loop interleaves reads with queued writes.
        let timeout = Some(Duration::from_millis(30));
        match socket.get_mut() {
            tungstenite::stream::MaybeTlsStream::Plain(s) => {
                let _ = s.set_read_timeout(timeout);
            }
            tungstenite::stream::MaybeTlsStream::Rustls(t) => {
                let _ = t.get_mut().set_read_timeout(timeout);
            }
            _ => {}
        }

        let parser = Arc::new(Mutex::new(vt100::Parser::new(rows, cols, 2000)));
        let closed = Arc::new(AtomicBool::new(false));
        let (tx, rx) = mpsc::channel::<AttachInput>();
        // Ask the daemon to match our pane before anything renders.
        let _ = tx.send(AttachInput::Resize(rows, cols));

        {
            let parser = Arc::clone(&parser);
            let closed = Arc::clone(&closed);
            std::thread::spawn(move || {
                loop {
                    if closed.load(Ordering::Relaxed) {
                        let _ = socket.close(None);
                        break;
                    }
                    let mut dead = false;
                    while let Ok(item) = rx.try_recv() {
                        let res = match item {
                            AttachInput::Bytes(b) => {
                                socket.send(tungstenite::Message::Binary(b.into()))
                            }
                            AttachInput::Resize(r, c) => socket.send(tungstenite::Message::Text(
                                format!("{{\"resize\":[{r},{c}]}}").into(),
                            )),
                        };
                        if res.is_err() {
                            dead = true;
                            break;
                        }
                    }
                    if dead {
                        break;
                    }
                    match socket.read() {
                        Ok(tungstenite::Message::Binary(bytes)) => {
                            if let Ok(mut p) = parser.lock() {
                                p.process(&bytes);
                            }
                        }
                        Ok(tungstenite::Message::Close(_)) => break,
                        Ok(_) => {}
                        Err(tungstenite::Error::Io(e))
                            if matches!(
                                e.kind(),
                                std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut
                            ) => {}
                        Err(_) => break,
                    }
                }
                closed.store(true, Ordering::Relaxed);
            });
        }

        Ok(RemoteAttach {
            remote_id,
            parser,
            closed,
            tx,
            size: (rows, cols),
        })
    }

    pub fn write_input(&self, bytes: &[u8]) {
        if !bytes.is_empty() {
            let _ = self.tx.send(AttachInput::Bytes(bytes.to_vec()));
        }
    }

    pub fn resize(&mut self, rows: u16, cols: u16) {
        let rows = rows.max(2);
        let cols = cols.max(10);
        if self.size == (rows, cols) {
            return;
        }
        self.size = (rows, cols);
        if let Ok(mut p) = self.parser.lock() {
            p.set_size(rows, cols);
        }
        let _ = self.tx.send(AttachInput::Resize(rows, cols));
    }

    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::Relaxed)
    }
}

impl Drop for RemoteAttach {
    fn drop(&mut self) {
        self.closed.store(true, Ordering::Relaxed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An older daemon that omits `activity` must still deserialize — the
    /// field defaults to an empty Vec (T-03-11 backward-compat). The rest of
    /// the `#[serde(default)]` optionals are also exercised by their absence.
    #[test]
    fn remote_info_deserializes_without_activity() {
        let json = r#"{"id":7,"name":"alpha","title":null,"status":"working",
            "waiting_for_ms":null,"model":null,"permission_mode":null,
            "context_used_pct":null,"branch":null,"session_cost_usd":null}"#;
        let r: RemoteInfo = serde_json::from_str(json).expect("deserialize without activity");
        assert_eq!(r.id, 7);
        assert!(r.activity.is_empty(), "missing activity defaults to empty");
    }

    /// A daemon that bundles `activity` deserializes it into the overlay's
    /// source Vec — the remote branch reads this directly, no round-trip.
    #[test]
    fn remote_info_deserializes_with_activity() {
        let json = r#"{"id":7,"name":"alpha","status":"working","activity":[
            {"event":"PostToolUse","tool":"Bash","ts":1000},
            {"event":"Notification","notification_type":"permission","ts":2000}
        ]}"#;
        let r: RemoteInfo = serde_json::from_str(json).expect("deserialize with activity");
        assert_eq!(r.activity.len(), 2);
        assert_eq!(r.activity[0].event, "PostToolUse");
        assert_eq!(r.activity[0].tool.as_deref(), Some("Bash"));
        assert_eq!(
            r.activity[1].notification_type.as_deref(),
            Some("permission")
        );
    }
}
