pub mod api;
pub mod config;
pub mod error;

use std::path::Path;

use axum::Router;
use sqlx::PgPool;
use tower_http::{
    services::{ServeDir, ServeFile},
    trace::TraceLayer,
};

/// Shared state handed to every request handler.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
}

/// Builds the full HTTP application.
///
/// When `static_dir` is set (production), the built frontend is served from it,
/// with unknown paths falling back to `index.html` so client-side routing works.
pub fn app(state: AppState, static_dir: Option<&Path>) -> Router {
    let router = Router::new()
        .nest("/api", api::router())
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
