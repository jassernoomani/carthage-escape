mod health;

use axum::{Router, routing::get};

use crate::{AppState, error::AppError};

pub use health::{Health, ServiceStatus};

/// Routes mounted under `/api`.
pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(health::health))
        // Unknown /api paths get a JSON 404 instead of the SPA's index.html.
        .fallback(|| async { AppError::NotFound })
}
