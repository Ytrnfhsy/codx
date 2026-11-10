//! Shooting, damage, deaths, grenades, melee and killstreaks.

use crate::game::{Dest, Fire, Game, Grenade, Heli, PendingBlast, WState};
use glam::{vec3, Vec3};
use toon_shared::ballistics::{trace_path, Ballistics, Path};
use toon_shared::consts::*;
use toon_shared::lang::{pick, t};
use toon_shared::protocol::{MatchState, ShotFx, S2C};
use toon_shared::ray::{ray_player, ray_sphere, HitZone};
use toon_shared::tr;
use toon_shared::weapons::{Cause, Killstreak, Perk2, Perk3, WeaponId};
use toon_shared::{look_dir, Mode, Team};

/// Result of tracing one bullet.
pub enum Hit {
    None,
    Player(u8, f32, HitZone),
    Heli(usize, f32),
    Vehicle(usize, f32),
    Car(usize, f32),
}

/// A bullet in flight.
pub struct Bullet {
    pub owner: u8,
    pub weapon: WeaponId,
    pub team: Team,
    pub origin: Vec3,
    pub path: Path,
    /// When it left the barrel.
    pub fired: f64,
    /// Players are checked where the shooter saw them: this much in the past (negative).
    pub rewind: f64,
    pub stopping: bool,
    /// Pieces of the path already checked.
    pub checked: usize,
    /// Which shot it belongs to (the pellets of a shotgun share one).
    pub shot: u32,
    /// Its tracer among this tick's shots.
    pub fx: Option<usize>,
}

/// Most bullets in the air at once.
const MAX_BULLETS: usize = 3000;

/// Radius of a helicopter's hit sphere.
const HELI_HIT_RADIUS: f32 = 2.6;

impl Game {
    fn combat_allowed(&self) -> bool {
        matches!(self.state, MatchState::Warmup | MatchState::Playing)
    }

    /// Traces a bullet against the world, players (rewound to `t`) and helicopters.
    /// A bullet's flight: how far it got, what it hit, and the share of its damage left
    /// after the wood and cloth it went through on the way.
    pub fn trace(
        &self,
        shooter: Option<u8>,
        o: Vec3,
        d: Vec3,
        max: f32,
        t: f64,
    ) -> (f32, Hit, f32) {
        let (best, _, pierced) = self.world.bullet_trace(o, d, max);
        let (best, hit) = self.trace_targets(shooter, o, d, best, t);
        let left = pierced
            .iter()
            .filter(|p| p.t < best)
            .filter_map(|p| p.mat.bullet_pass())
            .product();
        (best, hit, left)
    }

    /// The nearest player (rewound to `t`), helicopter or vehicle on a straight piece of
    /// a bullet's path, closer than `best`.
    fn trace_targets(
        &self,
        shooter: Option<u8>,
        o: Vec3,
        d: Vec3,
        mut best: f32,
        t: f64,
    ) -> (f32, Hit) {
        let mut hit = Hit::None;
        for p in self.players.iter().flatten() {
            if !p.alive || Some(p.id) == shooter {
                continue;
            }
            // far from this piece of the path (rewinding moves nobody more than a few metres)
            let rel = p.body.pos - o;
            let along = rel.dot(d).clamp(0.0, best);
            if (rel - d * along).length_squared() > 36.0 {
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
            if let Some(dist) = ray_sphere(o, d, h.pos, HELI_HIT_RADIUS) {
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
        for (i, c) in self.cars.iter().enumerate() {
            let (cc, r) = c.sphere();
            if let Some(dist) = ray_sphere(o, d, cc, r) {
                if dist < best {
                    best = dist;
                    hit = Hit::Car(i, dist);
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
        // the driver keeps both hands on the wheel
        if matches!(self.seat_of(id), Some((_, 0))) {
            return;
        }
        // nobody shoots while skydiving
        if self
            .p(id)
            .is_some_and(|p| p.body.chute != toon_shared::physics::Chute::None)
        {
            return;
        }
        let cur_mag = self.tuning.weapon(cur).mag;
        let def = self.tuning.weapon(weapon).clone();
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        let cur_mag = p.mag_cap(cur_mag);
        let quiet = p.suppressed();
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
        if !p.alive
            || p.downed
            || p.weapon().id != weapon
            || p.weapon().mag == 0
            || time < p.switch_end - slack
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
        // shooting puts the medkit away
        p.inv.healing = 0.0;
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
        // (a bot sees the world as it is now: nothing to rewind)
        let rewind = if p.is_bot() {
            0.0
        } else {
            (p.ping * 0.5 + INTERP_DELAY).clamp(0.0, MAX_REWIND) as f64
        };
        let stopping = p.has_perk2(Perk2::StoppingPower);
        let team = p.team;
        let t = time - rewind;

        let b = Ballistics::of(weapon);
        let mut fired = 0;
        for (k, d) in dirs.iter().take(def.pellets.max(1) as usize).enumerate() {
            if !d.is_finite() || d.length_squared() < 0.5 {
                continue;
            }
            let d = d.normalize();
            if d.dot(look).clamp(-1.0, 1.0).acos() > max_angle {
                continue;
            }
            let path = trace_path(&self.world, origin, d, &b);
            // the first pellets show a tracer to where the bullet ends up
            let fx = (k < 3).then(|| {
                self.shots.push(ShotFx {
                    id,
                    weapon,
                    from: origin.to_array(),
                    to: path.end().to_array(),
                    flesh: false,
                    quiet,
                });
                self.shots.len() - 1
            });
            fired += 1;
            if self.bullets.len() < MAX_BULLETS {
                self.bullets.push(Bullet {
                    owner: id,
                    weapon,
                    team,
                    origin,
                    path,
                    // it covers its first tick of flight at once: point blank is instant
                    fired: time - TICK_DT as f64,
                    rewind: t - time,
                    stopping,
                    checked: 0,
                    shot: self.shot_serial,
                    fx,
                });
            }
        }
        self.shot_serial = self.shot_serial.wrapping_add(1);
        if let Some(p) = self.pm(id) {
            if fired > 0 {
                p.shots_fired += 1;
            }
        }
        self.update_bullets();
    }

    /// Moves every bullet in flight along its path and applies what it hits.
    pub fn update_bullets(&mut self) {
        if self.bullets.is_empty() {
            return;
        }
        let time = self.time;
        let mut hits: Vec<(usize, Hit, f32)> = Vec::new();
        let mut done: Vec<usize> = Vec::new();
        for (bi, bl) in self.bullets.iter().enumerate() {
            let upto = bl.path.piece_at((time - bl.fired) as f32);
            let at = time + bl.rewind;
            let mut i = bl.checked;
            let mut found = None;
            while i < upto {
                let (a, c) = (bl.path.points[i], bl.path.points[i + 1]);
                let seg = c - a;
                let len = seg.length();
                if len > 1e-4 {
                    let (dist, hit) = self.trace_targets(Some(bl.owner), a, seg / len, len, at);
                    if !matches!(hit, Hit::None) {
                        found = Some((hit, bl.path.dist[i] + dist));
                        break;
                    }
                }
                i += 1;
            }
            match found {
                Some((hit, d)) => {
                    hits.push((bi, hit, d));
                    done.push(bi);
                }
                None if upto + 1 >= bl.path.points.len() => done.push(bi),
                None => {}
            }
        }
        for bl in self.bullets.iter_mut() {
            bl.checked = bl.path.piece_at((time - bl.fired) as f32);
        }
        // pellets of one shot that hit the same player add up into one hit
        let mut per_victim: Vec<(u8, u8, WeaponId, f32, bool, Vec3)> = Vec::new();
        let mut counted: Vec<(u8, u32)> = Vec::new();
        let mut heli_dmg: Vec<(u8, Team, usize, f32)> = Vec::new();
        let mut vehicle_dmg: Vec<(u8, usize, f32)> = Vec::new();
        let mut car_dmg: Vec<(u8, usize, f32)> = Vec::new();
        for (bi, hit, dist) in hits {
            let bl = &self.bullets[bi];
            let def = self.tuning.weapon(bl.weapon);
            let left = bl.path.left_at(dist);
            let base = def.damage_at(dist) * left;
            let point = bl.path.point_at(dist);
            // the tracer of a shot fired this tick ends where it really hit
            if let Some(i) = bl.fx.filter(|_| time - bl.fired <= TICK_DT as f64 + 1e-6) {
                if let Some(s) = self.shots.get_mut(i) {
                    s.to = point.to_array();
                    s.flesh = matches!(hit, Hit::Player(..));
                }
            }
            match hit {
                Hit::Player(v, _, zone) => {
                    let mut dmg = base * zone.mult(def.head_mult);
                    if bl.stopping {
                        dmg *= 1.4;
                    }
                    if !counted.contains(&(bl.owner, bl.shot)) {
                        counted.push((bl.owner, bl.shot));
                    }
                    match per_victim.iter_mut().find(|e| e.0 == bl.owner && e.1 == v) {
                        Some(e) => {
                            e.3 += dmg;
                            e.4 |= zone == HitZone::Head;
                        }
                        None => per_victim.push((
                            bl.owner,
                            v,
                            bl.weapon,
                            dmg,
                            zone == HitZone::Head,
                            bl.origin,
                        )),
                    }
                }
                Hit::Heli(i, _) => heli_dmg.push((bl.owner, bl.team, i, base)),
                Hit::Vehicle(i, _) => vehicle_dmg.push((bl.owner, i, base)),
                Hit::Car(i, _) => car_dmg.push((bl.owner, i, base)),
                Hit::None => {}
            }
        }
        done.sort_unstable();
        for bi in done.into_iter().rev() {
            self.bullets.swap_remove(bi);
        }
        for (owner, _) in counted {
            if let Some(p) = self.pm(owner) {
                p.shots_hit += 1;
            }
        }
        for (owner, v, weapon, dmg, head, origin) in per_victim {
            self.damage(v, Some(owner), dmg, Cause::Weapon(weapon), head, origin);
        }
        for (owner, i, dmg) in vehicle_dmg {
            self.damage_vehicle(i, dmg, Some(owner));
        }
        for (owner, i, dmg) in car_dmg {
            self.damage_car(i, dmg, Some(owner));
        }
        for (owner, team, i, dmg) in heli_dmg {
            if let Some(h) = self.helis.get_mut(i) {
                if h.team != team || self.cfg.friendly_fire {
                    h.hp -= dmg;
                    self.outbox.push((
                        Dest::One(owner),
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
        let solo = self.mode.solo();
        let time = self.time;
        let friends = attacker.is_some_and(|a| a != victim && self.allied_ids(a, victim));
        // battle royale squads: a knock-down instead of death while a squadmate still stands
        let can_knock = solo
            && self.p(victim).is_some_and(|v| {
                v.squad != 0
                    && !v.downed
                    && self
                        .players
                        .iter()
                        .flatten()
                        .any(|o| o.id != victim && o.squad == v.squad && o.alive && !o.downed)
            });
        let Some(v) = self.pm(victim) else { return };
        if !v.alive {
            return;
        }
        // spawn protection: nothing but falling out of the world hurts
        if time < v.protected_until && !matches!(cause, Cause::Fall | Cause::World) {
            return;
        }
        if friends && !ff {
            return;
        }
        let mut amount = amount;
        if v.has_perk2(Perk2::Juggernaut) {
            amount *= 0.75;
        }
        // battle royale: the vest takes part of a body hit, the helmet part of a head
        // hit, while they last
        if solo && !v.downed && !matches!(cause, Cause::Zone | Cause::Fall | Cause::World) {
            amount = v.inv.soak(amount, headshot);
            v.inv.healing = 0.0;
        }
        v.hp -= amount;
        // knocked down: on the ground with a fresh pool of "bleeding" health
        if v.hp <= 0.0 && can_knock && cause != Cause::World {
            v.downed = true;
            v.downed_by = attacker.filter(|a| *a != victim);
            v.hp = crate::br::DOWNED_HP;
            v.revive = 0.0;
            v.reload_end = None;
            v.inv.healing = 0.0;
            v.body.stance = toon_shared::physics::Stance::Prone;
            v.last_damage = time;
            let name = v.name.clone();
            self.say(Dest::One(victim), || S2C::Center {
                text: t(
                    "Тебе поранено! Повзи в укриття — напарник може підняти",
                    "You are down! Crawl to cover — a squadmate can pick you up",
                )
                .into(),
                secs: 3.0,
            });
            if let Some(a) = attacker.filter(|a| *a != victim) {
                self.send(
                    a,
                    S2C::HitConfirm {
                        kill: false,
                        headshot,
                    },
                );
                self.say(Dest::One(a), || S2C::Center {
                    text: tr!("Поранено: {name}", "Knocked down: {name}"),
                    secs: 1.5,
                });
            }
            return;
        }
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
        // a knocked-down player who bleeds out or is finished: the knock-down earns the kill
        let attacker = match (attacker, v.downed_by) {
            (None, Some(d)) if v.downed => Some(d),
            (a, _) => a,
        };
        v.downed = false;
        v.downed_by = None;
        v.revive = 0.0;
        v.alive = false;
        v.hp = 0.0;
        v.deaths += 1;
        v.streak = 0;
        v.reload_end = None;
        v.respawn_at = time + delay;
        v.history.clear();
        let vpos = v.body.pos;
        let vteam = v.team;
        let vsquad = v.squad;
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
        // (battle royale: the rounds are in the inventory, which drops by itself)
        let dropped = if self.mode.solo() {
            WState {
                reserve: 0,
                ..dropped
            }
        } else {
            dropped
        };
        if self.state != MatchState::PostMatch && self.mode == Mode::Br && self.br.running {
            // battle royale: the guns go in the death box with the rest
            let guns: Vec<WState> = self
                .p(victim)
                .map(|v| {
                    v.weapons
                        .iter()
                        .enumerate()
                        .filter(|(k, _)| v.empty_slot != Some(*k as u8))
                        .map(|(_, w)| WState { reserve: 0, ..*w })
                        .collect()
                })
                .unwrap_or_default();
            if !guns.is_empty() {
                let b = self.br_box_of(victim);
                self.br.boxes[b].guns.extend(guns);
            }
        } else if self.state != MatchState::PostMatch {
            self.drop_weapon(dropped, vpos);
            // battle royale: everything they carried is up for grabs
            if self.mode.solo() {
                let other = self
                    .p(victim)
                    .and_then(|v| v.weapons.iter().find(|w| w.id != dropped.id).copied());
                let empty = self.p(victim).and_then(|v| v.empty_slot);
                let other_slot = self
                    .p(victim)
                    .and_then(|v| v.weapons.iter().position(|w| w.id != dropped.id));
                if let Some(w) =
                    other.filter(|_| empty.is_none() || empty != other_slot.map(|s| s as u8))
                {
                    self.drop_weapon(WState { reserve: 0, ..w }, vpos + vec3(0.6, 0.0, 0.3));
                }
            }
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
                kind: 0,
            });
        }

        let solo = self.mode.solo();
        let mut killer_name = String::new();
        let mut killer_team = Team::Spectator;
        let counts = self.state == MatchState::Playing;
        if let Some(a) = attacker {
            if a != victim {
                let mut reward: Option<Killstreak> = None;
                let streaks = self.tuning.streaks.clone();
                let mut popup = None;
                if let Some(k) = self.pm(a) {
                    killer_name = k.name.clone();
                    killer_team = k.team;
                    if k.team != vteam || solo && !(vsquad != 0 && k.squad == vsquad) {
                        k.kills += 1;
                        k.streak += 1;
                        let pts = SCORE_KILL + if headshot { SCORE_HEADSHOT_BONUS } else { 0 };
                        k.score += pts;
                        popup = Some((
                            pts,
                            if headshot {
                                ("Хедшот!", "Headshot!")
                            } else {
                                ("Вбивство", "Kill")
                            },
                        ));
                        // no killstreaks in the battle royale
                        if !solo {
                            reward = Killstreak::for_streak(k.streak, &streaks);
                        }
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
                    self.say(Dest::One(a), || S2C::ScorePopup {
                        points: pts,
                        text: pick(text).into(),
                    });
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
                if p.team != vteam || solo {
                    p.assists += 1;
                    p.score += SCORE_ASSIST;
                    self.say(Dest::One(aid), || S2C::ScorePopup {
                        points: SCORE_ASSIST,
                        text: t("Допомога", "Assist").into(),
                    });
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
                    self.end_match(Some(t), ("Досягнуто ліміту очок", "Score limit reached"));
                    break;
                }
            }
        }
        if counts && self.mode == Mode::Sab && self.overtime {
            self.check_elimination();
        }
        if self.mode == Mode::Br {
            self.br_on_death(victim);
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
        if p.weapon().mag >= p.mag_cap(def.mag) || p.weapon().reserve == 0 {
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
            || p.downed
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
            kind: 0,
        });
    }

    /// Battle royale: a molotov (bursts into fire where it lands) or a flashbang (blinds
    /// those looking its way after a second and a half).
    pub fn throw_special(&mut self, id: u8, origin: Vec3, dir: Vec3, kind: u8) {
        use toon_shared::inventory::{THROW_FLASH, THROW_MOLOTOV};
        if !self.combat_allowed() || !dir.is_finite() || dir.length_squared() < 0.1 {
            return;
        }
        let time = self.time;
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        if !p.alive || p.downed || time < p.protected_until {
            return;
        }
        let left = match kind {
            THROW_MOLOTOV => &mut p.inv.molotovs,
            THROW_FLASH => &mut p.inv.flashes,
            _ => return,
        };
        if *left == 0 {
            return;
        }
        *left -= 1;
        p.inv.healing = 0.0;
        let eye = p.eye();
        let origin = if origin.is_finite() && origin.distance(eye) < 1.2 {
            origin
        } else {
            eye
        };
        let gt = &self.tuning.grenade;
        let vel = dir.normalize() * gt.throw_speed + p.body.vel * gt.inherit_velocity;
        let gid = self.new_ent_id();
        self.grenades.push(Grenade {
            id: gid,
            owner: id,
            pos: origin,
            vel,
            // a molotov bursts on impact (or after a while in the air)
            explode_at: time + if kind == THROW_MOLOTOV { 5.0 } else { 1.5 },
            smoke: false,
            kind,
        });
    }

    /// Fires burn whoever stands in them (twice a second) and die down.
    pub fn update_fires(&mut self) {
        let now = self.time;
        self.fires.retain(|f| f.until > now);
        if self.fires.is_empty() || now < self.next_burn_fire {
            return;
        }
        self.next_burn_fire = now + 0.5;
        let mut burn: Vec<(u8, u8, Vec3)> = Vec::new();
        for f in &self.fires {
            for p in self.players.iter().flatten() {
                let d = p.body.pos - f.pos;
                if p.alive && vec3(d.x, 0.0, d.z).length() < f.radius && d.y.abs() < 1.8 {
                    burn.push((p.id, f.owner, f.pos));
                }
            }
        }
        for (v, owner, at) in burn {
            self.damage(v, Some(owner), 7.0, Cause::Fire, false, at);
        }
    }

    /// A flashbang goes off: blinds those who can see it, the more the nearer and the
    /// more they look its way.
    fn flashbang(&mut self, pos: Vec3) {
        let time = self.time;
        let mut hit: Vec<(u8, f32, bool)> = Vec::new();
        for p in self.players.iter().flatten() {
            if !p.alive {
                continue;
            }
            let eye = p.eye();
            let d = pos.distance(eye);
            if d > 22.0 || !self.world.line_of_sight(eye, pos + vec3(0.0, 0.3, 0.0)) {
                continue;
            }
            let facing = look_dir(p.yaw, p.pitch).dot((pos - eye).normalize_or_zero());
            let k = (1.0 - d / 22.0) * (0.35 + 0.65 * ((facing + 1.0) * 0.5).powi(2));
            if k > 0.08 {
                hit.push((p.id, k.min(1.0), p.is_bot()));
            }
        }
        for (id, k, bot) in hit {
            if bot {
                if let Some(b) = self.pm(id).and_then(|p| p.bot.as_mut()) {
                    b.blind_until = time + 4.0 * k as f64;
                }
            } else {
                self.send(id, S2C::Flashed(k));
            }
        }
        self.broadcast(S2C::Explosion {
            pos: pos.to_array(),
            radius: 1.0,
            big: false,
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
                    // a molotov's bottle breaks on whatever it hits
                    if g.kind == toon_shared::inventory::THROW_MOLOTOV {
                        g.explode_at = self.time;
                    }
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
                boom.push((g.pos, g.owner, g.smoke, g.id, g.kind));
            }
        }
        let now = self.time;
        self.grenades.retain(|g| now < g.explode_at);
        for (pos, owner, smoke, gid, kind) in boom {
            if kind == toon_shared::inventory::THROW_MOLOTOV {
                // fire spreads over the ground where it broke
                let floor = self
                    .world
                    .floor_below(pos + vec3(0.0, 0.5, 0.0), 6.0)
                    .unwrap_or(pos.y);
                self.fires.push(Fire {
                    id: gid,
                    owner,
                    pos: vec3(pos.x, floor, pos.z),
                    radius: 3.5,
                    until: now + 8.0,
                });
                continue;
            }
            if kind == toon_shared::inventory::THROW_FLASH {
                self.flashbang(pos);
                continue;
            }
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
        // and cars
        let hits: Vec<(usize, f32)> = self
            .cars
            .iter()
            .enumerate()
            .filter_map(|(i, c)| {
                let (cc, r) = c.sphere();
                let d = (cc.distance(pos) - r).max(0.0);
                (d < radius).then(|| (i, max_dmg * 1.5 * (1.0 - d / radius)))
            })
            .collect();
        for (i, dmg) in hits {
            self.damage_car(i, dmg, owner);
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
        if !p.alive || p.downed || time < p.melee_ready || time < p.protected_until {
            return;
        }
        p.melee_ready = time + MELEE_COOLDOWN as f64;
        let eye = p.eye();
        let fwd = look_dir(p.yaw, p.pitch);
        let ff = self.cfg.friendly_fire;
        let target = self
            .players
            .iter()
            .flatten()
            .filter(|o| o.alive && o.id != id && (ff || !self.allied_ids(id, o.id)))
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
                self.uav_until[team.index()] = time + self.tuning.streaks.uav_time as f64;
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
                let st = self.tuning.streaks.clone();
                let n = st.airstrike_bombs as usize;
                for i in 0..n {
                    // a line of bombs centred on the target, ~2.8 m apart
                    let off = (i as f32 - (n as f32 - 1.0) / 2.0) * 2.8 + self.rng.range(-0.6, 0.6);
                    let side = vec3(-dir.z, 0.0, dir.x) * self.rng.range(-2.0, 2.0);
                    let mut p = tgt + dir * off + side;
                    p.y = self
                        .world
                        .floor_below(vec3(p.x, 120.0, p.z), 140.0)
                        .unwrap_or(0.0)
                        + 0.3;
                    self.blasts.push(PendingBlast {
                        at: time + st.airstrike_delay as f64 + i as f64 * 0.1,
                        pos: p,
                        radius: st.airstrike_radius,
                        damage: st.airstrike_damage,
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
                let st = self.tuning.streaks.clone();
                let spread = st.artillery_spread;
                // a barrage: shells land one after another over a few seconds
                for k in 0..st.artillery_shells {
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
                        at: time
                            + st.artillery_delay as f64
                            + k as f64 * st.artillery_interval as f64
                            + self.rng.range(0.0, 0.15) as f64,
                        pos: vec3(x, y, z),
                        radius: st.artillery_radius,
                        damage: st.artillery_damage,
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
                    self.say(Dest::One(id), || S2C::Center {
                        text: t("Гелікоптер вже в повітрі", "A helicopter is already up").into(),
                        secs: 2.0,
                    });
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
                    hp: self.tuning.streaks.heli_health,
                    until: time + self.tuning.streaks.heli_time as f64,
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
        let heli_interval = self.tuning.streaks.heli_fire_interval as f64;
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
                        h.next_shot = time + heli_interval;
                        // the gun fires from just outside the helicopter's own hit
                        // sphere: from inside it every bullet "hit" the helicopter
                        let muzzle = h.pos + d.normalize_or_zero() * (HELI_HIT_RADIUS + 0.3);
                        shots.push((h.owner, muzzle, tp));
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
            self.center_all(
                || t("Гелікоптер збито!", "Helicopter shot down!").into(),
                2.5,
            );
        }
        self.helis.retain(|h| time < h.until);
        for (owner, from, to) in shots {
            let spread = self.tuning.streaks.heli_spread;
            let d = (to - from).normalize();
            let d = (d + vec3(
                self.rng.range(-spread, spread),
                self.rng.range(-spread, spread),
                self.rng.range(-spread, spread),
            ))
            .normalize();
            let (dist, hit, left) = self.trace(None, from, d, 120.0, time);
            let mut flesh = false;
            if let Hit::Player(v, _, _) = hit {
                let same = self.p(owner).map(|o| o.team) == self.p(v).map(|p| p.team);
                if !same {
                    flesh = true;
                    let owner_present = self.p(owner).is_some();
                    self.damage(
                        v,
                        if owner_present { Some(owner) } else { None },
                        self.tuning.streaks.heli_damage * left,
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
                quiet: false,
            });
        }
    }
}
