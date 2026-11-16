//! «Депо» — a railway depot on a clear morning, 120 × 100 m.
//!
//! The line runs east–west through the middle with wagons standing on it and the bomb
//! between them, under a footbridge whose deck looks over the whole yard (and is seen
//! from all of it). Each team has, on its side of the line: an engine shed you fight
//! through, a container yard with a ladder onto the stacks, a filling station, a road
//! with a level crossing, a two-storey office with a balcony and, behind it, the
//! Sabotage site by the fuel tanks.
//!
//! The map is point-symmetric (x, z → −x, −z): the south half is built and copied over
//! to the north in other colours, so both teams have the same ground.

use crate::map::{
    door, pal, shade, swap_color, wide_door, window, Map, MapBuilder, MapId, Opening, Side,
};
use crate::maps::{site_pad, spawn_rows};
use crate::props::Tower;
use glam::{vec2, vec3};

const HALF_X: f32 = 60.0;
const HALF_Z: f32 = 50.0;

const BALLAST: [u8; 3] = [128, 122, 116];
const SLEEPER: [u8; 3] = [92, 70, 52];
const LOCO: [u8; 3] = [52, 110, 84];
const PLATFORM: [u8; 3] = [172, 170, 164];
/// The footbridge's deck.
pub const BRIDGE_Y: f32 = 3.6;

fn recolor(c: [u8; 3]) -> [u8; 3] {
    swap_color(
        c,
        &[
            (pal::CONT_RED, pal::CONT_GREEN),
            (pal::CONT_BLUE, pal::CONT_ORANGE),
            (pal::WALL_BLUE, pal::WALL_GREEN),
            (pal::WALL_PINK, pal::WALL_TAN),
            (pal::ROOF_RED, pal::ROOF_GREY),
            (pal::CAR_A, pal::CAR_C),
        ],
    )
}

fn upper_door(at: f32) -> Opening {
    Opening {
        at,
        w: 1.6,
        bottom: 0.0,
        top: 2.4,
    }
}

impl MapBuilder {
    /// A water tower: four legs and a tank high up.
    fn water_tower(&mut self, cx: f32, cz: f32) {
        for (dx, dz) in [(-1.6f32, -1.6f32), (1.6, -1.6), (-1.6, 1.6), (1.6, 1.6)] {
            self.cbox(cx + dx, cz + dz, 0.4, 0.4, 0.0, 8.0, pal::RUST);
        }
        self.push_detail(cx - 1.7, 4.0, cz - 1.7, cx + 1.7, 4.2, cz + 1.7, pal::RUST);
        self.cbox(cx, cz, 4.6, 4.6, 8.0, 3.4, pal::TANK);
        self.cbox(cx, cz, 5.0, 5.0, 11.4, 0.3, pal::ROOF_RED);
        self.push_detail(
            cx - 2.32,
            9.3,
            cz - 2.32,
            cx + 2.32,
            9.6,
            cz + 2.32,
            pal::METAL,
        );
    }

    /// A filling station: a canopy on posts over two pumps.
    fn filling_station(&mut self, cx: f32, cz: f32) {
        for (dx, dz) in [(-3.4f32, -2.0f32), (3.4, -2.0), (-3.4, 2.0), (3.4, 2.0)] {
            self.cbox(cx + dx, cz + dz, 0.35, 0.35, 0.0, 3.8, pal::WALL_WHITE);
        }
        self.cbox(cx, cz, 8.2, 5.4, 3.8, 0.45, pal::ROOF_RED);
        self.push_detail(
            cx - 4.12,
            3.9,
            cz - 2.72,
            cx + 4.12,
            4.15,
            cz + 2.72,
            pal::WALL_WHITE,
        );
        self.cbox(cx, cz, 5.2, 1.2, 0.0, 0.2, pal::CONCRETE);
        for dx in [-1.6f32, 1.6] {
            self.cbox(cx + dx, cz, 0.8, 0.55, 0.2, 1.5, pal::CONT_RED);
            self.push_detail(
                cx + dx - 0.3,
                1.1,
                cz - 0.3,
                cx + dx + 0.3,
                1.5,
                cz + 0.3,
                pal::WINDOW,
            );
        }
    }

    /// A stack of tyres.
    fn tyres(&mut self, cx: f32, cz: f32) {
        self.cbox(cx, cz, 0.9, 0.9, 0.0, 1.0, pal::TYRE);
        self.push_detail(
            cx - 0.47,
            0.32,
            cz - 0.47,
            cx + 0.47,
            0.36,
            cz + 0.47,
            pal::DARK,
        );
        self.push_detail(
            cx - 0.47,
            0.65,
            cz - 0.47,
            cx + 0.47,
            0.69,
            cz + 0.47,
            pal::DARK,
        );
    }
}

/// «Депо» — a railway depot.
pub fn depot() -> Map {
    let min = vec2(-HALF_X, -HALF_Z);
    let max = vec2(HALF_X, HALF_Z);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, pal::GRASS, pal::CONCRETE, 6.0);

    // the yard, the line through the middle, the roads with their level crossings
    b.decal(-50.0, -40.0, 50.0, 40.0, 0.0, pal::DIRT);
    b.decal(-HALF_X, -3.4, HALF_X, 3.4, 0.03, BALLAST);
    let mut x = -HALF_X + 0.6;
    while x < HALF_X {
        b.decal(x, -1.5, x + 0.4, 1.5, 0.06, SLEEPER);
        x += 1.5;
    }
    for z in [-1.0f32, 1.0] {
        b.push_detail(-HALF_X, 0.09, z - 0.07, HALF_X, 0.2, z + 0.07, pal::METAL);
    }
    for sx in [-1.0f32, 1.0] {
        let (r0, r1) = (sx * 32.0 - 4.0, sx * 32.0 + 4.0);
        b.decal(r0, -HALF_Z, r1, HALF_Z, 0.06, pal::ASPHALT);
        b.decal(r0, -3.4, r1, 3.4, 0.2, pal::ASPHALT);
        // (the planked crossing lies level with the rails' tops)
        b.decal(
            sx * 32.0 - 0.08,
            -HALF_Z,
            sx * 32.0 + 0.08,
            -4.0,
            0.09,
            pal::WALL_WHITE,
        );
        b.decal(
            sx * 32.0 - 0.08,
            4.0,
            sx * 32.0 + 0.08,
            HALF_Z,
            0.09,
            pal::WALL_WHITE,
        );
    }

    // the train on the line: an engine and wagons either side of the middle and beyond
    // the crossings (the same lengths both ways, so the cover is equal)
    b.wagon(-15.8, 0.0, true, Some(pal::CONT_BLUE));
    b.wagon(15.8, 0.0, true, None);
    b.wagon(45.8, 0.0, true, Some(pal::CONT_RED));
    // the engine
    b.cbox(-45.8, 0.0, 12.4, 2.7, 0.0, 1.1, pal::DARK);
    b.cbox(-46.8, 0.0, 9.6, 2.5, 1.1, 2.3, LOCO);
    b.cbox(-41.2, 0.0, 2.6, 2.7, 1.1, 3.0, shade(LOCO, 0.85));
    b.push_detail(-42.5, 2.9, -1.37, -39.9, 3.8, 1.37, pal::WINDOW);
    b.cbox(-49.5, 0.0, 0.8, 0.8, 3.4, 0.8, pal::DARK);
    b.push_detail(-51.65, 1.5, -1.0, -51.6, 2.0, 1.0, pal::CONT_YELLOW);

    // ---- the south half (the allies'); the north is its copy
    let mark = b.mark();

    // the footbridge over the line: stairs up from the yard, the deck across to the
    // other side (its railings are planks: no cover from bullets)
    let steps = 11;
    let rise = BRIDGE_Y / steps as f32;
    b.stairs(
        0.0,
        7.0 + steps as f32 * 0.62,
        (0.0, -1.0),
        2.6,
        steps,
        rise,
        0.62,
        0.0,
        PLATFORM,
    );
    b.block(-1.3, BRIDGE_Y - 0.25, 0.0, 1.3, BRIDGE_Y, 7.0, PLATFORM);
    for sx in [-1.0f32, 1.0] {
        b.wood(
            sx * 1.3 - 0.05,
            BRIDGE_Y,
            0.0,
            sx * 1.3 + 0.05,
            BRIDGE_Y + 1.0,
            7.0,
            pal::WOOD,
        );
        b.cbox(sx * 1.05, 4.4, 0.4, 0.4, 0.0, BRIDGE_Y - 0.25, pal::METAL);
    }

    // the engine shed: wide doors north and south, a wagon under repair inside
    b.building(
        -54.0,
        9.0,
        -34.0,
        31.0,
        0.0,
        6.5,
        &[
            (Side::MinZ, wide_door(-48.0)),
            (Side::MinZ, wide_door(-39.5)),
            (Side::MaxZ, wide_door(-48.0)),
            (Side::MaxZ, door(-38.0)),
            (Side::MaxX, wide_door(25.0)),
            (Side::MaxX, door(13.0)),
            (Side::MaxX, window(19.0)),
            (Side::MinX, window(15.0)),
            (Side::MinX, window(25.0)),
        ],
        pal::WALL_BLUE,
        Some(pal::ROOF_GREY),
    );
    b.decal(-53.7, 9.3, -34.3, 30.7, 0.0, PLATFORM);
    b.wagon(-48.0, 20.0, false, None);
    b.cbox(-41.0, 14.5, 1.0, 5.0, 0.0, 1.1, pal::WOOD);
    b.cbox(-36.5, 19.0, 1.4, 4.0, 0.0, 2.4, pal::METAL);
    b.crate_(-42.5, 27.5, 1.4, 0.0);
    b.crate_(-41.0, 27.8, 1.2, 0.0);
    b.crate_(-41.8, 27.6, 1.0, 1.4);
    b.barrel(-44.5, 11.0, pal::BARREL_BLUE);
    b.tyres(-36.0, 28.5);

    // between the shed and the middle: a lorry, the filling station, a broken wall
    b.lorry(-26.0, 15.0, pal::CAR_A, Some(pal::CONT_RED));
    b.filling_station(-16.0, 29.0);
    b.wall_x(
        -22.0,
        -13.0,
        20.0,
        0.0,
        2.4,
        0.5,
        &[window(-17.5)],
        pal::WALL_TAN,
    );
    b.wall_z(9.0, 15.0, -9.0, 0.0, 2.6, 0.5, &[], pal::WALL_TAN);
    b.container(-17.0, 9.0, true, 0.0, pal::CONT_GREEN);
    b.crate_(-12.6, 7.4, 1.2, 0.0);
    b.tyres(-12.0, 21.5);
    b.tyres(-11.1, 21.3);
    b.sacks(-6.5, 17.5);
    b.crate_(-5.0, 6.5, 1.3, 0.0);
    b.crate_(-6.3, 6.8, 1.0, 0.0);
    b.barrel(4.0, 15.5, pal::BARREL_RED);

    // the container yard east of the middle, a ladder onto its biggest stack
    b.container(9.0, 10.0, false, 0.0, pal::CONT_RED);
    b.container(18.0, 8.5, true, 0.0, pal::CONT_BLUE);
    b.container(18.0, 8.5, true, 2.6, pal::CONT_YELLOW);
    b.container(23.0, 17.0, false, 0.0, pal::CONT_GREEN);
    b.container(13.0, 19.0, true, 0.0, pal::CONT_ORANGE);
    b.container(13.0, 21.6, true, 0.0, pal::CONT_BLUE);
    b.container(13.0, 20.3, true, 2.6, pal::CONT_RED);
    b.ladder(vec3(10.0, 0.0, 20.3), vec3(-1.0, 0.0, 0.0), 5.2);
    b.container(21.0, 27.0, true, 0.0, pal::CONT_YELLOW);
    b.container(8.0, 29.0, false, 0.0, pal::CONT_GREEN);
    b.forklift(16.0, 14.0, true);
    b.reel(24.5, 9.5);

    // the road east of it: barriers by the crossing
    b.barrier(30.0, 7.0, true);
    b.lorry(33.6, 19.0, pal::CAR_C, Some(pal::CONT_BLUE));
    b.container(-30.6, 9.5, false, 0.0, pal::CONT_YELLOW);
    b.barrier(30.0, 26.0, true);

    // the office: two storeys, a balcony over the road
    b.tower(&Tower {
        x0: 40.0,
        z0: 9.0,
        x1: 52.0,
        z1: 21.0,
        floors: 2,
        storey: 3.4,
        color: pal::WALL_PINK,
        roof: pal::ROOF_RED,
        ground: &[
            (Side::MinX, door(11.0)),
            (Side::MaxZ, door(47.0)),
            (Side::MinZ, door(48.0)),
            (Side::MaxX, window(15.0)),
        ],
        upper: &[
            (Side::MinX, upper_door(11.0)),
            (Side::MinX, window(18.0)),
            (Side::MinZ, window(46.0)),
            (Side::MaxZ, window(46.0)),
            (Side::MaxX, window(15.0)),
        ],
        roof_access: false,
    });
    b.balcony(Side::MinX, 9.6, 13.0, 40.0, 3.4, 1.5);
    b.cbox(47.0, 12.0, 3.0, 1.0, 0.0, 1.0, pal::WOOD);
    b.cbox(49.5, 16.0, 1.0, 3.0, 3.4, 1.0, pal::WOOD);

    // behind the office: the Sabotage site by the fuel tanks, a wall north of it
    b.wall_x(
        40.0,
        51.0,
        26.5,
        0.0,
        2.6,
        0.5,
        &[door(44.0)],
        pal::WALL_TAN,
    );
    b.tank(54.5, 31.0);
    b.tank(54.5, 37.5);
    b.barrel(40.5, 38.5, pal::BARREL_RED);
    b.barrel(41.3, 38.9, pal::BARREL_RED);
    b.sacks(40.5, 31.0);

    // by the allies' end: a guard hut, sandbags, the water tower
    b.building(
        -47.0,
        36.5,
        -38.0,
        44.5,
        0.0,
        3.4,
        &[
            (Side::MinZ, door(-42.0)),
            (Side::MaxX, door(41.0)),
            (Side::MinX, window(40.0)),
        ],
        pal::WALL_GREEN,
        Some(pal::ROOF_RED),
    );
    b.water_tower(-52.0, 40.0);
    b.sandbags(-22.0, 38.0, -12.0, 39.0);
    b.sandbags(10.0, 38.0, 20.0, 39.0);
    b.sandbags(-4.0, 34.0, 4.0, 35.0);
    b.tree(-56.0, 46.0, false);
    b.tree(56.5, 45.0, false);
    b.tree(-57.0, 33.5, false);
    b.tree(56.0, 24.0, false);

    b.mirror_since(mark, recolor);

    let site_allies = vec3(46.0, 0.0, 33.0);
    let site_axis = vec3(-46.0, 0.0, -33.0);
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);
    site_pad(&mut b, site_axis, pal::PAD_AXIS);

    Map {
        id: MapId::Depot,
        name: "Депо",
        min,
        max,
        boxes: b.boxes,
        ladders: b.ladders,
        spawns: spawn_rows(28.0, 45.2, 12),
        bomb: vec3(0.0, 0.0, 0.0),
        sites: [site_allies, site_axis],
        // a fresh, clear morning: deep blue sky, crisp light
        sky: [120, 186, 246],
        fog: [196, 224, 244],
        tint: [0.98, 1.02, 1.0],
        terrain: None,
        loot: Vec::new(),
        nav_detail: Vec::new(),
        car_spots: Vec::new(),
        doors: Vec::new(),
    }
}
