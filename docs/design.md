# Design

## Data flow

```
Providers -> GameSnapshot[] -> Tracker (diff) -> GameEvent[] -> UI
 (ESPN, ...)                    (scorebug-core)                (Tauri + web)
```

The UI never talks to a data source. Providers produce normalized snapshots;
the tracker turns changes into events; the UI renders state plus events. The
simulator (`scorebug-sim`) produces the same snapshots, so everything
downstream can be built and tested without a live game.

## Crates

| crate | role |
|---|---|
| `scorebug-core` | model (`GameSnapshot`, ...), `Tracker`, `GameEvent`. No I/O. |
| `scorebug-sim` | deterministic accelerated NHL night; `scorebug-sim` prints NDJSON frames |
| `scorebug-providers` | `ScoreProvider` trait; ESPN NHL (scoreboard + per-game summary) |
| `scorebug-app` (`crates/app`) | Tauri 2 shell: feed thread, ticker overlay, grid window, tray |
| `ui/` | web front end: ticker + grid views (Vite + TypeScript, no framework) |

## Event rules (implemented in `Tracker`)

- First sighting of a game is a silent baseline (no replay of old goals).
- Score up -> `Goal` (one per increment, with running score). Score down ->
  `GoalDisallowed`, shown quietly, no celebration.
- Scorer details lag the score: `Goal` goes out immediately with
  `detail: null`, `ScorerResolved { goal_seq }` follows. A play that appears
  before the score is held and attached to the next goal.
- Shootout score changes are not goals; `Final` carries the result.
- After `Final` only late scorer details are emitted.
- Known blind spot: a goal and a reversal for the same side inside one poll
  cancel out and emit nothing.

## Display

- **Ticker**: full-width bottom bar, ~80-100px. Pages of ~4 games, crossfade
  every 5-6s (300ms), live games first. A goal takes over the bar with a
  team-color slate (~6s), queued by priority; alerts older than ~20s collapse
  into a summary.
- **Grid**: separate window, one scorebug card per game, alerts play inside
  the scoring game's card so simultaneous goals need no queue.
- Animate only `opacity`/`transform`.

### UI implementation (`ui/`)

- `Scoreboard` (`src/store.ts`) folds frames into current game state and
  `GoalRecord`s. Goal records are shared by reference, so a late
  `ScorerResolved` updates every place the goal is shown or queued.
- `AlertQueue` orders ticker takeovers: goals before finals, oldest first.
  An alert older than 20s jumps ahead of fresh ones so finals can't starve on
  a busy night. Two or more such alerts collapse into one "meanwhile" summary.
- A disallowed goal is dropped from the queue, or its slate is cut short if
  it is playing. The cell or card shows a quiet `NO GOAL` tag.
- Frames applied by a seek (`catchUp`) update state but raise no alerts, like
  the tracker's silent baseline.
- Team colors are lightened just enough to read on the dark surface. Slates
  use the raw color with black or white ink, whichever has more contrast.
- `?view=ticker|grid|both` picks the window. `ticker` has a transparent
  page background for the overlay window. `both` is the dev harness, with
  play, speed, seed and seek controls.
- In dev, `/api/sim?seed&games` runs `scorebug-sim` (a Vite middleware). The
  production build has no data source until the Tauri shell provides one
  (phase 3).

## Linux / NVIDIA / Wayland

- Tauri uses WebKitGTK. Set `WEBKIT_DISABLE_DMABUF_RENDERER=1` at startup for
  the proprietary NVIDIA driver; do not disable compositing.
- Wayland lets a client neither position its window nor keep it on top, so
  the app always runs through XWayland (`GDK_BACKEND=x11`). It overrides the
  inherited value because COSMIC exports `GDK_BACKEND=wayland,x11`
  session-wide. `SCOREBUG_GDK_BACKEND` overrides that.
- COSMIC's X window manager clamps a managed window to about 2/3 of the
  screen and centers it, even for `_NET_WM_WINDOW_TYPE_DOCK`. The ticker is
  therefore an **override-redirect** window: unmanaged, placed exactly, and
  stacked above managed windows. It is also never `resizable(false)`, because
  GTK then sizes it to the webview's natural 200px.
- Click-through (`set_ignore_cursor_events`) only works on a shown window
  (tao unwraps the GDK window), so it is applied after every show.
- Verified on the target machine (COSMIC, 5120x1440 at scale 1, RTX 5060 Ti,
  driver 580): the ticker shows at 5120x88+0+1352, above other windows, and
  the grid opens as a normal window.

## Data source

ESPN's unofficial NHL API (`site.api.espn.com/apis/site/v2/sports/hockey/nhl`).
The provider is written against captured responses in
`crates/providers/tests/fixtures/espn/`. What they showed:

- `scoreboard` has state, period, clock and score, but **no scoring plays**.
  Scorers come only from `summary?event=<id>`: `plays[]` with
  `scoringPlay: true`.
- A shootout winner's `score` includes one bonus goal (period 5, `SO`).
  Shootout attempts are scoring plays with `period.displayValue == "SO"` /
  `shotInfo.id == "905"`, and the provider drops them. `shootout` is set
  from period >= 5 outside the postseason.
- An empty-net goal has `strength: empty-net` in place of its manpower
  strength.
- Fetch policy: a summary is fetched when a game first has goals, when its
  score moves, while it has goals without scorers, every 30s while live, and
  once more after it goes final.
- A game with goals is **held back** until its summary loads once. The
  tracker's silent baseline then owns its earlier scorers. Otherwise they
  would look new and be pinned to the next goal.
- Not yet captured: a game **in progress** or **in intermission**.
  Intermission is inferred from `state == "in"` with a zero clock. Run
  `cargo run -p scorebug-providers --example record` during a live night to
  capture fixtures and check the mapping.

`api-web.nhle.com` is the planned fallback/second provider.

## App

- The feed thread polls (ESPN every 5s, with backoff on errors) or plays
  simulated nights in real time (`--sim`), runs the tracker, and emits
  `frame` events shaped like the simulator's NDJSON. A window that loads asks
  for `current_frame` and applies it as a catch-up (no alerts).
- The window label (`ticker` / `grid`) picks the view. Closing the grid hides
  it; the tray shows the grid, toggles the ticker and click-through, and quits.

## Status

- [x] Phase 1: core model, tracker, simulator, tests
- [x] Phase 2: web UI against simulator output (ticker, goal slate, grid)
- [x] Phase 3: ESPN provider, Tauri shell
- [ ] Phase 4: hardening on the target machine, sound, scorer late-binding polish
