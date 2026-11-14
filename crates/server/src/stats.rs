//! Player records kept between matches in `br_stats.json` in the server's data directory,
//! under the number each installation made up for itself (`#…`; under the name for a
//! client without one): every mode counts kills, deaths, wins and the longest kill; the
//! battle royale also its best place and damage. The leaderboards are built from them.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use toon_shared::lang::t;
use toon_shared::protocol::{Board, BoardKind};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BrRecord {
    /// The name last played under.
    pub name: String,
    pub matches: u32,
    pub wins: u32,
    pub kills: u32,
    pub deaths: u32,
    pub damage: f32,
    /// Best battle royale place (0 = none yet).
    pub best: u32,
    /// Longest kill, metres.
    pub longest: f32,
}

impl BrRecord {
    /// This record's number on a leaderboard (None: not on that one).
    pub fn value(&self, kind: BoardKind) -> Option<f32> {
        match kind {
            BoardKind::Kills => (self.kills > 0).then_some(self.kills as f32),
            BoardKind::Wins => (self.wins > 0).then_some(self.wins as f32),
            BoardKind::Longest => (self.longest > 0.0).then_some(self.longest),
            BoardKind::Matches => (self.matches > 0).then_some(self.matches as f32),
            // (only after a few matches: one lucky round is not a ratio)
            BoardKind::KillDeath => {
                (self.matches >= 5).then(|| self.kills as f32 / self.deaths.max(1) as f32)
            }
            BoardKind::Damage => (self.damage > 0.0).then_some(self.damage),
        }
    }
}

/// What a finished match did to the leaderboards, for the chat.
pub struct News(pub String);

pub struct BrStats {
    pub by_name: HashMap<String, BrRecord>,
    path: Option<PathBuf>,
}

impl BrStats {
    pub fn load(dir: &str) -> BrStats {
        let path = PathBuf::from(dir).join("br_stats.json");
        let mut by_name: HashMap<String, BrRecord> = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        // records from before names were kept: one kept under a name has it in its key
        for (k, r) in by_name.iter_mut() {
            if r.name.is_empty() && !k.starts_with('#') {
                r.name = k.clone();
            }
        }
        BrStats {
            by_name,
            path: Some(path),
        }
    }

    /// A record kept under the name alone (before installations had numbers) goes into the
    /// record of the installation now playing under that name.
    fn claim_old(&mut self, key: &str, name: &str) {
        if !key.starts_with('#') || name.starts_with('#') || key == name {
            return;
        }
        let Some(old) = self.by_name.remove(name) else {
            return;
        };
        let r = self.by_name.entry(key.to_string()).or_default();
        r.matches += old.matches;
        r.wins += old.wins;
        r.kills += old.kills;
        r.deaths += old.deaths;
        r.damage += old.damage;
        r.longest = r.longest.max(old.longest);
        if old.best != 0 && (r.best == 0 || old.best < r.best) {
            r.best = old.best;
        }
    }

    /// One finished battle royale round for a player: their place and what they did in it.
    pub fn record(
        &mut self,
        key: &str,
        name: &str,
        place: u32,
        kills: u32,
        damage: f32,
        longest: f32,
    ) -> Vec<News> {
        self.claim_old(key, name);
        let before = self.leaders();
        let r = self.by_name.entry(key.to_string()).or_default();
        r.name = name.to_string();
        r.matches += 1;
        if place == 1 {
            r.wins += 1;
        } else {
            r.deaths += 1;
        }
        r.kills += kills;
        r.damage += damage;
        r.longest = r.longest.max(longest);
        if r.best == 0 || place < r.best {
            r.best = place;
        }
        self.save();
        self.news(key, before)
    }

    /// One finished team match (deathmatch or sabotage) for a player.
    pub fn record_match(
        &mut self,
        key: &str,
        name: &str,
        won: bool,
        kills: u32,
        deaths: u32,
        longest: f32,
    ) -> Vec<News> {
        self.claim_old(key, name);
        let before = self.leaders();
        let r = self.by_name.entry(key.to_string()).or_default();
        r.name = name.to_string();
        r.matches += 1;
        r.wins += won as u32;
        r.kills += kills;
        r.deaths += deaths;
        r.longest = r.longest.max(longest);
        self.save();
        self.news(key, before)
    }

    pub fn get(&self, key: &str) -> BrRecord {
        self.by_name.get(key).cloned().unwrap_or_default()
    }

    /// The longest kill anyone has made here.
    pub fn longest_ever(&self) -> f32 {
        self.by_name.values().map(|r| r.longest).fold(0.0, f32::max)
    }

    /// Who is first on each leaderboard now.
    fn leaders(&self) -> Vec<Option<String>> {
        BoardKind::ALL
            .iter()
            .map(|k| self.top(*k, 1).first().map(|r| r.0.clone()))
            .collect()
    }

    /// A player who just became first somewhere, for everybody to hear.
    fn news(&self, key: &str, before: Vec<Option<String>>) -> Vec<News> {
        let mut out = Vec::new();
        for (k, kind) in BoardKind::ALL.iter().enumerate() {
            // (the longest kill is announced the moment it happens; matches played is no
            // achievement)
            if matches!(kind, BoardKind::Longest | BoardKind::Matches) {
                continue;
            }
            let now = self.top(*kind, 1);
            let Some((top_key, rec)) = now.first() else {
                continue;
            };
            if top_key == key && before[k].as_deref() != Some(key) {
                let v = rec.value(*kind).unwrap_or(0.0);
                out.push(News(toon_shared::tr!(
                    "★ {} тепер перший у рейтингу «{}» ({})",
                    "★ {} is now first in «{}» ({})",
                    rec.name,
                    kind.name(),
                    kind.format(v)
                )));
            }
        }
        out
    }

    /// The best `n` on a leaderboard: (key, record), best first.
    pub fn top(&self, kind: BoardKind, n: usize) -> Vec<(String, BrRecord)> {
        let mut all: Vec<(&String, &BrRecord, f32)> = self
            .by_name
            .iter()
            .filter_map(|(k, r)| r.value(kind).map(|v| (k, r, v)))
            .collect();
        all.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.cmp(b.0)));
        all.into_iter()
            .take(n)
            .map(|(k, r, _)| (k.clone(), r.clone()))
            .collect()
    }

    /// Every leaderboard: the best ten, and where `key` stands if not among them.
    pub fn boards(&self, key: Option<&str>) -> Vec<Board> {
        BoardKind::ALL
            .iter()
            .map(|&kind| {
                let mut all: Vec<(&String, &BrRecord, f32)> = self
                    .by_name
                    .iter()
                    .filter_map(|(k, r)| r.value(kind).map(|v| (k, r, v)))
                    .collect();
                all.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.cmp(b.0)));
                let mine = key.and_then(|key| {
                    all.iter()
                        .position(|e| e.0 == key)
                        .map(|i| ((i + 1) as u16, all[i].2))
                });
                Board {
                    kind,
                    rows: all
                        .iter()
                        .take(10)
                        .map(|e| (display_name(e.1), e.2))
                        .collect(),
                    mine,
                }
            })
            .collect()
    }

    fn save(&self) {
        let Some(p) = &self.path else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(t) = serde_json::to_string_pretty(&self.by_name) {
            if let Err(e) = std::fs::write(p, t) {
                eprintln!("{}: {e}", t_save());
            }
        }
    }
}

/// The name a record shows under (one from before names were kept has none yet: it gets
/// one the next time its player finishes a match).
fn display_name(r: &BrRecord) -> String {
    if r.name.is_empty() {
        t("(без імені)", "(no name)").to_string()
    } else {
        r.name.clone()
    }
}

fn t_save() -> &'static str {
    t(
        "Не вдалося зберегти статистику",
        "Could not save the records",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stats() -> BrStats {
        BrStats {
            by_name: HashMap::new(),
            path: None,
        }
    }

    #[test]
    fn leaderboards_rank_and_announce() {
        let mut s = stats();
        let news = s.record_match("#a", "Аня", true, 12, 3, 40.0);
        assert!(
            news.iter().any(|n| n.0.contains("Аня")),
            "first ever is news"
        );
        s.record("#b", "Боря", 1, 5, 400.0, 220.0);
        let news = s.record("#b", "Боря", 1, 9, 300.0, 10.0);
        assert!(
            news.iter().any(|n| n.0.contains("Боря")),
            "Боря took the wins"
        );
        let boards = s.boards(Some("#a"));
        let wins = boards.iter().find(|b| b.kind == BoardKind::Wins).unwrap();
        assert_eq!(wins.rows[0], ("Боря".to_string(), 2.0));
        assert_eq!(wins.mine, Some((2, 1.0)));
        let longest = boards
            .iter()
            .find(|b| b.kind == BoardKind::Longest)
            .unwrap();
        assert_eq!(longest.rows[0].0, "Боря");
        assert_eq!(s.longest_ever(), 220.0);
        // K/D only after five matches; the name follows the latest one played under
        let kd = boards
            .iter()
            .find(|b| b.kind == BoardKind::KillDeath)
            .unwrap();
        assert!(kd.rows.is_empty());
        s.record_match("#a", "Анна", false, 1, 1, 0.0);
        assert_eq!(s.get("#a").name, "Анна");
        assert_eq!(s.get("#a").matches, 2);
    }

    #[test]
    fn old_records_get_names_and_owners() {
        let mut s = stats();
        // kept under a name (before installation numbers) and without one (before names)
        s.by_name.insert(
            "Віра".into(),
            BrRecord {
                matches: 10,
                kills: 50,
                best: 2,
                longest: 300.0,
                ..Default::default()
            },
        );
        s.by_name.insert(
            "#old".into(),
            BrRecord {
                matches: 3,
                kills: 9,
                ..Default::default()
            },
        );
        let kills = |s: &BrStats| {
            s.boards(None)
                .into_iter()
                .find(|b| b.kind == BoardKind::Kills)
                .unwrap()
                .rows
        };
        assert!(kills(&s).iter().all(|r| !r.0.is_empty()), "{:?}", kills(&s));
        // playing under that name again takes the old record over
        s.record_match("#v", "Віра", true, 5, 1, 20.0);
        let r = s.get("#v");
        assert_eq!((r.matches, r.kills, r.best), (11, 55, 2));
        assert_eq!(r.longest, 300.0);
        assert!(!s.by_name.contains_key("Віра"));
    }
}
