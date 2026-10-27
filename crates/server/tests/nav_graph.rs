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
        let yaw = toon_shared::yaw_of(d);
        let inp = MoveInput {
            forward: 1.0,
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
        (MapId::Wreck, [-30.0, 10.2, -8.0]),
        (MapId::Wreck, [21.2, 3.4, -10.0]),
        (MapId::Wreck, [30.0, 3.6, 7.5]),
        (MapId::Site, [0.0, 3.4, 7.7]),
        (MapId::Site, [-4.0, 6.8, -3.5]),
        (MapId::Site, [-22.3, 3.4, -16.0]),
        (MapId::Site, [-29.0, 6.8, -16.0]),
        (MapId::Market, [-30.0, 3.4, -5.25]),
        (MapId::Market, [-18.0, 3.4, 5.25]),
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
