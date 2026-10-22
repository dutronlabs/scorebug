Real ESPN responses, captured 2026-10-14 from `site.api.espn.com`.

- `scoreboard-2026-10-13-final.json`: `/apis/site/v2/sports/hockey/nhl/scoreboard`
  the morning after a 16-game night (every game final; one OT, several SO).
- `scoreboard-2026-10-14-scheduled.json`: `.../scoreboard?dates=20261014`, pregame.
- `summary-<event id>.json`: `.../summary?event=<id>` for each game above.

Trimmed for size only. Fields the provider never reads were removed: broadcasts,
venue, leaders, stats, records, links. Summaries keep `header.competitions[0].status`,
every scoring play and the first three other plays. Nothing was edited or invented.

Still missing: a capture taken **during** a live game, including an intermission.
Record one with `cargo run -p scorebug-providers --example record`.
