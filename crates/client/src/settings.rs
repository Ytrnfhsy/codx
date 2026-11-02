//! Persistent player settings.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toon_shared::weapons::{Loadout, DEFAULT_CLASSES};

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct CustomClass {
    pub name: String,
    pub loadout: Loadout,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct HostSettings {
    pub name: String,
    pub port: u16,
    pub mode: String,
    pub map: String,
    pub bots: usize,
    pub bot_skill: f32,
    pub rcon: String,
    pub password: String,
    pub max_players: usize,
}

impl Default for HostSettings {
    fn default() -> Self {
        HostSettings {
            name: toon_shared::lang::t("Мій сервер", "My server").into(),
            port: toon_shared::consts::DEFAULT_PORT,
            mode: "tdm".into(),
            map: "depot".into(),
            bots: 6,
            bot_skill: 0.4,
            rcon: String::new(),
            password: String::new(),
            max_players: 12,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub name: String,
    pub address: String,
    pub password: String,
    pub sensitivity: f32,
    pub fov: f32,
    pub volume: f32,
    pub invert_y: bool,
    /// Average mouse movement over two frames: removes the stair-stepping you get when the
    /// mouse reports at a different rate than the game draws frames.
    pub mouse_smoothing: bool,
    pub show_fps: bool,
    pub fullscreen: bool,
    /// Draw cartoon outlines on characters (slightly slower).
    pub outlines: bool,
    /// Sun shadows from buildings and props.
    pub shadows: bool,
    /// Surface detail textures (brick, planks, sand...).
    pub textures: bool,
    /// 3D resolution scale (0.5 = half resolution, much faster on weak GPUs).
    pub render_scale: f32,
    /// Vertical sync (FPS locked to the monitor refresh rate). Applied on restart.
    pub vsync: bool,
    /// FPS cap when vsync is off (0 = unlimited).
    pub fps_limit: u32,
    /// Language of the game (the system's one at first start).
    pub lang: toon_shared::lang::Lang,
    pub custom: Vec<CustomClass>,
    pub last_class: usize,
    pub recent: Vec<String>,
    /// Favourite servers, as typed (an IP or a domain, with or without the port).
    pub favorites: Vec<String>,
    pub host: HostSettings,
}

impl Default for Settings {
    fn default() -> Self {
        // the system's language at first start, also for the default names below
        let lang = sys_locale::get_locale()
            .map(|l| toon_shared::lang::Lang::from_locale(&l))
            .unwrap_or_default();
        toon_shared::lang::with(lang, || Settings::defaults(lang))
    }
}

impl Settings {
    fn defaults(lang: toon_shared::lang::Lang) -> Self {
        use toon_shared::lang::t;
        Settings {
            name: format!(
                "{}{}",
                t("Боєць", "Soldier"),
                (std::process::id() % 900) + 100
            ),
            address: format!("127.0.0.1:{}", toon_shared::consts::DEFAULT_PORT),
            password: String::new(),
            sensitivity: 2.0,
            fov: 80.0,
            volume: 0.7,
            invert_y: false,
            mouse_smoothing: true,
            show_fps: true,
            fullscreen: false,
            outlines: true,
            shadows: true,
            textures: true,
            render_scale: 1.0,
            vsync: true,
            fps_limit: 144,
            lang,
            custom: (1..=3)
                .map(|i| CustomClass {
                    name: toon_shared::tr!("Мій клас {i}", "My class {i}"),
                    loadout: DEFAULT_CLASSES[(i - 1) % DEFAULT_CLASSES.len()].1,
                })
                .collect(),
            last_class: 0,
            recent: Vec::new(),
            favorites: Vec::new(),
            host: HostSettings::default(),
        }
    }
}

impl Default for CustomClass {
    fn default() -> Self {
        CustomClass {
            name: toon_shared::lang::t("Клас", "Class").into(),
            loadout: DEFAULT_CLASSES[0].1,
        }
    }
}

fn path() -> PathBuf {
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA").map(PathBuf::from)
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
    };
    base.unwrap_or_else(|| PathBuf::from("."))
        .join("toon-warfare")
        .join("settings.json")
}

impl Settings {
    pub fn load() -> Settings {
        let mut s: Settings = std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        while s.custom.len() < 3 {
            s.custom.push(CustomClass::default());
        }
        s.custom.truncate(3);
        for c in &mut s.custom {
            c.loadout = c.loadout.sanitized();
        }
        s
    }

    pub fn save(&self) {
        let p = path();
        if let Some(d) = p.parent() {
            let _ = std::fs::create_dir_all(d);
        }
        if let Ok(t) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(p, t);
        }
    }

    pub fn is_favorite(&self, addr: &str) -> bool {
        let a = server_key(addr);
        !a.is_empty() && self.favorites.iter().any(|f| server_key(f) == a)
    }

    /// Adds the server to the favourites, or takes it off.
    pub fn toggle_favorite(&mut self, addr: &str) {
        let a = server_key(addr);
        if a.is_empty() {
            return;
        }
        if self.is_favorite(addr) {
            self.favorites.retain(|f| server_key(f) != a);
        } else {
            self.favorites.push(addr.trim().to_string());
        }
    }

    /// Servers the browser asks directly: favourites, then recent ones.
    pub fn servers_to_ask(&self) -> Vec<String> {
        let mut v = self.favorites.clone();
        for r in &self.recent {
            if !self.is_favorite(r) {
                v.push(r.clone());
            }
        }
        v
    }

    pub fn remember_server(&mut self, addr: &str) {
        self.recent.retain(|a| a != addr);
        self.recent.insert(0, addr.to_string());
        self.recent.truncate(8);
    }

    /// All selectable classes: 5 defaults + custom ones.
    pub fn classes(&self) -> Vec<(String, Loadout)> {
        let mut v: Vec<(String, Loadout)> = DEFAULT_CLASSES
            .iter()
            .map(|(n, l)| (n.to_string(), *l))
            .collect();
        for c in &self.custom {
            v.push((c.name.clone(), c.loadout));
        }
        v
    }
}

/// A server address in one form for comparing: lower case, with the default port.
pub fn server_key(addr: &str) -> String {
    let a = addr.trim().to_lowercase();
    if a.is_empty() || a.contains(':') {
        a
    } else {
        format!("{a}:{}", toon_shared::consts::DEFAULT_PORT)
    }
}
