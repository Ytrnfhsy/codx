//! Server-side bots: perception, aiming, shooting and mode-aware goals.

use crate::game::Game;
use glam::{vec2, vec3, Vec2, Vec3};
use toon_shared::physics::{MoveInput, Stance};
use toon_shared::protocol::{flags, MatchState};
use toon_shared::rng::Rng;
use toon_shared::weapons::WeaponClass;
use toon_shared::{look_dir, wrap_angle, yaw_basis, yaw_of, Mode, Team};

pub const BOT_NAMES: [&str; 20] = [
    "Пиріжок", "Вареник", "Борщик", "Котик", "Бублик", "Сирник", "Галушка", "Кавун", "Пончик", "Їжак",
    "Хом'як", "Бобер", "Ковбаска", "Пельмень", "Шкварка", "Гарбуз", "Кабанчик", "Горішок", "Тарас", "Мурчик",
];

pub struct Bot {
    pub skill: f32,
    path: Vec<Vec3>,
    path_goal: Option<Vec3>,
    repath_at: f64,
    target: Option<u8>,
    seen_at: f64,
    last_seen: Option<(Vec3, f64)>,
    aim_err: Vec2,
    next_percept: f64,
    strafe: f32,
    strafe_until: f64,
    stuck_pos: Vec3,
    stuck_at: f64,
    stuck_count: u32,
    roam_goal: Option<Vec3>,
    roam_until: f64,
    burst_until: f64,
    pause_until: f64,
    next_semi: f64,
    nade_ready: f64,
    attacked_by: Option<(u8, Vec3, f64)>,
    detour_until: f64,
}

impl Bot {
    pub fn new(skill: f32, rng: &mut Rng) -> Bot {
        Bot {
            skill: (skill + rng.range(-0.15, 0.15)).clamp(0.0, 1.0),
            path: Vec::new(),
            path_goal: None,
            repath_at: 0.0,
            target: None,
            seen_at: 0.0,
            last_seen: None,
            aim_err: Vec2::ZERO,
            next_percept: 0.0,
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
        let eye = me.eye();
        let pos = me.body.pos;
        let mut yaw = me.yaw;
        let mut pitch = me.pitch;
        let weapon = me.weapon().id;
        let def = weapon.def();
        let mag = me.weapon().mag;
        let reserve = me.weapon().reserve;
        let reloading = me.reload_end.is_some();
        let grenades = me.grenades;
        let skill = bot.skill;

        // ---- perception
        if time >= bot.next_percept {
            bot.next_percept = time + 0.12;
            let fwd = look_dir(yaw, 0.0);
            let mut best: Option<(u8, f32)> = None;
            for o in self.players.iter().flatten() {
                if !o.alive || o.team == team || !o.team.is_playing() {
                    continue;
                }
                let c = o.chest();
                let to = c - eye;
                let d = to.length();
                if d > 85.0 {
                    continue;
                }
                let flat = vec3(to.x, 0.0, to.z).normalize_or_zero();
                let in_fov = flat.dot(fwd) > 0.25 || d < 7.0 || bot.attacked_by.map(|a| a.0 == o.id && time - a.2 < 2.5).unwrap_or(false);
                if !in_fov {
                    continue;
                }
                if !self.world.line_of_sight(eye, o.eye()) && !self.world.line_of_sight(eye, c) {
                    continue;
                }
                if best.map(|b| d < b.1).unwrap_or(true) {
                    best = Some((o.id, d));
                }
            }
            match best {
                Some((t, _)) => {
                    if bot.target != Some(t) {
                        bot.target = Some(t);
                        bot.seen_at = time;
                        let e = (1.0 - skill) * 0.14 + 0.025;
                        bot.aim_err = vec2(self.rng.range(-e, e), self.rng.range(-e, e) * 0.6);
                    }
                }
                None => bot.target = None,
            }
        }
        let target = bot.target.and_then(|t| self.p(t)).filter(|t| t.alive).map(|t| (t.id, t.chest(), t.eye(), t.body.pos));
        if target.is_none() {
            bot.target = None;
        }
        if let Some((_, _, _, tp)) = target {
            bot.last_seen = Some((tp, time));
        }

        // ---- aiming
        bot.aim_err *= 1.0 - (dt * (1.2 + 2.5 * skill)).min(1.0);
        let mut aim_err_angle = 10.0;
        let mut dist_to_target = 0.0;
        if let Some((_, chest, head, _)) = target {
            let aim = if skill > 0.7 { chest.lerp(head, 0.5) } else { chest };
            let d = aim - eye;
            dist_to_target = d.length();
            let want_yaw = yaw_of(d) + bot.aim_err.x;
            let want_pitch = (d.y / dist_to_target.max(0.01)).asin() + bot.aim_err.y;
            let turn = (2.5 + 7.0 * skill) * dt;
            let dy = wrap_angle(want_yaw - yaw);
            yaw = wrap_angle(yaw + dy.clamp(-turn, turn));
            pitch += (want_pitch - pitch).clamp(-turn, turn);
            aim_err_angle = (wrap_angle(yaw_of(d) - yaw)).abs() + (pitch - (d.y / dist_to_target.max(0.01)).asin()).abs();
        } else if let Some((_, from, t0)) = bot.attacked_by {
            if time - t0 < 2.0 {
                let d = from - eye;
                let dy = wrap_angle(yaw_of(d) - yaw);
                yaw = wrap_angle(yaw + dy.clamp(-6.0 * dt, 6.0 * dt));
            }
        }

        // ---- goal selection
        let (goal, mut use_key, urgent) = self.bot_goal(id, &mut bot, team, pos);
        let fighting = target.is_some() && !(use_key && self.p(id).map(|p| p.progress > 0.5).unwrap_or(false));
        if fighting {
            use_key = false;
        }

        // ---- shooting
        let mut ads = false;
        let mut fire_dirs: Option<Vec<Vec3>> = None;
        let reaction = (0.6 - 0.4 * skill) as f64;
        if let Some((_, _, _, _)) = target {
            ads = dist_to_target > 14.0 && def.class != WeaponClass::Shotgun;
            let tol = 0.05 + 0.8 / dist_to_target.max(1.0);
            let in_range = def.class != WeaponClass::Shotgun || dist_to_target < 20.0;
            if time - bot.seen_at > reaction && aim_err_angle < tol && mag > 0 && !reloading && in_range {
                let can = if def.auto {
                    if time > bot.burst_until && time > bot.pause_until {
                        bot.burst_until = time + self.rng.range(0.35, 0.9) as f64;
                        bot.pause_until = bot.burst_until + self.rng.range(0.15, 0.4) as f64;
                    }
                    time <= bot.burst_until
                } else if time >= bot.next_semi {
                    bot.next_semi = time + (def.fire_interval() * self.rng.range(1.15, 1.8)) as f64;
                    true
                } else {
                    false
                };
                if can {
                    let spread = if ads { def.ads_spread.max(0.25) } else { def.hip_spread * 0.85 };
                    let base = look_dir(yaw, pitch);
                    let dirs = (0..def.pellets.max(1)).map(|_| cone(&mut self.rng, base, spread.to_radians())).collect();
                    fire_dirs = Some(dirs);
                }
            }
            // grenade
            if grenades > 0 && time > bot.nade_ready && (9.0..24.0).contains(&dist_to_target) && self.rng.chance(0.015) {
                bot.nade_ready = time + 15.0;
                if let Some((_, chest, _, _)) = target {
                    let d = chest - eye;
                    let flat = vec3(d.x, 0.0, d.z);
                    let dir = (flat.normalize() + vec3(0.0, 0.25 + flat.length() * 0.012, 0.0)).normalize();
                    self.throw_grenade(id, eye, dir, 0.8);
                }
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
            if def.class == WeaponClass::Shotgun || def.class == WeaponClass::Smg {
                if dist_to_target > 6.0 {
                    move_dir += f * 0.8;
                }
            } else if dist_to_target < 5.0 {
                move_dir -= f * 0.6;
            }
            if matches!(def.class, WeaponClass::Lmg | WeaponClass::Sniper) && dist_to_target > 20.0 {
                stance = Stance::Crouch;
                move_dir *= 0.3;
            }
        } else if let Some(g) = goal {
            let flat_d = vec3(g.x - pos.x, 0.0, g.z - pos.z).length();
            let need_path = bot.path.is_empty()
                || bot.path_goal.map(|pg| pg.distance(g) > 2.5).unwrap_or(true)
                || time >= bot.repath_at;
            if need_path && flat_d > 1.2 {
                bot.path = self.nav.path(pos, g).unwrap_or_default();
                bot.path_goal = Some(g);
                bot.repath_at = time + 2.5;
            }
            while let Some(wp) = bot.path.first() {
                if vec3(wp.x - pos.x, 0.0, wp.z - pos.z).length() < 0.6 {
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
        };

        // apply
        let world = &self.world;
        let p = self.players[id as usize].as_mut().unwrap();
        p.yaw = yaw;
        p.pitch = pitch.clamp(-1.4, 1.4);
        p.body.step(world, &inp, dt);
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
    fn bot_goal(&mut self, id: u8, bot: &mut crate::bots::Bot, team: Team, pos: Vec3) -> (Option<Vec3>, bool, bool) {
        let time = self.time;
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
                    if s.owner.is_none() || s.owner == Some(team) || !id.is_multiple_of(3) {
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
        let reached = bot.roam_goal.map(|g| vec3(g.x - pos.x, 0.0, g.z - pos.z).length() < 2.0).unwrap_or(true);
        if reached || time > bot.roam_until {
            bot.roam_until = time + 18.0;
            let hunt = self.rng.chance(0.55);
            let enemy = if hunt {
                let enemies: Vec<Vec3> = self.players.iter().flatten().filter(|o| o.alive && o.team == team.enemy()).map(|o| o.body.pos).collect();
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
        let ok = bot.roam_goal.map(|g| g.distance(around) < r + 1.0).unwrap_or(false) && time < bot.roam_until;
        if !ok {
            bot.roam_until = time + 6.0;
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let rr = self.rng.range(1.5, r);
            let mut g = around + vec3(a.cos() * rr, 0.0, a.sin() * rr);
            let c = self.nav.cell_of(g);
            if let Some(w) = self.nav.nearest_walkable(c) {
                g = self.nav.center(w.0, w.1);
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
