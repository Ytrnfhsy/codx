//! Authoritative game state: players, match flow, spawning, input validation and snapshots.

use crate::bots::Bot;
use crate::config::ServerConfig;
use crate::nav::Nav;
use glam::{vec3, Vec3};
use std::collections::VecDeque;
use std::net::SocketAddr;
use toon_shared::consts::*;
use toon_shared::lang::{pick, t, Lang, Text};
use toon_shared::map::{Map, MapId, Spawn};
use toon_shared::physics::{body_box, Body, Stance, World};
use toon_shared::protocol::*;
use toon_shared::rng::Rng;
use toon_shared::weapons::{Cause, Killstreak, Loadout, Perk2, Perk3, WeaponId, DEFAULT_CLASSES};
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

pub struct Player {
    pub id: u8,
    pub name: String,
    /// Language the server writes to this player in.
    pub lang: Lang,
    pub addr: Option<SocketAddr>,
    pub ip: String,
    pub bot: Option<Box<Bot>>,
    pub team: Team,
    pub alive: bool,
    pub hp: f32,
    pub body: Body,
    pub yaw: f32,
    pub pitch: f32,
    pub flags: u8,
    pub slot: u8,
    pub loadout: Loadout,
    pub has_loadout: bool,
    pub next_loadout: Option<Loadout>,
    pub weapons: [WState; 2],
    pub grenades: u8,
    pub smokes: u8,
    /// The killstreak vehicle this player is steering.
    pub driving: Option<u16>,
    pub shot_budget: f32,
    pub reload_end: Option<f64>,
    pub switch_end: f64,
    pub melee_ready: f64,
    pub last_damage: f64,
    pub last_shot: f64,
    /// Last time an input had the sprint flag (weapon raise delay).
    pub last_sprint: f64,
    /// Invulnerable until this time (spawn protection).
    pub protected_until: f64,
    /// Accuracy stats for this match.
    pub shots_fired: u32,
    pub shots_hit: u32,
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    pub score: i32,
    pub streak: u32,
    pub rewards: Vec<Killstreak>,
    pub attackers: Vec<(u8, f64)>,
    pub respawn_at: f64,
    pub life: u16,
    pub ping: f32,
    /// Pressed «Ready» (the match waits for everybody).
    pub ready: bool,
    /// Battle royale squad (0 = on your own).
    pub squad: u8,
    /// Battle royale: knocked down (crawling, `hp` bleeds out) and who did it.
    pub downed: bool,
    pub downed_by: Option<u8>,
    /// Revive progress 0..1 (on the one being revived).
    pub revive: f32,
    /// The map marker this player shares with the squad.
    pub marker: Option<[f32; 2]>,
    /// The death box they look into.
    pub open_box: Option<u16>,
    /// Out of the battle royale: whom they watch (or a free camera), and who got them.
    pub spectating: Option<u8>,
    pub spec_free: bool,
    pub killed_by: Option<u8>,
    /// How far the one who knocked them down was (for the kill feed).
    pub downed_dist: u16,
    /// This battle royale round: damage dealt, longest kill, place once out, when out.
    pub br_dmg: f32,
    pub br_longest: f32,
    pub br_place: u16,
    pub br_out_at: f64,
    pub last_seq: u32,
    pub move_budget: f32,
    pub history: VecDeque<HistEntry>,
    pub rcon: bool,
    pub rcon_fails: u32,
    pub progress: f32,
    pub joined_at: f64,
    pub chat_budget: f32,
    /// The things on the ground this player's client knows of.
    pub lying: std::collections::HashMap<u16, EntSnap>,
    /// The height of the last place they stood on, and the highest point of the fall
    /// they are in (None: not falling, or it does not count — a parachute, a car).
    pub support_y: f32,
    pub fall_top: Option<f32>,
    fall_life: u16,
    /// Backpack actions left (refills; nobody needs to drop things a hundred times a second).
    pub item_budget: f32,
    /// Battle royale: still riding the plane.
    pub in_plane: bool,
    /// Battle royale: everything carried besides the guns.
    pub inv: toon_shared::inventory::Inventory,
    /// Battle royale: each slot's reserve as last matched with the inventory (a reload
    /// takes rounds out of the reserve, and so out of the inventory).
    pub ammo_seen: [u16; 2],
    /// Battle royale: the slot that holds no gun yet (the next one picked up goes there).
    pub empty_slot: Option<u8>,
    /// Battle royale character.
    pub skin: u8,
}

impl Player {
    /// How many rounds the gun in hand holds (an extended magazine holds more).
    pub fn mag_cap(&self, base: u16) -> u16 {
        toon_shared::inventory::mag_size(
            base,
            self.inv
                .has_mod(self.slot as usize, toon_shared::inventory::MOD_EXTMAG),
        )
    }

    /// The gun in hand has a suppressor on.
    pub fn suppressed(&self) -> bool {
        self.inv
            .has_mod(self.slot as usize, toon_shared::inventory::MOD_SUPPRESSOR)
    }

    /// Battle royale: every gun's reserve is the inventory's ammo of its calibre.
    pub fn br_refresh_reserve(&mut self) {
        for (k, w) in self.weapons.iter_mut().enumerate() {
            if self.empty_slot == Some(k as u8) {
                w.reserve = 0;
            } else {
                w.reserve = self.inv.ammo(toon_shared::inventory::AmmoType::of(w.id));
            }
            self.ammo_seen[k] = w.reserve;
        }
    }

    /// Battle royale: rounds a reload moved from a reserve into a magazine leave the
    /// inventory too.
    pub fn br_sync_ammo(&mut self) {
        for k in 0..2 {
            let w = self.weapons[k];
            if self.empty_slot == Some(k as u8) {
                continue;
            }
            let used = self.ammo_seen[k].saturating_sub(w.reserve);
            if used > 0 {
                let t = toon_shared::inventory::AmmoType::of(w.id);
                self.inv.ammo[t.index()] = self.inv.ammo[t.index()].saturating_sub(used);
            }
        }
        self.br_refresh_reserve();
    }

    pub fn new(id: u8, name: String, addr: Option<SocketAddr>, now: f64) -> Player {
        let l = DEFAULT_CLASSES[0].1;
        Player {
            id,
            name,
            lang: Lang::Uk,
            ip: addr
                .map(|a| a.ip().to_string())
                .unwrap_or_else(|| "bot".into()),
            addr,
            bot: None,
            team: Team::Spectator,
            alive: false,
            hp: MAX_HEALTH,
            body: Body::default(),
            yaw: 0.0,
            pitch: 0.0,
            flags: 0,
            slot: 0,
            loadout: l,
            has_loadout: false,
            next_loadout: None,
            weapons: [
                WState {
                    id: l.primary,
                    mag: 0,
                    reserve: 0,
                },
                WState {
                    id: l.secondary,
                    mag: 0,
                    reserve: 0,
                },
            ],
            grenades: 0,
            smokes: 0,
            driving: None,
            shot_budget: 1.0,
            reload_end: None,
            switch_end: 0.0,
            melee_ready: 0.0,
            last_damage: -100.0,
            last_shot: -100.0,
            last_sprint: -100.0,
            protected_until: 0.0,
            shots_fired: 0,
            shots_hit: 0,
            kills: 0,
            deaths: 0,
            assists: 0,
            score: 0,
            streak: 0,
            rewards: Vec::new(),
            attackers: Vec::new(),
            respawn_at: 0.0,
            life: 0,
            ping: 0.05,
            ready: false,
            squad: 0,
            downed: false,
            downed_by: None,
            revive: 0.0,
            marker: None,
            open_box: None,
            spectating: None,
            spec_free: false,
            killed_by: None,
            downed_dist: 0,
            br_dmg: 0.0,
            br_longest: 0.0,
            br_place: 0,
            br_out_at: 0.0,
            last_seq: 0,
            move_budget: 2.0,
            history: VecDeque::new(),
            rcon: false,
            rcon_fails: 0,
            progress: 0.0,
            joined_at: now,
            chat_budget: 5.0,
            lying: Default::default(),
            support_y: f32::MAX,
            fall_top: None,
            fall_life: 0,
            item_budget: 12.0,
            in_plane: false,
            inv: Default::default(),
            ammo_seen: [0; 2],
            empty_slot: None,
            skin: 0,
        }
    }
    pub fn is_bot(&self) -> bool {
        self.bot.is_some()
    }
    pub fn eye(&self) -> Vec3 {
        self.body.pos + vec3(0.0, self.body.stance.eye(), 0.0)
    }
    pub fn chest(&self) -> Vec3 {
        self.body.pos + vec3(0.0, self.body.stance.height() * 0.62, 0.0)
    }
    pub fn weapon(&self) -> &WState {
        &self.weapons[self.slot as usize & 1]
    }
    pub fn weapon_mut(&mut self) -> &mut WState {
        &mut self.weapons[self.slot as usize & 1]
    }
    pub fn has_perk2(&self, p: Perk2) -> bool {
        self.loadout.perk2 == p
    }
    pub fn has_perk3(&self, p: Perk3) -> bool {
        self.loadout.perk3 == p
    }
    /// Position/yaw/stance at a past time (lag compensation).
    pub fn rewound(&self, t: f64) -> (Vec3, f32, Stance) {
        let h = &self.history;
        if h.is_empty() {
            return (self.body.pos, self.yaw, self.body.stance);
        }
        if t >= h.back().unwrap().t {
            return (self.body.pos, self.yaw, self.body.stance);
        }
        for i in (0..h.len()).rev() {
            if h[i].t <= t {
                let a = h[i];
                let b = if i + 1 < h.len() {
                    h[i + 1]
                } else {
                    return (a.pos, a.yaw, a.stance);
                };
                let k = ((t - a.t) / (b.t - a.t).max(1e-6)) as f32;
                return (
                    a.pos.lerp(b.pos, k),
                    a.yaw,
                    if k < 0.5 { a.stance } else { b.stance },
                );
            }
        }
        let a = h.front().unwrap();
        (a.pos, a.yaw, a.stance)
    }
}

pub enum Dest {
    All,
    Team(Team),
    One(u8),
}

/// In the air nobody gets higher than this above where they last stood (a jump is 1.1 m).
const MAX_AIR_RISE: f32 = 2.2;
/// Falls up to this many metres do not hurt; each metre more costs this much health.
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
    pub cause: toon_shared::weapons::Cause,
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
    pub bullets: Vec<crate::combat::Bullet>,
    pub shot_serial: u32,
    pub log: VecDeque<String>,
    pub bans: crate::admin::Bans,
    /// Battle royale records between matches.
    pub br_stats: crate::stats::BrStats,
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
            .map(|&(_, c, born)| {
                (
                    c,
                    toon_shared::consts::smoke_radius((self.time - born) as f32),
                )
            })
            .collect()
    }

    pub fn new(cfg: ServerConfig) -> Game {
        let (map_id, mode) = cfg.rotation[0].resolve();
        let map = maps::build(map_id);
        let mut world = World::from_map(&map);
        let nav = Nav::build(&map, &world);
        for k in 0..world.doors.len() {
            world.set_door(k, false);
        }
        let bans = crate::admin::Bans::load(&cfg.data_dir);
        let br_stats = crate::stats::BrStats::load(&cfg.data_dir);
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
            rng: Rng::from_time(),
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
            ent_wrapped: false,
            uav_until: [0.0; 2],
            intel: Vec::new(),
            sab,
            br: Default::default(),
            rotation_idx: 0,
            outbox: Vec::new(),
            shots: Vec::new(),
            lobby_spots: None,
            bullets: Vec::new(),
            shot_serial: 0,
            log: VecDeque::new(),
            bans,
            br_stats,
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

    // ---------------------------------------------------------------- connections

    pub fn sanitize_name(&self, raw: &str, skip: Option<u8>) -> String {
        let mut name: String = raw
            .chars()
            .filter(|c| !c.is_control())
            .take(20)
            .collect::<String>()
            .trim()
            .to_string();
        if name.is_empty() {
            name = "Гравець".into();
        }
        let taken = |n: &str| {
            self.players
                .iter()
                .flatten()
                .any(|p| Some(p.id) != skip && p.name == n)
        };
        if !taken(&name) {
            return name;
        }
        for i in 2..100 {
            let n = format!("{name}({i})");
            if !taken(&n) {
                return n;
            }
        }
        name
    }

    /// Handles a `Hello` from a new address. Returns the player id or a reject reason.
    pub fn connect(
        &mut self,
        addr: SocketAddr,
        name: &str,
        password: &str,
        version: u16,
    ) -> Result<u8, String> {
        self.connect_as(addr, name, password, version, GAME_VERSION, Lang::Uk)
    }

    /// `connect` for a client of game version `game`.
    pub fn connect_as(
        &mut self,
        addr: SocketAddr,
        name: &str,
        password: &str,
        version: u16,
        game: &str,
        lang: Lang,
    ) -> Result<u8, String> {
        // a refusal is written in the newcomer's language
        let id = toon_shared::lang::with(lang, || self.admit(addr, name, password, version, game))?;
        if let Some(p) = self.pm(id) {
            p.lang = lang;
        }
        self.welcome(id);
        Ok(id)
    }

    fn admit(
        &mut self,
        addr: SocketAddr,
        name: &str,
        password: &str,
        version: u16,
        game: &str,
    ) -> Result<u8, String> {
        if version != PROTOCOL_VERSION || (self.cfg.require_same_version && game != GAME_VERSION) {
            if crate::update::newer(game, GAME_VERSION) {
                return Err(tr!(
                    "{} На сервері v{GAME_VERSION}, у тебе v{game}: адміну треба оновити сервер (toon-server --update).",
                    "{} The server has v{GAME_VERSION}, you have v{game}: the admin needs to update the server (toon-server --update).",
                    pick(SERVER_OUTDATED)
                ));
            }
            return Err(tr!(
                "{} На сервері гра v{GAME_VERSION}, у тебе v{game}. Онови гру: кнопка «Оновити» справа вгорі в меню (або завантаж її з Releases на GitHub).",
                "{} The server has v{GAME_VERSION}, you have v{game}. Update the game: the Update button in the top right of the menu (or download it from GitHub Releases).",
                pick(OUTDATED)
            ));
        }
        let ip = addr.ip().to_string();
        if let Some(b) = self.bans.find(&ip) {
            return Err(tr!(
                "Тебе заблоковано на цьому сервері: {}",
                "You are banned from this server: {}",
                b.reason
            ));
        }
        if !self.cfg.password.is_empty() && password != self.cfg.password {
            return Err(t("Невірний пароль сервера", "Wrong server password").into());
        }
        if self.humans() >= self.max_players() {
            return Err(t("Сервер заповнений", "The server is full").into());
        }
        // make room by removing a bot if needed
        if self.players.iter().flatten().count() >= self.max_players() {
            if let Some(bot) = self
                .players
                .iter()
                .flatten()
                .find(|p| p.is_bot())
                .map(|p| p.id)
            {
                self.remove_player(bot, None);
            }
        }
        let Some(slot) = self.players.iter().position(|p| p.is_none()) else {
            return Err(t("Сервер заповнений", "The server is full").into());
        };
        let id = slot as u8;
        let name = self.sanitize_name(name, None);
        self.players[slot] = Some(Player::new(id, name.clone(), Some(addr), self.time));
        self.logf(format!("+ {name} ({ip}) підключився як #{id}"));
        Ok(id)
    }

    /// First messages to a player who just got in, and the news for everyone.
    fn welcome(&mut self, id: u8) {
        let name = self.p(id).map(|p| p.name.clone()).unwrap_or_default();
        self.send(
            id,
            S2C::Welcome {
                id,
                server_name: self.cfg.name.clone(),
                motd: self.cfg.motd.clone(),
                map: self.map.id,
                mode: self.mode,
                settings: self.settings(),
            },
        );
        self.send(id, S2C::Tuning(Box::new(self.tuning.clone())));
        if !self.world.doors.is_empty() {
            let doors = self.doors_msg();
            self.send(id, doors);
        }
        self.say(Dest::All, || S2C::Chat {
            from: String::new(),
            team: Team::Spectator,
            text: tr!("{name} приєднався до гри", "{name} joined the game"),
            team_only: false,
        });
    }

    pub fn remove_player(&mut self, id: u8, reason: Option<&str>) {
        // leaving a battle royale round counts as being knocked out: the things stay in
        // a box, the place is recorded, whoever knocked them down gets the kill
        if self.mode == Mode::Br && self.br.running && self.p(id).is_some_and(|p| p.alive) {
            self.kill(id, None, toon_shared::weapons::Cause::World, false);
        }
        let Some(p) = self.players[id as usize].take() else {
            return;
        };
        if self.sab.carrier == Some(id) {
            self.sab.carrier = None;
            self.sab.pos = p.body.pos;
            self.sab.dropped_since = self.time;
        }
        if !p.is_bot() {
            let text = match reason {
                Some(r) => format!("{} покинув гру ({r})", p.name),
                None => format!("{} покинув гру", p.name),
            };
            let name = p.name.clone();
            self.say(Dest::All, || S2C::Chat {
                from: String::new(),
                team: Team::Spectator,
                text: tr!("{name} покинув гру", "{name} left the game"),
                team_only: false,
            });
            self.logf(format!("- {text}"));
        }
    }

    pub fn add_bot(&mut self) -> Option<u8> {
        let total = self.players.iter().flatten().count();
        if total >= self.max_players() {
            return None;
        }
        let slot = self.players.iter().position(|p| p.is_none())?;
        let id = slot as u8;
        let base = crate::bots::BOT_NAMES[self.rng.below(crate::bots::BOT_NAMES.len())];
        let name = self.sanitize_name(&format!("[Бот] {base}"), None);
        let mut p = Player::new(id, name, None, self.time);
        p.bot = Some(Box::new(Bot::new(&mut self.rng)));
        // bots dress up as somebody in the battle royale
        p.skin = 1 + self.rng.below(toon_shared::skins::count() as usize - 1) as u8;
        let class = DEFAULT_CLASSES[self.rng.below(DEFAULT_CLASSES.len())].1;
        p.loadout = class;
        p.has_loadout = true;
        self.players[slot] = Some(p);
        let team = self.auto_team(id);
        self.set_team(id, team, false);
        Some(id)
    }

    /// Player limit of the mode being played (the battle royale has its own).
    pub fn max_players(&self) -> usize {
        if self.mode == Mode::Br {
            self.cfg.br.max_players
        } else {
            self.cfg.max_players
        }
    }

    /// Bots fill the server up to this many players (per mode, like the limit).
    pub fn bot_fill(&self) -> usize {
        if self.mode == Mode::Br {
            self.cfg.br.bots
        } else {
            self.cfg.bots
        }
    }

    /// The current mode's bot setting and player limit, to change them.
    pub fn limits_mut(&mut self) -> (&mut usize, &mut usize) {
        if self.mode == Mode::Br {
            (&mut self.cfg.br.bots, &mut self.cfg.br.max_players)
        } else {
            (&mut self.cfg.bots, &mut self.cfg.max_players)
        }
    }

    pub fn auto_team(&self, id: u8) -> Team {
        // battle royale: no sides, everybody goes on the same list
        if self.mode.solo() {
            return Team::Allies;
        }
        let count = |t: Team| {
            self.players
                .iter()
                .flatten()
                .filter(|p| p.id != id && p.team == t)
                .count()
        };
        let (a, b) = (count(Team::Allies), count(Team::Axis));
        if a < b {
            Team::Allies
        } else if b < a {
            Team::Axis
        } else {
            let score = |t: Team| self.scores[t.index()];
            if score(Team::Allies) <= score(Team::Axis) {
                Team::Allies
            } else {
                Team::Axis
            }
        }
    }

    pub fn set_team(&mut self, id: u8, team: Team, notify: bool) {
        let Some(p) = self.pm(id) else { return };
        if p.team == team {
            return;
        }
        let was_alive = p.alive;
        p.team = team;
        p.alive = false;
        p.streak = 0;
        p.respawn_at = 0.0;
        let name = p.name.clone();
        if was_alive {
            self.drop_bomb_if_carrier(id);
        }
        self.send(id, S2C::TeamSet(team));
        if notify {
            self.say(Dest::All, || S2C::Chat {
                from: String::new(),
                team,
                text: format!("{name} → {}", team.name()),
                team_only: false,
            });
        }
        // battle royale: whoever joins while the plane is still over the map boards it
        if team.is_playing() {
            self.br_board(id);
        }
    }

    /// Bots fill the server up to `cfg.bots` players, keeping teams balanced.
    fn manage_bots(&mut self) {
        let humans_playing = self
            .players
            .iter()
            .flatten()
            .filter(|p| !p.is_bot() && p.team.is_playing())
            .count();
        let bots: Vec<u8> = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.is_bot())
            .map(|p| p.id)
            .collect();
        let want = self.bot_fill().saturating_sub(humans_playing);
        if bots.len() < want {
            // a few per second (big servers fill up quickly, without a burst of joins)
            for _ in 0..(want - bots.len()).min(8) {
                self.add_bot();
            }
        } else if bots.len() > want {
            // too many (say, after a battle royale with fifty): a few leave every second,
            // from the bigger team
            let mut left = bots.clone();
            for _ in 0..(bots.len() - want).min(8) {
                let big = if self.team_count(Team::Allies) >= self.team_count(Team::Axis) {
                    Team::Allies
                } else {
                    Team::Axis
                };
                let Some(k) = left
                    .iter()
                    .position(|&b| self.p(b).map(|p| p.team) == Some(big))
                    .or((!left.is_empty()).then_some(0))
                else {
                    break;
                };
                let victim = left.remove(k);
                self.remove_player(victim, None);
            }
        }
        // balance bots between teams (there are none in the battle royale: moving a bot
        // there would knock it out of the round)
        let (a, b) = (self.team_count(Team::Allies), self.team_count(Team::Axis));
        if a.abs_diff(b) > 1 && !self.mode.solo() {
            let (big, small) = if a > b {
                (Team::Allies, Team::Axis)
            } else {
                (Team::Axis, Team::Allies)
            };
            if let Some(bot) = self
                .players
                .iter()
                .flatten()
                .find(|p| p.is_bot() && p.team == big && !p.alive)
                .map(|p| p.id)
            {
                self.set_team(bot, small, false);
            } else if let Some(bot) = self
                .players
                .iter()
                .flatten()
                .find(|p| p.is_bot() && p.team == big)
                .map(|p| p.id)
            {
                self.set_team(bot, small, false);
            }
        }
    }

    // ---------------------------------------------------------------- spawning

    pub fn can_spawn(&self, id: u8) -> bool {
        let Some(p) = self.p(id) else { return false };
        if p.alive || !p.team.is_playing() || !p.has_loadout {
            return false;
        }
        match self.state {
            MatchState::PostMatch | MatchState::RoundEnd => false,
            MatchState::Playing => {
                !(self.mode == Mode::Sab && self.overtime || self.mode == Mode::Br)
            }
            _ => true,
        }
    }

    pub fn respawn_delay(&self) -> f32 {
        match self.mode {
            Mode::Tdm | Mode::Br => self.cfg.tdm.respawn_delay,
            Mode::Sab => self.cfg.sab.respawn_delay,
        }
    }

    /// Free spots on rings around the given spawn points (worked out once a map).
    fn ring_spots(&self, spawns: &[Spawn]) -> Vec<Spawn> {
        let mut out = Vec::new();
        for s in spawns {
            for ring in 1..=3 {
                let r = ring as f32 * 2.2;
                let n = 6 * ring;
                for k in 0..n {
                    let a = k as f32 / n as f32 * std::f32::consts::TAU + ring as f32;
                    let at = s.pos + vec3(a.cos() * r, 0.0, a.sin() * r);
                    let Some(y) = self.world.floor_below(at + vec3(0.0, 1.0, 0.0), 2.5) else {
                        continue;
                    };
                    let at = vec3(at.x, y, at.z);
                    if toon_shared::physics::fits(
                        &self.world,
                        at + vec3(0.0, 0.05, 0.0),
                        Stance::Stand,
                    ) {
                        out.push(Spawn {
                            pos: at,
                            yaw: s.yaw,
                        });
                    }
                }
            }
        }
        out
    }

    /// Picks a spawn point. Rounds and matches start at the team bases; during play anyone
    /// may appear anywhere on the ground floor: away from enemies and out of their sight,
    /// not where someone just died, and (in team deathmatch) not far from teammates.
    fn pick_spawn(&mut self, id: u8, side_only: bool) -> (Vec3, f32) {
        let team = self.p(id).map(|p| p.team).unwrap_or(Team::Allies);
        let time = self.time;
        self.recent_deaths.retain(|d| time - d.1 < 12.0);
        self.recent_spawns.retain(|d| time - d.1 < 4.0);
        let base = side_only || self.state == MatchState::PreRound || !team.is_playing();
        let mut cands: Vec<Spawn> = if base && team.is_playing() && !self.mode.solo() {
            self.map.spawns[team.index()].clone()
        } else {
            self.map.all_spawns().copied().collect()
        };
        // battle royale lobby: dozens of players wait on a handful of spawn points, so
        // each one gets rings of spots around it (nobody ends up inside someone else)
        if self.mode == Mode::Br && base {
            if self.lobby_spots.is_none() {
                self.lobby_spots = Some(self.ring_spots(&cands));
            }
            cands.extend(self.lobby_spots.iter().flatten().copied());
        }
        if !base {
            let centre = self.map.center();
            let centre = vec3(centre.x, 0.0, centre.y);
            for _ in 0..48 {
                let p = self.nav.random_point(&mut self.rng);
                // ground floor only (no spawning on roofs or balconies), with room to stand
                let floor = self.world.ground(p.x, p.z).unwrap_or(0.0);
                if p.y > floor + 0.6
                    || !toon_shared::physics::fits(
                        &self.world,
                        p + vec3(0.0, 0.05, 0.0),
                        Stance::Stand,
                    )
                {
                    continue;
                }
                let to = centre - p;
                let yaw = if to.length() > 1.0 {
                    toon_shared::yaw_of(to)
                } else {
                    self.rng.range(-3.1, 3.1)
                };
                cands.push(Spawn { pos: p, yaw });
            }
        }
        let mut best = (f32::NEG_INFINITY, cands[0]);
        for s in &cands {
            let mut score = self.rng.range(0.0, 6.0);
            let mut min_enemy = 60.0f32;
            let mut min_friend = 60.0f32;
            for p in self.players.iter().flatten() {
                if !p.alive || p.id == id {
                    continue;
                }
                let d = p.body.pos.distance(s.pos);
                if d < 1.5 {
                    score -= 200.0;
                } else if d < 2.5 {
                    score -= 40.0;
                }
                if p.team == team {
                    min_friend = min_friend.min(d);
                } else {
                    min_enemy = min_enemy.min(d);
                    if d < 12.0 {
                        score -= 100.0;
                    }
                    // (the battle royale lobby: nobody is fighting yet, and hundreds of
                    // spots would each need a look over the hills)
                    if d < 55.0
                        && !(self.mode == Mode::Br && base)
                        && self
                            .world
                            .line_of_sight(p.eye(), s.pos + vec3(0.0, 1.5, 0.0))
                    {
                        score -= 60.0;
                    }
                }
            }
            // beyond ~28 m every spot is equally safe: let chance and teammates decide
            score += min_enemy.min(28.0);
            for (dp, _) in &self.recent_deaths {
                if dp.distance(s.pos) < 10.0 {
                    score -= 25.0;
                }
            }
            for (sp, _) in &self.recent_spawns {
                if sp.distance(s.pos) < 3.0 {
                    score -= 30.0;
                }
            }
            if !base {
                match self.mode {
                    // near the team, but not on top of it
                    Mode::Tdm if min_friend < 60.0 => {
                        score -= (min_friend - 15.0).max(0.0) * 0.3;
                    }
                    // Sabotage: stay on your own half of the map
                    Mode::Sab if team.is_playing() => {
                        let home = self.map.spawns[team.index()][0].pos;
                        let away = self.map.spawns[team.enemy().index()][0].pos;
                        if s.pos.distance(home) > s.pos.distance(away) {
                            score -= 40.0;
                        }
                    }
                    _ => {}
                }
            }
            if score > best.0 {
                best = (score, *s);
            }
        }
        self.recent_spawns.push((best.1.pos, time));
        (best.1.pos + vec3(0.0, 0.02, 0.0), best.1.yaw)
    }

    pub fn spawn_player(&mut self, id: u8, side_only: bool) {
        let (pos, yaw) = self.pick_spawn(id, side_only);
        let time = self.time;
        // protection counts from the moment fighting is possible
        let protect_from = if self.state == MatchState::PreRound {
            self.state_end
        } else {
            time
        };
        let protected_until = protect_from + self.cfg.spawn_protection as f64;
        let tuning = &self.tuning;
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        if let Some(l) = p.next_loadout.take() {
            p.loadout = l;
        }
        let l = p.loadout;
        p.alive = true;
        p.hp = MAX_HEALTH;
        p.body = Body {
            pos,
            vel: Vec3::ZERO,
            on_ground: true,
            stance: Stance::Stand,
            ..Default::default()
        };
        p.yaw = yaw;
        p.pitch = 0.0;
        p.flags = 0;
        p.slot = 0;
        p.weapons = [
            WState {
                id: l.primary,
                mag: tuning.weapon(l.primary).mag,
                reserve: l.reserve_for(l.primary, tuning),
            },
            WState {
                id: l.secondary,
                mag: tuning.weapon(l.secondary).mag,
                reserve: l.reserve_for(l.secondary, tuning),
            },
        ];
        p.grenades = l.grenades();
        p.smokes = 1;
        p.driving = None;
        p.reload_end = None;
        p.switch_end = time + 0.3;
        p.shot_budget = 1.0;
        p.attackers.clear();
        p.life = p.life.wrapping_add(1);
        p.protected_until = protected_until;
        p.history.clear();
        p.move_budget = 1.0;
        p.last_damage = -100.0;
        p.progress = 0.0;
        let life = p.life;
        if let Some(b) = p.bot.as_mut() {
            b.on_spawn();
        }
        self.send(
            id,
            S2C::Spawned {
                pos: pos.to_array(),
                yaw,
                life,
                loadout: l,
            },
        );
    }

    // ---------------------------------------------------------------- client messages

    pub fn handle(&mut self, id: u8, msg: C2S) {
        match msg {
            C2S::Hello { .. } => {}
            C2S::JoinTeam(choice) => self.on_join_team(id, choice),
            C2S::SetLoadout(l) => {
                let l = l.sanitized();
                if let Some(p) = self.pm(id) {
                    p.next_loadout = Some(l);
                    if !p.has_loadout {
                        p.has_loadout = true;
                        p.loadout = l;
                    }
                }
                if self.can_spawn(id)
                    && self
                        .p(id)
                        .map(|p| self.time >= p.respawn_at)
                        .unwrap_or(false)
                {
                    self.spawn_player(id, false);
                }
            }
            C2S::Spawn => {
                if self.can_spawn(id)
                    && self
                        .p(id)
                        .map(|p| self.time >= p.respawn_at)
                        .unwrap_or(false)
                {
                    self.spawn_player(id, false);
                }
            }
            C2S::Input(inp) => self.on_input(id, inp),
            C2S::Fire {
                weapon,
                origin,
                dirs,
            } => {
                let dirs: Vec<Vec3> = dirs.iter().map(|d| Vec3::from_array(*d)).collect();
                self.fire(id, weapon, Vec3::from_array(origin), &dirs);
            }
            C2S::Reload => self.start_reload(id),
            C2S::Switch(slot) => self.switch_weapon(id, slot),
            C2S::Grenade {
                origin,
                dir,
                cook,
                smoke,
            } => self.throw_grenade(
                id,
                Vec3::from_array(origin),
                Vec3::from_array(dir),
                cook,
                smoke,
            ),
            C2S::Melee => self.melee(id),
            C2S::PickUp(ent) => self.pick_up(id, ent),
            C2S::BrJump => self.br_jump(id),
            C2S::UseMedkit => self.br_use_medkit(id),
            C2S::UseBandage => self.br_use(id, toon_shared::inventory::USE_BANDAGE),
            C2S::UseDrink => self.br_use(id, toon_shared::inventory::USE_DRINK),
            C2S::Throw { origin, dir, kind } => {
                self.throw_special(id, Vec3::from_array(origin), Vec3::from_array(dir), kind)
            }
            C2S::Item(a) => self.br_item(id, a),
            C2S::Door(k) => {
                self.use_door(id, k, None);
            }
            C2S::Spectate(cmd) => match cmd {
                0 => self.br_spectate_step(id, 1),
                1 => self.br_spectate_step(id, -1),
                2 | 3 => {
                    if let Some(p) = self.pm(id) {
                        p.spec_free = cmd == 2;
                        if p.spec_free {
                            p.spectating = None;
                        }
                    }
                }
                _ => {}
            },
            C2S::Box(cmd) => match cmd {
                toon_shared::protocol::BoxCmd::Open(b) => {
                    if let Some(p) = self.pm(id) {
                        p.open_box = Some(b);
                    }
                }
                toon_shared::protocol::BoxCmd::Take(b, k) => self.box_take(id, b, k as usize),
                toon_shared::protocol::BoxCmd::TakeAll(b) => self.box_take_all(id, b),
                toon_shared::protocol::BoxCmd::Close => {
                    if let Some(p) = self.pm(id) {
                        p.open_box = None;
                    }
                }
            },
            C2S::Car(cmd) => match cmd {
                toon_shared::protocol::CarCmd::Enter(c) => {
                    self.car_enter(id, c);
                    self.car_hint(id);
                }
                toon_shared::protocol::CarCmd::Exit => self.car_exit(id),
                toon_shared::protocol::CarCmd::Drive {
                    throttle,
                    steer,
                    brake,
                } => self.car_drive(id, throttle, steer, brake),
            },
            C2S::Marker(m) => {
                if let Some(p) = self.pm(id) {
                    p.marker = m.filter(|m| m[0].is_finite() && m[1].is_finite());
                }
            }
            C2S::Ready(r) => {
                if let Some(p) = self.pm(id) {
                    p.ready = r;
                }
            }
            C2S::SetSkin(s) => {
                if let Some(p) = self.pm(id) {
                    p.skin = s.min(toon_shared::skins::count() - 1);
                }
            }
            C2S::UseKillstreak { target, pilot } => self.use_killstreak(id, target, pilot),
            C2S::Drive {
                pos,
                yaw,
                pitch,
                keys,
            } => self.on_drive(id, Vec3::from_array(pos), yaw, pitch, keys),
            C2S::Chat { text, team_only } => self.on_chat(id, text, team_only),
            C2S::Rcon { password, cmd } => self.on_rcon(id, &password, &cmd),
            C2S::Pong(ms) => {
                let now_ms = (self.time * 1000.0) as u32;
                if let Some(p) = self.pm(id) {
                    let rtt = now_ms.wrapping_sub(ms) as f32 / 1000.0;
                    if rtt < 2.0 {
                        p.ping = p.ping * 0.7 + rtt * 0.3;
                    }
                }
            }
            C2S::DropProtection => {
                let time = self.time;
                if let Some(p) = self.pm(id) {
                    if p.alive && p.protected_until > time {
                        p.protected_until = 0.0;
                        // the weapon needs a moment to come up
                        p.switch_end = p.switch_end.max(time + 0.2);
                    }
                }
            }
            C2S::Disconnect => {}
            C2S::Lang(l) => {
                if let Some(p) = self.pm(id) {
                    p.lang = l;
                }
            }
        }
    }

    fn on_join_team(&mut self, id: u8, choice: Option<Team>) {
        let target = match choice {
            None => self.auto_team(id),
            // battle royale: any side means "play"
            Some(t) if t.is_playing() && self.mode.solo() => self.auto_team(id),
            Some(t) => t,
        };
        if target.is_playing() && self.cfg.auto_balance && choice.is_some() {
            let other = target.enemy();
            let count = |t: Team| {
                self.players
                    .iter()
                    .flatten()
                    .filter(|p| p.id != id && p.team == t)
                    .count()
            };
            let mine = count(target) + 1;
            let theirs = count(other);
            if mine > theirs + 1 {
                // try to swap a bot out instead of refusing
                if let Some(bot) = self
                    .players
                    .iter()
                    .flatten()
                    .find(|p| p.is_bot() && p.team == target)
                    .map(|p| p.id)
                {
                    self.set_team(bot, other, false);
                } else {
                    self.say(Dest::One(id), || S2C::Center {
                        text: t(
                            "Команда переповнена — автобаланс",
                            "Team is full — auto-balance",
                        )
                        .into(),
                        secs: 3.0,
                    });
                    let t = self.auto_team(id);
                    self.set_team(id, t, true);
                    return;
                }
            }
        }
        if self.p(id).map(|p| p.team) == Some(target) {
            self.send(id, S2C::TeamSet(target));
            return;
        }
        self.set_team(id, target, true);
    }

    fn on_input(&mut self, id: u8, inp: InputState) {
        let frozen = self.state == MatchState::PreRound || self.state == MatchState::PostMatch;
        // in a car the car moves you: only where you look counts
        let seated = !self.cars.is_empty() && self.seat_of(id).is_some();
        let world = &self.world;
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        if inp.seq <= p.last_seq && p.last_seq - inp.seq < 1_000_000 {
            return;
        }
        p.last_seq = inp.seq;
        // a broken client must not pass NaN or infinite angles on to everybody else
        if inp.yaw.is_finite() {
            p.yaw = toon_shared::wrap_angle(inp.yaw);
        }
        if inp.pitch.is_finite() {
            p.pitch = inp.pitch.clamp(-1.55, 1.55);
        }
        if !p.alive || inp.life != p.life {
            return;
        }
        if p.fall_life != p.life {
            // a new life starts on its feet, wherever it is
            p.fall_life = p.life;
            p.support_y = f32::MAX;
            p.fall_top = None;
        }
        p.flags = inp.flags & (flags::SPRINT | flags::ADS | flags::ON_GROUND | flags::USE);
        if p.flags & flags::SPRINT != 0 {
            p.last_sprint = self.time;
        }
        if seated {
            p.flags &= !flags::SPRINT;
            p.support_y = f32::MAX;
            p.fall_top = None;
            return;
        }
        let new = Vec3::from_array(inp.pos);
        if !new.is_finite() {
            return;
        }
        let stance = Stance::from_u8(inp.stance);
        let old = p.body.pos;
        let dh = vec3(new.x - old.x, 0.0, new.z - old.z).length();
        let dy = (new.y - old.y).abs();
        let skydiving = p.body.chute != toon_shared::physics::Chute::None;
        let mut ok = dh <= p.move_budget + 0.25 && (dy <= 4.0 || skydiving && dy <= 8.0);
        if frozen && dh > 0.05 {
            ok = false;
        }
        // standing on something (a floor, the ground, a ladder, the water)?
        let supported = toon_shared::physics::in_deep_water(world, new)
            || world.ladder_at(new).is_some()
            || world.ground(new.x, new.z).is_some_and(|g| new.y - g < 0.6)
            || world.floor_below(new + vec3(0.0, 0.1, 0.0), 0.7).is_some();
        // nobody rises higher than a jump above where they last stood
        if ok && !skydiving && !supported && new.y > p.support_y + MAX_AIR_RISE {
            ok = false;
        }
        let mut fell = 0.0;
        if ok {
            let mut bb = body_box(new, stance.height());
            bb.min += Vec3::splat(0.03);
            bb.max -= Vec3::splat(0.03);
            if world.overlaps(&bb) {
                ok = false;
            }
        }
        if ok {
            p.move_budget = (p.move_budget - dh).max(0.0);
            p.body.pos = new;
            p.body.vel = Vec3::from_array(inp.vel);
            p.body.stance = stance;
            p.body.on_ground = inp.flags & flags::ON_GROUND != 0;
            // falls: from the highest point since the feet left the ground
            if skydiving {
                p.fall_top = None;
                p.support_y = f32::MAX;
            } else if supported {
                let soft = toon_shared::physics::in_deep_water(world, new)
                    || world.ladder_at(new).is_some();
                if let Some(top) = p.fall_top.take().filter(|_| !soft) {
                    fell = top - new.y;
                }
                p.support_y = new.y;
            } else {
                let from = if p.support_y == f32::MAX {
                    new.y
                } else {
                    p.support_y
                };
                p.fall_top = Some(p.fall_top.unwrap_or(from).max(new.y));
            }
            if p.body.on_ground {
                p.body.chute = toon_shared::physics::Chute::None;
            } else if skydiving && p.body.vel.y > -12.0 {
                // sinking slowly: the canopy is open
                p.body.chute = toon_shared::physics::Chute::Open;
            }
        } else {
            let life = p.life;
            let pos = p.body.pos.to_array();
            self.outbox
                .push((Dest::One(id), S2C::Correction { pos, life }, true));
        }
        if fell > SAFE_FALL {
            let at = Vec3::from_array(inp.pos);
            self.damage(
                id,
                None,
                (fell - SAFE_FALL) * FALL_DAMAGE,
                Cause::Fall,
                false,
                at,
            );
        }
    }

    fn on_chat(&mut self, id: u8, text: String, team_only: bool) {
        let text: String = text.chars().filter(|c| !c.is_control()).take(120).collect();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let Some(p) = self.pm(id) else { return };
        if p.chat_budget < 1.0 {
            return;
        }
        p.chat_budget -= 1.0;
        let (from, team) = (p.name.clone(), p.team);
        self.logf(format!(
            "{}{from}: {text}",
            if team_only { "(команда) " } else { "" }
        ));
        let msg = S2C::Chat {
            from,
            team,
            text,
            team_only,
        };
        if team_only {
            self.send_team(team, msg);
        } else {
            self.broadcast(msg);
        }
    }

    fn on_rcon(&mut self, id: u8, password: &str, cmd: &str) {
        let ok = {
            let Some(p) = self.p(id) else { return };
            !self.cfg.rcon_password.is_empty()
                && p.rcon_fails < 5
                && password == self.cfg.rcon_password
        };
        if !ok {
            if let Some(p) = self.pm(id) {
                p.rcon_fails += 1;
            }
            self.say(Dest::One(id), || {
                S2C::RconReply(
                    t(
                        "RCON: невірний пароль або адміністрування вимкнено",
                        "RCON: wrong password, or administration is off",
                    )
                    .into(),
                )
            });
            return;
        }
        let name = self.p(id).map(|p| p.name.clone()).unwrap_or_default();
        self.logf(format!("RCON від {name}: {cmd}"));
        let reply = crate::admin::exec(self, cmd);
        self.send(id, S2C::RconReply(reply));
    }

    // ---------------------------------------------------------------- match flow

    fn ready_to_start(&self) -> bool {
        let playing = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.team.is_playing())
            .count();
        playing >= self.cfg.min_players
            && (self.mode.solo()
                || self.team_count(Team::Allies) > 0 && self.team_count(Team::Axis) > 0)
            && self.players.iter().flatten().any(|p| !p.is_bot())
            && (!self.cfg.ready_check || self.unready() == 0)
    }

    /// On the same side: the same team, or in the battle royale the same squad.
    pub fn allied(&self, a: &Player, b: &Player) -> bool {
        if self.mode.solo() {
            a.squad != 0 && a.squad == b.squad
        } else {
            a.team == b.team
        }
    }

    /// `allied` by ids (false for anybody missing).
    pub fn allied_ids(&self, a: u8, b: u8) -> bool {
        match (self.p(a), self.p(b)) {
            (Some(a), Some(b)) => self.allied(a, b),
            _ => false,
        }
    }

    /// Real players in the game who have not pressed «Ready» yet.
    pub fn unready(&self) -> usize {
        self.players
            .iter()
            .flatten()
            .filter(|p| !p.is_bot() && p.team.is_playing() && !p.ready)
            .count()
    }

    pub fn start_match(&mut self) {
        self.scores = [0, 0];
        self.round = 0;
        self.match_started = true;
        for p in self.players.iter_mut().flatten() {
            p.kills = 0;
            p.deaths = 0;
            p.assists = 0;
            p.score = 0;
            p.streak = 0;
            p.rewards.clear();
        }
        self.start_round(PREROUND_FIRST);
    }

    fn start_round(&mut self, pre: f64) {
        self.round += 1;
        self.state = MatchState::PreRound;
        self.state_end = self.time + pre;
        self.overtime = false;
        self.grenades.clear();
        self.fires.clear();
        self.bullets.clear();
        self.lobby_spots = None;
        self.smokes.clear();
        self.vehicles.clear();
        self.cars.clear();
        self.drops.clear();
        for p in self.players.iter_mut().flatten() {
            p.driving = None;
        }
        self.helis.clear();
        self.pickups.clear();
        self.blasts.clear();
        self.uav_until = [0.0; 2];
        self.reset_bomb();
        for id in self.ids() {
            if let Some(p) = self.pm(id) {
                p.alive = false;
                p.respawn_at = 0.0;
                p.progress = 0.0;
            }
            if self.can_spawn(id) {
                self.spawn_player(id, true);
            }
        }
        let (mode, map, round) = (self.mode, self.map.id, self.round);
        self.center_all(
            || match mode {
                Mode::Tdm | Mode::Br => format!("{} — {}", mode.name(), map.name()),
                Mode::Sab => tr!("{} — раунд {round}", "{} — round {round}", mode.name()),
            },
            pre as f32,
        );
    }

    pub fn reset_bomb(&mut self) {
        self.sab.pos = self.map.bomb;
        self.sab.carrier = None;
        self.sab.owner = None;
        self.sab.planted_at = None;
        self.sab.plant = None;
        self.sab.defuse = None;
        self.sab.dropped_since = self.time;
    }

    /// Ends the match with a free-form text (the battle royale's winner).
    pub fn end_match_text(&mut self, text: impl Fn() -> String) {
        if self.state == MatchState::PostMatch {
            return;
        }
        self.state = MatchState::PostMatch;
        self.state_end = self.time + POSTMATCH_TIME;
        self.logf(format!("Матч завершено: {}", text()));
        self.say(Dest::All, || S2C::MatchEnd {
            winner: None,
            text: text(),
        });
    }

    pub fn end_match(&mut self, winner: Option<Team>, reason: Text) {
        if self.state == MatchState::PostMatch {
            return;
        }
        self.state = MatchState::PostMatch;
        self.state_end = self.time + POSTMATCH_TIME;
        let text = || match winner {
            Some(t) => tr!("{} перемагають! {}", "{} win! {}", t.name(), pick(reason)),
            None => tr!("Нічия! {}", "Draw! {}", pick(reason)),
        };
        self.logf(format!(
            "Матч завершено: {} ({}:{})",
            text(),
            self.scores[0],
            self.scores[1]
        ));
        self.say(Dest::All, || S2C::MatchEnd {
            winner,
            text: text(),
        });
    }

    pub fn round_win(&mut self, winner: Option<Team>, reason: Text) {
        if self.state != MatchState::Playing {
            return;
        }
        if let Some(t) = winner {
            self.scores[t.index()] += 1;
        }
        let text = || match winner {
            Some(t) => tr!(
                "{} виграли раунд — {}",
                "{} win the round — {}",
                t.name(),
                pick(reason)
            ),
            None => tr!("Раунд нічийний — {}", "Round drawn — {}", pick(reason)),
        };
        self.logf(text());
        if let Some(t) = winner {
            if self.scores[t.index()] >= self.cfg.sab.rounds_to_win as i32 {
                self.end_match(Some(t), reason);
                return;
            }
        }
        self.state = MatchState::RoundEnd;
        self.state_end = self.time + ROUND_END_TIME;
        self.say(Dest::All, || S2C::RoundEnd {
            winner,
            text: text(),
        });
    }

    /// Switches map/mode immediately (admin or rotation).
    pub fn change_map(&mut self, map: MapId, mode: Mode) {
        // building a big map (and its routes) takes seconds: the same map again keeps what
        // is built, and the one before is kept aside for a rotation that goes back to it
        if self.map.id != map || self.map.boxes.is_empty() {
            let built = match self.spare_map.take() {
                Some(b) if b.0.id == map => b,
                _ => {
                    let m = maps::build(map);
                    let w = World::from_map(&m);
                    let n = Nav::build(&m, &w);
                    (m, w, n)
                }
            };
            let old_map = std::mem::replace(&mut self.map, built.0);
            let old_world = std::mem::replace(&mut self.world, built.1);
            let old_nav = std::mem::replace(&mut self.nav, built.2);
            self.spare_map = Some((old_map, old_world, old_nav));
        }
        self.mode = mode;
        self.state = MatchState::Warmup;
        self.match_started = false;
        self.scores = [0, 0];
        self.overtime = false;
        self.grenades.clear();
        self.smokes.clear();
        self.vehicles.clear();
        self.drops.clear();
        for p in self.players.iter_mut().flatten() {
            p.driving = None;
        }
        self.helis.clear();
        self.pickups.clear();
        self.blasts.clear();
        self.reset_bomb();
        self.br = Default::default();
        self.intel.clear();
        for p in self.players.iter_mut().flatten() {
            p.in_plane = false;
            p.alive = false;
            p.respawn_at = 0.0;
            p.kills = 0;
            p.deaths = 0;
            p.assists = 0;
            p.score = 0;
            p.streak = 0;
            p.rewards.clear();
            if let Some(b) = p.bot.as_mut() {
                b.on_spawn();
            }
        }
        let settings = self.settings();
        // (a new map: the clients forget what lay on the old one)
        for p in self.players.iter_mut().flatten() {
            p.lying.clear();
        }
        self.broadcast(S2C::MapChange {
            map,
            mode,
            settings,
        });
        self.logf(format!("Зміна карти: {} ({})", self.map.name, mode.name()));
        self.shut_doors();
        for id in self.ids() {
            if self.can_spawn(id) {
                self.spawn_player(id, true);
            }
        }
    }

    pub fn next_map(&mut self) {
        self.rotation_idx = (self.rotation_idx + 1) % self.cfg.rotation.len();
        let (m, mode) = self.cfg.rotation[self.rotation_idx].resolve();
        self.change_map(m, mode);
    }

    fn update_state(&mut self, dt: f32) {
        match self.state {
            MatchState::Warmup => {
                if self.ready_to_start() {
                    self.start_match();
                }
            }
            MatchState::PreRound => {
                if self.time >= self.state_end {
                    self.state = MatchState::Playing;
                    let limit = match self.mode {
                        Mode::Tdm => self.cfg.tdm.time_limit,
                        Mode::Sab => self.cfg.sab.time_limit,
                        // the zone decides when it ends
                        Mode::Br => 1.0e6,
                    };
                    self.round_end = self.time + limit as f64 * 60.0;
                    if self.mode == Mode::Br {
                        self.br_start();
                    } else {
                        self.center_all(|| t("В БІЙ!", "FIGHT!").into(), 1.5);
                    }
                }
            }
            MatchState::Playing => {
                if self.mode == Mode::Sab && self.sab.planted_at.is_some() {
                    // the round clock stops while the bomb ticks
                    self.round_end += dt as f64;
                }
                if self.time >= self.round_end && !self.overtime {
                    match self.mode {
                        Mode::Br => {}
                        Mode::Tdm => {
                            let w = if self.scores[0] > self.scores[1] {
                                Some(Team::Allies)
                            } else if self.scores[1] > self.scores[0] {
                                Some(Team::Axis)
                            } else {
                                None
                            };
                            self.end_match(w, ("Час вийшов", "Time is up"));
                        }
                        Mode::Sab => {
                            self.overtime = true;
                            self.sab.overtime_end =
                                self.time + self.cfg.sab.overtime_limit as f64 * 60.0;
                            self.center_all(
                                || {
                                    t("ОВЕРТАЙМ! Відродження вимкнено", "OVERTIME! No respawns")
                                        .into()
                                },
                                4.0,
                            );
                        }
                    }
                }
                if self.mode == Mode::Sab {
                    self.update_sab();
                }
                if self.mode == Mode::Br {
                    self.update_br(dt);
                }
            }
            MatchState::RoundEnd => {
                if self.time >= self.state_end {
                    self.start_round(PREROUND_NEXT);
                }
            }
            MatchState::PostMatch => {
                if self.time >= self.state_end {
                    self.next_map();
                }
            }
        }
    }

    pub fn time_left(&self) -> f32 {
        match self.state {
            MatchState::Playing => {
                if self.overtime {
                    0.0
                } else {
                    (self.round_end - self.time).max(0.0) as f32
                }
            }
            MatchState::Warmup => 0.0,
            _ => (self.state_end - self.time).max(0.0) as f32,
        }
    }

    // ---------------------------------------------------------------- tick

    pub fn tick(&mut self, dt: f32) {
        self.time += dt as f64;
        self.tick = self.tick.wrapping_add(1);
        self.path_budget = PATHS_PER_TICK;
        self.update_state(dt);
        self.update_bullets();
        self.update_pickups();

        // bots
        for id in self.ids() {
            if self.p(id).map(|p| p.is_bot()).unwrap_or(false) {
                self.update_bot(id, dt);
            }
        }

        let time = self.time;
        let mut fell = Vec::new();
        let mut auto_spawn = Vec::new();
        let tuning = &self.tuning;
        let max_speed = tuning.max_sprint_speed() * 1.2;
        let br_round = self.br.running;
        for p in self.players.iter_mut().flatten() {
            // gliding under a parachute is faster than running
            let speed = if p.body.chute != toon_shared::physics::Chute::None {
                20.0
            } else {
                max_speed
            };
            p.move_budget = (p.move_budget + speed * dt).min(3.0);
            p.chat_budget = (p.chat_budget + dt * 0.5).min(5.0);
            p.item_budget = (p.item_budget + dt * 6.0).min(12.0);
            if !p.alive {
                if p.team.is_playing() && p.has_loadout {
                    let wait = if p.is_bot() { 0.0 } else { 3.0 };
                    if time >= p.respawn_at + wait {
                        auto_spawn.push(p.id);
                    }
                }
                continue;
            }
            let def = tuning.weapon(p.weapon().id);
            p.shot_budget = (p.shot_budget + dt * def.rpm / 60.0).min(2.0);
            let mag_max = p.mag_cap(def.mag);
            if let Some(end) = p.reload_end {
                if time >= end {
                    let w = p.weapon_mut();
                    let take = mag_max.saturating_sub(w.mag).min(w.reserve);
                    w.mag += take;
                    w.reserve -= take;
                    p.reload_end = None;
                }
            }
            // no healing by itself in the battle royale (medkits do that)
            if time - p.last_damage > REGEN_DELAY as f64 && p.hp < MAX_HEALTH && !br_round {
                p.hp = (p.hp + REGEN_RATE * dt).min(MAX_HEALTH);
            }
            if p.body.pos.y < -15.0 {
                fell.push(p.id);
            }
            p.history.push_back(HistEntry {
                t: time,
                pos: p.body.pos,
                yaw: p.yaw,
                stance: p.body.stance,
            });
            while p.history.len() > 40 {
                p.history.pop_front();
            }
        }
        for id in fell {
            self.kill(id, None, toon_shared::weapons::Cause::Fall, false);
        }
        for id in auto_spawn {
            if self.can_spawn(id) {
                self.spawn_player(id, false);
            }
        }

        self.update_grenades(dt);
        self.update_fires();
        let now = self.time;
        self.smokes
            .retain(|s| ((now - s.2) as f32) < toon_shared::consts::SMOKE_TIME);
        self.update_blasts();
        self.update_helis(dt);
        self.update_vehicles(dt);
        self.update_cars(dt);
        self.bots_open_doors();
        self.sync_lying();

        if self.time >= self.next_housekeeping {
            self.next_housekeeping = self.time + 1.0;
            self.manage_bots();
            let rows = self.scoreboard();
            self.outbox.push((Dest::All, S2C::Scoreboard(rows), false));
            let ms = (self.time * 1000.0) as u32;
            self.outbox.push((Dest::All, S2C::Ping(ms), false));
        }
    }

    pub fn scoreboard(&self) -> Vec<ScoreRow> {
        self.players
            .iter()
            .flatten()
            .map(|p| ScoreRow {
                id: p.id,
                name: p.name.clone(),
                team: p.team,
                score: p.score,
                kills: p.kills,
                deaths: p.deaths,
                assists: p.assists,
                ping: if p.is_bot() {
                    0
                } else {
                    (p.ping * 1000.0) as u16
                },
                bot: p.is_bot(),
                alive: p.alive,
            })
            .collect()
    }

    /// What lies on the ground near a player (guns, supplies, death boxes): small things,
    /// a few pixels at most from further away.
    pub fn lying_near(&self, id: u8) -> Vec<EntSnap> {
        let focus = self.interest(id);
        let near = |q: Vec3| {
            focus.as_ref().is_none_or(|pts| {
                pts.iter().any(|f| {
                    vec3(q.x - f.x, 0.0, q.z - f.z).length_squared() < LOOT_RANGE * LOOT_RANGE
                })
            })
        };
        let mut out = Vec::new();
        for pk in &self.pickups {
            if near(pk.pos) {
                out.push(pk.snap());
            }
        }
        for l in &self.br.loot {
            if near(l.pos) {
                out.push(EntSnap::Loot {
                    id: l.id,
                    kind: l.kind,
                    amount: l.amount,
                    pos: l.pos.to_array(),
                });
            }
        }
        for b in &self.br.boxes {
            if near(b.pos) {
                out.push(EntSnap::DeathBox {
                    id: b.id,
                    pos: b.pos.to_array(),
                });
            }
        }
        out
    }

    /// Tells real players what appeared on the ground near them and what is gone, a few
    /// times a second (the things themselves are not in the snapshots).
    fn sync_lying(&mut self) {
        for id in 0..self.players.len() as u8 {
            if (self.tick + id as u32) % 3 != 0 || self.p(id).is_none_or(|p| p.is_bot()) {
                continue;
            }
            let now = self.lying_near(id);
            let Some(p) = self.pm(id) else { continue };
            let mut remove: Vec<u16> = Vec::new();
            p.lying.retain(|k, _| {
                let keep = now.iter().any(|e| e.id() == *k);
                if !keep {
                    remove.push(*k);
                }
                keep
            });
            let mut add: Vec<EntSnap> = Vec::new();
            for e in now {
                if p.lying.get(&e.id()) != Some(&e) {
                    p.lying.insert(e.id(), e.clone());
                    add.push(e);
                }
            }
            // (in pieces that fit a datagram)
            let mut first = true;
            for part in add.chunks(36) {
                let remove = if first {
                    std::mem::take(&mut remove)
                } else {
                    Vec::new()
                };
                first = false;
                self.outbox.push((
                    Dest::One(id),
                    S2C::Ents {
                        add: part.to_vec(),
                        remove,
                    },
                    true,
                ));
            }
            for part in remove.chunks(300) {
                self.outbox.push((
                    Dest::One(id),
                    S2C::Ents {
                        add: Vec::new(),
                        remove: part.to_vec(),
                    },
                    true,
                ));
            }
        }
    }

    /// Builds the snapshot for one recipient.
    /// Where a player looks from (their body and whatever they steer), on big maps; None
    /// sees everything (small maps, spectators).
    fn interest(&self, id: u8) -> Option<Vec<Vec3>> {
        self.map.terrain.as_ref()?;
        let p = self.p(id)?;
        if !p.team.is_playing() {
            return None;
        }
        // out of the battle royale and watching somebody: what is around them
        if !p.alive && self.mode == Mode::Br {
            if let Some(t) = p.spectating.and_then(|t| self.p(t)).filter(|t| t.alive) {
                return Some(vec![t.body.pos, p.body.pos]);
            }
        }
        let mut pts = vec![p.body.pos];
        pts.extend(
            self.vehicles
                .iter()
                .filter(|v| v.owner == id)
                .map(|v| v.pos),
        );
        Some(pts)
    }

    pub fn snapshot_for(&self, id: u8) -> Snapshot {
        let me = self.p(id);
        let my_team = me.map(|p| p.team).unwrap_or(Team::Spectator);
        let my_squad = me.map_or(0, |p| p.squad);
        let watched = me.filter(|p| !p.alive).and_then(|p| p.spectating);
        let uav = my_team.is_playing() && self.uav_until[my_team.index()] > self.time;
        // big maps: only what is near enough to matter (the fog hides the rest anyway)
        let focus = self.interest(id);
        let near = |q: Vec3, r: f32| {
            focus.as_ref().is_none_or(|pts| {
                pts.iter()
                    .any(|f| vec3(q.x - f.x, 0.0, q.z - f.z).length_squared() < r * r)
            })
        };
        let players = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.team.is_playing())
            .filter(|p| {
                p.id == id
                    || p.team == my_team && my_team.is_playing() && !self.mode.solo()
                    || my_squad != 0 && p.squad == my_squad
                    || watched == Some(p.id)
                    || near(p.body.pos, INTEREST_RANGE)
            })
            .map(|p| {
                let mut f = p.flags;
                if p.reload_end.is_some() {
                    f |= flags::RELOADING;
                }
                if self.mode == Mode::Sab && self.sab.carrier == Some(p.id) {
                    f |= flags::HAS_BOMB;
                }
                if self.time - p.last_shot < 0.15 {
                    f |= flags::FIRING;
                }
                if (p.team != my_team || self.mode.solo()) && p.id != id && p.alive {
                    let uav_vis = uav && !p.has_perk2(Perk2::UavJammer);
                    // gunfire shows on the radar (not in the battle royale: nobody has one)
                    let pinged = self.time - p.last_shot < RADAR_PING_TIME as f64
                        && !self.mode.solo();
                    if uav_vis || pinged {
                        f |= flags::ON_RADAR;
                    }
                }
                PlayerSnap {
                    id: p.id,
                    team: p.team,
                    alive: p.alive,
                    pos: p.body.pos.to_array(),
                    yaw: p.yaw,
                    pitch: p.pitch,
                    // bit 7: Dead Silence (no footstep sounds)
                    stance: p.body.stance.to_u8()
                        | if p.has_perk3(Perk3::DeadSilence) { stance_bits::SILENT } else { 0 }
                        // only teammates see the shield: it must not give away enemies
                        | if p.alive && p.team == my_team && !self.mode.solo() && p.protected_until > self.time { stance_bits::PROTECTED } else { 0 }
                        | if p.downed { stance_bits::DOWNED } else { 0 }
                        | match p.body.chute {
                            toon_shared::physics::Chute::None => 0,
                            toon_shared::physics::Chute::FreeFall => stance_bits::FREEFALL,
                            toon_shared::physics::Chute::Open => stance_bits::CHUTE,
                        },
                    weapon: p.weapon().id,
                    flags: f,
                    // characters are a battle royale thing: elsewhere sides wear uniforms
                    skin: if self.mode.solo() { p.skin } else { 0 },
                    squad: p.squad,
                }
            })
            .collect();
        let me_snap = match me {
            Some(p) => MeSnap {
                alive: p.alive,
                hp: p.hp,
                slot: p.slot,
                ammo: [
                    (p.weapons[0].mag, p.weapons[0].reserve),
                    (p.weapons[1].mag, p.weapons[1].reserve),
                ],
                weapons: [p.weapons[0].id, p.weapons[1].id],
                grenades: p.grenades,
                smokes: p.smokes,
                streak: p.streak as u16,
                rewards: p.rewards.clone(),
                respawn_in: (p.respawn_at - self.time).max(0.0) as f32,
                uav,
                ack_seq: p.last_seq,
                progress: p.progress,
                protected: (p.protected_until - self.time).max(0.0) as f32,
                vehicle: self.vehicle_hud(p.id),
                downed: p.downed,
                seat: self.seat_of(p.id).map(|(i, k)| (self.cars[i].id, k as u8)),
            },
            None => MeSnap::default(),
        };
        let mut ents = Vec::new();
        for g in &self.grenades {
            ents.push(EntSnap::Grenade {
                id: g.id,
                pos: g.pos.to_array(),
                smoke: g.smoke,
                kind: g.kind,
            });
        }
        for f in &self.fires {
            ents.push(EntSnap::Fire {
                id: f.id,
                pos: f.pos.to_array(),
                radius: f.radius,
            });
        }
        for v in &self.vehicles {
            ents.push(EntSnap::Vehicle {
                id: v.id,
                kind: v.kind,
                team: v.team,
                owner: v.owner,
                pos: v.pos.to_array(),
                yaw: v.yaw,
                pitch: v.pitch,
                auto: v.auto,
            });
        }
        for d in &self.drops {
            ents.push(EntSnap::Drop {
                id: d.id,
                pos: d.pos.to_array(),
            });
        }
        for &(id, pos, born) in &self.smokes {
            ents.push(EntSnap::Smoke {
                id,
                pos: pos.to_array(),
                age: (self.time - born) as f32,
            });
        }
        // (guns, supplies and boxes on the ground go out once: `sync_lying`)
        ents.extend(self.br_drop_snaps());
        ents.extend(self.car_snaps());
        for h in &self.helis {
            ents.push(EntSnap::Heli {
                id: h.id,
                pos: h.pos.to_array(),
                yaw: h.yaw,
                team: h.team,
            });
        }
        let bomb = if self.mode == Mode::Sab && self.state != MatchState::Warmup {
            Some(BombSnap {
                pos: self.sab.pos.to_array(),
                carrier: self.sab.carrier,
                planted_at: self.sab.planted_at,
                timer: if self.sab.planted_at.is_some() {
                    (self.sab.explode_at - self.time).max(0.0) as f32
                } else {
                    0.0
                },
                owner: self.sab.owner,
            })
        } else {
            None
        };
        if focus.is_some() {
            ents.retain(|e| match e {
                // aircraft are seen (and heard) from afar
                EntSnap::Heli { .. } | EntSnap::Plane { .. } => true,
                EntSnap::Grenade { pos, .. }
                | EntSnap::Smoke { pos, .. }
                | EntSnap::Vehicle { pos, .. }
                | EntSnap::Drop { pos, .. }
                | EntSnap::Car { pos, .. }
                | EntSnap::AirDrop { pos, .. }
                | EntSnap::Fire { pos, .. } => near(Vec3::from_array(*pos), INTEREST_RANGE),
                EntSnap::DeathBox { pos, .. }
                | EntSnap::Weapon { pos, .. }
                | EntSnap::Loot { pos, .. } => near(Vec3::from_array(*pos), LOOT_RANGE),
            });
        }
        // gunshots carry further than you can see
        let shots = self
            .shots
            .iter()
            .filter(|s| near(Vec3::from_array(s.from), INTEREST_RANGE + 150.0))
            .cloned()
            .collect();
        Snapshot {
            tick: self.tick,
            state: self.state,
            time_left: self.time_left(),
            scores: self.scores,
            overtime: self.overtime,
            players,
            me: me_snap,
            ents,
            bomb,
            shots,
            br: self.br_snap(id),
            ready: (self.cfg.ready_check && self.state == MatchState::Warmup).then(|| {
                let humans = self
                    .players
                    .iter()
                    .flatten()
                    .filter(|p| !p.is_bot() && p.team.is_playing())
                    .count();
                let me = self.p(id).is_some_and(|p| p.ready);
                ((humans - self.unready()) as u8, humans as u8, me)
            }),
        }
    }
}
