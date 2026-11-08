use std::collections::HashMap;
use std::sync::Mutex;

use scorebug_core::{GameState, ScoreKind, Side, Strength, Tracker};
use scorebug_providers::espn::{
    Espn, parse_football_scoring_plays, parse_scoreboard, parse_scoring_plays,
};
use scorebug_providers::{Fetch, ProviderError, ScoreProvider};

const DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/espn");

fn fixture(name: &str) -> String {
    std::fs::read_to_string(format!("{DIR}/{name}")).unwrap_or_else(|e| panic!("{name}: {e}"))
}

/// Serves fixture files by URL and records what was asked for.
struct Fixtures {
    routes: HashMap<String, String>,
    log: Mutex<Vec<String>>,
}

impl Fixtures {
    fn final_night() -> Self {
        let mut routes = HashMap::new();
        routes.insert(
            "x/scoreboard".into(),
            "scoreboard-2026-10-13-final.json".into(),
        );
        for g in parse_scoreboard(&fixture("scoreboard-2026-10-13-final.json")).unwrap() {
            routes.insert(
                format!("x/summary?event={}", g.id),
                format!("summary-{}.json", g.id),
            );
        }
        Fixtures {
            routes,
            log: Mutex::new(vec![]),
        }
    }
    fn requests(&self) -> Vec<String> {
        std::mem::take(&mut self.log.lock().unwrap())
    }
}

impl Fetch for Fixtures {
    fn fetch(&self, url: &str) -> Result<String, ProviderError> {
        self.log.lock().unwrap().push(url.into());
        match self.routes.get(url) {
            Some(f) => Ok(fixture(f)),
            None => Err(ProviderError(format!("404 {url}"))),
        }
    }
}

impl Fetch for &Fixtures {
    fn fetch(&self, url: &str) -> Result<String, ProviderError> {
        (*self).fetch(url)
    }
}

#[test]
fn final_scoreboard_maps_every_game() {
    let games = parse_scoreboard(&fixture("scoreboard-2026-10-13-final.json")).unwrap();
    assert_eq!(games.len(), 16);
    assert!(games.iter().all(|g| g.state == GameState::Final));

    let mtl = games.iter().find(|g| g.id == "401892490").unwrap();
    assert_eq!(
        (mtl.away.team.abbr.as_str(), mtl.home.team.abbr.as_str()),
        ("BUF", "MTL")
    );
    assert_eq!((mtl.away.score, mtl.home.score), (3, 4));
    assert!(mtl.shootout);
    assert_eq!(mtl.status_line(), "FINAL/SO");
    assert_eq!(mtl.home.team.color.as_deref(), Some("#c41230"));
    assert_eq!(mtl.home.team.name, "Montreal Canadiens");
    assert_eq!(mtl.start_unix, Some(1_791_930_600)); // 2026-10-13T22:30Z

    let lines: Vec<String> = games.iter().map(|g| g.status_line()).collect();
    assert_eq!(lines.iter().filter(|l| *l == "FINAL/SO").count(), 2);
    assert_eq!(lines.iter().filter(|l| *l == "FINAL/OT").count(), 1);
    assert_eq!(lines.iter().filter(|l| *l == "FINAL").count(), 13);
}

#[test]
fn scheduled_scoreboard() {
    let games = parse_scoreboard(&fixture("scoreboard-2026-10-14-scheduled.json")).unwrap();
    assert_eq!(games.len(), 2);
    for g in &games {
        assert_eq!(g.state, GameState::Scheduled);
        assert_eq!(g.period, 0);
        assert_eq!(g.clock, None);
        assert_eq!((g.home.score, g.away.score), (0, 0));
    }
    assert_eq!(games[0].away.team.abbr, "MTL");
    assert_eq!(games[0].home.team.abbr, "WSH");
}

#[test]
fn summary_goals_skip_shootout_attempts() {
    // BUF 3 @ MTL 4 (SO): 7 goals in play, 4 shootout attempts that are not.
    let plays = parse_scoring_plays(&fixture("summary-401892490.json"), "10", "2").unwrap();
    assert_eq!(plays.len(), 6);
    assert_eq!(plays.iter().filter(|p| p.side == Side::Away).count(), 3);
    assert_eq!(plays.iter().filter(|p| p.side == Side::Home).count(), 3);

    let first = &plays[0];
    assert_eq!(first.scorer, "Olen Zellweger");
    assert_eq!(first.assists, ["M. Samuelsson", "B. Malenstyn"]);
    assert_eq!((first.period, first.clock.as_deref()), (1, Some("2:16")));
    assert_eq!(first.strength, Some(Strength::Even));
    assert_eq!(plays[1].strength, Some(Strength::PowerPlay));
}

#[test]
fn every_regulation_goal_has_a_play() {
    let games = parse_scoreboard(&fixture("scoreboard-2026-10-13-final.json")).unwrap();
    let mut empty_net = 0;
    let mut short = 0;
    for g in &games {
        let plays = parse_scoring_plays(
            &fixture(&format!("summary-{}.json", g.id)),
            &g.home.team.id,
            &g.away.team.id,
        )
        .unwrap();
        let so_bonus = u16::from(g.shootout);
        let home = plays.iter().filter(|p| p.side == Side::Home).count() as u16;
        let away = plays.iter().filter(|p| p.side == Side::Away).count() as u16;
        assert_eq!(
            home + away + so_bonus,
            g.home.score + g.away.score,
            "game {}",
            g.id
        );
        empty_net += plays.iter().filter(|p| p.empty_net).count();
        short += plays
            .iter()
            .filter(|p| p.strength == Some(Strength::ShortHanded))
            .count();
    }
    assert_eq!(empty_net, 7);
    assert_eq!(short, 3);
}

#[test]
fn provider_fetches_summaries_once_for_finished_games() {
    let fx = Fixtures::final_night();
    let mut espn = Espn::with_base(&fx, "x");
    let games = espn.poll().unwrap();
    assert_eq!(games.len(), 16);
    assert!(games.iter().all(|g| !g.scoring_plays.is_empty()));
    assert_eq!(fx.requests().len(), 17);

    // Nothing changed: only the scoreboard is fetched again.
    espn.poll().unwrap();
    assert_eq!(fx.requests(), ["x/scoreboard"]);
}

#[test]
fn games_with_goals_wait_for_their_summary() {
    let mut fx = Fixtures::final_night();
    fx.routes.remove("x/summary?event=401892490");
    let mut espn = Espn::with_base(&fx, "x");
    let games = espn.poll().unwrap();
    assert_eq!(games.len(), 15);
    assert!(games.iter().all(|g| g.id != "401892490"));
}

#[test]
fn tracker_baseline_from_provider_is_silent() {
    let fx = Fixtures::final_night();
    let mut espn = Espn::with_base(&fx, "x");
    let mut tracker = Tracker::new();
    assert!(tracker.ingest(&espn.poll().unwrap()).is_empty());
    assert!(tracker.ingest(&espn.poll().unwrap()).is_empty());
}

#[test]
fn football_touchdowns_and_field_goals_from_summary() {
    // Atlanta (home, id 1) vs Chicago (away, id 3).
    let plays =
        parse_football_scoring_plays(&fixture("nfl-summary-401872999.json"), "1", "3").unwrap();
    assert_eq!(plays.len(), 8);
    let first = &plays[0];
    assert_eq!(first.side, Side::Away);
    assert_eq!(first.scorer, "Colston Loveland");
    assert_eq!(first.assists, ["Caleb Williams"]);
    assert_eq!(first.kind, Some(ScoreKind::Touchdown));
    assert_eq!((first.period, first.clock.as_deref()), (1, Some("8:15")));
    assert_eq!(first.score_after, Some((0, 7)));

    let rush = &plays[1];
    assert_eq!(rush.scorer, "D'Andre Swift");
    assert!(rush.assists.is_empty());

    let fg = &plays[5];
    assert_eq!(fg.kind, Some(ScoreKind::FieldGoal));
    assert_eq!(fg.scorer, "Cairo Santos");
    assert_eq!(fg.score_after, Some((7, 31)));
}
