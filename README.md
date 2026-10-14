# scorebug

A desktop scoreboard overlay in the style of ESPN's "bottom line": a dark,
team-color-accented ticker along the bottom of the screen that crossfades
between pages of games and flashes when something happens (a goal, a final).
A grid view shows many games at once, each as its own scorebug card.

First target: NHL, every game of the day, Linux/X11 (NVIDIA), Tauri front end.

See [docs/design.md](docs/design.md) for the design and current status.

## Try the pipeline

```sh
cargo test                                   # core + simulator invariants
cargo run -p scorebug-sim -- --games 16      # a simulated 16-game night as NDJSON
```
