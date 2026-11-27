//! Authoritative game state: players, match flow, spawning, input validation and snapshots.

mod player;
pub use player::Player;
mod connect;
mod handle;
mod match_flow;
mod movement;
mod snapshot;
mod spawn;
mod teams;
mod tick;
mod visibility;

use crate::bots::Bot;
use crate::config::ServerConfig;
use crate::nav::Nav;
use glam::{vec2, vec3, Vec3};
use std::collections::VecDeque;
use std::net::SocketAddr;
use toon_shared::consts::*;
use toon_shared::lang::{pick, t, Lang, Text};
use toon_shared::map::{Map, MapId, Spawn};
use toon_shared::physics::{body_box, Body, Stance, World};
use toon_shared::protocol::*;
use toon_shared::rng::Rng;
use toon_shared::weapons::{Cause, Killstreak, Loadout, WeaponId, DEFAULT_CLASSES};
use toon_shared::{maps, tr, Mode, Team};

pub const PREROUND_FIRST: f64 = 8.0;
/// Most bot route searches in one tick (the rest wait for the next ones).
pub const PATHS_PER_TICK: u32 = 6;
pub const PREROUND_NEXT: f64 = 5.0;
pub const ROUND_END_TIME: f64 = 9.0;
pub const POSTMATCH_TIME: f64 = 12.0;

#[derive(Clone, Copy, Debug)]
pub struct WState {
    pub id: WeaponId,
    pub mag: u16,
    pub reserve: u16,
}

#[derive(Clone, Copy, Debug)]
pub struct HistEntry {
    pub t: f64,
    pub pos: Vec3,
    pub yaw: f32,
    pub stance: Stance,
}

/// A player's match, for the awards at its end.
#[derive(Clone, Copy, Debug, Default)]
pub struct MatchStats {
    pub headshots: u16,
    /// The longest kill (metres) and the gun.
    pub longest: f32,
    pub longest_with: Option<WeaponId>,
    pub best_streak: u32,
    pub blast_kills: u16,
    pub arrow_kills: u16,
    pub knife_kills: u16,
    pub plants: u16,
    pub defuses: u16,
    /// Shots fired and hits when the match began (the totals run on across matches).
    pub fired_at_start: u32,
    pub hit_at_start: u32,
}

pub enum Dest {
    All,
    Team(Team),
    One(u8),
}

/// A number nobody can guess (from the system's randomness behind `RandomState`).
pub fn random_u64() -> u64 {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u64(0x5eed);
    h.finish()
}

/// An enemy stays in a player's snapshots this long after they could last see them (no
/// popping in and out at a corner, and room for the time the check runs at).
pub(crate) const SEEN_LINGER: f64 = 0.6;

/// In the air nobody gets higher than this above where they last stood, beyond a jump
/// (1.1 m with the default tuning, more when the server's tuning jumps higher).
const MAX_AIR_RISE: f32 = 1.1;
/// Falls up to this many metres do not hurt (and never one's own jump back down); each
/// metre more costs this much health.
pub const SAFE_FALL: f32 = 6.5;
pub const FALL_DAMAGE: f32 = 11.0;

pub struct Grenade {
    pub id: u16,
    pub owner: u8,
    pub pos: Vec3,
    pub vel: Vec3,
    pub explode_at: f64,
    /// Pops a smoke cloud instead of exploding.
    pub smoke: bool,
    /// A molotov or a flashbang (`inventory::THROW_*`; 0 for frag and smoke).
    pub kind: u8,
}

/// A patch of fire from a molotov.
pub struct Fire {
    pub id: u16,
    pub owner: u8,
    pub pos: Vec3,
    pub radius: f32,
    pub until: f64,
}

pub struct Heli {
    pub id: u16,
    pub owner: u8,
    pub team: Team,
    pub angle: f32,
    pub pos: Vec3,
    pub yaw: f32,
    pub hp: f32,
    pub until: f64,
    pub next_shot: f64,
    pub next_target: f64,
    pub target: Option<u8>,
}

pub struct PendingBlast {
    pub at: f64,
    pub pos: Vec3,
    pub radius: f32,
    pub damage: f32,
    pub owner: Option<u8>,
    pub cause: Cause,
}

pub struct Sab {
    pub pos: Vec3,
    pub carrier: Option<u8>,
    pub owner: Option<Team>,
    /// Site (by defending team) where the bomb is planted.
    pub planted_at: Option<Team>,
    pub explode_at: f64,
    pub plant: Option<(u8, f64)>,
    pub defuse: Option<(u8, f64)>,
    pub dropped_since: f64,
    pub overtime_end: f64,
}

pub struct Game {
    pub cfg: ServerConfig,
    /// Live movement/weapon tuning (sent to clients).
    pub tuning: toon_shared::tuning::Tuning,
    pub map: Map,
    pub world: World,
    /// What blasts have broken (see `destruct`).
    pub wreck: toon_shared::destruct::Wreck,
    pub nav: Nav,
    /// The map played before (with its world and routes), in case the rotation returns.
    spare_map: Option<(Map, World, Nav)>,
    /// When each door was last worked.
    pub door_used: std::collections::HashMap<u16, f64>,
    pub mode: Mode,
    pub players: Vec<Option<Player>>,
    pub state: MatchState,
    pub state_end: f64,
    pub round_end: f64,
    pub scores: [i32; 2],
    pub overtime: bool,
    pub round: u32,
    pub time: f64,
    pub tick: u32,
    pub rng: Rng,
    pub grenades: Vec<Grenade>,
    pub vehicles: Vec<crate::vehicles::Vehicle>,
    pub fires: Vec<Fire>,
    pub next_burn_fire: f64,
    /// Battle royale jeeps and boats.
    pub cars: Vec<crate::cars::Car>,
    pub drops: Vec<crate::vehicles::BombDrop>,
    /// Where and when players died or spawned lately (spawns avoid both).
    pub recent_deaths: Vec<(Vec3, f64)>,
    pub recent_spawns: Vec<(Vec3, f64)>,
    /// Smoke clouds: (entity id, centre, time it popped).
    pub smokes: Vec<(u16, Vec3, f64)>,
    pub helis: Vec<Heli>,
    pub pickups: Vec<crate::pickups::Pickup>,
    pub blasts: Vec<PendingBlast>,
    pub next_ent: u16,
    /// Pings up now, and the id of the last one.
    pub pings: Vec<crate::social::Ping>,
    pub next_ping: u16,
    /// Lobby requests: (who asks, whom).
    pub party_asks: Vec<(u8, u8)>,
    ent_wrapped: bool,
    pub uav_until: [f64; 2],
    /// Enemies bots have just spotted: (where, when, the team that saw them) — what a
    /// bot shares with the teammates around it.
    pub intel: Vec<(Vec3, f64, Team)>,
    pub sab: Sab,
    pub br: crate::br::BrState,
    pub rotation_idx: usize,
    pub outbox: Vec<(Dest, S2C, bool)>,
    pub shots: Vec<ShotFx>,
    /// Battle royale lobby: spots around the spawn points (worked out on first use).
    lobby_spots: Option<Vec<Spawn>>,
    /// The same rings round each team's base spawns (a team can outnumber its spawn
    /// points: nobody should start inside a teammate).
    base_spots: [Option<Vec<Spawn>>; 2],
    /// Dice for picking spawn spots (their own: how many spots there are to weigh does
    /// not shift the rest of the game's randomness).
    spawn_rng: Rng,
    /// The match starts at this time even if not everybody is ready.
    ready_deadline: Option<f64>,
    /// The longest kill on record here (a longer one is announced).
    pub record_longest: f32,
    pub bullets: Vec<crate::combat::Bullet>,
    pub shot_serial: u32,
    pub log: VecDeque<String>,
    pub bans: crate::admin::Bans,
    /// Battle royale records between matches.
    pub br_stats: crate::stats::BrStats,
    /// Who played here (for the admin's player pages).
    pub registry: crate::registry::Registry,
    /// On the public server list (shared with the announcing thread).
    pub public: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// Players to disconnect (id, reason) — handled by the network layer.
    pub kicked: Vec<(u8, String)>,
    pub next_housekeeping: f64,
    pub match_started: bool,
    /// Route searches bots may still make this tick.
    pub path_budget: u32,
    /// Where `save` writes the config.
    pub cfg_path: Option<std::path::PathBuf>,
}

impl Game {
    /// Smoke clouds as (centre, current radius), for line-of-sight checks.
    pub fn smoke_clouds(&self) -> Vec<(Vec3, f32)> {
        self.smokes
            .iter()
            .map(|&(_, c, born)| (c, smoke_radius((self.time - born) as f32)))
            .collect()
    }

    pub fn new(cfg: ServerConfig) -> Game {
        let seed = cfg.seed;
        let (map_id, mode) = cfg.rotation[0].resolve();
        let map = maps::build(map_id);
        let mut world = World::from_map(&map);
        let nav = Nav::build(&map, &world);
        for k in 0..world.doors.len() {
            world.set_door(k, false);
        }
        let bans = crate::admin::Bans::load(&cfg.data_dir);
        let br_stats = crate::stats::BrStats::load(&cfg.data_dir);
        let registry = crate::registry::Registry::load(&cfg.data_dir, cfg.geoip);
        let sab = Sab {
            pos: map.bomb,
            carrier: None,
            owner: None,
            planted_at: None,
            explode_at: 0.0,
            plant: None,
            defuse: None,
            dropped_since: 0.0,
            overtime_end: 0.0,
        };
        let tuning = cfg.tuning();
        let mut g = Game {
            tuning,
            cfg,
            wreck: toon_shared::destruct::Wreck::new(&map),
            map,
            world,
            nav,
            spare_map: None,
            door_used: Default::default(),
            mode,
            players: (0..MAX_PLAYERS).map(|_| None).collect(),
            state: MatchState::Warmup,
            state_end: 0.0,
            round_end: 0.0,
            scores: [0, 0],
            overtime: false,
            round: 0,
            time: 0.0,
            tick: 0,
            rng: seed.map_or_else(Rng::from_time, Rng::new),
            grenades: Vec::new(),
            vehicles: Vec::new(),
            fires: Vec::new(),
            next_burn_fire: 0.0,
            cars: Vec::new(),
            drops: Vec::new(),
            recent_deaths: Vec::new(),
            recent_spawns: Vec::new(),
            smokes: Vec::new(),
            helis: Vec::new(),
            pickups: Vec::new(),
            blasts: Vec::new(),
            next_ent: 1,
            pings: Vec::new(),
            next_ping: 0,
            party_asks: Vec::new(),
            ent_wrapped: false,
            uav_until: [0.0; 2],
            intel: Vec::new(),
            sab,
            br: Default::default(),
            rotation_idx: 0,
            outbox: Vec::new(),
            shots: Vec::new(),
            lobby_spots: None,
            base_spots: [None, None],
            spawn_rng: Rng::new(0x5a3d),
            ready_deadline: None,
            record_longest: br_stats.longest_ever(),
            bullets: Vec::new(),
            shot_serial: 0,
            log: VecDeque::new(),
            bans,
            br_stats,
            registry,
            public: Default::default(),
            kicked: Vec::new(),
            next_housekeeping: 0.0,
            match_started: false,
            path_budget: 0,
            cfg_path: None,
        };
        g.logf(format!("Карта: {} ({})", g.map.name, g.mode.name()));
        g
    }

    // ---------------------------------------------------------------- utilities

    pub fn logf(&mut self, s: String) {
        println!("[game] {s}");
        self.log.push_back(s);
        while self.log.len() > 300 {
            self.log.pop_front();
        }
    }

    pub fn send(&mut self, to: u8, msg: S2C) {
        self.outbox.push((Dest::One(to), msg, true));
    }
    pub fn broadcast(&mut self, msg: S2C) {
        self.outbox.push((Dest::All, msg, true));
    }
    pub fn send_team(&mut self, team: Team, msg: S2C) {
        self.outbox.push((Dest::Team(team), msg, true));
    }
    pub fn center_all(&mut self, text: impl Fn() -> String, secs: f32) {
        self.say(Dest::All, || S2C::Center { text: text(), secs });
    }

    /// Sends a message written in each recipient's own language: `f` runs once per player
    /// with their language current.
    pub fn say(&mut self, dest: Dest, f: impl Fn() -> S2C) {
        let to: Vec<(u8, Lang)> = self
            .players
            .iter()
            .flatten()
            .filter(|p| !p.is_bot())
            .filter(|p| match dest {
                Dest::All => true,
                Dest::One(id) => p.id == id,
                Dest::Team(t) => p.team == t,
            })
            .map(|p| (p.id, p.lang))
            .collect();
        for (id, l) in to {
            let msg = toon_shared::lang::with(l, &f);
            self.outbox.push((Dest::One(id), msg, true));
        }
    }

    pub fn p(&self, id: u8) -> Option<&Player> {
        self.players.get(id as usize).and_then(|p| p.as_ref())
    }
    pub fn pm(&mut self, id: u8) -> Option<&mut Player> {
        self.players.get_mut(id as usize).and_then(|p| p.as_mut())
    }
    pub fn ids(&self) -> Vec<u8> {
        self.players.iter().flatten().map(|p| p.id).collect()
    }
    pub fn humans(&self) -> usize {
        self.players
            .iter()
            .flatten()
            .filter(|p| !p.is_bot())
            .count()
    }
    pub fn team_count(&self, t: Team) -> usize {
        self.players
            .iter()
            .flatten()
            .filter(|p| p.team == t)
            .count()
    }
    pub fn settings(&self) -> GameSettings {
        GameSettings {
            score_limit: self.cfg.tdm.score_limit,
            time_limit: match self.mode {
                Mode::Tdm | Mode::Br => self.cfg.tdm.time_limit,
                Mode::Sab => self.cfg.sab.time_limit,
            },
            rounds_to_win: self.cfg.sab.rounds_to_win,
            friendly_fire: self.cfg.friendly_fire,
            plant_time: self.cfg.sab.plant_time,
            defuse_time: self.cfg.sab.defuse_time,
            bomb_timer: self.cfg.sab.bomb_timer,
        }
    }
    /// Applies new tuning live and pushes it to every client.
    pub fn set_tuning(&mut self, mut t: toon_shared::tuning::Tuning) {
        t.finalize();
        self.cfg.tuning = crate::config::tuning_json(&t);
        self.tuning = t;
        self.broadcast(S2C::Tuning(Box::new(self.tuning.clone())));
    }

    pub fn new_ent_id(&mut self) -> u16 {
        loop {
            self.next_ent = self.next_ent.wrapping_add(1);
            if self.next_ent == 0 {
                self.next_ent = 1;
                self.ent_wrapped = true;
            }
            // (once the numbers have gone round, some are still on the map)
            if !self.ent_wrapped || !self.ent_in_use(self.next_ent) {
                return self.next_ent;
            }
        }
    }

    /// Something that stays around for a while (loot, guns, cars, boxes, crates) has
    /// this id.
    fn ent_in_use(&self, id: u16) -> bool {
        self.pickups.iter().any(|p| p.id == id)
            || self.br.loot.iter().any(|l| l.id == id)
            || self.cars.iter().any(|c| c.id == id)
            || self.br.boxes.iter().any(|b| b.id == id)
            || self.br.drops.iter().any(|d| d.id == id)
            || self.fires.iter().any(|f| f.id == id)
            || self.grenades.iter().any(|g| g.id == id)
    }
}
