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
    /// PERF-07: the daemon SIGSTOPped this session's child under
    /// `idle_child_policy = "suspend"`. `#[serde(default)]` keeps an older
    /// daemon that omits the field deserializing to `false`.
    #[serde(default)]
    pub suspended: bool,
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
    /// Daemon workspace source label ("explicit", "folder binding", "derived",
    /// or "blank" for default). Comes from the daemon's /info endpoint.
    /// `#[serde(default)]` for backward-compat with older daemons.
    /// Currently stored here for testing; daemon workspace source is primarily
    /// accessed from RemoteSnapshot.
    #[serde(default)]
    #[allow(dead_code)]
    pub workspace_source: Option<String>,
}

impl PartialEq for RemoteInfo {
    /// Compare RemoteInfo for changes relevant to the UI.
    /// Excludes activity field which is UI-only and doesn't affect view changes.
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.name == other.name
            && self.title == other.title
            && self.status == other.status
            && self.waiting_for_ms == other.waiting_for_ms
            && self.model == other.model
            && self.permission_mode == other.permission_mode
            && self.context_used_pct == other.context_used_pct
            && self.rate_5h_used_pct == other.rate_5h_used_pct
            && self.rate_5h_resets_at_unix_s == other.rate_5h_resets_at_unix_s
            && self.branch == other.branch
            && self.session_cost_usd == other.session_cost_usd
            && self.state_source == other.state_source
            && self.last_tool == other.last_tool
            && self.waiting_reason == other.waiting_reason
            && self.archived == other.archived
            && self.suspended == other.suspended
            && self.gsd_active_phase == other.gsd_active_phase
            && self.workspace_source == other.workspace_source
    }
}

#[derive(Clone, Default)]
pub struct RemoteSnapshot {
    pub sessions: Vec<RemoteInfo>,
    /// `now_ms()` at fetch time — waiting timers tick client-side from here.
    pub fetched_ms: u64,
    pub ok: bool,
    /// Daemon's workspace name from /info endpoint (for displaying daemon identity).
    pub daemon_workspace: Option<String>,
    /// Daemon's workspace source from /info endpoint (explicit/bound/derived/blank).
    pub daemon_workspace_source: Option<String>,
    /// Count of collisions detected by daemon since start.
    pub daemon_collision_count: u32,
}

impl RemoteSnapshot {
    /// Compare snapshots for meaningful changes (ignoring time-based fetched_ms).
    /// Returns true if the view-relevant fields are the same.
    pub fn same_view(&self, other: &Self) -> bool {
        self.sessions == other.sessions
            && self.ok == other.ok
            && self.daemon_workspace == other.daemon_workspace
            && self.daemon_workspace_source == other.daemon_workspace_source
            && self.daemon_collision_count == other.daemon_collision_count
    }
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
        let info_url = format!("{base}/info");
        std::thread::spawn(move || loop {
            // Fetch sessions list
            let fetched = ureq::get(&url)
                .timeout(REQUEST_TIMEOUT)
                .call()
                .ok()
                .and_then(|r| r.into_json::<Vec<RemoteInfo>>().ok());

            // Fetch daemon workspace info from /info
            let daemon_info = ureq::get(&info_url)
                .timeout(REQUEST_TIMEOUT)
                .call()
                .ok()
                .and_then(|r| r.into_json::<serde_json::Value>().ok());

            if let Ok(mut d) = shared.lock() {
                match fetched {
                    Some(sessions) => {
                        let daemon_workspace = daemon_info
                            .as_ref()
                            .and_then(|v| v["workspace"].as_str())
                            .map(str::to_string);
                        let daemon_workspace_source = daemon_info
                            .as_ref()
                            .and_then(|v| v["workspace_source"].as_str())
                            .map(str::to_string);
                        let daemon_collision_count = daemon_info
                            .as_ref()
                            .and_then(|v| v["collision_count"].as_u64())
                            .unwrap_or(0)
                            as u32;
                        *d = RemoteSnapshot {
                            sessions,
                            fetched_ms: now_ms(),
                            ok: true,
                            daemon_workspace,
                            daemon_workspace_source,
                            daemon_collision_count,
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

    /// The workspace the daemon reports serving (`GET /info`). `None` when
    /// the daemon predates workspaces (404) or is unreachable.
    fn daemon_workspace(&self) -> Option<String> {
        ureq::get(&format!("{}/info", self.base))
            .timeout(REQUEST_TIMEOUT)
            .call()
            .ok()
            .and_then(|r| r.into_json::<serde_json::Value>().ok())
            .and_then(|v| v["workspace"].as_str().map(str::to_string))
    }

    pub fn create(
        &self,
        repo: &str,
        worktree: Option<&str>,
        name: Option<&str>,
    ) -> Result<(), String> {
        // Cross-workspace guard: a daemon serves exactly one workspace (and
        // thus one backend), so creating a session through a daemon bound to
        // a DIFFERENT workspace would silently spawn it under the wrong
        // backend and pollute the wrong session pool. Refuse loudly instead.
        // A pre-workspace daemon (no /info) is claude-era: allowed only from
        // the claude workspace.
        let local = &baude_core::workspace::active().name;
        match self.daemon_workspace() {
            Some(ws) if ws == *local => {}
            Some(ws) => {
                return Err(format!(
                "daemon serves workspace {ws:?}, not {local:?} — refusing cross-workspace session"
            ))
            }
            None if local == "claude" => {}
            None => {
                return Err(format!(
                "daemon reports no workspace (pre-workspace version) — refusing {local:?} session"
            ))
            }
        }
        let body = serde_json::json!({ "repo": repo, "worktree": worktree, "name": name });
        ureq::post(&format!("{}/sessions", self.base))
            .timeout(REQUEST_TIMEOUT)
            .set("Content-Type", "application/json")
            .send_string(&body.to_string())
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

pub(crate) enum AttachInput {
    Bytes(Vec<u8>),
    #[allow(dead_code)] // constructed by the IO loop; matched in app.rs tests
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

    /// Test-only attach: a live-looking `RemoteAttach` whose input channel
    /// the TEST holds — no socket, no IO thread. Lets app.rs prove the
    /// LINK-04 remote leg (the chord resolves this parser, the same one the
    /// render path draws) and assert that NO byte crosses to the child while
    /// the hints overlay is open, deterministically: `write_input` sends
    /// synchronously into the returned receiver.
    #[cfg(test)]
    pub(crate) fn test_stub(
        remote_id: u64,
        parser: Arc<Mutex<vt100::Parser>>,
    ) -> (RemoteAttach, mpsc::Receiver<AttachInput>) {
        let (tx, rx) = mpsc::channel();
        (
            RemoteAttach {
                remote_id,
                parser,
                closed: Arc::new(AtomicBool::new(false)),
                tx,
                size: (24, 80),
            },
            rx,
        )
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

/// A scripted daemon on a real local listener. Tests in this crate route
/// requests by method and path, so a background poll can never consume a
/// reply scripted for a removal.
#[cfg(test)]
pub(crate) mod test_double {
    use std::collections::VecDeque;
    use std::io::{Read, Write};
    use std::net::{Shutdown, TcpListener, TcpStream};
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex, MutexGuard};
    use std::time::{Duration, Instant};

    #[derive(Clone, Debug)]
    enum ReplyKind {
        Body {
            status: u16,
            content_type: &'static str,
            body: String,
        },
        ClosedUnanswered,
        Held(Duration),
    }

    /// One scripted answer to one request.
    #[derive(Clone, Debug)]
    pub(crate) struct Reply {
        kind: ReplyKind,
        delay: Duration,
    }

    impl Reply {
        /// A JSON body under `status`, as axum's `Json` sends it.
        pub(crate) fn json(status: u16, body: impl Into<String>) -> Reply {
            Reply {
                kind: ReplyKind::Body {
                    status,
                    content_type: "application/json",
                    body: body.into(),
                },
                delay: Duration::ZERO,
            }
        }

        /// A plain-text body under `status`, the daemon's error shape.
        pub(crate) fn text(status: u16, body: impl Into<String>) -> Reply {
            Reply {
                kind: ReplyKind::Body {
                    status,
                    content_type: "text/plain; charset=utf-8",
                    body: body.into(),
                },
                delay: Duration::ZERO,
            }
        }

        /// Wait this long after reading the request before the status line.
        pub(crate) fn delayed(mut self, delay: Duration) -> Reply {
            self.delay = delay;
            self
        }

        /// Read and record the request, then close the connection without
        /// writing a byte: a daemon killed with the request in hand.
        pub(crate) fn closed_unanswered() -> Reply {
            Reply {
                kind: ReplyKind::ClosedUnanswered,
                delay: Duration::ZERO,
            }
        }

        /// Read and record the request, write nothing, and keep the
        /// connection open this long or until the double is dropped: a
        /// handler stuck behind a lock.
        pub(crate) fn held(hold: Duration) -> Reply {
            Reply {
                kind: ReplyKind::Held(hold),
                delay: Duration::ZERO,
            }
        }
    }

    /// The replies one method and path hands out, first in first out.
    pub(crate) struct Route {
        method: String,
        path: String,
        replies: VecDeque<Reply>,
    }

    pub(crate) fn route(method: &str, path: &str, replies: Vec<Reply>) -> Route {
        Route {
            method: method.to_string(),
            path: path.to_string(),
            replies: replies.into(),
        }
    }

    /// One request as the double read it off the socket.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub(crate) struct Recorded {
        pub(crate) method: String,
        pub(crate) path: String,
        /// The `Content-Type` header, empty when the client sent none.
        pub(crate) content_type: String,
        pub(crate) body: String,
    }

    struct State {
        routes: Vec<Route>,
        requests: Vec<Recorded>,
        unscripted: Vec<Recorded>,
    }

    pub(crate) struct Double {
        base: String,
        state: Arc<Mutex<State>>,
        stop: Arc<AtomicBool>,
    }

    impl Double {
        /// `http://127.0.0.1:<port>`, ready for `RemotePoller::idle`.
        pub(crate) fn base(&self) -> String {
            self.base.clone()
        }

        /// Every request read in full, scripted or not.
        pub(crate) fn request_count(&self) -> usize {
            lock(&self.state).requests.len()
        }

        pub(crate) fn requests(&self) -> Vec<Recorded> {
            lock(&self.state).requests.clone()
        }

        /// Requests that matched no route, or a route with no reply left.
        pub(crate) fn unscripted(&self) -> Vec<Recorded> {
            lock(&self.state).unscripted.clone()
        }
    }

    impl Drop for Double {
        fn drop(&mut self) {
            self.stop.store(true, Ordering::Relaxed);
        }
    }

    fn lock(state: &Mutex<State>) -> MutexGuard<'_, State> {
        state
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub(crate) fn serve(routes: Vec<Route>) -> Double {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the daemon double");
        let addr = listener.local_addr().expect("daemon double address");
        listener
            .set_nonblocking(true)
            .expect("nonblocking daemon double listener");
        let state = Arc::new(Mutex::new(State {
            routes,
            requests: Vec::new(),
            unscripted: Vec::new(),
        }));
        let stop = Arc::new(AtomicBool::new(false));
        let accept_state = Arc::clone(&state);
        let accept_stop = Arc::clone(&stop);
        std::thread::spawn(move || {
            while !accept_stop.load(Ordering::Relaxed) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let state = Arc::clone(&accept_state);
                        let stop = Arc::clone(&accept_stop);
                        std::thread::spawn(move || handle(stream, &state, &stop));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(2)),
                }
            }
        });
        Double {
            base: format!("http://{addr}"),
            state,
            stop,
        }
    }

    fn handle(mut stream: TcpStream, state: &Mutex<State>, stop: &AtomicBool) {
        // macOS hands an accepted socket the listener's nonblocking flag.
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let Some(request) = read_request(&mut stream) else {
            return;
        };
        let reply = {
            let mut state = lock(state);
            state.requests.push(request.clone());
            let scripted = state
                .routes
                .iter_mut()
                .find(|route| route.method == request.method && route.path == request.path)
                .and_then(|route| route.replies.pop_front());
            if scripted.is_none() {
                state.unscripted.push(request);
            }
            scripted
        };
        let reply = reply.unwrap_or_else(|| Reply::text(500, "unscripted"));
        pause(reply.delay, stop);
        match reply.kind {
            ReplyKind::Body {
                status,
                content_type,
                body,
            } => {
                let head = format!(
                    "HTTP/1.1 {status} {}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                    reason(status),
                    body.len()
                );
                // A client that already left is not the double's problem.
                let _ = stream.write_all(head.as_bytes());
                let _ = stream.write_all(body.as_bytes());
                let _ = stream.flush();
                let _ = stream.shutdown(Shutdown::Write);
            }
            ReplyKind::ClosedUnanswered => {}
            ReplyKind::Held(hold) => pause(hold, stop),
        }
    }

    /// Sleep in short slices so a dropped double releases its threads.
    fn pause(total: Duration, stop: &AtomicBool) {
        let until = Instant::now() + total;
        while !stop.load(Ordering::Relaxed) {
            let left = until.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return;
            }
            std::thread::sleep(left.min(Duration::from_millis(5)));
        }
    }

    fn reason(status: u16) -> &'static str {
        match status {
            200 => "OK",
            404 => "Not Found",
            409 => "Conflict",
            500 => "Internal Server Error",
            503 => "Service Unavailable",
            _ => "Status",
        }
    }

    /// Read one request: the head up to the blank line, then exactly
    /// `Content-Length` body bytes. `None` when the peer left first.
    fn read_request(stream: &mut TcpStream) -> Option<Recorded> {
        let mut bytes = Vec::new();
        let mut buf = [0u8; 4096];
        let head_end = loop {
            if let Some(at) = find(&bytes, b"\r\n\r\n") {
                break at;
            }
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => return None,
                Ok(n) => bytes.extend_from_slice(&buf[..n]),
            }
        };
        let head = String::from_utf8_lossy(&bytes[..head_end]).into_owned();
        let mut lines = head.split("\r\n");
        let mut request_line = lines.next()?.split(' ');
        let method = request_line.next()?.to_string();
        let path = request_line.next()?.to_string();
        let mut content_length = 0usize;
        let mut content_type = String::new();
        for line in lines {
            let Some((name, value)) = line.split_once(':') else {
                continue;
            };
            if name.eq_ignore_ascii_case("content-length") {
                content_length = value.trim().parse().ok()?;
            } else if name.eq_ignore_ascii_case("content-type") {
                content_type = value.trim().to_string();
            }
        }
        let mut body = bytes[head_end + 4..].to_vec();
        while body.len() < content_length {
            match stream.read(&mut buf) {
                Ok(0) | Err(_) => return None,
                Ok(n) => body.extend_from_slice(&buf[..n]),
            }
        }
        body.truncate(content_length);
        Some(Recorded {
            method,
            path,
            content_type,
            body: String::from_utf8_lossy(&body).into_owned(),
        })
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Instant;

    use baude_core::removal::{
        ClearedItem, ClearingKind, KilledRuntime, PersistenceOutcome, RemovalReport, RemovalTarget,
        RowKey,
    };
    use baude_core::repository::{CheckoutKey, RepositoryKey, StandaloneKey};

    use super::test_double::{route, serve, Double, Reply};
    use super::*;

    /// The key newtypes keep their field private; serde is the one public
    /// road to a value without building a repository graph on disk.
    fn checkout_key(n: u64) -> CheckoutKey {
        serde_json::from_value(serde_json::json!(n)).expect("checkout key")
    }

    fn repository_key(n: u64) -> RepositoryKey {
        serde_json::from_value(serde_json::json!(n)).expect("repository key")
    }

    fn standalone_key(n: u64) -> StandaloneKey {
        serde_json::from_value(serde_json::json!(n)).expect("standalone key")
    }

    /// A preview as the daemon answers `POST /removals/preview`.
    fn planned_report(target: RemovalTarget, rows: Vec<RowKey>) -> RemovalReport {
        RemovalReport {
            target,
            planned: true,
            runtimes_killed: rows
                .first()
                .map(|row| KilledRuntime {
                    row: *row,
                    agent_pid: 91134,
                    shell_pid: None,
                })
                .into_iter()
                .collect(),
            rows_removed: rows,
            paths_deleted: Vec::new(),
            paths_kept: vec![PathBuf::from("/src/baude")],
            already_removed: Vec::new(),
            readers_parked: Vec::new(),
            persistence: PersistenceOutcome::Planned,
            cleared: vec![ClearedItem {
                kind: ClearingKind::SuppressionAdded,
                path: PathBuf::from("/src/baude"),
            }],
            clearing_failures: Vec::new(),
        }
    }

    /// The committed form of a preview, as `POST /removals` answers 200.
    fn committed_report(previewed: &RemovalReport) -> RemovalReport {
        RemovalReport {
            planned: false,
            persistence: PersistenceOutcome::Saved,
            ..previewed.clone()
        }
    }

    /// Every scripted report body goes through the core serde derive, so the
    /// wire shape is real by construction.
    fn wire<T: serde::Serialize>(value: &T) -> String {
        serde_json::to_string(value).expect("serialize a core value")
    }

    /// A `/rows` body in the 21-02 pinned field list. No core type owns this
    /// shape, so the keys are serialized core keys and the names are pinned.
    fn rows_body() -> String {
        serde_json::json!({
            "repositories": [{
                "repository_key": repository_key(1),
                "display_name": "baude",
                "path": "/src/baude",
                "health": {"status": "available"},
                "not_yet_invented": {"nested": [1, 2, 3]},
                "checkouts": [
                    {
                        "checkout_key": checkout_key(10),
                        "repository_key": repository_key(1),
                        "display_name": "baude",
                        "branch": "main",
                        "path": "/src/baude",
                        "role": "main",
                        "managed_by_baude": false,
                        "archived": false,
                        "session_id": 7
                    },
                    {
                        "checkout_key": checkout_key(11),
                        "repository_key": repository_key(1),
                        "display_name": "topic",
                        "branch": "feature/topic",
                        "path": "/src/.baude-worktrees/topic",
                        "role": "managed_branch",
                        "managed_by_baude": true,
                        "archived": false,
                        "session_id": null
                    }
                ]
            }],
            "standalone": [{
                "standalone_key": standalone_key(20),
                "display_name": "notes",
                "path": "/src/notes",
                "archived": false,
                "session_id": 9
            }]
        })
        .to_string()
    }

    fn rows_fixture() -> RemoteRows {
        serde_json::from_str(&rows_body()).expect("the pinned /rows field list parses")
    }

    /// A poller with no background thread, aimed at the double. The return
    /// type is spelled out so a missing removal method is named as such.
    fn idle_poller(double: &Double) -> RemotePoller {
        RemotePoller::idle(double.base())
    }

    /// Every test that scripts the double ends here: nothing it did not
    /// script was asked for, and nothing was sent twice.
    fn assert_settled(double: &Double, calls: usize) {
        assert_eq!(double.unscripted(), Vec::new(), "unscripted requests");
        assert_eq!(double.request_count(), calls, "{:?}", double.requests());
    }

    #[test]
    fn removal_preview_returns_the_report_the_daemon_sent() {
        let target = RemovalTarget::Checkout(checkout_key(10));
        let report = planned_report(target, vec![RowKey::Checkout(checkout_key(10))]);
        let body = wire(&report);
        // A silent change in core's serde shape is seen here, not in the UI.
        assert!(
            body.contains(r#""persistence":{"outcome":"planned"}"#),
            "{body}"
        );
        assert!(
            body.contains(r#""rows_removed":[{"checkout":10}]"#),
            "{body}"
        );
        let double = serve(vec![route(
            "POST",
            "/removals/preview",
            vec![Reply::json(200, body)],
        )]);
        let poller = idle_poller(&double);

        let reply = poller.preview_removal(&target);

        assert_eq!(reply, RemovalReply::Report(report));
        let requests = double.requests();
        assert_eq!(requests.len(), 1, "{requests:?}");
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].path, "/removals/preview");
        assert_eq!(requests[0].content_type, "application/json");
        let sent: RemovalTarget =
            serde_json::from_str(&requests[0].body).expect("the body is a RemovalTarget");
        assert_eq!(sent, target);
        assert_settled(&double, 1);
    }

    #[test]
    fn removal_execute_returns_the_committed_report() {
        let target = RemovalTarget::Checkout(checkout_key(10));
        let previewed = planned_report(target, vec![RowKey::Checkout(checkout_key(10))]);
        let committed = committed_report(&previewed);
        let double = serve(vec![route(
            "POST",
            "/removals",
            vec![Reply::json(200, wire(&committed))],
        )]);
        let poller = idle_poller(&double);

        let reply = poller.execute_removal(&previewed);

        assert_eq!(reply, RemovalReply::Report(committed));
        let requests = double.requests();
        assert_eq!(requests.len(), 1, "{requests:?}");
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].path, "/removals");
        assert_eq!(requests[0].content_type, "application/json");
        let sent: RemovalReport =
            serde_json::from_str(&requests[0].body).expect("the body is the previewed report");
        assert_eq!(sent, previewed);
        assert_settled(&double, 1);
    }

    #[test]
    fn removal_execute_409_returns_the_fresh_report_as_drift() {
        let target = RemovalTarget::Repository(repository_key(1));
        let previewed = planned_report(
            target,
            vec![
                RowKey::Checkout(checkout_key(10)),
                RowKey::Repository(repository_key(1)),
            ],
        );
        let mut fresh = planned_report(
            target,
            vec![
                RowKey::Checkout(checkout_key(10)),
                RowKey::Checkout(checkout_key(11)),
                RowKey::Repository(repository_key(1)),
            ],
        );
        fresh.already_removed = vec![PathBuf::from("/src/.baude-worktrees/topic")];
        assert_ne!(fresh, previewed);
        let double = serve(vec![route(
            "POST",
            "/removals",
            vec![Reply::json(409, wire(&fresh))],
        )]);
        let poller = idle_poller(&double);

        let reply = poller.execute_removal(&previewed);

        assert_eq!(reply, RemovalReply::Drift(fresh));
        assert_settled(&double, 1);
    }

    #[test]
    fn removal_execute_503_carries_the_daemon_text() {
        let target = RemovalTarget::Checkout(checkout_key(10));
        let previewed = planned_report(target, vec![RowKey::Checkout(checkout_key(10))]);
        let cases = [
            (
                503,
                "failed to save repository state: No space left on device",
            ),
            (500, "failed to terminate exact PTY process group"),
        ];
        for (status, text) in cases {
            let double = serve(vec![route(
                "POST",
                "/removals",
                vec![Reply::text(status, text)],
            )]);
            let poller = idle_poller(&double);

            let reply = poller.execute_removal(&previewed);

            assert_eq!(
                reply,
                RemovalReply::Refused {
                    status,
                    message: text.to_string(),
                }
            );
            assert_settled(&double, 1);
        }
    }

    #[test]
    fn removal_404_is_gone_not_refused() {
        let target = RemovalTarget::Checkout(checkout_key(10));
        let previewed = planned_report(target, vec![RowKey::Checkout(checkout_key(10))]);
        let double = serve(vec![
            route(
                "POST",
                "/removals/preview",
                vec![Reply::text(404, "unknown removal target\n")],
            ),
            route(
                "POST",
                "/removals",
                vec![Reply::text(404, "unknown removal target\n")],
            ),
        ]);
        let poller = idle_poller(&double);

        let gone = RemovalReply::Gone {
            message: "unknown removal target".to_string(),
        };
        assert_eq!(poller.preview_removal(&target), gone);
        assert_eq!(poller.execute_removal(&previewed), gone);
        assert_settled(&double, 2);
    }

    #[test]
    fn removal_503_busy_text_is_kept_from_its_first_word() {
        let target = RemovalTarget::Checkout(checkout_key(10));
        let previewed = planned_report(target, vec![RowKey::Checkout(checkout_key(10))]);
        let double = serve(vec![route(
            "POST",
            "/removals",
            vec![Reply::text(503, "repository busy: /src/baude")],
        )]);
        let poller = idle_poller(&double);

        match poller.execute_removal(&previewed) {
            RemovalReply::Refused { status, message } => {
                assert_eq!(status, 503);
                assert!(message.starts_with("repository busy:"), "{message}");
                assert_eq!(message, "repository busy: /src/baude");
            }
            other => panic!("expected Refused, got {other:?}"),
        }
        assert_settled(&double, 1);
    }

    #[test]
    fn removal_409_with_a_non_report_body_is_refused_not_a_panic() {
        let target = RemovalTarget::Checkout(checkout_key(10));
        let previewed = planned_report(target, vec![RowKey::Checkout(checkout_key(10))]);
        let double = serve(vec![route(
            "POST",
            "/removals",
            vec![Reply::text(409, "conflict, but not a report")],
        )]);
        let poller = idle_poller(&double);

        match poller.execute_removal(&previewed) {
            RemovalReply::Refused { status, .. } => assert_eq!(status, 409),
            other => panic!("expected Refused, got {other:?}"),
        }
        assert_settled(&double, 1);
    }

    #[test]
    fn removal_daemon_text_is_sanitized_and_capped() {
        let target = RemovalTarget::Checkout(checkout_key(10));
        let previewed = planned_report(target, vec![RowKey::Checkout(checkout_key(10))]);
        let hostile = format!("\u{1b}[31mred\nsecond   line\t{}", "x".repeat(5000));
        let double = serve(vec![route(
            "POST",
            "/removals",
            vec![Reply::text(500, hostile)],
        )]);
        let poller = idle_poller(&double);

        match poller.execute_removal(&previewed) {
            RemovalReply::Refused { status, message } => {
                assert_eq!(status, 500);
                assert!(!message.chars().any(char::is_control), "{message:?}");
                assert!(message.chars().count() <= 512, "{}", message.len());
                assert!(message.starts_with("[31mred second line x"), "{message}");
            }
            other => panic!("expected Refused, got {other:?}"),
        }
        assert_settled(&double, 1);
    }

    #[test]
    fn removal_oversize_json_body_is_refused() {
        let target = RemovalTarget::Checkout(checkout_key(10));
        let previewed = planned_report(target, vec![RowKey::Checkout(checkout_key(10))]);
        // A report that would parse, so only the cap can refuse it.
        let mut oversize = committed_report(&previewed);
        oversize.paths_kept = vec![PathBuf::from(format!("/{}", "a".repeat(1_100_000)))];
        let body = wire(&oversize);
        assert!(body.len() > 1024 * 1024);
        let double = serve(vec![route(
            "POST",
            "/removals",
            vec![Reply::json(200, body)],
        )]);
        let poller = idle_poller(&double);

        match poller.execute_removal(&previewed) {
            RemovalReply::Refused { status, message } => {
                assert_eq!(status, 200);
                assert!(message.chars().count() <= 512, "{}", message.len());
            }
            other => panic!("expected Refused, got {other:?}"),
        }
        assert_settled(&double, 1);
    }

    #[test]
    fn rows_deserialize_from_the_pinned_field_list() {
        // Keys are bare numbers on the wire.
        assert_eq!(
            serde_json::to_value(checkout_key(10)).expect("key"),
            serde_json::json!(10)
        );
        let rows = rows_fixture();

        assert_eq!(rows.repositories.len(), 1);
        let repository = &rows.repositories[0];
        assert_eq!(repository.repository_key, repository_key(1));
        assert_eq!(repository.display_name, "baude");
        assert_eq!(repository.path, "/src/baude");
        assert_eq!(repository.checkouts.len(), 2);
        let live = &repository.checkouts[0];
        assert_eq!(live.checkout_key, checkout_key(10));
        assert_eq!(live.repository_key, repository_key(1));
        assert_eq!(live.branch.as_deref(), Some("main"));
        assert_eq!(live.session_id, Some(7));
        assert!(!live.managed_by_baude);
        let sessionless = &repository.checkouts[1];
        assert_eq!(sessionless.checkout_key, checkout_key(11));
        assert_eq!(sessionless.session_id, None);
        assert!(sessionless.managed_by_baude);
        assert_eq!(rows.standalone.len(), 1);
        assert_eq!(rows.standalone[0].standalone_key, standalone_key(20));
        assert_eq!(rows.standalone[0].display_name, "notes");
        assert_eq!(rows.standalone[0].session_id, Some(9));

        // A parent with no children, and a daemon variant this client has
        // never heard of, must not fail the whole parse.
        let sparse: RemoteRows = serde_json::from_str(
            r#"{"repositories":[{"repository_key":3,"checkouts":[],
                "health":{"status":"unavailable","cause":{"kind":"from_the_future"}}}]}"#,
        )
        .expect("a sparse /rows body parses");
        assert!(sparse.repositories[0].checkouts.is_empty());
        assert!(sparse.standalone.is_empty());
        assert_eq!(RemoteRows::default(), RemoteRows::default());
    }

    #[test]
    fn removal_target_is_found_only_through_the_rows_join() {
        let rows = rows_fixture();

        assert_eq!(
            rows.target_for_session(7),
            Some(RemovalTarget::Checkout(checkout_key(10)))
        );
        assert_eq!(
            rows.target_for_session(9),
            Some(RemovalTarget::Standalone(standalone_key(20)))
        );
        assert_eq!(rows.target_for_session(8), None);
        assert_eq!(RemoteRows::default().target_for_session(7), None);

        assert_eq!(
            rows.repository_target_for_session(7),
            Some(RemovalTarget::Repository(repository_key(1)))
        );
        assert_eq!(rows.repository_target_for_session(9), None);
        assert_eq!(rows.repository_target_for_session(8), None);

        let label = rows
            .label(&RowKey::Checkout(checkout_key(11)))
            .expect("a sessionless checkout still has a name");
        assert_eq!(label.display_name, "topic");
        assert_eq!(label.branch.as_deref(), Some("feature/topic"));
        assert_eq!(label.path, "/src/.baude-worktrees/topic");
        assert_eq!(label.session_id, None);
        let parent = rows
            .label(&RowKey::Repository(repository_key(1)))
            .expect("the parent has a name");
        assert_eq!(parent.display_name, "baude");
        assert_eq!(parent.branch, None);
        let standalone = rows
            .label(&RowKey::Standalone(standalone_key(20)))
            .expect("the standalone row has a name");
        assert_eq!(standalone.display_name, "notes");
        assert_eq!(standalone.session_id, Some(9));
        assert_eq!(rows.label(&RowKey::Checkout(checkout_key(99))), None);
    }

    #[test]
    fn double_reports_unscripted_requests() {
        let target = RemovalTarget::Checkout(checkout_key(10));
        let report = planned_report(target, vec![RowKey::Checkout(checkout_key(10))]);
        let double = serve(vec![route(
            "POST",
            "/removals/preview",
            vec![Reply::json(200, wire(&report))],
        )]);
        let poller = idle_poller(&double);

        // No route for the execute path: answered 500 and reported.
        let reply = poller.execute_removal(&report);
        assert_eq!(
            reply,
            RemovalReply::Refused {
                status: 500,
                message: "unscripted".to_string(),
            }
        );
        let unscripted = double.unscripted();
        assert_eq!(unscripted.len(), 1, "{unscripted:?}");
        assert_eq!(unscripted[0].method, "POST");
        assert_eq!(unscripted[0].path, "/removals");

        // The scripted reply was not consumed by the stray request.
        assert_eq!(
            poller.preview_removal(&target),
            RemovalReply::Report(report.clone())
        );
        // An exhausted route is unscripted too.
        match poller.preview_removal(&target) {
            RemovalReply::Refused { status, .. } => assert_eq!(status, 500),
            other => panic!("expected Refused, got {other:?}"),
        }
        assert_eq!(double.unscripted().len(), 2);
        assert_eq!(double.request_count(), 3);
    }

    #[test]
    fn removal_closed_without_an_answer_is_unreachable_for_preview_and_indeterminate_for_execute() {
        let started = Instant::now();
        let target = RemovalTarget::Checkout(checkout_key(10));
        let previewed = planned_report(target, vec![RowKey::Checkout(checkout_key(10))]);
        let double = serve(vec![
            route(
                "POST",
                "/removals/preview",
                vec![Reply::closed_unanswered()],
            ),
            route("POST", "/removals", vec![Reply::closed_unanswered()]),
        ]);
        let poller = idle_poller(&double);

        let preview = poller.preview_removal(&target);
        assert!(
            matches!(preview, RemovalReply::Unreachable { .. }),
            "{preview:?}"
        );
        let execute = poller.execute_removal(&previewed);
        assert!(
            matches!(
                execute,
                RemovalReply::Indeterminate {
                    rows_gone: false,
                    ..
                }
            ),
            "{execute:?}"
        );

        // Exactly one POST per call, each with its whole body: the HTTP
        // layer resent neither over the closed connection.
        let requests = double.requests();
        assert_eq!(requests.len(), 2, "{requests:?}");
        assert_eq!(requests[0].method, "POST");
        assert_eq!(requests[0].path, "/removals/preview");
        assert_eq!(
            serde_json::from_str::<RemovalTarget>(&requests[0].body).expect("whole target body"),
            target
        );
        assert_eq!(requests[1].method, "POST");
        assert_eq!(requests[1].path, "/removals");
        assert_eq!(
            serde_json::from_str::<RemovalReport>(&requests[1].body).expect("whole report body"),
            previewed
        );
        assert_settled(&double, 2);
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
    }

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

    /// A daemon that includes workspace_source (WSPC-05) deserializes it
    /// correctly. An older daemon that omits it defaults to None.
    #[test]
    fn remote_info_deserializes_workspace_source() {
        // With workspace_source field
        let json = r#"{"id":7,"name":"alpha","status":"working",
            "workspace_source":"derived"}"#;
        let r: RemoteInfo = serde_json::from_str(json).expect("deserialize with workspace_source");
        assert_eq!(r.workspace_source.as_deref(), Some("derived"));

        // Without workspace_source field (older daemon) — defaults to None
        let json_old = r#"{"id":7,"name":"alpha","status":"working"}"#;
        let r_old: RemoteInfo = serde_json::from_str(json_old)
            .expect("deserialize without workspace_source (backward-compat)");
        assert_eq!(r_old.workspace_source, None);
    }
}
