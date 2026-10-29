//! Remote vehicles from killstreaks: the FPV kamikaze drone, the bomber drone and the
//! ground robot (НРК). Their movement lives here so the pilot's client can predict it and
//! the server can tell how far a vehicle may legally move.

use crate::physics::{Body, MoveInput, Stance, World};
use crate::tuning::MoveTuning;
use crate::{vec3, Vec2, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum VehicleKind {
    Fpv,
    Bomber,
    Nrk,
}

/// Control bits sent with `C2S::Drive`.
pub mod keys {
    /// FPV: detonate · bomber: drop a bomb · robot: machine gun.
    pub const FIRE: u8 = 1;
    /// Leave the vehicle early (the FPV detonates, the others are lost).
    pub const EXIT: u8 = 2;
    /// FPV: speed up.
    pub const BOOST: u8 = 4;
    /// FPV: slow down.
    pub const BRAKE: u8 = 8;
}

impl VehicleKind {
    pub fn name(self) -> &'static str {
        match self {
            VehicleKind::Fpv => "FPV-дрон",
            VehicleKind::Bomber => "Дрон-бомбер",
            VehicleKind::Nrk => "НРК",
        }
    }
    pub fn health(self) -> f32 {
        match self {
            VehicleKind::Fpv => 40.0,
            VehicleKind::Bomber => 220.0,
            VehicleKind::Nrk => 700.0,
        }
    }
    /// Seconds it lasts.
    pub fn duration(self) -> f32 {
        match self {
            VehicleKind::Fpv => 20.0,
            VehicleKind::Bomber => 45.0,
            VehicleKind::Nrk => 60.0,
        }
    }
    /// Hit sphere: radius and height of its centre above `pos`.
    pub fn hit_sphere(self) -> (f32, f32) {
        match self {
            VehicleKind::Fpv => (0.4, 0.0),
            VehicleKind::Bomber => (1.0, 0.0),
            VehicleKind::Nrk => (0.85, 0.6),
        }
    }
    /// Fastest legal speed (m/s), for the server's movement checks.
    pub fn max_speed(self) -> f32 {
        match self {
            VehicleKind::Fpv => FPV_BOOST,
            VehicleKind::Bomber => BOMBER_SPEED,
            VehicleKind::Nrk => NRK_SPEED,
        }
    }
    /// Bombs (bomber) or nothing.
    pub fn ammo(self) -> u8 {
        match self {
            VehicleKind::Bomber => BOMBER_DROPS,
            _ => 0,
        }
    }
}

// ---- FPV kamikaze drone
pub const FPV_SPEED: f32 = 22.0;
pub const FPV_BOOST: f32 = 32.0;
pub const FPV_SLOW: f32 = 12.0;
/// How hard the drone turns towards a new speed (1/s).
pub const FPV_ACCEL: f32 = 3.0;
pub const FPV_RADIUS: f32 = 0.35;
pub const FPV_BLAST_RADIUS: f32 = 6.5;
pub const FPV_BLAST_DAMAGE: f32 = 230.0;
/// Enemies this close set it off.
pub const FPV_TRIGGER: f32 = 1.6;

// ---- bomber drone
pub const BOMBER_ALT: f32 = 19.0;
pub const BOMBER_SPEED: f32 = 11.0;
pub const BOMBER_DROPS: u8 = 6;
pub const BOMBER_COOLDOWN: f32 = 0.9;
pub const DROP_RADIUS: f32 = 5.0;
pub const DROP_DAMAGE: f32 = 175.0;

// ---- ground robot
pub const NRK_SPEED: f32 = 5.0;
pub const NRK_FIRE_INTERVAL: f32 = 0.1;
pub const NRK_DAMAGE: f32 = 20.0;
pub const NRK_SPREAD: f32 = 0.022;
/// Turret muzzle above `pos`.
pub const NRK_GUN_Y: f32 = 1.05;

/// Moves the FPV drone along `dir`. Returns the new position and whether it hit something
/// (it then stops at the obstacle).
pub fn fpv_step(world: &World, pos: Vec3, dir: Vec3, speed: f32, dt: f32) -> (Vec3, bool) {
    let len = speed * dt;
    match world.raycast(pos, dir, len + FPV_RADIUS) {
        Some((t, _)) => (pos + dir * (t - FPV_RADIUS).max(0.0), true),
        None => (pos + dir * len, false),
    }
}

/// Moves the bomber drone at its fixed altitude; `forward`/`right` are -1..1 relative to
/// `yaw`, and it stays above the map.
pub fn bomber_step(pos: Vec3, yaw: f32, forward: f32, right: f32, dt: f32, min: Vec2, max: Vec2) -> Vec3 {
    let (f, r) = crate::yaw_basis(yaw);
    let mut v = f * forward + r * right;
    if v.length_squared() > 1.0 {
        v = v.normalize();
    }
    let p = pos + v * BOMBER_SPEED * dt;
    vec3(p.x.clamp(min.x, max.x), BOMBER_ALT, p.z.clamp(min.y, max.y))
}

/// Moves the ground robot with the same physics as soldiers (walls, stairs, gravity),
/// as a low, crouch-height body.
pub fn nrk_step(world: &World, body: &mut Body, forward: f32, right: f32, yaw: f32, t: &MoveTuning, dt: f32) {
    let inp = MoveInput {
        forward,
        right,
        yaw,
        want_stance: Stance::Crouch,
        speed_mult: NRK_SPEED / t.crouch_speed.max(0.1),
        ..Default::default()
    };
    body.stance = Stance::Crouch;
    body.step(world, &inp, t, dt);
}

/// Where a bomb dropped from `pos` moving at `vel` lands on flat ground at `ground_y`.
pub fn drop_landing(pos: Vec3, vel: Vec3, gravity: f32, ground_y: f32) -> Vec3 {
    let h = (pos.y - ground_y).max(0.0);
    // h = vy t + g t² / 2 (vy downwards)
    let vy = -vel.y;
    let t = (-vy + (vy * vy + 2.0 * gravity * h).sqrt()) / gravity.max(0.1);
    vec3(pos.x + vel.x * t, ground_y, pos.z + vel.z * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drop_landing_follows_the_drone() {
        let p = drop_landing(vec3(0.0, 19.0, 0.0), vec3(5.0, 0.0, 0.0), 20.0, 0.0);
        // 19 m at g = 20 takes ~1.38 s
        assert!((p.x - 6.9).abs() < 0.1, "{p}");
        let p = drop_landing(vec3(1.0, 10.0, 2.0), Vec3::ZERO, 20.0, 0.0);
        assert_eq!((p.x, p.z), (1.0, 2.0));
    }

    #[test]
    fn bomber_stays_over_the_map() {
        let min = crate::vec2(-10.0, -10.0);
        let max = crate::vec2(10.0, 10.0);
        let mut p = vec3(9.0, BOMBER_ALT, 0.0);
        for _ in 0..100 {
            p = bomber_step(p, -std::f32::consts::FRAC_PI_2, 1.0, 0.0, 0.1, min, max);
        }
        assert!(p.x <= 10.0 && p.y == BOMBER_ALT);
    }
}
