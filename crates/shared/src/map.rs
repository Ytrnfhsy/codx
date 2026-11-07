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
    /// 1 × 1 km battle royale test island (not in the normal rotation).
    Island,
}

impl MapId {
    pub const ALL: [MapId; 5] = [
        MapId::Wreck,
        MapId::Site,
        MapId::Depot,
        MapId::Market,
        MapId::Cargo,
    ];
    /// Every map, the big test island included.
    pub const EVERY: [MapId; 6] = [
        MapId::Wreck,
        MapId::Site,
        MapId::Depot,
        MapId::Market,
        MapId::Cargo,
        MapId::Island,
    ];
    pub fn short(self) -> &'static str {
        match self {
            MapId::Depot => "depot",
            MapId::Market => "market",
            MapId::Cargo => "cargo",
            MapId::Wreck => "wreck",
            MapId::Site => "site",
            MapId::Island => "island",
        }
    }
    /// The map's name in the current language.
    pub fn name(self) -> &'static str {
        use crate::lang::t;
        match self {
            MapId::Depot => t("Депо", "Depot"),
            MapId::Market => t("Ринок", "Market"),
            MapId::Cargo => t("Вантаж", "Cargo"),
            MapId::Wreck => t("Аварія", "Wreck"),
            MapId::Site => t("Будмайданчик", "Building Site"),
            MapId::Island => t("Острів", "Island"),
        }
    }
    pub fn parse(s: &str) -> Option<MapId> {
        let s = s.to_ascii_lowercase();
        MapId::EVERY.into_iter().find(|m| {
            m.short() == s
                || crate::lang::with(crate::lang::Lang::Uk, || m.name().to_lowercase()) == s
                || crate::lang::with(crate::lang::Lang::En, || m.name().to_lowercase()) == s
        })
    }
}

/// What a box is made of, for bullets and movement.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Mat {
    /// Brick, concrete, metal: stops bullets.
    #[default]
    Solid,
    /// Thin planks (fences, stall boards, doors): bullets go through, a little weaker;
    /// players cannot.
    Wood,
    /// Awnings, sheets, laundry: bullets go through almost unhurt, players too, but you
    /// cannot see through.
    Cloth,
}

impl Mat {
    /// Share of a bullet's damage left after going through, or None if it stops it.
    pub fn bullet_pass(self) -> Option<f32> {
        match self {
            Mat::Solid => None,
            Mat::Wood => Some(0.7),
            Mat::Cloth => Some(0.95),
        }
    }
    /// Blocks players walking into it (and holds them up).
    pub fn blocks_movement(self) -> bool {
        self != Mat::Cloth
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
    pub mat: Mat,
}

/// A ladder up a wall. Players climb it with W/S (or walk off a roof onto it); the
/// climbing column stands in front of the wall.
#[derive(Clone, Copy, Debug)]
pub struct Ladder {
    /// Foot of the ladder, where the climber stands (on the ground in front of the wall).
    pub base: Vec3,
    /// Unit vector from the wall towards the climber (horizontal).
    pub out: Vec3,
    /// Height of the floor at the top (the roof).
    pub top: f32,
}

impl Ladder {
    /// The climbing column: (centre of its floor print, half width across, reach).
    /// A point counts as on the ladder if it is within `half_w` across and between the
    /// wall and `reach` in front of it, from the bottom to a little above the top.
    pub fn holds(&self, p: Vec3) -> bool {
        let d = p - self.base;
        let along = d.dot(self.out);
        let across = d.dot(vec3(-self.out.z, 0.0, self.out.x));
        across.abs() <= 0.55
            && (-0.75..=0.45).contains(&along)
            && p.y >= self.base.y - 0.1
            && p.y <= self.top + 0.4
    }
    /// A point on the roof just past the top of the ladder.
    pub fn exit(&self) -> Vec3 {
        let p = self.base - self.out * 1.6;
        vec3(p.x, self.top, p.z)
    }
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
    pub ladders: Vec<Ladder>,
    /// Side spawns, indexed by `Team::index()`.
    pub spawns: [Vec<Spawn>; 2],
    /// Sabotage bomb spawn.
    pub bomb: Vec3,
    /// Sabotage sites indexed by the team that *defends* them.
    pub sites: [Vec3; 2],
    pub sky: [u8; 3],
    pub fog: [u8; 3],
    /// Colour of the light (multiplies everything lit): warm evening sun, cool overcast…
    pub tint: [f32; 3],
    /// Hills and valleys (big outdoor maps); None = flat ground.
    pub terrain: Option<std::sync::Arc<crate::terrain::Heightmap>>,
    /// Where battle royale supplies may lie (floors inside buildings and the like).
    pub loot: Vec<Vec3>,
    /// Big maps: areas (min x, min z, max x, max z) with buildings, where the bots'
    /// navigation needs a fine grid (doors, staircases).
    pub nav_detail: Vec<[f32; 4]>,
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
    pub const TYRE: [u8; 3] = [44, 44, 50];
    pub const LIGHT: [u8; 3] = [255, 240, 170];
    pub const FENCE: [u8; 3] = [70, 130, 190];
    pub const CABIN: [u8; 3] = [228, 230, 226];
    pub const WINDOW: [u8; 3] = [60, 80, 100];
}

/// A colour darkened (or lightened) by `k`.
pub fn shade(c: [u8; 3], k: f32) -> [u8; 3] {
    [
        (c[0] as f32 * k).min(255.0) as u8,
        (c[1] as f32 * k).min(255.0) as u8,
        (c[2] as f32 * k).min(255.0) as u8,
    ]
}

#[derive(Default)]
pub struct MapBuilder {
    pub boxes: Vec<MapBox>,
    pub ladders: Vec<Ladder>,
    pub loot: Vec<Vec3>,
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
            mat: Mat::Solid,
        });
    }

    /// Solid visible box from two corners.
    pub fn block(&mut self, x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32, c: [u8; 3]) {
        self.push(vec3(x0, y0, z0), vec3(x1, y1, z1), c, true, true);
    }

    /// Thin planks bullets go through (fences, stall boards, doors).
    pub fn wood(&mut self, x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32, c: [u8; 3]) {
        self.block_of(Mat::Wood, x0, y0, z0, x1, y1, z1, c);
    }

    /// Cloth (awnings, sheets): bullets and players pass, eyes do not.
    pub fn cloth(&mut self, x0: f32, y0: f32, z0: f32, x1: f32, y1: f32, z1: f32, c: [u8; 3]) {
        self.block_of(Mat::Cloth, x0, y0, z0, x1, y1, z1, c);
    }

    #[allow(clippy::too_many_arguments)]
    fn block_of(
        &mut self,
        m: Mat,
        x0: f32,
        y0: f32,
        z0: f32,
        x1: f32,
        y1: f32,
        z1: f32,
        c: [u8; 3],
    ) {
        let n = self.boxes.len();
        self.block(x0, y0, z0, x1, y1, z1, c);
        if let Some(b) = self.boxes.get_mut(n) {
            b.mat = m;
        }
    }

    /// Builds what `f` adds `h` higher up (houses, cars, trees on a terrace or a hill).
    pub fn lift(&mut self, h: f32, f: impl FnOnce(&mut Self)) {
        let (nb, nl, no) = (self.boxes.len(), self.ladders.len(), self.loot.len());
        f(self);
        for p in &mut self.loot[no..] {
            p.y += h;
        }
        for b in &mut self.boxes[nb..] {
            b.min.y += h;
            b.max.y += h;
        }
        for l in &mut self.ladders[nl..] {
            l.base.y += h;
            l.top += h;
        }
    }

    /// A raised block of ground from y = 0 up to `h`: its sides are a retaining wall of
    /// `wall`, its top is paved with `top`.
    #[allow(clippy::too_many_arguments)]
    pub fn terrace(
        &mut self,
        x0: f32,
        z0: f32,
        x1: f32,
        z1: f32,
        h: f32,
        wall: [u8; 3],
        top: [u8; 3],
    ) {
        self.block(x0, 0.0, z0, x1, h, z1, wall);
        self.decal(x0 + 0.02, z0 + 0.02, x1 - 0.02, z1 - 0.02, h, top);
    }

    /// A ladder against a wall: `face` is a point on the wall's outer face at the foot of
    /// the ladder, `out` points away from the wall, `top` is the roof's floor height.
    /// Roofs overhang their walls a little, so the ladder stands 0.4 m off the wall.
    pub fn ladder(&mut self, face: Vec3, out: Vec3, top: f32) {
        let out = vec3(out.x, 0.0, out.z).normalize_or(vec3(1.0, 0.0, 0.0));
        let across = vec3(-out.z, 0.0, out.x);
        let foot = face + out * 0.4;
        let c = [92, 70, 52];
        let rail_top = top + 1.0;
        for s in [-0.3f32, 0.3] {
            let p = foot + across * s;
            let (a, b) = (p - (across + out) * 0.04, p + (across + out) * 0.04);
            self.push(
                vec3(a.x, face.y, a.z),
                vec3(b.x, rail_top, b.z),
                c,
                false,
                true,
            );
        }
        let mut y = face.y + 0.3;
        while y < rail_top - 0.1 {
            let (a, b) = (foot - across * 0.3, foot + across * 0.3);
            let lo = a.min(b) - out * 0.03;
            let hi = a.max(b) + out * 0.03;
            self.push(
                vec3(lo.x, y, lo.z),
                vec3(hi.x, y + 0.05, hi.z),
                c,
                false,
                true,
            );
            y += 0.32;
        }
        self.ladders.push(Ladder {
            base: vec3(foot.x, face.y, foot.z) + out * 0.35,
            out,
            top,
        });
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
        self.trims(true, z, y0, h, t, ops, c);
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
        self.trims(false, x, y0, h, t, ops, c);
    }

    /// Sills under windows and lintels over doors and windows (visual only), a shade
    /// darker than the wall. `line` is the wall's centre line.
    #[allow(clippy::too_many_arguments)]
    fn trims(
        &mut self,
        along_x: bool,
        line: f32,
        y0: f32,
        h: f32,
        t: f32,
        ops: &[Opening],
        c: [u8; 3],
    ) {
        let dark = shade(c, 0.8);
        for o in ops {
            let (a, b) = (o.at - o.w / 2.0 - 0.12, o.at + o.w / 2.0 + 0.12);
            let mut add = |ya: f32, yb: f32, out: f32| {
                let (p0, p1) = (line - t / 2.0 - out, line + t / 2.0 + out);
                if along_x {
                    self.push(
                        vec3(a, y0 + ya, p0),
                        vec3(b, y0 + yb, p1),
                        dark,
                        false,
                        true,
                    );
                } else {
                    self.push(
                        vec3(p0, y0 + ya, a),
                        vec3(p1, y0 + yb, b),
                        dark,
                        false,
                        true,
                    );
                }
            };
            if o.bottom > 0.0 {
                add(o.bottom - 0.1, o.bottom, 0.12);
            }
            if o.top < h - 0.05 {
                add(o.top, (o.top + 0.14).min(h), 0.05);
            }
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

    /// Simple toon car: body + cabin, wheels and lights.
    pub fn car(&mut self, cx: f32, cz: f32, along_x: bool, c: [u8; 3]) {
        let (w, d) = if along_x { (4.2, 1.9) } else { (1.9, 4.2) };
        self.cbox(cx, cz, w, d, 0.0, 0.25, pal::DARK);
        self.cbox(cx, cz, w, d, 0.25, 0.85, c);
        let (cw, cd) = if along_x { (2.2, 1.7) } else { (1.7, 2.2) };
        self.cbox(cx, cz, cw, cd, 1.1, 0.7, pal::GLASS);
        self.cbox(cx, cz, cw + 0.05, cd + 0.05, 1.8, 0.08, c);
        self.car_details(cx, cz, along_x, pal::TYRE, true);
    }

    /// Wheels sticking out of the sides, head- and taillights (visual only).
    pub(crate) fn car_details(
        &mut self,
        cx: f32,
        cz: f32,
        along_x: bool,
        tyre: [u8; 3],
        lights: bool,
    ) {
        // local frame: `l` along the car, `s` across it
        let mut at = |l0: f32, l1: f32, s0: f32, s1: f32, y0: f32, y1: f32, c: [u8; 3]| {
            if along_x {
                self.push(
                    vec3(cx + l0, y0, cz + s0),
                    vec3(cx + l1, y1, cz + s1),
                    c,
                    false,
                    true,
                );
            } else {
                self.push(
                    vec3(cx + s0, y0, cz + l0),
                    vec3(cx + s1, y1, cz + l1),
                    c,
                    false,
                    true,
                );
            }
        };
        for l in [-1.35f32, 1.35] {
            for s in [-1.0f32, 1.0] {
                let (s0, s1) = if s < 0.0 { (-1.02, -0.8) } else { (0.8, 1.02) };
                at(l - 0.36, l + 0.36, s0, s1, 0.0, 0.66, tyre);
                at(
                    l - 0.16,
                    l + 0.16,
                    s0 - 0.01 * s,
                    s1 + 0.01 * s,
                    0.2,
                    0.46,
                    pal::METAL,
                );
            }
        }
        if lights {
            for s in [-0.65f32, 0.45] {
                at(-2.13, -2.08, s, s + 0.2, 0.75, 0.9, pal::LIGHT);
                at(2.08, 2.13, s, s + 0.2, 0.75, 0.9, pal::BARREL_RED);
            }
        }
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
pub(crate) fn wall_segments(a: f32, b: f32, h: f32, ops: &[Opening]) -> Vec<(f32, f32, f32, f32)> {
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
