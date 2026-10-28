//! Bigger building blocks for level design: multi-storey buildings with stairwells and roof
//! access, balconies, a crashed helicopter, scaffolding and small cover props.

use crate::map::{pal, shade, wall_segments, MapBuilder, Opening, Side};

/// Rise of one stair step (physics steps up to 0.5 m).
const STEP_RISE: f32 = 0.34;
const STEP_RUN: f32 = 0.62;

/// Description of a multi-storey building.
pub struct Tower<'a> {
    pub x0: f32,
    pub z0: f32,
    pub x1: f32,
    pub z1: f32,
    pub floors: u32,
    pub storey: f32,
    pub color: [u8; 3],
    pub roof: [u8; 3],
    /// Openings on the ground floor.
    pub ground: &'a [(Side, Opening)],
    /// Openings repeated on every upper floor (windows, balcony doors).
    pub upper: &'a [(Side, Opening)],
    /// Walkable roof with a parapet and a stairwell leading up.
    pub roof_access: bool,
}

impl MapBuilder {
    /// Multi-storey building. Stairs run along Z and alternate between the west and east
    /// walls on every floor, so a player can climb all the way up (and onto the roof).
    pub fn tower(&mut self, t: &Tower) {
        let wall = 0.3;
        let stair_w = 2.2;
        let steps = (t.storey / STEP_RISE).round().max(1.0) as u32;
        let rise = t.storey / steps as f32;
        let len = steps as f32 * STEP_RUN;
        // stairs ascend towards -z; the bottom step leaves a 1 m landing by the south wall
        let z_bottom = t.z1 - wall - 1.0;
        let z_top = z_bottom - len;
        assert!(
            z_top > t.z0 + wall + 1.0,
            "tower too shallow for its stairs"
        );
        let levels = if t.roof_access {
            t.floors
        } else {
            t.floors - 1
        };
        for k in 0..t.floors {
            let y0 = k as f32 * t.storey;
            let ops = if k == 0 { t.ground } else { t.upper };
            self.building(t.x0, t.z0, t.x1, t.z1, y0, t.storey, ops, t.color, None);
        }
        for k in 0..levels {
            let y0 = k as f32 * t.storey;
            let west = k % 2 == 0;
            let sx = if west {
                t.x0 + wall + stair_w / 2.0 + 0.05
            } else {
                t.x1 - wall - stair_w / 2.0 - 0.05
            };
            self.stairs(
                sx,
                z_bottom,
                (0.0, -1.0),
                stair_w,
                steps,
                rise,
                STEP_RUN,
                y0,
                pal::CONCRETE_DARK,
            );
            // the slab above gets a hole over this flight
            let hole = (
                sx - stair_w / 2.0 - 0.05,
                z_top - 0.05,
                sx + stair_w / 2.0 + 0.05,
                z_bottom + 0.1,
            );
            let y = y0 + t.storey;
            let is_roof = k + 1 == t.floors;
            let c = if is_roof { t.roof } else { pal::WOOD };
            self.floor_with_hole(
                t.x0 + wall,
                t.z0 + wall,
                t.x1 - wall,
                t.z1 - wall,
                y,
                hole,
                c,
            );
        }
        // plain slabs where no stair goes through
        for k in levels..t.floors {
            let y = (k + 1) as f32 * t.storey;
            if k + 1 == t.floors {
                self.block(
                    t.x0 - 0.25,
                    y,
                    t.z0 - 0.25,
                    t.x1 + 0.25,
                    y + 0.35,
                    t.z1 + 0.25,
                    t.roof,
                );
            }
        }
        if t.roof_access {
            // parapet around the roof edge (waist high: cover for snipers)
            let y = t.floors as f32 * t.storey;
            let h = 1.0;
            self.block(t.x0, y, t.z0, t.x1, y + h, t.z0 + wall, t.color);
            self.block(t.x0, y, t.z1 - wall, t.x1, y + h, t.z1, t.color);
            self.block(
                t.x0,
                y,
                t.z0 + wall,
                t.x0 + wall,
                y + h,
                t.z1 - wall,
                t.color,
            );
            self.block(
                t.x1 - wall,
                y,
                t.z0 + wall,
                t.x1,
                y + h,
                t.z1 - wall,
                t.color,
            );
        }
    }

    /// Balcony slab with a railing sticking out of a wall at height `y`.
    pub fn balcony(&mut self, side: Side, along0: f32, along1: f32, wall: f32, y: f32, depth: f32) {
        let t = 0.2;
        let rail = 1.0;
        match side {
            Side::MaxZ => {
                self.block(along0, y - t, wall, along1, y, wall + depth, pal::CONCRETE);
                self.block(
                    along0,
                    y,
                    wall + depth - 0.15,
                    along1,
                    y + rail,
                    wall + depth,
                    pal::WOOD,
                );
                self.block(
                    along0,
                    y,
                    wall,
                    along0 + 0.15,
                    y + rail,
                    wall + depth - 0.15,
                    pal::WOOD,
                );
                self.block(
                    along1 - 0.15,
                    y,
                    wall,
                    along1,
                    y + rail,
                    wall + depth - 0.15,
                    pal::WOOD,
                );
            }
            Side::MinZ => {
                self.block(along0, y - t, wall - depth, along1, y, wall, pal::CONCRETE);
                self.block(
                    along0,
                    y,
                    wall - depth,
                    along1,
                    y + rail,
                    wall - depth + 0.15,
                    pal::WOOD,
                );
                self.block(
                    along0,
                    y,
                    wall - depth + 0.15,
                    along0 + 0.15,
                    y + rail,
                    wall,
                    pal::WOOD,
                );
                self.block(
                    along1 - 0.15,
                    y,
                    wall - depth + 0.15,
                    along1,
                    y + rail,
                    wall,
                    pal::WOOD,
                );
            }
            Side::MaxX => {
                self.block(wall, y - t, along0, wall + depth, y, along1, pal::CONCRETE);
                self.block(
                    wall + depth - 0.15,
                    y,
                    along0,
                    wall + depth,
                    y + rail,
                    along1,
                    pal::WOOD,
                );
                self.block(
                    wall,
                    y,
                    along0,
                    wall + depth - 0.15,
                    y + rail,
                    along0 + 0.15,
                    pal::WOOD,
                );
                self.block(
                    wall,
                    y,
                    along1 - 0.15,
                    wall + depth - 0.15,
                    y + rail,
                    along1,
                    pal::WOOD,
                );
            }
            Side::MinX => {
                self.block(wall - depth, y - t, along0, wall, y, along1, pal::CONCRETE);
                self.block(
                    wall - depth,
                    y,
                    along0,
                    wall - depth + 0.15,
                    y + rail,
                    along1,
                    pal::WOOD,
                );
                self.block(
                    wall - depth + 0.15,
                    y,
                    along0,
                    wall,
                    y + rail,
                    along0 + 0.15,
                    pal::WOOD,
                );
                self.block(
                    wall - depth + 0.15,
                    y,
                    along1 - 0.15,
                    wall,
                    y + rail,
                    along1,
                    pal::WOOD,
                );
            }
        }
    }

    /// Crashed transport helicopter lying in the dirt (the centrepiece of a square).
    pub fn heli_wreck(&mut self, cx: f32, cz: f32) {
        let o = pal::HELI_OLIVE;
        let dark = pal::BURNT;
        // scorched ground
        self.decal(cx - 7.0, cz - 4.0, cx + 9.0, cz + 4.0, 0.0, [150, 130, 100]);
        self.decal(cx - 4.0, cz - 2.6, cx + 5.0, cz + 2.6, 0.03, dark);
        // fuselage, slightly dug in, with a cargo door opening on the south side
        self.block(cx - 3.8, 0.0, cz - 1.5, cx + 3.2, 2.6, cz - 1.2, o);
        self.block(cx - 3.8, 0.0, cz + 1.2, cx - 0.4, 2.6, cz + 1.5, o);
        self.block(cx + 1.6, 0.0, cz + 1.2, cx + 3.2, 2.6, cz + 1.5, o);
        self.block(cx - 0.4, 2.0, cz + 1.2, cx + 1.6, 2.6, cz + 1.5, o);
        self.block(cx - 3.8, 2.6, cz - 1.5, cx + 3.2, 2.9, cz + 1.5, o);
        self.block(cx + 3.2, 0.0, cz - 1.5, cx + 3.5, 2.9, cz + 1.5, o);
        // cockpit nose, buried, broken glass
        self.block(cx - 5.6, 0.0, cz - 1.3, cx - 3.8, 1.9, cz + 1.3, o);
        self.block(cx - 6.2, 0.0, cz - 1.0, cx - 5.6, 1.3, cz + 1.0, pal::GLASS);
        // tail boom rising towards the east in steps, with a fin
        self.block(cx + 3.5, 0.6, cz - 0.5, cx + 6.0, 1.6, cz + 0.5, o);
        self.block(cx + 6.0, 1.2, cz - 0.4, cx + 8.4, 2.1, cz + 0.4, o);
        self.block(cx + 8.4, 1.6, cz - 0.15, cx + 9.2, 3.6, cz + 0.15, o);
        // rotor mast and broken blades lying around
        self.block(cx - 0.4, 2.9, cz - 0.4, cx + 0.4, 3.4, cz + 0.4, dark);
        self.push_detail(cx - 1.0, 3.35, cz - 0.15, cx + 6.5, 3.45, cz + 0.15, dark);
        self.push_detail(cx - 0.15, 0.0, cz + 2.5, cx + 0.15, 0.12, cz + 7.5, dark);
        self.push_detail(cx - 6.0, 0.0, cz - 3.2, cx - 1.5, 0.12, cz - 2.9, dark);
        // skids and debris (low cover)
        self.block(
            cx - 3.0,
            0.0,
            cz - 2.2,
            cx + 1.0,
            0.35,
            cz - 1.9,
            pal::METAL,
        );
        self.crate_(cx + 5.0, cz + 2.6, 1.1, 0.0);
        self.crate_(cx + 5.6, cz + 3.5, 0.9, 0.0);
        self.rubble(cx - 6.5, cz + 2.5);
    }

    /// Visual-only detail (not solid): rotor blades, trims.
    pub fn push_detail(
        &mut self,
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
        if self.boxes.len() > n {
            self.boxes.last_mut().unwrap().solid = false;
        }
    }

    pub fn barrel(&mut self, cx: f32, cz: f32, c: [u8; 3]) {
        self.cbox(cx, cz, 0.65, 0.65, 0.0, 1.0, c);
        self.push_detail(
            cx - 0.36,
            0.3,
            cz - 0.36,
            cx + 0.36,
            0.38,
            cz + 0.36,
            pal::DARK,
        );
        self.push_detail(
            cx - 0.36,
            0.7,
            cz - 0.36,
            cx + 0.36,
            0.78,
            cz + 0.36,
            pal::DARK,
        );
    }

    /// A pile of broken concrete (low, steppable chunks).
    pub fn rubble(&mut self, cx: f32, cz: f32) {
        let c = pal::CONCRETE_DARK;
        self.cbox(cx, cz, 2.0, 1.4, 0.0, 0.45, c);
        self.cbox(cx + 0.3, cz - 0.2, 1.1, 0.8, 0.45, 0.4, c);
        self.cbox(cx - 0.9, cz + 0.5, 0.7, 0.6, 0.0, 0.7, pal::CONCRETE);
    }

    /// Concrete road barrier (waist high).
    pub fn barrier(&mut self, cx: f32, cz: f32, along_x: bool) {
        let (w, d) = if along_x { (2.4, 0.6) } else { (0.6, 2.4) };
        self.cbox(cx, cz, w, d, 0.0, 1.05, pal::CONCRETE);
        self.push_detail(
            cx - w / 2.0 - 0.01,
            0.75,
            cz - d / 2.0 - 0.01,
            cx + w / 2.0 + 0.01,
            0.85,
            cz + d / 2.0 + 0.01,
            pal::BARREL_RED,
        );
    }

    /// Blocky toon tree (palm-ish when `palm`).
    pub fn tree(&mut self, cx: f32, cz: f32, palm: bool) {
        if palm {
            self.cbox(cx, cz, 0.35, 0.35, 0.0, 5.0, pal::TRUNK);
            self.push_detail(
                cx - 2.2,
                4.8,
                cz - 0.5,
                cx + 2.2,
                5.2,
                cz + 0.5,
                pal::LEAVES,
            );
            self.push_detail(
                cx - 0.5,
                4.8,
                cz - 2.2,
                cx + 0.5,
                5.2,
                cz + 2.2,
                pal::LEAVES,
            );
            self.push_detail(
                cx - 0.8,
                5.0,
                cz - 0.8,
                cx + 0.8,
                5.6,
                cz + 0.8,
                pal::LEAVES,
            );
        } else {
            self.cbox(cx, cz, 0.45, 0.45, 0.0, 2.6, pal::TRUNK);
            self.push_detail(
                cx - 1.6,
                2.4,
                cz - 1.6,
                cx + 1.6,
                4.4,
                cz + 1.6,
                pal::LEAVES,
            );
            self.push_detail(
                cx - 1.1,
                4.4,
                cz - 1.1,
                cx + 1.1,
                5.2,
                cz + 1.1,
                pal::LEAVES,
            );
        }
    }

    /// Burnt-out car shell (lower, darker than a normal car).
    pub fn burnt_car(&mut self, cx: f32, cz: f32, along_x: bool) {
        let (w, d) = if along_x { (4.2, 1.9) } else { (1.9, 4.2) };
        self.cbox(cx, cz, w, d, 0.0, 1.0, pal::BURNT);
        let (cw, cd) = if along_x { (2.0, 1.6) } else { (1.6, 2.0) };
        self.cbox(cx, cz, cw, cd, 1.0, 0.55, [90, 70, 60]);
        // empty window frames and rusty streaks
        let (gw, gd) = if along_x { (1.6, 1.64) } else { (1.64, 1.6) };
        self.push_detail(
            cx - gw / 2.0,
            1.1,
            cz - gd / 2.0,
            cx + gw / 2.0,
            1.45,
            cz + gd / 2.0,
            [36, 32, 32],
        );
        let (rw, rd) = if along_x { (4.24, 0.5) } else { (0.5, 4.24) };
        self.push_detail(
            cx - rw / 2.0,
            0.55,
            cz - rd / 2.0,
            cx + rw / 2.0,
            0.7,
            cz + rd / 2.0,
            pal::RUST,
        );
        self.car_details(cx, cz, along_x, [34, 30, 30], false);
    }

    /// Low wall with a coping on top and square pillars at its ends and openings.
    #[allow(clippy::too_many_arguments)]
    pub fn low_wall(
        &mut self,
        along_x: bool,
        a: f32,
        b: f32,
        line: f32,
        h: f32,
        t: f32,
        ops: &[Opening],
        c: [u8; 3],
    ) {
        if along_x {
            self.wall_x(a, b, line, 0.0, h, t, ops, c);
        } else {
            self.wall_z(a, b, line, 0.0, h, t, ops, c);
        }
        let cap = shade(c, 0.86);
        let mut piece = |s0: f32, s1: f32, y0: f32, y1: f32, out: f32, solid: bool| {
            let (p0, p1) = (line - t / 2.0 - out, line + t / 2.0 + out);
            let (lo, hi) = if along_x {
                (glam::vec3(s0, y0, p0), glam::vec3(s1, y1, p1))
            } else {
                (glam::vec3(p0, y0, s0), glam::vec3(p1, y1, s1))
            };
            if solid {
                self.block(lo.x, lo.y, lo.z, hi.x, hi.y, hi.z, cap);
            } else {
                self.push_detail(lo.x, lo.y, lo.z, hi.x, hi.y, hi.z, cap);
            }
        };
        for (s0, s1, ya, _) in wall_segments(a, b, h, ops) {
            if ya > 0.0 {
                continue;
            }
            piece(s0, s1, h, h + 0.1, 0.06, false);
            for e in [s0, s1] {
                piece(e - 0.3, e + 0.3, 0.0, h + 0.3, 0.15, true);
            }
        }
    }

    /// Construction-site fence: blue panels on posts, solid (cover), with gaps in `ops`.
    pub fn site_fence(&mut self, along_x: bool, a: f32, b: f32, line: f32, ops: &[Opening]) {
        let h = 2.0;
        if along_x {
            self.wall_x(a, b, line, 0.0, h, 0.12, ops, pal::FENCE);
        } else {
            self.wall_z(a, b, line, 0.0, h, 0.12, ops, pal::FENCE);
        }
        // white stripe and posts every 2.5 m
        for (s0, s1, ya, _) in wall_segments(a, b, h, ops) {
            if ya > 0.0 {
                continue;
            }
            let mut k = s0;
            while k <= s1 + 0.01 {
                let (x0, z0, x1, z1) = if along_x {
                    (k - 0.06, line - 0.12, k + 0.06, line + 0.12)
                } else {
                    (line - 0.12, k - 0.06, line + 0.12, k + 0.06)
                };
                self.push_detail(x0, 0.0, z0, x1, h + 0.1, z1, pal::METAL);
                k += 2.5;
            }
            let (x0, z0, x1, z1) = if along_x {
                (s0, line - 0.08, s1, line + 0.08)
            } else {
                (line - 0.08, s0, line + 0.08, s1)
            };
            self.push_detail(x0, 1.45, z0, x1, 1.65, z1, pal::WALL_WHITE);
        }
    }

    /// Pallet stacked with bricks (waist-high cover).
    pub fn brick_pallet(&mut self, cx: f32, cz: f32) {
        self.cbox(cx, cz, 1.2, 1.0, 0.0, 0.15, pal::WOOD);
        self.cbox(cx, cz, 1.1, 0.9, 0.15, 0.7, pal::BRICK);
        for y in [0.38f32, 0.62] {
            self.push_detail(
                cx - 0.56,
                y,
                cz - 0.46,
                cx + 0.56,
                y + 0.03,
                cz + 0.46,
                shade(pal::BRICK, 0.75),
            );
        }
    }

    /// Site office cabin (6 × 2.4) with windows and a door on its +z / +x long side.
    pub fn site_cabin(&mut self, cx: f32, cz: f32, along_x: bool, c: [u8; 3]) {
        let (w, d) = if along_x { (6.0, 2.4) } else { (2.4, 6.0) };
        self.cbox(cx, cz, w, d, 0.0, 0.25, pal::DARK);
        self.cbox(cx, cz, w, d, 0.25, 2.4, c);
        self.cbox(cx, cz, w + 0.2, d + 0.2, 2.65, 0.15, pal::ROOF_GREY);
        // windows on both long sides, a door on the front
        for (l0, l1, y0, y1, col) in [
            (-2.4f32, -1.2f32, 1.2f32, 2.1f32, pal::WINDOW),
            (0.2, 1.4, 1.2, 2.1, pal::WINDOW),
            (1.8, 2.6, 0.25, 2.2, pal::WOOD),
        ] {
            if along_x {
                self.push_detail(
                    cx + l0,
                    y0,
                    cz - d / 2.0 - 0.04,
                    cx + l1,
                    y1,
                    cz + d / 2.0 + 0.04,
                    col,
                );
            } else {
                self.push_detail(
                    cx - w / 2.0 - 0.04,
                    y0,
                    cz + l0,
                    cx + w / 2.0 + 0.04,
                    y1,
                    cz + l1,
                    col,
                );
            }
        }
    }

    /// Portable toilet.
    pub fn toilet(&mut self, cx: f32, cz: f32, c: [u8; 3]) {
        self.cbox(cx, cz, 1.1, 1.1, 0.0, 2.2, c);
        self.cbox(cx, cz, 1.2, 1.2, 2.2, 0.12, pal::WALL_WHITE);
        self.push_detail(
            cx - 0.35,
            0.1,
            cz + 0.55,
            cx + 0.35,
            2.0,
            cz + 0.58,
            shade(c, 0.8),
        );
    }

    /// Tower crane: a solid lattice mast and a long jib high above (visual), with a hook.
    pub fn crane(&mut self, cx: f32, cz: f32, height: f32) {
        let y = pal::SCAFFOLD;
        self.cbox(cx, cz, 3.0, 3.0, 0.0, 0.6, pal::CONCRETE_DARK);
        self.cbox(cx, cz, 1.3, 1.3, 0.6, height - 0.6, y);
        // lattice bands on the mast
        let mut h = 1.6;
        while h < height - 1.0 {
            self.push_detail(
                cx - 0.7,
                h,
                cz - 0.7,
                cx + 0.7,
                h + 0.12,
                cz + 0.7,
                shade(y, 0.75),
            );
            h += 1.6;
        }
        // cab, jib towards -x, counter-jib with weights towards +x
        self.push_detail(
            cx - 0.8,
            height,
            cz + 0.65,
            cx + 0.8,
            height + 1.6,
            cz + 2.2,
            pal::CABIN,
        );
        self.push_detail(
            cx - 0.6,
            height + 0.9,
            cz + 2.2,
            cx + 0.6,
            height + 1.4,
            cz + 2.24,
            pal::WINDOW,
        );
        self.push_detail(
            cx - 22.0,
            height,
            cz - 0.5,
            cx + 0.65,
            height + 1.0,
            cz + 0.5,
            y,
        );
        self.push_detail(
            cx + 0.65,
            height,
            cz - 0.5,
            cx + 7.0,
            height + 0.7,
            cz + 0.5,
            y,
        );
        self.push_detail(
            cx + 5.0,
            height - 1.2,
            cz - 0.8,
            cx + 7.0,
            height,
            cz + 0.8,
            pal::CONCRETE_DARK,
        );
        self.push_detail(
            cx - 0.4,
            height + 1.0,
            cz - 0.4,
            cx + 0.4,
            height + 4.0,
            cz + 0.4,
            y,
        );
        // trolley, cable and a hook with a load of planks
        let hx = cx - 14.0;
        self.push_detail(
            hx - 0.5,
            height - 0.4,
            cz - 0.6,
            hx + 0.5,
            height,
            cz + 0.6,
            pal::DARK,
        );
        self.push_detail(
            hx - 0.04,
            6.2,
            cz - 0.04,
            hx + 0.04,
            height - 0.4,
            cz + 0.04,
            pal::DARK,
        );
        self.push_detail(hx - 1.4, 5.6, cz - 0.5, hx + 1.4, 6.2, cz + 0.5, pal::WOOD);
    }

    /// Scaffolding: a plank walkway at height `y` along a wall, with stairs up at one end
    /// and visual poles.
    pub fn scaffold_x(&mut self, x0: f32, x1: f32, z: f32, depth: f32, y: f32) {
        let steps = (y / STEP_RISE).round().max(1.0) as u32;
        let rise = y / steps as f32;
        let len = steps as f32 * STEP_RUN;
        // walkway
        self.block(x0, y - 0.15, z, x1, y, z + depth, pal::WOOD);
        // stairs climbing along +x towards the walkway start
        self.stairs(
            x0 - len,
            z + depth / 2.0,
            (1.0, 0.0),
            depth,
            steps,
            rise,
            STEP_RUN,
            0.0,
            pal::WOOD,
        );
        // poles (visual)
        let mut x = x0;
        while x <= x1 + 0.01 {
            self.push_detail(
                x - 0.06,
                0.0,
                z + depth - 0.12,
                x + 0.06,
                y + 1.1,
                z + depth,
                pal::SCAFFOLD,
            );
            x += 2.5;
        }
        // guard rail
        self.push_detail(
            x0,
            y + 0.95,
            z + depth - 0.1,
            x1,
            y + 1.05,
            z + depth,
            pal::SCAFFOLD,
        );
    }
}
