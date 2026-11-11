//! A whole battle royale round with bots on the island: everybody boards the plane and
//! jumps, lands, finds guns, the zone closes in, and exactly one player wins.

use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::physics::Chute;
use toon_shared::protocol::{MatchState, C2S, S2C};
use toon_shared::weapons::{Cause, WeaponClass, WeaponId, DEFAULT_CLASSES};
use toon_shared::{Mode, Team};

#[test]
fn a_round_of_battle_royale() {
    let mut cfg = ServerConfig {
        // (tests start straight away: nobody presses «Ready»)
        ready_check: false,
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

    // a little over a minute later (the last ones jump at the far end of a 2 km run)
    // every bot has landed and most found a real gun
    for _ in 0..(75.0 / TICK_DT) as usize {
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
        // (tests start straight away: nobody presses «Ready»)
        ready_check: false,
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
        // (tests start straight away: nobody presses «Ready»)
        ready_check: false,
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
        // (tests start straight away: nobody presses «Ready»)
        ready_check: false,
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
    assert_eq!(p.inv.medkits, 1, "one medkit to start with");
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
    // (one medkit comes with the jump)
    let kits = put(&mut g, LootKind::Medkit, 2);
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

    // sights: a 4× scope does not go on the pistol, so it waits in the backpack; once
    // there is a rifle it goes on from there; a red dot goes straight on the pistol
    use toon_shared::protocol::{Item, ItemAction};
    let scope = put(&mut g, LootKind::Sight(3), 1);
    g.handle(me, C2S::PickUp(scope));
    let p = g.p(me).unwrap();
    assert_eq!(p.inv.sights, [0, 0], "no gun takes it yet");
    assert_eq!(p.inv.spare_sights, [0, 0, 1, 0], "kept in the backpack");
    {
        let p = g.pm(me).unwrap();
        p.weapons[1] = toon_server::game::WState {
            id: toon_shared::weapons::WeaponId::M4,
            mag: 30,
            reserve: 0,
        };
        p.empty_slot = None;
        p.slot = 0;
    }
    g.handle(me, C2S::Item(ItemAction::AttachSight { level: 3, slot: 0 }));
    assert_eq!(
        g.p(me).unwrap().inv.sights,
        [0, 0],
        "a 4× does not go on a pistol"
    );
    g.handle(me, C2S::Item(ItemAction::AttachSight { level: 3, slot: 1 }));
    let dot = put(&mut g, LootKind::Sight(1), 1);
    g.handle(me, C2S::PickUp(dot));
    let p = g.p(me).unwrap();
    assert_eq!(p.inv.sights, [1, 3]);
    assert_eq!(p.inv.spare_sights, [0; 4]);
    // off and on again
    g.handle(me, C2S::Item(ItemAction::DetachSight(1)));
    assert_eq!(g.p(me).unwrap().inv.spare_sights, [0, 0, 1, 0]);
    g.handle(me, C2S::Item(ItemAction::AttachSight { level: 3, slot: 1 }));
    assert_eq!(g.p(me).unwrap().inv.sights, [1, 3]);

    // throwing rounds away: they land in front as a box
    let before = g.br.loot.len();
    g.handle(
        me,
        C2S::Item(ItemAction::Drop(Item::Ammo(AmmoType::Sniper), 4)),
    );
    assert_eq!(g.p(me).unwrap().inv.ammo(AmmoType::Sniper), 6);
    assert_eq!(
        g.br.loot[before..]
            .iter()
            .map(|l| (l.kind, l.amount))
            .collect::<Vec<_>>(),
        vec![(LootKind::Ammo(AmmoType::Sniper), 4)]
    );
    let back = g.br.loot.last().unwrap().id;
    g.handle(me, C2S::PickUp(back));
    assert_eq!(g.p(me).unwrap().inv.ammo(AmmoType::Sniper), 10);

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

    // throwing the rifle away: its sight goes in the backpack, the slot is empty again
    g.handle(me, C2S::Item(ItemAction::Drop(Item::Weapon(1), 1)));
    let p = g.p(me).unwrap();
    assert_eq!(p.empty_slot, Some(1));
    assert_eq!(p.inv.sights, [1, 0]);
    assert_eq!(p.inv.spare_sights, [0, 0, 1, 0]);
    assert!(g
        .pickups
        .iter()
        .any(|pk| pk.w.id == toon_shared::weapons::WeaponId::M4));
    // the pistol cannot go too: one gun stays in the hands
    g.handle(me, C2S::Item(ItemAction::Drop(Item::Weapon(0), 1)));
    assert_eq!(g.p(me).unwrap().empty_slot, Some(1));

    // knocked out: everything ends up in one box where they fell, guns too
    let loot_before = g.br.loot.len();
    g.kill(me, None, Cause::Zone, false);
    assert_eq!(g.br.loot.len(), loot_before, "nothing on the ground");
    assert_eq!(g.br.boxes.len(), 1, "one box");
    let b = g.br.boxes[0].clone();
    let dropped = b.items.clone();
    assert!(
        dropped.contains(&(LootKind::Ammo(AmmoType::Sniper), 10)),
        "{dropped:?}"
    );
    assert!(dropped.contains(&(LootKind::Medkit, 2)));
    assert!(dropped.iter().any(|(k, _)| *k == LootKind::Armor(2)));
    assert!(dropped.contains(&(LootKind::Sight(1), 1)));
    assert!(dropped.contains(&(LootKind::Sight(3), 1)));
    assert!(b
        .guns
        .iter()
        .any(|g| g.id == toon_shared::weapons::WeaponId::M9));

    // a passer-by: takes the sniper rounds, then everything else; the box goes once empty
    let other = g
        .players
        .iter()
        .flatten()
        .find(|p| p.is_bot())
        .map(|p| p.id)
        .expect("a bot to loot the box");
    {
        let p = g.pm(other).unwrap();
        p.alive = true;
        p.in_plane = false;
        p.body.chute = Chute::None;
        p.body.pos = b.pos + glam::vec3(1.0, 0.0, 0.0);
        p.inv = Default::default();
        p.inv.backpack = 3;
        // (not the pistol in the box: the same gun twice is not taken)
        p.weapons[1] = toon_server::game::WState {
            id: toon_shared::weapons::WeaponId::Deagle,
            mag: 7,
            reserve: 0,
        };
    }
    let k = b
        .items
        .iter()
        .position(|i| i.0 == LootKind::Ammo(AmmoType::Sniper))
        .unwrap();
    g.box_take(other, b.id, k);
    assert_eq!(g.p(other).unwrap().inv.ammo(AmmoType::Sniper), 10);
    assert_eq!(g.br.boxes[0].items.len(), b.items.len() - 1);
    g.box_take_all(other, b.id);
    // (only what is no better than what the bot has stays: its backpack is level III)
    let left: Vec<_> =
        g.br.boxes
            .first()
            .map(|b| b.items.clone())
            .unwrap_or_default();
    assert!(
        left.iter().all(|i| i.0 == LootKind::Backpack(1)),
        "left in the box: {left:?}"
    );
    // the pistol: swapped for the gun in hand, which goes in the box
    if let Some(bx) = g.br.boxes.first().cloned() {
        let before = g.p(other).unwrap().weapon().id;
        // (guns come after the supplies)
        g.box_take(other, bx.id, bx.items.len());
        let now = g.p(other).unwrap().weapon().id;
        assert_ne!(before, now, "the gun was not taken");
    }
}

/// Nobody waits for the round inside someone else, and the bots come down spread out
/// over the island instead of in a heap under the plane.
#[test]
fn lobby_and_landing_are_spread_out() {
    let mut cfg = ServerConfig {
        // (tests start straight away: nobody presses «Ready»)
        ready_check: false,
        data_dir: std::env::temp_dir().join("tw-br5").to_string_lossy().into(),
        ..Default::default()
    };
    cfg.br.bots = 40;
    cfg.br.max_players = 41;
    let mut g = Game::new(cfg);
    g.rng = toon_shared::rng::Rng::new(21);
    g.change_map(MapId::Island, Mode::Br);
    let me = g
        .connect(
            "127.0.0.1:5007".parse().unwrap(),
            "Гравець",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    let flat = |a: glam::Vec3, b: glam::Vec3| glam::vec2(a.x - b.x, a.z - b.z).length();
    // nearest neighbour of each position
    let nearest = |ps: &[glam::Vec3]| -> Vec<f32> {
        ps.iter()
            .enumerate()
            .map(|(i, a)| {
                ps.iter()
                    .enumerate()
                    .filter(|(j, _)| *j != i)
                    .map(|(_, b)| flat(*a, *b))
                    .fold(f32::MAX, f32::min)
            })
            .collect()
    };
    let mut lobby_min = f32::MAX;
    for _ in 0..(60.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
        if g.br.running {
            break;
        }
        let ps: Vec<_> = g
            .players
            .iter()
            .flatten()
            .filter(|p| p.alive)
            .map(|p| p.body.pos)
            .collect();
        if ps.len() > 10 {
            let n = nearest(&ps);
            lobby_min = lobby_min.min(n.iter().cloned().fold(f32::MAX, f32::min));
        }
    }
    assert!(g.br.running, "the round did not start");
    eprintln!("lobby: closest two players {lobby_min:.2} m apart");
    assert!(
        lobby_min > 0.9,
        "players inside each other in the lobby ({lobby_min:.2} m)"
    );

    g.handle(me, C2S::BrJump);
    let mut landed: std::collections::HashMap<u8, glam::Vec3> = Default::default();
    let mut aims: std::collections::HashMap<u8, glam::Vec3> = Default::default();
    for _ in 0..(110.0 / TICK_DT) as usize {
        for p in g.players.iter().flatten() {
            if let Some(l) = p.bot.as_ref().and_then(|b| b.br_land) {
                aims.insert(p.id, l);
            }
        }
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
        for p in g.players.iter().flatten().filter(|p| p.is_bot()) {
            if p.alive && !p.in_plane && p.body.chute == Chute::None {
                landed.entry(p.id).or_insert(p.body.pos);
            }
        }
    }
    for p in g.players.iter().flatten().filter(|p| p.is_bot()) {
        if !landed.contains_key(&p.id) {
            eprintln!(
                "not landed: {} alive {} plane {} chute {:?} at {:.1} vel {:.1}",
                p.name, p.alive, p.in_plane, p.body.chute, p.body.pos, p.body.vel
            );
        }
    }
    // they come down where they meant to
    let missed = landed
        .iter()
        .filter_map(|(id, at)| aims.get(id).map(|a| flat(*a, *at)))
        .filter(|d| *d > 40.0)
        .count();
    assert!(missed <= 2, "{missed} bots landed far from their spot");
    let ps: Vec<_> = landed.values().cloned().collect();
    // (one or two may be shot on the way down)
    assert!(ps.len() >= 34, "only {} bots landed", ps.len());
    let mut n = nearest(&ps);
    n.sort_by(f32::total_cmp);
    let median = n[n.len() / 2];
    let crowded = n.iter().filter(|d| **d < 15.0).count();
    eprintln!(
        "landing: nearest neighbour median {median:.0} m, min {:.1} m, {crowded} within 15 m",
        n[0]
    );
    assert!(
        median > 45.0,
        "bots land too close together (median {median:.0} m)"
    );
    assert!(crowded <= 2, "{crowded} bots landed within 15 m of another");
}

/// The match waits until every real player has pressed «Ready» (and can be told «not yet»
/// again before it starts).
#[test]
fn the_match_waits_for_ready_players() {
    let mut cfg = ServerConfig {
        data_dir: std::env::temp_dir().join("tw-br6").to_string_lossy().into(),
        ..Default::default()
    };
    assert!(cfg.ready_check, "on by default");
    cfg.br.bots = 4;
    cfg.br.max_players = 8;
    let mut g = Game::new(cfg);
    g.change_map(MapId::Market, Mode::Br);
    let me = g
        .connect(
            "127.0.0.1:5008".parse().unwrap(),
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
    run(&mut g, 20.0);
    assert_eq!(g.state, MatchState::Warmup, "started without anyone ready");
    assert_eq!(g.snapshot_for(me).ready, Some((0, 1, false)));
    g.handle(me, C2S::Ready(true));
    run(&mut g, 2.0);
    assert_ne!(g.state, MatchState::Warmup, "ready, but nothing started");
}

/// Squads of two: squadmates do not hurt each other; a lethal hit knocks you down while a
/// squadmate stands; holding the use key next to you picks you up; left alone you bleed
/// out (the one who knocked you down gets the kill); a squad with nobody standing is out.
#[test]
fn squads_knock_down_and_revive() {
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: std::env::temp_dir().join("tw-br7").to_string_lossy().into(),
        ..Default::default()
    };
    // (bots fill up to six players, the two of us included)
    cfg.br.bots = 6;
    cfg.br.max_players = 6;
    cfg.br.squad = 2;
    let mut g = Game::new(cfg);
    g.rng = toon_shared::rng::Rng::new(3);
    g.change_map(MapId::Island, Mode::Br);
    let join = |g: &mut Game, port: u16| {
        let id = g
            .connect(
                format!("127.0.0.1:{port}").parse().unwrap(),
                "Гравець",
                "",
                toon_shared::consts::PROTOCOL_VERSION,
            )
            .unwrap();
        g.handle(id, C2S::JoinTeam(None));
        id
    };
    let me = join(&mut g, 5009);
    let mate = join(&mut g, 5010);
    let step = |g: &mut Game, secs: f32| {
        for _ in 0..(secs / TICK_DT).max(1.0) as usize {
            // both hang in the sky, far from the bots
            for (id, x) in [(me, 0.0), (mate, 1.0)] {
                if let Some(p) = g.pm(id) {
                    if p.alive {
                        p.body.pos = glam::vec3(x, 400.0, 0.0);
                        p.body.vel = glam::Vec3::ZERO;
                        p.body.chute = Chute::None;
                        p.protected_until = 0.0;
                    }
                }
            }
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
    let (sq_me, sq_mate) = (g.p(me).unwrap().squad, g.p(mate).unwrap().squad);
    assert!(
        sq_me != 0 && sq_me == sq_mate,
        "the two real players play together"
    );
    for s in 1..=3u8 {
        let n = g.players.iter().flatten().filter(|p| p.squad == s).count();
        assert_eq!(n, 2, "squad {s} has {n}");
    }
    g.handle(me, C2S::BrJump);
    g.handle(mate, C2S::BrJump);
    step(&mut g, 0.5);
    let enemy = g
        .players
        .iter()
        .flatten()
        .find(|p| p.is_bot())
        .map(|p| p.id)
        .unwrap();
    let at = glam::Vec3::ZERO;

    // squadmates do not hurt each other
    g.damage(me, Some(mate), 50.0, Cause::Weapon(WeaponId::M4), false, at);
    assert_eq!(g.p(me).unwrap().hp, 100.0);

    // a lethal hit knocks down
    g.damage(
        me,
        Some(enemy),
        500.0,
        Cause::Weapon(WeaponId::M4),
        false,
        at,
    );
    let p = g.p(me).unwrap();
    assert!(p.alive && p.downed, "knocked down, not dead");
    assert!(g.snapshot_for(me).me.downed);
    // no shooting while down
    let mag = g.p(me).unwrap().weapon().mag;
    g.handle(
        me,
        C2S::Fire {
            weapon: g.p(me).unwrap().weapon().id,
            origin: g.p(me).unwrap().eye().to_array(),
            dirs: vec![[1.0, 0.0, 0.0]],
        },
    );
    assert_eq!(g.p(me).unwrap().weapon().mag, mag, "fired while down");

    // the squadmate holds the use key next to us: picked up after five seconds
    g.pm(mate).unwrap().flags = toon_shared::protocol::flags::USE;
    step(&mut g, 5.4);
    let p = g.p(me).unwrap();
    assert!(p.alive && !p.downed, "not picked up");
    assert!((p.hp - 30.0).abs() < 0.5, "hp {}", p.hp);
    g.pm(mate).unwrap().flags = 0;

    // knocked down again and left alone: bleeds out, the kill goes to the enemy
    let kills = g.p(enemy).unwrap().kills;
    g.damage(
        me,
        Some(enemy),
        500.0,
        Cause::Weapon(WeaponId::M4),
        false,
        at,
    );
    assert!(g.p(me).unwrap().downed);
    step(&mut g, 42.0);
    assert!(!g.p(me).unwrap().alive, "did not bleed out");
    assert_eq!(
        g.p(enemy).unwrap().kills,
        kills + 1,
        "the knock-down earns the kill"
    );

    // the squadmate is knocked down with nobody else standing: out at once
    g.damage(
        mate,
        Some(enemy),
        500.0,
        Cause::Weapon(WeaponId::M4),
        false,
        at,
    );
    step(&mut g, 0.2);
    assert!(
        !g.p(mate).unwrap().alive,
        "a squad with nobody standing is out"
    );
}

/// Jeeps: get in, drive (the car carries you), a passenger beside you, run an enemy over,
/// get out beside the car; enough bullets blow it up and hurt those inside.
#[test]
fn jeeps_drive_carry_and_blow_up() {
    use toon_shared::protocol::CarCmd;
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: std::env::temp_dir().join("tw-br8").to_string_lossy().into(),
        ..Default::default()
    };
    cfg.br.bots = 3;
    cfg.br.max_players = 4;
    let mut g = Game::new(cfg);
    g.rng = toon_shared::rng::Rng::new(4);
    g.change_map(MapId::Island, Mode::Br);
    let join = |g: &mut Game, port: u16| {
        let id = g
            .connect(
                format!("127.0.0.1:{port}").parse().unwrap(),
                "Водій",
                "",
                toon_shared::consts::PROTOCOL_VERSION,
            )
            .unwrap();
        g.handle(id, C2S::JoinTeam(None));
        id
    };
    let me = join(&mut g, 5011);
    let friend = join(&mut g, 5012);
    let tick = |g: &mut Game, secs: f32| {
        for _ in 0..(secs / TICK_DT).max(1.0) as usize {
            g.tick(TICK_DT);
            g.outbox.clear();
            g.shots.clear();
        }
    };
    for _ in 0..(60.0 / TICK_DT) as usize {
        tick(&mut g, TICK_DT);
        if g.br.running {
            break;
        }
    }
    assert!(g.br.running);
    assert!(g.cars.len() >= 15, "{} cars", g.cars.len());
    g.handle(me, C2S::BrJump);
    g.handle(friend, C2S::BrJump);
    // land both next to a jeep
    let (car_id, at) = g
        .cars
        .iter()
        .find(|c| c.kind == toon_shared::cars::CarKind::Jeep)
        .map(|c| (c.id, c.state.pos))
        .unwrap();
    for (id, dx) in [(me, -2.0), (friend, 2.0)] {
        let p = g.pm(id).unwrap();
        p.body.chute = Chute::None;
        p.body.pos = at + glam::vec3(dx, 0.0, 0.0);
        p.protected_until = 0.0;
    }
    g.handle(me, C2S::Car(CarCmd::Enter(car_id)));
    g.handle(friend, C2S::Car(CarCmd::Enter(car_id)));
    assert_eq!(g.seat_of(me).map(|s| s.1), Some(0), "the first in drives");
    assert_eq!(g.seat_of(friend).map(|s| s.1), Some(1));
    // the driver cannot shoot, the passenger can
    let mag = g.p(me).unwrap().weapon().mag;
    let fire = |g: &mut Game, id: u8| {
        let p = g.p(id).unwrap();
        let (w, eye) = (p.weapon().id, p.eye());
        let d = toon_shared::look_dir(p.yaw, p.pitch);
        g.handle(
            id,
            C2S::Fire {
                weapon: w,
                origin: eye.to_array(),
                dirs: vec![d.to_array()],
            },
        );
    };
    fire(&mut g, me);
    assert_eq!(g.p(me).unwrap().weapon().mag, mag, "the driver fired");

    // drive for three seconds
    for _ in 0..90 {
        g.handle(
            me,
            C2S::Car(CarCmd::Drive {
                throttle: 100,
                steer: 0,
                brake: false,
            }),
        );
        tick(&mut g, TICK_DT);
    }
    let c = g.cars.iter().find(|c| c.id == car_id).unwrap();
    let moved = c.state.pos.distance(at);
    assert!(moved > 15.0, "the jeep moved only {moved:.1} m");
    let seat = c.seat_pos(0);
    assert!(
        g.p(me).unwrap().body.pos.distance(seat) < 0.01,
        "the driver rides along"
    );
    fire(&mut g, friend);
    assert!(
        g.p(friend).unwrap().weapon().mag < mag,
        "the passenger could not fire"
    );

    // an enemy in the way is run over
    let enemy = g
        .players
        .iter()
        .flatten()
        .find(|p| p.is_bot() && p.alive)
        .map(|p| p.id);
    if let Some(e) = enemy {
        let c = g.cars.iter().find(|c| c.id == car_id).unwrap();
        let ahead = c.state.to_world(glam::vec3(0.0, 0.0, 3.5));
        let p = g.pm(e).unwrap();
        p.body.pos = ahead;
        p.protected_until = 0.0;
        p.hp = 100.0;
        g.handle(
            me,
            C2S::Car(CarCmd::Drive {
                throttle: 100,
                steer: 0,
                brake: false,
            }),
        );
        tick(&mut g, TICK_DT);
        let p = g.p(e).unwrap();
        assert!(p.hp < 100.0 || !p.alive, "not run over");
    }

    // out beside the car
    g.handle(me, C2S::Car(CarCmd::Exit));
    assert!(g.seat_of(me).is_none());
    let c = g.cars.iter().find(|c| c.id == car_id).unwrap();
    let d = g.p(me).unwrap().body.pos.distance(c.state.pos);
    assert!((1.0..4.0).contains(&d), "got out {d:.1} m away");

    // shot to pieces: it blows up and the passenger is hurt
    let i = g.cars.iter().position(|c| c.id == car_id).unwrap();
    g.damage_car(i, 10_000.0, Some(me));
    tick(&mut g, TICK_DT);
    assert!(g.cars.iter().all(|c| c.id != car_id), "still there");
    let p = g.p(friend).unwrap();
    assert!(
        !p.alive || p.hp < 100.0 || p.downed,
        "the passenger was not hurt"
    );
}

/// Supply crates: announced, come down inside the next circle under a parachute, spill
/// rare gear (level III, an 8× scope, a heavy gun) and go once it is all taken.
#[test]
fn supply_drops_come_down_with_rare_gear() {
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: std::env::temp_dir().join("tw-br9").to_string_lossy().into(),
        ..Default::default()
    };
    cfg.br.bots = 2;
    cfg.br.max_players = 4;
    let mut g = Game::new(cfg);
    g.rng = toon_shared::rng::Rng::new(6);
    g.change_map(MapId::Island, Mode::Br);
    let me = g
        .connect(
            "127.0.0.1:5013".parse().unwrap(),
            "Гравець",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    let tick = |g: &mut Game, secs: f32| {
        for _ in 0..(secs / TICK_DT).max(1.0) as usize {
            if let Some(p) = g.pm(me) {
                if p.alive {
                    p.body.pos = glam::vec3(0.0, 400.0, 0.0);
                    p.body.chute = Chute::None;
                }
            }
            g.tick(TICK_DT);
            g.outbox.clear();
            g.shots.clear();
        }
    };
    for _ in 0..(60.0 / TICK_DT) as usize {
        tick(&mut g, TICK_DT);
        if g.br.running {
            break;
        }
    }
    g.handle(me, C2S::BrJump);
    g.br.next_drop = g.time;
    tick(&mut g, TICK_DT);
    assert_eq!(g.br.drops.len(), 1, "no crate");
    let d = g.br.drops[0].clone();
    assert!(!d.landed && d.pos.y > d.ground + 100.0);
    let (c, r) = (g.br.zone.to_c, g.br.zone.to_r.max(30.0));
    assert!(
        glam::vec2(d.pos.x, d.pos.z).distance(c) <= r,
        "outside the next circle"
    );
    assert_eq!(
        g.snapshot_for(me).br.unwrap().drops.len(),
        1,
        "not on the map"
    );
    tick(&mut g, 36.0);
    let d = g.br.drops[0].clone();
    assert!(d.landed, "still falling at {}", d.pos);
    let kinds: Vec<_> =
        g.br.loot
            .iter()
            .filter(|l| d.items.contains(&l.id))
            .map(|l| l.kind)
            .collect();
    assert!(
        kinds.contains(&toon_shared::protocol::LootKind::Sight(4)),
        "{kinds:?}"
    );
    let level3 = kinds
        .iter()
        .filter(|k| {
            matches!(
                k,
                toon_shared::protocol::LootKind::Armor(3)
                    | toon_shared::protocol::LootKind::Helmet(3)
                    | toon_shared::protocol::LootKind::Backpack(3)
            )
        })
        .count();
    assert_eq!(level3, 2);
    assert!(g.pickups.iter().any(|p| d.items.contains(&p.id)), "no gun");
    // everything taken: the crate goes
    g.br.loot.retain(|l| !d.items.contains(&l.id));
    g.pickups.retain(|p| !d.items.contains(&p.id));
    tick(&mut g, TICK_DT);
    assert!(g.br.drops.is_empty());
}

/// Attachments: an extended magazine holds half as many rounds more, a suppressed shot is
/// quiet, a grip a pistol cannot take waits in the backpack, and taking the magazine off
/// puts the extra rounds back in the bag.
#[test]
fn attachments_change_the_gun() {
    use toon_shared::inventory::{AmmoType, MOD_EXTMAG, MOD_GRIP, MOD_SUPPRESSOR};
    use toon_shared::protocol::{ItemAction, LootKind};
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: std::env::temp_dir()
            .join("tw-br10")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    cfg.br.bots = 2;
    cfg.br.max_players = 3;
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Br);
    let me = g
        .connect(
            "127.0.0.1:5014".parse().unwrap(),
            "Гравець",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    let tick = |g: &mut Game, secs: f32| {
        for _ in 0..(secs / TICK_DT).max(1.0) as usize {
            if let Some(p) = g.pm(me) {
                if p.alive {
                    p.body.pos = glam::vec3(0.0, 400.0, 0.0);
                    p.body.chute = Chute::None;
                    p.protected_until = 0.0;
                }
            }
            g.tick(TICK_DT);
            g.outbox.clear();
        }
    };
    for _ in 0..(60.0 / TICK_DT) as usize {
        tick(&mut g, TICK_DT);
        g.shots.clear();
        if g.br.running {
            break;
        }
    }
    g.handle(me, C2S::BrJump);
    tick(&mut g, 0.2);
    // a rifle in the first hand, rounds for it
    {
        let p = g.pm(me).unwrap();
        p.weapons[0] = toon_server::game::WState {
            id: WeaponId::M4,
            mag: 30,
            reserve: 0,
        };
        p.slot = 0;
        p.inv.add_ammo(AmmoType::Rifle, 90);
        p.br_refresh_reserve();
    }
    let at = g.p(me).unwrap().body.pos;
    let put = |g: &mut Game, kind: LootKind| {
        let id = g.new_ent_id();
        g.br.loot.push(toon_server::br::Loot {
            id,
            kind,
            amount: 1,
            pos: at,
        });
        id
    };
    for k in [MOD_EXTMAG, MOD_SUPPRESSOR, MOD_GRIP] {
        let e = put(&mut g, LootKind::Mod(k));
        g.handle(me, C2S::PickUp(e));
    }
    assert_eq!(
        g.p(me).unwrap().inv.mods[0],
        [MOD_SUPPRESSOR, MOD_EXTMAG, MOD_GRIP]
    );
    // the magazine: 45 after a reload
    g.pm(me).unwrap().weapons[0].mag = 10;
    g.start_reload(me);
    tick(&mut g, 3.0);
    assert_eq!(g.p(me).unwrap().weapons[0].mag, 45);
    // quiet shots
    g.shots.clear();
    let p = g.p(me).unwrap();
    let (eye, d) = (p.eye(), toon_shared::look_dir(p.yaw, p.pitch));
    g.pm(me).unwrap().switch_end = 0.0;
    g.handle(
        me,
        C2S::Fire {
            weapon: WeaponId::M4,
            origin: eye.to_array(),
            dirs: vec![d.to_array()],
        },
    );
    assert!(g.shots.iter().all(|s| s.quiet) && !g.shots.is_empty());
    // a grip the pistol cannot take goes in the backpack
    let e = put(&mut g, LootKind::Mod(MOD_GRIP));
    g.pm(me).unwrap().slot = 1;
    g.handle(me, C2S::PickUp(e));
    let p = g.p(me).unwrap();
    assert_eq!(p.inv.spare_mods[MOD_GRIP as usize - 1], 1);
    // everything off the rifle: the magazine's extra rounds go back in the bag
    g.pm(me).unwrap().inv.backpack = 3;
    let before = g.p(me).unwrap().inv.ammo(AmmoType::Rifle);
    g.handle(me, C2S::Item(ItemAction::DetachMods(0)));
    let p = g.p(me).unwrap();
    assert_eq!(p.inv.mods[0], [0, 0, 0]);
    assert_eq!(p.weapons[0].mag, 30);
    assert_eq!(p.inv.ammo(AmmoType::Rifle), before + 14);
}

/// Consumables: a bandage heals a little and not past 75, an energy drink heals slowly
/// over time, a molotov sets the ground on fire for a while, a flashbang blinds a bot
/// looking at it.
#[test]
fn bandages_drinks_molotovs_and_flashbangs() {
    use toon_shared::inventory::{THROW_FLASH, THROW_MOLOTOV};
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: std::env::temp_dir()
            .join("tw-br11")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    cfg.br.bots = 2;
    cfg.br.max_players = 3;
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Br);
    let me = g
        .connect(
            "127.0.0.1:5015".parse().unwrap(),
            "Гравець",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    let sky = glam::vec3(0.0, 400.0, 0.0);
    let tick = |g: &mut Game, secs: f32| {
        for _ in 0..(secs / TICK_DT).max(1.0) as usize {
            if let Some(p) = g.pm(me) {
                if p.alive {
                    p.body.pos = sky;
                    p.body.chute = Chute::None;
                    p.protected_until = 0.0;
                }
            }
            g.tick(TICK_DT);
            g.outbox.clear();
            g.shots.clear();
        }
    };
    for _ in 0..(60.0 / TICK_DT) as usize {
        tick(&mut g, TICK_DT);
        if g.br.running {
            break;
        }
    }
    g.handle(me, C2S::BrJump);
    tick(&mut g, 0.2);
    {
        let p = g.pm(me).unwrap();
        p.inv.bandages = 2;
        p.inv.drinks = 1;
        p.inv.molotovs = 1;
        p.inv.flashes = 1;
        p.hp = 70.0;
    }
    // a bandage: 70 -> 75 (no further)
    g.handle(me, C2S::UseBandage);
    tick(&mut g, 4.3);
    let p = g.p(me).unwrap();
    assert!((p.hp - 75.0).abs() < 0.5, "hp {}", p.hp);
    assert_eq!(p.inv.bandages, 1);
    g.handle(me, C2S::UseBandage);
    tick(&mut g, 0.2);
    assert_eq!(g.p(me).unwrap().inv.healing, 0.0, "a bandage above 75");
    // a drink: the boost heals about a point a second
    g.handle(me, C2S::UseDrink);
    tick(&mut g, 3.2);
    let hp0 = g.p(me).unwrap().hp;
    assert!(g.p(me).unwrap().inv.boost > 35.0);
    tick(&mut g, 10.0);
    let gained = g.p(me).unwrap().hp - hp0;
    assert!((8.0..12.0).contains(&gained), "gained {gained}");

    // a molotov at a bot's feet: fire, and it burns
    let bot = g
        .players
        .iter()
        .flatten()
        .find(|p| p.is_bot())
        .map(|p| p.id)
        .unwrap();
    let ground = g.world.ground(0.0, 30.0).unwrap();
    let spot = glam::vec3(0.0, ground, 30.0);
    {
        let b = g.pm(bot).unwrap();
        b.alive = true;
        b.in_plane = false;
        b.body.chute = Chute::None;
        b.body.pos = spot;
        b.hp = 100.0;
        b.protected_until = 0.0;
    }
    g.handle(
        me,
        C2S::Throw {
            origin: (spot + glam::vec3(0.0, 1.0, 0.0)).to_array(),
            dir: [0.0, -1.0, 0.0],
            kind: THROW_MOLOTOV,
        },
    );
    // (thrown from up here the bottle falls a while)
    g.grenades.last_mut().unwrap().pos = spot + glam::vec3(0.0, 0.6, 0.0);
    for _ in 0..60 {
        if let Some(b) = g.pm(bot) {
            b.body.pos = spot;
            b.body.vel = glam::Vec3::ZERO;
        }
        tick(&mut g, TICK_DT);
    }
    assert!(!g.fires.is_empty(), "no fire");
    let b = g.p(bot).unwrap();
    assert!(b.hp < 100.0 || !b.alive, "the fire did not burn");

    // a flashbang right in front of a bot looking at it
    let b = g.pm(bot).unwrap();
    b.alive = true;
    b.hp = 100.0;
    b.yaw = toon_shared::yaw_of(glam::vec3(0.0, 0.0, 1.0));
    b.pitch = 0.0;
    let eye = g.p(bot).unwrap().eye();
    g.fires.clear();
    g.handle(
        me,
        C2S::Throw {
            origin: sky.to_array(),
            dir: [0.0, 0.0, 1.0],
            kind: THROW_FLASH,
        },
    );
    let gr = g.grenades.last_mut().unwrap();
    gr.pos = eye + glam::vec3(0.0, 0.0, 4.0);
    gr.vel = glam::Vec3::ZERO;
    for _ in 0..60 {
        if let Some(b) = g.pm(bot) {
            b.body.pos = spot;
        }
        tick(&mut g, TICK_DT);
    }
    let blind = g.p(bot).unwrap().bot.as_ref().unwrap().blind_until;
    assert!(blind > g.time - 1.0 && blind > 0.0, "not blinded ({blind})");
}

/// The red zone: marked inside the circle with a warning, shelled for a while, then gone.
#[test]
fn red_zone_is_marked_then_shelled() {
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: std::env::temp_dir()
            .join("tw-br12")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    cfg.br.bots = 2;
    cfg.br.max_players = 3;
    let mut g = Game::new(cfg);
    g.rng = toon_shared::rng::Rng::new(8);
    g.change_map(MapId::Island, Mode::Br);
    let me = g
        .connect(
            "127.0.0.1:5016".parse().unwrap(),
            "Гравець",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    let mut booms = 0;
    let tick = |g: &mut Game, secs: f32, booms: &mut usize| {
        for _ in 0..(secs / TICK_DT).max(1.0) as usize {
            if let Some(p) = g.pm(me) {
                if p.alive {
                    p.body.pos = glam::vec3(0.0, 400.0, 0.0);
                    p.body.chute = Chute::None;
                }
            }
            g.tick(TICK_DT);
            for (_, m, _) in g.outbox.drain(..) {
                if matches!(m, S2C::Explosion { big: true, .. }) {
                    *booms += 1;
                }
            }
            g.shots.clear();
        }
    };
    for _ in 0..(60.0 / TICK_DT) as usize {
        tick(&mut g, TICK_DT, &mut booms);
        if g.br.running {
            break;
        }
    }
    g.handle(me, C2S::BrJump);
    g.br.next_red = g.time;
    tick(&mut g, TICK_DT, &mut booms);
    let rz = g.br.red.expect("no red zone");
    let (c, r) = g.br.zone.at(g.time);
    assert!(rz.c.distance(c) <= r, "outside the circle");
    let snap = g.snapshot_for(me).br.unwrap().red.expect("not shown");
    assert!(snap[3] > 7.0, "no warning time");
    booms = 0;
    tick(&mut g, 7.0, &mut booms);
    assert_eq!(booms, 0, "shells during the warning");
    tick(&mut g, 21.5, &mut booms);
    assert!(booms > 25, "only {booms} shells");
    assert!(g.br.red.is_none(), "still shelling");
}

/// Out of the round: you watch someone still in (and see what is around them), get a
/// summary of how it went, and the server remembers it for next time.
#[test]
fn spectating_summary_and_records() {
    let dir = std::env::temp_dir().join("tw-br-stats");
    let _ = std::fs::remove_dir_all(&dir);
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: dir.to_string_lossy().into(),
        ..Default::default()
    };
    cfg.br.bots = 10;
    cfg.br.max_players = 10;
    let mut g = Game::new(cfg);
    g.rng = toon_shared::rng::Rng::new(9);
    g.change_map(MapId::Island, Mode::Br);
    let v = toon_shared::consts::PROTOCOL_VERSION;
    let a = g
        .connect("127.0.0.1:5101".parse().unwrap(), "Перший", "", v)
        .unwrap();
    let b = g
        .connect("127.0.0.1:5102".parse().unwrap(), "Другий", "", v)
        .unwrap();
    g.handle(a, C2S::JoinTeam(None));
    g.handle(b, C2S::JoinTeam(None));
    for _ in 0..(40.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
        for id in [a, b] {
            if g.br.running && g.p(id).is_some_and(|p| p.in_plane) {
                g.handle(id, C2S::BrJump);
            }
        }
    }
    assert!(g.br.running);
    // (put them far apart, so what A sees can only come from watching B)
    let far = g.p(b).unwrap().body.pos + glam::vec3(700.0, 0.0, 0.0);
    g.pm(a).unwrap().body.pos = far;
    g.kill(a, None, Cause::Zone, false);
    g.tick(TICK_DT);
    assert_eq!(g.state, MatchState::Playing, "B is still in");
    let snap = g.snapshot_for(a);
    let s = snap
        .br
        .as_ref()
        .and_then(|b| b.summary.clone())
        .expect("a summary");
    assert!(s.place > 1 && s.of >= 10, "place {} of {}", s.place, s.of);
    assert_eq!((s.matches, s.wins), (1, 0));
    // the server gave A somebody to watch, and sends what is around them
    let w = g.p(a).unwrap().spectating.expect("somebody to watch");
    assert!(g.p(w).unwrap().alive);
    assert_eq!(snap.br.as_ref().unwrap().watching, Some(w));
    assert!(snap.players.iter().any(|p| p.id == w));
    // stepping through the players gets to B, far away as they are
    let mut seen = vec![w];
    for _ in 0..12 {
        g.handle(a, C2S::Spectate(0));
        seen.push(g.p(a).unwrap().spectating.unwrap());
    }
    assert!(seen.contains(&b), "never got to B: {seen:?}");
    while g.p(a).unwrap().spectating != Some(b) {
        g.handle(a, C2S::Spectate(1));
    }
    let snap = g.snapshot_for(a);
    assert!(snap.players.iter().any(|p| p.id == b), "watching B shows B");
    // a free camera watches nobody; the one watched dying moves us on
    g.handle(a, C2S::Spectate(2));
    assert_eq!(g.snapshot_for(a).br.unwrap().watching, None);
    g.handle(a, C2S::Spectate(3));
    g.tick(TICK_DT);
    let w = g.p(a).unwrap().spectating.unwrap();
    if w != b {
        g.kill(w, None, Cause::Zone, false);
        g.tick(TICK_DT);
        let w2 = g.p(a).unwrap().spectating.unwrap();
        assert!(w2 != w && g.p(w2).unwrap().alive);
    }
    // saved, and read back by a new server
    let again = toon_server::stats::BrStats::load(&dir.to_string_lossy());
    assert_eq!(again.get("Перший").matches, 1);
    assert_eq!(again.get("Перший").best, s.place as u32);
    assert_eq!(again.get("Другий").matches, 0, "B's round is not over");
}

/// Island doors: shut at the start, the use key opens and shuts them (not through a
/// body), shut ones stop you, and bots open the ones they walk up to.
#[test]
fn doors_open_shut_and_block() {
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: std::env::temp_dir()
            .join("tw-br-doors")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    cfg.br.bots = 2;
    cfg.br.max_players = 4;
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Br);
    let n = g.world.doors.len();
    assert!(n > 100, "only {n} doors on the island");
    assert!(g.world.door_open.iter().all(|o| !o), "doors start shut");
    let v = toon_shared::consts::PROTOCOL_VERSION;
    let me = g
        .connect("127.0.0.1:5201".parse().unwrap(), "Гравець", "", v)
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    // (the bots come in on the first ticks)
    for _ in 0..5 {
        g.tick(TICK_DT);
    }
    g.outbox.clear();
    // stand in front of door 0 (outside, a metre away from its middle)
    let d = g.world.doors[0];
    let c = d.centre();
    let size = d.shut[1] - d.shut[0];
    let across = if size.x < size.z {
        glam::vec3(1.0, 0.0, 0.0)
    } else {
        glam::vec3(0.0, 0.0, 1.0)
    };
    let inward = (((d.open[0] + d.open[1]) * 0.5 - c) * across).normalize();
    let outside = glam::vec3(c.x, d.shut[0].y, c.z) - inward * 1.0;
    {
        let p = g.pm(me).unwrap();
        p.alive = true;
        p.in_plane = false;
        p.body.pos = outside;
        p.body.chute = Chute::None;
    }
    // shut: walking in is stopped by the panel
    let w = &g.world;
    let mut b = toon_shared::physics::Body {
        pos: outside,
        ..Default::default()
    };
    let go = toon_shared::physics::MoveInput {
        forward: 1.0,
        yaw: toon_shared::yaw_of(inward),
        ..Default::default()
    };
    for _ in 0..60 {
        b.step(w, &go, &g.tuning.movement, 1.0 / 30.0);
    }
    assert!(
        (b.pos - outside).dot(inward) < 0.9,
        "walked through a shut door"
    );
    // open it, then walk in
    g.handle(me, C2S::Door(0));
    assert!(g.world.door_open[0], "the door did not open");
    let mut b = toon_shared::physics::Body {
        pos: outside,
        ..Default::default()
    };
    for _ in 0..60 {
        b.step(&g.world, &go, &g.tuning.movement, 1.0 / 30.0);
    }
    assert!(
        (b.pos - outside).dot(inward) > 1.5,
        "could not walk in: {}",
        b.pos
    );
    // it cannot be shut on somebody standing in the doorway
    g.pm(me).unwrap().body.pos = glam::vec3(c.x, d.shut[0].y, c.z);
    g.time += 1.0;
    g.handle(me, C2S::Door(0));
    assert!(g.world.door_open[0], "shut on a body");
    // from outside it shuts; from far away nothing happens
    g.pm(me).unwrap().body.pos = outside;
    g.time += 1.0;
    g.handle(me, C2S::Door(0));
    assert!(!g.world.door_open[0]);
    g.pm(me).unwrap().body.pos = outside - inward * 10.0;
    g.time += 1.0;
    g.handle(me, C2S::Door(0));
    assert!(!g.world.door_open[0], "opened from 10 m away");
    // a bot walking up to it opens it
    let bot = g
        .players
        .iter()
        .flatten()
        .find(|p| p.is_bot())
        .map(|p| p.id)
        .unwrap();
    {
        let p = g.pm(bot).unwrap();
        p.alive = true;
        p.in_plane = false;
        p.body.chute = Chute::None;
        p.body.pos = outside;
    }
    g.pm(me).unwrap().alive = false;
    for _ in 0..10 {
        g.tick(TICK_DT);
        g.pm(bot).unwrap().body.pos = outside;
    }
    assert!(g.world.door_open[0], "the bot left the door shut");
}

/// A real player knocked out while their squad's bots fight on stays in the round (and
/// watches only the squad); when the squad is out they get their place and a record.
/// Leaving the game mid-round counts as being knocked out.
#[test]
fn squad_fights_on_and_leavers_are_out() {
    let dir = std::env::temp_dir().join("tw-br-squadon");
    let _ = std::fs::remove_dir_all(&dir);
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: dir.to_string_lossy().into(),
        ..Default::default()
    };
    cfg.br.bots = 8;
    cfg.br.max_players = 8;
    cfg.br.squad = 2;
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Br);
    let v = toon_shared::consts::PROTOCOL_VERSION;
    let a = g
        .connect("127.0.0.1:5401".parse().unwrap(), "Перший", "", v)
        .unwrap();
    let b = g
        .connect("127.0.0.1:5402".parse().unwrap(), "Другий", "", v)
        .unwrap();
    g.handle(a, C2S::JoinTeam(None));
    g.handle(b, C2S::JoinTeam(None));
    for _ in 0..(40.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
        for id in [a, b] {
            if g.br.running && g.p(id).is_some_and(|p| p.in_plane) {
                g.handle(id, C2S::BrJump);
            }
        }
    }
    assert!(g.br.running);
    let squad = g.p(a).unwrap().squad;
    assert!(squad != 0);
    // B leaves mid-round: out, with a place and a box where they stood
    let b_squad = g.p(b).unwrap().squad;
    let boxes = g.br.boxes.len();
    g.remove_player(b, None);
    assert!(g.p(b).is_none());
    assert!(
        g.br.boxes.len() > boxes || b_squad == squad,
        "no box left behind"
    );
    // A is knocked out for good; the bot mate fights on, so the round goes on
    let mate = g
        .players
        .iter()
        .flatten()
        .find(|p| p.squad == squad && p.id != a)
        .map(|p| p.id);
    g.kill(a, None, Cause::Zone, false);
    for _ in 0..5 {
        g.tick(TICK_DT);
    }
    if let Some(m) = mate.filter(|m| g.p(*m).is_some_and(|p| p.alive)) {
        assert_eq!(g.state, MatchState::Playing, "the squad still fights");
        assert_eq!(g.p(a).unwrap().br_place, 0);
        assert_eq!(
            g.p(a).unwrap().spectating,
            Some(m),
            "watching the squadmate"
        );
        // (and nobody else, however often they ask)
        for _ in 0..5 {
            g.handle(a, C2S::Spectate(0));
            assert_eq!(g.p(a).unwrap().spectating, Some(m));
        }
        // the mate falls too: the squad has its place, A a summary and a record
        g.kill(m, None, Cause::Zone, false);
        g.tick(TICK_DT);
    }
    let p = g.p(a).unwrap();
    assert!(p.br_place > 1, "place {}", p.br_place);
    assert!(g.snapshot_for(a).br.unwrap().summary.is_some());
    assert_eq!(g.br_stats.get("Перший").matches, 1);
    assert_eq!(g.state, MatchState::PostMatch, "no real player left");
}

/// What lies on the ground is not in the snapshots: a client is told once what is near,
/// then only what changes (taken things go, new ones come).
#[test]
fn lying_things_are_sent_as_changes() {
    use toon_shared::protocol::EntSnap;
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: std::env::temp_dir()
            .join("tw-br-lying")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    cfg.br.bots = 2;
    cfg.br.max_players = 4;
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Br);
    let v = toon_shared::consts::PROTOCOL_VERSION;
    let me = g
        .connect("127.0.0.1:5601".parse().unwrap(), "Гравець", "", v)
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    for _ in 0..(40.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
        if g.br.running {
            break;
        }
    }
    assert!(g.br.running && g.br.loot.len() > 200);
    // on the ground by some loot
    let at = g.br.loot[0].pos;
    {
        let p = g.pm(me).unwrap();
        p.in_plane = false;
        p.alive = true;
        p.body.chute = Chute::None;
        p.body.pos = at;
    }
    // what the client knows, built from the messages as a client would
    let mut known: std::collections::HashMap<u16, EntSnap> = Default::default();
    let mut msgs = 0;
    let mut pump = |g: &mut Game, known: &mut std::collections::HashMap<u16, EntSnap>| {
        let mut n = 0;
        for _ in 0..6 {
            g.tick(TICK_DT);
            for (_, m, reliable) in g.outbox.drain(..) {
                if let S2C::Ents { add, remove } = m {
                    assert!(reliable);
                    n += 1;
                    for id in remove {
                        assert!(
                            known.remove(&id).is_some(),
                            "told to forget an unknown thing"
                        );
                    }
                    for e in add {
                        known.insert(e.id(), e);
                    }
                }
            }
            g.shots.clear();
        }
        n
    };
    msgs += pump(&mut g, &mut known);
    assert!(msgs > 0 && !known.is_empty());
    let near = g.lying_near(me);
    assert_eq!(known.len(), near.len());
    assert!(near.iter().all(|e| known.get(&e.id()) == Some(e)));
    assert!(known.len() < g.br.loot.len(), "only what is near");
    // nothing in the snapshots, and nothing more to say while nothing changes
    let snap = g.snapshot_for(me);
    assert!(!snap.ents.iter().any(|e| e.lies_still()));
    let size = toon_shared::net::encode(&S2C::Snapshot(Box::new(snap))).len();
    assert!(size < 1100, "snapshot of {size} bytes");
    assert_eq!(pump(&mut g, &mut known), 0, "repeats itself");
    // a thing is taken: it goes; walking away, the rest goes too
    let first = g.br.loot[0].id;
    g.br.loot.remove(0);
    assert!(pump(&mut g, &mut known) > 0);
    assert!(!known.contains_key(&first));
    g.pm(me).unwrap().body.pos = at + glam::vec3(0.0, 0.0, 900.0);
    pump(&mut g, &mut known);
    let near = g.lying_near(me);
    assert_eq!(known.len(), near.len());
    assert!(near.iter().all(|e| known.contains_key(&e.id())));
}
