//! Battle royale records kept between matches in `br_stats.json` in the server's data
//! directory, under the number each installation made up for itself (`#…`; under the
//! name for a client without one).

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrRecord {
    pub matches: u32,
    pub wins: u32,
    pub kills: u32,
    pub damage: f32,
    /// Best place (0 = none yet).
    pub best: u32,
    /// Longest kill, metres.
    pub longest: f32,
}

pub struct BrStats {
    pub by_name: HashMap<String, BrRecord>,
    path: Option<PathBuf>,
}

impl BrStats {
    pub fn load(dir: &str) -> BrStats {
        let path = PathBuf::from(dir).join("br_stats.json");
        let by_name = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        BrStats {
            by_name,
            path: Some(path),
        }
    }

    /// One finished round for a player: their place and what they did in it.
    pub fn record(&mut self, name: &str, place: u32, kills: u32, damage: f32, longest: f32) {
        let r = self.by_name.entry(name.to_string()).or_default();
        r.matches += 1;
        if place == 1 {
            r.wins += 1;
        }
        r.kills += kills;
        r.damage += damage;
        r.longest = r.longest.max(longest);
        if r.best == 0 || place < r.best {
            r.best = place;
        }
        self.save();
    }

    pub fn get(&self, name: &str) -> BrRecord {
        self.by_name.get(name).copied().unwrap_or_default()
    }

    fn save(&self) {
        let Some(p) = &self.path else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(t) = serde_json::to_string_pretty(&self.by_name) {
            if let Err(e) = std::fs::write(p, t) {
                eprintln!("Не вдалося зберегти статистику: {e}");
            }
        }
    }
}
