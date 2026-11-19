//! Room definitions loaded from `content/rooms/*.yaml` and synced into `room_templates`.
//! Puzzle params are kept as raw JSON for now; typed validation arrives with the puzzle engine (M3).

use std::path::Path;

use serde::{Deserialize, Serialize};
use sqlx::PgPool;

use crate::game::puzzles::{AnyPuzzle, PuzzleSpec, puzzle_seed};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RoomDefinition {
    pub id: String,
    pub title: String,
    pub theme: String,
    pub players: u8,
    pub time_limit_secs: u32,
    pub hint_penalty_secs: u32,
    pub puzzles: Vec<PuzzleDefinition>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PuzzleDefinition {
    #[serde(rename = "type")]
    pub kind: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, thiserror::Error)]
pub enum ContentError {
    #[error("reading {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("parsing {path}: {source}")]
    Parse {
        path: String,
        source: serde_norway::Error,
    },
    #[error("invalid room {path}: {reason}")]
    Invalid { path: String, reason: String },
    #[error("saving room templates: {0}")]
    Database(#[from] sqlx::Error),
}

impl RoomDefinition {
    pub fn parse(yaml: &str, path: &str) -> Result<Self, ContentError> {
        let def: RoomDefinition =
            serde_norway::from_str(yaml).map_err(|source| ContentError::Parse {
                path: path.to_owned(),
                source,
            })?;
        def.validate().map_err(|reason| ContentError::Invalid {
            path: path.to_owned(),
            reason,
        })?;
        Ok(def)
    }

    fn validate(&self) -> Result<(), String> {
        let id_ok = !self.id.is_empty()
            && self
                .id
                .chars()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
        if !id_ok {
            return Err("id must be non-empty lowercase letters, digits and dashes".into());
        }
        if self.title.trim().is_empty() {
            return Err("title is empty".into());
        }
        if self.players != 2 {
            return Err("only 2-player rooms are supported in v1".into());
        }
        if self.time_limit_secs == 0 {
            return Err("time_limit_secs must be positive".into());
        }
        if self.puzzles.is_empty() {
            return Err("a room needs at least one puzzle".into());
        }
        // Parses every puzzle's params now, so a typo fails at startup, not mid-game.
        if self.playable_puzzles()?.is_empty() {
            return Err("none of this room's puzzle types are implemented yet".into());
        }
        Ok(())
    }

    /// The puzzles that can be played, in order. Types from the spec that aren't implemented yet
    /// are skipped (temporary until all MVP puzzle types exist in M3); unknown types or bad
    /// params are errors.
    pub fn playable_puzzles(&self) -> Result<Vec<PuzzleSpec>, String> {
        let mut specs = Vec::new();
        for (i, p) in self.puzzles.iter().enumerate() {
            match PuzzleSpec::parse(&p.kind, &p.params, self.players)
                .map_err(|e| format!("puzzle #{} ({}): {e}", i + 1, p.kind))?
            {
                Some(spec) => specs.push(spec),
                None => {
                    tracing::debug!(room = %self.id, kind = %p.kind, "puzzle type not implemented yet; skipped")
                }
            }
        }
        Ok(specs)
    }

    /// Builds the live puzzle chain for a session, deterministically from its seed.
    pub fn build_puzzles(&self, session_seed: u64) -> Result<Vec<AnyPuzzle>, String> {
        self.playable_puzzles()?
            .iter()
            .enumerate()
            .map(|(i, spec)| {
                AnyPuzzle::init(spec, puzzle_seed(session_seed, i), self.players)
                    .map_err(|e| e.to_string())
            })
            .collect()
    }
}

/// Loads every `rooms/*.yaml` under `content_dir`.
pub fn load_rooms(content_dir: &Path) -> Result<Vec<RoomDefinition>, ContentError> {
    let dir = content_dir.join("rooms");
    let io_err = |source| ContentError::Io {
        path: dir.display().to_string(),
        source,
    };
    let mut paths: Vec<_> = std::fs::read_dir(&dir)
        .map_err(io_err)?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.extension()
                .is_some_and(|ext| ext == "yaml" || ext == "yml")
        })
        .collect();
    paths.sort();

    let mut rooms: Vec<RoomDefinition> = Vec::with_capacity(paths.len());
    for path in paths {
        let shown = path.display().to_string();
        let yaml = std::fs::read_to_string(&path).map_err(|source| ContentError::Io {
            path: shown.clone(),
            source,
        })?;
        let room = RoomDefinition::parse(&yaml, &shown)?;
        if rooms.iter().any(|r| r.id == room.id) {
            return Err(ContentError::Invalid {
                path: shown,
                reason: format!("duplicate id {}", room.id),
            });
        }
        rooms.push(room);
    }
    Ok(rooms)
}

/// Upserts built-in room templates so the DB matches the YAML files.
pub async fn sync_rooms(db: &PgPool, content_dir: &Path) -> Result<usize, ContentError> {
    let rooms = load_rooms(content_dir)?;
    for room in &rooms {
        let definition = serde_json::to_value(room).map_err(|e| ContentError::Invalid {
            path: room.id.clone(),
            reason: e.to_string(),
        })?;
        sqlx::query!(
            r#"INSERT INTO room_templates (id, title, definition)
               VALUES ($1, $2, $3)
               ON CONFLICT (id) DO UPDATE SET title = EXCLUDED.title, definition = EXCLUDED.definition"#,
            room.id,
            room.title,
            definition
        )
        .execute(db)
        .await?;
    }
    Ok(rooms.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn content_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../content")
    }

    #[test]
    fn bundled_rooms_load() {
        let rooms = load_rooms(&content_dir()).unwrap();
        let carthage = rooms.iter().find(|r| r.id == "carthage-01").unwrap();
        assert_eq!(carthage.players, 2);
        assert_eq!(carthage.puzzles.len(), 5);
        assert_eq!(carthage.puzzles[0].kind, "lock_and_manual");
        assert_eq!(carthage.puzzles[0].params["digits"], 3);
    }

    #[test]
    fn rejects_invalid_rooms() {
        let base =
            "id: x\ntitle: T\ntheme: t\nplayers: 2\ntime_limit_secs: 60\nhint_penalty_secs: 0\n";
        let lock = "puzzles: [{type: lock_and_manual, params: {digits: 3, rule: sum_by_color, difficulty: easy}}]";
        assert!(RoomDefinition::parse(&format!("{base}{lock}"), "t").is_ok());

        let reject = |yaml: String, why: &str| {
            assert!(
                RoomDefinition::parse(&yaml, "t").is_err(),
                "should reject: {why}"
            );
        };
        reject(format!("{base}puzzles: []"), "no puzzles");
        reject(format!("{base}{lock}\nextra: 1"), "unknown field");
        let three = base.replace("players: 2", "players: 3");
        reject(format!("{three}{lock}"), "3 players");
        reject(
            format!("{base}puzzles: [{{type: made_up}}]"),
            "unknown puzzle type",
        );
        reject(
            format!(
                "{base}puzzles: [{{type: lock_and_manual, params: {{digits: 7, rule: sum_by_color, difficulty: easy}}}}]"
            ),
            "bad params",
        );
        reject(
            format!("{base}puzzles: [{{type: translator}}]"),
            "nothing playable yet",
        );
    }
}
