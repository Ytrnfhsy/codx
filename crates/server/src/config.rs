//! Server configuration (`server.json`). Missing fields fall back to defaults.

use serde::{Deserialize, Serialize};
use std::path::Path;
use toon_shared::consts::DEFAULT_PORT;
use toon_shared::map::MapId;
use toon_shared::tuning::Tuning;
use toon_shared::Mode;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RotationEntry {
    pub map: String,
    pub mode: String,
}

impl Default for RotationEntry {
    fn default() -> Self {
        RotationEntry {
            map: "depot".into(),
            mode: "tdm".into(),
        }
    }
}

impl RotationEntry {
    pub fn resolve(&self) -> (MapId, Mode) {
        (
            MapId::parse(&self.map).unwrap_or(MapId::Depot),
            Mode::parse(&self.mode).unwrap_or(Mode::Tdm),
        )
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct TdmConfig {
    /// Kills needed to win.
    pub score_limit: i32,
    /// Minutes.
    pub time_limit: f32,
    pub respawn_delay: f32,
}

impl Default for TdmConfig {
    fn default() -> Self {
        TdmConfig {
            score_limit: 75,
            time_limit: 10.0,
            respawn_delay: 3.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct SabConfig {
    /// Minutes per round before sudden-death overtime.
    pub time_limit: f32,
    pub rounds_to_win: u8,
    pub bomb_timer: f32,
    pub plant_time: f32,
    pub defuse_time: f32,
    pub respawn_delay: f32,
    /// Minutes of overtime before the round is a draw.
    pub overtime_limit: f32,
}

impl Default for SabConfig {
    fn default() -> Self {
        SabConfig {
            time_limit: 10.0,
            rounds_to_win: 2,
            bomb_timer: 30.0,
            plant_time: 5.0,
            defuse_time: 5.0,
            respawn_delay: 5.0,
            overtime_limit: 4.0,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
    pub name: String,
    /// Address to bind (UDP game + TCP web/admin on the same port number).
    pub bind: String,
    pub port: u16,
    pub max_players: usize,
    /// Join password (empty = public).
    pub password: String,
    /// Admin password for the web panel and in-game `/rcon` (empty = admin disabled).
    pub rcon_password: String,
    pub motd: String,
    pub rotation: Vec<RotationEntry>,
    pub tdm: TdmConfig,
    pub sab: SabConfig,
    pub friendly_fire: bool,
    pub auto_balance: bool,
    /// Fill the server with bots up to this many players.
    pub bots: usize,
    /// 0.0 (easy) .. 1.0 (hard).
    pub bot_skill: f32,
    /// Seconds of invulnerability after respawning (0 = off). Ends early when you shoot.
    pub spawn_protection: f32,
    /// Players required to start a match (otherwise warmup).
    pub min_players: usize,
    pub http_enabled: bool,
    /// Directory for bans and logs.
    pub data_dir: String,
    /// Fine tuning of movement and every weapon (see `tune` admin command).
    /// Missing fields fall back to factory defaults.
    pub tuning: serde_json::Value,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            name: "Toon Warfare сервер".into(),
            bind: "0.0.0.0".into(),
            port: DEFAULT_PORT,
            max_players: 18,
            password: String::new(),
            rcon_password: String::new(),
            motd: "Ласкаво просимо! Грай чесно :)".into(),
            rotation: vec![
                RotationEntry {
                    map: "depot".into(),
                    mode: "tdm".into(),
                },
                RotationEntry {
                    map: "market".into(),
                    mode: "sab".into(),
                },
                RotationEntry {
                    map: "cargo".into(),
                    mode: "tdm".into(),
                },
                RotationEntry {
                    map: "depot".into(),
                    mode: "sab".into(),
                },
                RotationEntry {
                    map: "market".into(),
                    mode: "tdm".into(),
                },
            ],
            tdm: TdmConfig::default(),
            sab: SabConfig::default(),
            friendly_fire: false,
            auto_balance: true,
            bots: 6,
            bot_skill: 0.4,
            spawn_protection: 3.0,
            min_players: 2,
            http_enabled: true,
            data_dir: "data".into(),
            tuning: tuning_json(&Tuning::default()),
        }
    }
}

impl ServerConfig {
    /// Loads the config, creating a default file if it does not exist.
    pub fn load_or_create(path: &Path) -> Result<ServerConfig, String> {
        if path.exists() {
            let text =
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let cfg: ServerConfig =
                serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
            Ok(cfg.sanitized())
        } else {
            let cfg = ServerConfig::default();
            cfg.save(path)?;
            Ok(cfg)
        }
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
    }

    /// Factory defaults overlaid with the config's (possibly partial) `tuning` section.
    pub fn tuning(&self) -> Tuning {
        let mut base = serde_json::to_value(Tuning::default()).expect("tuning json");
        merge_json(&mut base, &self.tuning);
        let mut t: Tuning = serde_json::from_value(base).unwrap_or_default();
        t.finalize();
        t
    }

    pub fn sanitized(mut self) -> ServerConfig {
        self.max_players = self.max_players.clamp(2, toon_shared::consts::MAX_PLAYERS);
        self.bots = self.bots.min(self.max_players);
        self.bot_skill = self.bot_skill.clamp(0.0, 1.0);
        self.spawn_protection = self.spawn_protection.clamp(0.0, 30.0);
        self.min_players = self.min_players.max(1);
        if self.rotation.is_empty() {
            self.rotation.push(RotationEntry::default());
        }
        self.tdm.score_limit = self.tdm.score_limit.max(1);
        self.tdm.time_limit = self.tdm.time_limit.max(0.5);
        self.sab.time_limit = self.sab.time_limit.max(0.5);
        self.sab.rounds_to_win = self.sab.rounds_to_win.max(1);
        self.sab.bomb_timer = self.sab.bomb_timer.max(5.0);
        self.sab.plant_time = self.sab.plant_time.max(0.5);
        self.sab.defuse_time = self.sab.defuse_time.max(0.5);
        // store the full, validated tuning so admins see every knob in the file
        self.tuning = tuning_json(&self.tuning());
        self
    }
}

/// Rounds every float to 4 decimals (f32 values otherwise show up as 1.399999976158142).
pub fn round_json(v: &mut serde_json::Value) {
    match v {
        serde_json::Value::Number(n) if n.is_f64() => {
            let f = (n.as_f64().unwrap_or(0.0) * 10_000.0).round() / 10_000.0;
            *v = serde_json::json!(f);
        }
        serde_json::Value::Array(a) => a.iter_mut().for_each(round_json),
        serde_json::Value::Object(o) => o.values_mut().for_each(round_json),
        _ => {}
    }
}

/// Tuning as clean JSON.
pub fn tuning_json(t: &Tuning) -> serde_json::Value {
    let mut v = serde_json::to_value(t).expect("tuning json");
    round_json(&mut v);
    v
}

/// Recursively overlays `over` onto `base` (objects merge, everything else replaces).
pub fn merge_json(base: &mut serde_json::Value, over: &serde_json::Value) {
    match (base, over) {
        (serde_json::Value::Object(b), serde_json::Value::Object(o)) => {
            for (k, v) in o {
                match b.get_mut(k) {
                    Some(bv) => merge_json(bv, v),
                    None => {
                        b.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (b, o) if !o.is_null() => *b = o.clone(),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_tuning_overrides_merge_onto_defaults() {
        let cfg: ServerConfig = serde_json::from_str(r#"{ "tuning": { "movement": { "sprint_speed": 9.5 }, "weapons": { "m249": { "rpm": 900, "weight": 12 } } } }"#).unwrap();
        let t = cfg.sanitized().tuning();
        assert_eq!(t.movement.sprint_speed, 9.5);
        assert_eq!(t.movement.walk_speed, Tuning::default().movement.walk_speed);
        let m249 = t.weapon(toon_shared::weapons::WeaponId::M249);
        assert_eq!(m249.rpm, 900.0);
        assert_eq!(m249.weight, 12.0);
        assert_eq!(m249.mag, 100);
    }
}
