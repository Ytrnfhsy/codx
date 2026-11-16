//! «Будмайданчик» — a building site on an overcast morning, 130 × 110 m.
//!
//! In the middle stands the unfinished concrete frame: a drive-through ground floor (the
//! bomb lies there), a first floor with half walls and two quarters of a roof, reached
//! by stairs inside and by the scaffolding outside. It commands the middle of the map —
//! and nothing else: each Sabotage site lies 65 m away in its own walled materials yard
//! in a far corner, which cannot be seen (or shot into) from the frame; a test checks
//! that.
//!
//! Round the frame: a tower crane, a two-storey site office, a one-storey garage block
//! with a ladder to its roof, cabins and toilets, an excavator, a tipper lorry, stacks
//! of pipes, pallets of bricks, heaps of sand, fences with gaps.
//!
//! The map is point-symmetric (x, z → −x, −z): the south half is built and copied over
//! to the north in other colours, so both teams have the same ground.

use crate::map::{door, pal, shade, wide_door, window, Map, MapBuilder, MapId, Side};
use crate::maps::{site_pad, spawn_rows};
use crate::maps_dress::{dress, keep_clear, litter, props as p, slot, Cover, Dressed, Set, Slot};
use crate::props::Tower;
use crate::rng::Rng;
use glam::{vec2, vec3};

const HALF_X: f32 = 65.0;
const HALF_Z: f32 = 55.0;
/// Height of a storey of the frame.
pub const STOREY: f32 = 3.4;
/// The frame's half size (it stands in the middle of the map).
pub const FRAME: (f32, f32) = (12.0, 9.0);
/// How high the materials yards' walls are.
const YARD_WALL: f32 = 4.6;

impl MapBuilder {
    /// An excavator: tracks, a body with a cab, the arm reaching out along +x.
    fn excavator(&mut self, cx: f32, cz: f32) {
        for s in [-1.2f32, 1.2] {
            self.cbox(cx, cz + s, 4.0, 0.7, 0.0, 0.9, pal::DARK);
        }
        self.cbox(cx, cz, 3.4, 2.6, 0.9, 1.5, pal::SCAFFOLD);
        self.cbox(cx - 0.5, cz + 0.5, 1.5, 1.3, 2.4, 1.3, pal::SCAFFOLD);
        self.push_detail(
            cx - 1.27,
            2.6,
            cz + 0.4,
            cx + 0.27,
            3.5,
            cz + 1.17,
            pal::WINDOW,
        );
        // the arm (visual): up and out, the bucket on the ground
        self.push_detail(
            cx + 1.2,
            2.2,
            cz - 0.9,
            cx + 4.2,
            2.7,
            cz - 0.5,
            pal::SCAFFOLD,
        );
        self.push_detail(
            cx + 4.0,
            0.9,
            cz - 0.9,
            cx + 4.4,
            2.7,
            cz - 0.5,
            pal::SCAFFOLD,
        );
        self.cbox(cx + 4.3, cz - 0.7, 1.1, 1.3, 0.0, 0.9, pal::DARK);
    }

    /// A stack of timber.
    fn timber(&mut self, cx: f32, cz: f32, along_x: bool) {
        let (w, d) = if along_x { (4.2, 1.4) } else { (1.4, 4.2) };
        self.cbox(cx, cz, w, d, 0.0, 0.8, pal::WOOD);
        self.cbox(cx, cz, w * 0.8, d * 0.85, 0.8, 0.45, shade(pal::WOOD, 0.9));
    }
}

/// How one half looks (and the parts of the frame that are its own).
struct Style {
    office: [u8; 3],
    roof: [u8; 3],
    garage: [u8; 3],
    garage_roof: [u8; 3],
    gate_cont: [u8; 3],
    cabin: [u8; 3],
    crane_h: f32,
}

const SOUTH: Style = Style {
    office: pal::BRICK,
    roof: pal::ROOF_GREY,
    garage: pal::CONCRETE,
    garage_roof: pal::CONCRETE_DARK,
    gate_cont: pal::CONT_RED,
    cabin: pal::CABIN,
    crane_h: 22.0,
};

const NORTH: Style = Style {
    office: pal::WALL_TAN,
    roof: [98, 118, 146],
    garage: [200, 186, 160],
    garage_roof: [140, 120, 100],
    gate_cont: pal::CONT_GREEN,
    cabin: pal::CONT_YELLOW,
    crane_h: 19.0,
};

/// One team's half, in the allies' (south) frame: what both teams have the same of.
fn skeleton(b: &mut MapBuilder, st: &Style) {
    let (fx, fz) = FRAME;
    let h = STOREY;

    // --- the frame: columns two storeys high
    for x in [-fx + 0.3, -4.0, 4.0, fx - 0.3] {
        for z in [3.0, fz - 0.3] {
            b.cbox(x, z, 0.6, 0.6, 0.0, 2.0 * h, pal::CONCRETE);
            for (dx, dz) in [(-0.15f32, -0.15f32), (0.15, 0.15)] {
                b.push_detail(
                    x + dx - 0.03,
                    2.0 * h,
                    z + dz - 0.03,
                    x + dx + 0.03,
                    2.0 * h + 0.7,
                    z + dz + 0.03,
                    pal::RUST,
                );
            }
        }
    }
    // brick infill on the ground floor: the road runs through between the halves
    b.wall_x(
        -fx,
        fx,
        fz - 0.15,
        0.0,
        h,
        0.3,
        &[wide_door(-8.0), wide_door(7.0), window(0.0)],
        pal::BRICK,
    );
    b.wall_z(3.3, fz - 0.3, -fx + 0.15, 0.0, h, 0.3, &[], pal::BRICK);
    b.wall_z(
        3.3,
        fz - 0.3,
        fx - 0.15,
        0.0,
        h,
        0.3,
        &[window(6.0)],
        pal::BRICK,
    );
    b.wall_z(3.3, 6.0, 0.0, 0.0, h, 0.3, &[], pal::BRICK);
    // the first floor, the stairs up to it by the east wall
    b.floor_with_hole(-fx, 0.0, fx, fz, h, (7.0, 1.6, 9.4, 8.1), pal::CONCRETE);
    b.stairs(
        8.2,
        8.0,
        (0.0, -1.0),
        2.2,
        10,
        h / 10.0,
        0.62,
        0.0,
        pal::CONCRETE_DARK,
    );
    // half walls round it (cover with a view), a gap where the scaffolding comes up
    b.wall_x(-fx, -4.5, fz - 0.15, h, 1.1, 0.3, &[], pal::BRICK);
    b.wall_x(6.5, fx, fz - 0.15, h, 1.1, 0.3, &[], pal::BRICK);
    b.wall_z(0.0, fz - 0.3, -fx + 0.15, h, 1.1, 0.3, &[], pal::BRICK);
    b.wall_z(0.0, fz - 0.3, fx - 0.15, h, 1.1, 0.3, &[], pal::BRICK);
    b.brick_pallet(2.0, 5.0);
    b.lift(h, |b| {
        b.brick_pallet(-2.0, 4.0);
        b.brick_pallet(4.5, 2.2);
    });
    // a quarter of the roof over the west end, its own stairs, a parapet
    b.floor_with_hole(
        -fx,
        0.0,
        0.0,
        fz,
        2.0 * h,
        (-9.4, 1.6, -7.0, 8.1),
        pal::CONCRETE,
    );
    b.stairs(
        -8.2,
        8.0,
        (0.0, -1.0),
        2.2,
        10,
        h / 10.0,
        0.62,
        h,
        pal::CONCRETE_DARK,
    );
    b.block(
        -fx,
        2.0 * h,
        fz - 0.3,
        0.0,
        2.0 * h + 0.9,
        fz,
        pal::CONCRETE,
    );
    b.block(
        -fx,
        2.0 * h,
        0.0,
        -fx + 0.3,
        2.0 * h + 0.9,
        fz - 0.3,
        pal::CONCRETE,
    );
    // scaffolding up the south face
    b.scaffold_x(-4.5, 6.5, fz, 1.4, h);

    // --- the tower crane
    b.crane(21.0, 7.0, st.crane_h);

    // --- south-west: the site office, two storeys
    b.tower(&Tower {
        x0: -40.0,
        z0: 12.0,
        x1: -28.0,
        z1: 24.0,
        floors: 2,
        storey: 3.4,
        color: st.office,
        roof: st.roof,
        ground: &[
            (Side::MaxX, door(14.0)),
            (Side::MinZ, door(-33.0)),
            (Side::MaxZ, door(-32.0)),
            (Side::MaxX, window(20.0)),
        ],
        upper: &[
            (Side::MaxX, window(14.5)),
            (Side::MaxX, window(20.0)),
            (Side::MinZ, window(-33.0)),
            (Side::MaxZ, window(-33.0)),
            (Side::MinX, window(15.0)),
        ],
        roof_access: false,
    });
    // a fence between the office and the frame
    b.site_fence(true, -26.0, -4.0, 31.0, &[wide_door(-15.0)]);

    // --- the allies' materials yard in the corner: the Sabotage site behind high walls
    b.wall_x(
        -HALF_X,
        -37.0,
        31.0,
        0.0,
        YARD_WALL,
        0.4,
        &[wide_door(-58.0)],
        pal::CONCRETE_DARK,
    );
    b.wall_z(
        31.2,
        HALF_Z,
        -37.0,
        0.0,
        YARD_WALL,
        0.4,
        &[wide_door(47.0)],
        pal::CONCRETE_DARK,
    );
    // (nobody shoots straight in through a gate: something stands behind each)
    b.container(-56.0, 36.5, true, 0.0, st.gate_cont);
    b.wall_z(43.0, 49.0, -42.5, 0.0, 2.6, 0.4, &[], pal::BRICK);
    b.tank(-61.0, 50.0);
    b.tank(-61.0, 44.0);
    b.site_cabin(-43.5, 34.5, true, st.cabin);
    b.brick_pallet(-45.0, 51.5);
    b.brick_pallet(-46.5, 51.5);
    b.timber(-55.0, 52.0, true);
    b.barrel(-39.0, 38.5, pal::BARREL_BLUE);

    // --- south-east: the garage block (a ladder to its roof), fences
    b.building(
        28.0,
        14.0,
        46.0,
        28.0,
        0.0,
        3.4,
        &[
            (Side::MinX, wide_door(21.0)),
            (Side::MinZ, wide_door(33.0)),
            (Side::MinZ, wide_door(41.0)),
            (Side::MaxZ, door(38.0)),
            (Side::MaxX, window(21.0)),
            (Side::MaxZ, window(32.0)),
        ],
        st.garage,
        Some(st.garage_roof),
    );
    b.cbox(37.0, 21.0, 0.6, 0.6, 0.0, 3.4, pal::CONCRETE);
    b.ladder(vec3(30.0, 0.0, 28.0), vec3(0.0, 0.0, 1.0), 3.75);
    b.lift(3.75, |b| {
        b.brick_pallet(34.0, 16.0);
        b.brick_pallet(41.0, 24.0);
        b.cbox(44.0, 16.5, 1.6, 1.6, 0.0, 0.9, pal::CONCRETE_DARK);
    });
    b.site_fence(false, 8.0, 30.0, 57.0, &[door(14.0), door(25.0)]);
    b.site_fence(true, 40.0, 56.0, 38.0, &[wide_door(48.0)]);
    // trees by the fence
    b.tree(62.0, 51.0, false);
    b.tree(-33.0, 51.5, false);
}

/// How many slots each half fills.
pub const SLOT_COUNT: usize = SLOTS.len();

/// The cover lying about in a half (allies' frame): where, and how high.
const SLOTS: &[Slot] = &[
    slot(-18.0, 21.0, Cover::Tall),
    slot(-18.0, 25.0, Cover::Tall),
    slot(-9.0, 27.5, Cover::Tall),
    slot(-57.0, 13.0, Cover::Tall),
    slot(12.0, 24.0, Cover::Tall),
    slot(52.0, 22.0, Cover::Tall),
    slot(48.0, 46.0, Cover::Tall),
    slot(-4.0, 33.5, Cover::Tall),
    slot(-28.0, 40.0, Cover::Stack),
    slot(-22.0, 11.5, Cover::Chest),
    slot(-46.0, 20.0, Cover::Chest),
    slot(-50.0, 27.5, Cover::Chest),
    slot(43.0, 33.0, Cover::Chest),
    slot(16.0, 34.0, Cover::Chest),
    slot(24.0, 34.0, Cover::Chest),
    slot(6.0, 27.0, Cover::Chest),
    slot(-8.0, 16.0, Cover::Chest),
    slot(-14.5, 6.0, Cover::Low),
    slot(10.0, 13.5, Cover::Low),
    slot(-2.0, 14.5, Cover::Low),
    slot(-20.0, 5.0, Cover::Low),
    slot(28.0, 5.0, Cover::Low),
    slot(15.0, 12.0, Cover::Low),
    slot(-27.0, 27.5, Cover::Low),
    slot(-60.0, 24.0, Cover::Low),
    slot(36.0, 34.5, Cover::Low),
    slot(56.0, 44.0, Cover::Low),
    slot(-12.0, 43.0, Cover::Low),
    slot(0.0, 40.0, Cover::Low),
    slot(12.0, 43.0, Cover::Low),
    slot(3.0, 20.0, Cover::Low),
];

fn excavator(b: &mut MapBuilder, x: f32, z: f32, _ax: bool, _r: &mut Rng) {
    b.excavator(x - 1.0, z);
}

fn tipper(b: &mut MapBuilder, x: f32, z: f32, _ax: bool, _r: &mut Rng) {
    b.lorry(x, z, pal::SCAFFOLD, None);
    b.cbox(x, z + 0.6, 2.3, 5.6, 1.0, 0.9, pal::SAND);
}

const SET: Set = Set {
    low: &[
        p::bricks,
        p::barrier,
        p::timber,
        p::sandbags,
        p::drums,
        p::tyres,
    ],
    chest: &[p::pipes, p::sand_heap, p::mixer, p::big_crates, p::reel],
    tall: &[
        p::cabin,
        p::container,
        p::toilets,
        p::van,
        tipper,
        excavator,
    ],
    stack: &[p::cabin_stack, p::stack2],
    litter: &[p::stain, p::puddle, p::plank, p::flat_pallet],
};

/// «Будмайданчик» — a building site.
pub fn site() -> Map {
    site_dressed().0
}

/// The map, and what each half got of its cover (for the balance tests).
pub fn site_dressed() -> (Map, [Dressed; 2]) {
    let min = vec2(-HALF_X, -HALF_Z);
    let max = vec2(HALF_X, HALF_Z);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::DIRT, pal::CONCRETE_DARK, 5.0);
    let (fx, fz) = FRAME;

    // the site road through the frame's ground floor, the trodden yard round it
    b.decal(-34.0, -22.0, 34.0, 22.0, 0.0, [178, 152, 108]);
    b.decal(-HALF_X, -2.6, HALF_X, 2.6, 0.03, pal::ASPHALT);
    b.decal(-fx, -fz, fx, fz, 0.06, pal::CONCRETE_DARK);

    // both halves' skeletons, the axis one turned round
    skeleton(&mut b, &SOUTH);
    let mark = b.mark();
    skeleton(&mut b, &NORTH);
    b.turn_since(mark);

    let site_allies = vec3(-50.0, 0.0, 42.0);
    let site_axis = vec3(50.0, 0.0, -42.0);
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);
    site_pad(&mut b, site_axis, pal::PAD_AXIS);
    let spawns = spawn_rows(26.0, 49.6, 12);
    let bomb = vec3(0.0, 0.0, 0.0);

    // the cover lying about: the same amount of each height, not the same things
    let all: Vec<_> = spawns.iter().flatten().map(|s| s.pos).collect();
    let keep = keep_clear(&b, &all, bomb, &[site_allies, site_axis]);
    let dressed = dress(&mut b, SLOTS, &SET, 0x5_17E0_u64, &keep, min, max, 1.8);
    let mut rng = Rng::new(0x5_17E1);
    for turned in [false, true] {
        litter(&mut b, &SET, 55, turned, &mut rng, min, max);
    }

    (
        Map {
            id: MapId::Site,
            name: "Будмайданчик",
            min,
            max,
            boxes: b.boxes,
            ladders: b.ladders,
            spawns,
            bomb,
            sites: [site_allies, site_axis],
            // an overcast morning on the building site: grey sky, flat cool light
            sky: [176, 188, 200],
            fog: [198, 204, 210],
            tint: [0.93, 0.96, 1.03],
            terrain: None,
            loot: Vec::new(),
            nav_detail: Vec::new(),
            car_spots: Vec::new(),
            doors: Vec::new(),
        },
        dressed,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::physics::World;

    /// Nobody standing anywhere on the frame (either floor, the roof, the scaffolding)
    /// sees a Sabotage site: no shooting the planter or the defuser from the middle.
    #[test]
    fn sites_are_hidden_from_the_frame() {
        let m = site();
        let w = World::from_map(&m);
        let (fx, fz) = FRAME;
        let mut eyes = Vec::new();
        let mut x = -fx - 1.5;
        while x <= fx + 1.5 {
            let mut z = -fz - 1.5;
            while z <= fz + 1.5 {
                for floor in [STOREY, 2.0 * STOREY] {
                    // (standing and jumping)
                    eyes.push(vec3(x, floor + 1.6, z));
                    eyes.push(vec3(x, floor + 2.6, z));
                }
                z += 0.75;
            }
            x += 0.75;
        }
        for site in m.sites {
            for dx in [-2.6f32, 0.0, 2.6] {
                for dz in [-2.6f32, 0.0, 2.6] {
                    for y in [0.4f32, 1.0, 1.7] {
                        let at = site + vec3(dx, y, dz);
                        for e in &eyes {
                            assert!(
                                !w.line_of_sight(*e, at),
                                "the site at {site:?} is seen from {e:?} (point {at:?})"
                            );
                        }
                    }
                }
            }
        }
        // and the sites are far apart
        assert!(m.sites[0].distance(m.sites[1]) > 120.0);
    }
}
