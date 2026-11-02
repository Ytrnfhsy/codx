//! Killstreak vehicles on the server: the FPV kamikaze drone, the bomber drone with its
//! falling bombs and the ground robot (НРК), piloted or fighting on its own.
//!
//! A pilot's client moves its vehicle (like a soldier) and sends `C2S::Drive`; the server
//! checks every move against the vehicle's top speed and the level, and does everything
//! that hurts: detonations, bombs, the robot's machine gun.

use crate::combat::Hit;
use crate::game::{Dest, Game};
use glam::{vec3, Vec3};
use toon_shared::physics::{fits, Body, Stance};
use toon_shared::protocol::{ShotFx, VehicleHud, S2C};
use toon_shared::ray::smoke_blocks;
use toon_shared::tr;
use toon_shared::vehicles::*;
use toon_shared::weapons::{Cause, WeaponId};
use toon_shared::{look_dir, yaw_of, Team};

pub struct Vehicle {
    pub id: u16,
    pub owner: u8,
    pub team: Team,
    pub kind: VehicleKind,
    pub pos: Vec3,
    pub vel: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub hp: f32,
    pub until: f64,
    pub ammo: u8,
    /// A robot fighting on its own (nobody steers it).
    pub auto: bool,
    /// Ground physics for the robot.
    pub body: Body,
    pub keys: u8,
    pub last_drive: f64,
    pub next_shot: f64,
    pub next_drop: f64,
    // autonomous robot
    pub path: Vec<Vec3>,
    pub next_think: f64,
    pub target: Option<u8>,
    pub stuck_since: (Vec3, f64),
}

impl Vehicle {
    /// Centre of the hit sphere and its radius.
    pub fn sphere(&self) -> (Vec3, f32) {
        let (r, y) = self.kind.hit_sphere();
        (self.pos + vec3(0.0, y, 0.0), r)
    }
}

/// A bomb falling from a drone.
pub struct BombDrop {
    pub id: u16,
    pub owner: u8,
    pub pos: Vec3,
    pub vel: Vec3,
}

/// Why a vehicle stops.
enum End {
    /// It blew up on purpose (FPV) — with damage.
    Detonate(Vec3),
    /// Shot down / destroyed — a harmless burst.
    Destroyed,
    /// Ran out of time (or the pilot left).
    Expired,
}

impl Game {
    /// Starts a killstreak vehicle for player `id`. `pilot` = false starts a robot that
    /// fights on its own. Returns false if it could not start.
    pub fn launch_vehicle(&mut self, id: u8, kind: VehicleKind, pilot: bool) -> bool {
        let time = self.time;
        let Some(p) = self.p(id) else { return false };
        if p.driving.is_some() {
            return false;
        }
        let pilot = pilot || kind != VehicleKind::Nrk;
        let team = p.team;
        let (pos0, yaw, pitch, eye) = (p.body.pos, p.yaw, p.pitch, p.eye());
        let fwd = look_dir(yaw, 0.0);
        let (pos, pitch) = match kind {
            // it takes off hovering just above the head (lower under a ceiling), looking
            // roughly level, and waits for the pilot
            VehicleKind::Fpv => {
                let (at, _, _) = fpv_move(&self.world, eye, vec3(0.0, 1.2, 0.0) + fwd * 0.4, 1.0);
                (at, pitch.clamp(-0.4, 0.4))
            }
            VehicleKind::Bomber => {
                let min = self.map.min;
                let max = self.map.max;
                // a little ahead of the soldier, so the first bomb is not on their head
                let ahead = pos0 + fwd * 8.0;
                (
                    vec3(
                        ahead.x.clamp(min.x, max.x),
                        BOMBER_ALT,
                        ahead.z.clamp(min.y, max.y),
                    ),
                    -1.45,
                )
            }
            VehicleKind::Nrk => {
                // in front of the soldier if there is room, else right where they stand
                let ahead = pos0 + fwd * 1.6;
                let spot = if fits(&self.world, ahead + vec3(0.0, 0.05, 0.0), Stance::Crouch)
                    && self.world.line_of_sight(eye, ahead + vec3(0.0, 0.6, 0.0))
                {
                    ahead
                } else {
                    pos0
                };
                (spot, 0.0)
            }
        };
        let vid = self.new_ent_id();
        self.vehicles.push(Vehicle {
            id: vid,
            owner: id,
            team,
            kind,
            pos,
            vel: Vec3::ZERO,
            yaw,
            pitch,
            hp: kind.health(&self.tuning.streaks),
            until: time + kind.duration(&self.tuning.streaks) as f64,
            ammo: kind.ammo(&self.tuning.streaks),
            auto: !pilot,
            body: Body {
                pos,
                stance: Stance::Crouch,
                on_ground: true,
                ..Default::default()
            },
            keys: 0,
            last_drive: time,
            next_shot: time + 0.5,
            next_drop: time + 0.6,
            path: Vec::new(),
            next_think: time,
            target: None,
            stuck_since: (pos, time),
        });
        if pilot {
            if let Some(p) = self.pm(id) {
                p.driving = Some(vid);
            }
        }
        true
    }

    /// A pilot moved their vehicle.
    pub fn on_drive(&mut self, id: u8, pos: Vec3, yaw: f32, pitch: f32, keys: u8) {
        let time = self.time;
        let Some(vid) = self.p(id).and_then(|p| p.driving) else {
            return;
        };
        let Some(i) = self.vehicles.iter().position(|v| v.id == vid) else {
            return;
        };
        let (min, max) = (self.map.min, self.map.max);
        let v = &mut self.vehicles[i];
        v.keys = keys;
        v.yaw = yaw;
        v.pitch = pitch.clamp(-1.55, 1.55);
        if !pos.is_finite() {
            return;
        }
        let dt = ((time - v.last_drive) as f32).clamp(1.0 / 60.0, 0.5);
        v.last_drive = time;
        // never faster than the vehicle can go (a little slack for network jitter)
        let reach = v.kind.max_speed(&self.tuning.streaks) * dt * 1.3 + 0.8;
        let mut to = pos;
        let d = to - v.pos;
        let flat = vec3(d.x, 0.0, d.z);
        if flat.length() > reach {
            to = v.pos + flat.normalize() * reach + vec3(0.0, d.y, 0.0);
        }
        match v.kind {
            VehicleKind::Fpv => {
                let d = to - v.pos;
                if d.y.abs() > reach {
                    to.y = v.pos.y + d.y.signum() * reach;
                }
                // it cannot fly through walls: it slides along them like on the pilot's
                // screen (crashing into one is the pilot's FIRE, sent by their client)
                let (at, _, _) = fpv_move(&self.world, v.pos, to - v.pos, 1.0);
                to = at;
            }
            VehicleKind::Bomber => {
                to = vec3(
                    to.x.clamp(min.x, max.x),
                    BOMBER_ALT,
                    to.z.clamp(min.y, max.y),
                );
            }
            VehicleKind::Nrk => {
                // falls are fine, climbing only as far as stairs allow
                if to.y > v.pos.y + 0.6 + dt * 3.0 {
                    to.y = v.pos.y;
                }
                if !fits(&self.world, to + vec3(0.0, 0.02, 0.0), Stance::Crouch) {
                    return;
                }
                v.body.pos = to;
            }
        }
        v.vel = (to - v.pos) / dt;
        v.pos = to;
        if v.kind == VehicleKind::Fpv && keys & keys::CRASH != 0 {
            let age = v.kind.duration(&self.tuning.streaks) - (v.until - time) as f32;
            let end = if age >= self.tuning.streaks.fpv_arm_time {
                End::Detonate(v.pos)
            } else {
                End::Destroyed
            };
            self.end_vehicle(i, end);
            return;
        }
        if keys & keys::EXIT != 0 {
            let end = if v.kind == VehicleKind::Fpv {
                End::Detonate(v.pos)
            } else {
                End::Expired
            };
            self.end_vehicle(i, end);
        }
    }

    pub fn update_vehicles(&mut self, dt: f32) {
        let time = self.time;
        let mut i = 0;
        while i < self.vehicles.len() {
            let v = &self.vehicles[i];
            let owner_ok = self
                .p(v.owner)
                .is_some_and(|p| p.team == v.team && (p.alive || v.auto));
            let end = if !owner_ok {
                Some(End::Expired)
            } else if v.hp <= 0.0 {
                Some(End::Destroyed)
            } else if time >= v.until {
                Some(if v.kind == VehicleKind::Fpv {
                    End::Detonate(v.pos)
                } else {
                    End::Expired
                })
            } else {
                None
            };
            if let Some(e) = end {
                self.end_vehicle(i, e);
                continue;
            }
            match v.kind {
                VehicleKind::Fpv => {
                    let (pos, team) = (v.pos, v.team);
                    let armed = v.kind.duration(&self.tuning.streaks) - (v.until - time) as f32
                        >= self.tuning.streaks.fpv_arm_time;
                    let fire = v.keys & keys::FIRE != 0 && armed;
                    let near = self.players.iter().flatten().any(|p| {
                        p.alive
                            && p.team != team
                            && p.team.is_playing()
                            && (p.chest().distance(pos) < FPV_TRIGGER
                                || p.eye().distance(pos) < FPV_TRIGGER)
                    });
                    if fire || (near && armed) {
                        self.end_vehicle(i, End::Detonate(pos));
                        continue;
                    }
                }
                VehicleKind::Bomber => {
                    if v.keys & keys::FIRE != 0 && v.ammo > 0 && time >= v.next_drop {
                        let (owner, pos, vel) = (v.owner, v.pos, v.vel);
                        let did = self.new_ent_id();
                        let v = &mut self.vehicles[i];
                        v.ammo -= 1;
                        v.next_drop = time + self.tuning.streaks.bomber_cooldown as f64;
                        self.drops.push(BombDrop {
                            id: did,
                            owner,
                            pos: pos - vec3(0.0, 0.8, 0.0),
                            vel: vec3(vel.x, -1.5, vel.z),
                        });
                    }
                }
                VehicleKind::Nrk => {
                    if v.auto {
                        self.think_robot(i, dt);
                    }
                    let v = &self.vehicles[i];
                    if v.keys & keys::FIRE != 0 && time >= v.next_shot {
                        self.vehicles[i].next_shot =
                            time + self.tuning.streaks.nrk_fire_interval as f64;
                        self.robot_fire(i);
                    }
                }
            }
            i += 1;
        }
        self.update_drops(dt);
    }

    fn update_drops(&mut self, dt: f32) {
        let g = self.tuning.movement.gravity;
        let mut boom = Vec::new();
        for d in self.drops.iter_mut() {
            d.vel.y -= g * dt;
            let step = d.vel * dt;
            let len = step.length();
            if len < 1e-5 {
                continue;
            }
            let dir = step / len;
            match self.world.raycast(d.pos, dir, len) {
                Some((t, _)) => {
                    boom.push((d.id, d.pos + dir * t + vec3(0.0, 0.2, 0.0), d.owner));
                }
                None => d.pos += step,
            }
            if d.pos.y < -5.0 {
                boom.push((d.id, d.pos, d.owner));
            }
        }
        for (id, pos, owner) in boom {
            self.drops.retain(|d| d.id != id);
            self.explode(
                pos,
                self.tuning.streaks.bomber_drop_radius,
                self.tuning.streaks.bomber_drop_damage,
                Some(owner),
                Cause::Drop,
                false,
            );
        }
    }

    /// One burst of the robot's machine gun along its turret.
    fn robot_fire(&mut self, i: usize) {
        let time = self.time;
        let v = &self.vehicles[i];
        let (owner, team) = (v.owner, v.team);
        let fwd = look_dir(v.yaw, v.pitch);
        let from = v.pos + vec3(0.0, NRK_GUN_Y, 0.0) + look_dir(v.yaw, 0.0) * 0.95;
        let s = self.tuning.streaks.nrk_spread;
        let d = (fwd
            + vec3(
                self.rng.range(-s, s),
                self.rng.range(-s, s),
                self.rng.range(-s, s),
            ))
        .normalize();
        let (dist, hit) = self.trace(Some(owner), from, d, 150.0, time);
        let mut flesh = false;
        match hit {
            Hit::Player(victim, _, _) => {
                let same = self.p(victim).map(|p| p.team) == Some(team);
                if !same || self.cfg.friendly_fire {
                    flesh = true;
                    self.damage(
                        victim,
                        Some(owner),
                        self.tuning.streaks.nrk_damage,
                        Cause::Nrk,
                        false,
                        from,
                    );
                }
            }
            Hit::Heli(h, _) => {
                if self.helis.get(h).is_some_and(|h| h.team != team) {
                    self.helis[h].hp -= self.tuning.streaks.nrk_damage;
                }
            }
            Hit::Vehicle(j, _) => {
                if j != i && self.vehicles[j].team != team {
                    self.vehicles[j].hp -= self.tuning.streaks.nrk_damage;
                }
            }
            Hit::None => {}
        }
        self.shots.push(ShotFx {
            id: 254,
            weapon: WeaponId::M249,
            from: from.to_array(),
            to: (from + d * dist).to_array(),
            flesh,
        });
    }

    /// The autonomous robot: drive towards the fight along the navigation graph, stop and
    /// shoot whatever enemy it sees.
    fn think_robot(&mut self, i: usize, dt: f32) {
        let time = self.time;
        let clouds = self.smoke_clouds();
        let (pos, team) = (self.vehicles[i].pos, self.vehicles[i].team);
        let gun = pos + vec3(0.0, NRK_GUN_Y, 0.0);
        if time >= self.vehicles[i].next_think {
            self.vehicles[i].next_think = time + 0.4;
            let target = self
                .players
                .iter()
                .flatten()
                .filter(|p| p.alive && p.team != team && p.team.is_playing())
                .filter(|p| p.chest().distance(gun) < 45.0)
                .filter(|p| {
                    self.world.line_of_sight(gun, p.chest())
                        && !smoke_blocks(gun, p.chest(), &clouds)
                })
                .min_by(|a, b| {
                    a.chest()
                        .distance(gun)
                        .partial_cmp(&b.chest().distance(gun))
                        .unwrap()
                })
                .map(|p| p.id);
            self.vehicles[i].target = target;
            // somewhere to go: the nearest enemy we know of, or anywhere on the map
            let v = &self.vehicles[i];
            let stuck = v.stuck_since.0.distance(pos) < 0.5 && time - v.stuck_since.1 > 2.5;
            if (v.path.is_empty() || stuck) && target.is_none() {
                let goal = self
                    .players
                    .iter()
                    .flatten()
                    .filter(|p| p.alive && p.team != team && p.team.is_playing())
                    .min_by(|a, b| {
                        a.body
                            .pos
                            .distance(pos)
                            .partial_cmp(&b.body.pos.distance(pos))
                            .unwrap()
                    })
                    .map(|p| p.body.pos)
                    .filter(|_| !stuck)
                    .unwrap_or_else(|| self.nav.random_point(&mut self.rng));
                let path = self.nav.path(pos, goal).unwrap_or_default();
                let v = &mut self.vehicles[i];
                v.path = path;
                v.stuck_since = (pos, time);
            }
            let v = &mut self.vehicles[i];
            if v.stuck_since.0.distance(pos) > 1.5 {
                v.stuck_since = (pos, time);
            }
        }
        let aim = self.vehicles[i]
            .target
            .and_then(|t| self.p(t))
            .filter(|p| p.alive)
            .map(|p| p.chest());
        let world = &self.world;
        let tuning = self.tuning.movement.clone();
        let nrk_speed = self.tuning.streaks.nrk_speed;
        let v = &mut self.vehicles[i];
        let mut forward = 0.0;
        let mut want_yaw = v.yaw;
        match aim {
            Some(c) => {
                let d = c - gun;
                want_yaw = yaw_of(d);
                v.pitch = (d.y / d.length().max(0.1)).asin();
            }
            None => {
                v.pitch *= 0.9;
                while let Some(&wp) = v.path.first() {
                    if vec3(wp.x - pos.x, 0.0, wp.z - pos.z).length() < 1.0 {
                        v.path.remove(0);
                    } else {
                        break;
                    }
                }
                if let Some(&wp) = v.path.first() {
                    want_yaw = yaw_of(wp - pos);
                    forward = 1.0;
                }
            }
        }
        // the turret turns at a limited rate
        let turn = toon_shared::wrap_angle(want_yaw - v.yaw);
        let max_turn = 3.5 * dt;
        v.yaw = toon_shared::wrap_angle(v.yaw + turn.clamp(-max_turn, max_turn));
        let on_target = aim.is_some() && turn.abs() < 0.12;
        v.keys = if on_target { keys::FIRE } else { 0 };
        if turn.abs() > 0.8 {
            forward = 0.0;
        }
        nrk_step(
            nrk_speed,
            world,
            &mut v.body,
            forward,
            0.0,
            v.yaw,
            &tuning,
            dt,
        );
        v.vel = (v.body.pos - v.pos) / dt.max(1e-3);
        v.pos = v.body.pos;
    }

    /// Damage to a vehicle; `attacker` gets a hit marker.
    pub fn damage_vehicle(&mut self, i: usize, amount: f32, attacker: Option<u8>) {
        let Some(v) = self.vehicles.get_mut(i) else {
            return;
        };
        let team = attacker.and_then(|a| self.players[a as usize].as_ref().map(|p| p.team));
        if team == Some(v.team) && !self.cfg.friendly_fire {
            return;
        }
        v.hp -= amount;
        if let Some(a) = attacker {
            self.outbox.push((
                Dest::One(a),
                S2C::HitConfirm {
                    kill: false,
                    headshot: false,
                },
                true,
            ));
        }
    }

    fn end_vehicle(&mut self, i: usize, end: End) {
        let v = self.vehicles.remove(i);
        if let Some(p) = self.pm(v.owner) {
            if p.driving == Some(v.id) {
                p.driving = None;
            }
        }
        match end {
            End::Detonate(at) => {
                self.explode(
                    at,
                    self.tuning.streaks.fpv_blast_radius,
                    self.tuning.streaks.fpv_blast_damage,
                    Some(v.owner),
                    Cause::Fpv,
                    false,
                );
            }
            End::Destroyed => {
                self.broadcast(S2C::Explosion {
                    pos: v.sphere().0.to_array(),
                    radius: 2.5,
                    big: false,
                });
                let kind = v.kind;
                self.say(Dest::One(v.owner), || S2C::Center {
                    text: tr!("{} знищено", "{} destroyed", kind.name()),
                    secs: 2.0,
                });
            }
            End::Expired => {}
        }
    }

    /// What the pilot's HUD needs.
    pub fn vehicle_hud(&self, id: u8) -> Option<VehicleHud> {
        let vid = self.p(id)?.driving?;
        let v = self.vehicles.iter().find(|v| v.id == vid)?;
        Some(VehicleHud {
            id: v.id,
            kind: v.kind,
            pos: v.pos.to_array(),
            yaw: v.yaw,
            hp: v.hp,
            time_left: (v.until - self.time).max(0.0) as f32,
            ammo: v.ammo,
        })
    }
}
