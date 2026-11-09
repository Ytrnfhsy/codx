//! In-game client: networking, prediction, weapons and interpolation of other players.

use crate::audio::{Audio, Sfx};
use crate::binds::{self, Action};
use crate::effects::Effects;
use crate::killcam::{has_killcam, Event, Frame, KillInfo, Killcam, Recorder};
use crate::mesh::{MeshBuilder, ToonMesh};
use crate::netclient::NetClient;
use crate::ragdoll::{DeathBlow, Ragdoll};
use crate::render::{Actor, VmState};
use crate::settings::Settings;
use macroquad::prelude::*;
use std::collections::{HashMap, VecDeque};
use toon_shared::consts::*;
use toon_shared::map::{Map, MapId};
use toon_shared::physics::{Body, MoveInput, Stance, World};
use toon_shared::protocol::*;
use toon_shared::ray::ray_player;
use toon_shared::rng::Rng;
use toon_shared::tr;
use toon_shared::tuning::{Tuning, WeaponStats};
use toon_shared::weapons::{Cause, Killstreak, Loadout, Perk2, Perk3, WeaponId, DEFAULT_CLASSES};
use toon_shared::{look_dir, maps, wrap_angle, yaw_basis, Mode, Team};

pub struct Remote {
    pub buf: VecDeque<(f64, PlayerSnap)>,
    pub team: Team,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub stance: Stance,
    pub weapon: WeaponId,
    pub flags: u8,
    pub silent: bool,
    /// Skydiving: 0 no, 1 free fall, 2 under the parachute.
    pub chute: u8,
    /// Spawn protection active.
    pub protected: bool,
    pub alive: bool,
    pub phase: f32,
    pub walk: f32,
    /// 0..1 how far into the sprint pose (smoothed).
    pub sprint: f32,
    pub death_time: Option<f64>,
    pub muzzle: Vec3,
    pub melee_t: f64,
    pub step_acc: f32,
    /// When we first saw this player reloading.
    pub reload_since: Option<f64>,
    pub vel: Vec3,
}

pub struct Line {
    pub text: String,
    pub color: Color,
    pub time: f64,
}

/// Something on the ground within reach of the use key.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Pick {
    Weapon(WeaponId),
    /// Battle royale supplies and how many (or per cent left).
    Loot(LootKind, u16),
}

pub struct KillLine {
    pub killer: String,
    pub victim: String,
    pub kteam: Team,
    pub vteam: Team,
    pub cause: Cause,
    pub headshot: bool,
    pub time: f64,
    /// The killer / the victim is us (highlighted in the battle royale, where nobody
    /// has a team colour).
    pub kme: bool,
    pub vme: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Overlay {
    None,
    Team,
    Class,
    Edit(usize),
    Pause,
    /// Picking a point on the map (airstrike, artillery).
    Airstrike,
    /// The ground robot: steer it or let it fight on its own.
    RobotMode,
    Settings,
    /// Key bindings.
    Controls,
    /// Battle royale: picking a character.
    Skin,
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
    pub my_id: u8,
    pub server_name: String,
    pub map: Map,
    pub world: World,
    pub level: crate::level::Level,
    pub sky_mesh: MeshBuilder,
    pub mode: Mode,
    pub gs: GameSettings,

    pub state: MatchState,
    pub time_left: f32,
    pub scores: [i32; 2],
    pub overtime: bool,
    pub me: MeSnap,
    pub bomb: Option<BombSnap>,
    /// Battle royale state (plane, zone, players left).
    pub br: Option<BrSnap>,
    /// Battle royale: the big map is open.
    pub big_map: bool,
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
    /// Server-provided movement/weapon tuning.
    pub tuning: Tuning,
    /// Consecutive shots in the current burst (recoil build-up).
    shot_streak: u32,
    /// Camera climb from recoil that recovers after the burst.
    recoil_accum: f32,
    /// When we last stopped sprinting (weapon raise delay).
    sprint_end: f64,
    /// Duration of the current weapon raise (for the animation).
    pub switch_dur: f32,
    /// We already asked the server to drop our spawn protection.
    protection_dropped: bool,
    /// Camera-only offset that eases out stair steps, stance changes and server corrections.
    cam_smooth: Vec3,
    pub last_fire: f64,
    pub stamina: f32,
    /// Ran out of breath: no sprinting until some stamina is back (or Shift is let go),
    /// or the sprint would flicker on and off every frame.
    pub winded: bool,
    /// 0..1 how far the first-person gun is into its sprint pose (smoothed).
    pub sprint_pose: f32,
    pub sprinting: bool,
    pub grenades: u8,
    pub smokes: u8,
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
    /// How much the scope sways (1 = freely; eases down to almost nothing while the
    /// breath is held, and back up after).
    pub steady: f32,
    /// Q is held with a smoke grenade ready: it is thrown when Q comes up.
    pub smoke_held: bool,
    /// The Controls page (opened from the pause menu's settings).
    pub binds_editor: crate::binds::Editor,
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
    /// Artillery barrages on their way: (target, radius, when called).
    pub artillery: Vec<(Vec3, f32, f64)>,
    /// The killstreak vehicle we steer.
    pub piloting: Option<crate::pilot::Piloting>,
    /// The language the server writes to us in (sent again when changed in the settings).
    sent_lang: toon_shared::lang::Lang,
    /// Character last told to the server (255: not yet).
    sent_skin: u8,
    /// Where the character chooser wants its turning 3D preview this frame.
    pub skin_preview: Option<Rect>,
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
    pub prev_state: MatchState,
    pub mouse_delta: Vec2,
    pub class_choice: usize,
    pub streak_hint: Option<Killstreak>,
    pub bomb_beep: f64,
    pub muzzle_flash: f64,

    // replays and bodies
    pub recorder: Recorder,
    pub killcam: Option<Killcam>,
    /// A killcam waiting to start: (real time to start, info, final killcam).
    pending_killcam: Option<(f64, KillInfo, bool)>,
    pub ragdolls: Vec<(u8, Ragdoll)>,
    /// How each recently killed player died (for ragdoll impulses).
    blows: HashMap<u8, (DeathBlow, f64)>,
    explosions: Vec<(Vec3, f64)>,
    pub hit_head: bool,
    reload_cue: usize,
    was_on_ground: bool,
    fall_speed: f32,
    /// When each dropped weapon was first seen (for the falling animation).
    pub pickup_seen: HashMap<u16, f64>,
    /// Sun shadow map of the current level (rendered on the first frame after a map loads).
    pub shadow: Option<crate::mesh::ShadowMap>,
    /// Dropped weapon we can pick up right now.
    pub pickup_target: Option<(u16, Pick)>,
    next_pickup: f64,
    spawn_time: f64,
}

/// Server time the client currently sees (clock offset applied).
const KILLCAM_DELAY: f64 = 1.1;

pub fn build_map_mesh(map: &Map) -> ToonMesh {
    crate::level::boxes_mesh(map.boxes.iter(), map.id as u64 + 3, true)
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
            let off = vec3(
                k as f32 * 9.0 - 13.0,
                rng.range(-2.0, 3.0),
                rng.range(-4.0, 4.0),
            );
            let s = rng.range(7.0, 12.0);
            let m = Mat4::from_translation(base + off) * Mat4::from_scale(vec3(1.3, 0.75, 1.0));
            sky.add_sphere_ex(&m, s, [255, 255, 255], 10, 6, 255, false);
        }
    }
    sky
}

impl GameClient {
    pub fn new(net: NetClient, w: Welcome, settings: &Settings, pending: Vec<S2C>) -> GameClient {
        let map = maps::build(w.map);
        let world = World::from_map(&map);
        let level = crate::level::Level::build(&map);
        let sky_mesh = build_sky(&map);
        let def = DEFAULT_CLASSES[0].1;
        let spec = vec3(map.center().x, 15.0, map.max.y);
        let mut g = GameClient {
            net,
            my_id: w.id,
            server_name: w.server_name,
            map,
            world,
            level,
            sky_mesh,
            mode: w.mode,
            gs: w.settings,
            state: MatchState::Warmup,
            time_left: 0.0,
            scores: [0, 0],
            overtime: false,
            me: MeSnap::default(),
            bomb: None,
            br: None,
            big_map: false,
            ents: Vec::new(),
            scoreboard: Vec::new(),
            latest_tick: 0,
            clock_offset: 0.0,
            have_clock: false,
            remotes: HashMap::new(),
            team: Team::Spectator,
            has_class: false,
            body: Body {
                pos: spec,
                ..Default::default()
            },
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
            tuning: Tuning::default(),
            shot_streak: 0,
            recoil_accum: 0.0,
            sprint_end: -10.0,
            switch_dur: 0.5,
            protection_dropped: false,
            cam_smooth: Vec3::ZERO,
            last_fire: -10.0,
            stamina: Tuning::default().movement.sprint_time,
            winded: false,
            sprint_pose: 0.0,
            sprinting: false,
            grenades: 0,
            smokes: 0,
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
            steady: 1.0,
            smoke_held: false,
            binds_editor: Default::default(),
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
            artillery: Vec::new(),
            piloting: None,
            sent_lang: toon_shared::lang::get(),
            sent_skin: 255,
            skin_preview: None,
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
            prev_state: MatchState::Warmup,
            mouse_delta: Vec2::ZERO,
            class_choice: settings.last_class,
            streak_hint: None,
            bomb_beep: 0.0,
            muzzle_flash: -10.0,
            recorder: Recorder::default(),
            killcam: None,
            pending_killcam: None,
            ragdolls: Vec::new(),
            blows: HashMap::new(),
            explosions: Vec::new(),
            hit_head: false,
            reload_cue: 0,
            was_on_ground: true,
            fall_speed: 0.0,
            pickup_seen: HashMap::new(),
            shadow: None,
            pickup_target: None,
            next_pickup: 0.0,
            spawn_time: -10.0,
        };
        if !w.motd.is_empty() {
            g.push_chat(format!("[{}] {}", g.server_name, w.motd), crate::ui::ACCENT);
        }
        {
            use binds::key as k;
            let mv = format!(
                "{}{}{}{}",
                k(Action::Forward),
                k(Action::Left),
                k(Action::Back),
                k(Action::Right)
            );
            let (sp, cr, pr, aim, rl) = (
                k(Action::Sprint),
                k(Action::Crouch),
                k(Action::Prone),
                k(Action::Aim),
                k(Action::Reload),
            );
            g.push_chat(
                tr!(
                    "{mv}, {sp} — біг, {cr} — присід, {pr} — лежати, {aim} — приціл, {rl} — перезарядка",
                    "{mv}, {sp} — sprint, {cr} — crouch, {pr} — prone, {aim} — aim, {rl} — reload"
                ),
                crate::ui::MUTED,
            );
            let (gr, sm, kn, us, ks) = (
                k(Action::Grenade),
                k(Action::Smoke),
                k(Action::Knife),
                k(Action::Use),
                k(Action::Killstreak),
            );
            let (ca, ct, sc, cl) = (
                k(Action::ChatAll),
                k(Action::ChatTeam),
                k(Action::Scoreboard),
                k(Action::Class),
            );
            g.push_chat(
                tr!(
                    "{gr} — граната, {sm} — дим, {kn} — ніж, {us} — дія, {ks} — кілстрік, {ca}/{ct} — чат, {sc} — рахунок, {cl} — клас",
                    "{gr} — grenade, {sm} — smoke, {kn} — knife, {us} — use, {ks} — killstreak, {ca}/{ct} — chat, {sc} — score, {cl} — class"
                ),
                crate::ui::MUTED,
            );
        }
        let mut audio_dummy = None;
        for m in pending {
            g.handle_msg(m, &mut audio_dummy);
        }
        g
    }

    pub fn push_chat(&mut self, text: String, color: Color) {
        self.chat.push(Line {
            text,
            color,
            time: get_time(),
        });
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
        self.scoreboard
            .iter()
            .find(|r| r.id == id)
            .map(|r| r.name.as_str())
    }

    fn load_map(&mut self, map: MapId, mode: Mode, gs: GameSettings) {
        self.map = maps::build(map);
        self.world = World::from_map(&self.map);
        self.level = crate::level::Level::build(&self.map);
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
        self.recorder.clear();
        self.killcam = None;
        self.pending_killcam = None;
        self.ragdolls.clear();
        self.shadow = None;
        self.spec_pos = vec3(self.map.center().x, 15.0, self.map.max.y);
        self.body.pos = self.spec_pos;
    }

    // ------------------------------------------------------------------ messages

    pub fn handle_msg(&mut self, m: S2C, audio: &mut Option<&Audio>) {
        let now = get_time();
        match m {
            S2C::Welcome { .. } => {}
            S2C::Reject(reason) => self.leave = Some(reason),
            S2C::MapChange {
                map,
                mode,
                settings,
            } => {
                self.load_map(map, mode, settings);
                let name = self.map.id.name();
                self.center_msg(format!("{name} — {}", mode.name()), 4.0, crate::ui::ACCENT);
            }
            S2C::Snapshot(s) => self.on_snapshot(*s, audio),
            S2C::Scoreboard(rows) => self.scoreboard = rows,
            S2C::Spawned {
                pos,
                yaw,
                life,
                loadout,
            } => {
                self.body = Body {
                    pos: Vec3::from_array(pos),
                    vel: Vec3::ZERO,
                    on_ground: true,
                    stance: Stance::Stand,
                    ..Default::default()
                };
                // battle royale: out of the plane, falling
                let p = Vec3::from_array(pos);
                let ground = self
                    .world
                    .floor_below(p, 400.0)
                    .into_iter()
                    .chain(self.world.ground(p.x, p.z))
                    .fold(f32::MIN, f32::max);
                if self.mode == Mode::Br && p.y > ground + 20.0 {
                    self.body.chute = toon_shared::physics::Chute::FreeFall;
                    self.body.on_ground = false;
                }
                self.cam_smooth = Vec3::ZERO;
                self.want_stance = Stance::Stand;
                self.yaw = yaw;
                self.pitch = 0.0;
                self.life = life;
                self.alive = true;
                self.loadout = loadout;
                self.weapons = [
                    (
                        loadout.primary,
                        self.tuning.weapon(loadout.primary).mag,
                        loadout.reserve_for(loadout.primary, &self.tuning),
                    ),
                    (
                        loadout.secondary,
                        self.tuning.weapon(loadout.secondary).mag,
                        loadout.reserve_for(loadout.secondary, &self.tuning),
                    ),
                ];
                self.slot = 0;
                self.grenades = loadout.grenades();
                self.smokes = 1;
                self.reload_until = None;
                self.switch_until = now + 0.3;
                self.switch_dur = 0.3;
                self.ads_t = 0.0;
                self.shot_streak = 0;
                self.recoil_accum = 0.0;
                self.protection_dropped = false;
                self.stamina = self.sprint_max();
                self.cooking = None;
                self.death_info = None;
                self.has_class = true;
                if !self.killcam.as_ref().is_some_and(|k| k.final_cam) {
                    self.killcam = None;
                }
                self.pending_killcam = None;
                self.reload_cue = 0;
                self.spawn_time = now;
                if matches!(self.overlay, Overlay::Class | Overlay::Team) {
                    self.overlay = Overlay::None;
                }
            }
            S2C::TeamSet(t) => {
                self.team = t;
                if t.is_playing() && self.mode == Mode::Br && !self.has_class {
                    // no classes in the battle royale (you find your weapons): pick who you
                    // are instead
                    self.choose_class(toon_shared::weapons::DEFAULT_CLASSES[0].1);
                    self.overlay = Overlay::Skin;
                } else if t.is_playing() {
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
                    let new = Vec3::from_array(pos);
                    // small corrections glide instead of teleporting the view
                    let err = self.body.pos - new;
                    if err.length() < 2.0 {
                        self.cam_smooth += err;
                    }
                    self.body.pos = new;
                    self.body.vel = Vec3::ZERO;
                }
            }
            S2C::Kill {
                killer,
                victim,
                cause,
                headshot,
                killer_name,
                victim_name,
                killer_team,
                victim_team,
            } => {
                let st = self.server_now();
                self.recorder.event(
                    st,
                    Event::Kill {
                        killer,
                        victim,
                        cause,
                        headshot,
                    },
                );
                let blow = DeathBlow {
                    cause,
                    headshot,
                    from: self.blow_source(killer, cause),
                };
                self.blows.insert(victim, (blow, now));
                if victim == self.my_id {
                    let was_alive = self.alive;
                    self.alive = false;
                    self.death_time = now;
                    self.death_pos = self.eye();
                    self.spec_pos = self.death_pos + vec3(0.0, 4.0, 0.0);
                    self.death_info =
                        Some((killer_name.clone(), cause, killer.filter(|k| *k != victim)));
                    self.cooking = None;
                    if was_alive && self.team.is_playing() {
                        let r = Ragdoll::new(
                            self.body.pos,
                            self.yaw,
                            self.body.stance,
                            self.body.vel,
                            self.team,
                            blow,
                            &mut self.rng,
                        );
                        self.add_ragdoll(victim, r);
                    }
                    // killcam a moment later, once the replay has the whole kill
                    if let Some(k) = killer.filter(|k| *k != victim && has_killcam(cause)) {
                        self.pending_killcam = Some((
                            now + KILLCAM_DELAY,
                            KillInfo {
                                killer: k,
                                victim,
                                killer_name: killer_name.clone(),
                                victim_name: victim_name.clone(),
                                killer_team,
                                cause,
                                headshot,
                            },
                            false,
                        ));
                    }
                }
                if killer == Some(self.my_id) && victim != self.my_id {
                    if let Some(a) = audio {
                        a.play(Sfx::Kill, 0.6);
                    }
                }
                self.killfeed.push(KillLine {
                    killer: killer_name,
                    victim: victim_name,
                    kteam: killer_team,
                    vteam: victim_team,
                    cause,
                    headshot,
                    time: now,
                    kme: killer == Some(self.my_id),
                    vme: victim == self.my_id,
                });
                if self.killfeed.len() > 6 {
                    self.killfeed.remove(0);
                }
            }
            S2C::HitConfirm { kill, headshot } => {
                self.hitmarker = now;
                self.hit_kill = kill;
                self.hit_head = headshot;
                if let Some(a) = audio {
                    a.play(Sfx::Hit, 0.55);
                    if headshot {
                        a.play(Sfx::HitHead, 0.55);
                    }
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
                self.recorder
                    .event(self.server_now(), Event::Explosion { pos: p, radius });
                self.explosions.push((p, now));
                self.fx.explosion(p, radius);
                let d = p.distance(self.eye());
                self.shake = (self.shake
                    + (1.0 - d / (radius * 4.0)).max(0.0) * if big { 1.0 } else { 0.6 })
                .min(1.2);
                if let Some(a) = audio {
                    a.play_at(Sfx::Explosion, p, self.eye(), 1.0, 120.0);
                }
            }
            S2C::Melee { id } => {
                self.recorder.event(self.server_now(), Event::Melee(id));
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
            S2C::Chat {
                from,
                team,
                text,
                team_only,
            } => {
                let line = if from.is_empty() {
                    text
                } else if team_only {
                    tr!("(команда) {from}: {text}", "(team) {from}: {text}")
                } else {
                    format!("{from}: {text}")
                };
                let color = if from.is_empty() {
                    crate::ui::MUTED
                } else if from == "Адмін" || from == "Admin" {
                    crate::ui::ACCENT
                } else {
                    crate::models::team_color(team)
                };
                self.push_chat(line, color);
            }
            S2C::Center { text, secs } => self.center_msg(text, secs as f64, crate::ui::TEXT),
            S2C::StreakReady(k) => {
                self.center_msg(
                    tr!(
                        "{} готовий! Натисни {}",
                        "{} ready! Press {}",
                        k.name(),
                        binds::key(Action::Killstreak)
                    ),
                    3.0,
                    crate::ui::ACCENT,
                );
                self.streak_hint = Some(k);
                if let Some(a) = audio {
                    a.play(Sfx::Streak, 0.7);
                }
            }
            S2C::StreakUsed { by, team, streak } => {
                let friendly = team == self.team;
                let text = if friendly {
                    tr!("{by}: {} — дружній!", "{by}: {} — friendly!", streak.name())
                } else {
                    tr!("Ворожий {}!", "Enemy {}!", streak.name().to_lowercase())
                };
                self.center_msg(
                    text,
                    2.5,
                    if friendly {
                        crate::ui::GOOD
                    } else {
                        crate::ui::BAD
                    },
                );
            }
            S2C::Artillery { target, radius } => {
                let t = Vec3::from_array(target);
                self.artillery.push((t, radius, now));
                // shells whistle in over a few seconds
                if let Some(a) = audio {
                    let d = t.distance(self.eye());
                    let v = (1.0 - d / 140.0).clamp(0.25, 1.0);
                    for k in 0..6 {
                        a.play_later(Sfx::Whistle, 2.2 + k as f32 * 0.7, v * 0.8);
                    }
                }
            }
            S2C::Airstrike { from, to } => {
                self.airstrikes
                    .push((Vec3::from_array(from), Vec3::from_array(to), now));
                if let Some(a) = audio {
                    a.play(Sfx::Jet, 0.9);
                }
            }
            S2C::MatchEnd { winner, text } => {
                self.match_end = Some((text, winner, now));
                self.queue_final_killcam(now);
            }
            S2C::RoundEnd { winner, text } => {
                self.queue_final_killcam(now);
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
            S2C::Tuning(mut t) => {
                t.finalize();
                if self.tuning.movement != t.movement || self.tuning.weapons != t.weapons {
                    if self.have_clock {
                        self.push_chat(
                            toon_shared::lang::t(
                                "Сервер оновив налаштування руху/зброї",
                                "The server changed movement/weapon settings",
                            )
                            .into(),
                            crate::ui::MUTED,
                        );
                    }
                }
                self.tuning = *t;
            }
        }
    }

    fn sprint_max(&self) -> f32 {
        let t = self.tuning.movement.sprint_time;
        if self.loadout.perk3 == Perk3::ExtremeConditioning {
            t * 2.0
        } else {
            t
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
        self.recorder.push(Frame {
            t: st,
            players: s.players.clone(),
            ents: s.ents.clone(),
            shots: s.shots.clone(),
            bomb: s.bomb.clone(),
        });
        self.state = s.state;
        self.time_left = s.time_left;
        self.scores = s.scores;
        self.overtime = s.overtime;

        let mut seen = Vec::new();
        for p in &s.players {
            crate::soldier::set_skin(p.id, p.skin);
        }
        for p in s.players {
            if p.id == self.my_id {
                self.team = p.team;
                continue;
            }
            seen.push(p.id);
            let r = self.remotes.entry(p.id).or_insert_with(|| Remote {
                buf: VecDeque::new(),
                team: p.team,
                pos: Vec3::from_array(p.pos),
                yaw: p.yaw,
                pitch: p.pitch,
                stance: Stance::from_u8(p.stance & 3),
                weapon: p.weapon,
                flags: p.flags,
                silent: false,
                chute: 0,
                protected: false,
                alive: p.alive,
                phase: 0.0,
                walk: 0.0,
                sprint: 0.0,
                death_time: None,
                muzzle: Vec3::from_array(p.pos),
                melee_t: -10.0,
                step_acc: 0.0,
                reload_since: None,
                vel: Vec3::ZERO,
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
        let quiet = now - self.last_fire > 0.5
            && self.reload_until.is_none()
            && now > self.switch_until + 0.3;
        // weapons picked up from the ground (the server is the authority)
        if me.alive && self.alive && now - self.spawn_time > 0.6 {
            for i in 0..2 {
                if me.weapons[i] != self.weapons[i].0 {
                    self.weapons[i] = (me.weapons[i], me.ammo[i].0, me.ammo[i].1);
                    if i == self.slot as usize {
                        let swap = self.tuning.weapon(me.weapons[i]).swap_time;
                        self.reload_until = None;
                        self.switch_until = now + swap as f64;
                        self.switch_dur = swap;
                        self.ads_t = 0.0;
                        self.shot_streak = 0;
                        if let Some(a) = audio {
                            a.play(Sfx::Raise, 0.5);
                        }
                    }
                }
            }
        }
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
            self.smokes = me.smokes;
        }
        self.sync_piloting(if self.alive { me.vehicle.clone() } else { None });
        self.me = me;
        self.bomb = s.bomb;
        if s.br.as_ref().is_some_and(|b| b.in_plane) && self.br.as_ref().is_none_or(|b| !b.in_plane)
        {
            // boarding: look along the plane's course
            if let Some((_, yaw)) = s.br.as_ref().and_then(|b| b.plane) {
                self.yaw = yaw;
                self.pitch = -0.25;
            }
        }
        self.br = s.br;
        self.ents = s.ents;
        let mut present = Vec::new();
        for e in &self.ents {
            match e {
                EntSnap::Weapon { id, .. } => {
                    self.pickup_seen.entry(*id).or_insert(now);
                    present.push(*id);
                }
                // a smoke cloud just popped: the hiss
                EntSnap::Smoke { id, pos, .. } => {
                    if !self.pickup_seen.contains_key(id) {
                        self.pickup_seen.insert(*id, now);
                        if let Some(a) = audio.as_ref() {
                            a.play_at(Sfx::Smoke, Vec3::from_array(*pos), self.eye(), 0.8, 50.0);
                        }
                    }
                    present.push(*id);
                }
                _ => {}
            }
        }
        self.pickup_seen.retain(|id, _| present.contains(id));

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
                self.remotes
                    .get(&shot.id)
                    .map(|r| r.muzzle)
                    .unwrap_or(Vec3::from_array(shot.from))
            };
            self.fx.tracer(from, to);
            let path = to - from;
            let len = path.length();
            if len > 0.1 {
                let (_, _, pierced) = self.world.bullet_trace(from, path / len, len);
                for p in &pierced {
                    self.fx.pierce(
                        from + path / len * p.t,
                        p.mat == toon_shared::map::Mat::Wood,
                    );
                }
            }
            if shot.flesh {
                self.fx.flesh(to);
            } else {
                self.fx.impact(to);
            }
            if shot.id != 255 {
                self.fx.muzzle(from);
            }
            if let Some(a) = audio {
                let sfx = if shot.id == 255 {
                    Sfx::HeliGun
                } else {
                    Sfx::for_weapon(shot.weapon)
                };
                a.play_shot(sfx, from, eye, 0.75);
                if shot.flesh {
                    a.play_at(Sfx::Flesh, to, eye, 0.5, 30.0);
                }
                // bullets cracking past your head
                if self.alive {
                    let d = to - from;
                    let len = d.length();
                    if len > 1.0 {
                        let k = ((eye - from).dot(d) / (len * len)).clamp(0.0, 1.0);
                        let miss = (from + d * k).distance(eye);
                        if miss < 2.5 && k > 0.02 && k < 0.98 {
                            a.play(Sfx::Whiz, 0.75 * (1.0 - miss / 2.5) + 0.15);
                        }
                    }
                }
            }
        }
    }

    /// Server time matching the newest snapshot.
    pub fn server_now(&self) -> f64 {
        self.now + self.clock_offset
    }

    /// Where the blow that killed somebody came from.
    fn blow_source(&self, killer: Option<u8>, cause: Cause) -> Option<Vec3> {
        match cause {
            Cause::Grenade | Cause::Airstrike | Cause::Bomb => self
                .explosions
                .iter()
                .rev()
                .find(|e| self.now - e.1 < 0.6)
                .map(|e| e.0),
            _ => match killer {
                Some(k) if k == self.my_id => Some(self.eye()),
                Some(k) => self.remotes.get(&k).map(|r| r.pos + vec3(0.0, 1.3, 0.0)),
                None => None,
            },
        }
    }

    fn add_ragdoll(&mut self, id: u8, r: Ragdoll) {
        self.ragdolls.push((id, r));
        if self.ragdolls.len() > 24 {
            self.ragdolls.remove(0);
        }
    }

    /// After a match or round ends, everybody watches the last kill again in slow motion.
    fn queue_final_killcam(&mut self, now: f64) {
        let since = self.server_now() - 6.0;
        let Some((_, killer, victim, cause, headshot)) = self.recorder.last_kill(since) else {
            return;
        };
        let name = |id: u8| -> String {
            self.scoreboard
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.name.clone())
                .unwrap_or_else(|| "?".into())
        };
        let (killer_name, victim_name) = (name(killer), name(victim));
        let killer_team = self
            .scoreboard
            .iter()
            .find(|r| r.id == killer)
            .map(|r| r.team)
            .unwrap_or(Team::Spectator);
        self.killcam = None;
        self.pending_killcam = Some((
            now + 0.9,
            KillInfo {
                killer,
                victim,
                killer_name,
                victim_name,
                killer_team,
                cause,
                headshot,
            },
            true,
        ));
    }

    fn start_killcam(&mut self, info: KillInfo, final_cam: bool) {
        // the replay ends at the moment the victim went down
        let since = self.server_now() - 8.0;
        let Some(t_kill) = self.recorder.death_time(info.victim, since) else {
            return;
        };
        self.killcam = Killcam::new(&self.recorder, info, t_kill, final_cam);
    }

    /// The closest dropped weapon in reach that we don't already carry.
    fn find_pickup(&self) -> Option<(u16, Pick)> {
        let me = self.body.pos;
        let near = |pos: &[f32; 3], r: f32| {
            let p = Vec3::from_array(*pos);
            let d = vec2(p.x - me.x, p.z - me.z).length();
            (d < r && (p.y - me.y).abs() < 1.8).then_some(d)
        };
        // (the battle royale's empty second slot holds a placeholder pistol)
        let empty_slot = self.mode == Mode::Br && self.weapons[1].1 == 0 && self.weapons[1].2 == 0;
        self.ents
            .iter()
            .filter_map(|e| match e {
                EntSnap::Weapon {
                    id, weapon, pos, ..
                } => {
                    let have = self
                        .weapons
                        .iter()
                        .enumerate()
                        .any(|(k, w)| w.0 == *weapon && !(k == 1 && empty_slot));
                    near(pos, 1.6)
                        .filter(|_| !have)
                        .map(|d| (d, *id, Pick::Weapon(*weapon)))
                }
                EntSnap::Loot {
                    id,
                    kind,
                    amount,
                    pos,
                } => near(pos, 1.5).map(|d| (d, *id, Pick::Loot(*kind, *amount))),
                _ => None,
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id, w)| (id, w))
    }

    /// Standing where F plants, defuses or picks up the bomb.
    pub fn near_objective(&self) -> bool {
        if self.mode != Mode::Sab {
            return false;
        }
        let me = self.body.pos;
        let flat = |p: Vec3| vec2(p.x - me.x, p.z - me.z).length();
        let bomb = self
            .bomb
            .as_ref()
            .map(|b| flat(Vec3::from_array(b.pos)) < 2.4);
        bomb.unwrap_or(false)
            || self
                .map
                .sites
                .iter()
                .any(|s| flat(*s) < SAB_SITE_RADIUS + 0.4)
    }

    /// Sleeve and hand colours for a first-person view.
    pub fn vm_colors(&self, id: u8, team: Team) -> ([u8; 3], [u8; 3]) {
        crate::soldier::arm_colors(team, id)
    }

    /// Battle royale: we are out of the round and fly around watching (a few seconds
    /// after dying, once the death cam has had its moment).
    pub fn br_out(&self) -> bool {
        self.mode == Mode::Br
            && self.state == MatchState::Playing
            && !self.alive
            && self.br.as_ref().is_some_and(|b| !b.in_plane)
            && self.killcam.is_none()
            && self.now - self.death_time > 4.0
    }

    /// Living remote players, interpolated for this frame.
    pub fn live_actors(&self) -> Vec<Actor> {
        let now = self.now;
        self.remotes
            .iter()
            .filter(|(_, r)| r.alive)
            .map(|(id, r)| Actor {
                id: *id,
                pos: r.pos,
                yaw: r.yaw,
                pitch: r.pitch,
                stance: r.stance,
                team: r.team,
                weapon: r.weapon,
                flags: r.flags,
                phase: r.phase,
                walk: r.walk,
                sprint: r.sprint,
                melee: if now - r.melee_t < 0.4 {
                    ((now - r.melee_t) as f32 / 0.4).clamp(0.0, 1.0)
                } else {
                    0.0
                },
                reload: r.reload_since.map(|t| (now - t) as f32),
                protected: r.protected,
                chute: r.chute,
            })
            .collect()
    }

    /// First-person weapon animation state.
    pub fn vm_state(&self) -> VmState {
        let now = self.now;
        VmState {
            weapon: self.weapon(),
            sleeve: self.vm_colors(self.my_id, self.team).0,
            hand: self.vm_colors(self.my_id, self.team).1,
            ads: self.ads_t,
            bob_phase: self.bob_phase,
            bob: self.walk_amount * (1.0 - self.ads_t * 0.85) * self.tuning.movement.view_bob,
            sway: self.sway,
            recoil: self.recoil_vis,
            sprint: self.sprint_pose,
            reload: self
                .reload_until
                .map(|(end, dur)| (1.0 - ((end - now) / dur) as f32).clamp(0.0, 1.0)),
            raise: if now < self.switch_until {
                ((self.switch_until - now) as f32 / self.switch_dur.max(0.05)).clamp(0.0, 1.0)
            } else {
                0.0
            },
            knife: (now < self.melee_until)
                .then(|| (1.0 - ((self.melee_until - now) / 0.45) as f32).clamp(0.0, 1.0)),
            throwing: self.cooking.is_some() || now < self.throw_until,
            flash: now - self.muzzle_flash < 0.05,
        }
    }

    // ------------------------------------------------------------------ simulation

    fn current_spread(&self) -> f32 {
        let def = self.ws();
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
        let mv = (speed / self.tuning.movement.walk_speed).min(1.3)
            * def.move_spread
            * (1.0 - self.ads_t * 0.8);
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

    /// Spawn protection active (and not given up yet).
    pub fn is_protected(&self) -> bool {
        self.alive && !self.protection_dropped && self.me.protected > 0.0
    }

    /// Stats of the weapon in hand.
    pub fn ws(&self) -> &WeaponStats {
        self.tuning.weapon(self.weapon())
    }

    /// Aim sway while aiming down sights (radians). Heavier/longer guns sway more;
    /// crouching, lying down and (for scopes) holding breath steady it.
    pub fn sway_offset(&self) -> Vec2 {
        if self.ads_t < 0.5 || !self.alive {
            return Vec2::ZERO;
        }
        let scoped = self.weapon().def().scoped;
        let stance = match self.body.stance {
            Stance::Stand => 1.0,
            Stance::Crouch => 0.7,
            Stance::Prone => 0.4,
        };
        let base = if scoped { 0.007 } else { 0.0018 };
        let k = base * self.ws().sway * stance * if scoped { self.steady } else { 1.0 };
        let t = self.now as f32;
        vec2(
            (t * 0.9).sin() * 1.5 + (t * 2.3).sin() * 0.4,
            (t * 1.7).sin() * 1.1,
        ) * k
    }

    pub fn aim_dir(&self) -> Vec3 {
        let s = self.sway_offset();
        look_dir(self.yaw + s.x, self.pitch + s.y)
    }

    fn fire(&mut self, audio: &Option<&Audio>) {
        let now = self.now;
        let w = self.weapon();
        let def = self.ws().clone();
        if now - self.last_fire > (def.fire_interval() * 2.5 + 0.05) as f64 {
            self.shot_streak = 0;
        }
        self.next_fire = now + def.fire_interval() as f64;
        self.weapons[self.slot as usize].1 -= 1;
        self.last_fire = now;
        let spread = self.current_spread().to_radians();
        let eye = self.eye();
        let base = self.aim_dir();
        let dirs: Vec<Vec3> = (0..def.pellets.max(1))
            .map(|_| cone(&mut self.rng, base, spread))
            .collect();
        self.net.send(C2S::Fire {
            weapon: w,
            origin: eye.to_array(),
            dirs: dirs.iter().map(|d| d.to_array()).collect(),
        });

        // local visuals: trace against the world and remote players
        let muzzle = self.viewmodel_muzzle_world();
        for (k, d) in dirs.iter().enumerate() {
            // bullets go through planks and cloth (as on the server)
            let (mut t, _, pierced) = self.world.bullet_trace(eye, *d, 300.0);
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
                for p in pierced.iter().filter(|p| p.t < t) {
                    self.fx
                        .pierce(eye + *d * p.t, p.mat == toon_shared::map::Mat::Wood);
                }
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
        // recoil: builds up over a burst (LMGs!), steadier crouched / prone (bipod)
        let ads_k = 1.0 - self.ads_t * 0.45;
        let stance_k = match self.body.stance {
            Stance::Stand => 1.0,
            Stance::Crouch => def.crouch_recoil,
            Stance::Prone => def.prone_recoil,
        };
        let ramp = def.recoil_scale(self.shot_streak);
        self.shot_streak += 1;
        let kick =
            (def.recoil_v * 0.55 * ads_k * stance_k * ramp * self.rng.range(0.8, 1.2)).to_radians();
        self.pitch = (self.pitch + kick).min(1.5);
        self.recoil_accum += kick;
        self.yaw +=
            (def.recoil_h * 0.5 * ads_k * stance_k * ramp.sqrt() * self.rng.range(-1.0, 1.0))
                .to_radians();
        self.spread_bonus = (self.spread_bonus + def.shot_spread).min(6.0);
        let heft = w.def().heft;
        self.recoil_vis = (self.recoil_vis + 0.5 + heft * 0.5).min(1.4);
        self.shake = (self.shake + heft * 0.05).min(0.3);
        if let Some(a) = audio {
            a.play(Sfx::for_weapon(w), 0.85);
            a.play_later(Sfx::Shell, 0.35 + self.rng.range(0.0, 0.2), 0.25);
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
        let def = self.tuning.weapon(w);
        if self.reload_until.is_some() || mag >= def.mag || res == 0 {
            return;
        }
        let k = if self.loadout.perk2 == Perk2::SleightOfHand {
            0.5
        } else {
            1.0
        };
        let dur = (def.reload_time * k) as f64;
        self.reload_until = Some((self.now + dur, dur));
        self.reload_cue = 0;
        self.net.send(C2S::Reload);
    }

    fn switch_to(&mut self, slot: u8) {
        if slot == self.slot || slot > 1 {
            return;
        }
        self.slot = slot;
        self.reload_until = None;
        let swap = self.tuning.weapon(self.weapons[slot as usize].0).swap_time;
        self.switch_until = self.now + swap as f64;
        self.switch_dur = swap;
        self.shot_streak = 0;
        self.ads_t = 0.0;
        self.net.send(C2S::Switch(slot));
    }

    /// Per-frame update. `input_enabled` is false while menus are open.
    pub fn update(&mut self, dt: f32, settings: &Settings, audio: Option<&Audio>) {
        if settings.skin != self.sent_skin {
            self.sent_skin = settings.skin;
            self.net.send(C2S::SetSkin(settings.skin));
        }
        if settings.lang != self.sent_lang {
            self.sent_lang = settings.lang;
            self.net.send(C2S::Lang(settings.lang));
        }
        self.now = get_time();
        let now = self.now;
        // while a replay plays, the live game is silent
        let mut audio_opt = if self.killcam.is_some() { None } else { audio };
        for m in self.net.poll() {
            self.handle_msg(m, &mut audio_opt);
        }
        if let Some(a) = audio {
            a.tick();
        }
        // killcam replay
        if self.pending_killcam.as_ref().is_some_and(|p| now >= p.0) {
            let (_, info, final_cam) = self.pending_killcam.take().unwrap();
            if final_cam || !self.alive {
                self.start_killcam(info, final_cam);
            }
        }
        if let Some(k) = &mut self.killcam {
            k.update(dt, &self.world, &self.tuning, audio);
            if k.done {
                self.killcam = None;
            }
        }
        for (_, r) in &mut self.ragdolls {
            r.update(&self.world, dt);
        }
        self.ragdolls
            .retain(|(_, r)| r.age < crate::ragdoll::LIFETIME);
        self.blows.retain(|_, b| now - b.1 < 3.0);
        self.explosions.retain(|e| now - e.1 < 2.0);
        if self.net.last_recv.elapsed().as_secs_f32() > 10.0 {
            self.leave = Some(
                toon_shared::lang::t(
                    "З'єднання з сервером втрачено",
                    "Lost connection to the server",
                )
                .into(),
            );
        }

        let playing_input = self.overlay == Overlay::None && self.chat_input.is_none();
        // ---- mouse look
        let md = self.mouse_delta;
        if playing_input && self.piloting.is_some() {
            self.pilot_look(md, settings.sensitivity * 0.0011, settings.invert_y);
        } else if playing_input {
            let zoom = 1.0 + (self.ws().ads_zoom - 1.0) * self.ads_t;
            let sens = settings.sensitivity * 0.0011 / zoom;
            self.yaw -= md.x * sens;
            let dy = md.y * sens * if settings.invert_y { -1.0 } else { 1.0 };
            self.pitch = (self.pitch - dy).clamp(-1.5, 1.5);
            if dy > 0.0 {
                // pulling down compensates recoil, so less to recover later
                self.recoil_accum = (self.recoil_accum - dy).max(0.0);
            }
            // heavier weapons lag behind the view more
            let inertia = 0.00025 + self.ws().weight * 0.00004;
            self.sway = (self.sway + vec2(md.x, md.y) * inertia)
                .clamp(Vec2::splat(-0.05), Vec2::splat(0.05));
        }
        let settle = 8.0 / (1.0 + self.ws().weight * 0.08);
        self.sway *= 1.0 - (dt * settle).min(1.0);
        // aim returns after a burst
        let ws = self.ws();
        if self.recoil_accum > 0.0 && now - self.last_fire > (ws.fire_interval() * 1.5) as f64 {
            let d = self.recoil_accum * (1.0 - (-ws.recoil_recover * dt).exp());
            self.pitch -= d;
            self.recoil_accum -= d;
        }
        self.yaw = wrap_angle(self.yaw);

        if !self.team.is_playing()
            || (!self.alive && self.state != MatchState::PostMatch && self.team == Team::Spectator)
            || self.br_out()
        {
            self.update_spectator(dt, playing_input);
        }

        // battle royale: riding the plane, the jump key gets you out
        if !self.alive
            && playing_input
            && self.br.as_ref().is_some_and(|b| b.in_plane)
            && binds::pressed(Action::Jump)
        {
            self.net.send(C2S::BrJump);
        }

        // ---- local player
        let steer = self.alive && playing_input && !self.frozen() && self.killcam.is_none();
        self.update_piloting(dt, steer);
        // while steering a vehicle the soldier just stands there
        let can_act = steer && self.piloting.is_none();
        let key = |a: Action| can_act && binds::down(a);
        let pressed = |a: Action| can_act && binds::pressed(a);
        let def = self.ws().clone();
        let scoped_weapon = self.weapon().def().scoped;
        if self.alive {
            if pressed(Action::Crouch) {
                self.want_stance = if self.want_stance == Stance::Crouch {
                    Stance::Stand
                } else {
                    Stance::Crouch
                };
            }
            if pressed(Action::Prone) {
                self.want_stance = if self.want_stance == Stance::Prone {
                    Stance::Stand
                } else {
                    Stance::Prone
                };
            }
            let mut fwd = 0.0;
            let mut right = 0.0;
            if key(Action::Forward) {
                fwd += 1.0;
            }
            if key(Action::Back) {
                fwd -= 1.0;
            }
            if key(Action::Right) {
                right += 1.0;
            }
            if key(Action::Left) {
                right -= 1.0;
            }
            let jump = pressed(Action::Jump);
            if jump {
                self.want_stance = Stance::Stand;
            }
            let ads_held = can_act && binds::down(Action::Aim);
            let reloading = self.reload_until.is_some();
            if self.winded && (!key(Action::Sprint) || self.stamina >= self.sprint_max() * 0.35) {
                self.winded = false;
            }
            let want_sprint = key(Action::Sprint)
                && fwd > 0.0
                && self.body.stance == Stance::Stand
                && !ads_held
                && self.stamina > 0.0
                && !self.winded
                && now - self.last_fire > 0.2;
            // scoped snipers use Shift to hold breath instead
            let want_sprint = want_sprint && !(scoped_weapon && self.ads_t > 0.5);
            if self.sprinting && !want_sprint {
                self.sprint_end = now;
            }
            self.sprinting = want_sprint;
            let target = if self.sprinting { 1.0 } else { 0.0 };
            self.sprint_pose += (target - self.sprint_pose) * (dt * 12.0).min(1.0);
            if self.sprinting {
                self.stamina = (self.stamina - dt).max(0.0);
                if self.stamina <= 0.0 {
                    self.winded = true;
                }
            } else {
                self.stamina = (self.stamina + dt * self.tuning.movement.sprint_recover)
                    .min(self.sprint_max());
            }
            let holding = scoped_weapon && self.ads_t > 0.5 && binds::down(Action::Sprint);
            if holding {
                self.breath = (self.breath - dt).max(0.0);
            } else {
                self.breath = (self.breath + dt * 0.5).min(4.0);
            }
            // the sway eases off while you hold your breath (the sight drifts back to the
            // crosshair instead of stopping where it was) and comes back gently after
            let (want, rate) = if holding && self.breath > 0.0 {
                (0.05, 3.0)
            } else {
                (1.0, 1.2)
            };
            self.steady += (want - self.steady) * (dt * rate).min(1.0);
            // after sprinting the weapon has to come back up first (longer for heavy guns)
            let raising = now - self.sprint_end < def.sprint_out_time as f64;
            let ads_ok = ads_held
                && !self.sprinting
                && !raising
                && !reloading
                && now >= self.switch_until
                && now >= self.melee_until;
            let speed = 1.0 / def.ads_time.max(0.05);
            self.ads_t = if ads_ok {
                (self.ads_t + dt * speed).min(1.0)
            } else {
                (self.ads_t - dt * speed * 1.5).max(0.0)
            };

            let inp = MoveInput {
                forward: if self.frozen() { 0.0 } else { fwd },
                right: if self.frozen() { 0.0 } else { right },
                yaw: self.yaw,
                jump: jump && !self.frozen(),
                sprint: self.sprinting,
                ads: self.ads_t > 0.5,
                want_stance: self.want_stance,
                speed_mult: def.move_mult,
                sprint_mult: def.sprint_mult,
                weight: def.weight,
            };
            let eye_before = self.body.stance.eye();
            let mut t = dt.min(0.1);
            while t > 0.0 {
                let h = t.min(1.0 / 60.0);
                self.body.step(&self.world, &inp, &self.tuning.movement, h);
                t -= h;
            }
            if !self.body.on_ground {
                self.fall_speed = self.fall_speed.max(-self.body.vel.y);
            } else if !self.was_on_ground {
                if self.fall_speed > 6.5 {
                    if let Some(a) = audio {
                        a.play(Sfx::Land, ((self.fall_speed - 5.0) / 8.0).clamp(0.2, 0.8));
                    }
                    self.shake = (self.shake + (self.fall_speed - 6.0) * 0.03).min(0.4);
                }
                self.fall_speed = 0.0;
            }
            self.was_on_ground = self.body.on_ground;
            // stairs and stance changes move the view smoothly, not in one jump
            let snap = self.body.take_snap() + (self.body.stance.eye() - eye_before);
            if snap.abs() < 1.5 {
                self.cam_smooth.y -= snap;
            }
            if self.body.stance != self.want_stance && self.want_stance == Stance::Stand {
                // blocked from standing up — keep trying silently
            }
            // footsteps & bob
            let hs = vec2(self.body.vel.x, self.body.vel.z).length();
            self.walk_amount += ((hs / self.tuning.movement.walk_speed).min(1.5)
                - self.walk_amount)
                * (dt * 8.0).min(1.0);
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
            self.spread_bonus = (self.spread_bonus - dt * def.spread_recover).max(0.0);
            self.recoil_vis = (self.recoil_vis - dt * 7.0).max(0.0);
            if let Some((end, _)) = self.reload_until {
                if now >= end {
                    let s = self.slot as usize;
                    let m = self.tuning.weapon(self.weapons[s].0).mag;
                    let take = m.saturating_sub(self.weapons[s].1).min(self.weapons[s].2);
                    self.weapons[s].1 += take;
                    self.weapons[s].2 -= take;
                    self.reload_until = None;
                }
            }
            if let (Some(a), Some((end, dur))) = (audio, self.reload_until) {
                let p = (1.0 - ((end - now) / dur) as f32).clamp(0.0, 1.0);
                let cues = crate::audio::reload_cues(self.weapon().def().class);
                while self.reload_cue < cues.len() && p >= cues[self.reload_cue].0 {
                    a.play(cues[self.reload_cue].1.of(self.weapon()), 0.5);
                    self.reload_cue += 1;
                }
            }
            let slot_before = self.slot;
            if pressed(Action::Primary) {
                self.switch_to(0);
            }
            if pressed(Action::Secondary) {
                self.switch_to(1);
            }
            let wheel = mouse_wheel().1;
            if can_act && wheel.abs() > 0.1 {
                let s = 1 - self.slot;
                self.switch_to(s);
            }
            if self.slot != slot_before {
                if let Some(a) = audio {
                    a.play(Sfx::Raise, 0.45);
                }
            }
            if pressed(Action::Reload) {
                self.start_reload();
            }
            // nobody shoots while skydiving
            let can_act = can_act && self.body.chute == toon_shared::physics::Chute::None;
            let trig_down = can_act && binds::down(Action::Fire);
            // also catch very quick clicks (press + release within one frame)
            let trig_pressed =
                can_act && (binds::pressed(Action::Fire) || (trig_down && !self.trigger_was_down));
            self.trigger_was_down = trig_down;
            // spawn protection: you can't be hurt, so you can't attack either.
            // Any attack key gives the protection up instead (no shot is fired).
            if self.is_protected() {
                if trig_pressed
                    || pressed(Action::Grenade)
                    || pressed(Action::Smoke)
                    || pressed(Action::Knife)
                    || pressed(Action::Killstreak)
                {
                    self.protection_dropped = true;
                    self.net.send(C2S::DropProtection);
                    self.next_fire = now + 0.25;
                    self.melee_ready = self.melee_ready.max(now + 0.25);
                    self.throw_until = self.throw_until.max(now + 0.25);
                    self.trigger_was_down = true; // the click must not also fire
                }
                // fall through with attacks blocked below
            }
            let shielded = self.is_protected();
            let trigger = if def.auto { trig_down } else { trig_pressed };
            let raising = now - self.sprint_end < def.sprint_out_time as f64;
            let busy = shielded
                || self.reload_until.is_some()
                || now < self.switch_until
                || now < self.melee_until
                || now < self.throw_until
                || self.sprinting
                || raising
                || self.cooking.is_some();
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
            if self.weapons[self.slot as usize].1 == 0
                && self.weapons[self.slot as usize].2 > 0
                && self.reload_until.is_none()
                && now - self.last_fire > 0.25
                && !busy
            {
                self.start_reload();
            }
            // grenade (hold G to cook)
            if !shielded
                && pressed(Action::Grenade)
                && self.grenades > 0
                && self.cooking.is_none()
                && now >= self.throw_until
            {
                self.cooking = Some(now);
                self.reload_until = None;
            }
            if let Some(start) = self.cooking {
                let release = !binds::down(Action::Grenade)
                    || !can_act
                    || now - start > (self.tuning.grenade.fuse - 0.35) as f64;
                if release {
                    self.cooking = None;
                    if self.alive {
                        // tilt the throw upwards by the server's arc setting
                        let aim = self.aim_dir();
                        let pitch = aim.y.clamp(-1.0, 1.0).asin()
                            + self.tuning.grenade.throw_arc.to_radians();
                        let flat = vec3(aim.x, 0.0, aim.z).normalize_or_zero();
                        let d = (flat * pitch.cos() + vec3(0.0, pitch.sin(), 0.0)).normalize();
                        self.net.send(C2S::Grenade {
                            origin: (self.eye() + d * 0.3).to_array(),
                            dir: d.to_array(),
                            cook: (now - start) as f32,
                            smoke: false,
                        });
                        self.grenades = self.grenades.saturating_sub(1);
                        self.last_throw = now;
                        self.throw_until = now + 0.5;
                        if let Some(a) = audio {
                            a.play(Sfx::Knife, 0.4);
                        }
                    }
                }
            }
            // smoke grenade (Q): ready on press, thrown on release (no cooking)
            if !shielded
                && pressed(Action::Smoke)
                && self.smokes > 0
                && self.cooking.is_none()
                && now >= self.throw_until
            {
                self.smoke_held = true;
                self.reload_until = None;
            }
            let release = self.smoke_held && (!binds::down(Action::Smoke) || !can_act);
            if release {
                self.smoke_held = false;
            }
            if release && can_act && self.alive && !shielded && self.smokes > 0 {
                let aim = self.aim_dir();
                let pitch =
                    aim.y.clamp(-1.0, 1.0).asin() + self.tuning.grenade.throw_arc.to_radians();
                let flat = vec3(aim.x, 0.0, aim.z).normalize_or_zero();
                let d = (flat * pitch.cos() + vec3(0.0, pitch.sin(), 0.0)).normalize();
                self.net.send(C2S::Grenade {
                    origin: (self.eye() + d * 0.3).to_array(),
                    dir: d.to_array(),
                    cook: 0.0,
                    smoke: true,
                });
                self.smokes -= 1;
                self.reload_until = None;
                self.last_throw = now;
                self.throw_until = now + 0.5;
                if let Some(a) = audio {
                    a.play(Sfx::Knife, 0.4);
                }
            }
            if !shielded && pressed(Action::Knife) && now >= self.melee_ready {
                self.melee_ready = now + MELEE_COOLDOWN as f64;
                self.melee_until = now + 0.45;
                self.reload_until = None;
                self.net.send(C2S::Melee);
                if let Some(a) = audio {
                    a.play(Sfx::Knife, 0.7);
                }
            }
            self.using = key(Action::Use);
            // dropped weapons: F swaps (unless F is needed for the bomb right here)
            self.pickup_target = self.find_pickup();
            if let Some((ent, _)) = self.pickup_target {
                if pressed(Action::Use) && now >= self.next_pickup && !self.near_objective() {
                    self.net.send(C2S::PickUp(ent));
                    self.next_pickup = now + 0.6;
                }
            }
            if self.mode == Mode::Br && pressed(Action::Heal) {
                self.net.send(C2S::UseMedkit);
            }
            if !shielded && pressed(Action::Killstreak) {
                match self.me.rewards.last() {
                    Some(k) if k.needs_target() => self.overlay = Overlay::Airstrike,
                    Some(Killstreak::Nrk) => self.overlay = Overlay::RobotMode,
                    Some(_) => self.net.send(C2S::UseKillstreak {
                        target: None,
                        pilot: true,
                    }),
                    None => self.center_msg(
                        toon_shared::lang::t(
                            "Немає доступних кілстріків",
                            "No killstreaks available",
                        ),
                        1.5,
                        crate::ui::MUTED,
                    ),
                }
            }
        } else {
            self.ads_t = 0.0;
            self.sprinting = false;
            self.using = false;
            // click skips the killcam, then respawns
            if playing_input && binds::pressed(Action::Fire) {
                // a click skips the killcam; once it is over, a click respawns
                if !self.skip_killcam() && self.team.is_playing() && self.me.respawn_in <= 0.0 {
                    self.net.send(C2S::Spawn);
                }
            }
        }

        // ---- send input @30 Hz
        if now - self.last_input_send >= 1.0 / 30.0 {
            self.last_input_send = now;
            self.send_drive();
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
        let live_audio = if self.killcam.is_some() { None } else { audio };
        self.update_remotes(dt, live_audio);

        // ---- misc timers
        self.shake = (self.shake - dt * 2.5).max(0.0);
        self.cam_smooth *= (-dt * 14.0).exp();
        if self.cam_smooth.length() < 0.001 {
            self.cam_smooth = Vec3::ZERO;
        }
        self.dmg_dirs.retain(|d| now - d.1 < 1.6);
        self.popups.retain(|p| now - p.1 < 1.6);
        if let Some((_, until, _)) = &self.center {
            if now > *until {
                self.center = None;
            }
        }
        self.airstrikes.retain(|a| now - a.2 < 5.0);
        self.artillery.retain(|a| now - a.2 < 9.0);
        self.fx.update(dt);
        // grenade smoke trails
        for e in &self.ents {
            if let EntSnap::Grenade { pos, .. } = e {
                self.fx.smoke_trail(Vec3::from_array(*pos));
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
            let heli = self
                .ents
                .iter()
                .filter_map(|e| {
                    if let EntSnap::Heli { pos, .. } = e {
                        Some(Vec3::from_array(*pos).distance(self.eye()))
                    } else {
                        None
                    }
                })
                .reduce(f32::min);
            a.heli(heli);
            // the nearest drone buzzes (our own one right in our ears)
            let ear = self.camera().0;
            let own = self
                .piloting
                .as_ref()
                .filter(|p| p.kind != toon_shared::vehicles::VehicleKind::Nrk)
                .map(|_| 3.0);
            let drone = self
                .ents
                .iter()
                .filter_map(|e| match e {
                    EntSnap::Vehicle { pos, kind, .. }
                        if *kind != toon_shared::vehicles::VehicleKind::Nrk =>
                    {
                        Some(Vec3::from_array(*pos).distance(ear))
                    }
                    _ => None,
                })
                .chain(own)
                .reduce(f32::min);
            a.drone(drone);
        }
    }

    fn update_spectator(&mut self, dt: f32, input: bool) {
        if !input {
            return;
        }
        let (f, r) = yaw_basis(self.yaw);
        let d = look_dir(self.yaw, self.pitch);
        let mut v = Vec3::ZERO;
        if binds::down(Action::Forward) {
            v += d;
        }
        if binds::down(Action::Back) {
            v -= d;
        }
        if binds::down(Action::Right) {
            v += r;
        }
        if binds::down(Action::Left) {
            v -= r;
        }
        if binds::down(Action::Jump) {
            v.y += 1.0;
        }
        if binds::down(Action::Crouch) {
            v.y -= 1.0;
        }
        let _ = f;
        let speed = if binds::down(Action::Sprint) {
            25.0
        } else {
            10.0
        };
        self.spec_pos += v.normalize_or_zero() * speed * dt;
    }

    fn update_remotes(&mut self, dt: f32, audio: Option<&Audio>) {
        let rt = self.render_time();
        let walk_speed = self.tuning.movement.walk_speed;
        let now = self.now;
        let listener = self.eye();
        let mut deaths = Vec::new();
        for (id, r) in self.remotes.iter_mut() {
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
            let old_pos = r.pos;
            let moved = vec2(new_pos.x - r.pos.x, new_pos.z - r.pos.z).length();
            let teleport = moved > 4.0;
            r.pos = new_pos;
            r.yaw = sa.yaw + wrap_angle(sb.yaw - sa.yaw) * k;
            r.pitch = sa.pitch + (sb.pitch - sa.pitch) * k;
            r.stance = Stance::from_u8(sb.stance & 3);
            r.silent = sb.stance & stance_bits::SILENT != 0;
            r.chute = if sb.stance & stance_bits::CHUTE != 0 {
                2
            } else if sb.stance & stance_bits::FREEFALL != 0 {
                1
            } else {
                0
            };
            r.protected = sb.alive && sb.stance & stance_bits::PROTECTED != 0;
            r.weapon = sb.weapon;
            r.flags = sb.flags;
            r.team = sb.team;
            let was_alive = r.alive;
            r.alive = sb.alive;
            if !teleport && dt > 0.0 {
                let v = ((new_pos - old_pos) / dt).clamp_length_max(9.0);
                r.vel += (v - r.vel) * (dt * 12.0).min(1.0);
            }
            if was_alive && !r.alive {
                r.death_time = Some(now);
                deaths.push((*id, r.pos, r.yaw, r.stance, r.vel, r.team, r.weapon));
            }
            match (r.flags & flags::RELOADING != 0 && r.alive, r.reload_since) {
                (true, None) => r.reload_since = Some(now),
                (false, Some(_)) => r.reload_since = None,
                _ => {}
            }
            if r.alive {
                r.death_time = None;
            }
            if !teleport && dt > 0.0 {
                let speed = moved / dt;
                r.walk += ((speed / walk_speed).min(1.4) - r.walk) * (dt * 6.0).min(1.0);
                let sprint = if r.flags & flags::SPRINT != 0 {
                    1.0
                } else {
                    0.0
                };
                r.sprint += (sprint - r.sprint) * (dt * 7.0).min(1.0);
                r.phase += moved * 2.2;
                if r.alive
                    && !r.silent
                    && r.flags & flags::ON_GROUND != 0
                    && r.stance == Stance::Stand
                    && speed > 2.0
                {
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
        for (id, pos, yaw, stance, vel, team, weapon) in deaths {
            let blow = self.blows.get(&id).map(|b| b.0).unwrap_or(DeathBlow {
                cause: Cause::World,
                headshot: false,
                from: None,
            });
            let _ = weapon;
            let r = Ragdoll::new(pos, yaw, stance, vel, team, blow, &mut self.rng);
            self.add_ragdoll(id, r);
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
                self.push_chat(
                    toon_shared::lang::t(
                        "[rcon] пароль збережено — тепер: /rcon <команда>",
                        "[rcon] password saved — now: /rcon <command>",
                    )
                    .into(),
                    crate::ui::ACCENT,
                );
            } else if self.rcon_pass.is_empty() {
                self.push_chat(
                    toon_shared::lang::t(
                        "[rcon] спершу: /rcon login <пароль>",
                        "[rcon] first: /rcon login <password>",
                    )
                    .into(),
                    crate::ui::BAD,
                );
            } else {
                let cmd = if rest.is_empty() {
                    "help".to_string()
                } else {
                    rest.to_string()
                };
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
            self.center_msg(
                toon_shared::lang::t(
                    "Клас зміниться після відродження",
                    "Your class changes when you respawn",
                ),
                2.0,
                crate::ui::MUTED,
            );
        }
    }

    /// Skips the killcam that is playing or about to play (the final one cannot be
    /// skipped). Returns whether there was one.
    pub fn skip_killcam(&mut self) -> bool {
        let skippable = self.killcam.as_ref().is_some_and(|k| !k.final_cam)
            || self.pending_killcam.as_ref().is_some_and(|p| !p.2);
        if skippable {
            self.killcam = None;
            self.pending_killcam = None;
        }
        skippable
    }

    /// Where closing the pause menu leads: back to picking a class if there is none yet.
    pub fn after_pause(&self) -> Overlay {
        // spectators just keep watching; the team is one click away in the pause menu
        if self.team.is_playing() && !self.has_class {
            Overlay::Class
        } else {
            Overlay::None
        }
    }

    pub fn join_team(&mut self, t: Option<Team>) {
        self.net.send(C2S::JoinTeam(t));
        self.overlay = if t == Some(Team::Spectator) || self.mode == Mode::Br {
            Overlay::None
        } else {
            Overlay::Class
        };
    }

    /// Starts the ground robot, steered or on its own.
    pub fn call_robot(&mut self, pilot: bool) {
        self.net.send(C2S::UseKillstreak {
            target: None,
            pilot,
        });
        self.overlay = Overlay::None;
    }

    pub fn call_airstrike(&mut self, x: f32, z: f32) {
        self.net.send(C2S::UseKillstreak {
            target: Some([x, z]),
            pilot: false,
        });
        self.overlay = Overlay::None;
    }

    pub fn disconnect(&mut self) {
        self.net.disconnect();
    }

    /// Camera position/orientation for this frame.
    pub fn camera(&self) -> (Vec3, f32, f32) {
        if let Some(k) = &self.killcam {
            return k.camera(self.now);
        }
        if self.alive {
            if let Some((pos, yaw, pitch, _)) = self.pilot_camera() {
                return (pos, yaw, pitch);
            }
        }
        let shake = vec2(
            ((self.now * 53.0).sin() as f32) * self.shake * 0.02,
            ((self.now * 61.0).cos() as f32) * self.shake * 0.02,
        );
        if self.alive {
            let s = self.sway_offset();
            return (
                self.eye() + self.cam_smooth,
                self.yaw + s.x + shake.x,
                self.pitch + s.y + shake.y,
            );
        }
        if !self.team.is_playing() || self.br_out() {
            return (self.spec_pos, self.yaw, self.pitch);
        }
        // battle royale: in the plane, the camera follows it from behind
        if let Some((p, _)) = self
            .br
            .as_ref()
            .filter(|b| b.in_plane)
            .and_then(|b| b.plane)
        {
            let dir = look_dir(self.yaw, self.pitch);
            return (
                Vec3::from_array(p) - dir * 42.0 + vec3(0.0, 12.0, 0.0),
                self.yaw,
                self.pitch,
            );
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
