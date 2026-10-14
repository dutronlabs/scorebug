//! The data side of the app: polls a source, runs the tracker, and emits
//! frames shaped exactly like `scorebug-sim`'s NDJSON (`ui/src/types.ts`).

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use scorebug_core::{GameEvent, GameSnapshot, Tracker};
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
        poll: Duration,
    },
    Sim {
        seed: u64,
        warp: f64,
    },
}

impl Source {
    pub fn from_args(args: impl Iterator<Item = String>) -> Result<Options, String> {
        let (mut sim, mut seed, mut warp, mut date, mut poll) = (false, 1u64, 10.0f64, None, 5u64);
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
                "--poll" => poll = value()?.parse().map_err(|_| "bad --poll")?,
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
                poll: Duration::from_secs(poll.max(2)),
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
}

/// Latest snapshot of every game, so a window that (re)loads can catch up.
#[derive(Default)]
pub struct Feed {
    games: Mutex<BTreeMap<String, GameSnapshot>>,
    started: Mutex<Option<Instant>>,
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
    }
}

pub fn spawn(app: AppHandle, source: Source) {
    std::thread::Builder::new()
        .name("feed".into())
        .spawn(move || match source {
            Source::Espn { date, poll } => live(&app, date, poll),
            Source::Sim { seed, warp } => sim(&app, seed, warp),
        })
        .expect("spawn feed thread");
}

/// Stores changed snapshots and emits the frame to every window.
fn publish(app: &AppHandle, snapshots: Vec<GameSnapshot>, events: Vec<GameEvent>) {
    let feed = app.state::<Feed>();
    {
        let mut games = feed.games.lock().unwrap();
        for s in &snapshots {
            games.insert(s.id.clone(), s.clone());
        }
    }
    if snapshots.is_empty() && events.is_empty() {
        return;
    }
    let frame = Frame {
        t_ms: feed.elapsed_ms(),
        snapshots,
        events,
    };
    if let Err(e) = app.emit("frame", &frame) {
        eprintln!("emit frame: {e}");
    }
}

fn live(app: &AppHandle, date: Option<String>, poll: Duration) {
    let mut espn = Espn::new(http_agent()).date(date);
    let mut tracker = Tracker::new();
    let mut last: BTreeMap<String, GameSnapshot> = BTreeMap::new();
    let mut failures = 0u32;
    loop {
        let started = Instant::now();
        match espn.poll() {
            Ok(snaps) => {
                failures = 0;
                let events = tracker.ingest(&snaps);
                let changed: Vec<GameSnapshot> = snaps
                    .into_iter()
                    .filter(|s| last.get(&s.id) != Some(s))
                    .collect();
                for s in &changed {
                    last.insert(s.id.clone(), s.clone());
                }
                publish(app, changed, events);
            }
            Err(e) => {
                failures += 1;
                eprintln!("espn: {e}");
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
            publish(app, f.snapshots, f.events);
        }
        std::thread::sleep(Duration::from_secs(15));
        seed += 1;
    }
}
