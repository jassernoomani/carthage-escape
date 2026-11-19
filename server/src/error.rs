use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};
use serde::Serialize;
use ts_rs::TS;

/// Machine-readable error code. The frontend translates these; `message` is for developers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ApiErrorCode {
    Validation,
    InvalidCredentials,
    EmailTaken,
    Unauthorized,
    NotFound,
    RoomNotFound,
    RoomFull,
    NicknameInvalid,
    NicknameTaken,
    GameAlreadyStarted,
    TooManyRequests,
    Internal,
}

#[derive(Debug, Serialize, TS)]
#[ts(export)]
pub struct ApiErrorBody {
    pub code: ApiErrorCode,
    pub message: String,
}

/// Error type returned by HTTP handlers. Internal details are logged, never sent to clients.
#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("validation failed: {0}")]
    Validation(String),
    #[error("invalid email or password")]
    InvalidCredentials,
    #[error("email already registered")]
    EmailTaken,
    #[error("not logged in")]
    Unauthorized,
    #[error("not found")]
    NotFound,
    #[error("room not found")]
    RoomNotFound,
    #[error("room is full")]
    RoomFull,
    #[error("invalid nickname: {0}")]
    NicknameInvalid(String),
    #[error("nickname already taken in this room")]
    NicknameTaken,
    #[error("game already started")]
    GameAlreadyStarted,
    #[error("too many requests")]
    TooManyRequests,
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),
    #[error("internal error: {0}")]
    Internal(String),
}

impl AppError {
    fn status_and_code(&self) -> (StatusCode, ApiErrorCode) {
        use ApiErrorCode as C;
        match self {
            AppError::Validation(_) => (StatusCode::BAD_REQUEST, C::Validation),
            AppError::InvalidCredentials => (StatusCode::UNAUTHORIZED, C::InvalidCredentials),
            AppError::EmailTaken => (StatusCode::CONFLICT, C::EmailTaken),
            AppError::Unauthorized => (StatusCode::UNAUTHORIZED, C::Unauthorized),
            AppError::NotFound => (StatusCode::NOT_FOUND, C::NotFound),
            AppError::RoomNotFound => (StatusCode::NOT_FOUND, C::RoomNotFound),
            AppError::RoomFull => (StatusCode::CONFLICT, C::RoomFull),
            AppError::NicknameInvalid(_) => (StatusCode::BAD_REQUEST, C::NicknameInvalid),
            AppError::NicknameTaken => (StatusCode::CONFLICT, C::NicknameTaken),
            AppError::GameAlreadyStarted => (StatusCode::CONFLICT, C::GameAlreadyStarted),
            AppError::TooManyRequests => (StatusCode::TOO_MANY_REQUESTS, C::TooManyRequests),
            AppError::Database(_) | AppError::Internal(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, C::Internal)
            }
        }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let (status, code) = self.status_and_code();
        let message = if status.is_server_error() {
            tracing::error!(err = %self, "request failed");
            "Something went wrong".to_owned()
        } else {
            self.to_string()
        };
        (status, Json(ApiErrorBody { code, message })).into_response()
    }
}

/// True if the error is a Postgres unique-constraint violation.
pub fn is_unique_violation(err: &sqlx::Error) -> bool {
    err.as_database_error()
        .is_some_and(|e| e.is_unique_violation())
}
