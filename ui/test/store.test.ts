import { describe, expect, it } from "vitest";
import { AlertQueue, Scoreboard, paginate, type GoalRecord } from "../src/store";
import type { Frame, GameEvent, GameSnapshot, GameState } from "../src/types";

function game(id: string, state: GameState, start: number, home = 0, away = 0): GameSnapshot {
  const team = (abbr: string) => ({ id: abbr, abbr, name: abbr });
  return {
    id,
    sport: "hockey",
    state,
    start_unix: start,
    period: state === "scheduled" ? 0 : 1,
    home: { team: team(`H${id}`), score: home },
    away: { team: team(`A${id}`), score: away },
    shootout: false,
  };
}

const frame = (snapshots: GameSnapshot[], events: GameEvent[] = []): Frame => ({ t_ms: 0, snapshots, events });

const goalRec = (seq: number, gameId = "g"): GoalRecord => ({
  seq,
  gameId,
  side: "home",
  homeScore: 1,
  awayScore: 0,
  disallowed: false,
});

describe("Scoreboard", () => {
  it("orders live first, then upcoming, then final", () => {
    const b = new Scoreboard();
    b.apply(
      frame([
        game("final", "final", 1),
        game("later", "scheduled", 50),
        game("live", "live", 30),
        game("break", "intermission", 20),
        game("soon", "scheduled", 40),
      ]),
    );
    expect(b.byPriority().map((g) => g.id)).toEqual(["break", "live", "soon", "later", "final"]);
    expect(b.bySchedule().map((g) => g.id)).toEqual(["final", "break", "live", "soon", "later"]);
  });

  it("attaches a late scorer to the goal it belongs to", () => {
    const b = new Scoreboard();
    b.apply(frame([game("g", "live", 0)]));
    const [n] = b.apply(frame([], [{ seq: 7, game_id: "g", type: "goal", side: "home", home_score: 1, away_score: 0 }]));
    expect(n.kind).toBe("goal");
    const detail = { scorer: "TOR #9", empty_net: false, period: 1 };
    const [r] = b.apply(frame([], [{ seq: 8, game_id: "g", type: "scorer_resolved", goal_seq: 7, detail }]));
    expect(r.kind === "scorer" && r.goal.detail).toEqual(detail);
    expect(b.lastGoal("g")?.detail).toEqual(detail);
  });

  it("takes back the latest goal for the disallowed side", () => {
    const b = new Scoreboard();
    b.apply(frame([game("g", "live", 0)]));
    b.apply(
      frame(
        [],
        [
          { seq: 1, game_id: "g", type: "goal", side: "home", home_score: 1, away_score: 0 },
          { seq: 2, game_id: "g", type: "goal", side: "away", home_score: 1, away_score: 1 },
          { seq: 3, game_id: "g", type: "goal", side: "home", home_score: 2, away_score: 1 },
        ],
      ),
    );
    const [n] = b.apply(frame([], [{ seq: 4, game_id: "g", type: "goal_disallowed", side: "home", home_score: 1, away_score: 1 }]));
    expect(n.kind === "disallowed" && n.goal?.seq).toBe(3);
    expect(b.lastGoal("g")?.seq).toBe(2);
  });
});

describe("paginate", () => {
  it("chunks", () => {
    expect(paginate([1, 2, 3, 4, 5], 2)).toEqual([[1, 2], [3, 4], [5]]);
    expect(paginate([], 4)).toEqual([]);
  });
});

describe("AlertQueue", () => {
  const goal = (q: AlertQueue, seq: number, now: number, gameId = "g") =>
    q.push(gameId, { kind: "goal", goal: goalRec(seq, gameId) }, now);
  const final = (q: AlertQueue, now: number) =>
    q.push("f", { kind: "final", event: { seq: 0, game_id: "f", type: "final", home_score: 1, away_score: 0, overtime_periods: 0, shootout: false } }, now);

  it("plays goals before finals, oldest first", () => {
    const q = new AlertQueue(20_000);
    final(q, 0);
    goal(q, 2, 100);
    goal(q, 1, 50);
    const order = [q.take(200), q.take(200), q.take(200)].map((s) =>
      s?.kind === "single" ? (s.alert.body.kind === "goal" ? s.alert.body.goal.seq : "final") : null,
    );
    expect(order).toEqual([1, 2, "final"]);
    expect(q.take(200)).toBeNull();
  });

  it("collapses two or more stale alerts into one summary", () => {
    const q = new AlertQueue(20_000);
    goal(q, 1, 0, "a");
    goal(q, 2, 1_000, "b");
    goal(q, 3, 25_000, "c");
    const s = q.take(25_500);
    expect(s?.kind).toBe("summary");
    expect(s?.kind === "summary" && s.alerts.map((a) => a.gameId)).toEqual(["a", "b"]);
    const next = q.take(25_500);
    expect(next?.kind === "single" && next.alert.gameId).toBe("c");
  });

  it("shows a single stale alert on its own", () => {
    const q = new AlertQueue(20_000);
    goal(q, 1, 0);
    expect(q.take(30_000)?.kind).toBe("single");
  });

  it("does not let fresh goals starve a stale final", () => {
    const q = new AlertQueue(20_000);
    final(q, 0);
    goal(q, 1, 21_000);
    const s = q.take(21_000);
    expect(s?.kind === "single" && s.alert.body.kind).toBe("final");
  });

  it("drops alerts by predicate", () => {
    const q = new AlertQueue();
    goal(q, 1, 0);
    goal(q, 2, 0);
    q.drop((a) => a.body.kind === "goal" && a.body.goal.seq === 1);
    expect(q.size).toBe(1);
  });
});
