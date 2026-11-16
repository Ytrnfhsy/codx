//! Dressing a map's two halves differently but fairly.
//!
//! A map's skeleton (buildings, stairs, ladders, bridges, the Sabotage yards) is the same
//! for both teams, turned half round. Everything else, the cover lying about, is given as
//! *slots*: a place and how high the cover there is (low, chest high, tall, a stack you
//! can stand on). Each half fills every slot with a prop of that height picked at random
//! from the map's own set, shifted a little and turned either way. So both teams find the
//! same amount of cover of each height in the same parts of the map, but it is not the
//! same cover and it does not stand in the same pattern.
//!
//! Props are placed only where they keep clear of walls (doorways stay usable), of other
//! props, of ladders, spawns, the bomb and the Sabotage pads.

use crate::map::{MapBox, MapBuilder};
use crate::rng::Rng;
use glam::{vec2, vec3, Vec2, Vec3};

/// How high the cover in a slot is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cover {
    /// Waist high (about 1 m): crouch behind it, shoot over it standing.
    Low,
    /// Chest high (about 1.5 m): stand behind it.
    Chest,
    /// Taller than a man (2.3 to 3 m): blocks sight.
    Tall,
    /// Two containers high or a stack to climb.
    Stack,
}

impl Cover {
    pub const ALL: [Cover; 4] = [Cover::Low, Cover::Chest, Cover::Tall, Cover::Stack];
}

/// Where a prop goes (in the allies' half: the axis half gets it turned round).
#[derive(Clone, Copy, Debug)]
pub struct Slot {
    pub x: f32,
    pub z: f32,
    pub cover: Cover,
}

pub const fn slot(x: f32, z: f32, cover: Cover) -> Slot {
    Slot { x, z, cover }
}

/// Builds a prop centred on (x, z) (along x or along z).
pub type Prop = fn(&mut MapBuilder, f32, f32, bool, &mut Rng);

/// What a map dresses itself with, per height of cover, and its litter (things that are
/// only painted on the ground or too low to matter).
pub struct Set {
    pub low: &'static [Prop],
    pub chest: &'static [Prop],
    pub tall: &'static [Prop],
    pub stack: &'static [Prop],
    pub litter: &'static [Prop],
}

impl Set {
    fn of(&self, c: Cover) -> &'static [Prop] {
        match c {
            Cover::Low => self.low,
            Cover::Chest => self.chest,
            Cover::Tall => self.tall,
            Cover::Stack => self.stack,
        }
    }
}

/// Places that must stay free: (centre, radius).
pub type Keep = Vec<(Vec2, f32)>;

/// Everything that must stay walkable on a map: spawns, the bomb, the pads, the feet of
/// the ladders.
pub fn keep_clear(b: &MapBuilder, spawns: &[Vec3], bomb: Vec3, sites: &[Vec3]) -> Keep {
    let mut k: Keep = spawns.iter().map(|p| (vec2(p.x, p.z), 1.6)).collect();
    k.push((vec2(bomb.x, bomb.z), 3.0));
    for s in sites {
        k.push((vec2(s.x, s.z), 4.2));
    }
    for l in &b.ladders {
        let f = l.base + l.out * 0.6;
        k.push((vec2(f.x, f.z), 2.0));
    }
    k
}

fn solid_boxes(b: &[MapBox]) -> impl Iterator<Item = &MapBox> {
    b.iter()
        .filter(|x| x.solid && x.min.y > -0.5 && x.max.y > 0.12)
}

/// Whether boxes `new` can stand where they are: off the walls (a gap a player fits
/// through), off the other props, inside the map and off the places to keep free.
fn fits(old: &[MapBox], new: &[MapBox], keep: &Keep, min: Vec2, max: Vec2) -> bool {
    for n in solid_boxes(new) {
        if n.min.x < min.x + 1.2
            || n.min.z < min.y + 1.2
            || n.max.x > max.x - 1.2
            || n.max.z > max.y - 1.2
        {
            return false;
        }
        for (c, r) in keep {
            let q = vec2(c.x.clamp(n.min.x, n.max.x), c.y.clamp(n.min.z, n.max.z));
            if q.distance(*c) < *r {
                return false;
            }
        }
        for o in solid_boxes(old) {
            if o.max.y < n.min.y || o.min.y > n.max.y + 0.5 {
                continue;
            }
            // a body (0.7 m) and a bit gets between a prop and anything that stands up
            let gap = if o.max.y - o.min.y > 0.6 { 1.1 } else { 0.5 };
            if n.max.x + gap > o.min.x
                && n.min.x - gap < o.max.x
                && n.max.z + gap > o.min.z
                && n.min.z - gap < o.max.z
            {
                return false;
            }
        }
    }
    true
}

/// What one half got: props per height of cover (Low, Chest, Tall, Stack).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Dressed(pub [usize; 4]);

/// One prop as picked for a slot: which of its height's set, its own random numbers.
#[derive(Clone, Copy)]
struct Pick {
    prop: usize,
    seed: u64,
}

/// Tries to put `pick` near (x, z): up to `tries` spots, the first anywhere within
/// `spread`, the later ones closer in.
#[allow(clippy::too_many_arguments)]
fn place(
    b: &mut MapBuilder,
    props: &[Prop],
    pick: Pick,
    x: f32,
    z: f32,
    rng: &mut Rng,
    keep: &Keep,
    min: Vec2,
    max: Vec2,
    spread: f32,
) -> bool {
    for attempt in 0..8 {
        let r = spread * (1.0 - attempt as f32 / 8.0);
        let (dx, dz) = (rng.range(-r, r), rng.range(-r, r));
        let along_x = rng.chance(0.5);
        let mut t = MapBuilder::new();
        prop_with(props[pick.prop], &mut t, x + dx, z + dz, along_x, pick.seed);
        if fits(&b.boxes, &t.boxes, keep, min, max) {
            b.boxes.extend(t.boxes);
            return true;
        }
    }
    false
}

fn prop_with(f: Prop, b: &mut MapBuilder, x: f32, z: f32, along_x: bool, seed: u64) {
    let mut r = Rng::new(seed);
    f(b, x, z, along_x, &mut r);
}

/// Fills `slots` on both halves. The allies' half (as given) gets a prop of the slot's
/// height picked at random for each slot; the axis half (turned round) gets the very
/// same props, but shuffled among the slots of each height, each shifted and turned its
/// own way. So both teams have the same things to hide behind, standing in other places
/// and other patterns. Returns what each half got.
#[allow(clippy::too_many_arguments)]
pub fn dress(
    b: &mut MapBuilder,
    slots: &[Slot],
    set: &Set,
    seed: u64,
    keep: &Keep,
    min: Vec2,
    max: Vec2,
    spread: f32,
) -> [Dressed; 2] {
    let mut rng = Rng::new(seed);
    let mut got = [Dressed::default(); 2];
    for (ci, cover) in Cover::ALL.iter().enumerate() {
        let props = set.of(*cover);
        // small things wander further from their slots than containers and cabins
        let spread = match cover {
            Cover::Low | Cover::Chest => spread * 1.6,
            _ => spread,
        };
        let mine: Vec<&Slot> = slots.iter().filter(|s| s.cover == *cover).collect();
        // the allies' half: a pick per slot (another one if it does not fit there)
        let mut picks = Vec::new();
        for s in &mine {
            let first = rng.below(props.len());
            for k in 0..props.len() {
                let pick = Pick {
                    prop: (first + k) % props.len(),
                    seed: rng.next_u64(),
                };
                if place(b, props, pick, s.x, s.z, &mut rng, keep, min, max, spread) {
                    picks.push(pick);
                    got[0].0[ci] += 1;
                    break;
                }
            }
        }
        // the axis half: the same picks, shuffled over the slots
        // (one cycle through all of them: no prop comes back to its own slot)
        let mut order: Vec<usize> = (0..picks.len()).collect();
        for i in (1..order.len()).rev() {
            order.swap(i, rng.below(i));
        }
        let mut left: Vec<Pick> = order.iter().map(|&i| picks[i]).collect();
        let mut empty: Vec<&Slot> = Vec::new();
        for s in &mine {
            // the next pick that fits at this slot
            let found = (0..left.len()).find(|&i| {
                place(
                    b, props, left[i], -s.x, -s.z, &mut rng, keep, min, max, spread,
                )
            });
            match found {
                Some(i) => {
                    left.remove(i);
                    got[1].0[ci] += 1;
                }
                None => empty.push(s),
            }
            if left.is_empty() {
                break;
            }
        }
        // what did not fit goes to the slots left empty, looking further round them, and
        // as a last resort near any slot of its height
        let wide: Vec<(&Slot, f32)> = empty
            .iter()
            .map(|s| (*s, spread * 2.5))
            .chain(mine.iter().map(|s| (*s, spread * 3.5)))
            .collect();
        for pick in left {
            if wide
                .iter()
                .any(|(s, r)| place(b, props, pick, -s.x, -s.z, &mut rng, keep, min, max, *r))
            {
                got[1].0[ci] += 1;
            }
        }
    }
    got
}

/// Scatters `n` bits of litter over one half (z > 0, or z < 0 when `turned`): oil stains,
/// puddles, planks — nothing anyone hides behind.
pub fn litter(
    b: &mut MapBuilder,
    set: &Set,
    n: usize,
    turned: bool,
    rng: &mut Rng,
    min: Vec2,
    max: Vec2,
) {
    if set.litter.is_empty() {
        return;
    }
    for _ in 0..n {
        let x = rng.range(min.x + 2.0, max.x - 2.0);
        let z = rng.range(2.0, max.y - 2.0);
        let z = if turned { -z } else { z };
        let prop = set.litter[rng.below(set.litter.len())];
        let along_x = rng.chance(0.5);
        let mut t = MapBuilder::new();
        prop(&mut t, x, z, along_x, rng);
        // litter is never solid: it cannot block anybody
        for mut bx in t.boxes {
            bx.solid = false;
            b.boxes.push(bx);
        }
    }
}

/// How much cover of each height a half of a map has: the ground area (m²) of solid
/// boxes that reach 0.9 to 1.3 m, 1.3 to 2.2 m and above 2.2 m, with the half's own
/// buildings counted too. For the balance tests.
pub fn cover_area(boxes: &[MapBox], north: bool, min: Vec2, max: Vec2) -> [f32; 3] {
    let mut out = [0.0f32; 3];
    for x in solid_boxes(boxes) {
        if x.min.y > 0.5 {
            continue;
        }
        let c = (x.min + x.max) * 0.5;
        if (c.z < 0.0) != north || c.z.abs() < 0.01 {
            continue;
        }
        // the perimeter walls are the same for everybody
        if x.min.x <= min.x + 0.01
            || x.max.x >= max.x - 0.01
            || x.min.z <= min.y + 0.01
            || x.max.z >= max.y - 0.01
        {
            continue;
        }
        let area = (x.max.x - x.min.x) * (x.max.z - x.min.z);
        let k = match x.max.y {
            h if h < 0.9 => continue,
            h if h < 1.3 => 0,
            h if h < 2.2 => 1,
            _ => 2,
        };
        out[k] += area;
    }
    out
}

impl MapBuilder {
    /// Turns everything built since `mark` half round the map's middle (x, z → −x, −z),
    /// in place.
    pub fn turn_since(&mut self, mark: (usize, usize)) {
        for b in &mut self.boxes[mark.0..] {
            let (lo, hi) = (b.min, b.max);
            b.min = vec3(-hi.x, lo.y, -hi.z);
            b.max = vec3(-lo.x, hi.y, -lo.z);
        }
        for l in &mut self.ladders[mark.1..] {
            l.base = vec3(-l.base.x, l.base.y, -l.base.z);
            l.out = -l.out;
        }
    }
}

/// Props for dressing, sorted by the height of cover they give. Every one keeps to its
/// band (low 0.9–1.3 m, chest 1.3–2.2 m, tall from 2.2 m), so a slot gives the same cover
/// whatever lands in it.
pub mod props {
    use crate::map::{pal, shade, MapBuilder};
    use crate::rng::Rng;

    pub const CONTAINERS: [[u8; 3]; 7] = [
        pal::CONT_RED,
        pal::CONT_BLUE,
        pal::CONT_GREEN,
        pal::CONT_ORANGE,
        pal::CONT_YELLOW,
        [150, 160, 170],
        [120, 70, 110],
    ];

    fn colour(r: &mut Rng) -> [u8; 3] {
        CONTAINERS[r.below(CONTAINERS.len())]
    }

    // ---- low

    pub fn sacks(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        b.sacks(x, z);
        if r.chance(0.6) {
            let (dx, dz) = if ax { (1.35, 0.0) } else { (0.0, 1.35) };
            b.sacks(x + dx, z + dz);
        }
    }

    pub fn barrier(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        b.barrier(x, z, ax);
        if r.chance(0.5) {
            let (dx, dz) = if ax { (2.5, 0.3) } else { (0.3, 2.5) };
            b.barrier(x + dx, z + dz, ax);
        }
    }

    pub fn sandbags(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let l = r.range(2.2, 3.4) / 2.0;
        if ax {
            b.block(x - l, 0.0, z - 0.45, x + l, 1.05, z + 0.45, pal::SANDBAG);
        } else {
            b.block(x - 0.45, 0.0, z - l, x + 0.45, 1.05, z + l, pal::SANDBAG);
        }
        b.push_detail(
            x - 0.3,
            1.05,
            z - 0.3,
            x + 0.3,
            1.12,
            z + 0.3,
            shade(pal::SANDBAG, 0.85),
        );
    }

    pub fn low_crates(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let n = 2 + r.below(2);
        for k in 0..n {
            let s = r.range(0.95, 1.15);
            let o = k as f32 * 1.15 - (n - 1) as f32 * 0.575;
            let (dx, dz) = if ax {
                (o, r.range(-0.15, 0.15))
            } else {
                (r.range(-0.15, 0.15), o)
            };
            b.crate_(x + dx, z + dz, s, 0.0);
        }
    }

    pub fn pallets(b: &mut MapBuilder, x: f32, z: f32, _ax: bool, r: &mut Rng) {
        let n = 6 + r.below(2);
        for k in 0..n {
            let y = k as f32 * 0.16;
            let j = if k % 2 == 0 { 0.0 } else { 0.05 };
            b.cbox(x + j, z - j, 1.2, 1.0, y, 0.12, pal::WOOD);
            b.push_detail(
                x - 0.61 + j,
                y + 0.12,
                z - 0.51 - j,
                x + 0.61 + j,
                y + 0.16,
                z + 0.51 - j,
                shade(pal::WOOD, 0.75),
            );
        }
    }

    pub fn tyres(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let n = 2 + r.below(2);
        for k in 0..n {
            let o = k as f32 * 0.95 - (n - 1) as f32 * 0.475;
            let (dx, dz) = if ax { (o, 0.0) } else { (0.0, o) };
            b.cbox(x + dx, z + dz, 0.9, 0.9, 0.0, 1.0, pal::TYRE);
            b.push_detail(
                x + dx - 0.47,
                0.32,
                z + dz - 0.47,
                x + dx + 0.47,
                0.36,
                z + dz + 0.47,
                pal::DARK,
            );
            b.push_detail(
                x + dx - 0.47,
                0.65,
                z + dz - 0.47,
                x + dx + 0.47,
                0.69,
                z + dz + 0.47,
                pal::DARK,
            );
        }
    }

    pub fn drums(b: &mut MapBuilder, x: f32, z: f32, _ax: bool, r: &mut Rng) {
        b.cbox(x, z, 1.6, 1.6, 0.0, 0.12, pal::WOOD);
        let c = if r.chance(0.5) {
            pal::BARREL_RED
        } else {
            pal::BARREL_BLUE
        };
        for (dx, dz) in [
            (-0.38f32, -0.38f32),
            (0.38, -0.38),
            (-0.38, 0.38),
            (0.38, 0.38),
        ] {
            if r.chance(0.85) {
                b.cbox(x + dx, z + dz, 0.66, 0.66, 0.12, 1.0, c);
                b.push_detail(
                    x + dx - 0.35,
                    0.5,
                    z + dz - 0.35,
                    x + dx + 0.35,
                    0.56,
                    z + dz + 0.35,
                    pal::DARK,
                );
            }
        }
        // (one always stands, so the cover is never empty)
        b.cbox(x - 0.38, z - 0.38, 0.66, 0.66, 0.12, 1.0, c);
    }

    pub fn timber(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let l = r.range(3.6, 4.6);
        let (w, d) = if ax { (l, 1.4) } else { (1.4, l) };
        b.cbox(x, z, w, d, 0.0, 0.6, pal::WOOD);
        b.cbox(x, z, w * 0.85, d * 0.85, 0.6, 0.45, shade(pal::WOOD, 0.88));
    }

    pub fn bricks(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let n = 2 + r.below(2);
        for k in 0..n {
            let o = k as f32 * 1.3 - (n - 1) as f32 * 0.65;
            let (dx, dz) = if ax { (o, 0.0) } else { (0.0, o) };
            b.cbox(x + dx, z + dz, 1.2, 1.0, 0.0, 0.15, pal::WOOD);
            b.cbox(x + dx, z + dz, 1.1, 0.9, 0.15, 0.8, pal::BRICK);
            b.push_detail(
                x + dx - 0.56,
                0.6,
                z + dz - 0.46,
                x + dx + 0.56,
                0.64,
                z + dz + 0.46,
                shade(pal::BRICK, 0.8),
            );
        }
    }

    // ---- chest high

    pub fn reel(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let (w, d) = if ax { (1.5, 0.9) } else { (0.9, 1.5) };
        b.cbox(x, z, w, d, 0.0, 1.5, pal::WOOD);
        b.push_detail(x - 0.5, 0.5, z - 0.5, x + 0.5, 1.0, z + 0.5, pal::DARK);
        if r.chance(0.5) {
            let (dx, dz) = if ax { (1.6, 0.2) } else { (0.2, 1.6) };
            b.cbox(x + dx, z + dz, w * 0.8, d * 0.8, 0.0, 1.35, pal::WOOD);
        }
    }

    pub fn big_crates(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        b.crate_(x, z, 1.5, 0.0);
        let (dx, dz) = if ax {
            (1.4, r.range(-0.3, 0.3))
        } else {
            (r.range(-0.3, 0.3), 1.4)
        };
        b.crate_(x + dx, z + dz, 1.3, 0.0);
        if r.chance(0.5) {
            b.crate_(x + dx * 0.5, z + dz * 0.5, 0.6, 1.5);
        }
    }

    pub fn forklift(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        b.forklift(x, z, ax);
        if r.chance(0.5) {
            let (dx, dz) = if ax { (2.0, 0.0) } else { (0.0, 2.0) };
            b.crate_(x + dx, z + dz, 0.9, 0.0);
        }
    }

    pub fn tank_cube(b: &mut MapBuilder, x: f32, z: f32, ax: bool, _r: &mut Rng) {
        for k in [-0.7f32, 0.7] {
            let (dx, dz) = if ax { (k, 0.0) } else { (0.0, k) };
            b.cbox(x + dx, z + dz, 1.25, 1.25, 0.0, 0.15, pal::DARK);
            b.cbox(x + dx, z + dz, 1.15, 1.15, 0.15, 1.25, pal::TANK);
            b.push_detail(
                x + dx - 0.6,
                0.5,
                z + dz - 0.6,
                x + dx + 0.6,
                0.55,
                z + dz + 0.6,
                pal::METAL,
            );
            b.push_detail(
                x + dx - 0.6,
                1.0,
                z + dz - 0.6,
                x + dx + 0.6,
                1.05,
                z + dz + 0.6,
                pal::METAL,
            );
        }
    }

    pub fn pipes(b: &mut MapBuilder, x: f32, z: f32, ax: bool, _r: &mut Rng) {
        let (w, d) = if ax { (4.0, 1.4) } else { (1.4, 4.0) };
        let (ox, oz) = if ax { (0.0, 0.75) } else { (0.75, 0.0) };
        for s in [-1.0f32, 1.0] {
            b.cbox(x + ox * s, z + oz * s, w, d, 0.0, 1.4, pal::CONCRETE);
        }
        let (ew, ed) = if ax { (w + 0.06, 0.9) } else { (0.9, d + 0.06) };
        b.push_detail(
            x + ox - ew / 2.0,
            0.25,
            z + oz - ed / 2.0,
            x + ox + ew / 2.0,
            1.15,
            z + oz + ed / 2.0,
            pal::DARK,
        );
    }

    pub fn mixer(b: &mut MapBuilder, x: f32, z: f32, _ax: bool, _r: &mut Rng) {
        b.cbox(x, z, 2.2, 2.2, 0.0, 0.5, pal::DARK);
        b.cbox(x, z, 1.8, 1.8, 0.5, 1.3, pal::CONT_ORANGE);
        b.push_detail(x - 0.6, 1.8, z - 0.6, x + 0.6, 2.1, z + 0.6, pal::METAL);
    }

    pub fn sand_heap(b: &mut MapBuilder, x: f32, z: f32, ax: bool, _r: &mut Rng) {
        let (w, d) = if ax { (5.0, 3.6) } else { (3.6, 5.0) };
        b.cbox(x, z, w, d, 0.0, 0.5, pal::SAND);
        b.cbox(x, z, w * 0.7, d * 0.7, 0.5, 0.5, pal::SAND);
        b.cbox(x, z, w * 0.4, d * 0.4, 1.0, 0.45, pal::SAND);
    }

    // ---- tall

    pub fn container(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let c = colour(r);
        b.container(x, z, ax, 0.0, c);
    }

    pub fn lorry(b: &mut MapBuilder, x: f32, z: f32, _ax: bool, r: &mut Rng) {
        let cab = [pal::CAR_A, pal::CAR_B, pal::CAR_C, pal::WALL_BLUE][r.below(4)];
        let load = if r.chance(0.7) { Some(colour(r)) } else { None };
        b.lorry(x, z, cab, load);
    }

    pub fn cabin(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let c = [
            pal::CABIN,
            pal::CONT_BLUE,
            pal::CONT_YELLOW,
            pal::WALL_GREEN,
        ][r.below(4)];
        b.site_cabin(x, z, ax, c);
    }

    pub fn van(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let c = [pal::WALL_WHITE, pal::CAR_A, pal::CAR_B, pal::CONT_RED][r.below(4)];
        let (w, d) = if ax { (4.8, 2.1) } else { (2.1, 4.8) };
        b.cbox(x, z, w, d, 0.0, 0.3, pal::DARK);
        b.cbox(x, z, w, d, 0.3, 2.1, c);
        let (gw, gd) = if ax { (0.08, 1.7) } else { (1.7, 0.08) };
        let (gx, gz) = if ax {
            (x - w / 2.0, z)
        } else {
            (x, z - d / 2.0)
        };
        b.push_detail(
            gx - gw / 2.0 - 0.02,
            1.4,
            gz - gd / 2.0 - 0.02,
            gx + gw / 2.0 + 0.02,
            2.1,
            gz + gd / 2.0 + 0.02,
            pal::WINDOW,
        );
        b.car_details(x, z, ax, pal::TYRE, false);
    }

    pub fn toilets(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        for k in 0..3 {
            let o = (k as f32 - 1.0) * 1.25;
            let (dx, dz) = if ax { (o, 0.0) } else { (0.0, o) };
            let c = [pal::CONT_BLUE, pal::CONT_GREEN, pal::CONT_ORANGE][(k + r.below(3)) % 3];
            b.toilet(x + dx, z + dz, c);
        }
    }

    // ---- stacks

    pub fn stack2(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        b.container(x, z, ax, 0.0, colour(r));
        b.container(x, z, ax, 2.6, colour(r));
    }

    pub fn stack3(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        stack2(b, x, z, ax, r);
        b.container(x, z, ax, 5.2, colour(r));
    }

    /// Two side by side, one on top across the gap.
    pub fn stack_pyramid(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let (dx, dz) = if ax { (0.0, 1.3) } else { (1.3, 0.0) };
        b.container(x - dx, z - dz, ax, 0.0, colour(r));
        b.container(x + dx, z + dz, ax, 0.0, colour(r));
        b.container(x, z, ax, 2.6, colour(r));
    }

    /// A container with crates piled on its roof.
    pub fn stack_crates(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        b.container(x, z, ax, 0.0, colour(r));
        b.container(x, z, ax, 2.6, colour(r));
        for k in 0..3 {
            let o = (k as f32 - 1.0) * 1.6;
            let (dx, dz) = if ax { (o, 0.0) } else { (0.0, o) };
            b.crate_(x + dx, z + dz, r.range(0.9, 1.3), 5.2);
        }
    }

    /// Two cabins, one on the other.
    pub fn cabin_stack(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let c = [pal::CABIN, pal::CONT_BLUE, pal::CONT_YELLOW][r.below(3)];
        b.site_cabin(x, z, ax, c);
        b.lift(2.8, |b| b.site_cabin(x, z, ax, pal::CABIN));
    }

    // ---- litter (never solid)

    pub fn stain(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let (w, d) = (r.range(0.8, 2.6), r.range(0.6, 1.8));
        let (w, d) = if ax { (w, d) } else { (d, w) };
        b.decal(
            x - w / 2.0,
            z - d / 2.0,
            x + w / 2.0,
            z + d / 2.0,
            0.02,
            [70, 70, 76],
        );
    }

    pub fn puddle(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let (w, d) = (r.range(1.2, 3.4), r.range(0.8, 2.0));
        let (w, d) = if ax { (w, d) } else { (d, w) };
        b.decal(
            x - w / 2.0,
            z - d / 2.0,
            x + w / 2.0,
            z + d / 2.0,
            0.025,
            [110, 132, 150],
        );
    }

    pub fn plank(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let l = r.range(1.6, 3.0);
        let (w, d) = if ax { (l, 0.25) } else { (0.25, l) };
        b.cbox(x, z, w, d, 0.0, 0.06, pal::WOOD);
    }

    pub fn flat_pallet(b: &mut MapBuilder, x: f32, z: f32, _ax: bool, _r: &mut Rng) {
        b.cbox(x, z, 1.2, 1.0, 0.0, 0.14, pal::WOOD);
    }

    pub fn paint(b: &mut MapBuilder, x: f32, z: f32, ax: bool, r: &mut Rng) {
        let c = [pal::CONT_YELLOW, pal::WALL_WHITE][r.below(2)];
        let (w, d) = if ax { (2.4, 0.2) } else { (0.2, 2.4) };
        b.decal(x - w / 2.0, z - d / 2.0, x + w / 2.0, z + d / 2.0, 0.02, c);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Both halves of every dressed map got all their cover, and about as much of each
    /// height (the ground area of what stands 0.9–1.3 m, 1.3–2.2 m and higher, their
    /// buildings included).
    #[test]
    fn halves_get_the_same_cover() {
        for (name, (m, dressed), slots) in [
            (
                "cargo",
                crate::maps_cargo::cargo_dressed(),
                crate::maps_cargo::SLOT_COUNT,
            ),
            (
                "depot",
                crate::maps_depot::depot_dressed(),
                crate::maps_depot::SLOT_COUNT,
            ),
            (
                "site",
                crate::maps_site::site_dressed(),
                crate::maps_site::SLOT_COUNT,
            ),
        ] {
            println!("{name}: {:?} of {slots}", dressed);
            assert_eq!(dressed[0], dressed[1], "{name}: halves got different cover");
            assert_eq!(
                dressed[0].0.iter().sum::<usize>(),
                slots,
                "{name}: a slot stayed empty"
            );
            let s = cover_area(&m.boxes, false, m.min, m.max);
            let n = cover_area(&m.boxes, true, m.min, m.max);
            println!("{name}: south {s:?}\n{name}: north {n:?}");
            for k in 0..3 {
                let (a, b) = (s[k], n[k]);
                assert!(
                    (a - b).abs() <= 0.25 * a.max(b) + 4.0,
                    "{name}: cover band {k}: {a:.0} m² against {b:.0} m²"
                );
            }
        }
    }

    /// The two halves do not look alike: most of the cover stands somewhere else, or is
    /// something else, than its counterpart across the middle.
    #[test]
    fn halves_are_not_mirror_images() {
        for m in [
            crate::maps_cargo::cargo(),
            crate::maps_depot::depot(),
            crate::maps_site::site(),
        ] {
            let solid: Vec<&MapBox> = m
                .boxes
                .iter()
                .filter(|b| b.solid && b.min.y > -0.5)
                .collect();
            let turned = |b: &MapBox| {
                (
                    vec3(-b.max.x, b.min.y, -b.max.z),
                    vec3(-b.min.x, b.max.y, -b.min.z),
                )
            };
            let twins = solid
                .iter()
                .filter(|b| {
                    let (lo, hi) = turned(b);
                    solid
                        .iter()
                        .any(|o| o.min.distance(lo) < 0.05 && o.max.distance(hi) < 0.05)
                })
                .count();
            let share = twins as f32 / solid.len() as f32;
            println!(
                "{:?}: {:.0}% of the solid boxes have a twin across the middle",
                m.id,
                share * 100.0
            );
            assert!(
                share < 0.85,
                "{:?}: still mostly a mirror image ({share})",
                m.id
            );
        }
    }
}
