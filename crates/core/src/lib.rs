//! Domain model and event detection for scorebug.
//!
//! Providers turn a data source into [`GameSnapshot`]s. The [`Tracker`] diffs
//! consecutive snapshots into [`GameEvent`]s. Nothing here does I/O or knows
//! about the UI, so it can be tested with scripted snapshots.

mod event;
mod model;
mod tracker;

pub use event::{EventKind, GameEvent, GoalDetail};
pub use model::{
    GameSnapshot, GameState, ScoreKind, ScoringPlay, Side, Sport, Strength, Team, TeamScore,
};
pub use tracker::Tracker;
