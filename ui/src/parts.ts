// Pieces shared by the ticker and the grid.

import { accent, slate } from "./color";
import { h } from "./dom";
import { goalTags, goalTime, statusLine } from "./format";
import type { GoalRecord } from "./store";
import type { GameSnapshot, GoalDetail, Side } from "./types";

export const teamVars = (color: string | undefined) => {
  const s = slate(color);
  return `--team:${accent(color)};--fill:${s.fill};--ink:${s.ink}`;
};

export function winner(g: GameSnapshot): Side | null {
  if (g.state !== "final" || g.home.score === g.away.score) return null;
  return g.home.score > g.away.score ? "home" : "away";
}

/** One team line: color chip, abbreviation, optional name, score. */
export function teamRow(g: GameSnapshot, side: Side, withName = false): HTMLElement {
  const ts = g[side];
  const w = winner(g);
  const cls = `row.${side}${w === side ? ".won" : w ? ".lost" : ""}`;
  return h(
    `div.${cls}`,
    { style: teamVars(ts.team.color) },
    h("i.chip"),
    h("b.abbr", null, ts.team.abbr),
    withName ? h("span.name", null, ts.team.name.replace(/^.* (?=\S+$)/, "")) : null,
    g.state === "scheduled" || g.state === "postponed" ? h("span.score.none") : h("span.score", null, `${ts.score}`),
  );
}

export function status(g: GameSnapshot): HTMLElement {
  return h(`span.status.${g.state}`, null, statusLine(g));
}

export function scorerLine(d: GoalDetail | undefined, withTime = true): HTMLElement {
  if (!d) return h("span.scorer.pending", null, "Scorer pending");
  const tags = goalTags(d);
  return h(
    "span.scorer",
    null,
    h("b", null, d.scorer),
    d.assists?.length ? h("span.assists", null, ` (${d.assists.join(", ")})`) : null,
    ...tags.map((t) => h("em.tag", null, t)),
    withTime ? h("span.when", null, goalTime(d)) : null,
  );
}

/** `MTL 2  TOR 3` with the scoring side's number highlighted. */
export function scoreline(g: GameSnapshot, goal: Pick<GoalRecord, "side" | "homeScore" | "awayScore">): HTMLElement {
  const part = (side: Side) =>
    h(
      `span.part${goal.side === side ? ".hot" : ""}`,
      null,
      h("b", null, g[side].team.abbr),
      h("span.n", null, `${side === "home" ? goal.homeScore : goal.awayScore}`),
    );
  return h("div.scoreline", null, part("away"), part("home"));
}
