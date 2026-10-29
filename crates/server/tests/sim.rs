//! Headless match simulations: bots must actually fight, plant and win.

use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::protocol::{MatchState, C2S, S2C};
use toon_shared::weapons::DEFAULT_CLASSES;
use toon_shared::{Mode, Team};

fn setup(map: MapId, mode: Mode) -> (Game, u8) {
    let cfg = ServerConfig {
        bots: 8,
        data_dir: std::env::temp_dir()
            .join("tw-test")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    // deterministic bots (names, skill, decisions) so the simulation is reproducible
    g.rng = toon_shared::rng::Rng::new(1234);
    g.change_map(map, mode);
    // a passive human spectator-ish player on a team so the match starts
    let id = g
        .connect(
            "127.0.0.1:5000".parse().unwrap(),
            "Тестер",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
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
    let mut st = Stats {
        kills: 0,
        plants: 0,
        round_ends: 0,
        match_end: false,
    };
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
    assert!(
        g.players.iter().flatten().filter(|p| p.is_bot()).count() >= 7,
        "bots should fill the server"
    );
    assert_ne!(g.state, MatchState::Warmup, "match should have started");
    assert!(st.kills >= 8, "only {} kills in 150s", st.kills);
    // bots that joined mid-match must spawn too
    for p in g.players.iter().flatten().filter(|p| p.is_bot()) {
        assert!(p.alive || p.deaths > 0, "{} never spawned", p.name);
    }
    assert!(g.scores[0] + g.scores[1] > 0);
}

#[test]
fn sab_bots_play_objective() {
    let (mut g, _) = setup(MapId::Depot, Mode::Sab);
    let st = run(&mut g, 900.0);
    println!(
        "kills={} plants={} rounds={} end={} scores={:?}",
        st.kills, st.plants, st.round_ends, st.match_end, g.scores
    );
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

#[test]
fn admin_tuning_applies_live_and_reaches_clients() {
    let (mut g, _) = setup(MapId::Depot, Mode::Tdm);
    g.outbox.clear();
    let out = toon_server::admin::exec(&mut g, "tune m249.rpm 900");
    assert_eq!(out, "weapons.m249.rpm = 900", "{out}");
    assert_eq!(
        g.tuning.weapon(toon_shared::weapons::WeaponId::M249).rpm,
        900.0
    );
    assert!(g
        .outbox
        .iter()
        .any(|(_, m, _)| matches!(m, S2C::Tuning(t) if t.weapons["m249"].rpm == 900.0)));
    let out = toon_server::admin::exec(&mut g, "tune movement.sprint_speed 999");
    assert!(out.contains("обмежено"), "{out}");
    assert_eq!(g.tuning.movement.sprint_speed, 25.0);
    let out = toon_server::admin::exec(&mut g, "tune sprint_speed 8.5");
    assert_eq!(out, "movement.sprint_speed = 8.5");
    let out = toon_server::admin::exec(&mut g, "tune w1200.auto true");
    assert!(out.ends_with("true"), "{out}");
    assert!(toon_server::admin::exec(&mut g, "tune m249").contains("Вага"));
    assert!(toon_server::admin::exec(&mut g, "tune m249.nope 1").contains("Невідомий"));
    toon_server::admin::exec(&mut g, "tune reset all");
    assert_eq!(
        g.tuning.weapon(toon_shared::weapons::WeaponId::M249).rpm,
        750.0
    );
    // persisted into the config section that `save` writes
    assert_eq!(g.cfg.tuning["weapons"]["m249"]["rpm"], 750.0);
}

/// Throws one grenade straight ahead (with the client's arc) and returns where it came to rest.
fn grenade_landing(g: &mut Game, id: u8) -> f32 {
    let start = g.p(id).unwrap().body.pos;
    let eye = g.p(id).unwrap().eye();
    let arc = g.tuning.grenade.throw_arc.to_radians();
    let dir = glam::vec3(arc.cos(), arc.sin(), 0.0);
    g.grenades.clear();
    if let Some(p) = g.pm(id) {
        p.grenades = 1;
        p.protected_until = 0.0;
    }
    g.throw_grenade(id, eye, dir, 0.0, false);
    assert_eq!(g.grenades.len(), 1, "grenade was not thrown");
    let mut last = start;
    while let Some(gr) = g.grenades.first() {
        last = gr.pos;
        g.tick(TICK_DT);
        g.outbox.clear();
    }
    (last - start).length()
}

#[test]
fn grenade_throw_range_is_tunable() {
    let (mut g, id) = setup(MapId::Depot, Mode::Tdm);
    toon_server::admin::exec(&mut g, "bots 0");
    run(&mut g, 1.0);
    // stand at the west end of the road (z = 0) and throw east along it: ~38 m of open ground
    let place = |g: &mut Game| {
        if let Some(p) = g.pm(id) {
            p.body.pos = glam::vec3(-18.0, 0.0, 0.0);
            p.alive = true;
        }
    };
    place(&mut g);
    let normal = grenade_landing(&mut g, id);
    toon_server::admin::exec(&mut g, "tune grenade.throw_speed 26");
    place(&mut g);
    let far = grenade_landing(&mut g, id);
    toon_server::admin::exec(&mut g, "tune grenade.throw_speed 9");
    place(&mut g);
    let near = grenade_landing(&mut g, id);
    println!("grenade lands at: speed 9 → {near:.1} m, 17 → {normal:.1} m, 26 → {far:.1} m");
    assert!(near < normal && normal < far, "{near} {normal} {far}");
    assert!(toon_server::admin::exec(&mut g, "tune grenade").contains("дальність"));
}

#[test]
fn dropped_weapons_can_be_picked_up_and_scavenged() {
    use toon_shared::weapons::{Cause, WeaponId};
    let (mut g, me) = setup(MapId::Depot, Mode::Tdm);
    toon_server::admin::exec(&mut g, "bots 0");
    let other = g
        .connect(
            "127.0.0.1:5001".parse().unwrap(),
            "Другий",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(other, C2S::JoinTeam(Some(Team::Axis)));
    g.handle(other, C2S::SetLoadout(DEFAULT_CLASSES[0].1));
    run(&mut g, 1.0);
    let spot = glam::vec3(-18.0, 0.0, 0.0);
    for (id, x) in [(me, 0.0), (other, 5.0)] {
        let p = g.pm(id).unwrap();
        p.alive = true;
        p.body.pos = spot + glam::vec3(x, 0.0, 0.0);
    }
    // the other player carries an LMG and dies: it lands on the floor
    g.pm(other).unwrap().weapons[0].id = WeaponId::M249;
    g.pm(other).unwrap().slot = 0;
    g.kill(other, Some(me), Cause::Weapon(WeaponId::M4), false);
    assert_eq!(g.pickups.len(), 1);
    let pk = g.pickups[0];
    assert_eq!(pk.w.id, WeaponId::M249);
    assert!(g.snapshot_for(me).ents.iter().any(|e| matches!(
        e,
        toon_shared::protocol::EntSnap::Weapon {
            weapon: WeaponId::M249,
            ..
        }
    )));

    // too far away: nothing happens
    let mine = g.p(me).unwrap().weapon().id;
    g.handle(me, C2S::PickUp(pk.id));
    assert_eq!(g.p(me).unwrap().weapon().id, mine);

    // next to it: swap, and our old gun is dropped in its place
    g.pm(me).unwrap().body.pos = pk.pos + glam::vec3(0.8, 0.0, 0.0);
    g.handle(me, C2S::PickUp(pk.id));
    assert_eq!(g.p(me).unwrap().weapon().id, WeaponId::M249);
    assert_eq!(g.snapshot_for(me).me.weapons[0], WeaponId::M249);
    assert_eq!(g.pickups.len(), 1);
    assert_eq!(g.pickups[0].w.id, mine);

    // walking over a gun we already carry takes its ammo
    g.pm(me).unwrap().weapons[0].reserve = 0;
    let ammo = g.pickups[0].w.mag + g.pickups[0].w.reserve;
    g.pickups[0].w.id = WeaponId::M249;
    g.pm(me).unwrap().body.pos = g.pickups[0].pos;
    g.tick(TICK_DT);
    assert_eq!(g.p(me).unwrap().weapons[0].reserve, ammo);
    assert!(g.pickups.is_empty(), "an emptied gun disappears");
}

#[test]
fn either_team_can_pick_up_a_dropped_bomb() {
    let (mut g, me) = setup(MapId::Depot, Mode::Sab);
    toon_server::admin::exec(&mut g, "bots 0");
    let other = g
        .connect(
            "127.0.0.1:5001".parse().unwrap(),
            "Другий",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(other, C2S::JoinTeam(Some(Team::Axis)));
    g.handle(other, C2S::SetLoadout(DEFAULT_CLASSES[0].1));
    // wait for the round to start
    for _ in 0..600 {
        run(&mut g, 0.5);
        if g.state == MatchState::Playing {
            break;
        }
    }
    assert_eq!(g.state, MatchState::Playing);
    // the bomb was carried by the other team and dropped
    g.sab.carrier = None;
    g.sab.owner = Some(Team::Axis);
    g.sab.dropped_since = g.time;
    let p = g.pm(me).unwrap();
    p.alive = true;
    let at = p.body.pos;
    g.sab.pos = at;
    run(&mut g, 0.2);
    assert_eq!(
        g.sab.carrier,
        Some(me),
        "an allied player picks up the axis bomb"
    );
    assert_eq!(g.sab.owner, Some(Team::Allies));
}

#[test]
fn smoke_grenade_makes_a_cloud_that_fades() {
    use toon_shared::consts::{SMOKE_FUSE, SMOKE_TIME};
    let (mut g, me) = setup(MapId::Depot, Mode::Tdm);
    toon_server::admin::exec(&mut g, "bots 0");
    run(&mut g, 1.0);
    let p = g.pm(me).unwrap();
    p.alive = true;
    p.protected_until = 0.0;
    assert_eq!(p.smokes, 1, "one smoke per life");
    let frags = p.grenades;
    let eye = p.eye();
    let throw = C2S::Grenade {
        origin: eye.to_array(),
        dir: [0.0, 0.3, -1.0],
        cook: 0.0,
        smoke: true,
    };
    g.handle(me, throw.clone());
    g.handle(me, throw);
    assert_eq!(g.grenades.len(), 1, "only one smoke to throw");
    assert_eq!(g.pm(me).unwrap().grenades, frags, "frags are untouched");
    run(&mut g, SMOKE_FUSE + 0.5);
    assert!(g.grenades.is_empty());
    assert_eq!(g.smokes.len(), 1, "the cloud popped");
    run(&mut g, 2.5);
    let clouds = g.smoke_clouds();
    assert!(clouds[0].1 > 4.0, "the cloud has grown: {}", clouds[0].1);
    // it hides what is behind it
    let c = clouds[0].0;
    let a = c + glam::vec3(-12.0, 0.2, 0.0);
    let b = c + glam::vec3(12.0, 0.2, 0.0);
    assert!(toon_shared::ray::smoke_blocks(a, b, &clouds));
    run(&mut g, SMOKE_TIME);
    assert!(g.smokes.is_empty(), "the cloud is gone");
}
