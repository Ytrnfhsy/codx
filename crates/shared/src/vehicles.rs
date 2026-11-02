//! Remote vehicles from killstreaks: the FPV kamikaze drone, the bomber drone and the
//! ground robot (НРК). Their movement lives here so the pilot's client can predict it and
//! the server can tell how far a vehicle may legally move.

use crate::lang::t;
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
    /// FPV: it ran into something hard (it blows up, or just breaks if not armed yet).
    pub const CRASH: u8 = 16;
}

impl VehicleKind {
    pub fn name(self) -> &'static str {
        match self {
            VehicleKind::Fpv => t("FPV-дрон", "FPV drone"),
            VehicleKind::Bomber => t("Дрон-бомбер", "Bomber drone"),
            VehicleKind::Nrk => t("НРК", "Ground robot"),
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
            // it may hover a while before the pilot finds a target
            VehicleKind::Fpv => 30.0,
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
// It takes off hovering and flies like a small quadcopter: W pushes it where the camera
// looks, A/D slide it sideways, Space/Ctrl climb and sink, and with no keys it brakes to a
// hover. The velocity follows the sticks with some inertia.
/// Cruise speed with W held.
pub const FPV_SPEED: f32 = 20.0;
/// With Shift as well.
pub const FPV_BOOST: f32 = 32.0;
/// Backwards (S), sideways (A/D) and up/down.
pub const FPV_BACK: f32 = 6.0;
pub const FPV_STRAFE: f32 = 7.0;
pub const FPV_CLIMB: f32 = 6.0;
/// How quickly the velocity follows the sticks (1/s), and how quickly it stops on its own.
pub const FPV_ACCEL: f32 = 2.6;
pub const FPV_DRAG: f32 = 2.0;
pub const FPV_RADIUS: f32 = 0.35;
/// Running into something hard at least this fast (m/s, straight at it) ends the drone:
/// the charge goes off, or before it is armed the drone just breaks. Only a feather touch
/// or a glancing scrape slides along.
pub const FPV_CRASH_SPEED: f32 = 3.0;
pub const FPV_BLAST_RADIUS: f32 = 6.5;
pub const FPV_BLAST_DAMAGE: f32 = 230.0;
/// Enemies this close set it off.
pub const FPV_TRIGGER: f32 = 1.6;
/// The charge arms this long after take-off; before that a crash only breaks the drone
/// (so it does not blow up its own pilot).
pub const FPV_ARM_TIME: f32 = 0.7;

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

/// The velocity the FPV drone wants for the held sticks: `fwd`/`right`/`up` are -1..1,
/// forward is along the camera (`yaw`, `pitch`), sideways and up/down are level.
pub fn fpv_target(yaw: f32, pitch: f32, fwd: f32, right: f32, up: f32, boost: bool) -> Vec3 {
    let along = if fwd > 0.0 {
        fwd * if boost { FPV_BOOST } else { FPV_SPEED }
    } else {
        fwd * FPV_BACK
    };
    let (_, r) = crate::yaw_basis(yaw);
    crate::look_dir(yaw, pitch) * along + r * right * FPV_STRAFE + Vec3::Y * up * FPV_CLIMB
}

/// Velocity after `dt` of chasing `target`: speeds up with the sticks, stops on its own.
pub fn fpv_accel(vel: Vec3, target: Vec3, dt: f32) -> Vec3 {
    let k = if target.length_squared() > 0.01 {
        FPV_ACCEL
    } else {
        FPV_DRAG
    };
    vel + (target - vel) * (dt * k).min(1.0)
}

/// Moves the FPV drone by `vel * dt`, one axis at a time so it slides along what it
/// touches. Returns the new position, the velocity left after any contact and how fast it
/// ran into something (0 when it touched nothing).
pub fn fpv_move(world: &World, pos: Vec3, vel: Vec3, dt: f32) -> (Vec3, Vec3, f32) {
    let mut p = pos;
    let mut v = vel;
    let mut impact = 0.0f32;
    for axis in [Vec3::X, Vec3::Y, Vec3::Z] {
        let s = v.dot(axis);
        let d = s * dt;
        if d.abs() < 1e-6 {
            continue;
        }
        let dir = axis * d.signum();
        match world.raycast(p, dir, d.abs() + FPV_RADIUS) {
            Some((t, _)) => {
                p += dir * (t - FPV_RADIUS).max(0.0);
                impact = impact.max(s.abs());
                v -= axis * s;
            }
            None => p += dir * d.abs(),
        }
    }
    (p, v, impact)
}

/// Moves the bomber drone at its fixed altitude; `forward`/`right` are -1..1 relative to
/// `yaw`, and it stays above the map.
pub fn bomber_step(
    pos: Vec3,
    yaw: f32,
    forward: f32,
    right: f32,
    dt: f32,
    min: Vec2,
    max: Vec2,
) -> Vec3 {
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
pub fn nrk_step(
    world: &World,
    body: &mut Body,
    forward: f32,
    right: f32,
    yaw: f32,
    t: &MoveTuning,
    dt: f32,
) {
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
