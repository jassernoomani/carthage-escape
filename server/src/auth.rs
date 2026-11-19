//! Host authentication (Argon2 passwords + cookie sessions) and signed WebSocket tokens.

use std::sync::LazyLock;

use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString, rand_core::OsRng},
};
use axum::{
    extract::FromRequestParts,
    http::{HeaderMap, HeaderValue, header, request::Parts},
};
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
use uuid::Uuid;

use crate::{AppState, error::AppError};

pub const SESSION_COOKIE: &str = "carthage_session";
pub const HOST_SESSION_TTL_DAYS: i64 = 7;
pub const WS_TOKEN_TTL_SECS: i64 = 12 * 60 * 60;

// ---------- Passwords ----------

pub async fn hash_password(password: String) -> Result<String, AppError> {
    tokio::task::spawn_blocking(move || {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| AppError::Internal(format!("hashing password: {e}")))
    })
    .await
    .map_err(|e| AppError::Internal(format!("hash task failed: {e}")))?
}

/// Verifies `password` against `stored_hash`. With `None` (unknown email), verifies against a
/// dummy hash anyway so response timing does not reveal which emails are registered.
pub async fn verify_password(
    password: String,
    stored_hash: Option<String>,
) -> Result<bool, AppError> {
    static DUMMY_HASH: LazyLock<Option<String>> = LazyLock::new(|| {
        let salt = SaltString::generate(&mut OsRng);
        Argon2::default()
            .hash_password(b"dummy password for timing", &salt)
            .ok()
            .map(|h| h.to_string())
    });

    tokio::task::spawn_blocking(move || {
        let known = stored_hash.is_some();
        let Some(hash) = stored_hash.or_else(|| DUMMY_HASH.clone()) else {
            return false;
        };
        let Ok(parsed) = PasswordHash::new(&hash) else {
            return false;
        };
        let ok = Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok();
        ok && known
    })
    .await
    .map_err(|e| AppError::Internal(format!("verify task failed: {e}")))
}

// ---------- Host session cookies ----------

/// A new random session token (sent in the cookie) and its hash (stored in the DB).
pub fn new_session_token() -> (String, Vec<u8>) {
    let mut bytes = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let token = URL_SAFE_NO_PAD.encode(bytes);
    let hash = hash_session_token(&token);
    (token, hash)
}

pub fn hash_session_token(token: &str) -> Vec<u8> {
    Sha256::digest(token.as_bytes()).to_vec()
}

pub fn session_cookie(token: &str, secure: bool) -> Result<HeaderValue, AppError> {
    let max_age = HOST_SESSION_TTL_DAYS * 24 * 60 * 60;
    build_cookie(token, max_age, secure)
}

pub fn clear_session_cookie(secure: bool) -> Result<HeaderValue, AppError> {
    build_cookie("", 0, secure)
}

fn build_cookie(value: &str, max_age: i64, secure: bool) -> Result<HeaderValue, AppError> {
    let secure = if secure { "; Secure" } else { "" };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={value}; Path=/; HttpOnly; SameSite=Lax; Max-Age={max_age}{secure}"
    ))
    .map_err(|e| AppError::Internal(format!("building cookie: {e}")))
}

pub fn cookie_value<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
        .filter(|v| !v.is_empty())
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export)]
pub struct HostInfo {
    pub id: Uuid,
    pub email: String,
    pub display_name: String,
}

/// Extractor for handlers that require a logged-in host.
pub struct HostAuth(pub HostInfo);

impl FromRequestParts<AppState> for HostAuth {
    type Rejection = AppError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, AppError> {
        let token = cookie_value(&parts.headers, SESSION_COOKIE).ok_or(AppError::Unauthorized)?;
        let hash = hash_session_token(token);
        let host = sqlx::query_as!(
            HostInfo,
            r#"SELECT h.id, h.email, h.display_name
               FROM host_sessions s JOIN hosts h ON h.id = s.host_id
               WHERE s.token_hash = $1 AND s.expires_at > now()"#,
            hash
        )
        .fetch_optional(&state.db)
        .await?
        .ok_or(AppError::Unauthorized)?;
        Ok(HostAuth(host))
    }
}

// ---------- WebSocket tokens ----------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenKind {
    Player,
    Host,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WsClaims {
    /// Player id (for players) or host id (for hosts).
    pub sub: Uuid,
    /// Game session id.
    pub sid: Uuid,
    /// Room code of that session.
    pub code: String,
    pub kind: TokenKind,
    pub exp: i64,
}

pub struct TokenKeys {
    encoding: EncodingKey,
    decoding: DecodingKey,
}

impl TokenKeys {
    pub fn new(secret: &[u8]) -> Self {
        Self {
            encoding: EncodingKey::from_secret(secret),
            decoding: DecodingKey::from_secret(secret),
        }
    }

    pub fn sign(
        &self,
        sub: Uuid,
        sid: Uuid,
        code: &str,
        kind: TokenKind,
    ) -> Result<String, AppError> {
        let claims = WsClaims {
            sub,
            sid,
            code: code.to_owned(),
            kind,
            exp: chrono::Utc::now().timestamp() + WS_TOKEN_TTL_SECS,
        };
        jsonwebtoken::encode(&Header::default(), &claims, &self.encoding)
            .map_err(|e| AppError::Internal(format!("signing token: {e}")))
    }

    pub fn verify(&self, token: &str) -> Option<WsClaims> {
        jsonwebtoken::decode::<WsClaims>(token, &self.decoding, &Validation::default())
            .ok()
            .map(|data| data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn password_roundtrip() {
        let hash = hash_password("correct horse".into()).await.unwrap();
        assert!(
            verify_password("correct horse".into(), Some(hash.clone()))
                .await
                .unwrap()
        );
        assert!(!verify_password("wrong".into(), Some(hash)).await.unwrap());
        assert!(!verify_password("anything".into(), None).await.unwrap());
    }

    #[test]
    fn token_roundtrip_and_tamper() {
        let keys = TokenKeys::new(b"0123456789abcdef0123456789abcdef");
        let (sub, sid) = (Uuid::new_v4(), Uuid::new_v4());
        let token = keys.sign(sub, sid, "ABC234", TokenKind::Player).unwrap();
        let claims = keys.verify(&token).unwrap();
        assert_eq!(
            (claims.sub, claims.sid, claims.kind),
            (sub, sid, TokenKind::Player)
        );

        let other = TokenKeys::new(b"ffffffffffffffffffffffffffffffff");
        assert!(other.verify(&token).is_none(), "wrong secret must fail");
        assert!(keys.verify("garbage").is_none());
    }

    #[test]
    fn parses_cookie_header() {
        let mut headers = HeaderMap::new();
        headers.insert(
            header::COOKIE,
            HeaderValue::from_static("a=1; carthage_session=xyz; b=2"),
        );
        assert_eq!(cookie_value(&headers, SESSION_COOKIE), Some("xyz"));
        assert_eq!(cookie_value(&headers, "missing"), None);
    }

    #[test]
    fn session_token_hash_is_stable() {
        let (token, hash) = new_session_token();
        assert_eq!(hash, hash_session_token(&token));
        assert_eq!(token.len(), 43);
    }
}
