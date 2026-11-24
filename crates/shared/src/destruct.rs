//! Things break. A blast cuts a chunk out of the walls, floors, fences and trees around
//! it; whatever is left with nothing holding it up falls and lies as a heap of blocks;
//! trees and posts cut through (or blown at) topple over; the ground gets a crater.
//!
//! The server and every client run this same code on the same blasts in the same order,
//! so all of them end up with the same map without any boxes being sent: only the
//! blasts are (`S2C::Blast`, and the whole list to someone who joins later).

use crate::map::{Map, MapBox, Mat};
use crate::physics::World;
use crate::rng::Rng;
use glam::{vec2, vec3, Vec3};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// One blast that breaks things: where, and how hard (half the size of the chunk it cuts
/// out of brick, metres).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Blast {
    pub pos: [f32; 3],
    pub power: f32,
}

impl Blast {
    /// The blast of an explosion of `radius` metres (`big`: bombs and shells).
    pub fn of_explosion(pos: Vec3, radius: f32, big: bool) -> Blast {
        let k = if big { 0.24 } else { 0.17 };
        Blast {
            pos: pos.to_array(),
            power: (radius * k).clamp(0.4, 2.4),
        }
    }
}

/// A tree (or post) falling over: it turns about `pivot` until it lies along `dir`.
#[derive(Clone, Debug)]
pub struct Topple {
    pub pivot: Vec3,
    pub dir: Vec3,
    /// The boxes it lies as now (indices into the map's boxes).
    pub boxes: Vec<usize>,
}

/// What one blast changed (for the renderer and the effects).
#[derive(Clone, Debug, Default)]
pub struct Change {
    /// The chunks blown out (flying debris of their colour).
    pub blown: Vec<MapBox>,
    /// Pieces that fell: their index, and how far above their resting place they were.
    pub falls: Vec<(usize, f32)>,
    pub topples: Vec<Topple>,
    /// Everything touched (min, max), to rebuild.
    pub min: Vec3,
    pub max: Vec3,
    /// The ground was dented (terrain maps): centre and radius.
    pub crater: Option<(Vec3, f32)>,
    /// Map boxes taken away.
    pub removed: Vec<usize>,
    /// The middles of the boxes taken away and added (where the level must be built
    /// again: a box is drawn with the part of the map its middle lies in).
    pub touched: Vec<Vec3>,
}

impl Change {
    fn grow(&mut self, min: Vec3, max: Vec3) {
        self.min = self.min.min(min);
        self.max = self.max.max(max);
    }
    pub fn is_empty(&self) -> bool {
        self.removed.is_empty() && self.crater.is_none()
    }
}

/// Grid over every map box (solid or not), for finding the boxes near a blast.
struct Grid {
    origin: (f32, f32),
    dims: (i32, i32),
    cells: Vec<Vec<u32>>,
}

const CELL: f32 = 8.0;

impl Grid {
    fn new(map: &Map) -> Grid {
        let pad = 8.0;
        let origin = (map.min.x - pad, map.min.y - pad);
        let dims = (
            ((map.max.x - map.min.x + 2.0 * pad) / CELL).ceil() as i32,
            ((map.max.y - map.min.y + 2.0 * pad) / CELL).ceil() as i32,
        );
        let mut g = Grid {
            origin,
            dims,
            cells: vec![Vec::new(); (dims.0 * dims.1) as usize],
        };
        for (i, b) in map.boxes.iter().enumerate() {
            g.add(i, b);
        }
        g
    }
    fn range(&self, min: Vec3, max: Vec3) -> (i32, i32, i32, i32) {
        let c = |v: f32, o: f32, n: i32| (((v - o) / CELL).floor() as i32).clamp(0, n - 1);
        (
            c(min.x, self.origin.0, self.dims.0),
            c(min.z, self.origin.1, self.dims.1),
            c(max.x, self.origin.0, self.dims.0),
            c(max.z, self.origin.1, self.dims.1),
        )
    }
    fn add(&mut self, i: usize, b: &MapBox) {
        let (x0, z0, x1, z1) = self.range(b.min, b.max);
        for z in z0..=z1 {
            for x in x0..=x1 {
                self.cells[(z * self.dims.0 + x) as usize].push(i as u32);
            }
        }
    }
    fn remove(&mut self, i: usize, b: &MapBox) {
        let (x0, z0, x1, z1) = self.range(b.min, b.max);
        for z in z0..=z1 {
            for x in x0..=x1 {
                self.cells[(z * self.dims.0 + x) as usize].retain(|&k| k as usize != i);
            }
        }
    }
    /// The boxes whose cells meet `min..max` (each once, in order).
    fn near(&self, min: Vec3, max: Vec3) -> Vec<usize> {
        let (x0, z0, x1, z1) = self.range(min, max);
        let mut out = Vec::new();
        for z in z0..=z1 {
            for x in x0..=x1 {
                out.extend(
                    self.cells[(z * self.dims.0 + x) as usize]
                        .iter()
                        .map(|&k| k as usize),
                );
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }
}

/// The map's breakage so far, and what it was like before (to put it back).
pub struct Wreck {
    orig: Vec<MapBox>,
    orig_terrain: Option<Arc<crate::terrain::Heightmap>>,
    grid: Grid,
    /// Every blast so far, in order.
    pub log: Vec<Blast>,
}

/// Leftovers thinner than this are dropped.
const SLIVER: f32 = 0.06;
/// Most pieces one blast may bring down (a big building comes down in bigger chunks).
const RUBBLE_MAX: usize = 360;

fn overlap(a: &MapBox, min: Vec3, max: Vec3) -> bool {
    a.min.x < max.x
        && a.max.x > min.x
        && a.min.y < max.y
        && a.max.y > min.y
        && a.min.z < max.z
        && a.max.z > min.z
}

fn touch(a: &MapBox, b: &MapBox) -> bool {
    let e = 0.03;
    a.min.x < b.max.x + e
        && a.max.x > b.min.x - e
        && a.min.y < b.max.y + e
        && a.max.y > b.min.y - e
        && a.min.z < b.max.z + e
        && a.max.z > b.min.z - e
}

fn alive(b: &MapBox) -> bool {
    b.visible || b.solid
}

/// `b` minus the box `min..max`: up to six pieces that fill the rest.
fn subtract(b: &MapBox, min: Vec3, max: Vec3) -> Vec<MapBox> {
    let mut out = Vec::new();
    let mut piece = |lo: Vec3, hi: Vec3| {
        let s = hi - lo;
        let o = b.max - b.min;
        // (a thin plank stays a thin plank; only the slivers a cut leaves go)
        let thin = |k: usize| s[k] < SLIVER.min(o[k] * 0.999);
        if s.min_element() <= 1e-4 || thin(0) || thin(1) || thin(2) {
            return;
        }
        out.push(MapBox {
            min: lo,
            max: hi,
            ..*b
        });
    };
    let c0 = min.max(b.min);
    let c1 = max.min(b.max);
    // left and right of the cut: the full height and depth
    piece(b.min, vec3(c0.x, b.max.y, b.max.z));
    piece(vec3(c1.x, b.min.y, b.min.z), b.max);
    // in front and behind, between those
    piece(vec3(c0.x, b.min.y, b.min.z), vec3(c1.x, b.max.y, c0.z));
    piece(vec3(c0.x, b.min.y, c1.z), vec3(c1.x, b.max.y, b.max.z));
    // below and above
    piece(vec3(c0.x, b.min.y, c0.z), vec3(c1.x, c0.y, c1.z));
    piece(vec3(c0.x, c1.y, c0.z), vec3(c1.x, b.max.y, c1.z));
    out
}

/// A tall thin solid thing: a tree trunk, a post, a pole.
fn is_pole(b: &MapBox) -> bool {
    let s = b.max - b.min;
    b.solid && s.y > 2.0 && s.y < 9.0 && s.x.max(s.z) < 0.8 && s.y > 3.5 * s.x.max(s.z)
}

impl Wreck {
    pub fn new(map: &Map) -> Wreck {
        Wreck {
            orig: map.boxes.clone(),
            orig_terrain: map.terrain.clone(),
            grid: Grid::new(map),
            log: Vec::new(),
        }
    }

    /// Puts the map back as it was before any blast. The world is built anew (its doors
    /// keep their state).
    pub fn reset(&mut self, map: &mut Map, world: &mut World) {
        if self.log.is_empty() {
            return;
        }
        self.log.clear();
        map.boxes = self.orig.clone();
        map.terrain = self.orig_terrain.clone();
        let mut w = World::from_map(map);
        for k in 0..w.doors.len() {
            w.set_door_state(k, world.door_state(k));
        }
        *world = w;
        self.grid = Grid::new(map);
    }

    /// Whether a blast may break this box: not the edge of the map, not the clips nobody
    /// sees, not the ground under a terrain map.
    fn breakable(map: &Map, b: &MapBox) -> bool {
        if !b.visible {
            return false;
        }
        let c = (b.min + b.max) * 0.5;
        let inside = c.x > map.min.x && c.x < map.max.x && c.z > map.min.y && c.z < map.max.y;
        inside && !Self::is_ground(b)
    }

    /// The flat ground slab of a small map.
    fn is_ground(b: &MapBox) -> bool {
        b.max.y <= 0.01 && b.min.y < -0.5
    }

    /// Height of whatever is under (x, z) at `y` or below: boxes and the ground.
    fn floor_at(&self, map: &Map, x0: f32, z0: f32, x1: f32, z1: f32, y: f32) -> f32 {
        let mut top = f32::MIN;
        let lo = vec3(x0, f32::MIN, z0);
        let hi = vec3(x1, y + 0.01, z1);
        for i in self.grid.near(lo, hi) {
            let b = &map.boxes[i];
            if b.solid && b.max.y <= y + 0.01 && overlap(b, lo, hi) {
                top = top.max(b.max.y);
            }
        }
        if let Some(t) = &map.terrain {
            let c = vec2((x0 + x1) * 0.5, (z0 + z1) * 0.5);
            let g = t
                .height(c.x, c.y)
                .max(t.height(x0, z0))
                .max(t.height(x1, z1));
            if g <= y + 0.3 {
                top = top.max(g);
            }
        }
        if top == f32::MIN {
            top = 0.0;
        }
        top
    }

    fn kill(&mut self, map: &mut Map, world: &mut World, i: usize, ch: &mut Change) {
        let b = map.boxes[i];
        if !alive(&b) {
            return;
        }
        self.grid.remove(i, &b);
        world.remove_map_box(i);
        ch.grow(b.min, b.max);
        ch.removed.push(i);
        ch.touched.push((b.min + b.max) * 0.5);
        map.boxes[i] = MapBox {
            min: Vec3::splat(-1.0e6),
            max: Vec3::splat(-1.0e6),
            solid: false,
            visible: false,
            ..b
        };
    }

    fn add(&mut self, map: &mut Map, world: &mut World, b: MapBox, ch: &mut Change) -> usize {
        let i = map.boxes.len();
        map.boxes.push(b);
        world.add_map_box(&b);
        self.grid.add(i, &b);
        ch.grow(b.min, b.max);
        ch.touched.push((b.min + b.max) * 0.5);
        i
    }

    /// Lets a blast go off: cuts, brings down, topples, dents. Returns what changed.
    pub fn blast(&mut self, map: &mut Map, world: &mut World, blast: Blast) -> Change {
        let seq = self.log.len() as u64;
        self.log.push(blast);
        let pos = Vec3::from(blast.pos);
        let power = blast.power.clamp(0.05, 4.0);
        let mut ch = Change {
            min: pos,
            max: pos,
            ..Default::default()
        };
        let mut rng = Rng::new(0xB1A5 + seq * 7919);
        let soft = power * 1.6;
        let reach = Vec3::splat(soft * 1.8 + 0.5);
        let mut fresh: Vec<usize> = Vec::new();

        // ---- the cut: a big chunk and two smaller ones off to the side, so the hole
        // has ragged edges
        let mut cuts: Vec<(Vec3, f32)> = vec![(pos, 0.85)];
        for _ in 0..2 {
            let off = vec3(
                rng.range(-1.0, 1.0),
                rng.range(-0.8, 0.8),
                rng.range(-1.0, 1.0),
            ) * power
                * 0.8;
            cuts.push((pos + off, rng.range(0.4, 0.62)));
        }
        for i in self.grid.near(pos - reach, pos + reach) {
            let b = map.boxes[i];
            if !alive(&b) {
                continue;
            }
            let ground = Self::is_ground(&b);
            // the boxes cut out of this one (min, max)
            let boxes: Vec<(Vec3, Vec3)> = if ground {
                // a pit in the ground of a small map, if it went off down there
                if pos.y > power + 0.3 || map.terrain.is_some() {
                    continue;
                }
                let depth = (power * 0.25).clamp(0.2, 0.45);
                let h = power * 0.75;
                vec![(
                    vec3(pos.x - h, -depth, pos.z - h),
                    vec3(pos.x + h, 1.0, pos.z + h),
                )]
            } else if !Self::breakable(map, &b) {
                continue;
            } else if is_pole(&b) && {
                let c = (b.min + b.max) * 0.5;
                vec2(c.x - pos.x, c.z - pos.z).length() < soft * 1.5 && pos.y < b.max.y
            } {
                // a tree or a post near the blast snaps at the foot
                let y = (b.min.y + 0.3).max(pos.y.min(b.max.y - 1.0) - 0.1);
                vec![(
                    vec3(b.min.x - 0.01, y, b.min.z - 0.01),
                    vec3(b.max.x + 0.01, y + 0.22, b.max.z + 0.01),
                )]
            } else if !b.solid && b.min.y > pos.y + 1.2 {
                // light things up above (a tree's crown, an awning) are blown about, not
                // cut: they fall with what holds them
                continue;
            } else {
                let h = if b.mat == Mat::Solid && b.solid {
                    power
                } else {
                    soft
                };
                cuts.iter()
                    .map(|&(c, k)| (c - Vec3::splat(h * k), c + Vec3::splat(h * k)))
                    .collect()
            };
            if !boxes.iter().any(|&(lo, hi)| overlap(&b, lo, hi)) {
                continue;
            }
            let mut pieces = vec![b];
            for &(lo, hi) in &boxes {
                let mut next = Vec::new();
                for p in pieces {
                    if overlap(&p, lo, hi) {
                        ch.blown.push(MapBox {
                            min: lo.max(p.min),
                            max: hi.min(p.max),
                            ..p
                        });
                        next.extend(subtract(&p, lo, hi));
                    } else {
                        next.push(p);
                    }
                }
                pieces = next;
            }
            self.kill(map, world, i, &mut ch);
            for mut p in pieces {
                if ground
                    && p.max.y < -0.05
                    && boxes.iter().any(|&(lo, hi)| {
                        p.max.y <= lo.y + 0.01
                            && p.min.x >= lo.x - 0.01
                            && p.max.x <= hi.x + 0.01
                            && p.min.z >= lo.z - 0.01
                            && p.max.z <= hi.z + 0.01
                    })
                {
                    // the scorched bottom of the pit
                    p.color = crate::map::shade(p.color, 0.55);
                }
                fresh.push(self.add(map, world, p, &mut ch));
            }
        }

        // ---- some of what was blown out lies under it as rubble
        let mut budget = RUBBLE_MAX;
        let rubble: Vec<MapBox> = ch
            .blown
            .iter()
            .filter(|b| b.solid && b.visible && !Self::is_ground(b) && b.max.y > b.min.y + 0.3)
            .filter(|b| (b.max - b.min).element_product() > 0.15)
            .map(|b| {
                let s = b.max - b.min;
                // a third of it, spread a little away from the blast
                let away = ((b.min + b.max) * 0.5 - pos) * vec3(1.0, 0.0, 1.0);
                let push = away.normalize_or_zero() * 0.4;
                MapBox {
                    min: b.min + push,
                    max: vec3(b.max.x, b.min.y + s.y * 0.33, b.max.z) + push,
                    ..*b
                }
            })
            .collect();
        if !rubble.is_empty() {
            self.heap(map, world, rubble, &mut rng, &mut budget, &mut ch);
        }

        // ---- the ground of a terrain map gets a crater
        if let Some(t) = map.terrain.as_ref() {
            let g = t.height(pos.x, pos.z);
            if pos.y - g < power * 1.2 {
                let r = power * 1.7;
                let depth = (power * 0.45).min(1.0);
                // (the world lets go of its handle first: then the map's is the only one
                // and the heights are changed in place, not copied)
                world.terrain = None;
                let hm = Arc::make_mut(map.terrain.as_mut().unwrap());
                let n = (r / hm.cell).ceil() as i64 + 1;
                if let Some((ci, cj)) = hm.index_of(pos.x, pos.z) {
                    for dj in -n..=n {
                        for di in -n..=n {
                            let (i, j) = (ci as i64 + di, cj as i64 + dj);
                            if i < 0 || j < 0 || i >= hm.w as i64 || j >= hm.h as i64 {
                                continue;
                            }
                            let p = hm.point(i as usize, j as usize);
                            let d = vec2(p.x - pos.x, p.z - pos.z).length() / r;
                            if d >= 1.0 {
                                continue;
                            }
                            let dent = depth * (1.0 - d * d);
                            hm.set(i as usize, j as usize, p.y - dent);
                            if d < 0.8 {
                                let k = j as usize * hm.w + i as usize;
                                hm.paint[k] = crate::terrain::paint::DIRT;
                            }
                        }
                    }
                }
                world.terrain = map.terrain.clone();
                ch.crater = Some((vec3(pos.x, g, pos.z), r));
                ch.grow(pos - Vec3::splat(r), pos + Vec3::splat(r));
            }
        }

        // ---- what is left hanging falls
        let look = Vec3::splat(soft * 1.8 + 1.5);
        let mut seeds: Vec<usize> = self
            .grid
            .near(pos - look, pos + look)
            .into_iter()
            .filter(|&i| {
                let b = &map.boxes[i];
                b.solid && Self::breakable(map, b) && overlap(b, pos - look, pos + look)
            })
            .collect();
        seeds.extend(
            fresh
                .iter()
                .copied()
                .filter(|&i| map.boxes[i].solid && Self::breakable(map, &map.boxes[i])),
        );
        seeds.sort_unstable();
        seeds.dedup();
        let mut held: Vec<usize> = Vec::new();
        let mut falling: Vec<Vec<usize>> = Vec::new();
        for s in seeds {
            if held.contains(&s) || falling.iter().any(|f| f.contains(&s)) {
                continue;
            }
            let (group, anchored) = self.component(map, s);
            if anchored {
                held.extend(group);
            } else {
                falling.push(group);
            }
        }
        for group in falling {
            // what hangs on it (leaves, trims) goes with it
            let (lo, hi) = group.iter().fold((Vec3::MAX, Vec3::MIN), |(lo, hi), &i| {
                (lo.min(map.boxes[i].min), hi.max(map.boxes[i].max))
            });
            let mut hangers: Vec<usize> = Vec::new();
            for i in self.grid.near(lo - Vec3::ONE, hi + Vec3::ONE) {
                let b = &map.boxes[i];
                if b.solid || !Self::breakable(map, b) || !alive(b) {
                    continue;
                }
                if !group.iter().any(|&g| touch(b, &map.boxes[g])) {
                    continue;
                }
                // still fixed to something standing?
                let fixed = self
                    .grid
                    .near(b.min - Vec3::ONE, b.max + Vec3::ONE)
                    .into_iter()
                    .any(|k| {
                        let o = &map.boxes[k];
                        o.solid && !group.contains(&k) && touch(b, o)
                    });
                if !fixed {
                    hangers.push(i);
                }
            }
            let poles: Vec<usize> = group
                .iter()
                .copied()
                .filter(|&i| is_pole(&map.boxes[i]))
                .collect();
            if group.len() == 1 && poles.len() == 1 {
                self.topple(map, world, group[0], &hangers, pos, &mut ch);
            } else {
                let mut all = group.clone();
                all.extend(hangers);
                self.collapse(map, world, &all, &mut rng, &mut budget, &mut ch);
            }
        }

        // ---- loose trims and leaves the blast left with nothing to hang on
        for i in self.grid.near(pos - look, pos + look) {
            let b = map.boxes[i];
            if b.solid || !Self::breakable(map, &b) || !alive(&b) || b.min.y < 0.3 {
                continue;
            }
            if map.terrain.as_ref().is_some_and(|t| {
                b.min.y < t.height((b.min.x + b.max.x) * 0.5, (b.min.z + b.max.z) * 0.5) + 0.3
            }) {
                continue;
            }
            let fixed = self
                .grid
                .near(b.min - Vec3::ONE, b.max + Vec3::ONE)
                .into_iter()
                .any(|k| {
                    let o = &map.boxes[k];
                    o.solid && touch(&b, o)
                });
            if !fixed {
                self.collapse(map, world, &[i], &mut rng, &mut budget, &mut ch);
            }
        }
        ch
    }

    /// The solid breakable boxes joined to box `s`, and whether any of them stands on
    /// something that holds (the ground, or a thing that does not break).
    fn component(&self, map: &Map, s: usize) -> (Vec<usize>, bool) {
        let mut seen = vec![s];
        let mut k = 0;
        while k < seen.len() {
            let b = map.boxes[seen[k]];
            k += 1;
            if self.rests_on_ground(map, &b) {
                return (seen, true);
            }
            if seen.len() > 2500 {
                // a whole town: it stands
                return (seen, true);
            }
            for j in self
                .grid
                .near(b.min - Vec3::splat(0.05), b.max + Vec3::splat(0.05))
            {
                let o = &map.boxes[j];
                if !o.solid || !touch(&b, o) || seen.contains(&j) {
                    continue;
                }
                if !Self::breakable(map, o) {
                    // fixed to the map's frame (the ground, the edge)
                    return (seen, true);
                }
                seen.push(j);
            }
        }
        (seen, false)
    }

    fn rests_on_ground(&self, map: &Map, b: &MapBox) -> bool {
        if b.min.y <= 0.03 && map.terrain.is_none() {
            return true;
        }
        map.terrain.as_ref().is_some_and(|t| {
            let g = t
                .height(b.min.x, b.min.z)
                .min(t.height(b.max.x, b.max.z))
                .min(t.height(b.min.x, b.max.z))
                .min(t.height(b.max.x, b.min.z));
            b.min.y <= g + 0.15
        })
    }

    /// Boxes with nothing under them come down in chunks and lie in a heap.
    fn collapse(
        &mut self,
        map: &mut Map,
        world: &mut World,
        boxes: &[usize],
        rng: &mut Rng,
        budget: &mut usize,
        ch: &mut Change,
    ) {
        let order: Vec<MapBox> = boxes.iter().map(|&i| map.boxes[i]).filter(alive).collect();
        for &i in boxes {
            self.kill(map, world, i, ch);
        }
        self.heap(map, world, order, rng, budget, ch);
    }

    /// These come down in chunks onto whatever is under them.
    fn heap(
        &mut self,
        map: &mut Map,
        world: &mut World,
        mut order: Vec<MapBox>,
        rng: &mut Rng,
        budget: &mut usize,
        ch: &mut Change,
    ) {
        // the lowest first: they land first, the rest on top of them
        order.sort_by(|a, b| a.min.y.total_cmp(&b.min.y));
        let total: f32 = order
            .iter()
            .map(|b| (b.max - b.min).element_product())
            .sum();
        for b in order {
            let s = b.max - b.min;
            // chunks about a metre (bigger when a lot comes down at once)
            let share = ((s.element_product() / total.max(1e-3)) * *budget as f32).max(1.0);
            let mut size = 1.1f32;
            let count = |size: f32| (s / size).ceil().max(Vec3::ONE);
            while count(size).element_product() > share.max(1.0) && size < 8.0 {
                size *= 1.25;
            }
            let n = count(size);
            let (nx, ny, nz) = (n.x as usize, n.y as usize, n.z as usize);
            *budget = budget.saturating_sub(nx * ny * nz);
            let step = s / n;
            for iy in 0..ny {
                for iz in 0..nz {
                    for ix in 0..nx {
                        let lo = b.min + step * vec3(ix as f32, iy as f32, iz as f32);
                        // a chunk is a bit smaller than its share (gaps in the heap) and
                        // lands lying down
                        let mut e = step
                            * vec3(
                                rng.range(0.6, 0.85),
                                rng.range(0.6, 0.85),
                                rng.range(0.6, 0.85),
                            );
                        if e.y > e.x.max(e.z) {
                            if rng.chance(0.5) {
                                e = vec3(e.y, e.x, e.z);
                            } else {
                                e = vec3(e.x, e.z, e.y);
                            }
                        }
                        let c = lo
                            + step * 0.5
                            + vec3(rng.range(-0.35, 0.35), 0.0, rng.range(-0.35, 0.35));
                        let (x0, z0) = (c.x - e.x * 0.5, c.z - e.z * 0.5);
                        let (x1, z1) = (x0 + e.x, z0 + e.z);
                        let floor = self.floor_at(map, x0, z0, x1, z1, lo.y);
                        let piece = MapBox {
                            min: vec3(x0, floor, z0),
                            max: vec3(x1, floor + e.y, z1),
                            color: crate::map::shade(b.color, 0.88),
                            ..b
                        };
                        let i = self.add(map, world, piece, ch);
                        ch.falls.push((i, (lo.y - floor).max(0.0)));
                    }
                }
            }
        }
    }

    /// A tree or a post cut at its foot falls over, away from the blast, with its leaves.
    fn topple(
        &mut self,
        map: &mut Map,
        world: &mut World,
        trunk: usize,
        hangers: &[usize],
        from: Vec3,
        ch: &mut Change,
    ) {
        let t = map.boxes[trunk];
        let c = (t.min + t.max) * 0.5;
        let away = vec2(c.x - from.x, c.z - from.z);
        // (along x or z: boxes cannot lie at a slant)
        let dir = if away.x.abs() >= away.y.abs() {
            vec3(away.x.signum(), 0.0, 0.0)
        } else {
            vec3(0.0, 0.0, away.y.signum())
        };
        let pivot = vec3(c.x, t.min.y, c.z);
        // y turns into `dir`, `dir` into −y
        let turn = |p: Vec3| -> Vec3 {
            let d = p - pivot;
            let along = d.x * dir.x + d.z * dir.z;
            let side = if dir.x != 0.0 {
                vec3(0.0, 0.0, d.z)
            } else {
                vec3(d.x, 0.0, 0.0)
            };
            pivot + dir * d.y + side + vec3(0.0, -along, 0.0)
        };
        let mut all: Vec<MapBox> = vec![t];
        all.extend(hangers.iter().map(|&i| map.boxes[i]).filter(alive));
        for i in std::iter::once(trunk).chain(hangers.iter().copied()) {
            self.kill(map, world, i, ch);
        }
        let mut laid: Vec<MapBox> = all
            .iter()
            .map(|b| {
                let (a, z) = (turn(b.min), turn(b.max));
                MapBox {
                    min: a.min(z),
                    max: a.max(z),
                    ..*b
                }
            })
            .collect();
        // the crown squashes flat under the trunk where it hits the ground
        let bottom = laid[0].min.y;
        for b in laid.iter_mut().skip(1) {
            b.min.y = b.min.y.max(bottom).min(b.max.y - 0.2);
        }
        // down onto the ground where it lands
        let lo = laid.iter().fold(Vec3::MAX, |m, b| m.min(b.min));
        let hi = laid.iter().fold(Vec3::MIN, |m, b| m.max(b.max));
        let floor = self.floor_at(map, lo.x, lo.z, hi.x, hi.z, pivot.y + 0.5);
        let dy = floor - laid[0].min.y.min(lo.y);
        let mut idx = Vec::new();
        for b in laid.iter_mut() {
            b.min.y += dy;
            b.max.y += dy;
            idx.push(self.add(map, world, *b, ch));
        }
        ch.topples.push(Topple {
            pivot: pivot + vec3(0.0, dy, 0.0),
            dir,
            boxes: idx,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::MapBuilder;
    use glam::Vec2;

    pub fn yard() -> Map {
        let mut b = MapBuilder::new();
        b.ground_and_bounds(
            Vec2::new(-30.0, -30.0),
            Vec2::new(30.0, 30.0),
            [90, 140, 70],
            [120, 120, 120],
            3.0,
        );
        // a hut: four walls and a roof
        b.block(-3.0, 0.0, -3.0, 3.0, 3.0, -2.8, [200, 180, 150]);
        b.block(-3.0, 0.0, 2.8, 3.0, 3.0, 3.0, [200, 180, 150]);
        b.block(-3.0, 0.0, -2.8, -2.8, 3.0, 2.8, [200, 180, 150]);
        b.block(2.8, 0.0, -2.8, 3.0, 3.0, 2.8, [200, 180, 150]);
        b.block(-3.2, 3.0, -3.2, 3.2, 3.2, 3.2, [150, 60, 50]);
        // a tree
        b.tree(12.0, 0.0, false);
        let mut m = crate::maps::test_yard();
        m.boxes = b.boxes;
        m
    }

    fn live(m: &Map) -> usize {
        m.boxes.iter().filter(|b| alive(b)).count()
    }

    #[test]
    fn a_blast_cuts_a_hole_in_a_wall() {
        let mut m = yard();
        let mut w = World::from_map(&m);
        let mut wr = Wreck::new(&m);
        let ch = wr.blast(
            &mut m,
            &mut w,
            Blast {
                pos: [0.0, 1.5, -3.5],
                power: 0.8,
            },
        );
        assert!(!ch.removed.is_empty());
        // the bullet that hit the wall goes through the hole now
        assert!(w
            .raycast(vec3(0.0, 1.5, -6.0), Vec3::Z, 6.0)
            .is_none_or(|h| h.0 > 3.5));
        // and the hut still stands: its roof is up there
        assert!(w
            .raycast(vec3(0.0, 10.0, 0.0), -Vec3::Y, 10.0)
            .is_some_and(|h| h.0 < 7.1));
    }

    #[test]
    fn a_roof_with_no_walls_comes_down() {
        let mut m = yard();
        let mut w = World::from_map(&m);
        let mut wr = Wreck::new(&m);
        let mut falls = 0;
        for p in [
            [0.0, -0.2, -2.9],
            [0.0, -0.2, 2.9],
            [-2.9, -0.2, 0.0],
            [2.9, -0.2, 0.0],
        ] {
            falls += wr
                .blast(&mut m, &mut w, Blast { pos: p, power: 3.6 })
                .falls
                .len();
        }
        assert!(falls > 10);
        // nothing up at roof height now, a heap on the floor
        let hit = w.raycast(vec3(0.0, 10.0, 0.0), -Vec3::Y, 20.0).unwrap();
        assert!(hit.0 > 7.5, "{}", hit.0);
    }

    #[test]
    fn a_tree_falls_over() {
        let mut m = yard();
        let mut w = World::from_map(&m);
        let mut wr = Wreck::new(&m);
        let ch = wr.blast(
            &mut m,
            &mut w,
            Blast {
                pos: [10.0, 0.5, 0.0],
                power: 1.0,
            },
        );
        assert_eq!(ch.topples.len(), 1);
        // lying along +x on the ground now
        let t = &m.boxes[ch.topples[0].boxes[0]];
        assert!(t.max.x - t.min.x > 3.0, "{t:?}");
        assert!(t.max.y < 1.0);
    }

    #[test]
    fn the_same_blasts_make_the_same_map() {
        let blasts = [
            Blast {
                pos: [0.0, 1.0, -3.0],
                power: 2.0,
            },
            Blast {
                pos: [2.5, 2.0, 0.0],
                power: 1.5,
            },
            Blast {
                pos: [11.0, 0.3, 0.5],
                power: 1.0,
            },
            Blast {
                pos: [-20.0, 0.1, 5.0],
                power: 1.2,
            },
        ];
        let run = || {
            let mut m = yard();
            let mut w = World::from_map(&m);
            let mut wr = Wreck::new(&m);
            for b in blasts {
                wr.blast(&mut m, &mut w, b);
            }
            m.boxes
                .iter()
                .map(|b| (b.min.to_array(), b.max.to_array()))
                .collect::<Vec<_>>()
        };
        assert_eq!(run(), run());
    }

    #[test]
    fn reset_puts_it_back() {
        let mut m = yard();
        let n = live(&m);
        let mut w = World::from_map(&m);
        let mut wr = Wreck::new(&m);
        wr.blast(
            &mut m,
            &mut w,
            Blast {
                pos: [0.0, 1.0, -3.0],
                power: 2.0,
            },
        );
        assert_ne!(live(&m), n);
        wr.reset(&mut m, &mut w);
        assert_eq!(live(&m), n);
        assert!(w.raycast(vec3(0.0, 1.5, -6.0), Vec3::Z, 6.0).is_some());
    }

    #[test]
    fn hills_get_craters_and_mend() {
        let mut m = yard();
        m.terrain = Some(Arc::new(crate::terrain::Heightmap::flat(
            Vec2::new(-40.0, -40.0),
            Vec2::new(40.0, 40.0),
            2.0,
            0.0,
        )));
        let mut w = World::from_map(&m);
        let mut wr = Wreck::new(&m);
        let ch = wr.blast(
            &mut m,
            &mut w,
            Blast {
                pos: [20.0, 0.3, 20.0],
                power: 1.5,
            },
        );
        assert!(ch.crater.is_some());
        let g = w.ground(20.0, 20.0).unwrap();
        assert!(g < -0.4, "{g}");
        assert!(w.ground(30.0, 30.0).unwrap().abs() < 1e-6);
        wr.reset(&mut m, &mut w);
        assert!(w.ground(20.0, 20.0).unwrap().abs() < 1e-6);
    }

    #[test]
    fn the_ground_gets_a_pit() {
        let mut m = yard();
        let mut w = World::from_map(&m);
        let mut wr = Wreck::new(&m);
        wr.blast(
            &mut m,
            &mut w,
            Blast {
                pos: [-20.0, 0.2, 0.0],
                power: 1.0,
            },
        );
        let h = w.raycast(vec3(-20.0, 5.0, 0.0), -Vec3::Y, 10.0).unwrap();
        assert!(h.0 > 5.15, "{}", h.0);
    }
}
#[cfg(test)]
mod timing {
    use super::*;
    #[test]
    #[ignore]
    fn blasts_on_real_maps() {
        for id in crate::map::MapId::EVERY {
            let mut m = crate::maps::build(id);
            let mut w = World::from_map(&m);
            let t0 = std::time::Instant::now();
            let mut wr = Wreck::new(&m);
            let t_new = t0.elapsed();
            let mut rng = Rng::new(5);
            // blasts next to walls: aim at random solid boxes
            let solid: Vec<MapBox> = m
                .boxes
                .iter()
                .filter(|b| b.solid && b.visible && b.min.y > -0.5)
                .copied()
                .collect();
            let t0 = std::time::Instant::now();
            let (mut worst, mut falls) = (0.0f64, 0);
            for _ in 0..200 {
                let b = solid[rng.below(solid.len())];
                let p = vec3(
                    rng.range(b.min.x, b.max.x),
                    rng.range(b.min.y, b.max.y.min(b.min.y + 3.0)),
                    rng.range(b.min.z, b.max.z),
                );
                let t = std::time::Instant::now();
                let ch = wr.blast(
                    &mut m,
                    &mut w,
                    Blast {
                        pos: p.to_array(),
                        power: if rng.chance(0.3) { 1.6 } else { 1.1 },
                    },
                );
                worst = worst.max(t.elapsed().as_secs_f64());
                falls += ch.falls.len();
            }
            eprintln!(
                "{:?}: boxes {} new {:?} 200 blasts {:?} worst {:.1} ms falls {} boxes now {}",
                id,
                wr.orig.len(),
                t_new,
                t0.elapsed(),
                worst * 1000.0,
                falls,
                m.boxes.len()
            );
        }
    }
}
