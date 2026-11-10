//! Fifty players on the 1 km island: the server keeps up with its tick rate and every
//! snapshot stays small, because players are told only about what is near them.

use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::{INTEREST_RANGE, TICK_DT};
use toon_shared::map::MapId;
use toon_shared::protocol::{C2S, S2C};
use toon_shared::weapons::DEFAULT_CLASSES;
use toon_shared::{Mode, Team};

#[test]
fn fifty_players_on_the_island() {
    let cfg = ServerConfig {
        // (tests start straight away: nobody presses «Ready»)
        ready_check: false,
        bots: 50,
        max_players: 50,
        data_dir: std::env::temp_dir().join("tw-big").to_string_lossy().into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Tdm);
    let me = g
        .connect(
            "127.0.0.1:5001".parse().unwrap(),
            "Глядач",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(Some(Team::Allies)));
    g.handle(me, C2S::SetLoadout(DEFAULT_CLASSES[0].1));
    for _ in 0..(8.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.outbox.clear();
    }
    let n = g.players.iter().flatten().count();
    assert_eq!(n, 50, "bots did not fill the server");

    let secs: f32 = std::env::var("BIG_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(30.0);
    let ticks = (secs / TICK_DT) as usize;
    let start: Vec<(u8, glam::Vec3)> = g
        .players
        .iter()
        .flatten()
        .map(|p| (p.id, p.body.pos))
        .collect();
    let t = std::time::Instant::now();
    let mut worst = std::time::Duration::ZERO;
    for _ in 0..ticks {
        let t0 = std::time::Instant::now();
        g.tick(TICK_DT);
        let e = t0.elapsed();
        if e.as_millis() > 10 {
            eprintln!("slow tick at {:.2} s: {e:?}", g.time);
        }
        worst = worst.max(e);
        g.outbox.clear();
        g.shots.clear();
    }
    let avg = t.elapsed() / ticks as u32;
    let moved = start
        .iter()
        .filter(|(id, p0)| {
            g.p(*id)
                .is_some_and(|p| p.is_bot() && p.body.pos.distance(*p0) > 15.0)
        })
        .count();
    eprintln!("{moved} bots moved more than 15 m; state {:?}", g.state);
    assert!(moved > 25, "bots stand still");
    eprintln!("50 players: {avg:?} per tick on average, worst {worst:?}");
    // a tick lasts 33 ms; leave most of it free (debug builds are much slower)
    if !cfg!(debug_assertions) {
        assert!(avg.as_secs_f32() < 0.010, "{avg:?} per tick");
    }

    // a snapshot names only the players near us
    let snap = g.snapshot_for(me);
    let pos = g.p(me).unwrap().body.pos;
    for p in &snap.players {
        let q = glam::Vec3::from_array(p.pos);
        let d = glam::vec2(q.x - pos.x, q.z - pos.z).length();
        let mate = p.team == Team::Allies;
        assert!(
            mate || d < INTEREST_RANGE,
            "far enemy {d} m away in the snapshot"
        );
    }
    let bytes = bincode::serialize(&S2C::Snapshot(Box::new(snap)))
        .unwrap()
        .len();
    eprintln!("snapshot: {bytes} bytes");
    assert!(bytes < 4000);
}
