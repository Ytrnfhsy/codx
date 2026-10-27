//! Bigger multi-level maps: «Аварія» (a desert town around a crashed helicopter) and
//! «Будмайданчик» (a construction site). Both use three lanes, high ground with stairs
//! (rooftops, balconies, an unfinished building) and plenty of cover.

use crate::map::{door, pal, wide_door, window, Map, MapBuilder, MapId, Opening, Side, Spawn};
use crate::props::Tower;
use glam::{vec2, vec3, Vec3};
use std::f32::consts::PI;

fn spawn(x: f32, z: f32, yaw: f32) -> Spawn {
    Spawn {
        pos: vec3(x, 0.0, z),
        yaw,
    }
}

fn site_pad(b: &mut MapBuilder, p: Vec3, c: [u8; 3]) {
    b.decal(p.x - 2.6, p.z - 2.6, p.x + 2.6, p.z + 2.6, 0.0, c);
    b.decal(
        p.x - 1.8,
        p.z - 1.8,
        p.x + 1.8,
        p.z + 1.8,
        0.03,
        pal::WALL_WHITE,
    );
    b.decal(p.x - 1.5, p.z - 1.5, p.x + 1.5, p.z + 1.5, 0.06, c);
}

/// An opening for a doorway on an upper floor (e.g. out onto a balcony).
fn upper_door(at: f32) -> Opening {
    Opening {
        at,
        w: 1.6,
        bottom: 0.0,
        top: 2.4,
    }
}

/// «Аварія» — desert town. A crashed helicopter in the square, a three-storey block with a
/// sniper roof on the west, a market house with a balcony over the square on the east.
pub fn wreck() -> Map {
    let min = vec2(-42.0, -38.0);
    let max = vec2(42.0, 38.0);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::SAND, pal::ADOBE, 6.0);

    // roads and the paved square
    b.decal(-4.0, -38.0, 4.0, 38.0, 0.0, [196, 176, 138]);
    b.decal(-17.0, -13.0, 17.0, 13.0, 0.0, pal::CONCRETE);
    b.decal(-42.0, -2.5, -17.0, 2.5, 0.0, [196, 176, 138]);
    b.decal(17.0, -2.5, 42.0, 2.5, 0.0, [196, 176, 138]);

    // centrepiece
    b.heli_wreck(-1.0, -1.0);

    // low adobe walls framing the square, with gaps to the lanes
    b.wall_z(
        -13.0,
        13.0,
        -17.0,
        0.0,
        1.2,
        0.5,
        &[door(-8.0), wide_door(0.0), door(8.0)],
        pal::ADOBE_LIGHT,
    );
    b.wall_z(
        -13.0,
        13.0,
        17.0,
        0.0,
        1.2,
        0.5,
        &[door(-8.0), wide_door(0.0), door(8.0)],
        pal::ADOBE_LIGHT,
    );

    // cover in the square
    b.burnt_car(-11.0, -8.0, false);
    b.burnt_car(10.5, 8.5, true);
    b.barrier(6.0, -8.0, true);
    b.barrier(-6.5, 9.5, true);
    b.barrel(-13.5, 3.0, pal::BARREL_BLUE);
    b.barrel(-13.0, 4.0, pal::BARREL_RED);
    b.barrel(13.5, -3.5, pal::BARREL_RED);
    for (x, z) in [(-15.0, -11.0), (15.0, 11.0), (15.0, -11.0), (-15.0, 11.0)] {
        b.tree(x, z, true);
    }
    // market stalls
    for (x, z, c) in [(-11.5, 9.5, pal::AWNING_A), (11.5, -9.0, pal::AWNING_B)] {
        b.cbox(x, z, 2.6, 1.0, 0.0, 1.0, pal::WOOD);
        b.push_detail(x - 1.6, 2.3, z - 1.0, x + 1.6, 2.4, z + 1.0, c);
        b.push_detail(x - 1.5, 0.0, z - 0.9, x - 1.4, 2.3, z - 0.8, pal::WOOD);
        b.push_detail(x + 1.4, 0.0, z - 0.9, x + 1.5, 2.3, z - 0.8, pal::WOOD);
    }

    // WEST: three-storey block with a sniper roof
    b.tower(&Tower {
        x0: -36.0,
        z0: -16.0,
        x1: -24.0,
        z1: -4.0,
        floors: 3,
        storey: 3.4,
        color: pal::ADOBE_LIGHT,
        roof: pal::ADOBE,
        ground: &[
            (Side::MaxX, door(-8.0)),
            (Side::MaxX, window(-13.0)),
            (Side::MinZ, door(-30.0)),
            (Side::MaxZ, door(-28.0)),
            (Side::MinX, window(-10.0)),
        ],
        upper: &[
            (Side::MaxX, window(-7.0)),
            (Side::MaxX, window(-12.0)),
            (Side::MinZ, window(-30.0)),
            (Side::MaxZ, window(-29.0)),
            (Side::MinX, window(-10.0)),
        ],
        roof_access: true,
    });
    b.crate_(-25.5, -14.5, 1.1, 0.0);
    b.cbox(-29.5, -14.8, 3.0, 0.8, 3.4, 1.0, pal::WOOD);

    // west lane: crates, barrels and a dried-up fountain
    b.crate_(-20.0, -22.0, 1.3, 0.0);
    b.crate_(-21.0, 20.0, 1.3, 0.0);
    b.crate_(-21.0, 20.0, 1.0, 1.3);
    b.barrel(-19.5, -6.0, pal::BARREL_BLUE);
    b.cbox(-20.5, 6.0, 3.0, 3.0, 0.0, 0.6, pal::CONCRETE_DARK);
    b.cbox(-20.5, 6.0, 0.8, 0.8, 0.6, 1.2, pal::CONCRETE);

    // south-west: two-storey house with a balcony looking at the allied site
    b.tower(&Tower {
        x0: -38.0,
        z0: 8.0,
        x1: -26.0,
        z1: 18.0,
        floors: 2,
        storey: 3.4,
        color: pal::ADOBE,
        roof: pal::ROOF_RED,
        ground: &[
            (Side::MaxX, door(11.0)),
            (Side::MinZ, door(-32.0)),
            (Side::MaxZ, window(-30.0)),
        ],
        upper: &[
            (Side::MaxX, upper_door(13.0)),
            (Side::MaxZ, window(-31.0)),
            (Side::MinZ, window(-32.0)),
        ],
        roof_access: false,
    });
    b.balcony(Side::MaxX, 10.5, 16.0, -26.0, 3.4, 1.6);

    // north-west house
    b.building(
        -38.0,
        -32.0,
        -28.0,
        -22.0,
        0.0,
        3.6,
        &[
            (Side::MaxX, door(-27.0)),
            (Side::MaxZ, door(-33.0)),
            (Side::MinX, window(-27.0)),
        ],
        pal::ADOBE_LIGHT,
        Some(pal::ADOBE),
    );

    // EAST: market house, two storeys, roof, balcony over the square
    b.tower(&Tower {
        x0: 22.0,
        z0: -16.0,
        x1: 36.0,
        z1: -4.0,
        floors: 2,
        storey: 3.4,
        color: pal::WALL_TAN,
        roof: pal::ADOBE,
        // the west strip holds the first flight of stairs, so doors sit north of it
        ground: &[
            (Side::MinX, door(-14.0)),
            (Side::MinX, window(-8.0)),
            (Side::MaxZ, wide_door(29.5)),
            (Side::MaxX, window(-10.0)),
        ],
        upper: &[
            (Side::MinX, upper_door(-14.0)),
            (Side::MinX, window(-8.0)),
            (Side::MaxZ, window(29.0)),
            (Side::MaxX, window(-10.0)),
        ],
        roof_access: true,
    });
    b.balcony(Side::MinX, -15.0, -5.0, 22.0, 3.4, 1.5);
    b.push_detail(22.0 - 1.6, 5.6, -15.0, 22.0, 5.7, -5.0, pal::AWNING_A);

    // south-east: small house with stairs onto its flat roof
    b.tower(&Tower {
        x0: 24.0,
        z0: 6.0,
        x1: 36.0,
        z1: 16.0,
        floors: 1,
        storey: 3.6,
        color: pal::ADOBE_LIGHT,
        roof: pal::ADOBE,
        ground: &[
            (Side::MinX, door(15.0)),
            (Side::MinZ, door(32.0)),
            (Side::MaxZ, door(26.0)),
        ],
        upper: &[],
        roof_access: true,
    });

    // east lane cover
    b.crate_(19.5, 18.0, 1.3, 0.0);
    b.barrel(19.5, -19.0, pal::BARREL_BLUE);
    b.barrel(20.3, -19.5, pal::BARREL_BLUE);
    b.burnt_car(19.0, 2.0, false);

    // north houses (axis side)
    b.building(
        -14.0,
        -32.0,
        -3.0,
        -22.0,
        0.0,
        3.8,
        &[
            (Side::MaxZ, door(-8.5)),
            (Side::MinX, door(-27.0)),
            (Side::MaxX, window(-27.0)),
        ],
        pal::ADOBE,
        Some(pal::ADOBE_LIGHT),
    );
    b.building(
        26.0,
        -34.0,
        36.0,
        -24.0,
        0.0,
        3.6,
        &[(Side::MinX, door(-29.0)), (Side::MaxZ, door(31.0))],
        pal::WALL_TAN,
        Some(pal::ROOF_RED),
    );
    b.sandbags(-2.0, -21.0, 6.0, -20.0);

    // south houses (allied side)
    b.building(
        6.0,
        19.0,
        18.0,
        29.0,
        0.0,
        3.8,
        &[
            (Side::MinZ, door(12.0)),
            (Side::MinX, door(24.0)),
            (Side::MaxX, window(24.0)),
        ],
        pal::ADOBE_LIGHT,
        Some(pal::ADOBE),
    );
    b.sandbags(-6.0, 20.0, 2.0, 21.0);
    b.rubble(-8.0, 24.0);

    let site_allies = vec3(-20.0, 0.0, 27.0);
    let site_axis = vec3(20.0, 0.0, -27.0);
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);
    site_pad(&mut b, site_axis, pal::PAD_AXIS);
    b.tank(-24.0, 31.0);
    b.tank(-16.0, 31.5);
    b.tank(16.5, -31.0);
    b.tank(22.5, -31.5);

    let allies = (0..9)
        .map(|i| spawn(-16.0 + i as f32 * 4.0, 34.8, 0.0))
        .collect();
    let axis = (0..9)
        .map(|i| spawn(-16.0 + i as f32 * 4.0, -34.8, PI))
        .collect();

    Map {
        id: MapId::Wreck,
        name: "Аварія",
        min,
        max,
        boxes: b.boxes,
        spawns: [allies, axis],
        bomb: vec3(3.0, 0.0, 6.5),
        sites: [site_allies, site_axis],
        sky: [140, 196, 246],
        fog: [236, 214, 176],
    }
}

/// «Будмайданчик» — construction site: an unfinished two-level concrete frame with
/// scaffolding in the middle, a red office with a roof on the west, a car lot on the east.
pub fn site() -> Map {
    let min = vec2(-40.0, -40.0);
    let max = vec2(40.0, 40.0);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::DIRT, pal::CONCRETE_DARK, 5.0);
    b.decal(-40.0, -3.0, 40.0, 3.0, 0.0, pal::ASPHALT);
    b.decal(14.0, -20.0, 38.0, 20.0, 0.0, pal::ASPHALT);
    for z in [-14.0f32, -6.0, 2.0, 10.0] {
        for x in [17.0f32, 23.0, 29.0, 35.0] {
            b.decal(x - 0.05, z - 2.0, x + 0.05, z + 2.0, 0.03, pal::WALL_WHITE);
        }
    }

    // --- centre: unfinished concrete frame (ground floor + first floor + half roof)
    let (x0, z0, x1, z1) = (-9.0f32, -7.0f32, 9.0f32, 7.0f32);
    let h = 3.4;
    for x in [x0, -3.0, 3.0, x1] {
        for z in [z0, 0.0, z1] {
            let px = x.clamp(x0 + 0.3, x1 - 0.3);
            let pz = z.clamp(z0 + 0.3, z1 - 0.3);
            b.cbox(px, pz, 0.6, 0.6, 0.0, 2.0 * h, pal::CONCRETE);
        }
    }
    // a few brick walls with big gaps on the ground floor
    b.wall_x(
        x0,
        x1,
        z0 + 0.15,
        0.0,
        h,
        0.3,
        &[wide_door(-5.0), wide_door(5.0), window(0.0)],
        pal::BRICK,
    );
    b.wall_z(
        z0 + 0.3,
        0.0,
        x0 + 0.15,
        0.0,
        h,
        0.3,
        &[door(-3.5)],
        pal::BRICK,
    );
    // first floor slab, with the stairwell hole on the east side and an unfinished gap
    b.floor_with_hole(x0, z0, x1, z1, h, (5.0, -1.0, 7.4, 5.7), pal::CONCRETE);
    b.stairs(
        6.2,
        5.6,
        (0.0, -1.0),
        2.2,
        10,
        0.34,
        0.62,
        0.0,
        pal::CONCRETE_DARK,
    );
    // half walls on the first floor (cover with a view)
    b.wall_x(x0, -2.0, z0 + 0.15, h, 1.1, 0.3, &[], pal::BRICK);
    b.wall_z(0.5, z1, x1 - 0.15, h, 1.1, 0.3, &[], pal::BRICK);
    // second stair up to the half roof over the west part
    b.stairs(
        -7.2,
        5.6,
        (0.0, -1.0),
        2.2,
        10,
        0.34,
        0.62,
        h,
        pal::CONCRETE_DARK,
    );
    b.floor_with_hole(
        x0,
        z0,
        0.0,
        z1,
        2.0 * h,
        (-8.4, -1.0, -6.0, 5.7),
        pal::CONCRETE,
    );
    b.block(x0, 2.0 * h, z0, 0.0, 2.0 * h + 0.9, z0 + 0.3, pal::CONCRETE);
    b.block(
        x0,
        2.0 * h,
        z0 + 0.3,
        x0 + 0.3,
        2.0 * h + 0.9,
        z1,
        pal::CONCRETE,
    );
    // scaffolding along the south face, leading onto the first floor
    b.scaffold_x(-5.0, 4.0, 7.0, 1.4, h);
    // building material around
    b.cbox(-3.0, -10.0, 4.0, 1.4, 0.0, 0.8, pal::WOOD);
    b.cbox(-3.0, -10.0, 3.0, 1.2, 0.8, 0.5, pal::WOOD);
    b.rubble(11.5, -4.0);
    b.rubble(-12.0, 3.0);

    // --- west: red office, two storeys with a roof
    b.tower(&Tower {
        x0: -35.0,
        z0: -22.0,
        x1: -23.0,
        z1: -10.0,
        floors: 2,
        storey: 3.4,
        color: pal::BRICK,
        roof: pal::ROOF_GREY,
        ground: &[
            (Side::MaxX, door(-13.0)),
            (Side::MaxX, window(-19.0)),
            (Side::MaxZ, door(-29.0)),
            (Side::MinZ, door(-27.0)),
        ],
        // on the upper floor the east strip holds the flight to the roof: balcony door north of it
        upper: &[
            (Side::MaxX, upper_door(-19.5)),
            (Side::MaxX, window(-13.0)),
            (Side::MaxZ, window(-30.0)),
            (Side::MinZ, window(-28.0)),
        ],
        roof_access: true,
    });
    b.balcony(Side::MaxX, -20.0, -12.0, -23.0, 3.4, 1.4);

    // --- east: car lot
    let colors = [pal::CAR_A, pal::CAR_B, pal::CAR_C, pal::WALL_BLUE];
    let mut k = 0;
    for z in [-14.0f32, -6.0, 2.0, 10.0] {
        for x in [20.0f32, 32.0] {
            if (k * 7) % 5 == 2 {
                b.burnt_car(x, z, false);
            } else {
                b.car(x, z, false, colors[k % colors.len()]);
            }
            k += 1;
        }
    }
    b.cbox(26.0, -20.5, 6.0, 1.0, 0.0, 2.6, pal::WALL_WHITE); // ticket booth
    b.cbox(26.0, -20.5, 6.4, 1.4, 2.6, 0.2, pal::ROOF_RED);
    b.wall_z(
        -18.0,
        16.0,
        38.5,
        0.0,
        2.2,
        0.4,
        &[door(-1.0)],
        pal::CONCRETE_DARK,
    );

    // --- north: storage yard
    b.container(-14.0, -28.0, true, 0.0, pal::CONT_BLUE);
    b.container(-14.0, -28.0, true, 2.6, pal::CONT_RED);
    b.container(4.0, -31.0, false, 0.0, pal::CONT_ORANGE);
    b.container(22.0, -31.0, true, 0.0, pal::CONT_GREEN);
    b.crate_(-6.0, -24.0, 1.3, 0.0);
    b.crate_(-5.0, -22.6, 1.0, 0.0);
    b.cbox(9.0, -22.0, 1.4, 4.0, 0.0, 1.4, pal::CONCRETE); // concrete pipes
    b.cbox(11.0, -22.0, 1.4, 4.0, 0.0, 1.4, pal::CONCRETE);

    // --- south-west: alley with a small shop and broken walls
    b.building(
        -35.0,
        16.0,
        -24.0,
        26.0,
        0.0,
        3.6,
        &[
            (Side::MaxX, door(21.0)),
            (Side::MinZ, door(-29.0)),
            (Side::MaxZ, window(-30.0)),
        ],
        pal::WALL_GREEN,
        Some(pal::ROOF_RED),
    );
    b.wall_x(
        -22.0,
        -10.0,
        14.0,
        0.0,
        2.2,
        0.4,
        &[door(-16.0)],
        pal::BRICK,
    );
    b.cbox(-18.0, 20.0, 2.2, 1.2, 0.0, 1.3, pal::CONT_GREEN); // dumpster
    b.barrel(-12.0, 18.5, pal::BARREL_RED);

    // --- south-east: sand piles and a cement mixer
    b.cbox(20.0, 26.0, 6.0, 4.0, 0.0, 0.5, pal::SAND);
    b.cbox(20.0, 26.0, 4.0, 2.6, 0.5, 0.5, pal::SAND);
    b.cbox(20.0, 26.0, 2.0, 1.2, 1.0, 0.5, pal::SAND);
    b.cbox(28.0, 22.0, 2.4, 2.4, 0.0, 1.8, pal::CONT_ORANGE);
    b.barrier(8.0, 16.0, true);
    b.barrier(-6.0, -16.0, true);
    b.tree(-36.5, 34.0, false);
    b.tree(36.0, 34.0, false);
    b.tree(-36.5, -36.0, false);

    let site_allies = vec3(-14.0, 0.0, 28.0);
    let site_axis = vec3(14.0, 0.0, -25.0);
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);
    site_pad(&mut b, site_axis, pal::PAD_AXIS);
    b.tank(-18.0, 31.5);
    b.tank(-10.0, 31.5);
    b.tank(17.5, -21.0);

    let allies = (0..9)
        .map(|i| spawn(-16.0 + i as f32 * 4.0, 36.5, 0.0))
        .collect();
    let axis = (0..9)
        .map(|i| spawn(-16.0 + i as f32 * 4.0, -36.5, PI))
        .collect();

    Map {
        id: MapId::Site,
        name: "Будмайданчик",
        min,
        max,
        boxes: b.boxes,
        spawns: [allies, axis],
        bomb: vec3(0.0, 0.0, 11.0),
        sites: [site_allies, site_axis],
        sky: [160, 200, 240],
        fog: [210, 214, 220],
    }
}
