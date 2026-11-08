import { describe, expect, it } from "vitest";
import { accent, contrast, slate, SURFACE } from "../src/color";

const rgb = (hex: string): [number, number, number] => {
  const n = parseInt(hex.slice(1), 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
};

describe("team colors", () => {
  it("lifts dark team colors until they read on the surface", () => {
    for (const c of ["#041E42", "#00205B", "#154734", "#002654"]) {
      expect(contrast(rgb(accent(c)), SURFACE)).toBeGreaterThanOrEqual(2.6);
    }
  });

  it("leaves bright colors alone", () => {
    expect(accent("#FF4C00")).toBe("#ff4c00");
  });

  it("picks dark ink on light fills and light ink on dark fills", () => {
    expect(slate("#99D9D9").ink).toBe("#0b0d12");
    expect(slate("#041E42").ink).toBe("#ffffff");
  });

  it("falls back on bad input", () => {
    expect(slate(undefined).fill).toMatch(/^#[0-9a-f]{6}$/);
  });
});
