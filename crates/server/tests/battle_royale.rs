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
    let mut cfg = ServerConfig {
        data_dir: std::env::temp_dir().join("tw-br").to_string_lossy().into(),
        ..Default::default()
    };
    cfg.br.bots = 16;
    cfg.br.max_players = 16;
    let mut g = Game::new(cfg);
    // the same plane course, loot and bot decisions every run
    g.rng = toon_shared::rng::Rng::new(77);
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
        armed * 5 >= alive_bots * 2,
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

/// The battle royale's player limit and bots are its own: after a round of it the team
/// modes in the rotation go back to theirs.
#[test]
fn battle_royale_keeps_its_own_player_count() {
    let mut cfg = ServerConfig {
        bots: 6,
        max_players: 12,
        data_dir: std::env::temp_dir().join("tw-br2").to_string_lossy().into(),
        ..Default::default()
    };
    cfg.br.bots = 30;
    cfg.br.max_players = 30;
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Br);
    let me = g
        .connect(
            "127.0.0.1:5004".parse().unwrap(),
            "Гравець",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    let run = |g: &mut Game, secs: f32| {
        for _ in 0..(secs / TICK_DT) as usize {
            g.tick(TICK_DT);
            g.outbox.clear();
            g.shots.clear();
        }
    };
    run(&mut g, 10.0);
    assert_eq!(
        g.players.iter().flatten().count(),
        30,
        "battle royale not filled"
    );
    g.change_map(MapId::Depot, Mode::Tdm);
    run(&mut g, 10.0);
    let n = g.players.iter().flatten().count();
    assert!(n <= 12, "{n} players left in team deathmatch (limit 12)");
    assert_eq!(n, 6, "team deathmatch should fill to its own 6");
}
