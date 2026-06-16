//! REST + SSE surface. Security is the tailnet (see docs/remote-daemon-plan.md):
//! bind the Tailscale interface, no auth layer here.

use std::convert::Infallible;
use std::path::PathBuf;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use baude_core::meta::{HookEvent, ACTIVITY_CAP};

use crate::manager::{lock, PermissionView, SessionInfo, Shared};
use crate::transcript::{self, ChatMessage, EventTail, Tail};

pub fn router(state: Shared) -> Router {
    Router::new()
        .merge(crate::web::router())
        .route("/sessions", get(list_sessions).post(create_session))
        .route("/sessions/{id}", get(get_session).delete(delete_session))
        .route(
            "/sessions/{id}/messages",
            get(get_messages).post(post_message),
        )
        .route("/sessions/{id}/interrupt", post(interrupt))
        .route("/sessions/{id}/restart", post(restart))
        .route("/sessions/{id}/archive", post(archive))
        .route("/sessions/{id}/unarchive", post(unarchive))
        .route("/sessions/{id}/queue", get(get_queue))
        .route("/sessions/{id}/screen", get(get_screen))
        .route("/sessions/{id}/keys", post(post_keys))
        .route("/sessions/{id}/pty", get(pty_ws))
        .route("/sessions/{id}/stream", get(stream))
        .route("/sessions/{id}/event", post(post_event))
        .route(
            "/sessions/{id}/permission",
            get(get_permission).post(post_permission),
        )
        .route("/sessions/{id}/activity", get(get_activity))
        .route("/sessions/{id}/activity-stream", get(activity_stream))
        .with_state(state)
}

/// Web Push subscription endpoints — separate state, merged by main.
pub fn push_router(state: crate::push::SharedPush) -> Router {
    use crate::push::{lock as plock, Subscription};

    #[derive(Deserialize)]
    struct SubKeys {
        p256dh: String,
        auth: String,
    }
    #[derive(Deserialize)]
    struct SubscribeBody {
        endpoint: String,
        keys: SubKeys,
    }
    #[derive(Deserialize)]
    struct UnsubscribeBody {
        endpoint: String,
    }

    Router::new()
        .route(
            "/push/key",
            get(|State(s): State<crate::push::SharedPush>| async move {
                Json(serde_json::json!({ "key": plock(&s).vapid.public_b64 }))
            }),
        )
        .route(
            "/push/subscribe",
            post(
                |State(s): State<crate::push::SharedPush>, Json(b): Json<SubscribeBody>| async move {
                    if !b.endpoint.starts_with("https://") {
                        return (StatusCode::BAD_REQUEST, "endpoint must be https").into_response();
                    }
                    plock(&s).subscribe(Subscription {
                        endpoint: b.endpoint,
                        p256dh: b.keys.p256dh,
                        auth: b.keys.auth,
                    });
                    StatusCode::CREATED.into_response()
                },
            )
            .delete(
                |State(s): State<crate::push::SharedPush>, Json(b): Json<UnsubscribeBody>| async move {
                    if plock(&s).unsubscribe(&b.endpoint) {
                        StatusCode::NO_CONTENT
                    } else {
                        StatusCode::NOT_FOUND
                    }
                },
            ),
        )
        .with_state(state)
}

type ApiError = (StatusCode, String);

fn not_found(e: anyhow::Error) -> ApiError {
    (StatusCode::NOT_FOUND, e.to_string())
}

async fn list_sessions(State(state): State<Shared>) -> Json<Vec<SessionInfo>> {
    Json(lock(&state).list())
}

async fn get_session(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<Json<SessionInfo>, ApiError> {
    lock(&state)
        .info(id)
        .map(Json)
        .ok_or((StatusCode::NOT_FOUND, format!("no session {id}")))
}

#[derive(Deserialize)]
struct CreateBody {
    repo: String,
    /// Branch name — when set, the session runs in a managed git worktree.
    worktree: Option<String>,
    name: Option<String>,
}

async fn create_session(
    State(state): State<Shared>,
    Json(body): Json<CreateBody>,
) -> Result<(StatusCode, Json<SessionInfo>), ApiError> {
    let info = lock(&state)
        .create(&body.repo, body.worktree.as_deref(), body.name.as_deref())
        .map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;
    Ok((StatusCode::CREATED, Json(info)))
}

async fn delete_session(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    lock(&state).remove(id).map_err(not_found)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
struct MessagesQuery {
    /// Return only messages after this uuid (exclusive).
    after: Option<String>,
}

async fn get_messages(
    State(state): State<Shared>,
    Path(id): Path<u64>,
    Query(q): Query<MessagesQuery>,
) -> Result<Json<Vec<ChatMessage>>, ApiError> {
    let path = lock(&state).transcript_path(id).map_err(not_found)?;
    let Some(path) = path else {
        // Session exists but Claude hasn't written a transcript yet.
        return Ok(Json(vec![]));
    };
    let messages = transcript::parse_file(&path);
    Ok(Json(match &q.after {
        Some(uuid) => transcript::after(messages, uuid),
        None => messages,
    }))
}

#[derive(Deserialize)]
struct ActivityQuery {
    /// Max number of recent events to return. Clamped to `ACTIVITY_CAP`
    /// (Security V5 — never honor an oversized/unbounded limit; T-03-06).
    limit: Option<usize>,
}

/// Recent hook events for a session as a JSON array (newest at back). The
/// `?limit` is defaulted to and clamped at `ACTIVITY_CAP` so a hostile/oversized
/// value never allocates more than the ring holds. Unknown id → 404, never 500.
async fn get_activity(
    State(state): State<Shared>,
    Path(id): Path<u64>,
    Query(q): Query<ActivityQuery>,
) -> Result<Json<Vec<HookEvent>>, ApiError> {
    let limit = q.limit.unwrap_or(ACTIVITY_CAP).min(ACTIVITY_CAP);
    let act = lock(&state).activity(id, limit).map_err(not_found)?;
    Ok(Json(act))
}

#[derive(Deserialize)]
struct PostBody {
    text: String,
}

async fn post_message(
    State(state): State<Shared>,
    Path(id): Path<u64>,
    Json(body): Json<PostBody>,
) -> Result<StatusCode, ApiError> {
    if body.text.trim().is_empty() {
        return Err((StatusCode::BAD_REQUEST, "empty message".into()));
    }
    lock(&state).post_message(id, &body.text).map_err(|e| {
        let msg = e.to_string();
        if msg.starts_with("no session") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            // exists but can't take input right now (starting / exited)
            (StatusCode::CONFLICT, msg)
        }
    })?;
    Ok(StatusCode::ACCEPTED)
}

async fn interrupt(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    lock(&state).interrupt(id).map_err(not_found)?;
    Ok(StatusCode::ACCEPTED)
}

async fn restart(State(state): State<Shared>, Path(id): Path<u64>) -> Result<StatusCode, ApiError> {
    lock(&state).restart(id).map_err(|e| {
        let msg = e.to_string();
        if msg.starts_with("no session") {
            (StatusCode::NOT_FOUND, msg)
        } else {
            (StatusCode::CONFLICT, msg) // still running
        }
    })?;
    Ok(StatusCode::ACCEPTED)
}

async fn archive(State(state): State<Shared>, Path(id): Path<u64>) -> Result<StatusCode, ApiError> {
    lock(&state).set_archived(id, true).map_err(not_found)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unarchive(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    lock(&state).set_archived(id, false).map_err(not_found)?;
    Ok(StatusCode::NO_CONTENT)
}

/// Ingest one hook event line from a managed session's `baude hook` child
/// (POSTed via the daemon-injected `$BAUDE_EVENT_URL`). The raw body is the
/// already-built event line; it is appended to the same `/tmp` file the poll
/// loop tails, converging the POST and file-tail transports (HOOK-03).
///
/// Security (T-02-08/09): the `Path<u64>` extractor rejects a non-numeric id
/// at the framework layer; an unknown / unresolvable session is a 404 via
/// `not_found`; a malformed body is appended best-effort. There is no 500
/// path. No auth layer — the route inherits the tailnet/loopback-bound,
/// single-user security model.
async fn post_event(
    State(state): State<Shared>,
    Path(id): Path<u64>,
    body: String,
) -> Result<StatusCode, ApiError> {
    lock(&state).ingest_event(id, &body).map_err(not_found)?;
    Ok(StatusCode::NO_CONTENT)
}

/// `GET /sessions/{id}/permission` (PERM-02) — the in-flight permission request
/// the `permission-mcp` bridge POSTed, or the resolved decision once a human
/// answered (the bridge long-polls this until `decision` appears). Returns JSON
/// `null` when nothing is pending and nothing resolved. Unknown id → 404 via
/// `not_found`; `Path<u64>` rejects a non-numeric id at the framework layer
/// (T-04-09). No auth — inherits the tailnet/loopback single-user model (T-04-08).
#[derive(Deserialize)]
struct PermissionQuery {
    /// Long-poll seconds: when > 0 the bridge blocks (bounded, clamped to 30s)
    /// until the pending request resolves or the window elapses. `0`/absent (the
    /// PWA) returns the current state immediately.
    wait: Option<u64>,
}

async fn get_permission(
    State(state): State<Shared>,
    Path(id): Path<u64>,
    Query(q): Query<PermissionQuery>,
) -> Result<Json<Option<PermissionView>>, ApiError> {
    // Long-poll (Pitfall 4): when the bridge asks to wait and a request is
    // pending with no decision yet, register the per-session Notify and await it
    // OUTSIDE the manager lock so one pending permission never stalls other
    // sessions. A bounded timeout caps the hang; the bridge re-polls. `?wait=0`
    // (the PWA) returns the current state immediately.
    if q.wait.unwrap_or(0) > 0 {
        // WR-04: close the missed-wakeup window. Acquire the per-session Notify,
        // register the `Notified` future (pin it) BEFORE re-reading pending/
        // decision under the lock, then re-check state and await. Because the
        // waiter is registered before the state re-read, a `resolve_pending`
        // that fires `notify_waiters()` in the (now-eliminated) window between
        // the read and the await is delivered to this registered waiter instead
        // of being dropped — so a resolved permission can no longer appear to
        // hang for the full wait window on an unlucky interleaving.
        let notify = {
            let mut m = lock(&state);
            m.permission_notify(id).map_err(not_found)?
        };
        // Register the waiter first. `Notify::notified()` only stores a permit
        // for an already-registered waiter, so this MUST precede the state read.
        let notified = notify.notified();
        tokio::pin!(notified);
        let (pending, decision) = {
            let m = lock(&state);
            let pending = m.pending(id).map_err(not_found)?;
            let decision = m.decision(id).map_err(not_found)?;
            (pending, decision)
        };
        // Only block when there is something pending and nothing resolved yet.
        // If a decision already landed (possibly during/after registration) we
        // skip the await entirely and fall through to the re-read below.
        if pending.is_some() && decision.is_none() {
            let wait = Duration::from_secs(q.wait.unwrap_or(0).min(30));
            let _ = tokio::time::timeout(wait, notified).await;
        }
    }

    let m = lock(&state);
    let pending = m.pending(id).map_err(not_found)?;
    let decision = m.decision(id).map_err(not_found)?;
    drop(m);
    let view = match (pending, decision) {
        // A pending request awaiting a human decision.
        (Some(p), _) => Some(PermissionView {
            request_id: Some(p.request_id),
            tool: Some(p.tool),
            input: Some(p.input),
            ts: Some(p.ts),
            decision: None,
            scope: None,
        }),
        // No pending request, but a resolved decision the bridge can read.
        (None, Some(d)) => Some(PermissionView {
            request_id: Some(d.request_id),
            tool: None,
            input: None,
            ts: Some(d.ts),
            decision: Some(d.decision),
            scope: d.scope,
        }),
        // Nothing in flight and nothing resolved → null.
        (None, None) => None,
    };
    Ok(Json(view))
}

/// `POST /sessions/{id}/permission` body — dual-purpose (PERM-02):
///
/// - The **`permission-mcp` bridge** POSTs a request to register pending state:
///   `{request_id, tool, input, ts?}` (no `decision`). This sets the pending
///   permission the PWA then sees via GET.
/// - The **PWA/phone** POSTs a `{decision: allow|deny, scope?}` to resolve it.
///
/// The presence of `decision` selects the path. A `decision` value other than
/// `allow`/`deny` is a 400, NEVER treated as allow (V5 + deny-default, T-04-05).
/// A request POST missing both `decision` and `tool` is a 400.
#[derive(Deserialize)]
struct PermissionBody {
    // Decision path (PWA → resolve).
    decision: Option<String>,
    scope: Option<String>,
    // Request path (bridge → set pending).
    request_id: Option<String>,
    tool: Option<String>,
    input: Option<serde_json::Value>,
    ts: Option<u64>,
}

async fn post_permission(
    State(state): State<Shared>,
    Path(id): Path<u64>,
    Json(body): Json<PermissionBody>,
) -> Result<StatusCode, ApiError> {
    use crate::manager::PendingPermission;
    use baude_core::meta::now_unix_ms;

    if let Some(decision) = body.decision {
        // ---- Decision path: resolve the pending request --------------------
        if decision != "allow" && decision != "deny" {
            return Err((
                StatusCode::BAD_REQUEST,
                format!("decision must be \"allow\" or \"deny\", got {decision:?}"),
            ));
        }
        lock(&state)
            .resolve_pending(id, &decision, body.scope)
            .map_err(not_found)?;
        return Ok(StatusCode::ACCEPTED);
    }

    // ---- Request path: the bridge registers pending state ------------------
    let Some(tool) = body.tool else {
        return Err((
            StatusCode::BAD_REQUEST,
            "permission POST needs either a decision or a tool request".into(),
        ));
    };
    let pending = PendingPermission {
        request_id: body.request_id.unwrap_or_default(),
        tool,
        input: body.input.unwrap_or(serde_json::Value::Null),
        ts: body.ts.unwrap_or_else(now_unix_ms),
    };
    lock(&state).set_pending(id, pending).map_err(not_found)?;
    Ok(StatusCode::ACCEPTED)
}

/// Messages typed while Claude was busy that it hasn't picked up yet.
async fn get_queue(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<Json<Vec<String>>, ApiError> {
    let path = lock(&state).transcript_path(id).map_err(not_found)?;
    Ok(Json(match path {
        Some(p) => transcript::queued(&p),
        None => vec![],
    }))
}

async fn get_screen(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<Json<crate::manager::Screenshot>, ApiError> {
    lock(&state).screen(id).map(Json).map_err(not_found)
}

#[derive(Deserialize)]
struct KeysBody {
    /// Named keys (up/down/left/right/enter/esc/tab/shift+tab/space/
    /// backspace/ctrl+x) or literal text to type.
    keys: Vec<String>,
}

async fn post_keys(
    State(state): State<Shared>,
    Path(id): Path<u64>,
    Json(body): Json<KeysBody>,
) -> Result<StatusCode, ApiError> {
    if body.keys.is_empty() {
        return Err((StatusCode::BAD_REQUEST, "no keys".into()));
    }
    lock(&state).send_keys(id, &body.keys).map_err(not_found)?;
    Ok(StatusCode::ACCEPTED)
}

/// Raw terminal attach over websocket: first frame out is a redraw snapshot,
/// then live output bytes (binary). Inbound: binary frames are keystrokes,
/// text frames carry control JSON (`{"resize":{"rows":40,"cols":120}}`).
/// The stream ends when the session exits or is deleted.
async fn pty_ws(
    State(state): State<Shared>,
    Path(id): Path<u64>,
    ws: axum::extract::ws::WebSocketUpgrade,
) -> Result<axum::response::Response, ApiError> {
    // Validate before upgrading so a bad id is a clean 404, not a dead socket.
    lock(&state).transcript_path(id).map_err(not_found)?;
    Ok(ws.on_upgrade(move |socket| pty_session(socket, state, id)))
}

#[derive(Deserialize)]
struct PtyControl {
    resize: Option<[u16; 2]>, // [rows, cols]
}

async fn pty_session(mut socket: axum::extract::ws::WebSocket, state: Shared, id: u64) {
    use axum::extract::ws::Message;

    // Bind before matching: the guard must drop before any await.
    let attached = lock(&state).attach(id);
    let (snapshot, rx) = match attached {
        Ok(x) => x,
        Err(_) => {
            let _ = socket.send(Message::Close(None)).await;
            return;
        }
    };
    if socket.send(Message::Binary(snapshot.into())).await.is_err() {
        return;
    }

    // Bridge the sync subscriber channel into async. The thread ends when
    // the PTY drops the sender (exit/kill) or the websocket side hangs up.
    let (tx, mut out) = tokio::sync::mpsc::channel::<Vec<u8>>(64);
    std::thread::spawn(move || {
        while let Ok(chunk) = rx.recv() {
            if tx.blocking_send(chunk).is_err() {
                break;
            }
        }
    });

    loop {
        tokio::select! {
            chunk = out.recv() => match chunk {
                Some(bytes) => {
                    if socket.send(Message::Binary(bytes.into())).await.is_err() {
                        break;
                    }
                }
                None => break, // PTY closed
            },
            msg = socket.recv() => match msg {
                Some(Ok(Message::Binary(bytes))) => {
                    if lock(&state).write_raw(id, &bytes).is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Text(text))) => {
                    if let Ok(ctl) = serde_json::from_str::<PtyControl>(&text) {
                        if let Some([rows, cols]) = ctl.resize {
                            let _ = lock(&state).resize_pty(id, rows, cols);
                        }
                    }
                }
                Some(Ok(Message::Close(_))) | None => break,
                Some(Ok(_)) => {} // ping/pong are handled by axum
                Some(Err(_)) => break,
            },
        }
    }
    let _ = socket.send(Message::Close(None)).await;
}

const STREAM_POLL_MS: u64 = 750;

/// SSE live tail: only messages appended after connect (fetch history via
/// `GET /messages` first). Each event's id is the message uuid. The stream
/// ends when the session is deleted.
async fn stream(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    lock(&state).transcript_path(id).map_err(not_found)?;

    let stream = async_stream::stream! {
        let mut current: Option<PathBuf> = None;
        let mut tail = Tail::default();
        loop {
            let path = match lock(&state).transcript_path(id) {
                Ok(p) => p,
                Err(_) => break, // session deleted
            };
            if let Some(path) = path {
                if current.as_ref() != Some(&path) {
                    // First sighting of an existing transcript: skip its
                    // history. A transcript that changes mid-stream (claude
                    // restarted) is all new — read it from the top.
                    tail = if current.is_none() {
                        Tail::end_of(&path)
                    } else {
                        Tail::default()
                    };
                    current = Some(path.clone());
                }
                for m in tail.read_new(&path) {
                    let data = serde_json::to_string(&m).unwrap_or_default();
                    yield Ok(Event::default().event("message").id(m.uuid.clone()).data(data));
                }
            }
            tokio::time::sleep(Duration::from_millis(STREAM_POLL_MS)).await;
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

/// SSE live tail of the session's hook-event file — a standalone channel,
/// independent of the chat `/stream`. Offset-tails `/tmp/baude-events-<sid>.jsonl`
/// via the dedicated `EventTail` (Pitfall 1: NOT the ChatMessage `Tail`, which
/// would yield zero events). Recent history is served by `GET /activity`; this
/// stream carries only events appended after connect. Unknown id → 404 via the
/// up-front guard. Hook events have no uuid, so no `Event::id` is set (Pitfall 2);
/// ordering is append-only and the PWA does GET-then-buffer.
async fn activity_stream(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<Sse<impl futures_core::Stream<Item = Result<Event, Infallible>>>, ApiError> {
    lock(&state).event_path(id).map_err(not_found)?;

    let stream = async_stream::stream! {
        let mut current: Option<PathBuf> = None;
        let mut tail = EventTail::default();
        loop {
            let path = match lock(&state).event_path(id) {
                Ok(p) => p,
                Err(_) => break, // session deleted
            };
            if let Some(path) = path {
                if current.as_ref() != Some(&path) {
                    // First sighting of an existing event file: skip its
                    // history. A rotated path (resumed/rotated Claude session)
                    // is all new — read it from the top.
                    tail = if current.is_none() {
                        EventTail::end_of(&path)
                    } else {
                        EventTail::default()
                    };
                    current = Some(path.clone());
                }
                for ev in tail.read_new(&path) {
                    let data = serde_json::to_string(&ev).unwrap_or_default();
                    yield Ok(Event::default().event("message").data(data));
                }
            }
            tokio::time::sleep(Duration::from_millis(STREAM_POLL_MS)).await;
        }
    };
    Ok(Sse::new(stream).keep_alive(KeepAlive::default()))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::manager::Manager;

    fn app() -> axum::Router {
        super::router(Arc::new(Mutex::new(Manager::new("sleep 30".into(), false))))
    }

    async fn body_json(res: axum::response::Response) -> serde_json::Value {
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn get(path: &str) -> Request<Body> {
        Request::get(path).body(Body::empty()).unwrap()
    }

    fn post_json(path: &str, json: &str) -> Request<Body> {
        Request::post(path)
            .header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(json.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn list_starts_empty() {
        let res = app().oneshot(get("/sessions")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body_json(res).await, serde_json::json!([]));
    }

    #[tokio::test]
    async fn unknown_session_is_404() {
        let app = app();
        for req in [
            get("/sessions/9"),
            get("/sessions/9/messages"),
            get("/sessions/9/screen"),
            Request::delete("/sessions/9").body(Body::empty()).unwrap(),
            post_json("/sessions/9/interrupt", ""),
            post_json("/sessions/9/keys", r#"{"keys":["enter"]}"#),
        ] {
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{:?}", res);
        }
    }

    #[tokio::test]
    async fn bad_requests_are_400() {
        let app = app();
        let res = app
            .clone()
            .oneshot(post_json("/sessions", r#"{"repo":"/nonexistent-xyz"}"#))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let res = app
            .clone()
            .oneshot(post_json("/sessions/9/messages", r#"{"text":"  "}"#))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        let res = app
            .oneshot(post_json("/sessions/9/keys", r#"{"keys":[]}"#))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn post_event_appends_and_404s_unknown() {
        use crate::manager::lock;

        let state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), false)));
        let id = lock(&state).create("/tmp", None, None).unwrap().id;
        // Pin a deterministic claude session_id so the /tmp path is isolated.
        let sid = format!("api-event-test-{}", std::process::id());
        let path = baude_core::hook::event_path(&sid);
        let _ = std::fs::remove_file(&path);
        {
            let mut m = lock(&state);
            m.session_id_for_test(id, &sid);
        }
        let app = super::router(Arc::clone(&state));

        // Known session: 204 and the line lands in the /tmp event file.
        let res = app
            .clone()
            .oneshot(
                Request::post(format!("/sessions/{id}/event"))
                    .body(Body::from(r#"{"schema":1,"event":"UserPromptSubmit"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let contents = std::fs::read_to_string(&path).unwrap();
        assert!(contents.contains("UserPromptSubmit"), "got: {contents}");

        // Bogus id: 404, never 500.
        let res = app
            .oneshot(
                Request::post("/sessions/9999/event")
                    .body(Body::from("{}"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        let _ = std::fs::remove_file(&path);
        lock(&state).kill_all();
    }

    #[tokio::test]
    async fn activity_returns_events_clamps_limit_and_404s_unknown() {
        use crate::manager::lock;

        let state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), false)));
        let id = lock(&state).create("/tmp", None, None).unwrap().id;
        let sid = format!("api-activity-test-{}", std::process::id());
        let path = baude_core::hook::event_path(&sid);
        let _ = std::fs::remove_file(&path);
        // Seed a few events onto the on-disk file, then drive the ring.
        std::fs::write(
            &path,
            concat!(
                r#"{"event":"UserPromptSubmit","ts":1}"#,
                "\n",
                r#"{"event":"PostToolUse","tool":"Read","ts":2}"#,
                "\n",
                r#"{"event":"Stop","ts":3}"#,
                "\n",
            ),
        )
        .unwrap();
        {
            let mut m = lock(&state);
            m.session_id_for_test(id, &sid);
            m.poll();
        }
        let app = super::router(Arc::clone(&state));

        // Default: full recent set as a JSON array, newest at back.
        let res = app
            .clone()
            .oneshot(get(&format!("/sessions/{id}/activity")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        let arr = body.as_array().unwrap();
        assert_eq!(arr.len(), 3);
        assert_eq!(arr.last().unwrap()["event"], "Stop");
        // PostToolUse carries `tool`; events without it omit the field.
        assert_eq!(arr[1]["tool"], "Read");

        // ?limit=1 returns exactly the newest event.
        let res = app
            .clone()
            .oneshot(get(&format!("/sessions/{id}/activity?limit=1")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let arr = body_json(res).await;
        let arr = arr.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["event"], "Stop");

        // An oversized ?limit is clamped (no 500, no oversized response).
        let res = app
            .clone()
            .oneshot(get(&format!("/sessions/{id}/activity?limit=100000")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body_json(res).await.as_array().unwrap().len(), 3);

        // Unknown id → 404, never 500.
        let res = app
            .clone()
            .oneshot(get("/sessions/9999/activity"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        let _ = std::fs::remove_file(&path);
        lock(&state).kill_all();
    }

    #[tokio::test]
    async fn activity_stream_guards_known_and_unknown() {
        use crate::manager::lock;

        let state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), false)));
        let id = lock(&state).create("/tmp", None, None).unwrap().id;
        let app = super::router(Arc::clone(&state));

        // Known id: the route exists and returns an SSE stream (200 +
        // text/event-stream). A full live-tail assertion is covered by the
        // EventTail unit test; here we assert the route + content-type.
        let res = app
            .clone()
            .oneshot(get(&format!("/sessions/{id}/activity-stream")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers()[header::CONTENT_TYPE].to_str().unwrap();
        assert!(ct.starts_with("text/event-stream"), "got: {ct}");

        // Unknown id → 404 via the up-front event_path guard.
        let res = app
            .oneshot(get("/sessions/9999/activity-stream"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        lock(&state).kill_all();
    }

    #[tokio::test]
    async fn permission_get_post_round_trip_and_validation() {
        use crate::manager::{lock, PendingPermission};

        let state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), false)));
        let id = lock(&state).create("/tmp", None, None).unwrap().id;
        let app = super::router(Arc::clone(&state));

        // No pending yet -> GET returns null (200).
        let res = app
            .clone()
            .oneshot(get(&format!("/sessions/{id}/permission")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body_json(res).await, serde_json::Value::Null);

        // Set a pending request directly on the manager (the bridge POSTs it).
        {
            let mut m = lock(&state);
            m.set_pending(
                id,
                PendingPermission {
                    request_id: "r1".into(),
                    tool: "Bash".into(),
                    input: serde_json::json!({"command": "ls"}),
                    ts: 1,
                },
            )
            .unwrap();
        }

        // GET now returns the pending request as JSON.
        let res = app
            .clone()
            .oneshot(get(&format!("/sessions/{id}/permission")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let body = body_json(res).await;
        assert_eq!(body["request_id"], "r1");
        assert_eq!(body["tool"], "Bash");
        assert!(body["decision"].is_null(), "no decision while pending");

        // Unknown decision -> 400, NEVER treated as allow.
        let res = app
            .clone()
            .oneshot(post_json(
                &format!("/sessions/{id}/permission"),
                r#"{"decision":"maybe"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::BAD_REQUEST);
        // The pending request is untouched by the rejected decision.
        let res = app
            .clone()
            .oneshot(get(&format!("/sessions/{id}/permission")))
            .await
            .unwrap();
        assert_eq!(body_json(res).await["request_id"], "r1");

        // Valid allow -> 202 and resolves.
        let res = app
            .clone()
            .oneshot(post_json(
                &format!("/sessions/{id}/permission"),
                r#"{"decision":"allow","scope":"session"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::ACCEPTED);

        // GET now exposes the decision for the bridge poll; pending is cleared.
        let res = app
            .clone()
            .oneshot(get(&format!("/sessions/{id}/permission")))
            .await
            .unwrap();
        let body = body_json(res).await;
        assert_eq!(body["decision"], "allow");

        // Unknown id -> 404 for both GET and POST, never 500.
        let res = app
            .clone()
            .oneshot(get("/sessions/9999/permission"))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);
        let res = app
            .oneshot(post_json(
                "/sessions/9999/permission",
                r#"{"decision":"deny"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NOT_FOUND);

        lock(&state).kill_all();
    }

    #[tokio::test]
    async fn permission_post_deny_resolves_deny() {
        use crate::manager::{lock, PendingPermission};

        let state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), false)));
        let id = lock(&state).create("/tmp", None, None).unwrap().id;
        {
            let mut m = lock(&state);
            m.set_pending(
                id,
                PendingPermission {
                    request_id: "r9".into(),
                    tool: "Write".into(),
                    input: serde_json::json!({}),
                    ts: 1,
                },
            )
            .unwrap();
        }
        let app = super::router(Arc::clone(&state));
        let res = app
            .clone()
            .oneshot(post_json(
                &format!("/sessions/{id}/permission"),
                r#"{"decision":"deny"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::ACCEPTED);
        let res = app
            .oneshot(get(&format!("/sessions/{id}/permission")))
            .await
            .unwrap();
        assert_eq!(body_json(res).await["decision"], "deny");
        lock(&state).kill_all();
    }

    /// WR-04: a decision that lands while a long-poll GET is in flight must wake
    /// the registered waiter promptly — the response returns with the decision
    /// well before the (generous) `wait` window would otherwise elapse. Because
    /// the `Notified` future is registered before the in-handler state re-read,
    /// the `notify_waiters()` fired by `resolve_pending` is delivered rather than
    /// dropped, so there is no missed-wakeup hang.
    #[tokio::test]
    async fn permission_long_poll_wakes_on_decision() {
        use std::time::{Duration, Instant};

        use crate::manager::{lock, PendingPermission};

        let state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), false)));
        let id = lock(&state).create("/tmp", None, None).unwrap().id;
        {
            let mut m = lock(&state);
            m.set_pending(
                id,
                PendingPermission {
                    request_id: "rw".into(),
                    tool: "Bash".into(),
                    input: serde_json::json!({}),
                    ts: 1,
                },
            )
            .unwrap();
        }
        let app = super::router(Arc::clone(&state));

        // Start the long-poll with a generous wait window (clamped to 30s in the
        // handler). If the wakeup were lost, this GET would block for ~the full
        // window; the assertion below proves it returns near-immediately.
        let poll = tokio::spawn({
            let app = app.clone();
            async move {
                let started = Instant::now();
                let res = app
                    .oneshot(get(&format!("/sessions/{id}/permission?wait=30")))
                    .await
                    .unwrap();
                (res, started.elapsed())
            }
        });

        // Give the handler a beat to register the waiter and reach the await,
        // then resolve the decision (the PWA → resolve path).
        tokio::time::sleep(Duration::from_millis(50)).await;
        app.clone()
            .oneshot(post_json(
                &format!("/sessions/{id}/permission"),
                r#"{"decision":"allow"}"#,
            ))
            .await
            .unwrap();

        let (res, elapsed) = poll.await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body_json(res).await["decision"], "allow");
        // The long-poll must have woken promptly (well under the 30s window),
        // not blocked on a missed wakeup.
        assert!(
            elapsed < Duration::from_secs(5),
            "long-poll did not wake promptly on decision: took {elapsed:?}"
        );

        lock(&state).kill_all();
    }

    #[tokio::test]
    async fn serves_the_pwa() {
        let res = app().oneshot(get("/")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers()[header::CONTENT_TYPE].to_str().unwrap();
        assert!(ct.starts_with("text/html"));
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        assert!(std::str::from_utf8(&bytes).unwrap().contains("baude"));
    }

    #[tokio::test]
    async fn session_lifecycle_over_http() {
        let app = app();
        // create
        let res = app
            .clone()
            .oneshot(post_json("/sessions", r#"{"repo":"/tmp","name":"t"}"#))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CREATED);
        let created = body_json(res).await;
        assert_eq!(created["name"], "t");
        let id = created["id"].as_u64().unwrap();

        // it lists, and the single-session endpoint agrees
        let res = app
            .clone()
            .oneshot(get(&format!("/sessions/{id}")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);

        // message while the stub never registers → 409
        let res = app
            .clone()
            .oneshot(post_json(
                &format!("/sessions/{id}/messages"),
                r#"{"text":"hi"}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::CONFLICT);

        // keys + screen + interrupt work on a live PTY
        let res = app
            .clone()
            .oneshot(post_json(
                &format!("/sessions/{id}/keys"),
                r#"{"keys":["enter"]}"#,
            ))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::ACCEPTED);
        let res = app
            .clone()
            .oneshot(get(&format!("/sessions/{id}/screen")))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let shot = body_json(res).await;
        assert_eq!(shot["rows"], 40);
        let res = app
            .clone()
            .oneshot(post_json(&format!("/sessions/{id}/interrupt"), ""))
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::ACCEPTED);

        // delete
        let res = app
            .clone()
            .oneshot(
                Request::delete(format!("/sessions/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(res.status(), StatusCode::NO_CONTENT);
        let res = app.oneshot(get("/sessions")).await.unwrap();
        assert_eq!(body_json(res).await, serde_json::json!([]));
    }
}

#[cfg(test)]
mod pty_ws_tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    use crate::manager::{lock, Manager};

    #[tokio::test]
    async fn pty_websocket_round_trip() {
        // Wrap the shell so the spawn-site permission flag (appended to the
        // base cmd, default `--dangerously-skip-permissions`) lands as the
        // harmless `$0` of `sh -c` instead of breaking bash's arg parsing.
        // Production uses `claude`, which accepts the flag.
        let state = Arc::new(Mutex::new(Manager::new(
            "sh -c 'exec bash --norc -i'".into(),
            false,
        )));
        let id = lock(&state).create("/tmp", None, None).unwrap().id;
        let app = super::router(Arc::clone(&state));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        tokio::time::sleep(Duration::from_millis(800)).await; // shell prompt up

        // unknown session: clean 404, no upgrade
        let err = tokio_tungstenite::connect_async(format!("ws://{addr}/sessions/99/pty")).await;
        assert!(err.is_err());

        let (mut ws, _) =
            tokio_tungstenite::connect_async(format!("ws://{addr}/sessions/{id}/pty"))
                .await
                .unwrap();

        // First frame is the redraw snapshot.
        let first = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .expect("no snapshot frame")
            .unwrap()
            .unwrap();
        assert!(
            matches!(first, WsMessage::Binary(_)),
            "snapshot must be binary"
        );

        // Type a command through the socket, expect its output back.
        ws.send(WsMessage::Binary(b"echo ws-round-trip\r".to_vec().into()))
            .await
            .unwrap();
        let deadline = tokio::time::Instant::now() + Duration::from_secs(10);
        let mut seen = String::new();
        while !seen.contains("ws-round-trip") {
            assert!(
                tokio::time::Instant::now() < deadline,
                "no echo back: {seen}"
            );
            if let Ok(Some(Ok(WsMessage::Binary(b)))) =
                tokio::time::timeout(Duration::from_millis(500), ws.next()).await
            {
                seen.push_str(&String::from_utf8_lossy(&b));
            }
        }

        // Resize via control frame reaches the PTY.
        ws.send(WsMessage::Text(r#"{"resize":[30,100]}"#.into()))
            .await
            .unwrap();
        tokio::time::sleep(Duration::from_millis(300)).await;
        let shot = lock(&state).screen(id).unwrap();
        assert_eq!((shot.rows, shot.cols), (30, 100));

        drop(ws);
        lock(&state).kill_all();
    }
}
