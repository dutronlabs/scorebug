use scorebug_core::*;

fn team(abbr: &str) -> Team {
    Team {
        id: abbr.into(),
        abbr: abbr.into(),
        name: abbr.into(),
        color: None,
        logo: None,
    }
}

fn game(state: GameState, period: u8, home: u16, away: u16) -> GameSnapshot {
    GameSnapshot {
        id: "g1".into(),
        sport: Sport::Hockey,
        state,
        start_unix: None,
        period,
        clock: Some("10:00".into()),
        home: TeamScore {
            team: team("BOS"),
            score: home,
        },
        away: TeamScore {
            team: team("NYR"),
            score: away,
        },
        shootout: false,
        scoring_plays: vec![],
    }
}

fn live(period: u8, home: u16, away: u16) -> GameSnapshot {
    game(GameState::Live, period, home, away)
}

fn play(id: &str, side: Side, scorer: &str) -> ScoringPlay {
    ScoringPlay {
        id: id.into(),
        side,
        scorer: scorer.into(),
        assists: vec![],
        strength: None,
        empty_net: false,
        period: 1,
        clock: None,
        kind: None,
        score_after: None,
    }
}

fn with_plays(mut g: GameSnapshot, plays: Vec<ScoringPlay>) -> GameSnapshot {
    g.scoring_plays = plays;
    g
}

fn kinds(events: &[GameEvent]) -> Vec<&EventKind> {
    events.iter().map(|e| &e.kind).collect()
}

fn scorer(kind: &EventKind) -> Option<&str> {
    match kind {
        EventKind::Goal { detail, .. } => detail.as_ref().map(|d| d.scorer.as_str()),
        _ => None,
    }
}

#[test]
fn first_snapshot_is_a_silent_baseline() {
    let mut t = Tracker::new();
    let g = with_plays(live(2, 3, 2), vec![play("a", Side::Home, "Old Goal")]);
    assert!(t.ingest(std::slice::from_ref(&g)).is_empty());
    assert!(t.ingest(&[g]).is_empty());
}

#[test]
fn goal_with_scorer() {
    let mut t = Tracker::new();
    t.ingest(&[live(1, 0, 0)]);
    let ev = t.ingest(&[with_plays(
        live(1, 1, 0),
        vec![play("a", Side::Home, "Pasta")],
    )]);
    assert_eq!(ev.len(), 1);
    assert!(matches!(
        ev[0].kind,
        EventKind::Goal {
            side: Side::Home,
            home_score: 1,
            away_score: 0,
            ..
        }
    ));
    assert_eq!(scorer(&ev[0].kind), Some("Pasta"));
}

#[test]
fn two_goals_in_one_poll_get_running_scores_and_their_own_scorers() {
    let mut t = Tracker::new();
    t.ingest(&[live(1, 0, 0)]);
    let ev = t.ingest(&[with_plays(
        live(1, 2, 0),
        vec![play("a", Side::Home, "One"), play("b", Side::Home, "Two")],
    )]);
    assert_eq!(ev.len(), 2);
    assert!(matches!(ev[0].kind, EventKind::Goal { home_score: 1, .. }));
    assert!(matches!(ev[1].kind, EventKind::Goal { home_score: 2, .. }));
    assert_eq!(scorer(&ev[0].kind), Some("One"));
    assert_eq!(scorer(&ev[1].kind), Some("Two"));
    assert!(ev[0].seq < ev[1].seq);
}

#[test]
fn scorer_arriving_late_is_resolved_against_the_original_goal() {
    let mut t = Tracker::new();
    t.ingest(&[live(1, 0, 0)]);
    let goal = t.ingest(&[live(1, 0, 1)]);
    assert_eq!(scorer(&goal[0].kind), None);

    let still_waiting = t.ingest(&[live(1, 0, 1)]);
    assert!(still_waiting.is_empty());

    let late = t.ingest(&[with_plays(
        live(1, 0, 1),
        vec![play("a", Side::Away, "Late")],
    )]);
    assert_eq!(late.len(), 1);
    match &late[0].kind {
        EventKind::ScorerResolved { goal_seq, detail } => {
            assert_eq!(*goal_seq, goal[0].seq);
            assert_eq!(detail.scorer, "Late");
        }
        k => panic!("unexpected {k:?}"),
    }
    // Never resolved twice.
    let again = t.ingest(&[with_plays(
        live(1, 0, 1),
        vec![play("a", Side::Away, "Late")],
    )]);
    assert!(again.is_empty());
}

#[test]
fn play_appearing_before_the_score_attaches_to_the_goal() {
    let mut t = Tracker::new();
    t.ingest(&[live(1, 0, 0)]);
    let early = t.ingest(&[with_plays(
        live(1, 0, 0),
        vec![play("a", Side::Home, "Early")],
    )]);
    assert!(early.is_empty());
    let ev = t.ingest(&[with_plays(
        live(1, 1, 0),
        vec![play("a", Side::Home, "Early")],
    )]);
    assert_eq!(ev.len(), 1);
    assert_eq!(scorer(&ev[0].kind), Some("Early"));
}

#[test]
fn score_decrease_is_a_disallowed_goal_and_cancels_its_pending_scorer() {
    let mut t = Tracker::new();
    t.ingest(&[live(2, 1, 1)]);
    let goal = t.ingest(&[live(2, 2, 1)]);
    assert_eq!(goal.len(), 1);
    let ev = t.ingest(&[live(2, 1, 1)]);
    assert_eq!(
        kinds(&ev),
        vec![&EventKind::GoalDisallowed {
            side: Side::Home,
            home_score: 1,
            away_score: 1
        }]
    );
    // A genuine play for the other team must not be matched to the reversed goal.
    let ev = t.ingest(&[with_plays(
        live(2, 1, 1),
        vec![play("x", Side::Home, "Ghost")],
    )]);
    assert!(ev.is_empty());
}

#[test]
fn lifecycle_events() {
    let mut t = Tracker::new();
    t.ingest(&[game(GameState::Scheduled, 0, 0, 0)]);
    let ev = t.ingest(&[live(1, 0, 0)]);
    assert_eq!(kinds(&ev), vec![&EventKind::GameStart]);

    let ev = t.ingest(&[game(GameState::Intermission, 1, 0, 0)]);
    assert_eq!(kinds(&ev), vec![&EventKind::PeriodEnd { period: 1 }]);

    let ev = t.ingest(&[live(2, 0, 0)]);
    assert_eq!(kinds(&ev), vec![&EventKind::PeriodStart { period: 2 }]);

    let ev = t.ingest(&[game(GameState::Final, 3, 2, 1)]);
    assert_eq!(ev.len(), 4); // three missed goals, then the final
    assert!(matches!(
        ev.last().unwrap().kind,
        EventKind::Final {
            home_score: 2,
            away_score: 1,
            overtime_periods: 0,
            shootout: false
        }
    ));
}

#[test]
fn missed_intermission_still_ends_and_starts_periods() {
    let mut t = Tracker::new();
    t.ingest(&[live(1, 0, 0)]);
    let ev = t.ingest(&[live(2, 0, 0)]);
    assert_eq!(
        kinds(&ev),
        vec![
            &EventKind::PeriodEnd { period: 1 },
            &EventKind::PeriodStart { period: 2 }
        ]
    );
}

#[test]
fn goal_in_the_same_poll_as_game_start_comes_after_it() {
    let mut t = Tracker::new();
    t.ingest(&[game(GameState::Scheduled, 0, 0, 0)]);
    let ev = t.ingest(&[live(1, 1, 0)]);
    assert!(matches!(ev[0].kind, EventKind::GameStart));
    assert!(matches!(ev[1].kind, EventKind::Goal { .. }));
}

#[test]
fn nothing_after_final_except_late_scorers() {
    let mut t = Tracker::new();
    t.ingest(&[live(3, 1, 1)]);
    let ev = t.ingest(&[game(GameState::Final, 4, 2, 1)]);
    // Overtime winner without a scorer yet, then the final.
    assert!(matches!(ev[0].kind, EventKind::Goal { .. }));
    assert!(matches!(
        ev[1].kind,
        EventKind::Final {
            overtime_periods: 1,
            ..
        }
    ));

    // Score "correction" after the horn is ignored.
    assert!(t.ingest(&[game(GameState::Final, 4, 3, 1)]).is_empty());

    let ev = t.ingest(&[with_plays(
        game(GameState::Final, 4, 3, 1),
        vec![play("w", Side::Home, "OT Hero")],
    )]);
    assert!(matches!(ev[0].kind, EventKind::ScorerResolved { .. }));
}

#[test]
fn shootout_score_is_not_a_goal() {
    let mut t = Tracker::new();
    let mut so = live(5, 2, 2);
    so.shootout = true;
    t.ingest(&[so.clone()]);
    let mut done = game(GameState::Final, 5, 3, 2);
    done.shootout = true;
    let ev = t.ingest(&[done]);
    assert_eq!(
        kinds(&ev),
        vec![&EventKind::Final {
            home_score: 3,
            away_score: 2,
            overtime_periods: 1,
            shootout: true
        }]
    );
}

#[test]
fn games_are_tracked_independently() {
    let mut t = Tracker::new();
    let mut a = live(1, 0, 0);
    let mut b = live(1, 0, 0);
    a.id = "a".into();
    b.id = "b".into();
    t.ingest(&[a.clone(), b.clone()]);
    a.home.score = 1;
    let ev = t.ingest(&[a, b]);
    assert_eq!(ev.len(), 1);
    assert_eq!(ev[0].game_id, "a");
}

#[test]
fn status_lines() {
    assert_eq!(live(2, 0, 0).status_line(), "2nd 10:00");
    assert_eq!(
        game(GameState::Intermission, 1, 0, 0).status_line(),
        "End 1st"
    );
    assert_eq!(game(GameState::Final, 3, 0, 0).status_line(), "FINAL");
    assert_eq!(game(GameState::Final, 4, 0, 0).status_line(), "FINAL/OT");
    assert_eq!(game(GameState::Final, 5, 0, 0).status_line(), "FINAL/2OT");
    let mut so = game(GameState::Final, 5, 0, 0);
    so.shootout = true;
    assert_eq!(so.status_line(), "FINAL/SO");
}

#[test]
fn events_serialize_with_a_flat_type_tag() {
    let ev = GameEvent {
        seq: 7,
        game_id: "g1".into(),
        kind: EventKind::PeriodStart { period: 2 },
    };
    let v: serde_json::Value = serde_json::to_value(&ev).unwrap();
    assert_eq!(v["type"], "period_start");
    assert_eq!(v["period"], 2);
    assert_eq!(v["seq"], 7);
}

// ---- football ----------------------------------------------------------

fn football(period: u8, home: u16, away: u16, plays: Vec<ScoringPlay>) -> GameSnapshot {
    let mut g = live(period, home, away);
    g.sport = Sport::Football;
    g.scoring_plays = plays;
    g
}

fn score_play(id: &str, side: Side, kind: ScoreKind, after: (u16, u16)) -> ScoringPlay {
    ScoringPlay {
        kind: Some(kind),
        score_after: Some(after),
        ..play(id, side, "Player")
    }
}

#[test]
fn touchdown_is_one_goal_and_the_extra_point_is_not() {
    let td = score_play("p1", Side::Home, ScoreKind::Touchdown, (7, 0));
    let mut t = Tracker::new();
    t.ingest(&[football(1, 0, 0, vec![])]);
    // Six points first, then the kick makes it seven: still one event.
    let ev = t.ingest(&[football(1, 6, 0, vec![td.clone()])]);
    assert_eq!(ev.len(), 1);
    let EventKind::Goal {
        side,
        home_score,
        detail,
        ..
    } = &ev[0].kind
    else {
        panic!("{:?}", ev[0].kind)
    };
    assert_eq!((*side, *home_score), (Side::Home, 7));
    assert_eq!(detail.as_ref().unwrap().kind, Some(ScoreKind::Touchdown));
    assert!(t.ingest(&[football(1, 7, 0, vec![td])]).is_empty());
}

#[test]
fn field_goal_is_a_goal_and_a_safety_is_not() {
    let fg = score_play("p1", Side::Away, ScoreKind::FieldGoal, (0, 3));
    let mut safety = play("p2", Side::Home, "Defense");
    safety.score_after = Some((2, 3));
    let mut t = Tracker::new();
    t.ingest(&[football(1, 0, 0, vec![])]);
    let ev = t.ingest(&[football(1, 0, 3, vec![fg.clone()])]);
    assert!(matches!(
        ev[0].kind,
        EventKind::Goal {
            side: Side::Away,
            away_score: 3,
            ..
        }
    ));
    assert!(t.ingest(&[football(1, 2, 3, vec![fg, safety])]).is_empty());
}

#[test]
fn football_baseline_is_silent_and_nullified_touchdown_is_disallowed() {
    let old = score_play("p1", Side::Home, ScoreKind::FieldGoal, (3, 0));
    let td = score_play("p2", Side::Away, ScoreKind::Touchdown, (3, 7));
    let mut t = Tracker::new();
    assert!(t.ingest(&[football(2, 3, 0, vec![old.clone()])]).is_empty());
    assert_eq!(
        t.ingest(&[football(2, 3, 7, vec![old.clone(), td])]).len(),
        1
    );
    let ev = t.ingest(&[football(2, 3, 0, vec![old])]);
    assert!(matches!(
        ev[0].kind,
        EventKind::GoalDisallowed {
            side: Side::Away,
            away_score: 0,
            ..
        }
    ));
}
