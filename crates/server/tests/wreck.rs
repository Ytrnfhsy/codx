//! Blasts break the map: the server's map and every client's break the same way, someone
//! who joins later gets the whole list, a new round mends it.

use glam::vec3;
use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::destruct::Wreck;
use toon_shared::map::MapId;
use toon_shared::physics::World;
use toon_shared::protocol::{C2S, S2C};
use toon_shared::weapons::Cause;
use toon_shared::Mode;

fn blasts_sent(g: &mut Game) -> Vec<S2C> {
    std::mem::take(&mut g.outbox)
        .into_iter()
        .map(|(_, m, _)| m)
        .filter(|m| matches!(m, S2C::Blast(_) | S2C::Blasts(_)))
        .collect()
}

#[test]
fn a_blast_breaks_the_same_on_server_and_client() {
    let cfg = ServerConfig {
        data_dir: std::env::temp_dir()
            .join("tw-wreck")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    g.change_map(MapId::Market, Mode::Tdm);
    let whole = g.map.boxes.len();
    // the client's copy, built on its own
    let mut map = toon_shared::maps::build(MapId::Market);
    let mut world = World::from_map(&map);
    let mut wreck = Wreck::new(&map);
    // blasts by walls all over the map
    let spots: Vec<_> = g
        .map
        .boxes
        .iter()
        .filter(|b| b.solid && b.visible && b.min.y >= 0.0 && b.max.y > 2.0)
        .step_by(23)
        .take(25)
        .map(|b| vec3(b.min.x, 1.0, b.min.z))
        .collect();
    let n = spots.len();
    assert!(n > 15);
    blasts_sent(&mut g);
    for p in spots {
        g.explode(p, 7.0, 50.0, None, Cause::Grenade, true);
    }
    let sent = blasts_sent(&mut g);
    assert_eq!(sent.len(), n);
    for m in sent {
        if let S2C::Blast(b) = m {
            wreck.blast(&mut map, &mut world, b);
        }
    }
    assert!(g.map.boxes.len() > whole, "nothing broke");
    let shape = |m: &toon_shared::map::Map| {
        m.boxes
            .iter()
            .map(|b| (b.min.to_array(), b.max.to_array(), b.solid, b.visible))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        shape(&g.map),
        shape(&map),
        "the client's map broke differently"
    );

    // somebody joining now hears every blast so far
    let id = g
        .connect(
            "127.0.0.1:5990".parse().unwrap(),
            "Пізній",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(id, C2S::JoinTeam(None));
    let late = blasts_sent(&mut g);
    assert!(late
        .iter()
        .any(|m| matches!(m, S2C::Blasts(l) if l.len() == n)));

    // a new match: whole again, and everyone told
    g.start_match();
    assert_eq!(g.map.boxes.len(), whole);
    assert!(blasts_sent(&mut g)
        .iter()
        .any(|m| matches!(m, S2C::Blasts(l) if l.is_empty())));
}
