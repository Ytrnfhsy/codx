//! Jeeps and boats in the battle royale: anybody can get in, the driver steers, the
//! passengers may shoot. The server drives them; the driver's client runs the same step to
//! see its own car answer at once.

use crate::physics::{Aabb, World};
use crate::{look_dir, wrap_angle};
use glam::{vec3, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CarKind {
    Jeep,
    Boat,
}

/// Seats: the driver first.
pub const SEATS: usize = 4;

impl CarKind {
    pub fn name(self) -> &'static str {
        match self {
            CarKind::Jeep => crate::lang::t("Джип", "Jeep"),
            CarKind::Boat => crate::lang::t("Човен", "Boat"),
        }
    }
    pub fn max_speed(self) -> f32 {
        match self {
            CarKind::Jeep => 24.0,
            CarKind::Boat => 16.0,
        }
    }
    pub fn health(self) -> f32 {
        match self {
            CarKind::Jeep => 450.0,
            CarKind::Boat => 300.0,
        }
    }
    /// Half the footprint: across, along.
    pub fn half(self) -> (f32, f32) {
        match self {
            CarKind::Jeep => (1.05, 2.1),
            CarKind::Boat => (1.0, 2.4),
        }
    }
    /// Where a seat is, relative to the car (x to the right, z forward), and how high the
    /// sitter's feet are.
    pub fn seat(self, k: usize) -> Vec3 {
        let (x, z) = [(-0.45, 0.35), (0.45, 0.35), (-0.45, -0.85), (0.45, -0.85)][k % SEATS];
        let y = match self {
            CarKind::Jeep => 0.45,
            CarKind::Boat => 0.15,
        };
        vec3(x, y, z)
    }
}

/// Where a car waits at the start of a round.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CarSpot {
    pub pos: Vec3,
    pub yaw: f32,
    pub kind: CarKind,
}

/// Where a car is and how it moves.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CarState {
    pub pos: Vec3,
    pub yaw: f32,
    /// Along the heading (negative: reversing).
    pub speed: f32,
    /// Falling (off a ledge).
    pub vy: f32,
}

/// What the driver does: throttle and steering (-1..1), handbrake.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct CarInput {
    pub throttle: f32,
    pub steer: f32,
    pub brake: bool,
}

impl CarState {
    pub fn forward(&self) -> Vec3 {
        look_dir(self.yaw, 0.0)
    }
    /// Unit vector to the car's right.
    pub fn right(&self) -> Vec3 {
        let f = self.forward();
        vec3(-f.z, 0.0, f.x)
    }
    /// A point given relative to the car (x right, y up, z forward) in the world.
    pub fn to_world(&self, local: Vec3) -> Vec3 {
        self.pos + self.right() * local.x + Vec3::Y * local.y + self.forward() * local.z
    }
    pub fn velocity(&self) -> Vec3 {
        self.forward() * self.speed + Vec3::Y * self.vy
    }
}

fn water(world: &World) -> Option<f32> {
    world.terrain.as_ref().map(|t| t.water)
}

/// Can the car stand at `p` (its feet at `p.y`)? Returns the height it rests at.
fn rest_at(world: &World, kind: CarKind, s: &CarState, p: Vec3) -> Option<f32> {
    let ground = world.ground(p.x, p.z);
    let (hx, hz) = kind.half();
    let probe = CarState { pos: p, ..*s };
    match kind {
        CarKind::Boat => {
            // only on water deep enough, at its surface
            let w = water(world)?;
            for (x, z) in [(0.0, hz), (-hx, hz * 0.5), (hx, hz * 0.5), (0.0, -hz)] {
                let q = probe.to_world(vec3(x, 0.0, z));
                if world.ground(q.x, q.z).is_some_and(|g| g > w - 0.5) {
                    return None;
                }
            }
            Some(w - 0.15)
        }
        CarKind::Jeep => {
            // not into deep water
            if let (Some(g), Some(w)) = (ground, water(world)) {
                if g < w - 0.45 {
                    return None;
                }
            }
            let floor = world.floor_below(p + vec3(0.0, 1.0, 0.0), 6.0)?;
            // a kerb is fine, a wall is not
            if floor > s.pos.y + 0.8 {
                return None;
            }
            // nothing solid in the way at body height
            for (x, z) in [
                (-hx, hz),
                (hx, hz),
                (0.0, hz),
                (-hx, -hz),
                (hx, -hz),
                (-hx, 0.0),
                (hx, 0.0),
            ] {
                let q = probe.to_world(vec3(x, 0.0, z));
                let bb = Aabb {
                    min: vec3(q.x - 0.15, floor + 0.55, q.z - 0.15),
                    max: vec3(q.x + 0.15, floor + 1.5, q.z + 0.15),
                };
                if world.overlaps(&bb) {
                    return None;
                }
            }
            Some(floor)
        }
    }
}

/// One step of driving. Returns the speed of a crash (0 when nothing was hit).
pub fn car_step(world: &World, kind: CarKind, s: &mut CarState, inp: CarInput, dt: f32) -> f32 {
    let max = kind.max_speed();
    let throttle = inp.throttle.clamp(-1.0, 1.0);
    let steer = inp.steer.clamp(-1.0, 1.0);
    // engine, brakes and rolling resistance
    let mut a = if throttle > 0.05 {
        if s.speed < -0.5 {
            16.0 * throttle
        } else {
            9.0 * throttle
        }
    } else if throttle < -0.05 {
        if s.speed > 0.5 {
            16.0 * throttle
        } else {
            6.0 * throttle
        }
    } else {
        -s.speed.signum() * 3.0
    };
    if inp.brake {
        a = -s.speed.signum() * 18.0;
    }
    let before = s.speed;
    s.speed += a * dt;
    if throttle.abs() <= 0.05 && before.signum() != s.speed.signum() {
        s.speed = 0.0;
    }
    s.speed = s.speed.clamp(-max * 0.3, max);
    // steering bites harder the faster you go (up to a point), and reverses backwards
    let grip = (s.speed / 6.0).clamp(-1.0, 1.0);
    let rate = match kind {
        CarKind::Jeep => 1.5,
        CarKind::Boat => 1.0,
    };
    s.yaw = wrap_angle(s.yaw + steer * rate * grip * dt);

    let next = s.pos + s.forward() * s.speed * dt;
    match rest_at(world, kind, s, next) {
        Some(y) => {
            s.pos.x = next.x;
            s.pos.z = next.z;
            if y >= s.pos.y - 0.4 || kind == CarKind::Boat {
                // following the ground (or the water)
                s.pos.y = y;
                s.vy = 0.0;
            } else {
                // off a ledge
                s.vy -= 20.0 * dt;
                s.pos.y = (s.pos.y + s.vy * dt).max(y);
                if s.pos.y <= y {
                    s.vy = 0.0;
                }
            }
            0.0
        }
        None => {
            // hit something: bounce back a little
            let crash = s.speed.abs();
            s.speed = -s.speed * 0.25;
            crash
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::MapId;

    #[test]
    fn a_jeep_drives_and_stops_at_walls_a_boat_stays_afloat() {
        let map = crate::maps::build(MapId::Island);
        let world = World::from_map(&map);
        let spot = map
            .car_spots
            .iter()
            .find(|c| c.kind == CarKind::Jeep)
            .unwrap();
        let mut s = CarState {
            pos: spot.pos,
            yaw: spot.yaw,
            ..Default::default()
        };
        let start = s.pos;
        let go = CarInput {
            throttle: 1.0,
            ..Default::default()
        };
        for _ in 0..60 {
            car_step(&world, CarKind::Jeep, &mut s, go, 1.0 / 30.0);
        }
        assert!(s.speed > 10.0, "speed {}", s.speed);
        assert!(s.pos.distance(start) > 10.0);
        // the wheels stay on the ground
        let g = world.floor_below(s.pos + Vec3::Y, 3.0).unwrap();
        assert!((s.pos.y - g).abs() < 0.5, "{} vs ground {g}", s.pos.y);

        // a boat on the lake floats at the surface and never climbs the shore
        let spot = map
            .car_spots
            .iter()
            .find(|c| c.kind == CarKind::Boat)
            .unwrap();
        let mut b = CarState {
            pos: spot.pos,
            yaw: spot.yaw,
            ..Default::default()
        };
        let w = world.terrain.as_ref().unwrap().water;
        for _ in 0..30 * 40 {
            car_step(&world, CarKind::Boat, &mut b, go, 1.0 / 30.0);
            assert!(
                world.ground(b.pos.x, b.pos.z).unwrap() < w,
                "the boat ran ashore at {}",
                b.pos
            );
        }
        assert!((b.pos.y - (w - 0.15)).abs() < 0.01);
    }
}
