//! «Вантаж» — a container port at dusk, 96 × 72 m.
//!
//! A ship lies along the east quay and a goods train along the west fence; between them
//! the yard: a gantry crane over the middle (the bomb under it), rows of containers one,
//! two and three high with ladders onto some, a warehouse you can run through, a harbour
//! control building with an upper floor, fuel tanks by the Sabotage sites, trucks,
//! forklifts, cable reels and pallets.
//!
//! The yard is point-symmetric (x, z → −x, −z): the south half is built and copied over
//! to the north in other colours, so both teams have the same ground.

use crate::map::{door, pal, shade, swap_color, wide_door, window, Map, MapBuilder, MapId, Side};
use crate::maps::{site_pad, spawn_rows};
use crate::props::Tower;
use glam::{vec2, vec3};

const HALF_X: f32 = 48.0;
const HALF_Z: f32 = 36.0;

const HULL: [u8; 3] = [44, 52, 70];
const HULL_RED: [u8; 3] = [150, 52, 46];
const DECK: [u8; 3] = [196, 198, 204];
const QUAY: [u8; 3] = [150, 152, 158];
const CRANE: [u8; 3] = [226, 120, 44];

/// The other team's containers come in other colours.
pub(crate) fn recolor(c: [u8; 3]) -> [u8; 3] {
    swap_color(
        c,
        &[
            (pal::CONT_RED, pal::CONT_GREEN),
            (pal::CONT_BLUE, pal::CONT_ORANGE),
            (pal::CONT_YELLOW, pal::WALL_PINK),
            (pal::WALL_BLUE, pal::WALL_GREEN),
            (pal::CAR_A, pal::CAR_B),
        ],
    )
}

impl MapBuilder {
    /// Containers stacked `levels` high.
    fn stack(&mut self, cx: f32, cz: f32, along_x: bool, cols: &[[u8; 3]]) {
        for (k, c) in cols.iter().enumerate() {
            self.container(cx, cz, along_x, k as f32 * 2.6, *c);
        }
    }

    /// A forklift: a body, a mast and two forks.
    pub(crate) fn forklift(&mut self, cx: f32, cz: f32, along_x: bool) {
        let (w, d) = if along_x { (2.2, 1.2) } else { (1.2, 2.2) };
        self.cbox(cx, cz, w, d, 0.0, 1.3, pal::SCAFFOLD);
        self.cbox(cx, cz, w * 0.5, d * 0.5, 1.3, 0.9, pal::DARK);
        let (mx, mz) = if along_x {
            (cx + 1.2, cz)
        } else {
            (cx, cz + 1.2)
        };
        self.cbox(mx, mz, 0.25, 0.25, 0.0, 2.6, pal::DARK);
        for s in [-0.35f32, 0.35] {
            if along_x {
                self.push_detail(
                    mx,
                    0.12,
                    cz + s - 0.07,
                    mx + 1.1,
                    0.2,
                    cz + s + 0.07,
                    pal::METAL,
                );
            } else {
                self.push_detail(
                    cx + s - 0.07,
                    0.12,
                    mz,
                    cx + s + 0.07,
                    0.2,
                    mz + 1.1,
                    pal::METAL,
                );
            }
        }
    }

    /// A lorry along z: the cab at the −z end, a container on its trailer.
    pub(crate) fn lorry(&mut self, cx: f32, cz: f32, cab: [u8; 3], load: Option<[u8; 3]>) {
        self.cbox(cx, cz - 3.6, 2.4, 2.0, 0.0, 2.4, cab);
        self.push_detail(
            cx - 1.0,
            1.3,
            cz - 4.63,
            cx + 1.0,
            2.1,
            cz - 4.58,
            pal::WINDOW,
        );
        self.cbox(cx, cz + 0.6, 2.5, 6.4, 0.0, 1.0, pal::DARK);
        if let Some(c) = load {
            self.container(cx, cz + 0.6, false, 1.0, c);
        }
        for dz in [-3.6f32, -0.8, 2.6] {
            for s in [-1.0f32, 1.0] {
                self.push_detail(
                    cx + s * 1.28 - 0.1,
                    0.0,
                    cz + dz - 0.45,
                    cx + s * 1.28 + 0.1,
                    0.9,
                    cz + dz + 0.45,
                    pal::TYRE,
                );
            }
        }
    }

    /// A cable reel on its side (chest-high cover).
    pub(crate) fn reel(&mut self, cx: f32, cz: f32) {
        self.cbox(cx, cz, 1.5, 0.9, 0.0, 1.5, pal::WOOD);
        self.push_detail(cx - 0.5, 0.5, cz - 0.5, cx + 0.5, 1.0, cz + 0.5, pal::DARK);
    }

    /// A pallet of sacks (waist high).
    pub(crate) fn sacks(&mut self, cx: f32, cz: f32) {
        self.cbox(cx, cz, 1.3, 1.1, 0.0, 0.15, pal::WOOD);
        self.cbox(cx, cz, 1.2, 1.0, 0.15, 0.75, pal::SANDBAG);
        self.push_detail(
            cx - 0.61,
            0.5,
            cz - 0.51,
            cx + 0.61,
            0.54,
            cz + 0.51,
            shade(pal::SANDBAG, 0.8),
        );
    }

    /// A railway wagon: a flat car with two containers, or a box car.
    pub(crate) fn wagon(&mut self, cx: f32, cz: f32, along_x: bool, load: Option<[u8; 3]>) {
        let (w, d) = if along_x { (12.4, 2.7) } else { (2.7, 12.4) };
        self.cbox(cx, cz, w, d, 0.0, 1.1, pal::DARK);
        match load {
            Some(c) => {
                for (s, col) in [(-3.1f32, c), (3.1, recolor(c))] {
                    if along_x {
                        self.container(cx + s, cz, true, 1.1, col);
                    } else {
                        self.container(cx, cz + s, false, 1.1, col);
                    }
                }
            }
            None => {
                self.cbox(cx, cz, w - 0.4, d - 0.1, 1.1, 2.7, pal::RUST);
                let (dw, dd) = if along_x {
                    (2.4, d + 0.08)
                } else {
                    (w + 0.08, 2.4)
                };
                self.push_detail(
                    cx - dw / 2.0,
                    1.3,
                    cz - dd / 2.0,
                    cx + dw / 2.0,
                    3.4,
                    cz + dd / 2.0,
                    shade(pal::RUST, 0.8),
                );
            }
        }
    }
}

/// «Вантаж» — a container port.
pub fn cargo() -> Map {
    let min = vec2(-HALF_X, -HALF_Z);
    let max = vec2(HALF_X, HALF_Z);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::CONCRETE, pal::CONCRETE_DARK, 5.0);

    // painted lanes, the quay's edge, the rails
    b.decal(-HALF_X, -0.6, 43.0, 0.6, 0.0, pal::CONT_YELLOW);
    for x in [-20.0f32, 20.0] {
        b.decal(x - 0.15, -HALF_Z, x + 0.15, HALF_Z, 0.0, pal::WALL_WHITE);
    }
    b.decal(40.0, -HALF_Z, 43.0, HALF_Z, 0.0, QUAY);
    b.decal(42.6, -HALF_Z, 43.0, HALF_Z, 0.03, pal::CONT_YELLOW);
    for x in [-46.0f32, -44.4] {
        b.decal(x - 0.1, -HALF_Z, x + 0.1, HALF_Z, 0.03, pal::METAL);
    }
    b.decal(-47.0, -HALF_Z, -43.4, HALF_Z, 0.0, [112, 104, 96]);

    // the ship along the east quay: hull, deck, bridge and funnel, cargo on deck
    b.block(43.4, 0.0, -33.0, HALF_X, 1.6, 33.0, HULL_RED);
    b.block(43.4, 1.6, -33.0, HALF_X, 6.2, 33.0, HULL);
    b.push_detail(43.3, 4.4, -33.0, 43.4, 4.7, 33.0, pal::WALL_WHITE);
    b.block(43.4, 6.2, -33.0, HALF_X, 6.5, 33.0, DECK);
    b.cbox(46.0, 24.0, 4.0, 9.0, 6.5, 6.0, pal::WALL_WHITE);
    b.push_detail(43.9, 10.2, 20.0, 44.0, 11.4, 28.0, pal::WINDOW);
    b.cbox(46.2, 26.0, 1.8, 2.2, 12.5, 3.2, CRANE);
    for (z, c) in [
        (-24.0f32, pal::CONT_BLUE),
        (-17.5, pal::CONT_RED),
        (-11.0, pal::CONT_YELLOW),
        (-4.5, pal::CONT_GREEN),
        (2.0, pal::CONT_ORANGE),
        (8.5, pal::CONT_BLUE),
    ] {
        b.container(45.8, z, false, 6.5, c);
        if (z as i32) % 3 != 0 {
            b.container(45.8, z, false, 9.1, recolor(c));
        }
    }
    // bollards and mooring lines along the quay
    for k in -4..=4 {
        let z = k as f32 * 7.5;
        b.cbox(42.2, z, 0.5, 0.5, 0.0, 0.7, pal::DARK);
    }

    // the goods train along the west fence (nobody gets behind it)
    b.clip(-HALF_X, 0.0, -HALF_Z, -46.2, 5.0, HALF_Z);
    b.wagon(-45.0, -27.0, false, Some(pal::CONT_GREEN));
    b.wagon(-45.0, -13.5, false, None);
    b.wagon(-45.0, 0.0, false, Some(pal::CONT_ORANGE));
    b.wagon(-45.0, 13.5, false, None);
    b.wagon(-45.0, 27.0, false, Some(pal::CONT_BLUE));

    // the gantry crane's rails and beams over the middle (its legs stand in each half)
    for z in [-5.0f32, 5.0] {
        b.push_detail(-14.0, 11.0, z - 0.7, 14.0, 12.2, z + 0.7, CRANE);
    }
    b.push_detail(-1.6, 11.2, -5.0, 1.6, 12.6, 5.0, shade(CRANE, 0.85));
    b.push_detail(-0.08, 8.2, -0.08, 0.08, 11.2, 0.08, pal::DARK);
    b.push_detail(-3.0, 5.6, -1.22, 3.0, 8.2, 1.22, pal::CONT_RED);

    // ---- the south half (the allies'); the north is its copy
    let mark = b.mark();

    // crane legs
    for x in [-12.0f32, 12.0] {
        b.cbox(x, 5.0, 1.2, 1.2, 0.0, 11.0, CRANE);
        b.cbox(x, 5.0, 2.2, 2.2, 0.0, 0.5, pal::CONCRETE_DARK);
    }

    // the first row of containers, by the middle
    b.stack(-34.0, 7.0, true, &[pal::CONT_RED, pal::CONT_YELLOW]);
    b.stack(-23.0, 9.0, false, &[pal::CONT_BLUE]);
    b.stack(-7.0, 10.0, true, &[pal::CONT_GREEN, pal::CONT_ORANGE]);
    b.ladder(vec3(-10.0, 0.0, 10.0), vec3(-1.0, 0.0, 0.0), 5.2);
    b.stack(8.0, 9.0, true, &[pal::CONT_ORANGE]);
    b.ladder(vec3(11.0, 0.0, 9.0), vec3(1.0, 0.0, 0.0), 2.6);
    b.stack(
        19.0,
        7.0,
        false,
        &[pal::CONT_YELLOW, pal::CONT_BLUE, pal::CONT_RED],
    );
    b.stack(32.0, 6.0, true, &[pal::CONT_RED]);

    // the second row
    b.stack(-28.0, 17.0, false, &[pal::CONT_GREEN]);
    b.stack(-15.0, 16.0, true, &[pal::CONT_BLUE]);
    b.stack(-15.0, 18.6, true, &[pal::CONT_ORANGE]);
    b.container(-15.0, 17.3, true, 2.6, pal::CONT_YELLOW);
    b.stack(11.0, 16.0, false, &[pal::CONT_RED, pal::CONT_GREEN]);
    // (north of the site: no straight shot at it from the middle)
    b.stack(-36.0, 20.5, true, &[pal::CONT_BLUE, pal::CONT_GREEN]);

    // the harbour control building: stairs to an upper floor that looks over the yard
    b.tower(&Tower {
        x0: -5.0,
        z0: 21.0,
        x1: 5.0,
        z1: 30.5,
        floors: 2,
        storey: 3.4,
        color: pal::WALL_WHITE,
        roof: pal::ROOF_GREY,
        ground: &[
            (Side::MaxZ, door(2.0)),
            (Side::MinZ, door(1.5)),
            (Side::MaxX, door(26.0)),
            (Side::MinX, window(27.0)),
        ],
        upper: &[
            (Side::MinZ, window(-2.5)),
            (Side::MinZ, window(2.5)),
            (Side::MinX, window(23.5)),
            (Side::MaxX, window(23.5)),
        ],
        roof_access: false,
    });
    b.push_detail(-5.05, 2.6, 20.9, 5.05, 3.0, 21.0, pal::CONT_BLUE);

    // the warehouse: wide doors on three sides, shelves inside, a ladder to its roof
    b.building(
        23.0,
        13.0,
        40.0,
        27.0,
        0.0,
        5.2,
        &[
            (Side::MinX, wide_door(20.0)),
            (Side::MinZ, wide_door(28.0)),
            (Side::MinZ, wide_door(36.0)),
            (Side::MaxZ, door(33.0)),
            (Side::MaxX, door(20.0)),
            (Side::MaxZ, window(27.0)),
        ],
        pal::WALL_BLUE,
        Some(pal::ROOF_GREY),
    );
    b.decal(23.3, 13.3, 39.7, 26.7, 0.0, QUAY);
    b.cbox(32.0, 21.0, 1.2, 6.0, 0.0, 2.6, pal::METAL);
    b.cbox(27.0, 24.5, 4.0, 1.2, 0.0, 2.6, pal::METAL);
    b.crate_(37.0, 24.0, 1.4, 0.0);
    b.crate_(37.0, 22.5, 1.2, 0.0);
    b.crate_(36.8, 23.4, 1.0, 1.4);
    b.sacks(26.0, 16.5);
    b.ladder(vec3(25.5, 0.0, 27.0), vec3(0.0, 0.0, 1.0), 5.55);

    // around the Sabotage site: fuel tanks, barrels
    b.tank(-41.0, 31.5);
    b.tank(-41.0, 24.0);
    b.barrel(-31.5, 29.5, pal::BARREL_RED);
    b.barrel(-30.7, 29.9, pal::BARREL_BLUE);
    b.sacks(-31.0, 24.0);

    // lorries, forklifts, reels and crates about the yard
    b.lorry(14.0, 26.0, pal::CAR_A, Some(pal::CONT_GREEN));
    b.lorry(-20.0, 27.0, pal::CAR_C, None);
    b.forklift(-20.0, 4.0, true);
    b.forklift(27.0, 9.5, false);
    b.reel(36.0, 31.0);
    b.reel(-40.0, 13.0);
    b.reel(2.5, 14.0);
    b.sacks(-2.0, 15.5);
    b.crate_(3.4, 2.6, 1.2, 0.0);
    b.crate_(4.6, 3.4, 1.0, 0.0);
    b.crate_(-4.0, 4.5, 1.3, 0.0);
    b.crate_(38.5, 3.0, 1.3, 0.0);
    b.crate_(38.5, 4.4, 1.1, 0.0);
    b.barrel(17.0, 13.0, pal::BARREL_BLUE);
    b.barrier(41.0, 10.0, true);
    b.barrier(-41.0, 4.0, true);

    b.mirror_since(mark, recolor);

    let site_allies = vec3(-36.0, 0.0, 27.0);
    let site_axis = vec3(36.0, 0.0, -27.0);
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);
    site_pad(&mut b, site_axis, pal::PAD_AXIS);

    Map {
        id: MapId::Cargo,
        name: "Вантаж",
        min,
        max,
        boxes: b.boxes,
        ladders: b.ladders,
        spawns: spawn_rows(24.0, 32.2, 12),
        bomb: vec3(0.0, 0.0, 0.0),
        sites: [site_allies, site_axis],
        // the port at dusk: teal sea haze, violet sky, cool light
        sky: [150, 140, 190],
        fog: [160, 190, 200],
        tint: [0.9, 0.95, 1.06],
        terrain: None,
        loot: Vec::new(),
        nav_detail: Vec::new(),
        car_spots: Vec::new(),
        doors: Vec::new(),
    }
}
