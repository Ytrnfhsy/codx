//! Shooting, damage, deaths, grenades, melee and killstreaks.

use crate::game::{Dest, Game, Grenade, Heli, PendingBlast};
use glam::{vec3, Vec3};
use toon_shared::consts::*;
use toon_shared::protocol::{MatchState, ShotFx, S2C};
use toon_shared::ray::{ray_player, ray_sphere, HitZone};
use toon_shared::weapons::{Cause, Killstreak, Perk2, Perk3, WeaponId};
use toon_shared::{look_dir, Mode, Team};

/// Result of tracing one bullet.
pub enum Hit {
    None,
    Player(u8, f32, HitZone),
    Heli(usize, f32),
    Vehicle(usize, f32),
}

impl Game {
    fn combat_allowed(&self) -> bool {
        matches!(self.state, MatchState::Warmup | MatchState::Playing)
    }

    /// Traces a bullet against the world, players (rewound to `t`) and helicopters.
    pub fn trace(&self, shooter: Option<u8>, o: Vec3, d: Vec3, max: f32, t: f64) -> (f32, Hit) {
        let mut best = self.world.raycast(o, d, max).map(|(t, _)| t).unwrap_or(max);
        let mut hit = Hit::None;
        for p in self.players.iter().flatten() {
            if !p.alive || Some(p.id) == shooter {
                continue;
            }
            let (pos, yaw, stance) = p.rewound(t);
            if let Some((dist, zone)) = ray_player(o, d, pos, yaw, stance, best) {
                if dist < best {
                    best = dist;
                    hit = Hit::Player(p.id, dist, zone);
                }
            }
        }
        for (i, h) in self.helis.iter().enumerate() {
            if let Some(dist) = ray_sphere(o, d, h.pos, 2.6) {
                if dist < best {
                    best = dist;
                    hit = Hit::Heli(i, dist);
                }
            }
        }
        for (i, v) in self.vehicles.iter().enumerate() {
            let (c, r) = v.sphere();
            if let Some(dist) = ray_sphere(o, d, c, r) {
                if dist < best {
                    best = dist;
                    hit = Hit::Vehicle(i, dist);
                }
            }
        }
        (best, hit)
    }

    pub fn fire(&mut self, id: u8, weapon: WeaponId, origin: Vec3, dirs: &[Vec3]) {
        if !self.combat_allowed() {
            return;
        }
        let time = self.time;
        let Some(cur) = self.p(id).map(|p| p.weapon().id) else {
            return;
        };
        let cur_mag = self.tuning.weapon(cur).mag;
        let def = self.tuning.weapon(weapon).clone();
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        // The client finishes reloads/switches by its own clock, which runs ahead of ours by
        // roughly the one-way latency; accept shots that arrive slightly "early".
        let slack = (p.ping * 0.5 + 0.12).min(0.35) as f64;
        if let Some(end) = p.reload_end {
            if time >= end - slack {
                let w = p.weapon_mut();
                let take = cur_mag.saturating_sub(w.mag).min(w.reserve);
                w.mag += take;
                w.reserve -= take;
                p.reload_end = None;
            } else {
                return;
            }
        }
        if !p.alive || p.weapon().id != weapon || p.weapon().mag == 0 || time < p.switch_end - slack
        {
            return;
        }
        if p.shot_budget < 0.8 {
            return;
        }
        // invulnerable players can't attack (they must drop the protection first)
        if time < p.protected_until {
            return;
        }
        // weapons need a moment to come up after sprinting (heavier ones longer)
        if time - p.last_sprint < (def.sprint_out_time * 0.5) as f64 {
            return;
        }
        p.shot_budget -= 1.0;
        p.weapon_mut().mag -= 1;
        p.last_shot = time;
        let eye = p.eye();
        let look = look_dir(p.yaw, p.pitch);
        let origin = if origin.is_finite() && origin.distance(eye) < 1.2 {
            origin
        } else {
            eye
        };
        let max_angle =
            (def.hip_spread.max(def.ads_spread) * 2.0 + def.move_spread + 12.0).to_radians();
        let rewind = (p.ping * 0.5 + INTERP_DELAY).clamp(0.0, MAX_REWIND) as f64;
        let stopping = p.has_perk2(Perk2::StoppingPower);
        let team = p.team;
        let t = time - rewind;

        let mut per_victim: Vec<(u8, f32, bool)> = Vec::new();
        let mut heli_dmg: Vec<(usize, f32)> = Vec::new();
        let mut vehicle_dmg: Vec<(usize, f32)> = Vec::new();
        for (k, d) in dirs.iter().take(def.pellets.max(1) as usize).enumerate() {
            if !d.is_finite() || d.length_squared() < 0.5 {
                continue;
            }
            let d = d.normalize();
            if d.dot(look).clamp(-1.0, 1.0).acos() > max_angle {
                continue;
            }
            let (dist, hit) = self.trace(Some(id), origin, d, 400.0, t);
            let mut flesh = false;
            match hit {
                Hit::Player(v, dist, zone) => {
                    let mut dmg = def.damage_at(dist) * zone.mult(def.head_mult);
                    if stopping {
                        dmg *= 1.4;
                    }
                    flesh = true;
                    match per_victim.iter_mut().find(|e| e.0 == v) {
                        Some(e) => {
                            e.1 += dmg;
                            e.2 |= zone == HitZone::Head;
                        }
                        None => per_victim.push((v, dmg, zone == HitZone::Head)),
                    }
                }
                Hit::Heli(i, dist) => heli_dmg.push((i, def.damage_at(dist))),
                Hit::Vehicle(i, dist) => vehicle_dmg.push((i, def.damage_at(dist))),
                Hit::None => {}
            }
            if k < 3 {
                self.shots.push(ShotFx {
                    id,
                    weapon,
                    from: origin.to_array(),
                    to: (origin + d * dist).to_array(),
                    flesh,
                });
            }
        }
        if let Some(p) = self.pm(id) {
            p.shots_fired += 1;
            if !per_victim.is_empty() {
                p.shots_hit += 1;
            }
        }
        for (v, dmg, head) in per_victim {
            self.damage(v, Some(id), dmg, Cause::Weapon(weapon), head, origin);
        }
        for (i, dmg) in vehicle_dmg {
            self.damage_vehicle(i, dmg, Some(id));
        }
        for (i, dmg) in heli_dmg {
            if let Some(h) = self.helis.get_mut(i) {
                if h.team != team || self.cfg.friendly_fire {
                    h.hp -= dmg;
                    self.outbox.push((
                        Dest::One(id),
                        S2C::HitConfirm {
                            kill: false,
                            headshot: false,
                        },
                        true,
                    ));
                }
            }
        }
    }

    /// Applies damage. `from` is where the damage came from (for the indicator).
    pub fn damage(
        &mut self,
        victim: u8,
        attacker: Option<u8>,
        amount: f32,
        cause: Cause,
        headshot: bool,
        from: Vec3,
    ) {
        // your own killstreak never hurts you (the pilot often stands right under it)
        if attacker == Some(victim)
            && matches!(
                cause,
                Cause::Drop | Cause::Artillery | Cause::Fpv | Cause::Nrk | Cause::Airstrike
            )
        {
            return;
        }
        let ff = self.cfg.friendly_fire;
        let time = self.time;
        let att_team = attacker.and_then(|a| self.p(a)).map(|a| a.team);
        let Some(v) = self.pm(victim) else { return };
        if !v.alive {
            return;
        }
        // spawn protection: nothing but falling out of the world hurts
        if time < v.protected_until && !matches!(cause, Cause::Fall | Cause::World) {
            return;
        }
        if let Some(a) = attacker {
            if a != victim && att_team == Some(v.team) && !ff {
                return;
            }
        }
        let mut amount = amount;
        if v.has_perk2(Perk2::Juggernaut) {
            amount *= 0.75;
        }
        v.hp -= amount;
        v.last_damage = time;
        if let Some(a) = attacker {
            if a != victim {
                v.attackers.retain(|e| e.0 != a);
                v.attackers.push((a, time));
            }
        }
        let dead = v.hp <= 0.0;
        if let Some(b) = v.bot.as_mut() {
            if let Some(a) = attacker {
                b.on_damaged(a, from);
            }
        }
        self.send(
            victim,
            S2C::Damaged {
                from: from.to_array(),
                amount,
            },
        );
        if let Some(a) = attacker {
            if a != victim {
                self.send(
                    a,
                    S2C::HitConfirm {
                        kill: dead,
                        headshot,
                    },
                );
            }
        }
        if dead {
            self.kill(victim, attacker, cause, headshot);
        }
    }

    pub fn kill(&mut self, victim: u8, attacker: Option<u8>, cause: Cause, headshot: bool) {
        let time = self.time;
        let delay = self.respawn_delay() as f64;
        let Some(v) = self.pm(victim) else { return };
        if !v.alive {
            return;
        }
        let dropped = *v.weapon();
        v.alive = false;
        v.hp = 0.0;
        v.deaths += 1;
        v.streak = 0;
        v.reload_end = None;
        v.respawn_at = time + delay;
        v.history.clear();
        let vpos = v.body.pos;
        let vteam = v.team;
        let vname = v.name.clone();
        let martyr = v.has_perk3(Perk3::Martyrdom);
        let assist_list: Vec<u8> = v
            .attackers
            .iter()
            .filter(|e| time - e.1 < 10.0 && Some(e.0) != attacker)
            .map(|e| e.0)
            .collect();
        v.attackers.clear();
        self.recent_deaths.push((vpos, time));
        self.drop_bomb_if_carrier(victim);
        if self.state != MatchState::PostMatch {
            self.drop_weapon(dropped, vpos);
        }
        if martyr && !matches!(cause, Cause::Bomb | Cause::Fall) {
            let id = self.new_ent_id();
            self.grenades.push(Grenade {
                id,
                owner: victim,
                pos: vpos + vec3(0.0, 0.4, 0.0),
                vel: vec3(0.0, 1.0, 0.0),
                explode_at: time + 2.5,
                smoke: false,
            });
        }

        let mut killer_name = String::new();
        let mut killer_team = Team::Spectator;
        let counts = self.state == MatchState::Playing;
        if let Some(a) = attacker {
            if a != victim {
                let mut reward: Option<Killstreak> = None;
                let mut popup = None;
                if let Some(k) = self.pm(a) {
                    killer_name = k.name.clone();
                    killer_team = k.team;
                    if k.team != vteam {
                        k.kills += 1;
                        k.streak += 1;
                        let pts = SCORE_KILL + if headshot { SCORE_HEADSHOT_BONUS } else { 0 };
                        k.score += pts;
                        popup = Some((
                            pts,
                            if headshot {
                                "Хедшот!".to_string()
                            } else {
                                "Вбивство".to_string()
                            },
                        ));
                        reward = Killstreak::for_streak(k.streak);
                        if let Some(r) = reward {
                            if !k.rewards.contains(&r) {
                                k.rewards.push(r);
                            }
                        }
                    } else {
                        k.score -= SCORE_KILL;
                    }
                }
                if let Some((pts, text)) = popup {
                    self.send(a, S2C::ScorePopup { points: pts, text });
                    if counts && self.mode == Mode::Tdm {
                        self.scores[killer_team.index()] += 1;
                    }
                }
                if let Some(r) = reward {
                    self.send(a, S2C::StreakReady(r));
                }
            }
        }
        for aid in assist_list {
            if let Some(p) = self.pm(aid) {
                if p.team != vteam {
                    p.assists += 1;
                    p.score += SCORE_ASSIST;
                    self.send(
                        aid,
                        S2C::ScorePopup {
                            points: SCORE_ASSIST,
                            text: "Допомога".into(),
                        },
                    );
                }
            }
        }
        let line = if killer_name.is_empty() || attacker == Some(victim) {
            format!("{vname} загинув ({})", cause.name())
        } else {
            format!("{killer_name} [{}] {vname}", cause.name())
        };
        self.logf(line);
        self.broadcast(S2C::Kill {
            killer: attacker,
            victim,
            cause,
            headshot,
            killer_name,
            victim_name: vname,
            killer_team,
            victim_team: vteam,
        });

        if counts && self.mode == Mode::Tdm {
            for t in [Team::Allies, Team::Axis] {
                if self.scores[t.index()] >= self.cfg.tdm.score_limit {
                    self.end_match(Some(t), "Досягнуто ліміту очок");
                    break;
                }
            }
        }
        if counts && self.mode == Mode::Sab && self.overtime {
            self.check_elimination();
        }
    }

    pub fn start_reload(&mut self, id: u8) {
        let time = self.time;
        let Some(cur) = self.p(id).map(|p| p.weapon().id) else {
            return;
        };
        let def = self.tuning.weapon(cur).clone();
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        if !p.alive || p.reload_end.is_some() {
            return;
        }
        if p.weapon().mag >= def.mag || p.weapon().reserve == 0 {
            return;
        }
        let k = if p.has_perk2(Perk2::SleightOfHand) {
            0.5
        } else {
            1.0
        };
        p.reload_end = Some(time + (def.reload_time * k) as f64 - 0.05);
    }

    pub fn switch_weapon(&mut self, id: u8, slot: u8) {
        let time = self.time;
        let Some(next) = self.p(id).map(|p| p.weapons[slot as usize & 1].id) else {
            return;
        };
        let swap = self.tuning.weapon(next).swap_time;
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        if !p.alive || slot > 1 || slot == p.slot {
            return;
        }
        p.slot = slot;
        p.reload_end = None;
        p.switch_end = time + swap as f64;
        p.shot_budget = p.shot_budget.min(1.0);
    }

    pub fn throw_grenade(&mut self, id: u8, origin: Vec3, dir: Vec3, cook: f32, smoke: bool) {
        if !self.combat_allowed() {
            return;
        }
        let time = self.time;
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        let left = if smoke { p.smokes } else { p.grenades };
        if !p.alive
            || left == 0
            || !dir.is_finite()
            || dir.length_squared() < 0.1
            || time < p.protected_until
        {
            return;
        }
        if smoke {
            p.smokes -= 1;
        } else {
            p.grenades -= 1;
        }
        let eye = p.eye();
        let origin = if origin.is_finite() && origin.distance(eye) < 1.2 {
            origin
        } else {
            eye
        };
        let gt = &self.tuning.grenade;
        let vel = dir.normalize() * gt.throw_speed + p.body.vel * gt.inherit_velocity;
        let (fuse, cook) = if smoke {
            (SMOKE_FUSE, 0.0)
        } else {
            (gt.fuse, cook.clamp(0.0, (gt.fuse - 0.3).max(0.0)))
        };
        let gid = self.new_ent_id();
        self.grenades.push(Grenade {
            id: gid,
            owner: id,
            pos: origin,
            vel,
            explode_at: time + (fuse - cook) as f64,
            smoke,
        });
    }

    pub fn update_grenades(&mut self, dt: f32) {
        let bounce = self.tuning.grenade.bounce;
        let mut boom = Vec::new();
        for g in self.grenades.iter_mut() {
            g.vel.y -= self.tuning.movement.gravity * dt;
            let step = g.vel * dt;
            let len = step.length();
            if len > 1e-5 {
                let d = step / len;
                if let Some((t, n)) = self.world.raycast(g.pos, d, len + 0.08) {
                    g.pos += d * (t - 0.08).max(0.0);
                    g.vel = g.vel - 2.0 * g.vel.dot(n) * n;
                    g.vel *= bounce;
                    if n.y > 0.5 && g.vel.y.abs() < 1.0 {
                        g.vel.y = 0.0;
                        g.vel *= 0.6;
                    }
                } else {
                    g.pos += step;
                }
            }
            if self.time >= g.explode_at {
                boom.push((g.pos, g.owner, g.smoke, g.id));
            }
        }
        let now = self.time;
        self.grenades.retain(|g| now < g.explode_at);
        for (pos, owner, smoke, gid) in boom {
            if smoke {
                self.smokes
                    .push((gid, pos + vec3(0.0, SMOKE_LIFT, 0.0), now));
                continue;
            }
            let (r, dmg) = (self.tuning.grenade.radius, self.tuning.grenade.damage);
            self.explode(
                pos + vec3(0.0, 0.2, 0.0),
                r,
                dmg,
                Some(owner),
                Cause::Grenade,
                false,
            );
        }
    }

    pub fn update_blasts(&mut self) {
        let now = self.time;
        let due: Vec<PendingBlast> = {
            let (due, keep): (Vec<_>, Vec<_>) = std::mem::take(&mut self.blasts)
                .into_iter()
                .partition(|b| b.at <= now);
            self.blasts = keep;
            due
        };
        for b in due {
            self.explode(b.pos, b.radius, b.damage, b.owner, b.cause, true);
        }
    }

    /// Radius damage with line-of-sight check.
    pub fn explode(
        &mut self,
        pos: Vec3,
        radius: f32,
        max_dmg: f32,
        owner: Option<u8>,
        cause: Cause,
        big: bool,
    ) {
        self.broadcast(S2C::Explosion {
            pos: pos.to_array(),
            radius,
            big,
        });
        let victims: Vec<(u8, f32)> = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.alive)
            .filter_map(|p| {
                let c = p.chest();
                let d = c.distance(pos);
                if d > radius {
                    return None;
                }
                let probe = pos + vec3(0.0, 0.25, 0.0);
                if !self.world.line_of_sight(probe, c) && !self.world.line_of_sight(probe, p.eye())
                {
                    return None;
                }
                let k = 1.0 - d / radius;
                Some((p.id, (max_dmg * k).max(15.0)))
            })
            .collect();
        for (v, dmg) in victims {
            self.damage(v, owner, dmg, cause, false, pos);
        }
        // vehicles caught in the blast
        let hits: Vec<(usize, f32)> = self
            .vehicles
            .iter()
            .enumerate()
            .filter_map(|(i, v)| {
                let (c, r) = v.sphere();
                let d = (c.distance(pos) - r).max(0.0);
                (d < radius).then(|| (i, max_dmg * (1.0 - d / radius)))
            })
            .collect();
        for (i, dmg) in hits {
            self.damage_vehicle(i, dmg, owner);
        }
        // helicopters caught in big blasts
        if big {
            for h in self.helis.iter_mut() {
                if h.pos.distance(pos) < radius {
                    h.hp -= max_dmg;
                }
            }
        }
    }

    pub fn melee(&mut self, id: u8) {
        if !self.combat_allowed() {
            return;
        }
        let time = self.time;
        let Some(p) = self.pm(id) else { return };
        if !p.alive || time < p.melee_ready || time < p.protected_until {
            return;
        }
        p.melee_ready = time + MELEE_COOLDOWN as f64;
        let eye = p.eye();
        let fwd = look_dir(p.yaw, p.pitch);
        let team = p.team;
        let ff = self.cfg.friendly_fire;
        let target = self
            .players
            .iter()
            .flatten()
            .filter(|o| o.alive && o.id != id && (o.team != team || ff))
            .filter_map(|o| {
                let c = o.chest();
                let to = c - eye;
                let d = to.length();
                if d > MELEE_RANGE + 0.5
                    || to.normalize().dot(fwd) < 0.55
                    || !self.world.line_of_sight(eye, c)
                {
                    return None;
                }
                Some((o.id, d))
            })
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        self.outbox.push((Dest::All, S2C::Melee { id }, false));
        if let Some((v, _)) = target {
            self.damage(v, Some(id), MELEE_DAMAGE, Cause::Knife, false, eye);
        }
    }

    pub fn use_killstreak(&mut self, id: u8, target: Option<[f32; 2]>, pilot: bool) {
        if self.state != MatchState::Playing && self.state != MatchState::Warmup {
            return;
        }
        let time = self.time;
        let Some(p) = self.pm(id) else { return };
        if !p.alive || time < p.protected_until || p.driving.is_some() {
            return;
        }
        let Some(&k) = p.rewards.last() else { return };
        let team = p.team;
        let name = p.name.clone();
        match k {
            Killstreak::Uav => {
                self.uav_until[team.index()] = time + UAV_DURATION as f64;
            }
            Killstreak::Airstrike => {
                let Some([tx, tz]) = target else { return };
                let tx = tx.clamp(self.map.min.x, self.map.max.x);
                let tz = tz.clamp(self.map.min.y, self.map.max.y);
                // planes come from our side of the map
                let home = self.map.spawns[team.index()]
                    .iter()
                    .fold(Vec3::ZERO, |a, s| a + s.pos)
                    / self.map.spawns[team.index()].len().max(1) as f32;
                let tgt = vec3(tx, 0.0, tz);
                let mut dir = vec3(tgt.x - home.x, 0.0, tgt.z - home.z);
                if dir.length_squared() < 1.0 {
                    dir = vec3(1.0, 0.0, 0.0);
                }
                let dir = dir.normalize();
                for i in 0..9 {
                    let off = (i as f32 - 4.0) * 2.8 + self.rng.range(-0.6, 0.6);
                    let side = vec3(-dir.z, 0.0, dir.x) * self.rng.range(-2.0, 2.0);
                    let mut p = tgt + dir * off + side;
                    p.y = self
                        .world
                        .floor_below(vec3(p.x, 30.0, p.z), 40.0)
                        .unwrap_or(0.0)
                        + 0.3;
                    self.blasts.push(PendingBlast {
                        at: time + 2.6 + i as f64 * 0.1,
                        pos: p,
                        radius: 7.0,
                        damage: 190.0,
                        owner: Some(id),
                        cause: Cause::Airstrike,
                    });
                }
                let from = tgt - dir * 120.0 + vec3(0.0, 35.0, 0.0);
                let to = tgt + dir * 120.0 + vec3(0.0, 35.0, 0.0);
                self.broadcast(S2C::Airstrike {
                    from: from.to_array(),
                    to: to.to_array(),
                });
            }
            Killstreak::Artillery => {
                let Some([tx, tz]) = target else { return };
                let tx = tx.clamp(self.map.min.x, self.map.max.x);
                let tz = tz.clamp(self.map.min.y, self.map.max.y);
                let spread = 9.0;
                // a barrage: shells land one after another over a few seconds
                for k in 0..14 {
                    let a = self.rng.range(0.0, std::f32::consts::TAU);
                    let r = spread * self.rng.f32().sqrt();
                    let x = tx + a.cos() * r;
                    let z = tz + a.sin() * r;
                    let y = self
                        .world
                        .floor_below(vec3(x, 40.0, z), 50.0)
                        .unwrap_or(0.0)
                        + 0.3;
                    self.blasts.push(PendingBlast {
                        at: time + 3.0 + k as f64 * 0.32 + self.rng.range(0.0, 0.15) as f64,
                        pos: vec3(x, y, z),
                        radius: 5.5,
                        damage: 150.0,
                        owner: Some(id),
                        cause: Cause::Artillery,
                    });
                }
                self.broadcast(S2C::Artillery {
                    target: [tx, 0.0, tz],
                    radius: spread,
                });
            }
            Killstreak::Fpv | Killstreak::Bomber | Killstreak::Nrk => {
                let kind = k.vehicle().unwrap();
                if !self.launch_vehicle(id, kind, pilot) {
                    return;
                }
            }
            Killstreak::Helicopter => {
                if self.helis.iter().any(|h| h.team == team) {
                    self.send(
                        id,
                        S2C::Center {
                            text: "Гелікоптер вже в повітрі".into(),
                            secs: 2.0,
                        },
                    );
                    return;
                }
                let hid = self.new_ent_id();
                let c = self.map.center();
                self.helis.push(Heli {
                    id: hid,
                    owner: id,
                    team,
                    angle: 0.0,
                    pos: vec3(c.x, HELI_HEIGHT, c.y),
                    yaw: 0.0,
                    hp: HELI_HEALTH,
                    until: time + HELI_DURATION as f64,
                    next_shot: time + 2.0,
                    next_target: time,
                    target: None,
                });
            }
        }
        if let Some(p) = self.pm(id) {
            p.rewards.pop();
        }
        self.logf(format!("{name}: {}", k.name()));
        self.broadcast(S2C::StreakUsed {
            by: name,
            team,
            streak: k,
        });
    }

    pub fn update_helis(&mut self, dt: f32) {
        let time = self.time;
        let c = self.map.center();
        let radius = (self.map.size().min_element() * 0.28).max(8.0);
        let mut shots: Vec<(u8, Vec3, Vec3)> = Vec::new();
        let mut downed = Vec::new();
        for (i, h) in self.helis.iter_mut().enumerate() {
            h.angle += 7.0 / radius * dt;
            let goal = vec3(
                c.x + h.angle.cos() * radius,
                HELI_HEIGHT,
                c.y + h.angle.sin() * radius,
            );
            h.pos = h.pos.lerp(goal, (dt * 1.5).min(1.0));
            if h.hp <= 0.0 {
                downed.push(i);
                continue;
            }
            if time >= h.next_target {
                h.next_target = time + 0.5;
                let eye = h.pos - vec3(0.0, 1.5, 0.0);
                h.target = self
                    .players
                    .iter()
                    .flatten()
                    .filter(|p| p.alive && p.team != h.team && p.team.is_playing())
                    .filter(|p| self.world.line_of_sight(eye, p.chest()))
                    .min_by(|a, b| {
                        a.body
                            .pos
                            .distance(h.pos)
                            .partial_cmp(&b.body.pos.distance(h.pos))
                            .unwrap()
                    })
                    .map(|p| p.id);
            }
            let tpos = h
                .target
                .and_then(|t| self.players[t as usize].as_ref())
                .filter(|p| p.alive)
                .map(|p| p.chest());
            match tpos {
                Some(tp) => {
                    let d = tp - h.pos;
                    h.yaw = toon_shared::yaw_of(d);
                    if time >= h.next_shot {
                        h.next_shot = time + 0.11;
                        shots.push((h.owner, h.pos - vec3(0.0, 1.5, 0.0), tp));
                    }
                }
                None => {
                    h.yaw = toon_shared::yaw_of(vec3(-h.angle.sin(), 0.0, h.angle.cos()));
                }
            }
        }
        for i in downed.into_iter().rev() {
            let h = self.helis.remove(i);
            self.broadcast(S2C::Explosion {
                pos: h.pos.to_array(),
                radius: 6.0,
                big: true,
            });
            self.center_all("Гелікоптер збито!", 2.5);
        }
        self.helis.retain(|h| time < h.until);
        for (owner, from, to) in shots {
            let spread = 0.045;
            let d = (to - from).normalize();
            let d = (d + vec3(
                self.rng.range(-spread, spread),
                self.rng.range(-spread, spread),
                self.rng.range(-spread, spread),
            ))
            .normalize();
            let (dist, hit) = self.trace(None, from, d, 120.0, time);
            let mut flesh = false;
            if let Hit::Player(v, _, _) = hit {
                let same = self.p(owner).map(|o| o.team) == self.p(v).map(|p| p.team);
                if !same {
                    flesh = true;
                    let owner_present = self.p(owner).is_some();
                    self.damage(
                        v,
                        if owner_present { Some(owner) } else { None },
                        22.0,
                        Cause::Helicopter,
                        false,
                        from,
                    );
                }
            }
            self.shots.push(ShotFx {
                id: 255,
                weapon: WeaponId::M249,
                from: from.to_array(),
                to: (from + d * dist).to_array(),
                flesh,
            });
        }
    }
}
