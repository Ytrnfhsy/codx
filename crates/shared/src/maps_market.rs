//! «Ринок» — a market district, 200 × 104 m.
//!
//! A main street runs west–east through a market square full of stalls and carts. Rows of
//! shops and houses line it, with alleys between them; behind them run back lanes hung
//! with laundry, then warehouses, apartment blocks and courtyards (the Sabotage sites),
//! and parking strips along the outer walls. The teams start at bus stations at either
//! end. The map is point-symmetric (east half turned round = west half), so both sides
//! get the same streets.
//!
//! Awnings, sheets and canopies are cloth and stall boards, carts and plank fences are
//! wood: bullets go through both. One-storey shops and warehouses have ladders up to
//! their flat roofs.

use crate::map::{door, pal, wide_door, window, Map, MapBuilder, MapId, Opening, Side, Spawn};
use crate::props::Tower;
use glam::{vec2, vec3, Vec3};
use std::f32::consts::FRAC_PI_2;

const HALF_X: f32 = 100.0;
const HALF_Z: f32 = 52.0;

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

/// The east half (x > 0); the west half is the same turned round.
fn half(b: &mut MapBuilder, h: Half) -> Vec3 {
    let east = h.s > 0.0;
    // ---- market square (x 0..24): stalls, carts, a fountain
    for (i, &x) in [5.0f32, 11.0, 17.0].iter().enumerate() {
        for (j, &z) in [10.0f32, 15.5, -10.0, -15.5].iter().enumerate() {
            stall(b, h, x, z, true, i * 4 + j + if east { 0 } else { 2 });
        }
    }
    fountain(b, h, 0.5, 12.8);
    cart(b, h, 8.0, 7.6, true, 1);
    cart(b, h, 21.0, -7.4, true, 3);
    cart(b, h, 22.5, 12.8, false, 4);
    for (x, z) in [(14.0, 18.0), (2.5, -18.2)] {
        let (px, pz) = h.p(x, z);
        b.crate_(px, pz, 1.0, 0.0);
    }
    b.tree(h.x(23.0), h.x(17.5), false);
    b.tree(h.x(23.0), h.x(-17.5), false);

    // ---- main street (z −6..6): parked cars, a kiosk at the square's corner
    for (x, z, c) in [
        (30.0, 4.6, pal::CAR_A),
        (47.0, -4.6, pal::CAR_C),
        (58.0, 4.6, pal::CAR_B),
        (74.0, -4.6, pal::CAR_A),
        (79.0, 4.6, pal::CAR_C),
    ] {
        let (px, pz) = h.p(x, z);
        b.car(px, pz, true, c);
    }
    kiosk(b, h, 25.5, 5.0, pal::WALL_BLUE);
    cart(b, h, 38.0, -5.0, true, 2);

    // ---- row A (z 7.5..19 and −19..−7.5): shops and houses facing the street
    let south_a = |x0, x1| Lot {
        x0,
        x1,
        front: 7.5,
        back: 19.0,
    };
    let north_a = |x0, x1| Lot {
        x0,
        x1,
        front: -7.5,
        back: -19.0,
    };
    shop(
        b,
        h,
        south_a(26.0, 38.0),
        pal::WALL_PINK,
        pal::ROOF_RED,
        Some(30.0),
    );
    house(
        b,
        h,
        south_a(42.0, 54.0),
        pal::WALL_BLUE,
        pal::ROOF_GREY,
        false,
    );
    shop(
        b,
        h,
        south_a(58.0, 70.0),
        pal::WALL_GREEN,
        pal::ROOF_RED,
        Some(66.0),
    );
    shop(
        b,
        h,
        south_a(74.0, 84.0),
        pal::WALL_TAN,
        pal::ROOF_GREY,
        None,
    );
    house(
        b,
        h,
        north_a(26.0, 40.0),
        pal::WALL_WHITE,
        pal::ROOF_RED,
        true,
    );
    shop(
        b,
        h,
        north_a(44.0, 56.0),
        pal::WALL_TAN,
        pal::ROOF_GREY,
        Some(48.0),
    );
    shop(
        b,
        h,
        north_a(60.0, 72.0),
        pal::WALL_PINK,
        pal::ROOF_RED,
        Some(68.0),
    );
    shop(
        b,
        h,
        north_a(76.0, 84.0),
        pal::WALL_BLUE,
        pal::ROOF_GREY,
        None,
    );

    // ---- back lanes (z 19..25 and −25..−19): laundry, carts, fences, bins
    laundry(b, h, 33.0, 19.2, 24.8, 0);
    laundry(b, h, 63.0, 19.2, 24.8, 1);
    laundry(b, h, 47.0, -24.8, -19.2, 2);
    laundry(b, h, 77.0, -24.8, -19.2, 3);
    cart(b, h, 40.0, 21.2, true, 0);
    cart(b, h, 70.0, -22.6, true, 5);
    dumpster(b, h, 55.0, 23.6);
    dumpster(b, h, 28.0, -23.6);
    dumpster(b, h, 82.0, 20.6);
    fence_x(b, h, 72.0, 76.5, 22.0);
    fence_x(b, h, 51.0, 55.0, -22.0);
    for (x, z, s) in [
        (36.0, 23.8, 1.2),
        (37.2, 23.9, 0.9),
        (60.0, -20.0, 1.1),
        (84.0, -23.5, 1.2),
    ] {
        let (px, pz) = h.p(x, z);
        b.crate_(px, pz, s, 0.0);
    }

    // ---- row B (z 25..43 and −43..−25): warehouses, apartments, a courtyard site
    warehouse(
        b,
        h,
        Lot {
            x0: 26.0,
            x1: 44.0,
            front: 25.0,
            back: 41.0,
        },
        pal::WALL_TAN,
    );
    let site = courtyard(
        b,
        h,
        48.0,
        62.0,
        27.0,
        40.0,
        if east { pal::PAD_AXIS } else { pal::PAD_ALLIES },
    );
    house(
        b,
        h,
        Lot {
            x0: 66.0,
            x1: 80.0,
            front: 25.0,
            back: 37.0,
        },
        pal::WALL_GREEN,
        pal::ROOF_RED,
        true,
    );
    shop(
        b,
        h,
        Lot {
            x0: 26.0,
            x1: 38.0,
            front: -25.0,
            back: -37.0,
        },
        pal::WALL_BLUE,
        pal::ROOF_RED,
        Some(34.0),
    );
    warehouse(
        b,
        h,
        Lot {
            x0: 44.0,
            x1: 62.0,
            front: -25.0,
            back: -41.0,
        },
        pal::BRICK,
    );
    house(
        b,
        h,
        Lot {
            x0: 66.0,
            x1: 78.0,
            front: -25.0,
            back: -37.0,
        },
        pal::WALL_PINK,
        pal::ROOF_GREY,
        false,
    );
    shop(
        b,
        h,
        Lot {
            x0: 80.0,
            x1: 86.0,
            front: -25.0,
            back: -35.0,
        },
        pal::WALL_WHITE,
        pal::ROOF_RED,
        None,
    );

    // ---- parking strips along the outer walls
    for (x, z, c, along) in [
        (30.0, 47.5, pal::CAR_B, false),
        (36.0, 47.5, pal::CAR_C, false),
        (54.0, 47.5, pal::CAR_A, false),
        (72.0, 47.0, pal::CAR_B, true),
        (40.0, -47.5, pal::CAR_A, false),
        (66.0, -47.5, pal::CAR_C, false),
    ] {
        let (px, pz) = h.p(x, z);
        b.car(px, pz, along, c);
    }
    let (px, pz) = h.p(84.0, -47.0);
    b.container(px, pz, true, 0.0, pal::CONT_ORANGE);
    for (x, z) in [(46.0, 49.5), (62.0, -49.5), (22.0, 46.0), (22.0, -46.0)] {
        b.tree(h.x(x), h.x(z), false);
    }
    // the square's back ends: low walls with planters
    h.block(b, 4.0, 0.0, 30.0, 20.0, 0.9, 31.0, STONE);
    h.block(b, 4.0, 0.0, -31.0, 20.0, 0.9, -30.0, STONE);
    dumpster(b, h, 12.0, 38.0);
    cart(b, h, 10.0, -36.0, false, 2);

    // ---- bus station at the end (x 86..100): the team's spawn
    bus(b, h, 92.0, -14.0);
    bus(b, h, 92.0, 16.0);
    kiosk(b, h, 90.0, 33.0, pal::WALL_WHITE);
    for z in [-30.0f32, 0.0, 30.0] {
        let (px, pz) = h.p(87.0, z + 4.0);
        b.barrier(px, pz, false);
    }
    h.decal(b, 86.0, -52.0, 100.0, 52.0, 0.0, pal::ASPHALT);
    site
}

pub fn market() -> Map {
    let min = vec2(-HALF_X, -HALF_Z);
    let max = vec2(HALF_X, HALF_Z);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::DIRT, pal::WALL_TAN, 7.0);
    // street, sidewalks and the square's paving
    b.decal(-HALF_X, -6.0, HALF_X, 6.0, 0.0, pal::ASPHALT);
    b.decal(-HALF_X, -7.5, HALF_X, -6.0, 0.01, pal::CONCRETE);
    b.decal(-HALF_X, 6.0, HALF_X, 7.5, 0.01, pal::CONCRETE);
    b.decal(-24.0, -19.0, 24.0, -7.5, 0.0, STONE);
    b.decal(-24.0, 7.5, 24.0, 19.0, 0.0, STONE);
    for z in [-1.0f32, 1.0] {
        let z0 = z * 21.5;
        b.decal(-86.0, z0 - 3.0, 86.0, z0 + 3.0, 0.0, pal::SAND);
    }
    let site_east = half(&mut b, Half { s: 1.0 });
    let site_west = half(&mut b, Half { s: -1.0 });

    // allies start in the west, axis in the east, all across the bus station
    let zs = [-38.0, -27.0, -16.0, -5.0, 5.0, 16.0, 27.0, 38.0];
    let allies = zs.iter().map(|&z| spawn(-96.0, z, -FRAC_PI_2)).collect();
    let axis = zs.iter().map(|&z| spawn(96.0, -z, FRAC_PI_2)).collect();

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
        sky: [250, 196, 150],
        fog: [252, 220, 186],
    }
}

fn spawn(x: f32, z: f32, yaw: f32) -> Spawn {
    Spawn {
        pos: vec3(x, 0.0, z),
        yaw,
    }
}
