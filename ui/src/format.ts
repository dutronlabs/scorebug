import type { GameSnapshot, GoalDetail, Side, Strength } from "./types";

// Ports of `GameSnapshot::{period_label, overtime_periods, status_line}`.

export function overtimePeriods(g: GameSnapshot): number {
  const n = Math.max(0, g.period - 3);
  return g.shootout ? Math.max(0, n - 1) : n;
}

export function periodLabel(g: Pick<GameSnapshot, "period" | "shootout">): string {
  if (g.shootout) return "SO";
  switch (g.period) {
    case 0:
      return "";
    case 1:
      return "1st";
    case 2:
      return "2nd";
    case 3:
      return "3rd";
    case 4:
      return "OT";
    default:
      return `${g.period - 3}OT`;
  }
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
      return `End ${periodLabel(g)}`;
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

export function goalTime(d: GoalDetail): string {
  const p = periodLabel({ period: d.period, shootout: false });
  return d.clock ? `${p} ${d.clock}` : p;
}

export const other = (s: Side): Side => (s === "home" ? "away" : "home");
