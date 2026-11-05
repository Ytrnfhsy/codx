//! A whole battle royale round with bots on the island: everybody boards the plane and
//! jumps, lands, finds guns, the zone closes in, and exactly one player wins.

use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::physics::Chute;
use toon_shared::protocol::{MatchState, C2S, S2C};
use toon_shared::weapons::{WeaponClass, DEFAULT_CLASSES};
use toon_shared::{Mode, Team};

#[test]
fn a_round_of_battle_royale() {
    let cfg = ServerConfig {
        bots: 16,
        max_players: 16,
        data_dir: std::env::temp_dir().join("tw-br").to_string_lossy().into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Br);
    let me = g
        .connect(
            "127.0.0.1:5002".parse().unwrap(),
            "Гравець",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    g.handle(me, C2S::SetLoadout(DEFAULT_CLASSES[0].1));

    let mut step = |g: &mut Game| {
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
    };
    // the plane takes off
    for _ in 0..(60.0 / TICK_DT) as usize {
        step(&mut g);
        if g.br.running {
            break;
        }
    }
    assert!(g.br.running, "the round did not start ({:?})", g.state);
    assert_eq!(g.br.entrants, 16);
    assert!(g.players.iter().flatten().all(|p| p.in_plane && !p.alive));
    assert!(!g.pickups.is_empty() && !g.br.loot.is_empty(), "no loot");
    assert!(g
        .players
        .iter()
        .flatten()
        .all(|p| p.team == Team::Allies || !p.team.is_playing()));

    // we jump right away: free fall, then the parachute, then the ground
    g.handle(me, C2S::BrJump);
    let p = g.p(me).unwrap();
    assert!(p.alive && !p.in_plane && p.body.chute == Chute::FreeFall);
    assert!(p.body.pos.y > 150.0);

    // a minute later every bot has landed and most found a real gun
    for _ in 0..(60.0 / TICK_DT) as usize {
        step(&mut g);
    }
    let bots: Vec<_> = g.players.iter().flatten().filter(|p| p.is_bot()).collect();
    assert!(bots.iter().all(|p| !p.in_plane), "bots still in the plane");
    let landed = bots
        .iter()
        .filter(|p| !p.alive || p.body.chute == Chute::None)
        .count();
    assert!(landed >= bots.len() - 1, "only {landed} bots landed");
    let alive_bots = bots.iter().filter(|p| p.alive).count();
    let armed = bots
        .iter()
        .filter(|p| p.alive)
        .filter(|p| {
            p.weapons
                .iter()
                .any(|w| w.id.def().class != WeaponClass::Pistol && w.mag + w.reserve > 0)
        })
        .count();
    eprintln!(
        "after a minute: {landed} landed, {armed} found a gun, {} alive",
        g.br_alive()
    );
    assert!(
        armed * 2 >= alive_bots,
        "bots do not pick up guns ({armed} of {alive_bots})"
    );

    // play it out: the zone ends it within ~8 minutes
    let mut winner = None;
    for _ in 0..(12.0 * 60.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        for (_, msg, _) in g.outbox.drain(..) {
            if let S2C::MatchEnd { text, .. } = msg {
                winner = Some(text);
            }
        }
        g.shots.clear();
        if g.state == MatchState::PostMatch {
            break;
        }
    }
    eprintln!("result: {winner:?} at zone phase {}", g.br.zone.phase);
    assert_eq!(g.state, MatchState::PostMatch, "the round never ended");
    assert!(winner.is_some());
    assert!(g.br_alive() <= 1);
}
