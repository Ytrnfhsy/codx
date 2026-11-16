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

use crate::map::{door, pal, shade, swap_color, wide_door, window, Map, MapBuilder, MapId, Side};
use crate::maps::{site_pad, spawn_rows};
use crate::props::Tower;
use glam::{vec2, vec3};

const HALF_X: f32 = 65.0;
const HALF_Z: f32 = 55.0;
/// Height of a storey of the frame.
pub const STOREY: f32 = 3.4;
/// The frame's half size (it stands in the middle of the map).
pub const FRAME: (f32, f32) = (12.0, 9.0);
/// How high the materials yards' walls are.
const YARD_WALL: f32 = 4.6;

fn recolor(c: [u8; 3]) -> [u8; 3] {
    swap_color(
        c,
        &[
            (pal::CONT_RED, pal::CONT_GREEN),
            (pal::CONT_BLUE, pal::CONT_ORANGE),
            (pal::BRICK, pal::WALL_TAN),
            (pal::CABIN, pal::CONT_YELLOW),
            (pal::WALL_GREEN, pal::WALL_BLUE),
            (pal::ROOF_RED, pal::ROOF_GREY),
        ],
    )
}

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

    /// Concrete pipes: two on the ground and one on top (chest-high cover, a step up).
    fn pipes(&mut self, cx: f32, cz: f32, along_x: bool) {
        let (w, d) = if along_x { (4.0, 1.4) } else { (1.4, 4.0) };
        let (ox, oz) = if along_x { (0.0, 0.75) } else { (0.75, 0.0) };
        for s in [-1.0f32, 1.0] {
            self.cbox(cx + ox * s, cz + oz * s, w, d, 0.0, 1.4, pal::CONCRETE);
        }
        self.cbox(cx, cz, w, d, 1.4, 1.3, pal::CONCRETE);
        let (ew, ed) = if along_x {
            (w + 0.06, 0.9)
        } else {
            (0.9, d + 0.06)
        };
        self.push_detail(
            cx - ew / 2.0,
            1.6,
            cz - ed / 2.0,
            cx + ew / 2.0,
            2.5,
            cz + ed / 2.0,
            pal::DARK,
        );
    }

    /// A heap of sand: three steps up.
    fn sand_heap(&mut self, cx: f32, cz: f32) {
        self.cbox(cx, cz, 6.0, 4.4, 0.0, 0.45, pal::SAND);
        self.cbox(cx, cz, 4.2, 3.0, 0.45, 0.45, pal::SAND);
        self.cbox(cx, cz, 2.4, 1.6, 0.9, 0.45, pal::SAND);
    }

    /// A stack of timber.
    fn timber(&mut self, cx: f32, cz: f32, along_x: bool) {
        let (w, d) = if along_x { (4.2, 1.4) } else { (1.4, 4.2) };
        self.cbox(cx, cz, w, d, 0.0, 0.8, pal::WOOD);
        self.cbox(cx, cz, w * 0.8, d * 0.85, 0.8, 0.45, shade(pal::WOOD, 0.9));
    }
}

/// «Будмайданчик» — a building site.
pub fn site() -> Map {
    let min = vec2(-HALF_X, -HALF_Z);
    let max = vec2(HALF_X, HALF_Z);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::DIRT, pal::CONCRETE_DARK, 5.0);
    let (fx, fz) = FRAME;
    let h = STOREY;

    // the site road through the frame's ground floor, the trodden yard round it
    b.decal(-34.0, -22.0, 34.0, 22.0, 0.0, [178, 152, 108]);
    b.decal(-HALF_X, -2.6, HALF_X, 2.6, 0.03, pal::ASPHALT);
    b.decal(-fx, -fz, fx, fz, 0.06, pal::CONCRETE_DARK);

    // ---- the south half (the allies'); the north is its copy
    let mark = b.mark();

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

    // --- the tower crane, material by the frame
    b.crane(21.0, 7.0, 22.0);
    b.brick_pallet(-14.5, 5.5);
    b.brick_pallet(-14.5, 7.0);
    b.brick_pallet(10.0, 13.5);
    b.timber(-2.0, 14.5, true);
    b.rubble(15.0, 12.0);
    b.barrier(-20.0, 5.0, false);
    b.barrier(28.0, 5.0, false);

    // --- south-west: the site office, two storeys
    b.tower(&Tower {
        x0: -40.0,
        z0: 12.0,
        x1: -28.0,
        z1: 24.0,
        floors: 2,
        storey: 3.4,
        color: pal::BRICK,
        roof: pal::ROOF_GREY,
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
    // cabins, toilets, pipes and a fence between the office and the frame
    b.site_cabin(-18.0, 21.0, true, pal::CABIN);
    b.site_cabin(-18.0, 24.2, true, pal::CONT_BLUE);
    b.toilet(-10.0, 27.0, pal::CONT_BLUE);
    b.toilet(-8.7, 27.0, pal::CONT_GREEN);
    b.pipes(-22.0, 11.5, false);
    b.site_fence(true, -26.0, -4.0, 31.0, &[wide_door(-15.0)]);
    b.sand_heap(-46.0, 20.0);
    b.container(-57.0, 13.0, false, 0.0, pal::CONT_GREEN);
    b.pipes(-50.0, 27.0, true);
    b.timber(-60.0, 24.0, false);
    b.barrel(-27.0, 27.5, pal::BARREL_RED);

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
    b.container(-56.0, 36.5, true, 0.0, pal::CONT_RED);
    b.wall_z(43.0, 49.0, -42.5, 0.0, 2.6, 0.4, &[], pal::BRICK);
    b.tank(-61.0, 50.0);
    b.tank(-61.0, 44.0);
    b.site_cabin(-43.5, 34.5, true, pal::CABIN);
    b.brick_pallet(-45.0, 51.5);
    b.brick_pallet(-46.5, 51.5);
    b.timber(-55.0, 52.0, true);
    b.barrel(-39.0, 38.5, pal::BARREL_BLUE);

    // --- south-east: the garage block (a ladder to its roof), plant and material
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
        pal::CONCRETE,
        Some(pal::CONCRETE_DARK),
    );
    b.cbox(37.0, 21.0, 0.6, 0.6, 0.0, 3.4, pal::CONCRETE);
    b.ladder(vec3(30.0, 0.0, 28.0), vec3(0.0, 0.0, 1.0), 3.75);
    b.lift(3.75, |b| {
        b.brick_pallet(34.0, 16.0);
        b.brick_pallet(41.0, 24.0);
        b.cbox(44.0, 16.5, 1.6, 1.6, 0.0, 0.9, pal::CONCRETE_DARK);
    });
    b.crate_(43.0, 25.0, 1.3, 0.0);
    b.excavator(12.0, 24.0);
    b.sand_heap(16.0, 34.0);
    b.lorry(52.0, 22.0, pal::SCAFFOLD, None);
    b.cbox(52.0, 22.6, 2.3, 5.6, 1.0, 0.9, pal::SAND);
    b.pipes(24.0, 34.0, true);
    b.timber(36.0, 34.5, false);
    b.cbox(6.0, 27.0, 2.2, 2.2, 0.0, 1.8, pal::CONT_ORANGE); // cement mixer
    b.push_detail(5.2, 1.8, 26.2, 6.8, 2.5, 27.8, pal::METAL);
    b.site_fence(false, 8.0, 30.0, 57.0, &[door(14.0), door(25.0)]);
    b.site_fence(true, 40.0, 56.0, 38.0, &[wide_door(48.0)]);
    b.rubble(56.0, 44.0);
    b.container(48.0, 46.0, true, 0.0, pal::CONT_BLUE);

    // --- the allies' end: barriers, a cabin, trees by the fence
    b.barrier(-12.0, 43.0, true);
    b.barrier(0.0, 40.0, true);
    b.barrier(12.0, 43.0, true);
    b.site_cabin(-4.0, 33.5, true, pal::CABIN);
    b.brick_pallet(3.0, 20.0);
    b.tree(62.0, 51.0, false);
    b.tree(-33.0, 51.5, false);

    b.mirror_since(mark, recolor);

    let site_allies = vec3(-50.0, 0.0, 42.0);
    let site_axis = vec3(50.0, 0.0, -42.0);
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);
    site_pad(&mut b, site_axis, pal::PAD_AXIS);

    Map {
        id: MapId::Site,
        name: "Будмайданчик",
        min,
        max,
        boxes: b.boxes,
        ladders: b.ladders,
        spawns: spawn_rows(26.0, 49.6, 12),
        bomb: vec3(0.0, 0.0, 0.0),
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
    }
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
