//! Records raw ESPN responses during a live night, for new test fixtures.
//!
//! `cargo run -p scorebug-providers --example record -- [DIR] [SECS]`
//!
//! Every SECS (default 20) it saves the scoreboard, plus the summary of every
//! game that is in progress, as `DIR/<unix>-scoreboard.json` and
//! `DIR/<unix>-summary-<id>.json`. It also prints what the provider makes of
//! each poll, so mapping mistakes show up live.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use scorebug_providers::espn::{BASE, Espn, parse_scoreboard};
use scorebug_providers::{Fetch, ScoreProvider, http_agent};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().unwrap_or_else(|| "espn-capture".into());
    let secs: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(20);
    std::fs::create_dir_all(&dir).expect("create capture dir");
    let http = http_agent();
    let mut espn = Espn::new(http_agent());
    loop {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        match http.fetch(&format!("{BASE}/scoreboard")) {
            Ok(body) => {
                save(&dir, &format!("{now}-scoreboard.json"), &body);
                for g in parse_scoreboard(&body).unwrap_or_default() {
                    use scorebug_core::GameState::{Intermission, Live};
                    if !matches!(g.state, Live | Intermission) {
                        continue;
                    }
                    match http.fetch(&format!("{BASE}/summary?event={}", g.id)) {
                        Ok(s) => save(&dir, &format!("{now}-summary-{}.json", g.id), &s),
                        Err(e) => eprintln!("{e}"),
                    }
                }
            }
            Err(e) => eprintln!("{e}"),
        }
        match espn.poll() {
            Ok(games) => {
                for g in games {
                    println!(
                        "{:>10} {:>4} {:>2} - {:<2} {:<4} {:<12} plays={}",
                        g.id,
                        g.away.team.abbr,
                        g.away.score,
                        g.home.score,
                        g.home.team.abbr,
                        g.status_line(),
                        g.scoring_plays.len()
                    );
                }
                println!();
            }
            Err(e) => eprintln!("provider: {e}"),
        }
        std::thread::sleep(Duration::from_secs(secs));
    }
}

fn save(dir: &str, name: &str, body: &str) {
    if let Err(e) = std::fs::write(format!("{dir}/{name}"), body) {
        eprintln!("write {name}: {e}");
    }
}
