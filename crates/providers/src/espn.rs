//! ESPN's unofficial NHL endpoints (`site.api.espn.com`).
//!
//! Written against captured responses in `tests/fixtures/espn`. The facts it
//! relies on:
//! - The scoreboard has state, period, clock and score, but no scoring plays.
//!   Scorers come only from `summary?event=<id>`: `plays[]` entries with
//!   `scoringPlay: true`.
//! - Scores are strings. After a shootout the winner's score includes one
//!   extra goal, credited to period 5 ("SO").
//! - Shootout attempts are scoring plays with `period.displayValue == "SO"` and
//!   `shotInfo.id == "905"`; they are not goals and are dropped here.
//! - An empty-net goal has `strength.abbreviation == "empty-net"` instead of
//!   its manpower strength.
//! - Team colors come without `#`; event dates have no seconds
//!   (`2026-10-13T22:30Z`).
//!
//! Not yet verified against a capture: how a live intermission looks. It is
//! inferred from `state == "in"` and a zero clock (or `STATUS_END_PERIOD`).

use std::collections::HashMap;
use std::time::{Duration, Instant};

use scorebug_core::{
    GameSnapshot, GameState, ScoreKind, ScoringPlay, Side, Sport, Strength, Team, TeamScore,
};
use serde::Deserialize;

use crate::{Fetch, ProviderError, ScoreProvider};

pub const BASE: &str = "https://site.api.espn.com/apis/site/v2/sports/hockey/nhl";
pub const NBA_BASE: &str = "https://site.api.espn.com/apis/site/v2/sports/basketball/nba";
pub const NFL_BASE: &str = "https://site.api.espn.com/apis/site/v2/sports/football/nfl";

/// Postseason. Playoff overtime never ends in a shootout.
const SEASON_POSTSEASON: u8 = 3;

/// How often a live game's summary is refreshed when nothing is missing.
const LIVE_REFRESH: Duration = Duration::from_secs(30);
/// How long to keep looking for scorers after a game ends.
const FINAL_GRACE: Duration = Duration::from_secs(15 * 60);

// ---- scoreboard -----------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Scoreboard {
    #[serde(default)]
    events: Vec<Event>,
}

#[derive(Debug, Deserialize)]
struct Event {
    id: String,
    date: String,
    #[serde(default)]
    season: Option<Season>,
    status: Status,
    competitions: Vec<Competition>,
}

#[derive(Debug, Deserialize)]
struct Season {
    #[serde(rename = "type")]
    kind: u8,
}

#[derive(Debug, Deserialize)]
struct Status {
    #[serde(default)]
    clock: f64,
    #[serde(rename = "displayClock", default)]
    display_clock: Option<String>,
    #[serde(default)]
    period: u8,
    #[serde(rename = "type")]
    kind: StatusType,
}

#[derive(Debug, Deserialize)]
struct StatusType {
    name: String,
    /// `pre`, `in` or `post`.
    state: String,
}

#[derive(Debug, Deserialize)]
struct Competition {
    competitors: Vec<Competitor>,
}

#[derive(Debug, Deserialize)]
struct Competitor {
    #[serde(rename = "homeAway")]
    home_away: String,
    #[serde(default)]
    score: Option<String>,
    team: EspnTeam,
}

#[derive(Debug, Deserialize)]
struct EspnTeam {
    id: String,
    abbreviation: String,
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(default)]
    color: Option<String>,
    #[serde(default)]
    logo: Option<String>,
}

/// Scoreboard snapshots, without scoring plays (those need the summary).
pub fn parse_scoreboard(json: &str) -> Result<Vec<GameSnapshot>, ProviderError> {
    parse_scoreboard_for(json, Sport::Hockey)
}

pub fn parse_scoreboard_for(json: &str, sport: Sport) -> Result<Vec<GameSnapshot>, ProviderError> {
    let sb: Scoreboard = serde_json::from_str(json)?;
    sb.events.iter().map(|e| snapshot(e, sport)).collect()
}

fn snapshot(e: &Event, sport: Sport) -> Result<GameSnapshot, ProviderError> {
    let comp = e
        .competitions
        .first()
        .ok_or_else(|| ProviderError(format!("event {} has no competition", e.id)))?;
    let side = |want: &str| {
        comp.competitors
            .iter()
            .find(|c| c.home_away == want)
            .ok_or_else(|| ProviderError(format!("event {} has no {want} team", e.id)))
    };
    let (home, away) = (side("home")?, side("away")?);

    let st = &e.status;
    let postseason = e
        .season
        .as_ref()
        .is_some_and(|s| s.kind == SEASON_POSTSEASON);
    let shootout = sport == Sport::Hockey && !postseason && st.period >= 5;
    let name = st.kind.name.as_str();
    let state = if name.contains("POSTPONED") || name.contains("CANCELED") {
        GameState::Postponed
    } else {
        match st.kind.state.as_str() {
            "pre" => GameState::Scheduled,
            "post" => GameState::Final,
            _ if name == "STATUS_HALFTIME" => GameState::Intermission,
            _ if name == "STATUS_END_PERIOD" || (st.clock <= 0.0 && st.period > 0 && !shootout) => {
                GameState::Intermission
            }
            _ => GameState::Live,
        }
    };
    let clock = (state == GameState::Live)
        .then(|| st.display_clock.clone())
        .flatten();

    Ok(GameSnapshot {
        id: e.id.clone(),
        sport,
        state,
        start_unix: parse_date(&e.date),
        period: if state == GameState::Scheduled {
            0
        } else {
            st.period
        },
        clock,
        home: team_score(home),
        away: team_score(away),
        shootout,
        scoring_plays: vec![],
    })
}

fn team_score(c: &Competitor) -> TeamScore {
    let t = &c.team;
    TeamScore {
        team: Team {
            id: t.id.clone(),
            abbr: t.abbreviation.clone(),
            name: t.display_name.clone(),
            color: t
                .color
                .as_deref()
                .filter(|c| c.len() == 6 && c.chars().all(|ch| ch.is_ascii_hexdigit()))
                .map(|c| format!("#{c}")),
            logo: t.logo.clone(),
        },
        score: c.score.as_deref().and_then(|s| s.parse().ok()).unwrap_or(0),
    }
}

fn parse_date(s: &str) -> Option<i64> {
    ["%Y-%m-%dT%H:%MZ", "%Y-%m-%dT%H:%M:%SZ"]
        .iter()
        .find_map(|f| chrono::NaiveDateTime::parse_from_str(s, f).ok())
        .map(|d| d.and_utc().timestamp())
}

// ---- summary --------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct Summary {
    /// Kept loose: only scoring plays are decoded, and play shapes vary
    /// (some participants have no `type`).
    #[serde(default)]
    plays: Vec<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct Play {
    id: String,
    #[serde(rename = "type")]
    kind: Option<Abbr>,
    #[serde(default)]
    team: Option<IdOnly>,
    period: Option<PlayPeriod>,
    clock: Option<PlayClock>,
    #[serde(default)]
    participants: Vec<Participant>,
    strength: Option<Abbr>,
    #[serde(rename = "shotInfo")]
    shot_info: Option<Abbr>,
}

#[derive(Debug, Deserialize)]
struct Abbr {
    #[serde(default)]
    id: Option<String>,
    #[serde(default)]
    abbreviation: Option<String>,
}

#[derive(Debug, Deserialize)]
struct IdOnly {
    id: String,
}

#[derive(Debug, Deserialize)]
struct PlayPeriod {
    number: u8,
    #[serde(rename = "displayValue", default)]
    display_value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PlayClock {
    #[serde(rename = "displayValue")]
    display_value: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Participant {
    athlete: Athlete,
    #[serde(rename = "type", default)]
    kind: String,
}

#[derive(Debug, Deserialize)]
struct Athlete {
    #[serde(rename = "displayName")]
    display_name: String,
    #[serde(rename = "shortName", default)]
    short_name: Option<String>,
}

/// Touchdowns and field goals from a football summary, in game order. Other
/// scores (safeties) are dropped: they change the score but raise no alert.
pub fn parse_football_scoring_plays(
    json: &str,
    home_team_id: &str,
    away_team_id: &str,
) -> Result<Vec<ScoringPlay>, ProviderError> {
    let s: FootballSummary = serde_json::from_str(json)?;
    Ok(s.scoring_plays
        .iter()
        .filter_map(|p| football_play(p, home_team_id, away_team_id))
        .collect())
}

#[derive(Debug, Deserialize)]
struct FootballSummary {
    #[serde(rename = "scoringPlays", default)]
    scoring_plays: Vec<FootballPlay>,
}

#[derive(Debug, Deserialize)]
struct FootballPlay {
    id: String,
    #[serde(default)]
    text: String,
    team: Option<IdOnly>,
    period: Option<PlayPeriod>,
    clock: Option<PlayClock>,
    #[serde(rename = "scoringType")]
    scoring_type: Option<ScoringType>,
    #[serde(rename = "homeScore")]
    home_score: Option<u16>,
    #[serde(rename = "awayScore")]
    away_score: Option<u16>,
}

#[derive(Debug, Deserialize)]
struct ScoringType {
    name: String,
}

fn football_play(p: &FootballPlay, home: &str, away: &str) -> Option<ScoringPlay> {
    let kind = match p.scoring_type.as_ref()?.name.as_str() {
        "touchdown" => ScoreKind::Touchdown,
        "field-goal" => ScoreKind::FieldGoal,
        _ => return None,
    };
    let side = match p.team.as_ref()?.id.as_str() {
        id if id == home => Side::Home,
        id if id == away => Side::Away,
        _ => return None,
    };
    let (scorer, passer) = football_names(&p.text)?;
    Some(ScoringPlay {
        id: p.id.clone(),
        side,
        scorer,
        assists: passer.into_iter().collect(),
        strength: None,
        empty_net: false,
        period: p.period.as_ref()?.number,
        clock: p.clock.as_ref().and_then(|c| c.display_value.clone()),
        kind: Some(kind),
        score_after: p.home_score.zip(p.away_score),
    })
}

/// Scorer and passer from ESPN's play text:
/// `Foster Moreau 6 Yd pass from C.J. Stroud (Ka'imi Fairbairn Kick)`,
/// `Jawhar Jordan 25 Yd Rush (...)`, `Ka'imi Fairbairn 25 Yd Field Goal`.
fn football_names(text: &str) -> Option<(String, Option<String>)> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let yards = words.windows(2).position(|w| {
        w[0].bytes().all(|b| b.is_ascii_digit()) && w[1].eq_ignore_ascii_case("yd")
    })?;
    let scorer = words[..yards].join(" ");
    if scorer.is_empty() {
        return None;
    }
    let passer = text
        .split_once(" pass from ")
        .map(|(_, rest)| rest.split(" (").next().unwrap_or(rest).trim().to_string());
    Some((scorer, passer.filter(|p| !p.is_empty())))
}

/// Goals in game order. Shootout attempts and plays without a known scorer
/// are dropped (the tracker picks the latter up once they appear).
pub fn parse_scoring_plays(
    json: &str,
    home_team_id: &str,
    away_team_id: &str,
) -> Result<Vec<ScoringPlay>, ProviderError> {
    let s: Summary = serde_json::from_str(json)?;
    Ok(s.plays
        .into_iter()
        .filter(|p| p.get("scoringPlay").and_then(|v| v.as_bool()) == Some(true))
        .filter_map(|p| serde_json::from_value::<Play>(p).ok())
        .filter(|p| p.kind.as_ref().and_then(|k| k.abbreviation.as_deref()) == Some("goal"))
        .filter_map(|p| scoring_play(&p, home_team_id, away_team_id))
        .collect())
}

fn scoring_play(p: &Play, home: &str, away: &str) -> Option<ScoringPlay> {
    let period = p.period.as_ref()?;
    let shootout_attempt = period.display_value.as_deref() == Some("SO")
        || p.shot_info.as_ref().and_then(|s| s.id.as_deref()) == Some("905");
    if shootout_attempt {
        return None;
    }
    let side = match p.team.as_ref()?.id.as_str() {
        id if id == home => Side::Home,
        id if id == away => Side::Away,
        _ => return None,
    };
    let scorer = p.participants.iter().find(|x| x.kind == "scorer")?;
    let assists = p
        .participants
        .iter()
        .filter(|x| x.kind == "assister")
        .map(|x| {
            x.athlete
                .short_name
                .clone()
                .unwrap_or_else(|| x.athlete.display_name.clone())
        })
        .collect();
    let abbr = |a: &Option<Abbr>| a.as_ref().and_then(|x| x.abbreviation.clone());
    let (strength_abbr, shot_abbr) = (abbr(&p.strength), abbr(&p.shot_info));
    let strength = match strength_abbr.as_deref() {
        Some("even-strength") => Some(Strength::Even),
        Some("power-play") => Some(Strength::PowerPlay),
        Some("short-handed") => Some(Strength::ShortHanded),
        Some("penalty-shot") => Some(Strength::PenaltyShot),
        _ => None,
    };
    Some(ScoringPlay {
        id: p.id.clone(),
        side,
        scorer: scorer.athlete.display_name.clone(),
        assists,
        strength,
        empty_net: strength_abbr.as_deref() == Some("empty-net")
            || shot_abbr.as_deref() == Some("empty-net"),
        period: period.number,
        clock: p.clock.as_ref().and_then(|c| c.display_value.clone()),
        kind: None,
        score_after: None,
    })
}

// ---- provider -------------------------------------------------------------

/// Game id, the score it was fetched for, and the plays (or why not).
type SummaryResult = (String, (u16, u16), Result<Vec<ScoringPlay>, ProviderError>);

#[derive(Debug, Default)]
struct Cached {
    plays: Option<Vec<ScoringPlay>>,
    fetched_at: Option<Instant>,
    fetched_for: (u16, u16),
    final_since: Option<Instant>,
}

/// Polls the scoreboard and, for games that need it, their summaries.
pub struct Espn<F: Fetch> {
    http: F,
    base: String,
    sport: Sport,
    /// `YYYYMMDD`; `None` follows ESPN's notion of "today".
    date: Option<String>,
    cache: HashMap<String, Cached>,
}

impl<F: Fetch> Espn<F> {
    pub fn new(http: F) -> Self {
        Self::with_base(http, BASE)
    }

    /// The NFL scoreboard. Touchdowns and field goals come from the summary.
    pub fn nfl(http: F) -> Self {
        Espn {
            sport: Sport::Football,
            ..Self::with_base(http, NFL_BASE)
        }
    }

    /// The NBA scoreboard. Scores only: there are no scoring plays to fetch.
    pub fn nba(http: F) -> Self {
        Espn {
            sport: Sport::Basketball,
            ..Self::with_base(http, NBA_BASE)
        }
    }

    pub fn with_base(http: F, base: &str) -> Self {
        Espn {
            http,
            base: base.trim_end_matches('/').into(),
            sport: Sport::Hockey,
            date: None,
            cache: HashMap::new(),
        }
    }

    pub fn date(mut self, yyyymmdd: Option<String>) -> Self {
        self.date = yyyymmdd;
        self
    }

    pub fn scoreboard_url(&self) -> String {
        match &self.date {
            Some(d) => format!("{}/scoreboard?dates={d}", self.base),
            None => format!("{}/scoreboard", self.base),
        }
    }

    pub fn summary_url(&self, event_id: &str) -> String {
        format!("{}/summary?event={event_id}", self.base)
    }

    fn needs_summary(&self, g: &GameSnapshot, now: Instant) -> bool {
        if self.sport == Sport::Basketball {
            return false;
        }
        let c = self.cache.get(&g.id);
        let Some(c) = c.filter(|c| c.plays.is_some()) else {
            // Never fetched: needed once the game has goals to attribute.
            return g.home.score + g.away.score > 0;
        };
        let fetched_at = c.fetched_at.unwrap_or(now);
        // Football points don't map to plays one to one: rely on the refresh.
        let missing =
            self.sport == Sport::Hockey && c.plays.as_ref().map_or(0, Vec::len) < goals_expected(g);
        let score_moved = c.fetched_for != (g.home.score, g.away.score);
        match g.state {
            GameState::Live | GameState::Intermission => {
                missing || score_moved || now.duration_since(fetched_at) >= LIVE_REFRESH
            }
            GameState::Final => {
                let final_since = c.final_since.unwrap_or(now);
                score_moved
                    || (missing && now.duration_since(final_since) < FINAL_GRACE)
                    || c.fetched_at.is_some_and(|t| t < final_since)
            }
            GameState::Scheduled | GameState::Postponed => false,
        }
    }
}

/// Goals that should have a scoring play: the score, minus the bonus goal
/// ESPN credits the shootout winner.
fn goals_expected(g: &GameSnapshot) -> usize {
    let total = usize::from(g.home.score + g.away.score);
    if g.shootout && g.state == GameState::Final {
        total.saturating_sub(1)
    } else {
        total
    }
}

impl<F: Fetch> ScoreProvider for Espn<F> {
    /// Games with goals are held back until their summary has loaded once, so
    /// the tracker's silent baseline already owns their earlier scorers.
    /// Otherwise those plays would look new and be pinned to the next goal.
    fn poll(&mut self) -> Result<Vec<GameSnapshot>, ProviderError> {
        let now = Instant::now();
        let mut games =
            parse_scoreboard_for(&self.http.fetch(&self.scoreboard_url())?, self.sport)?;

        for g in &games {
            let c = self.cache.entry(g.id.clone()).or_default();
            if g.state == GameState::Final && c.final_since.is_none() {
                c.final_since = Some(now);
            }
        }

        let wanted: Vec<&GameSnapshot> = games
            .iter()
            .filter(|g| self.needs_summary(g, now))
            .collect();
        let sport = self.sport;
        let fetched: Vec<SummaryResult> = std::thread::scope(|s| {
            let handles: Vec<_> = wanted
                .iter()
                .map(|g| {
                    let url = self.summary_url(&g.id);
                    let http = &self.http;
                    s.spawn(move || {
                        http.fetch(&url).and_then(|body| {
                            let (home, away) = (&g.home.team.id, &g.away.team.id);
                            if sport == Sport::Football {
                                parse_football_scoring_plays(&body, home, away)
                            } else {
                                parse_scoring_plays(&body, home, away)
                            }
                        })
                    })
                })
                .collect();
            wanted
                .iter()
                .zip(handles)
                .map(|(g, h)| {
                    let r = h
                        .join()
                        .unwrap_or_else(|_| Err(ProviderError("summary thread panicked".into())));
                    (g.id.clone(), (g.home.score, g.away.score), r)
                })
                .collect()
        });
        for (id, score, result) in fetched {
            let c = self.cache.entry(id).or_default();
            // On failure keep the last good plays; the next poll retries.
            if let Ok(plays) = result {
                c.plays = Some(plays);
                c.fetched_at = Some(now);
                c.fetched_for = score;
            }
        }

        if self.sport == Sport::Basketball {
            return Ok(games);
        }
        games.retain_mut(|g| {
            let plays = self.cache.get(&g.id).and_then(|c| c.plays.clone());
            match plays {
                Some(p) => {
                    g.scoring_plays = p;
                    true
                }
                None => g.home.score + g.away.score == 0,
            }
        });
        Ok(games)
    }
}
