mod auth;
mod health;
mod join;
mod sessions;

use axum::{
    Router,
    routing::{get, post},
};

use crate::{AppState, error::AppError};

pub use health::{Health, ServiceStatus};

/// Routes mounted under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/auth/logout", post(auth::logout))
        .route("/me", get(auth::me))
        .route("/rooms", get(sessions::list_rooms))
        .route(
            "/sessions",
            get(sessions::list_sessions).post(sessions::create_session),
        )
        .route(
            "/sessions/{id}",
            get(sessions::get_session).delete(sessions::delete_session),
        )
        .route("/join", post(join::join))
        // Unknown /api paths get a JSON 404 instead of the SPA's index.html.
        .fallback(|| async { AppError::NotFound })
}
