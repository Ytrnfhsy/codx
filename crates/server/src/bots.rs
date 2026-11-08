//! Server-side bots: perception, aiming, shooting and mode-aware goals.

use crate::game::Game;
use glam::{vec2, vec3, Vec2, Vec3};
use toon_shared::physics::{MoveInput, Stance};
use toon_shared::protocol::{flags, MatchState};
use toon_shared::ray::smoke_blocks;
use toon_shared::rng::Rng;
use toon_shared::weapons::{Perk3, WeaponClass};
use toon_shared::{look_dir, wrap_angle, yaw_basis, yaw_of, Mode, Team};

pub const BOT_NAMES: [&str; 20] = [
    "Пиріжок",
    "Вареник",
    "Борщик",
    "Котик",
    "Бублик",
    "Сирник",
    "Галушка",
    "Кавун",
    "Пончик",
    "Їжак",
    "Хом'як",
    "Бобер",
    "Ковбаска",
    "Пельмень",
    "Шкварка",
    "Гарбуз",
    "Кабанчик",
    "Горішок",
    "Тарас",
    "Мурчик",
];

/// Everything that depends on difficulty, derived from a single skill value 0..1.
#[derive(Clone, Copy, Debug)]
pub struct SkillProfile {
    /// Seconds between noticing an enemy and opening fire.
    pub reaction: f32,
    /// Base seconds to notice a fully visible enemy right in front, plus per metre.
    pub notice_base: f32,
    pub notice_per_m: f32,
    /// cos of the half field of view.
    pub fov_cos: f32,
    pub max_sight: f32,
    /// Radius in which gunfire is heard.
    pub hearing: f32,
    /// Radius in which footsteps are heard.
    pub footsteps: f32,
    /// Initial aim error (rad) when acquiring a target.
    pub aim_initial: f32,
    /// How fast the initial error settles (1/s).
    pub aim_settle: f32,
    /// Permanent hand wobble (rad) that never goes away.
    pub aim_wobble: f32,
    /// Bots aim where a moving target was this many seconds ago.
    pub track_lag: f32,
    /// Fires once the aim is within this angle (rad) — sloppy bots shoot too early.
    pub fire_tolerance: f32,
    /// Max turn speed (rad/s).
    pub turn_speed: f32,
    /// Uses ADS beyond this distance.
    pub ads_distance: f32,
    /// 0 = aims at the chest, 1 = at the head.
    pub head_bias: f32,
    /// Seconds a lost target is remembered.
    pub memory: f32,
    /// How well recoil is pulled down (0..1).
    pub recoil_control: f32,
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Geometric interpolation: every step of skill multiplies precision by the same factor.
fn glerp(a: f32, b: f32, t: f32) -> f32 {
    a * (b / a).powf(t)
}

impl SkillProfile {
    pub fn new(s: f32) -> SkillProfile {
        let s = s.clamp(0.0, 1.0);
        // aiming improves slowly at first: "normal" bots still miss a fair bit,
        // only hard/veteran bots are really precise
        let a = s;
        SkillProfile {
            reaction: lerp(0.95, 0.2, s),
            notice_base: lerp(0.9, 0.12, s),
            notice_per_m: lerp(0.045, 0.006, s),
            fov_cos: lerp(50f32, 75.0, s).to_radians().cos(),
            max_sight: lerp(45.0, 95.0, s),
            hearing: lerp(14.0, 38.0, s),
            footsteps: lerp(3.0, 9.0, s),
            aim_initial: glerp(0.4, 0.05, a),
            aim_settle: lerp(0.7, 4.5, a),
            aim_wobble: glerp(0.22, 0.006, a),
            track_lag: glerp(0.2, 0.03, a),
            fire_tolerance: lerp(0.22, 0.035, a),
            turn_speed: lerp(2.0, 9.0, s),
            ads_distance: lerp(30.0, 8.0, s),
            head_bias: lerp(0.0, 0.35, s * s),
            memory: lerp(1.5, 4.5, s),
            recoil_control: lerp(0.1, 0.85, a),
        }
    }
}

/// Parses a difficulty: number 0..1 or a preset name.
pub fn parse_skill(v: &str) -> Option<f32> {
    match v.to_lowercase().as_str() {
        "easy" | "легко" | "легкий" => Some(0.1),
        "normal" | "нормально" | "середній" => Some(0.4),
        "hard" | "важко" | "важкий" => Some(0.7),
        "veteran" | "ветеран" => Some(1.0),
        n => n
            .replace(',', ".")
            .parse::<f32>()
            .ok()
            .filter(|x| x.is_finite())
            .map(|x| x.clamp(0.0, 1.0)),
    }
}

pub fn skill_name(s: f32) -> &'static str {
    match s {
        x if x < 0.25 => toon_shared::lang::t("легкі", "easy"),
        x if x < 0.55 => toon_shared::lang::t("звичайні", "normal"),
        x if x < 0.85 => toon_shared::lang::t("важкі", "hard"),
        _ => toon_shared::lang::t("ветерани", "veteran"),
    }
}

pub struct Bot {
    /// Personal deviation from the server-wide skill.
    pub skill_offset: f32,
    /// Battle royale: when to jump out of the plane.
    pub br_jump_at: f64,
    /// How close (0..1) the bot is to noticing each enemy.
    awareness: Vec<(u8, f32)>,
    last_percept: f64,
    /// Something heard/felt outside the view: turn there.
    alert: Option<(Vec3, f64)>,
    wobble_phase: f32,
    shot_streak: u32,
    last_fire: f64,
    was_ads: bool,
    path: Vec<Vec3>,
    path_goal: Option<Vec3>,
    repath_at: f64,
    /// The last search for a route failed: do not search again before this.
    path_fail_until: f64,
    target: Option<u8>,
    seen_at: f64,
    last_seen: Option<(Vec3, f64)>,
    aim_err: Vec2,
    strafe: f32,
    strafe_until: f64,
    stuck_pos: Vec3,
    stuck_at: f64,
    stuck_count: u32,
    pub(crate) roam_goal: Option<Vec3>,
    pub(crate) roam_until: f64,
    burst_until: f64,
    pause_until: f64,
    next_semi: f64,
    nade_ready: f64,
    attacked_by: Option<(u8, Vec3, f64)>,
    detour_until: f64,
}

impl Bot {
    /// What the bot is up to, for tests and logs.
    pub fn describe(&self) -> String {
        format!(
            "target {:?}, roam {:?}, path {} pts (goal {:?}, fail until {:.1}), last seen {:?}, attacked {:?}",
            self.target,
            self.roam_goal,
            self.path.len(),
            self.path_goal,
            self.path_fail_until,
            self.last_seen,
            self.attacked_by
        )
    }

    pub fn new(rng: &mut Rng) -> Bot {
        Bot {
            skill_offset: rng.range(-0.1, 0.1),
            awareness: Vec::new(),
            last_percept: 0.0,
            alert: None,
            wobble_phase: rng.range(0.0, 100.0),
            shot_streak: 0,
            last_fire: -10.0,
            was_ads: false,
            path: Vec::new(),
            path_goal: None,
            repath_at: 0.0,
            path_fail_until: 0.0,
            br_jump_at: 0.0,
            target: None,
            seen_at: 0.0,
            last_seen: None,
            aim_err: Vec2::ZERO,
            strafe: 1.0,
            strafe_until: 0.0,
            stuck_pos: Vec3::ZERO,
            stuck_at: 0.0,
            stuck_count: 0,
            roam_goal: None,
            roam_until: 0.0,
            burst_until: 0.0,
            pause_until: 0.0,
            next_semi: 0.0,
            nade_ready: 0.0,
            attacked_by: None,
            detour_until: 0.0,
        }
    }

    pub fn on_spawn(&mut self) {
        self.path.clear();
        self.path_goal = None;
        self.target = None;
        self.last_seen = None;
        self.roam_goal = None;
        self.attacked_by = None;
        self.stuck_count = 0;
        self.awareness.clear();
        self.alert = None;
    }

    /// Current target (for tests / debugging).
    pub fn target(&self) -> Option<u8> {
        self.target
    }

    pub fn on_damaged(&mut self, attacker: u8, from: Vec3) {
        self.attacked_by = Some((attacker, from, 0.0));
    }
}

impl Game {
    pub fn update_bot(&mut self, id: u8, dt: f32) {
        let time = self.time;
        let active = matches!(self.state, MatchState::Warmup | MatchState::Playing);
        let Some(p) = self.pm(id) else { return };
        if !p.alive {
            return;
        }
        let Some(mut bot) = p.bot.take() else { return };
        if !active {
            p.flags = flags::ON_GROUND;
            p.bot = Some(bot);
            return;
        }
        if let Some(a) = bot.attacked_by.as_mut() {
            if a.2 == 0.0 {
                a.2 = time;
            }
        }

        let me = self.p(id).unwrap();
        let team = me.team;
        let skydiving = me.body.chute != toon_shared::physics::Chute::None;
        let eye = me.eye();
        let pos = me.body.pos;
        let mut yaw = me.yaw;
        let mut pitch = me.pitch;
        let weapon = me.weapon().id;
        let class = weapon.def().class;
        let def = self.tuning.weapon(weapon).clone();
        let mag = me.weapon().mag;
        let reserve = me.weapon().reserve;
        let reloading = me.reload_end.is_some();
        let grenades = me.grenades;
        let smokes = me.smokes;
        let team_now = me.team;
        let vel = me.body.vel;
        let protected_me = me.protected_until > time;
        let skill = (self.cfg.bot_skill + bot.skill_offset).clamp(0.0, 1.0);
        let sk = SkillProfile::new(skill);
        let walk_speed = self.tuning.movement.walk_speed;

        let clouds = self.smoke_clouds();
        // ---- perception: enemies are noticed gradually, only inside the field of view
        if time - bot.last_percept >= 0.1 {
            let pdt = (time - bot.last_percept).min(0.5) as f32;
            bot.last_percept = time;
            let fwd = look_dir(yaw, 0.0);
            let mut best: Option<(u8, f32)> = None;
            let mut heard: Option<(Vec3, f32)> = None;
            let solo = self.mode.solo();
            for o in self.players.iter().flatten() {
                if !o.alive || o.id == id || o.team == team && !solo || !o.team.is_playing() {
                    continue;
                }
                let c = o.chest();
                let to = c - eye;
                let d = to.length();
                let flat = vec3(to.x, 0.0, to.z).normalize_or_zero();
                let facing = flat.dot(fwd);
                let shooting = time - o.last_shot < 0.6;
                let hit_me = bot
                    .attacked_by
                    .map(|a| a.0 == o.id && time - a.2 < 2.0)
                    .unwrap_or(false);
                let in_view = d < sk.max_sight && (facing > sk.fov_cos || d < 2.5);
                let los = in_view
                    && ((self.world.line_of_sight(eye, o.eye())
                        && !smoke_blocks(eye, o.eye(), &clouds))
                        || (self.world.line_of_sight(eye, c) && !smoke_blocks(eye, c, &clouds)));
                // sounds outside the view make the bot turn around
                let moving = vec2(o.body.vel.x, o.body.vel.z).length() > 2.0
                    && o.body.stance == Stance::Stand;
                let audible = (shooting && d < sk.hearing)
                    || (moving && !o.has_perk3(Perk3::DeadSilence) && d < sk.footsteps)
                    || hit_me;
                if !los && audible && heard.map(|h| d < h.1).unwrap_or(true) {
                    heard = Some((o.body.pos, d));
                }
                let entry = match bot.awareness.iter().position(|e| e.0 == o.id) {
                    Some(i) => i,
                    None => {
                        bot.awareness.push((o.id, 0.0));
                        bot.awareness.len() - 1
                    }
                };
                let aw = &mut bot.awareness[entry].1;
                if los {
                    // farther, on the edge of the view, crouched/prone or still = harder to spot
                    let edge = ((1.0 - facing) / (1.0 - sk.fov_cos).max(0.05)).clamp(0.0, 1.0);
                    let mut t = (sk.notice_base + d * sk.notice_per_m) * (1.0 + 1.5 * edge);
                    t *= match o.body.stance {
                        Stance::Stand => 1.0,
                        Stance::Crouch => 1.3,
                        Stance::Prone => 1.8,
                    };
                    if vec2(o.body.vel.x, o.body.vel.z).length() < 0.5 {
                        t *= 1.25;
                    }
                    if shooting || hit_me {
                        t *= 0.4;
                    }
                    *aw = (*aw + pdt / t.max(0.05)).min(1.5);
                } else {
                    *aw = (*aw - pdt / sk.memory).max(0.0);
                }
                // freshly respawned (protected) players are not engaged
                let protected = o.protected_until > time;
                if los && *aw >= 1.0 && !protected && best.map(|b| d < b.1).unwrap_or(true) {
                    best = Some((o.id, d));
                }
            }
            bot.awareness
                .retain(|e| e.1 > 0.0 || Some(e.0) == bot.target);
            if let Some((p, _)) = heard {
                bot.alert = Some((p, time));
            }
            match best {
                Some((t, _)) => {
                    if bot.target != Some(t) {
                        bot.target = Some(t);
                        bot.seen_at = time + self.rng.range(-0.1, 0.25) as f64;
                        let a = self.rng.range(0.0, std::f32::consts::TAU);
                        let m = sk.aim_initial * self.rng.range(0.6, 1.0);
                        bot.aim_err = vec2(a.cos() * m, a.sin() * m * 0.6);
                        bot.shot_streak = 0;
                    }
                }
                None => bot.target = None,
            }
        }
        let target = bot
            .target
            .and_then(|t| self.p(t))
            .filter(|t| t.alive)
            .map(|t| (t.id, t.chest(), t.eye(), t.body.pos, t.body.vel));
        if target.is_none() {
            bot.target = None;
        }
        if let Some((_, _, _, tp, _)) = target {
            bot.last_seen = Some((tp, time));
        }

        // ---- aiming: the initial error settles into a permanent wobble; moving targets are tracked late
        bot.wobble_phase += dt;
        let w = bot.wobble_phase;
        // aiming down sights steadies the hands
        let steady = if bot.was_ads { 0.55 } else { 1.0 };
        let wobble = vec2(
            (w * 1.3).sin() + (w * 3.1).sin() * 0.5,
            (w * 1.7).cos() * 0.8 + (w * 2.6).sin() * 0.3,
        ) * sk.aim_wobble
            * steady;
        bot.aim_err = bot.aim_err.lerp(wobble, (sk.aim_settle * dt).min(1.0));
        let mut aim_err_angle = 10.0;
        let mut dist_to_target = 0.0;
        if let Some((_, chest, head, _, tvel)) = target {
            let aim = chest.lerp(head, sk.head_bias) - tvel * sk.track_lag;
            let d = aim - eye;
            dist_to_target = d.length();
            let want_yaw = yaw_of(d) + bot.aim_err.x;
            let want_pitch = (d.y / dist_to_target.max(0.01)).asin() + bot.aim_err.y;
            let turn = sk.turn_speed * dt;
            let dy = wrap_angle(want_yaw - yaw);
            yaw = wrap_angle(yaw + dy.clamp(-turn, turn));
            // recoil climb is pulled down only partially by weaker bots
            let pull = turn * (0.3 + 0.7 * sk.recoil_control);
            let dp = want_pitch - pitch;
            pitch += if dp < 0.0 {
                dp.max(-pull)
            } else {
                dp.min(turn)
            };
            // the bot fires when it is on the point it *believes* is right; its aim error
            // then turns into honest misses instead of waiting for a lucky alignment
            aim_err_angle = wrap_angle(want_yaw - yaw).abs() + (want_pitch - pitch).abs();
        } else {
            let look_at = bot
                .attacked_by
                .filter(|a| time - a.2 < 2.0)
                .map(|a| a.1)
                .or(bot.alert.filter(|a| time - a.1 < 1.5).map(|a| a.0));
            if let Some(from) = look_at {
                let d = from - eye;
                let dy = wrap_angle(yaw_of(d) - yaw);
                let turn = sk.turn_speed * 0.7 * dt;
                yaw = wrap_angle(yaw + dy.clamp(-turn, turn));
            }
        }

        // ---- goal selection
        let (goal, mut use_key, urgent) = self.bot_goal(id, &mut bot, team, pos);
        if target.is_none() {
            bot.was_ads = false;
        }
        let fighting =
            target.is_some() && !(use_key && self.p(id).map(|p| p.progress > 0.5).unwrap_or(false));
        if fighting {
            use_key = false;
        }

        // ---- shooting (with real spread and recoil, like players)
        let mut ads = false;
        let mut fire_dirs: Option<Vec<Vec3>> = None;
        if target.is_some() {
            ads = dist_to_target > sk.ads_distance && class != WeaponClass::Shotgun;
            bot.was_ads = ads;
            let tol = sk.fire_tolerance + 0.5 / dist_to_target.max(1.0);
            let in_range = class != WeaponClass::Shotgun || dist_to_target < 20.0;
            let reacted = time - bot.seen_at > sk.reaction as f64;
            if reacted && protected_me {
                // like a player: give up spawn protection to start fighting
                if let Some(p) = self.pm(id) {
                    p.protected_until = 0.0;
                    p.switch_end = p.switch_end.max(time + 0.2);
                }
            }
            if reacted && !protected_me && aim_err_angle < tol && mag > 0 && !reloading && in_range
            {
                let can = if def.auto {
                    if time > bot.burst_until && time > bot.pause_until {
                        // weak bots spray long bursts, good ones tap
                        let len = lerp(1.1, 0.45, skill) * self.rng.range(0.6, 1.2);
                        bot.burst_until = time + len as f64;
                        bot.pause_until = bot.burst_until
                            + lerp(0.2, 0.35, skill) as f64 * self.rng.range(0.7, 1.4) as f64;
                    }
                    time <= bot.burst_until
                } else if time >= bot.next_semi {
                    bot.next_semi = time
                        + (def.fire_interval() * self.rng.range(1.15, lerp(2.6, 1.5, skill)))
                            as f64;
                    true
                } else {
                    false
                };
                if can && time >= bot.last_fire + def.fire_interval() as f64 {
                    let speed = vec2(vel.x, vel.z).length();
                    let hip = def.hip_spread;
                    let mut spread = if ads { def.ads_spread } else { hip };
                    spread += (speed / walk_speed).min(1.3)
                        * def.move_spread
                        * if ads { 0.2 } else { 1.0 };
                    let base = look_dir(yaw, pitch);
                    let dirs = (0..def.pellets.max(1))
                        .map(|_| cone(&mut self.rng, base, spread.to_radians()))
                        .collect();
                    fire_dirs = Some(dirs);
                    if time - bot.last_fire > (def.fire_interval() * 2.5 + 0.05) as f64 {
                        bot.shot_streak = 0;
                    }
                    bot.last_fire = time;
                    // recoil kicks the bot's aim up, exactly like a player
                    let ramp = def.recoil_scale(bot.shot_streak);
                    bot.shot_streak += 1;
                    let ads_k = if ads { 0.55 } else { 1.0 };
                    pitch += (def.recoil_v * 0.55 * ads_k * ramp * self.rng.range(0.8, 1.2))
                        .to_radians();
                    yaw += (def.recoil_h * 0.5 * ads_k * self.rng.range(-1.0, 1.0)).to_radians();
                }
            }
            // grenade
            let gt = &self.tuning.grenade;
            let (g_speed, g_reach) = (
                gt.throw_speed,
                gt.max_range(self.tuning.movement.gravity) * 0.9,
            );
            if !protected_me
                && grenades > 0
                && time > bot.nade_ready
                && dist_to_target > 7.0
                && dist_to_target < g_reach
                && self.rng.chance(lerp(0.004, 0.015, skill))
            {
                bot.nade_ready = time + 15.0;
                if let Some((_, chest, _, _, _)) = target {
                    // low ballistic arc for the configured throw speed: R = v² sin(2θ) / g
                    let d = chest - eye;
                    let flat = vec3(d.x, 0.0, d.z);
                    let s = (self.tuning.movement.gravity * flat.length() / (g_speed * g_speed))
                        .min(1.0);
                    let theta = 0.5 * s.asin();
                    let dir = flat.normalize() * theta.cos() + vec3(0.0, theta.sin(), 0.0);
                    let dir = cone(&mut self.rng, dir, lerp(0.15, 0.03, skill));
                    self.throw_grenade(id, eye, dir, 0.8, false);
                }
            }
        }

        // the bomb carrier covers the last stretch to the enemy site with smoke
        if self.mode == Mode::Sab && self.sab.carrier == Some(id) && smokes > 0 && !protected_me {
            let site = self.map.sites[team_now.enemy().index()];
            let to = site - pos;
            let d = vec3(to.x, 0.0, to.z).length();
            if d > 7.0 && d < 24.0 && self.rng.chance(0.03) {
                let flat = vec3(to.x, 0.0, to.z).normalize_or_zero();
                // drop it about halfway, on a low arc
                let dir = (flat + vec3(0.0, 0.35, 0.0)).normalize();
                self.throw_grenade(id, eye, dir, 0.0, true);
            }
        }

        // ---- movement
        let mut move_dir = Vec3::ZERO;
        let mut sprint = false;
        let mut jump = false;
        let mut stance = Stance::Stand;
        if fighting && !urgent {
            if time > bot.strafe_until {
                bot.strafe_until = time + self.rng.range(0.6, 1.6) as f64;
                bot.strafe = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
            }
            let (f, r) = yaw_basis(yaw);
            move_dir = r * bot.strafe;
            if class == WeaponClass::Shotgun || class == WeaponClass::Smg {
                if dist_to_target > 6.0 {
                    move_dir += f * 0.8;
                }
            } else if dist_to_target < 5.0 {
                move_dir -= f * 0.6;
            }
            if matches!(class, WeaponClass::Lmg | WeaponClass::Sniper) && dist_to_target > 20.0 {
                stance = Stance::Crouch;
                move_dir *= 0.3;
            }
        } else if let Some(g) = goal.filter(|_| skydiving) {
            // falling from the plane: steer straight for where we want to land
            move_dir = vec3(g.x - pos.x, 0.0, g.z - pos.z).normalize_or_zero();
        } else if let Some(g) = goal {
            let flat_d = vec3(g.x - pos.x, 0.0, g.z - pos.z).length();
            // far goals may drift a little before the route is worth planning again
            let slack = (flat_d * 0.08).max(2.5);
            let need_path = bot.path.is_empty()
                || bot
                    .path_goal
                    .map(|pg| pg.distance(g) > slack)
                    .unwrap_or(true)
                || time >= bot.repath_at;
            // a few route searches per tick at most, so that fifty bots never plan all at once
            if need_path && flat_d > 1.2 && time >= bot.path_fail_until && self.path_budget > 0 {
                self.path_budget -= 1;
                bot.path_goal = Some(g);
                bot.repath_at =
                    time + 2.5 + (flat_d as f64 / 60.0).min(5.0) + self.rng.range(0.0, 1.5) as f64;
                match self.nav.path(pos, g) {
                    Some(p) => bot.path = p,
                    None => {
                        // no way there: rest a moment and pick somewhere else to go
                        bot.path.clear();
                        bot.path_fail_until = time + 1.5;
                        bot.roam_until = time;
                    }
                }
            }
            while let Some(wp) = bot.path.first() {
                // reached when close horizontally and on (roughly) the same floor
                if vec3(wp.x - pos.x, 0.0, wp.z - pos.z).length() < 0.55
                    && (wp.y - pos.y).abs() < 1.5
                {
                    bot.path.remove(0);
                } else {
                    break;
                }
            }
            if let Some(wp) = bot.path.first() {
                move_dir = vec3(wp.x - pos.x, 0.0, wp.z - pos.z).normalize_or_zero();
            } else if flat_d > 0.8 {
                move_dir = vec3(g.x - pos.x, 0.0, g.z - pos.z).normalize_or_zero();
            }
            sprint = flat_d > 8.0 && !use_key && !fighting;
            if fighting {
                // run-and-gun towards the objective
                if time > bot.strafe_until {
                    bot.strafe_until = time + self.rng.range(0.5, 1.2) as f64;
                    bot.strafe = if self.rng.chance(0.5) { 1.0 } else { -1.0 };
                }
                let side = vec3(-move_dir.z, 0.0, move_dir.x) * bot.strafe * 0.35;
                move_dir = (move_dir + side).normalize_or_zero();
            }
            if use_key {
                move_dir = Vec3::ZERO;
                stance = Stance::Crouch;
            }
            // look where we walk
            if target.is_none() && move_dir.length_squared() > 0.01 {
                let want = yaw_of(move_dir);
                let dy = wrap_angle(want - yaw);
                yaw = wrap_angle(yaw + dy.clamp(-5.0 * dt, 5.0 * dt));
                pitch *= 1.0 - (3.0 * dt).min(1.0);
            }
        }
        if time < bot.detour_until {
            move_dir = (move_dir + vec3(bot.strafe, 0.0, -bot.strafe) * 0.8).normalize_or_zero();
        }

        // stuck detection
        if time - bot.stuck_at > 1.0 {
            if move_dir.length_squared() > 0.1 && pos.distance(bot.stuck_pos) < 0.4 {
                bot.stuck_count += 1;
                jump = true;
                if bot.stuck_count >= 3 {
                    bot.path.clear();
                    bot.detour_until = time + 1.2;
                    bot.strafe = -bot.strafe;
                    bot.stuck_count = 0;
                }
            } else {
                bot.stuck_count = 0;
            }
            bot.stuck_pos = pos;
            bot.stuck_at = time;
        }

        let (f, r) = yaw_basis(yaw);
        let inp = MoveInput {
            forward: move_dir.dot(f),
            right: move_dir.dot(r),
            yaw,
            jump,
            sprint,
            ads,
            want_stance: stance,
            speed_mult: def.move_mult,
            sprint_mult: def.sprint_mult,
            weight: def.weight,
        };

        // apply
        let world = &self.world;
        let mt = &self.tuning.movement;
        let p = self.players[id as usize].as_mut().unwrap();
        p.yaw = yaw;
        p.pitch = pitch.clamp(-1.4, 1.4);
        p.body.step(world, &inp, mt, dt);
        let mut fl = 0;
        if p.body.on_ground {
            fl |= flags::ON_GROUND;
        }
        if ads {
            fl |= flags::ADS;
        }
        if sprint && inp.forward > 0.5 {
            fl |= flags::SPRINT;
        }
        if use_key {
            fl |= flags::USE;
        }
        p.flags = fl;
        p.bot = Some(bot);

        if let Some(dirs) = fire_dirs {
            let eye = self.p(id).unwrap().eye();
            self.fire(id, weapon, eye, &dirs);
        }
        let need_reload = mag == 0 || (target.is_none() && (mag as f32) < def.mag as f32 * 0.4);
        if need_reload && reserve > 0 && !reloading {
            self.start_reload(id);
        }
    }

    /// Where the bot wants to go, whether to hold "use", and whether the objective is urgent
    /// enough to keep moving while under fire.
    fn bot_goal(
        &mut self,
        id: u8,
        bot: &mut crate::bots::Bot,
        team: Team,
        pos: Vec3,
    ) -> (Option<Vec3>, bool, bool) {
        let time = self.time;
        if self.mode == Mode::Br && self.state == MatchState::Playing && self.br.running {
            return self.br_bot_goal(id, bot, pos);
        }
        if self.mode == Mode::Sab && self.state == MatchState::Playing {
            let s = &self.sab;
            if let Some(site) = s.planted_at {
                if site == team {
                    let near = pos.distance(s.pos) < 1.4;
                    return (Some(s.pos), near, true);
                }
                // guard our planted bomb
                return (Some(self.guard_point(bot, s.pos, 7.0)), false, false);
            }
            match s.carrier {
                Some(c) if c == id => {
                    let site = self.map.sites[team.enemy().index()];
                    let near = pos.distance(site) < 1.6;
                    return (Some(site), near, true);
                }
                Some(c) => {
                    let cpos = self.p(c).map(|p| p.body.pos).unwrap_or(s.pos);
                    let carrier_team = self.p(c).map(|p| p.team);
                    if carrier_team == Some(team) {
                        let site = self.map.sites[team.enemy().index()];
                        let g = if id.is_multiple_of(2) { cpos } else { site };
                        return (Some(self.guard_point(bot, g, 4.0)), false, false);
                    }
                    return (Some(cpos), false, false);
                }
                None => {
                    // a loose bomb is anyone's: most bots race for it, a few hold home
                    if s.owner.is_none() || !id.is_multiple_of(3) {
                        let close = pos.distance(s.pos) < 20.0;
                        return (Some(s.pos), false, close);
                    }
                    let mine = self.map.sites[team.index()];
                    return (Some(self.guard_point(bot, mine, 6.0)), false, false);
                }
            }
        }
        // deathmatch-style hunting / roaming
        if let Some((p, t)) = bot.last_seen {
            if time - t < 4.0 && pos.distance(p) > 2.0 {
                return (Some(p), false, false);
            }
        }
        if let Some((_, from, t)) = bot.attacked_by {
            if time - t < 3.0 {
                return (Some(from), false, false);
            }
        }
        let reached = bot
            .roam_goal
            .map(|g| vec3(g.x - pos.x, 0.0, g.z - pos.z).length() < 2.0)
            .unwrap_or(true);
        if reached || time > bot.roam_until {
            bot.roam_until = time + 18.0;
            let hunt = self.rng.chance(0.55);
            let enemy = if hunt {
                let enemies: Vec<Vec3> = self
                    .players
                    .iter()
                    .flatten()
                    .filter(|o| {
                        o.alive
                            && o.id != id
                            && (o.team == team.enemy() || self.mode.solo() && o.team.is_playing())
                    })
                    .map(|o| o.body.pos)
                    .collect();
                if enemies.is_empty() {
                    None
                } else {
                    Some(enemies[self.rng.below(enemies.len())])
                }
            } else {
                None
            };
            bot.roam_goal = Some(enemy.unwrap_or_else(|| self.nav.random_point(&mut self.rng)));
        }
        (bot.roam_goal, false, false)
    }

    fn guard_point(&mut self, bot: &mut crate::bots::Bot, around: Vec3, r: f32) -> Vec3 {
        let time = self.time;
        let ok = bot
            .roam_goal
            .map(|g| g.distance(around) < r + 1.0)
            .unwrap_or(false)
            && time < bot.roam_until;
        if !ok {
            bot.roam_until = time + 6.0;
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let rr = self.rng.range(1.5, r);
            let mut g = around + vec3(a.cos() * rr, 0.0, a.sin() * rr);
            if let Some(w) = self.nav.snap(g) {
                g = w;
            }
            bot.roam_goal = Some(g);
        }
        bot.roam_goal.unwrap()
    }
}

/// Random direction inside a cone of half-angle `a` around `dir`.
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
