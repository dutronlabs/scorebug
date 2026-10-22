use std::collections::HashMap;

use scorebug_core::{EventKind, Side};
use scorebug_sim::{Frame, run_night};

#[derive(Default, Debug)]
struct Tally {
    starts: u32,
    finals: u32,
    home_net: i32,
    away_net: i32,
    unresolved: i32,
    final_score: Option<(u16, u16)>,
}

fn tally(frames: &[Frame]) -> HashMap<String, Tally> {
    let mut games: HashMap<String, Tally> = HashMap::new();
    for e in frames.iter().flat_map(|f| &f.events) {
        let t = games.entry(e.game_id.clone()).or_default();
        match &e.kind {
            EventKind::GameStart => t.starts += 1,
            EventKind::Goal { side, detail, .. } => {
                match side {
                    Side::Home => t.home_net += 1,
                    Side::Away => t.away_net += 1,
                }
                if detail.is_none() {
                    t.unresolved += 1;
                }
            }
            // Simulated reversals never had a scorer, so this settles one.
            EventKind::GoalDisallowed { side, .. } => {
                match side {
                    Side::Home => t.home_net -= 1,
                    Side::Away => t.away_net -= 1,
                }
                t.unresolved -= 1;
            }
            EventKind::ScorerResolved { .. } => t.unresolved -= 1,
            EventKind::Final {
                home_score,
                away_score,
                ..
            } => {
                t.finals += 1;
                t.final_score = Some((*home_score, *away_score));
            }
            _ => {}
        }
    }
    games
}

#[test]
fn every_game_starts_and_finishes_once_and_the_events_add_up() {
    for seed in 1..=25 {
        let frames = run_night(seed, 16, 5, 30.0);
        let games = tally(&frames);
        assert_eq!(games.len(), 16, "seed {seed}");
        for (id, t) in &games {
            assert_eq!((t.starts, t.finals), (1, 1), "seed {seed} {id}: {t:?}");
            let (h, a) = t.final_score.unwrap();
            // Shootout winners are reported by Final, not as goals.
            let so = (i32::from(h) - t.home_net).abs() + (i32::from(a) - t.away_net).abs();
            assert!(
                so <= 1,
                "seed {seed} {id}: net goals {t:?} vs final {h}-{a}"
            );
            assert_eq!(t.unresolved, 0, "seed {seed} {id}: {t:?}");
        }
    }
}

#[test]
fn same_seed_same_night() {
    let a = serde_json::to_string(&run_night(7, 16, 5, 30.0)).unwrap();
    let b = serde_json::to_string(&run_night(7, 16, 5, 30.0)).unwrap();
    assert_eq!(a, b);
    assert_ne!(
        a,
        serde_json::to_string(&run_night(8, 16, 5, 30.0)).unwrap()
    );
}

#[test]
fn a_night_exercises_the_interesting_cases() {
    let frames: Vec<_> = (1..=10).flat_map(|s| run_night(s, 16, 5, 30.0)).collect();
    let kinds: Vec<_> = frames
        .iter()
        .flat_map(|f| &f.events)
        .map(|e| &e.kind)
        .collect();
    let any = |f: fn(&EventKind) -> bool| kinds.iter().any(|k| f(k));
    assert!(any(|k| matches!(k, EventKind::ScorerResolved { .. })));
    assert!(any(|k| matches!(k, EventKind::GoalDisallowed { .. })));
    assert!(any(|k| matches!(
        k,
        EventKind::Final {
            overtime_periods: 1..,
            ..
        }
    )));
    assert!(any(|k| matches!(
        k,
        EventKind::Final { shootout: true, .. }
    )));
}

#[test]
fn frames_only_carry_changed_snapshots_after_the_first() {
    let frames = run_night(3, 16, 5, 30.0);
    assert_eq!(frames[0].snapshots.len(), 16);
    // Early in the night most games are still scheduled and unchanged.
    assert!(frames[1].snapshots.len() < 16);
    assert!(frames.windows(2).all(|w| w[0].t_ms <= w[1].t_ms));
}
