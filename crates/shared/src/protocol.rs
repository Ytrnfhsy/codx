//! Network messages. Encoded with bincode and carried by [`crate::net::Channel`].

use crate::map::MapId;
use crate::vehicles::VehicleKind;
use crate::weapons::{Cause, Killstreak, Loadout, WeaponId};
use crate::{Mode, Team};
use serde::{Deserialize, Serialize};

pub type V3 = [f32; 3];

/// Extra bits packed into `PlayerSnap::stance` (low 2 bits are the stance).
pub mod stance_bits {
    /// Skydiving from the battle royale plane (free fall / under the parachute).
    pub const FREEFALL: u8 = 0x04;
    pub const CHUTE: u8 = 0x08;
    /// Battle royale: knocked down, crawling.
    pub const DOWNED: u8 = 0x10;
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
    /// Jump out of the battle royale plane.
    BrJump,
    /// The character the player wants to be in the battle royale (`skins::SKINS`).
    SetSkin(u8),
    /// Battle royale: use a medkit.
    UseMedkit,
    /// Battle royale: a bandage, an energy drink.
    UseBandage,
    UseDrink,
    /// Battle royale: a molotov or a flashbang (`inventory::THROW_*`).
    Throw {
        origin: V3,
        dir: V3,
        kind: u8,
    },
    /// Battle royale: something done with an item in the inventory screen.
    Item(ItemAction),
    /// Ready (or not yet) for the match to start.
    Ready(bool),
    /// The map marker shown to the squad (None removes it).
    Marker(Option<[f32; 2]>),
    /// Jeeps and boats.
    Car(CarCmd),
    /// Death boxes.
    Box(BoxCmd),
    /// Out of the battle royale: whose eyes to watch through (255 = a free camera).
    Spectate(u8),
    /// Open or shut door `k` (standing next to it).
    Door(u16),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoxCmd {
    Open(u16),
    /// Take thing `idx` (supplies first, then guns).
    Take(u16, u8),
    /// Take all the supplies.
    TakeAll(u16),
    Close,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CarCmd {
    Enter(u16),
    Exit,
    /// The driver's controls: throttle and steering in -100..100, handbrake.
    Drive {
        throttle: i8,
        steer: i8,
        brake: bool,
    },
}

/// What can be thrown out of the battle royale inventory.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Item {
    Ammo(crate::inventory::AmmoType),
    Medkit,
    Armor,
    Helmet,
    Backpack,
    /// A spare sight of this level, from the backpack.
    Sight(u8),
    /// A spare attachment of this kind, from the backpack.
    Mod(u8),
    Bandages,
    Drinks,
    Molotovs,
    Flashes,
    /// The gun in this hand slot.
    Weapon(u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ItemAction {
    /// Throw this many of it on the ground in front of you.
    Drop(Item, u16),
    /// Put a spare sight of `level` on the gun in `slot` (the old one goes in the backpack).
    AttachSight { level: u8, slot: u8 },
    /// Take the sight off the gun in `slot` into the backpack.
    DetachSight(u8),
    /// Put a spare attachment on the gun in `slot` (one already in its place goes in the
    /// backpack).
    AttachMod { kind: u8, slot: u8 },
    /// Take every attachment off the gun in `slot` into the backpack.
    DetachMods(u8),
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
    /// Battle royale character (`skins::SKINS`; 0 = team uniform).
    pub skin: u8,
    /// Battle royale squad (0 = on their own).
    pub squad: u8,
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
    /// Battle royale: knocked down (crawling; `hp` is what is left before bleeding out).
    pub downed: bool,
    /// The car and seat we sit in (seat 0 drives).
    pub seat: Option<(u16, u8)>,
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
        /// 0 frag or smoke, `inventory::THROW_*` otherwise.
        kind: u8,
    },
    /// A patch of fire from a molotov.
    Fire {
        id: u16,
        pos: V3,
        radius: f32,
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
    /// Battle royale supplies lying on the ground (taken by walking over them).
    Loot {
        id: u16,
        kind: LootKind,
        /// Rounds, medkits, or per cent left of a vest or helmet.
        amount: u16,
        pos: V3,
    },
    /// Everything a knocked-out player carried.
    DeathBox {
        id: u16,
        pos: V3,
    },
    /// A supply crate, coming down under its parachute or landed.
    AirDrop {
        id: u16,
        pos: V3,
        landed: bool,
    },
    /// A jeep or a boat.
    Car {
        id: u16,
        kind: crate::cars::CarKind,
        pos: V3,
        yaw: f32,
        speed: f32,
        /// Per cent of its health left.
        hp: u8,
        /// Who sits in each seat (255 = nobody; the driver first).
        seats: [u8; crate::cars::SEATS],
    },
}

/// Battle royale supplies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum LootKind {
    /// A box of rounds of one calibre (`amount` rounds).
    Ammo(crate::inventory::AmmoType),
    /// `amount` medkits.
    Medkit,
    /// A vest of a level 1..3 (`amount`: what is left of it, per cent).
    Armor(u8),
    /// A helmet of a level 1..3 (`amount`: per cent left).
    Helmet(u8),
    /// A backpack of a level 1..3.
    Backpack(u8),
    /// Two grenades and a smoke.
    Grenades,
    /// A sight for a gun (level 1..4: red dot, 2×, 4×, 8×).
    Sight(u8),
    /// Another attachment (`inventory::MOD_*`).
    Mod(u8),
    /// `amount` bandages.
    Bandages,
    Drink,
    Molotov,
    Flash,
}

impl LootKind {
    pub fn name(self) -> String {
        use crate::inventory::level_name;
        use crate::lang::t;
        match self {
            LootKind::Ammo(a) => a.name().into(),
            LootKind::Medkit => t("Аптечка", "Medkit").into(),
            LootKind::Armor(l) => format!("{} {}", t("Бронежилет", "Vest"), level_name(l)),
            LootKind::Helmet(l) => format!("{} {}", t("Шолом", "Helmet"), level_name(l)),
            LootKind::Backpack(l) => format!("{} {}", t("Рюкзак", "Backpack"), level_name(l)),
            LootKind::Grenades => t("Гранати", "Grenades").into(),
            LootKind::Sight(l) => crate::inventory::sight_name(l).into(),
            LootKind::Mod(k) => crate::inventory::mod_name(k).into(),
            LootKind::Bandages => t("Бинти", "Bandages").into(),
            LootKind::Drink => t("Енергетик", "Energy drink").into(),
            LootKind::Molotov => t("Коктейль Молотова", "Molotov").into(),
            LootKind::Flash => t("Світлошумова граната", "Flashbang").into(),
        }
    }
}

/// The battle royale state a player sees.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BrSnap {
    /// The round's weather and time of day.
    pub weather: crate::weather::Weather,
    /// Players still in the game (in the plane or on the ground).
    pub alive: u16,
    /// Safe zone now: centre x, z and radius.
    pub zone: [f32; 3],
    /// Where it shrinks to next.
    pub next: [f32; 3],
    /// Seconds until the zone starts shrinking (or, while it shrinks, until it stops).
    pub timer: f32,
    pub shrinking: bool,
    /// Damage per second outside the zone.
    pub dps: f32,
    /// The plane: position and heading, while it flies.
    pub plane: Option<(V3, f32)>,
    /// We are still in the plane (jump to leave it).
    pub in_plane: bool,
    /// What we carry.
    pub inv: crate::inventory::Inventory,
    /// The hand slot without a gun yet (or after throwing one away).
    pub empty_slot: Option<u8>,
    /// The squad (us included), in a fixed order.
    pub squad: Vec<SquadMate>,
    /// Supply crates (for the map, wherever they are).
    pub drops: Vec<V3>,
    /// The red zone: centre x, z, radius, seconds before the shelling starts.
    pub red: Option<[f32; 4]>,
    /// Our round once we are out (or it is over).
    pub summary: Option<BrSummary>,
    /// The death box we are looking into.
    pub open_box: Option<BoxView>,
}

/// How our battle royale round went, and our records on this server.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct BrSummary {
    pub place: u16,
    pub of: u16,
    pub kills: u16,
    pub damage: f32,
    /// Longest kill this round, metres.
    pub longest: f32,
    /// Seconds from the plane taking off to being out.
    pub survived: f32,
    pub matches: u32,
    pub wins: u32,
    pub total_kills: u32,
    pub best: u32,
}

/// What is in a death box: supplies, then guns (with the rounds in their magazines).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoxView {
    pub id: u16,
    pub name: String,
    pub items: Vec<(LootKind, u16)>,
    pub guns: Vec<(WeaponId, u16)>,
}

/// A member of our battle royale squad.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SquadMate {
    pub id: u8,
    pub name: String,
    /// Health (or what is left before bleeding out when downed).
    pub hp: f32,
    pub alive: bool,
    pub downed: bool,
    pub pos: V3,
    /// The marker they put on the map.
    pub marker: Option<[f32; 2]>,
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
    /// Fired through a suppressor: hardly heard, no flash.
    pub quiet: bool,
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
    /// Battle royale only.
    pub br: Option<BrSnap>,
    /// Before the match starts: (players ready, players in total, am I ready).
    pub ready: Option<(u8, u8, bool)>,
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
        /// How far the killer was, metres (0: no killer, or their own doing).
        dist: u16,
    },
    HitConfirm {
        kill: bool,
        headshot: bool,
    },
    Damaged {
        from: V3,
        amount: f32,
    },
    /// A flashbang went off in front of us: how hard (0..1).
    Flashed(f32),
    /// The doors that stand open (all the others are shut).
    Doors(Vec<u16>),
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
