use axum::{Json, extract::State, http::StatusCode};
use serde::Serialize;
use ts_rs::TS;

use crate::AppState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ServiceStatus {
    Ok,
    Degraded,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct Health {
    pub status: ServiceStatus,
    pub version: String,
    pub database: ServiceStatus,
}

/// `GET /api/health`: 200 when everything works, 503 when the database is unreachable.
pub async fn health(State(state): State<AppState>) -> (StatusCode, Json<Health>) {
    let db_ok = sqlx::query("SELECT 1").execute(&state.db).await.is_ok();
    let (code, status) = if db_ok {
        (StatusCode::OK, ServiceStatus::Ok)
    } else {
        (StatusCode::SERVICE_UNAVAILABLE, ServiceStatus::Degraded)
    };

    (
        code,
        Json(Health {
            status,
            version: env!("CARGO_PKG_VERSION").to_owned(),
            database: status,
        }),
    )
}
