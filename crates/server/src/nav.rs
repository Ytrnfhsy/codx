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
/// Grid on big terrain maps (a 0.5 m grid over a square kilometre would not fit in memory).
const TERRAIN_CELL: f32 = 2.0;
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
    /// Stands on the terrain (not on a box).
    ground: bool,
}

/// A regular grid of columns. Small maps have one fine grid; big terrain maps have a
/// coarse one over the land and fine ones where the buildings are.
struct Grid {
    ox: f32,
    oz: f32,
    cell: f32,
    w: i32,
    h: i32,
    /// Node ids per column, sorted by height.
    cols: Vec<Vec<u32>>,
}

impl Grid {
    fn new(ox: f32, oz: f32, x1: f32, z1: f32, cell: f32) -> Grid {
        let w = ((x1 - ox) / cell).ceil().max(1.0) as i32;
        let h = ((z1 - oz) / cell).ceil().max(1.0) as i32;
        Grid {
            ox,
            oz,
            cell,
            w,
            h,
            cols: vec![Vec::new(); (w * h) as usize],
        }
    }
    fn center(&self, i: i32, j: i32) -> (f32, f32) {
        (
            self.ox + (i as f32 + 0.5) * self.cell,
            self.oz + (j as f32 + 0.5) * self.cell,
        )
    }
    fn cell_of(&self, p: Vec3) -> (i32, i32) {
        (
            ((p.x - self.ox) / self.cell).floor() as i32,
            ((p.z - self.oz) / self.cell).floor() as i32,
        )
    }
    fn contains(&self, p: Vec3) -> bool {
        let (i, j) = self.cell_of(p);
        i >= 0 && j >= 0 && i < self.w && j < self.h
    }
    fn col(&self, i: i32, j: i32) -> Option<&Vec<u32>> {
        if i < 0 || j < 0 || i >= self.w || j >= self.h {
            None
        } else {
            Some(&self.cols[(j * self.w + i) as usize])
        }
    }
}

pub struct Nav {
    /// Grid 0 covers the whole map; the others (finer) cut holes into it.
    grids: Vec<Grid>,
    /// Water surface (bots do not wander into rivers and the sea).
    water: f32,
    /// Big terrain map (hills: steeper links, longer straight walks).
    terrain: bool,
    nodes: Vec<NavNode>,
    reachable_ids: Vec<u32>,
    /// A*'s per-node bookkeeping, kept between searches (clearing a quarter of a million
    /// entries for every path would cost more than the search itself).
    scratch: std::sync::Mutex<Scratch>,
}

#[derive(Default)]
struct Scratch {
    cost: Vec<f32>,
    from: Vec<u32>,
    /// Search that last touched each node; older entries count as unset.
    stamp: Vec<u32>,
    search: u32,
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

/// Nothing solid in the way of a body walking straight from `a` to `b`.
fn walkable_between(world: &World, a: Vec3, b: Vec3) -> bool {
    (1..4).all(|k| {
        let p = a.lerp(b, k as f32 / 4.0);
        let y = world
            .ground(p.x, p.z)
            .map(|g| g.max(p.y.min(a.y.max(b.y))))
            .unwrap_or(p.y);
        let bb = Aabb {
            min: vec3(p.x - 0.3, y + 0.55, p.z - 0.3),
            max: vec3(p.x + 0.3, y + 1.7, p.z + 0.3),
        };
        !world.overlaps(&bb)
    })
}

impl Nav {
    pub fn build(map: &Map, world: &World) -> Nav {
        let ground = world.terrain.clone();
        let mut grids = Vec::new();
        if ground.is_some() {
            // a coarse grid over the land, fine ones over the towns (snapped to the coarse
            // cells so that every spot belongs to exactly one grid)
            let coarse = Grid::new(map.min.x, map.min.y, map.max.x, map.max.y, TERRAIN_CELL);
            for r in &map.nav_detail {
                let snap = |v: f32, o: f32, up: bool| {
                    let k = (v - o) / TERRAIN_CELL;
                    o + if up { k.ceil() } else { k.floor() } * TERRAIN_CELL
                };
                let x0 = snap(r[0].max(map.min.x), map.min.x, false);
                let z0 = snap(r[1].max(map.min.y), map.min.y, false);
                let x1 = snap(r[2].min(map.max.x), map.min.x, true);
                let z1 = snap(r[3].min(map.max.y), map.min.y, true);
                grids.push(Grid::new(x0, z0, x1, z1, CELL));
            }
            grids.insert(0, coarse);
        } else {
            grids.push(Grid::new(map.min.x, map.min.y, map.max.x, map.max.y, CELL));
        }
        let fine_rects: Vec<(f32, f32, f32, f32)> = grids[1..]
            .iter()
            .map(|g| {
                (
                    g.ox,
                    g.oz,
                    g.ox + g.w as f32 * g.cell,
                    g.oz + g.h as f32 * g.cell,
                )
            })
            .collect();
        let in_fine = |x: f32, z: f32| {
            fine_rects
                .iter()
                .any(|r| x >= r.0 && x < r.2 && z >= r.1 && z < r.3)
        };

        let mut nodes: Vec<NavNode> = Vec::new();
        for (gi, grid) in grids.iter_mut().enumerate() {
            let (ox, oz, cell, w, h) = (grid.ox, grid.oz, grid.cell, grid.w, grid.h);
            // candidate floor heights: the top of every solid box under a column's centre
            let mut tops: Vec<Vec<f32>> = vec![Vec::new(); (w * h) as usize];
            for (b, m) in world.boxes.iter().zip(&world.mats) {
                if b.max.y > 30.0 && ground.is_none()
                    || b.max.y - b.min.y > 60.0
                    || !m.blocks_movement()
                {
                    continue; // invisible clip walls, and cloth nobody can stand on
                }
                let i0 = (((b.min.x - ox) / cell) - 0.5).ceil().max(0.0) as i32;
                let i1 = (((b.max.x - ox) / cell) - 0.5).floor().min((w - 1) as f32) as i32;
                let j0 = (((b.min.z - oz) / cell) - 0.5).ceil().max(0.0) as i32;
                let j1 = (((b.max.z - oz) / cell) - 0.5).floor().min((h - 1) as f32) as i32;
                for j in j0..=j1 {
                    for i in i0..=i1 {
                        tops[(j * w + i) as usize].push(b.max.y);
                    }
                }
            }
            // the ground itself, where it is not too steep to walk; tops buried in it go
            let mut on_ground = vec![f32::NAN; (w * h) as usize];
            if let Some(t) = &ground {
                for j in 0..h {
                    for i in 0..w {
                        let (x, z) = grid.center(i, j);
                        let y = t.height(x, z);
                        let k = (j * w + i) as usize;
                        tops[k].retain(|&top| top > y + 0.1);
                        if t.slope(x, z) <= toon_shared::terrain::MAX_WALK_SLOPE * 1.05 {
                            tops[k].push(y);
                            on_ground[k] = y;
                        }
                    }
                }
            }
            for j in 0..h {
                for i in 0..w {
                    let k = (j * w + i) as usize;
                    let (x, z) = grid.center(i, j);
                    // the coarse grid leaves the towns to the fine grids
                    if gi == 0 && in_fine(x, z) {
                        continue;
                    }
                    let t = &mut tops[k];
                    t.sort_by(|a, b| a.partial_cmp(b).unwrap());
                    t.dedup_by(|a, b| (*a - *b).abs() < 0.03);
                    for &y in t.iter() {
                        // obstacles lower than a step (stairs, kerbs) are stepped over, so
                        // they do not block
                        let bb = Aabb {
                            min: vec3(x - CLEAR_R, y + 0.5, z - CLEAR_R),
                            max: vec3(x + CLEAR_R, y + 1.85, z + CLEAR_R),
                        };
                        if !world.overlaps(&bb) {
                            grid.cols[k].push(nodes.len() as u32);
                            nodes.push(NavNode {
                                pos: vec3(x, y, z),
                                links: Vec::new(),
                                reachable: false,
                                ground: on_ground[k] == y,
                            });
                        }
                    }
                }
            }
        }

        let mut nav = Nav {
            grids,
            water: ground
                .as_ref()
                .map(|t| t.water)
                .unwrap_or(f32::NEG_INFINITY),
            terrain: ground.is_some(),
            scratch: Default::default(),
            nodes,
            reachable_ids: Vec::new(),
        };
        let link_cost = |nav: &Nav, a: u32, b: u32, flat: f32| {
            let (pa, pb) = (nav.nodes[a as usize].pos, nav.nodes[b as usize].pos);
            let dy = pb.y - pa.y;
            let mut cost = flat
                + if dy > 0.0 {
                    dy * 0.6
                } else if dy < -MAX_UP && !nav.nodes[b as usize].ground {
                    1.0 - dy * 0.3
                } else {
                    0.0
                };
            // wading is slow
            if pb.y < nav.water - 0.35 {
                cost *= 2.5;
            }
            cost
        };
        // links inside each grid
        for gi in 0..nav.grids.len() {
            let (w, h, cell) = (nav.grids[gi].w, nav.grids[gi].h, nav.grids[gi].cell);
            for j in 0..h {
                for i in 0..w {
                    let ids = nav.grids[gi].cols[(j * w + i) as usize].clone();
                    for n in ids {
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
                            let Some(m) = nav.step_target(n, gi, i + di, j + dj, !diag) else {
                                continue;
                            };
                            if diag
                                && (nav.step_target(n, gi, i + di, j, false).is_none()
                                    || nav.step_target(n, gi, i, j + dj, false).is_none())
                            {
                                continue;
                            }
                            let (pa, pb) = (nav.nodes[n as usize].pos, nav.nodes[m as usize].pos);
                            // on the coarse grid a wall can stand between two free cells
                            if cell > CELL && !walkable_between(world, pa, pb) {
                                continue;
                            }
                            let flat = if diag { cell * 1.414 } else { cell };
                            links.push((m, link_cost(&nav, n, m, flat)));
                        }
                        nav.nodes[n as usize].links = links;
                    }
                }
            }
        }
        // seams: the edge of every fine grid joins the coarse ground round it
        for gi in 1..nav.grids.len() {
            let (w, h) = (nav.grids[gi].w, nav.grids[gi].h);
            let mut edge = Vec::new();
            for j in 0..h {
                for i in 0..w {
                    let out: Vec<(i32, i32)> = [(-1, 0), (1, 0), (0, -1), (0, 1)]
                        .into_iter()
                        .filter(|&(di, dj)| {
                            let (ni, nj) = (i + di, j + dj);
                            ni < 0 || nj < 0 || ni >= w || nj >= h
                        })
                        .collect();
                    if !out.is_empty() {
                        for &n in &nav.grids[gi].cols[(j * w + i) as usize] {
                            edge.push((n, out.clone()));
                        }
                    }
                }
            }
            for (n, dirs) in edge {
                let pa = nav.nodes[n as usize].pos;
                for (di, dj) in dirs {
                    let q = pa + vec3(di as f32, 0.0, dj as f32) * (CELL * 0.5 + 0.05);
                    let Some((g2, ci, cj)) = nav.locate(q) else {
                        continue;
                    };
                    if g2 == gi {
                        continue;
                    }
                    let Some(col) = nav.grids[g2].col(ci, cj) else {
                        continue;
                    };
                    // the closest floor in height that a body can walk to
                    let best = col
                        .iter()
                        .copied()
                        .filter(|&m| {
                            let pb = nav.nodes[m as usize].pos;
                            let dist = vec3(pb.x - pa.x, 0.0, pb.z - pa.z).length();
                            let up = if nav.nodes[n as usize].ground && nav.nodes[m as usize].ground
                            {
                                (dist * toon_shared::terrain::MAX_WALK_SLOPE).max(MAX_UP)
                            } else {
                                MAX_UP
                            };
                            (pb.y - pa.y).abs() <= up && walkable_between(world, pa, pb)
                        })
                        .min_by(|&a, &b| {
                            let da = (nav.nodes[a as usize].pos.y - pa.y).abs();
                            let db = (nav.nodes[b as usize].pos.y - pa.y).abs();
                            da.total_cmp(&db)
                        });
                    if let Some(m) = best {
                        let pb = nav.nodes[m as usize].pos;
                        let flat = vec3(pb.x - pa.x, 0.0, pb.z - pa.z).length();
                        let (c1, c2) = (link_cost(&nav, n, m, flat), link_cost(&nav, m, n, flat));
                        if !nav.nodes[n as usize].links.iter().any(|l| l.0 == m) {
                            nav.nodes[n as usize].links.push((m, c1));
                        }
                        if !nav.nodes[m as usize].links.iter().any(|l| l.0 == n) {
                            nav.nodes[m as usize].links.push((n, c2));
                        }
                    }
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

    /// The grid a point belongs to (a fine one where there is one) and its column there.
    fn locate(&self, p: Vec3) -> Option<(usize, i32, i32)> {
        let g = (1..self.grids.len())
            .find(|&g| self.grids[g].contains(p))
            .or_else(|| self.grids[0].contains(p).then_some(0))?;
        let (i, j) = self.grids[g].cell_of(p);
        Some((g, i, j))
    }

    /// Node in column (i, j) of grid `g` reachable from node `from` (by stepping, or
    /// dropping if allowed).
    fn step_target(&self, from: u32, g: usize, i: i32, j: i32, allow_drop: bool) -> Option<u32> {
        let col = self.grids[g].col(i, j)?;
        let a = &self.nodes[from as usize];
        let y = a.pos.y;
        let mut best: Option<(u32, f32)> = None;
        for &m in col {
            let b = &self.nodes[m as usize];
            let dy = b.pos.y - y;
            // walking over the hills: as steep as the ground may be
            let up = if a.ground && b.ground {
                (self.grids[g].cell * toon_shared::terrain::MAX_WALK_SLOPE).max(MAX_UP)
            } else {
                MAX_UP
            };
            let ok = dy.abs() <= up || (allow_drop && dy < 0.0 && dy >= -MAX_DROP);
            if ok && best.map(|b| dy.abs() < b.1).unwrap_or(true) {
                best = Some((m, dy.abs()));
            }
        }
        best.map(|b| b.0)
    }

    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Closest walkable node to `p` in grid `g` (same floor preferred).
    fn nearest_in(&self, g: usize, p: Vec3) -> Option<(u32, f32)> {
        let grid = &self.grids[g];
        let (ci, cj) = grid.cell_of(p);
        let mut best: Option<(u32, f32)> = None;
        for r in 0..10i32 {
            // farther rings cannot beat what we have (a roof right overhead must not win
            // over the floor a few steps away)
            if best.is_some_and(|b| b.1 < (r as f32 - 1.0) * grid.cell) {
                break;
            }
            for dj in -r..=r {
                for di in -r..=r {
                    if di.abs() != r && dj.abs() != r {
                        continue;
                    }
                    let Some(col) = grid.col(ci + di, cj + dj) else {
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
        }
        best
    }

    /// Closest walkable node to `p` (same floor preferred), from any grid nearby.
    fn nearest(&self, p: Vec3) -> Option<u32> {
        let home = self.locate(p).map(|l| l.0).unwrap_or(0);
        if let Some((n, d)) = self.nearest_in(home, p) {
            if d < 1.5 || self.grids.len() == 1 {
                return Some(n);
            }
        }
        // near the edge of a fine grid the closest floor may lie in the next grid
        (0..self.grids.len())
            .filter(|&g| {
                let gr = &self.grids[g];
                let m = 10.0 * gr.cell;
                p.x >= gr.ox - m
                    && p.z >= gr.oz - m
                    && p.x <= gr.ox + gr.w as f32 * gr.cell + m
                    && p.z <= gr.oz + gr.h as f32 * gr.cell + m
            })
            .filter_map(|g| self.nearest_in(g, p))
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|b| b.0)
    }

    /// Snaps a point onto the navigation mesh.
    pub fn snap(&self, p: Vec3) -> Option<Vec3> {
        self.nearest(p).map(|n| self.nodes[n as usize].pos)
    }

    /// Random reachable point (bots roam over every floor they can actually get to, but
    /// not into the water).
    pub fn random_point(&self, rng: &mut Rng) -> Vec3 {
        if self.reachable_ids.is_empty() {
            return Vec3::ZERO;
        }
        let mut p = Vec3::ZERO;
        for _ in 0..8 {
            p = self.nodes[self.reachable_ids[rng.below(self.reachable_ids.len())] as usize].pos;
            if p.y > self.water {
                break;
            }
        }
        p
    }

    /// Up to `n` random reachable floor points within `r` metres of `p` on about the same
    /// level (for picking cover or a spot to hold nearby).
    pub fn sample_near(&self, p: Vec3, r: f32, n: usize, rng: &mut Rng) -> Vec<Vec3> {
        let mut out = Vec::new();
        for _ in 0..n * 4 {
            if out.len() >= n {
                break;
            }
            let a = rng.range(0.0, std::f32::consts::TAU);
            let d = r * rng.f32().sqrt();
            let q = p + vec3(a.cos() * d, 0.0, a.sin() * d);
            let Some((g, i, j)) = self.locate(q) else {
                continue;
            };
            let Some(col) = self.grids[g].col(i, j) else {
                continue;
            };
            if let Some(&m) = col.iter().find(|&&m| {
                let nd = &self.nodes[m as usize];
                nd.reachable && (nd.pos.y - p.y).abs() < 2.5 && nd.pos.y > self.water
            }) {
                out.push(self.nodes[m as usize].pos);
            }
        }
        out
    }

    pub fn reachable_count(&self) -> usize {
        self.reachable_ids.len()
    }

    /// Straight walk from `a` to `b` stays on floors close to the interpolated height.
    fn clear_line(&self, a: Vec3, b: Vec3) -> bool {
        let d = b - a;
        let len = vec3(d.x, 0.0, d.z).length();
        // does the line touch a town (fine grid)? then walk it in fine steps
        let (lo, hi) = (a.min(b), a.max(b));
        let fine = self.grids[1..].iter().any(|g| {
            lo.x <= g.ox + g.w as f32 * g.cell
                && hi.x >= g.ox
                && lo.z <= g.oz + g.h as f32 * g.cell
                && hi.z >= g.oz
        }) || self.grids[0].cell <= CELL;
        // nothing walkable is steeper than stairs (stacked flights share the same footprint)
        let steep = if self.terrain && !fine { 1.05 } else { 0.75 };
        if d.y.abs() > len * steep + 0.3 {
            return false;
        }
        let step = if fine {
            CELL * 0.5
        } else {
            self.grids[0].cell * 0.5
        };
        let n = (len / step).ceil().max(1.0) as i32;
        for k in 0..=n {
            let t = k as f32 / n as f32;
            let p = a + d * t;
            let Some((g, i, j)) = self.locate(p) else {
                return false;
            };
            let tol = (self.grids[g].cell * 0.5).max(0.6);
            let Some(col) = self.grids[g].col(i, j) else {
                return false;
            };
            if !col
                .iter()
                .any(|&m| (self.nodes[m as usize].pos.y - p.y).abs() < tol)
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
        let mut guard = self.scratch.lock().unwrap_or_else(|e| e.into_inner());
        let sc = &mut *guard;
        if sc.stamp.len() != n {
            sc.cost = vec![f32::INFINITY; n];
            sc.from = vec![u32::MAX; n];
            sc.stamp = vec![0; n];
            sc.search = 0;
        }
        sc.search = sc.search.wrapping_add(1);
        if sc.search == 0 {
            sc.stamp.fill(0);
            sc.search = 1;
        }
        let search = sc.search;
        let Scratch {
            cost, from, stamp, ..
        } = sc;
        let mut touch = |i: usize, cost: &mut Vec<f32>, from: &mut Vec<u32>| {
            if stamp[i] != search {
                stamp[i] = search;
                cost[i] = f32::INFINITY;
                from[i] = u32::MAX;
            }
        };
        // big maps: a greedier search (paths a little longer, found much faster)
        let greed = if self.terrain { 1.4 } else { 1.0 };
        let mut open = BinaryHeap::new();
        touch(s as usize, cost, from);
        touch(g as usize, cost, from);
        cost[s as usize] = 0.0;
        open.push(Open {
            f: self.nodes[s as usize].pos.distance(goal) * greed,
            i: s,
        });
        let mut expanded = 0;
        while let Some(Open { i, f }) = open.pop() {
            if i == g {
                break;
            }
            // stale heap entry (a cheaper way here was found later)
            if f - self.nodes[i as usize].pos.distance(goal) * greed > cost[i as usize] + 1e-3 {
                continue;
            }
            expanded += 1;
            if expanded > if self.terrain { 25_000 } else { 120_000 } {
                return None;
            }
            for &(m, c) in &self.nodes[i as usize].links {
                touch(m as usize, cost, from);
                let nc = cost[i as usize] + c;
                if nc < cost[m as usize] {
                    cost[m as usize] = nc;
                    from[m as usize] = i;
                    open.push(Open {
                        f: nc + self.nodes[m as usize].pos.distance(goal) * greed,
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
        drop(guard);
        let pts: Vec<Vec3> = chain.iter().map(|&c| self.nodes[c as usize].pos).collect();
        // string pulling
        let mut out = Vec::new();
        let mut anchor = pts[0];
        let mut k = 1;
        while k < pts.len() {
            let mut far = k;
            // look a limited way ahead (long paths over a big map)
            for m in (k..pts.len().min(k + 40)).rev() {
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

    #[test]
    fn island_paths_cross_the_hills_and_the_river() {
        let m = maps::build(MapId::Island);
        let w = World::from_map(&m);
        let t = std::time::Instant::now();
        let nav = Nav::build(&m, &w);
        eprintln!(
            "island nav: {} nodes, {} reachable, built in {:?}",
            nav.node_count(),
            nav.reachable_count(),
            t.elapsed()
        );
        assert!(nav.reachable_count() > 100_000);
        let a = m.spawns[0][0].pos;
        let b = m.spawns[1][0].pos;
        let t = std::time::Instant::now();
        let p = nav.path(a, b).expect("no path across the island");
        eprintln!("path of {} points in {:?}", p.len(), t.elapsed());
        let end = *p.last().unwrap();
        assert!(vec3(end.x - b.x, 0.0, end.z - b.z).length() < 3.0);
        // across the river to the port town in the south-east
        let port = vec3(150.0, 0.0, 330.0);
        assert!(nav.path(a, port).is_some(), "no path to the port");
    }
}
