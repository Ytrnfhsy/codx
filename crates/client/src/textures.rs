//! Procedural detail textures for the level: a 4×4 atlas of seamless greyscale tiles
//! (brick, planks, plaster, sand...). The world shader multiplies the baked toon colour by
//! the detail value, so one small texture adds surface variety to every map without any
//! asset files, and the colours stay exactly as the map palette defines them.

use macroquad::prelude::*;
use toon_shared::map::pal;

pub const ATLAS: usize = 512;
const TILE: usize = ATLAS / 4;

// tile ids (0 = untextured)
pub const GRASS: u8 = 1;
pub const SAND: u8 = 2;
pub const ASPHALT: u8 = 3;
pub const CONCRETE: u8 = 4;
pub const PLASTER: u8 = 5;
pub const ROOF: u8 = 6;
pub const METAL: u8 = 7;
pub const WOOD: u8 = 8;
pub const BRICK: u8 = 9;
pub const FABRIC: u8 = 10;
pub const LEAVES: u8 = 11;
pub const GENERIC: u8 = 12;
pub const DIRT: u8 = 13;

/// Detail tile and its size in metres for a palette colour.
pub fn material_for(c: [u8; 3]) -> (u8, f32) {
    let t = match c {
        pal::GRASS => GRASS,
        pal::DIRT => DIRT,
        pal::SAND => SAND,
        pal::SANDBAG => FABRIC,
        pal::ASPHALT => ASPHALT,
        pal::CONCRETE | pal::CONCRETE_DARK => CONCRETE,
        pal::WALL_TAN | pal::WALL_PINK | pal::WALL_BLUE | pal::WALL_WHITE | pal::WALL_GREEN => {
            PLASTER
        }
        pal::ADOBE | pal::ADOBE_LIGHT => PLASTER,
        pal::ROOF_RED => ROOF,
        pal::ROOF_GREY => METAL,
        pal::WOOD | pal::CRATE | pal::TRUNK => WOOD,
        pal::METAL
        | pal::CONT_RED
        | pal::CONT_BLUE
        | pal::CONT_GREEN
        | pal::CONT_ORANGE
        | pal::CONT_YELLOW
        | pal::TANK
        | pal::RUST
        | pal::BARREL_BLUE
        | pal::BARREL_RED => METAL,
        pal::BRICK => BRICK,
        pal::AWNING_A | pal::AWNING_B => FABRIC,
        pal::LEAVES => LEAVES,
        pal::GLASS | pal::PAD_ALLIES | pal::PAD_AXIS | pal::CAR_A | pal::CAR_B | pal::CAR_C => 0,
        _ => GENERIC,
    };
    let scale = match t {
        GRASS => 3.0,
        SAND | DIRT => 4.0,
        ASPHALT | CONCRETE | PLASTER | GENERIC => 3.0,
        ROOF => 2.0,
        METAL => 2.4,
        WOOD => 1.6,
        BRICK => 2.0,
        FABRIC => 1.0,
        LEAVES => 2.0,
        _ => 1.0,
    };
    (t, scale)
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32)
        .wrapping_mul(374761393)
        .wrapping_add((y as u32).wrapping_mul(668265263))
        .wrapping_add(seed.wrapping_mul(2246822519));
    h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    h ^= h >> 16;
    (h & 0xffff) as f32 / 65535.0
}

/// Value noise that tiles with the given period (in lattice cells).
fn vnoise(x: f32, y: f32, period: i32, seed: u32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (fx, fy) = (x - xi as f32, y - yi as f32);
    let s = |v: f32| v * v * (3.0 - 2.0 * v);
    let (sx, sy) = (s(fx), s(fy));
    let h = |a: i32, b: i32| hash(a.rem_euclid(period), b.rem_euclid(period), seed);
    let top = h(xi, yi) + (h(xi + 1, yi) - h(xi, yi)) * sx;
    let bot = h(xi, yi + 1) + (h(xi + 1, yi + 1) - h(xi, yi + 1)) * sx;
    top + (bot - top) * sy
}

/// Seamless fractal noise over a unit tile (u, v in 0..1), roughly 0..1.
fn fbm(u: f32, v: f32, base: i32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut amp, mut norm) = (0.0, 1.0, 0.0);
    let mut p = base;
    for o in 0..octaves {
        sum += vnoise(u * p as f32, v * p as f32, p, seed + o) * amp;
        norm += amp;
        amp *= 0.5;
        p *= 2;
    }
    sum / norm
}

fn grain(px: usize, py: usize, seed: u32) -> f32 {
    hash(px as i32, py as i32, seed) - 0.5
}

/// Detail value (0.5 = neutral) of tile `t` at (u, v); px/py are pixel coords for grain.
fn sample(t: u8, u: f32, v: f32, px: usize, py: usize) -> f32 {
    use std::f32::consts::TAU;
    match t {
        GRASS => {
            let clumps = fbm(u, v, 6, 3, 11) - 0.5;
            let blades = vnoise(u * 48.0, v * 12.0, 48, 12) - 0.5;
            0.5 + clumps * 0.45 + blades * 0.2 + grain(px, py, 13) * 0.08
        }
        SAND => {
            let warp = fbm(u, v, 3, 2, 21) - 0.5;
            let ripple = ((v * 7.0 + warp * 1.4) * TAU).sin();
            0.5 + ripple * 0.035 + (fbm(u, v, 4, 3, 22) - 0.5) * 0.2 + grain(px, py, 23) * 0.12
        }
        DIRT => {
            let blot = fbm(u, v, 4, 4, 31) - 0.5;
            let pebble = if hash(px as i32 / 3, py as i32 / 3, 32) > 0.985 {
                -0.18
            } else {
                0.0
            };
            0.5 + blot * 0.35 + pebble + grain(px, py, 33) * 0.1
        }
        ASPHALT => {
            let crack = (fbm(u, v, 5, 3, 41) - 0.5).abs() < 0.006;
            0.5 + (fbm(u, v, 6, 2, 42) - 0.5) * 0.15
                + grain(px, py, 43) * 0.22
                + if crack { -0.22 } else { 0.0 }
        }
        CONCRETE => {
            let seam = (v * 2.0).fract() < 0.012 || u.fract() < 0.006;
            let pore = hash(px as i32, py as i32, 51) > 0.996;
            0.52 + (fbm(u, v, 3, 4, 52) - 0.5) * 0.22
                + grain(px, py, 53) * 0.06
                + if seam { -0.12 } else { 0.0 }
                + if pore { -0.2 } else { 0.0 }
        }
        PLASTER => {
            let crack = (fbm(u, v, 3, 4, 61) - 0.5).abs() < 0.004 && fbm(u, v, 2, 2, 62) > 0.55;
            0.52 + (fbm(u, v, 2, 4, 63) - 0.5) * 0.24
                + grain(px, py, 64) * 0.05
                + if crack { -0.2 } else { 0.0 }
        }
        ROOF => {
            let rows = 5.0;
            let row = (v * rows).floor();
            let ly = (v * rows).fract();
            let lx = (u * 6.0 + row * 0.5).fract();
            let tile_id = ((u * 6.0 + row * 0.5).floor() as i32, row as i32);
            let bump = (lx * std::f32::consts::PI).sin() * 0.08;
            let edge = if lx < 0.05 || ly > 0.92 { -0.2 } else { 0.0 };
            0.62 - ly * 0.22 + bump + edge + (hash(tile_id.0, tile_id.1, 71) - 0.5) * 0.08
        }
        METAL => {
            let ribs = (u * 10.0 * TAU).sin();
            0.5 + ribs * 0.13 + (fbm(u, v, 4, 3, 81) - 0.5) * 0.18 + grain(px, py, 82) * 0.04
        }
        WOOD => {
            let planks = 4.0;
            let plank = (v * planks).floor();
            let ly = (v * planks).fract();
            let seam = ly < 0.05 || ly > 0.97;
            let warp = fbm(u, v, 2, 2, 91) - 0.5;
            let rings = ((u * 3.0 + warp * 0.6 + plank * 0.37) * TAU * 3.0).sin();
            0.5 + rings * 0.06
                + (hash(plank as i32, 0, 92) - 0.5) * 0.14
                + grain(px, py, 93) * 0.05
                + if seam { -0.25 } else { 0.0 }
        }
        BRICK => {
            let rows = 8.0;
            let row = (v * rows).floor();
            let ly = (v * rows).fract();
            let x = u * 4.0 + if row as i32 % 2 == 1 { 0.5 } else { 0.0 };
            let lx = x.fract();
            if ly < 0.1 || lx < 0.045 {
                0.68 + grain(px, py, 101) * 0.05
            } else {
                0.48 + (hash(x.floor() as i32, row as i32, 102) - 0.5) * 0.16
                    + (fbm(u, v, 8, 2, 103) - 0.5) * 0.1
                    + grain(px, py, 104) * 0.05
            }
        }
        FABRIC => {
            let weave = (u * 32.0 * TAU).sin() * (v * 32.0 * TAU).sin();
            let seam = (v * 2.0).fract() < 0.03;
            0.5 + weave * 0.05 + (fbm(u, v, 3, 3, 111) - 0.5) * 0.2 + if seam { -0.15 } else { 0.0 }
        }
        LEAVES => {
            let n = fbm(u, v, 6, 3, 121);
            0.5 + (n - 0.5) * 0.6 + grain(px, py, 122) * 0.06
        }
        _ => 0.5 + (fbm(u, v, 4, 3, 131) - 0.5) * 0.12 + grain(px, py, 132) * 0.04,
    }
}

/// Builds the 512×512 detail atlas (tile n sits at column (n-1)%4, row (n-1)/4).
pub fn detail_atlas() -> Texture2D {
    let mut img = vec![0u8; ATLAS * ATLAS * 4];
    for t in 1..=16u8 {
        let cx = ((t - 1) % 4) as usize * TILE;
        let cy = ((t - 1) / 4) as usize * TILE;
        for y in 0..TILE {
            for x in 0..TILE {
                let u = x as f32 / TILE as f32;
                let v = y as f32 / TILE as f32;
                let d = sample(t, u, v, x, y).clamp(0.0, 1.0);
                let b = (d * 255.0) as u8;
                let i = ((cy + y) * ATLAS + cx + x) * 4;
                img[i..i + 4].copy_from_slice(&[b, b, b, 255]);
            }
        }
    }
    let tex = Texture2D::from_rgba8(ATLAS as u16, ATLAS as u16, &img);
    tex.set_filter(FilterMode::Linear);
    tex
}
