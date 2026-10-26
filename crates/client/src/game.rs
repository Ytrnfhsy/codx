//! In-game client: networking, prediction, weapons and interpolation of other players.

use crate::audio::{Audio, Sfx};
use crate::effects::Effects;
use crate::mesh::{MeshBuilder, ToonMesh};
use crate::netclient::NetClient;
use crate::settings::Settings;
use macroquad::prelude::*;
use std::collections::{HashMap, VecDeque};
use toon_shared::consts::*;
use toon_shared::map::{Map, MapId};
use toon_shared::physics::{Body, MoveInput, Stance, World};
use toon_shared::protocol::*;
use toon_shared::ray::ray_player;
use toon_shared::rng::Rng;
use toon_shared::weapons::{Cause, Killstreak, Loadout, Perk2, Perk3, WeaponClass, WeaponId, DEFAULT_CLASSES};
use toon_shared::{look_dir, maps, wrap_angle, yaw_basis, Mode, Team};

pub struct Remote {
    pub id: u8,
    pub buf: VecDeque<(f64, PlayerSnap)>,
    pub team: Team,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub stance: Stance,
    pub weapon: WeaponId,
    pub flags: u8,
    pub silent: bool,
    pub alive: bool,
    pub phase: f32,
    pub walk: f32,
    pub death_time: Option<f64>,
    pub muzzle: Vec3,
    pub melee_t: f64,
    pub step_acc: f32,
}

pub struct Line {
    pub text: String,
    pub color: Color,
    pub time: f64,
}

pub struct KillLine {
    pub killer: String,
    pub victim: String,
    pub kteam: Team,
    pub vteam: Team,
    pub cause: Cause,
    pub headshot: bool,
    pub time: f64,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Overlay {
    None,
    Team,
    Class,
    Edit(usize),
    Pause,
    Airstrike,
    Settings,
}

pub struct Welcome {
    pub id: u8,
    pub server_name: String,
    pub motd: String,
    pub map: MapId,
    pub mode: Mode,
    pub settings: GameSettings,
}

pub struct GameClient {
    pub net: NetClient,
    pub address: String,
    pub my_id: u8,
    pub server_name: String,
    pub motd: String,
    pub map: Map,
    pub world: World,
    pub map_mesh: ToonMesh,
    pub sky_mesh: MeshBuilder,
    pub mode: Mode,
    pub gs: GameSettings,

    pub state: MatchState,
    pub time_left: f32,
    pub scores: [i32; 2],
    pub overtime: bool,
    pub me: MeSnap,
    pub bomb: Option<BombSnap>,
    pub ents: Vec<EntSnap>,
    pub scoreboard: Vec<ScoreRow>,
    latest_tick: u32,
    clock_offset: f64,
    have_clock: bool,
    pub remotes: HashMap<u8, Remote>,
    pub team: Team,
    pub has_class: bool,

    // local player
    pub body: Body,
    pub yaw: f32,
    pub pitch: f32,
    pub alive: bool,
    pub life: u16,
    pub loadout: Loadout,
    seq: u32,
    pub want_stance: Stance,
    pub weapons: [(WeaponId, u16, u16); 2],
    pub slot: u8,
    next_fire: f64,
    pub reload_until: Option<(f64, f64)>,
    pub switch_until: f64,
    pub ads_t: f32,
    spread_bonus: f32,
    pub last_fire: f64,
    pub stamina: f32,
    pub sprinting: bool,
    pub grenades: u8,
    pub cooking: Option<f64>,
    pub melee_until: f64,
    pub melee_ready: f64,
    pub throw_until: f64,
    pub recoil_vis: f32,
    pub bob_phase: f32,
    pub walk_amount: f32,
    pub sway: Vec2,
    step_acc: f32,
    pub breath: f32,
    pub using: bool,
    last_throw: f64,
    trigger_was_down: bool,

    // feedback
    pub hitmarker: f64,
    pub hit_kill: bool,
    pub dmg_dirs: Vec<(Vec3, f64)>,
    pub dmg_flash: f64,
    pub center: Option<(String, f64, Color)>,
    pub popups: Vec<(String, f64)>,
    pub killfeed: Vec<KillLine>,
    pub chat: Vec<Line>,
    pub chat_input: Option<(String, bool)>,
    pub death_info: Option<(String, Cause, Option<u8>)>,
    pub death_pos: Vec3,
    pub death_time: f64,
    pub match_end: Option<(String, Option<Team>, f64)>,
    pub overlay: Overlay,
    pub airstrikes: Vec<(Vec3, Vec3, f64)>,
    pub fx: Effects,
    pub rcon_pass: String,
    pub spec_pos: Vec3,
    last_input_send: f64,
    pub shake: f32,
    pub frame_mesh: ToonMesh,
    pub fx_mesh: MeshBuilder,
    pub leave: Option<String>,
    pub rng: Rng,
    pub now: f64,
    pub last_grenades: HashMap<u16, Vec3>,
    pub prev_state: MatchState,
    pub mouse_delta: Vec2,
    pub class_choice: usize,
    pub streak_hint: Option<Killstreak>,
    pub bomb_beep: f64,
    pub muzzle_flash: f64,
}

pub fn build_map_mesh(map: &Map) -> ToonMesh {
    let mut m = ToonMesh::new();
    let mut rng = Rng::new(map.id as u64 + 3);
    for b in &map.boxes {
        if !b.visible {
            continue;
        }
        // tiny colour variation per box for a hand-painted feel
        let j = rng.range(-10.0, 10.0);
        let c = [
            (b.color[0] as f32 + j).clamp(0.0, 255.0) as u8,
            (b.color[1] as f32 + j).clamp(0.0, 255.0) as u8,
            (b.color[2] as f32 + j * 0.8).clamp(0.0, 255.0) as u8,
        ];
        m.fill.add_aabb(b.min, b.max, c);
        let size = b.max - b.min;
        let is_ground = b.min.y < -0.5;
        if b.solid && !is_ground && size.min_element() > 0.15 {
            let w = Vec3::splat(0.045);
            m.outline.add_box_ex(&Mat4::from_translation((b.min + b.max) * 0.5), size * 0.5 + w, crate::mesh::OUTLINE, 255, true);
        }
    }
    m
}

pub fn build_sky(map: &Map) -> MeshBuilder {
    let mut sky = MeshBuilder::new();
    let top = [map.sky[0], map.sky[1], map.sky[2]];
    let hor = [map.fog[0], map.fog[1], map.fog[2]];
    // gradient dome
    let seg = 16;
    let rings = 8;
    let r = 380.0;
    let mut verts = Vec::new();
    for j in 0..=rings {
        let phi = j as f32 / rings as f32 * std::f32::consts::FRAC_PI_2 * 1.15;
        for i in 0..=seg {
            let th = i as f32 / seg as f32 * std::f32::consts::TAU;
            let n = vec3(phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin());
            let k = (1.0 - n.y.max(0.0)).powf(2.0);
            let c = [
                (top[0] as f32 + (hor[0] as f32 - top[0] as f32) * k) as u8,
                (top[1] as f32 + (hor[1] as f32 - top[1] as f32) * k) as u8,
                (top[2] as f32 + (hor[2] as f32 - top[2] as f32) * k) as u8,
                255,
            ];
            verts.push((n * r, c));
        }
    }
    for j in 0..rings {
        for i in 0..seg {
            let a = j * (seg + 1) + i;
            let b = a + seg + 1;
            for tri in [[a, b, a + 1], [a + 1, b, b + 1]] {
                let p = [verts[tri[0]], verts[tri[1]], verts[tri[2]]];
                sky.add_quad([p[0].0, p[1].0, p[2].0, p[2].0], p[0].1);
                // per-vertex colours: patch the last quad's colours
                let ch = sky.chunks.last_mut().unwrap();
                let n = ch.verts.len();
                ch.verts[n - 4].color = p[0].1;
                ch.verts[n - 3].color = p[1].1;
                ch.verts[n - 2].color = p[2].1;
                ch.verts[n - 1].color = p[2].1;
            }
        }
    }
    // puffy clouds
    let mut rng = Rng::new(map.id as u64 + 11);
    for _ in 0..14 {
        let a = rng.range(0.0, std::f32::consts::TAU);
        let d = rng.range(120.0, 260.0);
        let base = vec3(a.cos() * d, rng.range(60.0, 110.0), a.sin() * d);
        for k in 0..4 {
            let off = vec3(k as f32 * 9.0 - 13.0, rng.range(-2.0, 3.0), rng.range(-4.0, 4.0));
            let s = rng.range(7.0, 12.0);
            let m = Mat4::from_translation(base + off) * Mat4::from_scale(vec3(1.3, 0.75, 1.0));
            sky.add_sphere_ex(&m, s, [255, 255, 255], 10, 6, 255, false);
        }
    }
    sky
}

impl GameClient {
    pub fn new(net: NetClient, address: String, w: Welcome, settings: &Settings, pending: Vec<S2C>) -> GameClient {
        let map = maps::build(w.map);
        let world = World::from_map(&map);
        let map_mesh = build_map_mesh(&map);
        let sky_mesh = build_sky(&map);
        let def = DEFAULT_CLASSES[0].1;
        let spec = vec3(map.center().x, 15.0, map.max.y);
        let mut g = GameClient {
            net,
            address,
            my_id: w.id,
            server_name: w.server_name,
            motd: w.motd.clone(),
            map,
            world,
            map_mesh,
            sky_mesh,
            mode: w.mode,
            gs: w.settings,
            state: MatchState::Warmup,
            time_left: 0.0,
            scores: [0, 0],
            overtime: false,
            me: MeSnap::default(),
            bomb: None,
            ents: Vec::new(),
            scoreboard: Vec::new(),
            latest_tick: 0,
            clock_offset: 0.0,
            have_clock: false,
            remotes: HashMap::new(),
            team: Team::Spectator,
            has_class: false,
            body: Body { pos: spec, ..Default::default() },
            yaw: 0.0,
            pitch: -0.3,
            alive: false,
            life: 0,
            loadout: def,
            seq: 0,
            want_stance: Stance::Stand,
            weapons: [(def.primary, 0, 0), (def.secondary, 0, 0)],
            slot: 0,
            next_fire: 0.0,
            reload_until: None,
            switch_until: 0.0,
            ads_t: 0.0,
            spread_bonus: 0.0,
            last_fire: -10.0,
            stamina: SPRINT_TIME,
            sprinting: false,
            grenades: 0,
            cooking: None,
            melee_until: 0.0,
            melee_ready: 0.0,
            throw_until: 0.0,
            recoil_vis: 0.0,
            bob_phase: 0.0,
            walk_amount: 0.0,
            sway: Vec2::ZERO,
            step_acc: 0.0,
            breath: 4.0,
            using: false,
            last_throw: -10.0,
            trigger_was_down: false,
            hitmarker: -10.0,
            hit_kill: false,
            dmg_dirs: Vec::new(),
            dmg_flash: -10.0,
            center: None,
            popups: Vec::new(),
            killfeed: Vec::new(),
            chat: Vec::new(),
            chat_input: None,
            death_info: None,
            death_pos: spec,
            death_time: -10.0,
            match_end: None,
            overlay: Overlay::Team,
            airstrikes: Vec::new(),
            fx: Effects::new(),
            rcon_pass: String::new(),
            spec_pos: spec,
            last_input_send: 0.0,
            shake: 0.0,
            frame_mesh: ToonMesh::new(),
            fx_mesh: MeshBuilder::new(),
            leave: None,
            rng: Rng::from_time(),
            now: get_time(),
            last_grenades: HashMap::new(),
            prev_state: MatchState::Warmup,
            mouse_delta: Vec2::ZERO,
            class_choice: settings.last_class,
            streak_hint: None,
            bomb_beep: 0.0,
            muzzle_flash: -10.0,
        };
        if !w.motd.is_empty() {
            g.push_chat(format!("[{}] {}", g.server_name, w.motd), crate::ui::ACCENT);
        }
        g.push_chat("WASD, Shift — біг, C — присід, Ctrl — лежати, ПКМ — приціл, R — перезарядка".into(), crate::ui::MUTED);
        g.push_chat("G — граната, V — ніж, F — дія, 5 — кілстрік, T/Y — чат, Tab — рахунок, M — клас".into(), crate::ui::MUTED);
        let mut audio_dummy = None;
        for m in pending {
            g.handle_msg(m, &mut audio_dummy);
        }
        g
    }

    pub fn push_chat(&mut self, text: String, color: Color) {
        self.chat.push(Line { text, color, time: get_time() });
        if self.chat.len() > 60 {
            self.chat.remove(0);
        }
    }

    pub fn center_msg(&mut self, text: impl Into<String>, secs: f64, color: Color) {
        self.center = Some((text.into(), self.now + secs, color));
    }

    pub fn eye(&self) -> Vec3 {
        self.body.pos + vec3(0.0, self.body.stance.eye(), 0.0)
    }

    pub fn weapon(&self) -> WeaponId {
        self.weapons[self.slot as usize & 1].0
    }

    pub fn frozen(&self) -> bool {
        matches!(self.state, MatchState::PreRound | MatchState::PostMatch)
    }

    /// Server time we render remote players at.
    pub fn render_time(&self) -> f64 {
        self.now + self.clock_offset - INTERP_DELAY as f64
    }

    pub fn name_of(&self, id: u8) -> Option<&str> {
        self.scoreboard.iter().find(|r| r.id == id).map(|r| r.name.as_str())
    }

    fn load_map(&mut self, map: MapId, mode: Mode, gs: GameSettings) {
        self.map = maps::build(map);
        self.world = World::from_map(&self.map);
        self.map_mesh = build_map_mesh(&self.map);
        self.sky_mesh = build_sky(&self.map);
        self.mode = mode;
        self.gs = gs;
        self.remotes.clear();
        self.alive = false;
        self.match_end = None;
        self.bomb = None;
        self.ents.clear();
        self.airstrikes.clear();
        self.killfeed.clear();
        self.spec_pos = vec3(self.map.center().x, 15.0, self.map.max.y);
        self.body.pos = self.spec_pos;
    }

    // ------------------------------------------------------------------ messages

    pub fn handle_msg(&mut self, m: S2C, audio: &mut Option<&Audio>) {
        let now = get_time();
        match m {
            S2C::Welcome { .. } => {}
            S2C::Reject(reason) => self.leave = Some(reason),
            S2C::MapChange { map, mode, settings } => {
                self.load_map(map, mode, settings);
                let name = self.map.name;
                self.center_msg(format!("{name} — {}", mode.name()), 4.0, crate::ui::ACCENT);
            }
            S2C::Snapshot(s) => self.on_snapshot(*s, audio),
            S2C::Scoreboard(rows) => self.scoreboard = rows,
            S2C::Spawned { pos, yaw, life, loadout } => {
                self.body = Body { pos: Vec3::from_array(pos), vel: Vec3::ZERO, on_ground: true, stance: Stance::Stand };
                self.want_stance = Stance::Stand;
                self.yaw = yaw;
                self.pitch = 0.0;
                self.life = life;
                self.alive = true;
                self.loadout = loadout;
                self.weapons = [
                    (loadout.primary, loadout.primary.def().mag, loadout.reserve_for(loadout.primary)),
                    (loadout.secondary, loadout.secondary.def().mag, loadout.reserve_for(loadout.secondary)),
                ];
                self.slot = 0;
                self.grenades = loadout.grenades();
                self.reload_until = None;
                self.switch_until = now + 0.3;
                self.ads_t = 0.0;
                self.stamina = self.sprint_max();
                self.cooking = None;
                self.death_info = None;
                self.has_class = true;
                if matches!(self.overlay, Overlay::Class | Overlay::Team) {
                    self.overlay = Overlay::None;
                }
            }
            S2C::TeamSet(t) => {
                self.team = t;
                if t.is_playing() {
                    if !self.has_class {
                        self.overlay = Overlay::Class;
                    }
                } else {
                    self.alive = false;
                    if self.overlay == Overlay::Team {
                        self.overlay = Overlay::None;
                    }
                }
            }
            S2C::Correction { pos, life } => {
                if life == self.life {
                    self.body.pos = Vec3::from_array(pos);
                    self.body.vel = Vec3::ZERO;
                }
            }
            S2C::Kill { killer, victim, cause, headshot, killer_name, victim_name, killer_team, victim_team } => {
                if victim == self.my_id {
                    self.alive = false;
                    self.death_time = now;
                    self.death_pos = self.eye();
                    self.death_info = Some((killer_name.clone(), cause, killer.filter(|k| *k != victim)));
                    self.cooking = None;
                }
                if killer == Some(self.my_id) && victim != self.my_id {
                    if let Some(a) = audio {
                        a.play(Sfx::Kill, 0.6);
                    }
                }
                self.killfeed.push(KillLine { killer: killer_name, victim: victim_name, kteam: killer_team, vteam: victim_team, cause, headshot, time: now });
                if self.killfeed.len() > 6 {
                    self.killfeed.remove(0);
                }
            }
            S2C::HitConfirm { kill, headshot } => {
                self.hitmarker = now;
                self.hit_kill = kill || headshot;
                if let Some(a) = audio {
                    a.play(Sfx::Hit, 0.5);
                }
            }
            S2C::Damaged { from, amount } => {
                self.dmg_dirs.push((Vec3::from_array(from), now));
                self.dmg_flash = now;
                self.shake = (self.shake + amount * 0.01).min(0.6);
                if let Some(a) = audio {
                    a.play(Sfx::Hurt, 0.5);
                }
            }
            S2C::Explosion { pos, radius, big } => {
                let p = Vec3::from_array(pos);
                self.fx.explosion(p, radius);
                let d = p.distance(self.eye());
                self.shake = (self.shake + (1.0 - d / (radius * 4.0)).max(0.0) * if big { 1.0 } else { 0.6 }).min(1.2);
                if let Some(a) = audio {
                    a.play_at(Sfx::Explosion, p, self.eye(), 1.0, 120.0);
                }
            }
            S2C::Melee { id } => {
                if let Some(r) = self.remotes.get_mut(&id) {
                    r.melee_t = now;
                    if let Some(a) = audio {
                        a.play_at(Sfx::Knife, r.pos, self.body.pos, 0.8, 25.0);
                    }
                }
            }
            S2C::ScorePopup { points, text } => {
                self.popups.push((format!("+{points} {text}"), now));
            }
            S2C::Chat { from, team, text, team_only } => {
                let line = if from.is_empty() {
                    text
                } else if team_only {
                    format!("(команда) {from}: {text}")
                } else {
                    format!("{from}: {text}")
                };
                let color = if from.is_empty() { crate::ui::MUTED } else if from == "Адмін" { crate::ui::ACCENT } else { crate::models::team_color(team) };
                self.push_chat(line, color);
            }
            S2C::Center { text, secs } => self.center_msg(text, secs as f64, crate::ui::TEXT),
            S2C::StreakReady(k) => {
                self.center_msg(format!("{} готовий! Натисни 5", k.name()), 3.0, crate::ui::ACCENT);
                self.streak_hint = Some(k);
                if let Some(a) = audio {
                    a.play(Sfx::Streak, 0.7);
                }
            }
            S2C::StreakUsed { by, team, streak } => {
                let friendly = team == self.team;
                let text = if friendly {
                    format!("{by}: {} — дружній!", streak.name())
                } else {
                    format!("Ворожий {}!", streak.name().to_lowercase())
                };
                self.center_msg(text, 2.5, if friendly { crate::ui::GOOD } else { crate::ui::BAD });
            }
            S2C::Airstrike { from, to } => {
                self.airstrikes.push((Vec3::from_array(from), Vec3::from_array(to), now));
                if let Some(a) = audio {
                    a.play(Sfx::Jet, 0.9);
                }
            }
            S2C::MatchEnd { winner, text } => {
                self.match_end = Some((text, winner, now));
            }
            S2C::RoundEnd { winner, text } => {
                let c = match winner {
                    Some(t) if t == self.team => crate::ui::GOOD,
                    Some(_) => crate::ui::BAD,
                    None => crate::ui::TEXT,
                };
                self.center_msg(text, 5.0, c);
            }
            S2C::BombEvent { text } => {
                self.center_msg(text, 2.5, crate::ui::ACCENT);
                if let Some(a) = audio {
                    a.play(Sfx::Bomb, 0.6);
                }
            }
            S2C::RconReply(text) => {
                for l in text.lines() {
                    self.push_chat(format!("[rcon] {l}"), crate::ui::ACCENT);
                }
            }
            S2C::Ping(ms) => self.net.send_unreliable(C2S::Pong(ms)),
        }
    }

    fn sprint_max(&self) -> f32 {
        if self.loadout.perk3 == Perk3::ExtremeConditioning {
            SPRINT_TIME * 2.0
        } else {
            SPRINT_TIME
        }
    }

    fn on_snapshot(&mut self, s: Snapshot, audio: &mut Option<&Audio>) {
        if s.tick <= self.latest_tick && self.latest_tick - s.tick < 1000 {
            return;
        }
        self.latest_tick = s.tick;
        let st = s.tick as f64 * TICK_DT as f64;
        let target = st - self.now;
        if !self.have_clock || (target - self.clock_offset).abs() > 0.25 {
            self.clock_offset = target;
            self.have_clock = true;
        } else {
            self.clock_offset += (target - self.clock_offset) * 0.05;
        }
        if s.state != self.state {
            self.prev_state = self.state;
            if s.state == MatchState::Playing && self.state == MatchState::PreRound {
                self.match_end = None;
            }
        }
        self.state = s.state;
        self.time_left = s.time_left;
        self.scores = s.scores;
        self.overtime = s.overtime;

        let mut seen = Vec::new();
        for p in s.players {
            if p.id == self.my_id {
                self.team = p.team;
                continue;
            }
            seen.push(p.id);
            let r = self.remotes.entry(p.id).or_insert_with(|| Remote {
                id: p.id,
                buf: VecDeque::new(),
                team: p.team,
                pos: Vec3::from_array(p.pos),
                yaw: p.yaw,
                pitch: p.pitch,
                stance: Stance::from_u8(p.stance & 3),
                weapon: p.weapon,
                flags: p.flags,
                silent: false,
                alive: p.alive,
                phase: 0.0,
                walk: 0.0,
                death_time: None,
                muzzle: Vec3::from_array(p.pos),
                melee_t: -10.0,
                step_acc: 0.0,
            });
            r.buf.push_back((st, p));
            while r.buf.len() > 40 {
                r.buf.pop_front();
            }
        }
        self.remotes.retain(|id, _| seen.contains(id));

        // our own state
        let me = s.me;
        let now = self.now;
        if !me.alive && self.alive {
            self.alive = false;
            self.death_time = now;
            self.death_pos = self.eye();
        }
        let quiet = now - self.last_fire > 0.5 && self.reload_until.is_none() && now > self.switch_until + 0.3;
        if quiet && self.alive {
            for i in 0..2 {
                self.weapons[i].1 = me.ammo[i].0;
                self.weapons[i].2 = me.ammo[i].1;
            }
            if me.slot != self.slot {
                self.slot = me.slot;
            }
        }
        if now - self.last_throw > 0.6 {
            self.grenades = me.grenades;
        }
        self.me = me;
        self.bomb = s.bomb;
        self.ents = s.ents;

        // effects for other players' shots
        let eye = self.eye();
        for shot in s.shots {
            if shot.id == self.my_id {
                continue;
            }
            let to = Vec3::from_array(shot.to);
            let from = if shot.id == 255 {
                Vec3::from_array(shot.from)
            } else {
                self.remotes.get(&shot.id).map(|r| r.muzzle).unwrap_or(Vec3::from_array(shot.from))
            };
            self.fx.tracer(from, to);
            if shot.flesh {
                self.fx.flesh(to);
            } else {
                self.fx.impact(to);
            }
            if shot.id != 255 {
                self.fx.muzzle(from);
            }
            if let Some(a) = audio {
                let sfx = if shot.id == 255 { Sfx::Lmg } else { Sfx::for_weapon(shot.weapon) };
                a.play_at(sfx, from, eye, 0.7, 140.0);
            }
        }
    }

    // ------------------------------------------------------------------ simulation

    fn current_spread(&self) -> f32 {
        let def = self.weapon().def();
        let mut hip = def.hip_spread;
        if self.loadout.perk3 == Perk3::SteadyAim {
            hip *= 0.65;
        }
        hip *= match self.body.stance {
            Stance::Stand => 1.0,
            Stance::Crouch => 0.8,
            Stance::Prone => 0.65,
        };
        let base = hip + (def.ads_spread - hip) * self.ads_t;
        let speed = vec2(self.body.vel.x, self.body.vel.z).length();
        let mv = (speed / WALK_SPEED).min(1.3) * def.move_spread * (1.0 - self.ads_t * 0.8);
        let air = if self.body.on_ground { 0.0 } else { 3.0 };
        base + mv + air + self.spread_bonus
    }

    pub fn spread_px(&self, fovy: f32) -> f32 {
        let s = self.current_spread().to_radians();
        s.tan() / (fovy * 0.5).tan() * screen_height() * 0.5
    }

    pub fn scoped(&self) -> bool {
        self.weapon().def().scoped && self.ads_t > 0.85
    }

    /// Sniper scope sway (radians).
    pub fn sway_offset(&self) -> Vec2 {
        if !self.weapon().def().scoped || self.ads_t < 0.5 {
            return Vec2::ZERO;
        }
        let holding = is_key_down(KeyCode::LeftShift) && self.breath > 0.0;
        let k = if holding { 0.08 } else { 1.0 } * if self.body.stance == Stance::Prone { 0.4 } else { 1.0 };
        let t = self.now as f32;
        vec2((t * 0.9).sin() * 0.011 + (t * 2.3).sin() * 0.003, (t * 1.7).sin() * 0.008) * k
    }

    pub fn aim_dir(&self) -> Vec3 {
        let s = self.sway_offset();
        look_dir(self.yaw + s.x, self.pitch + s.y)
    }

    fn fire(&mut self, audio: &Option<&Audio>) {
        let now = self.now;
        let w = self.weapon();
        let def = w.def();
        self.next_fire = now + def.fire_interval() as f64;
        self.weapons[self.slot as usize].1 -= 1;
        self.last_fire = now;
        let spread = self.current_spread().to_radians();
        let eye = self.eye();
        let base = self.aim_dir();
        let dirs: Vec<Vec3> = (0..def.pellets.max(1)).map(|_| cone(&mut self.rng, base, spread)).collect();
        self.net.send(C2S::Fire { weapon: w, origin: eye.to_array(), dirs: dirs.iter().map(|d| d.to_array()).collect() });

        // local visuals: trace against the world and remote players
        let muzzle = self.viewmodel_muzzle_world();
        for (k, d) in dirs.iter().enumerate() {
            let mut t = self.world.raycast(eye, *d, 300.0).map(|h| h.0).unwrap_or(300.0);
            let mut flesh = false;
            for r in self.remotes.values() {
                if !r.alive || r.team == self.team {
                    continue;
                }
                if let Some((rt, _)) = ray_player(eye, *d, r.pos, r.yaw, r.stance, t) {
                    t = rt;
                    flesh = true;
                }
            }
            let hit = eye + *d * t;
            if k < 3 {
                self.fx.tracer(muzzle, hit);
                if flesh {
                    self.fx.flesh(hit);
                } else {
                    self.fx.impact(hit - *d * 0.05);
                }
            }
        }
        self.muzzle_flash = now;
        // recoil
        let ads_k = 1.0 - self.ads_t * 0.45;
        let crouch_k = match self.body.stance {
            Stance::Stand => 1.0,
            Stance::Crouch => 0.8,
            Stance::Prone => 0.6,
        };
        self.pitch += (def.recoil_v * 0.55 * ads_k * crouch_k * self.rng.range(0.8, 1.2)).to_radians();
        self.yaw += (def.recoil_h * 0.5 * ads_k * self.rng.range(-1.0, 1.0)).to_radians();
        self.spread_bonus = (self.spread_bonus + def.shot_spread).min(4.0);
        self.recoil_vis = (self.recoil_vis + 0.5 + def.heft * 0.5).min(1.4);
        self.shake = (self.shake + def.heft * 0.05).min(0.3);
        if let Some(a) = audio {
            a.play(Sfx::for_weapon(w), 0.85);
        }
    }

    pub fn viewmodel_muzzle_world(&self) -> Vec3 {
        let (f, r) = yaw_basis(self.yaw);
        let d = look_dir(self.yaw, self.pitch);
        let up = r.cross(d);
        let _ = f;
        let k = 1.0 - self.ads_t;
        self.eye() + d * 0.9 + r * (0.16 * k) - up * (0.12 * k + 0.04)
    }

    fn start_reload(&mut self) {
        let (w, mag, res) = self.weapons[self.slot as usize];
        let def = w.def();
        if self.reload_until.is_some() || mag >= def.mag || res == 0 {
            return;
        }
        let k = if self.loadout.perk2 == Perk2::SleightOfHand { 0.5 } else { 1.0 };
        let dur = (def.reload_time * k) as f64;
        self.reload_until = Some((self.now + dur, dur));
        self.net.send(C2S::Reload);
    }

    fn switch_to(&mut self, slot: u8) {
        if slot == self.slot || slot > 1 {
            return;
        }
        self.slot = slot;
        self.reload_until = None;
        self.switch_until = self.now + WEAPON_SWITCH_TIME as f64;
        self.ads_t = 0.0;
        self.net.send(C2S::Switch(slot));
    }

    /// Per-frame update. `input_enabled` is false while menus are open.
    pub fn update(&mut self, dt: f32, settings: &Settings, audio: Option<&Audio>) {
        self.now = get_time();
        let now = self.now;
        let mut audio_opt = audio;
        for m in self.net.poll() {
            self.handle_msg(m, &mut audio_opt);
        }
        if self.net.last_recv.elapsed().as_secs_f32() > 10.0 {
            self.leave = Some("З'єднання з сервером втрачено".into());
        }

        let playing_input = self.overlay == Overlay::None && self.chat_input.is_none();
        // ---- mouse look
        let md = self.mouse_delta;
        if playing_input {
            let zoom = 1.0 + (self.weapon().def().ads_zoom - 1.0) * self.ads_t;
            let sens = settings.sensitivity * 0.0011 / zoom;
            self.yaw -= md.x * sens;
            let dy = md.y * sens * if settings.invert_y { -1.0 } else { 1.0 };
            self.pitch = (self.pitch - dy).clamp(-1.5, 1.5);
            self.sway = (self.sway + vec2(md.x, md.y) * 0.00035).clamp(Vec2::splat(-0.04), Vec2::splat(0.04));
        }
        self.sway *= 1.0 - (dt * 8.0).min(1.0);
        self.yaw = wrap_angle(self.yaw);

        if !self.team.is_playing() || (!self.alive && self.state != MatchState::PostMatch && self.team == Team::Spectator) {
            self.update_spectator(dt, playing_input);
        }

        // ---- local player
        let can_act = self.alive && playing_input && !self.frozen();
        let key = |k: KeyCode| can_act && is_key_down(k);
        let pressed = |k: KeyCode| can_act && is_key_pressed(k);
        let def = self.weapon().def();
        if self.alive {
            if pressed(KeyCode::C) {
                self.want_stance = if self.want_stance == Stance::Crouch { Stance::Stand } else { Stance::Crouch };
            }
            if pressed(KeyCode::LeftControl) || pressed(KeyCode::Z) {
                self.want_stance = if self.want_stance == Stance::Prone { Stance::Stand } else { Stance::Prone };
            }
            let mut fwd = 0.0;
            let mut right = 0.0;
            if key(KeyCode::W) {
                fwd += 1.0;
            }
            if key(KeyCode::S) {
                fwd -= 1.0;
            }
            if key(KeyCode::D) {
                right += 1.0;
            }
            if key(KeyCode::A) {
                right -= 1.0;
            }
            let jump = pressed(KeyCode::Space);
            if jump {
                self.want_stance = Stance::Stand;
            }
            let ads_held = can_act && is_mouse_button_down(MouseButton::Right);
            let reloading = self.reload_until.is_some();
            let want_sprint = key(KeyCode::LeftShift) && fwd > 0.0 && self.body.stance == Stance::Stand && !ads_held && self.stamina > 0.0 && now - self.last_fire > 0.2;
            // scoped snipers use Shift to hold breath instead
            let want_sprint = want_sprint && !(def.scoped && self.ads_t > 0.5);
            self.sprinting = want_sprint;
            if self.sprinting {
                self.stamina = (self.stamina - dt).max(0.0);
            } else {
                self.stamina = (self.stamina + dt * 0.6).min(self.sprint_max());
            }
            if def.scoped && self.ads_t > 0.5 && is_key_down(KeyCode::LeftShift) {
                self.breath = (self.breath - dt).max(0.0);
            } else {
                self.breath = (self.breath + dt * 0.5).min(4.0);
            }
            let ads_ok = ads_held && !self.sprinting && !reloading && now >= self.switch_until && now >= self.melee_until;
            let speed = 1.0 / def.ads_time.max(0.05);
            self.ads_t = if ads_ok { (self.ads_t + dt * speed).min(1.0) } else { (self.ads_t - dt * speed * 1.5).max(0.0) };

            let inp = MoveInput {
                forward: if self.frozen() { 0.0 } else { fwd },
                right: if self.frozen() { 0.0 } else { right },
                yaw: self.yaw,
                jump: jump && !self.frozen(),
                sprint: self.sprinting,
                ads: self.ads_t > 0.5,
                want_stance: self.want_stance,
                speed_mult: def.move_mult,
            };
            let mut t = dt.min(0.1);
            while t > 0.0 {
                let h = t.min(1.0 / 60.0);
                self.body.step(&self.world, &inp, h);
                t -= h;
            }
            if self.body.stance != self.want_stance && self.want_stance == Stance::Stand {
                // blocked from standing up — keep trying silently
            }
            // footsteps & bob
            let hs = vec2(self.body.vel.x, self.body.vel.z).length();
            self.walk_amount += ((hs / WALK_SPEED).min(1.5) - self.walk_amount) * (dt * 8.0).min(1.0);
            self.bob_phase += hs * dt * 1.8;
            if self.body.on_ground && hs > 1.5 && self.body.stance == Stance::Stand {
                self.step_acc += hs * dt;
                if self.step_acc > if self.sprinting { 2.6 } else { 2.0 } {
                    self.step_acc = 0.0;
                    if let Some(a) = audio {
                        a.play(Sfx::Step, 0.25);
                    }
                }
            }

            // ---- weapon actions
            self.spread_bonus = (self.spread_bonus - dt * 8.0).max(0.0);
            self.recoil_vis = (self.recoil_vis - dt * 7.0).max(0.0);
            if let Some((end, _)) = self.reload_until {
                if now >= end {
                    let s = self.slot as usize;
                    let m = self.weapons[s].0.def().mag;
                    let take = (m - self.weapons[s].1).min(self.weapons[s].2);
                    self.weapons[s].1 += take;
                    self.weapons[s].2 -= take;
                    self.reload_until = None;
                }
            }
            if pressed(KeyCode::Key1) {
                self.switch_to(0);
            }
            if pressed(KeyCode::Key2) {
                self.switch_to(1);
            }
            let wheel = mouse_wheel().1;
            if can_act && wheel.abs() > 0.1 {
                let s = 1 - self.slot;
                self.switch_to(s);
            }
            if pressed(KeyCode::R) {
                self.start_reload();
                if self.reload_until.is_some() {
                    if let Some(a) = audio {
                        a.play(Sfx::Reload, 0.5);
                    }
                }
            }
            let trig_down = can_act && is_mouse_button_down(MouseButton::Left);
            let trig_pressed = trig_down && !self.trigger_was_down;
            self.trigger_was_down = trig_down;
            let trigger = if def.auto { trig_down } else { trig_pressed };
            let busy = self.reload_until.is_some() || now < self.switch_until || now < self.melee_until || now < self.throw_until || self.sprinting || self.cooking.is_some();
            if trigger && !busy && now >= self.next_fire {
                let (_, mag, res) = self.weapons[self.slot as usize];
                if mag > 0 {
                    self.fire(&audio);
                } else {
                    if trig_pressed {
                        if let Some(a) = audio {
                            a.play(Sfx::Empty, 0.5);
                        }
                    }
                    if res > 0 {
                        self.start_reload();
                    }
                }
            }
            // auto reload on empty after the last shot
            if self.weapons[self.slot as usize].1 == 0 && self.weapons[self.slot as usize].2 > 0 && self.reload_until.is_none() && now - self.last_fire > 0.25 && !busy {
                self.start_reload();
            }
            // grenade (hold G to cook)
            if pressed(KeyCode::G) && self.grenades > 0 && self.cooking.is_none() && now >= self.throw_until {
                self.cooking = Some(now);
                self.reload_until = None;
            }
            if let Some(start) = self.cooking {
                let release = !is_key_down(KeyCode::G) || !can_act || now - start > (GRENADE_FUSE - 0.35) as f64;
                if release {
                    self.cooking = None;
                    if self.alive {
                        let d = (self.aim_dir() + vec3(0.0, 0.18, 0.0)).normalize();
                        self.net.send(C2S::Grenade { origin: (self.eye() + d * 0.3).to_array(), dir: d.to_array(), cook: (now - start) as f32 });
                        self.grenades = self.grenades.saturating_sub(1);
                        self.last_throw = now;
                        self.throw_until = now + 0.5;
                        if let Some(a) = audio {
                            a.play(Sfx::Knife, 0.4);
                        }
                    }
                }
            }
            if pressed(KeyCode::V) && now >= self.melee_ready {
                self.melee_ready = now + MELEE_COOLDOWN as f64;
                self.melee_until = now + 0.45;
                self.reload_until = None;
                self.net.send(C2S::Melee);
                if let Some(a) = audio {
                    a.play(Sfx::Knife, 0.7);
                }
            }
            self.using = key(KeyCode::F);
            if pressed(KeyCode::Key5) || pressed(KeyCode::Key4) {
                match self.me.rewards.last() {
                    Some(Killstreak::Airstrike) => self.overlay = Overlay::Airstrike,
                    Some(_) => self.net.send(C2S::UseKillstreak { target: None }),
                    None => self.center_msg("Немає доступних кілстріків", 1.5, crate::ui::MUTED),
                }
            }
        } else {
            self.ads_t = 0.0;
            self.sprinting = false;
            self.using = false;
            // click to respawn
            if self.team.is_playing() && playing_input && is_mouse_button_pressed(MouseButton::Left) && self.me.respawn_in <= 0.0 {
                self.net.send(C2S::Spawn);
            }
        }

        // ---- send input @30 Hz
        if now - self.last_input_send >= 1.0 / 30.0 {
            self.last_input_send = now;
            self.seq += 1;
            let mut f = 0;
            if self.sprinting {
                f |= flags::SPRINT;
            }
            if self.ads_t > 0.5 {
                f |= flags::ADS;
            }
            if self.body.on_ground {
                f |= flags::ON_GROUND;
            }
            if self.using {
                f |= flags::USE;
            }
            self.net.send_unreliable(C2S::Input(InputState {
                seq: self.seq,
                life: self.life,
                pos: self.body.pos.to_array(),
                vel: self.body.vel.to_array(),
                yaw: self.yaw,
                pitch: self.pitch,
                stance: self.body.stance.to_u8(),
                flags: f,
                slot: self.slot,
            }));
            self.net.flush();
        }
        self.net.maybe_flush(1.0 / 60.0);

        // ---- remotes interpolation
        self.update_remotes(dt, audio);

        // ---- misc timers
        self.shake = (self.shake - dt * 2.5).max(0.0);
        self.dmg_dirs.retain(|d| now - d.1 < 1.6);
        self.popups.retain(|p| now - p.1 < 1.6);
        if let Some((_, until, _)) = &self.center {
            if now > *until {
                self.center = None;
            }
        }
        self.airstrikes.retain(|a| now - a.2 < 5.0);
        self.fx.update(dt);
        // grenade smoke trails
        for e in &self.ents {
            if let EntSnap::Grenade { pos, .. } = e {
                if self.rng.chance(0.3) {
                    self.fx.smoke_trail(Vec3::from_array(*pos));
                }
            }
        }
        // planted bomb beeps faster near the end
        if let Some(b) = &self.bomb {
            if b.planted_at.is_some() {
                let period = (b.timer / 30.0).clamp(0.12, 1.0) as f64;
                if now - self.bomb_beep > period {
                    self.bomb_beep = now;
                    if let Some(a) = audio {
                        a.play_at(Sfx::Beep, Vec3::from_array(b.pos), self.eye(), 0.6, 60.0);
                    }
                }
            }
        }
        if let Some(a) = audio {
            let heli = self.ents.iter().filter_map(|e| if let EntSnap::Heli { pos, .. } = e { Some(Vec3::from_array(*pos).distance(self.eye())) } else { None }).reduce(f32::min);
            a.heli(heli);
        }
    }

    fn update_spectator(&mut self, dt: f32, input: bool) {
        if !input {
            return;
        }
        let (f, r) = yaw_basis(self.yaw);
        let d = look_dir(self.yaw, self.pitch);
        let mut v = Vec3::ZERO;
        if is_key_down(KeyCode::W) {
            v += d;
        }
        if is_key_down(KeyCode::S) {
            v -= d;
        }
        if is_key_down(KeyCode::D) {
            v += r;
        }
        if is_key_down(KeyCode::A) {
            v -= r;
        }
        if is_key_down(KeyCode::Space) {
            v.y += 1.0;
        }
        if is_key_down(KeyCode::C) {
            v.y -= 1.0;
        }
        let _ = f;
        let speed = if is_key_down(KeyCode::LeftShift) { 25.0 } else { 10.0 };
        self.spec_pos += v.normalize_or_zero() * speed * dt;
    }

    fn update_remotes(&mut self, dt: f32, audio: Option<&Audio>) {
        let rt = self.render_time();
        let now = self.now;
        let listener = self.eye();
        for r in self.remotes.values_mut() {
            // find the two snapshots around rt
            let mut a = None;
            let mut b = None;
            for (i, (t, _)) in r.buf.iter().enumerate() {
                if *t <= rt {
                    a = Some(i);
                } else {
                    b = Some(i);
                    break;
                }
            }
            let (pa, pb, k) = match (a, b) {
                (Some(i), Some(j)) => {
                    let (ta, tb) = (r.buf[i].0, r.buf[j].0);
                    (i, j, ((rt - ta) / (tb - ta).max(1e-6)) as f32)
                }
                (Some(i), None) => (i, i, 0.0),
                (None, Some(j)) => (j, j, 0.0),
                (None, None) => continue,
            };
            let sa = &r.buf[pa].1;
            let sb = &r.buf[pb].1;
            let new_pos = Vec3::from_array(sa.pos).lerp(Vec3::from_array(sb.pos), k);
            let moved = vec2(new_pos.x - r.pos.x, new_pos.z - r.pos.z).length();
            let teleport = moved > 4.0;
            r.pos = new_pos;
            r.yaw = sa.yaw + wrap_angle(sb.yaw - sa.yaw) * k;
            r.pitch = sa.pitch + (sb.pitch - sa.pitch) * k;
            r.stance = Stance::from_u8(sb.stance & 3);
            r.silent = sb.stance & 0x80 != 0;
            r.weapon = sb.weapon;
            r.flags = sb.flags;
            r.team = sb.team;
            let was_alive = r.alive;
            r.alive = sb.alive;
            if was_alive && !r.alive {
                r.death_time = Some(now);
            }
            if r.alive {
                r.death_time = None;
            }
            if !teleport && dt > 0.0 {
                let speed = moved / dt;
                r.walk += ((speed / WALK_SPEED).min(1.4) - r.walk) * (dt * 10.0).min(1.0);
                r.phase += moved * 2.2;
                if r.alive && !r.silent && r.flags & flags::ON_GROUND != 0 && r.stance == Stance::Stand && speed > 2.0 {
                    r.step_acc += moved;
                    if r.step_acc > 2.2 {
                        r.step_acc = 0.0;
                        if let Some(a) = audio {
                            a.play_at(Sfx::Step, r.pos, listener, 0.35, 22.0);
                        }
                    }
                }
            }
            // drop very old samples
            while r.buf.len() > 2 && r.buf[1].0 < rt - 0.5 {
                r.buf.pop_front();
            }
        }
    }

    pub fn send_chat(&mut self, text: String, team_only: bool) {
        let t = text.trim().to_string();
        if t.is_empty() {
            return;
        }
        if let Some(rest) = t.strip_prefix("/rcon") {
            let rest = rest.trim();
            if let Some(p) = rest.strip_prefix("login") {
                self.rcon_pass = p.trim().to_string();
                self.push_chat("[rcon] пароль збережено — тепер: /rcon <команда>".into(), crate::ui::ACCENT);
            } else if self.rcon_pass.is_empty() {
                self.push_chat("[rcon] спершу: /rcon login <пароль>".into(), crate::ui::BAD);
            } else {
                let cmd = if rest.is_empty() { "help".to_string() } else { rest.to_string() };
                let password = self.rcon_pass.clone();
                self.net.send(C2S::Rcon { password, cmd });
            }
            return;
        }
        self.net.send(C2S::Chat { text: t, team_only });
    }

    pub fn choose_class(&mut self, loadout: Loadout) {
        self.net.send(C2S::SetLoadout(loadout));
        self.has_class = true;
        self.overlay = Overlay::None;
        if self.alive {
            self.center_msg("Клас зміниться після відродження", 2.0, crate::ui::MUTED);
        }
    }

    pub fn join_team(&mut self, t: Option<Team>) {
        self.net.send(C2S::JoinTeam(t));
        self.overlay = if t == Some(Team::Spectator) { Overlay::None } else { Overlay::Class };
    }

    pub fn call_airstrike(&mut self, x: f32, z: f32) {
        self.net.send(C2S::UseKillstreak { target: Some([x, z]) });
        self.overlay = Overlay::None;
    }

    pub fn disconnect(&mut self) {
        self.net.disconnect();
    }

    /// Camera position/orientation for this frame.
    pub fn camera(&self) -> (Vec3, f32, f32) {
        let shake = vec2(
            ((self.now * 53.0).sin() as f32) * self.shake * 0.02,
            ((self.now * 61.0).cos() as f32) * self.shake * 0.02,
        );
        if self.alive {
            let s = self.sway_offset();
            return (self.eye(), self.yaw + s.x + shake.x, self.pitch + s.y + shake.y);
        }
        if !self.team.is_playing() {
            return (self.spec_pos, self.yaw, self.pitch);
        }
        // death cam: rise and look at the killer if visible
        let t = ((self.now - self.death_time) as f32 / 1.2).min(1.0);
        let pos = self.death_pos + vec3(0.0, 1.8 * t, 0.0);
        if let Some((_, _, Some(k))) = &self.death_info {
            if let Some(r) = self.remotes.get(k) {
                let d = r.pos + vec3(0.0, 1.2, 0.0) - pos;
                let yaw = toon_shared::yaw_of(d);
                let pitch = (d.y / d.length().max(0.1)).asin();
                return (pos, yaw, pitch);
            }
        }
        (pos, self.yaw, self.pitch - 0.4 * t)
    }

    pub fn is_enemy(&self, t: Team) -> bool {
        t.is_playing() && t != self.team
    }

    pub fn weapon_class(&self) -> WeaponClass {
        self.weapon().def().class
    }
}

/// Random direction inside a cone of half-angle `a`.
pub fn cone(rng: &mut Rng, dir: Vec3, a: f32) -> Vec3 {
    if a <= 0.0 {
        return dir;
    }
    let up = if dir.y.abs() > 0.95 { Vec3::X } else { Vec3::Y };
    let u = dir.cross(up).normalize();
    let v = dir.cross(u).normalize();
    let r = a * rng.f32().sqrt();
    let t = rng.range(0.0, std::f32::consts::TAU);
    (dir + u * (r * t.cos()) + v * (r * t.sin())).normalize()
}
