use axum::{Json, extract::State, http::StatusCode};
use chrono::{DateTime, Utc};
use rand::Rng;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

use super::extract::{ApiJson, ApiPath};
use crate::{
    AppState,
    auth::{HostAuth, TokenKind},
    content::RoomDefinition,
    error::{AppError, is_unique_violation},
    game::{
        protocol::RoomStatus,
        room::{Input, RoomMeta},
    },
};

/// No 0/O, 1/I/L: codes are read aloud and typed by kids.
const CODE_ALPHABET: &[u8] = b"ABCDEFGHJKMNPQRSTUVWXYZ23456789";
pub const CODE_LEN: usize = 6;
const CODE_ATTEMPTS: usize = 10;

pub fn generate_code() -> String {
    let mut rng = rand::thread_rng();
    (0..CODE_LEN)
        .map(|_| char::from(CODE_ALPHABET[rng.gen_range(0..CODE_ALPHABET.len())]))
        .collect()
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct RoomTemplateInfo {
    pub id: String,
    pub title: String,
    pub theme: String,
    pub players: u8,
    pub time_limit_secs: u32,
    pub puzzle_count: u32,
}

pub async fn list_rooms(
    State(state): State<AppState>,
    HostAuth(_host): HostAuth,
) -> Result<Json<Vec<RoomTemplateInfo>>, AppError> {
    let rows = sqlx::query!("SELECT definition FROM room_templates ORDER BY id")
        .fetch_all(&state.db)
        .await?;
    let rooms = rows
        .into_iter()
        .filter_map(|r| serde_json::from_value::<RoomDefinition>(r.definition).ok())
        .map(|d| RoomTemplateInfo {
            puzzle_count: u32::try_from(d.puzzles.len()).unwrap_or(u32::MAX),
            id: d.id,
            title: d.title,
            theme: d.theme,
            players: d.players,
            time_limit_secs: d.time_limit_secs,
        })
        .collect();
    Ok(Json(rooms))
}

#[derive(Deserialize, TS)]
#[ts(export)]
pub struct CreateSessionRequest {
    pub template_id: String,
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct CreatedSession {
    pub id: Uuid,
    pub code: String,
    pub host_token: String,
}

pub async fn create_session(
    State(state): State<AppState>,
    HostAuth(host): HostAuth,
    ApiJson(req): ApiJson<CreateSessionRequest>,
) -> Result<(StatusCode, Json<CreatedSession>), AppError> {
    let template = sqlx::query!(
        "SELECT definition FROM room_templates WHERE id = $1",
        req.template_id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;
    let definition: RoomDefinition = serde_json::from_value(template.definition)
        .map_err(|e| AppError::Internal(format!("bad stored room definition: {e}")))?;

    let seed: i64 = rand::thread_rng().r#gen();
    let mut created = None;
    for _ in 0..CODE_ATTEMPTS {
        let code = generate_code();
        let result = sqlx::query_scalar!(
            "INSERT INTO game_sessions (code, host_id, template_id, seed) VALUES ($1, $2, $3, $4) RETURNING id",
            code,
            host.id,
            definition.id,
            seed
        )
        .fetch_one(&state.db)
        .await;
        match result {
            Ok(id) => {
                created = Some((id, code));
                break;
            }
            Err(e) if is_unique_violation(&e) => continue, // code in use by an active session
            Err(e) => return Err(e.into()),
        }
    }
    let (id, code) =
        created.ok_or_else(|| AppError::Internal("could not allocate a room code".into()))?;

    state.rooms.spawn(
        RoomMeta {
            session_id: id,
            code: code.clone(),
            title: definition.title,
            players_needed: definition.players,
            time_limit_secs: definition.time_limit_secs,
        },
        state.db.clone(),
    );
    let host_token = state.tokens.sign(host.id, id, &code, TokenKind::Host)?;
    Ok((
        StatusCode::CREATED,
        Json(CreatedSession {
            id,
            code,
            host_token,
        }),
    ))
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct SessionSummary {
    pub id: Uuid,
    pub code: String,
    pub template_title: String,
    pub status: RoomStatus,
    pub player_count: u32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub ended_at: Option<DateTime<Utc>>,
}

pub async fn list_sessions(
    State(state): State<AppState>,
    HostAuth(host): HostAuth,
) -> Result<Json<Vec<SessionSummary>>, AppError> {
    let rows = sqlx::query!(
        r#"SELECT s.id, s.code, t.title AS template_title, s.status AS "status: RoomStatus",
                  s.created_at, s.started_at, s.ended_at,
                  (SELECT count(*) FROM session_players p WHERE p.session_id = s.id) AS "player_count!"
           FROM game_sessions s JOIN room_templates t ON t.id = s.template_id
           WHERE s.host_id = $1
           ORDER BY s.created_at DESC
           LIMIT 100"#,
        host.id
    )
    .fetch_all(&state.db)
    .await?;
    Ok(Json(
        rows.into_iter()
            .map(|r| SessionSummary {
                id: r.id,
                code: r.code,
                template_title: r.template_title,
                status: r.status,
                player_count: u32::try_from(r.player_count).unwrap_or(0),
                created_at: r.created_at,
                started_at: r.started_at,
                ended_at: r.ended_at,
            })
            .collect(),
    ))
}

#[derive(Serialize, TS)]
#[ts(export)]
pub struct SessionDetail {
    pub id: Uuid,
    pub code: String,
    pub template_title: String,
    pub status: RoomStatus,
    /// Token for the host dashboard's WebSocket; `None` once the room has ended.
    pub host_token: Option<String>,
}

pub async fn get_session(
    State(state): State<AppState>,
    HostAuth(host): HostAuth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<Json<SessionDetail>, AppError> {
    let row = sqlx::query!(
        r#"SELECT s.code, t.title AS template_title, s.status AS "status: RoomStatus"
           FROM game_sessions s JOIN room_templates t ON t.id = s.template_id
           WHERE s.id = $1 AND s.host_id = $2"#,
        id,
        host.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    let host_token = match state.rooms.get_session(&row.code, id) {
        Some(_) => Some(state.tokens.sign(host.id, id, &row.code, TokenKind::Host)?),
        None => None,
    };
    Ok(Json(SessionDetail {
        id,
        code: row.code,
        template_title: row.template_title,
        status: row.status,
        host_token,
    }))
}

pub async fn delete_session(
    State(state): State<AppState>,
    HostAuth(host): HostAuth,
    ApiPath(id): ApiPath<Uuid>,
) -> Result<StatusCode, AppError> {
    let code = sqlx::query_scalar!(
        "DELETE FROM game_sessions WHERE id = $1 AND host_id = $2 RETURNING code",
        id,
        host.id
    )
    .fetch_optional(&state.db)
    .await?
    .ok_or(AppError::NotFound)?;

    if let Some(room) = state.rooms.get_session(&code, id) {
        room.send(Input::Close).await;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_use_unambiguous_alphabet() {
        for _ in 0..200 {
            let code = generate_code();
            assert_eq!(code.len(), CODE_LEN);
            assert!(code.bytes().all(|b| CODE_ALPHABET.contains(&b)), "{code}");
            assert!(!code.contains(['0', 'O', '1', 'I', 'L']));
        }
    }
}
