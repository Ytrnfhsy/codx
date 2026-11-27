//! In-game client: networking, prediction, weapons and interpolation of other players.

mod input;
mod interact;
mod modes;
mod net;
mod ping;
mod replay;
mod snapshot;
mod spectate;
mod weapons;

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
use toon_shared::weapons::{Cause, Killstreak, Loadout, WeaponId, DEFAULT_CLASSES};
use toon_shared::weather::Weather;
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
    /// Battle royale squad (0 = on their own) and knocked down.
    pub squad: u8,
    pub downed: bool,
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
    /// A jeep or a boat to get into.
    Car(toon_shared::cars::CarKind),
    /// A death box to look into.
    DeathBox,
    /// A door (and whether it stands open).
    Door(bool),
}

/// A jeep or a boat as we draw it: smoothed towards where the server says it is.
#[derive(Clone, Debug)]
pub struct CarView {
    pub kind: toon_shared::cars::CarKind,
    /// What we draw.
    pub state: toon_shared::cars::CarState,
    /// The latest from the server, and when it came.
    pub target: toon_shared::cars::CarState,
    pub at: f64,
    pub hp: u8,
    pub seats: [u8; toon_shared::cars::SEATS],
}

impl CarView {
    pub fn seat_pos(&self, k: usize) -> Vec3 {
        self.state.to_world(self.kind.seat(k))
    }
}

/// The footstep of somebody standing at `pos`: soft on the bare ground of a big map
/// (earth, grass, sand), hard on floors, roads' kerbs, roofs and small maps.
fn step_sfx(world: &World, pos: Vec3) -> Sfx {
    let on_earth = world
        .terrain
        .as_ref()
        .is_some_and(|t| (pos.y - t.height(pos.x, pos.z)).abs() < 0.12);
    if on_earth {
        Sfx::StepSoft
    } else {
        Sfx::Step
    }
}

/// How far (along the ground) guns and supplies can be picked up from.
pub const PICK_REACH: f32 = 2.3;

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
    /// Metres between them (0: not shown).
    pub dist: u16,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Overlay {
    None,
    Team,
    /// The loadout screen (team modes).
    Class,
    Pause,
    /// Picking a point on the map (airstrike, artillery).
    Airstrike,
    /// The ground robot: steer it or let it fight on its own.
    RobotMode,
    Settings,
    /// Video settings (from the settings page).
    Video,
    /// Key bindings.
    Controls,
    /// Battle royale: picking a character.
    Skin,
    /// Battle royale: what you carry, to throw away or move sights around.
    Inventory,
    /// Battle royale: looking into a death box.
    DeathBox,
    /// The server's leaderboards.
    Leaders,
    /// The squad lobby before a battle royale match.
    Squad,
    /// What we have achieved on this server.
    Achievements,
}

pub struct Welcome {
    pub id: u8,
    pub server_name: String,
    pub motd: String,
    pub map: MapId,
    pub mode: Mode,
    pub settings: GameSettings,
    pub nonce: u64,
}

/// Silence from the server after which we stop and wait for it.
pub const NET_LOST_AFTER: f32 = 1.5;
/// Silence after which we give up and go back to the menu (the server drops a silent
/// player after as long).
pub const NET_GIVE_UP: f32 = 30.0;
/// The map marker goes away once we get this close to it (metres).
const MARKER_REACHED: f32 = 15.0;

pub struct GameClient {
    pub net: NetClient,
    pub my_id: u8,
    pub server_name: String,
    pub map: Map,
    pub world: World,
    pub level: crate::level::Level,
    /// What blasts have broken, and the pieces still coming down.
    pub ruins: crate::ruins::Ruins,
    pub sky_mesh: MeshBuilder,
    pub sun_mesh: MeshBuilder,
    pub mode: Mode,
    pub gs: GameSettings,

    pub state: MatchState,
    pub time_left: f32,
    /// Sabotage: the last countdown second sounded before respawns end.
    overtime_tick: i32,
    /// When respawns ended (for the banner), and whether we knew it last frame.
    pub overtime_at: f64,
    was_overtime: bool,
    pub scores: [i32; 2],
    pub overtime: bool,
    pub me: MeSnap,
    pub bomb: Option<BombSnap>,
    /// Battle royale state (plane, zone, players left).
    pub br: Option<BrSnap>,
    /// Guns, supplies and boxes on the ground near us, as the server last told us.
    pub lying: HashMap<u16, EntSnap>,
    /// The server's leaderboards, as last asked for.
    pub leaderboards: Option<Vec<Board>>,
    /// How far each door has swung open (0 shut .. 1 open), for drawing.
    pub door_swing: Vec<f32>,
    /// The battle royale round's weather (clear elsewhere).
    pub weather: Weather,
    /// Battle royale: the big map is open.
    pub big_map: bool,
    /// A marker put on the big map (x, z), shown on the minimap and in the world.
    pub marker: Option<Vec2>,
    /// Pings of our side (and ours), as last sent.
    pub pings: Vec<PingSnap>,
    /// The squad lobby (a battle royale in squads, before the match).
    pub lobby: Option<LobbySnap>,
    seen_pings: std::collections::HashSet<u16>,
    /// The last ping key press: when, and the place it pointed at (a second press there
    /// right after means «enemy here»).
    last_ping: (f64, Option<Vec3>),
    /// Before the match: (players ready, players, am I ready).
    pub ready: Option<(u8, u8, bool, u16)>,
    /// Battle royale squad (0 = on our own).
    pub squad: u8,
    /// Jeeps and boats around.
    pub cars: HashMap<u16, CarView>,
    /// When the driving controls were last sent.
    next_drive: f64,
    /// When the crates' signal smoke puffs next.
    next_flare: f64,
    /// What G throws in the battle royale: 0 a grenade, else `inventory::THROW_*`.
    pub throw_kind: u8,
    /// Blinded by a flashbang: until when, and how hard.
    pub flashed: (f64, f32),
    /// When the fires' flames flicker next.
    next_flame: f64,
    /// Out of the battle royale: whose eyes we watch through (None with a free camera).
    pub spectate: Option<u8>,
    pub spec_free: bool,
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
    /// Sat in a car last frame (getting out stands us up).
    was_seated: bool,
    pub weapons: [(WeaponId, u16, u16); 2],
    pub slot: u8,
    next_fire: f64,
    /// When the bow's string started being drawn (held trigger).
    pub draw_from: Option<f64>,
    /// (smoke tests) the trigger held down
    pub hold_fire: bool,
    /// (smoke tests) aiming down the sights held
    pub hold_ads: bool,
    /// The draw was let down (R): no new draw until the trigger is let go.
    draw_blocked: bool,
    /// How long the trigger has been held (a machine gun spinning up).
    spin_held: f32,
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
    /// The loadout of this life (which grenades the counts above are).
    pub kit: Loadout,
    pub cooking: Option<f64>,
    pub melee_until: f64,
    pub melee_ready: f64,
    pub throw_until: f64,
    pub recoil_vis: f32,
    pub bob_phase: f32,
    pub walk_amount: f32,
    /// How far the view leans sideways right now (radians).
    pub view_roll: f32,
    /// The health the HUD shows draining (catches up with the real one).
    pub hud_hp: f32,
    /// How far each hand's variable scope is turned up (0 weakest .. 1 strongest).
    pub scope_turn: [f32; 2],
    /// The last enemy the scope rested on: how far, and when.
    pub range_lock: Option<(f32, f64)>,
    pub sway: Vec2,
    step_acc: f32,
    pub breath: f32,
    /// How much the scope sways (1 = freely; eases down to almost nothing while the
    /// breath is held, and back up after).
    pub steady: f32,
    /// Q is held with a smoke grenade ready: it is thrown when Q comes up.
    pub smoke_held: bool,
    /// The Controls page (opened from the pause menu's settings).
    pub binds_editor: binds::Editor,
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
    /// The lobby: how far our character is turned round (dragging the mouse), how close
    /// the camera came (the wheel, 0..1), and where a drag was last frame.
    pub lobby_turn: f32,
    pub lobby_zoom: f32,
    pub lobby_drag: Option<Vec2>,
    pub fx: Effects,
    pub rcon_pass: String,
    /// The connection's number for admin command proofs, and our count of them.
    pub rcon_nonce: u64,
    rcon_seq: u32,
    pub spec_pos: Vec3,
    last_input_send: f64,
    /// Until when the inputs say we kicked off a wall (`flags::KICKED`).
    kicked_until: f64,
    pub shake: f32,
    pub frame_mesh: ToonMesh,
    pub fx_mesh: MeshBuilder,
    pub leave: Option<String>,
    pub rng: Rng,
    pub now: f64,
    pub prev_state: MatchState,
    pub mouse_delta: Vec2,
    /// The loadout being put together on the loadout screen, and the column picked by its
    /// number key (the next number picks a gun in it).
    pub pick: Option<Loadout>,
    pub pick_col: Option<usize>,
    pub streak_hint: Option<Killstreak>,
    pub bomb_beep: f64,
    pub muzzle_flash: f64,

    // replays and bodies
    pub recorder: Recorder,
    pub killcam: Option<Killcam>,
    /// The match's best moments so far.
    pub highlights: crate::highlights::Highlights,
    /// The results of the match just over (the screen between matches), and when.
    pub summary: Option<(MatchSummary, f64)>,
    /// Where the results screen shows its best player (side, gun).
    pub mvp_preview: Option<(Rect, Team, WeaponId)>,
    /// The results screen's replay window, as drawn this frame.
    pub reel_texture: Option<Texture2D>,
    /// Which moment the results screen plays (index into the highlights).
    pub reel: usize,
    /// When the match began (for the moments' clock).
    match_t0: f64,
    /// A killcam waiting to start: (real time to start, info, final killcam).
    pub pending_killcam: Option<(f64, KillInfo, bool)>,
    /// When the inventory screen was opened (its key closes it again only after that).
    pub inventory_opened: f64,
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
const KILLCAM_DELAY: f64 = 2.5;
/// The final killcam (end of a round or match) waits a little longer: first the result.
const FINAL_KILLCAM_DELAY: f64 = 3.0;

pub fn build_map_mesh(map: &Map) -> ToonMesh {
    crate::level::boxes_mesh(map.boxes.iter(), map.id as u64 + 3, true)
}

pub fn build_sky(map: &Map) -> MeshBuilder {
    build_sky_in(map, Weather::Clear)
}

/// The sky dome in this weather (grey clouds when it rains).
pub fn build_sky_in(map: &Map, weather: Weather) -> MeshBuilder {
    let mut sky = MeshBuilder::new();
    let (top, hor, _) = weather.colours(map.sky, map.fog, map.tint);
    let cloud = match weather {
        Weather::Rain => [150, 154, 162],
        Weather::Overcast | Weather::Fog => [214, 216, 220],
        Weather::Evening => [255, 206, 176],
        Weather::Clear => [255, 255, 255],
        Weather::Night => [70, 82, 118],
    };
    let clouds = if matches!(weather, Weather::Rain | Weather::Overcast) {
        26
    } else {
        14
    };
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
    // stars at night: flat dots high on the dome
    if weather == Weather::Night {
        let mut rng = Rng::new(map.id as u64 + 5);
        for _ in 0..160 {
            let a = rng.range(0.0, std::f32::consts::TAU);
            let up = rng.range(0.25, 1.0f32);
            let flat = (1.0 - up * up).sqrt();
            let p = vec3(a.cos() * flat, up, a.sin() * flat) * 360.0;
            let s = rng.range(0.5, 1.3);
            let shine = rng.range(170.0, 255.0) as u8;
            let m = Mat4::from_translation(p);
            sky.add_sphere_ex(&m, s, [shine, shine, 255], 4, 2, 255, true);
        }
    }
    // puffy clouds
    let mut rng = Rng::new(map.id as u64 + 11);
    for _ in 0..clouds {
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
            // two flat tones, toon style: a bright puff with a bluish belly showing under it
            let squash = Mat4::from_scale(vec3(1.3, 0.75, 1.0));
            let belly = [
                (cloud[0] as f32 * 0.84) as u8,
                (cloud[1] as f32 * 0.87) as u8,
                (cloud[2] as f32 * 0.93) as u8,
            ];
            let low = Mat4::from_translation(base + off - vec3(0.0, s * 0.16, 0.0)) * squash;
            sky.add_sphere_ex(&low, s * 0.95, belly, 10, 6, 255, true);
            let m = Mat4::from_translation(base + off) * squash;
            sky.add_sphere_ex(&m, s, cloud, 10, 6, 255, true);
        }
    }
    sky
}

/// The sun where the light comes from, with a glow round it (none behind thick clouds).
pub fn build_sun(weather: Weather) -> MeshBuilder {
    let mut m = MeshBuilder::new();
    let (core, glow, size) = match weather {
        Weather::Clear => ([255, 250, 224], [255, 244, 200], 1.0),
        Weather::Evening => ([255, 214, 150], [255, 170, 110], 1.35),
        // (the moon stands where the light comes from)
        Weather::Night => ([232, 238, 255], [150, 170, 230], 0.8),
        Weather::Overcast | Weather::Rain | Weather::Fog => return m,
    };
    let at = Mat4::from_translation(crate::mesh::LIGHT_DIR.normalize() * 340.0);
    // (from the inside out: each wider glow lies in front of what it surrounds)
    m.add_sphere_ex(&at, 13.0 * size, core, 14, 8, 255, true);
    for (r, alpha) in [(19.0, 90u8), (28.0, 50), (42.0, 26)] {
        m.add_sphere_ex(&at, r * size, glow, 14, 8, alpha, true);
    }
    m
}

impl GameClient {
    pub fn new(net: NetClient, w: Welcome, _settings: &Settings, pending: Vec<S2C>) -> GameClient {
        let map = maps::build(w.map);
        let world = World::from_map(&map);
        let level = crate::level::Level::build(&map);
        let ruins = crate::ruins::Ruins::new(&map);
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
            ruins,
            sky_mesh,
            sun_mesh: build_sun(Weather::Clear),
            mode: w.mode,
            gs: w.settings,
            state: MatchState::Warmup,
            time_left: 0.0,
            overtime_tick: 0,
            overtime_at: -100.0,
            was_overtime: false,
            scores: [0, 0],
            overtime: false,
            me: MeSnap::default(),
            bomb: None,
            weather: Weather::Clear,
            br: None,
            lying: HashMap::new(),
            leaderboards: None,
            door_swing: Vec::new(),
            big_map: false,
            marker: None,
            pings: Vec::new(),
            lobby: None,
            seen_pings: Default::default(),
            last_ping: (f64::NEG_INFINITY, None),
            ready: None,
            squad: 0,
            cars: HashMap::new(),
            next_drive: 0.0,
            next_flare: 0.0,
            throw_kind: 0,
            flashed: (0.0, 0.0),
            next_flame: 0.0,
            spectate: None,
            spec_free: false,
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
            was_seated: false,
            weapons: [(def.primary, 0, 0), (def.secondary, 0, 0)],
            slot: 0,
            next_fire: 0.0,
            draw_from: None,
            hold_fire: false,
            hold_ads: false,
            draw_blocked: false,
            spin_held: 0.0,
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
            kit: DEFAULT_CLASSES[0].1,
            cooking: None,
            melee_until: 0.0,
            melee_ready: 0.0,
            throw_until: 0.0,
            recoil_vis: 0.0,
            bob_phase: 0.0,
            walk_amount: 0.0,
            view_roll: 0.0,
            hud_hp: 0.0,
            scope_turn: [0.0; 2],
            range_lock: None,
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
            lobby_turn: 0.0,
            lobby_zoom: 0.0,
            lobby_drag: None,
            skin_preview: None,
            fx: Effects::new(),
            rcon_pass: String::new(),
            rcon_nonce: w.nonce,
            rcon_seq: 0,
            spec_pos: spec,
            last_input_send: 0.0,
            kicked_until: 0.0,
            shake: 0.0,
            frame_mesh: ToonMesh::new(),
            fx_mesh: MeshBuilder::new(),
            leave: None,
            rng: Rng::from_time(),
            now: crate::clock::now(),
            prev_state: MatchState::Warmup,
            mouse_delta: Vec2::ZERO,
            pick: None,
            pick_col: None,
            streak_hint: None,
            bomb_beep: 0.0,
            muzzle_flash: -10.0,
            recorder: Recorder::default(),
            killcam: None,
            highlights: Default::default(),
            summary: None,
            reel: 0,
            reel_texture: None,
            mvp_preview: None,
            match_t0: 0.0,
            pending_killcam: None,
            inventory_opened: 0.0,
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
                    "{gr} — граната, {sm} — дим, {kn} — ніж, {us} — дія, {ks} — кілстрік, {ca}/{ct} — чат, {sc} — рахунок, {cl} — спорядження",
                    "{gr} — grenade, {sm} — smoke, {kn} — knife, {us} — use, {ks} — killstreak, {ca}/{ct} — chat, {sc} — score, {cl} — loadout"
                ),
                crate::ui::MUTED,
            );
        }
        let mut audio_dummy = None;
        for m in pending {
            g.handle_msg(m, &mut audio_dummy);
        }
        // with a squad from the main lobby: the server keeps it together
        let tag = crate::netclient::PARTY_TAG.load(std::sync::atomic::Ordering::Relaxed);
        if tag != 0 {
            g.net.send(C2S::PartyTag(tag));
        }
        // the battle royale: straight into the lobby (watching is a button there); come
        // with a squad, straight onto its team
        if g.mode == Mode::Br || tag != 0 {
            g.join_team(None);
        }
        // «Play» in the main lobby: ready for the match already
        if crate::netclient::MATCHED.swap(false, std::sync::atomic::Ordering::Relaxed) {
            g.net.send(C2S::Ready(true));
        }
        g
    }

    pub fn push_chat(&mut self, text: String, color: Color) {
        self.chat.push(Line {
            text,
            color,
            time: crate::clock::now(),
        });
        if self.chat.len() > 60 {
            self.chat.remove(0);
        }
    }

    pub fn center_msg(&mut self, text: impl Into<String>, secs: f64, color: Color) {
        self.center = Some((text.into(), self.now + secs, color));
    }

    /// A bullet flying along `dir` struck the world at `at`: dust, chips and a mark, by
    /// what it hit.
    fn bullet_hit(&mut self, at: Vec3, dir: Vec3) {
        use crate::effects::Surface;
        let normal = self
            .world
            .raycast(at - dir * 0.3, dir, 0.6)
            .map_or(-dir, |h| h.1);
        // the bare ground of a big map is earth; everything else is hard
        let earth = self.map.terrain.is_some()
            && normal.y > 0.5
            && self
                .map
                .terrain
                .as_ref()
                .is_some_and(|t| (t.height(at.x, at.z) - at.y).abs() < 0.12);
        self.fx.hit(
            at,
            normal,
            if earth { Surface::Dirt } else { Surface::Hard },
        );
    }

    /// Sky, fog and light colours in the current weather.
    pub fn colours(&self) -> ([u8; 3], [u8; 3], [f32; 3]) {
        self.weather
            .colours(self.map.sky, self.map.fog, self.map.tint)
    }

    pub fn eye(&self) -> Vec3 {
        self.body.pos + vec3(0.0, self.body.stance.eye(), 0.0)
    }

    pub fn weapon(&self) -> WeaponId {
        self.weapons[self.slot as usize & 1].0
    }

    pub fn frozen(&self) -> bool {
        matches!(self.state, MatchState::PreRound | MatchState::PostMatch) || self.net_lost()
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
        self.ruins = crate::ruins::Ruins::new(&self.map);
        self.sky_mesh = build_sky(&self.map);
        self.sun_mesh = build_sun(Weather::Clear);
        self.weather = Weather::Clear;
        self.mode = mode;
        self.gs = gs;
        self.remotes.clear();
        self.alive = false;
        self.match_end = None;
        self.bomb = None;
        self.ents.clear();
        self.lying.clear();
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

    fn sprint_max(&self) -> f32 {
        self.tuning.movement.sprint_time
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

    /// Where somebody sits in a car (in the car as we draw it).
    fn seated_at(&self, id: u8) -> Option<(Vec3, f32)> {
        self.cars.values().find_map(|c| {
            c.seats
                .iter()
                .position(|s| *s == id)
                .map(|k| (c.seat_pos(k), c.state.yaw))
        })
    }

    /// Spawn protection active (and not given up yet).
    pub fn is_protected(&self) -> bool {
        self.alive && !self.protection_dropped && self.me.protected > 0.0
    }

    /// Where «Back» leads from the leaderboards, achievements and settings: the lobby
    /// if we came from there, otherwise the pause menu.
    pub fn menu_back(&self) -> Overlay {
        if self.in_br_lobby() {
            Overlay::None
        } else {
            Overlay::Pause
        }
    }

    /// On our side: our team, or in the battle royale our squad.
    /// The colours a team is shown in: ours always blue, the enemy always red, whichever
    /// side we play (so nobody sees the other side any better than the other). Spectators
    /// and the battle royale (its own looks) see the real ones.
    pub fn seen_team(&self, team: Team) -> Team {
        if self.mode.solo() || !self.team.is_playing() || !team.is_playing() {
            team
        } else if team == self.team {
            Team::Allies
        } else {
            Team::Axis
        }
    }

    /// A team's colour as we see it (see [`Self::seen_team`]).
    pub fn tcol(&self, team: Team) -> Color {
        crate::models::team_color(self.seen_team(team))
    }

    pub fn is_friend(&self, r: &Remote) -> bool {
        if self.mode.solo() {
            self.squad != 0 && r.squad == self.squad
        } else {
            r.team == self.team
        }
    }

    /// The car we sit in and our seat (seat 0 drives).
    pub fn seat(&self) -> Option<(&CarView, usize)> {
        if !self.alive {
            return None;
        }
        let (car, k) = self.me.seat?;
        self.cars.get(&car).map(|c| (c, k as usize))
    }

    /// Knocked down (battle royale squads): crawling, no shooting.
    pub fn downed(&self) -> bool {
        self.alive && self.me.downed
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
        // in a car: from behind and above it
        if let Some((c, _)) = self.seat() {
            let dir = look_dir(self.yaw, self.pitch.min(0.6));
            let mut cam = c.state.pos + vec3(0.0, 2.4, 0.0) - dir * 7.5;
            if let Some(g) = self.world.ground(cam.x, cam.z) {
                cam.y = cam.y.max(g + 0.8);
            }
            return (cam, self.yaw, self.pitch);
        }
        if self.alive {
            let s = self.sway_offset();
            // (a slight bob with each step, less when aiming)
            let bob = (self.bob_phase * 2.0).sin()
                * 0.014
                * self.walk_amount
                * (1.0 - self.ads_t)
                * self.tuning.movement.view_bob;
            // (the smoothing never takes the view below the knees: whatever glides, the
            // camera stays up out of the floor)
            let mut cam = self.eye() + self.cam_smooth + vec3(0.0, bob, 0.0);
            cam.y = cam.y.max(self.body.pos.y + PRONE_EYE * 0.8);
            return (cam, self.yaw + s.x + shake.x, self.pitch + s.y + shake.y);
        }
        if self.br_out() {
            if let Some(r) = self.spectate.and_then(|t| self.remotes.get(&t)) {
                let pos = self
                    .seated_at(self.spectate.unwrap_or(0))
                    .map(|s| s.0)
                    .unwrap_or(r.pos);
                return (pos + vec3(0.0, r.stance.eye(), 0.0), r.yaw, r.pitch);
            }
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

/// How high over its point a ping's badge floats (metres).
pub fn ping_lift(what: PingWhat) -> f32 {
    match what {
        PingWhat::Enemy => 2.2,
        PingWhat::Car(_) | PingWhat::AirDrop => 2.0,
        PingWhat::Spot | PingWhat::Danger => 0.0,
        _ => 0.7,
    }
}

/// What a ping says, in words.
pub fn ping_label(what: PingWhat) -> String {
    use toon_shared::lang::t;
    match what {
        PingWhat::Spot => t("Сюди", "Over here").into(),
        PingWhat::Danger => t("Тут ворог!", "Enemy here!").into(),
        PingWhat::Enemy => t("Ворог!", "Enemy!").into(),
        PingWhat::Loot(k, _) => k.name(),
        PingWhat::Weapon(w) => w.def().name().to_string(),
        PingWhat::DeathBox => t("Ящик із речами", "Death box").into(),
        PingWhat::AirDrop => t("Скидання припасів", "Supply drop").into(),
        PingWhat::Car(k) => k.name().into(),
    }
}

/// Eases 0..1 in and out.
fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}
