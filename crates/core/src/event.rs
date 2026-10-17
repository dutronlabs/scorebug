use serde::{Deserialize, Serialize};

use crate::{ScoringPlay, Side, Strength};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalDetail {
    pub scorer: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub assists: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strength: Option<Strength>,
    #[serde(default)]
    pub empty_net: bool,
    pub period: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<String>,
}

impl From<&ScoringPlay> for GoalDetail {
    fn from(p: &ScoringPlay) -> Self {
        GoalDetail {
            scorer: p.scorer.clone(),
            assists: p.assists.clone(),
            strength: p.strength,
            empty_net: p.empty_net,
            period: p.period,
            clock: p.clock.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum EventKind {
    GameStart,
    PeriodStart {
        period: u8,
    },
    PeriodEnd {
        period: u8,
    },
    /// The score went up. `detail` is `None` when the scorer is not known
    /// yet; a [`EventKind::ScorerResolved`] follows once it is.
    Goal {
        side: Side,
        home_score: u16,
        away_score: u16,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        detail: Option<GoalDetail>,
    },
    /// The score went down (review or challenge). Not a celebration.
    GoalDisallowed {
        side: Side,
        home_score: u16,
        away_score: u16,
    },
    /// Late scorer info for the `Goal` event with sequence number `goal_seq`.
    ScorerResolved {
        goal_seq: u64,
        detail: GoalDetail,
    },
    Final {
        home_score: u16,
        away_score: u16,
        overtime_periods: u8,
        shootout: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameEvent {
    /// Monotonic across the tracker's lifetime.
    pub seq: u64,
    pub game_id: String,
    #[serde(flatten)]
    pub kind: EventKind,
}
