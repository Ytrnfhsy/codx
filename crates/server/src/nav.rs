//! Ground-level navigation grid with A* path finding for bots.

use glam::{vec3, Vec3};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use toon_shared::map::Map;
use toon_shared::physics::{Aabb, World};
use toon_shared::rng::Rng;

const CELL: f32 = 1.0;

pub struct Nav {
    ox: f32,
    oz: f32,
    pub w: i32,
    pub h: i32,
    walk: Vec<bool>,
}

#[derive(PartialEq)]
struct Node {
    f: f32,
    i: usize,
}
impl Eq for Node {}
impl PartialOrd for Node {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Node {
    fn cmp(&self, o: &Self) -> Ordering {
        o.f.partial_cmp(&self.f).unwrap_or(Ordering::Equal)
    }
}

impl Nav {
    pub fn build(map: &Map, world: &World) -> Nav {
        let ox = map.min.x;
        let oz = map.min.y;
        let w = ((map.max.x - map.min.x) / CELL).ceil() as i32;
        let h = ((map.max.y - map.min.y) / CELL).ceil() as i32;
        let mut walk = vec![false; (w * h) as usize];
        for j in 0..h {
            for i in 0..w {
                let x = ox + (i as f32 + 0.5) * CELL;
                let z = oz + (j as f32 + 0.5) * CELL;
                let bb = Aabb {
                    min: vec3(x - 0.42, 0.05, z - 0.42),
                    max: vec3(x + 0.42, 1.85, z + 0.42),
                };
                walk[(j * w + i) as usize] = !world.overlaps(&bb);
            }
        }
        Nav { ox, oz, w, h, walk }
    }

    fn idx(&self, i: i32, j: i32) -> Option<usize> {
        if i < 0 || j < 0 || i >= self.w || j >= self.h {
            None
        } else {
            Some((j * self.w + i) as usize)
        }
    }

    pub fn walkable(&self, i: i32, j: i32) -> bool {
        self.idx(i, j).map(|k| self.walk[k]).unwrap_or(false)
    }

    pub fn cell_of(&self, p: Vec3) -> (i32, i32) {
        (
            ((p.x - self.ox) / CELL).floor() as i32,
            ((p.z - self.oz) / CELL).floor() as i32,
        )
    }

    pub fn center(&self, i: i32, j: i32) -> Vec3 {
        vec3(
            self.ox + (i as f32 + 0.5) * CELL,
            0.0,
            self.oz + (j as f32 + 0.5) * CELL,
        )
    }

    pub fn nearest_walkable(&self, c: (i32, i32)) -> Option<(i32, i32)> {
        if self.walkable(c.0, c.1) {
            return Some(c);
        }
        for r in 1..6i32 {
            for dj in -r..=r {
                for di in -r..=r {
                    if di.abs() != r && dj.abs() != r {
                        continue;
                    }
                    if self.walkable(c.0 + di, c.1 + dj) {
                        return Some((c.0 + di, c.1 + dj));
                    }
                }
            }
        }
        None
    }

    pub fn random_point(&self, rng: &mut Rng) -> Vec3 {
        for _ in 0..200 {
            let i = rng.below(self.w as usize) as i32;
            let j = rng.below(self.h as usize) as i32;
            if self.walkable(i, j) {
                return self.center(i, j);
            }
        }
        self.center(self.w / 2, self.h / 2)
    }

    /// Walkable straight line between two points (sampled).
    pub fn clear_line(&self, a: Vec3, b: Vec3) -> bool {
        let d = b - a;
        let len = vec3(d.x, 0.0, d.z).length();
        let n = (len / 0.35).ceil().max(1.0) as i32;
        for k in 0..=n {
            let p = a + d * (k as f32 / n as f32);
            let (i, j) = self.cell_of(p);
            if !self.walkable(i, j) {
                return false;
            }
        }
        true
    }

    /// A* path from `a` to `b`. Returns waypoints (excluding the start).
    pub fn path(&self, a: Vec3, b: Vec3) -> Option<Vec<Vec3>> {
        let s = self.nearest_walkable(self.cell_of(a))?;
        let g = self.nearest_walkable(self.cell_of(b))?;
        let si = self.idx(s.0, s.1)?;
        let gi = self.idx(g.0, g.1)?;
        let n = self.walk.len();
        let mut cost = vec![f32::INFINITY; n];
        let mut from = vec![usize::MAX; n];
        let mut open = BinaryHeap::new();
        let hfn = |i: usize| {
            let (x, z) = ((i as i32 % self.w), (i as i32 / self.w));
            let dx = (x - g.0).abs() as f32;
            let dz = (z - g.1).abs() as f32;
            dx.max(dz) + 0.414 * dx.min(dz)
        };
        cost[si] = 0.0;
        open.push(Node { f: hfn(si), i: si });
        let mut expanded = 0;
        while let Some(Node { i, .. }) = open.pop() {
            if i == gi {
                break;
            }
            expanded += 1;
            if expanded > 40_000 {
                return None;
            }
            let (x, z) = (i as i32 % self.w, i as i32 / self.w);
            for (dx, dz) in [
                (1, 0),
                (-1, 0),
                (0, 1),
                (0, -1),
                (1, 1),
                (1, -1),
                (-1, 1),
                (-1, -1),
            ] {
                let (nx, nz) = (x + dx, z + dz);
                if !self.walkable(nx, nz) {
                    continue;
                }
                if dx != 0 && dz != 0 && (!self.walkable(x + dx, z) || !self.walkable(x, z + dz)) {
                    continue;
                }
                let ni = (nz * self.w + nx) as usize;
                let c = cost[i] + if dx != 0 && dz != 0 { 1.414 } else { 1.0 };
                if c < cost[ni] {
                    cost[ni] = c;
                    from[ni] = i;
                    open.push(Node {
                        f: c + hfn(ni),
                        i: ni,
                    });
                }
            }
        }
        if from[gi] == usize::MAX && si != gi {
            return None;
        }
        let mut cells = vec![gi];
        let mut cur = gi;
        while cur != si {
            cur = from[cur];
            if cur == usize::MAX {
                return None;
            }
            cells.push(cur);
        }
        cells.reverse();
        let pts: Vec<Vec3> = cells
            .iter()
            .map(|&c| self.center(c as i32 % self.w, c as i32 / self.w))
            .collect();
        // string pulling
        let mut out = Vec::new();
        let mut anchor = a;
        let mut k = 0;
        while k < pts.len() {
            let mut far = k;
            for m in (k..pts.len()).rev() {
                if self.clear_line(anchor, pts[m]) {
                    far = m;
                    break;
                }
            }
            out.push(pts[far]);
            anchor = pts[far];
            k = far + 1;
        }
        if let Some(last) = out.last_mut() {
            let (bi, bj) = self.cell_of(b);
            if self.walkable(bi, bj) {
                *last = vec3(b.x, 0.0, b.z);
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toon_shared::map::MapId;
    use toon_shared::maps;

    #[test]
    fn objectives_reachable_from_spawns() {
        for id in MapId::ALL {
            let m = maps::build(id);
            let w = World::from_map(&m);
            let nav = Nav::build(&m, &w);
            for t in 0..2 {
                let s = m.spawns[t][0].pos;
                for goal in [m.bomb, m.sites[0], m.sites[1], m.spawns[1 - t][0].pos] {
                    let p = nav.path(s, goal);
                    assert!(p.is_some(), "{:?}: no path {:?} -> {:?}", id, s, goal);
                }
            }
        }
    }
}
