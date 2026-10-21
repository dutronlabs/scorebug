import { describe, expect, it } from "vitest";
import { goalTime, scoreTag, statusLine } from "../src/format";
import type { GameSnapshot } from "../src/types";

const g = (over: Partial<GameSnapshot>): GameSnapshot => ({
  id: "g",
  sport: "hockey",
  state: "live",
  period: 2,
  clock: "8:41",
  home: { team: { id: "H", abbr: "H", name: "H" }, score: 0 },
  away: { team: { id: "A", abbr: "A", name: "A" }, score: 0 },
  shootout: false,
  ...over,
});

// Same cases as `status_line` in crates/core.
describe("statusLine", () => {
  it.each([
    [{}, "2nd 8:41"],
    [{ state: "intermission" }, "End 2nd"],
    [{ state: "final", period: 3 }, "FINAL"],
    [{ state: "final", period: 4 }, "FINAL/OT"],
    [{ state: "final", period: 5 }, "FINAL/2OT"],
    [{ state: "final", period: 5, shootout: true }, "FINAL/SO"],
    [{ state: "live", period: 5, shootout: true }, "SO"],
    [{ state: "postponed" }, "POSTPONED"],
  ] as [Partial<GameSnapshot>, string][])("%o -> %s", (over, want) => {
    expect(statusLine(g(over))).toBe(want);
  });
});

describe("football", () => {
  it.each([
    [{}, "Q2 8:41"],
    [{ state: "intermission" }, "Halftime"],
    [{ state: "intermission", period: 3 }, "End Q3"],
    [{ state: "live", period: 5 }, "OT 8:41"],
    [{ state: "final", period: 4 }, "FINAL"],
    [{ state: "final", period: 5 }, "FINAL/OT"],
  ] as [Partial<GameSnapshot>, string][])("%o -> %s", (over, want) => {
    expect(statusLine(g({ sport: "football", ...over }))).toBe(want);
  });

  it("tags and times scoring plays", () => {
    const d = { scorer: "X", empty_net: false, period: 3, clock: "4:10" };
    expect(scoreTag({ ...d, kind: "touchdown" })).toBe("TOUCHDOWN");
    expect(scoreTag({ ...d, kind: "field_goal" })).toBe("FIELD GOAL");
    expect(scoreTag(d)).toBe("GOAL");
    expect(goalTime({ ...d, kind: "field_goal" })).toBe("Q3 4:10");
    expect(goalTime(d)).toBe("3rd 4:10");
  });
});
