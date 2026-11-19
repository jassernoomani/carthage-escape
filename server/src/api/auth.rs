use axum::{
    Json,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::{
    AppState,
    auth::{self, HOST_SESSION_TTL_DAYS, HostAuth, HostInfo, SESSION_COOKIE},
    error::{AppError, is_unique_violation},
    rate_limit::{Bucket, ClientIp},
};

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    pub display_name: String,
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct MeResponse {
    pub host: HostInfo,
}

const MIN_PASSWORD_CHARS: usize = 8;
const MAX_PASSWORD_CHARS: usize = 128;
const MAX_DISPLAY_NAME_CHARS: usize = 50;

fn normalize_email(email: &str) -> Result<String, AppError> {
    let email = email.trim().to_lowercase();
    let valid = email.len() <= 254
        && email.split_once('@').is_some_and(|(user, domain)| {
            !user.is_empty() && domain.contains('.') && !domain.starts_with('.')
        });
    if valid {
        Ok(email)
    } else {
        Err(AppError::Validation("invalid email address".into()))
    }
}

pub async fn register(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.limiter.check(Bucket::Register, ip)?;

    let email = normalize_email(&req.email)?;
    let password_len = req.password.chars().count();
    if !(MIN_PASSWORD_CHARS..=MAX_PASSWORD_CHARS).contains(&password_len) {
        return Err(AppError::Validation(format!(
            "password must be {MIN_PASSWORD_CHARS}-{MAX_PASSWORD_CHARS} characters"
        )));
    }
    let display_name = req.display_name.trim().to_owned();
    if display_name.is_empty() || display_name.chars().count() > MAX_DISPLAY_NAME_CHARS {
        return Err(AppError::Validation(format!(
            "display name must be 1-{MAX_DISPLAY_NAME_CHARS} characters"
        )));
    }

    let password_hash = auth::hash_password(req.password).await?;
    let host = sqlx::query_as!(
        HostInfo,
        r#"INSERT INTO hosts (email, password_hash, display_name) VALUES ($1, $2, $3)
           RETURNING id, email, display_name"#,
        email,
        password_hash,
        display_name
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        if is_unique_violation(&e) {
            AppError::EmailTaken
        } else {
            e.into()
        }
    })?;

    let headers = start_session(&state, &host).await?;
    Ok((StatusCode::CREATED, headers, Json(MeResponse { host })))
}

pub async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    Json(req): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    state.limiter.check(Bucket::Login, ip)?;

    let email = req.email.trim().to_lowercase();
    let row = sqlx::query!(
        "SELECT id, email, display_name, password_hash FROM hosts WHERE email = $1",
        email
    )
    .fetch_optional(&state.db)
    .await?;

    let stored_hash = row.as_ref().map(|r| r.password_hash.clone());
    if !auth::verify_password(req.password, stored_hash).await? {
        return Err(AppError::InvalidCredentials);
    }
    let row = row.ok_or(AppError::InvalidCredentials)?;
    let host = HostInfo {
        id: row.id,
        email: row.email,
        display_name: row.display_name,
    };

    let headers = start_session(&state, &host).await?;
    Ok((headers, Json(MeResponse { host })))
}

pub async fn logout(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<impl IntoResponse, AppError> {
    if let Some(token) = auth::cookie_value(&headers, SESSION_COOKIE) {
        let hash = auth::hash_session_token(token);
        sqlx::query!("DELETE FROM host_sessions WHERE token_hash = $1", hash)
            .execute(&state.db)
            .await?;
    }
    let mut out = HeaderMap::new();
    out.insert(
        header::SET_COOKIE,
        auth::clear_session_cookie(state.cookie_secure)?,
    );
    Ok((StatusCode::NO_CONTENT, out))
}

pub async fn me(HostAuth(host): HostAuth) -> Json<MeResponse> {
    Json(MeResponse { host })
}

async fn start_session(state: &AppState, host: &HostInfo) -> Result<HeaderMap, AppError> {
    let (token, hash) = auth::new_session_token();
    let expires_at = Utc::now() + Duration::days(HOST_SESSION_TTL_DAYS);
    sqlx::query!(
        "INSERT INTO host_sessions (token_hash, host_id, expires_at) VALUES ($1, $2, $3)",
        hash,
        host.id,
        expires_at
    )
    .execute(&state.db)
    .await?;
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        auth::session_cookie(&token, state.cookie_secure)?,
    );
    Ok(headers)
}

#[cfg(test)]
mod tests {
    use super::normalize_email;

    #[test]
    fn email_normalization() {
        assert_eq!(
            normalize_email("  Teacher@School.TN ").unwrap(),
            "teacher@school.tn"
        );
        for bad in ["", "nope", "@x.com", "a@b", "a@.com"] {
            assert!(normalize_email(bad).is_err(), "{bad:?} should be rejected");
        }
    }
}
