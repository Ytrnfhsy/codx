//! Battle royale: everybody boards a plane that crosses the map, jumps wherever they like,
//! scavenges weapons and supplies, and fights everybody else inside a safe zone that keeps
//! shrinking. One life each; the last one standing wins.

mod airdrop;
mod bot_goal;
mod downed;
mod inventory;
mod loot;
mod plane;
mod round;
mod snap;
mod spectate;
mod zone;

use crate::game::{Dest, Game, WState};
use crate::pickups::Pickup;
use glam::{vec2, vec3, Vec2, Vec3};
use toon_shared::consts::MAX_HEALTH;
use toon_shared::inventory::{
    armor_durability, helmet_durability, mag_size, mod_fits, mod_place, sight_fits, AmmoType,
    BANDAGE_HEAL, BANDAGE_MAX, BANDAGE_TIME, DRINK_BOOST, DRINK_TIME, MAX_SIGHT, MEDKIT_HEAL,
    MEDKIT_TIME, MODS, MOD_EXTMAG, MOD_SUPPRESSOR, USE_BANDAGE, USE_DRINK, USE_MEDKIT,
};
use toon_shared::lang::t;
use toon_shared::physics::{Body, Chute, Stance};
use toon_shared::protocol::{flags, BrSnap, EntSnap, Item, ItemAction, LootKind, MatchState, S2C};
use toon_shared::tr;
use toon_shared::weapons::{Cause, WeaponId};
use toon_shared::{look_dir, yaw_of, Mode};

/// The plane flies this high above the sea…
pub const PLANE_HEIGHT: f32 = 230.0;
/// …this fast (m/s).
pub const PLANE_SPEED: f32 = 75.0;
/// Walking this close to supplies picks them up.
const LOOT_RADIUS: f32 = 1.3;
/// A knocked-down player's pool of health: it bleeds out over the server's `br.bleed_time`
/// seconds and takes hits like normal health (no vest or helmet). Picking them up takes
/// `br.revive_time` seconds of a squadmate holding the use key; they get up with
/// `br.revive_hp`.
pub const DOWNED_HP: f32 = 100.0;

/// How far sideways a skydiver gets from `height` metres above the ground: with the
/// parachute opened right away, or in free fall until it opens by itself.
pub fn glide_reach(height: f32, open_now: bool) -> f32 {
    use toon_shared::physics::{
        CHUTE_GLIDE, CHUTE_OPEN_HEIGHT, CHUTE_SINK, FREEFALL_GLIDE, FREEFALL_SPEED,
    };
    let h = height.max(0.0);
    if open_now {
        return h / CHUTE_SINK * CHUTE_GLIDE;
    }
    let fall = (h - CHUTE_OPEN_HEIGHT).max(0.0);
    fall / FREEFALL_SPEED * FREEFALL_GLIDE + h.min(CHUTE_OPEN_HEIGHT) / CHUTE_SINK * CHUTE_GLIDE
}

/// Zone phases: (wait before shrinking, shrinking time, radius after it, damage per second
/// outside). The first wait counts from the moment the plane takes off.
const PHASES: [(f32, f32, f32, f32); 6] = [
    (75.0, 50.0, 380.0, 1.0),
    (45.0, 40.0, 220.0, 2.0),
    (40.0, 35.0, 120.0, 4.0),
    (30.0, 30.0, 60.0, 7.0),
    (25.0, 25.0, 25.0, 10.0),
    (20.0, 20.0, 0.0, 15.0),
];

#[derive(Clone, Copy, Debug, Default)]
pub struct Zone {
    pub from_c: Vec2,
    pub from_r: f32,
    pub to_c: Vec2,
    pub to_r: f32,
    /// Index into `PHASES` of the shrink coming next (or under way).
    pub phase: usize,
    pub shrink_at: f64,
    pub shrink_end: f64,
    pub dps: f32,
}

impl Zone {
    /// Centre and radius at `time`.
    pub fn at(&self, time: f64) -> (Vec2, f32) {
        if time <= self.shrink_at || self.shrink_end <= self.shrink_at {
            return (self.from_c, self.from_r);
        }
        let k =
            ((time - self.shrink_at) / (self.shrink_end - self.shrink_at)).clamp(0.0, 1.0) as f32;
        (
            self.from_c.lerp(self.to_c, k),
            self.from_r + (self.to_r - self.from_r) * k,
        )
    }
    pub fn contains(&self, time: f64, p: Vec3) -> bool {
        let (c, r) = self.at(time);
        vec2(p.x, p.z).distance(c) <= r
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Loot {
    pub id: u16,
    pub kind: LootKind,
    /// Rounds, medkits, or per cent left of a vest or helmet.
    pub amount: u16,
    pub pos: Vec3,
}

#[derive(Clone, Debug, Default)]
pub struct BrState {
    /// A round is under way (the plane took off).
    pub running: bool,
    pub plane_from: Vec3,
    pub plane_dir: Vec3,
    pub plane_t0: f64,
    /// Seconds the plane spends over the map.
    pub plane_time: f32,
    pub zone: Zone,
    /// Players who boarded the plane.
    pub entrants: usize,
    pub loot: Vec<Loot>,
    /// Next whole second at which the zone hurts those outside it.
    pub next_burn: f64,
    /// Supply crates coming down (or down) and when the next one comes.
    pub drops: Vec<AirDrop>,
    pub next_drop: f64,
    /// Crates sent this round.
    pub launched: usize,
    /// What knocked-out players left.
    pub boxes: Vec<crate::boxes::DeathBox>,
    /// The bombarded area, if any, and when the next one comes.
    pub red: Option<RedZone>,
    /// This round's weather and time of day.
    pub weather: toon_shared::weather::Weather,
    pub next_red: f64,
}

/// An area shelled for a while (after a warning): buildings and cover protect.
#[derive(Clone, Copy, Debug)]
pub struct RedZone {
    pub c: Vec2,
    pub r: f32,
    /// Shells fall from this moment until `end`.
    pub start: f64,
    pub end: f64,
    pub next_shell: f64,
}

const RED_FIRST: f64 = 130.0;
const RED_EVERY: f64 = 90.0;
const RED_WARNING: f64 = 8.0;
const RED_TIME: f64 = 20.0;

/// A supply crate: it comes down under a parachute inside the next circle and spills rare
/// gear around it when it lands.
#[derive(Clone, Debug)]
pub struct AirDrop {
    pub id: u16,
    pub pos: Vec3,
    pub ground: f32,
    pub landed: bool,
    /// The loot it left (gone once all of it is taken).
    pub items: Vec<u16>,
}

/// Supply crates per round, the first one this long after the plane leaves, then apart.
const DROPS: usize = 3;
const FIRST_DROP: f64 = 100.0;
const DROP_EVERY: f64 = 120.0;
/// How fast a crate sinks under its parachute.
const DROP_SINK: f32 = 5.0;

impl BrState {
    pub fn plane_pos(&self, time: f64) -> Vec3 {
        self.plane_from + self.plane_dir * PLANE_SPEED * (time - self.plane_t0) as f32
    }
    pub fn plane_flying(&self, time: f64) -> bool {
        self.running && ((time - self.plane_t0) as f32) < self.plane_time
    }
}
