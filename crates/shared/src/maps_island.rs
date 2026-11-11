//! «Острів» — a 2 × 2 km map for the battle royale: mountains in the north-east, a
//! river running from their foot to the southern sea, a lake in the north-west, forests,
//! fields, four towns, a dozen villages and two army camps joined by dirt roads and bridges.
//!
//! Everything is generated from fixed seeds, so the client and the server build exactly
//! the same island.

use crate::map::{door, pal, shade, window, Map, MapBuilder, MapId, Side, Spawn};
use crate::props::Tower;
use crate::rng::Rng;
use crate::terrain::{fbm, paint, Heightmap};
use glam::{vec2, vec3, Vec2};
use std::f32::consts::PI;
use std::sync::Arc;

/// Half the playable size (m).
pub const HALF: f32 = 1000.0;
const SEED: u32 = 1977;
/// Height map spacing (m).
const CELL: f32 = 2.0;
/// Water surface.
const WATER: f32 = 0.0;
/// Top of the bridge decks.
const DECK: f32 = 1.6;

/// The river from the foot of the mountains to the southern sea.
const RIVER: [(f32, f32); 9] = [
    (340.0, -300.0),
    (240.0, -140.0),
    (120.0, 0.0),
    (20.0, 120.0),
    (-120.0, 220.0),
    (-220.0, 380.0),
    (-260.0, 600.0),
    (-300.0, 840.0),
    (-340.0, 1160.0),
];
const LAKE: (f32, f32, f32) = (-540.0, -440.0, 110.0);
const PEAK: (f32, f32) = (620.0, -620.0);

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Town,
    Village,
    /// A fenced army camp: barracks, containers, watchtowers.
    Base,
}

/// A settlement: a levelled pad with houses on it.
#[derive(Clone, Copy)]
struct Place {
    x: f32,
    z: f32,
    w: f32,
    d: f32,
    kind: Kind,
}

impl Place {
    fn min(&self) -> Vec2 {
        vec2(self.x - self.w / 2.0, self.z - self.d / 2.0)
    }
    fn max(&self) -> Vec2 {
        vec2(self.x + self.w / 2.0, self.z + self.d / 2.0)
    }
    /// Distance from a point to the pad (0 inside).
    fn dist(&self, x: f32, z: f32) -> f32 {
        let dx = (self.min().x - x).max(x - self.max().x).max(0.0);
        let dz = (self.min().y - z).max(z - self.max().y).max(0.0);
        (dx * dx + dz * dz).sqrt()
    }
}

const fn place(x: f32, z: f32, w: f32, d: f32, kind: Kind) -> Place {
    Place { x, z, w, d, kind }
}

const PLACES: [Place; 17] = [
    // the town in the middle, by the river
    place(-120.0, -80.0, 128.0, 104.0, Kind::Town),
    // the port town on the south-east coast
    place(300.0, 660.0, 108.0, 88.0, Kind::Town),
    place(-660.0, 80.0, 60.0, 52.0, Kind::Village),
    place(-300.0, -660.0, 60.0, 52.0, Kind::Village),
    place(220.0, -760.0, 52.0, 52.0, Kind::Village),
    place(680.0, 80.0, 60.0, 52.0, Kind::Village),
    place(-660.0, 660.0, 60.0, 52.0, Kind::Village),
    place(0.0, 500.0, 52.0, 52.0, Kind::Village),
    // the western market town and the south-western one
    place(-640.0, -180.0, 116.0, 96.0, Kind::Town),
    place(-430.0, 420.0, 108.0, 92.0, Kind::Town),
    place(440.0, -60.0, 60.0, 52.0, Kind::Village),
    place(-340.0, -390.0, 56.0, 52.0, Kind::Village),
    place(-740.0, 340.0, 56.0, 52.0, Kind::Village),
    place(300.0, 240.0, 60.0, 52.0, Kind::Village),
    place(-60.0, 790.0, 56.0, 52.0, Kind::Village),
    // army camps: the best guns are there
    place(-40.0, -540.0, 96.0, 76.0, Kind::Base),
    place(640.0, 420.0, 96.0, 76.0, Kind::Base),
];

/// Dirt roads between the settlements.
const ROADS: [&[(f32, f32)]; 18] = [
    &[(-660.0, 80.0), (-400.0, 20.0), (-248.0, -40.0)],
    &[(8.0, -80.0), (400.0, -44.0), (680.0, 80.0)],
    &[(-100.0, 24.0), (-80.0, 280.0), (0.0, 500.0), (300.0, 660.0)],
    &[(-160.0, -184.0), (-240.0, -440.0), (-300.0, -660.0)],
    &[(220.0, -760.0), (100.0, -440.0), (-20.0, -184.0)],
    &[(300.0, 660.0), (560.0, 400.0), (680.0, 80.0)],
    &[(-660.0, 660.0), (-360.0, 580.0), (0.0, 500.0)],
    // new towns, villages and camps
    &[(-660.0, 80.0), (-640.0, -132.0)],
    &[(-582.0, -180.0), (-400.0, -150.0), (-248.0, -80.0)],
    &[(-430.0, 374.0), (-300.0, 200.0), (-100.0, 24.0)],
    &[(-484.0, 420.0), (-600.0, 400.0), (-712.0, 340.0)],
    &[(-430.0, 466.0), (-560.0, 600.0), (-660.0, 660.0)],
    &[(440.0, -60.0), (400.0, -44.0)],
    &[(-340.0, -416.0), (-300.0, -634.0)],
    &[(300.0, 266.0), (300.0, 400.0), (300.0, 614.0)],
    &[(270.0, 240.0), (-60.0, 200.0), (-80.0, 280.0)],
    &[(-60.0, 764.0), (0.0, 526.0)],
    &[(-40.0, -578.0), (100.0, -440.0)],
];

/// Farm fields: (min x, min z, max x, max z, stripes along x).
const FIELDS: [(f32, f32, f32, f32, bool); 16] = [
    (-840.0, 0.0, -740.0, 140.0, false),
    (-600.0, 160.0, -480.0, 240.0, true),
    (-440.0, -600.0, -372.0, -500.0, false),
    (120.0, -700.0, 160.0, -600.0, true),
    (740.0, 160.0, 840.0, 260.0, true),
    (760.0, -60.0, 860.0, 20.0, false),
    (-800.0, 500.0, -700.0, 580.0, true),
    (-560.0, 720.0, -460.0, 820.0, false),
    (80.0, 360.0, 180.0, 440.0, true),
    // around the new villages
    (-820.0, -320.0, -740.0, -240.0, true),
    (-520.0, 500.0, -460.0, 580.0, false),
    (480.0, 20.0, 560.0, 90.0, true),
    (380.0, 180.0, 460.0, 260.0, false),
    (-200.0, 760.0, -120.0, 840.0, true),
    (-480.0, -330.0, -400.0, -280.0, false),
    (-880.0, 380.0, -820.0, 460.0, true),
];

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Distance from `p` to the segment a–b and how far along it (0..1) the closest point is.
fn seg_dist(p: Vec2, a: Vec2, b: Vec2) -> (f32, f32) {
    let ab = b - a;
    let t = ((p - a).dot(ab) / ab.length_squared().max(1e-6)).clamp(0.0, 1.0);
    ((a + ab * t - p).length(), t)
}

fn line_dist(p: Vec2, pts: &[(f32, f32)]) -> f32 {
    pts.windows(2)
        .map(|w| seg_dist(p, vec2(w[0].0, w[0].1), vec2(w[1].0, w[1].1)).0)
        .fold(f32::MAX, f32::min)
}

/// River half width at a point along it (narrow at the spring, wide at the mouth).
fn river_half_width(z: f32) -> f32 {
    5.0 + smoothstep(-300.0, 900.0, z) * 7.0
}

/// Ridged noise for mountain crests.
fn ridged(x: f32, z: f32) -> f32 {
    let n = fbm(SEED + 3, x, z, 4);
    1.0 - (2.0 * n - 1.0).abs()
}

/// The untouched land: island shape, rolling hills and the mountains.
fn natural(x: f32, z: f32) -> f32 {
    // a wobbly coastline
    let r = vec2(x, z).length() * 1.0 + 140.0 * (fbm(SEED, x / 320.0, z / 320.0, 3) - 0.5);
    let r = r.max(vec2(x, z).abs().max_element() * 0.92);
    let land = smoothstep(1000.0, 880.0, r);
    let hills = 3.0 + 12.0 * fbm(SEED + 1, x / 260.0, z / 260.0, 4);
    let peak = vec2(x - PEAK.0, z - PEAK.1).length();
    let m = smoothstep(560.0, 140.0, peak);
    let mountains = m * m * (22.0 + 64.0 * ridged(x / 150.0, z / 150.0));
    let sw = vec2(x + 520.0, z - 500.0).length();
    let downs = smoothstep(260.0, 60.0, sw) * 14.0 * ridged(x / 90.0 + 3.0, z / 90.0);
    // a second range of low hills in the north-west
    let nw = vec2(x + 700.0, z + 700.0).length();
    let ridge = smoothstep(320.0, 80.0, nw) * 18.0 * ridged(x / 110.0 + 7.0, z / 110.0);
    let h = hills + mountains + downs + ridge;
    // the beach: from the land down to the shallow sea floor, and deeper out at sea
    -1.1 - 3.0 * smoothstep(960.0, 1040.0, r) + (h + 1.1) * land
}

/// A bridge where a road crosses the river.
#[derive(Clone, Copy, Debug)]
struct Bridge {
    c: Vec2,
    along_x: bool,
    /// Half length of the deck.
    half: f32,
}

fn bridges() -> Vec<Bridge> {
    let mut out = Vec::new();
    for road in ROADS {
        for rw in road.windows(2) {
            let (a, b) = (vec2(rw[0].0, rw[0].1), vec2(rw[1].0, rw[1].1));
            for vw in RIVER.windows(2) {
                let (c, d) = (vec2(vw[0].0, vw[0].1), vec2(vw[1].0, vw[1].1));
                let r = b - a;
                let s = d - c;
                let den = r.perp_dot(s);
                if den.abs() < 1e-6 {
                    continue;
                }
                let t = (c - a).perp_dot(s) / den;
                let u = (c - a).perp_dot(r) / den;
                if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                    let p = a + r * t;
                    let along_x = r.x.abs() > r.y.abs();
                    // a slanting river is wider along the deck
                    let axis = if along_x {
                        vec2(1.0, 0.0)
                    } else {
                        vec2(0.0, 1.0)
                    };
                    let sin = axis.perp_dot(s.normalize()).abs().max(0.45);
                    out.push(Bridge {
                        c: p,
                        along_x,
                        half: (river_half_width(p.y) + 10.0) / sin,
                    });
                }
            }
        }
    }
    out
}

fn heightmap() -> Heightmap {
    let ext = HALF + 40.0;
    let mut m = Heightmap::flat(vec2(-ext, -ext), vec2(ext, ext), CELL, 0.0);
    m.water = WATER;
    let river: Vec<Vec2> = RIVER.iter().map(|p| vec2(p.0, p.1)).collect();
    for j in 0..m.h {
        for i in 0..m.w {
            let p = m.point(i, j);
            let q = vec2(p.x, p.z);
            let mut h = natural(p.x, p.z);
            // river valley: a channel below the water and soft banks
            let (mut d, mut along) = (f32::MAX, 0.0);
            for (k, w) in river.windows(2).enumerate() {
                let (dd, t) = seg_dist(q, w[0], w[1]);
                if dd < d {
                    d = dd;
                    along = k as f32 + t;
                }
            }
            let hw = river_half_width(p.z);
            // the spring: the channel fades in over the first stretch
            let spring = smoothstep(0.0, 0.6, along);
            let bed = WATER - 0.9;
            let k = smoothstep(hw + 16.0 + 14.0 * spring, hw, d) * spring.max(0.15);
            if d < hw * spring + 0.5 {
                h = bed;
            } else {
                h += (bed + 1.6 - h).min(0.0) * k;
            }
            // the lake
            let lr = LAKE.2 * (0.85 + 0.3 * fbm(SEED + 5, p.x / 40.0, p.z / 40.0, 2));
            let ld = vec2(p.x - LAKE.0, p.z - LAKE.1).length();
            if ld < lr + 24.0 {
                let k = smoothstep(lr + 24.0, lr, ld);
                // shallow at the shore, deep enough to swim further in
                let deep = 2.8 * smoothstep(lr * 0.85, lr * 0.45, ld);
                h += (WATER - 1.1 - deep - h) * k;
            }
            m.set(i, j, h);
        }
    }
    // level the settlements
    for pl in PLACES {
        let mut sum = 0.0;
        let mut n = 0.0;
        let (a, b) = (pl.min(), pl.max());
        let mut x = a.x;
        while x <= b.x {
            let mut z = a.y;
            while z <= b.y {
                sum += m.height(x, z);
                n += 1.0;
                z += 4.0;
            }
            x += 4.0;
        }
        let y = (sum / n).max(2.0);
        m.flatten(a, b, y, 28.0);
    }
    // roads dip into cuttings (and run on embankments) towards the bridges
    for br in bridges() {
        let dir = if br.along_x {
            vec2(1.0, 0.0)
        } else {
            vec2(0.0, 1.0)
        };
        let side = vec2(-dir.y, dir.x);
        let reach = br.half + 34.0;
        for j in 0..m.h {
            for i in 0..m.w {
                let p = m.point(i, j);
                let rel = vec2(p.x, p.z) - br.c;
                let s = rel.dot(dir).abs();
                let across = rel.dot(side).abs();
                if s > reach || across > 12.0 {
                    continue;
                }
                let k = smoothstep(12.0, 4.0, across);
                let idx = j * m.w + i;
                let h = m.heights[idx];
                if s < br.half - 3.0 {
                    // under the deck: never higher than its underside
                    // (room to walk under it along the river: nobody gets stuck there)
                    m.heights[idx] = h + ((DECK - 2.6) - h).min(0.0) * k;
                } else if s < br.half {
                    // the abutment: the deck's ends rest on the bank
                    m.heights[idx] = h + ((DECK - 0.3) - h) * k;
                } else {
                    let target = DECK + (s - br.half) * 0.3;
                    let ks = smoothstep(reach, reach - 12.0, s);
                    m.heights[idx] = h + (target - h) * k * ks;
                }
            }
        }
    }
    // ground cover
    for j in 0..m.h {
        for i in 0..m.w {
            let p = m.point(i, j);
            let q = vec2(p.x, p.z);
            let idx = j * m.w + i;
            let mut c = paint::AUTO;
            let wet = p.y < WATER + 1.4;
            let coast = natural(p.x, p.z) < 2.2;
            if wet && coast {
                c = paint::SAND;
            }
            let forest = fbm(SEED + 7, p.x / 130.0, p.z / 130.0, 3);
            if c == paint::AUTO && forest > 0.56 && p.y > 1.5 {
                c = paint::FOREST;
            } else if c == paint::AUTO && forest < 0.4 && p.y > 1.5 {
                c = paint::MEADOW;
            }
            for &(x0, z0, x1, z1, along_x) in &FIELDS {
                if p.x >= x0 && p.x <= x1 && p.z >= z0 && p.z <= z1 && !wet {
                    let t = if along_x { p.z } else { p.x };
                    c = if (t / 4.0).floor() as i32 % 2 == 0 {
                        paint::FIELD_A
                    } else {
                        paint::FIELD_B
                    };
                }
            }
            for pl in PLACES {
                if pl.dist(p.x, p.z) < 1.0 {
                    c = paint::DIRT;
                }
            }
            if ROADS.iter().any(|r| line_dist(q, r) < 3.2) && !(p.y < WATER) {
                c = paint::ROAD;
            }
            m.paint[idx] = c;
        }
    }
    // streets inside the settlements
    for pl in PLACES {
        let street = match pl.kind {
            Kind::Town => 4.0,
            Kind::Village => 3.0,
            // camps: concrete all over
            Kind::Base => 1000.0,
        };
        let (a, b) = (pl.min(), pl.max());
        for j in 0..m.h {
            for i in 0..m.w {
                let p = m.point(i, j);
                if p.x < a.x || p.x > b.x || p.z < a.y || p.z > b.y {
                    continue;
                }
                let on = (p.x - pl.x).abs() < street || (p.z - pl.z).abs() < street;
                if on {
                    m.paint[j * m.w + i] = if pl.kind != Kind::Village {
                        paint::PAVED
                    } else {
                        paint::ROAD
                    };
                }
            }
        }
    }
    m
}

/// Lowest ground under a footprint (so props never float on a slope).
fn base(m: &Heightmap, x: f32, z: f32, r: f32) -> f32 {
    [(-r, -r), (r, -r), (-r, r), (r, r), (0.0, 0.0)]
        .iter()
        .map(|(dx, dz)| m.height(x + dx, z + dz))
        .fold(f32::MAX, f32::min)
}

/// A leafy tree of size `s`.
fn tree(b: &mut MapBuilder, x: f32, z: f32, s: f32, leaves: [u8; 3]) {
    b.cbox(x, z, 0.45 * s, 0.45 * s, -0.4, 2.8 * s + 0.4, pal::TRUNK);
    b.push_detail(
        x - 1.7 * s,
        2.2 * s,
        z - 1.7 * s,
        x + 1.7 * s,
        4.4 * s,
        z + 1.7 * s,
        leaves,
    );
    b.push_detail(
        x - 1.1 * s,
        4.4 * s,
        z - 1.1 * s,
        x + 1.1 * s,
        5.3 * s,
        z + 1.1 * s,
        shade(leaves, 1.08),
    );
}

/// A tall pine of size `s`.
fn pine(b: &mut MapBuilder, x: f32, z: f32, s: f32) {
    let c = [60, 128, 84];
    b.cbox(x, z, 0.4 * s, 0.4 * s, -0.4, 3.0 * s + 0.4, pal::TRUNK);
    for (k, (w, y0, y1)) in [(3.0, 1.6, 3.4), (2.2, 3.4, 5.0), (1.3, 5.0, 6.6)]
        .into_iter()
        .enumerate()
    {
        let w = w * s * 0.5;
        b.push_detail(
            x - w,
            y0 * s,
            z - w,
            x + w,
            y1 * s,
            z + w,
            shade(c, 1.0 + k as f32 * 0.07),
        );
    }
}

/// A one-storey cottage with a stepped roof.
#[allow(clippy::too_many_arguments)]
fn cottage(
    b: &mut MapBuilder,
    x0: f32,
    z0: f32,
    x1: f32,
    z1: f32,
    front: Side,
    wall: [u8; 3],
    roof: [u8; 3],
) {
    let (cx, cz) = ((x0 + x1) / 2.0, (z0 + z1) / 2.0);
    let at = |s: Side| match s {
        Side::MinZ | Side::MaxZ => cx,
        _ => cz,
    };
    let back = match front {
        Side::MinX => Side::MaxX,
        Side::MaxX => Side::MinX,
        Side::MinZ => Side::MaxZ,
        Side::MaxZ => Side::MinZ,
    };
    let sides: [Side; 2] = match front {
        Side::MinZ | Side::MaxZ => [Side::MinX, Side::MaxX],
        _ => [Side::MinZ, Side::MaxZ],
    };
    let ops = [
        (front, door(at(front) - 1.2)),
        (front, window(at(front) + 1.8)),
        (back, window(at(back))),
        (sides[0], window(at(sides[0]))),
        (sides[1], door(at(sides[1]) + 1.0)),
    ];
    let h = 3.1;
    b.building(x0, z0, x1, z1, 0.0, h, &ops, wall, Some(roof));
    b.loot.push(vec3(cx, 0.3, cz));
    // stepped "pitched" roof
    let (w, d) = (x1 - x0, z1 - z0);
    let (ix, iz) = if w > d {
        (1.2, d * 0.25)
    } else {
        (w * 0.25, 1.2)
    };
    b.block(
        x0 + ix,
        h + 0.35,
        z0 + iz,
        x1 - ix,
        h + 0.95,
        z1 - iz,
        shade(roof, 0.92),
    );
}

/// A two- or three-storey house with stairs inside (and maybe a walkable roof).
#[allow(clippy::too_many_arguments)]
fn house(
    b: &mut MapBuilder,
    x0: f32,
    z0: f32,
    x1: f32,
    z1: f32,
    floors: u32,
    wall: [u8; 3],
    roof: [u8; 3],
    roof_access: bool,
) {
    let cx = (x0 + x1) / 2.0;
    let cz = (z0 + z1) / 2.0;
    let ground = [
        (Side::MaxZ, door(cx + 0.6)),
        (Side::MinZ, door(cx + 0.6)),
        (Side::MaxX, window(cz)),
        (Side::MinZ, window(x1 - 1.6)),
    ];
    let upper = [
        (Side::MaxZ, window(cx)),
        (Side::MinZ, window(cx)),
        (Side::MinX, window(cz)),
        (Side::MaxX, window(cz)),
    ];
    b.tower(&Tower {
        x0,
        z0,
        x1,
        z1,
        floors,
        storey: 3.4,
        color: wall,
        roof,
        ground: &ground,
        upper: &upper,
        roof_access,
    });
    // something to find on every floor (and on the roof)
    let levels = floors + roof_access as u32;
    for k in 0..levels {
        b.loot.push(vec3(cx + 0.8, k as f32 * 3.4 + 0.4, cz));
    }
}

/// A barn: tall, wide doors at both ends, hay inside.
fn barn(b: &mut MapBuilder, x0: f32, z0: f32, x1: f32, z1: f32) {
    let cx = (x0 + x1) / 2.0;
    let ops = [
        (Side::MinZ, crate::map::wide_door(cx)),
        (Side::MaxZ, crate::map::wide_door(cx)),
    ];
    let c = [168, 70, 56];
    b.building(x0, z0, x1, z1, 0.0, 5.0, &ops, c, Some(pal::ROOF_GREY));
    b.loot.push(vec3(cx, 0.3, (z0 + z1) / 2.0));
    b.block(
        x0 + 1.0,
        5.35,
        z0 - 0.2,
        x1 - 1.0,
        6.2,
        z1 + 0.2,
        shade(pal::ROOF_GREY, 0.9),
    );
    for (dx, dz) in [(0.6, 0.25), (0.75, 0.6)] {
        let x = x0 + (x1 - x0) * dx;
        let z = z0 + (z1 - z0) * dz;
        b.cbox(x, z, 1.4, 1.4, 0.0, 1.3, [222, 196, 104]);
    }
}

const WALLS: [[u8; 3]; 7] = [
    pal::WALL_TAN,
    pal::WALL_PINK,
    pal::WALL_BLUE,
    pal::WALL_WHITE,
    pal::WALL_GREEN,
    [236, 214, 140],
    [214, 196, 226],
];
const ROOFS: [[u8; 3]; 4] = [pal::ROOF_RED, pal::ROOF_GREY, [150, 84, 60], [90, 110, 140]];

/// A watchtower: four legs, a railed platform and a ladder up the south side.
fn watchtower(b: &mut MapBuilder, x: f32, z: f32) {
    let top = 5.2;
    let leg = [96, 84, 66];
    for (dx, dz) in [(-1.3, -1.3), (1.3, -1.3), (-1.3, 1.3), (1.3, 1.3)] {
        b.cbox(x + dx, z + dz, 0.3, 0.3, 0.0, top, leg);
    }
    b.block(
        x - 1.6,
        top,
        z - 1.6,
        x + 1.6,
        top + 0.25,
        z + 1.6,
        [120, 100, 74],
    );
    // planks bullets go through, low enough to shoot over
    let rail = [140, 112, 80];
    let (y0, y1) = (top + 0.25, top + 1.2);
    b.wood(x - 1.6, y0, z - 1.6, x + 1.6, y1, z - 1.45, rail);
    b.wood(x - 1.6, y0, z - 1.6, x - 1.45, y1, z + 1.6, rail);
    b.wood(x + 1.45, y0, z - 1.6, x + 1.6, y1, z + 1.6, rail);
    // the ladder side is open
    b.wood(x - 1.6, y0, z + 1.45, x - 0.5, y1, z + 1.6, rail);
    b.wood(x + 0.5, y0, z + 1.45, x + 1.6, y1, z + 1.6, rail);
    b.block(
        x - 1.7,
        top + 2.4,
        z - 1.7,
        x + 1.7,
        top + 2.6,
        z + 1.7,
        pal::ROOF_GREY,
    );
    for (dx, dz) in [(-1.5, -1.5), (1.5, -1.5), (-1.5, 1.5), (1.5, 1.5)] {
        b.cbox(x + dx, z + dz, 0.12, 0.12, top + 1.2, 1.2, leg);
    }
    b.ladder(vec3(x, 0.0, z + 1.6), vec3(0.0, 0.0, 1.0), top + 0.25);
    b.loot.push(vec3(x, top + 0.6, z));
}

/// An army camp: a fence with four gates, barracks, rows of containers, sandbags and a
/// watchtower in every corner.
fn camp(b: &mut MapBuilder, m: &Heightmap, pl: &Place, rng: &mut Rng) {
    let y = m.height(pl.x, pl.z);
    let (a, bb) = (pl.min() + vec2(2.0, 2.0), pl.max() - vec2(2.0, 2.0));
    let fence = [150, 150, 140];
    let gate = 5.0;
    b.lift(y, |b| {
        // the fence, broken by a gate in the middle of each side
        let (h, t) = (2.4, 0.3);
        b.block(a.x, 0.0, a.y, pl.x - gate, h, a.y + t, fence);
        b.block(pl.x + gate, 0.0, a.y, bb.x, h, a.y + t, fence);
        b.block(a.x, 0.0, bb.y - t, pl.x - gate, h, bb.y, fence);
        b.block(pl.x + gate, 0.0, bb.y - t, bb.x, h, bb.y, fence);
        b.block(a.x, 0.0, a.y, a.x + t, h, pl.z - gate, fence);
        b.block(a.x, 0.0, pl.z + gate, a.x + t, h, bb.y, fence);
        b.block(bb.x - t, 0.0, a.y, bb.x, h, pl.z - gate, fence);
        b.block(bb.x - t, 0.0, pl.z + gate, bb.x, h, bb.y, fence);
        // watchtowers in the corners
        for (x, z) in [
            (a.x + 4.0, a.y + 4.0),
            (bb.x - 4.0, a.y + 4.0),
            (a.x + 4.0, bb.y - 6.0),
            (bb.x - 4.0, bb.y - 6.0),
        ] {
            watchtower(b, x, z);
        }
        // two barracks on the north side, with roofs to hold
        let wall = [176, 168, 128];
        house(
            b,
            pl.x - 30.0,
            a.y + 9.0,
            pl.x - 12.0,
            a.y + 20.0,
            2,
            wall,
            pal::ROOF_GREY,
            true,
        );
        house(
            b,
            pl.x + 12.0,
            a.y + 9.0,
            pl.x + 30.0,
            a.y + 20.0,
            2,
            wall,
            pal::ROOF_GREY,
            true,
        );
        // containers in rows, some stacked
        let colors = [[176, 72, 56], [60, 110, 150], [92, 120, 60], [210, 160, 60]];
        for row in 0..2 {
            let z = pl.z + 10.0 + row as f32 * 8.0;
            for k in 0..4 {
                let x = pl.x - 30.0 + k as f32 * 9.0 + if row == 1 { 22.0 } else { 0.0 };
                if (x - pl.x).abs() < 5.0 {
                    continue; // the way from the south gate stays open
                }
                let c = colors[rng.below(colors.len())];
                b.container(x, z, true, 0.0, c);
                if rng.chance(0.35) {
                    b.container(x, z, true, 2.6, colors[rng.below(colors.len())]);
                }
                b.loot.push(vec3(x + 3.6, 0.3, z));
            }
        }
        // sandbag nests by the gates and a fuel tank
        b.sandbags(pl.x - 9.0, a.y + 3.0, pl.x - 6.0, a.y + 3.6);
        b.sandbags(pl.x + 6.0, bb.y - 3.6, pl.x + 9.0, bb.y - 3.0);
        b.sandbags(a.x + 3.0, pl.z + 6.0, a.x + 3.6, pl.z + 9.0);
        b.sandbags(bb.x - 3.6, pl.z - 9.0, bb.x - 3.0, pl.z - 6.0);
        b.tank(bb.x - 10.0, pl.z - 2.0);
        for k in 0..6 {
            let x = pl.x + rng.range(-0.4, 0.4) * pl.w;
            let z = pl.z + rng.range(-0.15, 0.1) * pl.d;
            b.crate_(x, z, 1.1, 0.0);
            if k % 2 == 0 {
                b.loot.push(vec3(x + 1.2, 0.3, z));
            }
        }
    });
}

/// Fills a settlement: houses around the crossing streets.
fn settle(b: &mut MapBuilder, m: &Heightmap, pl: &Place, rng: &mut Rng) {
    if pl.kind == Kind::Base {
        camp(b, m, pl, rng);
        return;
    }
    let y = m.height(pl.x, pl.z);
    let street = if pl.kind == Kind::Town { 4.0 } else { 3.0 };
    let (a, bb) = (pl.min(), pl.max());
    b.lift(y, |b| {
        // four quarters around the crossroads
        for (qx, qz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let (x0, x1) = if qx < 0.0 {
                (a.x + 2.0, pl.x - street - 2.0)
            } else {
                (pl.x + street + 2.0, bb.x - 2.0)
            };
            let (z0, z1) = if qz < 0.0 {
                (a.y + 2.0, pl.z - street - 2.0)
            } else {
                (pl.z + street + 2.0, bb.y - 2.0)
            };
            let lot_w = if pl.kind == Kind::Town { 15.0 } else { 14.0 };
            let lot_d = if pl.kind == Kind::Town { 14.0 } else { 13.0 };
            let nx = ((x1 - x0) / lot_w).floor().max(1.0) as i32;
            let nz = ((z1 - z0) / lot_d).floor().max(1.0) as i32;
            for iz in 0..nz {
                for ix in 0..nx {
                    // villages leave gaps between the houses (gardens)
                    if pl.kind == Kind::Village && rng.chance(0.35) {
                        continue;
                    }
                    if pl.kind == Kind::Town && rng.chance(0.08) {
                        continue;
                    }
                    let lx = x0 + ix as f32 * lot_w;
                    let lz = z0 + iz as f32 * lot_d;
                    let w = lot_w - rng.range(3.0, 5.0);
                    let d = lot_d - rng.range(2.6, 3.4);
                    let ox = lx + rng.range(0.0, lot_w - w);
                    let oz = lz + rng.range(0.0, lot_d - d);
                    let wall = WALLS[rng.below(WALLS.len())];
                    let roof = ROOFS[rng.below(ROOFS.len())];
                    let r = rng.f32();
                    if pl.kind == Kind::Town && r < 0.55 && d >= 10.0 {
                        let floors = if rng.chance(0.35) { 3 } else { 2 };
                        house(
                            b,
                            ox,
                            oz,
                            ox + w,
                            oz + d,
                            floors,
                            wall,
                            roof,
                            rng.chance(0.6),
                        );
                    } else if pl.kind == Kind::Village && r < 0.18 && d >= 10.0 {
                        house(b, ox, oz, ox + w, oz + d, 2, wall, roof, false);
                    } else if pl.kind == Kind::Village && r < 0.32 {
                        barn(b, ox, oz, ox + w, oz + d);
                    } else {
                        let front = if qz < 0.0 { Side::MaxZ } else { Side::MinZ };
                        cottage(b, ox, oz, ox + w, oz + d, front, wall, roof);
                    }
                }
            }
        }
        // street clutter: cars, crates, a well in villages
        let n = if pl.kind == Kind::Town { 7 } else { 2 };
        for k in 0..n {
            let along_x = k % 2 == 0;
            let t = rng.range(-0.4, 0.4);
            let (cx, cz) = if along_x {
                (
                    pl.x + t * pl.w,
                    pl.z + street * 0.55 * if rng.chance(0.5) { 1.0 } else { -1.0 },
                )
            } else {
                (
                    pl.x + street * 0.55 * if rng.chance(0.5) { 1.0 } else { -1.0 },
                    pl.z + t * pl.d,
                )
            };
            if (cx - pl.x).abs() < 8.0 && (cz - pl.z).abs() < 8.0 {
                continue; // keep the crossroads clear
            }
            let cars = [pal::CAR_A, pal::CAR_B, pal::CAR_C, pal::WALL_BLUE];
            b.car(cx, cz, along_x, cars[rng.below(cars.len())]);
        }
        if pl.kind == Kind::Village {
            b.cbox(
                pl.x + street + 3.0,
                pl.z + street + 3.0,
                1.8,
                1.8,
                0.0,
                0.9,
                pal::CONCRETE_DARK,
            );
        } else {
            // a fountain square in the middle of the crossroads
            b.cbox(pl.x, pl.z, 5.0, 5.0, 0.0, 0.7, pal::CONCRETE);
            b.cbox(pl.x, pl.z, 1.0, 1.0, 0.7, 1.6, pal::CONCRETE_DARK);
        }
        // a few things lying about in the streets
        for k in 0..3 {
            let a = k as f32 * 2.1 + rng.range(0.0, 1.0);
            b.loot
                .push(vec3(pl.x + a.cos() * 10.0, 0.3, pl.z + a.sin() * 10.0));
        }
    });
}

fn bridge(b: &mut MapBuilder, br: &Bridge) {
    let (hx, hz) = if br.along_x {
        (br.half, 2.6)
    } else {
        (2.6, br.half)
    };
    let (x0, x1, z0, z1) = (br.c.x - hx, br.c.x + hx, br.c.y - hz, br.c.y + hz);
    let wood = [150, 104, 64];
    b.block(x0, DECK - 0.3, z0, x1, DECK, z1, wood);
    // railings: planks bullets go through
    if br.along_x {
        b.wood(x0, DECK, z0, x1, DECK + 1.0, z0 + 0.2, shade(wood, 0.85));
        b.wood(x0, DECK, z1 - 0.2, x1, DECK + 1.0, z1, shade(wood, 0.85));
    } else {
        b.wood(x0, DECK, z0, x0 + 0.2, DECK + 1.0, z1, shade(wood, 0.85));
        b.wood(x1 - 0.2, DECK, z0, x1, DECK + 1.0, z1, shade(wood, 0.85));
    }
    // piers in the water
    for k in [-0.5f32, 0.0, 0.5] {
        let (px, pz) = if br.along_x {
            (br.c.x + k * br.half, br.c.y)
        } else {
            (br.c.x, br.c.y + k * br.half)
        };
        b.cbox(px, pz, 1.0, 1.0, -2.0, DECK + 1.5, pal::CONCRETE_DARK);
    }
}

fn spawn_at(m: &Heightmap, x: f32, z: f32, yaw: f32) -> Spawn {
    Spawn {
        pos: vec3(x, m.height(x, z) + 0.05, z),
        yaw,
    }
}

pub fn island() -> Map {
    let m = heightmap();
    let mut b = MapBuilder::new();
    let mut rng = Rng::new(SEED as u64);
    let brs = bridges();

    for pl in PLACES {
        settle(&mut b, &m, &pl, &mut rng);
    }
    for br in &brs {
        bridge(&mut b, br);
    }

    // haystacks on the fields
    for &(x0, z0, x1, z1, _) in &FIELDS {
        for _ in 0..4 {
            let x = rng.range(x0 + 4.0, x1 - 4.0);
            let z = rng.range(z0 + 4.0, z1 - 4.0);
            let y = base(&m, x, z, 0.8);
            if y > WATER + 0.3 {
                b.cbox(x, z, 1.6, 1.6, y - 0.3, 1.7, [226, 198, 110]);
            }
        }
    }

    // forests, lone trees, mountain rocks
    let clear = |x: f32, z: f32| -> bool {
        let q = vec2(x, z);
        PLACES.iter().all(|p| p.dist(x, z) > 5.0)
            && ROADS.iter().all(|r| line_dist(q, r) > 6.0)
            && FIELDS
                .iter()
                .all(|f| x < f.0 - 2.0 || x > f.2 + 2.0 || z < f.1 - 2.0 || z > f.3 + 2.0)
            && brs.iter().all(|br| (br.c - q).length() > br.half + 6.0)
    };
    let step = 7.0;
    let n = (2.0 * HALF / step) as i32;
    for jz in 0..n {
        for ix in 0..n {
            let x = -HALF + (ix as f32 + rng.range(0.15, 0.85)) * step;
            let z = -HALF + (jz as f32 + rng.range(0.15, 0.85)) * step;
            if x.abs() > HALF - 8.0 || z.abs() > HALF - 8.0 {
                continue;
            }
            let h = m.height(x, z);
            if h < WATER + 1.0 || m.slope(x, z) > 0.9 || !clear(x, z) {
                continue;
            }
            let forest = fbm(SEED + 7, x / 130.0, z / 130.0, 3);
            let p = if forest > 0.56 { 0.6 } else { 0.01 };
            if !rng.chance(p) {
                if h > 30.0 && m.slope(x, z) > 0.45 && rng.chance(0.05) {
                    let s = rng.range(1.0, 2.6);
                    b.cbox(
                        x,
                        z,
                        s * 1.4,
                        s * 1.1,
                        base(&m, x, z, s) - 0.6,
                        s + 0.6,
                        shade(pal::CONCRETE_DARK, rng.range(0.75, 0.95)),
                    );
                }
                continue;
            }
            let s = rng.range(0.8, 1.3);
            let y = base(&m, x, z, 0.3);
            b.lift(y, |b| {
                if h > 26.0 || rng.chance(0.25) {
                    pine(b, x, z, s);
                } else {
                    let g = rng.range(0.85, 1.12);
                    tree(b, x, z, s, shade(pal::LEAVES, g));
                }
            });
        }
    }

    // nobody leaves the island
    let (lo, hi) = (-HALF, HALF);
    let t = 1.0;
    b.clip(lo - t, -20.0, lo - t, hi + t, 300.0, lo);
    b.clip(lo - t, -20.0, hi, hi + t, 300.0, hi + t);
    b.clip(lo - t, -20.0, lo, lo, 300.0, hi);
    b.clip(hi, -20.0, lo, hi + t, 300.0, hi);

    let w = &PLACES[2];
    let e = &PLACES[5];
    let allies = (0..9)
        .map(|k| {
            spawn_at(
                &m,
                w.x + (k % 3) as f32 * 6.0 - 6.0,
                w.z + (k / 3) as f32 * 6.0 - 6.0 + w.d * 0.5 + 6.0,
                -PI / 2.0,
            )
        })
        .collect();
    let axis = (0..9)
        .map(|k| {
            spawn_at(
                &m,
                e.x + (k % 3) as f32 * 6.0 - 6.0,
                e.z + (k / 3) as f32 * 6.0 - 6.0 + e.d * 0.5 + 6.0,
                PI / 2.0,
            )
        })
        .collect();
    let at = |x: f32, z: f32| vec3(x, m.height(x, z), z);
    let centre = &PLACES[0];

    // jeeps by every town and village and in the camps, boats on the lake and the river
    let free = |x: f32, z: f32, y: f32| {
        b.boxes.iter().all(|bx| {
            !bx.solid
                || bx.max.x < x - 2.6
                || bx.min.x > x + 2.6
                || bx.max.z < z - 2.6
                || bx.min.z > z + 2.6
                || bx.max.y < y + 0.3
                || bx.min.y > y + 2.0
        })
    };
    let mut car_spots = Vec::new();
    for pl in PLACES {
        let spots: Vec<(f32, f32)> = match pl.kind {
            Kind::Base => vec![
                (pl.x - 18.0, pl.max().y - 12.0),
                (pl.x + 18.0, pl.max().y - 12.0),
            ],
            _ => vec![(pl.max().x + 7.0, pl.z + 1.5)],
        };
        for (x, z) in spots {
            // a little further out until it stands clear of trees and walls
            let mut x = x;
            for _ in 0..6 {
                if free(x, z, m.height(x, z)) {
                    break;
                }
                x += 3.0;
            }
            if free(x, z, m.height(x, z)) && m.height(x, z) > WATER + 0.5 {
                car_spots.push(crate::cars::CarSpot {
                    pos: at(x, z),
                    yaw: crate::yaw_of(vec3(1.0, 0.0, 0.0)),
                    kind: crate::cars::CarKind::Jeep,
                });
            }
        }
    }
    let boats = [
        (LAKE.0 + 20.0, LAKE.1, RIVER[1]),
        (RIVER[2].0, RIVER[2].1, RIVER[3]),
        (RIVER[5].0, RIVER[5].1, RIVER[6]),
        (RIVER[7].0, RIVER[7].1, RIVER[8]),
    ];
    for (x, z, towards) in boats {
        if m.height(x, z) < WATER - 0.6 {
            car_spots.push(crate::cars::CarSpot {
                pos: vec3(x, WATER - 0.15, z),
                yaw: crate::yaw_of(vec3(towards.0 - x, 0.0, towards.1 - z)),
                kind: crate::cars::CarKind::Boat,
            });
        }
    }

    Map {
        id: MapId::Island,
        name: "Острів",
        min: vec2(-HALF, -HALF),
        max: vec2(HALF, HALF),
        boxes: b.boxes,
        ladders: b.ladders,
        spawns: [allies, axis],
        bomb: at(centre.x + 10.0, centre.z + 10.0),
        sites: [at(-400.0, 24.0), at(400.0, -40.0)],
        sky: [150, 196, 238],
        fog: [196, 214, 226],
        tint: [1.0, 1.0, 1.0],
        terrain: Some(Arc::new(m)),
        loot: b.loot,
        // the towns and villages (a few metres round them for the doors and yards)
        nav_detail: PLACES
            .iter()
            .map(|p| {
                let (a, b) = (p.min(), p.max());
                [a.x - 4.0, a.y - 4.0, b.x + 4.0, b.y + 4.0]
            })
            .collect(),
        car_spots,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::terrain::MAX_WALK_SLOPE;

    #[test]
    fn island_is_playable() {
        let t = std::time::Instant::now();
        let map = island();
        eprintln!("island: {} boxes in {:?}", map.boxes.len(), t.elapsed());
        let m = map.terrain.as_ref().unwrap();
        for s in map.all_spawns() {
            let g = m.height(s.pos.x, s.pos.z);
            assert!(g > WATER + 0.5, "spawn in the water {}", s.pos);
            assert!(
                m.slope(s.pos.x, s.pos.z) < MAX_WALK_SLOPE,
                "spawn on a cliff {}",
                s.pos
            );
        }
        // settlements are dry and level
        for pl in PLACES {
            let y = m.height(pl.x, pl.z);
            assert!(y > WATER + 1.5, "flooded settlement at {} {}", pl.x, pl.z);
            assert!((m.height(pl.min().x + 1.0, pl.min().y + 1.0) - y).abs() < 0.01);
        }
        // the river really is water, and the mountains are high
        assert!(m.height(120.0, 0.0) < WATER);
        assert!(
            m.height(PEAK.0, PEAK.1) > 30.0,
            "{}",
            m.height(PEAK.0, PEAK.1)
        );
        assert!(!bridges().is_empty());
        assert!(map.boxes.len() < 70_000, "{}", map.boxes.len());
        let jeeps = map
            .car_spots
            .iter()
            .filter(|c| c.kind == crate::cars::CarKind::Jeep)
            .count();
        let boats = map.car_spots.len() - jeeps;
        assert!(jeeps >= 14 && boats >= 3, "{jeeps} jeeps, {boats} boats");
    }

    #[test]
    fn bridge_decks_meet_the_road() {
        let map = island();
        let m = map.terrain.as_ref().unwrap();
        for br in bridges() {
            let dir = if br.along_x {
                vec2(1.0, 0.0)
            } else {
                vec2(0.0, 1.0)
            };
            for s in [-1.0, 1.0] {
                let end = br.c + dir * s * (br.half + 0.5);
                let g = m.height(end.x, end.y);
                assert!(
                    (g - DECK).abs() < 0.45,
                    "{br:?}: ground {g} at the deck's end"
                );
            }
        }
    }
}

#[cfg(test)]
mod swim_tests {
    use super::*;
    use crate::physics::{Body, MoveInput, Stance, World, SWIM_SINK};
    use crate::tuning::MoveTuning;

    /// Jumping into the deep part of the lake: you float at the surface, paddle slowly
    /// towards the shore and walk out on your feet.
    #[test]
    fn swim_across_the_lake_and_walk_out() {
        let map = island();
        let world = World::from_map(&map);
        let t = MoveTuning::default();
        let (cx, cz) = (LAKE.0, LAKE.1);
        let tr = world.terrain.as_ref().unwrap();
        assert!(tr.height(cx, cz) < WATER - 2.5, "the lake is deep in the middle");
        let mut b = Body {
            pos: vec3(cx, WATER + 3.0, cz),
            stance: Stance::Stand,
            ..Default::default()
        };
        let dt = 1.0 / 60.0;
        let idle = MoveInput::default();
        for _ in 0..180 {
            b.step(&world, &idle, &t, dt);
        }
        assert!(b.swimming(&world), "afloat at {}", b.pos);
        assert!((b.pos.y - (WATER - SWIM_SINK)).abs() < 0.05, "at the surface {}", b.pos);
        // paddle east (yaw -pi/2 looks along +x) for a while: slower than walking
        let go = MoveInput {
            forward: 1.0,
            sprint: true,
            yaw: -std::f32::consts::FRAC_PI_2,
            want_stance: Stance::Stand,
            ..Default::default()
        };
        let x0 = b.pos.x;
        for _ in 0..120 {
            b.step(&world, &go, &t, dt);
        }
        let speed = (b.pos.x - x0) / 2.0;
        assert!(speed > 1.0 && speed < t.walk_speed * 0.6, "swim speed {speed}");
        // keep going to the shore
        let mut out = false;
        for _ in 0..60 * 120 {
            b.step(&world, &go, &t, dt);
            if !b.swimming(&world) && tr.height(b.pos.x, b.pos.z) > WATER + 0.2 {
                out = true;
                break;
            }
        }
        assert!(out, "never got out of the water: {}", b.pos);
        assert!(b.pos.y >= tr.height(b.pos.x, b.pos.z) - 0.01);
    }
}
