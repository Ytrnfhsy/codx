use std::time::Instant;
use toon_server::nav::Nav;
use toon_shared::map::MapId;
use toon_shared::physics::World;
use toon_shared::rng::Rng;
#[test]
fn navperf() {
    for id in MapId::ALL {
        let m = toon_shared::maps::build(id);
        let w = World::from_map(&m);
        let t = Instant::now();
        let nav = Nav::build(&m, &w);
        let build = t.elapsed();
        let mut rng = Rng::new(3);
        let t = Instant::now();
        let mut ok = 0;
        for _ in 0..200 {
            let a = nav.random_point(&mut rng);
            let b = nav.random_point(&mut rng);
            if nav.path(a, b).is_some() {
                ok += 1;
            }
        }
        println!(
            "{:?}: nodes {} reachable {} build {:?} path avg {:?} ok {}/200",
            id,
            nav.node_count(),
            nav.reachable_count(),
            build,
            t.elapsed() / 200,
            ok
        );
    }
}

#[test]
fn random_points_are_connected() {
    for id in MapId::ALL {
        let m = toon_shared::maps::build(id);
        let w = World::from_map(&m);
        let nav = Nav::build(&m, &w);
        let mut rng = Rng::new(9);
        for _ in 0..100 {
            let a = nav.random_point(&mut rng);
            let b = nav.random_point(&mut rng);
            assert!(nav.path(a, b).is_some(), "{:?}: {:?} -> {:?}", id, a, b);
        }
    }
}

/// Walks a body with the real player physics along a nav path. Returns the final position.
fn walk(
    m: &toon_shared::map::Map,
    w: &World,
    nav: &Nav,
    from: glam::Vec3,
    to: glam::Vec3,
) -> glam::Vec3 {
    walk_looking(m, w, nav, from, to, None)
}

/// `walk`, looking at `look` (a fixed yaw) instead of where it goes, if given.
fn walk_looking(
    m: &toon_shared::map::Map,
    w: &World,
    nav: &Nav,
    from: glam::Vec3,
    to: glam::Vec3,
    look: Option<f32>,
) -> glam::Vec3 {
    use toon_shared::physics::{Body, MoveInput};
    use toon_shared::tuning::MoveTuning;
    let _ = m;
    let mt = MoveTuning::default();
    let mut body = Body {
        pos: from + glam::vec3(0.0, 0.02, 0.0),
        on_ground: true,
        ..Default::default()
    };
    let mut path = nav.path(from, to).expect("path");
    let mut stuck = 0;
    let mut last = body.pos;
    for tick in 0..(90.0 * 60.0) as usize {
        while let Some(wp) = path.first() {
            let flat = glam::vec3(wp.x - body.pos.x, 0.0, wp.z - body.pos.z).length();
            if flat < 0.45 && (wp.y - body.pos.y).abs() < 1.5 {
                path.remove(0);
            } else {
                break;
            }
        }
        let Some(wp) = path.first().copied() else {
            break;
        };
        let d = glam::vec3(wp.x - body.pos.x, 0.0, wp.z - body.pos.z).normalize_or_zero();
        // walk towards the waypoint, looking either the same way or (like a bot in a
        // fight) somewhere else
        let yaw = look.unwrap_or_else(|| toon_shared::yaw_of(d));
        let (f, r) = toon_shared::yaw_basis(yaw);
        let inp = MoveInput {
            forward: d.dot(f),
            right: d.dot(r),
            yaw,
            ..Default::default()
        };
        body.step(w, &inp, &mt, 1.0 / 60.0);
        if tick % 60 == 59 {
            if body.pos.distance(last) < 0.3 {
                stuck += 1;
                if stuck > 4 {
                    break;
                }
            } else {
                stuck = 0;
            }
            last = body.pos;
        }
    }
    body.pos
}

#[test]
fn bodies_can_walk_up_to_high_ground() {
    let spots: &[(MapId, [f32; 3])] = &[
        (MapId::Wreck, [-46.0, 12.6, -30.0]),
        (MapId::Wreck, [44.0, 9.2, 28.0]),
        (MapId::Wreck, [43.25, 4.6, -22.0]),
        (MapId::Wreck, [-46.25, 4.6, 11.75]),
        (MapId::Site, [0.0, 3.4, 7.7]),
        (MapId::Site, [-4.0, 6.8, -3.5]),
        (MapId::Site, [-22.3, 3.4, -16.0]),
        (MapId::Site, [-29.0, 6.8, -16.0]),
        (MapId::Market, [27.5, 6.8, -11.25]),
        (MapId::Market, [-27.5, 6.8, -11.25]),
        (MapId::Market, [43.0, 3.4, 5.8]),
        // up ladders
        (MapId::Market, [27.0, 4.55, 11.25]),
        (MapId::Market, [-41.0, 4.55, -11.25]),
        (MapId::Market, [-29.0, 4.35, 11.25]),
        (MapId::Market, [28.5, 6.35, 27.5]),
    ];
    for &(id, p) in spots {
        let m = toon_shared::maps::build(id);
        let w = World::from_map(&m);
        let nav = Nav::build(&m, &w);
        let target = glam::Vec3::from_array(p);
        let end = walk(&m, &w, &nav, m.spawns[0][0].pos, target);
        assert!(
            (end.y - p[1]).abs() < 0.3
                && glam::vec3(end.x - p[0], 0.0, end.z - p[2]).length() < 1.5,
            "{id:?}: walker got stuck at {end:?} on the way to {target:?}"
        );
    }
}

#[test]
fn bodies_climb_down_ladders_too() {
    let m = toon_shared::maps::build(MapId::Market);
    let w = World::from_map(&m);
    let nav = Nav::build(&m, &w);
    // the shop roof is 4.55 m up: too high to jump, so the way down is the ladder
    let from = glam::vec3(27.0, 4.56, 11.25);
    let to = glam::vec3(27.0, 0.0, 0.0);
    let end = walk(&m, &w, &nav, from, to);
    assert!(
        end.y < 0.3 && glam::vec3(end.x - to.x, 0.0, end.z - to.z).length() < 1.5,
        "walker got stuck at {end:?}"
    );
}

/// Market's two sides are built differently but must play the same: from each team's
/// yard the walks to the bomb, to its own site, to the enemy site and up to its high
/// house are about as long.
#[test]
fn market_sides_are_equally_far() {
    let m = toon_shared::maps::build(MapId::Market);
    let w = World::from_map(&m);
    let nav = Nav::build(&m, &w);
    let len = |a: glam::Vec3, b: glam::Vec3| -> f32 {
        let path = nav.path(a, b).expect("path");
        let mut l = 0.0;
        let mut p = a;
        for q in path {
            l += p.distance(q);
            p = q;
        }
        l
    };
    // allies (0) live in the west and defend sites[0]; axis (1) the east
    let high = [
        glam::vec3(-27.5, 6.8, -11.25),
        glam::vec3(27.5, 6.8, -11.25),
    ];
    let mut rows = Vec::new();
    for team in 0..2 {
        let mut sum = [0.0f32; 4];
        for s in &m.spawns[team] {
            sum[0] += len(s.pos, m.bomb);
            sum[1] += len(s.pos, m.sites[team]);
            sum[2] += len(s.pos, m.sites[1 - team]);
            sum[3] += len(s.pos, high[team]);
        }
        let n = m.spawns[team].len() as f32;
        rows.push(sum.map(|x| x / n));
    }
    println!("west {:?}\neast {:?}", rows[0], rows[1]);
    for (k, what) in ["bomb", "own site", "enemy site", "own high house"]
        .iter()
        .enumerate()
    {
        let (a, b) = (rows[0][k], rows[1][k]);
        assert!(
            (a - b).abs() / a.max(b) < 0.08,
            "{what}: west walks {a:.0} m, east {b:.0} m"
        );
    }
}

/// Wreck's terrain is the same for both teams (turned round), the buildings are not: the
/// walks from both ends must still be about as long.
#[test]
fn wreck_sides_are_equally_far() {
    let m = toon_shared::maps::build(MapId::Wreck);
    let w = World::from_map(&m);
    let nav = Nav::build(&m, &w);
    let len = |a: glam::Vec3, b: glam::Vec3| -> f32 {
        let path = nav.path(a, b).expect("path");
        let mut l = 0.0;
        let mut p = a;
        for q in path {
            l += p.distance(q);
            p = q;
        }
        l
    };
    let mut rows = Vec::new();
    for team in 0..2 {
        let mut sum = [0.0f32; 3];
        for s in &m.spawns[team] {
            sum[0] += len(s.pos, m.bomb);
            sum[1] += len(s.pos, m.sites[team]);
            sum[2] += len(s.pos, m.sites[1 - team]);
        }
        let n = m.spawns[team].len() as f32;
        rows.push(sum.map(|x| x / n));
    }
    println!("allies {:?}\naxis {:?}", rows[0], rows[1]);
    for (k, what) in ["bomb", "own site", "enemy site"].iter().enumerate() {
        let (a, b) = (rows[0][k], rows[1][k]);
        assert!(
            (a - b).abs() / a.max(b) < 0.08,
            "{what}: allies walk {a:.0} m, axis {b:.0} m"
        );
    }
}

/// Every ladder on Market can be climbed up and down by a body following the bots'
/// routes, also while it looks the other way (a bot fighting on the ladder).
#[test]
fn every_ladder_works_whichever_way_you_look() {
    let m = toon_shared::maps::build(MapId::Market);
    let w = World::from_map(&m);
    let nav = Nav::build(&m, &w);
    assert!(!m.ladders.is_empty());
    for (i, l) in m.ladders.iter().enumerate() {
        let foot = l.base + l.out * 1.2;
        let top = l.exit();
        for look in [None, Some(0.0), Some(2.0), Some(-2.4)] {
            let end = walk_looking(&m, &w, &nav, foot, top, look);
            assert!(
                (end.y - top.y).abs() < 0.3
                    && glam::vec3(end.x - top.x, 0.0, end.z - top.z).length() < 1.5,
                "ladder {i} at {}: going up looking {look:?} stuck at {end}",
                l.base
            );
            let end = walk_looking(&m, &w, &nav, top + glam::vec3(0.0, 0.01, 0.0), foot, look);
            assert!(
                end.y < l.base.y + 0.3
                    && glam::vec3(end.x - foot.x, 0.0, end.z - foot.z).length() < 1.5,
                "ladder {i} at {}: going down looking {look:?} stuck at {end}",
                l.base
            );
        }
    }
}
