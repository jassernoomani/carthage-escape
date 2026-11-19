//! Lock & Manual: one player holds a lock covered in colored number tiles; the other holds the
//! manual explaining how each digit of the code is computed from those tiles.
//!
//! - Lock holder (role 0) sees the tiles and enters the code, but not the rules.
//! - Manual reader (role 1) sees the rules ("Digit 1: add up all the RED numbers"), but not the
//!   numbers.
//!
//! Neither can open the lock alone: they have to describe, listen and add up together.
//! Teaches arithmetic (sums, counting, comparing) and reading instructions.

use rand::{Rng, SeedableRng, seq::SliceRandom};
use rand_chacha::ChaCha8Rng;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::{ActionOutcome, Difficulty, Hint, PuzzleError, PuzzleType, Role};

pub const ROLE_LOCK: Role = 0;
pub const ROLE_MANUAL: Role = 1;
/// Wrong attempts shown to both players (most recent last).
const ATTEMPTS_SHOWN: usize = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename = "TileColor")]
pub enum TileColor {
    Red,
    Blue,
    Green,
    Yellow,
    Purple,
    Orange,
}

const ALL_COLORS: [TileColor; 6] = [
    TileColor::Red,
    TileColor::Blue,
    TileColor::Green,
    TileColor::Yellow,
    TileColor::Purple,
    TileColor::Orange,
];

/// Which kinds of rules the manual may use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleSet {
    /// Every digit is the sum of one color's numbers.
    SumByColor,
    /// Sums, counts, largest and smallest.
    Mixed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Params {
    /// Length of the code, 1..=4.
    pub digits: u8,
    pub rule: RuleSet,
    pub difficulty: Difficulty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
#[ts(export, rename = "LockRuleKind")]
pub enum RuleKind {
    /// Add up all the numbers of the color.
    Sum,
    /// How many tiles of the color there are.
    Count,
    /// The biggest number of the color.
    Largest,
    /// The smallest number of the color.
    Smallest,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "LockRule")]
pub struct Rule {
    /// 1-based position of the digit this rule computes.
    pub digit: u8,
    pub kind: RuleKind,
    pub color: TileColor,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export, rename = "LockTile")]
pub struct Tile {
    pub color: TileColor,
    pub number: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    digits: u8,
    tiles: Vec<Tile>,
    rules: Vec<Rule>,
    /// The answer. Never leaves the server.
    code: Vec<u8>,
    wrong_attempts: Vec<Vec<u8>>,
    total_wrong: u32,
    solved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(tag = "type", rename_all = "snake_case")]
#[ts(export, rename = "LockAction")]
pub enum Action {
    /// The lock holder tries a code.
    Submit { code: Vec<u8> },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "role", rename_all = "snake_case")]
#[ts(export, rename = "LockView")]
pub enum View {
    Lock {
        tiles: Vec<Tile>,
        digits: u8,
        /// Recent wrong codes, so both players can talk about what was tried.
        wrong_attempts: Vec<Vec<u8>>,
        solved: bool,
    },
    Manual {
        rules: Vec<Rule>,
        digits: u8,
        wrong_attempts: Vec<Vec<u8>>,
        solved: bool,
    },
}

pub struct LockAndManual;

impl RuleKind {
    fn eval(self, color: TileColor, tiles: &[Tile]) -> u8 {
        let numbers = tiles.iter().filter(|t| t.color == color).map(|t| t.number);
        match self {
            RuleKind::Sum => numbers.sum(),
            RuleKind::Count => u8::try_from(numbers.count()).unwrap_or(u8::MAX),
            RuleKind::Largest => numbers.max().unwrap_or(0),
            RuleKind::Smallest => numbers.min().unwrap_or(0),
        }
    }
}

/// How the lock is laid out for a difficulty: tiles per color, highest number, decoy colors.
/// Every result fits in one digit 1..=9: max sum is 2×4 = 8 (easy) or 3×3 = 9 (others).
fn layout(difficulty: Difficulty) -> (std::ops::RangeInclusive<usize>, u8, usize) {
    match difficulty {
        Difficulty::Easy => (1..=2, 4, 1),
        Difficulty::Medium => (2..=3, 3, 1),
        Difficulty::Hard => (2..=3, 3, 2),
    }
}

impl PuzzleType for LockAndManual {
    type Params = Params;
    type State = State;
    type Action = Action;
    type View = View;

    fn validate(params: &Params, players: u8) -> Result<(), PuzzleError> {
        if !(1..=4).contains(&params.digits) {
            return Err(PuzzleError::BadParams(
                "digits must be between 1 and 4".into(),
            ));
        }
        if players < 2 {
            return Err(PuzzleError::BadParams("needs at least 2 players".into()));
        }
        Ok(())
    }

    fn init(params: &Params, seed: u64, players: u8) -> Result<State, PuzzleError> {
        Self::validate(params, players)?;
        let mut rng = ChaCha8Rng::seed_from_u64(seed);
        let digits = usize::from(params.digits);
        let (tiles_per_color, max_number, decoys) = layout(params.difficulty);
        let decoys = decoys.min(ALL_COLORS.len() - digits);

        let mut colors = ALL_COLORS.to_vec();
        colors.shuffle(&mut rng);
        let rule_colors = &colors[..digits];
        let decoy_colors = &colors[digits..digits + decoys];

        let mut tiles = Vec::new();
        for &color in rule_colors.iter().chain(decoy_colors) {
            for _ in 0..rng.gen_range(tiles_per_color.clone()) {
                tiles.push(Tile {
                    color,
                    number: rng.gen_range(1..=max_number),
                });
            }
        }
        tiles.shuffle(&mut rng);

        let kinds: &[RuleKind] = match params.rule {
            RuleSet::SumByColor => &[RuleKind::Sum],
            RuleSet::Mixed => &[
                RuleKind::Sum,
                RuleKind::Count,
                RuleKind::Largest,
                RuleKind::Smallest,
            ],
        };
        let rules: Vec<Rule> = rule_colors
            .iter()
            .zip(1..)
            .map(|(&color, digit)| Rule {
                digit,
                kind: kinds[rng.gen_range(0..kinds.len())],
                color,
            })
            .collect();
        let code = rules.iter().map(|r| r.kind.eval(r.color, &tiles)).collect();

        Ok(State {
            digits: params.digits,
            tiles,
            rules,
            code,
            wrong_attempts: Vec::new(),
            total_wrong: 0,
            solved: false,
        })
    }

    fn view_for(state: &State, role: Role) -> View {
        let wrong_attempts = state.wrong_attempts.clone();
        if role == ROLE_LOCK {
            View::Lock {
                tiles: state.tiles.clone(),
                digits: state.digits,
                wrong_attempts,
                solved: state.solved,
            }
        } else {
            // Role 1, and any extra players (3-4 player rooms later), read the manual.
            View::Manual {
                rules: state.rules.clone(),
                digits: state.digits,
                wrong_attempts,
                solved: state.solved,
            }
        }
    }

    fn apply(state: &mut State, role: Role, action: Action) -> Result<ActionOutcome, PuzzleError> {
        match action {
            Action::Submit { code } => {
                if role != ROLE_LOCK {
                    return Err(PuzzleError::NotYourRole);
                }
                if state.solved {
                    return Err(PuzzleError::AlreadySolved);
                }
                if code.len() != usize::from(state.digits) || code.iter().any(|d| *d > 9) {
                    return Err(PuzzleError::Invalid(format!(
                        "the code has {} digits from 0 to 9",
                        state.digits
                    )));
                }
                if code == state.code {
                    state.solved = true;
                    return Ok(ActionOutcome::Solved);
                }
                state.total_wrong += 1;
                state.wrong_attempts.push(code);
                if state.wrong_attempts.len() > ATTEMPTS_SHOWN {
                    state.wrong_attempts.remove(0);
                }
                Ok(ActionOutcome::Wrong)
            }
        }
    }

    fn hint(state: &State, level: u8) -> Option<Hint> {
        let first = state.rules.first()?;
        match level {
            1 => Some(Hint::new("lock_and_manual.hint_1")),
            2 => Some(
                Hint::new("lock_and_manual.hint_2")
                    .with("digit", first.digit)
                    .with("color", serde_plain_color(first.color)),
            ),
            3 => {
                // Near-solution: every digit except the last one.
                let shown: Vec<String> = state
                    .code
                    .iter()
                    .enumerate()
                    .map(|(i, d)| {
                        if i + 1 < state.code.len() {
                            d.to_string()
                        } else {
                            "?".into()
                        }
                    })
                    .collect();
                Some(Hint::new("lock_and_manual.hint_3").with("code", shown.join(" ")))
            }
            _ => None,
        }
    }

    fn is_solved(state: &State) -> bool {
        state.solved
    }
}

/// What two kids do by talking: combine the lock holder's tiles with the manual's rules.
/// Test-only; proves the two views together are enough to open the lock.
#[cfg(test)]
pub(crate) fn code_from_views(lock: &View, manual: &View) -> Vec<u8> {
    let (View::Lock { tiles, .. }, View::Manual { rules, .. }) = (lock, manual) else {
        panic!("expected a lock view and a manual view");
    };
    rules.iter().map(|r| r.kind.eval(r.color, tiles)).collect()
}

fn serde_plain_color(color: TileColor) -> &'static str {
    match color {
        TileColor::Red => "red",
        TileColor::Blue => "blue",
        TileColor::Green => "green",
        TileColor::Yellow => "yellow",
        TileColor::Purple => "purple",
        TileColor::Orange => "orange",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::puzzles::{AnyPuzzle, PuzzleSpec, test_support::assert_views_hide};

    fn params(digits: u8, rule: RuleSet, difficulty: Difficulty) -> Params {
        Params {
            digits,
            rule,
            difficulty,
        }
    }

    fn easy() -> Params {
        params(3, RuleSet::SumByColor, Difficulty::Easy)
    }

    fn all_param_combos() -> Vec<Params> {
        let mut out = Vec::new();
        for digits in 1..=4 {
            for rule in [RuleSet::SumByColor, RuleSet::Mixed] {
                for difficulty in [Difficulty::Easy, Difficulty::Medium, Difficulty::Hard] {
                    out.push(params(digits, rule, difficulty));
                }
            }
        }
        out
    }

    #[test]
    fn init_is_deterministic_for_a_seed() {
        for p in all_param_combos() {
            assert_eq!(LockAndManual::init(&p, 7, 2), LockAndManual::init(&p, 7, 2));
        }
        let different = (0..20)
            .filter(|s| {
                LockAndManual::init(&easy(), *s, 2) != LockAndManual::init(&easy(), 1000, 2)
            })
            .count();
        assert!(
            different >= 19,
            "different seeds should give different locks"
        );
    }

    #[test]
    fn generated_locks_are_well_formed() {
        for p in all_param_combos() {
            for seed in 0..200 {
                let s = LockAndManual::init(&p, seed, 2).unwrap();
                assert_eq!(s.code.len(), usize::from(p.digits));
                assert!(
                    s.code.iter().all(|d| (1..=9).contains(d)),
                    "digits are 1-9: {:?}",
                    s.code
                );
                assert_eq!(s.rules.len(), usize::from(p.digits));
                // Each rule uses a different color, and that color is on the lock.
                let mut colors: Vec<_> = s.rules.iter().map(|r| r.color).collect();
                colors.sort();
                colors.dedup();
                assert_eq!(colors.len(), s.rules.len());
                for rule in &s.rules {
                    assert!(s.tiles.iter().any(|t| t.color == rule.color));
                }
                // There is at least one decoy color nobody needs (when colors remain).
                if p.digits < 6 {
                    assert!(
                        s.tiles.iter().any(|t| !colors.contains(&t.color)),
                        "decoy expected"
                    );
                }
                if p.rule == RuleSet::SumByColor {
                    assert!(s.rules.iter().all(|r| r.kind == RuleKind::Sum));
                }
            }
        }
    }

    #[test]
    fn no_view_leaks_the_code_or_the_other_roles_secrets() {
        for p in all_param_combos() {
            for seed in 0..100 {
                let state = LockAndManual::init(&p, seed, 2).unwrap();
                let code: String = state.code.iter().map(u8::to_string).collect();
                // The code must not appear in any form.
                let as_array = serde_json::to_string(&state.code).unwrap();
                let puzzle = AnyPuzzle::LockAndManual(state.clone());
                assert_views_hide(&puzzle, 2, &[as_array, format!("\"{code}\"")]);

                // The lock holder never sees the rules; the manual reader never sees numbers.
                let lock =
                    serde_json::to_value(LockAndManual::view_for(&state, ROLE_LOCK)).unwrap();
                let manual =
                    serde_json::to_value(LockAndManual::view_for(&state, ROLE_MANUAL)).unwrap();
                assert!(lock.get("rules").is_none());
                assert!(manual.get("tiles").is_none());
                assert!(!manual.to_string().contains("\"number\""));
            }
        }
    }

    #[test]
    fn manual_reader_view_does_not_depend_on_the_numbers() {
        // Stronger leak check: change every number on the lock; the manual must look identical.
        let state = LockAndManual::init(&easy(), 3, 2).unwrap();
        let mut altered = state.clone();
        for t in &mut altered.tiles {
            t.number = 9 - t.number;
        }
        assert_eq!(
            LockAndManual::view_for(&state, ROLE_MANUAL),
            LockAndManual::view_for(&altered, ROLE_MANUAL)
        );
    }

    #[test]
    fn right_code_solves_wrong_code_does_not_reveal_anything() {
        let mut s = LockAndManual::init(&easy(), 11, 2).unwrap();
        let code = s.code.clone();
        let mut wrong = code.clone();
        wrong[0] = if wrong[0] == 9 { 1 } else { wrong[0] + 1 };

        let outcome = LockAndManual::apply(
            &mut s,
            ROLE_LOCK,
            Action::Submit {
                code: wrong.clone(),
            },
        );
        assert_eq!(outcome, Ok(ActionOutcome::Wrong));
        assert!(!LockAndManual::is_solved(&s));
        // Both players see the wrong attempt (to talk about it), never the right one.
        for role in [ROLE_LOCK, ROLE_MANUAL] {
            let v = LockAndManual::view_for(&s, role);
            let (View::Lock { wrong_attempts, .. } | View::Manual { wrong_attempts, .. }) = v;
            assert_eq!(wrong_attempts, vec![wrong.clone()]);
        }

        assert_eq!(
            LockAndManual::apply(&mut s, ROLE_LOCK, Action::Submit { code }),
            Ok(ActionOutcome::Solved)
        );
        assert!(LockAndManual::is_solved(&s));
    }

    #[test]
    fn only_the_lock_holder_can_submit_and_input_is_checked() {
        let mut s = LockAndManual::init(&easy(), 5, 2).unwrap();
        let code = s.code.clone();
        assert_eq!(
            LockAndManual::apply(&mut s, ROLE_MANUAL, Action::Submit { code: code.clone() }),
            Err(PuzzleError::NotYourRole)
        );
        for bad in [vec![1, 2], vec![1, 2, 3, 4], vec![1, 2, 10]] {
            assert!(matches!(
                LockAndManual::apply(&mut s, ROLE_LOCK, Action::Submit { code: bad }),
                Err(PuzzleError::Invalid(_))
            ));
        }
        assert_eq!(
            s.total_wrong, 0,
            "malformed input doesn't count as a wrong answer"
        );
        LockAndManual::apply(&mut s, ROLE_LOCK, Action::Submit { code: code.clone() }).unwrap();
        assert_eq!(
            LockAndManual::apply(&mut s, ROLE_LOCK, Action::Submit { code }),
            Err(PuzzleError::AlreadySolved)
        );
    }

    #[test]
    fn only_recent_wrong_attempts_are_kept() {
        let mut s =
            LockAndManual::init(&params(1, RuleSet::SumByColor, Difficulty::Easy), 1, 2).unwrap();
        let right = s.code[0];
        let wrongs: Vec<u8> = (0..=9).filter(|d| *d != right).collect();
        for d in &wrongs {
            LockAndManual::apply(&mut s, ROLE_LOCK, Action::Submit { code: vec![*d] }).unwrap();
        }
        assert_eq!(s.wrong_attempts.len(), ATTEMPTS_SHOWN);
        assert_eq!(s.total_wrong, 9);
        assert_eq!(
            s.wrong_attempts.last(),
            Some(&vec![*wrongs.last().unwrap()])
        );
    }

    #[test]
    fn hints_escalate_and_stop_after_three() {
        let s = LockAndManual::init(&easy(), 21, 2).unwrap();
        assert_eq!(
            LockAndManual::hint(&s, 1),
            Some(Hint::new("lock_and_manual.hint_1"))
        );
        let h2 = LockAndManual::hint(&s, 2).unwrap();
        assert_eq!(h2.params["digit"], "1");
        let h3 = LockAndManual::hint(&s, 3).unwrap();
        let expected = format!("{} {} ?", s.code[0], s.code[1]);
        assert_eq!(
            h3.params["code"], expected,
            "near-solution hides the last digit"
        );
        assert_eq!(LockAndManual::hint(&s, 4), None);
    }

    #[test]
    fn params_are_validated() {
        let bad = params(0, RuleSet::Mixed, Difficulty::Easy);
        assert!(LockAndManual::init(&bad, 1, 2).is_err());
        assert!(
            LockAndManual::init(&easy(), 1, 1).is_err(),
            "needs two players"
        );
        assert!(
            PuzzleSpec::parse("lock_and_manual", &serde_json::json!({"digits": 3}), 2).is_err()
        );
    }

    #[test]
    fn rule_evaluation() {
        let tiles = [
            Tile {
                color: TileColor::Red,
                number: 2,
            },
            Tile {
                color: TileColor::Red,
                number: 5,
            },
            Tile {
                color: TileColor::Blue,
                number: 3,
            },
        ];
        assert_eq!(RuleKind::Sum.eval(TileColor::Red, &tiles), 7);
        assert_eq!(RuleKind::Count.eval(TileColor::Red, &tiles), 2);
        assert_eq!(RuleKind::Largest.eval(TileColor::Red, &tiles), 5);
        assert_eq!(RuleKind::Smallest.eval(TileColor::Red, &tiles), 2);
        assert_eq!(RuleKind::Sum.eval(TileColor::Blue, &tiles), 3);
    }
}
