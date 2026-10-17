use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sport {
    Hockey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Side {
    Home,
    Away,
}

impl Side {
    pub const BOTH: [Side; 2] = [Side::Home, Side::Away];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Team {
    pub id: String,
    pub abbr: String,
    pub name: String,
    /// Primary color as `#rrggbb`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logo: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamScore {
    pub team: Team,
    pub score: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GameState {
    Scheduled,
    Live,
    Intermission,
    Final,
    Postponed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Strength {
    Even,
    PowerPlay,
    ShortHanded,
    PenaltyShot,
}

/// One goal as reported by the data source. Providers must drop plays whose
/// scorer is not known yet; the tracker picks them up when they appear.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoringPlay {
    /// Stable id within a game.
    pub id: String,
    pub side: Side,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSnapshot {
    pub id: String,
    pub sport: Sport,
    pub state: GameState,
    /// Scheduled start, unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_unix: Option<i64>,
    /// 0 before puck drop, 1-3 regulation, 4+ overtime.
    pub period: u8,
    /// Display clock, e.g. `8:41`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clock: Option<String>,
    pub home: TeamScore,
    pub away: TeamScore,
    /// True while in, or after, a shootout.
    #[serde(default)]
    pub shootout: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub scoring_plays: Vec<ScoringPlay>,
}

impl GameSnapshot {
    pub fn team_score(&self, side: Side) -> &TeamScore {
        match side {
            Side::Home => &self.home,
            Side::Away => &self.away,
        }
    }

    pub fn score(&self, side: Side) -> u16 {
        self.team_score(side).score
    }

    /// Overtime periods played; the shootout does not count.
    pub fn overtime_periods(&self) -> u8 {
        let n = self.period.saturating_sub(3);
        if self.shootout {
            n.saturating_sub(1)
        } else {
            n
        }
    }

    pub fn period_label(&self) -> String {
        if self.shootout {
            return "SO".into();
        }
        match self.period {
            0 => String::new(),
            1 => "1st".into(),
            2 => "2nd".into(),
            3 => "3rd".into(),
            4 => "OT".into(),
            n => format!("{}OT", n - 3),
        }
    }

    /// Short status text for a ticker cell (`2nd 8:41`, `End 2nd`, `FINAL/OT`).
    pub fn status_line(&self) -> String {
        match self.state {
            GameState::Scheduled => "SCHEDULED".into(),
            GameState::Postponed => "POSTPONED".into(),
            GameState::Live => match &self.clock {
                Some(c) if !self.shootout => format!("{} {}", self.period_label(), c),
                _ => self.period_label(),
            },
            GameState::Intermission => format!("End {}", self.period_label()),
            GameState::Final => {
                if self.shootout {
                    "FINAL/SO".into()
                } else {
                    match self.overtime_periods() {
                        0 => "FINAL".into(),
                        1 => "FINAL/OT".into(),
                        n => format!("FINAL/{n}OT"),
                    }
                }
            }
        }
    }
}
