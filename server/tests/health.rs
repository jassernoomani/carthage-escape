use axum::{
    body::Body,
    http::{Request, StatusCode},
};
use carthage_server::AppState;
use http_body_util::BodyExt;
use serde_json::Value;
use sqlx::PgPool;
use tower::ServiceExt;

async fn get_json(pool: PgPool, uri: &str) -> (StatusCode, Value) {
    let app = carthage_server::app(AppState { db: pool }, None);
    let res = app
        .oneshot(Request::get(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let bytes = res.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&bytes).unwrap())
}

// `#[sqlx::test]` creates a throwaway database and applies ./migrations to it,
// so this also checks that the migrations run cleanly.
#[sqlx::test]
async fn health_is_ok_when_db_is_up(pool: PgPool) {
    let (status, body) = get_json(pool, "/api/health").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["database"], "ok");
    assert_eq!(body["version"], env!("CARGO_PKG_VERSION"));
}

#[sqlx::test]
async fn health_is_degraded_when_db_is_down(pool: PgPool) {
    pool.close().await;
    let (status, body) = get_json(pool, "/api/health").await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(body["status"], "degraded");
    assert_eq!(body["database"], "degraded");
}

#[sqlx::test]
async fn unknown_api_route_is_json_404(pool: PgPool) {
    let (status, body) = get_json(pool, "/api/nope").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["code"], "not_found");
}
