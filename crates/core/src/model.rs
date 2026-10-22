use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sport {
    Hockey,
    Basketball,
    Football,
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

/// What kind of score a football play was. Only these two raise an alert;
/// safeties and conversions show up in the score but not as events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScoreKind {
    Touchdown,
    FieldGoal,
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
    /// Football only: touchdown or field goal.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ScoreKind>,
    /// Football only: `(home, away)` once this play was counted. Football
    /// scores move by several points (and a conversion follows a touchdown),
    /// so the tracker reads plays rather than score deltas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub score_after: Option<(u16, u16)>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GameSnapshot {
    pub id: String,
    pub sport: Sport,
    pub state: GameState,
    /// Scheduled start, unix seconds.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub start_unix: Option<i64>,
    /// 0 before the start, then regulation periods, then overtime.
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

    /// Regulation periods: three in hockey, four in basketball and football.
    pub fn regulation_periods(&self) -> u8 {
        match self.sport {
            Sport::Hockey => 3,
            Sport::Basketball | Sport::Football => 4,
        }
    }

    /// Overtime periods played; the shootout does not count.
    pub fn overtime_periods(&self) -> u8 {
        let n = self.period.saturating_sub(self.regulation_periods());
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
        let regulation = self.regulation_periods();
        match self.period {
            0 => String::new(),
            n if n > regulation => match n - regulation {
                1 => "OT".into(),
                ot => format!("{ot}OT"),
            },
            n => match self.sport {
                Sport::Hockey => match n {
                    1 => "1st".into(),
                    2 => "2nd".into(),
                    _ => "3rd".into(),
                },
                Sport::Basketball | Sport::Football => format!("Q{n}"),
            },
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
            GameState::Intermission if self.sport != Sport::Hockey && self.period == 2 => {
                "Halftime".into()
            }
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
