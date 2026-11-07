//! «Ринок» — a market district, 168 × 88 m.
//!
//! A main street runs west–east through a market square full of stalls and carts. Rows of
//! shops and houses line it, with alleys between them; behind them run back lanes hung
//! with laundry, then warehouses, houses and fenced courtyards (the Sabotage sites), and
//! strips along the outer walls. The teams start in yards at either end.
//!
//! The two sides are built differently but play the same: from each yard three routes
//! lead in (the street and both back lanes), each side has its site courtyard, one house
//! with a walled roof over the street, a warehouse to go through and about six roofs
//! reached by ladders, all at the same distances from its yard and from the bomb. The
//! east has shops under awnings, stall rows, a fountain and a bus station; the west a
//! covered market hall, garages, a café terrace, a well and a lorry depot, and its site
//! and warehouse sit across the street from where the east has them.
//!
//! Awnings, sheets, canopies and lorry covers are cloth; stall boards, carts, tables and
//! plank fences are wood: bullets go through both.

use crate::map::{door, pal, wide_door, window, Map, MapBuilder, MapId, Opening, Side, Spawn};
use crate::props::Tower;
use glam::{vec2, vec3, Vec3};
use std::f32::consts::FRAC_PI_2;

const HALF_X: f32 = 84.0;
const HALF_Z: f32 = 44.0;

/// Tomatoes, oranges, cabbages, lemons, aubergines, melons.
const PRODUCE: [[u8; 3]; 6] = [
    [220, 60, 50],
    [250, 160, 40],
    [120, 190, 70],
    [240, 220, 80],
    [150, 80, 160],
    [80, 150, 70],
];
const CANVAS: [[u8; 3]; 5] = [
    [240, 96, 96],
    [96, 196, 230],
    [250, 214, 90],
    [150, 210, 140],
    [240, 240, 228],
];
const SHEETS: [[u8; 3]; 4] = [
    [245, 245, 240],
    [170, 210, 240],
    [250, 200, 200],
    [240, 230, 170],
];
const STONE: [u8; 3] = [214, 204, 186];
const WATER: [u8; 3] = [110, 180, 220];
const BUS: [u8; 3] = [240, 190, 60];

/// One half of the map: everything is drawn in "east" coordinates and turned round
/// (x, z → −x, −z) for the west half.
#[derive(Clone, Copy)]
struct Half {
    s: f32,
}

impl Half {
    fn x(&self, v: f32) -> f32 {
        self.s * v
    }
    fn p(&self, x: f32, z: f32) -> (f32, f32) {
        (self.s * x, self.s * z)
    }
    fn v(&self, v: Vec3) -> Vec3 {
        vec3(self.s * v.x, v.y, self.s * v.z)
    }
    /// Sorted rectangle (x0, z0, x1, z1).
    fn rect(&self, x0: f32, z0: f32, x1: f32, z1: f32) -> (f32, f32, f32, f32) {
        let (a, b) = self.p(x0, z0);
        let (c, d) = self.p(x1, z1);
        (a.min(c), b.min(d), a.max(c), b.max(d))
    }
    fn side(&self, side: Side) -> Side {
        if self.s > 0.0 {
            return side;
        }
        match side {
            Side::MinX => Side::MaxX,
            Side::MaxX => Side::MinX,
            Side::MinZ => Side::MaxZ,
            Side::MaxZ => Side::MinZ,
        }
    }
    fn ops(&self, ops: &[(Side, Opening)]) -> Vec<(Side, Opening)> {
        ops.iter()
            .map(|(sd, o)| {
                (
                    self.side(*sd),
                    Opening {
                        at: self.s * o.at,
                        ..*o
                    },
                )
            })
            .collect()
    }
    fn block(
        &self,
        b: &mut MapBuilder,
        x0: f32,
        y0: f32,
        z0: f32,
        x1: f32,
        y1: f32,
        z1: f32,
        c: [u8; 3],
    ) {
        let (a, bz, cx, d) = self.rect(x0, z0, x1, z1);
        b.block(a, y0, bz, cx, y1, d, c);
    }
    fn wood(
        &self,
        b: &mut MapBuilder,
        x0: f32,
        y0: f32,
        z0: f32,
        x1: f32,
        y1: f32,
        z1: f32,
        c: [u8; 3],
    ) {
        let (a, bz, cx, d) = self.rect(x0, z0, x1, z1);
        b.wood(a, y0, bz, cx, y1, d, c);
    }
    fn cloth(
        &self,
        b: &mut MapBuilder,
        x0: f32,
        y0: f32,
        z0: f32,
        x1: f32,
        y1: f32,
        z1: f32,
        c: [u8; 3],
    ) {
        let (a, bz, cx, d) = self.rect(x0, z0, x1, z1);
        b.cloth(a, y0, bz, cx, y1, d, c);
    }
    fn detail(
        &self,
        b: &mut MapBuilder,
        x0: f32,
        y0: f32,
        z0: f32,
        x1: f32,
        y1: f32,
        z1: f32,
        c: [u8; 3],
    ) {
        let (a, bz, cx, d) = self.rect(x0, z0, x1, z1);
        b.push_detail(a, y0, bz, cx, y1, d, c);
    }
    fn decal(&self, b: &mut MapBuilder, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, c: [u8; 3]) {
        let (a, bz, cx, d) = self.rect(x0, z0, x1, z1);
        b.decal(a, bz, cx, d, y, c);
    }
}

/// Which way a house faces: its front wall is at `front`, its back wall at `back`.
#[derive(Clone, Copy)]
struct Lot {
    x0: f32,
    x1: f32,
    front: f32,
    back: f32,
}

impl Lot {
    /// +1 when the street is towards +z.
    fn out(&self) -> f32 {
        (self.front - self.back).signum()
    }
    fn front_side(&self) -> Side {
        if self.out() > 0.0 {
            Side::MaxZ
        } else {
            Side::MinZ
        }
    }
    fn back_side(&self) -> Side {
        if self.out() > 0.0 {
            Side::MinZ
        } else {
            Side::MaxZ
        }
    }
    fn mid_z(&self) -> f32 {
        (self.front + self.back) / 2.0
    }
    fn rect(&self) -> (f32, f32, f32, f32) {
        (
            self.x0,
            self.front.min(self.back),
            self.x1,
            self.front.max(self.back),
        )
    }
}

/// One-storey shop with a flat roof, a cloth awning over its street door and, if asked, a
/// ladder up its back wall at `ladder` (x).
fn shop(b: &mut MapBuilder, h: Half, l: Lot, color: [u8; 3], roof: [u8; 3], ladder: Option<f32>) {
    let height = 4.2;
    let door_x = l.x0 + 3.0;
    let ops = h.ops(&[
        (l.front_side(), door(door_x)),
        (l.front_side(), window(l.x1 - 3.0)),
        (l.back_side(), door(l.x1 - 2.5)),
        (Side::MinX, window(l.mid_z())),
        (Side::MaxX, window(l.mid_z())),
    ]);
    let (x0, z0, x1, z1) = l.rect();
    let (a, bz, c, d) = h.rect(x0, z0, x1, z1);
    b.building(a, bz, c, d, 0.0, height, &ops, color, Some(roof));
    // awning over the door, and a counter inside
    let o = l.out();
    let canvas = CANVAS[((l.x0.abs() as usize) / 7) % CANVAS.len()];
    h.cloth(
        b,
        door_x - 1.7,
        2.85,
        l.front,
        door_x + 1.7,
        2.95,
        l.front + o * 1.8,
        canvas,
    );
    let inner = l.front - o * 2.2;
    h.wood(
        b,
        l.x0 + 5.0,
        0.0,
        inner - 0.4,
        l.x1 - 1.0,
        1.0,
        inner + 0.4,
        pal::WOOD,
    );
    for (k, x) in [l.x0 + 6.0, l.x0 + 7.5].iter().enumerate() {
        h.detail(
            b,
            x - 0.5,
            1.0,
            inner - 0.3,
            x + 0.5,
            1.25,
            inner + 0.3,
            PRODUCE[k * 2 % 6],
        );
    }
    if let Some(lx) = ladder {
        let face = h.v(vec3(lx, 0.0, l.back));
        let out = h.v(vec3(0.0, 0.0, -o));
        b.ladder(face, out, height + 0.35);
    }
}

/// Two-storey house with stairs inside, a balcony over the street and, if `roof_access`,
/// a stairwell onto a walled roof.
fn house(b: &mut MapBuilder, h: Half, l: Lot, color: [u8; 3], roof: [u8; 3], roof_access: bool) {
    let cx = (l.x0 + l.x1) / 2.0;
    let fs = l.front_side();
    let bs = l.back_side();
    let ground = h.ops(&[
        (fs, door(cx - 2.5)),
        (fs, window(cx + 2.5)),
        (bs, door(cx + 2.5)),
        (Side::MaxX, window(l.mid_z())),
    ]);
    let upper = h.ops(&[
        (
            fs,
            Opening {
                at: cx + 2.5,
                w: 1.6,
                bottom: 0.0,
                top: 2.4,
            },
        ),
        (fs, window(cx - 2.5)),
        (bs, window(cx)),
        (Side::MinX, window(l.mid_z())),
        (Side::MaxX, window(l.mid_z())),
    ]);
    let (x0, z0, x1, z1) = l.rect();
    let (a, bz, c, d) = h.rect(x0, z0, x1, z1);
    b.tower(&Tower {
        x0: a,
        z0: bz,
        x1: c,
        z1: d,
        floors: 2,
        storey: 3.4,
        color,
        roof,
        ground: &ground,
        upper: &upper,
        roof_access,
    });
    let (al0, al1) = (h.x(cx + 0.5), h.x(cx + 5.0));
    let wall = if h.s > 0.0 { l.front } else { -l.front };
    b.balcony(h.side(fs), al0.min(al1), al0.max(al1), wall, 3.4, 1.5);
}

/// Tall warehouse with wide doors on both ends, crates inside and a ladder up one side.
fn warehouse(b: &mut MapBuilder, h: Half, l: Lot, color: [u8; 3]) {
    let height = 6.0;
    let cx = (l.x0 + l.x1) / 2.0;
    let ops = h.ops(&[
        (l.front_side(), wide_door(cx)),
        (l.back_side(), wide_door(cx - 3.0)),
        (Side::MinX, door(l.mid_z())),
        (Side::MaxX, window(l.mid_z())),
    ]);
    let (x0, z0, x1, z1) = l.rect();
    let (a, bz, c, d) = h.rect(x0, z0, x1, z1);
    b.building(a, bz, c, d, 0.0, height, &ops, color, Some(pal::ROOF_GREY));
    // stacks of crates and a pallet of sacks inside
    let mz = l.mid_z();
    for (dx, dz, s, y) in [
        (-4.5, -3.0, 1.4, 0.0),
        (-4.5, -3.0, 1.1, 1.4),
        (-2.8, -3.2, 1.2, 0.0),
        (4.0, 2.5, 1.4, 0.0),
        (5.4, 2.5, 1.2, 0.0),
        (4.0, 2.5, 1.0, 1.4),
        (2.0, -4.0, 1.2, 0.0),
    ] {
        let (px, pz) = h.p(cx + dx, mz + dz);
        b.crate_(px, pz, s, y);
    }
    h.block(
        b,
        cx - 1.0,
        0.0,
        mz + 3.5,
        cx + 1.6,
        0.9,
        mz + 5.0,
        pal::SANDBAG,
    );
    // ladder up the east (MaxX) wall
    let face = h.v(vec3(l.x1, 0.0, mz + 2.0));
    b.ladder(face, h.v(vec3(1.0, 0.0, 0.0)), height + 0.35);
}

/// Market stall: a wooden counter with produce under a cloth canopy on four poles.
fn stall(b: &mut MapBuilder, h: Half, cx: f32, cz: f32, along_x: bool, k: usize) {
    let (w, d) = if along_x { (2.6, 1.0) } else { (1.0, 2.6) };
    h.wood(
        b,
        cx - w / 2.0,
        0.0,
        cz - d / 2.0,
        cx + w / 2.0,
        1.0,
        cz + d / 2.0,
        pal::WOOD,
    );
    for j in 0..3 {
        let t = (j as f32 - 1.0) * 0.8;
        let (px, pz) = if along_x { (cx + t, cz) } else { (cx, cz + t) };
        h.detail(
            b,
            px - 0.35,
            1.0,
            pz - 0.35,
            px + 0.35,
            1.28,
            pz + 0.35,
            PRODUCE[(k + j) % 6],
        );
    }
    let (pw, pd) = (w / 2.0 + 0.2, d / 2.0 + 0.5);
    for (sx, sz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let (px, pz) = (cx + sx * pw, cz + sz * pd);
        h.block(
            b,
            px - 0.06,
            0.0,
            pz - 0.06,
            px + 0.06,
            2.5,
            pz + 0.06,
            pal::TRUNK,
        );
    }
    h.cloth(
        b,
        cx - pw - 0.2,
        2.5,
        cz - pd - 0.2,
        cx + pw + 0.2,
        2.58,
        cz + pd + 0.2,
        CANVAS[k % CANVAS.len()],
    );
}

/// Hand cart full of produce: wooden bed and sides, two wheels, handles.
fn cart(b: &mut MapBuilder, h: Half, cx: f32, cz: f32, along_x: bool, k: usize) {
    let (w, d) = if along_x { (2.2, 1.2) } else { (1.2, 2.2) };
    let (x0, z0, x1, z1) = (cx - w / 2.0, cz - d / 2.0, cx + w / 2.0, cz + d / 2.0);
    h.wood(b, x0, 0.55, z0, x1, 0.75, z1, pal::WOOD);
    h.wood(b, x0, 0.75, z0, x1, 1.1, z0 + 0.06, pal::CRATE);
    h.wood(b, x0, 0.75, z1 - 0.06, x1, 1.1, z1, pal::CRATE);
    h.wood(b, x0, 0.75, z0, x0 + 0.06, 1.1, z1, pal::CRATE);
    h.wood(b, x1 - 0.06, 0.75, z0, x1, 1.1, z1, pal::CRATE);
    // wheels on the long sides
    for s in [-1.0f32, 1.0] {
        if along_x {
            h.block(
                b,
                cx - 0.35,
                0.0,
                cz + s * (d / 2.0 + 0.05) - 0.07,
                cx + 0.35,
                0.7,
                cz + s * (d / 2.0 + 0.05) + 0.07,
                pal::TYRE,
            );
        } else {
            h.block(
                b,
                cx + s * (w / 2.0 + 0.05) - 0.07,
                0.0,
                cz - 0.35,
                cx + s * (w / 2.0 + 0.05) + 0.07,
                0.7,
                cz + 0.35,
                pal::TYRE,
            );
        }
    }
    // a heap of produce and the handles
    for j in 0..4 {
        let t = (j as f32 - 1.5) * 0.5;
        let (px, pz) = if along_x { (cx + t, cz) } else { (cx, cz + t) };
        h.detail(
            b,
            px - 0.25,
            0.75,
            pz - 0.4,
            px + 0.25,
            1.2 + 0.1 * (j % 2) as f32,
            pz + 0.4,
            PRODUCE[(k + j) % 6],
        );
    }
    if along_x {
        h.detail(b, x1, 0.8, cz - 0.4, x1 + 0.9, 0.86, cz - 0.34, pal::TRUNK);
        h.detail(b, x1, 0.8, cz + 0.34, x1 + 0.9, 0.86, cz + 0.4, pal::TRUNK);
    } else {
        h.detail(b, cx - 0.4, 0.8, z1, cx - 0.34, 0.86, z1 + 0.9, pal::TRUNK);
        h.detail(b, cx + 0.34, 0.8, z1, cx + 0.4, 0.86, z1 + 0.9, pal::TRUNK);
    }
}

/// Washing line across a lane (from z0 to z1 at x) with sheets hanging from it: cloth
/// you can walk and shoot through but not see through.
fn laundry(b: &mut MapBuilder, h: Half, x: f32, z0: f32, z1: f32, k: usize) {
    h.detail(b, x - 0.02, 2.75, z0, x + 0.02, 2.8, z1, pal::DARK);
    let n = ((z1 - z0) / 1.5).floor().max(1.0) as usize;
    let w = (z1 - z0) / n as f32;
    for j in 0..n {
        let a = z0 + j as f32 * w + 0.08;
        let low = 1.0 + 0.25 * ((j + k) % 3) as f32;
        h.cloth(
            b,
            x - 0.03,
            low,
            a,
            x + 0.03,
            2.75,
            a + w - 0.16,
            SHEETS[(j + k) % SHEETS.len()],
        );
    }
}

/// Plank fence along x (at z) from x0 to x1, chest high.
fn fence_x(b: &mut MapBuilder, h: Half, x0: f32, x1: f32, z: f32) {
    h.wood(b, x0, 0.0, z - 0.04, x1, 1.6, z + 0.04, pal::WOOD);
    let mut x = x0 + 0.2;
    while x < x1 {
        h.block(
            b,
            x - 0.06,
            0.0,
            z - 0.1,
            x + 0.06,
            1.75,
            z + 0.1,
            pal::TRUNK,
        );
        x += 2.0;
    }
}

/// Plank fence along z (at x).
fn fence_z(b: &mut MapBuilder, h: Half, z0: f32, z1: f32, x: f32) {
    h.wood(b, x - 0.04, 0.0, z0, x + 0.04, 1.6, z1, pal::WOOD);
    let mut z = z0 + 0.2;
    while z < z1 {
        h.block(
            b,
            x - 0.1,
            0.0,
            z - 0.06,
            x + 0.1,
            1.75,
            z + 0.06,
            pal::TRUNK,
        );
        z += 2.0;
    }
}

fn dumpster(b: &mut MapBuilder, h: Half, cx: f32, cz: f32) {
    h.block(
        b,
        cx - 1.1,
        0.0,
        cz - 0.6,
        cx + 1.1,
        1.3,
        cz + 0.6,
        pal::CONT_GREEN,
    );
    h.detail(
        b,
        cx - 1.15,
        1.3,
        cz - 0.65,
        cx + 1.15,
        1.38,
        cz + 0.65,
        pal::DARK,
    );
}

/// City bus parked along z.
fn bus(b: &mut MapBuilder, h: Half, cx: f32, cz: f32) {
    h.block(b, cx - 1.3, 0.3, cz - 5.0, cx + 1.3, 3.1, cz + 5.0, BUS);
    h.detail(
        b,
        cx - 1.32,
        1.6,
        cz - 4.6,
        cx + 1.32,
        2.5,
        cz + 4.6,
        pal::WINDOW,
    );
    h.detail(
        b,
        cx - 1.2,
        3.1,
        cz - 4.6,
        cx + 1.2,
        3.25,
        cz + 4.6,
        pal::METAL,
    );
    for z in [-3.4, 3.4] {
        h.block(
            b,
            cx - 1.35,
            0.0,
            cz + z - 0.5,
            cx + 1.35,
            0.6,
            cz + z + 0.5,
            pal::TYRE,
        );
    }
}

/// Small kiosk with a roof.
fn kiosk(b: &mut MapBuilder, h: Half, cx: f32, cz: f32, c: [u8; 3]) {
    h.block(b, cx - 1.5, 0.0, cz - 1.5, cx + 1.5, 2.6, cz + 1.5, c);
    h.block(
        b,
        cx - 1.8,
        2.6,
        cz - 1.8,
        cx + 1.8,
        2.85,
        cz + 1.8,
        pal::ROOF_RED,
    );
    h.detail(
        b,
        cx - 1.52,
        1.1,
        cz - 1.0,
        cx + 1.52,
        1.9,
        cz + 1.0,
        pal::WINDOW,
    );
}

/// Stone fountain: a basin, water and a column.
fn fountain(b: &mut MapBuilder, h: Half, cx: f32, cz: f32) {
    h.block(b, cx - 2.2, 0.0, cz - 2.2, cx + 2.2, 0.7, cz - 1.8, STONE);
    h.block(b, cx - 2.2, 0.0, cz + 1.8, cx + 2.2, 0.7, cz + 2.2, STONE);
    h.block(b, cx - 2.2, 0.0, cz - 1.8, cx - 1.8, 0.7, cz + 1.8, STONE);
    h.block(b, cx + 1.8, 0.0, cz - 1.8, cx + 2.2, 0.7, cz + 1.8, STONE);
    h.decal(b, cx - 1.8, cz - 1.8, cx + 1.8, cz + 1.8, 0.45, WATER);
    h.block(b, cx - 0.4, 0.0, cz - 0.4, cx + 0.4, 2.2, cz + 0.4, STONE);
    h.block(b, cx - 0.7, 2.2, cz - 0.7, cx + 0.7, 2.45, cz + 0.7, STONE);
}

/// Courtyard with a Sabotage site: wooden fences with gates, a shed, a fuel tank.
fn courtyard(
    b: &mut MapBuilder,
    h: Half,
    x0: f32,
    x1: f32,
    z0: f32,
    z1: f32,
    pad: [u8; 3],
) -> Vec3 {
    let cx = (x0 + x1) / 2.0;
    let cz = (z0 + z1) / 2.0;
    // fences with gaps (gates) in the middle of every side
    fence_x(b, h, x0, cx - 2.0, z0);
    fence_x(b, h, cx + 2.0, x1, z0);
    fence_x(b, h, x0, cx - 2.0, z1);
    fence_x(b, h, cx + 2.0, x1, z1);
    fence_z(b, h, z0, cz - 2.0, x0);
    fence_z(b, h, cz + 2.0, z1, x0);
    fence_z(b, h, z0, cz - 2.0, x1);
    fence_z(b, h, cz + 2.0, z1, x1);
    // shed in a corner, a tank, sandbags
    h.block(
        b,
        x1 - 4.0,
        0.0,
        z1 - 3.5,
        x1 - 0.6,
        2.6,
        z1 - 0.6,
        pal::WALL_WHITE,
    );
    h.block(
        b,
        x1 - 4.3,
        2.6,
        z1 - 3.8,
        x1 - 0.3,
        2.85,
        z1 - 0.3,
        pal::ROOF_GREY,
    );
    let (tx, tz) = h.p(x0 + 2.0, z0 + 2.2);
    b.tank(tx, tz);
    h.block(
        b,
        cx - 3.5,
        0.0,
        cz + 3.0,
        cx - 1.0,
        1.05,
        cz + 3.8,
        pal::SANDBAG,
    );
    let site = h.v(vec3(cx, 0.0, cz));
    b.decal(
        site.x - 2.6,
        site.z - 2.6,
        site.x + 2.6,
        site.z + 2.6,
        0.0,
        pad,
    );
    b.decal(
        site.x - 1.8,
        site.z - 1.8,
        site.x + 1.8,
        site.z + 1.8,
        0.03,
        pal::WALL_WHITE,
    );
    b.decal(
        site.x - 1.5,
        site.z - 1.5,
        site.x + 1.5,
        site.z + 1.5,
        0.06,
        pad,
    );
    site
}

/// Open market hall: pillars round the edge, a flat roof (with a ladder up its back
/// edge) and stalls underneath.
fn market_hall(b: &mut MapBuilder, h: Half, l: Lot, ladder_x: f32) {
    let height = 4.0;
    let (x0, z0, x1, z1) = l.rect();
    let mut x = x0 + 0.3;
    while x <= x1 - 0.2 {
        for z in [z0 + 0.3, z1 - 0.3] {
            h.block(
                b,
                x - 0.25,
                0.0,
                z - 0.25,
                x + 0.25,
                height,
                z + 0.25,
                pal::WALL_WHITE,
            );
        }
        x += (x1 - x0 - 0.6) / 3.0;
    }
    h.block(b, x0, height, z0, x1, height + 0.35, z1, pal::ROOF_RED);
    let mid = l.mid_z();
    for (k, sx) in [x0 + 3.5, x0 + 7.5, x1 - 3.0].iter().enumerate() {
        stall(b, h, *sx, mid, true, k + 3);
    }
    let face = h.v(vec3(ladder_x, 0.0, l.back));
    b.ladder(face, h.v(vec3(0.0, 0.0, -l.out())), height + 0.35);
}

/// Small garage with a wide door to the street and a car inside.
fn garage(b: &mut MapBuilder, h: Half, l: Lot, color: [u8; 3], car: [u8; 3]) {
    let cx = (l.x0 + l.x1) / 2.0;
    let ops = h.ops(&[(l.front_side(), wide_door(cx)), (l.back_side(), window(cx))]);
    let (x0, z0, x1, z1) = l.rect();
    let (a, bz, c, d) = h.rect(x0, z0, x1, z1);
    b.building(a, bz, c, d, 0.0, 3.4, &ops, color, Some(pal::ROOF_GREY));
    let (px, pz) = h.p(cx, l.mid_z());
    b.car(px, pz, false, car);
}

/// Café terrace in front of a shop: wooden tables under cloth parasols.
fn terrace(b: &mut MapBuilder, h: Half, x0: f32, x1: f32, z: f32) {
    let mut x = x0 + 1.2;
    let mut k = 0;
    while x < x1 - 0.8 {
        h.wood(b, x - 0.5, 0.0, z - 0.5, x + 0.5, 0.8, z + 0.5, pal::WOOD);
        h.block(
            b,
            x - 0.05,
            0.8,
            z - 0.05,
            x + 0.05,
            2.3,
            z + 0.05,
            pal::METAL,
        );
        h.cloth(
            b,
            x - 1.0,
            2.3,
            z - 1.0,
            x + 1.0,
            2.38,
            z + 1.0,
            CANVAS[k % CANVAS.len()],
        );
        x += 2.6;
        k += 1;
    }
}

/// Well with a little roof (the west square's centrepiece).
fn well(b: &mut MapBuilder, h: Half, cx: f32, cz: f32) {
    h.block(b, cx - 1.1, 0.0, cz - 1.1, cx + 1.1, 0.9, cz + 1.1, STONE);
    h.decal(b, cx - 0.8, cz - 0.8, cx + 0.8, cz + 0.8, 0.9, WATER);
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, 1.0)] {
        h.block(
            b,
            cx + sx * 0.95 - 0.08,
            0.9,
            cz + sz * 0.95 - 0.08,
            cx + sx * 0.95 + 0.08,
            2.6,
            cz + sz * 0.95 + 0.08,
            pal::TRUNK,
        );
    }
    h.block(
        b,
        cx - 1.4,
        2.6,
        cz - 1.4,
        cx + 1.4,
        2.8,
        cz + 1.4,
        pal::ROOF_RED,
    );
}

/// Lorry: cab and a canvas-covered cargo box (cloth: bullets go through the back).
fn lorry(b: &mut MapBuilder, h: Half, cx: f32, cz: f32, c: [u8; 3]) {
    h.block(
        b,
        cx - 1.2,
        0.0,
        cz - 4.2,
        cx + 1.2,
        0.6,
        cz + 4.2,
        pal::DARK,
    );
    h.block(b, cx - 1.2, 0.6, cz + 2.4, cx + 1.2, 2.8, cz + 4.2, c);
    h.detail(
        b,
        cx - 1.22,
        1.6,
        cz + 3.3,
        cx + 1.22,
        2.5,
        cz + 4.22,
        pal::WINDOW,
    );
    h.wood(
        b,
        cx - 1.2,
        0.6,
        cz - 4.2,
        cx + 1.2,
        1.2,
        cz + 2.3,
        pal::WOOD,
    );
    h.cloth(
        b,
        cx - 1.2,
        1.2,
        cz - 4.2,
        cx + 1.2,
        3.0,
        cz + 2.3,
        [120, 130, 90],
    );
}

// The map in numbers (half of it, x > 0, z > 0 towards the team's "south" in local
// coordinates): street |z| ≤ 5, sidewalks to 6.5, the front rows 6.5..16, back lanes
// 16..21, the back rows 21..35, parking 35..44; the square x < 20, the rows 22..68, the
// team's yard 70..84.
const ROW_A: (f32, f32) = (6.5, 16.0);
const LANE: (f32, f32) = (16.0, 21.0);

fn row_a(x0: f32, x1: f32, north: bool) -> Lot {
    let s = if north { -1.0 } else { 1.0 };
    Lot {
        x0,
        x1,
        front: s * ROW_A.0,
        back: s * ROW_A.1,
    }
}

fn row_b(x0: f32, x1: f32, north: bool, depth: f32) -> Lot {
    let s = if north { -1.0 } else { 1.0 };
    Lot {
        x0,
        x1,
        front: s * LANE.1,
        back: s * (LANE.1 + depth),
    }
}

/// The east side (the axis team's): shops under awnings, a square of stall rows and a
/// fountain, a bus station.
fn east(b: &mut MapBuilder) -> Vec3 {
    let h = Half { s: 1.0 };
    // ---- the square's east part: four rows of stalls, a fountain, carts
    for (i, &x) in [4.5f32, 10.0, 15.5].iter().enumerate() {
        for (j, &z) in [9.5f32, 13.5, -9.5, -13.5].iter().enumerate() {
            stall(b, h, x, z, true, i * 4 + j);
        }
    }
    fountain(b, h, 0.5, 11.5);
    cart(b, h, 8.0, 6.9, true, 1);
    cart(b, h, 18.0, -6.9, true, 3);
    cart(b, h, 19.5, 11.5, false, 4);
    b.tree(19.5, 15.0, false);
    b.tree(19.5, -15.0, false);

    // ---- the street
    for (x, z, c) in [
        (26.0, 3.8, pal::CAR_A),
        (40.0, -3.8, pal::CAR_C),
        (51.0, 3.8, pal::CAR_B),
        (63.0, -3.8, pal::CAR_A),
    ] {
        b.car(x, z, true, c);
    }
    kiosk(b, h, 21.0, 3.8, pal::WALL_BLUE);
    cart(b, h, 33.0, -4.0, true, 2);

    // ---- front rows
    shop(
        b,
        h,
        row_a(22.0, 32.0, false),
        pal::WALL_PINK,
        pal::ROOF_RED,
        Some(26.0),
    );
    house(
        b,
        h,
        row_a(35.0, 45.0, false),
        pal::WALL_BLUE,
        pal::ROOF_GREY,
        false,
    );
    shop(
        b,
        h,
        row_a(48.0, 58.0, false),
        pal::WALL_GREEN,
        pal::ROOF_RED,
        Some(54.0),
    );
    shop(
        b,
        h,
        row_a(60.0, 69.0, false),
        pal::WALL_TAN,
        pal::ROOF_GREY,
        None,
    );
    house(
        b,
        h,
        row_a(22.0, 33.0, true),
        pal::WALL_WHITE,
        pal::ROOF_RED,
        true,
    );
    shop(
        b,
        h,
        row_a(36.0, 46.0, true),
        pal::WALL_TAN,
        pal::ROOF_GREY,
        Some(40.0),
    );
    shop(
        b,
        h,
        row_a(49.0, 59.0, true),
        pal::WALL_PINK,
        pal::ROOF_RED,
        None,
    );
    shop(
        b,
        h,
        row_a(60.0, 69.0, true),
        pal::WALL_BLUE,
        pal::ROOF_GREY,
        Some(64.0),
    );

    // ---- back lanes
    laundry(b, h, 33.5, 16.2, 20.8, 0);
    laundry(b, h, 59.0, 16.2, 20.8, 1);
    laundry(b, h, 47.5, -20.8, -16.2, 2);
    laundry(b, h, 66.0, -20.8, -16.2, 3);
    cart(b, h, 40.0, 18.5, true, 0);
    cart(b, h, 56.0, -18.6, true, 5);
    dumpster(b, h, 51.0, 19.9);
    dumpster(b, h, 25.0, -19.9);
    fence_x(b, h, 62.0, 66.0, 18.5);
    fence_x(b, h, 43.0, 46.5, -18.5);
    for (x, z, s) in [(29.0, 20.0, 1.1), (30.2, 20.1, 0.8), (53.0, -17.0, 1.0)] {
        b.crate_(x, z, s, 0.0);
    }

    // ---- back rows: a warehouse, the site, houses
    warehouse(b, h, row_b(22.0, 35.0, false, 13.0), pal::WALL_TAN);
    let site = courtyard(b, h, 39.0, 51.0, 22.5, 33.5, pal::PAD_AXIS);
    house(
        b,
        h,
        row_b(55.0, 67.0, false, 12.0),
        pal::WALL_GREEN,
        pal::ROOF_RED,
        false,
    );
    shop(
        b,
        h,
        row_b(22.0, 31.0, true, 10.0),
        pal::WALL_BLUE,
        pal::ROOF_RED,
        Some(27.0),
    );
    warehouse(b, h, row_b(35.0, 50.0, true, 13.0), pal::BRICK);
    house(
        b,
        h,
        row_b(54.0, 66.0, true, 12.0),
        pal::WALL_PINK,
        pal::ROOF_GREY,
        false,
    );

    // ---- behind the square: planters, a kiosk
    h.block(b, 3.0, 0.0, 26.0, 17.0, 0.9, 27.0, STONE);
    kiosk(b, h, 10.0, -26.0, pal::WALL_GREEN);
    dumpster(b, h, 14.0, 33.0);
    cart(b, h, 6.0, -33.0, false, 2);

    // ---- parking along the walls
    for (x, z, c, along) in [
        (28.0, 39.5, pal::CAR_B, false),
        (34.0, 39.5, pal::CAR_C, false),
        (48.0, 39.5, pal::CAR_A, false),
        (62.0, 39.5, pal::CAR_B, true),
        (36.0, -39.5, pal::CAR_A, false),
        (58.0, -39.5, pal::CAR_C, false),
    ] {
        b.car(x, z, along, c);
    }
    b.container(66.0, -39.0, true, 0.0, pal::CONT_ORANGE);
    for (x, z) in [(42.0, 41.5), (54.0, -41.5), (18.0, 38.0), (18.0, -38.0)] {
        b.tree(x, z, false);
    }

    // ---- bus station: the team's yard
    bus(b, h, 77.0, -12.0);
    bus(b, h, 77.0, 14.0);
    kiosk(b, h, 75.0, 28.0, pal::WALL_WHITE);
    for z in [-26.0f32, 4.0, 30.0] {
        b.barrier(71.0, z, false);
    }
    b.decal(70.0, -44.0, 84.0, 44.0, 0.0, pal::ASPHALT);
    site
}

/// The west side (the allies'): the same routes and the same kinds of positions as the
/// east, built differently: a covered market hall, garages, a café terrace, a well in the
/// square, a lorry depot.
/// Drawn in local coordinates turned round (x, z → −x, −z).
fn west(b: &mut MapBuilder) -> Vec3 {
    let h = Half { s: -1.0 };
    // ---- the square's west part: long stall rows one way, a well and carts the other
    for (i, &x) in [5.0f32, 10.0, 15.0].iter().enumerate() {
        for (j, &z) in [9.2f32, 13.4].iter().enumerate() {
            stall(b, h, x, z, false, i * 2 + j + 1);
        }
    }
    well(b, h, 9.0, -11.5);
    for (k, &x) in [4.0f32, 15.0].iter().enumerate() {
        stall(b, h, x, -9.5, true, k + 4);
        stall(b, h, x, -14.0, true, k + 6);
    }
    cart(b, h, 12.0, 7.0, true, 2);
    cart(b, h, 3.0, -7.0, true, 4);
    cart(b, h, 19.0, -11.0, false, 0);
    cart(b, h, 19.0, 11.0, false, 1);
    let (tx, tz) = h.p(1.5, 15.0);
    b.tree(tx, tz, true);
    let (tx, tz) = h.p(1.5, -15.0);
    b.tree(tx, tz, true);

    // ---- the street
    for (x, z, c) in [
        (24.0, -3.8, pal::CAR_B),
        (37.0, 3.8, pal::CAR_A),
        (47.0, -3.8, pal::CAR_C),
        (60.0, 3.8, pal::CAR_B),
    ] {
        let (px, pz) = h.p(x, z);
        b.car(px, pz, true, c);
    }
    cart(b, h, 30.0, 4.0, true, 5);
    let (bx, bz) = h.p(54.0, -3.0);
    b.barrier(bx, bz, true);

    // ---- front rows: the high house across the street from the east one's side
    house(
        b,
        h,
        row_a(22.0, 33.0, false),
        pal::WALL_TAN,
        pal::ROOF_RED,
        true,
    );
    shop(
        b,
        h,
        row_a(36.0, 46.0, false),
        pal::WALL_GREEN,
        pal::ROOF_GREY,
        Some(40.0),
    );
    terrace(b, h, 36.0, 46.0, 5.6);
    garage(b, h, row_a(49.0, 56.0, false), pal::WALL_WHITE, pal::CAR_C);
    house(
        b,
        h,
        row_a(58.0, 69.0, false),
        pal::WALL_PINK,
        pal::ROOF_GREY,
        false,
    );
    market_hall(b, h, row_a(22.0, 36.0, true), 26.0);
    shop(
        b,
        h,
        row_a(39.0, 49.0, true),
        pal::WALL_BLUE,
        pal::ROOF_RED,
        Some(45.0),
    );
    shop(
        b,
        h,
        row_a(52.0, 61.0, true),
        pal::WALL_TAN,
        pal::ROOF_GREY,
        None,
    );
    garage(b, h, row_a(63.0, 69.0, true), pal::BRICK, pal::CAR_A);

    // ---- back lanes
    laundry(b, h, 28.0, 16.2, 20.8, 2);
    laundry(b, h, 52.5, 16.2, 20.8, 0);
    laundry(b, h, 38.0, -20.8, -16.2, 1);
    laundry(b, h, 62.0, -20.8, -16.2, 3);
    cart(b, h, 45.0, 18.6, true, 3);
    cart(b, h, 30.0, -18.5, true, 1);
    dumpster(b, h, 64.0, 19.9);
    dumpster(b, h, 50.0, -19.9);
    fence_x(b, h, 33.0, 37.0, 18.5);
    fence_x(b, h, 55.0, 59.0, -18.5);
    for (x, z, s) in [(40.0, 20.0, 1.2), (24.0, -17.0, 1.0), (25.1, -17.1, 0.8)] {
        let (px, pz) = h.p(x, z);
        b.crate_(px, pz, s, 0.0);
    }

    // ---- back rows: shops, the site, a long warehouse
    shop(
        b,
        h,
        row_b(22.0, 34.0, false, 10.0),
        pal::WALL_WHITE,
        pal::ROOF_RED,
        Some(30.0),
    );
    let site = courtyard(b, h, 39.0, 51.0, 22.5, 33.5, pal::PAD_ALLIES);
    house(
        b,
        h,
        row_b(55.0, 67.0, false, 12.0),
        pal::WALL_BLUE,
        pal::ROOF_GREY,
        false,
    );
    warehouse(b, h, row_b(22.0, 40.0, true, 13.0), pal::WALL_GREEN);
    shop(
        b,
        h,
        row_b(44.0, 54.0, true, 10.0),
        pal::WALL_PINK,
        pal::ROOF_GREY,
        Some(49.0),
    );
    house(
        b,
        h,
        row_b(57.0, 68.0, true, 12.0),
        pal::WALL_TAN,
        pal::ROOF_RED,
        false,
    );

    // ---- behind the square: a little park and a newsstand
    for (x, z) in [(5.0, 27.0), (12.0, 31.0), (16.0, 25.0)] {
        let (px, pz) = h.p(x, z);
        b.tree(px, pz, false);
    }
    h.wood(b, 7.0, 0.0, 27.5, 10.0, 0.5, 28.1, pal::WOOD);
    h.wood(b, 11.0, 0.0, 24.0, 14.0, 0.5, 24.6, pal::WOOD);
    kiosk(b, h, 9.0, -27.0, pal::WALL_PINK);
    for (x, z) in [(14.0, -30.0), (4.0, -24.0)] {
        let (px, pz) = h.p(x, z);
        b.crate_(px, pz, 1.2, 0.0);
    }

    // ---- outer strips: a car wreck lot one side, parked cars the other
    for (x, z, c) in [
        (30.0, 39.5, pal::CAR_C),
        (44.0, 39.5, pal::CAR_A),
        (60.0, 39.5, pal::CAR_B),
    ] {
        let (px, pz) = h.p(x, z);
        b.car(px, pz, false, c);
    }
    for (x, z, c) in [(28.0, -39.0, pal::CONT_BLUE), (50.0, -39.0, pal::CONT_RED)] {
        let (px, pz) = h.p(x, z);
        b.container(px, pz, true, 0.0, c);
    }
    let (px, pz) = h.p(40.0, -40.5);
    b.burnt_car(px, pz, true);
    let (px, pz) = h.p(62.0, -40.0);
    b.tree(px, pz, false);

    // ---- lorry depot: the team's yard
    lorry(b, h, 77.0, -14.0, pal::CONT_RED);
    lorry(b, h, 77.0, 12.0, pal::CONT_BLUE);
    let (px, pz) = h.p(76.0, 30.0);
    b.container(px, pz, false, 0.0, pal::CONT_GREEN);
    let (px, pz) = h.p(76.0, -30.0);
    b.container(px, pz, false, 0.0, pal::CONT_YELLOW);
    for z in [-4.0f32, 22.0] {
        let (px, pz) = h.p(71.0, z);
        b.barrier(px, pz, false);
    }
    h.decal(b, 70.0, -44.0, 84.0, 44.0, 0.0, pal::CONCRETE_DARK);
    site
}

pub fn market() -> Map {
    let min = vec2(-HALF_X, -HALF_Z);
    let max = vec2(HALF_X, HALF_Z);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::DIRT, pal::WALL_TAN, 7.0);
    // street, sidewalks, the square's paving, the back lanes
    b.decal(-HALF_X, -5.0, HALF_X, 5.0, 0.0, pal::ASPHALT);
    b.decal(-HALF_X, -6.5, HALF_X, -5.0, 0.01, pal::CONCRETE);
    b.decal(-HALF_X, 5.0, HALF_X, 6.5, 0.01, pal::CONCRETE);
    b.decal(-20.0, -16.0, 20.0, -6.5, 0.0, STONE);
    b.decal(-20.0, 6.5, 20.0, 16.0, 0.0, STONE);
    for s in [-1.0f32, 1.0] {
        let (z0, z1) = (s * LANE.0, s * LANE.1);
        b.decal(-70.0, z0.min(z1), 70.0, z0.max(z1), 0.0, pal::SAND);
    }
    let site_east = east(&mut b);
    let site_west = west(&mut b);

    // allies start in the west, axis in the east, spread across their yards
    let zs = [-32.0, -23.0, -14.0, -5.0, 5.0, 14.0, 23.0, 32.0];
    let allies = zs.iter().map(|&z| spawn(-80.0, z, -FRAC_PI_2)).collect();
    let axis = zs.iter().map(|&z| spawn(80.0, z, FRAC_PI_2)).collect();

    Map {
        id: MapId::Market,
        name: "Ринок",
        min,
        max,
        boxes: b.boxes,
        ladders: b.ladders,
        spawns: [allies, axis],
        bomb: vec3(0.0, 0.0, 0.0),
        // each team defends the courtyard on its own side
        sites: [site_west, site_east],
        // a golden evening: peach sky, warm low sun
        sky: [250, 190, 140],
        fog: [250, 214, 176],
        tint: [1.08, 0.97, 0.86],
        terrain: None,
        loot: Vec::new(),
        nav_detail: Vec::new(),
    }
}

fn spawn(x: f32, z: f32, yaw: f32) -> Spawn {
    Spawn {
        pos: vec3(x, 0.0, z),
        yaw,
    }
}
