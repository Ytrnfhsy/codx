//! Bullets that fly: every gun has a muzzle velocity and a range, bullets drop under
//! gravity and take time to arrive. Far targets need the sights held higher, and running
//! ones need leading.
//!
//! The world never moves, so a bullet's path through it (where it stops, the planks and
//! cloth it goes through) is worked out once when it is fired; only players and vehicles
//! are checked as the bullet travels along that path, tick after tick.

use crate::physics::World;
use crate::weapons::{WeaponClass, WeaponId};
use glam::{vec3, Vec3};

pub const GRAVITY: f32 = 9.81;
/// Length in seconds of one piece of a bullet's path.
pub const STEP: f32 = 1.0 / 60.0;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Ballistics {
    /// Muzzle velocity, m/s.
    pub velocity: f32,
    /// The bullet is spent (falls harmlessly) after this many metres.
    pub max_range: f32,
    /// Distance at which bots open fire with it.
    pub engage: f32,
    /// How well it goes through solid walls (0 = not at all; see `WeaponId::pierce`).
    pub pierce: f32,
}

impl Ballistics {
    pub fn of(w: WeaponId) -> Ballistics {
        let (velocity, max_range) = match w {
            WeaponId::Uar15 => (900.0, 600.0),
            WeaponId::Fort221 => (880.0, 550.0),
            WeaponId::Ak12 => (715.0, 550.0),
            WeaponId::ScarH => (800.0, 700.0),
            WeaponId::Vector => (300.0, 180.0),
            WeaponId::Mp7 => (720.0, 220.0),
            WeaponId::Bizon => (330.0, 200.0),
            WeaponId::Svd => (830.0, 900.0),
            WeaponId::Mk14 => (850.0, 800.0),
            WeaponId::Negev => (900.0, 700.0),
            WeaponId::Pkm => (825.0, 800.0),
            WeaponId::Aa12 => (350.0, 60.0),
            WeaponId::Ksg => (400.0, 75.0),
            WeaponId::Awm => (900.0, 1300.0),
            WeaponId::Barrett => (850.0, 1500.0),
            WeaponId::Alligator => (1000.0, 1800.0),
            WeaponId::Fort17 => (330.0, 140.0),
            WeaponId::Glock18 => (360.0, 120.0),
            WeaponId::Revolver => (440.0, 170.0),
            // (slow enough to watch fly, and to fall: aim a little up far away)
            WeaponId::Rpg7 => (115.0, 500.0),
            WeaponId::Crossbow => (85.0, 300.0),
            WeaponId::Bow => (70.0, 250.0),
        };
        let engage = match w.def().class {
            WeaponClass::Pistol => 45.0,
            WeaponClass::Smg => 75.0,
            WeaponClass::Shotgun => 22.0,
            WeaponClass::Rifle => 170.0,
            WeaponClass::Lmg => 200.0,
            WeaponClass::Marksman => 300.0,
            WeaponClass::Sniper => 420.0,
            WeaponClass::Launcher => 90.0,
        };
        Ballistics {
            velocity,
            max_range,
            engage,
            pierce: w.pierce(),
        }
    }

    /// Seconds to cover `dist` metres (drag is ignored: the bullet keeps its speed).
    pub fn flight_time(&self, dist: f32) -> f32 {
        dist / self.velocity
    }

    /// How far below the line of the barrel the bullet is after `dist` metres.
    pub fn drop_at(&self, dist: f32) -> f32 {
        let t = self.flight_time(dist);
        0.5 * GRAVITY * t * t
    }
}

/// One step of a bullet's flight.
pub fn step(pos: Vec3, vel: Vec3, dt: f32) -> (Vec3, Vec3) {
    let nv = vel - vec3(0.0, GRAVITY * dt, 0.0);
    (pos + (vel + nv) * 0.5 * dt, nv)
}

/// A bullet's path through the world.
#[derive(Clone, Debug, Default)]
pub struct Path {
    /// Points `STEP` seconds apart (the last one is where the bullet stops).
    pub points: Vec<Vec3>,
    /// Distance travelled at each point.
    pub dist: Vec<f32>,
    /// Wood and cloth on the way: distance along the path and the share of damage left.
    pub pierced: Vec<(f32, f32)>,
    /// Stopped by a wall or the ground (rather than running out of range).
    pub hit_world: bool,
}

impl Path {
    pub fn end(&self) -> Vec3 {
        *self.points.last().unwrap_or(&Vec3::ZERO)
    }
    pub fn length(&self) -> f32 {
        *self.dist.last().unwrap_or(&0.0)
    }
    /// Share of the damage left at `dist` metres along the path.
    pub fn left_at(&self, dist: f32) -> f32 {
        self.pierced
            .iter()
            .filter(|p| p.0 < dist)
            .map(|p| p.1)
            .product()
    }
    /// The point `dist` metres along the path.
    pub fn point_at(&self, dist: f32) -> Vec3 {
        let i = self.dist.iter().position(|d| *d >= dist).unwrap_or(0);
        if i == 0 {
            return self.points.first().copied().unwrap_or(Vec3::ZERO);
        }
        let (da, db) = (self.dist[i - 1], self.dist[i]);
        let k = ((dist - da) / (db - da).max(1e-4)).clamp(0.0, 1.0);
        self.points[i - 1].lerp(self.points[i], k)
    }
    /// Index of the piece the bullet is on `t` seconds after it was fired.
    pub fn piece_at(&self, t: f32) -> usize {
        ((t / STEP).ceil().max(0.0) as usize).min(self.points.len().saturating_sub(1))
    }
    /// Where the bullet is `t` seconds after it was fired.
    pub fn pos_at(&self, t: f32) -> Vec3 {
        let k = (t / STEP).max(0.0);
        let i = k.floor() as usize;
        if i + 1 >= self.points.len() {
            return self.end();
        }
        self.points[i].lerp(self.points[i + 1], k - i as f32)
    }
}

/// Flies a bullet from `o` along `dir` through the world until it hits something solid or
/// runs out of range.
pub fn trace_path(world: &World, o: Vec3, dir: Vec3, b: &Ballistics) -> Path {
    let mut path = Path {
        points: vec![o],
        dist: vec![0.0],
        ..Default::default()
    };
    let mut pos = o;
    let mut vel = dir.normalize_or_zero() * b.velocity;
    let mut total = 0.0;
    // (a safety net: no bullet flies longer than a few seconds)
    for _ in 0..(6.0 / STEP) as usize {
        let (np, nv) = step(pos, vel, STEP);
        let seg = np - pos;
        let len = seg.length();
        if len < 1e-4 {
            break;
        }
        let d = seg / len;
        let max = len.min(b.max_range - total);
        let (t, _, pierced) = world.bullet_trace(pos, d, max);
        for p in &pierced {
            if let Some(m) = p.mat.bullet_pass() {
                path.pierced.push((total + p.t, m));
            }
        }
        if t < max - 1e-4 {
            // a heavy round goes on through a thin enough wall (never the ground)
            if let Some((thick, left)) = through_wall(world, pos + d * t, d, b.pierce) {
                path.pierced.push((total + t, left));
                let skip = t + thick + 0.02;
                total += skip;
                pos += d * skip;
                vel = d * vel.length();
                path.points.push(pos);
                path.dist.push(total);
                if total >= b.max_range - 1e-3 {
                    break;
                }
                continue;
            }
            path.points.push(pos + d * t);
            path.dist.push(total + t);
            path.hit_world = true;
            return path;
        }
        total += max;
        pos += d * max;
        vel = nv;
        path.points.push(pos);
        path.dist.push(total);
        if total >= b.max_range - 1e-3 {
            break;
        }
    }
    path
}

/// How thick the wall struck at `at` is along `d`, if a round with `pierce` goes through
/// it, and the share of its force it keeps.
fn through_wall(world: &World, at: Vec3, d: Vec3, pierce: f32) -> Option<(f32, f32)> {
    if pierce <= 0.0 {
        return None;
    }
    if let Some(t) = &world.terrain {
        if t.height(at.x, at.z) >= at.y - 0.08 {
            return None;
        }
    }
    let max = pierce * 1.2;
    // look back from beyond the wall: the first face met is where it comes out
    let far = at + d * (max + 0.02);
    let (tb, _, _) = world.bullet_trace(far, -d, max + 0.02);
    let thick = max + 0.02 - tb;
    if tb <= 0.01 || thick >= max {
        return None;
    }
    Some((thick, pierce * (1.0 - 0.5 * thick / max)))
}

/// Where to aim to hit something at `target` moving at `vel` with a bullet of speed
/// `speed`: ahead of it by the flight time, and above it by the drop. Also returns the
/// flight time.
pub fn lead(from: Vec3, target: Vec3, vel: Vec3, speed: f32) -> (Vec3, f32) {
    let mut t = from.distance(target) / speed;
    let mut at = target;
    for _ in 0..3 {
        at = target + vel * t;
        t = from.distance(at) / speed;
    }
    (at + vec3(0.0, 0.5 * GRAVITY * t * t, 0.0), t)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::{Map, MapId};
    use glam::vec2;

    /// Open sky, nothing to hit.
    fn empty() -> World {
        World::from_map(&Map {
            id: MapId::Depot,
            name: "test",
            min: vec2(-2000.0, -2000.0),
            max: vec2(2000.0, 2000.0),
            boxes: Vec::new(),
            ladders: Vec::new(),
            spawns: [Vec::new(), Vec::new()],
            bomb: Vec3::ZERO,
            sites: [Vec3::ZERO; 2],
            sky: [0; 3],
            fog: [0; 3],
            tint: [1.0; 3],
            terrain: None,
            loot: Vec::new(),
            nav_detail: Vec::new(),
            car_spots: Vec::new(),
            doors: Vec::new(),
        })
    }

    #[test]
    fn bullets_drop_and_take_time() {
        let world = empty();
        let b = Ballistics::of(WeaponId::Uar15);
        let p = trace_path(&world, vec3(0.0, 100.0, 0.0), Vec3::X, &b);
        assert!(!p.hit_world);
        assert!((p.length() - b.max_range).abs() < 1.0, "{}", p.length());
        // after 400 m the bullet is about a metre lower
        let i = p.dist.iter().position(|d| *d >= 400.0).unwrap();
        let fell = 100.0 - p.points[i].y;
        assert!((fell - b.drop_at(400.0)).abs() < 0.15, "fell {fell}");
        assert!(fell > 0.8 && fell < 1.3);
        // and needs about half a second to get there
        assert!((i as f32 * STEP - 400.0 / 880.0).abs() < 0.03);
        // a pistol bullet runs out much sooner
        let pistol = trace_path(
            &world,
            Vec3::ZERO,
            Vec3::X,
            &Ballistics::of(WeaponId::Fort17),
        );
        assert!(pistol.length() < 160.0);
    }

    #[test]
    fn leading_a_runner() {
        let from = Vec3::ZERO;
        let target = vec3(200.0, 0.0, 0.0);
        let vel = vec3(0.0, 0.0, 6.0);
        let (at, t) = lead(from, target, vel, 800.0);
        assert!((t - 0.25).abs() < 0.01);
        assert!((at.z - 1.5).abs() < 0.05, "{at}");
        assert!(at.y > 0.25 && at.y < 0.35, "{at}");
        // a bullet fired at that point meets the runner
        let b = Ballistics {
            velocity: 800.0,
            max_range: 1000.0,
            engage: 0.0,
            pierce: 0.0,
        };
        let p = trace_path(&empty(), from, (at - from).normalize(), &b);
        let hit = p.pos_at(t);
        let runner = target + vel * t;
        assert!(hit.distance(runner) < 0.15, "{hit} vs {runner}");
    }

    /// A world with one wall `thick` metres thick across the line of fire at x = 20.
    fn walled(thick: f32) -> World {
        let mut w = empty();
        w.add_map_box(&crate::map::MapBox {
            min: vec3(20.0, -5.0, -5.0),
            max: vec3(20.0 + thick, 5.0, 5.0),
            color: [0; 3],
            solid: true,
            visible: true,
            mat: crate::map::Mat::Solid,
            shade: 0,
        });
        w
    }

    #[test]
    fn heavy_rounds_go_through_thin_walls() {
        let o = vec3(0.0, 1.0, 0.0);
        let thin = walled(0.25);
        let rifle = trace_path(&thin, o, Vec3::X, &Ballistics::of(WeaponId::Uar15));
        assert!(rifle.hit_world && rifle.end().x < 20.1, "{}", rifle.end());
        let big = trace_path(&thin, o, Vec3::X, &Ballistics::of(WeaponId::Barrett));
        assert!(big.length() > 100.0, "the .50 goes on: {}", big.end());
        let left = big.left_at(50.0);
        assert!(left > 0.2 && left < 0.5, "but weaker: {left}");
        // a thick wall stops even it; the 14.5 mm gun still goes through
        let thick = walled(0.7);
        let big = trace_path(&thick, o, Vec3::X, &Ballistics::of(WeaponId::Barrett));
        assert!(big.end().x < 20.1);
        let huge = trace_path(&thick, o, Vec3::X, &Ballistics::of(WeaponId::Alligator));
        assert!(huge.length() > 100.0);
    }
}
