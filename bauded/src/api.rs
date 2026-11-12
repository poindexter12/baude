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
use baude_core::removal::{RemovalError, RemovalReport, RemovalTarget};

use crate::manager::{
    lock, MutationError, PermissionView, RemoveRowsError, RowsView, SessionInfo, Shared,
};
use crate::transcript::{self, ChatMessage, EventTail, Tail};

pub fn router(state: Shared) -> Router {
    Router::new()
        .merge(crate::web::router())
        .route("/info", get(info))
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
        .route("/removals/preview", post(preview_removal))
        .route("/removals", post(execute_removal))
        .route("/rows", get(list_rows))
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

fn mutation_error(error: MutationError, fallback: StatusCode) -> ApiError {
    match error {
        MutationError::Persistence(error) => (StatusCode::SERVICE_UNAVAILABLE, error.to_string()),
        // The session exists but its runtime would not stop: a server-side
        // failure, never the caller's "not found"/"bad request" fallback.
        MutationError::Teardown(error) => (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()),
        MutationError::Domain(error) => (fallback, error.to_string()),
    }
}

/// Status for a removal that did not happen. Every body is plain text except
/// the 409 drift response, which carries the fresh report as JSON (D-03).
fn removal_error(error: RemoveRowsError) -> axum::response::Response {
    let status = match &error {
        RemoveRowsError::Drift(_) => StatusCode::CONFLICT,
        RemoveRowsError::Removal(RemovalError::UnknownTarget(_)) => StatusCode::NOT_FOUND,
        // 503: refused because of the daemon's own state. A missing runtime
        // handle is raised by core's preflight before any stage runs, so
        // nothing was signalled, deleted or saved, unlike every 500 below.
        RemoveRowsError::Removal(
            RemovalError::Persist { .. } | RemovalError::RuntimeHandleMissing(_),
        )
        | RemoveRowsError::Blocked(_)
        | RemoveRowsError::Busy(_) => StatusCode::SERVICE_UNAVAILABLE,
        RemoveRowsError::Removal(
            RemovalError::RuntimeSurvived { .. }
            | RemovalError::DiskRemoval { .. }
            | RemovalError::RuntimeIdentityMismatch { .. }
            | RemovalError::Validation(_),
        ) => StatusCode::INTERNAL_SERVER_ERROR,
    };
    match error {
        RemoveRowsError::Drift(report) => (status, Json(*report)).into_response(),
        other => (status, other.to_string()).into_response(),
    }
}

/// `POST /removals/preview`: the core dry run for one target, mutating nothing.
async fn preview_removal(
    State(state): State<Shared>,
    Json(target): Json<RemovalTarget>,
) -> Result<Json<RemovalReport>, ApiError> {
    lock(&state)
        .preview_removal(target)
        .map(Json)
        .map_err(|error| match error {
            RemovalError::UnknownTarget(_) => (StatusCode::NOT_FOUND, error.to_string()),
            other => (StatusCode::INTERNAL_SERVER_ERROR, other.to_string()),
        })
}

/// `POST /removals`: the body is the previewed report echoed verbatim. Only
/// its target and its three compared sets are read; every path, runtime and
/// row acted on comes from the daemon's own state.
async fn execute_removal(
    State(state): State<Shared>,
    Json(previewed): Json<RemovalReport>,
) -> axum::response::Response {
    let mut manager = lock(&state);
    match manager.remove_rows(&previewed) {
        Ok(report) => Json(report).into_response(),
        Err(RemoveRowsError::Busy(repository)) => {
            // The key alone names nothing a client can show, so the body
            // carries the repository path the daemon resolved for itself.
            match manager.repository_path(repository) {
                Some(path) => (
                    StatusCode::SERVICE_UNAVAILABLE,
                    format!("repository busy: {}", path.display()),
                )
                    .into_response(),
                None => removal_error(RemoveRowsError::Busy(repository)),
            }
        }
        Err(error) => removal_error(error),
    }
}

/// `GET /info` — daemon identity: which workspace (and thus backend) this
/// daemon serves, so clients can refuse to route sessions into the wrong
/// pool (the TUI's cross-workspace guard). Daemons predating workspaces
/// 404 here; clients treat that as "claude workspace, old version".
async fn info(State(state): State<Shared>) -> Json<serde_json::Value> {
    let ws = baude_core::workspace::active();
    let manager = lock(&state);
    let persistence = manager.persistence_status();
    // workspace_source shows how the workspace was resolved: "(explicit)",
    // "(folder binding)", "(derived)", or "(blank)" for default. Trim
    // parentheses for the API response.
    let workspace_source = ws
        .display_hint()
        .trim_matches('(')
        .trim_matches(')')
        .to_string();
    let collisions = manager.collisions.clone();
    let collision_count = collisions.len();
    let startup_ms = manager.startup_timing.to_hashmap();
    Json(serde_json::json!({
        "workspace": ws.name,
        "backend": ws.backend.name(),
        "workspace_source": workspace_source,
        "version": env!("CARGO_PKG_VERSION"),
        "persistence": persistence,
        "collisions": collisions,
        "collision_count": collision_count,
        "startup_ms": startup_ms,
    }))
}

async fn list_sessions(State(state): State<Shared>) -> Json<Vec<SessionInfo>> {
    Json(lock(&state).list())
}

/// `GET /rows` — the hierarchy projection and the key to name index. A read
/// of owned state cannot fail, and the guard is held across no await.
async fn list_rows(State(state): State<Shared>) -> Json<RowsView> {
    Json(lock(&state).rows())
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
        .map_err(|error| mutation_error(error, StatusCode::BAD_REQUEST))?;
    crate::permission_bridge::watch_if_needed(&state, info.id);
    Ok((StatusCode::CREATED, Json(info)))
}

async fn delete_session(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    lock(&state)
        .remove(id)
        .map_err(|error| mutation_error(error, StatusCode::NOT_FOUND))?;
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
    lock(&state).restart(id).map_err(|error| {
        match error {
            MutationError::Persistence(error) => {
                (StatusCode::SERVICE_UNAVAILABLE, error.to_string())
            }
            MutationError::Teardown(error) => {
                (StatusCode::INTERNAL_SERVER_ERROR, error.to_string())
            }
            MutationError::Domain(error) => {
                let message = error.to_string();
                if message.starts_with("no session") {
                    (StatusCode::NOT_FOUND, message)
                } else {
                    (StatusCode::CONFLICT, message) // still running
                }
            }
        }
    })?;
    // A restart re-rolls the opencode server port — wire a fresh watcher.
    crate::permission_bridge::watch_if_needed(&state, id);
    Ok(StatusCode::ACCEPTED)
}

async fn archive(State(state): State<Shared>, Path(id): Path<u64>) -> Result<StatusCode, ApiError> {
    lock(&state)
        .set_archived(id, true)
        .map_err(|error| mutation_error(error, StatusCode::NOT_FOUND))?;
    Ok(StatusCode::NO_CONTENT)
}

async fn unarchive(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    lock(&state)
        .set_archived(id, false)
        .map_err(|error| mutation_error(error, StatusCode::NOT_FOUND))?;
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
        }),
        // No pending request, but a resolved decision the bridge can read.
        (None, Some(d)) => Some(PermissionView {
            request_id: Some(d.request_id),
            tool: None,
            input: None,
            ts: Some(d.ts),
            decision: Some(d.decision),
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
///
/// WR-03: `scope` is accepted for forward-compatibility but is NOT enforced in
/// v0.7. Each `tools/call` mints a fresh `request_id`/pending request, so an
/// `{decision:"allow", scope:"session"}` only ever resolves the single in-flight
/// call — there is no session-scoped allow. Rich scope enforcement is deferred;
/// the field is parsed and discarded so a future client may send it, but nothing
/// reads it. It is deliberately NOT stored or echoed back (that would imply a
/// contract that does not exist).
#[derive(Deserialize)]
struct PermissionBody {
    // Decision path (PWA → resolve).
    decision: Option<String>,
    /// Accepted-but-ignored in v0.7 (WR-03): see the struct doc above. Parsed so
    /// a forward-compat client may send it; enforcement is deferred.
    #[allow(dead_code)]
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
        // WR-03: `body.scope` is intentionally dropped — scope is accepted for
        // forward-compat but not enforced/stored in v0.7 (see PermissionBody).
        lock(&state)
            .resolve_pending(id, &decision)
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
    use std::path::Path;
    use std::process::Command;
    use std::sync::{Arc, Mutex};
    use std::time::{Duration, Instant};

    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use axum::Router;
    use baude_core::removal::{RemovalError, RemovalTarget, RowKey};
    use baude_core::repository::CheckoutKey;
    use baude_core::workspace::Workspace;
    use std::path::PathBuf;

    use crate::manager::{Manager, Shared};

    /// The minimal API isolation owner: a unique synthetic root, the redirect
    /// that pins every resolved path inside it, and the literal workspace
    /// identity resolved under that root. For router cases that need no
    /// repository fixture — they still reach `workspace::active()` through the
    /// handlers.
    ///
    /// `pub(super)` so the sibling `pty_ws_tests` module can own one too: the
    /// websocket case drives the same handlers over a real socket and reaches
    /// the same ambient config reads.
    pub(super) struct ApiScope {
        /// Struct fields drop in declaration order, so the identity is restored
        /// while its own root is still installed.
        _identity: baude_core::testing::TestRedirect,
        _redirect: baude_core::testing::TestRedirect,
    }

    #[must_use = "the returned ApiScope owns this case's root and identity; bind it to a named \
                  local that outlives every handler await"]
    pub(super) fn api_scope(label: &str) -> ApiScope {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bauded-scope-{label}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let redirect = baude_core::testing::TestRedirect::new(&root);
        let identity = baude_core::workspace::override_for_test(
            &baude_core::persist::Config {
                workspace: Some(label.to_string()),
                ..baude_core::persist::Config::default()
            },
            None,
        );
        ApiScope {
            _identity: identity,
            _redirect: redirect,
        }
    }

    /// The router together with the scope that isolates it.
    ///
    /// Every handler resolves the active workspace and names its state file
    /// from it, so the scope must be retained for the whole test body —
    /// including across every `await`, which is where session creation reaches
    /// managed worktree allocation. Returning the bare router would compile and
    /// run the case against the developer's real workspace and data dir.
    #[must_use = "the leading ApiScope isolates this router; bind it to a named local that \
                  outlives every handler await"]
    fn app() -> (ApiScope, axum::Router) {
        let scope = api_scope("router");
        let router = super::router(Arc::new(Mutex::new(Manager::new("sleep 30".into(), false))));
        (scope, router)
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

    /// Run git in `repo`, asserting success. Output is captured and shown
    /// only on failure: an inherited stdout lands in the middle of libtest's
    /// own `test name ... ok` line and splits it across lines.
    fn git(repo: &Path, args: &[&str]) {
        let out = Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git {args:?} exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A real repository with one commit under `root`, with no redirect of its
    /// own: for a test that already holds a scope and needs a path git reports
    /// as a repository, so the admitted row is a checkout.
    fn committed_repo(root: &Path) -> PathBuf {
        let repo = root.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        repo
    }

    /// A fixture repository together with the redirect that contains it.
    ///
    /// The redirect is thread-local and drops with this value, so the owner must
    /// be held in a NAMED binding for the whole test body — including across
    /// every handler `await`, since that is where session creation reaches
    /// managed worktree allocation. Returning a bare `TestRedirect` from the
    /// helper, or letting the owner live only as a temporary, would compile and
    /// leave the fixture running completely unredirected (issue #72).
    struct FixtureRepo {
        repo: std::path::PathBuf,
        /// Struct fields drop in DECLARATION order, the reverse of locals, so
        /// the identity scope is declared before the root redirect and is
        /// therefore restored while its own root is still installed — mirroring
        /// the acquisition order in `initialized_repo_in_workspace` (root
        /// first, identity second).
        _identity: baude_core::testing::TestRedirect,
        _redirect: baude_core::testing::TestRedirect,
    }

    impl FixtureRepo {
        fn path(&self) -> &Path {
            &self.repo
        }
    }

    /// The default fixture identity is the literal `claude` workspace, which is
    /// exactly the identity these tests were written against and the one the
    /// `persist_at_for_test` call sites pair their state with. It is a literal,
    /// never the developer's configured workspace.
    #[must_use = "the returned FixtureRepo owns the redirect that contains this fixture; bind it \
                  to a named local that outlives every handler await"]
    fn initialized_repo(root: &Path, name: &str) -> FixtureRepo {
        initialized_repo_in_workspace(root, name, "claude")
    }

    /// The same fixture under an explicitly chosen literal workspace, for cases
    /// that need two fixtures to be distinguishable by identity alone.
    #[must_use = "the returned FixtureRepo owns the redirect that contains this fixture; bind it \
                  to a named local that outlives every handler await"]
    fn initialized_repo_in_workspace(root: &Path, name: &str, workspace: &str) -> FixtureRepo {
        // Contain managed worktree allocation (issue #72): every API test that
        // restarts or activates a session can reach worktree creation.
        let redirect = baude_core::testing::TestRedirect::new(root);
        // Identity second, under the root just installed. Held by the returned
        // owner so it survives every handler `await`, which is where session
        // creation reaches managed worktree allocation.
        let identity = baude_core::workspace::override_for_test(
            &baude_core::persist::Config {
                workspace: Some(workspace.to_string()),
                ..baude_core::persist::Config::default()
            },
            None,
        );
        let repo = root.join(name);
        std::fs::create_dir_all(&repo).unwrap();
        git(&repo, &["init", "-b", "main"]);
        git(&repo, &["config", "user.email", "test@example.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        std::fs::write(repo.join("file"), b"one").unwrap();
        git(&repo, &["add", "file"]);
        git(&repo, &["commit", "-m", "initial"]);
        FixtureRepo {
            repo,
            _identity: identity,
            _redirect: redirect,
        }
    }

    /// Owner-only workspace-identity isolation.
    ///
    /// These cases resolve identity and compose managed paths only: no router
    /// is driven, no Manager is created, no PTY is spawned. Session workers are
    /// not isolated until plan 08.
    mod fixture_identity_isolation {
        use super::*;
        use std::sync::Barrier;

        /// Two API fixtures built on two threads under two explicit literal
        /// workspaces keep independent identities and managed paths after their
        /// helpers returned.
        #[test]
        fn concurrent_api_fixtures_keep_independent_identities() {
            let barrier = Arc::new(Barrier::new(2));
            let mut threads = Vec::new();
            for workspace in ["api-identity-alpha", "api-identity-beta"] {
                let barrier = Arc::clone(&barrier);
                threads.push(std::thread::spawn(move || {
                    let root = std::env::temp_dir().join(format!(
                        "bauded-{workspace}-{}-{:?}",
                        std::process::id(),
                        std::thread::current().id()
                    ));
                    let _ = std::fs::remove_dir_all(&root);
                    std::fs::create_dir_all(&root).unwrap();
                    let fixture = initialized_repo_in_workspace(&root, "repo", workspace);
                    barrier.wait();
                    let resolved = baude_core::workspace::active().name.clone();
                    let managed = baude_core::git::managed_default_worktree_path("7", 11);
                    barrier.wait();
                    assert_eq!(resolved, workspace);
                    assert!(
                        managed.starts_with(&root),
                        "{} escaped the fixture root {}",
                        managed.display(),
                        root.display()
                    );
                    assert!(
                        managed
                            .components()
                            .any(|c| c.as_os_str() == std::ffi::OsStr::new(workspace)),
                        "{} does not carry the fixture's own workspace {workspace}",
                        managed.display()
                    );
                    assert!(fixture.path().exists());
                    drop(fixture);
                    let _ = std::fs::remove_dir_all(&root);
                }));
            }
            for thread in threads {
                thread.join().expect("identity thread panicked");
            }
        }

        /// An override-free thread cannot inherit a live API fixture identity.
        #[test]
        fn an_override_free_probe_cannot_inherit_a_fixture_identity() {
            let root =
                std::env::temp_dir().join(format!("bauded-identity-escape-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&root);
            std::fs::create_dir_all(&root).unwrap();
            let _fixture = initialized_repo(&root, "repo");
            let probe = std::thread::spawn(|| baude_core::workspace::active().name.clone()).join();
            let payload = probe.expect_err("an override-free reader must not resolve an identity");
            let message = payload
                .downcast_ref::<String>()
                .map(String::as_str)
                .or_else(|| payload.downcast_ref::<&str>().copied())
                .unwrap_or_default()
                .to_string();
            assert!(
                message.contains(baude_core::workspace::IDENTITY_ESCAPE_PANIC_MARKER),
                "expected the identity-escape panic, got {message:?}"
            );
            let _ = std::fs::remove_dir_all(&root);
        }
    }

    /// Requires a caller-owned isolation scope established BEFORE the call:
    /// the manager it returns polls session metadata through
    /// `backend::active()` and names its state file from `workspace::active()`.
    /// Every caller already binds an `initialized_repo` owner first; this
    /// assertion is what keeps the next one from forgetting.
    fn exited_tracked_manager(repo: &Path) -> (crate::manager::Shared, u64) {
        assert!(
            baude_core::testing::workspace_override().is_some(),
            "exited_tracked_manager requires a caller-owned isolation scope: bind an \
             initialized_repo (or api_scope) owner before calling it"
        );
        let state = Arc::new(Mutex::new(Manager::new("true".into(), false)));
        let id = crate::manager::lock(&state)
            .create(repo.to_str().unwrap(), None, None)
            .unwrap()
            .id;
        crate::manager::lock(&state).track_runtime_for_test(id);
        let deadline = Instant::now() + Duration::from_secs(10);
        while crate::manager::lock(&state).info(id).unwrap().status != "exited" {
            assert!(Instant::now() < deadline, "stub never exited");
            std::thread::sleep(Duration::from_millis(50));
        }
        (state, id)
    }

    #[tokio::test]
    async fn list_starts_empty() {
        let (_scope, router) = app();
        let res = router.oneshot(get("/sessions")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        assert_eq!(body_json(res).await, serde_json::json!([]));
    }

    #[tokio::test]
    async fn flat_session_api_remains_a_non_hierarchical_compatibility_projection() {
        let root = std::env::temp_dir().join(format!(
            "bauded-flat-api-compatibility-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let fixture = initialized_repo(&root, "repo");
        let repo = fixture.path().canonicalize().unwrap();
        let workspace = baude_core::workspace::resolve(
            Some("claude"),
            None,
            &baude_core::persist::Config::default(),
            |_| {},
        );
        let state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), true)));
        crate::manager::lock(&state).persist_at_for_test(&root, &workspace, None);
        let app = super::router(Arc::clone(&state));

        let create = app
            .clone()
            .oneshot(post_json(
                "/sessions",
                &serde_json::json!({ "repo": repo, "name": "flat-main" }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(create.status(), StatusCode::CREATED);
        let created = body_json(create).await;
        let id = created["id"].as_u64().unwrap();

        let listed = app.clone().oneshot(get("/sessions")).await.unwrap();
        assert_eq!(listed.status(), StatusCode::OK);
        let listed = body_json(listed).await;
        let rows = listed.as_array().expect("flat SessionInfo array");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["id"], id);
        for forbidden in [
            "repository",
            "repository_key",
            "checkout",
            "checkout_key",
            "parent",
            "children",
            "hierarchy",
            "worktrees",
        ] {
            assert!(rows[0].get(forbidden).is_none(), "unexpected {forbidden}");
        }

        for request in [
            get("/repositories"),
            get(&format!("/sessions/{id}/children")),
            Request::delete(format!("/sessions/{id}/remove-worktree"))
                .body(Body::empty())
                .unwrap(),
            Request::delete(format!("/worktrees/{id}"))
                .body(Body::empty())
                .unwrap(),
        ] {
            let response = app.clone().oneshot(request).await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert!(crate::manager::lock(&state).info(id).is_some());
        }

        let deleted = app
            .clone()
            .oneshot(
                Request::delete(format!("/sessions/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);
        assert_eq!(
            body_json(app.clone().oneshot(get("/sessions")).await.unwrap()).await,
            serde_json::json!([])
        );

        let retained =
            baude_core::persist::load_current_at(&root, &workspace.state_file("daemon-state"))
                .unwrap()
                .state;
        assert_eq!(retained.repositories.len(), 1);
        assert_eq!(retained.checkouts.len(), 1);
        assert!(!retained.checkouts[0].active_intent());
        assert_eq!(retained.checkouts[0].observed_path.to_path_buf(), repo);
        assert!(repo.join("file").is_file());
        let inventory = baude_core::git::discover_repository(&repo).unwrap();
        assert_eq!(inventory.worktrees.len(), 1);
        assert_eq!(inventory.worktrees[0].path, repo);
        assert!(Command::new("git")
            .args(["show-ref", "--verify", "--quiet", "--", "refs/heads/main"])
            .current_dir(&repo)
            .status()
            .unwrap()
            .success());

        crate::manager::lock(&state).kill_all();
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn unknown_session_is_404() {
        let (_scope, app) = app();
        for req in [
            get("/sessions/9"),
            get("/sessions/9/messages"),
            get("/sessions/9/screen"),
            Request::delete("/sessions/9").body(Body::empty()).unwrap(),
            post_json("/sessions/9/interrupt", ""),
            post_json("/sessions/9/restart", ""),
            post_json("/sessions/9/archive", ""),
            post_json("/sessions/9/unarchive", ""),
            post_json("/sessions/9/keys", r#"{"keys":["enter"]}"#),
        ] {
            let res = app.clone().oneshot(req).await.unwrap();
            assert_eq!(res.status(), StatusCode::NOT_FOUND, "{:?}", res);
        }
    }

    #[tokio::test]
    async fn bad_requests_are_400() {
        let (_scope, app) = app();
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

    /// Assert a mutation's status and carry the response body into the
    /// failure message. A bare status comparison hides which error the handler
    /// mapped: `delete_session` turns every `MutationError::Domain` into 404,
    /// so "404" alone cannot distinguish a missing session from a failed
    /// runtime teardown.
    async fn assert_status(
        response: axum::response::Response,
        expected: StatusCode,
        leg: &str,
        failure: &baude_core::persist::AtomicFailure,
    ) {
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            status,
            expected,
            "{leg} under {failure:?}: body={}",
            String::from_utf8_lossy(&body)
        );
    }

    #[tokio::test]
    async fn real_atomic_persistence_failures_are_503_for_every_mutation() {
        use baude_core::persist::{self, AtomicFailure};

        // Declared first so it drops last: every `Manager::new` below reads
        // config ambiently even though the state roots are passed explicitly.
        let _scope = api_scope("atomic-persistence");

        for (failure, committed) in [
            (AtomicFailure::Rename, false),
            (AtomicFailure::DirectorySync, true),
        ] {
            let suffix = format!("{:?}-{}", failure, std::process::id());
            let workspace = baude_core::workspace::resolve(
                Some("claude"),
                None,
                &persist::Config::default(),
                |_| {},
            );

            let create_root = std::env::temp_dir().join(format!("bauded-api-create-{suffix}"));
            let _ = std::fs::remove_dir_all(&create_root);
            std::fs::create_dir_all(&create_root).unwrap();
            let create_state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), true)));
            crate::manager::lock(&create_state).persist_at_for_test(
                &create_root,
                &workspace,
                Some(failure),
            );
            let create_repo = committed_repo(&create_root);
            let response = super::router(Arc::clone(&create_state))
                .oneshot(post_json(
                    "/sessions",
                    &serde_json::json!({ "repo": create_repo }).to_string(),
                ))
                .await
                .unwrap();
            assert_status(
                response,
                StatusCode::SERVICE_UNAVAILABLE,
                "POST /sessions",
                &failure,
            )
            .await;
            assert!(crate::manager::lock(&create_state).list().is_empty());
            let create_file = create_root.join(workspace.state_file("daemon-state"));
            assert_eq!(create_file.exists(), committed);
            if committed {
                assert_eq!(
                    persist::load_current_at(&create_root, &workspace.state_file("daemon-state"))
                        .unwrap()
                        .state
                        .checkouts
                        .len(),
                    1
                );
            }

            let delete_root = std::env::temp_dir().join(format!("bauded-api-delete-{suffix}"));
            let _ = std::fs::remove_dir_all(&delete_root);
            std::fs::create_dir_all(&delete_root).unwrap();
            let delete_state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), true)));
            let delete_id = {
                let mut manager = crate::manager::lock(&delete_state);
                manager.persist_at_for_test(&delete_root, &workspace, None);
                let repo = committed_repo(&delete_root);
                let id = manager
                    .create(repo.to_str().unwrap(), None, None)
                    .unwrap()
                    .id;
                manager.persist_at_for_test(&delete_root, &workspace, Some(failure));
                id
            };
            let response = super::router(Arc::clone(&delete_state))
                .oneshot(
                    Request::delete(format!("/sessions/{delete_id}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_status(
                response,
                StatusCode::SERVICE_UNAVAILABLE,
                "DELETE /sessions/{id}",
                &failure,
            )
            .await;
            assert_eq!(
                crate::manager::lock(&delete_state)
                    .info(delete_id)
                    .is_none(),
                committed
            );
            let retained =
                persist::load_current_at(&delete_root, &workspace.state_file("daemon-state"))
                    .unwrap()
                    .state;
            assert_eq!(retained.repositories.len(), 1);
            assert_eq!(retained.checkouts.len(), 1);
            assert_eq!(retained.checkouts[0].active_intent(), !committed);

            let archive_root = std::env::temp_dir().join(format!("bauded-api-archive-{suffix}"));
            let _ = std::fs::remove_dir_all(&archive_root);
            std::fs::create_dir_all(&archive_root).unwrap();
            let archive_state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), true)));
            let archive_id = {
                let mut manager = crate::manager::lock(&archive_state);
                manager.persist_at_for_test(&archive_root, &workspace, None);
                let repo = committed_repo(&archive_root);
                let id = manager
                    .create(repo.to_str().unwrap(), None, None)
                    .unwrap()
                    .id;
                manager.persist_at_for_test(&archive_root, &workspace, Some(failure));
                id
            };
            let response = super::router(Arc::clone(&archive_state))
                .oneshot(post_json(&format!("/sessions/{archive_id}/archive"), ""))
                .await
                .unwrap();
            assert_status(
                response,
                StatusCode::SERVICE_UNAVAILABLE,
                "POST /sessions/{id}/archive",
                &failure,
            )
            .await;
            assert_eq!(
                crate::manager::lock(&archive_state)
                    .info(archive_id)
                    .unwrap()
                    .archived,
                committed
            );
            assert_eq!(
                persist::load_current_at(&archive_root, &workspace.state_file("daemon-state"))
                    .unwrap()
                    .state
                    .checkouts[0]
                    .session
                    .archived,
                committed
            );

            let restart_root = std::env::temp_dir().join(format!("bauded-api-restart-{suffix}"));
            let _ = std::fs::remove_dir_all(&restart_root);
            std::fs::create_dir_all(&restart_root).unwrap();
            let restart_fixture = initialized_repo(&restart_root, "repo");
            let repo = restart_fixture.path().to_path_buf();
            let restart_state = Arc::new(Mutex::new(Manager::new("true".into(), true)));
            let restart_id = {
                let mut manager = crate::manager::lock(&restart_state);
                manager.persist_at_for_test(&restart_root, &workspace, None);
                manager
                    .create(repo.to_str().unwrap(), None, None)
                    .unwrap()
                    .id
            };
            let deadline = Instant::now() + Duration::from_secs(10);
            while crate::manager::lock(&restart_state)
                .info(restart_id)
                .unwrap()
                .status
                != "exited"
            {
                assert!(Instant::now() < deadline, "stub never exited");
                std::thread::sleep(Duration::from_millis(50));
            }
            crate::manager::lock(&restart_state).persist_at_for_test(
                &restart_root,
                &workspace,
                Some(failure),
            );
            let response = super::router(Arc::clone(&restart_state))
                .oneshot(post_json(&format!("/sessions/{restart_id}/restart"), ""))
                .await
                .unwrap();
            assert_status(
                response,
                StatusCode::SERVICE_UNAVAILABLE,
                "POST /sessions/{id}/restart",
                &failure,
            )
            .await;
            assert_eq!(
                crate::manager::lock(&restart_state)
                    .info(restart_id)
                    .unwrap()
                    .status,
                "exited"
            );

            for state in [&create_state, &delete_state, &archive_state, &restart_state] {
                crate::manager::lock(state).kill_all();
            }
            for root in [create_root, delete_root, archive_root, restart_root] {
                std::fs::remove_dir_all(root).unwrap();
            }
        }
    }

    /// A runtime that will not stop is a server failure, not "no such
    /// session". Before SQ-6 `remove` classified teardown errors as
    /// `MutationError::Domain`, which `delete_session` reports as 404 — the
    /// exact status of the macOS CI flake in the atomic-persistence test.
    #[tokio::test]
    async fn delete_teardown_failure_is_500_and_unknown_id_stays_404() {
        use baude_core::persist;

        let _scope = api_scope("delete-teardown-failure");
        let workspace = baude_core::workspace::resolve(
            Some("claude"),
            None,
            &persist::Config::default(),
            |_| {},
        );
        let root =
            std::env::temp_dir().join(format!("bauded-api-delete-teardown-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), true)));
        let id = {
            let mut manager = crate::manager::lock(&state);
            manager.persist_at_for_test(&root, &workspace, None);
            let repo = committed_repo(&root);
            let id = manager
                .create(repo.to_str().unwrap(), None, None)
                .unwrap()
                .id;
            manager.fail_next_agent_teardown_for_test(id, "agent stop refused once");
            id
        };
        let delete = |id: u64| {
            super::router(Arc::clone(&state)).oneshot(
                Request::delete(format!("/sessions/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
        };

        let response = delete(id).await.unwrap();
        let status = response.status();
        let body = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&body);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR, "body={body}");
        assert!(body.contains("agent stop refused once"), "body={body}");
        assert!(
            crate::manager::lock(&state).info(id).is_some(),
            "a failed teardown must keep the session"
        );

        // The failure is retryable: the next DELETE stops it for real.
        assert_eq!(delete(id).await.unwrap().status(), StatusCode::NO_CONTENT);
        // A genuinely unknown id is still 404.
        assert_eq!(delete(id).await.unwrap().status(), StatusCode::NOT_FOUND);

        crate::manager::lock(&state).kill_all();
        std::fs::remove_dir_all(root).unwrap();
    }

    /// End a fixture runtime the way a crashed agent does, by signalling its
    /// process from outside, and wait until the daemon reports the session
    /// as exited. Restart refuses a session whose agent is still running, so
    /// a restart test over a live fixture would get its 409 for that reason
    /// and prove nothing about the branch.
    async fn exit_fixture_runtime(fixture: &TwoCheckoutFixture, id: u64, key: CheckoutKey) {
        let report = preview_report(&fixture.app, key).await;
        let pid = previewed_agent_pid(&report, key);
        assert!(Command::new("kill")
            .arg(pid.to_string())
            .status()
            .unwrap()
            .success());
        let deadline = Instant::now() + Duration::from_secs(10);
        while crate::manager::lock(&fixture.shared)
            .info(id)
            .unwrap()
            .status
            != "exited"
        {
            assert!(
                Instant::now() < deadline,
                "runtime {pid} never read as exited"
            );
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    /// A managed worktree row is pinned to its branch, so a restart after a
    /// real branch switch in that worktree is refused and nothing moves.
    /// Until 21-09 this ran on a main checkout, which the daemon mis-roled as
    /// `managed_branch`; the subject is now a row that is pinned by design.
    #[tokio::test]
    async fn restart_api_refuses_checkout_branch_change() {
        let fixture = removal_fixture_two_checkouts("restart-pinned").await;
        let app = &fixture.app;
        let stored = crate::manager::lock(&fixture.shared)
            .stored_observed_branch_for_test(fixture.worktree_key);
        exit_fixture_runtime(&fixture, fixture.worktree_id, fixture.worktree_key).await;
        git(&fixture.worktree_path, &["checkout", "-b", "changed"]);
        let before = rows_body(app).await["repositories"][0]["checkouts"].clone();

        let (status, body) = removal_call(
            app,
            &format!("/sessions/{}/restart", fixture.worktree_id),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::CONFLICT, "body={body}");
        assert!(
            !body.contains("still running"),
            "the refusal must be about the branch, not a live agent: {body}"
        );
        assert!(body.contains("BranchChanged"), "body={body}");

        // The refusal records the repository as unavailable, which is the
        // one thing allowed to move; both checkout rows read as they did.
        assert_eq!(rows_body(app).await["repositories"][0]["checkouts"], before);
        assert_eq!(
            crate::manager::lock(&fixture.shared)
                .stored_observed_branch_for_test(fixture.worktree_key),
            stored
        );
        assert_eq!(
            crate::manager::lock(&fixture.shared)
                .info(fixture.worktree_id)
                .unwrap()
                .status,
            "exited"
        );
        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// A main checkout follows its branch: after a real `git switch` in the
    /// main checkout, restart is accepted and the stored observed branch is
    /// the new branch's full ref, read back from the fixture.
    #[tokio::test]
    async fn restart_api_follows_a_branch_switch_on_a_main_checkout() {
        let fixture = removal_fixture_two_checkouts("restart-follows").await;
        let main_path = fixture.repo.path().canonicalize().unwrap();
        exit_fixture_runtime(&fixture, fixture.main_id, fixture.main_key).await;
        git(&main_path, &["switch", "-c", "changed"]);
        let expected = git_read(&main_path, &["symbolic-ref", "-q", "HEAD"])
            .expect("the main checkout sits on a branch");
        assert_ne!(
            crate::manager::lock(&fixture.shared).stored_observed_branch_for_test(fixture.main_key),
            Some(expected.clone()),
            "the switch must be something the daemon has not yet observed"
        );

        let (status, body) = removal_call(
            &fixture.app,
            &format!("/sessions/{}/restart", fixture.main_id),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED, "body={body}");
        assert_eq!(
            crate::manager::lock(&fixture.shared).stored_observed_branch_for_test(fixture.main_key),
            Some(expected)
        );
        removal_teardown(&fixture.shared, &fixture.root);
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn restart_api_refuses_replaced_repository_path() {
        use std::os::unix::fs::symlink;

        let root = std::env::temp_dir().join(format!(
            "bauded-api-restart-replaced-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&root);
        let fixture = initialized_repo(&root, "repo");
        let repo = fixture.path().to_path_buf();
        let replacement_fixture = initialized_repo(&root, "replacement");
        let replacement = replacement_fixture.path().to_path_buf();
        let (state, id) = exited_tracked_manager(&repo);
        std::fs::rename(&repo, root.join("original")).unwrap();
        symlink(&replacement, &repo).unwrap();

        let response = super::router(Arc::clone(&state))
            .oneshot(post_json(&format!("/sessions/{id}/restart"), ""))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        assert_eq!(
            crate::manager::lock(&state).info(id).unwrap().status,
            "exited"
        );
        std::fs::remove_dir_all(root).unwrap();
    }

    #[tokio::test]
    async fn post_event_appends_and_404s_unknown() {
        use crate::manager::lock;

        let _scope = api_scope("post-event");
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

        let _scope = api_scope("activity-events");
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
            m.poll_claude_meta_for_test(id);
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

        let _scope = api_scope("activity-stream");
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

        let _scope = api_scope("permission-round-trip");
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

        let _scope = api_scope("permission-deny");
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

        let _scope = api_scope("permission-long-poll");
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
        let (_scope, router) = app();
        let res = router.oneshot(get("/")).await.unwrap();
        assert_eq!(res.status(), StatusCode::OK);
        let ct = res.headers()[header::CONTENT_TYPE].to_str().unwrap();
        assert!(ct.starts_with("text/html"));
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        assert!(std::str::from_utf8(&bytes).unwrap().contains("baude"));
    }

    #[tokio::test]
    async fn session_lifecycle_over_http() {
        let (_scope, app) = app();
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

    #[tokio::test]
    async fn info_endpoint_includes_workspace_source() {
        let _scope = api_scope("info-test");
        let state = Arc::new(Mutex::new(Manager::new("claude".to_string(), true)));
        let app = super::router(Arc::clone(&state));

        // Test that /info includes workspace_source field
        let req = Request::builder().uri("/info").body(Body::empty()).unwrap();

        let response = app.clone().oneshot(req).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Verify /info response includes required fields
        assert!(
            json["workspace"].is_string(),
            "workspace field must be a string"
        );
        assert!(
            json["backend"].is_string(),
            "backend field must be a string"
        );
        assert!(
            json["workspace_source"].is_string(),
            "workspace_source field must be a string"
        );
        assert!(
            json["version"].is_string(),
            "version field must be a string"
        );
        assert!(
            json["persistence"].is_object(),
            "persistence field must be an object"
        );

        // workspace_source should be one of the four valid source labels
        let source = json["workspace_source"].as_str().unwrap();
        assert!(
            ["explicit", "folder binding", "derived", "blank"].contains(&source),
            "workspace_source must be one of: explicit, folder binding, derived, blank (got: {})",
            source
        );
    }

    #[test]
    fn startup_timing_recorded_on_manager() {
        let _scope = api_scope("startup-timing");
        let manager = crate::manager::Manager::new("test".into(), false);
        // Manager should have a startup_timing field (even if empty for this test)
        assert_eq!(manager.startup_timing.total_ms, 0);
    }

    #[tokio::test]
    async fn info_reports_daemon_startup_ms() {
        let (_scope, app) = app();
        let request = Request::builder().uri("/info").body(Body::empty()).unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = response.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();

        // Check that startup_ms is present and is an object
        assert!(
            json["startup_ms"].is_object(),
            "startup_ms field must be an object"
        );

        // Check that it contains a total field
        assert!(
            json["startup_ms"]["total"].is_number(),
            "startup_ms.total field must be a number"
        );
    }

    /// The removal fixtures' agent command. The daemon appends a permission
    /// flag to every spawn, and a bare `sleep 30` rejects it and exits, so the
    /// runtime would be dead before any removal ran. The shell wrapper takes
    /// the appended argument as `$0` and keeps sleeping, which is the double
    /// the Docker smoke in `ci.yml` uses for the same reason.
    const REMOVAL_AGENT_CMD: &str = "sh -c \"sleep 30\"";

    /// Whether `pid` is a live process, asked of `ps` rather than inferred
    /// from a non-zero persisted pid.
    fn pid_is_alive(pid: u64) -> bool {
        Command::new("ps")
            .args(["-o", "pid=", "-p", &pid.to_string()])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    }

    /// Poll until `pid` is gone, bounded at five seconds.
    fn wait_for_pid_gone(pid: u64) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if !pid_is_alive(pid) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// The setup every removal fixture shares: an isolated root, one
    /// initialized fixture repository, and a persisting daemon whose state
    /// file lives under that root. No session is admitted yet.
    fn removal_setup(label: &str) -> (PathBuf, FixtureRepo, Workspace, Shared, Router) {
        let root = removal_root(label);
        let fixture = initialized_repo(&root, "repo");
        let (workspace, shared, app) = removal_daemon(&root);
        (root, fixture, workspace, shared, app)
    }

    /// A fresh, empty, isolated root for one removal fixture.
    fn removal_root(label: &str) -> PathBuf {
        let root =
            std::env::temp_dir().join(format!("bauded-removal-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        root
    }

    /// A persisting daemon whose state file lives under `root`, over
    /// whatever repositories the caller already built there. Requires the
    /// caller to hold a `FixtureRepo` for `root` first: the workspace is
    /// resolved under the identity scope that owner installed.
    fn removal_daemon(root: &Path) -> (Workspace, Shared, Router) {
        let workspace = baude_core::workspace::resolve(
            Some("claude"),
            None,
            &baude_core::persist::Config::default(),
            |_| {},
        );
        let shared: Shared = Arc::new(Mutex::new(Manager::new(REMOVAL_AGENT_CMD.into(), true)));
        crate::manager::lock(&shared).persist_at_for_test(root, &workspace, None);
        let app = super::router(Arc::clone(&shared));
        (workspace, shared, app)
    }

    /// One fixture repository whose main checkout carries a live runtime,
    /// behind a persisting daemon whose state file lives under the returned
    /// root. Returns the root, the fixture owner, the workspace, the shared
    /// manager, the router, the session id and the checkout key.
    #[must_use = "the returned FixtureRepo owns the redirect that contains this fixture; bind it \
                  to a named local that outlives every handler await"]
    async fn removal_fixture(
        label: &str,
    ) -> (
        PathBuf,
        FixtureRepo,
        Workspace,
        Shared,
        Router,
        u64,
        CheckoutKey,
    ) {
        let (root, fixture, workspace, shared, app) = removal_setup(label);
        let repo = fixture.path().canonicalize().unwrap();
        let (id, key, _) = admit_removal_session(&shared, &app, &repo, None).await;
        (root, fixture, workspace, shared, app, id, key)
    }

    /// One fixture repository with two checkouts, each owning a live runtime:
    /// the main checkout and a managed worktree on branch `topic`.
    struct TwoCheckoutFixture {
        root: PathBuf,
        repo: FixtureRepo,
        workspace: Workspace,
        shared: Shared,
        app: Router,
        main_id: u64,
        main_key: CheckoutKey,
        worktree_id: u64,
        worktree_key: CheckoutKey,
        worktree_path: PathBuf,
    }

    /// The two-checkout fixture, built on `removal_setup`. Both sessions are
    /// admitted by `POST /sessions` through the router, the worktree with
    /// `"worktree": "topic"`, and both runtimes are seen alive by `ps` before
    /// this returns.
    async fn removal_fixture_two_checkouts(label: &str) -> TwoCheckoutFixture {
        let (root, repo, workspace, shared, app) = removal_setup(label);
        // A managed worktree needs a repository default the daemon can
        // verify, which a bare `git init` cannot give. Stops at `push -u`,
        // the shape `gh repo create` leaves behind and the one the manager's
        // own worktree fixtures use, so admission is not helped by a
        // hand-written `refs/remotes/origin/HEAD`.
        push_to_bare_origin(&root, repo.path());
        let repo_path = repo.path().canonicalize().unwrap();
        let (main_id, main_key, _) = admit_removal_session(&shared, &app, &repo_path, None).await;
        let (worktree_id, worktree_key, worktree_info) =
            admit_removal_session(&shared, &app, &repo_path, Some("topic")).await;
        assert_ne!(main_key, worktree_key);
        let worktree_path = PathBuf::from(worktree_info["cwd"].as_str().unwrap());
        assert!(worktree_path.is_dir(), "{}", worktree_path.display());
        assert_ne!(worktree_path.canonicalize().unwrap(), repo_path);
        for key in [main_key, worktree_key] {
            let report = preview_report(&app, key).await;
            let agent_pid = previewed_agent_pid(&report, key);
            assert!(
                pid_is_alive(agent_pid),
                "runtime {agent_pid} must be live before the fixture is handed out"
            );
        }
        TwoCheckoutFixture {
            root,
            repo,
            workspace,
            shared,
            app,
            main_id,
            main_key,
            worktree_id,
            worktree_key,
            worktree_path,
        }
    }

    /// Give `repo` a real bare `origin` under `root` and push `main` to it
    /// with upstream tracking.
    fn push_to_bare_origin(root: &Path, repo: &Path) {
        let origin = root.join("origin.git");
        std::fs::create_dir_all(&origin).unwrap();
        git(&origin, &["init", "--bare", "-b", "main"]);
        git(repo, &["remote", "add", "origin", origin.to_str().unwrap()]);
        git(repo, &["push", "-u", "origin", "main"]);
    }

    /// `POST /sessions` through the router, returning the created session id
    /// and the durable checkout key its runtime is registered under.
    async fn admit_removal_session(
        shared: &Shared,
        app: &Router,
        repo: &Path,
        worktree: Option<&str>,
    ) -> (u64, CheckoutKey, serde_json::Value) {
        let body = match worktree {
            Some(branch) => serde_json::json!({ "repo": repo, "worktree": branch }),
            None => serde_json::json!({ "repo": repo }),
        };
        let create = app
            .clone()
            .oneshot(post_json("/sessions", &body.to_string()))
            .await
            .unwrap();
        let status = create.status();
        let bytes = create.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(
            status,
            StatusCode::CREATED,
            "POST /sessions {body}: {}",
            String::from_utf8_lossy(&bytes)
        );
        let info: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        let id = info["id"].as_u64().unwrap();
        let key = crate::manager::lock(shared)
            .checkout_key_for_runtime_for_test(id)
            .expect("a created session is registered under a checkout key");
        (id, key, info)
    }

    /// Preview `key` and execute the previewed report echoed verbatim.
    async fn execute_previewed(app: &Router, key: CheckoutKey) -> axum::response::Response {
        let report = preview_report(app, key).await;
        app.clone()
            .oneshot(post_json("/removals", &report.to_string()))
            .await
            .unwrap()
    }

    /// The state file a persisting removal fixture writes.
    fn removal_state_file(root: &Path, workspace: &Workspace) -> PathBuf {
        root.join(workspace.state_file("daemon-state"))
    }

    /// Stop every runtime the fixture still owns and drop its root.
    fn removal_teardown(shared: &Shared, root: &Path) {
        crate::manager::lock(shared).kill_all();
        std::fs::remove_dir_all(root).unwrap();
    }

    async fn preview_report(app: &Router, key: CheckoutKey) -> serde_json::Value {
        let preview = app
            .clone()
            .oneshot(post_json(
                "/removals/preview",
                &serde_json::json!({ "checkout": key }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(preview.status(), StatusCode::OK);
        body_json(preview).await
    }

    /// The agent pid a preview report names for `key`'s one live runtime.
    fn previewed_agent_pid(report: &serde_json::Value, key: CheckoutKey) -> u64 {
        let killed = report["runtimes_killed"].as_array().unwrap();
        assert_eq!(killed.len(), 1, "{report}");
        assert_eq!(killed[0]["row"], serde_json::json!({ "checkout": key }));
        let agent_pid = killed[0]["agent_pid"].as_u64().unwrap();
        assert_ne!(agent_pid, 0);
        agent_pid
    }

    #[tokio::test]
    async fn removals_execute_drops_a_checkout_row_in_one_save() {
        let (root, fixture, _workspace, shared, app, id, key) = removal_fixture("execute").await;
        let repo = fixture.path().canonicalize().unwrap();

        let report = preview_report(&app, key).await;
        assert_eq!(report["planned"], true);
        assert_eq!(
            report["rows_removed"],
            serde_json::json!([{ "checkout": key }])
        );
        let agent_pid = previewed_agent_pid(&report, key);
        assert_eq!(report["paths_deleted"], serde_json::json!([]));
        assert!(
            report["paths_kept"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!(repo)),
            "{report}"
        );
        let listed = body_json(app.clone().oneshot(get("/sessions")).await.unwrap()).await;
        assert_eq!(
            listed.as_array().unwrap().len(),
            1,
            "preview removed nothing"
        );

        assert!(
            pid_is_alive(agent_pid),
            "the fixture runtime must be live before the removal"
        );

        let executed = app
            .clone()
            .oneshot(post_json("/removals", &report.to_string()))
            .await
            .unwrap();
        assert_eq!(executed.status(), StatusCode::OK);
        let executed = body_json(executed).await;
        assert_eq!(executed["planned"], false);
        assert_eq!(
            executed["rows_removed"],
            serde_json::json!([{ "checkout": key }])
        );
        assert_eq!(
            executed["persistence"],
            serde_json::json!({ "outcome": "saved" })
        );

        assert!(
            wait_for_pid_gone(agent_pid),
            "the removed runtime's agent process is still alive"
        );
        let listed = body_json(app.clone().oneshot(get("/sessions")).await.unwrap()).await;
        assert_eq!(listed, serde_json::json!([]));
        {
            let manager = crate::manager::lock(&shared);
            assert_eq!(manager.checkout_key_for_runtime_for_test(id), None);
            assert!(manager.info(id).is_none());
        }
        removal_teardown(&shared, &root);
    }

    /// HIGH #1 regression: the removal save must carry the live metadata of
    /// every surviving row. Archiving reaches the state file only through the
    /// merged projection, so a save that wrote core's bare candidate would
    /// read back un-archived here.
    #[tokio::test]
    async fn removals_preserve_surviving_session_metadata() {
        let fixture = removal_fixture_two_checkouts("survivor").await;
        let app = &fixture.app;

        let archived = app
            .clone()
            .oneshot(post_json(
                &format!("/sessions/{}/archive", fixture.main_id),
                "",
            ))
            .await
            .unwrap();
        assert_eq!(archived.status(), StatusCode::NO_CONTENT);
        let live = crate::manager::lock(&fixture.shared)
            .info(fixture.main_id)
            .unwrap();
        assert!(live.archived);

        let executed = execute_previewed(app, fixture.worktree_key).await;
        assert_eq!(executed.status(), StatusCode::OK);
        let executed = body_json(executed).await;
        assert_eq!(
            executed["rows_removed"],
            serde_json::json!([{ "checkout": fixture.worktree_key }])
        );
        assert!(
            crate::manager::lock(&fixture.shared)
                .info(fixture.worktree_id)
                .is_none(),
            "the removed worktree runtime is still registered"
        );

        let reloaded = baude_core::persist::load_current_at(
            &fixture.root,
            &fixture.workspace.state_file("daemon-state"),
        )
        .unwrap();
        assert_eq!(reloaded.state.checkouts.len(), 1, "{:?}", reloaded.state);
        let survivor = &reloaded.state.checkouts[0];
        assert_eq!(survivor.key, fixture.main_key);
        assert!(
            survivor.session.archived,
            "the removal save regressed the survivor's archived flag"
        );
        assert_eq!(survivor.session.name, live.name);
        assert_eq!(survivor.session.branch, live.branch);

        let mut restored = Manager::new(REMOVAL_AGENT_CMD.into(), true);
        restored.restore_at(&fixture.root, &fixture.workspace);
        assert!(restored
            .preview_removal(RemovalTarget::Checkout(fixture.main_key))
            .is_ok());
        assert!(matches!(
            restored.preview_removal(RemovalTarget::Checkout(fixture.worktree_key)),
            Err(RemovalError::UnknownTarget(_))
        ));
        let after_restore = baude_core::persist::load_current_at(
            &fixture.root,
            &fixture.workspace.state_file("daemon-state"),
        )
        .unwrap();
        let survivor = &after_restore.state.checkouts[0];
        assert_eq!(survivor.key, fixture.main_key);
        assert!(survivor.session.archived);
        assert_eq!(survivor.session.name, live.name);
        assert_eq!(survivor.session.branch, live.branch);

        restored.kill_all();
        removal_teardown(&fixture.shared, &fixture.root);
    }

    #[tokio::test]
    async fn removals_persistence_bookkeeping_matches_save_checked() {
        use baude_core::persist::AtomicFailure;

        let fixture = removal_fixture_two_checkouts("bookkeeping").await;
        let app = &fixture.app;
        let repo_path = fixture.repo.path().canonicalize().unwrap();

        let first = execute_previewed(app, fixture.worktree_key).await;
        assert_status(
            first,
            StatusCode::OK,
            "POST /removals for the managed worktree",
            &AtomicFailure::Rename,
        )
        .await;
        assert!(
            !fixture.worktree_path.exists(),
            "the managed worktree directory survived its removal"
        );
        assert!(repo_path.is_dir(), "the main checkout must be kept");
        let status = crate::manager::lock(&fixture.shared).persistence_status();
        assert!(!status.dirty, "{:?}", status.error);
        assert_eq!(status.error, None);

        let report = preview_report(app, fixture.main_key).await;
        let main_pid = previewed_agent_pid(&report, fixture.main_key);
        assert!(
            pid_is_alive(main_pid),
            "the main runtime must be live before the failing removal"
        );
        crate::manager::lock(&fixture.shared).persist_at_for_test(
            &fixture.root,
            &fixture.workspace,
            Some(AtomicFailure::Rename),
        );
        let second = app
            .clone()
            .oneshot(post_json("/removals", &report.to_string()))
            .await
            .unwrap();
        assert_status(
            second,
            StatusCode::SERVICE_UNAVAILABLE,
            "POST /removals for the main checkout",
            &AtomicFailure::Rename,
        )
        .await;

        {
            let manager = crate::manager::lock(&fixture.shared);
            let status = manager.persistence_status();
            assert!(status.dirty);
            assert!(status.error.is_some_and(|error| !error.is_empty()));
            assert!(manager
                .preview_removal(RemovalTarget::Checkout(fixture.main_key))
                .is_ok());
            assert_eq!(
                manager.checkout_key_for_runtime_for_test(fixture.main_id),
                Some(fixture.main_key)
            );
        }
        let listed = body_json(app.clone().oneshot(get("/sessions")).await.unwrap()).await;
        let listed_ids: Vec<u64> = listed
            .as_array()
            .unwrap()
            .iter()
            .map(|session| session["id"].as_u64().unwrap())
            .collect();
        assert_eq!(listed_ids, vec![fixture.main_id]);
        // Restored is not running: core kills before it persists, so the
        // handle that came back belongs to a runtime that is already gone.
        assert!(
            wait_for_pid_gone(main_pid),
            "core kills before it persists, so the restored handle must be dead"
        );

        removal_teardown(&fixture.shared, &fixture.root);
    }

    #[tokio::test]
    async fn removals_execute_survives_a_fresh_restore() {
        let (root, _fixture, workspace, shared, app, _id, key) = removal_fixture("restore").await;
        let executed = execute_previewed(&app, key).await;
        assert_eq!(executed.status(), StatusCode::OK);

        let mut restored = Manager::new(REMOVAL_AGENT_CMD.into(), true);
        assert_eq!(restored.restore_at(&root, &workspace), 0);
        assert_eq!(restored.list().len(), 0);
        assert!(matches!(
            restored.preview_removal(RemovalTarget::Checkout(key)),
            Err(RemovalError::UnknownTarget(_))
        ));

        removal_teardown(&shared, &root);
    }

    /// Remove `target` over the two removal routes, asserting 200 on both,
    /// and return the executed report as core's own type.
    async fn removed_over_routes(
        app: &Router,
        target: &serde_json::Value,
    ) -> baude_core::removal::RemovalReport {
        let preview = preview_target(app, target).await;
        let (status, body) = removal_call(app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "execute {target}: {body}");
        serde_json::from_str(&body)
            .unwrap_or_else(|error| panic!("the 200 body is a removal report: {error}: {body}"))
    }

    /// Every checkout entry `/rows` serves, across all repository parents.
    fn checkout_entries(rows: &serde_json::Value) -> Vec<serde_json::Value> {
        rows["repositories"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|parent| parent["checkouts"].as_array().unwrap().iter().cloned())
            .collect()
    }

    /// The checkout entries `/rows` serves whose `path` is `path`.
    fn checkout_entries_at(rows: &serde_json::Value, path: &Path) -> Vec<serde_json::Value> {
        checkout_entries(rows)
            .into_iter()
            .filter(|entry| entry["path"].as_str().map(Path::new) == Some(path))
            .collect()
    }

    /// The worktree a user makes by hand: `git worktree add -b <branch> <dir>`
    /// run from `main`, asserted to exit 0. The daemon is not involved.
    fn add_by_hand_worktree(main: &Path, dir: &Path, branch: &str) {
        assert!(!dir.exists(), "{}", dir.display());
        let added = Command::new("git")
            .arg("-C")
            .arg(main)
            .args(["worktree", "add", "-b", branch])
            .arg(dir)
            .output()
            .unwrap();
        assert_eq!(
            added.status.code(),
            Some(0),
            "git worktree add: {}",
            String::from_utf8_lossy(&added.stderr)
        );
    }

    /// Every regular file directly inside `dir` with its bytes, sorted by
    /// name: what a user's directory holds, read from disk.
    fn dir_files(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut files: Vec<(String, Vec<u8>)> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap())
            .filter(|entry| entry.file_type().unwrap().is_file())
            .map(|entry| {
                (
                    entry.file_name().to_string_lossy().into_owned(),
                    std::fs::read(entry.path()).unwrap(),
                )
            })
            .collect();
        files.sort();
        files
    }

    /// A linked worktree the user made with git, opened by its path. It is a
    /// checkout of its repository that the daemon does not manage: the
    /// session records the repository's main worktree and the worktree fact
    /// from discovery, and removing the row keeps the directory.
    #[tokio::test]
    async fn by_hand_worktree_is_admitted_unmanaged_and_removed_with_its_directory_kept() {
        let (root, fixture, _workspace, shared, app) = removal_setup("by-hand");
        let main_path = fixture.path().canonicalize().unwrap();
        let by_hand = root.canonicalize().unwrap().join("by-hand");
        add_by_hand_worktree(&main_path, &by_hand, "by-hand");
        // The `.git` file and every checked-out file, with their bytes.
        let files_before = dir_files(&by_hand);
        assert!(files_before.len() > 1, "{files_before:?}");
        assert!(files_before.iter().any(|(name, _)| name == ".git"));

        // Admitted first, before any row of its repository is stored.
        let (status, body) = open_folder(&app, by_hand.to_str().unwrap(), None).await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "21-10 by-hand worktree pin: POST /sessions for the hand-made linked worktree {} \
             was refused: {body}",
            by_hand.display()
        );
        let info: serde_json::Value = serde_json::from_str(&body).unwrap();
        let id = info["id"].as_u64().unwrap();
        assert_eq!(info["cwd"].as_str(), by_hand.to_str(), "{info}");
        assert_eq!(info["repo_root"].as_str(), main_path.to_str(), "{info}");
        assert_eq!(info["is_worktree"], true, "{info}");

        let rows = rows_body(&app).await;
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");
        assert_eq!(rows["repositories"].as_array().unwrap().len(), 1, "{rows}");
        let entries = checkout_entries_at(&rows, &by_hand);
        assert_eq!(entries.len(), 1, "{rows}");
        let entry = &entries[0];
        assert_eq!(entry["session_id"].as_u64(), Some(id), "{entry}");
        assert_eq!(entry["role"], "managed_branch", "{entry}");
        assert_eq!(
            entry["managed_by_baude"], false,
            "21-10 by-hand worktree pin: a worktree the user made is never managed: {entry}"
        );

        // The main checkout joins the same repository row afterwards.
        let main_id = opened_folder_id(&app, &main_path).await;
        let rows = rows_body(&app).await;
        assert_eq!(rows["repositories"].as_array().unwrap().len(), 1, "{rows}");
        assert_eq!(checkout_entries(&rows).len(), 2, "{rows}");
        let main_entries = checkout_entries_at(&rows, &main_path);
        assert_eq!(main_entries.len(), 1, "{rows}");
        assert_eq!(main_entries[0]["role"], "main", "{rows}");
        assert_eq!(
            main_entries[0]["session_id"].as_u64(),
            Some(main_id),
            "{rows}"
        );

        // Removing the row ends its runtime and keeps the user's directory.
        let target = serde_json::json!({ "checkout": entry["checkout_key"] });
        let preview = preview_target(&app, &target).await;
        let agent_pid = only_runtime_pid(&preview);
        assert!(pid_is_alive(agent_pid));
        let removed = removed_over_routes(&app, &target).await;
        assert!(
            !removed.paths_deleted.contains(&by_hand),
            "21-10 by-hand worktree pin: removal must not delete the user's directory: \
             {removed:?}"
        );
        assert_eq!(removed.paths_deleted, Vec::<PathBuf>::new(), "{removed:?}");
        assert!(removed.paths_kept.contains(&by_hand), "{removed:?}");
        assert!(wait_for_pid_gone(agent_pid));
        assert!(by_hand.is_dir());
        assert_eq!(dir_files(&by_hand), files_before);
        let rows = rows_body(&app).await;
        assert_eq!(
            checkout_entries_at(&rows, &by_hand),
            Vec::<serde_json::Value>::new(),
            "{rows}"
        );
        assert_eq!(checkout_entries_at(&rows, &main_path).len(), 1, "{rows}");

        removal_teardown(&shared, &root);
    }

    /// The bytes `folder_fixture` writes to `todo.txt`, read back after a
    /// removal to prove the folder's contents were left alone.
    const FOLDER_TODO_BYTES: &[u8] = b"water the ferns\ncall the plumber\n";

    /// What a user really has when they open a plain folder: a directory
    /// `notes` that is not a git repository, holding `todo.txt`, behind a
    /// persisting daemon whose state file lives under the returned root. No
    /// row is seeded. Returns the root, the redirect owner, the canonicalized
    /// folder, the workspace, the shared manager and the router.
    #[must_use = "the returned FixtureRepo owns the redirect that contains this fixture; bind it \
                  to a named local that outlives every handler await"]
    fn folder_fixture(label: &str) -> (PathBuf, FixtureRepo, PathBuf, Workspace, Shared, Router) {
        let root = removal_root(label);
        // Root first, identity second, the order `initialized_repo_in_workspace`
        // acquires them in.
        let redirect = baude_core::testing::TestRedirect::new(&root);
        let identity = baude_core::workspace::override_for_test(
            &baude_core::persist::Config {
                workspace: Some("claude".to_string()),
                ..baude_core::persist::Config::default()
            },
            None,
        );
        let folder = root.join("notes");
        std::fs::create_dir_all(&folder).unwrap();
        std::fs::write(folder.join("todo.txt"), FOLDER_TODO_BYTES).unwrap();
        let folder = folder.canonicalize().unwrap();

        // Precondition, asked of git and not assumed: a temp directory that
        // happens to sit inside a repository fails here, loudly, and never
        // reaches the wrong admission route.
        let probe = Command::new("git")
            .arg("-C")
            .arg(&folder)
            .args(["rev-parse", "--git-dir"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&probe.stderr);
        assert!(
            !probe.status.success(),
            "the fixture folder {} sits inside a git repository",
            folder.display()
        );
        assert!(
            stderr.starts_with("fatal: not a git repository"),
            "git refused {} for another reason: {stderr}",
            folder.display()
        );

        let owner = FixtureRepo {
            repo: folder.clone(),
            _identity: identity,
            _redirect: redirect,
        };
        let (workspace, shared, app) = removal_daemon(&root);
        (root, owner, folder, workspace, shared, app)
    }

    /// 21-10 tracer. A plain folder opened over `POST /sessions` is one
    /// standalone row with a live runtime; the two removal routes remove the
    /// row and end the runtime, and the folder and its contents stay.
    #[tokio::test]
    async fn standalone_folder_session_is_removed_end_to_end() {
        let (root, owner, folder, workspace, shared, app) = folder_fixture("standalone-e2e");
        assert_eq!(owner.path(), folder.as_path());
        let folder_text = folder.to_str().unwrap().to_string();

        let created = app
            .clone()
            .oneshot(post_json(
                "/sessions",
                &serde_json::json!({ "repo": folder_text }).to_string(),
            ))
            .await
            .unwrap();
        assert_eq!(created.status(), StatusCode::CREATED);
        let id = body_json(created).await["id"].as_u64().unwrap();

        // The standalone pin, asserted first so it is what goes red.
        let rows = rows_body(&app).await;
        let pinned = rows["repositories"].as_array().is_some_and(Vec::is_empty)
            && rows["standalone"]
                .as_array()
                .is_some_and(|entries| entries.len() == 1);
        assert!(
            pinned,
            "21-10 standalone pin: a plain folder is exactly one standalone row and no \
             repository row, but /rows served: {rows}"
        );
        let entry = &rows["standalone"][0];
        assert_rows_fields(
            entry,
            &[
                "archived",
                "display_name",
                "path",
                "session_id",
                "standalone_key",
            ],
        );
        assert_eq!(entry["session_id"].as_u64(), Some(id), "{entry}");
        assert_eq!(entry["path"].as_str(), Some(folder_text.as_str()));
        assert_eq!(entry["archived"], serde_json::json!(false));
        let key = entry["standalone_key"]
            .as_u64()
            .unwrap_or_else(|| panic!("standalone_key is a JSON number: {entry}"));
        let row = serde_json::json!({ "standalone": key });

        // D-09 Option B: `/sessions` lists the session and carries no row key.
        let sessions = body_json(app.clone().oneshot(get("/sessions")).await.unwrap()).await;
        let listed = sessions.as_array().unwrap();
        assert_eq!(listed.len(), 1, "{sessions}");
        assert_eq!(listed[0]["id"].as_u64(), Some(id));
        assert!(
            listed[0].get("standalone_key").is_none() && listed[0].get("checkout_key").is_none(),
            "{sessions}"
        );

        // Preview.
        let preview = preview_target(&app, &row).await;
        assert_eq!(preview["rows_removed"], serde_json::json!([row]));
        assert_eq!(
            report_paths(&preview, "paths_deleted"),
            Vec::<String>::new()
        );
        assert_eq!(
            report_paths(&preview, "paths_kept"),
            vec![folder_text.clone()]
        );
        let planned = preview["runtimes_killed"].as_array().unwrap();
        assert_eq!(planned.len(), 1, "{preview}");
        assert_eq!(planned[0]["row"], row, "{preview}");
        assert_eq!(
            preview["persistence"],
            serde_json::json!({ "outcome": "planned" })
        );
        assert!(
            preview["cleared"]
                .as_array()
                .unwrap()
                .iter()
                .all(|item| item["kind"] != "suppression_added"),
            "{preview}"
        );
        let agent_pid = planned[0]["agent_pid"]
            .as_u64()
            .unwrap_or_else(|| panic!("the planned runtime names its agent pid: {preview}"));
        assert!(pid_is_alive(agent_pid));

        // Execute, echoing the previewed report verbatim.
        let (status, body) = removal_call(&app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let executed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(executed["rows_removed"], preview["rows_removed"]);
        let killed = executed["runtimes_killed"].as_array().unwrap();
        assert_eq!(killed.len(), 1, "{executed}");
        assert_eq!(killed[0]["row"], row, "{executed}");
        assert_eq!(
            executed["persistence"],
            serde_json::json!({ "outcome": "saved" })
        );

        // The world afterwards.
        assert!(wait_for_pid_gone(agent_pid));
        assert!(folder.is_dir());
        assert_eq!(
            std::fs::read(folder.join("todo.txt")).unwrap(),
            FOLDER_TODO_BYTES
        );
        let rows = rows_body(&app).await;
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");
        assert_eq!(rows["repositories"], serde_json::json!([]), "{rows}");
        assert_eq!(session_ids(&app).await, Vec::<u64>::new());

        let mut restored = Manager::new(REMOVAL_AGENT_CMD.into(), true);
        assert_eq!(restored.restore_at(&root, &workspace), 0);
        assert_eq!(restored.list().len(), 0);
        let restored_rows = restored.rows();
        assert!(restored_rows.standalone.is_empty());
        assert!(restored_rows.repositories.is_empty());
        assert!(!baude_core::suppression::is_suppressed(
            &baude_core::persist::config_dir(),
            &folder
        ));

        removal_teardown(&shared, &root);
    }

    /// The command `recording_folder_fixture` hands its daemon for `root`.
    fn recording_agent_cmd(root: &Path) -> String {
        format!("sh {}", root.join("agent.sh").display())
    }

    /// `folder_fixture` with an agent double that records every launch. The
    /// command is `sh` followed by `agent.sh` in the test root; the script
    /// appends its whole argument list as one line to `launches.log` in that
    /// root and then replaces itself with `sleep 30`. It accepts every
    /// argument the daemon appends, so a continuation launch is recorded and
    /// not refused. Tests read the log to learn how many launches happened
    /// and whether two of them were given the same arguments.
    #[must_use = "the returned FixtureRepo owns the redirect that contains this fixture; bind it \
                  to a named local that outlives every handler await"]
    fn recording_folder_fixture(
        label: &str,
    ) -> (PathBuf, FixtureRepo, PathBuf, Workspace, Shared, Router) {
        let (root, owner, folder, workspace, _plain_shared, _plain_app) = folder_fixture(label);
        let root_text = root.to_str().unwrap();
        assert!(
            !root_text.contains(char::is_whitespace) && !root_text.contains('\''),
            "the recording agent command is spliced unquoted: {root_text}"
        );
        std::fs::write(
            root.join("agent.sh"),
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\nexec sleep 30\n",
                root.join("launches.log").display()
            ),
        )
        .unwrap();
        let shared: Shared = Arc::new(Mutex::new(Manager::new(recording_agent_cmd(&root), true)));
        crate::manager::lock(&shared).persist_at_for_test(&root, &workspace, None);
        let app = super::router(Arc::clone(&shared));
        (root, owner, folder, workspace, shared, app)
    }

    /// The lines of `launches.log`, read once at least `expected` launches
    /// are recorded or the five second bound has passed. The agent double
    /// writes its line a moment after the daemon reports the launch.
    fn launch_lines(root: &Path, expected: usize) -> Vec<String> {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            let lines: Vec<String> = std::fs::read_to_string(root.join("launches.log"))
                .unwrap_or_default()
                .lines()
                .map(str::to_string)
                .collect();
            if lines.len() >= expected || Instant::now() >= deadline {
                return lines;
            }
            std::thread::sleep(Duration::from_millis(25));
        }
    }

    /// A second persisting daemon over the same root and command, restored
    /// from the state file the first one wrote. The first instance is neither
    /// dropped nor asked to end anything first: its agent process is still
    /// alive when this one restores, which is the state a crashed daemon
    /// leaves. Callers `std::mem::forget` the first instance at their end.
    fn restarted_daemon(root: &Path, workspace: &Workspace, cmd: &str) -> (Shared, Router) {
        let mut manager = Manager::new(cmd.to_string(), true);
        manager.persist_at_for_test(root, workspace, None);
        manager.restore_at(root, workspace);
        let shared: Shared = Arc::new(Mutex::new(manager));
        let app = super::router(Arc::clone(&shared));
        (shared, app)
    }

    /// `POST /sessions` for a folder, returning the status and the raw body.
    async fn open_folder(app: &Router, repo: &str, name: Option<&str>) -> (StatusCode, String) {
        let mut body = serde_json::json!({ "repo": repo });
        if let Some(name) = name {
            body["name"] = serde_json::json!(name);
        }
        removal_call(app, "/sessions", &body).await
    }

    /// Open `folder` as a new session, asserting 201, and return its id.
    async fn opened_folder_id(app: &Router, folder: &Path) -> u64 {
        let (status, body) = open_folder(app, folder.to_str().unwrap(), None).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        serde_json::from_str::<serde_json::Value>(&body).unwrap()["id"]
            .as_u64()
            .unwrap()
    }

    /// The single standalone entry `/rows` serves, with no repository row.
    async fn only_standalone_row(app: &Router) -> serde_json::Value {
        let rows = rows_body(app).await;
        assert_eq!(rows["repositories"], serde_json::json!([]), "{rows}");
        let entries = rows["standalone"].as_array().unwrap();
        assert_eq!(entries.len(), 1, "{rows}");
        entries[0].clone()
    }

    fn standalone_key_of(entry: &serde_json::Value) -> u64 {
        entry["standalone_key"]
            .as_u64()
            .unwrap_or_else(|| panic!("standalone_key is a JSON number: {entry}"))
    }

    /// The agent pid of the one runtime a report plans to kill or killed.
    fn only_runtime_pid(report: &serde_json::Value) -> u64 {
        let runtimes = report["runtimes_killed"].as_array().unwrap();
        assert_eq!(runtimes.len(), 1, "{report}");
        runtimes[0]["agent_pid"]
            .as_u64()
            .unwrap_or_else(|| panic!("the runtime names its agent pid: {report}"))
    }

    async fn delete_session_status(app: &Router, id: u64) -> (StatusCode, String) {
        let res = app
            .clone()
            .oneshot(
                Request::delete(format!("/sessions/{id}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    fn assert_folder_untouched(folder: &Path) {
        assert!(folder.is_dir(), "{} is gone", folder.display());
        assert_eq!(
            std::fs::read(folder.join("todo.txt")).unwrap(),
            FOLDER_TODO_BYTES
        );
    }

    /// Custody on close. `DELETE /sessions/{id}` on a folder session ends the
    /// runtime and keeps the row, closed; a daemon restart does not relaunch
    /// it; and the closed row is removed with nothing left to kill.
    #[tokio::test]
    async fn standalone_closed_row_stays_closed_across_a_restart_and_is_removable() {
        let (root, _owner, folder, workspace, shared, app) =
            recording_folder_fixture("standalone-closed");
        let id = opened_folder_id(&app, &folder).await;
        let key = standalone_key_of(&only_standalone_row(&app).await);
        let row = serde_json::json!({ "standalone": key });
        let agent_pid = only_runtime_pid(&preview_target(&app, &row).await);
        assert!(pid_is_alive(agent_pid));
        assert_eq!(launch_lines(&root, 1).len(), 1);

        let (status, body) = delete_session_status(&app, id).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        assert!(wait_for_pid_gone(agent_pid));
        assert_eq!(session_ids(&app).await, Vec::<u64>::new());
        let entry = only_standalone_row(&app).await;
        assert_eq!(standalone_key_of(&entry), key);
        assert_eq!(entry["session_id"], serde_json::Value::Null, "{entry}");

        // The custody pin, asserted next so it is what goes red.
        let (status, body) = removal_call(&app, "/removals/preview", &row).await;
        let nothing_to_kill = status == StatusCode::OK
            && serde_json::from_str::<serde_json::Value>(&body)
                .ok()
                .and_then(|report| report["runtimes_killed"].as_array().cloned())
                .is_some_and(|runtimes| runtimes.is_empty());
        assert!(
            nothing_to_kill,
            "21-10 custody pin: a closed folder row records no runtime, but its preview \
             returned {status}: {body}"
        );

        let (restarted, restarted_app) =
            restarted_daemon(&root, &workspace, &recording_agent_cmd(&root));
        let entry = only_standalone_row(&restarted_app).await;
        assert_eq!(standalone_key_of(&entry), key);
        assert_eq!(entry["session_id"], serde_json::Value::Null, "{entry}");
        assert_eq!(session_ids(&restarted_app).await, Vec::<u64>::new());
        let launches = launch_lines(&root, 1);
        assert_eq!(launches.len(), 1, "{launches:?}");

        let preview = preview_target(&restarted_app, &row).await;
        let (status, body) = removal_call(&restarted_app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let executed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(executed["rows_removed"], serde_json::json!([row]));
        assert_eq!(executed["runtimes_killed"], serde_json::json!([]));
        assert_folder_untouched(&folder);
        let rows = rows_body(&restarted_app).await;
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");

        std::mem::forget((shared, app));
        removal_teardown(&restarted, &root);
    }

    /// Custody across a daemon restart. The first daemon's agent is still
    /// alive when the second restores: the second ends it by its recorded
    /// identity, relaunches the row under the same key as a continuation and
    /// keeps the archive flag, and the relaunched row is removable.
    #[tokio::test]
    async fn standalone_restart_tears_down_the_recorded_runtime_and_relaunches() {
        let (root, _owner, folder, workspace, shared, app) =
            recording_folder_fixture("standalone-restart");
        let id = opened_folder_id(&app, &folder).await;
        let (status, body) = removal_call(
            &app,
            &format!("/sessions/{id}/archive"),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        let key = standalone_key_of(&only_standalone_row(&app).await);
        let row = serde_json::json!({ "standalone": key });
        let first_pid = only_runtime_pid(&preview_target(&app, &row).await);
        assert!(pid_is_alive(first_pid));
        assert_eq!(launch_lines(&root, 1).len(), 1);

        let (restarted, restarted_app) =
            restarted_daemon(&root, &workspace, &recording_agent_cmd(&root));
        assert!(
            wait_for_pid_gone(first_pid),
            "the restart left the first daemon's agent running"
        );
        let entry = only_standalone_row(&restarted_app).await;
        assert_eq!(standalone_key_of(&entry), key);
        assert_eq!(entry["archived"], serde_json::json!(true), "{entry}");
        assert!(
            entry["session_id"].as_u64().is_some(),
            "21-10 custody pin: a folder session that was live at the restart is relaunched \
             under its row, but /rows served: {entry}"
        );

        let preview = preview_target(&restarted_app, &row).await;
        let second_pid = only_runtime_pid(&preview);
        assert_ne!(second_pid, first_pid);
        assert!(pid_is_alive(second_pid));
        let (status, body) = removal_call(&restarted_app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let executed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(only_runtime_pid(&executed), second_pid);
        assert!(wait_for_pid_gone(second_pid));
        assert_folder_untouched(&folder);
        let rows = rows_body(&restarted_app).await;
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");

        // The first launch was fresh and the second was a continuation.
        let launches = launch_lines(&root, 2);
        assert_eq!(launches.len(), 2, "{launches:?}");
        assert_ne!(launches[0], launches[1], "{launches:?}");

        std::mem::forget((shared, app));
        removal_teardown(&restarted, &root);
    }

    /// One row per folder. A repeated `POST /sessions` for a live folder is
    /// the same session; for a closed folder it reopens the same row.
    #[tokio::test]
    async fn standalone_reposting_a_folder_reuses_its_row() {
        let (root, _owner, folder, _workspace, shared, app) =
            recording_folder_fixture("standalone-repost");
        let id = opened_folder_id(&app, &folder).await;
        let first = only_standalone_row(&app).await;
        let key = standalone_key_of(&first);
        let admitted_name = first["display_name"].clone();

        // The same folder, spelled through a parent hop.
        let respelled = format!("{}/notes/../notes", root.display());
        let (status, body) = open_folder(&app, &respelled, Some("renamed by a repost")).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let reposted: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(reposted["id"].as_u64(), Some(id), "{reposted}");
        let entry = only_standalone_row(&app).await;
        assert_eq!(standalone_key_of(&entry), key);
        assert_eq!(entry["display_name"], admitted_name, "{entry}");
        assert_eq!(session_ids(&app).await, vec![id]);
        let launches = launch_lines(&root, 1);
        assert_eq!(launches.len(), 1, "{launches:?}");

        let (status, body) = delete_session_status(&app, id).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        let (status, body) = open_folder(&app, folder.to_str().unwrap(), None).await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "21-10 custody pin: a closed folder row is reopened by a new POST /sessions, but \
             the daemon answered: {body}"
        );
        let reopened_id = serde_json::from_str::<serde_json::Value>(&body).unwrap()["id"]
            .as_u64()
            .unwrap();
        let entry = only_standalone_row(&app).await;
        assert_eq!(standalone_key_of(&entry), key);
        assert_eq!(entry["session_id"].as_u64(), Some(reopened_id), "{entry}");
        assert_eq!(entry["display_name"], admitted_name, "{entry}");
        assert_eq!(session_ids(&app).await, vec![reopened_id]);
        let launches = launch_lines(&root, 2);
        assert_eq!(launches.len(), 2, "{launches:?}");
        assert_ne!(launches[0], launches[1], "{launches:?}");

        removal_teardown(&shared, &root);
    }

    /// The restart route on a folder session whose agent died under the
    /// user. The relaunch stays owned by the row, so the row is removable.
    #[tokio::test]
    async fn standalone_restart_route_keeps_the_row_removable() {
        let (root, _owner, folder, _workspace, shared, app) =
            recording_folder_fixture("standalone-restart-route");
        let id = opened_folder_id(&app, &folder).await;
        let key = standalone_key_of(&only_standalone_row(&app).await);
        let row = serde_json::json!({ "standalone": key });
        let ended_pid = only_runtime_pid(&preview_target(&app, &row).await);
        assert!(pid_is_alive(ended_pid));
        assert_eq!(launch_lines(&root, 1).len(), 1);

        // End the agent from outside, the way it dies under a user.
        let killed = Command::new("kill")
            .arg(ended_pid.to_string())
            .status()
            .unwrap();
        assert!(killed.success());
        assert!(wait_for_pid_gone(ended_pid));

        // Restart refuses a session whose agent the daemon has not yet seen
        // exit; repeat until that refusal is over.
        let deadline = Instant::now() + Duration::from_secs(5);
        let (status, body) = loop {
            let (status, body) = removal_call(
                &app,
                &format!("/sessions/{id}/restart"),
                &serde_json::json!({}),
            )
            .await;
            if status != StatusCode::CONFLICT || Instant::now() >= deadline {
                break (status, body);
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        };
        assert!(status.is_success(), "restart answered {status}: {body}");

        // The session id is read back; it is never assumed unchanged.
        let entry = only_standalone_row(&app).await;
        assert_eq!(standalone_key_of(&entry), key);
        let restarted_id = entry["session_id"].as_u64();
        assert!(
            restarted_id.is_some(),
            "21-10 custody pin: a restarted folder session stays owned by its row, but /rows \
             served: {entry}"
        );
        assert_eq!(session_ids(&app).await, vec![restarted_id.unwrap()]);

        let preview = preview_target(&app, &row).await;
        let relaunched_pid = only_runtime_pid(&preview);
        assert_ne!(relaunched_pid, ended_pid);
        assert!(pid_is_alive(relaunched_pid));
        let (status, body) = removal_call(&app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let executed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(only_runtime_pid(&executed), relaunched_pid);
        assert!(wait_for_pid_gone(relaunched_pid));
        assert_folder_untouched(&folder);
        let rows = rows_body(&app).await;
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");

        removal_teardown(&shared, &root);
    }

    /// A folder deleted by hand while the daemon was down. The row is still
    /// served and still removable, and removal recreates nothing.
    #[tokio::test]
    async fn standalone_row_whose_folder_vanished_is_still_removable() {
        let (root, _owner, folder, workspace, shared, app) = folder_fixture("standalone-vanished");
        let folder_text = folder.to_str().unwrap().to_string();
        opened_folder_id(&app, &folder).await;
        let key = standalone_key_of(&only_standalone_row(&app).await);
        let row = serde_json::json!({ "standalone": key });
        let first_pid = only_runtime_pid(&preview_target(&app, &row).await);
        assert!(pid_is_alive(first_pid));

        std::fs::remove_dir_all(&folder).unwrap();
        let (restarted, restarted_app) = restarted_daemon(&root, &workspace, REMOVAL_AGENT_CMD);
        assert!(
            wait_for_pid_gone(first_pid),
            "the restart left the first daemon's agent running"
        );
        let entry = only_standalone_row(&restarted_app).await;
        assert_eq!(standalone_key_of(&entry), key);
        assert_eq!(entry["path"].as_str(), Some(folder_text.as_str()));
        assert_eq!(
            entry["session_id"],
            serde_json::Value::Null,
            "21-10 custody pin: a row whose folder vanished is served closed, but /rows \
             served: {entry}"
        );

        let preview = preview_target(&restarted_app, &row).await;
        assert_eq!(preview["runtimes_killed"], serde_json::json!([]));
        assert_eq!(
            report_paths(&preview, "paths_deleted"),
            Vec::<String>::new()
        );
        let (status, body) = removal_call(&restarted_app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let executed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(executed["rows_removed"], serde_json::json!([row]));
        let rows = rows_body(&restarted_app).await;
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");
        assert!(!folder.exists(), "removal recreated {}", folder.display());

        std::mem::forget((shared, app));
        removal_teardown(&restarted, &root);
    }

    /// Two executes of one previewed standalone report contend on the manager
    /// mutex; exactly one commits and only it reports the kill.
    #[tokio::test]
    async fn standalone_concurrent_removals_of_one_folder_commit_once() {
        let (root, _owner, folder, workspace, shared, app) =
            folder_fixture("standalone-concurrent");
        opened_folder_id(&app, &folder).await;
        let key = standalone_key_of(&only_standalone_row(&app).await);
        let row = serde_json::json!({ "standalone": key });
        let preview = preview_target(&app, &row).await;
        let agent_pid = only_runtime_pid(&preview);
        assert!(pid_is_alive(agent_pid));
        let body = preview.to_string();

        let (first, second) = tokio::join!(
            app.clone().oneshot(post_json("/removals", &body)),
            app.clone().oneshot(post_json("/removals", &body)),
        );
        let mut outcomes = Vec::new();
        for response in [first.unwrap(), second.unwrap()] {
            let status = response.status().as_u16();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            outcomes.push((status, String::from_utf8_lossy(&bytes).into_owned()));
        }
        let winners = outcomes.iter().filter(|(status, _)| *status == 200).count();
        assert_eq!(winners, 1, "exactly one removal commits: {outcomes:?}");
        let mut statuses: Vec<u16> = outcomes.iter().map(|(status, _)| *status).collect();
        statuses.sort_unstable();
        assert_eq!(
            statuses,
            [200, 404],
            "exactly one removal commits: {outcomes:?}"
        );
        for (status, text) in &outcomes {
            let reports_a_kill = serde_json::from_str::<serde_json::Value>(text)
                .ok()
                .and_then(|value| value["runtimes_killed"].as_array().cloned())
                .is_some_and(|killed| !killed.is_empty());
            assert_eq!(reports_a_kill, *status == 200, "{outcomes:?}");
        }

        assert!(wait_for_pid_gone(agent_pid));
        assert_folder_untouched(&folder);
        let rows = rows_body(&app).await;
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");
        let mut restored = Manager::new(REMOVAL_AGENT_CMD.into(), true);
        assert_eq!(restored.restore_at(&root, &workspace), 0);
        assert!(restored.rows().standalone.is_empty());

        removal_teardown(&shared, &root);
    }

    /// The standalone sibling of `delete_teardown_failure_is_500_and_unknown_id_stays_404`.
    /// A folder session whose agent refuses to stop answers 500, keeps its
    /// session and its row, and is closed by the retry.
    #[tokio::test]
    async fn standalone_delete_teardown_failure_is_500_and_is_retryable() {
        let (root, _owner, folder, _workspace, shared, app) =
            folder_fixture("standalone-teardown-failure");
        let id = opened_folder_id(&app, &folder).await;
        let key = standalone_key_of(&only_standalone_row(&app).await);
        let row = serde_json::json!({ "standalone": key });
        let agent_pid = only_runtime_pid(&preview_target(&app, &row).await);
        crate::manager::lock(&shared)
            .fail_next_agent_teardown_for_test(id, "agent stop refused once");

        let (status, body) = delete_session_status(&app, id).await;
        assert_eq!(
            status,
            StatusCode::INTERNAL_SERVER_ERROR,
            "21-10 custody pin: a folder session whose teardown fails answers 500: {body}"
        );
        assert!(body.contains("agent stop refused once"), "body={body}");
        assert_eq!(
            session_ids(&app).await,
            vec![id],
            "a failed teardown must keep the session"
        );
        let entry = only_standalone_row(&app).await;
        assert_eq!(standalone_key_of(&entry), key);
        assert_eq!(entry["session_id"].as_u64(), Some(id), "{entry}");

        // The failure is retryable: the next DELETE closes it for real.
        let (status, body) = delete_session_status(&app, id).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        assert!(wait_for_pid_gone(agent_pid));
        let entry = only_standalone_row(&app).await;
        assert_eq!(standalone_key_of(&entry), key);
        assert_eq!(entry["session_id"], serde_json::Value::Null, "{entry}");
        let preview = preview_target(&app, &row).await;
        assert_eq!(preview["runtimes_killed"], serde_json::json!([]));
        // The closed session's id is unknown from then on.
        let (status, body) = delete_session_status(&app, id).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{body}");

        removal_teardown(&shared, &root);
    }

    /// The standalone sibling of the `DELETE` step of
    /// `real_atomic_persistence_failures_are_503_for_every_mutation`. A save
    /// that never replaced the file refuses the close before anything is
    /// ended; a save that replaced it but could not confirm durability ends
    /// the runtime and still answers 503.
    #[tokio::test]
    async fn standalone_delete_persistence_failure_is_503() {
        use baude_core::persist::{self, AtomicFailure};

        for (failure, committed, label) in [
            (AtomicFailure::Rename, false, "standalone-503-rename"),
            (AtomicFailure::DirectorySync, true, "standalone-503-sync"),
        ] {
            let (root, _owner, folder, workspace, shared, app) = folder_fixture(label);
            let id = opened_folder_id(&app, &folder).await;
            let key = standalone_key_of(&only_standalone_row(&app).await);
            let row = serde_json::json!({ "standalone": key });
            let agent_pid = only_runtime_pid(&preview_target(&app, &row).await);
            crate::manager::lock(&shared).persist_at_for_test(&root, &workspace, Some(failure));

            let (status, body) = delete_session_status(&app, id).await;
            assert_eq!(
                status,
                StatusCode::SERVICE_UNAVAILABLE,
                "21-10 custody pin: DELETE /sessions/{{id}} on a folder session under \
                 {failure:?}: {body}"
            );
            assert_eq!(
                crate::manager::lock(&shared).info(id).is_none(),
                committed,
                "{failure:?}"
            );
            let retained = persist::load_current_at(&root, &workspace.state_file("daemon-state"))
                .unwrap()
                .state;
            assert_eq!(retained.standalone_sessions.len(), 1, "{failure:?}");
            assert_eq!(
                retained.standalone_sessions[0].owned_runtime().is_some(),
                !committed,
                "{failure:?}"
            );
            let entry = only_standalone_row(&app).await;
            assert_eq!(standalone_key_of(&entry), key);
            if committed {
                assert!(wait_for_pid_gone(agent_pid), "{failure:?}");
                assert_eq!(entry["session_id"], serde_json::Value::Null, "{entry}");
            } else {
                assert!(pid_is_alive(agent_pid), "{failure:?}");
                assert_eq!(entry["session_id"].as_u64(), Some(id), "{entry}");
                // With the disk healthy again the same close goes through.
                crate::manager::lock(&shared).persist_at_for_test(&root, &workspace, None);
                let (status, body) = delete_session_status(&app, id).await;
                assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
                assert!(wait_for_pid_gone(agent_pid), "{failure:?}");
            }

            removal_teardown(&shared, &root);
        }
    }

    /// The paths of the `kind` items an executed report cleared.
    fn cleared_paths(
        report: &baude_core::removal::RemovalReport,
        kind: baude_core::removal::ClearingKind,
    ) -> Vec<PathBuf> {
        report
            .cleared
            .iter()
            .filter(|item| item.kind == kind)
            .map(|item| item.path.clone())
            .collect()
    }

    /// The one path an executed report says it suppressed. Every suppressed
    /// path in these tests comes from here and none is rebuilt by a test.
    fn suppression_added_path(report: &baude_core::removal::RemovalReport) -> PathBuf {
        let added = cleared_paths(report, baude_core::removal::ClearingKind::SuppressionAdded);
        assert_eq!(
            added.len(),
            1,
            "exactly one suppression entry was written: {report:?}"
        );
        added[0].clone()
    }

    /// Whether the store under the test root holds an entry for `path`.
    fn reads_suppressed(path: &Path) -> bool {
        baude_core::suppression::is_suppressed(&baude_core::persist::config_dir(), path)
    }

    /// D-11 lift rule. An explicit admission lifts the entry for exactly the
    /// path it admits. A refused request and a branch recreated on a new
    /// path lift nothing.
    #[tokio::test]
    async fn suppression_explicit_admission_lifts_the_exact_path() {
        let fixture = removal_fixture_two_checkouts("suppress-lift").await;
        let app = &fixture.app;
        let main_path = fixture.repo.path().canonicalize().unwrap();

        // A managed worktree removed with its directory: P.
        let removed = removed_over_routes(
            app,
            &serde_json::json!({ "checkout": fixture.worktree_key }),
        )
        .await;
        let p = suppression_added_path(&removed);
        assert!(!removed.paths_deleted.is_empty(), "{removed:?}");
        assert!(!p.exists(), "{}", p.display());
        assert!(reads_suppressed(&p), "{}", p.display());

        // A refused request lifts nothing: nothing is at P to admit.
        let (status, body) = open_folder(app, p.to_str().unwrap(), None).await;
        assert_ne!(status, StatusCode::CREATED, "{body}");
        assert!(!p.exists(), "{}", p.display());
        assert!(reads_suppressed(&p), "{}", p.display());

        // Recreating the branch is not a lift: it lands on a new path.
        let (status, body) = removal_call(
            app,
            "/sessions",
            &serde_json::json!({ "repo": main_path, "worktree": "topic" }),
        )
        .await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        let recreated_id = serde_json::from_str::<serde_json::Value>(&body).unwrap()["id"]
            .as_u64()
            .unwrap();
        let rows = rows_body(app).await;
        let recreated = rows_checkout_for_session(&rows, recreated_id);
        let recreated_path = PathBuf::from(recreated["path"].as_str().unwrap());
        assert_ne!(recreated_path, p, "{rows}");
        assert!(!p.exists(), "{}", p.display());
        assert!(reads_suppressed(&p), "{}", p.display());
        assert!(
            !reads_suppressed(&recreated_path),
            "{}",
            recreated_path.display()
        );

        // A worktree the user made by hand, removed with its directory kept: Q.
        let by_hand = fixture.root.canonicalize().unwrap().join("by-hand");
        add_by_hand_worktree(&main_path, &by_hand, "by-hand");
        let by_hand_id = opened_folder_id(app, &by_hand).await;
        let rows = rows_body(app).await;
        let by_hand_entries = checkout_entries_at(&rows, &by_hand);
        assert_eq!(by_hand_entries.len(), 1, "{rows}");
        assert_eq!(
            by_hand_entries[0]["session_id"].as_u64(),
            Some(by_hand_id),
            "{rows}"
        );
        let removed = removed_over_routes(
            app,
            &serde_json::json!({ "checkout": by_hand_entries[0]["checkout_key"] }),
        )
        .await;
        assert_eq!(removed.paths_deleted, Vec::<PathBuf>::new(), "{removed:?}");
        assert!(removed.paths_kept.contains(&by_hand), "{removed:?}");
        let q = suppression_added_path(&removed);
        assert!(by_hand.is_dir());
        assert!(by_hand.join(".git").is_file());
        assert!(q.is_dir(), "{}", q.display());
        assert!(reads_suppressed(&q), "{}", q.display());

        // The suppression pin: the directory is present throughout, and the
        // only thing that changes between the two readings is the admission.
        let (status, body) = open_folder(app, q.to_str().unwrap(), None).await;
        assert_eq!(status, StatusCode::CREATED, "{body}");
        assert!(q.is_dir(), "{}", q.display());
        assert!(
            !reads_suppressed(&q),
            "21-10 suppression pin: an explicit POST /sessions admitted {} and its \
             suppression entry is still in the store",
            q.display()
        );
        let rows = rows_body(app).await;
        assert_eq!(checkout_entries_at(&rows, &q).len(), 1, "{rows}");

        // Admitting Q lifted Q and nothing else.
        assert!(!p.exists(), "{}", p.display());
        assert!(reads_suppressed(&p), "{}", p.display());

        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// D-11 write rule, the direction a user sees: removing the repository
    /// parent drops the entry its removed child left behind.
    #[tokio::test]
    async fn suppression_parent_removal_drops_the_child_entry() {
        let fixture = removal_fixture_two_checkouts("suppress-parent").await;
        let app = &fixture.app;

        let removed = removed_over_routes(
            app,
            &serde_json::json!({ "checkout": fixture.worktree_key }),
        )
        .await;
        let p = suppression_added_path(&removed);
        assert!(!p.exists(), "{}", p.display());
        assert!(reads_suppressed(&p), "{}", p.display());

        let rows = rows_body(app).await;
        let parents = rows["repositories"].as_array().unwrap();
        assert_eq!(parents.len(), 1, "{rows}");
        let removed = removed_over_routes(
            app,
            &serde_json::json!({ "repository": parents[0]["repository_key"] }),
        )
        .await;
        assert!(
            !cleared_paths(
                &removed,
                baude_core::removal::ClearingKind::SuppressionDropped
            )
            .is_empty(),
            "{removed:?}"
        );
        assert!(!p.exists(), "{}", p.display());
        assert!(!reads_suppressed(&p), "{}", p.display());

        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// D-11: nothing but an explicit admission lifts. A daemon restart admits
    /// no row for a removed path, relaunches what was running, and leaves
    /// every entry in the store, including one for a row it does relaunch.
    #[tokio::test]
    async fn suppression_restart_admits_nothing_and_lifts_nothing() {
        let fixture = removal_fixture_two_checkouts("suppress-restart").await;
        let app = &fixture.app;
        let main_path = fixture.repo.path().canonicalize().unwrap();
        let before = checkout_entries(&rows_body(app).await).len();

        let removed = removed_over_routes(
            app,
            &serde_json::json!({ "checkout": fixture.worktree_key }),
        )
        .await;
        let p = suppression_added_path(&removed);
        assert!(!p.exists(), "{}", p.display());
        assert!(reads_suppressed(&p), "{}", p.display());

        // An entry for the main checkout, written the way the TUI's removal
        // writes one through core: the path, and the repository's common
        // directory as git reports it for that path.
        let common_dir = PathBuf::from(
            git_read(
                &main_path,
                &["rev-parse", "--path-format=absolute", "--git-common-dir"],
            )
            .expect("git reports the main checkout's common directory"),
        );
        baude_core::suppression::suppress(
            &baude_core::persist::config_dir(),
            &main_path,
            &common_dir,
            baude_core::meta::now_unix_ms(),
        )
        .unwrap();
        assert!(reads_suppressed(&main_path));

        let (restarted, restarted_app) =
            restarted_daemon(&fixture.root, &fixture.workspace, REMOVAL_AGENT_CMD);
        let rows = rows_body(&restarted_app).await;
        let entries = checkout_entries(&rows);
        assert_eq!(entries.len(), before - 1, "{rows}");
        assert_eq!(
            checkout_entries_at(&rows, &p),
            Vec::<serde_json::Value>::new(),
            "{rows}"
        );
        let main_entries = checkout_entries_at(&rows, &main_path);
        assert_eq!(main_entries.len(), 1, "{rows}");
        assert!(
            main_entries[0]["session_id"].is_u64(),
            "the restart relaunched the main checkout: {rows}"
        );
        assert!(!p.exists(), "{}", p.display());
        assert!(reads_suppressed(&p), "{}", p.display());
        assert!(reads_suppressed(&main_path), "{}", main_path.display());

        crate::manager::lock(&fixture.shared).kill_all();
        removal_teardown(&restarted, &fixture.root);
    }

    /// D-11 write rule: a standalone row writes no entry of its own, so a
    /// removed folder is admitted again at once.
    #[tokio::test]
    async fn suppression_standalone_removal_writes_no_entry() {
        let (root, _owner, folder, _workspace, shared, app) = folder_fixture("suppress-standalone");
        opened_folder_id(&app, &folder).await;
        let key = standalone_key_of(&only_standalone_row(&app).await);

        let removed = removed_over_routes(&app, &serde_json::json!({ "standalone": key })).await;
        assert_eq!(
            cleared_paths(
                &removed,
                baude_core::removal::ClearingKind::SuppressionAdded
            ),
            Vec::<PathBuf>::new(),
            "{removed:?}"
        );
        assert!(folder.is_dir());
        assert!(!reads_suppressed(&folder));

        let id = opened_folder_id(&app, &folder).await;
        let entry = only_standalone_row(&app).await;
        assert_eq!(entry["session_id"].as_u64(), Some(id), "{entry}");

        removal_teardown(&shared, &root);
    }

    /// A folder an earlier daemon stored as an unavailable repository plus a
    /// checkout keeps that shape, takes a second session into the same
    /// repository row, and becomes a standalone row only after that
    /// repository row is removed.
    #[tokio::test]
    async fn standalone_earlier_folder_row_keeps_its_shape_and_is_removable() {
        let (root, _owner, folder, _workspace, shared, app) = folder_fixture("standalone-earlier");
        let folder_text = folder.to_str().unwrap();
        let first_id = crate::manager::lock(&shared)
            .create_pre_plan_folder_session_for_test(&folder, None)
            .unwrap_or_else(|error| panic!("the earlier shape is stored and launched: {error}"));

        // Served verbatim.
        let rows = rows_body(&app).await;
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");
        let parents = rows["repositories"].as_array().unwrap();
        assert_eq!(parents.len(), 1, "{rows}");
        let children = parents[0]["checkouts"].as_array().unwrap();
        assert_eq!(children.len(), 1, "{rows}");
        assert_eq!(children[0]["path"].as_str(), Some(folder_text), "{rows}");
        assert_eq!(children[0]["role"], "managed_branch", "{rows}");
        assert_eq!(children[0]["session_id"].as_u64(), Some(first_id), "{rows}");
        let repository_key = parents[0]["repository_key"].clone();

        // A second session joins the stored repository row.
        let second_id = opened_folder_id(&app, &folder).await;
        assert_ne!(second_id, first_id);
        let rows = rows_body(&app).await;
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");
        let parents = rows["repositories"].as_array().unwrap();
        assert_eq!(parents.len(), 1, "{rows}");
        assert_eq!(parents[0]["repository_key"], repository_key, "{rows}");
        assert_eq!(
            parents[0]["checkouts"].as_array().unwrap().len(),
            2,
            "{rows}"
        );

        // Removing the repository row frees the folder and keeps it.
        let target = serde_json::json!({ "repository": repository_key });
        let preview = preview_target(&app, &target).await;
        let agent_pids: Vec<u64> = preview["runtimes_killed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|killed| killed["agent_pid"].as_u64().unwrap())
            .collect();
        assert_eq!(agent_pids.len(), 2, "{preview}");
        for pid in &agent_pids {
            assert!(pid_is_alive(*pid), "{preview}");
        }
        let (status, body) = removal_call(&app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let executed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            executed["rows_removed"].as_array().unwrap().len(),
            3,
            "{executed}"
        );
        assert_eq!(
            executed["runtimes_killed"].as_array().unwrap().len(),
            2,
            "{executed}"
        );
        for pid in &agent_pids {
            assert!(wait_for_pid_gone(*pid), "{executed}");
        }
        assert_folder_untouched(&folder);
        let rows = rows_body(&app).await;
        assert_eq!(rows["repositories"], serde_json::json!([]), "{rows}");
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");

        // Only now is the path free for a standalone row.
        let third_id = opened_folder_id(&app, &folder).await;
        let entry = only_standalone_row(&app).await;
        assert_eq!(entry["session_id"].as_u64(), Some(third_id), "{entry}");
        assert_eq!(entry["path"].as_str(), Some(folder_text), "{entry}");

        removal_teardown(&shared, &root);
    }

    /// The folder a user is left with after deleting a repository but not
    /// its worktree: a `.git` file pointing at nothing. Git says it is not a
    /// repository, so it is one standalone row and no repository row.
    #[tokio::test]
    async fn standalone_stale_worktree_folder_is_admitted_without_a_repository() {
        let (root, _owner, _notes, _workspace, shared, app) = folder_fixture("standalone-stale");
        let canonical_root = root.canonicalize().unwrap();
        let repo = committed_repo(&canonical_root);
        let stale = canonical_root.join("stale");
        git(
            &repo,
            &["worktree", "add", "-b", "stale", stale.to_str().unwrap()],
        );
        assert!(stale.join(".git").is_file());
        assert_eq!(std::fs::read(stale.join("file")).unwrap(), b"one");
        std::fs::remove_dir_all(&repo).unwrap();
        assert!(!repo.exists());
        assert!(stale.join(".git").is_file());
        assert_eq!(std::fs::read(stale.join("file")).unwrap(), b"one");

        let probe = Command::new("git")
            .arg("-C")
            .arg(&stale)
            .args(["rev-parse", "--git-dir"])
            .output()
            .unwrap();
        let stderr = String::from_utf8_lossy(&probe.stderr);
        assert!(!probe.status.success(), "{stderr}");
        assert!(
            stderr.starts_with("fatal: not a git repository"),
            "git refused {} for another reason: {stderr}",
            stale.display()
        );

        let id = opened_folder_id(&app, &stale).await;
        let entry = only_standalone_row(&app).await;
        assert_eq!(entry["session_id"].as_u64(), Some(id), "{entry}");
        assert_eq!(entry["path"].as_str(), stale.to_str(), "{entry}");

        let removed = removed_over_routes(
            &app,
            &serde_json::json!({ "standalone": standalone_key_of(&entry) }),
        )
        .await;
        assert_eq!(removed.rows_removed.len(), 1, "{removed:?}");
        assert!(stale.is_dir());
        assert!(stale.join(".git").is_file());
        assert_eq!(std::fs::read(stale.join("file")).unwrap(), b"one");
        let rows = rows_body(&app).await;
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");
        assert_eq!(rows["repositories"], serde_json::json!([]), "{rows}");

        removal_teardown(&shared, &root);
    }

    /// API-01 concurrency edge. Both requests carry the same previewed report
    /// and contend on the manager mutex; whichever loses must find the row
    /// already gone. The fresh restore at the end is what witnesses that no
    /// partial state is readable: core leaves the state untouched on a
    /// failure and persists with one atomic save.
    #[tokio::test]
    async fn removals_concurrent_execute_of_the_same_row_commits_once() {
        let (root, _fixture, workspace, shared, app, id, key) = removal_fixture("concurrent").await;
        let report = preview_report(&app, key).await;
        let agent_pid = previewed_agent_pid(&report, key);
        assert!(pid_is_alive(agent_pid));
        let body = report.to_string();

        let (first, second) = tokio::join!(
            app.clone().oneshot(post_json("/removals", &body)),
            app.clone().oneshot(post_json("/removals", &body)),
        );
        let mut outcomes = Vec::new();
        for response in [first.unwrap(), second.unwrap()] {
            let status = response.status().as_u16();
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            outcomes.push((status, String::from_utf8_lossy(&bytes).into_owned()));
        }
        outcomes.sort();
        let statuses: Vec<u16> = outcomes.iter().map(|(status, _)| *status).collect();
        assert_eq!(
            statuses,
            [200, 404],
            "one removal must win and the other must find the row gone: {outcomes:?}"
        );

        // One kill: the process is gone, and only the winner reports it.
        assert!(
            wait_for_pid_gone(agent_pid),
            "the removed runtime's agent process is still alive"
        );
        let kills_reported = outcomes
            .iter()
            .filter(|(_, text)| {
                serde_json::from_str::<serde_json::Value>(text)
                    .ok()
                    .and_then(|value| value["runtimes_killed"].as_array().cloned())
                    .is_some_and(|killed| !killed.is_empty())
            })
            .count();
        assert_eq!(kills_reported, 1, "{outcomes:?}");
        let winner: serde_json::Value = serde_json::from_str(&outcomes[0].1).unwrap();
        assert_eq!(winner["planned"], false);
        assert_eq!(
            winner["persistence"],
            serde_json::json!({ "outcome": "saved" })
        );

        // One save: a second kill would have reported a survived runtime and
        // a second persist would have left an error behind.
        {
            let manager = crate::manager::lock(&shared);
            let status = manager.persistence_status();
            assert_eq!(status.error, None);
            assert!(!status.dirty);
            assert!(manager.info(id).is_none());
            assert_eq!(manager.checkout_key_for_runtime_for_test(id), None);
        }
        let mut restored = Manager::new(REMOVAL_AGENT_CMD.into(), true);
        assert_eq!(restored.restore_at(&root, &workspace), 0);
        assert!(matches!(
            restored.preview_removal(RemovalTarget::Checkout(key)),
            Err(RemovalError::UnknownTarget(_))
        ));

        removal_teardown(&shared, &root);
    }

    #[tokio::test]
    async fn removals_preview_leaves_the_state_file_untouched() {
        let (root, _fixture, workspace, shared, app, id, key) = removal_fixture("preview").await;
        let state_file = removal_state_file(&root, &workspace);
        let before = std::fs::read(&state_file).unwrap();
        assert!(!before.is_empty());

        let report = preview_report(&app, key).await;
        assert_eq!(report["planned"], true);

        assert_eq!(std::fs::read(&state_file).unwrap(), before);
        let listed = body_json(app.clone().oneshot(get("/sessions")).await.unwrap()).await;
        assert_eq!(listed.as_array().unwrap().len(), 1);
        assert_eq!(listed[0]["id"].as_u64(), Some(id));

        removal_teardown(&shared, &root);
    }

    #[tokio::test]
    async fn removals_unknown_target_is_404() {
        let _scope = api_scope("removals-unknown");
        let app = super::router(Arc::new(Mutex::new(Manager::new(
            REMOVAL_AGENT_CMD.into(),
            false,
        ))));
        let unknown_report = serde_json::json!({
            "target": { "checkout": 9999 },
            "planned": true,
            "rows_removed": [{ "checkout": 9999 }],
            "runtimes_killed": [],
            "paths_deleted": [],
            "paths_kept": [],
            "already_removed": [],
            "readers_parked": [],
            "persistence": { "outcome": "planned" },
            "cleared": [],
            "clearing_failures": [],
        });
        for (path, body) in [
            (
                "/removals/preview",
                serde_json::json!({ "checkout": 9999 }).to_string(),
            ),
            ("/removals", unknown_report.to_string()),
        ] {
            let response = app.clone().oneshot(post_json(path, &body)).await.unwrap();
            assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
            let content_type = response
                .headers()
                .get(header::CONTENT_TYPE)
                .map(|value| value.to_str().unwrap().to_string())
                .unwrap_or_default();
            assert!(!content_type.contains("json"), "{path}: {content_type}");
            let bytes = response.into_body().collect().await.unwrap().to_bytes();
            let text = String::from_utf8_lossy(&bytes);
            assert!(text.contains("9999"), "{path}: {text}");
        }
    }

    /// The field names the `GET /rows` wire contract pins for a repository
    /// parent entry, sorted.
    const ROWS_REPOSITORY_FIELDS: [&str; 5] = [
        "checkouts",
        "display_name",
        "health",
        "path",
        "repository_key",
    ];

    /// The field names the wire contract pins for a checkout child entry,
    /// sorted.
    const ROWS_CHECKOUT_FIELDS: [&str; 9] = [
        "archived",
        "branch",
        "checkout_key",
        "display_name",
        "managed_by_baude",
        "path",
        "repository_key",
        "role",
        "session_id",
    ];

    /// `GET /rows` through the router, as the raw response bytes.
    async fn rows_bytes(app: &Router) -> Vec<u8> {
        let response = app.clone().oneshot(get("/rows")).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec()
    }

    async fn rows_body(app: &Router) -> serde_json::Value {
        serde_json::from_slice(&rows_bytes(app).await).unwrap()
    }

    /// An entry carries exactly the pinned names: none missing, and none
    /// extra, so a conversation identifier cannot ride along unnoticed.
    fn assert_rows_fields(entry: &serde_json::Value, pinned: &[&str]) {
        let mut names: Vec<&str> = entry
            .as_object()
            .unwrap_or_else(|| panic!("a rows entry is a JSON object: {entry}"))
            .keys()
            .map(String::as_str)
            .collect();
        names.sort_unstable();
        assert_eq!(names, pinned, "{entry}");
    }

    /// The checkout entry for `key`, searched under every repository parent.
    fn rows_checkout(rows: &serde_json::Value, key: CheckoutKey) -> Option<serde_json::Value> {
        rows["repositories"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|parent| parent["checkouts"].as_array().unwrap().iter())
            .find(|entry| entry["checkout_key"] == key.get())
            .cloned()
    }

    #[tokio::test]
    async fn rows_projection_names_every_row_by_key() {
        let fixture = removal_fixture_two_checkouts("rows-names").await;
        let repo_path = fixture.repo.path().canonicalize().unwrap();

        let rows = rows_body(&fixture.app).await;
        // Shown on failure, and under --nocapture for a client plan that
        // wants a real body to copy.
        println!("GET /rows -> {rows:#}");
        assert_rows_fields(&rows, &["repositories", "standalone"]);

        let repositories = rows["repositories"].as_array().unwrap();
        assert_eq!(repositories.len(), 1, "{rows}");
        let parent = &repositories[0];
        assert_rows_fields(parent, &ROWS_REPOSITORY_FIELDS);
        assert!(parent["repository_key"].is_u64(), "{parent}");
        assert_eq!(
            parent["display_name"],
            repo_path.file_name().unwrap().to_str().unwrap()
        );
        assert_eq!(parent["path"], repo_path.to_str().unwrap());
        assert_eq!(
            parent["health"],
            serde_json::json!({ "status": "available" })
        );

        let checkouts = parent["checkouts"].as_array().unwrap();
        assert_eq!(checkouts.len(), 2, "{parent}");
        let sessions =
            body_json(fixture.app.clone().oneshot(get("/sessions")).await.unwrap()).await;
        let sessions = sessions.as_array().unwrap();
        for (id, key) in [
            (fixture.main_id, fixture.main_key),
            (fixture.worktree_id, fixture.worktree_key),
        ] {
            let entry = checkouts
                .iter()
                .find(|entry| entry["checkout_key"] == key.get())
                .unwrap_or_else(|| panic!("no checkout entry for {key:?}: {parent}"));
            assert_rows_fields(entry, &ROWS_CHECKOUT_FIELDS);
            assert_eq!(entry["repository_key"], parent["repository_key"]);

            // The D-09 Option B join, both ways. From the row to the session:
            // `session_id` names exactly one `/sessions` entry.
            assert_eq!(entry["session_id"], id, "{entry}");
            let session = sessions
                .iter()
                .find(|session| session["id"] == entry["session_id"])
                .unwrap_or_else(|| panic!("no /sessions entry for {entry}"));
            // And from the session back to the row: the key the daemon holds
            // for that session id is the key the entry carries.
            let registered = crate::manager::lock(&fixture.shared)
                .checkout_key_for_runtime_for_test(session["id"].as_u64().unwrap())
                .expect("a live session is registered under a checkout key");
            assert_eq!(entry["checkout_key"], registered.get(), "{entry}");

            assert_eq!(entry["display_name"], session["name"], "{entry}");
            assert!(
                !entry["display_name"].as_str().unwrap().is_empty(),
                "{entry}"
            );
            assert_eq!(entry["path"], session["cwd"], "{entry}");
            assert_eq!(entry["archived"], false, "{entry}");
            assert!(entry["managed_by_baude"].is_boolean(), "{entry}");
            // Branch and role are served verbatim. Their correctness is
            // 21-09's, so only presence and type are asserted here.
            assert!(
                entry["branch"].is_null() || entry["branch"].is_string(),
                "branch is a string or null: {entry}"
            );
            assert!(entry["role"].is_string(), "role is a string: {entry}");
        }
        let main = rows_checkout(&rows, fixture.main_key).unwrap();
        assert_eq!(main["path"], repo_path.to_str().unwrap());
        let worktree = rows_checkout(&rows, fixture.worktree_key).unwrap();
        assert_eq!(worktree["path"], fixture.worktree_path.to_str().unwrap());

        // No standalone session exists on this fixture. 21-10 adds the daemon
        // standalone registry and extends this with the five standalone names:
        // standalone_key, display_name, path, archived, session_id.
        assert_eq!(rows["standalone"], serde_json::json!([]), "{rows}");

        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// HIGH #6: a parent's children resolve to a key and a name with no live
    /// session, which is what lets a client render a parent removal report or
    /// a 409 drift body that names a child it holds no session for.
    #[tokio::test]
    async fn rows_parent_lists_children_with_no_live_session() {
        let fixture = removal_fixture_two_checkouts("rows-no-live").await;
        let before = rows_checkout(&rows_body(&fixture.app).await, fixture.worktree_key).unwrap();
        assert_eq!(before["session_id"], fixture.worktree_id);

        let deleted = fixture
            .app
            .clone()
            .oneshot(
                Request::delete(format!("/sessions/{}", fixture.worktree_id))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(deleted.status(), StatusCode::NO_CONTENT);

        let rows = rows_body(&fixture.app).await;
        let parent = &rows["repositories"][0];
        assert_eq!(parent["checkouts"].as_array().unwrap().len(), 2, "{rows}");
        let child = rows_checkout(&rows, fixture.worktree_key)
            .unwrap_or_else(|| panic!("the durable child row is gone: {rows}"));
        assert_rows_fields(&child, &ROWS_CHECKOUT_FIELDS);
        assert!(child["session_id"].is_null(), "{child}");
        assert!(!child["display_name"].as_str().unwrap().is_empty());
        assert_eq!(child["display_name"], before["display_name"], "{child}");
        assert_eq!(child["branch"], before["branch"], "{child}");
        assert_eq!(child["path"], before["path"], "{child}");

        let main = rows_checkout(&rows, fixture.main_key).unwrap();
        assert_eq!(main["session_id"], fixture.main_id, "{main}");

        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// API-03 idempotency: the projection is a pure read. Two successive
    /// fetches are byte-identical and leave the state file byte-identical.
    #[tokio::test]
    async fn rows_projection_is_a_pure_idempotent_read() {
        let fixture = removal_fixture_two_checkouts("rows-pure").await;
        let state_file = removal_state_file(&fixture.root, &fixture.workspace);
        let state_before = std::fs::read(&state_file).unwrap();
        assert!(!state_before.is_empty());

        let first = rows_bytes(&fixture.app).await;
        let second = rows_bytes(&fixture.app).await;
        assert_eq!(
            first,
            second,
            "two reads differ:\n{}\n{}",
            String::from_utf8_lossy(&first),
            String::from_utf8_lossy(&second)
        );
        assert_eq!(std::fs::read(&state_file).unwrap(), state_before);

        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// A row the user archived must read as archived on the index, and its
    /// sibling must not, so a client can tell the two apart.
    #[tokio::test]
    async fn rows_reports_an_archived_row_as_archived() {
        let fixture = removal_fixture_two_checkouts("rows-archived").await;
        let archived = fixture
            .app
            .clone()
            .oneshot(post_json(
                &format!("/sessions/{}/archive", fixture.worktree_id),
                "{}",
            ))
            .await
            .unwrap();
        assert!(archived.status().is_success(), "{}", archived.status());

        let rows = rows_body(&fixture.app).await;
        let child = rows_checkout(&rows, fixture.worktree_key).unwrap();
        assert_eq!(child["archived"], true, "{child}");
        let main = rows_checkout(&rows, fixture.main_key).unwrap();
        assert_eq!(main["archived"], false, "{main}");

        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// D-09 Option B: a removal report names rows by key only. Holding
    /// nothing but the key a report carries, a client resolves a label
    /// through `GET /rows` alone, with no `/sessions` read and no join.
    #[tokio::test]
    async fn rows_resolves_a_label_for_a_key_with_no_sessions_field() {
        let fixture = removal_fixture_two_checkouts("rows-label").await;
        let report = preview_report(&fixture.app, fixture.worktree_key).await;
        let removed = report["rows_removed"].as_array().unwrap();
        assert_eq!(removed.len(), 1, "{report}");
        let key = &removed[0]["checkout"];
        assert!(key.is_u64(), "a report names a row by a bare key: {report}");

        let rows = rows_body(&fixture.app).await;
        let entry = rows["repositories"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|parent| parent["checkouts"].as_array().unwrap().iter())
            .find(|entry| &entry["checkout_key"] == key)
            .unwrap_or_else(|| panic!("key {key} from {report} has no entry in {rows}"));
        assert!(
            !entry["display_name"].as_str().unwrap().is_empty(),
            "{entry}"
        );
        assert_eq!(
            entry["path"],
            fixture.worktree_path.to_str().unwrap(),
            "{entry}"
        );

        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// API-03 concurrency: a read racing a removal sees the projection from
    /// before the removal or from after it, never a half-applied one, because
    /// both handlers take the one manager mutex. Both candidates are observed
    /// bodies. No sleep forces an ordering: the mutex is what is under test.
    ///
    /// The two requests are joined on the test thread, since the fixture's
    /// containment redirect is thread-local and a worker thread would escape
    /// it. Each join order runs once, so neither is the only one exercised.
    #[tokio::test]
    async fn rows_concurrent_with_a_removal_is_never_half_applied() {
        for read_first in [true, false] {
            let label = if read_first {
                "rows-race-read"
            } else {
                "rows-race-removal"
            };
            let (root, _fixture, _workspace, shared, app, _id, key) = removal_fixture(label).await;
            let report = preview_report(&app, key).await;
            let before = rows_body(&app).await;
            assert!(rows_checkout(&before, key).is_some(), "{before}");

            let read = app.clone().oneshot(get("/rows"));
            let removal = app
                .clone()
                .oneshot(post_json("/removals", &report.to_string()));
            let (read, removal) = if read_first {
                tokio::join!(read, removal)
            } else {
                let (removal, read) = tokio::join!(removal, read);
                (read, removal)
            };
            let read = read.unwrap();
            let removal = removal.unwrap();
            assert_eq!(removal.status(), StatusCode::OK);
            assert_eq!(read.status(), StatusCode::OK);
            let raced = body_json(read).await;

            let after = rows_body(&app).await;
            assert!(rows_checkout(&after, key).is_none(), "{after}");
            assert_ne!(before, after);
            println!(
                "{label}: the raced read saw the {} projection\nbefore: {before}\nafter: {after}",
                if raced == before {
                    "pre-removal"
                } else {
                    "post-removal"
                }
            );
            assert!(
                raced == before || raced == after,
                "{label}: a read racing a removal saw a third shape\nraced: {raced}\nbefore: {before}\nafter: {after}"
            );

            removal_teardown(&shared, &root);
        }
    }

    /// Trimmed stdout of a git read in `dir`, or `None` when git exits
    /// non-zero. Reads go through this and not `git`, which asserts success,
    /// because a non-zero exit is an expected answer for a detached HEAD.
    fn git_read(dir: &Path, args: &[&str]) -> Option<String> {
        let out = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        out.status
            .success()
            .then(|| String::from_utf8(out.stdout).unwrap().trim().to_string())
    }

    /// The `/rows` checkout entry whose `session_id` is `id`. D-09 Option B:
    /// a client joins `/rows` to a created session on `session_id`.
    fn rows_checkout_for_session(rows: &serde_json::Value, id: u64) -> serde_json::Value {
        rows["repositories"]
            .as_array()
            .unwrap()
            .iter()
            .flat_map(|parent| parent["checkouts"].as_array().unwrap().iter())
            .find(|entry| entry["session_id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("no /rows checkout carries session_id {id}: {rows}"))
    }

    /// `POST` a JSON body to a removal route, returning the status and the
    /// raw body, which is JSON on 200 and 409 and plain text otherwise.
    async fn removal_call(
        app: &Router,
        path: &str,
        body: &serde_json::Value,
    ) -> (StatusCode, String) {
        let res = app
            .clone()
            .oneshot(post_json(path, &body.to_string()))
            .await
            .unwrap();
        let status = res.status();
        let bytes = res.into_body().collect().await.unwrap().to_bytes();
        (status, String::from_utf8_lossy(&bytes).into_owned())
    }

    /// Preview a removal target, asserting 200, and return the report.
    async fn preview_target(app: &Router, target: &serde_json::Value) -> serde_json::Value {
        let (status, body) = removal_call(app, "/removals/preview", target).await;
        assert_eq!(status, StatusCode::OK, "preview {target}: {body}");
        serde_json::from_str(&body).unwrap()
    }

    /// The paths a report lists under `field`, as strings.
    fn report_paths(report: &serde_json::Value, field: &str) -> Vec<String> {
        report[field]
            .as_array()
            .unwrap()
            .iter()
            .map(|path| path.as_str().unwrap().to_string())
            .collect()
    }

    /// `rows_removed` without its last element, sorted by JSON text so two
    /// reports compare as sets.
    fn child_rows(report: &serde_json::Value) -> Vec<String> {
        let rows = report["rows_removed"].as_array().unwrap();
        let mut children: Vec<String> = rows[..rows.len() - 1]
            .iter()
            .map(|row| row.to_string())
            .collect();
        children.sort();
        children
    }

    #[tokio::test]
    async fn row_kinds_repository_parent_drops_three_rows_in_one_save() {
        let fixture = removal_fixture_two_checkouts("kinds-parent").await;
        let app = &fixture.app;
        let main_path = fixture.repo.path().canonicalize().unwrap();
        let worktree_path = fixture.worktree_path.canonicalize().unwrap();

        // Role pin, first, so it is what goes red.
        let rows = rows_body(app).await;
        let main_row = rows_checkout_for_session(&rows, fixture.main_id);
        let worktree_row = rows_checkout_for_session(&rows, fixture.worktree_id);
        assert_eq!(main_row["path"], main_path.to_str().unwrap());
        assert_eq!(
            main_row["role"], "main",
            "21-09 role pin: the checkout at the repository path was served as {}",
            main_row["role"]
        );
        assert_eq!(main_row["managed_by_baude"], false);
        assert_eq!(worktree_row["role"], "managed_branch", "{worktree_row}");
        assert_eq!(worktree_row["managed_by_baude"], true);

        let worktree_branch = git_read(&worktree_path, &["symbolic-ref", "--short", "-q", "HEAD"])
            .expect("the managed worktree sits on a branch");
        let repository_key = main_row["repository_key"].clone();
        let main_key = main_row["checkout_key"].clone();
        let worktree_key = worktree_row["checkout_key"].clone();
        let target = serde_json::json!({ "repository": repository_key });
        let mut expected_children = vec![
            serde_json::json!({ "checkout": main_key }).to_string(),
            serde_json::json!({ "checkout": worktree_key }).to_string(),
        ];
        expected_children.sort();

        // Preview.
        let preview = preview_target(app, &target).await;
        assert_eq!(
            preview["rows_removed"].as_array().unwrap().last().unwrap(),
            &target,
            "{preview}"
        );
        assert_eq!(child_rows(&preview), expected_children, "{preview}");
        let deleted = report_paths(&preview, "paths_deleted");
        assert_eq!(deleted.len(), 1, "{preview}");
        assert_eq!(
            Path::new(&deleted[0]).canonicalize().unwrap(),
            worktree_path
        );
        assert!(
            report_paths(&preview, "paths_kept")
                .iter()
                .any(|kept| Path::new(kept).canonicalize().unwrap() == main_path),
            "{preview}"
        );
        assert_eq!(
            preview["persistence"],
            serde_json::json!({ "outcome": "planned" })
        );

        let state_file = removal_state_file(&fixture.root, &fixture.workspace);
        let before = std::fs::read(&state_file).unwrap();

        // Execute, echoing the previewed report verbatim.
        let (status, body) = removal_call(app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let executed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(executed["rows_removed"], preview["rows_removed"]);
        assert_eq!(executed["paths_deleted"], preview["paths_deleted"]);
        assert_eq!(
            executed["persistence"],
            serde_json::json!({ "outcome": "saved" })
        );
        let killed = executed["runtimes_killed"].as_array().unwrap();
        assert_eq!(killed.len(), 2, "{executed}");
        let mut killed_rows: Vec<String> = killed.iter().map(|k| k["row"].to_string()).collect();
        killed_rows.sort();
        assert_eq!(killed_rows, expected_children);

        // The world afterwards.
        let after_rows = rows_body(app).await;
        assert!(
            after_rows["repositories"]
                .as_array()
                .unwrap()
                .iter()
                .all(|parent| parent["repository_key"] != repository_key),
            "{after_rows}"
        );
        let sessions = app
            .clone()
            .oneshot(Request::get("/sessions").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let sessions = body_json(sessions).await;
        for id in [fixture.main_id, fixture.worktree_id] {
            assert!(
                sessions
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|session| session["id"] != id),
                "{sessions}"
            );
        }
        for entry in killed {
            let pid = entry["agent_pid"].as_u64().unwrap();
            assert!(wait_for_pid_gone(pid), "agent {pid} outlived the removal");
        }
        assert!(!worktree_path.exists(), "{}", worktree_path.display());
        assert!(main_path.is_dir());
        assert!(git_read(&main_path, &["rev-parse", "--git-dir"]).is_some());
        let branch_ref = format!("refs/heads/{worktree_branch}");
        assert!(
            git_read(
                &main_path,
                &["show-ref", "--verify", "--quiet", &branch_ref]
            )
            .is_some(),
            "the local branch {branch_ref} must survive a repository removal"
        );

        // One save, and it survives a fresh restore.
        assert_ne!(std::fs::read(&state_file).unwrap(), before);
        let mut restored = Manager::new(REMOVAL_AGENT_CMD.into(), true);
        assert_eq!(restored.restore_at(&fixture.root, &fixture.workspace), 0);
        assert_eq!(restored.list().len(), 0);
        for key in [fixture.main_key, fixture.worktree_key] {
            assert!(matches!(
                restored.preview_removal(RemovalTarget::Checkout(key)),
                Err(RemovalError::UnknownTarget(_))
            ));
        }
        assert!(restored.rows().repositories.is_empty());

        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// A real `git clone`, not a `git init`: an upstream built by
    /// `initialized_repo`, cloned once into a sibling directory under the
    /// same isolated root. Returns the upstream's owner, which holds the
    /// root redirect and identity scope for the whole test, and the
    /// canonicalized clone path. Sessions point at the clone, never at the
    /// upstream.
    #[must_use = "the returned FixtureRepo owns the redirect that contains this fixture; bind it \
                  to a named local that outlives every handler await"]
    fn cloned_repo(root: &Path) -> (FixtureRepo, PathBuf) {
        let upstream = initialized_repo(root, "upstream");
        let clone = root.join("clone");
        let out = Command::new("git")
            .arg("clone")
            .arg(upstream.path())
            .arg(&clone)
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git clone exited {}: {}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        (upstream, clone.canonicalize().unwrap())
    }

    #[tokio::test]
    async fn row_kinds_branch_is_the_short_name_git_reports() {
        let root = removal_root("kinds-branch");
        let (_upstream, clone) = cloned_repo(&root);
        git(&clone, &["switch", "-c", "chore/workspace"]);
        let (_workspace, shared, app) = removal_daemon(&root);
        let (clone_id, clone_key, _) = admit_removal_session(&shared, &app, &clone, None).await;
        let (worktree_id, worktree_key, _) =
            admit_removal_session(&shared, &app, &clone, Some("topic")).await;

        let rows = rows_body(&app).await;
        let clone_row = rows_checkout_for_session(&rows, clone_id);
        let worktree_row = rows_checkout_for_session(&rows, worktree_id);
        assert_eq!(clone_row["path"], clone.to_str().unwrap());

        // What git itself reports in each row's directory.
        let reported = |row: &serde_json::Value| {
            let path = PathBuf::from(row["path"].as_str().unwrap());
            let short = git_read(&path, &["symbolic-ref", "--short", "-q", "HEAD"])
                .expect("the row's directory sits on a branch");
            let full = git_read(&path, &["symbolic-ref", "-q", "HEAD"])
                .expect("the row's directory sits on a branch");
            (short, full)
        };
        let (clone_short, clone_full) = reported(&clone_row);
        let (worktree_short, worktree_full) = reported(&worktree_row);
        // Preconditions on the fixture, so the pin cannot pass vacuously.
        assert!(clone_short.contains('/'), "{clone_short}");
        assert_ne!(clone_short, worktree_short);

        for (row, short) in [(&clone_row, &clone_short), (&worktree_row, &worktree_short)] {
            assert_eq!(
                row["branch"],
                short.as_str(),
                "21-09 branch pin: /rows served {} where git reports {short}",
                row["branch"]
            );
        }

        // The label follows git's main worktree, not a branch name.
        assert_eq!(clone_row["role"], "main", "{clone_row}");
        assert_eq!(worktree_row["role"], "managed_branch", "{worktree_row}");

        // The stored value is still the full ref.
        for (key, full) in [(clone_key, &clone_full), (worktree_key, &worktree_full)] {
            assert!(full.starts_with("refs/heads/"), "{full}");
            assert_eq!(
                crate::manager::lock(&shared).stored_observed_branch_for_test(key),
                Some(full.clone())
            );
        }

        removal_teardown(&shared, &root);
    }

    /// The ids `GET /sessions` lists, sorted.
    async fn session_ids(app: &Router) -> Vec<u64> {
        let sessions = body_json(app.clone().oneshot(get("/sessions")).await.unwrap()).await;
        let mut ids: Vec<u64> = sessions
            .as_array()
            .unwrap()
            .iter()
            .map(|session| session["id"].as_u64().unwrap())
            .collect();
        ids.sort_unstable();
        ids
    }

    #[tokio::test]
    async fn row_kinds_detached_head_reads_a_null_branch() {
        let root = removal_root("kinds-detached");
        let (_upstream, clone) = cloned_repo(&root);
        git(&clone, &["checkout", "--detach"]);
        // Precondition on the fixture: git reports no branch here.
        assert_eq!(git_read(&clone, &["symbolic-ref", "-q", "HEAD"]), None);
        let (_workspace, shared, app) = removal_daemon(&root);
        let (id, key, _) = admit_removal_session(&shared, &app, &clone, None).await;

        let rows = rows_body(&app).await;
        let row = rows_checkout_for_session(&rows, id);
        assert_eq!(row["path"], clone.to_str().unwrap());
        assert_eq!(
            row.as_object().unwrap().get("branch"),
            Some(&serde_json::Value::Null),
            "{row}"
        );
        assert_eq!(row["role"], "main", "{row}");
        assert_eq!(
            crate::manager::lock(&shared).stored_observed_branch_for_test(key),
            None
        );

        removal_teardown(&shared, &root);
    }

    #[tokio::test]
    async fn row_kinds_second_plain_session_on_a_main_checkout_is_admitted() {
        let (root, fixture, _workspace, shared, app) = removal_setup("kinds-second");
        push_to_bare_origin(&root, fixture.path());
        let repo = fixture.path().canonicalize().unwrap();
        let (first_id, _, _) = admit_removal_session(&shared, &app, &repo, None).await;
        let (second_id, _, _) = admit_removal_session(&shared, &app, &repo, None).await;
        assert_ne!(first_id, second_id);

        let rows = rows_body(&app).await;
        let repositories = rows["repositories"].as_array().unwrap();
        assert_eq!(repositories.len(), 1, "{rows}");
        let checkouts = repositories[0]["checkouts"].as_array().unwrap();
        assert_eq!(checkouts.len(), 2, "{rows}");
        for entry in checkouts {
            assert_eq!(entry["path"], repo.to_str().unwrap(), "{entry}");
        }
        let (mains, others): (Vec<_>, Vec<_>) =
            checkouts.iter().partition(|entry| entry["role"] == "main");
        assert_eq!(mains.len(), 1, "{rows}");
        assert_eq!(others.len(), 1, "{rows}");
        let other = others[0];
        assert_eq!(other["role"], "managed_branch", "{other}");
        assert_eq!(other["managed_by_baude"], false, "{other}");

        // The save path still validates with both rows present.
        admit_removal_session(&shared, &app, &repo, Some("topic")).await;

        let preview = preview_target(
            &app,
            &serde_json::json!({ "checkout": other["checkout_key"] }),
        )
        .await;
        assert!(
            report_paths(&preview, "paths_deleted").is_empty(),
            "{preview}"
        );
        assert!(
            report_paths(&preview, "paths_kept")
                .iter()
                .any(|kept| kept == other["path"].as_str().unwrap()),
            "{preview}"
        );

        removal_teardown(&shared, &root);
    }

    #[tokio::test]
    async fn row_kinds_main_checkout_removal_keeps_the_directory() {
        let fixture = removal_fixture_two_checkouts("kinds-main").await;
        let app = &fixture.app;
        let main_path = fixture.repo.path().canonicalize().unwrap();
        let rows = rows_body(app).await;
        let main_row = rows_checkout_for_session(&rows, fixture.main_id);
        let repository_key = main_row["repository_key"].clone();
        let target = serde_json::json!({ "checkout": main_row["checkout_key"] });

        let preview = preview_target(app, &target).await;
        assert_eq!(
            preview["rows_removed"],
            serde_json::json!([target]),
            "{preview}"
        );
        assert!(
            report_paths(&preview, "paths_deleted").is_empty(),
            "{preview}"
        );
        let kept = report_paths(&preview, "paths_kept");
        assert_eq!(kept.len(), 1, "{preview}");
        assert_eq!(Path::new(&kept[0]).canonicalize().unwrap(), main_path);

        let (status, body) = removal_call(app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let executed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            executed["runtimes_killed"].as_array().unwrap().len(),
            1,
            "{executed}"
        );

        assert!(main_path.is_dir());
        assert!(git_read(&main_path, &["rev-parse", "--git-dir"]).is_some());
        let after = rows_body(app).await;
        let repositories = after["repositories"].as_array().unwrap();
        assert_eq!(repositories.len(), 1, "{after}");
        assert_eq!(repositories[0]["repository_key"], repository_key);
        let checkouts = repositories[0]["checkouts"].as_array().unwrap();
        assert_eq!(checkouts.len(), 1, "{after}");
        assert_eq!(checkouts[0]["checkout_key"], fixture.worktree_key.get());
        assert_eq!(checkouts[0]["session_id"], fixture.worktree_id);
        assert_eq!(session_ids(app).await, [fixture.worktree_id]);

        removal_teardown(&fixture.shared, &fixture.root);
    }

    #[tokio::test]
    async fn row_kinds_parent_removal_leaves_a_sibling_repository_untouched() {
        let (root, fixture, _workspace, shared, app) = removal_setup("kinds-sibling");
        let repo = fixture.path().canonicalize().unwrap();
        // A second repository whose directory name extends the first's,
        // built while the first owner's redirect is alive.
        let sibling = root.join("repo-sibling");
        std::fs::create_dir_all(&sibling).unwrap();
        git(&sibling, &["init", "-b", "main"]);
        git(&sibling, &["config", "user.email", "test@example.com"]);
        git(&sibling, &["config", "user.name", "Test"]);
        std::fs::write(sibling.join("file"), b"one").unwrap();
        git(&sibling, &["add", "file"]);
        git(&sibling, &["commit", "-m", "initial"]);
        let sibling = sibling.canonicalize().unwrap();
        assert!(sibling
            .to_str()
            .unwrap()
            .starts_with(repo.to_str().unwrap()));

        let (repo_id, _, _) = admit_removal_session(&shared, &app, &repo, None).await;
        let (sibling_id, _, _) = admit_removal_session(&shared, &app, &sibling, None).await;
        let rows = rows_body(&app).await;
        assert_eq!(rows["repositories"].as_array().unwrap().len(), 2, "{rows}");
        let repo_row = rows_checkout_for_session(&rows, repo_id);
        let sibling_row = rows_checkout_for_session(&rows, sibling_id);
        assert_ne!(repo_row["repository_key"], sibling_row["repository_key"]);

        let target = serde_json::json!({ "repository": repo_row["repository_key"] });
        let preview = preview_target(&app, &target).await;
        let (status, body) = removal_call(&app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let executed: serde_json::Value = serde_json::from_str(&body).unwrap();
        let removed = executed["rows_removed"].as_array().unwrap();
        for sibling_key in [
            serde_json::json!({ "repository": sibling_row["repository_key"] }),
            serde_json::json!({ "checkout": sibling_row["checkout_key"] }),
        ] {
            assert!(!removed.contains(&sibling_key), "{executed}");
        }
        assert_eq!(report_paths(&executed, "paths_deleted").len(), 0);

        let after = rows_body(&app).await;
        let repositories = after["repositories"].as_array().unwrap();
        assert_eq!(repositories.len(), 1, "{after}");
        assert_eq!(repositories[0]["path"], sibling.to_str().unwrap());
        assert_eq!(
            repositories[0]["checkouts"],
            serde_json::json!([sibling_row]),
            "{after}"
        );
        assert_eq!(session_ids(&app).await, [sibling_id]);
        assert!(sibling.is_dir());
        assert!(git_read(&sibling, &["rev-parse", "--git-dir"]).is_some());

        removal_teardown(&shared, &root);
    }

    /// Whether `/rows` still carries a repository parent with `key`.
    fn rows_has_repository(rows: &serde_json::Value, key: &serde_json::Value) -> bool {
        rows["repositories"]
            .as_array()
            .unwrap()
            .iter()
            .any(|parent| &parent["repository_key"] == key)
    }

    #[tokio::test]
    async fn row_kinds_parent_removal_racing_a_child_removal_commits_one() {
        for parent_first in [true, false] {
            let label = if parent_first {
                "kinds-race-parent"
            } else {
                "kinds-race-child"
            };
            let fixture = removal_fixture_two_checkouts(label).await;
            let app = &fixture.app;
            let main_path = fixture.repo.path().canonicalize().unwrap();
            let worktree_path = fixture.worktree_path.canonicalize().unwrap();
            let rows = rows_body(app).await;
            let main_row = rows_checkout_for_session(&rows, fixture.main_id);
            let repository_key = main_row["repository_key"].clone();

            // Both previews are taken before either execute is issued.
            let parent_report =
                preview_target(app, &serde_json::json!({ "repository": repository_key })).await;
            let child_report = preview_report(app, fixture.worktree_key).await;

            let parent = removal_call(app, "/removals", &parent_report);
            let child = removal_call(app, "/removals", &child_report);
            let ((parent_status, parent_body), (child_status, child_body)) = if parent_first {
                tokio::join!(parent, child)
            } else {
                let (child, parent) = tokio::join!(child, parent);
                (parent, child)
            };
            let outcome = format!(
                "{label}: exactly one of a repository removal and a removal of its own child \
                 must commit; repository removal answered {parent_status} ({parent_body}), \
                 child removal answered {child_status} ({child_body})"
            );
            let parent_won = parent_status == StatusCode::OK;
            let child_won = child_status == StatusCode::OK;
            assert!(parent_won != child_won, "{outcome}");
            // The loser's status names why: its target is gone, or its
            // compared sets changed under it.
            if parent_won {
                assert_eq!(child_status, StatusCode::NOT_FOUND, "{outcome}");
            } else {
                assert_eq!(parent_status, StatusCode::CONFLICT, "{outcome}");
            }

            let after = rows_body(app).await;
            if parent_won {
                assert!(
                    !rows_has_repository(&after, &repository_key),
                    "{outcome}\n{after}"
                );
                assert_eq!(session_ids(app).await, [0u64; 0], "{outcome}");
            } else {
                let repositories = after["repositories"].as_array().unwrap();
                assert_eq!(repositories.len(), 1, "{outcome}\n{after}");
                assert_eq!(repositories[0]["repository_key"], repository_key);
                assert_eq!(
                    repositories[0]["checkouts"],
                    serde_json::json!([main_row]),
                    "{outcome}\n{after}"
                );
                assert_eq!(session_ids(app).await, [fixture.main_id], "{outcome}");
            }
            println!(
                "{label}: the {} removal won; repository {parent_status}, child {child_status}",
                if parent_won { "repository" } else { "child" }
            );
            assert!(main_path.is_dir(), "{outcome}");
            assert!(git_read(&main_path, &["rev-parse", "--git-dir"]).is_some());
            assert!(!worktree_path.exists(), "{outcome}");

            removal_teardown(&fixture.shared, &fixture.root);
        }
    }

    #[tokio::test]
    async fn row_kinds_rows_read_racing_a_parent_removal_is_all_or_nothing() {
        for read_first in [true, false] {
            let label = if read_first {
                "kinds-rows-race-read"
            } else {
                "kinds-rows-race-removal"
            };
            let fixture = removal_fixture_two_checkouts(label).await;
            let app = &fixture.app;
            let before = rows_bytes(app).await;
            let before_json: serde_json::Value = serde_json::from_slice(&before).unwrap();
            let repository_key =
                rows_checkout_for_session(&before_json, fixture.main_id)["repository_key"].clone();
            let report =
                preview_target(app, &serde_json::json!({ "repository": repository_key })).await;

            let read = rows_bytes(app);
            let removal = removal_call(app, "/removals", &report);
            let (raced, (status, body)) = if read_first {
                tokio::join!(read, removal)
            } else {
                let (removal, read) = tokio::join!(removal, read);
                (read, removal)
            };
            assert_eq!(status, StatusCode::OK, "{body}");

            let after = rows_bytes(app).await;
            assert_ne!(before, after);
            let after_json: serde_json::Value = serde_json::from_slice(&after).unwrap();
            assert!(!rows_has_repository(&after_json, &repository_key));
            println!(
                "{label}: the raced read saw the {} projection",
                if raced == before {
                    "pre-removal"
                } else {
                    "post-removal"
                }
            );
            assert!(
                raced == before || raced == after,
                "{label}: a read racing a repository removal saw a third shape\nraced: {}\nbefore: {}\nafter: {}",
                String::from_utf8_lossy(&raced),
                String::from_utf8_lossy(&before),
                String::from_utf8_lossy(&after)
            );

            removal_teardown(&fixture.shared, &fixture.root);
        }
    }

    #[tokio::test]
    async fn row_kinds_parent_removal_converges_after_the_worktree_was_removed_by_hand() {
        let fixture = removal_fixture_two_checkouts("kinds-by-hand").await;
        let app = &fixture.app;
        let main_path = fixture.repo.path().canonicalize().unwrap();
        let rows = rows_body(app).await;
        let main_row = rows_checkout_for_session(&rows, fixture.main_id);
        let worktree_row = rows_checkout_for_session(&rows, fixture.worktree_id);
        let stored_worktree_path = worktree_row["path"].as_str().unwrap().to_string();
        assert_eq!(
            Path::new(&stored_worktree_path).canonicalize().unwrap(),
            fixture.worktree_path.canonicalize().unwrap()
        );
        let worktree_branch = git_read(
            &fixture.worktree_path,
            &["symbolic-ref", "--short", "-q", "HEAD"],
        )
        .expect("the managed worktree sits on a branch");

        // What an interruption between core's disk stage and its save
        // leaves behind, and what a user does in a terminal.
        git(
            &main_path,
            &[
                "worktree",
                "remove",
                "--force",
                fixture.worktree_path.to_str().unwrap(),
            ],
        );
        assert!(!fixture.worktree_path.exists());

        let repository_key = main_row["repository_key"].clone();
        let target = serde_json::json!({ "repository": repository_key });
        let preview = preview_target(app, &target).await;
        assert_eq!(
            report_paths(&preview, "already_removed"),
            [stored_worktree_path],
            "{preview}"
        );
        assert!(
            report_paths(&preview, "paths_deleted").is_empty(),
            "{preview}"
        );
        let mut expected_children = vec![
            serde_json::json!({ "checkout": main_row["checkout_key"] }).to_string(),
            serde_json::json!({ "checkout": worktree_row["checkout_key"] }).to_string(),
        ];
        expected_children.sort();
        assert_eq!(child_rows(&preview), expected_children, "{preview}");
        assert_eq!(
            preview["rows_removed"].as_array().unwrap().last().unwrap(),
            &target,
            "{preview}"
        );

        let (status, body) = removal_call(app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        let executed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(
            executed["persistence"],
            serde_json::json!({ "outcome": "saved" })
        );

        let after = rows_body(app).await;
        assert!(!rows_has_repository(&after, &repository_key), "{after}");
        assert!(git_read(&main_path, &["rev-parse", "--git-dir"]).is_some());
        let branch_ref = format!("refs/heads/{worktree_branch}");
        assert!(
            git_read(
                &main_path,
                &["show-ref", "--verify", "--quiet", &branch_ref]
            )
            .is_some(),
            "the local branch {branch_ref} must survive"
        );

        removal_teardown(&fixture.shared, &fixture.root);
    }

    #[tokio::test]
    async fn row_kinds_legacy_mis_roled_main_row_keeps_its_directory() {
        let fixture = removal_fixture_two_checkouts("kinds-legacy").await;
        let app = &fixture.app;
        let main_path = fixture.repo.path().canonicalize().unwrap();
        let worktree_path = fixture.worktree_path.canonicalize().unwrap();
        // The stored shape every daemon before 21-09 wrote for a main
        // checkout: `managed_branch`, with the managed flag already false.
        crate::manager::lock(&fixture.shared).set_checkout_role_for_test(
            fixture.main_key,
            baude_core::repository::CheckoutRole::ManagedBranch,
        );

        let rows = rows_body(app).await;
        let main_row = rows_checkout_for_session(&rows, fixture.main_id);
        assert_eq!(main_row["path"], main_path.to_str().unwrap());
        // Served verbatim: the projection does not relabel a stored row.
        assert_eq!(main_row["role"], "managed_branch", "{main_row}");
        assert_eq!(main_row["managed_by_baude"], false, "{main_row}");

        let target = serde_json::json!({ "repository": main_row["repository_key"] });
        let preview = preview_target(app, &target).await;
        let deleted = report_paths(&preview, "paths_deleted");
        assert_eq!(deleted.len(), 1, "{preview}");
        assert_eq!(
            Path::new(&deleted[0]).canonicalize().unwrap(),
            worktree_path
        );
        assert!(
            report_paths(&preview, "paths_kept")
                .iter()
                .any(|kept| Path::new(kept).canonicalize().unwrap() == main_path),
            "{preview}"
        );

        let (status, body) = removal_call(app, "/removals", &preview).await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert!(main_path.is_dir());
        assert!(git_read(&main_path, &["rev-parse", "--git-dir"]).is_some());
        assert!(!worktree_path.exists());

        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// Collect a refused response. The status is pinned with `marker`, and
    /// the message carries the status and body actually returned. A refusal
    /// other than 409 is plain text, so the `content-type` must not be JSON.
    /// Returns the body text.
    async fn assert_refused(
        response: axum::response::Response,
        expected: StatusCode,
        marker: &str,
    ) -> String {
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|value| value.to_str().unwrap().to_string());
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = String::from_utf8_lossy(&bytes).into_owned();
        assert_eq!(
            status, expected,
            "21-03 status pin: {marker}: the daemon answered {status} with body {body}"
        );
        assert!(
            !content_type
                .as_deref()
                .is_some_and(|value| value.contains("json")),
            "21-03 body pin: {marker}: a {status} refusal carried content-type {content_type:?}"
        );
        body
    }

    /// The world a refused removal must leave alone: the `/rows` body, the
    /// `/sessions` body and the state file bytes.
    async fn world_snapshot(
        app: &Router,
        state_path: &Path,
    ) -> (serde_json::Value, serde_json::Value, Vec<u8>) {
        let rows = rows_body(app).await;
        let sessions = body_json(app.clone().oneshot(get("/sessions")).await.unwrap()).await;
        let state = std::fs::read(state_path).unwrap();
        (rows, sessions, state)
    }

    #[tokio::test]
    async fn matrix_missing_runtime_handle_is_503_and_touches_nothing() {
        let (root, _fixture, workspace, shared, app, id, key) =
            removal_fixture("matrix-handle-missing").await;
        let state_path = removal_state_file(&root, &workspace);

        let report = preview_report(&app, key).await;
        let agent_pid = previewed_agent_pid(&report, key);
        assert!(pid_is_alive(agent_pid));

        crate::manager::lock(&shared).forget_runtime_handle_for_test(key);
        assert_eq!(
            crate::manager::lock(&shared).checkout_key_for_runtime_for_test(id),
            None,
            "the seam drops the registry entry"
        );
        let stale = preview_report(&app, key).await;
        assert_eq!(
            stale["runtimes_killed"].as_array().unwrap().len(),
            1,
            "the row still records its runtime: {stale}"
        );
        // The world the refusal must leave alone is the one the request
        // found: `/rows` reads `session_id` through the registry the seam
        // just emptied.
        let (rows_before, _sessions_before, state_before) = world_snapshot(&app, &state_path).await;

        let refused = app
            .clone()
            .oneshot(post_json("/removals", &report.to_string()))
            .await
            .unwrap();
        let body = assert_refused(
            refused,
            StatusCode::SERVICE_UNAVAILABLE,
            "a row whose runtime handle the daemon does not hold",
        )
        .await;
        let row = format!("{:?}", RowKey::Checkout(key));
        assert!(body.contains(&row), "the body names the row {row}: {body}");
        assert!(
            body.contains("runtime") && body.contains("handle"),
            "the body says the runtime handle is missing: {body}"
        );

        let (rows_after, _sessions_after, state_after) = world_snapshot(&app, &state_path).await;
        assert_eq!(rows_after, rows_before);
        assert_eq!(state_after, state_before);
        assert!(
            pid_is_alive(agent_pid),
            "a refused removal must not stop the agent process"
        );

        removal_teardown(&shared, &root);
    }

    /// `/sessions` without the three fields the daemon derives from the clock
    /// and from PTY output, which move between two reads with no request in
    /// between. Everything a removal could change stays in.
    fn settled_sessions(sessions: &serde_json::Value) -> serde_json::Value {
        let mut settled = sessions.clone();
        for entry in settled.as_array_mut().unwrap() {
            let entry = entry.as_object_mut().unwrap();
            for field in ["status", "state_source", "waiting_for_ms"] {
                entry.remove(field);
            }
        }
        settled
    }

    /// The world still equals `before`: `/rows` and the state file exactly,
    /// `/sessions` through `settled_sessions`.
    async fn assert_world_unchanged(
        app: &Router,
        state_path: &Path,
        before: &(serde_json::Value, serde_json::Value, Vec<u8>),
        marker: &str,
    ) {
        let after = world_snapshot(app, state_path).await;
        assert_eq!(after.0, before.0, "{marker}: /rows changed");
        assert_eq!(
            settled_sessions(&after.1),
            settled_sessions(&before.1),
            "{marker}: /sessions changed"
        );
        assert!(after.2 == before.2, "{marker}: the state file changed");
    }

    /// `POST /removals` with `body`, pinned to 409. The one JSON error body:
    /// it must deserialize as a `RemovalReport` and be a preview. Returns the
    /// raw body, which a client resends verbatim, and its JSON value.
    async fn assert_drift(
        app: &Router,
        body: &serde_json::Value,
        marker: &str,
    ) -> (String, serde_json::Value) {
        let response = app
            .clone()
            .oneshot(post_json("/removals", &body.to_string()))
            .await
            .unwrap();
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|value| value.to_str().unwrap().to_string());
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let raw = String::from_utf8_lossy(&bytes).into_owned();
        assert_eq!(
            status,
            StatusCode::CONFLICT,
            "21-03 status pin: {marker}: the daemon answered {status} with body {raw}"
        );
        assert!(
            content_type
                .as_deref()
                .is_some_and(|value| value.contains("json")),
            "21-03 body pin: {marker}: a 409 carried content-type {content_type:?}"
        );
        let fresh: baude_core::removal::RemovalReport = serde_json::from_str(&raw)
            .unwrap_or_else(|error| panic!("{marker}: a 409 body is a report: {error}: {raw}"));
        assert!(fresh.planned, "{marker}: a 409 body is a preview: {raw}");
        let value = serde_json::from_str(&raw).unwrap();
        (raw, value)
    }

    /// Every agent pid a report names under `runtimes_killed`.
    fn runtime_pids(report: &serde_json::Value) -> Vec<u64> {
        report["runtimes_killed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|runtime| runtime["agent_pid"].as_u64().unwrap())
            .collect()
    }

    /// A real directory beside the fixture's repositories, holding one file.
    /// Returns the directory, the file and the file's bytes.
    fn sibling_directory(root: &Path) -> (PathBuf, PathBuf, Vec<u8>) {
        let sibling = root.join("sibling-project");
        std::fs::create_dir_all(&sibling).unwrap();
        let sibling = sibling.canonicalize().unwrap();
        let file = sibling.join("thesis.md");
        let bytes = b"three years of work\n".to_vec();
        std::fs::write(&file, &bytes).unwrap();
        (sibling, file, bytes)
    }

    #[tokio::test]
    async fn matrix_drift_row_set_change_is_409_then_resend_executes_then_replay_is_404() {
        let fixture = removal_fixture_two_checkouts("matrix-drift-rows").await;
        let app = &fixture.app;
        let state_path = removal_state_file(&fixture.root, &fixture.workspace);
        let repo_path = fixture.repo.path().canonicalize().unwrap();
        let main_row = rows_checkout_for_session(&rows_body(app).await, fixture.main_id);
        let repository_key = main_row["repository_key"].clone();
        let target = serde_json::json!({ "repository": repository_key });

        let stale = preview_target(app, &target).await;
        let stale_rows = stale["rows_removed"].as_array().unwrap().len();

        // The row set changes while the modal is open: a third session on a
        // second managed worktree of the same repository.
        let (_third_id, third_key, third_info) =
            admit_removal_session(&fixture.shared, app, &repo_path, Some("second")).await;
        let second_worktree = PathBuf::from(third_info["cwd"].as_str().unwrap());
        assert!(second_worktree.is_dir());
        let pids = runtime_pids(&preview_target(app, &target).await);
        assert_eq!(pids.len(), 3);
        let before = world_snapshot(app, &state_path).await;

        let (raw, fresh) = assert_drift(app, &stale, "a row-set change").await;
        assert_eq!(
            fresh["rows_removed"].as_array().unwrap().len(),
            stale_rows + 1,
            "{raw}"
        );
        assert!(
            fresh["rows_removed"]
                .as_array()
                .unwrap()
                .contains(&serde_json::json!({ "checkout": third_key })),
            "{raw}"
        );
        assert_world_unchanged(app, &state_path, &before, "after the 409").await;
        for pid in &pids {
            assert!(pid_is_alive(*pid), "a 409 stopped agent {pid}");
        }
        assert!(fixture.worktree_path.is_dir());
        assert!(second_worktree.is_dir());

        // Resending the 409 body verbatim executes it.
        let resent = app
            .clone()
            .oneshot(post_json("/removals", &raw))
            .await
            .unwrap();
        let status = resent.status();
        let executed =
            String::from_utf8_lossy(&resent.into_body().collect().await.unwrap().to_bytes())
                .into_owned();
        assert_eq!(
            status,
            StatusCode::OK,
            "21-03 status pin: a resent 409 body: the daemon answered {status} with body {executed}"
        );
        let rows_after = rows_bytes(app).await;
        let rows: serde_json::Value = serde_json::from_slice(&rows_after).unwrap();
        assert!(!rows_has_repository(&rows, &repository_key), "{rows}");
        for pid in &pids {
            assert!(wait_for_pid_gone(*pid), "agent {pid} outlived its row");
        }
        assert_eq!(session_ids(app).await, Vec::<u64>::new());

        // The executed report is state-bound, not time-bound: it is now 404.
        let state_after = std::fs::read(&state_path).unwrap();
        let replayed = app
            .clone()
            .oneshot(post_json("/removals", &raw))
            .await
            .unwrap();
        assert_refused(
            replayed,
            StatusCode::NOT_FOUND,
            "an executed report sent again",
        )
        .await;
        assert_eq!(rows_bytes(app).await, rows_after);
        assert!(std::fs::read(&state_path).unwrap() == state_after);

        removal_teardown(&fixture.shared, &fixture.root);
    }

    #[tokio::test]
    async fn matrix_drift_runtime_only_change_is_409() {
        let (root, _fixture, workspace, shared, app, id, key) =
            removal_fixture("matrix-drift-runtime").await;
        let state_path = removal_state_file(&root, &workspace);

        let stale = preview_report(&app, key).await;
        let previewed_pid = previewed_agent_pid(&stale, key);

        // The agent dies and is restarted while the modal is open.
        let signalled = Command::new("kill")
            .arg(previewed_pid.to_string())
            .status()
            .unwrap();
        assert!(signalled.success());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let sessions = body_json(app.clone().oneshot(get("/sessions")).await.unwrap()).await;
            let exited = sessions
                .as_array()
                .unwrap()
                .iter()
                .any(|session| session["id"] == id && session["status"] == "exited");
            if exited {
                break;
            }
            assert!(
                Instant::now() < deadline,
                "session {id} never read as exited: {sessions}"
            );
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let (status, body) = removal_call(
            &app,
            &format!("/sessions/{id}/restart"),
            &serde_json::json!({}),
        )
        .await;
        assert_eq!(status, StatusCode::ACCEPTED, "restart: {body}");
        let before = world_snapshot(&app, &state_path).await;

        let (raw, fresh) = assert_drift(&app, &stale, "a runtime-only change").await;
        assert_eq!(fresh["rows_removed"], stale["rows_removed"], "{raw}");
        assert_eq!(fresh["paths_deleted"], stale["paths_deleted"], "{raw}");
        assert_ne!(fresh["runtimes_killed"], stale["runtimes_killed"], "{raw}");
        let fresh_pid = previewed_agent_pid(&fresh, key);
        assert_ne!(fresh_pid, previewed_pid);
        assert_world_unchanged(&app, &state_path, &before, "after the 409").await;
        assert!(
            pid_is_alive(fresh_pid),
            "a 409 stopped the restarted agent {fresh_pid}"
        );
        eprintln!(
            "21-03 observation: runtime-only drift previewed agent_pid {previewed_pid}, fresh \
             agent_pid {fresh_pid}, session ids after restart {:?}, previewed session id {id}",
            session_ids(&app).await
        );

        removal_teardown(&shared, &root);
    }

    #[tokio::test]
    async fn matrix_drift_tampered_paths_deleted_is_409_and_touches_nothing() {
        let fixture = removal_fixture_two_checkouts("matrix-drift-paths").await;
        let app = &fixture.app;
        let state_path = removal_state_file(&fixture.root, &fixture.workspace);
        let target = serde_json::json!({ "checkout": fixture.worktree_key });
        let previewed = preview_target(app, &target).await;
        let planned_paths = report_paths(&previewed, "paths_deleted");
        assert_eq!(planned_paths.len(), 1, "{previewed}");
        assert_eq!(
            Path::new(&planned_paths[0]).canonicalize().unwrap(),
            fixture.worktree_path.canonicalize().unwrap()
        );
        let (sibling, sibling_file, sibling_bytes) = sibling_directory(&fixture.root);

        let mut tampered = previewed.clone();
        tampered["paths_deleted"] = serde_json::json!([sibling]);
        let before = world_snapshot(app, &state_path).await;

        let (raw, fresh) = assert_drift(app, &tampered, "a tampered paths_deleted").await;
        assert_eq!(
            report_paths(&fresh, "paths_deleted"),
            planned_paths,
            "{raw}"
        );
        assert!(sibling.is_dir());
        assert_eq!(std::fs::read(&sibling_file).unwrap(), sibling_bytes);
        assert!(fixture.worktree_path.is_dir());
        assert_world_unchanged(app, &state_path, &before, "after the 409").await;
        for pid in runtime_pids(&previewed) {
            assert!(pid_is_alive(pid), "a 409 stopped agent {pid}");
        }

        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// The property the superseded plan registered as T-21-05: a body that
    /// passes the recheck names nothing the daemon acts on.
    #[tokio::test]
    async fn matrix_drift_uncompared_fields_cannot_steer_the_daemon() {
        let fixture = removal_fixture_two_checkouts("matrix-drift-steer").await;
        let app = &fixture.app;
        let target = serde_json::json!({ "checkout": fixture.worktree_key });
        let previewed = preview_target(app, &target).await;
        let planned_paths = report_paths(&previewed, "paths_deleted");
        assert_eq!(planned_paths.len(), 1, "{previewed}");
        let (sibling, sibling_file, sibling_bytes) = sibling_directory(&fixture.root);
        let sibling_text = sibling.to_str().unwrap().to_string();

        // The three compared sets and the target stay exactly as previewed.
        // Every other field is rewritten with a type-valid value that names
        // the sibling.
        let mut steering = previewed.clone();
        steering["paths_kept"] = serde_json::json!([sibling]);
        steering["already_removed"] = serde_json::json!([sibling]);
        steering["planned"] = serde_json::json!(false);
        steering["persistence"] = serde_json::json!({ "outcome": "saved" });
        steering["cleared"] = serde_json::json!([
            { "kind": "repository_dir", "path": sibling },
            { "kind": "marker", "path": sibling },
        ]);
        steering["extra"] = serde_json::json!({ "delete": sibling });
        for field in ["target", "rows_removed", "runtimes_killed", "paths_deleted"] {
            assert_eq!(steering[field], previewed[field]);
        }

        let (status, body) = removal_call(app, "/removals", &steering).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "21-03 status pin: uncompared fields rewritten: the daemon answered {status} with \
             body {body}"
        );
        let executed: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(report_paths(&executed, "paths_deleted"), planned_paths);
        for field in ["paths_kept", "already_removed"] {
            assert!(
                !report_paths(&executed, field).contains(&sibling_text),
                "{field} echoed the sibling: {body}"
            );
        }
        assert!(
            !executed["cleared"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item["path"] == sibling_text.as_str()),
            "cleared echoed the sibling: {body}"
        );
        assert!(!fixture.worktree_path.exists());
        assert!(sibling.is_dir());
        assert_eq!(std::fs::read(&sibling_file).unwrap(), sibling_bytes);

        removal_teardown(&fixture.shared, &fixture.root);
    }

    #[tokio::test]
    async fn matrix_drift_holds_for_a_standalone_row() {
        let (root, _owner, folder, workspace, shared, app) = folder_fixture("matrix-drift-folder");
        let state_path = removal_state_file(&root, &workspace);
        let id = opened_folder_id(&app, &folder).await;
        let key = standalone_key_of(&only_standalone_row(&app).await);
        let row = serde_json::json!({ "standalone": key });

        let stale = preview_target(&app, &row).await;
        let agent_pid = only_runtime_pid(&stale);

        // Closing the session keeps the row and ends its runtime.
        let (status, body) = delete_session_status(&app, id).await;
        assert_eq!(status, StatusCode::NO_CONTENT, "{body}");
        assert!(wait_for_pid_gone(agent_pid));
        let before = world_snapshot(&app, &state_path).await;

        let (raw, fresh) = assert_drift(&app, &stale, "a standalone row whose runtime ended").await;
        assert_eq!(fresh["rows_removed"], stale["rows_removed"], "{raw}");
        assert_eq!(fresh["runtimes_killed"], serde_json::json!([]), "{raw}");
        assert_eq!(fresh["paths_deleted"], serde_json::json!([]), "{raw}");
        assert_world_unchanged(&app, &state_path, &before, "after the 409").await;

        let resent = app
            .clone()
            .oneshot(post_json("/removals", &raw))
            .await
            .unwrap();
        let status = resent.status();
        let executed =
            String::from_utf8_lossy(&resent.into_body().collect().await.unwrap().to_bytes())
                .into_owned();
        assert_eq!(
            status,
            StatusCode::OK,
            "21-03 status pin: a resent 409 body: the daemon answered {status} with body {executed}"
        );
        assert_eq!(rows_body(&app).await["standalone"], serde_json::json!([]));
        assert!(folder.is_dir());
        assert_eq!(
            std::fs::read(folder.join("todo.txt")).unwrap(),
            FOLDER_TODO_BYTES
        );

        removal_teardown(&shared, &root);
    }

    /// `POST /removals` with `body`, pinned to 200. Returns the executed
    /// report.
    async fn assert_executed(
        app: &Router,
        body: &serde_json::Value,
        marker: &str,
    ) -> serde_json::Value {
        let (status, text) = removal_call(app, "/removals", body).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "21-03 status pin: {marker}: the daemon answered {status} with body {text}"
        );
        serde_json::from_str(&text).unwrap()
    }

    /// `POST /removals` with `body` and hand back the response unread, for
    /// `assert_refused`.
    async fn removal_response(app: &Router, body: &serde_json::Value) -> axum::response::Response {
        app.clone()
            .oneshot(post_json("/removals", &body.to_string()))
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn matrix_failure_runtime_survived_is_500_then_retry_succeeds() {
        let (root, _fixture, workspace, shared, app, id, key) =
            removal_fixture("matrix-survived").await;
        let state_path = removal_state_file(&root, &workspace);
        let previewed = preview_report(&app, key).await;
        let agent_pid = previewed_agent_pid(&previewed, key);
        let before = world_snapshot(&app, &state_path).await;

        crate::manager::lock(&shared)
            .fail_next_agent_teardown_for_test(id, "agent stop refused once");
        let body = assert_refused(
            removal_response(&app, &previewed).await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "a runtime that survived the kill stage",
        )
        .await;
        assert!(body.contains("survived the kill stage"), "{body}");
        assert!(
            body.contains(&format!("{:?}", RowKey::Checkout(key))),
            "{body}"
        );
        assert_eq!(session_ids(&app).await, [id]);
        {
            let manager = crate::manager::lock(&shared);
            assert_eq!(manager.checkout_key_for_runtime_for_test(id), Some(key));
            assert!(!manager.persistence_status().dirty);
        }
        assert_eq!(rows_body(&app).await, before.0);
        assert!(std::fs::read(&state_path).unwrap() == before.2);

        // A fresh preview and its execute then succeed.
        let retried = preview_report(&app, key).await;
        assert_executed(&app, &retried, "the retry after a survived runtime").await;
        assert!(wait_for_pid_gone(agent_pid));
        assert_eq!(session_ids(&app).await, Vec::<u64>::new());

        removal_teardown(&shared, &root);
    }

    #[tokio::test]
    async fn matrix_failure_standalone_runtime_survived_is_500_then_retry_succeeds() {
        let (root, _owner, folder, workspace, shared, app) =
            folder_fixture("matrix-survived-folder");
        let state_path = removal_state_file(&root, &workspace);
        let id = opened_folder_id(&app, &folder).await;
        let entry = only_standalone_row(&app).await;
        assert_eq!(entry["session_id"].as_u64(), Some(id), "{entry}");
        let row = serde_json::json!({ "standalone": standalone_key_of(&entry) });

        let previewed = preview_target(&app, &row).await;
        let agent_pid = only_runtime_pid(&previewed);
        let before = world_snapshot(&app, &state_path).await;

        crate::manager::lock(&shared)
            .fail_next_agent_teardown_for_test(id, "agent stop refused once");
        let body = assert_refused(
            removal_response(&app, &previewed).await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "a standalone runtime that survived the kill stage",
        )
        .await;
        assert!(body.contains("survived the kill stage"), "{body}");
        assert_eq!(session_ids(&app).await, [id]);
        let held = only_standalone_row(&app).await;
        assert_eq!(held["session_id"].as_u64(), Some(id), "{held}");
        assert!(std::fs::read(&state_path).unwrap() == before.2);
        assert!(!crate::manager::lock(&shared).persistence_status().dirty);
        assert_folder_untouched(&folder);

        let retried = preview_target(&app, &row).await;
        let executed = assert_executed(&app, &retried, "the retry on a standalone row").await;
        let killed = executed["runtimes_killed"].as_array().unwrap();
        assert_eq!(killed.len(), 1, "{executed}");
        assert_eq!(killed[0]["row"], row, "{executed}");
        assert!(wait_for_pid_gone(agent_pid));
        assert_eq!(rows_body(&app).await["standalone"], serde_json::json!([]));
        assert_eq!(session_ids(&app).await, Vec::<u64>::new());
        assert_folder_untouched(&folder);

        removal_teardown(&shared, &root);
    }

    #[tokio::test]
    async fn matrix_failure_parent_abort_keeps_every_handle_held() {
        let fixture = removal_fixture_two_checkouts("matrix-parent-abort").await;
        let app = &fixture.app;
        let state_path = removal_state_file(&fixture.root, &fixture.workspace);
        let main_row = rows_checkout_for_session(&rows_body(app).await, fixture.main_id);
        let repository_key = main_row["repository_key"].clone();
        let target = serde_json::json!({ "repository": repository_key });
        let previewed = preview_target(app, &target).await;
        let planned = previewed["runtimes_killed"].as_array().unwrap();
        assert_eq!(planned.len(), 2, "{previewed}");
        let main_json = serde_json::json!({ "checkout": fixture.main_key });
        let worktree_json = serde_json::json!({ "checkout": fixture.worktree_key });

        // Arm the row the preview lists last, so one child may stop first.
        let armed_is_main = planned[1]["row"] == main_json;
        assert!(armed_is_main || planned[1]["row"] == worktree_json);
        let (armed_id, armed_key, other_pid) = if armed_is_main {
            (fixture.main_id, fixture.main_key, &planned[0]["agent_pid"])
        } else {
            (
                fixture.worktree_id,
                fixture.worktree_key,
                &planned[0]["agent_pid"],
            )
        };
        let other_pid = other_pid.as_u64().unwrap();
        let before = world_snapshot(app, &state_path).await;

        crate::manager::lock(&fixture.shared)
            .fail_next_agent_teardown_for_test(armed_id, "agent stop refused once");
        let body = assert_refused(
            removal_response(app, &previewed).await,
            StatusCode::INTERNAL_SERVER_ERROR,
            "a repository removal that aborted on its second runtime",
        )
        .await;
        assert!(
            body.contains(&format!("{:?}", RowKey::Checkout(armed_key))),
            "the 500 names the armed row: {body}"
        );
        assert_eq!(
            session_ids(app).await,
            [fixture.main_id, fixture.worktree_id]
        );
        {
            let manager = crate::manager::lock(&fixture.shared);
            assert_eq!(
                manager.checkout_key_for_runtime_for_test(fixture.main_id),
                Some(fixture.main_key)
            );
            assert_eq!(
                manager.checkout_key_for_runtime_for_test(fixture.worktree_id),
                Some(fixture.worktree_key)
            );
            assert!(!manager.persistence_status().dirty);
        }
        assert_eq!(
            rows_body(app).await,
            before.0,
            "all three rows still served"
        );
        assert!(fixture.worktree_path.is_dir());
        assert!(std::fs::read(&state_path).unwrap() == before.2);
        eprintln!(
            "21-03 observation: parent abort armed the {} row, listed last; the other child's \
             agent pid {other_pid} was {} after the 500",
            if armed_is_main { "main" } else { "worktree" },
            if pid_is_alive(other_pid) {
                "alive"
            } else {
                "dead"
            }
        );

        let retried = preview_target(app, &target).await;
        let executed = assert_executed(app, &retried, "the retry after a parent abort").await;
        assert_eq!(executed["rows_removed"].as_array().unwrap().len(), 3);
        assert!(!rows_has_repository(&rows_body(app).await, &repository_key));
        assert_eq!(session_ids(app).await, Vec::<u64>::new());
        for pid in runtime_pids(&previewed) {
            assert!(wait_for_pid_gone(pid), "agent {pid} outlived its row");
        }

        removal_teardown(&fixture.shared, &fixture.root);
    }

    /// The API-01 interrupted edge: core's disk stage runs before its one
    /// save, so a save that fails leaves the row stored and the worktree
    /// gone. The retry converges through 409 to 200.
    #[tokio::test]
    async fn matrix_failure_persist_is_503_and_the_retry_converges() {
        let root = removal_root("matrix-persist");
        let (_upstream, clone) = cloned_repo(&root);
        let (workspace, shared, app) = removal_daemon(&root);
        let state_path = removal_state_file(&root, &workspace);
        admit_removal_session(&shared, &app, &clone, None).await;
        let (worktree_id, worktree_key, info) =
            admit_removal_session(&shared, &app, &clone, Some("topic")).await;
        let worktree_path = PathBuf::from(info["cwd"].as_str().unwrap());
        assert!(worktree_path.is_dir());
        let branch_ref = git_read(&worktree_path, &["symbolic-ref", "-q", "HEAD"])
            .expect("the managed worktree sits on a branch");
        let target = serde_json::json!({ "checkout": worktree_key });
        let previewed = preview_target(&app, &target).await;
        let planned_paths = report_paths(&previewed, "paths_deleted");
        assert_eq!(planned_paths.len(), 1, "{previewed}");
        let before = world_snapshot(&app, &state_path).await;

        crate::manager::lock(&shared).persist_at_for_test(
            &root,
            &workspace,
            Some(baude_core::persist::AtomicFailure::Rename),
        );
        let interrupted = assert_refused(
            removal_response(&app, &previewed).await,
            StatusCode::SERVICE_UNAVAILABLE,
            "a save that failed before the replacement committed",
        )
        .await;
        assert!(interrupted.contains("could not persist"), "{interrupted}");
        let rows = rows_body(&app).await;
        assert_eq!(
            rows_checkout_for_session(&rows, worktree_id)["checkout_key"],
            serde_json::json!(worktree_key),
            "the row is stored and its handle restored: {rows}"
        );
        assert!(session_ids(&app).await.contains(&worktree_id));
        assert!(std::fs::read(&state_path).unwrap() == before.2);
        {
            let status = crate::manager::lock(&shared).persistence_status();
            assert!(status.dirty, "a failed save leaves the daemon dirty");
            assert!(status.error.is_some(), "a failed save records its error");
        }
        assert!(
            !worktree_path.exists(),
            "the disk stage ran before the save"
        );

        // Replayed with no re-arming: the path is gone, so the body drifted.
        let (replayed, fresh) =
            assert_drift(&app, &previewed, "the interrupted removal replayed").await;
        assert_eq!(report_paths(&fresh, "already_removed"), planned_paths);
        assert_eq!(fresh["paths_deleted"], serde_json::json!([]), "{replayed}");

        let (status, converged) = removal_call(&app, "/removals", &fresh).await;
        assert_eq!(
            status,
            StatusCode::OK,
            "21-03 status pin: the 409 body resent: the daemon answered {status} with body \
             {converged}"
        );
        let executed: serde_json::Value = serde_json::from_str(&converged).unwrap();
        assert_eq!(
            executed["persistence"],
            serde_json::json!({ "outcome": "saved" })
        );
        eprintln!("21-03 observation: convergence 503 body: {interrupted}");
        eprintln!("21-03 observation: convergence 409 body: {replayed}");
        eprintln!("21-03 observation: convergence 200 body: {converged}");

        assert!(
            git_read(&clone, &["show-ref", "--verify", &branch_ref]).is_some(),
            "{branch_ref} no longer resolves"
        );
        assert!(git_read(&clone, &["rev-parse", "--git-dir"]).is_some());
        assert!(!crate::manager::lock(&shared).persistence_status().dirty);

        // A fresh restore does not list the row.
        crate::manager::lock(&shared).kill_all();
        let mut restored = Manager::new(REMOVAL_AGENT_CMD.into(), true);
        restored.restore_at(&root, &workspace);
        assert!(matches!(
            restored.preview_removal(RemovalTarget::Checkout(worktree_key)),
            Err(RemovalError::UnknownTarget(_))
        ));
        restored.kill_all();
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[tokio::test]
    async fn matrix_failure_committed_replacement_is_a_200_saved_degraded() {
        let (root, _fixture, workspace, shared, app, id, key) =
            removal_fixture("matrix-degraded").await;
        let previewed = preview_report(&app, key).await;
        let agent_pid = previewed_agent_pid(&previewed, key);

        crate::manager::lock(&shared).persist_at_for_test(
            &root,
            &workspace,
            Some(baude_core::persist::AtomicFailure::DirectorySync),
        );
        let executed =
            assert_executed(&app, &previewed, "a save whose replacement committed").await;
        assert_eq!(
            executed["persistence"]["outcome"], "saved_degraded",
            "{executed}"
        );
        let detail = executed["persistence"]["detail"]
            .as_str()
            .unwrap_or_else(|| panic!("a degraded save carries a detail: {executed}"))
            .to_string();
        assert!(!detail.is_empty());
        assert_eq!(
            checkout_entries(&rows_body(&app).await),
            Vec::<serde_json::Value>::new()
        );
        assert_eq!(session_ids(&app).await, Vec::<u64>::new());
        assert!(wait_for_pid_gone(agent_pid));
        {
            let manager = crate::manager::lock(&shared);
            assert_eq!(manager.checkout_key_for_runtime_for_test(id), None);
            let status = manager.persistence_status();
            assert!(status.dirty, "a degraded save leaves the daemon dirty");
            assert_eq!(status.error.as_deref(), Some(detail.as_str()));
        }

        let mut restored = Manager::new(REMOVAL_AGENT_CMD.into(), true);
        assert_eq!(restored.restore_at(&root, &workspace), 0);
        assert!(matches!(
            restored.preview_removal(RemovalTarget::Checkout(key)),
            Err(RemovalError::UnknownTarget(_))
        ));

        removal_teardown(&shared, &root);
    }

    #[tokio::test]
    async fn matrix_failure_busy_repository_is_503_before_any_kill() {
        let (root, _fixture, workspace, shared, app, id, key) =
            removal_fixture("matrix-busy").await;
        let state_path = removal_state_file(&root, &workspace);
        let row = rows_checkout_for_session(&rows_body(&app).await, id);
        let repository: baude_core::repository::RepositoryKey =
            serde_json::from_value(row["repository_key"].clone()).unwrap();
        let previewed = preview_report(&app, key).await;
        let agent_pid = previewed_agent_pid(&previewed, key);
        let before = world_snapshot(&app, &state_path).await;

        // A real reservation, held across the request with the manager lock
        // released.
        let reservation = crate::manager::lock(&shared).reserve_repository_for_test(repository);
        let body = assert_refused(
            removal_response(&app, &previewed).await,
            StatusCode::SERVICE_UNAVAILABLE,
            "a repository another mutation holds",
        )
        .await;
        assert!(body.starts_with("repository busy:"), "{body}");
        assert!(pid_is_alive(agent_pid), "a busy refusal stopped the agent");
        assert_world_unchanged(&app, &state_path, &before, "after the busy 503").await;

        drop(reservation);
        assert_executed(
            &app,
            &previewed,
            "the same body once the repository is free",
        )
        .await;
        assert!(wait_for_pid_gone(agent_pid));

        removal_teardown(&shared, &root);
    }

    #[tokio::test]
    async fn matrix_failure_blocked_persistence_is_503_before_the_target_is_looked_up() {
        let (root, _fixture, workspace, shared, app, _id, key) =
            removal_fixture("matrix-blocked").await;
        let report = preview_report(&app, key).await;
        let agent_pid = previewed_agent_pid(&report, key);

        // A second daemon whose state file the loader rejects: persistence is
        // blocked by its real cause, and this daemon stores no row at all.
        let second_root = removal_root("matrix-blocked-second");
        let second_state = removal_state_file(&second_root, &workspace);
        std::fs::create_dir_all(second_state.parent().unwrap()).unwrap();
        std::fs::write(&second_state, b"this is not a state file\n").unwrap();
        let (blocked, blocked_app) = restarted_daemon(&second_root, &workspace, REMOVAL_AGENT_CMD);
        {
            let manager = crate::manager::lock(&blocked);
            assert!(
                manager.persistence_status().blocked,
                "the loader rejected the state file, so persistence is blocked"
            );
            assert!(matches!(
                manager.preview_removal(RemovalTarget::Checkout(key)),
                Err(RemovalError::UnknownTarget(_))
            ));
        }

        let body = assert_refused(
            removal_response(&blocked_app, &report).await,
            StatusCode::SERVICE_UNAVAILABLE,
            "blocked persistence, for a target this daemon does not store",
        )
        .await;
        assert!(body.contains("persistence is blocked"), "{body}");
        assert!(pid_is_alive(agent_pid));

        std::fs::remove_dir_all(&second_root).unwrap();
        removal_teardown(&shared, &root);
    }

    #[tokio::test]
    async fn matrix_refused_admission_is_400_plain_text() {
        let fixture = removal_fixture_two_checkouts("matrix-admission").await;
        let app = &fixture.app;

        // 21-10's recorded case: the path of a managed worktree the daemon
        // removed, so nothing is there to admit.
        let removed = fixture.worktree_path.clone();
        removed_over_routes(
            app,
            &serde_json::json!({ "checkout": fixture.worktree_key }),
        )
        .await;
        assert!(!removed.exists());
        // A path that never existed, and a path that is a file.
        let never_made = fixture.root.join("never-made");
        let file = fixture.root.join("notes.txt");
        std::fs::write(&file, b"not a folder\n").unwrap();

        for (case, path) in [
            ("a removed managed worktree", &removed),
            ("a path that never existed", &never_made),
            ("a regular file", &file),
        ] {
            let rows_before = rows_bytes(app).await;
            let sessions_before = session_ids(app).await;
            let response = app
                .clone()
                .oneshot(post_json(
                    "/sessions",
                    &serde_json::json!({ "repo": path }).to_string(),
                ))
                .await
                .unwrap();
            let body = assert_refused(response, StatusCode::BAD_REQUEST, case).await;
            assert!(body.contains("not a directory"), "{case}: {body}");
            assert_eq!(rows_bytes(app).await, rows_before, "{case}");
            assert_eq!(session_ids(app).await, sessions_before, "{case}");
        }

        removal_teardown(&fixture.shared, &fixture.root);
    }
}

#[cfg(test)]
mod pty_ws_tests {
    use std::sync::{Arc, Mutex};
    use std::time::Duration;

    use futures_util::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message as WsMessage;

    use super::tests::api_scope;
    use crate::manager::{lock, Manager};

    #[tokio::test]
    async fn pty_websocket_round_trip() {
        // Declared first so it drops last: the spawned `axum::serve` task and
        // the PTY it drives run on this same current-thread runtime, so the
        // thread-local redirect must outlive the socket, not just the setup.
        let _scope = api_scope("pty-websocket");
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
