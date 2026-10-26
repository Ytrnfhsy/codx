//! Headless match simulations: bots must actually fight, plant and win.

use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::protocol::{MatchState, C2S, S2C};
use toon_shared::weapons::DEFAULT_CLASSES;
use toon_shared::{Mode, Team};

fn setup(map: MapId, mode: Mode) -> (Game, u8) {
    let mut cfg = ServerConfig::default();
    cfg.bots = 8;
    cfg.data_dir = std::env::temp_dir().join("tw-test").to_string_lossy().into();
    let mut g = Game::new(cfg);
    g.change_map(map, mode);
    // a passive human spectator-ish player on a team so the match starts
    let id = g.connect("127.0.0.1:5000".parse().unwrap(), "Тестер", "", toon_shared::consts::PROTOCOL_VERSION).unwrap();
    g.handle(id, C2S::JoinTeam(Some(Team::Allies)));
    g.handle(id, C2S::SetLoadout(DEFAULT_CLASSES[0].1));
    (g, id)
}

struct Stats {
    kills: usize,
    plants: usize,
    round_ends: usize,
    match_end: bool,
}

fn run(g: &mut Game, secs: f32) -> Stats {
    let mut st = Stats { kills: 0, plants: 0, round_ends: 0, match_end: false };
    let ticks = (secs / TICK_DT) as usize;
    for _ in 0..ticks {
        g.tick(TICK_DT);
        // the network layer would do this
        let _ = g.snapshot_for(0);
        g.shots.clear();
        for (_, m, _) in g.outbox.drain(..) {
            match m {
                S2C::Kill { .. } => st.kills += 1,
                S2C::BombEvent { text } if text.contains("заклав") => st.plants += 1,
                S2C::RoundEnd { .. } => st.round_ends += 1,
                S2C::MatchEnd { .. } => st.match_end = true,
                _ => {}
            }
        }
    }
    st
}

#[test]
fn tdm_bots_fight() {
    let (mut g, _) = setup(MapId::Depot, Mode::Tdm);
    let st = run(&mut g, 150.0);
    assert!(g.players.iter().flatten().filter(|p| p.is_bot()).count() >= 7, "bots should fill the server");
    assert_ne!(g.state, MatchState::Warmup, "match should have started");
    assert!(st.kills >= 8, "only {} kills in 150s", st.kills);
    assert!(g.scores[0] + g.scores[1] > 0);
}

#[test]
fn sab_bots_play_objective() {
    let (mut g, _) = setup(MapId::Cargo, Mode::Sab);
    let st = run(&mut g, 420.0);
    println!("kills={} plants={} rounds={} end={} scores={:?}", st.kills, st.plants, st.round_ends, st.match_end, g.scores);
    assert!(st.plants >= 1, "bots never planted the bomb");
    assert!(st.round_ends + st.match_end as usize >= 1);
}

#[test]
fn market_map_runs() {
    let (mut g, _) = setup(MapId::Market, Mode::Tdm);
    let st = run(&mut g, 120.0);
    assert!(st.kills >= 4, "only {} kills", st.kills);
}

#[test]
fn admin_commands() {
    let (mut g, _) = setup(MapId::Depot, Mode::Tdm);
    run(&mut g, 2.0);
    let out = toon_server::admin::exec(&mut g, "status");
    assert!(out.contains("Тестер"));
    let out = toon_server::admin::exec(&mut g, "map cargo sab");
    assert!(out.contains("Вантаж"), "{out}");
    assert_eq!(g.mode, Mode::Sab);
    let out = toon_server::admin::exec(&mut g, "set scorelimit 10");
    assert_eq!(out, "scorelimit = 10");
    assert_eq!(g.cfg.tdm.score_limit, 10);
    toon_server::admin::exec(&mut g, "kick Тестер погано");
    assert_eq!(g.kicked.len(), 1);
    assert!(toon_server::admin::exec(&mut g, "foo").contains("Невідома"));
}
