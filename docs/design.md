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
| `providers` (planned) | ESPN NHL scoreboard behind a `ScoreProvider` trait |
| `app` (planned) | Tauri shell: overlay window, grid window, tray, config |
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

## Linux / NVIDIA / X11

- Tauri uses WebKitGTK. Set `WEBKIT_DISABLE_DMABUF_RENDERER=1` at startup for
  the proprietary NVIDIA driver; do not disable compositing.
- X11: transparent, borderless, always-on-top, optionally click-through; needs
  a compositor.

## Data source

ESPN's unofficial NHL scoreboard (`site.api.espn.com/apis/site/v2/sports/hockey/nhl/scoreboard`).
The provider model must be written against a **captured real response**
(stored as a test fixture), not from memory. `api-web.nhle.com` is the
fallback/second provider.

## Status

- [x] Phase 1: core model, tracker, simulator, tests
- [x] Phase 2: web UI against simulator output (ticker, goal slate, grid)
- [ ] Phase 3: ESPN provider, Tauri shell
- [ ] Phase 4: hardening on the target machine, sound, scorer late-binding polish
