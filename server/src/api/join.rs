use axum::{Json, extract::State};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::extract::ApiJson;
use crate::{
    AppState,
    auth::TokenKind,
    error::AppError,
    game::{actor::JoinFailure, room::JoinError},
    nickname,
    rate_limit::{Bucket, ClientIp},
};

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct JoinRequest {
    pub code: String,
    pub nickname: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct JoinResponse {
    pub player_token: String,
    pub code: String,
    pub nickname: String,
}

/// `POST /api/join`: a kid enters a room code and a nickname, and gets a player token.
pub async fn join(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ApiJson(req): ApiJson<JoinRequest>,
) -> Result<Json<JoinResponse>, AppError> {
    state.limiter.check(Bucket::Join, ip)?;

    let code: String = req
        .code
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .to_uppercase();
    let nickname =
        nickname::validate(&req.nickname).map_err(|e| AppError::NicknameInvalid(e.to_string()))?;
    let room = state.rooms.get(&code).ok_or(AppError::RoomNotFound)?;

    let joined = room.join(nickname.clone()).await.map_err(|e| match e {
        JoinFailure::Rejected(JoinError::RoomFull) => AppError::RoomFull,
        JoinFailure::Rejected(JoinError::NicknameTaken) => AppError::NicknameTaken,
        JoinFailure::Rejected(JoinError::GameAlreadyStarted) => AppError::GameAlreadyStarted,
        JoinFailure::Database(e) => AppError::Database(e),
        JoinFailure::RoomGone => AppError::RoomNotFound,
    })?;

    let player_token =
        state
            .tokens
            .sign(joined.player_id, room.session_id, &code, TokenKind::Player)?;
    Ok(Json(JoinResponse {
        player_token,
        code,
        nickname: joined.nickname,
    }))
}
