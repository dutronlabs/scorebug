import type { GameSnapshot, GoalDetail, Side, Strength } from "./types";

// Ports of `GameSnapshot::{period_label, overtime_periods, status_line}`.

const regulation = (g: Pick<GameSnapshot, "sport">) => (g.sport === "hockey" ? 3 : 4);

export function overtimePeriods(g: GameSnapshot): number {
  const n = Math.max(0, g.period - regulation(g));
  return g.shootout ? Math.max(0, n - 1) : n;
}

export function periodLabel(g: Pick<GameSnapshot, "sport" | "period" | "shootout">): string {
  if (g.shootout) return "SO";
  const reg = regulation(g);
  if (g.period === 0) return "";
  if (g.period > reg) return g.period === reg + 1 ? "OT" : `${g.period - reg}OT`;
  if (g.sport !== "hockey") return `Q${g.period}`;
  return ["1st", "2nd", "3rd"][g.period - 1];
}

export function statusLine(g: GameSnapshot): string {
  switch (g.state) {
    case "scheduled":
      return g.start_unix != null ? startTime(g.start_unix) : "SCHEDULED";
    case "postponed":
      return "POSTPONED";
    case "live":
      return g.clock && !g.shootout ? `${periodLabel(g)} ${g.clock}` : periodLabel(g);
    case "intermission":
      return g.sport !== "hockey" && g.period === 2 ? "Halftime" : `End ${periodLabel(g)}`;
    case "final": {
      if (g.shootout) return "FINAL/SO";
      const ot = overtimePeriods(g);
      return ot === 0 ? "FINAL" : ot === 1 ? "FINAL/OT" : `FINAL/${ot}OT`;
    }
  }
}

export function startTime(unix: number): string {
  return new Date(unix * 1000).toLocaleTimeString([], { hour: "numeric", minute: "2-digit" });
}

const STRENGTH: Record<Strength, string> = {
  even: "",
  power_play: "PPG",
  short_handed: "SHG",
  penalty_shot: "PS",
};

/** Tags for a goal, e.g. `["PPG", "EN"]`. */
export function goalTags(d: GoalDetail): string[] {
  const tags = [];
  if (d.strength && STRENGTH[d.strength]) tags.push(STRENGTH[d.strength]);
  if (d.empty_net) tags.push("EN");
  return tags;
}

/** `TOUCHDOWN` / `FIELD GOAL` for football plays, `GOAL` otherwise. */
export function scoreTag(d: GoalDetail | undefined): string {
  return d?.kind === "touchdown" ? "TOUCHDOWN" : d?.kind === "field_goal" ? "FIELD GOAL" : "GOAL";
}

export function goalTime(d: GoalDetail): string {
  // Only football plays carry a `kind`.
  const p = periodLabel({ sport: d.kind ? "football" : "hockey", period: d.period, shootout: false });
  return d.clock ? `${p} ${d.clock}` : p;
}

export const other = (s: Side): Side => (s === "home" ? "away" : "home");
