//! Network messages. Encoded with bincode and carried by [`crate::net::Channel`].

use crate::map::MapId;
use crate::vehicles::VehicleKind;
use crate::weapons::{Cause, Killstreak, Loadout, WeaponId};
use crate::{Mode, Team};
use serde::{Deserialize, Serialize};

pub type V3 = [f32; 3];

/// Extra bits packed into `PlayerSnap::stance` (low 2 bits are the stance).
pub mod stance_bits {
    pub const PROTECTED: u8 = 0x40;
    pub const SILENT: u8 = 0x80;
}

pub mod flags {
    pub const SPRINT: u8 = 1;
    pub const ADS: u8 = 2;
    pub const ON_GROUND: u8 = 4;
    pub const USE: u8 = 8;
    pub const RELOADING: u8 = 16;
    pub const HAS_BOMB: u8 = 32;
    /// Visible on the receiving player's radar (UAV or recently fired).
    pub const ON_RADAR: u8 = 64;
    pub const FIRING: u8 = 128;
}

/// Client → server.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum C2S {
    Hello {
        name: String,
        password: String,
        version: u16,
        /// `GAME_VERSION` of the client build.
        game: String,
    },
    JoinTeam(Option<Team>),
    /// Selects a class; spawns immediately if allowed.
    SetLoadout(Loadout),
    /// Request to respawn now (when the respawn timer allows).
    Spawn,
    Input(InputState),
    Fire {
        weapon: WeaponId,
        origin: V3,
        dirs: Vec<V3>,
    },
    Reload,
    Switch(u8),
    Grenade {
        origin: V3,
        dir: V3,
        cook: f32,
        /// A smoke grenade instead of a frag.
        smoke: bool,
    },
    Melee,
    UseKillstreak {
        target: Option<[f32; 2]>,
        /// The ground robot: drive it yourself (true) or let it fight on its own.
        pilot: bool,
    },
    /// Steering a killstreak vehicle (sent like `Input`, unreliable): where the pilot's
    /// client moved it and which `vehicles::keys` are held.
    Drive {
        pos: V3,
        yaw: f32,
        pitch: f32,
        keys: u8,
    },
    Chat {
        text: String,
        team_only: bool,
    },
    Rcon {
        password: String,
        cmd: String,
    },
    Pong(u32),
    /// Give up spawn protection early (so you can fight).
    DropProtection,
    /// Swap the current weapon for a dropped one (entity id).
    PickUp(u16),
    Disconnect,
    /// The player's language, sent before `Hello` and again when it changes: the server
    /// writes its messages to them in it. (Last in the list, and separate from `Hello`, so
    /// servers that do not know it simply skip it.)
    Lang(crate::lang::Lang),
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct InputState {
    pub seq: u32,
    /// Matches `S2C::Spawned::life` — inputs from a previous life are ignored.
    pub life: u16,
    pub pos: V3,
    pub vel: V3,
    pub yaw: f32,
    pub pitch: f32,
    pub stance: u8,
    pub flags: u8,
    pub slot: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum MatchState {
    /// Waiting for enough players — free play, no score.
    Warmup,
    /// Countdown before the match/round begins (movement frozen).
    PreRound,
    Playing,
    /// Sabotage round finished, short pause.
    RoundEnd,
    /// Match finished, scoreboard shown before the next map.
    PostMatch,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PlayerSnap {
    pub id: u8,
    pub team: Team,
    pub alive: bool,
    pub pos: V3,
    pub yaw: f32,
    pub pitch: f32,
    pub stance: u8,
    pub weapon: WeaponId,
    pub flags: u8,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MeSnap {
    pub alive: bool,
    pub hp: f32,
    pub slot: u8,
    pub ammo: [(u16, u16); 2],
    /// Weapons in both slots (they change when you pick one up).
    pub weapons: [WeaponId; 2],
    pub grenades: u8,
    pub smokes: u8,
    pub streak: u16,
    pub rewards: Vec<Killstreak>,
    pub respawn_in: f32,
    /// Our team has an active UAV.
    pub uav: bool,
    /// Last processed input sequence.
    pub ack_seq: u32,
    /// Plant/defuse progress 0..1 when we are doing it.
    pub progress: f32,
    /// Seconds of spawn protection left.
    pub protected: f32,
    /// The vehicle we are steering, if any.
    pub vehicle: Option<VehicleHud>,
}

/// The vehicle a player steers, as the server sees it.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VehicleHud {
    pub id: u16,
    pub kind: VehicleKind,
    pub pos: V3,
    pub yaw: f32,
    pub hp: f32,
    pub time_left: f32,
    /// Bombs left (bomber).
    pub ammo: u8,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EntSnap {
    Grenade {
        id: u16,
        pos: V3,
        smoke: bool,
    },
    /// A smoke cloud; `age` in seconds (its size follows `consts::smoke_radius`).
    Smoke {
        id: u16,
        pos: V3,
        age: f32,
    },
    Heli {
        id: u16,
        pos: V3,
        yaw: f32,
        team: Team,
    },
    Plane {
        id: u16,
        pos: V3,
        yaw: f32,
    },
    /// A killstreak vehicle (`auto`: a robot fighting on its own).
    Vehicle {
        id: u16,
        kind: VehicleKind,
        team: Team,
        owner: u8,
        pos: V3,
        yaw: f32,
        pitch: f32,
        auto: bool,
    },
    /// A bomb falling from a drone.
    Drop {
        id: u16,
        pos: V3,
    },
    /// A dropped weapon lying on the ground.
    Weapon {
        id: u16,
        weapon: WeaponId,
        pos: V3,
        yaw: f32,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BombSnap {
    pub pos: V3,
    pub carrier: Option<u8>,
    /// Site (by defending team) where the bomb is planted.
    pub planted_at: Option<Team>,
    pub timer: f32,
    /// Team that may use the bomb (None = neutral at round start).
    pub owner: Option<Team>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShotFx {
    pub id: u8,
    pub weapon: WeaponId,
    pub from: V3,
    pub to: V3,
    /// Hit a player (spawn a toon "pow" instead of dust).
    pub flesh: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Snapshot {
    pub tick: u32,
    pub state: MatchState,
    pub time_left: f32,
    pub scores: [i32; 2],
    pub overtime: bool,
    pub players: Vec<PlayerSnap>,
    pub me: MeSnap,
    pub ents: Vec<EntSnap>,
    pub bomb: Option<BombSnap>,
    pub shots: Vec<ShotFx>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ScoreRow {
    pub id: u8,
    pub name: String,
    pub team: Team,
    pub score: i32,
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    pub ping: u16,
    pub bot: bool,
    pub alive: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameSettings {
    pub score_limit: i32,
    pub time_limit: f32,
    pub rounds_to_win: u8,
    pub friendly_fire: bool,
    pub plant_time: f32,
    pub defuse_time: f32,
    pub bomb_timer: f32,
}

/// Server → client.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum S2C {
    Welcome {
        id: u8,
        server_name: String,
        motd: String,
        map: MapId,
        mode: Mode,
        settings: GameSettings,
    },
    Reject(String),
    /// Map/mode change: the client rebuilds the level.
    MapChange {
        map: MapId,
        mode: Mode,
        settings: GameSettings,
    },
    Snapshot(Box<Snapshot>),
    Scoreboard(Vec<ScoreRow>),
    Spawned {
        pos: V3,
        yaw: f32,
        life: u16,
        loadout: Loadout,
    },
    TeamSet(Team),
    Correction {
        pos: V3,
        life: u16,
    },
    Kill {
        killer: Option<u8>,
        victim: u8,
        cause: Cause,
        headshot: bool,
        killer_name: String,
        victim_name: String,
        killer_team: Team,
        victim_team: Team,
    },
    HitConfirm {
        kill: bool,
        headshot: bool,
    },
    Damaged {
        from: V3,
        amount: f32,
    },
    Explosion {
        pos: V3,
        radius: f32,
        big: bool,
    },
    Melee {
        id: u8,
    },
    ScorePopup {
        points: i32,
        text: String,
    },
    Chat {
        from: String,
        team: Team,
        text: String,
        team_only: bool,
    },
    Center {
        text: String,
        secs: f32,
    },
    StreakReady(Killstreak),
    StreakUsed {
        by: String,
        team: Team,
        streak: Killstreak,
    },
    Airstrike {
        from: V3,
        to: V3,
    },
    /// Artillery shells are on their way to this point (they land over a few seconds).
    Artillery {
        target: V3,
        radius: f32,
    },
    MatchEnd {
        winner: Option<Team>,
        text: String,
    },
    RoundEnd {
        winner: Option<Team>,
        text: String,
    },
    BombEvent {
        text: String,
    },
    RconReply(String),
    /// Server gameplay tuning (sent after `Welcome` and whenever an admin changes it).
    Tuning(Box<crate::tuning::Tuning>),
    /// Latency probe; the client answers with `C2S::Pong`.
    Ping(u32),
}
