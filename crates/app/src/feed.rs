//! The data side of the app: polls a source, runs the tracker, and emits
//! frames shaped exactly like `scorebug-sim`'s NDJSON (`ui/src/types.ts`),
//! plus `status` events while the feed is failing.

use std::collections::{BTreeMap, HashMap};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use scorebug_core::{GameEvent, GameSnapshot, Sport, Tracker};
use scorebug_providers::espn::Espn;
use scorebug_providers::{ScoreProvider, http_agent};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

#[derive(Debug, Clone)]
pub struct Options {
    pub source: Source,
    /// Open the grid window at startup.
    pub grid: bool,
}

#[derive(Debug, Clone)]
pub enum Source {
    Espn {
        date: Option<String>,
        /// `None`: use the config file's `poll_secs`.
        poll: Option<Duration>,
    },
    Sim {
        seed: u64,
        warp: f64,
    },
}

impl Source {
    pub fn from_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
        let (mut sim, mut seed, mut warp, mut date, mut poll) =
            (false, 1u64, 10.0f64, None, None::<u64>);
        let mut grid = false;
        let mut args = args;
        while let Some(flag) = args.next() {
            let mut value = || args.next().ok_or_else(|| format!("{flag} needs a value"));
            match flag.as_str() {
                "--sim" => sim = true,
                "--grid" => grid = true,
                "--seed" => seed = value()?.parse().map_err(|_| "bad --seed")?,
                "--warp" => warp = value()?.parse().map_err(|_| "bad --warp")?,
                "--date" => date = Some(value()?),
                "--poll" => poll = Some(value()?.parse().map_err(|_| "bad --poll")?),
                other => return Err(format!("unknown argument {other}")),
            }
        }
        let source = if sim {
            Source::Sim {
                seed,
                warp: warp.max(0.1),
            }
        } else {
            Source::Espn {
                date,
                poll: poll.map(|p| Duration::from_secs(p.max(2))),
            }
        };
        Ok(Options { source, grid })
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Frame {
    pub t_ms: u64,
    pub snapshots: Vec<GameSnapshot>,
    pub events: Vec<GameEvent>,
    /// Games no longer on the source's slate (e.g. yesterday's, after the
    /// scoreboard rolls over to a new day).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub removed: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FeedStatus {
    /// False after the last poll failed.
    pub ok: bool,
    pub error: Option<String>,
    /// Unix seconds of the last successful poll.
    pub last_ok: Option<u64>,
}

/// Latest snapshot of every game, so a window that (re)loads can catch up.
#[derive(Default)]
pub struct Feed {
    games: Mutex<BTreeMap<String, GameSnapshot>>,
    started: Mutex<Option<Instant>>,
    status: Mutex<FeedStatus>,
}

impl Feed {
    fn elapsed_ms(&self) -> u64 {
        let mut s = self.started.lock().unwrap();
        s.get_or_insert_with(Instant::now).elapsed().as_millis() as u64
    }
}

/// Everything known right now, as a single frame with no events.
#[tauri::command]
pub fn current_frame(feed: State<'_, Feed>) -> Frame {
    Frame {
        t_ms: feed.elapsed_ms(),
        snapshots: feed.games.lock().unwrap().values().cloned().collect(),
        events: vec![],
        removed: vec![],
    }
}

#[tauri::command]
pub fn feed_status(feed: State<'_, Feed>) -> FeedStatus {
    feed.status.lock().unwrap().clone()
}

pub fn spawn(app: AppHandle, source: Source, default_poll: Duration, nba: bool, nfl: bool) {
    std::thread::Builder::new()
        .name("feed".into())
        .spawn(move || match source {
            Source::Espn { date, poll } => live(&app, date, poll.unwrap_or(default_poll), nba, nfl),
            Source::Sim { seed, warp } => sim(&app, seed, warp),
        })
        .expect("spawn feed thread");
}

/// Stores changed snapshots and emits the frame to every window.
fn publish(
    app: &AppHandle,
    snapshots: Vec<GameSnapshot>,
    events: Vec<GameEvent>,
    removed: Vec<String>,
) {
    let feed = app.state::<Feed>();
    {
        let mut games = feed.games.lock().unwrap();
        for s in &snapshots {
            games.insert(s.id.clone(), s.clone());
        }
        for id in &removed {
            games.remove(id);
        }
    }
    if snapshots.is_empty() && events.is_empty() && removed.is_empty() {
        return;
    }
    let frame = Frame {
        t_ms: feed.elapsed_ms(),
        snapshots,
        events,
        removed,
    };
    if let Err(e) = app.emit("frame", &frame) {
        eprintln!("emit frame: {e}");
    }
}

fn set_status(app: &AppHandle, next: FeedStatus) {
    let feed = app.state::<Feed>();
    let mut cur = feed.status.lock().unwrap();
    // `last_ok` ticks every poll; only tell the UI when health changes.
    let changed = cur.ok != next.ok || cur.error != next.error;
    *cur = next.clone();
    drop(cur);
    if changed {
        let _ = app.emit("status", &next);
    }
}

fn unix_now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// A game is dropped after this many consecutive polls without it, so one
/// odd response doesn't make games flicker away.
const MISSING_POLLS: u32 = 3;

fn live(app: &AppHandle, date: Option<String>, poll: Duration, nba: bool, nfl: bool) {
    let mut nhl = Espn::new(http_agent()).date(date.clone());
    let mut nfl = nfl.then(|| Espn::nfl(http_agent()).date(date.clone()));
    let mut nba = nba.then(|| Espn::nba(http_agent()).date(date));
    let mut tracker = Tracker::new();
    let mut last: BTreeMap<String, GameSnapshot> = BTreeMap::new();
    let mut missing: HashMap<String, u32> = HashMap::new();
    let mut failures = 0u32;
    let mut last_ok = None;
    loop {
        let started = Instant::now();
        // One league failing must not blank the other, so keep what loaded.
        let mut snaps = vec![];
        let mut errors = vec![];
        let mut any_ok = false;
        let polled = [
            ("nhl", Some(nhl.poll())),
            ("nfl", nfl.as_mut().map(|p| p.poll())),
            ("nba", nba.as_mut().map(|p| p.poll())),
        ];
        for (name, result) in polled {
            match result {
                Some(Ok(mut games)) => {
                    any_ok = true;
                    snaps.append(&mut games);
                }
                Some(Err(e)) => {
                    eprintln!("espn {name}: {e}");
                    errors.push(e.to_string());
                }
                None => {}
            }
        }
        match any_ok.then_some(snaps) {
            Some(snaps) => {
                failures = 0;
                last_ok = Some(unix_now());
                let error = (!errors.is_empty()).then(|| errors.join("; "));
                set_status(
                    app,
                    FeedStatus {
                        ok: error.is_none(),
                        error,
                        last_ok,
                    },
                );
                // Basketball is scores only: no events, so no alerts.
                let alerting: Vec<GameSnapshot> = snaps
                    .iter()
                    .filter(|s| s.sport != Sport::Basketball)
                    .cloned()
                    .collect();
                let events = tracker.ingest(&alerting);
                let changed: Vec<GameSnapshot> = snaps
                    .iter()
                    .filter(|s| last.get(&s.id) != Some(*s))
                    .cloned()
                    .collect();
                for s in &changed {
                    last.insert(s.id.clone(), s.clone());
                }
                let mut removed = vec![];
                for id in last.keys() {
                    if snaps.iter().any(|s| &s.id == id) {
                        missing.remove(id);
                    } else {
                        let n = missing.entry(id.clone()).or_default();
                        *n += 1;
                        if *n >= MISSING_POLLS {
                            removed.push(id.clone());
                        }
                    }
                }
                for id in &removed {
                    last.remove(id);
                    missing.remove(id);
                }
                publish(app, changed, events, removed);
            }
            None => {
                failures += 1;
                set_status(
                    app,
                    FeedStatus {
                        ok: false,
                        error: Some(errors.join("; ")),
                        last_ok,
                    },
                );
            }
        }
        // Back off while the feed is failing, up to a minute.
        let wait = (poll * 2u32.pow(failures.min(4))).min(Duration::from_secs(60));
        std::thread::sleep(wait.saturating_sub(started.elapsed()));
    }
}

/// Plays simulated nights back to back in real time.
fn sim(app: &AppHandle, mut seed: u64, warp: f64) {
    const SIM_WARP: f64 = 30.0;
    loop {
        // A new night reuses game ids and restarts event numbering.
        app.state::<Feed>().games.lock().unwrap().clear();
        let _ = app.emit("reset", ());
        let start = Instant::now();
        for f in scorebug_sim::run_night(seed, 16, 5, SIM_WARP) {
            let at = Duration::from_secs_f64(f.t_ms as f64 / 1000.0 * SIM_WARP / warp);
            std::thread::sleep(at.saturating_sub(start.elapsed()));
            publish(app, f.snapshots, f.events, vec![]);
        }
        std::thread::sleep(Duration::from_secs(15));
        seed += 1;
    }
}
