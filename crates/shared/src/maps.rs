//! The built-in maps. Geometry is generated in code so client and server always agree.

use crate::map::{door, pal, wide_door, window, Map, MapBuilder, MapId, Opening, Side, Spawn};
use glam::{vec2, vec3, Vec3};
use std::f32::consts::PI;

pub fn build(id: MapId) -> Map {
    match id {
        MapId::Depot => depot(),
        MapId::Market => crate::maps_market::market(),
        MapId::Cargo => cargo(),
        MapId::Wreck => crate::maps_wreck::wreck(),
        MapId::Site => crate::maps_extra::site(),
        // generating the island takes a moment: build it once, hand out copies
        MapId::Island => {
            static ISLAND: std::sync::OnceLock<Map> = std::sync::OnceLock::new();
            ISLAND.get_or_init(crate::maps_island::island).clone()
        }
    }
}

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
    b.floor_with_hole(
        ox0 + 0.3,
        oz0 + 0.3,
        ox1 - 0.3,
        oz1 - 0.3,
        storey,
        (29.0, -9.7, 31.7, -2.9),
        pal::WOOD,
    );
    // stairs climb towards -z; the top step is next to the floor on the west side of the hole
    b.stairs(
        30.35,
        -2.9,
        (0.0, -1.0),
        2.6,
        10,
        storey / 10.0,
        0.67,
        0.0,
        pal::CONCRETE,
    );
    b.crate_(22.0, 4.0, 1.2, 0.0);
    b.cbox(25.0, -7.5, 3.0, 1.0, storey, 1.0, pal::WOOD);

    // mid-field cover walls
    b.wall_x(
        -17.0,
        -9.0,
        -14.0,
        0.0,
        2.4,
        0.5,
        &[window(-13.0)],
        pal::WALL_TAN,
    );
    b.wall_x(
        9.0,
        17.0,
        14.0,
        0.0,
        2.4,
        0.5,
        &[window(13.0)],
        pal::WALL_TAN,
    );
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
        &[
            (Side::MaxZ, door(-26.0)),
            (Side::MaxX, window(-30.5)),
            (Side::MinX, door(-30.5)),
        ],
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
        &[
            (Side::MinZ, door(26.0)),
            (Side::MinX, window(30.5)),
            (Side::MaxX, door(30.5)),
        ],
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

    let allies = (0..9)
        .map(|i| spawn(-16.0 + i as f32 * 4.0, 36.5, 0.0))
        .collect();
    let axis = (0..9)
        .map(|i| spawn(-16.0 + i as f32 * 4.0, -36.5, PI))
        .collect();

    Map {
        id: MapId::Depot,
        name: "Депо",
        min,
        max,
        boxes: b.boxes,
        ladders: b.ladders,
        spawns: [allies, axis],
        bomb: vec3(0.0, 0.0, 0.0),
        sites: [site_allies, site_axis],
        // a fresh, clear morning: deep blue sky, crisp light
        sky: [120, 186, 246],
        fog: [196, 224, 244],
        tint: [0.98, 1.02, 1.0],
        terrain: None,
        loot: Vec::new(),
        nav_detail: Vec::new(),
    }
}

/// «Вантаж» — tiny container yard for chaotic close-quarters fights.
fn cargo() -> Map {
    let min = vec2(-20.0, -20.0);
    let max = vec2(20.0, 20.0);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::CONCRETE, pal::CONT_BLUE, 5.0);
    b.decal(-20.0, -1.0, 20.0, 1.0, 0.0, pal::CONT_YELLOW);
    let cols = [
        pal::CONT_RED,
        pal::CONT_BLUE,
        pal::CONT_GREEN,
        pal::CONT_ORANGE,
        pal::CONT_YELLOW,
    ];
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
    for (x, z, s) in [
        (4.0, 2.5, 1.2),
        (-4.0, -2.5, 1.2),
        (5.5, -3.0, 1.0),
        (-5.5, 3.0, 1.0),
        (0.0, 13.5, 1.2),
        (0.0, -13.5, 1.2),
    ] {
        b.crate_(x, z, s, 0.0);
    }

    let site_allies = vec3(-14.5, 0.0, 14.5);
    let site_axis = vec3(14.5, 0.0, -14.5);
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);
    site_pad(&mut b, site_axis, pal::PAD_AXIS);

    let allies = [-16.0, -10.0, -5.0, 0.0, 5.0, 10.0, 16.0]
        .iter()
        .map(|&x| spawn(x, 18.0, 0.0))
        .collect();
    let axis = [-16.0, -10.0, -5.0, 0.0, 5.0, 10.0, 16.0]
        .iter()
        .map(|&x| spawn(x, -18.0, PI))
        .collect();

    Map {
        id: MapId::Cargo,
        name: "Вантаж",
        min,
        max,
        boxes: b.boxes,
        ladders: b.ladders,
        spawns: [allies, axis],
        bomb: vec3(0.0, 0.0, 0.0),
        sites: [site_allies, site_axis],
        // the port at dusk: teal sea haze, violet sky, cool light
        sky: [150, 140, 190],
        fog: [160, 190, 200],
        tint: [0.9, 0.95, 1.06],
        terrain: None,
        loot: Vec::new(),
        nav_detail: Vec::new(),
    }
}

/// Kept for map authors: an opening helper with explicit dimensions.
pub fn opening(at: f32, w: f32, bottom: f32, top: f32) -> Opening {
    Opening { at, w, bottom, top }
}
