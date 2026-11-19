//! Enemies a player cannot see are left out of their snapshots (nothing for a wallhack to
//! show); teammates and enemies in sight are in them.

use glam::vec3;
use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::TICK_DT;
use toon_shared::protocol::C2S;
use toon_shared::{Mode, Team};

fn join(g: &mut Game, port: u16, name: &str, team: Team) -> u8 {
    let id = g
        .connect(
            format!("127.0.0.1:{port}").parse().unwrap(),
            name,
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(id, C2S::JoinTeam(Some(team)));
    id
}

fn place(g: &mut Game, id: u8, x: f32, z: f32) {
    let p = g.pm(id).unwrap();
    p.alive = true;
    p.body.pos = vec3(x, 0.0, z);
    p.body.vel = glam::Vec3::ZERO;
    p.last_shot = -100.0;
}

fn sees(g: &Game, viewer: u8, other: u8) -> bool {
    g.snapshot_for(viewer).players.iter().any(|p| p.id == other)
}

#[test]
fn enemies_behind_walls_are_not_sent() {
    let cfg = ServerConfig {
        ready_check: false,
        bots: 0,
        data_dir: std::env::temp_dir().join("tw-vis").to_string_lossy().into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    g.play_on(toon_shared::maps::test_yard(), Mode::Tdm);
    let me = join(&mut g, 5601, "Я", Team::Allies);
    let foe = join(&mut g, 5602, "Ворог", Team::Axis);
    let mate = join(&mut g, 5603, "Друг", Team::Allies);
    for _ in 0..10 {
        g.tick(TICK_DT);
    }
    let step = |g: &mut Game| {
        for _ in 0..8 {
            g.tick(TICK_DT);
        }
    };

    // the red container (-6, -7.5, along x, 2.6 m tall) between us
    place(&mut g, me, -6.0, -12.0);
    place(&mut g, foe, -6.0, -3.0);
    place(&mut g, mate, -6.0, -2.0);
    step(&mut g);
    // (and past the moment after losing sight in which they are still sent)
    for _ in 0..30 {
        g.tick(TICK_DT);
        place(&mut g, foe, -6.0, -3.0);
    }
    assert!(!sees(&g, me, foe), "an enemy behind the container was sent");
    assert!(sees(&g, me, mate), "a teammate is always sent");

    // out in the open, on the same line
    place(&mut g, foe, 6.0, -12.0);
    step(&mut g);
    assert!(sees(&g, me, foe), "an enemy in plain sight was not sent");

    // back behind it: gone again a moment later
    for _ in 0..30 {
        place(&mut g, foe, -6.0, -3.0);
        g.tick(TICK_DT);
    }
    assert!(
        !sees(&g, me, foe),
        "the enemy stayed after going behind cover"
    );

    // running close by behind cover: heard, so sent (the client plays the footsteps)
    {
        let p = g.pm(foe).unwrap();
        p.body.vel = vec3(7.0, 0.0, 0.0);
    }
    for _ in 0..4 {
        g.pm(foe).unwrap().body.vel = vec3(7.0, 0.0, 0.0);
        g.tick(TICK_DT);
    }
    g.pm(foe).unwrap().body.vel = vec3(7.0, 0.0, 0.0);
    assert!(sees(&g, me, foe), "a running enemy close by was not sent");
}
