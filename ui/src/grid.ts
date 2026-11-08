import { h, pulse } from "./dom";
import { scoreTag } from "./format";
import { scoreline, scorerLine, status, teamRow, teamVars } from "./parts";
import type { GoalRecord, Notice, Scoreboard } from "./store";
import type { GameSnapshot } from "./types";

export interface GridTiming {
  goalMs: number;
  finalMs: number;
  noGoalMs: number;
}

export const GRID_TIMING: GridTiming = { goalMs: 6000, finalMs: 4000, noGoalMs: 8000 };

interface CardState {
  el: HTMLElement;
  /** Goal whose slate is currently playing in this card. */
  showing?: GoalRecord;
  overlayTimer?: ReturnType<typeof setTimeout>;
  noGoalUntil: number;
}

/**
 * One scorebug card per game, in schedule order so cards never jump. Alerts
 * play inside the scoring game's card, so simultaneous goals need no queue.
 */
export class Grid {
  readonly el: HTMLElement;
  private cards = new Map<string, CardState>();

  constructor(
    private board: Scoreboard,
    private t: GridTiming = GRID_TIMING,
  ) {
    this.el = h("div.grid");
  }

  reset(): void {
    for (const c of this.cards.values()) clearTimeout(c.overlayTimer);
    this.cards.clear();
    this.el.replaceChildren();
  }

  handle(notices: Notice[], catchUp: boolean): void {
    let added = false;
    for (const n of notices) {
      switch (n.kind) {
        case "snapshot":
          if (!this.cards.has(n.game.id)) added = true;
          else this.redraw(n.game.id);
          break;
        case "goal":
          if (!catchUp) this.overlay(n.goal.gameId, "goal", n.goal);
          this.redraw(n.goal.gameId);
          break;
        case "scorer": {
          const c = this.cards.get(n.goal.gameId);
          if (c?.showing === n.goal) this.overlay(n.goal.gameId, "goal", n.goal, true);
          this.redraw(n.goal.gameId);
          break;
        }
        case "disallowed": {
          const c = this.cards.get(n.gameId);
          if (c && n.goal && c.showing === n.goal) this.clearOverlay(c);
          if (c && !catchUp) c.noGoalUntil = performance.now() + this.t.noGoalMs;
          this.redraw(n.gameId);
          if (c && !catchUp) setTimeout(() => this.redraw(n.gameId), this.t.noGoalMs + 50);
          break;
        }
        case "final":
          if (!catchUp) this.overlay(n.gameId, "final");
          break;
      }
    }
    if (added) this.rebuild();
  }

  private rebuild(): void {
    for (const g of this.board.bySchedule()) {
      if (!this.cards.has(g.id)) this.cards.set(g.id, { el: h("div"), noGoalUntil: 0 });
      this.redraw(g.id);
    }
    this.el.replaceChildren(...this.board.bySchedule().map((g) => this.cards.get(g.id)!.el));
  }

  private redraw(id: string): void {
    const c = this.cards.get(id);
    const g = this.board.games.get(id);
    if (!c || !g) return;
    const prev = [...c.el.querySelectorAll(".rows .score")].map((e) => e.textContent);
    const overlays = c.el.querySelectorAll(".card-alert");
    const last = this.board.lastGoal(id);
    const noGoal = c.noGoalUntil > performance.now();
    c.el.className = `card ${g.state}`;
    c.el.replaceChildren(
      h("header", null, status(g), noGoal ? h("span.nogoal", null, "NO GOAL") : null),
      h("div.rows", null, teamRow(g, "away", true), teamRow(g, "home", true)),
      h("footer", null, lastGoalLine(g, last)),
      ...overlays,
    );
    c.el.querySelectorAll(".rows .score").forEach((e, i) => {
      if (prev[i] != null && prev[i] !== e.textContent) pulse(e, "bump");
    });
  }

  private overlay(id: string, kind: "goal" | "final", goal?: GoalRecord, inPlace = false): void {
    const c = this.cards.get(id);
    const g = this.board.games.get(id);
    if (!c || !g) return;
    const team = goal ? g[goal.side].team : null;
    const w = g.home.score > g.away.score ? g.home : g.away.score > g.home.score ? g.away : null;
    const el = h(
      `div.card-alert.${kind}${inPlace ? ".static" : ""}`,
      { style: teamVars(team?.color ?? w?.team.color) },
      h("div.sweep"),
      h("div.ca-tag", null, kind === "goal" ? `${team!.abbr} ${scoreTag(goal!.detail)}` : status(g).textContent),
      goal
        ? scoreline(g, goal)
        : scoreline(g, { side: w === g.away ? "away" : "home", homeScore: g.home.score, awayScore: g.away.score }),
      goal ? h("div.ca-detail", null, scorerLine(goal.detail)) : null,
    );
    c.el.querySelectorAll(".card-alert:not(.out)").forEach((e) => e.remove());
    c.el.append(el);
    if (!inPlace) requestAnimationFrame(() => el.classList.add("in"));
    else el.classList.add("in");
    c.showing = goal;
    if (!inPlace) {
      clearTimeout(c.overlayTimer);
      c.overlayTimer = setTimeout(() => this.clearOverlay(c), kind === "goal" ? this.t.goalMs : this.t.finalMs);
    }
  }

  private clearOverlay(c: CardState): void {
    clearTimeout(c.overlayTimer);
    c.showing = undefined;
    const el = c.el.querySelector(".card-alert:not(.out)");
    if (!el) return;
    el.classList.remove("in");
    el.classList.add("out");
    setTimeout(() => el.remove(), 400);
  }
}

/** Latest goal: from events if we saw it happen, else the snapshot's plays
 * (a game that was already underway or over when we connected). */
function lastGoalLine(g: GameSnapshot, goal: GoalRecord | undefined): HTMLElement | null {
  const play = g.scoring_plays?.at(-1);
  const side = goal?.side ?? play?.side;
  if (!side) return null;
  const detail = goal ? goal.detail : play;
  return h("div.last", { style: teamVars(g[side].team.color) }, h("i.chip"), scorerLine(detail));
}
