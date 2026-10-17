import type { Frame, GameEvent, GameSnapshot, GoalDetail, Side } from "./types";

/** A goal as the UI knows it. Shared by reference, so a late scorer shows up
 * wherever the goal is already displayed or queued. */
export interface GoalRecord {
  seq: number;
  gameId: string;
  side: Side;
  homeScore: number;
  awayScore: number;
  detail?: GoalDetail;
  disallowed: boolean;
}

export type Notice =
  | { kind: "snapshot"; game: GameSnapshot }
  | { kind: "goal"; goal: GoalRecord }
  | { kind: "scorer"; goal: GoalRecord }
  | { kind: "disallowed"; gameId: string; side: Side; goal?: GoalRecord }
  | { kind: "final"; gameId: string; event: Extract<GameEvent, { type: "final" }> };

const RANK: Record<GameSnapshot["state"], number> = {
  live: 0,
  intermission: 0,
  scheduled: 1,
  final: 2,
  postponed: 3,
};

const byStart = (a: GameSnapshot, b: GameSnapshot) =>
  (a.start_unix ?? 0) - (b.start_unix ?? 0) || a.id.localeCompare(b.id);

/** Current state of every game, built from replayed or live frames. */
export class Scoreboard {
  readonly games = new Map<string, GameSnapshot>();
  private goals = new Map<number, GoalRecord>();
  private byGame = new Map<string, GoalRecord[]>();

  reset(): void {
    this.games.clear();
    this.goals.clear();
    this.byGame.clear();
  }

  apply(frame: Frame): Notice[] {
    const out: Notice[] = [];
    for (const s of frame.snapshots) {
      this.games.set(s.id, s);
      out.push({ kind: "snapshot", game: s });
    }
    for (const e of frame.events) {
      const n = this.event(e);
      if (n) out.push(n);
    }
    return out;
  }

  private event(e: GameEvent): Notice | null {
    switch (e.type) {
      case "goal": {
        const goal: GoalRecord = {
          seq: e.seq,
          gameId: e.game_id,
          side: e.side,
          homeScore: e.home_score,
          awayScore: e.away_score,
          detail: e.detail,
          disallowed: false,
        };
        this.goals.set(e.seq, goal);
        const list = this.byGame.get(e.game_id) ?? [];
        list.push(goal);
        this.byGame.set(e.game_id, list);
        return { kind: "goal", goal };
      }
      case "scorer_resolved": {
        const goal = this.goals.get(e.goal_seq);
        if (!goal) return null;
        goal.detail = e.detail;
        return { kind: "scorer", goal };
      }
      case "goal_disallowed": {
        // The tracker reports a score decrease; the goal taken back is the
        // latest standing one for that side.
        const goal = (this.byGame.get(e.game_id) ?? [])
          .filter((g) => g.side === e.side && !g.disallowed)
          .at(-1);
        if (goal) goal.disallowed = true;
        return { kind: "disallowed", gameId: e.game_id, side: e.side, goal };
      }
      case "final":
        return { kind: "final", gameId: e.game_id, event: e };
      default:
        return null;
    }
  }

  /** Latest goal still on the board. */
  lastGoal(gameId: string): GoalRecord | undefined {
    return (this.byGame.get(gameId) ?? []).filter((g) => !g.disallowed).at(-1);
  }

  /** Stable order: by scheduled start. */
  bySchedule(): GameSnapshot[] {
    return [...this.games.values()].sort(byStart);
  }

  /** Ticker order: live games first, then upcoming, then finished. */
  byPriority(): GameSnapshot[] {
    return [...this.games.values()].sort((a, b) => RANK[a.state] - RANK[b.state] || byStart(a, b));
  }
}

export function paginate<T>(items: T[], size: number): T[][] {
  const pages: T[][] = [];
  for (let i = 0; i < items.length; i += size) pages.push(items.slice(i, i + size));
  return pages;
}

export interface Alert {
  id: number;
  gameId: string;
  /** Higher plays first. */
  priority: number;
  createdAt: number;
  body: { kind: "goal"; goal: GoalRecord } | { kind: "final"; event: Extract<GameEvent, { type: "final" }> };
}

export type Slot = { kind: "single"; alert: Alert } | { kind: "summary"; alerts: Alert[] };

export const PRIORITY = { goal: 2, final: 1 } as const;

/**
 * Takeover queue for the ticker. Highest priority first, then oldest. An
 * alert that has waited longer than `staleMs` jumps ahead of fresh ones (so a
 * busy night cannot starve finals), and two or more such alerts come out as
 * one summary instead of one slate each.
 */
export class AlertQueue {
  private items: Alert[] = [];
  private nextId = 1;

  constructor(readonly staleMs = 20_000) {}

  get size(): number {
    return this.items.length;
  }

  push(gameId: string, body: Alert["body"], now: number): Alert {
    const alert: Alert = { id: this.nextId++, gameId, priority: PRIORITY[body.kind], createdAt: now, body };
    this.items.push(alert);
    return alert;
  }

  drop(pred: (a: Alert) => boolean): void {
    this.items = this.items.filter((a) => !pred(a));
  }

  clear(): void {
    this.items = [];
  }

  take(now: number): Slot | null {
    if (!this.items.length) return null;
    const stale = this.items.filter((a) => now - a.createdAt > this.staleMs);
    type Candidate = { slot: Slot; stale: boolean; priority: number; createdAt: number };
    const candidates: Candidate[] = [];
    let pool = this.items;
    if (stale.length >= 2) {
      candidates.push({
        slot: { kind: "summary", alerts: stale },
        stale: true,
        priority: Math.max(...stale.map((a) => a.priority)),
        createdAt: Math.min(...stale.map((a) => a.createdAt)),
      });
      pool = this.items.filter((a) => !stale.includes(a));
    }
    for (const a of pool) candidates.push({
        slot: { kind: "single", alert: a },
        stale: stale.includes(a),
        priority: a.priority,
        createdAt: a.createdAt,
      });
    candidates.sort((x, y) => +y.stale - +x.stale || y.priority - x.priority || x.createdAt - y.createdAt);
    const pick = candidates[0].slot;
    const taken = pick.kind === "summary" ? pick.alerts : [pick.alert];
    this.items = this.items.filter((a) => !taken.includes(a));
    return pick;
  }
}
