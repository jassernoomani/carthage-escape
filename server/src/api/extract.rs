//! Drop-in replacements for axum's `Json` / `Path` extractors whose rejections are `AppError`s,
//! so malformed input gets the same `{code, message}` JSON body as every other API error.

use axum::{
    extract::rejection::{JsonRejection, PathRejection},
    extract::{FromRequest, FromRequestParts},
};

use crate::error::AppError;

#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(AppError))]
pub struct ApiJson<T>(pub T);

#[derive(FromRequestParts)]
#[from_request(via(axum::extract::Path), rejection(AppError))]
pub struct ApiPath<T>(pub T);

impl From<JsonRejection> for AppError {
    fn from(rejection: JsonRejection) -> Self {
        AppError::Validation(rejection.body_text())
    }
}

impl From<PathRejection> for AppError {
    fn from(rejection: PathRejection) -> Self {
        AppError::Validation(rejection.body_text())
    }
}
