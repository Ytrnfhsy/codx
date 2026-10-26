//! Server configuration (`server.json`). Missing fields fall back to defaults.

use serde::{Deserialize, Serialize};
use std::path::Path;
use toon_shared::consts::DEFAULT_PORT;
use toon_shared::map::MapId;
use toon_shared::Mode;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct RotationEntry {
    pub map: String,
    pub mode: String,
}

impl Default for RotationEntry {
    fn default() -> Self {
        RotationEntry { map: "depot".into(), mode: "tdm".into() }
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
        TdmConfig { score_limit: 75, time_limit: 10.0, respawn_delay: 3.0 }
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
    /// Players required to start a match (otherwise warmup).
    pub min_players: usize,
    pub http_enabled: bool,
    /// Directory for bans and logs.
    pub data_dir: String,
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
            motd: "Ласкаво просимо! Грай чесно 🙂".into(),
            rotation: vec![
                RotationEntry { map: "depot".into(), mode: "tdm".into() },
                RotationEntry { map: "market".into(), mode: "sab".into() },
                RotationEntry { map: "cargo".into(), mode: "tdm".into() },
                RotationEntry { map: "depot".into(), mode: "sab".into() },
                RotationEntry { map: "market".into(), mode: "tdm".into() },
            ],
            tdm: TdmConfig::default(),
            sab: SabConfig::default(),
            friendly_fire: false,
            auto_balance: true,
            bots: 6,
            bot_skill: 0.5,
            min_players: 2,
            http_enabled: true,
            data_dir: "data".into(),
        }
    }
}

impl ServerConfig {
    /// Loads the config, creating a default file if it does not exist.
    pub fn load_or_create(path: &Path) -> Result<ServerConfig, String> {
        if path.exists() {
            let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let cfg: ServerConfig = serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
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

    pub fn sanitized(mut self) -> ServerConfig {
        self.max_players = self.max_players.clamp(2, toon_shared::consts::MAX_PLAYERS);
        self.bots = self.bots.min(self.max_players);
        self.bot_skill = self.bot_skill.clamp(0.0, 1.0);
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
        self
    }
}
