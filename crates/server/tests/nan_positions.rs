//! A body whose position went bad (NaN) must not bring the server down where it picks
//! the nearest of several: a knife's victim, a gunship's or a car's target.

use glam::Vec3;
use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::{MAX_HEALTH, TICK_DT};
use toon_shared::look_dir;
use toon_shared::protocol::{MatchState, C2S};
use toon_shared::weapons::DEFAULT_CLASSES;
use toon_shared::{Mode, Team};

#[test]
fn a_knife_ignores_a_body_with_a_broken_position() {
    let cfg = ServerConfig {
        ready_check: false,
        bots: 0,
        data_dir: std::env::temp_dir()
            .join("tw-test-nan")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    g.play_on(toon_shared::maps::test_yard(), Mode::Tdm);
    let mut ids = Vec::new();
    for (k, team) in [Team::Allies, Team::Axis, Team::Axis]
        .into_iter()
        .enumerate()
    {
        let id = g
            .connect(
                format!("127.0.0.1:{}", 6100 + k).parse().unwrap(),
                &format!("P{k}"),
                "",
                toon_shared::consts::PROTOCOL_VERSION,
            )
            .unwrap();
        g.handle(id, C2S::JoinTeam(Some(team)));
        g.handle(id, C2S::SetLoadout(DEFAULT_CLASSES[0].1));
        ids.push(id);
    }
    for _ in 0..1200 {
        g.tick(TICK_DT);
        if g.state == MatchState::Playing {
            break;
        }
    }
    assert_eq!(g.state, MatchState::Playing);
    let (me, foe, broken) = (ids[0], ids[1], ids[2]);
    let at = Vec3::new(-18.0, 0.0, 0.0);
    for (id, pos) in [
        (me, at),
        (foe, at + look_dir(0.0, 0.0) * 1.2),
        (broken, Vec3::NAN),
    ] {
        let p = g.pm(id).unwrap();
        p.alive = true;
        p.hp = MAX_HEALTH;
        p.protected_until = 0.0;
        p.body.pos = pos;
        p.yaw = 0.0;
        p.pitch = 0.0;
    }
    g.handle(me, C2S::Melee);
    assert!(
        g.p(foe).unwrap().hp < MAX_HEALTH || !g.p(foe).unwrap().alive,
        "the one in front was not stabbed"
    );
    assert!(
        g.p(broken).unwrap().hp >= MAX_HEALTH,
        "a NaN body was stabbed"
    );
}
