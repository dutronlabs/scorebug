import type { Frame } from "./types";

export async function loadSim(seed: number, games: number): Promise<Frame[]> {
  const res = await fetch(`/api/sim?seed=${seed}&games=${games}`);
  if (!res.ok) throw new Error(`simulator: ${res.status} ${await res.text()}`);
  return parseNdjson(await res.text());
}

export function parseNdjson(text: string): Frame[] {
  return text
    .split("\n")
    .filter((l) => l.trim())
    .map((l) => JSON.parse(l) as Frame);
}

/**
 * Plays recorded frames on their own timeline. `speed` scales it: frames from
 * `scorebug-sim` are stamped at 30 game seconds per real second, so speed 1/3
 * is a 10x night.
 */
export class Replay {
  speed = 1;
  private i = 0;
  private vt = 0;
  private last = 0;
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(
    private frames: Frame[],
    /** `catchUp` is true for frames applied by a seek: show state, skip alerts. */
    private onFrame: (f: Frame, catchUp: boolean) => void,
    private onReset: () => void,
  ) {}

  get playing(): boolean {
    return this.timer != null;
  }

  get duration(): number {
    return this.frames.at(-1)?.t_ms ?? 0;
  }

  get position(): number {
    return this.vt;
  }

  play(): void {
    if (this.timer) return;
    this.last = performance.now();
    this.timer = setInterval(() => this.tick(), 50);
  }

  pause(): void {
    if (this.timer) clearInterval(this.timer);
    this.timer = null;
  }

  /** Restart from the beginning, then fast-forward silently to `ms`. */
  seek(ms: number): void {
    this.onReset();
    this.i = 0;
    this.vt = ms;
    this.drain(true);
    this.last = performance.now();
  }

  private tick(): void {
    const now = performance.now();
    this.vt += (now - this.last) * this.speed;
    this.last = now;
    this.drain(false);
    if (this.i >= this.frames.length) this.pause();
  }

  private drain(catchUp: boolean): void {
    while (this.i < this.frames.length && this.frames[this.i].t_ms <= this.vt) {
      this.onFrame(this.frames[this.i++], catchUp);
    }
  }
}
