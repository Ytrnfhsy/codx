//! The built-in maps. Geometry is generated in code so client and server always agree.

use crate::map::{door, pal, wide_door, window, Map, MapBuilder, MapId, Opening, Side, Spawn};
use glam::{vec2, vec3, Vec3};
use std::f32::consts::{FRAC_PI_2, PI};

pub fn build(id: MapId) -> Map {
    match id {
        MapId::Depot => depot(),
        MapId::Market => market(),
        MapId::Cargo => cargo(),
    }
}

fn spawn(x: f32, z: f32, yaw: f32) -> Spawn {
    Spawn { pos: vec3(x, 0.0, z), yaw }
}

fn site_pad(b: &mut MapBuilder, p: Vec3, c: [u8; 3]) {
    b.decal(p.x - 2.6, p.z - 2.6, p.x + 2.6, p.z + 2.6, 0.0, c);
    b.decal(p.x - 1.8, p.z - 1.8, p.x + 1.8, p.z + 1.8, 0.03, pal::WALL_WHITE);
    b.decal(p.x - 1.5, p.z - 1.5, p.x + 1.5, p.z + 1.5, 0.06, c);
}

/// «Депо» — medium sized depot: warehouse, two-storey office, container yard.
fn depot() -> Map {
    let min = vec2(-40.0, -40.0);
    let max = vec2(40.0, 40.0);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::GRASS, pal::CONCRETE, 6.0);

    // dirt yard and roads
    b.decal(-16.0, -20.0, 16.0, 20.0, 0.0, pal::DIRT);
    b.decal(-40.0, -3.0, 40.0, 3.0, 0.0, pal::ASPHALT);
    b.decal(-3.0, -40.0, 3.0, -20.0, 0.0, pal::ASPHALT);
    b.decal(-3.0, 20.0, 3.0, 40.0, 0.0, pal::ASPHALT);

    // central containers around the bomb
    b.container(-6.0, -7.5, true, 0.0, pal::CONT_RED);
    b.container(6.0, 7.5, true, 0.0, pal::CONT_BLUE);
    b.container(6.0, 7.5, true, 2.6, pal::CONT_YELLOW);
    b.container(-10.0, 5.0, false, 0.0, pal::CONT_GREEN);
    b.container(10.0, -5.0, false, 0.0, pal::CONT_ORANGE);
    b.crate_(3.2, -2.5, 1.2, 0.0);
    b.crate_(-3.2, 2.5, 1.2, 0.0);
    b.crate_(-3.2, 2.5, 0.9, 1.2);
    b.crate_(-2.0, -3.6, 1.0, 0.0);
    b.crate_(13.5, 12.5, 1.3, 0.0);
    b.crate_(-13.5, -12.5, 1.3, 0.0);

    // west warehouse
    b.building(
        -34.0,
        -12.0,
        -20.0,
        12.0,
        0.0,
        6.0,
        &[
            (Side::MaxX, wide_door(-6.0)),
            (Side::MaxX, wide_door(6.0)),
            (Side::MaxX, window(0.0)),
            (Side::MinZ, door(-27.0)),
            (Side::MaxZ, door(-27.0)),
            (Side::MinX, window(-4.0)),
            (Side::MinX, window(4.0)),
        ],
        pal::WALL_BLUE,
        Some(pal::ROOF_GREY),
    );
    b.crate_(-30.5, -7.0, 1.4, 0.0);
    b.crate_(-30.5, -5.5, 1.4, 0.0);
    b.crate_(-30.5, -6.2, 1.2, 1.4);
    b.crate_(-24.5, 1.0, 1.4, 0.0);
    b.crate_(-29.0, 7.0, 1.4, 0.0);
    b.cbox(-27.0, -1.0, 5.0, 1.0, 0.0, 1.1, pal::WOOD);

    // east two-storey office with stairs
    let (ox0, oz0, ox1, oz1) = (20.0, -10.0, 32.0, 6.0);
    let storey = 3.4;
    b.building(
        ox0,
        oz0,
        ox1,
        oz1,
        0.0,
        storey,
        &[
            (Side::MinX, door(-6.0)),
            (Side::MinX, door(2.0)),
            (Side::MaxZ, door(26.0)),
            (Side::MinZ, door(23.0)),
            (Side::MaxX, window(-2.0)),
        ],
        pal::WALL_PINK,
        None,
    );
    b.building(
        ox0,
        oz0,
        ox1,
        oz1,
        storey,
        3.2,
        &[
            (Side::MinX, window(-6.5)),
            (Side::MinX, window(-2.0)),
            (Side::MinX, window(3.0)),
            (Side::MaxZ, window(25.0)),
            (Side::MinZ, window(25.0)),
        ],
        pal::WALL_PINK,
        Some(pal::ROOF_RED),
    );
    b.floor_with_hole(ox0 + 0.3, oz0 + 0.3, ox1 - 0.3, oz1 - 0.3, storey, (29.0, -9.7, 31.7, -2.9), pal::WOOD);
    // stairs climb towards -z; the top step is next to the floor on the west side of the hole
    b.stairs(30.35, -2.9, (0.0, -1.0), 2.6, 10, storey / 10.0, 0.67, 0.0, pal::CONCRETE);
    b.crate_(22.0, 4.0, 1.2, 0.0);
    b.cbox(25.0, -7.5, 3.0, 1.0, storey, 1.0, pal::WOOD);

    // mid-field cover walls
    b.wall_x(-17.0, -9.0, -14.0, 0.0, 2.4, 0.5, &[window(-13.0)], pal::WALL_TAN);
    b.wall_x(9.0, 17.0, 14.0, 0.0, 2.4, 0.5, &[window(13.0)], pal::WALL_TAN);
    b.wall_z(-14.0, -7.0, 16.0, 0.0, 2.8, 0.5, &[], pal::WALL_TAN);
    b.wall_z(7.0, 14.0, -16.0, 0.0, 2.8, 0.5, &[], pal::WALL_TAN);

    // truck
    b.cbox(13.0, -17.0, 2.4, 5.0, 0.0, 2.3, pal::CONT_GREEN);
    b.cbox(13.0, -20.6, 2.4, 1.8, 0.0, 1.8, pal::CAR_A);
    b.cbox(-13.0, 17.0, 2.4, 5.0, 0.0, 2.3, pal::CONT_RED);
    b.cbox(-13.0, 20.6, 2.4, 1.8, 0.0, 1.8, pal::CAR_A);

    // north base (axis)
    b.sandbags(-20.0, -22.5, -8.0, -21.5);
    b.sandbags(8.0, -22.5, 20.0, -21.5);
    b.building(
        -31.0,
        -35.0,
        -21.0,
        -26.0,
        0.0,
        3.6,
        &[(Side::MaxZ, door(-26.0)), (Side::MaxX, window(-30.5)), (Side::MinX, door(-30.5))],
        pal::WALL_GREEN,
        Some(pal::ROOF_RED),
    );
    // south base (allies)
    b.sandbags(-20.0, 21.5, -8.0, 22.5);
    b.sandbags(8.0, 21.5, 20.0, 22.5);
    b.building(
        21.0,
        26.0,
        31.0,
        35.0,
        0.0,
        3.6,
        &[(Side::MinZ, door(26.0)), (Side::MinX, window(30.5)), (Side::MaxX, door(30.5))],
        pal::WALL_GREEN,
        Some(pal::ROOF_RED),
    );

    // sabotage sites: allies defend the south one, axis the north one
    let site_allies = vec3(-14.0, 0.0, 28.0);
    let site_axis = vec3(14.0, 0.0, -28.0);
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);
    site_pad(&mut b, site_axis, pal::PAD_AXIS);
    b.tank(-18.0, 30.5);
    b.tank(-10.0, 31.0);
    b.tank(18.0, -30.5);
    b.tank(10.0, -31.0);

    let allies = (0..9).map(|i| spawn(-16.0 + i as f32 * 4.0, 36.5, 0.0)).collect();
    let axis = (0..9).map(|i| spawn(-16.0 + i as f32 * 4.0, -36.5, PI)).collect();

    Map {
        id: MapId::Depot,
        name: "Депо",
        min,
        max,
        boxes: b.boxes,
        spawns: [allies, axis],
        bomb: vec3(0.0, 0.0, 0.0),
        sites: [site_allies, site_axis],
        sky: [136, 200, 250],
        fog: [196, 226, 250],
    }
}

/// «Ринок» — long market street with alleys behind two rows of houses.
fn market() -> Map {
    let min = vec2(-50.0, -26.0);
    let max = vec2(50.0, 26.0);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::DIRT, pal::WALL_TAN, 7.0);
    b.decal(-50.0, -5.0, 50.0, 5.0, 0.0, pal::ASPHALT);
    b.decal(-50.0, -5.4, 50.0, -5.0, 0.0, pal::CONCRETE);
    b.decal(-50.0, 5.0, 50.0, 5.4, 0.0, pal::CONCRETE);

    let colors = [pal::WALL_PINK, pal::WALL_BLUE, pal::WALL_WHITE, pal::WALL_GREEN, pal::WALL_TAN];
    let centers = [-32.0f32, -16.0, 0.0, 16.0, 32.0];
    for (i, &cx) in centers.iter().enumerate() {
        let (x0, x1) = (cx - 6.0, cx + 6.0);
        // north row: street side is MaxZ (z = -6)
        let north_c = colors[i % colors.len()];
        let south_c = colors[(i + 2) % colors.len()];
        if i == 2 {
            // two-storey house in the middle of the north row
            b.building(
                x0,
                -15.0,
                x1,
                -6.0,
                0.0,
                3.4,
                &[(Side::MaxZ, door(cx - 3.0)), (Side::MinZ, door(cx + 3.0)), (Side::MaxZ, window(cx + 3.0))],
                north_c,
                None,
            );
            b.building(
                x0,
                -15.0,
                x1,
                -6.0,
                3.4,
                3.0,
                &[
                    (Side::MaxZ, window(cx - 3.5)),
                    (Side::MaxZ, window(cx + 0.5)),
                    (Side::MaxZ, window(cx + 4.0)),
                    (Side::MinX, window(-10.5)),
                    (Side::MaxX, window(-10.5)),
                ],
                north_c,
                Some(pal::ROOF_RED),
            );
            b.floor_with_hole(x0 + 0.3, -14.7, x1 - 0.3, -6.3, 3.4, (x0 + 0.3, -14.7, x0 + 7.3, -12.0), pal::WOOD);
            b.stairs(x0 + 7.3, -13.35, (-1.0, 0.0), 2.6, 10, 0.34, 0.68, 0.0, pal::CONCRETE);
        } else {
            b.building(
                x0,
                -15.0,
                x1,
                -6.0,
                0.0,
                4.2,
                &[
                    (Side::MaxZ, door(cx - 2.5)),
                    (Side::MaxZ, window(cx + 2.5)),
                    (Side::MinZ, door(cx + 2.5)),
                    (Side::MinX, window(-10.5)),
                    (Side::MaxX, window(-10.5)),
                ],
                north_c,
                Some(if i % 2 == 0 { pal::ROOF_RED } else { pal::ROOF_GREY }),
            );
            b.crate_(cx + 3.5, -12.5, 1.2, 0.0);
        }
        if i == 2 {
            // open market hall in the middle of the south row
            for (px, pz) in [(x0 + 0.3, 6.3), (x1 - 0.3, 6.3), (x0 + 0.3, 14.7), (x1 - 0.3, 14.7), (cx, 6.3), (cx, 14.7)] {
                b.cbox(px, pz, 0.5, 0.5, 0.0, 4.0, pal::WOOD);
            }
            b.block(x0 - 0.2, 4.0, 6.0, x1 + 0.2, 4.3, 15.0, pal::AWNING_B);
            for (k, sx) in [cx - 3.5, cx + 3.5].iter().enumerate() {
                b.cbox(*sx, 10.5, 2.4, 1.0, 0.0, 1.1, pal::WOOD);
                b.cbox(*sx, 10.5, 2.6, 1.2, 1.1, 0.12, if k == 0 { pal::AWNING_A } else { pal::CONT_YELLOW });
            }
        } else {
            b.building(
                x0,
                6.0,
                x1,
                15.0,
                0.0,
                4.2,
                &[
                    (Side::MinZ, door(cx + 2.5)),
                    (Side::MinZ, window(cx - 2.5)),
                    (Side::MaxZ, door(cx - 2.5)),
                    (Side::MinX, window(10.5)),
                    (Side::MaxX, window(10.5)),
                ],
                south_c,
                Some(if i % 2 == 1 { pal::ROOF_RED } else { pal::ROOF_GREY }),
            );
            b.crate_(cx - 3.5, 12.5, 1.2, 0.0);
        }
        // awnings over the street-side doors (thin, above head height)
        b.block(cx - 4.0, 2.9, -6.0, cx - 1.0, 3.0, -4.6, if i % 2 == 0 { pal::AWNING_A } else { pal::AWNING_B });
    }

    // cars and stalls in the street
    b.car(-21.0, -2.2, true, pal::CAR_A);
    b.car(11.0, 2.4, true, pal::CAR_B);
    b.car(29.0, -1.6, true, pal::CAR_C);
    b.car(-36.0, 2.2, true, pal::CAR_B);
    b.cbox(-8.0, 2.8, 2.0, 1.0, 0.0, 1.1, pal::WOOD);
    b.cbox(6.0, -3.0, 2.0, 1.0, 0.0, 1.1, pal::WOOD);
    b.crate_(-1.5, 3.6, 1.0, 0.0);
    b.crate_(1.8, -3.8, 1.0, 0.0);

    // back alleys: crates, dumpsters, fences
    for (x, z) in [(-24.0, -20.0), (-8.0, -21.0), (8.0, -19.5), (24.0, -21.0), (-24.0, 20.5), (-8.0, 19.5), (8.0, 21.0), (24.0, 19.5)] {
        b.cbox(x, z, 2.2, 1.2, 0.0, 1.3, pal::CONT_GREEN);
    }
    for (x, z) in [(-16.0, -22.0), (16.0, 22.0), (0.0, -21.5), (0.0, 21.5)] {
        b.crate_(x, z, 1.3, 0.0);
    }
    b.wall_z(-26.0, -18.5, -2.0, 0.0, 2.2, 0.3, &[], pal::WOOD);
    b.wall_z(18.5, 26.0, 2.0, 0.0, 2.2, 0.3, &[], pal::WOOD);

    // spawns: allies west, axis east
    let site_allies = vec3(-44.0, 0.0, -19.0);
    let site_axis = vec3(44.0, 0.0, 19.0);
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);
    site_pad(&mut b, site_axis, pal::PAD_AXIS);
    b.tank(-47.0, -22.5);
    b.tank(47.0, 22.5);
    b.sandbags(-42.0, -12.0, -41.0, -6.0);
    b.sandbags(41.0, 6.0, 42.0, 12.0);

    let allies = [-14.0, -9.0, -3.0, 0.0, 3.0, 9.0, 14.0, 20.0]
        .iter()
        .map(|&z| spawn(-46.5, z, -FRAC_PI_2))
        .collect();
    let axis = [-20.0, -14.0, -9.0, -3.0, 0.0, 3.0, 9.0, 14.0]
        .iter()
        .map(|&z| spawn(46.5, z, FRAC_PI_2))
        .collect();

    Map {
        id: MapId::Market,
        name: "Ринок",
        min,
        max,
        boxes: b.boxes,
        spawns: [allies, axis],
        bomb: vec3(0.0, 0.0, 0.0),
        sites: [site_allies, site_axis],
        sky: [250, 196, 150],
        fog: [252, 220, 186],
    }
}

/// «Вантаж» — tiny container yard for chaotic close-quarters fights.
fn cargo() -> Map {
    let min = vec2(-20.0, -20.0);
    let max = vec2(20.0, 20.0);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::CONCRETE, pal::CONT_BLUE, 5.0);
    b.decal(-20.0, -1.0, 20.0, 1.0, 0.0, pal::CONT_YELLOW);
    let cols = [pal::CONT_RED, pal::CONT_BLUE, pal::CONT_GREEN, pal::CONT_ORANGE, pal::CONT_YELLOW];
    let mut k = 0;
    for &x in &[-10.0f32, 0.0, 10.0] {
        for &z in &[-8.0f32, 8.0] {
            b.container(x, z, true, 0.0, cols[k % cols.len()]);
            k += 1;
        }
    }
    b.container(-10.0, 8.0, true, 2.6, pal::CONT_ORANGE);
    b.container(10.0, -8.0, true, 2.6, pal::CONT_GREEN);
    b.container(-15.0, 0.0, false, 0.0, pal::CONT_YELLOW);
    b.container(15.0, 0.0, false, 0.0, pal::CONT_RED);
    for (x, z, s) in [(4.0, 2.5, 1.2), (-4.0, -2.5, 1.2), (5.5, -3.0, 1.0), (-5.5, 3.0, 1.0), (0.0, 13.5, 1.2), (0.0, -13.5, 1.2)] {
        b.crate_(x, z, s, 0.0);
    }

    let site_allies = vec3(-14.5, 0.0, 14.5);
    let site_axis = vec3(14.5, 0.0, -14.5);
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);
    site_pad(&mut b, site_axis, pal::PAD_AXIS);

    let allies = [-16.0, -10.0, -5.0, 0.0, 5.0, 10.0, 16.0].iter().map(|&x| spawn(x, 18.0, 0.0)).collect();
    let axis = [-16.0, -10.0, -5.0, 0.0, 5.0, 10.0, 16.0].iter().map(|&x| spawn(x, -18.0, PI)).collect();

    Map {
        id: MapId::Cargo,
        name: "Вантаж",
        min,
        max,
        boxes: b.boxes,
        spawns: [allies, axis],
        bomb: vec3(0.0, 0.0, 0.0),
        sites: [site_allies, site_axis],
        sky: [150, 190, 236],
        fog: [200, 214, 236],
    }
}

/// Kept for map authors: an opening helper with explicit dimensions.
pub fn opening(at: f32, w: f32, bottom: f32, top: f32) -> Opening {
    Opening { at, w, bottom, top }
}
