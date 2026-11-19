//! The squad lobby (who lands with whom) and pings (pointing things out to our side).

use glam::vec3;
use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::protocol::{MarkAt, PartyCmd, PingWhat, C2S};
use toon_shared::{Mode, Team};

fn join(g: &mut Game, port: u16, name: &str, team: Option<Team>) -> u8 {
    let id = g
        .connect(
            format!("127.0.0.1:{port}").parse().unwrap(),
            name,
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(id, C2S::JoinTeam(team));
    id
}

#[test]
fn friends_from_the_lobby_land_in_one_squad() {
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: std::env::temp_dir()
            .join("tw-lobby")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    cfg.br.squad = 2;
    cfg.br.bots = 3;
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Br);
    // they came in this order: by the order alone 0+1 and 2+3 would be together
    let ids: Vec<u8> = (0..4)
        .map(|k| join(&mut g, 5700 + k, &format!("Гравець{k}"), None))
        .collect();
    let lobby = g
        .lobby_for(ids[0])
        .expect("the lobby is open before the match");
    assert_eq!(lobby.size, 2);
    // 0 asks 3, 3 says yes; 1 asks 2, 2 says no
    g.handle(ids[0], C2S::Party(PartyCmd::Ask(ids[3])));
    assert_eq!(g.lobby_for(ids[3]).unwrap().asking_me, vec![ids[0]]);
    g.handle(ids[3], C2S::Party(PartyCmd::Accept(ids[0])));
    g.handle(ids[1], C2S::Party(PartyCmd::Ask(ids[2])));
    g.handle(ids[2], C2S::Party(PartyCmd::Decline(ids[1])));
    let l = g.lobby_for(ids[1]).unwrap();
    assert!(l.i_asked.is_empty(), "a declined ask stays: {l:?}");
    assert!(l.parties.contains(&vec![ids[0], ids[3]]), "{:?}", l.parties);
    // a third one cannot squeeze into a full duo
    g.handle(ids[1], C2S::Party(PartyCmd::Ask(ids[0])));
    g.handle(ids[0], C2S::Party(PartyCmd::Accept(ids[1])));
    let party = |g: &Game, k: usize| g.players[ids[k] as usize].as_ref().unwrap().party;
    assert_eq!(party(&g, 1), 0, "the duo took a third");
    // the round
    for _ in 0..(12.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        if g.br.running {
            break;
        }
    }
    assert!(g.br.running, "the round did not start");
    let squad = |k: usize| g.players[ids[k] as usize].as_ref().unwrap().squad;
    assert!(
        squad(0) != 0 && squad(0) == squad(3),
        "the friends were split"
    );
    assert_ne!(squad(0), squad(1));
    // the two left over play together (the gaps are filled by players first)
    assert!(
        squad(1) != 0 && squad(1) == squad(2),
        "{} {}",
        squad(1),
        squad(2)
    );
    // the lobby closes once the match is on, the squad is kept for the next one
    assert!(g.lobby_for(ids[0]).is_none());
    assert_ne!(party(&g, 0), 0);
}

#[test]
fn pings_go_to_our_side_only() {
    let cfg = ServerConfig {
        ready_check: false,
        bots: 0,
        data_dir: std::env::temp_dir()
            .join("tw-pings")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    g.play_on(toon_shared::maps::test_yard(), Mode::Tdm);
    let me = join(&mut g, 5711, "Я", Some(Team::Allies));
    let mate = join(&mut g, 5712, "Друг", Some(Team::Allies));
    let foe = join(&mut g, 5713, "Ворог", Some(Team::Axis));
    for _ in 0..10 {
        g.tick(TICK_DT);
    }
    for (id, x, z, team) in [
        (me, 6.0, -12.0, Team::Allies),
        (mate, 8.0, -12.0, Team::Allies),
        (foe, 6.0, 0.0, Team::Axis),
    ] {
        let p = g.pm(id).unwrap();
        p.team = team;
        p.alive = true;
        p.body.pos = vec3(x, 0.0, z);
        p.last_shot = -100.0;
    }
    for _ in 0..8 {
        g.tick(TICK_DT);
    }
    g.handle(
        me,
        C2S::Mark(MarkAt::Spot {
            pos: [0.0, 0.0, -5.0],
            danger: false,
        }),
    );
    g.tick(TICK_DT);
    let has = |g: &Game, id: u8, w: PingWhat| g.snapshot_for(id).pings.iter().any(|p| p.what == w);
    assert!(has(&g, me, PingWhat::Spot) && has(&g, mate, PingWhat::Spot));
    assert!(!has(&g, foe, PingWhat::Spot), "the enemy saw our ping");
    // the enemy in plain sight
    for _ in 0..8 {
        g.tick(TICK_DT);
    }
    g.handle(me, C2S::Mark(MarkAt::Enemy(foe)));
    g.tick(TICK_DT);
    assert!(
        has(&g, mate, PingWhat::Enemy),
        "the spotted enemy was not shown"
    );
    // a place far off the map, or nonsense, is not a ping
    for _ in 0..8 {
        g.tick(TICK_DT);
    }
    g.handle(
        me,
        C2S::Mark(MarkAt::Spot {
            pos: [f32::NAN, 0.0, 0.0],
            danger: false,
        }),
    );
    g.handle(me, C2S::Mark(MarkAt::Thing(60000)));
    assert!(g.pings.len() <= 2, "{:?}", g.pings);
    // a ping runs out
    for _ in 0..(30.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
    }
    assert!(!has(&g, mate, PingWhat::Spot), "the ping stayed for good");
}
