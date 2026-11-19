pub mod api;
pub mod auth;
pub mod config;
pub mod content;
pub mod db;
pub mod error;
pub mod game;
pub mod nickname;
pub mod rate_limit;
pub mod ws;

use std::{path::Path, sync::Arc};

use axum::{Router, routing::get};
use sqlx::PgPool;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

use crate::{auth::TokenKeys, game::RoomManager, rate_limit::RateLimiter};

/// Shared state handed to every request handler.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub rooms: RoomManager,
    pub tokens: Arc<TokenKeys>,
    pub limiter: RateLimiter,
    pub cookie_secure: bool,
}

impl AppState {
    pub fn new(db: PgPool, jwt_secret: &[u8], cookie_secure: bool) -> Self {
        Self {
            db,
            rooms: RoomManager::default(),
            tokens: Arc::new(TokenKeys::new(jwt_secret)),
            limiter: RateLimiter::default(),
            cookie_secure,
        }
    }
}

/// Builds the full HTTP application.
///
/// When `static_dir` is set (production), the built frontend is served from it,
/// with unknown paths falling back to `index.html` so client-side routing works.
pub fn app(state: AppState, static_dir: Option<&Path>) -> Router {
    let router = Router::new()
        .nest("/api", api::router())
        .route("/ws", get(ws::ws_handler))
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    match static_dir {
        Some(dir) => {
            let spa = ServeDir::new(dir).fallback(ServeFile::new(dir.join("index.html")));
            router.fallback_service(spa)
        }
        None => router,
    }
}
