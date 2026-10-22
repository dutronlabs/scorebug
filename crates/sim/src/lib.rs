//! A deterministic, accelerated NHL night: up to 16 games with staggered
//! starts, scoring, overtime, shootouts, delayed scorer info and reversed
//! goals. It produces the same [`GameSnapshot`]s a real provider would, so
//! the tracker, UI and animations can be developed without a live game.

mod teams;

use std::collections::VecDeque;

use scorebug_core::{
    GameEvent, GameSnapshot, GameState, ScoringPlay, Side, Sport, Strength, TeamScore, Tracker,
};
use serde::Serialize;

const PERIOD_SECS: u32 = 20 * 60;
const INTERMISSION_SECS: u32 = 15 * 60;
const OVERTIME_SECS: u32 = 5 * 60;
const STAGGER_SECS: u32 = 40 * 60;
const BASE_UNIX: i64 = 1_760_400_000;

/// Goals per team per second of play.
const GOAL_RATE: f64 = 3.0 / 3600.0;
/// Chance a goal's scorer shows up late.
const LATE_SCORER: f64 = 0.25;
/// Chance a goal is later taken off the board.
const REVERSAL: f64 = 0.08;

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn chance(&mut self, p: f64) -> bool {
        ((self.next() >> 11) as f64 / (1u64 << 53) as f64) < p
    }
    /// Inclusive range.
    fn range(&mut self, lo: u32, hi: u32) -> u32 {
        lo + (self.next() % u64::from(hi - lo + 1)) as u32
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Phase {
    Scheduled,
    Play,
    Intermission,
    Final,
}

struct Reveal {
    in_secs: u32,
    play: ScoringPlay,
}

struct SimGame {
    id: String,
    teams: [scorebug_core::Team; 2],
    start_at: u32,
    phase: Phase,
    period: u8,
    clock: u32,
    score: [u16; 2],
    shootout: bool,
    plays: Vec<ScoringPlay>,
    queue: VecDeque<Reveal>,
    /// Seconds until a provisional goal is taken off the board.
    reversal: [Option<u32>; 2],
    next_play: u32,
}

fn idx(side: Side) -> usize {
    match side {
        Side::Home => 0,
        Side::Away => 1,
    }
}

fn clock_str(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

impl SimGame {
    fn tick(&mut self, t: u32, rng: &mut Rng) {
        if let Some(front) = self.queue.front_mut() {
            front.in_secs = front.in_secs.saturating_sub(1);
        }
        self.drain();

        match self.phase {
            Phase::Scheduled => {
                if t >= self.start_at {
                    self.phase = Phase::Play;
                    self.period = 1;
                    self.clock = PERIOD_SECS;
                }
            }
            Phase::Intermission => {
                self.clock -= 1;
                if self.clock == 0 {
                    self.phase = Phase::Play;
                    self.period += 1;
                    self.clock = PERIOD_SECS;
                }
            }
            Phase::Play => self.play_second(rng),
            Phase::Final => {}
        }
    }

    fn play_second(&mut self, rng: &mut Rng) {
        for side in Side::BOTH {
            let i = idx(side);
            if let Some(n) = self.reversal[i] {
                if n <= 1 {
                    self.score[i] -= 1;
                    self.reversal[i] = None;
                } else {
                    self.reversal[i] = Some(n - 1);
                }
            }
        }

        let overtime = self.period >= 4;
        for side in Side::BOTH {
            if self.reversal[idx(side)].is_none() && rng.chance(GOAL_RATE) {
                self.goal(side, rng);
                if overtime {
                    self.phase = Phase::Final;
                    return;
                }
            }
        }

        self.clock -= 1;
        if self.clock > 0 {
            return;
        }
        let tied = self.score[0] == self.score[1];
        match self.period {
            1 | 2 => {
                self.phase = Phase::Intermission;
                self.clock = INTERMISSION_SECS;
            }
            3 if tied => {
                self.period = 4;
                self.clock = OVERTIME_SECS;
            }
            4 if tied => {
                self.period = 5;
                self.shootout = true;
                self.score[rng.range(0, 1) as usize] += 1;
                self.phase = Phase::Final;
            }
            _ => self.phase = Phase::Final,
        }
    }

    fn goal(&mut self, side: Side, rng: &mut Rng) {
        let i = idx(side);
        self.score[i] += 1;

        // Keep reversals clear of the final horn so the final score is stable.
        let safe_to_reverse = self.period < 3 || (self.period == 3 && self.clock > 90);
        if safe_to_reverse && rng.chance(REVERSAL) {
            self.reversal[i] = Some(rng.range(20, 60));
            return;
        }

        let leading = self.score[i] > self.score[1 - i] + 1;
        let empty_net = self.period == 3 && self.clock < 120 && leading && rng.chance(0.4);
        let strength = if rng.chance(0.22) {
            Strength::PowerPlay
        } else if rng.chance(0.03) {
            Strength::ShortHanded
        } else {
            Strength::Even
        };
        self.next_play += 1;
        let abbr = &self.teams[i].abbr;
        let play = ScoringPlay {
            id: format!("{}-{}", self.id, self.next_play),
            side,
            scorer: format!("{abbr} #{}", rng.range(2, 98)),
            assists: vec![format!("{abbr} #{}", rng.range(2, 98))],
            strength: Some(strength),
            empty_net,
            period: self.period,
            clock: Some(clock_str(self.period_len() - self.clock)),
            kind: None,
            score_after: None,
        };
        let delay = if rng.chance(LATE_SCORER) {
            rng.range(3, 25)
        } else {
            0
        };
        self.queue.push_back(Reveal {
            in_secs: delay,
            play,
        });
        self.drain();
    }

    fn period_len(&self) -> u32 {
        if self.period >= 4 {
            OVERTIME_SECS
        } else {
            PERIOD_SECS
        }
    }

    /// Reveals happen in order; a slow one holds up the ones behind it.
    fn drain(&mut self) {
        while self.queue.front().is_some_and(|r| r.in_secs == 0) {
            let r = self.queue.pop_front().expect("front checked");
            self.plays.push(r.play);
        }
    }

    fn snapshot(&self) -> GameSnapshot {
        let (state, clock) = match self.phase {
            Phase::Scheduled => (GameState::Scheduled, None),
            Phase::Play => (GameState::Live, Some(clock_str(self.clock))),
            Phase::Intermission => (GameState::Intermission, Some(clock_str(self.clock))),
            Phase::Final => (GameState::Final, None),
        };
        GameSnapshot {
            id: self.id.clone(),
            sport: Sport::Hockey,
            state,
            start_unix: Some(BASE_UNIX + i64::from(self.start_at)),
            period: self.period,
            clock,
            home: TeamScore {
                team: self.teams[0].clone(),
                score: self.score[0],
            },
            away: TeamScore {
                team: self.teams[1].clone(),
                score: self.score[1],
            },
            shootout: self.shootout,
            scoring_plays: self.plays.clone(),
        }
    }
}

pub struct Simulator {
    t: u32,
    rng: Rng,
    games: Vec<SimGame>,
}

impl Simulator {
    /// `n_games` is clamped to 1..=16.
    pub fn new(seed: u64, n_games: usize) -> Self {
        let mut rng = Rng::new(seed);
        let mut pool = teams::all();
        for i in (1..pool.len()).rev() {
            pool.swap(i, rng.range(0, i as u32) as usize);
        }
        let games = pool
            .chunks(2)
            .take(n_games.clamp(1, 16))
            .enumerate()
            .map(|(n, pair)| SimGame {
                id: format!("sim-{}", n + 1),
                teams: [pair[0].clone(), pair[1].clone()],
                start_at: rng.range(1, STAGGER_SECS),
                phase: Phase::Scheduled,
                period: 0,
                clock: 0,
                score: [0, 0],
                shootout: false,
                plays: vec![],
                queue: VecDeque::new(),
                reversal: [None, None],
                next_play: 0,
            })
            .collect();
        Simulator { t: 0, rng, games }
    }

    /// Simulated seconds elapsed.
    pub fn time(&self) -> u32 {
        self.t
    }

    pub fn tick(&mut self) {
        self.t += 1;
        for g in &mut self.games {
            g.tick(self.t, &mut self.rng);
        }
    }

    pub fn snapshots(&self) -> Vec<GameSnapshot> {
        self.games.iter().map(SimGame::snapshot).collect()
    }

    /// All games final and every delayed scorer revealed.
    pub fn is_finished(&self) -> bool {
        self.games
            .iter()
            .all(|g| g.phase == Phase::Final && g.queue.is_empty())
    }
}

/// One simulated poll: what changed and what the tracker made of it.
#[derive(Debug, Serialize)]
pub struct Frame {
    /// Milliseconds since the start of the replay.
    pub t_ms: u64,
    /// Snapshots that changed since the previous frame (all of them on the first).
    pub snapshots: Vec<GameSnapshot>,
    pub events: Vec<GameEvent>,
}

/// Runs a whole night, polling every `poll_secs` of game time, and returns
/// the frames a live provider + tracker would have produced. `warp` is how
/// many game seconds pass per real second in the replay timeline.
pub fn run_night(seed: u64, n_games: usize, poll_secs: u32, warp: f64) -> Vec<Frame> {
    let poll_secs = poll_secs.max(1);
    let mut sim = Simulator::new(seed, n_games);
    let mut tracker = Tracker::new();
    let mut last: Vec<GameSnapshot> = Vec::new();
    let mut frames = Vec::new();

    let mut emit = |sim: &Simulator, tracker: &mut Tracker, last: &mut Vec<GameSnapshot>| {
        let now = sim.snapshots();
        let events = tracker.ingest(&now);
        let changed: Vec<_> = now
            .iter()
            .filter(|s| last.iter().find(|l| l.id == s.id) != Some(*s))
            .cloned()
            .collect();
        *last = now;
        frames.push(Frame {
            t_ms: (f64::from(sim.time()) * 1000.0 / warp) as u64,
            snapshots: changed,
            events,
        });
    };

    emit(&sim, &mut tracker, &mut last);
    while !sim.is_finished() {
        sim.tick();
        if sim.time().is_multiple_of(poll_secs) {
            emit(&sim, &mut tracker, &mut last);
        }
    }
    emit(&sim, &mut tracker, &mut last);
    frames
}
