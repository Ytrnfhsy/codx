//! Every door on every map leads somewhere: from either side of it you can walk away
//! (no container, crate or wall shutting it into a pocket).

use glam::{vec3, Vec3};
use std::collections::{HashSet, VecDeque};
use toon_shared::map::MapId;
use toon_shared::physics::{fits, Stance, World};

const STEP: f32 = 0.35;

/// Standing room at `p` (with a step up or down of up to half a metre).
fn stand(world: &World, p: Vec3) -> Option<Vec3> {
    let f = world.floor_below(p + Vec3::Y * 0.55, 1.1)?;
    let q = vec3(p.x, f + 0.02, p.z);
    fits(world, q, Stance::Stand).then_some(q)
}

/// Walks from `start` over a grid; true when it gets `far` metres from `from`.
fn walks_away(world: &World, start: Vec3, from: Vec3, far: f32) -> bool {
    let Some(s) = stand(world, start) else {
        return false;
    };
    let key = |p: Vec3| ((p.x / STEP).round() as i32, (p.z / STEP).round() as i32);
    let mut seen = HashSet::new();
    let mut q = VecDeque::new();
    seen.insert(key(s));
    q.push_back(s);
    while let Some(p) = q.pop_front() {
        let d = p - from;
        if vec3(d.x, 0.0, d.z).length() >= far {
            return true;
        }
        if seen.len() > 6000 {
            return true;
        }
        for (dx, dz) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
            let n = p + vec3(dx * STEP, 0.0, dz * STEP);
            if seen.contains(&key(n)) {
                continue;
            }
            if let Some(m) = stand(world, n) {
                // (not through the doorway back inside, and not up a wall)
                if (m.y - p.y).abs() < 0.6 {
                    seen.insert(key(n));
                    q.push_back(m);
                }
            }
        }
    }
    false
}

/// Doors you cannot get away from: (map, the door's middle, which side).
fn shut_in() -> Vec<(MapId, Vec3, f32)> {
    let mut bad = Vec::new();
    for id in MapId::EVERY {
        let (map, ways) = toon_shared::map::with_doorways(|| toon_shared::maps::build(id));
        let mut world = World::from_map(&map);
        // (the doors open, as somebody going through has them)
        for k in 0..map.doors.len() {
            world.set_door(k, true);
        }
        // (the island's houses are built in place and then moved: its doors are checked)
        for w in ways.iter().filter(|_| id != MapId::Island) {
            let along = Vec3::Y.cross(w.across);
            let half = (w.width * 0.5 - 0.4).max(0.0);
            for side in [-1.0f32, 1.0] {
                // through any part of the doorway
                let ok = [-half, 0.0, half].iter().any(|&o| {
                    let start = w.at + Vec3::Y * 0.05 + w.across * side * 0.7 + along * o;
                    walks_away(&world, start, w.at, 6.0)
                });
                if !ok {
                    bad.push((id, w.at, side));
                }
            }
        }
        for d in &map.doors {
            let c = d.centre();
            let size = d.shut[1] - d.shut[0];
            let n = if size.x > size.z { Vec3::Z } else { Vec3::X };
            let floor = d.shut[0].y;
            for side in [-1.0f32, 1.0] {
                let start = vec3(c.x, floor + 0.05, c.z) + n * side * 0.7;
                if !walks_away(&world, start, c, 6.0) {
                    bad.push((id, c, side));
                }
            }
        }
        let _ = &mut world;
    }
    bad
}

#[test]
fn every_door_leads_somewhere() {
    let bad = shut_in();
    for (id, c, side) in &bad {
        eprintln!("{id:?}: door at {c:.1} shut in on side {side}");
    }
    assert!(bad.is_empty(), "{} doors lead nowhere", bad.len());
}
