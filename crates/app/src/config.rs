//! `~/.config/scorebug/config.toml`. Written with defaults and comments on
//! first run; every key is optional.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Bottom,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    /// Screen edge the ticker sits on.
    pub edge: Edge,
    /// Gap in pixels between the ticker and that edge (e.g. to clear a panel).
    pub offset: u32,
    /// Monitor name as the system reports it (e.g. `DP-3`); empty = primary.
    pub monitor: String,
    pub sound: bool,
    /// 0.0 to 1.0.
    pub volume: f32,
    /// Team abbreviations as ESPN writes them (`TOR`, `NJ`, `TB`). Their
    /// games are listed first; when non-empty, only their goals play sound.
    pub favorites: Vec<String>,
    /// Seconds between ESPN polls (minimum 2).
    pub poll_secs: u64,
    /// Show NBA games in the ticker (scores only, no alerts).
    pub nba: bool,
    /// Show NFL games in the ticker, with touchdown and field goal alerts.
    pub nfl: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            edge: Edge::Bottom,
            offset: 0,
            monitor: String::new(),
            sound: true,
            volume: 0.6,
            favorites: vec![],
            poll_secs: 5,
            nba: true,
            nfl: true,
        }
    }
}

const TEMPLATE: &str = r#"# scorebug settings. Every key is optional; delete one to get its default.

# Screen edge for the ticker: "bottom" or "top".
edge = "bottom"
# Gap in pixels between the ticker and that edge, e.g. to clear a panel.
offset = 0
# Monitor name (e.g. "DP-3"); empty means the primary monitor.
monitor = ""

# Goal horn and final chime.
sound = true
volume = 0.6

# Team abbreviations as ESPN writes them, e.g. ["TOR", "MTL", "NJ", "TB"].
# Their games come first in the ticker; when set, only their goals make sound.
favorites = []

# Seconds between ESPN polls (minimum 2).
poll_secs = 5

# Also show NBA games in the ticker. Scores only: no goal slates or sound.
nba = true

# Also show NFL games. Touchdowns and field goals get the full slate.
nfl = true
"#;

impl Config {
    pub fn path(config_dir: &Path) -> PathBuf {
        config_dir.join("scorebug").join("config.toml")
    }

    /// Loads the file, creating it from the template if it is missing. A
    /// broken file is reported and ignored rather than fatal.
    pub fn load_or_init(path: &Path) -> Config {
        match std::fs::read_to_string(path) {
            Ok(text) => Config::parse(&text).unwrap_or_else(|e| {
                eprintln!("{}: {e}; using defaults", path.display());
                Config::default()
            }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let _ = path.parent().map(std::fs::create_dir_all);
                if let Err(e) = std::fs::write(path, TEMPLATE) {
                    eprintln!("{}: {e}", path.display());
                }
                Config::default()
            }
            Err(e) => {
                eprintln!("{}: {e}; using defaults", path.display());
                Config::default()
            }
        }
    }

    pub fn parse(text: &str) -> Result<Config, toml::de::Error> {
        let mut c: Config = toml::from_str(text)?;
        c.volume = c.volume.clamp(0.0, 1.0);
        c.poll_secs = c.poll_secs.max(2);
        for f in &mut c.favorites {
            *f = f.trim().to_uppercase();
        }
        Ok(c)
    }

    pub fn is_favorite(&self, abbr: &str) -> bool {
        self.favorites.iter().any(|f| f == abbr)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_parses_to_defaults() {
        assert_eq!(Config::parse(TEMPLATE).unwrap(), Config::default());
    }

    #[test]
    fn partial_file_and_normalizing() {
        let c =
            Config::parse("edge = \"top\"\nvolume = 3.0\nfavorites = [\" tor \"]\npoll_secs = 0")
                .unwrap();
        assert_eq!(c.edge, Edge::Top);
        assert_eq!(c.volume, 1.0);
        assert_eq!(c.favorites, ["TOR"]);
        assert_eq!(c.poll_secs, 2);
        assert!(c.sound);
    }

    #[test]
    fn typos_are_errors() {
        assert!(Config::parse("edg = \"top\"").is_err());
    }
}
