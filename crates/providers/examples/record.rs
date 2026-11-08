//! Records raw ESPN responses during a live night, for new test fixtures.
//!
//! `cargo run -p scorebug-providers --example record -- [DIR] [SECS]`
//!
//! Polls every SECS (default 15). Saves only what changed, to keep a night to
//! tens of megabytes:
//! - `DIR/<unix>-scoreboard.json` when any game's status name, period or
//!   score changes, and at least every 5 minutes;
//! - `DIR/<unix>-summary-<id>.json` for an in-progress game when its own
//!   status or score changes, and on every poll while it has goals without
//!   scorers (to measure how late scorers show up).
//!
//! Every change is logged with ESPN's raw status next to what the provider
//! maps it to, so mapping mistakes (e.g. intermissions) show up in the log.
//! Exits once every game has gone final after at least one was seen live.

use std::collections::HashMap;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use scorebug_core::GameState;
use scorebug_providers::espn::{BASE, parse_scoreboard, parse_scoring_plays};
use scorebug_providers::{Fetch, http_agent};
use serde_json::Value;

const SCOREBOARD_EVERY: Duration = Duration::from_secs(300);

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| "espn-capture".into());
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(15);
    std::fs::create_dir_all(&dir).expect("create capture dir");
    let http = http_agent();

    let mut last_key: HashMap<String, String> = HashMap::new();
    let mut last_saved: Option<Instant> = None;
    let mut seen_live = false;
    loop {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let body = match http.fetch(&format!("{BASE}/scoreboard")) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("{now} {e}");
                std::thread::sleep(Duration::from_secs(secs));
                continue;
            }
        };
        let raw = raw_status(&body);
        let games = parse_scoreboard(&body).unwrap_or_else(|e| {
            eprintln!("{now} provider: {e}");
            vec![]
        });

        let mut any_changed = false;
        for g in &games {
            let Some((key, line)) = raw.get(&g.id) else {
                continue;
            };
            let live = matches!(g.state, GameState::Live | GameState::Intermission);
            seen_live |= live;
            let changed = last_key.get(&g.id) != Some(key);
            if changed {
                any_changed = true;
                println!(
                    "{now} {:>10} {:>4} {:>2}-{:<2} {:<4} raw[{line}] -> {:?} {:?}",
                    g.id,
                    g.away.team.abbr,
                    g.away.score,
                    g.home.score,
                    g.home.team.abbr,
                    g.state,
                    g.status_line()
                );
                last_key.insert(g.id.clone(), key.clone());
            }
            if !live && (!changed || g.state != GameState::Final) {
                continue;
            }
            // Summaries: on change, and every poll while scorers are missing.
            let url = format!("{BASE}/summary?event={}", g.id);
            let missing = |s: &str| {
                parse_scoring_plays(s, &g.home.team.id, &g.away.team.id)
                    .map(|p| p.len() < usize::from(g.home.score + g.away.score))
                    .unwrap_or(true)
            };
            if changed || g.home.score + g.away.score > 0 {
                match http.fetch(&url) {
                    Ok(s) => {
                        let lagging = live && missing(&s);
                        if changed || lagging {
                            save(&dir, &format!("{now}-summary-{}.json", g.id), &s);
                        }
                        if lagging {
                            println!("{now} {:>10} scorer pending", g.id);
                        }
                    }
                    Err(e) => eprintln!("{now} {e}"),
                }
            }
        }
        if any_changed || last_saved.is_none_or(|t| t.elapsed() >= SCOREBOARD_EVERY) {
            save(&dir, &format!("{now}-scoreboard.json"), &body);
            last_saved = Some(Instant::now());
        }

        if seen_live && !games.is_empty() && games.iter().all(|g| g.state == GameState::Final) {
            println!("{now} all games final; done");
            return;
        }
        std::thread::sleep(Duration::from_secs(secs));
    }
}

/// Per event: a change key (status name, period, scores) and a readable
/// line with ESPN's own status fields.
fn raw_status(body: &str) -> HashMap<String, (String, String)> {
    let v: Value = serde_json::from_str(body).unwrap_or(Value::Null);
    let mut out = HashMap::new();
    for e in v["events"].as_array().into_iter().flatten() {
        let st = &e["status"];
        let scores: Vec<&str> = e["competitions"][0]["competitors"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|c| c["score"].as_str())
            .collect();
        let name = st["type"]["name"].as_str().unwrap_or("?");
        let key = format!("{name}|{}|{}", st["period"], scores.join("-"));
        let line = format!(
            "{name} {} p{} {} \"{}\"",
            st["type"]["state"].as_str().unwrap_or("?"),
            st["period"],
            st["displayClock"].as_str().unwrap_or("?"),
            st["type"]["shortDetail"].as_str().unwrap_or("?"),
        );
        if let Some(id) = e["id"].as_str() {
            out.insert(id.to_string(), (key, line));
        }
    }
    out
}

fn save(dir: &str, name: &str, body: &str) {
    if let Err(e) = std::fs::write(format!("{dir}/{name}"), body) {
        eprintln!("write {name}: {e}");
    }
}
