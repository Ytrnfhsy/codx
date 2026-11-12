//! Battle royale jeeps and boats on the server: getting in and out, driving, running people
//! over, taking hits and blowing up.

use crate::game::{Dest, Game};
use glam::{vec3, Vec3};
use toon_shared::cars::{car_step, CarInput, CarKind, CarState, SEATS};
use toon_shared::lang::t;
use toon_shared::protocol::{EntSnap, S2C};
use toon_shared::weapons::Cause;
use toon_shared::Mode;

/// How close you must be to get in.
pub const ENTER_RADIUS: f32 = 3.6;
/// Without input from its driver for this long, a car rolls to a stop.
const INPUT_TIMEOUT: f64 = 0.5;

pub struct Car {
    pub id: u16,
    pub kind: CarKind,
    pub state: CarState,
    pub hp: f32,
    /// Who sits where (the driver first).
    pub seats: [Option<u8>; SEATS],
    pub input: CarInput,
    pub input_at: f64,
}

impl Car {
    /// Centre and radius of the part bullets hit (the body below the sitters' heads).
    pub fn sphere(&self) -> (Vec3, f32) {
        let r = match self.kind {
            CarKind::Jeep => 1.5,
            CarKind::Boat => 1.4,
        };
        (self.state.pos + vec3(0.0, 0.55, 0.0), r)
    }
    pub fn seat_pos(&self, k: usize) -> Vec3 {
        self.state.to_world(self.kind.seat(k))
    }
}

impl Game {
    /// The cars on the map's spots, fresh for a new round.
    pub fn spawn_cars(&mut self) {
        self.cars.clear();
        for k in 0..self.map.car_spots.len() {
            let spot = self.map.car_spots[k];
            let id = self.new_ent_id();
            self.cars.push(Car {
                id,
                kind: spot.kind,
                state: CarState {
                    pos: spot.pos,
                    yaw: spot.yaw,
                    ..Default::default()
                },
                hp: spot.kind.health(),
                seats: [None; SEATS],
                input: CarInput::default(),
                input_at: 0.0,
            });
        }
    }

    /// The car and seat a player sits in.
    pub fn seat_of(&self, id: u8) -> Option<(usize, usize)> {
        self.cars
            .iter()
            .enumerate()
            .find_map(|(i, c)| c.seats.iter().position(|s| *s == Some(id)).map(|k| (i, k)))
    }

    /// Gets in (the driver's seat if it is free, otherwise the first free one).
    pub fn car_enter(&mut self, id: u8, car: u16) {
        if self.mode != Mode::Br || self.seat_of(id).is_some() {
            return;
        }
        let Some(p) = self.p(id) else { return };
        if !p.alive || p.downed || p.in_plane || p.body.chute != toon_shared::physics::Chute::None {
            return;
        }
        let pos = p.body.pos;
        let Some(c) = self.cars.iter_mut().find(|c| c.id == car) else {
            return;
        };
        if c.state.pos.distance(pos) > ENTER_RADIUS + 1.0 {
            return;
        }
        let Some(k) = c.seats.iter().position(|s| s.is_none()) else {
            return;
        };
        c.seats[k] = Some(id);
        let at = c.seat_pos(k);
        if let Some(p) = self.pm(id) {
            p.body.pos = at;
            p.body.vel = Vec3::ZERO;
            p.reload_end = None;
            p.inv.healing = 0.0;
        }
    }

    /// Gets out beside the car (whichever side is free).
    pub fn car_exit(&mut self, id: u8) {
        let Some((i, k)) = self.seat_of(id) else {
            return;
        };
        self.cars[i].seats[k] = None;
        let c = &self.cars[i];
        let (hx, _) = c.kind.half();
        let side = if k % 2 == 0 { -1.0 } else { 1.0 };
        let mut spot = None;
        for s in [side, -side] {
            let q = c.state.to_world(vec3(s * (hx + 0.9), 0.0, 0.0));
            let floor = self
                .world
                .floor_below(q + vec3(0.0, 2.5, 0.0), 6.0)
                .unwrap_or(q.y);
            let at = vec3(q.x, floor, q.z);
            if toon_shared::physics::fits(
                &self.world,
                at + vec3(0.0, 0.05, 0.0),
                toon_shared::physics::Stance::Stand,
            ) {
                spot = Some(at);
                break;
            }
        }
        // (a boat in the middle of the lake: into the water you go)
        let at = spot.unwrap_or(c.state.pos + vec3(0.0, 1.2, 0.0));
        let vel = c.state.velocity();
        let life = self.p(id).map_or(0, |p| p.life);
        if let Some(p) = self.pm(id) {
            p.body.pos = at;
            p.body.vel = vel * 0.5;
            p.body.stance = toon_shared::physics::Stance::Stand;
            p.move_budget = 3.0;
        }
        self.send(
            id,
            S2C::Correction {
                pos: at.to_array(),
                life,
            },
        );
    }

    /// The driver's controls.
    pub fn car_drive(&mut self, id: u8, throttle: i8, steer: i8, brake: bool) {
        let time = self.time;
        if let Some((i, 0)) = self.seat_of(id) {
            let c = &mut self.cars[i];
            c.input = CarInput {
                throttle: throttle as f32 / 100.0,
                steer: steer as f32 / 100.0,
                brake,
            };
            c.input_at = time;
        }
    }

    pub fn update_cars(&mut self, dt: f32) {
        if self.cars.is_empty() {
            return;
        }
        let time = self.time;
        // nobody stays in a seat out of the game (or knocked down)
        for c in 0..self.cars.len() {
            for k in 0..SEATS {
                if let Some(id) = self.cars[c].seats[k] {
                    if !self.p(id).is_some_and(|p| p.alive && !p.downed) {
                        self.cars[c].seats[k] = None;
                    }
                }
            }
        }
        let mut crashes: Vec<(usize, f32)> = Vec::new();
        for (i, c) in self.cars.iter_mut().enumerate() {
            let driven = c.seats[0].is_some() && time - c.input_at < INPUT_TIMEOUT;
            let inp = if driven { c.input } else { CarInput::default() };
            let crash = car_step(&self.world, c.kind, &mut c.state, inp, dt);
            if crash > 9.0 {
                crashes.push((i, crash));
            }
        }
        // the sitters ride along
        for c in &self.cars {
            for k in 0..SEATS {
                let Some(id) = c.seats[k] else { continue };
                let at = c.seat_pos(k);
                let vel = c.state.velocity();
                let yaw = c.state.yaw;
                if let Some(p) = self.players[id as usize].as_mut() {
                    p.body.pos = at;
                    p.body.vel = vel;
                    p.body.on_ground = true;
                    p.body.stance = toon_shared::physics::Stance::Crouch;
                    p.history.clear();
                    let _ = yaw;
                }
            }
        }
        // crashes hurt the car and those inside
        for (i, crash) in crashes {
            let dmg = (crash - 9.0) * 6.0;
            self.cars[i].hp -= dmg;
            let inside: Vec<u8> = self.cars[i].seats.iter().flatten().copied().collect();
            let at = self.cars[i].state.pos;
            for id in inside {
                self.damage(id, None, dmg * 0.25, Cause::World, false, at);
            }
        }
        self.car_run_over();
        // wrecks blow up
        let mut k = 0;
        while k < self.cars.len() {
            if self.cars[k].hp <= 0.0 {
                let c = self.cars.remove(k);
                let driver = c.seats[0];
                let inside: Vec<u8> = c.seats.iter().flatten().copied().collect();
                let at = c.state.pos + vec3(0.0, 0.8, 0.0);
                for id in inside {
                    self.damage(id, None, 60.0, Cause::Grenade, false, at);
                }
                self.explode(at, 6.0, 110.0, driver, Cause::Grenade, true);
                continue;
            }
            k += 1;
        }
    }

    /// A car going fast knocks over whoever is in front of it.
    fn car_run_over(&mut self) {
        let mut hits: Vec<(u8, Option<u8>, f32, Vec3)> = Vec::new();
        for c in &self.cars {
            let speed = c.state.speed.abs();
            if speed < 6.0 {
                continue;
            }
            let (hx, hz) = c.kind.half();
            let dir = c.state.speed.signum();
            let front = c.state.to_world(vec3(0.0, 0.0, hz * dir));
            for p in self.players.iter().flatten() {
                if !p.alive || c.seats.contains(&Some(p.id)) || self.seat_of(p.id).is_some() {
                    continue;
                }
                let rel = p.body.pos - front;
                if rel.y.abs() > 2.0 {
                    continue;
                }
                let along = rel.dot(c.state.forward()) * dir;
                let across = rel.dot(c.state.right()).abs();
                if (-1.0..1.2).contains(&along) && across < hx + 0.4 {
                    let driver = c.seats[0];
                    let friend = driver.is_some_and(|d| self.allied_ids(d, p.id));
                    if !friend {
                        hits.push((p.id, driver, speed * 5.0, front));
                    }
                }
            }
        }
        for (id, driver, dmg, at) in hits {
            self.damage(id, driver, dmg, Cause::World, false, at);
            if let Some(p) = self.pm(id) {
                p.last_damage = p.last_damage.max(0.0);
            }
        }
    }

    /// Bullets and blasts on a car.
    pub fn damage_car(&mut self, i: usize, amount: f32, attacker: Option<u8>) {
        let Some(c) = self.cars.get_mut(i) else {
            return;
        };
        c.hp -= amount;
        if let Some(a) = attacker {
            self.send(
                a,
                S2C::HitConfirm {
                    kill: false,
                    headshot: false,
                },
            );
        }
    }

    pub fn car_snaps(&self) -> impl Iterator<Item = EntSnap> + '_ {
        self.cars.iter().map(|c| EntSnap::Car {
            id: c.id,
            kind: c.kind,
            pos: c.state.pos.to_array(),
            yaw: c.state.yaw,
            speed: c.state.speed,
            hp: ((c.hp / c.kind.health()).clamp(0.0, 1.0) * 100.0) as u8,
            seats: c.seats.map(|s| s.unwrap_or(255)),
        })
    }

    /// Tells a player getting in where the controls are.
    pub fn car_hint(&mut self, id: u8) {
        if let Some((_, k)) = self.seat_of(id) {
            self.say(Dest::One(id), || S2C::Center {
                text: if k == 0 {
                    t(
                        "W/S — газ/гальмо, A/D — кермо, Пробіл — ручник, F — вийти",
                        "W/S — throttle/brake, A/D — steer, Space — handbrake, F — get out",
                    )
                    .into()
                } else {
                    t(
                        "Ти пасажир: можна стріляти. F — вийти",
                        "You are a passenger: you may shoot. F — get out",
                    )
                    .into()
                },
                secs: 3.0,
            });
        }
    }
}
