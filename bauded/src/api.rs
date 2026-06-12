//! REST + SSE surface. Security is the tailnet (see docs/remote-daemon-plan.md):
//! bind the Tailscale interface, no auth layer here.

use std::convert::Infallible;
use std::path::PathBuf;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::manager::{lock, SessionInfo, Shared};
use crate::transcript::{self, ChatMessage, Tail};

pub fn router(state: Shared) -> Router {
    Router::new()
        .route("/sessions", get(list_sessions).post(create_session))
        .route("/sessions/{id}", delete(delete_session))
        .route(
            "/sessions/{id}/messages",
            get(get_messages).post(post_message),
        )
        .route("/sessions/{id}/interrupt", post(interrupt))
        .route("/sessions/{id}/stream", get(stream))
        .with_state(state)
}

type ApiError = (StatusCode, String);

fn not_found(e: anyhow::Error) -> ApiError {
    (StatusCode::NOT_FOUND, e.to_string())
}

async fn list_sessions(State(state): State<Shared>) -> Json<Vec<SessionInfo>> {
    Json(lock(&state).list())
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
    lock(&state)
        .post_message(id, &body.text)
        .map_err(not_found)?;
    Ok(StatusCode::ACCEPTED)
}

async fn interrupt(
    State(state): State<Shared>,
    Path(id): Path<u64>,
) -> Result<StatusCode, ApiError> {
    lock(&state).interrupt(id).map_err(not_found)?;
    Ok(StatusCode::ACCEPTED)
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
