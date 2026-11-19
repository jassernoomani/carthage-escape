//! Puzzle engine (PROJECT_SPEC.md §5): puzzle *types* are code, puzzle *instances* are data.
//!
//! Every type implements [`PuzzleType`]. The full state (including the answer) stays on the
//! server; each player only ever receives `view_for(state, role)`. [`AnyPuzzle`] wraps all types
//! so a room can hold a heterogeneous chain.
//!
//! Puzzles work with *roles* (seat indexes), not player ids, so the same seed always produces
//! the same puzzle. The room decides which player plays which role (see [`role_for`]).

pub mod lock_and_manual;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize, de::DeserializeOwned};
use ts_rs::TS;

pub use lock_and_manual::LockAndManual;

/// A seat within one puzzle (0 = first role, e.g. "lock holder").
pub type Role = u8;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum ActionOutcome {
    /// Accepted, puzzle not finished yet.
    Progress,
    /// A wrong answer ("not quite!"). Never says *what* was wrong.
    Wrong,
    Solved,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PuzzleError {
    #[error("this action belongs to the other player's role")]
    NotYourRole,
    #[error("this puzzle is already solved")]
    AlreadySolved,
    #[error("invalid action: {0}")]
    Invalid(String),
    #[error("this action is for a different kind of puzzle")]
    WrongPuzzle,
    #[error("invalid puzzle parameters: {0}")]
    BadParams(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

/// A hint, as an i18n key plus parameters (the frontend owns the wording).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export)]
pub struct Hint {
    pub key: String,
    pub params: BTreeMap<String, String>,
}

impl Hint {
    pub fn new(key: &str) -> Self {
        Self {
            key: key.to_owned(),
            params: BTreeMap::new(),
        }
    }

    pub fn with(mut self, name: &str, value: impl ToString) -> Self {
        self.params.insert(name.to_owned(), value.to_string());
        self
    }
}

/// Hints come in three levels: gentle nudge, strong nudge, near-solution.
pub const HINT_LEVELS: u8 = 3;

pub trait PuzzleType {
    /// From the room definition file.
    type Params: DeserializeOwned + Clone;
    /// Full server-side state, including the answer.
    type State: Clone;
    /// What a player can submit.
    type Action: DeserializeOwned;
    /// What one role sees. Must never contain the answer.
    type View: Serialize;

    /// Validates params at content-load time, so a bad room file fails on startup.
    fn validate(params: &Self::Params, players: u8) -> Result<(), PuzzleError>;
    /// Builds a puzzle. Deterministic: the same params, seed and player count give the same state.
    fn init(params: &Self::Params, seed: u64, players: u8) -> Result<Self::State, PuzzleError>;
    fn view_for(state: &Self::State, role: Role) -> Self::View;
    fn apply(
        state: &mut Self::State,
        role: Role,
        action: Self::Action,
    ) -> Result<ActionOutcome, PuzzleError>;
    /// `level` is 1..=HINT_LEVELS.
    fn hint(state: &Self::State, level: u8) -> Option<Hint>;
    fn is_solved(state: &Self::State) -> bool;
}

/// A parsed puzzle entry from a room file.
#[derive(Debug, Clone, PartialEq)]
pub enum PuzzleSpec {
    LockAndManual(lock_and_manual::Params),
}

impl PuzzleSpec {
    /// Parses and validates a `{type, params}` entry. `Ok(None)` means the type is known from the
    /// spec but not implemented yet (temporary until M3, when all MVP types exist).
    pub fn parse(
        kind: &str,
        params: &serde_json::Value,
        players: u8,
    ) -> Result<Option<Self>, String> {
        fn typed<T: DeserializeOwned>(params: &serde_json::Value) -> Result<T, String> {
            serde_json::from_value(params.clone()).map_err(|e| e.to_string())
        }
        let spec = match kind {
            "lock_and_manual" => {
                let p: lock_and_manual::Params = typed(params)?;
                LockAndManual::validate(&p, players).map_err(|e| e.to_string())?;
                PuzzleSpec::LockAndManual(p)
            }
            "blind_maze" | "translator" | "balance_scale" | "timeline" => return Ok(None),
            other => return Err(format!("unknown puzzle type {other:?}")),
        };
        Ok(Some(spec))
    }
}

/// One live puzzle of any type.
#[derive(Debug, Clone)]
pub enum AnyPuzzle {
    LockAndManual(lock_and_manual::State),
}

/// What one player sees of the current puzzle, tagged by puzzle kind.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "kind", content = "view", rename_all = "snake_case")]
#[ts(export)]
pub enum PuzzleView {
    LockAndManual(lock_and_manual::View),
}

/// A player's action, tagged by the puzzle kind it targets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "kind", content = "action", rename_all = "snake_case")]
#[ts(export)]
pub enum PuzzleAction {
    LockAndManual(lock_and_manual::Action),
}

impl AnyPuzzle {
    pub fn init(spec: &PuzzleSpec, seed: u64, players: u8) -> Result<Self, PuzzleError> {
        Ok(match spec {
            PuzzleSpec::LockAndManual(p) => {
                AnyPuzzle::LockAndManual(LockAndManual::init(p, seed, players)?)
            }
        })
    }

    pub fn view_for(&self, role: Role) -> PuzzleView {
        match self {
            AnyPuzzle::LockAndManual(s) => {
                PuzzleView::LockAndManual(LockAndManual::view_for(s, role))
            }
        }
    }

    pub fn apply(
        &mut self,
        role: Role,
        action: PuzzleAction,
    ) -> Result<ActionOutcome, PuzzleError> {
        match (self, action) {
            (AnyPuzzle::LockAndManual(s), PuzzleAction::LockAndManual(a)) => {
                LockAndManual::apply(s, role, a)
            }
        }
    }

    pub fn hint(&self, level: u8) -> Option<Hint> {
        match self {
            AnyPuzzle::LockAndManual(s) => LockAndManual::hint(s, level),
        }
    }

    pub fn is_solved(&self) -> bool {
        match self {
            AnyPuzzle::LockAndManual(s) => LockAndManual::is_solved(s),
        }
    }
}

/// Derives an independent seed for puzzle `index` from the session seed (SplitMix64 mixing),
/// so puzzles in one session differ but are reproducible.
pub fn puzzle_seed(session_seed: u64, index: usize) -> u64 {
    let mut z = session_seed.wrapping_add((index as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Which puzzle role a player gets: roles rotate along the chain so the same kid isn't always
/// the one holding the lock.
pub fn role_for(player_role: u8, puzzle_index: usize, players: u8) -> Role {
    let players = players.max(1);
    let shift = (puzzle_index % usize::from(players)) as u8;
    (player_role + shift) % players
}

#[cfg(test)]
pub(crate) mod test_support {
    //! Shared checks every puzzle type must pass (spec §5.3: "no secret in any view").
    use super::*;

    /// Serializes every role's view and asserts none of the `secrets` appears in it.
    pub fn assert_views_hide(puzzle: &AnyPuzzle, players: u8, secrets: &[String]) {
        for role in 0..players {
            let json = serde_json::to_string(&puzzle.view_for(role)).expect("view serializes");
            for secret in secrets {
                assert!(
                    !json.contains(secret.as_str()),
                    "role {role}'s view leaks secret {secret:?}: {json}"
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn puzzle_seeds_are_stable_and_distinct() {
        assert_eq!(puzzle_seed(42, 0), puzzle_seed(42, 0));
        assert_ne!(puzzle_seed(42, 0), puzzle_seed(42, 1));
        assert_ne!(puzzle_seed(42, 0), puzzle_seed(43, 0));
    }

    #[test]
    fn roles_rotate_along_the_chain() {
        // Two players: seat 0 holds the lock on puzzles 0, 2, 4...; seat 1 on 1, 3...
        assert_eq!(
            (0..4).map(|i| role_for(0, i, 2)).collect::<Vec<_>>(),
            [0, 1, 0, 1]
        );
        assert_eq!(
            (0..4).map(|i| role_for(1, i, 2)).collect::<Vec<_>>(),
            [1, 0, 1, 0]
        );
    }

    #[test]
    fn spec_parsing() {
        let ok = serde_json::json!({"digits": 3, "rule": "sum_by_color", "difficulty": "easy"});
        assert!(matches!(
            PuzzleSpec::parse("lock_and_manual", &ok, 2),
            Ok(Some(_))
        ));
        assert_eq!(
            PuzzleSpec::parse("translator", &ok, 2),
            Ok(None),
            "known but not implemented yet"
        );
        assert!(PuzzleSpec::parse("nonsense", &ok, 2).is_err());
        let bad = serde_json::json!({"digits": 9, "rule": "sum_by_color", "difficulty": "easy"});
        assert!(PuzzleSpec::parse("lock_and_manual", &bad, 2).is_err());
        let typo = serde_json::json!({"digit": 3, "rule": "sum_by_color", "difficulty": "easy"});
        assert!(PuzzleSpec::parse("lock_and_manual", &typo, 2).is_err());
    }
}
