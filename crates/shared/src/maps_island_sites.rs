//! The Island's landmarks: what makes one place unlike another. A church and a town hall
//! with a clock in the old town, a market square, a works with a chimney, a harbour with
//! piers, ranches with corrals, a sawmill, a mine, a radio mast on the peak, a campsite by
//! the lake, a ruined castle on a hill, a lighthouse, a filling station, parks — and a
//! few things nobody expects to find.
//!
//! Every builder works in the place's own level (the caller lifts it onto the ground) and
//! leaves loot where a player would look for it.

use crate::map::{door, pal, shade, wide_door, window, MapBuilder, Side};
use crate::props::Tower;
use crate::rng::Rng;
use glam::vec3;

const STONE: [u8; 3] = [168, 164, 156];
const STONE_DARK: [u8; 3] = [128, 124, 118];
const PLANK: [u8; 3] = [150, 108, 70];
const PLANK_DARK: [u8; 3] = [112, 80, 54];
const RED_BARN: [u8; 3] = [168, 70, 56];
const WHITE: [u8; 3] = [240, 238, 232];
const GOLD: [u8; 3] = [236, 196, 64];
const WATER_BLUE: [u8; 3] = [86, 150, 206];

/// A rectangle on the ground (x0, z0, x1, z1).
#[derive(Clone, Copy, Debug)]
pub struct Lot {
    pub x0: f32,
    pub z0: f32,
    pub x1: f32,
    pub z1: f32,
}

impl Lot {
    pub fn cx(&self) -> f32 {
        (self.x0 + self.x1) * 0.5
    }
    pub fn cz(&self) -> f32 {
        (self.z0 + self.z1) * 0.5
    }
    pub fn w(&self) -> f32 {
        self.x1 - self.x0
    }
    pub fn d(&self) -> f32 {
        self.z1 - self.z0
    }
    pub fn centered(cx: f32, cz: f32, w: f32, d: f32) -> Lot {
        Lot {
            x0: cx - w * 0.5,
            z0: cz - d * 0.5,
            x1: cx + w * 0.5,
            z1: cz + d * 0.5,
        }
    }
}

// ---------------------------------------------------------------- small things

/// A park bench.
pub fn bench(b: &mut MapBuilder, x: f32, z: f32, along_x: bool) {
    let (w, d) = if along_x { (1.8, 0.5) } else { (0.5, 1.8) };
    b.wood(
        x - w / 2.0,
        0.4,
        z - d / 2.0,
        x + w / 2.0,
        0.5,
        z + d / 2.0,
        PLANK,
    );
    for s in [-0.7f32, 0.7] {
        let (px, pz) = if along_x { (x + s, z) } else { (x, z + s) };
        b.push_detail(
            px - 0.06,
            0.0,
            pz - 0.2,
            px + 0.06,
            0.4,
            pz + 0.2,
            pal::DARK,
        );
    }
}

/// A street lamp (a post that falls over when blown at).
pub fn lamp(b: &mut MapBuilder, x: f32, z: f32) {
    b.cbox(x, z, 0.16, 0.16, 0.0, 3.6, pal::DARK);
    b.push_detail(x - 0.3, 3.6, z - 0.3, x + 0.3, 3.9, z + 0.3, pal::LIGHT);
}

/// A run of wooden fence (posts and two rails) from a to b (along x or z).
pub fn fence(b: &mut MapBuilder, x0: f32, z0: f32, x1: f32, z1: f32, c: [u8; 3]) {
    let along_x = (x1 - x0).abs() >= (z1 - z0).abs();
    let (a, e) = if along_x {
        (x0.min(x1), x0.max(x1))
    } else {
        (z0.min(z1), z0.max(z1))
    };
    let line = if along_x { z0 } else { x0 };
    let mut t = a;
    while t <= e + 0.01 {
        if along_x {
            b.cbox(t, line, 0.16, 0.16, 0.0, 1.25, shade(c, 0.85));
        } else {
            b.cbox(line, t, 0.16, 0.16, 0.0, 1.25, shade(c, 0.85));
        }
        t += 2.5;
    }
    for y in [0.45f32, 0.95] {
        if along_x {
            b.wood(a, y, line - 0.05, e, y + 0.12, line + 0.05, c);
        } else {
            b.wood(line - 0.05, y, a, line + 0.05, y + 0.12, e, c);
        }
    }
}

/// A market stall: a counter, four posts and a striped awning.
pub fn stall(b: &mut MapBuilder, x: f32, z: f32, awning: [u8; 3], rng: &mut Rng) {
    b.wood(x - 1.4, 0.0, z - 0.5, x + 1.4, 0.95, z + 0.5, PLANK);
    for (dx, dz) in [(-1.4, -0.9), (1.4, -0.9), (-1.4, 0.9), (1.4, 0.9)] {
        b.push_detail(
            x + dx - 0.06,
            0.0,
            z + dz - 0.06,
            x + dx + 0.06,
            2.3,
            z + dz + 0.06,
            PLANK_DARK,
        );
    }
    b.cloth(x - 1.6, 2.3, z - 1.1, x + 1.6, 2.42, z + 1.1, awning);
    b.cloth(x - 1.6, 2.42, z - 0.4, x + 1.6, 2.5, z + 0.4, WHITE);
    // what is for sale: fruit, fish, pots
    let goods = [
        [230, 80, 60],
        [250, 180, 60],
        [120, 190, 80],
        [200, 120, 200],
        [150, 190, 220],
    ];
    for k in 0..3 {
        let c = goods[rng.below(goods.len())];
        let gx = x - 0.9 + k as f32 * 0.9;
        b.push_detail(gx - 0.35, 0.95, z - 0.3, gx + 0.35, 1.2, z + 0.3, c);
    }
}

/// A hay bale.
fn bale(b: &mut MapBuilder, x: f32, z: f32, y: f32) {
    b.cbox(x, z, 1.5, 1.1, y, 1.0, [226, 198, 110]);
}

/// A log lying along x or z.
fn log(b: &mut MapBuilder, x: f32, z: f32, y: f32, len: f32, along_x: bool) {
    let (w, d) = if along_x { (len, 0.55) } else { (0.55, len) };
    b.cbox(x, z, w, d, y, 0.55, pal::TRUNK);
}

/// A rowing boat or a fishing boat lying on something.
fn boat(b: &mut MapBuilder, x: f32, z: f32, y: f32, along_x: bool, len: f32, c: [u8; 3]) {
    let (w, d) = if along_x {
        (len, len * 0.36)
    } else {
        (len * 0.36, len)
    };
    b.cbox(x, z, w, d, y, 0.5, c);
    let (iw, id) = (w - 0.3, d - 0.3);
    b.cbox(x, z, iw * 0.92, id * 0.92, y + 0.5, 0.35, shade(c, 0.8));
    let (tw, td) = if along_x {
        (len * 0.2, d)
    } else {
        (w, len * 0.2)
    };
    b.cbox(x, z, tw, td, y + 0.5, 0.45, WHITE);
}

// ---------------------------------------------------------------- the old town

/// A church: a tall nave and a bell tower with a ladder to the belfry (a sniper's nest
/// everyone can see).
pub fn church(b: &mut MapBuilder, l: Lot) {
    let wall = [232, 226, 206];
    let (nx0, nx1) = (l.cx() - 6.0, l.cx() + 6.0);
    let (nz0, nz1) = (l.z0 + 6.0, (l.z0 + 28.0).min(l.z1 - 2.0));
    let ops = [
        (Side::MinZ, door(l.cx())),
        (Side::MaxZ, door(l.cx() + 3.0)),
        (Side::MinX, window(nz0 + 5.0)),
        (Side::MinX, window(nz0 + 11.0)),
        (Side::MinX, window(nz0 + 17.0)),
        (Side::MaxX, window(nz0 + 5.0)),
        (Side::MaxX, window(nz0 + 11.0)),
        (Side::MaxX, window(nz0 + 17.0)),
    ];
    b.building(
        nx0,
        nz0,
        nx1,
        nz1,
        0.0,
        7.0,
        &ops,
        wall,
        Some(pal::ROOF_RED),
    );
    // a stepped pitched roof
    for (k, inset) in [2.0f32, 4.0].iter().enumerate() {
        b.block(
            nx0 + inset,
            7.35 + k as f32 * 0.9,
            nz0 - 0.2,
            nx1 - inset,
            8.25 + k as f32 * 0.9,
            nz1 + 0.2,
            shade(pal::ROOF_RED, 0.95 - k as f32 * 0.05),
        );
    }
    // pews and the altar
    for k in 0..5 {
        let z = nz0 + 4.0 + k as f32 * 2.6;
        for s in [-1.0f32, 1.0] {
            b.wood(
                l.cx() + s * 3.2 - 1.8,
                0.0,
                z,
                l.cx() + s * 3.2 + 1.8,
                0.8,
                z + 0.5,
                PLANK_DARK,
            );
        }
    }
    b.cbox(l.cx(), nz1 - 2.4, 3.0, 1.2, 0.0, 1.1, WHITE);
    b.push_detail(
        l.cx() - 0.08,
        1.1,
        nz1 - 2.5,
        l.cx() + 0.08,
        2.4,
        nz1 - 2.3,
        GOLD,
    );
    b.push_detail(
        l.cx() - 0.45,
        1.85,
        nz1 - 2.5,
        l.cx() + 0.45,
        2.0,
        nz1 - 2.3,
        GOLD,
    );
    b.loot.push(vec3(l.cx(), 0.3, nz1 - 4.0));
    b.loot.push(vec3(l.cx() - 3.0, 0.3, nz0 + 3.0));
    // the bell tower before the west front: solid up to the belfry
    let (tx, tz) = (nx0 - 2.6, nz0 + 3.0);
    let top = 13.0;
    b.cbox(tx, tz, 4.4, 4.4, 0.0, top, wall);
    b.cbox(tx, tz, 4.8, 4.8, top, 0.3, STONE);
    // the belfry: four pillars, a bell, a roof and a spire
    for (dx, dz) in [(-1.9, -1.9), (1.9, -1.9), (-1.9, 1.9), (1.9, 1.9)] {
        b.cbox(tx + dx, tz + dz, 0.6, 0.6, top + 0.3, 3.0, wall);
    }
    b.push_detail(
        tx - 0.6,
        top + 1.4,
        tz - 0.6,
        tx + 0.6,
        top + 2.6,
        tz + 0.6,
        GOLD,
    );
    b.cbox(tx, tz, 5.0, 5.0, top + 3.3, 0.4, pal::ROOF_GREY);
    for (k, w) in [3.6f32, 2.6, 1.6, 0.8].iter().enumerate() {
        b.cbox(
            tx,
            tz,
            *w,
            *w,
            top + 3.7 + k as f32 * 1.2,
            1.2,
            shade(pal::ROOF_GREY, 1.0 - k as f32 * 0.04),
        );
    }
    b.push_detail(
        tx - 0.08,
        top + 8.5,
        tz - 0.08,
        tx + 0.08,
        top + 10.0,
        tz + 0.08,
        GOLD,
    );
    b.push_detail(
        tx - 0.45,
        top + 9.3,
        tz - 0.08,
        tx + 0.45,
        top + 9.45,
        tz + 0.08,
        GOLD,
    );
    // a rail round the belfry floor, and the ladder up the west face
    b.wood(
        tx - 2.4,
        top + 0.3,
        tz - 2.4,
        tx + 2.4,
        top + 1.2,
        tz - 2.25,
        PLANK_DARK,
    );
    b.wood(
        tx - 2.4,
        top + 0.3,
        tz + 2.25,
        tx + 2.4,
        top + 1.2,
        tz + 2.4,
        PLANK_DARK,
    );
    b.wood(
        tx + 2.25,
        top + 0.3,
        tz - 2.25,
        tx + 2.4,
        top + 1.2,
        tz + 2.25,
        PLANK_DARK,
    );
    b.ladder(vec3(tx - 2.2, 0.0, tz), vec3(-1.0, 0.0, 0.0), top + 0.3);
    b.loot.push(vec3(tx + 0.6, top + 0.6, tz + 0.6));
    // the churchyard: crosses in rows
    let mut z = nz0 + 2.0;
    while z < nz1 - 2.0 {
        for x in [nx1 + 2.5, nx1 + 5.0] {
            if x < l.x1 - 0.8 {
                b.cbox(x, z, 0.2, 0.2, 0.0, 1.2, STONE);
                b.push_detail(x - 0.4, 0.8, z - 0.1, x + 0.4, 0.95, z + 0.1, STONE);
            }
        }
        z += 3.0;
    }
}

/// A town hall: two storeys and a roof to hold, with a clock tower on top.
pub fn town_hall(b: &mut MapBuilder, l: Lot) {
    let wall = [226, 206, 168];
    let (x0, x1) = (l.cx() - 9.0, l.cx() + 9.0);
    let (z0, z1) = (l.z1 - 16.0, l.z1 - 2.0);
    let ground = [
        (Side::MinZ, door(l.cx() + 1.0)),
        (Side::MaxZ, door(l.cx() + 1.0)),
        (Side::MinZ, window(x0 + 3.0)),
        (Side::MinZ, window(x1 - 3.0)),
        (Side::MinX, window((z0 + z1) / 2.0)),
        (Side::MaxX, window((z0 + z1) / 2.0)),
    ];
    let upper = [
        (Side::MinZ, window(x0 + 3.0)),
        (Side::MinZ, window(l.cx())),
        (Side::MinZ, window(x1 - 3.0)),
        (Side::MaxZ, window(l.cx())),
        (Side::MinX, window((z0 + z1) / 2.0)),
        (Side::MaxX, window((z0 + z1) / 2.0)),
    ];
    b.tower(&Tower {
        x0,
        z0,
        x1,
        z1,
        floors: 2,
        storey: 3.6,
        color: wall,
        roof: pal::ROOF_GREY,
        ground: &ground,
        upper: &upper,
        roof_access: true,
    });
    for k in 0..3 {
        b.loot
            .push(vec3(l.cx() + 2.0, k as f32 * 3.6 + 0.4, (z0 + z1) / 2.0));
    }
    // columns by the door, steps up to it
    for dx in [-2.0f32, 2.0] {
        b.cbox(l.cx() + 1.0 + dx, z0 - 0.6, 0.6, 0.6, 0.0, 3.4, WHITE);
    }
    b.cbox(l.cx() + 1.0, z0 - 0.6, 5.4, 1.2, 3.4, 0.4, WHITE);
    // the clock tower: on the roof's west half (the stairs come up by the east wall)
    let (cx, cz) = (l.cx() - 3.5, (z0 + z1) / 2.0 - 1.0);
    let y = 7.2;
    b.cbox(cx, cz, 3.4, 3.4, y, 4.2, wall);
    for (n, (dx, dz)) in [(0.0f32, -1.75f32), (0.0, 1.75), (-1.75, 0.0), (1.75, 0.0)]
        .iter()
        .enumerate()
    {
        let (w, d) = if n < 2 { (2.2, 0.12) } else { (0.12, 2.2) };
        b.push_detail(
            cx + dx - w / 2.0,
            y + 1.8,
            cz + dz - d / 2.0,
            cx + dx + w / 2.0,
            y + 4.0,
            cz + dz + d / 2.0,
            WHITE,
        );
        // the hands: ten to two
        let (hw, hd) = if n < 2 { (0.1, 0.16) } else { (0.16, 0.1) };
        b.push_detail(
            cx + dx - hw,
            y + 2.9,
            cz + dz - hd,
            cx + dx + hw,
            y + 3.8,
            cz + dz + hd,
            pal::DARK,
        );
        let (sw, sd) = if n < 2 { (0.7, 0.16) } else { (0.16, 0.7) };
        b.push_detail(
            cx + dx - sw,
            y + 2.85,
            cz + dz - sd,
            cx + dx + sw * 0.2,
            y + 3.0,
            cz + dz + sd * 0.2,
            pal::DARK,
        );
    }
    b.cbox(cx, cz, 4.0, 4.0, y + 4.2, 0.3, pal::ROOF_GREY);
    b.cbox(cx, cz, 2.4, 2.4, y + 4.5, 1.0, pal::ROOF_GREY);
    b.cbox(cx, cz, 1.0, 1.0, y + 5.5, 1.0, shade(pal::ROOF_GREY, 0.9));
}

/// A statue on a plinth (the town's founder, looking rather pleased with himself).
pub fn statue(b: &mut MapBuilder, x: f32, z: f32) {
    let bronze = [92, 140, 120];
    b.cbox(x, z, 3.0, 3.0, 0.0, 0.5, STONE);
    b.cbox(x, z, 1.8, 1.8, 0.5, 1.4, STONE_DARK);
    b.cbox(x, z, 0.8, 0.5, 1.9, 1.2, bronze);
    b.cbox(x, z, 0.5, 0.5, 3.1, 0.55, bronze);
    // one arm pointing the way to the harbour
    b.push_detail(x + 0.4, 2.7, z - 0.12, x + 1.3, 2.9, z + 0.12, bronze);
    b.push_detail(
        x - 0.35,
        3.65,
        z - 0.35,
        x + 0.35,
        3.75,
        z + 0.35,
        shade(bronze, 0.8),
    );
}

/// A park: paths, trees, benches and lamps, a pond with a fountain and a band stand.
pub fn park(b: &mut MapBuilder, l: Lot, rng: &mut Rng, ducks: bool) {
    let path = [214, 200, 170];
    b.decal(l.cx() - 1.5, l.z0, l.cx() + 1.5, l.z1, 0.01, path);
    b.decal(l.x0, l.cz() - 1.5, l.x1, l.cz() + 1.5, 0.01, path);
    // the pond in one corner, with a rim to sit on
    let (px, pz) = (l.x0 + l.w() * 0.27, l.z0 + l.d() * 0.3);
    let (pw, pd) = ((l.w() * 0.3).min(14.0), (l.d() * 0.32).min(10.0));
    b.cbox(px, pz, pw + 0.8, 0.4, 0.0, 0.45, STONE);
    b.cbox(px, pz - pd / 2.0, pw + 0.8, 0.4, 0.0, 0.45, STONE);
    b.cbox(px, pz + pd / 2.0, pw + 0.8, 0.4, 0.0, 0.45, STONE);
    b.cbox(px - pw / 2.0, pz, 0.4, pd, 0.0, 0.45, STONE);
    b.cbox(px + pw / 2.0, pz, 0.4, pd, 0.0, 0.45, STONE);
    b.decal(
        px - pw / 2.0 + 0.2,
        pz - pd / 2.0 + 0.2,
        px + pw / 2.0 - 0.2,
        pz + pd / 2.0 - 0.2,
        0.06,
        WATER_BLUE,
    );
    b.cbox(px, pz, 1.2, 1.2, 0.0, 0.9, STONE_DARK);
    b.push_detail(
        px - 0.15,
        0.9,
        pz - 0.15,
        px + 0.15,
        1.8,
        pz + 0.15,
        [170, 210, 240],
    );
    if ducks {
        for k in 0..3 {
            let (dx, dz) = (
                px - pw * 0.3 + k as f32 * 1.1,
                pz + pd * 0.25 - k as f32 * 0.5,
            );
            b.push_detail(dx - 0.25, 0.08, dz - 0.15, dx + 0.25, 0.3, dz + 0.15, WHITE);
            b.push_detail(dx + 0.15, 0.3, dz - 0.08, dx + 0.3, 0.45, dz + 0.08, WHITE);
            b.push_detail(
                dx + 0.3,
                0.33,
                dz - 0.04,
                dx + 0.42,
                0.38,
                dz + 0.04,
                [240, 150, 40],
            );
        }
    }
    // the band stand (a roof to shoot from, if one climbs on the rail)
    let (gx, gz) = (l.x0 + l.w() * 0.75, l.z0 + l.d() * 0.72);
    b.cbox(gx, gz, 6.0, 6.0, 0.0, 0.5, STONE);
    for (dx, dz) in [(-2.6, -2.6), (2.6, -2.6), (-2.6, 2.6), (2.6, 2.6)] {
        b.cbox(gx + dx, gz + dz, 0.3, 0.3, 0.5, 2.8, WHITE);
    }
    b.wood(gx - 2.8, 0.5, gz - 2.8, gx + 2.8, 1.4, gz - 2.65, WHITE);
    b.wood(gx - 2.8, 0.5, gz + 2.65, gx + 2.8, 1.4, gz + 2.8, WHITE);
    b.cbox(gx, gz, 6.8, 6.8, 3.3, 0.3, [90, 140, 110]);
    b.cbox(gx, gz, 4.2, 4.2, 3.6, 0.6, shade([90, 140, 110], 0.92));
    b.cbox(gx, gz, 1.8, 1.8, 4.2, 0.6, shade([90, 140, 110], 0.85));
    b.loot.push(vec3(gx, 0.8, gz));
    // trees, benches and lamps along the paths
    for k in 0..10 {
        let x = rng.range(l.x0 + 3.0, l.x1 - 3.0);
        let z = rng.range(l.z0 + 3.0, l.z1 - 3.0);
        let clear = (x - l.cx()).abs() > 3.5
            && (z - l.cz()).abs() > 3.5
            && ((x - px).abs() > pw / 2.0 + 3.0 || (z - pz).abs() > pd / 2.0 + 3.0)
            && ((x - gx).abs() > 6.0 || (z - gz).abs() > 6.0);
        if clear {
            let s = rng.range(0.75, 1.05);
            b.cbox(x, z, 0.4 * s, 0.4 * s, 0.0, 4.6 * s, pal::TRUNK);
            let g = shade(pal::LEAVES, rng.range(0.9, 1.15));
            b.push_detail(
                x - 1.6 * s,
                2.0 * s,
                z - 1.6 * s,
                x + 1.6 * s,
                4.0 * s,
                z + 1.6 * s,
                g,
            );
            b.push_detail(
                x - 1.0 * s,
                4.0 * s,
                z - 1.0 * s,
                x + 1.0 * s,
                4.7 * s,
                z + 1.0 * s,
                shade(g, 1.08),
            );
        }
        if k < 4 {
            let along = k % 2 == 0;
            let t = if k < 2 { 0.25 } else { 0.75 };
            if along {
                bench(b, l.x0 + l.w() * t, l.cz() + 2.4, true);
                lamp(b, l.x0 + l.w() * t + 2.0, l.cz() - 2.2);
            } else {
                bench(b, l.cx() + 2.4, l.z0 + l.d() * t, false);
                lamp(b, l.cx() - 2.2, l.z0 + l.d() * t + 2.0);
            }
        }
    }
    b.loot.push(vec3(l.cx() + 2.0, 0.3, l.cz() + 2.0));
}

// ---------------------------------------------------------------- the market town

/// A market square: rows of stalls round a covered market hall.
pub fn market_square(b: &mut MapBuilder, l: Lot, rng: &mut Rng) {
    let paving = [206, 190, 160];
    b.decal(l.x0 + 1.0, l.z0 + 1.0, l.x1 - 1.0, l.z1 - 1.0, 0.01, paving);
    // the hall: a roof on pillars, open on every side
    let (hx, hz) = (l.cx(), l.cz());
    let (hw, hd) = ((l.w() * 0.45).min(20.0), (l.d() * 0.4).min(12.0));
    let mut x = hx - hw / 2.0;
    while x <= hx + hw / 2.0 + 0.01 {
        for z in [hz - hd / 2.0, hz + hd / 2.0] {
            b.cbox(x, z, 0.6, 0.6, 0.0, 4.2, STONE);
        }
        x += hw / 4.0;
    }
    b.cbox(hx, hz, hw + 1.6, hd + 1.6, 4.2, 0.35, [176, 90, 70]);
    b.cbox(
        hx,
        hz,
        hw - 2.0,
        hd - 3.0,
        4.55,
        0.9,
        shade([176, 90, 70], 0.92),
    );
    // stalls under it and round it
    let awnings = [
        pal::AWNING_A,
        pal::AWNING_B,
        [250, 200, 70],
        [120, 200, 120],
        [200, 130, 220],
    ];
    for k in 0..3 {
        let x = hx - hw / 3.0 + k as f32 * hw / 3.0;
        stall(b, x, hz, awnings[rng.below(awnings.len())], rng);
        b.loot.push(vec3(x, 0.3, hz + 1.6));
    }
    for k in 0..8 {
        let side = k % 4;
        let t = rng.range(0.15, 0.85);
        let (x, z) = match side {
            0 => (l.x0 + l.w() * t, l.z0 + 2.5),
            1 => (l.x0 + l.w() * t, l.z1 - 2.5),
            2 => (l.x0 + 2.8, l.z0 + l.d() * t),
            _ => (l.x1 - 2.8, l.z0 + l.d() * t),
        };
        if (x - hx).abs() < hw / 2.0 + 3.0 && (z - hz).abs() < hd / 2.0 + 3.0 {
            continue;
        }
        stall(b, x, z, awnings[rng.below(awnings.len())], rng);
    }
    for k in 0..5 {
        let x = rng.range(l.x0 + 2.0, l.x1 - 2.0);
        let z = rng.range(l.z0 + 2.0, l.z1 - 2.0);
        if (x - hx).abs() > hw / 2.0 + 2.0 || (z - hz).abs() > hd / 2.0 + 2.0 {
            b.crate_(x, z, 0.8, 0.0);
            if k % 2 == 0 {
                b.loot.push(vec3(x + 1.0, 0.3, z));
            }
        }
    }
}

// ---------------------------------------------------------------- the works

/// A works: a big shed with a gallery inside, a brick chimney with a ladder to a ring
/// high up, two silos joined to the shed's roof by a walkway.
pub fn works(b: &mut MapBuilder, l: Lot) {
    let wall = [178, 170, 156];
    let (x0, x1) = (l.x0 + 2.0, l.x0 + 2.0 + (l.w() * 0.62).min(30.0));
    let (z0, z1) = (l.z0 + 3.0, l.z1 - 3.0);
    let h = 7.5;
    let ops = [
        (Side::MinZ, wide_door((x0 + x1) / 2.0)),
        (Side::MaxZ, wide_door(x0 + 6.0)),
        (Side::MinX, door((z0 + z1) / 2.0)),
        (Side::MaxX, door(z0 + 4.0)),
        (Side::MinZ, crate::map::window(x0 + 4.0)),
        (Side::MinZ, crate::map::window(x1 - 4.0)),
    ];
    b.building(x0, z0, x1, z1, 0.0, h, &ops, wall, Some(pal::ROOF_GREY));
    // saw-tooth skylights
    let mut x = x0 + 3.0;
    while x < x1 - 3.0 {
        b.block(
            x,
            h + 0.35,
            z0 + 1.0,
            x + 2.0,
            h + 1.4,
            z1 - 1.0,
            shade(pal::ROOF_GREY, 0.88),
        );
        x += 5.0;
    }
    // inside: machines, a gallery along the north wall with stairs up
    for k in 0..3 {
        let mx = x0 + 5.0 + k as f32 * ((x1 - x0 - 10.0) / 2.0);
        b.cbox(
            mx,
            (z0 + z1) / 2.0 + 1.0,
            3.0,
            2.0,
            0.0,
            1.8,
            [90, 120, 150],
        );
        b.cbox(
            mx,
            (z0 + z1) / 2.0 + 1.0,
            1.0,
            1.0,
            1.8,
            1.2,
            [200, 160, 60],
        );
        b.loot.push(vec3(mx, 0.3, (z0 + z1) / 2.0 - 1.4));
    }
    let gy = 3.8;
    b.block(
        x0 + 0.3,
        gy - 0.25,
        z1 - 3.3,
        x1 - 0.3,
        gy,
        z1 - 0.3,
        pal::METAL,
    );
    b.wood(
        x0 + 4.0,
        gy,
        z1 - 3.4,
        x1 - 0.3,
        gy + 1.0,
        z1 - 3.3,
        pal::METAL,
    );
    {
        // the flight up to the gallery, rising northwards to its edge
        let steps = (gy / 0.34).round() as u32;
        let rise = gy / steps as f32;
        let z_start = z1 - 3.3 - steps as f32 * 0.62;
        b.stairs(
            x0 + 1.6,
            z_start,
            (0.0, 1.0),
            1.6,
            steps,
            rise,
            0.62,
            0.0,
            pal::CONCRETE_DARK,
        );
    }
    b.loot.push(vec3(x1 - 3.0, gy + 0.3, z1 - 1.8));
    // the chimney east of the shed: a ladder to a ring at 16 m
    let (cx, cz) = (x1 + 4.0, z0 + 4.0);
    b.cbox(cx, cz, 2.8, 2.8, 0.0, 24.0, pal::BRICK);
    for y in [8.0f32, 16.0, 22.0] {
        b.cbox(cx, cz, 3.1, 3.1, y, 0.35, shade(pal::BRICK, 0.8));
    }
    b.cbox(cx, cz, 5.6, 5.6, 16.0, 0.3, pal::METAL);
    b.wood(
        cx - 2.8,
        16.3,
        cz - 2.8,
        cx + 2.8,
        17.2,
        cz - 2.65,
        pal::METAL,
    );
    b.wood(
        cx - 2.8,
        16.3,
        cz + 2.65,
        cx + 2.8,
        17.2,
        cz + 2.8,
        pal::METAL,
    );
    b.wood(
        cx - 2.8,
        16.3,
        cz - 2.65,
        cx - 2.65,
        17.2,
        cz + 2.65,
        pal::METAL,
    );
    // (a caged ladder standing at the ring's edge on the far side from the shed, clear of
    // its door: it comes out onto the ring)
    b.ladder(vec3(cx + 2.8, 0.0, cz), vec3(1.0, 0.0, 0.0), 16.3);
    b.push_detail(
        cx - 1.0,
        24.0,
        cz - 1.0,
        cx + 1.0,
        24.3,
        cz + 1.0,
        pal::DARK,
    );
    b.loot.push(vec3(cx + 2.0, 16.6, cz + 2.0));
    // silos, with a ladder each and a walkway to the shed's roof
    for (k, sz) in [z1 - 6.0, z1 - 13.0].iter().enumerate() {
        let sx = x1 + 6.0;
        if sx + 3.0 > l.x1 || *sz - 3.0 < cz + 3.0 {
            continue;
        }
        b.cbox(sx, *sz, 5.0, 5.0, 0.0, 10.0, [210, 210, 214]);
        b.cbox(sx, *sz, 4.0, 4.0, 10.0, 1.0, [190, 190, 196]);
        b.cbox(sx, *sz, 2.0, 2.0, 11.0, 0.6, [170, 170, 176]);
        b.push_detail(
            sx - 2.6,
            3.0 + k as f32 * 2.0,
            *sz - 2.6,
            sx + 2.6,
            3.4 + k as f32 * 2.0,
            *sz + 2.6,
            pal::RUST,
        );
        b.ladder(vec3(sx + 2.5, 0.0, *sz), vec3(1.0, 0.0, 0.0), 11.0);
        if k == 0 {
            b.block(
                x1 + 0.2,
                h + 0.35,
                *sz - 0.8,
                sx - 2.5,
                h + 0.6,
                *sz + 0.8,
                pal::METAL,
            );
        }
    }
    // a yard of pallets, barrels and a lorry
    let yz = z0 - 0.0;
    for k in 0..4 {
        b.barrel(
            x1 + 3.0 + k as f32 * 1.0,
            yz + 9.0,
            if k % 2 == 0 {
                pal::BARREL_BLUE
            } else {
                pal::BARREL_RED
            },
        );
    }
    b.brick_pallet(x0 + 4.0, z1 + 1.6);
}

// ---------------------------------------------------------------- the harbour

/// The harbour: warehouses on the quay, piers out over the sea, boats, a crane.
/// `pier_len(x)`: how far out (south of the quay) a pier at `x` must run to reach water
/// deep enough for boats; `water`: the sea's level in the quay's own (the quay stands a
/// little above it).
pub fn harbour(
    b: &mut MapBuilder,
    l: Lot,
    pier_len: &dyn Fn(f32) -> f32,
    water: f32,
    rng: &mut Rng,
) {
    let quay = [176, 170, 160];
    b.decal(l.x0, l.z0, l.x1, l.z1, 0.01, quay);
    // two warehouses on the quay
    for (k, x) in [l.x0 + 4.0, l.x0 + 26.0].iter().enumerate() {
        let (x0, x1) = (*x, *x + 18.0);
        let (z0, z1) = (l.z0 + 3.0, l.z0 + 17.0);
        let c = if k == 0 {
            [150, 92, 70]
        } else {
            [110, 130, 150]
        };
        let ops = [
            (Side::MaxZ, wide_door((x0 + x1) / 2.0)),
            (Side::MinZ, door(x0 + 3.0)),
            (Side::MinX, crate::map::window((z0 + z1) / 2.0)),
        ];
        b.building(x0, z0, x1, z1, 0.0, 5.5, &ops, c, Some(pal::ROOF_GREY));
        b.block(
            x0 + 1.0,
            5.85,
            z0 - 0.2,
            x1 - 1.0,
            6.6,
            z1 + 0.2,
            shade(pal::ROOF_GREY, 0.9),
        );
        for j in 0..3 {
            let cx = x0 + 3.5 + j as f32 * 5.0;
            b.crate_(cx, z0 + 4.0, 1.2, 0.0);
            if j != 1 {
                b.crate_(cx, z0 + 4.0, 1.0, 1.2);
            }
            b.loot.push(vec3(cx, 0.3, z0 + 7.0));
        }
    }
    // the crane by the quay's edge
    b.crane(l.x1 - 10.0, l.z0 + 8.0, 16.0);
    // fish crates, barrels and nets
    for k in 0..6 {
        let x = rng.range(l.x0 + 3.0, l.x1 - 3.0);
        let z = rng.range(l.z1 - 10.0, l.z1 - 3.0);
        b.cbox(x, z, 1.2, 0.8, 0.0, 0.5, [100, 140, 170]);
        if k % 2 == 0 {
            b.loot.push(vec3(x + 1.0, 0.3, z));
        }
    }
    // the piers: decks on posts out over the water, boats moored alongside
    let deck = 0.0;
    for (k, x) in [l.x0 + 12.0, l.cx() + 6.0, l.x1 - 6.0].iter().enumerate() {
        // (where the beach runs far out, no pier: it would cross dry land)
        let len = pier_len(*x);
        if len > 50.0 {
            continue;
        }
        let (z0, z1) = (l.z1 - 0.2, l.z1 + len + 6.0 + 3.0 * k as f32);
        b.block(x - 1.6, deck - 0.3, z0, x + 1.6, deck, z1, PLANK);
        let mut z = z0 + 3.0;
        while z < z1 {
            for s in [-1.5f32, 1.5] {
                b.cbox(
                    x + s,
                    z,
                    0.35,
                    0.35,
                    water - 3.0,
                    deck - 0.3 - (water - 3.0),
                    PLANK_DARK,
                );
            }
            z += 4.0;
        }
        // bollards
        for s in [-1.4f32, 1.4] {
            b.cbox(x + s, z1 - 1.0, 0.3, 0.3, deck, 0.5, pal::DARK);
        }
        let colours = [[214, 76, 64], [64, 120, 206], [240, 200, 60], [80, 168, 96]];
        let c = colours[rng.below(colours.len())];
        boat(b, x + 3.4, z1 - 6.0, water - 0.3, false, 7.0, c);
        b.loot.push(vec3(*x, deck + 0.3, z1 - 2.0));
    }
}

/// A lighthouse: red and white bands, a gallery and a lantern, a keeper's cottage.
pub fn lighthouse(b: &mut MapBuilder, l: Lot) {
    let (x, z) = (l.cx(), l.cz() - 2.0);
    let mut y = 0.0;
    for k in 0..6 {
        let w = 5.0 - k as f32 * 0.35;
        let c = if k % 2 == 0 { WHITE } else { [210, 60, 50] };
        b.cbox(x, z, w, w, y, 3.0, c);
        y += 3.0;
    }
    let top = y;
    b.cbox(x, z, 5.4, 5.4, top, 0.3, pal::DARK);
    b.wood(
        x - 2.7,
        top + 0.3,
        z - 2.7,
        x + 2.7,
        top + 1.2,
        z - 2.55,
        pal::DARK,
    );
    b.wood(
        x - 2.7,
        top + 0.3,
        z + 2.55,
        x + 2.7,
        top + 1.2,
        z + 2.7,
        pal::DARK,
    );
    b.wood(
        x + 2.55,
        top + 0.3,
        z - 2.55,
        x + 2.7,
        top + 1.2,
        z + 2.55,
        pal::DARK,
    );
    b.cloth(
        x - 1.4,
        top + 0.3,
        z - 1.4,
        x + 1.4,
        top + 2.6,
        z + 1.4,
        pal::GLASS,
    );
    b.push_detail(
        x - 0.7,
        top + 0.8,
        z - 0.7,
        x + 0.7,
        top + 2.0,
        z + 0.7,
        pal::LIGHT,
    );
    b.cbox(x, z, 3.4, 3.4, top + 2.6, 0.4, [210, 60, 50]);
    b.cbox(x, z, 1.6, 1.6, top + 3.0, 0.8, [210, 60, 50]);
    b.ladder(
        vec3(x - 2.5 + 0.35 * 5.0 * 0.0, 0.0, z),
        vec3(-1.0, 0.0, 0.0),
        top + 0.3,
    );
    b.loot.push(vec3(x + 1.6, top + 0.6, z + 1.6));
    // the keeper's cottage
    let (cx0, cz0) = (l.x0 + 1.5, l.z1 - 8.5);
    b.building(
        cx0,
        cz0,
        cx0 + 8.0,
        cz0 + 7.0,
        0.0,
        3.0,
        &[
            (Side::MinZ, door(cx0 + 3.0)),
            (Side::MaxX, window(cz0 + 3.5)),
        ],
        WHITE,
        Some([60, 90, 140]),
    );
    b.loot.push(vec3(cx0 + 4.0, 0.3, cz0 + 3.5));
}

/// A filling station: a canopy over the pumps, a little shop, a sign on a post.
pub fn fuel_station(b: &mut MapBuilder, l: Lot) {
    let (cx, cz) = (l.cx() - 4.0, l.cz());
    for (dx, dz) in [(-5.0, -3.5), (5.0, -3.5), (-5.0, 3.5), (5.0, 3.5)] {
        b.cbox(cx + dx, cz + dz, 0.4, 0.4, 0.0, 4.4, WHITE);
    }
    b.cbox(cx, cz, 12.0, 9.0, 4.4, 0.6, [220, 60, 50]);
    b.push_detail(cx - 6.05, 4.6, cz - 4.55, cx + 6.05, 4.8, cz + 4.55, WHITE);
    for dx in [-2.5f32, 2.5] {
        b.cbox(cx + dx, cz, 0.8, 0.6, 0.0, 1.5, [60, 140, 90]);
        b.push_detail(
            cx + dx - 0.3,
            1.1,
            cz - 0.31,
            cx + dx + 0.3,
            1.4,
            cz + 0.31,
            pal::DARK,
        );
    }
    // the shop
    let (sx0, sz0) = (l.x1 - 9.0, l.z0 + 2.0);
    let ops = [
        (Side::MinX, door(sz0 + 3.0)),
        (Side::MinX, window(sz0 + 6.5)),
        (Side::MaxZ, window(sx0 + 4.0)),
    ];
    b.building(
        sx0,
        sz0,
        sx0 + 8.0,
        sz0 + 9.0,
        0.0,
        3.2,
        &ops,
        WHITE,
        Some([220, 60, 50]),
    );
    b.wood(
        sx0 + 1.0,
        0.0,
        sz0 + 1.0,
        sx0 + 2.0,
        1.4,
        sz0 + 7.0,
        [200, 200, 210],
    );
    b.loot.push(vec3(sx0 + 4.5, 0.3, sz0 + 4.5));
    b.loot.push(vec3(cx, 0.3, cz));
    // the sign
    let (px, pz) = (l.x0 + 1.5, l.z0 + 1.5);
    b.cbox(px, pz, 0.3, 0.3, 0.0, 6.0, pal::DARK);
    b.push_detail(
        px - 1.2,
        6.0,
        pz - 0.15,
        px + 1.2,
        7.6,
        pz + 0.15,
        [220, 60, 50],
    );
    for k in 0..3 {
        b.push_detail(
            px - 0.9,
            6.15 + k as f32 * 0.48,
            pz - 0.18,
            px + 0.6,
            6.45 + k as f32 * 0.48,
            pz + 0.18,
            WHITE,
        );
    }
    b.car(cx + 2.5, cz + 0.1, true, pal::CAR_A);
}

// ---------------------------------------------------------------- the countryside

/// A ranch: a house with a porch, a long stable, a corral, a water tower and hay.
pub fn ranch(b: &mut MapBuilder, l: Lot, rng: &mut Rng) {
    // the house with a porch to the south
    let (hx0, hz0) = (l.x0 + 3.0, l.z0 + 3.0);
    let (hx1, hz1) = (hx0 + 12.0, hz0 + 9.0);
    let wood_wall = [196, 150, 100];
    let ops = [
        (Side::MaxZ, door(hx0 + 4.0)),
        (Side::MaxZ, window(hx0 + 8.5)),
        (Side::MinZ, window(hx0 + 6.0)),
        (Side::MinX, door(hz0 + 4.5)),
        (Side::MaxX, window(hz0 + 4.5)),
    ];
    b.building(
        hx0,
        hz0,
        hx1,
        hz1,
        0.0,
        3.2,
        &ops,
        wood_wall,
        Some(pal::ROOF_RED),
    );
    b.block(
        hx0 + 1.0,
        3.55,
        hz0 + 2.0,
        hx1 - 1.0,
        4.3,
        hz1 - 2.0,
        shade(pal::ROOF_RED, 0.92),
    );
    b.block(hx0, 0.0, hz1, hx1, 0.25, hz1 + 2.6, PLANK);
    for x in [hx0 + 0.2, hx1 - 0.2, (hx0 + hx1) / 2.0] {
        b.cbox(x, hz1 + 2.4, 0.2, 0.2, 0.25, 2.9, PLANK_DARK);
    }
    b.cloth(
        hx0 - 0.2,
        3.15,
        hz1,
        hx1 + 0.2,
        3.3,
        hz1 + 2.7,
        shade(pal::ROOF_RED, 0.85),
    );
    b.wood(
        hx0 + 6.0,
        0.25,
        hz1 + 1.0,
        hx0 + 7.0,
        0.75,
        hz1 + 2.0,
        PLANK_DARK,
    );
    b.loot.push(vec3(hx0 + 6.0, 0.3, hz0 + 4.0));
    b.loot.push(vec3(hx1 - 2.0, 0.5, hz1 + 1.3));
    // the stable, open at the ends, stalls inside
    let (sx0, sz0) = (l.x1 - 15.0, l.z0 + 3.0);
    let (sx1, sz1) = (l.x1 - 3.0, l.z0 + 3.0 + 20.0f32.min(l.d() - 6.0));
    let ops = [
        (Side::MinZ, wide_door((sx0 + sx1) / 2.0)),
        (Side::MaxZ, wide_door((sx0 + sx1) / 2.0)),
        (Side::MinX, window(sz0 + 5.0)),
        (Side::MinX, window(sz1 - 5.0)),
    ];
    b.building(
        sx0,
        sz0,
        sx1,
        sz1,
        0.0,
        4.2,
        &ops,
        RED_BARN,
        Some(pal::ROOF_GREY),
    );
    b.block(
        sx0 + 1.5,
        4.55,
        sz0 - 0.2,
        sx1 - 1.5,
        5.4,
        sz1 + 0.2,
        shade(pal::ROOF_GREY, 0.9),
    );
    let mut z = sz0 + 3.5;
    while z < sz1 - 3.0 {
        b.wood(sx0 + 0.3, 0.0, z, sx0 + 3.6, 1.4, z + 0.15, PLANK);
        b.wood(sx1 - 3.6, 0.0, z, sx1 - 0.3, 1.4, z + 0.15, PLANK);
        z += 3.5;
    }
    bale(b, sx0 + 2.0, sz0 + 5.0, 0.0);
    bale(b, sx1 - 2.0, sz1 - 5.0, 0.0);
    b.loot.push(vec3((sx0 + sx1) / 2.0, 0.3, (sz0 + sz1) / 2.0));
    // the corral, a gate on the house side
    let (cx0, cz0, cx1, cz1) = (l.x0 + 3.0, hz1 + 6.0, sx0 - 4.0, l.z1 - 3.0);
    if cx1 - cx0 > 8.0 && cz1 - cz0 > 6.0 {
        let gate = (cx0 + cx1) / 2.0;
        fence(b, cx0, cz0, gate - 1.8, cz0, PLANK);
        fence(b, gate + 1.8, cz0, cx1, cz0, PLANK);
        fence(b, cx0, cz1, cx1, cz1, PLANK);
        fence(b, cx0, cz0, cx0, cz1, PLANK);
        fence(b, cx1, cz0, cx1, cz1, PLANK);
        // a trough, and the horses gone (only hoofprints)
        b.cbox(cx0 + 3.0, cz1 - 1.5, 2.6, 0.8, 0.0, 0.6, PLANK_DARK);
        b.decal(cx0 + 2.0, cz1 - 1.8, cx0 + 4.0, cz1 - 1.2, 0.58, WATER_BLUE);
        for _ in 0..3 {
            let x = rng.range(cx0 + 2.0, cx1 - 2.0);
            let z = rng.range(cz0 + 2.0, cz1 - 2.0);
            bale(b, x, z, 0.0);
        }
        b.loot.push(vec3((cx0 + cx1) / 2.0, 0.3, (cz0 + cz1) / 2.0));
    }
    // the water tower with a windmill on it
    let (wx, wz) = (sx0 - 4.0, sz0 + 3.0);
    for (dx, dz) in [(-1.2, -1.2), (1.2, -1.2), (-1.2, 1.2), (1.2, 1.2)] {
        b.cbox(wx + dx, wz + dz, 0.25, 0.25, 0.0, 6.0, PLANK_DARK);
    }
    b.cbox(wx, wz, 3.2, 3.2, 6.0, 0.25, PLANK);
    b.cbox(wx, wz, 2.6, 2.6, 6.25, 2.4, [120, 130, 140]);
    b.ladder(vec3(wx + 0.6, 0.0, wz + 1.3), vec3(0.0, 0.0, 1.0), 8.65);
    b.cbox(wx, wz, 0.2, 0.2, 8.65, 3.0, pal::DARK);
    for (bx, by) in [(1.6f32, 0.15f32), (0.15, 1.6)] {
        b.push_detail(
            wx - bx,
            11.0 - by,
            wz - 0.25,
            wx + bx,
            11.0 + by,
            wz - 0.15,
            WHITE,
        );
    }
    b.loot.push(vec3(wx - 0.7, 8.95, wz - 0.7));
    // a tractor
    let (tx, tz) = (l.x0 + 20.0f32.min(l.w() - 6.0), l.z0 + 5.0);
    b.cbox(tx, tz, 2.0, 3.2, 0.5, 1.2, [200, 60, 50]);
    b.cbox(tx, tz + 0.6, 1.6, 1.4, 1.7, 1.2, [200, 60, 50]);
    b.push_detail(tx - 0.7, 2.9, tz - 0.1, tx + 0.7, 2.95, tz + 1.3, pal::DARK);
    for (dx, dz, r) in [
        (-1.1f32, -1.0f32, 0.6f32),
        (1.1, -1.0, 0.6),
        (-1.15, 1.0, 0.9),
        (1.15, 1.0, 0.9),
    ] {
        b.cbox(tx + dx, tz + dz, 0.4, r * 2.0, 0.0, r * 2.0, pal::TYRE);
    }
}

/// A sawmill in the woods: a saw shed, stacks of logs, a lorry, the foreman's hut.
pub fn sawmill(b: &mut MapBuilder, l: Lot, rng: &mut Rng) {
    // the saw shed: a roof on posts, the saw bench under it
    let (x0, z0, x1, z1) = (l.x0 + 3.0, l.z0 + 3.0, l.x0 + 21.0, l.z0 + 13.0);
    let mut x = x0;
    while x <= x1 + 0.01 {
        for z in [z0, z1] {
            b.cbox(x, z, 0.35, 0.35, 0.0, 4.5, PLANK_DARK);
        }
        x += 4.5;
    }
    b.block(
        x0 - 0.5,
        4.5,
        z0 - 0.5,
        x1 + 0.5,
        4.8,
        z1 + 0.5,
        pal::ROOF_GREY,
    );
    b.block(
        x0 + 2.0,
        0.0,
        (z0 + z1) / 2.0 - 0.8,
        x1 - 2.0,
        0.9,
        (z0 + z1) / 2.0 + 0.8,
        PLANK,
    );
    b.push_detail(
        (x0 + x1) / 2.0 - 0.05,
        0.9,
        (z0 + z1) / 2.0 - 0.8,
        (x0 + x1) / 2.0 + 0.05,
        1.8,
        (z0 + z1) / 2.0 + 0.8,
        [200, 205, 210],
    );
    log(b, (x0 + x1) / 2.0 - 3.0, (z0 + z1) / 2.0, 0.9, 6.0, true);
    b.loot.push(vec3(x0 + 2.0, 0.3, z0 + 1.5));
    b.loot.push(vec3(x1 - 2.0, 0.3, z1 - 1.5));
    // stacks of logs (cover you can climb)
    for k in 0..4 {
        let sx = l.x0 + 6.0 + k as f32 * 9.0;
        let sz = l.z1 - 8.0;
        if sx + 4.0 > l.x1 {
            break;
        }
        let layers = 2 + rng.below(3);
        for j in 0..layers {
            for i in 0..(5 - j) {
                log(
                    b,
                    sx,
                    sz - 1.4 + i as f32 * 0.6 + j as f32 * 0.3,
                    j as f32 * 0.55,
                    6.5,
                    true,
                );
            }
        }
        if k % 2 == 0 {
            b.loot.push(vec3(sx, 0.3, sz + 2.4));
        }
    }
    // the foreman's hut
    let (hx0, hz0) = (l.x1 - 11.0, l.z0 + 3.0);
    b.building(
        hx0,
        hz0,
        hx0 + 8.0,
        hz0 + 7.0,
        0.0,
        3.0,
        &[
            (Side::MaxZ, door(hx0 + 2.5)),
            (Side::MinX, window(hz0 + 3.5)),
        ],
        [160, 116, 74],
        Some(pal::ROOF_GREY),
    );
    b.loot.push(vec3(hx0 + 4.0, 0.3, hz0 + 3.5));
    // a lorry with logs on the back
    let (lx, lz) = (l.x1 - 6.0, l.z1 - 18.0);
    if lz > hz0 + 10.0 {
        b.cbox(lx, lz - 3.0, 2.4, 2.2, 0.6, 2.2, [70, 110, 80]);
        b.cbox(lx, lz + 1.5, 2.6, 6.0, 0.6, 0.5, pal::DARK);
        for i in 0..3 {
            log(b, lx - 0.8 + i as f32 * 0.8, lz + 1.5, 1.1, 5.6, false);
        }
        for dz in [-3.0f32, 0.5, 3.2] {
            b.cbox(lx, lz + dz, 2.8, 1.0, 0.0, 1.0, pal::TYRE);
        }
    }
    // stumps all round
    for _ in 0..10 {
        let x = rng.range(l.x0 + 1.0, l.x1 - 1.0);
        let z = rng.range(l.z0 + 1.0, l.z1 - 1.0);
        b.cbox(x, z, 0.6, 0.6, 0.0, 0.5, pal::TRUNK);
    }
}

/// A mine at the foot of the mountain: the pit-head tower with its wheel, the entrance
/// in the rock face, rails and wagons of ore, spoil heaps and the shed.
pub fn mine(b: &mut MapBuilder, l: Lot, rng: &mut Rng) {
    // the entrance on the uphill (north) side: a timber portal on the cut face
    let (ex, ez) = (l.cx() - 8.0, l.z0 + 0.5);
    b.cbox(ex, ez + 1.5, 5.6, 3.0, 0.0, 0.01, pal::DARK);
    b.block(
        ex - 2.6,
        0.0,
        ez - 3.0,
        ex + 2.6,
        3.6,
        ez + 0.6,
        [30, 28, 26],
    );
    for s in [-2.3f32, 2.3] {
        b.cbox(ex + s, ez + 0.8, 0.5, 0.5, 0.0, 3.9, PLANK_DARK);
    }
    b.cbox(ex, ez + 0.8, 5.6, 0.6, 3.9, 0.5, PLANK_DARK);
    b.push_detail(
        ex - 1.5,
        4.4,
        ez + 0.6,
        ex + 1.5,
        4.9,
        ez + 0.8,
        [230, 200, 60],
    );
    // rails out of it, and wagons
    for s in [-0.6f32, 0.6] {
        b.decal(
            ex + s - 0.06,
            ez + 0.6,
            ex + s + 0.06,
            l.z1 - 4.0,
            0.08,
            [120, 110, 100],
        );
    }
    let mut z = ez + 1.0;
    while z < l.z1 - 4.0 {
        b.decal(ex - 0.9, z, ex + 0.9, z + 0.25, 0.04, PLANK_DARK);
        z += 1.2;
    }
    for k in 0..2 {
        let wz = ez + 6.0 + k as f32 * 6.0;
        b.cbox(ex, wz, 1.4, 2.2, 0.15, 1.1, pal::RUST);
        b.push_detail(
            ex - 0.6,
            1.25,
            wz - 0.95,
            ex + 0.6,
            1.5,
            wz + 0.95,
            [90, 90, 96],
        );
        b.loot.push(vec3(ex + 1.4, 0.3, wz));
    }
    // the pit-head tower: legs, a platform with a wheel, a ladder up
    let (hx, hz) = (l.cx() + 8.0, l.cz() - 2.0);
    for (dx, dz) in [(-2.0, -2.0), (2.0, -2.0), (-2.0, 2.0), (2.0, 2.0)] {
        b.cbox(hx + dx, hz + dz, 0.4, 0.4, 0.0, 12.0, PLANK_DARK);
    }
    for y in [4.0f32, 8.0] {
        b.wood(hx - 2.2, y, hz - 2.2, hx + 2.2, y + 0.25, hz + 2.2, PLANK);
    }
    b.block(hx - 2.4, 12.0, hz - 2.4, hx + 2.4, 12.3, hz + 2.4, PLANK);
    b.cbox(hx, hz, 0.4, 4.2, 12.3, 4.2, pal::RUST);
    b.cbox(hx, hz, 0.6, 0.6, 14.2, 0.4, pal::DARK);
    b.ladder(vec3(hx + 1.2, 0.0, hz + 2.2), vec3(0.0, 0.0, 1.0), 12.3);
    b.loot.push(vec3(hx + 1.4, 12.6, hz - 1.4));
    // the miners' shed
    let (sx0, sz0) = (l.x1 - 12.0, l.z1 - 10.0);
    b.building(
        sx0,
        sz0,
        sx0 + 9.0,
        sz0 + 7.0,
        0.0,
        3.0,
        &[
            (Side::MinZ, door(sx0 + 3.0)),
            (Side::MinX, window(sz0 + 3.5)),
        ],
        [150, 140, 120],
        Some(pal::RUST),
    );
    b.loot.push(vec3(sx0 + 4.5, 0.3, sz0 + 3.5));
    // spoil heaps and loose rocks
    for k in 0..3 {
        let x = l.x0 + 5.0 + k as f32 * 7.0;
        let z = l.z1 - 6.0;
        for (j, w) in [4.0f32, 2.6, 1.2].iter().enumerate() {
            b.cbox(
                x,
                z,
                *w,
                *w * 0.9,
                j as f32 * 0.8,
                0.8,
                shade([110, 104, 96], 1.0 - j as f32 * 0.06),
            );
        }
    }
    for _ in 0..6 {
        let x = rng.range(l.x0 + 2.0, l.x1 - 2.0);
        let z = rng.range(l.z0 + 6.0, l.z1 - 2.0);
        b.cbox(x, z, 0.9, 0.7, 0.0, 0.6, [120, 116, 110]);
    }
}

/// A radio station on the peak: a lattice mast (a ladder to a platform half way), a hut,
/// a dish — and somebody built a snowman.
pub fn radio_station(b: &mut MapBuilder, l: Lot) {
    let (x, z) = (l.cx() - 3.0, l.cz());
    let h = 28.0;
    for (dx, dz) in [(-1.0, -1.0), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        b.cbox(x + dx, z + dz, 0.2, 0.2, 0.0, h, [220, 70, 60]);
    }
    let mut y = 2.0;
    while y < h {
        b.push_detail(x - 1.1, y, z - 1.1, x + 1.1, y + 0.12, z + 1.1, WHITE);
        y += 3.0;
    }
    b.block(x - 2.0, 14.0, z - 2.0, x + 2.0, 14.25, z + 2.0, pal::METAL);
    b.wood(x - 2.0, 14.25, z - 2.0, x + 2.0, 15.1, z - 1.85, pal::METAL);
    b.wood(x - 2.0, 14.25, z + 1.85, x + 2.0, 15.1, z + 2.0, pal::METAL);
    b.wood(
        x - 2.0,
        14.25,
        z - 1.85,
        x - 1.85,
        15.1,
        z + 1.85,
        pal::METAL,
    );
    b.ladder(vec3(x + 1.1, 0.0, z), vec3(1.0, 0.0, 0.0), 14.25);
    b.push_detail(
        x - 0.3,
        h,
        z - 0.3,
        x + 0.3,
        h + 0.5,
        z + 0.3,
        [255, 80, 60],
    );
    b.loot.push(vec3(x + 1.2, 14.6, z + 1.2));
    // the hut
    let (hx0, hz0) = (l.x1 - 8.0, l.z0 + 1.0);
    b.building(
        hx0,
        hz0,
        hx0 + 7.0,
        hz0 + 6.0,
        0.0,
        2.8,
        &[
            (Side::MaxZ, door(hx0 + 2.0)),
            (Side::MinX, window(hz0 + 3.0)),
        ],
        [200, 204, 200],
        Some(pal::ROOF_GREY),
    );
    b.cbox(hx0 + 4.5, hz0 + 3.0, 1.6, 0.8, 0.0, 1.0, [80, 90, 100]);
    b.push_detail(
        hx0 + 4.0,
        1.0,
        hz0 + 2.8,
        hx0 + 5.0,
        1.4,
        hz0 + 3.2,
        [100, 220, 120],
    );
    b.loot.push(vec3(hx0 + 2.0, 0.3, hz0 + 3.0));
    // a dish on a stand
    let (dx, dz) = (l.x0 + 4.0, l.z1 - 4.0);
    b.cbox(dx, dz, 0.4, 0.4, 0.0, 2.0, pal::METAL);
    b.push_detail(dx - 1.6, 2.0, dz - 0.4, dx + 1.6, 3.6, dz - 0.2, WHITE);
    b.push_detail(dx - 1.1, 2.3, dz - 0.6, dx + 1.1, 3.3, dz - 0.4, WHITE);
    b.push_detail(
        dx - 0.06,
        2.6,
        dz - 1.6,
        dx + 0.06,
        2.8,
        dz - 0.6,
        pal::DARK,
    );
    // the snowman, with a carrot nose and a bucket hat
    let (sx, sz) = (l.x1 - 4.0, l.z1 - 4.0);
    b.cbox(sx, sz, 1.6, 1.6, 0.0, 1.3, WHITE);
    b.cbox(sx, sz, 1.1, 1.1, 1.3, 1.0, WHITE);
    b.cbox(sx, sz, 0.75, 0.75, 2.3, 0.75, WHITE);
    b.push_detail(
        sx - 0.06,
        2.6,
        sz - 0.75,
        sx + 0.06,
        2.72,
        sz - 0.37,
        [240, 130, 40],
    );
    for ex in [-0.18f32, 0.18] {
        b.push_detail(
            sx + ex - 0.05,
            2.8,
            sz - 0.39,
            sx + ex + 0.05,
            2.9,
            sz - 0.36,
            pal::DARK,
        );
    }
    b.push_detail(
        sx - 0.4,
        3.05,
        sz - 0.4,
        sx + 0.4,
        3.5,
        sz + 0.4,
        [200, 60, 50],
    );
    for y in [1.5f32, 1.85] {
        b.push_detail(
            sx - 0.05,
            y,
            sz - 0.57,
            sx + 0.05,
            y + 0.1,
            sz - 0.54,
            pal::DARK,
        );
    }
    b.push_detail(
        sx + 0.5,
        1.8,
        sz - 0.04,
        sx + 1.4,
        1.88,
        sz + 0.04,
        pal::TRUNK,
    );
    b.push_detail(
        sx - 0.42,
        2.25,
        sz - 0.42,
        sx + 0.42,
        2.4,
        sz + 0.42,
        [60, 120, 200],
    );
}

/// A campsite by the lake: tents, a campfire with log seats, two cabins, canoes.
pub fn campsite(b: &mut MapBuilder, l: Lot, rng: &mut Rng) {
    let colours = [
        [230, 120, 60],
        [70, 150, 210],
        [100, 180, 90],
        [230, 200, 70],
        [200, 90, 140],
    ];
    // the campfire in the middle
    let (fx, fz) = (l.cx(), l.cz());
    for k in 0..8 {
        let a = k as f32 * std::f32::consts::TAU / 8.0;
        b.cbox(
            fx + a.cos() * 1.2,
            fz + a.sin() * 1.2,
            0.4,
            0.4,
            0.0,
            0.3,
            STONE_DARK,
        );
    }
    b.push_detail(fx - 0.6, 0.0, fz - 0.1, fx + 0.6, 0.2, fz + 0.1, pal::TRUNK);
    b.push_detail(fx - 0.1, 0.0, fz - 0.6, fx + 0.1, 0.2, fz + 0.6, pal::TRUNK);
    b.push_detail(
        fx - 0.3,
        0.2,
        fz - 0.3,
        fx + 0.3,
        0.7,
        fz + 0.3,
        [255, 150, 40],
    );
    b.push_detail(
        fx - 0.15,
        0.7,
        fz - 0.15,
        fx + 0.15,
        1.0,
        fz + 0.15,
        [255, 220, 90],
    );
    for (dx, dz, ax) in [
        (0.0f32, -3.2f32, true),
        (0.0, 3.2, true),
        (-3.2, 0.0, false),
        (3.2, 0.0, false),
    ] {
        log(b, fx + dx, fz + dz, 0.0, 2.4, ax);
    }
    b.loot.push(vec3(fx + 2.0, 0.3, fz + 2.0));
    // tents round it
    for k in 0..6 {
        let a = k as f32 * std::f32::consts::TAU / 6.0 + 0.3;
        let r = (l.w().min(l.d()) * 0.5 - 5.0).max(7.0);
        let (tx, tz) = (fx + a.cos() * r, fz + a.sin() * r);
        let c = colours[rng.below(colours.len())];
        b.cloth(tx - 1.4, 0.0, tz - 1.6, tx + 1.4, 0.7, tz + 1.6, c);
        b.cloth(
            tx - 0.95,
            0.7,
            tz - 1.6,
            tx + 0.95,
            1.3,
            tz + 1.6,
            shade(c, 0.92),
        );
        b.cloth(
            tx - 0.45,
            1.3,
            tz - 1.6,
            tx + 0.45,
            1.75,
            tz + 1.6,
            shade(c, 0.85),
        );
        if k % 2 == 0 {
            b.loot.push(vec3(tx, 0.3, tz));
        }
    }
    // two cabins on the far side
    for (k, cx) in [l.x0 + 3.0, l.x0 + 3.0].iter().enumerate() {
        let cz = if k == 0 { l.z0 + 2.0 } else { l.z1 - 9.0 };
        b.building(
            *cx,
            cz,
            cx + 7.0,
            cz + 7.0,
            0.0,
            3.0,
            &[(Side::MaxX, door(cz + 3.5)), (Side::MinZ, window(cx + 3.5))],
            [150, 106, 66],
            Some([90, 120, 80]),
        );
        b.loot.push(vec3(cx + 3.5, 0.3, cz + 3.5));
    }
    // canoes on racks
    for k in 0..2 {
        let z = l.z0 + 4.0 + k as f32 * 2.0;
        boat(
            b,
            l.x1 - 4.0,
            z,
            0.6 + k as f32 * 0.0,
            true,
            4.6,
            colours[(k * 2) % colours.len()],
        );
        b.cbox(l.x1 - 6.0, z, 0.2, 0.2, 0.0, 0.6, PLANK_DARK);
        b.cbox(l.x1 - 2.0, z, 0.2, 0.2, 0.0, 0.6, PLANK_DARK);
    }
}

/// A ruined castle on its hill: a curtain wall broken in places, with battlements; a
/// tower at each corner (a ladder to the top); a keep in the middle with a roof to hold.
pub fn castle(b: &mut MapBuilder, l: Lot, rng: &mut Rng) {
    let (x0, z0, x1, z1) = (l.x0 + 3.0, l.z0 + 3.0, l.x1 - 3.0, l.z1 - 3.0);
    let (h, t) = (6.0, 1.4);
    // the walls, each in a few stretches; some fallen in (ruins)
    let sides = [
        (x0, z0, x1, z0 + t),
        (x0, z1 - t, x1, z1),
        (x0, z0, x0 + t, z1),
        (x1 - t, z0, x1, z1),
    ];
    for (k, &(a, bz, c, d)) in sides.iter().enumerate() {
        let along_x = (c - a) > (d - bz);
        let len = if along_x { c - a } else { d - bz };
        let n = 5;
        for i in 0..n {
            let s0 = i as f32 * len / n as f32;
            let s1 = (i + 1) as f32 * len / n as f32;
            // the gate in the middle of the south wall
            if k == 1 && i == n / 2 {
                b.block(a + s0, 3.8, bz, a + s1, h, d, STONE);
                continue;
            }
            // a stretch fallen down to rubble
            let wh = if rng.chance(0.18) {
                rng.range(1.0, 2.4)
            } else {
                h
            };
            let (bx0, bz0, bx1, bz1) = if along_x {
                (a + s0, bz, a + s1, d)
            } else {
                (a, bz + s0, c, bz + s1)
            };
            b.block(
                bx0,
                0.0,
                bz0,
                bx1,
                wh,
                bz1,
                shade(STONE, rng.range(0.92, 1.05)),
            );
            if wh == h {
                // battlements: teeth along the outer edge
                let mut s = 0.0;
                while s < (if along_x { bx1 - bx0 } else { bz1 - bz0 }) - 0.6 {
                    let (px0, pz0, px1, pz1) = if along_x {
                        let o = if k == 0 { bz0 } else { bz1 - 0.4 };
                        (bx0 + s, o, bx0 + s + 0.8, o + 0.4)
                    } else {
                        let o = if k == 2 { bx0 } else { bx1 - 0.4 };
                        (o, bz0 + s, o + 0.4, bz0 + s + 0.8)
                    };
                    b.block(px0, h, pz0, px1, h + 0.9, pz1, STONE_DARK);
                    s += 1.6;
                }
            } else {
                for _ in 0..3 {
                    let rx = rng.range(bx0 - 1.5, bx1 + 1.5);
                    let rz = rng.range(bz0 - 1.5, bz1 + 1.5);
                    b.cbox(
                        rx,
                        rz,
                        rng.range(0.6, 1.2),
                        rng.range(0.6, 1.2),
                        0.0,
                        rng.range(0.4, 0.9),
                        STONE_DARK,
                    );
                }
            }
        }
    }
    // corner towers: hollow inside, a ladder up inside to the top
    for (cx, cz, ox, oz) in [
        (x0, z0, 1.0, 1.0),
        (x1, z0, -1.0, 1.0),
        (x0, z1, 1.0, -1.0),
        (x1, z1, -1.0, -1.0),
    ] {
        let s = 5.0;
        let top = 9.0;
        let (tx0, tx1) = (cx.min(cx + ox * s), cx.max(cx + ox * s));
        let (tz0, tz1) = (cz.min(cz + oz * s), cz.max(cz + oz * s));
        b.building(tx0, tz0, tx1, tz1, 0.0, top, &[], STONE, None);
        // a doorway from the courtyard side (cut as a gap)
        b.block(
            tx0 + 0.3,
            top - 0.3,
            tz0 + 0.3,
            tx1 - 0.3,
            top,
            tz1 - 0.3,
            STONE_DARK,
        );
        for (k, (px, pz)) in [
            (tx0, tz0),
            (tx1 - 0.6, tz0),
            (tx0, tz1 - 0.6),
            (tx1 - 0.6, tz1 - 0.6),
        ]
        .iter()
        .enumerate()
        {
            let _ = k;
            b.block(*px, top, *pz, px + 0.6, top + 1.0, pz + 0.6, STONE_DARK);
        }
        b.ladder(
            vec3(cx + ox * s * 0.5, 0.0, cz + oz * (s + 0.0)),
            vec3(0.0, 0.0, oz),
            top,
        );
        b.loot
            .push(vec3(cx + ox * s * 0.5, top + 0.3, cz + oz * s * 0.5));
    }
    // the keep
    let (kx0, kz0) = (l.cx() - 7.0, l.cz() - 7.0);
    let ground = [
        (Side::MaxZ, door(l.cx() + 0.6)),
        (Side::MinX, window(l.cz())),
        (Side::MaxX, window(l.cz())),
    ];
    let upper = [
        (Side::MinZ, window(l.cx())),
        (Side::MaxZ, window(l.cx())),
        (Side::MinX, window(l.cz())),
        (Side::MaxX, window(l.cz())),
    ];
    b.tower(&Tower {
        x0: kx0,
        z0: kz0,
        x1: kx0 + 14.0,
        z1: kz0 + 14.0,
        floors: 3,
        storey: 3.4,
        color: shade(STONE, 0.95),
        roof: STONE_DARK,
        ground: &ground,
        upper: &upper,
        roof_access: true,
    });
    for k in 0..4 {
        b.loot
            .push(vec3(l.cx() + 1.5, k as f32 * 3.4 + 0.4, l.cz()));
    }
    // a banner on the keep's roof
    b.cbox(kx0 + 12.0, kz0 + 2.0, 0.2, 0.2, 10.2, 4.0, pal::DARK);
    b.cloth(
        kx0 + 12.1,
        12.6,
        kz0 + 1.95,
        kx0 + 14.2,
        14.0,
        kz0 + 2.05,
        [200, 50, 60],
    );
    b.push_detail(
        kx0 + 12.9,
        13.0,
        kz0 + 1.9,
        kx0 + 13.4,
        13.6,
        kz0 + 2.1,
        GOLD,
    );
}

/// A crashed flying saucer in a burnt clearing, its pilot standing by, looking lost.
pub fn ufo(b: &mut MapBuilder, l: Lot) {
    let (x, z) = (l.cx(), l.cz());
    let hull = [190, 196, 206];
    // the burnt furrow it dug
    b.decal(x - 2.5, z - 16.0, x + 2.5, z - 3.0, 0.01, [52, 46, 40]);
    b.decal(x - 7.0, z - 7.0, x + 7.0, z + 7.0, 0.012, [60, 54, 46]);
    // the saucer, nose down in the ground (a tilted stack of discs)
    for (k, (w, y)) in [(11.0f32, 0.0f32), (9.0, 0.7), (6.0, 1.4)]
        .iter()
        .enumerate()
    {
        let tilt = k as f32 * 0.5;
        b.cbox(
            x,
            z + tilt,
            *w,
            *w * 0.92,
            *y,
            0.7,
            shade(hull, 1.0 - k as f32 * 0.05),
        );
    }
    b.cbox(x, z + 1.4, 12.4, 1.2, 0.3, 0.3, shade(hull, 0.8));
    b.cbox(x, z + 1.4, 1.2, 11.4, 0.3, 0.3, shade(hull, 0.8));
    b.cloth(
        x - 1.8,
        2.1,
        z - 0.4,
        x + 1.8,
        3.4,
        z + 3.2,
        [120, 240, 170],
    );
    for (dx, dz) in [(-4.0, 0.0), (4.0, 0.0), (0.0, -4.0), (0.0, 4.0)] {
        b.push_detail(
            x + dx - 0.3,
            0.75,
            z + dz - 0.3,
            x + dx + 0.3,
            1.0,
            z + dz + 0.3,
            [120, 255, 160],
        );
    }
    // its hatch is open: something good inside
    b.loot.push(vec3(x + 0.8, 2.1, z + 1.2));
    b.loot.push(vec3(x + 5.0, 0.3, z + 6.0));
    b.loot.push(vec3(x - 5.0, 0.3, z + 6.0));
    // the pilot: big head, big black eyes
    let (px, pz) = (x + 6.5, z - 4.0);
    let green = [140, 220, 120];
    b.push_detail(px - 0.15, 0.0, pz - 0.1, px - 0.05, 0.5, pz + 0.1, green);
    b.push_detail(px + 0.05, 0.0, pz - 0.1, px + 0.15, 0.5, pz + 0.1, green);
    b.push_detail(px - 0.22, 0.5, pz - 0.15, px + 0.22, 1.0, pz + 0.15, green);
    b.push_detail(px - 0.35, 1.0, pz - 0.3, px + 0.35, 1.55, pz + 0.3, green);
    for ex in [-0.16f32, 0.16] {
        b.push_detail(
            px + ex - 0.1,
            1.2,
            pz - 0.33,
            px + ex + 0.1,
            1.38,
            pz - 0.29,
            [20, 20, 24],
        );
    }
    b.push_detail(
        px + 0.22,
        0.85,
        pz - 0.05,
        px + 0.55,
        0.95,
        pz + 0.05,
        green,
    );
    // broken trees round the clearing
    for (dx, dz) in [(-9.0f32, 5.0f32), (8.0, 8.0), (-8.0, -9.0), (10.0, -2.0)] {
        b.cbox(x + dx, z + dz, 0.45, 0.45, 0.0, 1.2, pal::BURNT);
        b.cbox(x + dx + 1.8, z + dz, 3.6, 0.45, 0.0, 0.45, pal::BURNT);
    }
}

/// Standing stones in a ring, a few with lintels, a flat stone in the middle.
pub fn stones(b: &mut MapBuilder, l: Lot) {
    let (x, z) = (l.cx(), l.cz());
    let r = 9.0;
    let n = 12;
    for k in 0..n {
        let a = k as f32 * std::f32::consts::TAU / n as f32;
        let (sx, sz) = (x + a.cos() * r, z + a.sin() * r);
        let tall = if k % 3 == 0 { 3.2 } else { 4.2 };
        b.cbox(
            sx,
            sz,
            1.1,
            1.1,
            -0.3,
            tall + 0.3,
            shade(STONE, 0.9 + (k % 4) as f32 * 0.03),
        );
        if k % 2 == 0 && k % 3 != 0 {
            let a2 = (k + 1) as f32 * std::f32::consts::TAU / n as f32;
            let (tx, tz) = (x + a2.cos() * r, z + a2.sin() * r);
            let (mx, mz) = ((sx + tx) / 2.0, (sz + tz) / 2.0);
            let w = (sx - tx).abs().max(1.2) + 1.2;
            let d = (sz - tz).abs().max(1.2) + 1.2;
            b.cbox(mx, mz, w, d, 4.2, 0.6, STONE_DARK);
        }
    }
    b.cbox(x, z, 3.0, 1.6, 0.0, 0.8, STONE_DARK);
    b.loot.push(vec3(x, 0.9, z));
    b.loot.push(vec3(x + 4.0, 0.3, z));
}

// ---------------------------------------------------------------- odd finds

/// A rubber duck the size of a boat, afloat (you can stand on it).
pub fn rubber_duck(b: &mut MapBuilder, x: f32, z: f32, water: f32) {
    let y = water - 0.5;
    let yel = [250, 214, 60];
    b.cbox(x, z, 4.0, 3.0, y, 1.8, yel);
    b.cbox(x - 1.9, z, 0.8, 1.6, y + 1.0, 0.9, yel);
    b.cbox(x + 1.1, z, 2.0, 2.0, y + 1.8, 1.8, yel);
    b.push_detail(
        x + 2.1,
        y + 2.3,
        z - 0.45,
        x + 3.0,
        y + 2.7,
        z + 0.45,
        [240, 120, 40],
    );
    for s in [-1.0f32, 1.0] {
        b.push_detail(
            x + 1.6,
            y + 3.0,
            z + s * 1.0 - 0.15,
            x + 1.95,
            y + 3.3,
            z + s * 1.0 + 0.15,
            pal::DARK,
        );
    }
}

/// Three humps and a long neck in the lake. Nobody believes it.
pub fn lake_monster(b: &mut MapBuilder, x: f32, z: f32, water: f32) {
    let g = [70, 130, 90];
    for k in 0..3 {
        let hx = x - 4.0 * k as f32;
        b.push_detail(
            hx - 1.0,
            water - 0.2,
            z - 0.6,
            hx + 1.0,
            water + 0.9,
            z + 0.6,
            shade(g, 1.0 - k as f32 * 0.05),
        );
    }
    b.push_detail(
        x + 1.6,
        water - 0.2,
        z - 0.4,
        x + 2.3,
        water + 3.0,
        z + 0.4,
        g,
    );
    b.push_detail(
        x + 1.6,
        water + 3.0,
        z - 0.45,
        x + 3.4,
        water + 3.7,
        z + 0.45,
        g,
    );
    b.push_detail(
        x + 3.0,
        water + 3.35,
        z - 0.48,
        x + 3.15,
        water + 3.5,
        z + 0.48,
        pal::DARK,
    );
}

/// Letters of blocks on a hillside (a 5-high font). `base` gives the ground under a point.
pub fn letters(
    b: &mut MapBuilder,
    text: &str,
    x: f32,
    z: f32,
    cell: f32,
    base: &dyn Fn(f32, f32) -> f32,
) {
    let font = |c: char| -> [&'static str; 5] {
        match c {
            'T' => ["###", ".#.", ".#.", ".#.", ".#."],
            'O' => ["###", "#.#", "#.#", "#.#", "###"],
            'N' => ["#..#", "##.#", "#.##", "#..#", "#..#"],
            _ => ["...", "...", "...", "...", "..."],
        }
    };
    let mut cx = x;
    for ch in text.chars() {
        let rows = font(ch);
        for (r, row) in rows.iter().enumerate() {
            for (c, px) in row.chars().enumerate() {
                if px != '#' {
                    continue;
                }
                let px0 = cx + c as f32 * cell;
                let y0 = base(px0 + cell * 0.5, z) - 0.5;
                // letters stand up: rows go up from the ground
                let row_y = y0 + (4 - r) as f32 * cell;
                b.block(
                    px0,
                    row_y,
                    z - 0.6,
                    px0 + cell,
                    row_y + cell,
                    z + 0.6,
                    WHITE,
                );
                // a post down to the ground under every block that is not on it
                if r < 4 && rows[r + 1].chars().nth(c) != Some('#') {
                    b.block(
                        px0 + cell * 0.35,
                        y0,
                        z - 0.2,
                        px0 + cell * 0.65,
                        row_y,
                        z + 0.2,
                        STONE_DARK,
                    );
                }
            }
        }
        cx += cell * (rows[0].len() as f32 + 1.0);
    }
}

/// A wreck on the beach, broken in two, a mast leaning on the sand — and a chest.
pub fn shipwreck(b: &mut MapBuilder, x: f32, z: f32, y: f32) {
    let hull = [120, 86, 60];
    b.lift(y, |b| {
        // the bow half
        b.block(x - 2.2, -0.4, z - 9.0, x + 2.2, 2.2, z - 1.5, hull);
        b.block(
            x - 1.8,
            2.2,
            z - 8.0,
            x + 1.8,
            2.6,
            z - 1.5,
            shade(hull, 0.85),
        );
        b.block(x - 1.4, -0.4, z - 10.4, x + 1.4, 1.6, z - 9.0, hull);
        // the stern, off to the side and lower, open to the sky
        let (sx, sz) = (x + 1.2, z + 3.5);
        b.block(
            sx - 2.2,
            -0.6,
            sz - 1.0,
            sx + 2.2,
            0.6,
            sz + 6.0,
            shade(hull, 0.95),
        );
        b.block(sx - 2.2, 0.6, sz - 1.0, sx - 1.9, 2.0, sz + 6.0, hull);
        b.block(sx + 1.9, 0.6, sz - 1.0, sx + 2.2, 2.0, sz + 6.0, hull);
        b.block(sx - 2.2, 0.6, sz + 5.7, sx + 2.2, 2.4, sz + 6.0, hull);
        // the mast, fallen across the sand
        b.cbox(x - 5.0, z + 1.0, 9.0, 0.4, 0.0, 0.4, PLANK_DARK);
        b.cloth(
            x - 8.0,
            0.0,
            z + 1.4,
            x - 3.0,
            0.15,
            z + 4.0,
            [220, 210, 190],
        );
        // the treasure chest in the stern, lid open, gold showing
        b.cbox(sx, sz + 3.0, 1.4, 0.9, 0.6, 0.8, [140, 90, 40]);
        b.push_detail(sx - 0.6, 1.4, sz + 2.6, sx + 0.6, 1.55, sz + 3.4, GOLD);
        b.push_detail(
            sx - 0.7,
            1.4,
            sz + 3.4,
            sx + 0.7,
            2.2,
            sz + 3.5,
            [120, 76, 34],
        );
        for k in 0..4 {
            b.loot.push(vec3(
                sx - 1.0 + (k % 2) as f32 * 2.0,
                0.9,
                sz + 1.5 + (k / 2) as f32 * 2.5,
            ));
        }
        // a skeleton of the last crewman, sitting on the bow
        b.push_detail(x - 0.2, 2.6, z - 4.0, x + 0.2, 3.2, z - 3.8, WHITE);
        b.push_detail(x - 0.18, 3.2, z - 4.05, x + 0.18, 3.55, z - 3.75, WHITE);
    });
}

/// A forester's hut hidden deep in the woods: someone's stash.
pub fn stash_hut(b: &mut MapBuilder, x: f32, z: f32) {
    b.building(
        x - 3.0,
        z - 2.5,
        x + 3.0,
        z + 2.5,
        0.0,
        2.6,
        &[(Side::MaxZ, door(x - 1.0))],
        [110, 80, 50],
        Some([80, 110, 70]),
    );
    b.block(x - 2.0, 2.95, z - 1.5, x + 2.0, 3.5, z + 1.5, [70, 100, 60]);
    // antlers over the door
    b.push_detail(
        x - 0.6,
        2.2,
        z + 2.5,
        x + 0.6,
        2.3,
        z + 2.6,
        [230, 220, 200],
    );
    b.push_detail(
        x - 0.6,
        2.3,
        z + 2.5,
        x - 0.5,
        2.6,
        z + 2.6,
        [230, 220, 200],
    );
    b.push_detail(
        x + 0.5,
        2.3,
        z + 2.5,
        x + 0.6,
        2.6,
        z + 2.6,
        [230, 220, 200],
    );
    b.cbox(x + 1.6, z - 1.2, 1.2, 0.8, 0.0, 0.7, PLANK_DARK);
    for k in 0..4 {
        b.loot.push(vec3(
            x - 1.8 + k as f32 * 1.2,
            0.3,
            z + (k % 2) as f32 * 1.0 - 0.5,
        ));
    }
}

/// A ring of toadstools in a clearing (red caps, white spots).
pub fn fairy_ring(b: &mut MapBuilder, x: f32, z: f32, base: &dyn Fn(f32, f32) -> f32) {
    for k in 0..11 {
        let a = k as f32 * std::f32::consts::TAU / 11.0;
        let (mx, mz) = (x + a.cos() * 3.6, z + a.sin() * 3.6);
        let y = base(mx, mz);
        let s = 0.7 + (k % 3) as f32 * 0.2;
        b.push_detail(
            mx - 0.1 * s,
            y,
            mz - 0.1 * s,
            mx + 0.1 * s,
            y + 0.45 * s,
            mz + 0.1 * s,
            WHITE,
        );
        b.push_detail(
            mx - 0.32 * s,
            y + 0.45 * s,
            mz - 0.32 * s,
            mx + 0.32 * s,
            y + 0.62 * s,
            mz + 0.32 * s,
            [220, 50, 50],
        );
        b.push_detail(
            mx - 0.08,
            y + 0.62 * s,
            mz - 0.08,
            mx + 0.05,
            y + 0.66 * s,
            mz + 0.05,
            WHITE,
        );
    }
    // a giant one in the middle: you can stand on it
    let y = base(x, z);
    b.cbox(x, z, 0.8, 0.8, y, 1.6, WHITE);
    b.cbox(x, z, 3.2, 3.2, y + 1.6, 0.6, [220, 50, 50]);
    for (dx, dz) in [(-0.8f32, -0.6f32), (0.7, 0.5), (0.1, -1.1), (-0.4, 0.9)] {
        b.push_detail(
            x + dx - 0.2,
            y + 2.2,
            z + dz - 0.2,
            x + dx + 0.2,
            y + 2.25,
            z + dz + 0.2,
            WHITE,
        );
    }
}

/// Garden gnomes holding a meeting round a well.
pub fn gnomes(b: &mut MapBuilder, x: f32, z: f32, base: &dyn Fn(f32, f32) -> f32) {
    let y = base(x, z);
    b.cbox(x, z, 1.4, 1.4, y, 0.7, STONE);
    for k in 0..7 {
        let a = k as f32 * std::f32::consts::TAU / 7.0;
        let (gx, gz) = (x + a.cos() * 2.2, z + a.sin() * 2.2);
        let gy = base(gx, gz);
        let coat = [[60, 110, 200], [80, 160, 80], [200, 140, 60]][k % 3];
        b.push_detail(
            gx - 0.14,
            gy,
            gz - 0.12,
            gx + 0.14,
            gy + 0.3,
            gz + 0.12,
            coat,
        );
        b.push_detail(
            gx - 0.1,
            gy + 0.3,
            gz - 0.1,
            gx + 0.1,
            gy + 0.45,
            gz + 0.1,
            [240, 200, 170],
        );
        b.push_detail(
            gx - 0.1,
            gy + 0.22,
            gz - 0.13,
            gx + 0.1,
            gy + 0.36,
            gz - 0.1,
            WHITE,
        );
        b.push_detail(
            gx - 0.09,
            gy + 0.45,
            gz - 0.09,
            gx + 0.09,
            gy + 0.62,
            gz + 0.09,
            [220, 50, 50],
        );
        b.push_detail(
            gx - 0.04,
            gy + 0.62,
            gz - 0.04,
            gx + 0.04,
            gy + 0.72,
            gz + 0.04,
            [220, 50, 50],
        );
    }
}

/// A picnic laid for giants: a table you can climb onto, a chair, an apple, a cup.
pub fn giant_picnic(b: &mut MapBuilder, x: f32, z: f32) {
    let wood = [176, 120, 70];
    for (dx, dz) in [(-3.2, -1.8), (3.2, -1.8), (-3.2, 1.8), (3.2, 1.8)] {
        b.cbox(x + dx, z + dz, 0.6, 0.6, 0.0, 3.0, wood);
    }
    b.block(x - 3.8, 3.0, z - 2.4, x + 3.8, 3.4, z + 2.4, wood);
    // a red and white cloth
    let mut i = 0;
    let mut cx = x - 3.6;
    while cx < x + 3.6 - 0.01 {
        let c = if i % 2 == 0 { [220, 60, 60] } else { WHITE };
        b.decal(cx, z - 2.2, cx + 1.2, z + 2.2, 3.4, c);
        cx += 1.2;
        i += 1;
    }
    b.cbox(x - 1.5, z, 1.6, 1.6, 3.43, 1.5, [210, 40, 40]);
    b.push_detail(
        x - 1.55,
        4.93,
        z - 0.05,
        x - 1.45,
        5.4,
        z + 0.05,
        pal::TRUNK,
    );
    b.push_detail(x - 1.45, 5.2, z - 0.05, x - 1.0, 5.3, z + 0.3, pal::LEAVES);
    b.cbox(x + 1.8, z + 0.5, 1.2, 1.2, 3.43, 1.3, WHITE);
    b.decal(x + 1.3, z, x + 2.3, z + 1.0, 4.73, [110, 70, 40]);
    // the chair: a seat you can jump onto from the table's corner
    let (sx, sz) = (x, z + 4.6);
    for (dx, dz) in [(-1.6, -1.6), (1.6, -1.6), (-1.6, 1.6), (1.6, 1.6)] {
        b.cbox(sx + dx, sz + dz, 0.5, 0.5, 0.0, 2.0, wood);
    }
    b.block(sx - 2.0, 2.0, sz - 2.0, sx + 2.0, 2.4, sz + 2.0, wood);
    b.block(sx - 2.0, 2.4, sz + 1.6, sx + 2.0, 6.0, sz + 2.0, wood);
    b.loot.push(vec3(x + 0.5, 3.7, z - 1.2));
}
