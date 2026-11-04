//! Height-map terrain for big outdoor maps: hills, mountains, river valleys. The ground is
//! a regular grid of heights; between grid points it is two triangles per cell, so the
//! physics, the bullets and the renderer all see exactly the same surface.
//!
//! Maps without terrain stay flat (their ground is a box, as before).

use glam::{vec2, vec3, Vec2, Vec3};

#[derive(Clone, Debug)]
pub struct Heightmap {
    /// World position of grid point (0, 0) (x, z).
    pub origin: Vec2,
    /// Distance between grid points (m).
    pub cell: f32,
    /// Grid points along x and z.
    pub w: usize,
    pub h: usize,
    pub heights: Vec<f32>,
    /// Water surface: ground below it is under water (rivers, lakes).
    pub water: f32,
    /// What covers each grid point (see [`paint`]); 0 lets the renderer choose by height
    /// and slope (grass, sand, rock, snow).
    pub paint: Vec<u8>,
}

/// Ground cover painted onto grid points.
pub mod paint {
    pub const AUTO: u8 = 0;
    pub const ROAD: u8 = 1;
    pub const DIRT: u8 = 2;
    pub const FIELD_A: u8 = 3;
    pub const FIELD_B: u8 = 4;
    pub const PAVED: u8 = 5;
    pub const SAND: u8 = 6;
    pub const FOREST: u8 = 7;
    pub const MEADOW: u8 = 8;
}

/// Steepest ground you can walk up (rise over run): about 45°.
pub const MAX_WALK_SLOPE: f32 = 1.0;

impl Heightmap {
    /// A flat height map covering `min..max` at `cell` spacing.
    pub fn flat(min: Vec2, max: Vec2, cell: f32, y: f32) -> Heightmap {
        let w = ((max.x - min.x) / cell).ceil() as usize + 1;
        let h = ((max.y - min.y) / cell).ceil() as usize + 1;
        Heightmap {
            origin: min,
            cell,
            w,
            h,
            heights: vec![y; w * h],
            water: f32::NEG_INFINITY,
            paint: vec![paint::AUTO; w * h],
        }
    }

    /// Height of grid point (i, j), clamped to the grid.
    pub fn at(&self, i: i64, j: i64) -> f32 {
        let i = i.clamp(0, self.w as i64 - 1) as usize;
        let j = j.clamp(0, self.h as i64 - 1) as usize;
        self.heights[j * self.w + i]
    }

    /// Ground cover of grid point (i, j), clamped to the grid.
    pub fn paint_at(&self, i: i64, j: i64) -> u8 {
        let i = i.clamp(0, self.w as i64 - 1) as usize;
        let j = j.clamp(0, self.h as i64 - 1) as usize;
        self.paint[j * self.w + i]
    }

    /// Grid point nearest to world (x, z), if inside the grid.
    pub fn index_of(&self, x: f32, z: f32) -> Option<(usize, usize)> {
        let i = ((x - self.origin.x) / self.cell).round();
        let j = ((z - self.origin.y) / self.cell).round();
        (i >= 0.0 && j >= 0.0 && (i as usize) < self.w && (j as usize) < self.h)
            .then_some((i as usize, j as usize))
    }

    pub fn set(&mut self, i: usize, j: usize, y: f32) {
        if i < self.w && j < self.h {
            self.heights[j * self.w + i] = y;
        }
    }

    /// World position of grid point (i, j).
    pub fn point(&self, i: usize, j: usize) -> Vec3 {
        vec3(
            self.origin.x + i as f32 * self.cell,
            self.at(i as i64, j as i64),
            self.origin.y + j as f32 * self.cell,
        )
    }

    /// Ground height at (x, z): on the cell's two triangles (split along the diagonal from
    /// (i, j) to (i+1, j+1)), the same surface the renderer draws.
    pub fn height(&self, x: f32, z: f32) -> f32 {
        let fx = (x - self.origin.x) / self.cell;
        let fz = (z - self.origin.y) / self.cell;
        let i = fx.floor();
        let j = fz.floor();
        let (u, v) = (fx - i, fz - j);
        let (i, j) = (i as i64, j as i64);
        let h00 = self.at(i, j);
        let h11 = self.at(i + 1, j + 1);
        if u >= v {
            let h10 = self.at(i + 1, j);
            h00 + (h10 - h00) * u + (h11 - h10) * v
        } else {
            let h01 = self.at(i, j + 1);
            h00 + (h11 - h01) * u + (h01 - h00) * v
        }
    }

    /// Surface normal at (x, z).
    pub fn normal(&self, x: f32, z: f32) -> Vec3 {
        let e = self.cell * 0.5;
        let dx = self.height(x + e, z) - self.height(x - e, z);
        let dz = self.height(x, z + e) - self.height(x, z - e);
        vec3(-dx, 2.0 * e, -dz).normalize()
    }

    /// Steepness at (x, z): rise over run.
    pub fn slope(&self, x: f32, z: f32) -> f32 {
        let n = self.normal(x, z);
        (1.0 - n.y * n.y).max(0.0).sqrt() / n.y.max(1e-3)
    }

    /// World rectangle covered (x, z).
    pub fn bounds(&self) -> (Vec2, Vec2) {
        (
            self.origin,
            self.origin + vec2((self.w - 1) as f32, (self.h - 1) as f32) * self.cell,
        )
    }

    /// Where a ray from `o` along `d` (unit) first meets the ground, within `max`:
    /// distance and normal. It steps in proportion to its height above the ground (big
    /// steps high up, small ones near the surface) and then bisects to the crossing.
    pub fn raycast(&self, o: Vec3, d: Vec3, max: f32) -> Option<(f32, Vec3)> {
        let above = |t: f32| {
            let p = o + d * t;
            p.y - self.height(p.x, p.z)
        };
        let mut t = 0.0f32;
        let mut a = above(0.0);
        if a < 0.0 {
            return Some((0.0, self.normal(o.x, o.z)));
        }
        // the ground is never steeper than ~3:1 here, so this step cannot jump through it
        let min_step = self.cell * 0.25;
        while t < max {
            let step = (a * 0.3).max(min_step).min(max - t + 1e-3);
            let nt = t + step;
            let na = above(nt);
            if na < 0.0 {
                let (mut lo, mut hi) = (t, nt);
                for _ in 0..12 {
                    let mid = (lo + hi) * 0.5;
                    if above(mid) < 0.0 {
                        hi = mid;
                    } else {
                        lo = mid;
                    }
                }
                if hi > max {
                    return None;
                }
                let p = o + d * hi;
                return Some((hi, self.normal(p.x, p.z)));
            }
            t = nt;
            a = na;
        }
        None
    }

    /// Makes the ground inside the rectangle `min..max` level at `y`, blending into the
    /// surrounding terrain over `blend` metres (for towns and fields).
    pub fn flatten(&mut self, min: Vec2, max: Vec2, y: f32, blend: f32) {
        for j in 0..self.h {
            for i in 0..self.w {
                let p = self.point(i, j);
                let dx = (min.x - p.x).max(p.x - max.x).max(0.0);
                let dz = (min.y - p.z).max(p.z - max.y).max(0.0);
                let d = (dx * dx + dz * dz).sqrt();
                if d >= blend {
                    continue;
                }
                let k = if d <= 0.0 {
                    1.0
                } else {
                    let s = 1.0 - d / blend;
                    s * s * (3.0 - 2.0 * s)
                };
                self.heights[j * self.w + i] = p.y + (y - p.y) * k;
            }
        }
    }
}

/// Smooth value noise in 0..1 (for generating terrain), repeatable for a seed.
pub fn noise(seed: u32, x: f32, z: f32) -> f32 {
    let hash = |i: i32, j: i32| -> f32 {
        let mut h = (i as u32).wrapping_mul(374_761_393)
            ^ (j as u32).wrapping_mul(668_265_263)
            ^ seed.wrapping_mul(2_246_822_519);
        h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
        h ^= h >> 16;
        (h & 0xffff) as f32 / 65535.0
    };
    let (i, j) = (x.floor() as i32, z.floor() as i32);
    let (u, v) = (x - x.floor(), z - z.floor());
    let s = |t: f32| t * t * (3.0 - 2.0 * t);
    let (su, sv) = (s(u), s(v));
    let a = hash(i, j) + (hash(i + 1, j) - hash(i, j)) * su;
    let b = hash(i, j + 1) + (hash(i + 1, j + 1) - hash(i, j + 1)) * su;
    a + (b - a) * sv
}

/// Several octaves of noise (fractal), 0..1.
pub fn fbm(seed: u32, x: f32, z: f32, octaves: u32) -> f32 {
    let (mut sum, mut amp, mut freq, mut norm) = (0.0, 1.0, 1.0, 0.0);
    for k in 0..octaves {
        sum += noise(seed.wrapping_add(k * 101), x * freq, z * freq) * amp;
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hills() -> Heightmap {
        let mut m = Heightmap::flat(vec2(-50.0, -50.0), vec2(50.0, 50.0), 2.0, 0.0);
        for j in 0..m.h {
            for i in 0..m.w {
                let p = m.point(i, j);
                m.set(i, j, 6.0 * fbm(3, p.x * 0.03, p.z * 0.03, 4) + 0.05 * p.x);
            }
        }
        m
    }

    #[test]
    fn height_matches_grid_points_and_is_continuous() {
        let m = hills();
        for (i, j) in [(0, 0), (10, 7), (49, 50)] {
            let p = m.point(i, j);
            assert!((m.height(p.x, p.z) - p.y).abs() < 1e-4);
        }
        // no jumps between neighbouring samples
        let mut last = m.height(-40.0, 3.3);
        let mut x = -40.0;
        while x < 40.0 {
            x += 0.05;
            let h = m.height(x, 3.3);
            assert!((h - last).abs() < 0.1, "jump at {x}");
            last = h;
        }
    }

    #[test]
    fn rays_land_on_the_surface() {
        let m = hills();
        let mut rng = crate::rng::Rng::new(5);
        for _ in 0..500 {
            let o = vec3(
                rng.range(-40.0, 40.0),
                rng.range(8.0, 20.0),
                rng.range(-40.0, 40.0),
            );
            let d = vec3(
                rng.range(-1.0, 1.0),
                rng.range(-1.0, -0.05),
                rng.range(-1.0, 1.0),
            )
            .normalize();
            if let Some((t, n)) = m.raycast(o, d, 200.0) {
                let p = o + d * t;
                assert!((p.y - m.height(p.x, p.z)).abs() < 0.02, "{p}");
                assert!(n.y > 0.0);
            }
        }
        // straight down always hits, straight up never
        assert!(m
            .raycast(vec3(1.0, 30.0, 1.0), vec3(0.0, -1.0, 0.0), 100.0)
            .is_some());
        assert!(m
            .raycast(vec3(1.0, 30.0, 1.0), vec3(0.0, 1.0, 0.0), 100.0)
            .is_none());
    }

    #[test]
    fn flatten_levels_a_town() {
        let mut m = hills();
        m.flatten(vec2(-10.0, -10.0), vec2(10.0, 10.0), 3.0, 8.0);
        assert!((m.height(0.0, 0.0) - 3.0).abs() < 1e-4);
        assert!((m.height(9.0, -9.0) - 3.0).abs() < 1e-4);
    }
}
