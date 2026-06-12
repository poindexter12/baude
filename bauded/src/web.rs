//! The phone PWA, embedded in the binary so deploying bauded deploys the
//! frontend. Plain HTML/CSS/JS in ../web — no build step.

use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::manager::Shared;

macro_rules! asset {
    ($path:literal, $mime:literal) => {
        get(|| async {
            (
                [
                    (CONTENT_TYPE, $mime),
                    // network-first; the service worker provides offline
                    (CACHE_CONTROL, "no-cache"),
                ],
                include_bytes!(concat!("../web/", $path)).as_slice(),
            )
                .into_response()
        })
    };
}

pub fn router() -> Router<Shared> {
    Router::new()
        .route("/", asset!("index.html", "text/html; charset=utf-8"))
        .route(
            "/app.js",
            asset!("app.js", "text/javascript; charset=utf-8"),
        )
        .route("/style.css", asset!("style.css", "text/css; charset=utf-8"))
        .route("/sw.js", asset!("sw.js", "text/javascript; charset=utf-8"))
        .route(
            "/manifest.webmanifest",
            asset!("manifest.webmanifest", "application/manifest+json"),
        )
        .route("/icon.svg", asset!("icon.svg", "image/svg+xml"))
        .route("/icon-192.png", asset!("icon-192.png", "image/png"))
        .route("/icon-512.png", asset!("icon-512.png", "image/png"))
        .route(
            "/apple-touch-icon.png",
            asset!("apple-touch-icon.png", "image/png"),
        )
}
