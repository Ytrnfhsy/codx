//! «Вантаж» — a container port at dusk, 96 × 72 m.
//!
//! A ship lies along the east quay and a goods train along the west fence; between them
//! the yard: a gantry crane over the middle (the bomb under it), rows of containers one,
//! two and three high with ladders onto some, a warehouse you can run through, a harbour
//! control building with an upper floor, fuel tanks by the Sabotage sites, trucks,
//! forklifts, cable reels and pallets.
//!
//! Each team's half has the same skeleton (the crane's legs, the control building, the
//! warehouse, the stacks with ladders, the site's tanks and wall of containers), turned
//! half round; the cover lying about is the same in kind and amount but not the same
//! things in the same places (see `maps_dress`).

use crate::map::{door, pal, shade, swap_color, wide_door, window, Map, MapBuilder, MapId, Side};
use crate::maps::{site_pad, spawn_rows};
use crate::maps_dress::{dress, keep_clear, litter, props as p, slot, Cover, Dressed, Set, Slot};
use crate::props::Tower;
use crate::rng::Rng;
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
        // the mast as wide as the truck (nothing to squeeze into beside it)
        let (mw, md) = if along_x { (0.25, 1.0) } else { (1.0, 0.25) };
        self.cbox(mx, mz, mw, md, 0.0, 2.6, pal::DARK);
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

/// How one half looks.
struct Style {
    office: [u8; 3],
    stripe: [u8; 3],
    warehouse: [u8; 3],
    roof: [u8; 3],
    /// The stacks with ladders and the wall of containers by the site.
    conts: [[u8; 3]; 5],
}

const SOUTH: Style = Style {
    office: pal::WALL_WHITE,
    stripe: pal::CONT_BLUE,
    warehouse: pal::WALL_BLUE,
    roof: pal::ROOF_GREY,
    conts: [
        pal::CONT_GREEN,
        pal::CONT_ORANGE,
        pal::CONT_ORANGE,
        pal::CONT_BLUE,
        pal::CONT_GREEN,
    ],
};

const NORTH: Style = Style {
    office: [226, 214, 190],
    stripe: pal::CONT_RED,
    warehouse: [182, 96, 78],
    roof: [92, 96, 104],
    conts: [
        pal::CONT_RED,
        pal::CONT_YELLOW,
        [150, 160, 170],
        pal::CONT_GREEN,
        pal::CONT_RED,
    ],
};

/// One team's half, in the allies' (south) frame: what both teams have the same of.
fn skeleton(b: &mut MapBuilder, st: &Style) {
    // the gantry crane's legs
    for x in [-12.0f32, 12.0] {
        b.cbox(x, 5.0, 1.2, 1.2, 0.0, 11.0, CRANE);
        b.cbox(x, 5.0, 2.2, 2.2, 0.0, 0.5, pal::CONCRETE_DARK);
    }
    // the stacks with ladders: one two high by the middle, one low
    b.stack(-7.0, 10.0, true, &[st.conts[0], st.conts[1]]);
    b.ladder(vec3(-10.0, 0.0, 10.0), vec3(-1.0, 0.0, 0.0), 5.2);
    b.stack(8.0, 9.0, true, &[st.conts[2]]);
    b.ladder(vec3(11.0, 0.0, 9.0), vec3(1.0, 0.0, 0.0), 2.6);
    // the wall of containers north of the site: no straight shot at it from the middle
    b.stack(-36.0, 20.5, true, &[st.conts[3], st.conts[4]]);

    // the harbour control building: stairs to an upper floor that looks over the yard
    b.tower(&Tower {
        x0: -5.0,
        z0: 21.0,
        x1: 5.0,
        z1: 30.5,
        floors: 2,
        storey: 3.4,
        color: st.office,
        roof: st.roof,
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
    b.push_detail(-5.05, 2.6, 20.9, 5.05, 3.0, 21.0, st.stripe);

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
        st.warehouse,
        Some(st.roof),
    );
    b.push_detail(22.9, 4.2, 12.9, 40.1, 4.5, 13.0, st.stripe);
    b.decal(23.3, 13.3, 39.7, 26.7, 0.0, QUAY);
    b.cbox(32.0, 21.0, 1.2, 6.0, 0.0, 2.6, pal::METAL);
    b.cbox(27.0, 24.5, 4.0, 1.2, 0.0, 2.6, pal::METAL);
    b.crate_(37.0, 24.0, 1.4, 0.0);
    b.crate_(37.0, 22.7, 1.2, 0.0);
    b.crate_(36.8, 23.4, 1.0, 1.4);
    b.sacks(26.0, 16.5);
    b.ladder(vec3(25.5, 0.0, 27.0), vec3(0.0, 0.0, 1.0), 5.55);

    // the site's fuel tanks
    b.tank(-41.0, 31.5);
    b.tank(-41.0, 24.0);
}

/// How many slots each half fills.
pub const SLOT_COUNT: usize = SLOTS.len();

/// The cover lying about in a half (allies' frame): where, and how high.
const SLOTS: &[Slot] = &[
    // stacks you hide behind (none to climb)
    slot(-34.0, 7.0, Cover::Stack),
    slot(19.0, 7.0, Cover::Stack),
    slot(11.0, 16.5, Cover::Stack),
    slot(-15.0, 17.3, Cover::Stack),
    // single containers, lorries, vans, cabins
    slot(-23.0, 9.0, Cover::Tall),
    slot(32.0, 6.0, Cover::Tall),
    slot(-28.0, 16.0, Cover::Tall),
    slot(14.0, 26.5, Cover::Tall),
    slot(-20.0, 27.0, Cover::Tall),
    // reels, big crates, forklifts, tanks
    slot(-20.0, 3.5, Cover::Chest),
    slot(27.0, 9.0, Cover::Chest),
    slot(35.5, 31.0, Cover::Chest),
    slot(-40.0, 12.5, Cover::Chest),
    slot(2.5, 14.0, Cover::Chest),
    slot(4.0, 3.5, Cover::Chest),
    slot(-4.0, 4.5, Cover::Chest),
    slot(38.0, 3.7, Cover::Chest),
    slot(-10.5, 25.5, Cover::Chest),
    // sacks, barriers, crates, drums, tyres
    slot(-30.5, 24.0, Cover::Low),
    slot(-30.5, 29.7, Cover::Low),
    slot(-2.0, 16.0, Cover::Low),
    slot(41.0, 10.0, Cover::Low),
    slot(-41.0, 4.0, Cover::Low),
    slot(17.0, 13.0, Cover::Low),
    slot(25.0, 31.0, Cover::Low),
    slot(-24.0, 21.5, Cover::Low),
];

const SET: Set = Set {
    low: &[
        p::sacks,
        p::barrier,
        p::low_crates,
        p::pallets,
        p::drums,
        p::tyres,
    ],
    chest: &[p::reel, p::big_crates, p::forklift, p::tank_cube],
    tall: &[p::container, p::lorry, p::van, p::cabin],
    stack: &[p::stack2, p::stack3, p::stack_pyramid, p::stack_crates],
    litter: &[p::stain, p::puddle, p::paint, p::plank, p::flat_pallet],
};

/// «Вантаж» — a container port.
pub fn cargo() -> Map {
    cargo_dressed().0
}

/// The map, and what each half got of its cover (for the balance tests).
pub fn cargo_dressed() -> (Map, [Dressed; 2]) {
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
    // bollards along the quay
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

    // the gantry crane's beams over the middle
    for z in [-5.0f32, 5.0] {
        b.push_detail(-14.0, 11.0, z - 0.7, 14.0, 12.2, z + 0.7, CRANE);
    }
    b.push_detail(-1.6, 11.2, -5.0, 1.6, 12.6, 5.0, shade(CRANE, 0.85));
    b.push_detail(-0.08, 8.2, -0.08, 0.08, 11.2, 0.08, pal::DARK);
    b.push_detail(-3.0, 5.6, -1.22, 3.0, 8.2, 1.22, pal::CONT_RED);

    // both halves' skeletons, the axis one turned round
    skeleton(&mut b, &SOUTH);
    let mark = b.mark();
    skeleton(&mut b, &NORTH);
    b.turn_since(mark);

    let site_allies = vec3(-36.0, 0.0, 27.0);
    let site_axis = vec3(36.0, 0.0, -27.0);
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);
    site_pad(&mut b, site_axis, pal::PAD_AXIS);
    let spawns = spawn_rows(24.0, 32.2, 12);
    let bomb = vec3(0.0, 0.0, 0.0);

    // the cover lying about: the same amount of each height, not the same things
    let all: Vec<_> = spawns.iter().flatten().map(|s| s.pos).collect();
    let keep = keep_clear(&b, &all, bomb, &[site_allies, site_axis]);
    let dressed = dress(&mut b, SLOTS, &SET, 0xCA_560u64, &keep, min, max, 1.6);
    let mut rng = Rng::new(0xCA_5A1);
    for turned in [false, true] {
        litter(&mut b, &SET, 40, turned, &mut rng, min, max);
    }

    (
        Map {
            id: MapId::Cargo,
            name: "Вантаж",
            min,
            max,
            boxes: b.boxes,
            ladders: b.ladders,
            spawns,
            bomb,
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
        },
        dressed,
    )
}
