import "@fontsource/barlow-condensed/500.css";
import "@fontsource/barlow-condensed/600.css";
import "@fontsource/barlow-condensed/700.css";
import "./styles.css";

import { h } from "./dom";
import { Grid } from "./grid";
import { loadSim, Replay } from "./source";
import { Scoreboard } from "./store";
import { Ticker } from "./ticker";

// `?view=ticker|grid|both` (default both, with dev controls).
// `?seed=N&games=N` pick the simulated night, `?warp=N` game seconds per real
// second, `?at=S` starts S real seconds into the replay.
const q = new URLSearchParams(location.search);
const view = q.get("view") ?? "both";
const seed = Number(q.get("seed") ?? 1);
const games = Number(q.get("games") ?? 16);
const SIM_WARP = 30;
const WARPS = [5, 10, 30, 60, 120];
let warp = Number(q.get("warp") ?? 10);

const board = new Scoreboard();
const ticker = view !== "grid" ? new Ticker(board) : null;
const grid = view !== "ticker" ? new Grid(board) : null;

document.documentElement.dataset.view = view;
const app = document.getElementById("app")!;
if (grid) app.append(h("main.grid-wrap", null, grid.el));
if (ticker) app.append(ticker.el);

const frames = await loadSim(seed, games).catch((e: Error) => {
  app.append(h("pre.error", null, e.message));
  throw e;
});

const replay = new Replay(
  frames,
  (f, catchUp) => {
    const notices = board.apply(f);
    ticker?.handle(notices, catchUp);
    grid?.handle(notices, catchUp);
  },
  () => {
    board.reset();
    ticker?.reset();
    grid?.reset();
  },
);
replay.speed = warp / SIM_WARP;
replay.seek(Number(q.get("at") ?? 0) * 1000);
replay.play();

if (view === "both") app.prepend(controls());

function controls(): HTMLElement {
  const play = h("button", { type: "button" }, "Pause");
  play.onclick = () => {
    if (replay.playing) replay.pause();
    else replay.play();
    play.textContent = replay.playing ? "Pause" : "Play";
  };
  const speed = h("select") as HTMLSelectElement;
  for (const w of WARPS) speed.append(h("option", { value: `${w}`, ...(w === warp ? { selected: "" } : {}) }, `${w}x`));
  speed.onchange = () => {
    warp = Number(speed.value);
    replay.speed = warp / SIM_WARP;
  };
  const seedInput = h("input", { type: "number", value: `${seed}`, min: "0", "aria-label": "seed" }) as HTMLInputElement;
  const reload = h("button", { type: "button" }, "New night");
  reload.onclick = () => {
    q.set("seed", seedInput.value);
    q.set("warp", `${warp}`);
    q.delete("at");
    location.search = q.toString();
  };
  const bar = h("progress", { max: "1", value: "0" }) as HTMLProgressElement;
  bar.onclick = (e) => {
    const r = bar.getBoundingClientRect();
    replay.seek(((e.clientX - r.left) / r.width) * replay.duration);
  };
  setInterval(() => (bar.value = replay.position / Math.max(1, replay.duration)), 250);
  return h(
    "div.controls",
    null,
    play,
    h("label", null, "speed ", speed),
    h("label", null, "seed ", seedInput),
    reload,
    bar,
    h("a", { href: `?view=ticker&seed=${seed}&warp=${warp}`, target: "_blank" }, "ticker"),
    h("a", { href: `?view=grid&seed=${seed}&warp=${warp}`, target: "_blank" }, "grid"),
  );
}
