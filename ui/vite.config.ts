import { execFile } from "node:child_process";
import { fileURLToPath } from "node:url";
import { defineConfig, type Plugin } from "vite";

const workspace = fileURLToPath(new URL("..", import.meta.url));

/**
 * Dev-only endpoint: `/api/sim?seed=N&games=N` runs `scorebug-sim` and serves
 * its NDJSON frames. Results are cached per parameter set.
 */
function simEndpoint(): Plugin {
  const cache = new Map<string, Promise<string>>();
  const run = (seed: number, games: number) =>
    new Promise<string>((resolve, reject) => {
      execFile(
        "cargo",
        ["run", "-q", "--release", "-p", "scorebug-sim", "--", "--seed", `${seed}`, "--games", `${games}`],
        { cwd: workspace, maxBuffer: 256 * 1024 * 1024 },
        (err, stdout, stderr) => (err ? reject(new Error(stderr || err.message)) : resolve(stdout)),
      );
    });
  return {
    name: "scorebug-sim",
    configureServer(server) {
      server.middlewares.use("/api/sim", (req, res) => {
        const q = new URL(req.url ?? "", "http://x").searchParams;
        const seed = Math.max(0, Math.trunc(Number(q.get("seed") ?? 1)) || 0);
        const games = Math.min(16, Math.max(1, Math.trunc(Number(q.get("games") ?? 16)) || 16));
        const key = `${seed}/${games}`;
        if (!cache.has(key)) cache.set(key, run(seed, games));
        cache.get(key)!.then(
          (body) => {
            res.setHeader("content-type", "application/x-ndjson");
            res.end(body);
          },
          (err: Error) => {
            cache.delete(key);
            res.statusCode = 500;
            res.end(err.message);
          },
        );
      });
    },
  };
}

export default defineConfig({
  plugins: [simEndpoint()],
  // The desktop app loads this URL in dev (crates/app/tauri.conf.json).
  server: { port: 5173, strictPort: true },
});
