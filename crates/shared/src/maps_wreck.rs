//! «Аварія» — a desert town on a hillside, 120 × 108 m.
//!
//! A crossroads in the valley (level 0) with the crashed helicopter in its square. The
//! north-east and south-west quarters stand on 1.2 m terraces behind retaining walls,
//! with houses, stalls and the Sabotage sites; the north-west and south-east quarters lie
//! low, each with a 2.4 m hill: an old fort with a sniper roof on one, a house with a
//! water tower on the other. Stairs link the levels. The terrain is point-symmetric
//! (x, z → −x, −z), so both teams get the same climbs and distances, but the buildings on
//! each side are different.
//!
//! Dusty noon: a pale hot sky, sand and adobe, warm light.

use crate::map::{door, pal, wide_door, window, Map, MapBuilder, MapId, Opening, Side, Spawn};
use crate::props::Tower;
use glam::{vec2, vec3, Vec3};
use std::f32::consts::PI;

const HALF_X: f32 = 60.0;
const HALF_Z: f32 = 54.0;
/// Terrace and hill heights.
const TERRACE: f32 = 1.2;
const HILL: f32 = 2.4;

const SAND: [u8; 3] = [224, 198, 146];
const DUST: [u8; 3] = [206, 182, 138];
const RETAIN: [u8; 3] = [170, 140, 100];
const PAVING: [u8; 3] = [214, 192, 150];
const ADOBE: [u8; 3] = [212, 166, 112];
const ADOBE_LIGHT: [u8; 3] = [236, 206, 160];
const ADOBE_PINK: [u8; 3] = [222, 160, 128];
const WHITEWASH: [u8; 3] = [240, 232, 214];
const ROOF_CLAY: [u8; 3] = [186, 104, 70];
const TEAL: [u8; 3] = [70, 150, 150];

fn upper_door(at: f32) -> Opening {
    Opening {
        at,
        w: 1.6,
        bottom: 0.0,
        top: 2.4,
    }
}

fn spawn(x: f32, z: f32, yaw: f32) -> Spawn {
    Spawn {
        pos: vec3(x, 0.0, z),
        yaw,
    }
}

/// Site marker on the ground at `p` (p.y is the ground's height).
fn site_pad(b: &mut MapBuilder, p: Vec3, c: [u8; 3]) {
    b.decal(p.x - 2.6, p.z - 2.6, p.x + 2.6, p.z + 2.6, p.y + 0.01, c);
    b.decal(
        p.x - 1.8,
        p.z - 1.8,
        p.x + 1.8,
        p.z + 1.8,
        p.y + 0.04,
        pal::WALL_WHITE,
    );
    b.decal(p.x - 1.5, p.z - 1.5, p.x + 1.5, p.z + 1.5, p.y + 0.07, c);
}

/// Stairs climbing `h` (rise 0.3, run 0.55), ending at the edge (`x`, `z`) of the higher
/// ground and going up in direction `dir`.
fn stairs_up(b: &mut MapBuilder, x: f32, z: f32, dir: (f32, f32), h: f32, width: f32) {
    let steps = (h / 0.3).round() as u32;
    let run = 0.55;
    let len = steps as f32 * run;
    b.stairs(
        x - dir.0 * len,
        z - dir.1 * len,
        dir,
        width,
        steps,
        h / steps as f32,
        run,
        0.0,
        RETAIN,
    );
}

/// The same terrain on both sides: terraces, hills and the stairs between the levels.
fn terrain(b: &mut MapBuilder, s: f32) {
    let r = |x0: f32, z0: f32, x1: f32, z1: f32| {
        let (a, c) = (s * x0, s * x1);
        let (bz, d) = (s * z0, s * z1);
        (a.min(c), bz.min(d), a.max(c), bz.max(d))
    };
    // the north-east terrace (south-west when turned round)
    for (x0, z0, x1, z1) in [r(16.0, -44.0, 60.0, -3.0), r(4.0, -44.0, 16.0, -12.0)] {
        b.terrace(x0, z0, x1, z1, TERRACE, RETAIN, PAVING);
    }
    // the hill in the north-west (south-east)
    let (x0, z0, x1, z1) = r(-56.0, -40.0, -32.0, -16.0);
    b.terrace(x0, z0, x1, z1, HILL, RETAIN, DUST);
    // stairs up the terrace: from the road, the north road, the square, the yard
    for (x, z, dir) in [
        (26.0, -3.0, (0.0, -1.0)),
        (50.0, -3.0, (0.0, -1.0)),
        (4.0, -24.0, (1.0, 0.0)),
        (4.0, -38.0, (1.0, 0.0)),
        (10.0, -12.0, (0.0, -1.0)),
        (16.0, -7.5, (1.0, 0.0)),
        (22.0, -44.0, (0.0, 1.0)),
        (46.0, -44.0, (0.0, 1.0)),
    ] {
        stairs_up(b, s * x, s * z, (s * dir.0, s * dir.1), TERRACE, 3.0);
    }
    // stairs up the hill
    for (x, z, dir) in [
        (-32.0, -28.0, (-1.0, 0.0)),
        (-44.0, -16.0, (0.0, -1.0)),
        (-38.0, -40.0, (0.0, 1.0)),
    ] {
        stairs_up(b, s * x, s * z, (s * dir.0, s * dir.1), HILL, 3.0);
    }
    // waist-high walls along the terrace edge over the road, open at the stairs
    let (a, bb) = (s * 16.0, s * 60.0);
    let line = s * -3.3;
    let ops = [wide_door(s * 26.0), wide_door(s * 50.0)];
    b.lift(TERRACE, |b| {
        b.low_wall(
            true,
            a.min(bb),
            a.max(bb),
            line,
            0.9,
            0.4,
            &ops,
            ADOBE_LIGHT,
        )
    });
    // and round the hill top, with gates where the stairs come up
    let (x0, z0, x1, z1) = r(-56.0, -40.0, -32.0, -16.0);
    // (edge the stairs reach, along which axis, where along it)
    let gates = [
        (s * -32.0, false, s * -28.0),
        (s * -16.0, true, s * -44.0),
        (s * -40.0, true, s * -38.0),
    ];
    let ops_at = |line: f32, along_x: bool| -> Vec<Opening> {
        gates
            .iter()
            .filter(|g| g.1 == along_x && (g.0 - line).abs() < 0.5)
            .map(|g| wide_door(g.2))
            .collect()
    };
    let (n, so) = (ops_at(z0, true), ops_at(z1, true));
    let (w, e) = (ops_at(x0, false), ops_at(x1, false));
    b.lift(HILL, |b| {
        b.low_wall(true, x0, x1, z0 + 0.2, 1.0, 0.4, &n, ADOBE);
        b.low_wall(true, x0, x1, z1 - 0.2, 1.0, 0.4, &so, ADOBE);
        b.low_wall(false, z0 + 0.4, z1 - 0.4, x0 + 0.2, 1.0, 0.4, &w, ADOBE);
        b.low_wall(false, z0 + 0.4, z1 - 0.4, x1 - 0.2, 1.0, 0.4, &e, ADOBE);
    });
}

/// Water tower: four legs and a tank (on the south-east hill).
fn water_tower(b: &mut MapBuilder, cx: f32, cz: f32) {
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        b.cbox(
            cx + sx * 1.3,
            cz + sz * 1.3,
            0.25,
            0.25,
            0.0,
            6.0,
            pal::METAL,
        );
    }
    b.cbox(cx, cz, 3.4, 3.4, 6.0, 2.6, TEAL);
    b.cbox(cx, cz, 3.8, 3.8, 8.6, 0.3, pal::METAL);
}

/// A stall with a cloth awning and a wooden counter.
fn stall(b: &mut MapBuilder, cx: f32, cz: f32, c: [u8; 3]) {
    b.wood(cx - 1.3, 0.0, cz - 0.5, cx + 1.3, 1.0, cz + 0.5, pal::WOOD);
    for (sx, sz) in [(-1.4f32, -0.9f32), (1.4, -0.9), (-1.4, 0.9), (1.4, 0.9)] {
        b.push_detail(
            cx + sx - 0.05,
            0.0,
            cz + sz - 0.05,
            cx + sx + 0.05,
            2.3,
            cz + sz + 0.05,
            pal::WOOD,
        );
    }
    b.cloth(cx - 1.6, 2.3, cz - 1.1, cx + 1.6, 2.38, cz + 1.1, c);
    for (k, dx) in [-0.7f32, 0.0, 0.7].iter().enumerate() {
        let col = [[220, 60, 50], [250, 160, 40], [120, 190, 70]][k];
        b.push_detail(
            cx + dx - 0.3,
            1.0,
            cz - 0.3,
            cx + dx + 0.3,
            1.25,
            cz + 0.3,
            col,
        );
    }
}

/// Plank fence (wood: bullets go through).
fn plank_fence(b: &mut MapBuilder, x0: f32, z0: f32, x1: f32, z1: f32) {
    b.wood(x0, 0.0, z0, x1, 1.7, z1, pal::WOOD);
}

pub fn wreck() -> Map {
    let min = vec2(-HALF_X, -HALF_Z);
    let max = vec2(HALF_X, HALF_Z);
    let mut b = MapBuilder::new();
    b.ground_and_bounds(min, max, SAND, ADOBE, 7.0);

    // ---- the valley: the crossroads and the square
    b.decal(-4.0, -HALF_Z, 4.0, HALF_Z, 0.0, DUST);
    b.decal(-HALF_X, -3.0, HALF_X, 3.0, 0.0, DUST);
    b.decal(-16.0, -12.0, 16.0, 12.0, 0.01, pal::CONCRETE);
    terrain(&mut b, 1.0);
    terrain(&mut b, -1.0);

    // the square: the wreck, burnt cars, barriers, barrels, palms, stalls
    b.heli_wreck(-7.0, -6.5);
    b.burnt_car(9.0, 6.5, true);
    b.burnt_car(-10.5, 7.5, false);
    b.barrier(6.0, -7.0, true);
    b.barrier(-5.0, 9.5, true);
    b.barrier(12.0, -1.0, false);
    for (x, z, c) in [
        (-13.0, 2.0, pal::BARREL_BLUE),
        (-13.5, 3.0, pal::BARREL_RED),
        (13.0, 4.5, pal::BARREL_RED),
    ] {
        b.barrel(x, z, c);
    }
    for (x, z) in [(-14.5, -10.5), (14.5, 10.5), (14.5, -10.5), (-14.5, 10.5)] {
        b.tree(x, z, true);
    }
    stall(&mut b, 2.0, 10.0, pal::AWNING_A);
    stall(&mut b, -2.0, -10.0, pal::AWNING_B);

    // ---- north-west (low quarter, the axis side): the old fort on the hill
    b.lift(HILL, |b| {
        b.tower(&Tower {
            x0: -52.0,
            z0: -36.0,
            x1: -40.0,
            z1: -24.0,
            floors: 3,
            storey: 3.4,
            color: ADOBE_LIGHT,
            roof: ADOBE,
            ground: &[
                (Side::MaxX, door(-30.0)),
                (Side::MaxZ, door(-48.0)),
                (Side::MinZ, door(-45.0)),
                (Side::MinX, window(-30.0)),
            ],
            upper: &[
                (Side::MaxX, window(-29.0)),
                (Side::MaxX, window(-33.0)),
                (Side::MaxZ, window(-46.0)),
                (Side::MinZ, window(-46.0)),
                (Side::MinX, window(-30.0)),
            ],
            roof_access: true,
        });
        b.crate_(-54.0, -21.0, 1.2, 0.0);
        b.crate_(-35.0, -38.0, 1.1, 0.0);
    });
    // a two-storey house looking at the square
    b.tower(&Tower {
        x0: -30.0,
        z0: -24.0,
        x1: -19.0,
        z1: -14.0,
        floors: 2,
        storey: 3.4,
        color: ADOBE,
        roof: ROOF_CLAY,
        ground: &[
            (Side::MaxX, door(-20.0)),
            (Side::MaxZ, door(-22.0)),
            (Side::MinZ, window(-24.0)),
        ],
        upper: &[
            (Side::MaxX, upper_door(-17.0)),
            (Side::MaxZ, window(-25.0)),
            (Side::MinX, window(-19.0)),
        ],
        roof_access: false,
    });
    b.balcony(Side::MaxX, -18.5, -15.0, -19.0, 3.4, 1.5);
    b.building(
        -28.0,
        -42.0,
        -18.0,
        -32.0,
        0.0,
        3.6,
        &[
            (Side::MaxX, door(-37.0)),
            (Side::MinZ, door(-23.0)),
            (Side::MaxZ, window(-23.0)),
        ],
        ADOBE_PINK,
        Some(ADOBE),
    );
    b.building(
        -14.0,
        -40.0,
        -6.0,
        -30.0,
        0.0,
        3.4,
        &[(Side::MaxX, door(-35.0)), (Side::MaxZ, door(-10.0))],
        WHITEWASH,
        Some(ROOF_CLAY),
    );
    b.building(
        -58.0,
        -12.0,
        -48.0,
        -4.0,
        0.0,
        3.6,
        &[(Side::MaxZ, door(-53.0)), (Side::MaxX, window(-8.0))],
        ADOBE_LIGHT,
        Some(ADOBE),
    );
    b.building(
        -41.0,
        -11.0,
        -33.0,
        -4.0,
        0.0,
        3.4,
        &[(Side::MaxZ, door(-37.0)), (Side::MinX, door(-7.5))],
        ADOBE_PINK,
        Some(ROOF_CLAY),
    );
    plank_fence(&mut b, -31.0, -12.6, -24.0, -12.5);
    b.car(-24.0, -30.0, false, pal::WALL_BLUE);
    b.barrel(-16.0, -26.0, pal::BARREL_BLUE);
    for (x, z) in [(-10.0, -24.0), (-58.0, -30.0)] {
        b.tree(x, z, true);
    }

    // ---- north-east terrace (the axis site)
    let site_axis = vec3(38.0, TERRACE, -28.0);
    b.lift(TERRACE, |b| {
        b.building(
            20.0,
            -42.0,
            30.0,
            -32.0,
            0.0,
            3.6,
            &[
                (Side::MaxZ, door(25.0)),
                (Side::MaxX, door(-37.0)),
                (Side::MinX, window(-37.0)),
            ],
            ADOBE,
            Some(ROOF_CLAY),
        );
        b.tower(&Tower {
            x0: 44.0,
            z0: -24.0,
            x1: 56.0,
            z1: -13.0,
            floors: 2,
            storey: 3.4,
            color: WHITEWASH,
            roof: ADOBE,
            ground: &[
                (Side::MinX, door(-16.0)),
                (Side::MaxZ, door(47.0)),
                (Side::MinZ, window(50.0)),
            ],
            upper: &[
                // north of the stair flight that runs along this wall
                (Side::MinX, upper_door(-22.2)),
                (Side::MaxZ, window(52.0)),
                (Side::MaxX, window(-18.0)),
            ],
            roof_access: false,
        });
        b.balcony(Side::MinX, -23.6, -20.6, 44.0, 3.4, 1.5);
        b.building(
            48.0,
            -42.0,
            58.0,
            -33.0,
            0.0,
            3.4,
            &[(Side::MinX, door(-37.0)), (Side::MaxZ, window(53.0))],
            ADOBE_PINK,
            Some(ROOF_CLAY),
        );
        b.building(
            6.0,
            -40.0,
            14.0,
            -30.0,
            0.0,
            3.4,
            &[(Side::MaxZ, door(10.0)), (Side::MinX, window(-35.0))],
            ADOBE_LIGHT,
            Some(ADOBE),
        );
        stall(b, 32.0, -8.0, pal::AWNING_A);
        stall(b, 38.0, -8.0, pal::AWNING_B);
        b.tank(34.0, -32.5);
        b.tank(41.0, -33.0);
        b.sandbags(35.0, -23.0, 40.0, -22.2);
        b.car(24.0, -20.0, true, pal::WALL_TAN);
        b.crate_(30.0, -16.0, 1.2, 0.0);
        b.crate_(31.2, -16.3, 1.0, 0.0);
        plank_fence(b, 44.0, -30.0, 44.1, -26.0);
        b.tree(10.0, -20.0, true);
        b.tree(57.0, -8.0, true);
    });
    site_pad(&mut b, site_axis, pal::PAD_AXIS);

    // ---- south-east (low quarter, the allied side): the water-tower hill
    b.lift(HILL, |b| {
        b.tower(&Tower {
            x0: 38.0,
            z0: 22.0,
            x1: 50.0,
            z1: 34.0,
            floors: 2,
            storey: 3.4,
            color: WHITEWASH,
            roof: ROOF_CLAY,
            ground: &[
                (Side::MinX, door(28.0)),
                (Side::MinZ, door(44.0)),
                (Side::MaxX, window(28.0)),
            ],
            upper: &[
                (Side::MinX, window(27.0)),
                (Side::MinZ, window(46.0)),
                (Side::MaxX, window(30.0)),
                (Side::MaxZ, window(44.0)),
            ],
            roof_access: true,
        });
        water_tower(b, 53.5, 37.0);
        b.crate_(35.0, 37.5, 1.2, 0.0);
    });
    // a domed house, a shop row along the road, a garage yard
    b.building(
        18.0,
        30.0,
        29.0,
        41.0,
        0.0,
        4.2,
        &[
            (Side::MinX, door(35.0)),
            (Side::MinZ, wide_door(23.5)),
            (Side::MaxX, window(33.0)),
        ],
        WHITEWASH,
        Some(ADOBE_LIGHT),
    );
    b.cbox(23.5, 35.5, 5.0, 5.0, 4.55, 1.0, ADOBE_LIGHT);
    b.cbox(23.5, 35.5, 3.0, 3.0, 5.55, 0.8, TEAL);
    b.building(
        34.0,
        4.0,
        42.0,
        11.0,
        0.0,
        3.4,
        &[(Side::MinZ, door(38.0)), (Side::MinX, window(7.5))],
        ADOBE_PINK,
        Some(ROOF_CLAY),
    );
    b.building(
        48.0,
        4.0,
        58.0,
        12.0,
        0.0,
        3.6,
        &[(Side::MinZ, door(53.0)), (Side::MaxX, window(8.0))],
        ADOBE,
        Some(ADOBE_LIGHT),
    );
    b.building(
        18.0,
        14.0,
        29.0,
        24.0,
        0.0,
        3.6,
        &[
            (Side::MinX, door(19.0)),
            (Side::MaxZ, door(26.0)),
            (Side::MinZ, window(23.0)),
        ],
        ADOBE_LIGHT,
        Some(ROOF_CLAY),
    );
    plank_fence(&mut b, 6.0, 27.0, 14.0, 27.1);
    b.car(10.0, 31.0, false, pal::CAR_A);
    b.car(32.0, 46.0, true, pal::WALL_TAN);
    for (x, z) in [(8.0, 18.0), (58.0, 30.0), (30.0, 50.0)] {
        b.tree(x, z, true);
    }
    b.barrel(15.0, 26.0, pal::BARREL_RED);

    // ---- south-west terrace (the allied site): a walled compound and houses
    let site_allies = vec3(-38.0, TERRACE, 28.0);
    b.lift(TERRACE, |b| {
        // compound wall round the site, gates on three sides
        b.low_wall(
            true,
            -46.0,
            -30.0,
            20.0,
            1.6,
            0.4,
            &[wide_door(-38.0)],
            ADOBE,
        );
        b.low_wall(
            true,
            -46.0,
            -30.0,
            36.0,
            1.6,
            0.4,
            &[wide_door(-42.0)],
            ADOBE,
        );
        b.low_wall(
            false,
            20.2,
            35.8,
            -46.0,
            1.6,
            0.4,
            &[wide_door(28.0)],
            ADOBE,
        );
        b.tank(-44.0, 33.5);
        b.tank(-32.5, 33.0);
        b.sandbags(-40.0, 22.5, -36.0, 23.3);
        b.tower(&Tower {
            x0: -58.0,
            z0: 8.0,
            x1: -47.0,
            z1: 19.0,
            floors: 2,
            storey: 3.4,
            color: ADOBE_PINK,
            roof: ROOF_CLAY,
            ground: &[
                (Side::MaxX, door(15.0)),
                (Side::MinZ, door(-52.0)),
                (Side::MaxZ, window(-53.0)),
            ],
            upper: &[
                (Side::MaxX, upper_door(12.0)),
                (Side::MinZ, window(-50.0)),
                (Side::MaxZ, window(-54.0)),
            ],
            roof_access: false,
        });
        b.balcony(Side::MaxX, 9.5, 14.0, -47.0, 3.4, 1.5);
        b.building(
            -30.0,
            38.0,
            -20.0,
            43.0,
            0.0,
            3.4,
            &[(Side::MinZ, door(-25.0)), (Side::MaxX, window(40.5))],
            WHITEWASH,
            Some(ADOBE),
        );
        b.building(
            -14.0,
            30.0,
            -6.0,
            40.0,
            0.0,
            3.4,
            &[(Side::MinZ, door(-10.0)), (Side::MinX, door(35.0))],
            ADOBE_LIGHT,
            Some(ROOF_CLAY),
        );
        b.building(
            -26.0,
            6.0,
            -18.0,
            13.0,
            0.0,
            3.4,
            &[(Side::MinZ, door(-22.0)), (Side::MaxX, window(9.5))],
            ADOBE,
            Some(ADOBE_LIGHT),
        );
        stall(b, -52.0, 6.5, pal::AWNING_B);
        b.car(-24.0, 22.0, true, pal::CAR_B);
        b.crate_(-16.0, 20.0, 1.2, 0.0);
        plank_fence(b, -30.0, 16.0, -29.9, 20.0);
        b.tree(-12.0, 18.0, true);
        b.tree(-57.0, 40.0, true);
    });
    site_pad(&mut b, site_allies, pal::PAD_ALLIES);

    // ---- the yards at both ends of the north–south road
    for s in [-1.0f32, 1.0] {
        b.barrier(s * 10.0, s * -47.0, true);
        b.barrier(s * -10.0, s * -47.5, true);
        b.sandbags(
            s * 13.0 - 2.0,
            s * -46.0 - 0.4,
            s * 13.0 + 2.0,
            s * -46.0 + 0.4,
        );
    }
    b.car(-48.0, -49.0, true, pal::WALL_BLUE);
    b.container(30.0, -50.5, true, 0.0, pal::CONT_ORANGE);
    b.car(48.0, 49.0, true, pal::CAR_B);
    b.container(-32.0, 50.5, true, 0.0, pal::CONT_GREEN);

    let xs = [-20.0, -15.0, -10.0, -5.0, 0.0, 5.0, 10.0, 15.0, 20.0];
    let allies = xs.iter().map(|&x| spawn(x, 50.0, 0.0)).collect();
    let axis = xs.iter().map(|&x| spawn(x, -50.0, PI)).collect();

    Map {
        id: MapId::Wreck,
        name: "Аварія",
        min,
        max,
        boxes: b.boxes,
        ladders: b.ladders,
        spawns: [allies, axis],
        bomb: vec3(0.0, 0.0, 0.0),
        sites: [site_allies, site_axis],
        sky: [200, 222, 236],
        fog: [232, 214, 180],
        tint: [1.06, 1.0, 0.9],
        terrain: None,
        loot: Vec::new(),
        nav_detail: Vec::new(),
        car_spots: Vec::new(),
        doors: Vec::new(),
    }
}
