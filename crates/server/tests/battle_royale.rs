//! A whole battle royale round with bots on the island: everybody boards the plane and
//! jumps, lands, finds guns, the zone closes in, and exactly one player wins.

use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::physics::Chute;
use toon_shared::protocol::{MatchState, C2S, S2C};
use toon_shared::weapons::{Cause, WeaponClass, DEFAULT_CLASSES};
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

    let step = |g: &mut Game| {
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
    // either one survivor, or the round stopped once our player was out
    assert!(
        g.br_alive() <= 1 || winner.as_deref().is_some_and(|w| w.contains("вибули")),
        "{winner:?}"
    );
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

/// Once every real player is out, the bots do not play the round out on their own.
#[test]
fn round_ends_when_the_last_real_player_is_out() {
    let mut cfg = ServerConfig {
        data_dir: std::env::temp_dir().join("tw-br3").to_string_lossy().into(),
        ..Default::default()
    };
    cfg.br.bots = 12;
    cfg.br.max_players = 12;
    let mut g = Game::new(cfg);
    g.rng = toon_shared::rng::Rng::new(5);
    g.change_map(MapId::Island, Mode::Br);
    let me = g
        .connect(
            "127.0.0.1:5005".parse().unwrap(),
            "Гравець",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    for _ in 0..(40.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
        if g.br.running && g.p(me).is_some_and(|p| p.in_plane) {
            g.handle(me, C2S::BrJump);
        }
    }
    assert!(g.br.running && g.br_alive() > 3);
    g.kill(me, None, toon_shared::weapons::Cause::Zone, false);
    g.tick(TICK_DT);
    assert_eq!(
        g.state,
        MatchState::PostMatch,
        "the bots kept playing on their own"
    );
}

/// Supplies are picked up with the use key (ammo even for a gun you do not carry),
/// medkits are kept and used on demand, a vest soaks hits, and a knocked out player
/// leaves everything they carried on the ground.
#[test]
fn inventory_pick_up_heal_and_drop() {
    use toon_shared::inventory::AmmoType;
    use toon_shared::protocol::LootKind;
    let mut cfg = ServerConfig {
        data_dir: std::env::temp_dir().join("tw-br4").to_string_lossy().into(),
        ..Default::default()
    };
    cfg.br.bots = 2;
    cfg.br.max_players = 4;
    let mut g = Game::new(cfg);
    g.rng = toon_shared::rng::Rng::new(9);
    g.change_map(MapId::Island, Mode::Br);
    let me = g
        .connect(
            "127.0.0.1:5006".parse().unwrap(),
            "Гравець",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    let step = |g: &mut Game, secs: f32| {
        for _ in 0..(secs / TICK_DT).max(1.0) as usize {
            g.tick(TICK_DT);
            g.outbox.clear();
            g.shots.clear();
        }
    };
    for _ in 0..(60.0 / TICK_DT) as usize {
        step(&mut g, TICK_DT);
        if g.br.running {
            break;
        }
    }
    assert!(g.br.running);
    g.handle(me, C2S::BrJump);
    // (our client would fly the parachute down: put us on the ground)
    let ground = {
        let p = g.p(me).unwrap();
        let (x, z) = (p.body.pos.x, p.body.pos.z);
        glam::vec3(x, g.world.ground(x, z).unwrap_or(0.0), z)
    };
    {
        let p = g.pm(me).unwrap();
        p.body.chute = Chute::None;
        p.body.pos = ground;
        p.body.vel = glam::Vec3::ZERO;
    }
    let p = g.p(me).unwrap();
    assert!(p.alive);
    assert_eq!(p.inv.ammo(AmmoType::Pistol), 15);
    let at = p.body.pos;

    // put supplies at our feet: walking over them takes nothing, the use key does
    let put = |g: &mut Game, kind: LootKind, amount: u16| {
        let id = g.new_ent_id();
        g.br.loot.push(toon_server::br::Loot {
            id,
            kind,
            amount,
            pos: at,
        });
        id
    };
    let sniper = put(&mut g, LootKind::Ammo(AmmoType::Sniper), 10);
    let kits = put(&mut g, LootKind::Medkit, 3);
    let vest = put(&mut g, LootKind::Armor(2), 100);
    step(&mut g, 1.0);
    assert_eq!(g.p(me).unwrap().inv.ammo(AmmoType::Sniper), 0);
    for e in [sniper, kits, vest] {
        g.handle(me, C2S::PickUp(e));
    }
    let p = g.p(me).unwrap();
    assert_eq!(p.inv.ammo(AmmoType::Sniper), 10, "no sniper rifle needed");
    assert_eq!(p.inv.medkits, 2, "no backpack: two medkits fit");
    assert_eq!(p.inv.armor, 2);
    let kit_left = g.br.loot.iter().find(|l| l.id == kits).map(|l| l.amount);
    assert_eq!(kit_left, Some(1), "what does not fit stays on the ground");

    // a backpack makes room for the third
    let bag = put(&mut g, LootKind::Backpack(1), 1);
    g.handle(me, C2S::PickUp(bag));
    g.handle(me, C2S::PickUp(kits));
    assert_eq!(g.p(me).unwrap().inv.medkits, 3);

    // the vest takes its share of a hit
    let hp0 = g.p(me).unwrap().hp;
    g.damage(me, None, 50.0, Cause::Zone, false, ground);
    let zone_hit = hp0 - g.p(me).unwrap().hp;
    assert!((zone_hit - 50.0).abs() < 0.1, "the zone ignores vests");
    g.pm(me).unwrap().hp = 100.0;
    g.damage(
        me,
        None,
        50.0,
        Cause::Weapon(toon_shared::weapons::WeaponId::M4),
        false,
        ground,
    );
    let hit = 100.0 - g.p(me).unwrap().hp;
    assert!(
        (hit - 30.0).abs() < 0.1,
        "a level II vest takes 40% ({hit})"
    );

    // the medkit takes three seconds and heals
    let hp1 = g.p(me).unwrap().hp;
    g.handle(me, C2S::UseMedkit);
    step(&mut g, 1.0);
    assert_eq!(g.p(me).unwrap().hp, hp1, "heals only once it is used up");
    step(&mut g, 2.5);
    let p = g.p(me).unwrap();
    assert!(
        p.hp > (hp1 + 59.0).min(99.9) && p.inv.medkits == 2,
        "hp {} kits {}",
        p.hp,
        p.inv.medkits
    );

    // knocked out: everything ends up on the ground around the body
    let before = g.br.loot.len();
    g.kill(me, None, Cause::Zone, false);
    let dropped: Vec<_> = g.br.loot[before..]
        .iter()
        .map(|l| (l.kind, l.amount))
        .collect();
    assert!(
        dropped.contains(&(LootKind::Ammo(AmmoType::Sniper), 10)),
        "{dropped:?}"
    );
    assert!(dropped.contains(&(LootKind::Medkit, 2)));
    assert!(dropped.iter().any(|(k, _)| *k == LootKind::Armor(2)));
    assert!(dropped.contains(&(LootKind::Backpack(1), 1)));
}
