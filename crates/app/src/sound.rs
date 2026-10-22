//! Goal horn and final chime, synthesized (no audio assets, no licensing
//! questions) into WAV files and played by the system's command-line player
//! (`pw-play`, `paplay` or `aplay`), so no audio libraries are linked.

use std::f32::consts::TAU;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const RATE: u32 = 44_100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cue {
    Goal,
    Final,
}

impl Cue {
    pub fn parse(s: &str) -> Option<Cue> {
        match s {
            "goal" => Some(Cue::Goal),
            "final" => Some(Cue::Final),
            _ => None,
        }
    }
}

pub struct Sounds {
    goal: PathBuf,
    fin: PathBuf,
    player: Option<&'static str>,
}

impl Sounds {
    /// Renders both cues at `volume` into `dir`.
    pub fn prepare(dir: &Path, volume: f32) -> std::io::Result<Sounds> {
        std::fs::create_dir_all(dir)?;
        let goal = dir.join("goal.wav");
        let fin = dir.join("final.wav");
        std::fs::write(&goal, wav(&horn(volume)))?;
        std::fs::write(&fin, wav(&chime(volume)))?;
        let player = ["pw-play", "paplay", "aplay"]
            .into_iter()
            .find(|p| on_path(p));
        if player.is_none() {
            eprintln!("sound: no pw-play, paplay or aplay on PATH; staying silent");
        }
        Ok(Sounds { goal, fin, player })
    }

    /// Starts playback and returns immediately.
    pub fn play(&self, cue: Cue) {
        let Some(player) = self.player else { return };
        let file = match cue {
            Cue::Goal => &self.goal,
            Cue::Final => &self.fin,
        };
        let mut cmd = Command::new(player);
        if player == "aplay" {
            cmd.arg("-q");
        }
        match cmd
            .arg(file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
        {
            // Reap it off-thread so no zombie is left behind.
            Ok(mut child) => {
                std::thread::spawn(move || child.wait());
            }
            Err(e) => eprintln!("sound: {player}: {e}"),
        }
    }
}

fn on_path(bin: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|p| std::env::split_paths(&p).any(|d| d.join(bin).is_file()))
}

/// A stadium-style goal horn: a D major triad of brassy, slightly detuned
/// voices with a slow flutter, soft-clipped. About 2.4 s.
pub fn horn(volume: f32) -> Vec<f32> {
    let len = (RATE as f32 * 2.4) as usize;
    let voices = [146.8f32, 185.0, 220.0, 147.6];
    let mut out = Vec::with_capacity(len);
    for i in 0..len {
        let t = i as f32 / RATE as f32;
        let env = (t / 0.08).min(1.0) * ((2.4 - t) / 0.4).clamp(0.0, 1.0);
        let flutter = 1.0 + 0.04 * (TAU * 5.5 * t).sin();
        let mut s = 0.0;
        for f in voices {
            for n in 1..=14u32 {
                let n = n as f32;
                s += (TAU * f * n * t).sin() / n.powf(1.15);
            }
        }
        out.push((s * 0.18 * flutter).tanh() * env * 0.8 * volume);
    }
    out
}

/// Two soft bell tones (G5, C6) for a final. About 0.9 s.
pub fn chime(volume: f32) -> Vec<f32> {
    let len = (RATE as f32 * 0.9) as usize;
    (0..len)
        .map(|i| {
            let t = i as f32 / RATE as f32;
            let bell = |start: f32, f: f32| {
                let u = t - start;
                if u < 0.0 {
                    return 0.0;
                }
                let env = (u / 0.01).min(1.0) * (-u * 5.0).exp();
                env * ((TAU * f * u).sin() + 0.3 * (TAU * 2.0 * f * u).sin())
            };
            (bell(0.0, 784.0) + bell(0.28, 1046.5)) * 0.35 * volume
        })
        .collect()
}

/// 16-bit mono PCM WAV.
pub fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data_len as usize);
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data_len).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    b.extend_from_slice(&1u16.to_le_bytes()); // PCM
    b.extend_from_slice(&1u16.to_le_bytes()); // mono
    b.extend_from_slice(&RATE.to_le_bytes());
    b.extend_from_slice(&(RATE * 2).to_le_bytes());
    b.extend_from_slice(&2u16.to_le_bytes());
    b.extend_from_slice(&16u16.to_le_bytes());
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        b.extend_from_slice(&((s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16).to_le_bytes());
    }
    b
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cues_stay_in_range_and_fade_out() {
        for s in [horn(1.0), chime(1.0)] {
            assert!(s.iter().all(|x| x.abs() <= 1.0));
            assert!(s.iter().any(|x| x.abs() > 0.3), "audible");
            assert!(s.last().unwrap().abs() < 0.01, "no click at the end");
        }
        assert!(horn(0.0).iter().all(|x| *x == 0.0));
    }

    #[test]
    fn wav_header() {
        let w = wav(&[0.0, 1.0, -1.0]);
        assert_eq!(&w[0..4], b"RIFF");
        assert_eq!(&w[8..16], b"WAVEfmt ");
        assert_eq!(u32::from_le_bytes(w[40..44].try_into().unwrap()), 6);
        assert_eq!(w.len(), 44 + 6);
    }
}
