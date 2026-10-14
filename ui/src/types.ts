// Mirrors the serde output of `scorebug-core` (crates/core/src/{model,event}.rs).

export type Side = "home" | "away";
export type GameState = "scheduled" | "live" | "intermission" | "final" | "postponed";
export type Strength = "even" | "power_play" | "short_handed" | "penalty_shot";

export interface Team {
  id: string;
  abbr: string;
  name: string;
  color?: string;
  logo?: string;
}

export interface TeamScore {
  team: Team;
  score: number;
}

export interface ScoringPlay {
  id: string;
  side: Side;
  scorer: string;
  assists?: string[];
  strength?: Strength;
  empty_net: boolean;
  period: number;
  clock?: string;
}

export interface GameSnapshot {
  id: string;
  sport: "hockey";
  state: GameState;
  start_unix?: number;
  period: number;
  clock?: string;
  home: TeamScore;
  away: TeamScore;
  shootout: boolean;
  scoring_plays?: ScoringPlay[];
}

export interface GoalDetail {
  scorer: string;
  assists?: string[];
  strength?: Strength;
  empty_net: boolean;
  period: number;
  clock?: string;
}

export type EventKind =
  | { type: "game_start" }
  | { type: "period_start"; period: number }
  | { type: "period_end"; period: number }
  | { type: "goal"; side: Side; home_score: number; away_score: number; detail?: GoalDetail }
  | { type: "goal_disallowed"; side: Side; home_score: number; away_score: number }
  | { type: "scorer_resolved"; goal_seq: number; detail: GoalDetail }
  | { type: "final"; home_score: number; away_score: number; overtime_periods: number; shootout: boolean };

export type GameEvent = EventKind & { seq: number; game_id: string };

/** One poll, as produced by `scorebug-sim` (and later the live provider). */
export interface Frame {
  t_ms: number;
  snapshots: GameSnapshot[];
  events: GameEvent[];
}
