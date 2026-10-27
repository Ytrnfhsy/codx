//! Code shared by the dedicated server and the game client:
//! gameplay constants, weapons, maps, movement physics, raycasts and the network protocol.

pub mod consts;
pub mod map;
pub mod maps;
pub mod maps_extra;
pub mod net;
pub mod physics;
pub mod props;
pub mod protocol;
pub mod ray;
pub mod rng;
pub mod skin;
pub mod tuning;
pub mod weapons;

pub use glam::{vec2, vec3, Vec2, Vec3};

/// Team of a player. `Spectator` never spawns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum Team {
    Spectator,
    Allies,
    Axis,
}

impl Team {
    pub fn index(self) -> usize {
        match self {
            Team::Allies => 0,
            Team::Axis => 1,
            Team::Spectator => 2,
        }
    }
    pub fn enemy(self) -> Team {
        match self {
            Team::Allies => Team::Axis,
            Team::Axis => Team::Allies,
            Team::Spectator => Team::Spectator,
        }
    }
    pub fn is_playing(self) -> bool {
        self != Team::Spectator
    }
    pub fn name(self) -> &'static str {
        match self {
            Team::Allies => "Морпіхи",
            Team::Axis => "Опфор",
            Team::Spectator => "Глядачі",
        }
    }
    pub fn from_index(i: usize) -> Team {
        match i {
            0 => Team::Allies,
            1 => Team::Axis,
            _ => Team::Spectator,
        }
    }
}

/// Game mode.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Mode {
    /// Team Deathmatch — «Командний бій».
    Tdm,
    /// Sabotage — «Саботаж».
    Sab,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Tdm => "Командний бій",
            Mode::Sab => "Саботаж",
        }
    }
    pub fn short(self) -> &'static str {
        match self {
            Mode::Tdm => "tdm",
            Mode::Sab => "sab",
        }
    }
    pub fn parse(s: &str) -> Option<Mode> {
        match s.to_ascii_lowercase().as_str() {
            "tdm" | "war" | "teamdeathmatch" => Some(Mode::Tdm),
            "sab" | "sabotage" => Some(Mode::Sab),
            _ => None,
        }
    }
}

/// Forward look vector for a yaw/pitch pair (yaw = 0 looks towards -Z, Y is up).
pub fn look_dir(yaw: f32, pitch: f32) -> Vec3 {
    let cp = pitch.cos();
    vec3(-yaw.sin() * cp, pitch.sin(), -yaw.cos() * cp)
}

/// Horizontal forward/right vectors for a yaw.
pub fn yaw_basis(yaw: f32) -> (Vec3, Vec3) {
    let fwd = vec3(-yaw.sin(), 0.0, -yaw.cos());
    let right = vec3(yaw.cos(), 0.0, -yaw.sin());
    (fwd, right)
}

/// Yaw that looks along the horizontal component of `dir`.
pub fn yaw_of(dir: Vec3) -> f32 {
    (-dir.x).atan2(-dir.z)
}

/// Wraps an angle into [-PI, PI].
pub fn wrap_angle(a: f32) -> f32 {
    let mut a = a % std::f32::consts::TAU;
    if a > std::f32::consts::PI {
        a -= std::f32::consts::TAU;
    } else if a < -std::f32::consts::PI {
        a += std::f32::consts::TAU;
    }
    a
}
