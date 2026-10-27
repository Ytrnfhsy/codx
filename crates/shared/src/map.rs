//! Map data model and a small builder for blocky "toon" level geometry.

use glam::{vec3, Vec2, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MapId {
    Depot,
    Market,
    Cargo,
    Wreck,
    Site,
}

impl MapId {
    pub const ALL: [MapId; 5] = [
        MapId::Wreck,
        MapId::Site,
        MapId::Depot,
        MapId::Market,
        MapId::Cargo,
    ];
    pub fn short(self) -> &'static str {
        match self {
            MapId::Depot => "depot",
            MapId::Market => "market",
            MapId::Cargo => "cargo",
            MapId::Wreck => "wreck",
            MapId::Site => "site",
        }
    }
    pub fn parse(s: &str) -> Option<MapId> {
        let s = s.to_ascii_lowercase();
        MapId::ALL
            .into_iter()
            .find(|m| m.short() == s || crate::maps::build(*m).name.to_lowercase() == s)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct MapBox {
    pub min: Vec3,
    pub max: Vec3,
    pub color: [u8; 3],
    /// Collides with players and bullets.
    pub solid: bool,
    /// Rendered.
    pub visible: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Spawn {
    pub pos: Vec3,
    pub yaw: f32,
}

#[derive(Clone, Debug)]
pub struct Map {
    pub id: MapId,
    pub name: &'static str,
    pub min: Vec2,
    pub max: Vec2,
    pub boxes: Vec<MapBox>,
    /// Side spawns, indexed by `Team::index()`.
    pub spawns: [Vec<Spawn>; 2],
    /// Sabotage bomb spawn.
    pub bomb: Vec3,
    /// Sabotage sites indexed by the team that *defends* them.
    pub sites: [Vec3; 2],
    pub sky: [u8; 3],
    pub fog: [u8; 3],
}

impl Map {
    pub fn all_spawns(&self) -> impl Iterator<Item = &Spawn> {
        self.spawns[0].iter().chain(self.spawns[1].iter())
    }
    pub fn size(&self) -> Vec2 {
        self.max - self.min
    }
    pub fn center(&self) -> Vec2 {
        (self.min + self.max) * 0.5
    }
    /// Colour of the ground slab (used for the terrain outside the playable area).
    pub fn ground_color(&self) -> [u8; 3] {
        self.boxes
            .iter()
            .find(|b| b.min.y < -0.5)
            .map(|b| b.color)
            .unwrap_or([120, 170, 90])
    }
}

/// An opening in a wall, measured along the wall and from the wall's base.
#[derive(Clone, Copy, Debug)]
pub struct Opening {
    pub at: f32,
    pub w: f32,
    pub bottom: f32,
    pub top: f32,
}

pub fn door(at: f32) -> Opening {
    Opening {
        at,
        w: 1.7,
        bottom: 0.0,
        top: 2.5,
    }
}
pub fn wide_door(at: f32) -> Opening {
    Opening {
        at,
        w: 3.2,
        bottom: 0.0,
        top: 3.0,
    }
}
pub fn window(at: f32) -> Opening {
    Opening {
        at,
        w: 1.5,
        bottom: 1.05,
        top: 2.1,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    MinX,
    MaxX,
    MinZ,
    MaxZ,
}

pub mod pal {
    pub const GRASS: [u8; 3] = [118, 178, 86];
    pub const DIRT: [u8; 3] = [196, 168, 116];
    pub const ASPHALT: [u8; 3] = [92, 96, 108];
    pub const CONCRETE: [u8; 3] = [196, 192, 182];
    pub const WALL_TAN: [u8; 3] = [232, 200, 150];
    pub const WALL_PINK: [u8; 3] = [236, 168, 150];
    pub const WALL_BLUE: [u8; 3] = [150, 190, 226];
    pub const WALL_WHITE: [u8; 3] = [238, 234, 222];
    pub const WALL_GREEN: [u8; 3] = [164, 206, 150];
    pub const ROOF_RED: [u8; 3] = [196, 82, 66];
    pub const ROOF_GREY: [u8; 3] = [120, 120, 132];
    pub const WOOD: [u8; 3] = [178, 124, 72];
    pub const CRATE: [u8; 3] = [206, 150, 82];
    pub const METAL: [u8; 3] = [140, 150, 160];
    pub const SANDBAG: [u8; 3] = [210, 190, 136];
    pub const CONT_RED: [u8; 3] = [214, 76, 64];
    pub const CONT_BLUE: [u8; 3] = [64, 120, 206];
    pub const CONT_GREEN: [u8; 3] = [80, 168, 96];
    pub const CONT_ORANGE: [u8; 3] = [236, 146, 50];
    pub const CONT_YELLOW: [u8; 3] = [240, 200, 60];
    pub const TANK: [u8; 3] = [226, 226, 226];
    pub const PAD_ALLIES: [u8; 3] = [90, 140, 230];
    pub const PAD_AXIS: [u8; 3] = [230, 90, 80];
    pub const AWNING_A: [u8; 3] = [240, 96, 96];
    pub const AWNING_B: [u8; 3] = [96, 196, 230];
    pub const CAR_A: [u8; 3] = [250, 210, 70];
    pub const CAR_B: [u8; 3] = [110, 200, 140];
    pub const CAR_C: [u8; 3] = [230, 120, 190];
    pub const GLASS: [u8; 3] = [150, 210, 240];
    pub const DARK: [u8; 3] = [70, 70, 80];
    pub const SAND: [u8; 3] = [222, 196, 142];
    pub const ADOBE: [u8; 3] = [214, 172, 120];
    pub const ADOBE_LIGHT: [u8; 3] = [236, 206, 160];
    pub const HELI_OLIVE: [u8; 3] = [112, 124, 78];
    pub const BURNT: [u8; 3] = [60, 54, 50];
    pub const RUST: [u8; 3] = [168, 92, 52];
    pub const BARREL_BLUE: [u8; 3] = [60, 110, 190];
    pub const BARREL_RED: [u8; 3] = [200, 64, 52];
    pub const CONCRETE_DARK: [u8; 3] = [150, 150, 146];
    pub const SCAFFOLD: [u8; 3] = [240, 190, 40];
    pub const LEAVES: [u8; 3] = [92, 168, 84];
    pub const TRUNK: [u8; 3] = [130, 90, 60];
    pub const BRICK: [u8; 3] = [190, 92, 72];
}

#[derive(Default)]
pub struct MapBuilder {
    pub boxes: Vec<MapBox>,
}

impl MapBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    fn push(&mut self, a: Vec3, b: Vec3, color: [u8; 3], solid: bool, visible: bool) {
        let min = a.min(b);
        let max = a.max(b);
        if (max - min).min_element() <= 1e-4 {
            return;
        }
        self.boxes.push(MapBox {
            min,
            max,
            color,
            solid,
            visible,
        });
    }

    /// Solid visible box from two corners.
    pub fn block(&mut self, x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32, c: [u8; 3]) {
        self.push(vec3(x0, y0, z0), vec3(x1, y1, z1), c, true, true);
    }

    /// Non-solid decal-like box (floor paint, pads).
    pub fn decal(&mut self, x0: f32, z0: f32, x1: f32, z1: f32, y: f32, c: [u8; 3]) {
        self.push(vec3(x0, y, z0), vec3(x1, y + 0.03, z1), c, false, true);
    }

    /// Invisible collision.
    pub fn clip(&mut self, x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32) {
        self.push(vec3(x0, y0, z0), vec3(x1, y1, z1), [0, 0, 0], true, false);
    }

    /// Centered box standing on `y0`.
    pub fn cbox(&mut self, cx: f32, cz: f32, w: f32, d: f32, y0: f32, h: f32, c: [u8; 3]) {
        self.block(
            cx - w / 2.0,
            y0,
            cz - d / 2.0,
            cx + w / 2.0,
            y0 + h,
            cz + d / 2.0,
            c,
        );
    }

    /// Wall along X (from x0 to x1) centered on `z`, with openings.
    pub fn wall_x(
        &mut self,
        x0: f32,
        x1: f32,
        z: f32,
        y0: f32,
        h: f32,
        t: f32,
        ops: &[Opening],
        c: [u8; 3],
    ) {
        for (a, b, ya, yb) in wall_segments(x0, x1, h, ops) {
            self.block(a, y0 + ya, z - t / 2.0, b, y0 + yb, z + t / 2.0, c);
        }
    }

    /// Wall along Z (from z0 to z1) centered on `x`, with openings.
    pub fn wall_z(
        &mut self,
        z0: f32,
        z1: f32,
        x: f32,
        y0: f32,
        h: f32,
        t: f32,
        ops: &[Opening],
        c: [u8; 3],
    ) {
        for (a, b, ya, yb) in wall_segments(z0, z1, h, ops) {
            self.block(x - t / 2.0, y0 + ya, a, x + t / 2.0, y0 + yb, b, c);
        }
    }

    /// Four walls (inside the rectangle) with openings per side, optional roof.
    #[allow(clippy::too_many_arguments)]
    pub fn building(
        &mut self,
        x0: f32,
        z0: f32,
        x1: f32,
        z1: f32,
        y0: f32,
        h: f32,
        ops: &[(Side, Opening)],
        c: [u8; 3],
        roof: Option<[u8; 3]>,
    ) {
        let t = 0.3;
        let pick = |s: Side| {
            ops.iter()
                .filter(|o| o.0 == s)
                .map(|o| o.1)
                .collect::<Vec<_>>()
        };
        self.wall_x(x0, x1, z0 + t / 2.0, y0, h, t, &pick(Side::MinZ), c);
        self.wall_x(x0, x1, z1 - t / 2.0, y0, h, t, &pick(Side::MaxZ), c);
        self.wall_z(z0 + t, z1 - t, x0 + t / 2.0, y0, h, t, &pick(Side::MinX), c);
        self.wall_z(z0 + t, z1 - t, x1 - t / 2.0, y0, h, t, &pick(Side::MaxX), c);
        if let Some(rc) = roof {
            self.block(
                x0 - 0.25,
                y0 + h,
                z0 - 0.25,
                x1 + 0.25,
                y0 + h + 0.35,
                z1 + 0.25,
                rc,
            );
        }
    }

    /// Floor slab with a rectangular hole (for stairs).
    #[allow(clippy::too_many_arguments)]
    pub fn floor_with_hole(
        &mut self,
        x0: f32,
        z0: f32,
        x1: f32,
        z1: f32,
        y: f32,
        hole: (f32, f32, f32, f32),
        c: [u8; 3],
    ) {
        let (hx0, hz0, hx1, hz1) = hole;
        let t = 0.25;
        self.block(x0, y - t, z0, x1, y, hz0, c);
        self.block(x0, y - t, hz1, x1, y, z1, c);
        self.block(x0, y - t, hz0, hx0, y, hz1, c);
        self.block(hx1, y - t, hz0, x1, y, hz1, c);
    }

    /// Solid staircase. `dir` is one of (±1,0)/(0,±1): direction of ascent.
    #[allow(clippy::too_many_arguments)]
    pub fn stairs(
        &mut self,
        x: f32,
        z: f32,
        dir: (f32, f32),
        width: f32,
        steps: u32,
        rise: f32,
        run: f32,
        y0: f32,
        c: [u8; 3],
    ) {
        for i in 0..steps {
            let a = i as f32 * run;
            let b = a + run;
            let top = y0 + rise * (i + 1) as f32;
            if dir.0 != 0.0 {
                let (xa, xb) = (x + dir.0 * a, x + dir.0 * b);
                self.block(xa, y0, z - width / 2.0, xb, top, z + width / 2.0, c);
            } else {
                let (za, zb) = (z + dir.1 * a, z + dir.1 * b);
                self.block(x - width / 2.0, y0, za, x + width / 2.0, top, zb, c);
            }
        }
    }

    /// Shipping container (6 × 2.6 × 2.45).
    pub fn container(&mut self, cx: f32, cz: f32, along_x: bool, y0: f32, c: [u8; 3]) {
        let (w, d) = if along_x { (6.0, 2.45) } else { (2.45, 6.0) };
        self.cbox(cx, cz, w, d, y0, 2.6, c);
        // ribs to make it look like a container
        let rib = [
            (c[0] as f32 * 0.8) as u8,
            (c[1] as f32 * 0.8) as u8,
            (c[2] as f32 * 0.8) as u8,
        ];
        if along_x {
            for k in [-2.0, 0.0, 2.0] {
                self.push(
                    vec3(cx + k - 0.1, y0 + 0.1, cz - d / 2.0 - 0.05),
                    vec3(cx + k + 0.1, y0 + 2.5, cz + d / 2.0 + 0.05),
                    rib,
                    false,
                    true,
                );
            }
        } else {
            for k in [-2.0, 0.0, 2.0] {
                self.push(
                    vec3(cx - w / 2.0 - 0.05, y0 + 0.1, cz + k - 0.1),
                    vec3(cx + w / 2.0 + 0.05, y0 + 2.5, cz + k + 0.1),
                    rib,
                    false,
                    true,
                );
            }
        }
    }

    pub fn crate_(&mut self, cx: f32, cz: f32, s: f32, y0: f32) {
        self.cbox(cx, cz, s, s, y0, s, pal::CRATE);
    }

    /// Simple toon car: body + cabin.
    pub fn car(&mut self, cx: f32, cz: f32, along_x: bool, c: [u8; 3]) {
        let (w, d) = if along_x { (4.2, 1.9) } else { (1.9, 4.2) };
        self.cbox(cx, cz, w, d, 0.0, 0.25, pal::DARK);
        self.cbox(cx, cz, w, d, 0.25, 0.85, c);
        let (cw, cd) = if along_x { (2.2, 1.7) } else { (1.7, 2.2) };
        self.cbox(cx, cz, cw, cd, 1.1, 0.7, pal::GLASS);
        self.cbox(cx, cz, cw + 0.05, cd + 0.05, 1.8, 0.08, c);
    }

    /// Low sandbag wall.
    pub fn sandbags(&mut self, x0: f32, z0: f32, x1: f32, z1: f32) {
        self.block(x0, 0.0, z0, x1, 1.05, z1, pal::SANDBAG);
    }

    /// Big white fuel tank (sabotage site decoration).
    pub fn tank(&mut self, cx: f32, cz: f32) {
        self.cbox(cx, cz, 2.2, 2.2, 0.0, 3.0, pal::TANK);
        self.cbox(cx, cz, 1.6, 1.6, 3.0, 0.4, pal::METAL);
    }

    /// Ground plane and perimeter walls.
    pub fn ground_and_bounds(
        &mut self,
        min: Vec2,
        max: Vec2,
        ground: [u8; 3],
        wall: [u8; 3],
        wall_h: f32,
    ) {
        self.block(
            min.x - 2.0,
            -1.0,
            min.y - 2.0,
            max.x + 2.0,
            0.0,
            max.y + 2.0,
            ground,
        );
        let t = 1.0;
        self.block(min.x - t, 0.0, min.y - t, max.x + t, wall_h, min.y, wall);
        self.block(min.x - t, 0.0, max.y, max.x + t, wall_h, max.y + t, wall);
        self.block(min.x - t, 0.0, min.y, min.x, wall_h, max.y, wall);
        self.block(max.x, 0.0, min.y, max.x + t, wall_h, max.y, wall);
        // invisible tall clip so nobody jumps out
        self.clip(min.x - t, wall_h, min.y - t, max.x + t, 40.0, min.y);
        self.clip(min.x - t, wall_h, max.y, max.x + t, 40.0, max.y + t);
        self.clip(min.x - t, wall_h, min.y, min.x, 40.0, max.y);
        self.clip(max.x, wall_h, min.y, max.x + t, 40.0, max.y);
    }
}

/// Splits a wall into solid pieces around openings: (from, to, y_from, y_to) relative to wall base.
fn wall_segments(a: f32, b: f32, h: f32, ops: &[Opening]) -> Vec<(f32, f32, f32, f32)> {
    let mut ops = ops.to_vec();
    ops.sort_by(|p, q| p.at.partial_cmp(&q.at).unwrap());
    let mut out = Vec::new();
    let mut cur = a;
    for o in ops {
        let o0 = (o.at - o.w / 2.0).max(a);
        let o1 = (o.at + o.w / 2.0).min(b);
        if o1 <= o0 {
            continue;
        }
        if o0 > cur {
            out.push((cur, o0, 0.0, h));
        }
        if o.bottom > 0.0 {
            out.push((o0, o1, 0.0, o.bottom));
        }
        if o.top < h {
            out.push((o0, o1, o.top, h));
        }
        cur = o1;
    }
    if b > cur {
        out.push((cur, b, 0.0, h));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wall_with_door_and_window() {
        let segs = wall_segments(0.0, 10.0, 3.0, &[door(2.0), window(7.0)]);
        // left piece, above door, middle piece, below window, above window, right piece
        assert_eq!(segs.len(), 6);
        assert!(segs.iter().all(|s| s.1 > s.0 && s.3 > s.2));
    }
}
