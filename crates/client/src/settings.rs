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
    /// The battle royale's own player limit and bots.
    pub br_max_players: usize,
    pub br_bots: usize,
    /// Players per battle royale squad (1 = everyone on their own).
    pub br_squad: u8,
    /// Battle royale weather: "random" or a weather's name.
    #[serde(default = "random_weather")]
    pub br_weather: String,
}

fn random_weather() -> String {
    "random".into()
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
            br_max_players: 50,
            br_bots: 50,
            br_squad: 1,
            br_weather: random_weather(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// A number this installation made up once: servers keep our records under it.
    #[serde(default)]
    pub player_key: u64,
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
    /// Battle royale character (`toon_shared::skins`).
    pub skin: u8,
    /// Smooth jagged edges (4× multisampling).
    #[serde(default = "yes")]
    pub msaa: bool,
    /// Colour grading of the finished picture (contrast, richer colours, darker corners).
    #[serde(default = "yes")]
    pub color_grade: bool,
    /// Quick edge smoothing on the finished picture (cheaper than multisampling).
    #[serde(default)]
    pub fxaa: bool,
    /// Very bright things (the sun, fire, flashes) glow.
    #[serde(default)]
    pub bloom: bool,
    /// FPS cap when vsync is off (0 = unlimited).
    pub fps_limit: u32,
    /// Sharpness of sun shadows: 1 low, 2 medium, 3 high.
    pub shadow_quality: u8,
    /// How far the world is drawn, as a share of the full distance (0.5..1).
    pub view_distance: f32,
    /// Smoke, dust, sparks and debris: 0 few, 1 normal, 2 many.
    pub effects: u8,
    /// Grass near you on big maps: 0 none, 1 some, 2 thick.
    #[serde(default = "one")]
    pub grass: u8,
    /// Language of the game (the system's one at first start).
    pub lang: toon_shared::lang::Lang,
    pub custom: Vec<CustomClass>,
    pub last_class: usize,
    pub recent: Vec<String>,
    /// Keys and mouse buttons for every action.
    pub binds: crate::binds::Binds,
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
            player_key: 0,
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
            msaa: true,
            color_grade: true,
            fxaa: false,
            bloom: false,
            skin: 0,
            fps_limit: 144,
            shadow_quality: 2,
            view_distance: 1.0,
            effects: 1,
            grass: 1,
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
            binds: Default::default(),
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
        if s.player_key == 0 {
            use std::hash::{BuildHasher, Hasher};
            let mut h = std::collections::hash_map::RandomState::new().build_hasher();
            h.write_u64(1);
            s.player_key = h.finish().max(1);
            s.save();
        }
        crate::netclient::PLAYER_KEY.store(s.player_key, std::sync::atomic::Ordering::Relaxed);
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

/// Ready-made video settings, from the weakest computers up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Preset {
    Low,
    Medium,
    High,
    Ultra,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Low, Preset::Medium, Preset::High, Preset::Ultra];

    pub fn name(self) -> &'static str {
        use toon_shared::lang::t;
        match self {
            Preset::Low => t("Низька", "Low"),
            Preset::Medium => t("Середня", "Medium"),
            Preset::High => t("Висока", "High"),
            Preset::Ultra => t("Максимум", "Ultra"),
        }
    }
}

impl Settings {
    /// Sets every video option that decides speed to the preset's values.
    pub fn apply_preset(&mut self, p: Preset) {
        let (scale, shadows, sq, textures, outlines, msaa, view, fx) = match p {
            Preset::Low => (0.6, false, 1, false, false, false, 0.6, 0),
            Preset::Medium => (0.8, true, 1, true, true, false, 0.8, 1),
            Preset::High => (1.0, true, 2, true, true, true, 1.0, 1),
            Preset::Ultra => (1.0, true, 3, true, true, true, 1.0, 2),
        };
        // (without multisampling the quick smoothing takes over; glow from High up)
        self.fxaa = matches!(p, Preset::Medium);
        self.bloom = matches!(p, Preset::High | Preset::Ultra);
        self.grass = match p {
            Preset::Low => 0,
            Preset::Medium | Preset::High => 1,
            Preset::Ultra => 2,
        };
        self.render_scale = scale;
        self.shadows = shadows;
        self.shadow_quality = sq;
        self.textures = textures;
        self.outlines = outlines;
        self.msaa = msaa;
        self.view_distance = view;
        self.effects = fx;
    }

    /// The preset the current options match, if any.
    pub fn preset(&self) -> Option<Preset> {
        Preset::ALL.into_iter().find(|p| {
            let mut s = self.clone();
            s.apply_preset(*p);
            (s.render_scale - self.render_scale).abs() < 0.01
                && s.shadows == self.shadows
                && (s.shadow_quality == self.shadow_quality || !self.shadows)
                && s.textures == self.textures
                && s.outlines == self.outlines
                && s.msaa == self.msaa
                && (s.view_distance - self.view_distance).abs() < 0.01
                && s.effects == self.effects
                && s.grass == self.grass
                && s.fxaa == self.fxaa
                && s.bloom == self.bloom
        })
    }

    /// Size of the shadow map.
    pub fn shadow_res(&self) -> u32 {
        match self.shadow_quality {
            0 | 1 => 1024,
            2 => 2048,
            _ => 4096,
        }
    }
}

fn yes() -> bool {
    true
}

fn one() -> u8 {
    1
}
