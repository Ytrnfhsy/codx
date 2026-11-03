//! Multi-level navigation for bots: a 0.5 m grid where each column can hold several walkable
//! floors (ground, upper storeys, rooftops). Neighbouring floors are linked when the height
//! difference can be stepped (stairs) or dropped down (jumping off a roof), so A* paths lead
//! up stairwells, across balconies and onto roofs; ladders link their foot with the roof.

use glam::{vec3, Vec3};
use std::cmp::Ordering;
use std::collections::BinaryHeap;
use toon_shared::map::Map;
use toon_shared::physics::{Aabb, World};
use toon_shared::rng::Rng;

const CELL: f32 = 0.5;
const CLEAR_R: f32 = 0.4;
/// Highest step a bot climbs between neighbouring cells (stairs are 0.34).
const MAX_UP: f32 = 0.55;
/// Highest drop a bot takes on purpose.
const MAX_DROP: f32 = 3.2;

struct NavNode {
    pos: Vec3,
    links: Vec<(u32, f32)>,
    /// Reachable from the spawn areas (rooftops without stairs are not).
    reachable: bool,
}

pub struct Nav {
    ox: f32,
    oz: f32,
    pub w: i32,
    pub h: i32,
    /// Node ids per column, sorted by height.
    cols: Vec<Vec<u32>>,
    nodes: Vec<NavNode>,
    reachable_ids: Vec<u32>,
}

#[derive(PartialEq)]
struct Open {
    f: f32,
    i: u32,
}
impl Eq for Open {}
impl PartialOrd for Open {
    fn partial_cmp(&self, o: &Self) -> Option<Ordering> {
        Some(self.cmp(o))
    }
}
impl Ord for Open {
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
        let center = |i: i32, j: i32| (ox + (i as f32 + 0.5) * CELL, oz + (j as f32 + 0.5) * CELL);

        // candidate floor heights: the top of every solid box under a column's centre
        let mut tops: Vec<Vec<f32>> = vec![Vec::new(); (w * h) as usize];
        for (b, m) in world.boxes.iter().zip(&world.mats) {
            if b.max.y > 30.0 || !m.blocks_movement() {
                continue; // invisible clip ceilings, and cloth nobody can stand on
            }
            let i0 = (((b.min.x - ox) / CELL) - 0.5).ceil().max(0.0) as i32;
            let i1 = (((b.max.x - ox) / CELL) - 0.5).floor().min((w - 1) as f32) as i32;
            let j0 = (((b.min.z - oz) / CELL) - 0.5).ceil().max(0.0) as i32;
            let j1 = (((b.max.z - oz) / CELL) - 0.5).floor().min((h - 1) as f32) as i32;
            for j in j0..=j1 {
                for i in i0..=i1 {
                    tops[(j * w + i) as usize].push(b.max.y);
                }
            }
        }

        let mut cols: Vec<Vec<u32>> = vec![Vec::new(); (w * h) as usize];
        let mut nodes: Vec<NavNode> = Vec::new();
        for j in 0..h {
            for i in 0..w {
                let k = (j * w + i) as usize;
                let t = &mut tops[k];
                t.sort_by(|a, b| a.partial_cmp(b).unwrap());
                t.dedup_by(|a, b| (*a - *b).abs() < 0.03);
                let (x, z) = center(i, j);
                for &y in t.iter() {
                    // obstacles lower than a step (stairs, kerbs) are stepped over, so they do not block
                    let bb = Aabb {
                        min: vec3(x - CLEAR_R, y + 0.5, z - CLEAR_R),
                        max: vec3(x + CLEAR_R, y + 1.85, z + CLEAR_R),
                    };
                    if !world.overlaps(&bb) {
                        cols[k].push(nodes.len() as u32);
                        nodes.push(NavNode {
                            pos: vec3(x, y, z),
                            links: Vec::new(),
                            reachable: false,
                        });
                    }
                }
            }
        }

        let mut nav = Nav {
            ox,
            oz,
            w,
            h,
            cols,
            nodes,
            reachable_ids: Vec::new(),
        };
        // links
        for j in 0..h {
            for i in 0..w {
                let ids = nav.cols[(j * w + i) as usize].clone();
                for n in ids {
                    let y = nav.nodes[n as usize].pos.y;
                    let mut links = Vec::new();
                    for (di, dj) in [
                        (1, 0),
                        (-1, 0),
                        (0, 1),
                        (0, -1),
                        (1, 1),
                        (1, -1),
                        (-1, 1),
                        (-1, -1),
                    ] {
                        let diag = di != 0 && dj != 0;
                        let Some(m) = nav.step_target(i + di, j + dj, y, !diag) else {
                            continue;
                        };
                        if diag
                            && (nav.step_target(i + di, j, y, false).is_none()
                                || nav.step_target(i, j + dj, y, false).is_none())
                        {
                            continue;
                        }
                        let dy = nav.nodes[m as usize].pos.y - y;
                        let flat = if diag { CELL * 1.414 } else { CELL };
                        let cost = flat
                            + if dy > 0.0 {
                                dy * 0.6
                            } else if dy < -MAX_UP {
                                1.0 - dy * 0.3
                            } else {
                                0.0
                            };
                        links.push((m, cost));
                    }
                    nav.nodes[n as usize].links = links;
                }
            }
        }
        // ladders link their foot with the roof they lead to
        for l in &world.ladders {
            let (Some(a), Some(b)) = (nav.nearest(l.base), nav.nearest(l.exit())) else {
                continue;
            };
            let (pa, pb) = (nav.nodes[a as usize].pos, nav.nodes[b as usize].pos);
            // only if both ends really are the ground at its foot and the roof at its top
            if (pa.y - l.base.y).abs() > 0.6 || (pb.y - l.top).abs() > 0.6 || a == b {
                continue;
            }
            let cost = 2.0 + (l.top - l.base.y).abs() * 1.5;
            nav.nodes[a as usize].links.push((b, cost));
            nav.nodes[b as usize].links.push((a, cost));
        }
        // flood fill from every spawn point
        let mut stack: Vec<u32> = map
            .all_spawns()
            .filter_map(|sp| nav.nearest(sp.pos))
            .collect();
        while let Some(n) = stack.pop() {
            if nav.nodes[n as usize].reachable {
                continue;
            }
            nav.nodes[n as usize].reachable = true;
            for k in 0..nav.nodes[n as usize].links.len() {
                let m = nav.nodes[n as usize].links[k].0;
                if !nav.nodes[m as usize].reachable {
                    stack.push(m);
                }
            }
        }
        nav.reachable_ids = (0..nav.nodes.len() as u32)
            .filter(|&i| nav.nodes[i as usize].reachable)
            .collect();
        nav
    }

    /// Node in column (i, j) reachable from height `y` (by stepping, or dropping if allowed).
    fn step_target(&self, i: i32, j: i32, y: f32, allow_drop: bool) -> Option<u32> {
        let col = self.col(i, j)?;
        let mut best: Option<(u32, f32)> = None;
        for &m in col {
            let dy = self.nodes[m as usize].pos.y - y;
            let ok = dy.abs() <= MAX_UP || (allow_drop && dy < 0.0 && dy >= -MAX_DROP);
            if ok && best.map(|b| dy.abs() < b.1).unwrap_or(true) {
                best = Some((m, dy.abs()));
            }
        }
        best.map(|b| b.0)
    }

    fn col(&self, i: i32, j: i32) -> Option<&Vec<u32>> {
        if i < 0 || j < 0 || i >= self.w || j >= self.h {
            None
        } else {
            Some(&self.cols[(j * self.w + i) as usize])
        }
    }

    pub fn cell_of(&self, p: Vec3) -> (i32, i32) {
        (
            ((p.x - self.ox) / CELL).floor() as i32,
            ((p.z - self.oz) / CELL).floor() as i32,
        )
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Closest walkable node to `p` (same floor preferred).
    fn nearest(&self, p: Vec3) -> Option<u32> {
        let (ci, cj) = self.cell_of(p);
        for r in 0..10i32 {
            let mut best: Option<(u32, f32)> = None;
            for dj in -r..=r {
                for di in -r..=r {
                    if di.abs() != r && dj.abs() != r {
                        continue;
                    }
                    let Some(col) = self.col(ci + di, cj + dj) else {
                        continue;
                    };
                    for &m in col {
                        // once the reachable set is known, never snap onto isolated rooftops
                        if !self.reachable_ids.is_empty() && !self.nodes[m as usize].reachable {
                            continue;
                        }
                        let q = self.nodes[m as usize].pos;
                        // floors far above us are much "further" than ones below
                        let dy = q.y - p.y;
                        let vertical = if dy > 0.6 { dy * 3.0 } else { dy.abs() };
                        let d = vec3(q.x - p.x, 0.0, q.z - p.z).length() + vertical;
                        if best.map(|b| d < b.1).unwrap_or(true) {
                            best = Some((m, d));
                        }
                    }
                }
            }
            if best.is_some() {
                return best.map(|b| b.0);
            }
        }
        None
    }

    /// Snaps a point onto the navigation mesh.
    pub fn snap(&self, p: Vec3) -> Option<Vec3> {
        self.nearest(p).map(|n| self.nodes[n as usize].pos)
    }

    /// Random reachable point (bots roam over every floor they can actually get to).
    pub fn random_point(&self, rng: &mut Rng) -> Vec3 {
        if self.reachable_ids.is_empty() {
            return Vec3::ZERO;
        }
        self.nodes[self.reachable_ids[rng.below(self.reachable_ids.len())] as usize].pos
    }

    pub fn reachable_count(&self) -> usize {
        self.reachable_ids.len()
    }

    /// Straight walk from `a` to `b` stays on floors close to the interpolated height.
    fn clear_line(&self, a: Vec3, b: Vec3) -> bool {
        let d = b - a;
        let len = vec3(d.x, 0.0, d.z).length();
        // nothing walkable is steeper than stairs (stacked flights share the same footprint)
        if d.y.abs() > len * 0.75 + 0.3 {
            return false;
        }
        let n = (len / 0.25).ceil().max(1.0) as i32;
        for k in 0..=n {
            let t = k as f32 / n as f32;
            let p = a + d * t;
            let (i, j) = self.cell_of(p);
            let Some(col) = self.col(i, j) else {
                return false;
            };
            if !col
                .iter()
                .any(|&m| (self.nodes[m as usize].pos.y - p.y).abs() < 0.6)
            {
                return false;
            }
        }
        true
    }

    /// A* path from `a` to `b`. Returns waypoints (excluding the start), with heights.
    pub fn path(&self, a: Vec3, b: Vec3) -> Option<Vec<Vec3>> {
        let s = self.nearest(a)?;
        let g = self.nearest(b)?;
        let goal = self.nodes[g as usize].pos;
        let n = self.nodes.len();
        let mut cost = vec![f32::INFINITY; n];
        let mut from = vec![u32::MAX; n];
        let mut open = BinaryHeap::new();
        cost[s as usize] = 0.0;
        open.push(Open {
            f: self.nodes[s as usize].pos.distance(goal),
            i: s,
        });
        let mut expanded = 0;
        while let Some(Open { i, .. }) = open.pop() {
            if i == g {
                break;
            }
            expanded += 1;
            if expanded > 120_000 {
                return None;
            }
            for &(m, c) in &self.nodes[i as usize].links {
                let nc = cost[i as usize] + c;
                if nc < cost[m as usize] {
                    cost[m as usize] = nc;
                    from[m as usize] = i;
                    open.push(Open {
                        f: nc + self.nodes[m as usize].pos.distance(goal),
                        i: m,
                    });
                }
            }
        }
        if from[g as usize] == u32::MAX && s != g {
            return None;
        }
        let mut chain = vec![g];
        let mut cur = g;
        while cur != s {
            cur = from[cur as usize];
            if cur == u32::MAX {
                return None;
            }
            chain.push(cur);
        }
        chain.reverse();
        let pts: Vec<Vec3> = chain.iter().map(|&c| self.nodes[c as usize].pos).collect();
        // string pulling
        let mut out = Vec::new();
        let mut anchor = pts[0];
        let mut k = 1;
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
        if out.is_empty() {
            out.push(pts[0]);
        }
        if let Some(last) = out.last_mut() {
            if vec3(b.x - last.x, 0.0, b.z - last.z).length() < 1.5 && (b.y - last.y).abs() < 0.6 {
                *last = vec3(b.x, last.y, b.z);
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

    #[test]
    fn upper_floors_are_reachable() {
        for id in MapId::ALL {
            let m = maps::build(id);
            let w = World::from_map(&m);
            if id == MapId::Cargo {
                continue; // deliberately flat close-quarters map
            }
            let nav = Nav::build(&m, &w);
            let s = m.spawns[0][0].pos;
            // every map must offer some high ground bots can walk up to
            let high = nav
                .nodes
                .iter()
                .filter(|n| n.pos.y > 2.5)
                .map(|n| n.pos)
                .collect::<Vec<_>>();
            if high.is_empty() {
                continue;
            }
            let reachable = high
                .iter()
                .step_by((high.len() / 40).max(1))
                .filter(|p| {
                    nav.path(s, **p)
                        .map(|path| {
                            path.last()
                                .map(|l| (l.y - p.y).abs() < 0.6)
                                .unwrap_or(false)
                        })
                        .unwrap_or(false)
                })
                .count();
            assert!(reachable > 0, "{:?}: no upper floor reachable", id);
        }
    }

    #[test]
    fn key_high_ground_is_reachable() {
        let spots: &[(MapId, [f32; 3], &str)] = &[
            (
                MapId::Wreck,
                [-46.0, 12.6, -30.0],
                "roof of the fort on the hill",
            ),
            (MapId::Wreck, [-46.0, 9.2, -33.0], "third floor of the fort"),
            (
                MapId::Wreck,
                [-18.25, 3.4, -16.75],
                "north-west balcony over the square",
            ),
            (
                MapId::Wreck,
                [44.0, 9.2, 28.0],
                "roof of the water-tower house",
            ),
            (MapId::Wreck, [43.25, 4.6, -22.0], "terrace house balcony"),
            (
                MapId::Wreck,
                [-46.25, 4.6, 11.75],
                "south-west terrace balcony",
            ),
            (
                MapId::Site,
                [0.0, 3.4, -3.5],
                "first floor of the unfinished building",
            ),
            (MapId::Site, [0.0, 3.4, 7.7], "scaffolding"),
            (MapId::Site, [-4.0, 6.8, -3.5], "half roof"),
            (MapId::Site, [-29.0, 6.8, -16.0], "office roof"),
            (MapId::Site, [-22.3, 3.4, -16.0], "office balcony"),
            (MapId::Market, [27.5, 6.8, -11.25], "east high house roof"),
            (MapId::Market, [-27.5, 6.8, -11.25], "west high house roof"),
            (MapId::Market, [43.0, 3.4, 5.8], "balcony over the street"),
            (
                MapId::Market,
                [27.0, 4.55, 11.25],
                "east shop roof (by ladder)",
            ),
            (
                MapId::Market,
                [-41.0, 4.55, -11.25],
                "west shop roof (by ladder)",
            ),
            (
                MapId::Market,
                [-29.0, 4.35, 11.25],
                "market hall roof (by ladder)",
            ),
            (
                MapId::Market,
                [28.5, 6.35, 27.5],
                "warehouse roof (by ladder)",
            ),
            (MapId::Depot, [25.0, 3.4, 0.0], "office upper floor"),
        ];
        for &(id, p, what) in spots {
            let m = maps::build(id);
            let w = World::from_map(&m);
            let nav = Nav::build(&m, &w);
            let target = Vec3::from_array(p);
            for team in 0..2 {
                let s = m.spawns[team][0].pos;
                let path = nav
                    .path(s, target)
                    .unwrap_or_else(|| panic!("{id:?}: no path to {what}"));
                let end = *path.last().unwrap();
                assert!(
                    vec3(end.x - p[0], 0.0, end.z - p[2]).length() < 1.0
                        && (end.y - p[1]).abs() < 0.3,
                    "{id:?}: {what} not reached, path ends at {end:?}"
                );
            }
        }
    }
}
