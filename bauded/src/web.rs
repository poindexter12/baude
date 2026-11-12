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

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use axum::body::Body;
    use axum::http::{header, Request, StatusCode};
    use axum::Router;
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    use crate::manager::Manager;

    /// The isolation a `Manager` needs before it may be built in a test: a
    /// synthetic root every resolved path is pinned inside, and a literal
    /// workspace identity under it. The asset routes read no state, but the
    /// router is typed over `Shared`, so one has to exist.
    struct WebScope {
        /// Fields drop in declaration order, so the identity is restored while
        /// its own root is still installed.
        _identity: baude_core::testing::TestRedirect,
        _redirect: baude_core::testing::TestRedirect,
    }

    /// `web::router` with a state, as `api::router` mounts it.
    fn app(label: &str) -> (WebScope, Router) {
        static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "bauded-web-{label}-{}-{}",
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
        let scope = WebScope {
            _identity: identity,
            _redirect: redirect,
        };
        let state = Arc::new(Mutex::new(Manager::new("sleep 30".into(), false)));
        (scope, super::router().with_state(state))
    }

    /// One served asset: status, content type and body as text.
    async fn fetch(app: &Router, path: &str) -> (StatusCode, String, String) {
        let request = Request::get(path).body(Body::empty()).unwrap();
        let response = app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let content_type = response
            .headers()
            .get(header::CONTENT_TYPE)
            .map(|value| value.to_str().unwrap().to_string())
            .unwrap_or_default();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        (
            status,
            content_type,
            String::from_utf8_lossy(&bytes).into_owned(),
        )
    }

    #[tokio::test]
    async fn removal_assets_are_served_in_load_order() {
        let (_scope, app) = app("assets-load-order");

        for path in ["/removal.js", "/removal-controller.js"] {
            let (status, content_type, body) = fetch(&app, path).await;
            assert_eq!(status, StatusCode::OK, "{path}");
            assert!(
                content_type.starts_with("text/javascript"),
                "{path} served as {content_type:?}"
            );
            assert!(!body.is_empty(), "{path} served an empty body");
        }

        // The page loads the view model, then the controller, then the app
        // that constructs it. A later script cannot see an earlier one's
        // global if the order is anything else.
        let (status, _, page) = fetch(&app, "/").await;
        assert_eq!(status, StatusCode::OK);
        let position = |script: &str| {
            let tag = format!("<script src=\"{script}\"></script>");
            page.find(&tag)
                .unwrap_or_else(|| panic!("the served page has no {tag}"))
        };
        let removal = position("/removal.js");
        let controller = position("/removal-controller.js");
        let app_js = position("/app.js");
        assert!(
            removal < controller,
            "removal.js must precede the controller"
        );
        assert!(controller < app_js, "the controller must precede app.js");
    }
}
