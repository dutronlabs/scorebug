// Team colors arrive as arbitrary `#rrggbb`; these keep them legible on the
// dark UI and pick readable text for slates filled with them.

type Rgb = [number, number, number];

const FALLBACK = "#5a6270";
export const SURFACE: Rgb = [14, 17, 23];

function parse(hex: string | undefined): Rgb | null {
  const m = /^#?([0-9a-f]{6})$/i.exec(hex ?? "");
  if (!m) return null;
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

const hex = (c: Rgb) => "#" + c.map((v) => Math.round(v).toString(16).padStart(2, "0")).join("");

function luminance([r, g, b]: Rgb): number {
  const lin = (v: number) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : ((s + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * lin(r) + 0.7152 * lin(g) + 0.0722 * lin(b);
}

export function contrast(a: Rgb, b: Rgb): number {
  const [x, y] = [luminance(a), luminance(b)].sort((p, q) => q - p);
  return (x + 0.05) / (y + 0.05);
}

const mix = (a: Rgb, b: Rgb, t: number): Rgb => [0, 1, 2].map((i) => a[i] + (b[i] - a[i]) * t) as Rgb;

/** The team color, lightened just enough to read against the dark surface. */
export function accent(color: string | undefined, min = 2.6): string {
  const c = parse(color) ?? parse(FALLBACK)!;
  for (let t = 0; t <= 1; t += 0.05) {
    const m = mix(c, [255, 255, 255], t);
    if (contrast(m, SURFACE) >= min) return hex(m);
  }
  return "#ffffff";
}

/** Slate fill (the raw team color) and the text color that reads on it. */
export function slate(color: string | undefined): { fill: string; ink: string } {
  const c = parse(color) ?? parse(FALLBACK)!;
  const ink = contrast(c, [255, 255, 255]) >= contrast(c, [0, 0, 0]) ? "#ffffff" : "#0b0d12";
  return { fill: hex(c), ink };
}
