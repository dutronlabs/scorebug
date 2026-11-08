use std::collections::{HashMap, HashSet};

use crate::{EventKind, GameEvent, GameSnapshot, GameState, GoalDetail, Side, Sport};

/// Turns a stream of polled snapshots into discrete [`GameEvent`]s.
///
/// Rules worth knowing:
/// - The first snapshot of a game is a silent baseline, so launching mid-game
///   never replays old goals.
/// - A score increase is a `Goal`; one poll can contain several.
/// - A score decrease is `GoalDisallowed`.
/// - Scorer details often lag the score. A `Goal` emitted without details is
///   followed by `ScorerResolved` when the play shows up. Plays that show up
///   before the score are held and attached to the next goal.
/// - Shootout score changes are not goals; `Final` carries the result.
/// - Football has no one-point goals: a touchdown is 6 to 8 points and a
///   conversion follows it. So football games emit a `Goal` per new touchdown
///   or field goal in `scoring_plays` (the score alone never raises one), and
///   `GoalDisallowed` when such a play drops out of the list (nullified).
/// - Nothing is emitted after `Final`, except late scorer details.
/// - Changes that cancel out between two polls (a goal plus a reversal for
///   the same side) are invisible by construction.
#[derive(Debug, Default)]
pub struct Tracker {
    games: HashMap<String, GameMemory>,
    next_seq: u64,
}

#[derive(Debug)]
struct GameMemory {
    last: GameSnapshot,
    /// Scoring play ids already attributed to a goal.
    claimed: HashSet<String>,
    /// Goals emitted without details, oldest first.
    pending: Vec<Pending>,
    /// Football: plays that raised a `Goal`, so a nullified one is noticed.
    announced: Vec<(String, Side)>,
}

#[derive(Debug)]
struct Pending {
    goal_seq: u64,
    side: Side,
}

impl Tracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn ingest(&mut self, snapshots: &[GameSnapshot]) -> Vec<GameEvent> {
        let mut out = Vec::new();
        for next in snapshots {
            match self.games.get_mut(&next.id) {
                None => {
                    self.games
                        .insert(next.id.clone(), GameMemory::baseline(next));
                }
                Some(mem) => mem.advance(next, &mut self.next_seq, &mut out),
            }
        }
        out
    }
}

/// Appends events for one game, assigning sequence numbers as it goes.
struct Emitter<'a> {
    game_id: &'a str,
    seq: &'a mut u64,
    out: &'a mut Vec<GameEvent>,
}

impl Emitter<'_> {
    fn push(&mut self, kind: EventKind) -> u64 {
        let seq = *self.seq;
        *self.seq += 1;
        self.out.push(GameEvent {
            seq,
            game_id: self.game_id.to_string(),
            kind,
        });
        seq
    }
}

impl GameMemory {
    fn baseline(snap: &GameSnapshot) -> Self {
        GameMemory {
            claimed: snap.scoring_plays.iter().map(|p| p.id.clone()).collect(),
            last: snap.clone(),
            pending: Vec::new(),
            announced: Vec::new(),
        }
    }

    fn take_unclaimed(&mut self, next: &GameSnapshot, side: Side) -> Option<GoalDetail> {
        let play = next
            .scoring_plays
            .iter()
            .find(|p| p.side == side && !self.claimed.contains(&p.id))?;
        self.claimed.insert(play.id.clone());
        Some(play.into())
    }

    fn advance(&mut self, next: &GameSnapshot, seq: &mut u64, out: &mut Vec<GameEvent>) {
        let prev = std::mem::replace(&mut self.last, next.clone());
        let mut em = Emitter {
            game_id: &next.id,
            seq,
            out,
        };

        if prev.state != GameState::Final {
            if prev.state == GameState::Scheduled
                && matches!(next.state, GameState::Live | GameState::Intermission)
            {
                em.push(EventKind::GameStart);
            }

            if next.sport == Sport::Football {
                self.football_plays(next, &mut em);
            } else if !next.shootout {
                self.score_changes(&prev, next, &mut em);
            }

            let period_ended = prev.state == GameState::Live
                && (next.state == GameState::Intermission
                    || (next.state == GameState::Live && next.period > prev.period));
            if period_ended {
                em.push(EventKind::PeriodEnd {
                    period: prev.period,
                });
            }
            if next.state == GameState::Live
                && prev.state != GameState::Scheduled
                && next.period > prev.period
            {
                em.push(EventKind::PeriodStart {
                    period: next.period,
                });
            }
            if next.state == GameState::Final {
                em.push(EventKind::Final {
                    home_score: next.home.score,
                    away_score: next.away.score,
                    overtime_periods: next.overtime_periods(),
                    shootout: next.shootout,
                });
            }
        }

        self.resolve_pending(next, &mut em);
    }

    fn football_plays(&mut self, next: &GameSnapshot, em: &mut Emitter) {
        let (mut home, mut away) = (next.home.score, next.away.score);
        let gone: Vec<(String, Side)> = self
            .announced
            .iter()
            .filter(|(id, _)| !next.scoring_plays.iter().any(|p| &p.id == id))
            .cloned()
            .collect();
        for (id, side) in gone {
            self.announced.retain(|(a, _)| a != &id);
            em.push(EventKind::GoalDisallowed {
                side,
                home_score: home,
                away_score: away,
            });
        }
        for p in &next.scoring_plays {
            if p.kind.is_none() || self.claimed.contains(&p.id) {
                continue;
            }
            self.claimed.insert(p.id.clone());
            self.announced.push((p.id.clone(), p.side));
            if let Some((h, a)) = p.score_after {
                (home, away) = (h, a);
            }
            em.push(EventKind::Goal {
                side: p.side,
                home_score: home,
                away_score: away,
                detail: Some(p.into()),
            });
        }
    }

    fn score_changes(&mut self, prev: &GameSnapshot, next: &GameSnapshot, em: &mut Emitter) {
        let (mut home, mut away) = (prev.home.score, prev.away.score);
        for side in Side::BOTH {
            let (p, n) = (prev.score(side), next.score(side));
            for _ in p..n {
                match side {
                    Side::Home => home += 1,
                    Side::Away => away += 1,
                }
                let detail = self.take_unclaimed(next, side);
                let has_detail = detail.is_some();
                let goal_seq = em.push(EventKind::Goal {
                    side,
                    home_score: home,
                    away_score: away,
                    detail,
                });
                if !has_detail {
                    self.pending.push(Pending { goal_seq, side });
                }
            }
            for _ in n..p {
                match side {
                    Side::Home => home -= 1,
                    Side::Away => away -= 1,
                }
                em.push(EventKind::GoalDisallowed {
                    side,
                    home_score: home,
                    away_score: away,
                });
                // A reversed goal never gets a scorer: drop the newest
                // detail-less goal for that side.
                if let Some(i) = self.pending.iter().rposition(|x| x.side == side) {
                    self.pending.remove(i);
                }
            }
        }
    }

    fn resolve_pending(&mut self, next: &GameSnapshot, em: &mut Emitter) {
        for side in Side::BOTH {
            while let Some(i) = self.pending.iter().position(|x| x.side == side) {
                let Some(detail) = self.take_unclaimed(next, side) else {
                    break;
                };
                let p = self.pending.remove(i);
                em.push(EventKind::ScorerResolved {
                    goal_seq: p.goal_seq,
                    detail,
                });
            }
        }
    }
}
