//! Queries used outside request handlers (room actors, startup).

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use crate::game::protocol::RoomStatus;

pub async fn insert_player(
    db: &PgPool,
    session_id: Uuid,
    nickname: &str,
    role_index: u8,
) -> Result<Uuid, sqlx::Error> {
    sqlx::query_scalar!(
        "INSERT INTO session_players (session_id, nickname, role_index) VALUES ($1, $2, $3) RETURNING id",
        session_id,
        nickname,
        i16::from(role_index)
    )
    .fetch_one(db)
    .await
}

pub async fn delete_player(db: &PgPool, player_id: Uuid) -> Result<(), sqlx::Error> {
    sqlx::query!("DELETE FROM session_players WHERE id = $1", player_id)
        .execute(db)
        .await?;
    Ok(())
}

/// Saves a status change, stamping `started_at` / `ended_at` the first time they apply.
pub async fn set_session_status(
    db: &PgPool,
    session_id: Uuid,
    status: RoomStatus,
    at_ms: i64,
) -> Result<(), sqlx::Error> {
    let at = DateTime::<Utc>::from_timestamp_millis(at_ms).unwrap_or_else(Utc::now);
    let started = status == RoomStatus::Running;
    let ended = !status.is_active();
    sqlx::query!(
        r#"UPDATE game_sessions SET
             status = $2,
             started_at = CASE WHEN $3 THEN COALESCE(started_at, $5) ELSE started_at END,
             ended_at   = CASE WHEN $4 THEN COALESCE(ended_at, $5) ELSE ended_at END
           WHERE id = $1"#,
        session_id,
        status as RoomStatus,
        started,
        ended,
        at
    )
    .execute(db)
    .await?;
    Ok(())
}

/// Live room state is in memory only, so sessions left active by a previous run (crash or
/// restart) can't be resumed yet. Mark them abandoned; returns how many there were.
pub async fn abandon_stale_sessions(db: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!(
        r#"UPDATE game_sessions SET status = 'abandoned', ended_at = now()
           WHERE status IN ('lobby', 'running', 'paused')"#
    )
    .execute(db)
    .await?;
    Ok(result.rows_affected())
}

pub async fn purge_expired_host_sessions(db: &PgPool) -> Result<u64, sqlx::Error> {
    let result = sqlx::query!("DELETE FROM host_sessions WHERE expires_at <= now()")
        .execute(db)
        .await?;
    Ok(result.rows_affected())
}
