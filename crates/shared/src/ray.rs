//! Ray tests: ray vs AABB and player hitboxes (oriented by yaw).

use crate::physics::Stance;
use glam::{vec3, Quat, Vec3};
use serde::{Deserialize, Serialize};

/// Slab test. `inv` is 1/dir. Returns entry distance (0 if starting inside) and the hit normal.
pub fn ray_aabb(o: Vec3, inv: Vec3, min: Vec3, max: Vec3) -> Option<(f32, Vec3)> {
    let t1 = (min - o) * inv;
    let t2 = (max - o) * inv;
    let tmin_v = t1.min(t2);
    let tmax_v = t1.max(t2);
    // NaN-safe (0 * inf) handling: treat NaN as "inside the slab"
    let fix = |v: f32, d: f32| if v.is_nan() { d } else { v };
    let tx0 = fix(tmin_v.x, f32::NEG_INFINITY);
    let ty0 = fix(tmin_v.y, f32::NEG_INFINITY);
    let tz0 = fix(tmin_v.z, f32::NEG_INFINITY);
    let tx1 = fix(tmax_v.x, f32::INFINITY);
    let ty1 = fix(tmax_v.y, f32::INFINITY);
    let tz1 = fix(tmax_v.z, f32::INFINITY);
    let tmin = tx0.max(ty0).max(tz0);
    let tmax = tx1.min(ty1).min(tz1);
    if tmax < 0.0 || tmin > tmax {
        return None;
    }
    let n = if tmin == tx0 {
        vec3(-inv.x.signum(), 0.0, 0.0)
    } else if tmin == ty0 {
        vec3(0.0, -inv.y.signum(), 0.0)
    } else {
        vec3(0.0, 0.0, -inv.z.signum())
    };
    Some((tmin.max(0.0), n))
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum HitZone {
    Head,
    Torso,
    Legs,
}

impl HitZone {
    pub fn mult(self, head_mult: f32) -> f32 {
        match self {
            HitZone::Head => head_mult,
            HitZone::Torso => 1.0,
            HitZone::Legs => 0.85,
        }
    }
}

/// Local-space hitboxes (model faces -Z) for a stance. They follow the chibi characters:
/// a big round head on a small body.
pub fn hitboxes(stance: Stance) -> [(HitZone, Vec3, Vec3); 3] {
    match stance {
        Stance::Stand => [
            (
                HitZone::Head,
                vec3(-0.38, 1.16, -0.36),
                vec3(0.38, 1.9, 0.36),
            ),
            (HitZone::Torso, vec3(-0.3, 0.58, -0.2), vec3(0.3, 1.16, 0.2)),
            (
                HitZone::Legs,
                vec3(-0.24, 0.0, -0.16),
                vec3(0.24, 0.58, 0.16),
            ),
        ],
        Stance::Crouch => [
            (
                HitZone::Head,
                vec3(-0.38, 0.86, -0.4),
                vec3(0.38, 1.6, 0.32),
            ),
            (
                HitZone::Torso,
                vec3(-0.3, 0.28, -0.25),
                vec3(0.3, 0.86, 0.25),
            ),
            (HitZone::Legs, vec3(-0.3, 0.0, -0.42), vec3(0.3, 0.3, 0.25)),
        ],
        Stance::Prone => [
            (
                HitZone::Head,
                vec3(-0.38, 0.0, -1.12),
                vec3(0.38, 0.62, -0.36),
            ),
            (HitZone::Torso, vec3(-0.3, 0.0, -0.38), vec3(0.3, 0.42, 0.2)),
            (HitZone::Legs, vec3(-0.26, 0.0, 0.2), vec3(0.26, 0.3, 0.8)),
        ],
    }
}

/// Ray vs a player's hitboxes. Returns the closest (distance, zone).
pub fn ray_player(
    o: Vec3,
    d: Vec3,
    pos: Vec3,
    yaw: f32,
    stance: Stance,
    max_dist: f32,
) -> Option<(f32, HitZone)> {
    let inv_rot = Quat::from_rotation_y(-yaw);
    let lo = inv_rot * (o - pos);
    let ld = inv_rot * d;
    // quick reject using a bounding sphere
    let c = vec3(0.0, 0.8, 0.0);
    let oc = lo - c;
    let b = oc.dot(ld);
    let cc = oc.length_squared() - 1.6 * 1.6;
    if cc > 0.0 && b > 0.0 {
        return None;
    }
    if b * b - cc < 0.0 {
        return None;
    }
    let inv = vec3(1.0 / ld.x, 1.0 / ld.y, 1.0 / ld.z);
    let mut best: Option<(f32, HitZone)> = None;
    for (zone, mn, mx) in hitboxes(stance) {
        if let Some((t, _)) = ray_aabb(lo, inv, mn, mx) {
            if t <= max_dist && best.is_none_or(|(bt, _)| t < bt) {
                best = Some((t, zone));
            }
        }
    }
    best
}

/// Ray vs sphere (helicopter etc.).
pub fn ray_sphere(o: Vec3, d: Vec3, c: Vec3, r: f32) -> Option<f32> {
    let oc = o - c;
    let b = oc.dot(d);
    let cc = oc.length_squared() - r * r;
    let disc = b * b - cc;
    if disc < 0.0 {
        return None;
    }
    let t = -b - disc.sqrt();
    if t >= 0.0 {
        Some(t)
    } else if cc < 0.0 {
        Some(0.0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aabb_hit() {
        let d = vec3(1.0, 0.0, 0.0);
        let inv = vec3(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let (t, n) = ray_aabb(
            vec3(0.0, 0.5, 0.5),
            inv,
            vec3(2.0, 0.0, 0.0),
            vec3(3.0, 1.0, 1.0),
        )
        .unwrap();
        assert!((t - 2.0).abs() < 1e-5);
        assert_eq!(n, vec3(-1.0, 0.0, 0.0));
        assert!(ray_aabb(
            vec3(0.0, 2.0, 0.5),
            inv,
            vec3(2.0, 0.0, 0.0),
            vec3(3.0, 1.0, 1.0)
        )
        .is_none());
    }

    #[test]
    fn headshot_and_body() {
        let pos = vec3(0.0, 0.0, -10.0);
        let (_, z) = ray_player(
            vec3(0.0, 1.62, 0.0),
            vec3(0.0, 0.0, -1.0),
            pos,
            0.0,
            Stance::Stand,
            100.0,
        )
        .unwrap();
        assert_eq!(z, HitZone::Head);
        let (t, z) = ray_player(
            vec3(0.0, 1.0, 0.0),
            vec3(0.0, 0.0, -1.0),
            pos,
            1.3,
            Stance::Stand,
            100.0,
        )
        .unwrap();
        assert_eq!(z, HitZone::Torso);
        assert!(t > 9.0 && t < 10.0);
        assert!(ray_player(
            vec3(2.0, 1.0, 0.0),
            vec3(0.0, 0.0, -1.0),
            pos,
            0.0,
            Stance::Stand,
            100.0
        )
        .is_none());
        // prone target: head is in front of the body along its facing direction
        let (_, z) = ray_player(
            vec3(0.0, 5.0, -10.8),
            vec3(0.0, -1.0, 0.0),
            pos,
            0.0,
            Stance::Prone,
            100.0,
        )
        .unwrap();
        assert_eq!(z, HitZone::Head);
    }
}
