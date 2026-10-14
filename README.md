# scorebug

A desktop scoreboard overlay in the style of ESPN's "bottom line": a dark,
team-color-accented ticker along the bottom of the screen that crossfades
between pages of games and flashes when something happens (a goal, a final).
A grid view shows many games at once, each as its own scorebug card.

First target: NHL, every game of the day, Linux (NVIDIA; X11 or Wayland via XWayland), Tauri front end.

See [docs/design.md](docs/design.md) for the design and current status.

## Try the pipeline

```sh
cargo test                                   # core + simulator invariants
cargo run -p scorebug-sim -- --games 16      # a simulated 16-game night as NDJSON
```

## Try the UI

```sh
cd ui && npm install && npm run dev          # http://localhost:5173
```

The default page plays a simulated night with the grid and ticker together, and
has playback controls. `?view=ticker` and `?view=grid` show each window on
its own. `?seed=N`, `?warp=N` (game seconds per real second) and `?at=S` (start
S seconds in) pick the night.

## Run the desktop app

Needs the WebKitGTK 4.1 development packages (`libwebkit2gtk-4.1-dev`).

```sh
(cd ui && npm run build)                                  # the app embeds ui/dist
cargo run -p scorebug-app --features custom-protocol      # live ESPN
cargo run -p scorebug-app --features custom-protocol -- --sim --warp 20 --grid
```

Without `--features custom-protocol` the app loads the Vite dev server
(`cd ui && npm run dev`, port 5173), so UI edits hot-reload. Use the tray icon
to open the grid, hide the ticker, toggle click-through or quit.
