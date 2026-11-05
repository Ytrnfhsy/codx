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
use toon_shared::weapons::{Killstreak, Loadout, Perk2, Perk3, WeaponId, DEFAULT_CLASSES};
use toon_shared::{maps, tr, Mode, Team};

pub const PREROUND_FIRST: f64 = 8.0;
pub const PREROUND_NEXT: f64 = 5.0;
pub const ROUND_END_TIME: f64 = 6.0;
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
    pub last_seq: u32,
    pub move_budget: f32,
    pub history: VecDeque<HistEntry>,
    pub rcon: bool,
    pub rcon_fails: u32,
    pub progress: f32,
    pub joined_at: f64,
    pub chat_budget: f32,
}

impl Player {
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
            last_seq: 0,
            move_budget: 2.0,
            history: VecDeque::new(),
            rcon: false,
            rcon_fails: 0,
            progress: 0.0,
            joined_at: now,
            chat_budget: 5.0,
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

pub struct Grenade {
    pub id: u16,
    pub owner: u8,
    pub pos: Vec3,
    pub vel: Vec3,
    pub explode_at: f64,
    /// Pops a smoke cloud instead of exploding.
    pub smoke: bool,
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
    pub uav_until: [f64; 2],
    pub sab: Sab,
    pub rotation_idx: usize,
    pub outbox: Vec<(Dest, S2C, bool)>,
    pub shots: Vec<ShotFx>,
    pub log: VecDeque<String>,
    pub bans: crate::admin::Bans,
    /// Players to disconnect (id, reason) — handled by the network layer.
    pub kicked: Vec<(u8, String)>,
    pub next_housekeeping: f64,
    pub match_started: bool,
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
        let world = World::from_map(&map);
        let nav = Nav::build(&map, &world);
        let bans = crate::admin::Bans::load(&cfg.data_dir);
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
            drops: Vec::new(),
            recent_deaths: Vec::new(),
            recent_spawns: Vec::new(),
            smokes: Vec::new(),
            helis: Vec::new(),
            pickups: Vec::new(),
            blasts: Vec::new(),
            next_ent: 1,
            uav_until: [0.0; 2],
            sab,
            rotation_idx: 0,
            outbox: Vec::new(),
            shots: Vec::new(),
            log: VecDeque::new(),
            bans,
            kicked: Vec::new(),
            next_housekeeping: 0.0,
            match_started: false,
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
                Mode::Tdm => self.cfg.tdm.time_limit,
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
        self.next_ent = self.next_ent.wrapping_add(1).max(1);
        self.next_ent
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
        if self.humans() >= self.cfg.max_players {
            return Err(t("Сервер заповнений", "The server is full").into());
        }
        // make room by removing a bot if needed
        if self.players.iter().flatten().count() >= self.cfg.max_players {
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
        self.say(Dest::All, || S2C::Chat {
            from: String::new(),
            team: Team::Spectator,
            text: tr!("{name} приєднався до гри", "{name} joined the game"),
            team_only: false,
        });
    }

    pub fn remove_player(&mut self, id: u8, reason: Option<&str>) {
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
        if total >= self.cfg.max_players {
            return None;
        }
        let slot = self.players.iter().position(|p| p.is_none())?;
        let id = slot as u8;
        let base = crate::bots::BOT_NAMES[self.rng.below(crate::bots::BOT_NAMES.len())];
        let name = self.sanitize_name(&format!("[Бот] {base}"), None);
        let mut p = Player::new(id, name, None, self.time);
        p.bot = Some(Box::new(Bot::new(&mut self.rng)));
        let class = DEFAULT_CLASSES[self.rng.below(DEFAULT_CLASSES.len())].1;
        p.loadout = class;
        p.has_loadout = true;
        self.players[slot] = Some(p);
        let team = self.auto_team(id);
        self.set_team(id, team, false);
        Some(id)
    }

    pub fn auto_team(&self, id: u8) -> Team {
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
        let want = self.cfg.bots.saturating_sub(humans_playing);
        if bots.len() < want {
            // a few per second (big servers fill up quickly, without a burst of joins)
            for _ in 0..(want - bots.len()).min(8) {
                self.add_bot();
            }
        } else if bots.len() > want {
            // remove a bot from the bigger team
            let big = if self.team_count(Team::Allies) >= self.team_count(Team::Axis) {
                Team::Allies
            } else {
                Team::Axis
            };
            let victim = bots
                .iter()
                .copied()
                .find(|&b| self.p(b).map(|p| p.team) == Some(big))
                .unwrap_or(bots[0]);
            self.remove_player(victim, None);
        }
        // balance bots between teams
        let (a, b) = (self.team_count(Team::Allies), self.team_count(Team::Axis));
        if a.abs_diff(b) > 1 {
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
            MatchState::Playing => !(self.mode == Mode::Sab && self.overtime),
            _ => true,
        }
    }

    pub fn respawn_delay(&self) -> f32 {
        match self.mode {
            Mode::Tdm => self.cfg.tdm.respawn_delay,
            Mode::Sab => self.cfg.sab.respawn_delay,
        }
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
        let mut cands: Vec<Spawn> = if base && team.is_playing() {
            self.map.spawns[team.index()].clone()
        } else {
            self.map.all_spawns().copied().collect()
        };
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
                }
                if p.team == team {
                    min_friend = min_friend.min(d);
                } else {
                    min_enemy = min_enemy.min(d);
                    if d < 12.0 {
                        score -= 100.0;
                    }
                    if d < 55.0
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
        let world = &self.world;
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        if inp.seq <= p.last_seq && p.last_seq - inp.seq < 1_000_000 {
            return;
        }
        p.last_seq = inp.seq;
        p.yaw = inp.yaw;
        p.pitch = inp.pitch.clamp(-1.55, 1.55);
        if !p.alive || inp.life != p.life {
            return;
        }
        p.flags = inp.flags & (flags::SPRINT | flags::ADS | flags::ON_GROUND | flags::USE);
        if p.flags & flags::SPRINT != 0 {
            p.last_sprint = self.time;
        }
        let new = Vec3::from_array(inp.pos);
        if !new.is_finite() {
            return;
        }
        let stance = Stance::from_u8(inp.stance);
        let old = p.body.pos;
        let dh = vec3(new.x - old.x, 0.0, new.z - old.z).length();
        let dy = (new.y - old.y).abs();
        let mut ok = dh <= p.move_budget + 0.25 && dy <= 4.0;
        if frozen && dh > 0.05 {
            ok = false;
        }
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
        } else {
            let life = p.life;
            let pos = p.body.pos.to_array();
            self.outbox
                .push((Dest::One(id), S2C::Correction { pos, life }, true));
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
            && self.team_count(Team::Allies) > 0
            && self.team_count(Team::Axis) > 0
            && self.players.iter().flatten().any(|p| !p.is_bot())
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
        self.smokes.clear();
        self.vehicles.clear();
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
                Mode::Tdm => format!("{} — {}", mode.name(), map.name()),
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
        self.map = maps::build(map);
        self.world = World::from_map(&self.map);
        self.nav = Nav::build(&self.map, &self.world);
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
        for p in self.players.iter_mut().flatten() {
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
        self.broadcast(S2C::MapChange {
            map,
            mode,
            settings,
        });
        self.logf(format!("Зміна карти: {} ({})", self.map.name, mode.name()));
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
                    };
                    self.round_end = self.time + limit as f64 * 60.0;
                    self.center_all(|| t("В БІЙ!", "FIGHT!").into(), 1.5);
                }
            }
            MatchState::Playing => {
                if self.mode == Mode::Sab && self.sab.planted_at.is_some() {
                    // the round clock stops while the bomb ticks
                    self.round_end += dt as f64;
                }
                if self.time >= self.round_end && !self.overtime {
                    match self.mode {
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
        self.update_state(dt);
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
        for p in self.players.iter_mut().flatten() {
            p.move_budget = (p.move_budget + max_speed * dt).min(3.0);
            p.chat_budget = (p.chat_budget + dt * 0.5).min(5.0);
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
            let mag_max = def.mag;
            if let Some(end) = p.reload_end {
                if time >= end {
                    let w = p.weapon_mut();
                    let take = mag_max.saturating_sub(w.mag).min(w.reserve);
                    w.mag += take;
                    w.reserve -= take;
                    p.reload_end = None;
                }
            }
            if time - p.last_damage > REGEN_DELAY as f64 && p.hp < MAX_HEALTH {
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
        let now = self.time;
        self.smokes
            .retain(|s| ((now - s.2) as f32) < toon_shared::consts::SMOKE_TIME);
        self.update_blasts();
        self.update_helis(dt);
        self.update_vehicles(dt);

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

    /// Builds the snapshot for one recipient.
    /// Where a player looks from (their body and whatever they steer), on big maps; None
    /// sees everything (small maps, spectators).
    fn interest(&self, id: u8) -> Option<Vec<Vec3>> {
        self.map.terrain.as_ref()?;
        let p = self.p(id)?;
        if !p.team.is_playing() {
            return None;
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
            .filter(|p| p.id == id || p.team == my_team && my_team.is_playing() || near(p.body.pos, INTEREST_RANGE))
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
                if p.team != my_team && p.alive {
                    let uav_vis = uav && !p.has_perk2(Perk2::UavJammer);
                    if uav_vis || self.time - p.last_shot < RADAR_PING_TIME as f64 {
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
                        | if p.alive && p.team == my_team && p.protected_until > self.time { stance_bits::PROTECTED } else { 0 },
                    weapon: p.weapon().id,
                    flags: f,
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
            },
            None => MeSnap::default(),
        };
        let mut ents = Vec::new();
        for g in &self.grenades {
            ents.push(EntSnap::Grenade {
                id: g.id,
                pos: g.pos.to_array(),
                smoke: g.smoke,
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
        for pk in &self.pickups {
            ents.push(pk.snap());
        }
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
                | EntSnap::Weapon { pos, .. } => near(Vec3::from_array(*pos), INTEREST_RANGE),
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
        }
    }
}
