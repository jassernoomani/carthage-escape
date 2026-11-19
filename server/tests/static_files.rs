//! Serving the built frontend: SPA fallback, caching, compression.

use std::path::PathBuf;

use axum::{
    body::Body,
    http::{Request, StatusCode, header},
};
use carthage_server::AppState;
use sqlx::PgPool;
use tower::ServiceExt;

fn fake_dist() -> PathBuf {
    let dir = std::env::temp_dir().join(format!("carthage-dist-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("index.html"), "<!doctype html><title>app</title>").unwrap();
    // Big and repetitive enough to be worth compressing.
    std::fs::write(
        dir.join("assets/app-abc123.js"),
        "console.log('hi');\n".repeat(500),
    )
    .unwrap();
    dir
}

async fn get(pool: PgPool, dir: &std::path::Path, uri: &str) -> axum::response::Response {
    let app = carthage_server::app(
        AppState::new(pool, b"test-secret-test-secret-test-secret!", false),
        Some(dir),
    );
    app.oneshot(
        Request::get(uri)
            .header(header::ACCEPT_ENCODING, "gzip, br")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap()
}

#[sqlx::test]
async fn hashed_assets_are_compressed_and_cached_forever(pool: PgPool) {
    let dir = fake_dist();
    let res = get(pool, &dir, "/assets/app-abc123.js").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        res.headers().contains_key(header::CONTENT_ENCODING),
        "should be compressed"
    );
    assert_eq!(
        res.headers()[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
}

#[sqlx::test]
async fn client_routes_fall_back_to_index_which_is_never_cached(pool: PgPool) {
    let dir = fake_dist();
    let res = get(pool, &dir, "/host/sessions/whatever").await;
    assert_eq!(res.status(), StatusCode::OK);
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-cache");
}

#[sqlx::test]
async fn missing_assets_are_404_not_index(pool: PgPool) {
    let dir = fake_dist();
    let res = get(pool, &dir, "/assets/nope.js").await;
    assert_eq!(res.status(), StatusCode::NOT_FOUND);
}
