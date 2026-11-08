//! Prints a simulated night as newline-delimited JSON frames.
//!
//! `scorebug-sim [--seed N] [--games N] [--poll SECS] [--warp X]`

use std::io::Write;

fn main() {
    let (mut seed, mut games, mut poll, mut warp) = (1u64, 16usize, 5u32, 30.0f64);
    let mut args = std::env::args().skip(1);
    while let Some(flag) = args.next() {
        let mut value = |name: &str| {
            args.next().unwrap_or_else(|| {
                eprintln!("{name} needs a value");
                std::process::exit(2)
            })
        };
        match flag.as_str() {
            "--seed" => seed = parse(&value("--seed")),
            "--games" => games = parse(&value("--games")),
            "--poll" => poll = parse(&value("--poll")),
            "--warp" => warp = parse(&value("--warp")),
            other => {
                eprintln!(
                    "unknown argument {other}\nusage: scorebug-sim [--seed N] [--games N] [--poll SECS] [--warp X]"
                );
                std::process::exit(2);
            }
        }
    }

    let stdout = std::io::stdout();
    let mut out = std::io::BufWriter::new(stdout.lock());
    for frame in scorebug_sim::run_night(seed, games, poll, warp) {
        if serde_json::to_writer(&mut out, &frame).is_err() || out.write_all(b"\n").is_err() {
            return; // e.g. broken pipe
        }
    }
}

fn parse<T: std::str::FromStr>(s: &str) -> T {
    s.parse().unwrap_or_else(|_| {
        eprintln!("invalid value {s:?}");
        std::process::exit(2)
    })
}
