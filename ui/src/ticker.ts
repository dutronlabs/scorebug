import { h, pulse } from "./dom";
import { goalTime, scoreTag, statusLine } from "./format";
import { scoreline, scorerLine, status, teamRow, teamVars, winner } from "./parts";
import { AlertQueue, paginate, type Alert, type Notice, type Scoreboard, type Slot } from "./store";
import type { GameSnapshot } from "./types";

export interface TickerTiming {
  pageMs: number;
  fadeMs: number;
  goalMs: number;
  finalMs: number;
  summaryMs: number;
  staleMs: number;
  /** How long a quiet "no goal" marker stays on a cell. */
  noGoalMs: number;
}

export const TICKER_TIMING: TickerTiming = {
  pageMs: 5500,
  fadeMs: 300,
  goalMs: 6000,
  finalMs: 4000,
  summaryMs: 7000,
  staleMs: 20_000,
  noGoalMs: 8000,
};

const CELL_MIN_PX = 300;

const LEAGUE: Record<GameSnapshot["sport"], string> = { hockey: "NHL", basketball: "NBA", football: "NFL" };

/** Brand label for a page, which only ever holds one league. */
const league = (games: GameSnapshot[]) => (games[0] ? LEAGUE[games[0].sport] : "NHL");

/**
 * Bottom-line ticker. Rotates pages of games and hands the whole stage to a
 * slate when a goal or final comes in. Only one slate shows at a time; the
 * rest wait in an [`AlertQueue`].
 */
export class Ticker {
  readonly el: HTMLElement;
  private stage: HTMLElement;
  private dots: HTMLElement;
  private brand: HTMLElement;
  private current: HTMLElement | null = null;
  private queue: AlertQueue;
  private mode: "pages" | "slate" = "pages";
  private showing: Slot | null = null;
  private page = 0;
  private pageIds: string[] = [];
  private timer: ReturnType<typeof setTimeout> | null = null;
  private noGoal = new Map<string, number>();

  constructor(
    private board: Scoreboard,
    private t: TickerTiming = TICKER_TIMING,
  ) {
    this.queue = new AlertQueue(t.staleMs);
    this.dots = h("span.dots");
    this.stage = h("div.tk-stage");
    this.brand = h("b", null, "NHL");
    this.el = h("div.ticker", null, h("div.tk-brand", null, this.brand, this.dots), this.stage);
    this.el.style.setProperty("--fade", `${t.fadeMs}ms`);
  }

  reset(): void {
    this.queue.clear();
    this.noGoal.clear();
    this.showing = null;
    this.mode = "pages";
    this.page = 0;
    this.showPage(false);
  }

  handle(notices: Notice[], catchUp: boolean): void {
    const now = performance.now();
    let pagesDirty = false;
    for (const n of notices) {
      switch (n.kind) {
        case "snapshot":
          if (this.pageIds.includes(n.game.id)) this.refreshCell(n.game);
          else pagesDirty ||= this.pageIds.length === 0;
          break;
        case "goal":
          if (!catchUp) this.queue.push(n.goal.gameId, { kind: "goal", goal: n.goal }, now);
          break;
        case "scorer":
          if (this.isShowingGoal(n.goal.seq)) this.redrawSlate();
          break;
        case "disallowed":
          if (n.goal) {
            const seq = n.goal.seq;
            this.queue.drop((a) => a.body.kind === "goal" && a.body.goal.seq === seq);
            if (this.isShowingGoal(seq)) this.endSlate();
          }
          if (!catchUp) {
            this.noGoal.set(n.gameId, now + this.t.noGoalMs);
            const g = this.board.games.get(n.gameId);
            if (g && this.pageIds.includes(g.id)) this.refreshCell(g);
          }
          break;
        case "final":
          if (!catchUp) this.queue.push(n.gameId, { kind: "final", event: n.event }, now);
          break;
      }
    }
    if (pagesDirty && this.mode === "pages") this.showPage(false);
    if (this.mode === "pages" && this.queue.size) this.nextSlate();
  }

  // ---- pages -------------------------------------------------------------

  private perPage(): number {
    const w = this.stage.clientWidth || window.innerWidth;
    return Math.max(2, Math.min(6, Math.floor(w / CELL_MIN_PX)));
  }

  private showPage(advance: boolean): void {
    this.mode = "pages";
    const per = this.perPage();
    this.stage.style.setProperty("--per-page", `${per}`);
    // Pages never mix leagues: paginate each sport on its own.
    const all = this.board.byPriority();
    const pages = (Object.keys(LEAGUE) as GameSnapshot["sport"][]).flatMap((sport) =>
      paginate(
        all.filter((g) => g.sport === sport),
        per,
      ),
    );
    if (advance) this.page++;
    if (this.page >= pages.length) this.page = 0;
    const games = pages[this.page] ?? [];
    const ids = games.map((g) => g.id);
    this.brand.textContent = league(games);
    const same = ids.join() === this.pageIds.join();
    this.pageIds = ids;
    if (same && this.current?.classList.contains("page")) {
      games.forEach((g) => this.refreshCell(g));
    } else {
      this.swap(h("div.layer.page", null, ...games.map((g) => this.cell(g))));
    }
    this.dots.replaceChildren(...pages.map((_, i) => h(`i${i === this.page ? ".on" : ""}`)));
    this.schedule(this.t.pageMs, () => this.showPage(true));
  }

  private cell(g: GameSnapshot): HTMLElement {
    const noGoal = (this.noGoal.get(g.id) ?? 0) > performance.now();
    return h(
      `div.cell.${g.state}`,
      { "data-game": g.id },
      h("div.rows", null, teamRow(g, "away"), teamRow(g, "home")),
      h("div.meta", null, status(g), noGoal ? h("span.nogoal", null, "NO GOAL") : null),
    );
  }

  private refreshCell(g: GameSnapshot): void {
    const old = this.current?.querySelector<HTMLElement>(`.cell[data-game="${g.id}"]`);
    if (!old) return;
    const prev = [...old.querySelectorAll(".score")].map((e) => e.textContent);
    const next = this.cell(g);
    old.replaceWith(next);
    next.querySelectorAll(".score").forEach((e, i) => {
      if (prev[i] != null && prev[i] !== e.textContent) pulse(e, "bump");
    });
  }

  // ---- slates ------------------------------------------------------------

  private nextSlate(): void {
    const slot = this.queue.take(performance.now());
    if (!slot) {
      this.showing = null;
      this.pageIds = [];
      this.showPage(false);
      return;
    }
    this.mode = "slate";
    this.showing = slot;
    this.pageIds = [];
    this.swap(this.slate(slot));
    this.dots.replaceChildren();
    const ms =
      slot.kind === "summary"
        ? this.t.summaryMs
        : slot.alert.body.kind === "goal"
          ? this.t.goalMs
          : this.t.finalMs;
    this.schedule(ms, () => this.nextSlate());
  }

  private endSlate(): void {
    this.schedule(0, () => this.nextSlate());
  }

  private redrawSlate(): void {
    if (!this.showing || !this.current) return;
    const fresh = this.slate(this.showing);
    fresh.classList.add("static");
    this.current.replaceChildren(...fresh.childNodes);
  }

  private isShowingGoal(seq: number): boolean {
    const s = this.showing;
    return s?.kind === "single" && s.alert.body.kind === "goal" && s.alert.body.goal.seq === seq;
  }

  private slate(slot: Slot): HTMLElement {
    if (slot.kind === "summary") return this.summarySlate(slot.alerts);
    const { alert } = slot;
    const g = this.board.games.get(alert.gameId);
    if (!g) return h("div.layer");
    if (alert.body.kind === "goal") {
      const goal = alert.body.goal;
      const team = g[goal.side].team;
      const tag = scoreTag(goal.detail);
      return h(
        "div.layer.slate.goal",
        { style: teamVars(team.color) },
        h("div.sweep"),
        h(`div.sl-tag${tag.length > 5 ? ".long" : ""}`, null, tag),
        h("div.sl-team", null, h("b", null, team.abbr), h("span", null, team.name)),
        scoreline(g, goal),
        h(
          "div.sl-detail",
          null,
          scorerLine(goal.detail, false),
          h("span.when", null, goal.detail ? goalTime(goal.detail) : status(g).textContent),
        ),
      );
    }
    const e = alert.body.event;
    const w = winner(g);
    return h(
      "div.layer.slate.final",
      { style: teamVars(w ? g[w].team.color : undefined) },
      h("div.sweep"),
      h("div.sl-tag", null, statusLine(g)),
      h("div.sl-team", null, h("b", null, w ? g[w].team.abbr : ""), h("span", null, w ? `${g[w].team.name} win` : "")),
      scoreline(g, { side: w ?? "home", homeScore: e.home_score, awayScore: e.away_score }),
    );
  }

  /** Several stale alerts at once: current score of each game involved. */
  private summarySlate(alerts: Alert[]): HTMLElement {
    const latest = new Map<string, Alert>();
    for (const a of alerts) latest.set(a.gameId, a);
    const goals = alerts.filter((a) => a.body.kind === "goal").length;
    const finals = alerts.length - goals;
    const tag = [goals && `${goals} GOAL${goals > 1 ? "S" : ""}`, finals && `${finals} FINAL${finals > 1 ? "S" : ""}`]
      .filter(Boolean)
      .join(" · ");
    const shown = [...latest.values()].slice(0, 5);
    return h(
      "div.layer.slate.summary",
      null,
      h("div.sl-tag", null, h("small", null, "MEANWHILE"), tag),
      h(
        "div.sum-list",
        null,
        ...shown.map((a) => {
          const g = this.board.games.get(a.gameId);
          if (!g) return null;
          let note = statusLine(g);
          if (a.body.kind === "goal" && g.state !== "final") {
            const goal = a.body.goal;
            note = goal.detail?.scorer ?? `${g[goal.side].team.abbr} goal`;
          }
          return h("div.sum-item", null, teamRow(g, "away"), teamRow(g, "home"), h("span.sum-note", null, note));
        }),
        latest.size > shown.length ? h("span.sum-more", null, `+${latest.size - shown.length}`) : null,
      ),
    );
  }

  // ---- plumbing ----------------------------------------------------------

  /** Crossfade to `next`. Only opacity and transform animate. */
  private swap(next: HTMLElement): void {
    const prev = this.current;
    this.current = next;
    this.stage.append(next);
    requestAnimationFrame(() => next.classList.add("in"));
    if (prev) {
      prev.classList.remove("in");
      prev.classList.add("out");
      setTimeout(() => prev.remove(), this.t.fadeMs + 50);
    }
  }

  private schedule(ms: number, fn: () => void): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(fn, ms);
  }
}
