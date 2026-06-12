//! REST + SSE surface. Security is the tailnet (see docs/remote-daemon-plan.md):
//! bind the Tailscale interface, no auth layer here.

use std::convert::Infallible;
use std::path::PathBuf;
use std::time::Duration;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;

use crate::manager::{lock, SessionInfo, Shared};
use crate::transcript::{self, ChatMessage, Tail};

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
        .route("/sessions/{id}/screen", get(get_screen))
        .route("/sessions/{id}/keys", post(post_keys))
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
