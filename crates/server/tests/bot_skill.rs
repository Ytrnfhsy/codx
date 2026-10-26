//! Bot difficulty must scale: weak bots miss more and notice enemies later; nobody sees behind.

use glam::vec3;
use toon_server::config::ServerConfig;
use toon_server::game::{Dest, Game};
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::protocol::{MatchState, C2S, S2C};
use toon_shared::rng::Rng;
use toon_shared::weapons::DEFAULT_CLASSES;
use toon_shared::{Mode, Team};

fn game(skill: f32, bots: usize, seed: u64, protection: f32) -> (Game, u8) {
    let cfg = ServerConfig {
        bots,
        bot_skill: skill,
        spawn_protection: protection,
        data_dir: std::env::temp_dir().join("tw-skill").to_string_lossy().into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    g.rng = Rng::new(seed);
    g.change_map(MapId::Depot, Mode::Tdm);
    let id = g.connect("127.0.0.1:5000".parse().unwrap(), "Ціль", "", toon_shared::consts::PROTOCOL_VERSION).unwrap();
    g.handle(id, C2S::JoinTeam(Some(Team::Allies)));
    g.handle(id, C2S::SetLoadout(DEFAULT_CLASSES[0].1));
    (g, id)
}

fn wait_playing(g: &mut Game) {
    for _ in 0..(20.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
        if g.state == MatchState::Playing {
            return;
        }
    }
    panic!("match did not start");
}

/// Duel: one bot with an M4 shoots at a strafing enemy `dist` m away for 8 s. Returns hit ratio.
fn duel_accuracy(skill: f32, seed: u64, dist: f32) -> (u32, u32) {
    let (mut g, human) = game(skill, 2, seed, 0.0);
    wait_playing(&mut g);
    let bot = g.players.iter().flatten().find(|p| p.is_bot()).unwrap().id;
    let m4 = DEFAULT_CLASSES[0].1;
    if let Some(b) = g.pm(bot) {
        b.loadout = m4;
        b.weapons[0].id = m4.primary;
        b.weapons[0].mag = 30;
        b.weapons[0].reserve = 900;
        b.slot = 0;
    }
    for k in 0..(8.0 / TICK_DT) as usize {
        let t = k as f32 * TICK_DT;
        if let Some(h) = g.pm(human) {
            // strafes left/right like a real player
            let x = (t * 1.6).sin() * 2.5;
            h.body.pos = vec3(x, 0.0, 18.0 - dist);
            h.body.vel = vec3((t * 1.6).cos() * 4.0, 0.0, 0.0);
            h.hp = 1000.0;
            h.alive = true;
        }
        if let Some(b) = g.pm(bot) {
            b.body.pos = vec3(0.0, 0.0, 18.0);
            b.body.vel = glam::Vec3::ZERO;
        }
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
    }
    let b = g.p(bot).unwrap();
    (b.shots_fired, b.shots_hit)
}

fn accuracy(skill: f32, dist: f32) -> f32 {
    let (f, h) = (0..10).map(|s| duel_accuracy(skill, 40 + s, dist)).fold((0, 0), |a, x| (a.0 + x.0, a.1 + x.1));
    assert!(f > 20, "bot barely shot ({f}) at skill {skill}");
    h as f32 / f as f32
}

#[test]
fn accuracy_scales_with_skill() {
    for dist in [10.0, 25.0] {
        let easy = accuracy(0.0, dist);
        let normal = accuracy(0.4, dist);
        let hard = accuracy(0.7, dist);
        let veteran = accuracy(1.0, dist);
        println!("{dist} m: easy={easy:.2} normal={normal:.2} hard={hard:.2} veteran={veteran:.2}");
        assert!(easy < normal && normal < hard && hard < veteran + 0.08, "{dist} m: {easy} {normal} {hard} {veteran}");
        assert!(easy < 0.2, "easy bots hit {easy} at {dist} m");
        assert!(veteran > 0.4, "veteran bots hit only {veteran} at {dist} m");
    }
}

/// Seconds until a single bot targets / first damages a still enemy `dist` metres in front.
fn engage_time(skill: f32, seed: u64, dist: f32) -> (f32, f32) {
    let (mut g, human) = game(skill, 2, seed, 0.0);
    wait_playing(&mut g);
    let bot = g.players.iter().flatten().find(|p| p.is_bot()).unwrap().id;
    let start = g.time;
    let (mut noticed, mut hurt) = (None, None);
    for _ in 0..(10.0 / TICK_DT) as usize {
        if let Some(h) = g.pm(human) {
            h.body.pos = vec3(0.0, 0.0, 18.0 - dist);
            h.body.vel = glam::Vec3::ZERO;
            h.hp = 100.0;
        }
        if noticed.is_none() {
            if let Some(b) = g.pm(bot) {
                b.body.pos = vec3(0.0, 0.0, 18.0);
                b.yaw = 0.0; // looking towards -z, straight at the enemy
                b.pitch = 0.0;
            }
        }
        g.tick(TICK_DT);
        g.shots.clear();
        for (d, m, _) in g.outbox.drain(..) {
            if matches!((d, m), (Dest::One(t), S2C::Damaged { .. }) if t == human) && hurt.is_none() {
                hurt = Some((g.time - start) as f32);
            }
        }
        if noticed.is_none() && g.p(bot).and_then(|b| b.bot.as_ref()).and_then(|b| b.target()) == Some(human) {
            noticed = Some((g.time - start) as f32);
        }
        if hurt.is_some() {
            break;
        }
    }
    (noticed.unwrap_or(10.0), hurt.unwrap_or(10.0))
}

#[test]
fn harder_bots_notice_and_shoot_sooner() {
    let avg = |skill: f32| {
        let v: Vec<(f32, f32)> = (0..6).map(|s| engage_time(skill, 100 + s, 20.0)).collect();
        (v.iter().map(|x| x.0).sum::<f32>() / 6.0, v.iter().map(|x| x.1).sum::<f32>() / 6.0)
    };
    let (easy_n, easy_h) = avg(0.0);
    let (hard_n, hard_h) = avg(1.0);
    println!("20 m: easy notice {easy_n:.2}s first hit {easy_h:.2}s | veteran notice {hard_n:.2}s first hit {hard_h:.2}s");
    assert!(easy_n > 1.0, "easy bots spot too fast: {easy_n}");
    assert!(hard_n < 0.6, "veteran bots too slow: {hard_n}");
    assert!(easy_h > hard_h + 0.8);
}

#[test]
fn bots_do_not_see_behind_them() {
    for skill in [0.0, 1.0] {
        let (mut g, human) = game(skill, 2, 5, 0.0);
        wait_playing(&mut g);
        let bot = g.players.iter().flatten().find(|p| p.is_bot()).unwrap().id;
        for _ in 0..(3.0 / TICK_DT) as usize {
            if let Some(h) = g.pm(human) {
                h.body.pos = vec3(0.0, 0.0, 26.0); // 8 m behind the bot, standing still and silent
                h.body.vel = glam::Vec3::ZERO;
            }
            if let Some(b) = g.pm(bot) {
                b.body.pos = vec3(0.0, 0.0, 18.0);
                b.yaw = 0.0;
            }
            g.tick(TICK_DT);
            g.outbox.clear();
            g.shots.clear();
            assert_ne!(g.p(bot).unwrap().bot.as_ref().unwrap().target(), Some(human), "skill {skill}: saw an enemy behind its back");
        }
    }
}

#[test]
fn spawn_protection_rules() {
    let (mut g, human) = game(0.5, 2, 9, 3.0);
    wait_playing(&mut g);
    g.kill(human, None, toon_shared::weapons::Cause::World, false);
    for _ in 0..(10.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.handle(human, C2S::Spawn);
        if g.p(human).unwrap().alive {
            break;
        }
    }
    assert!(g.p(human).unwrap().alive);
    let pos = g.p(human).unwrap().body.pos;
    // invulnerable
    g.damage(human, None, 500.0, toon_shared::weapons::Cause::Grenade, false, pos);
    assert!(g.p(human).unwrap().alive, "protected player died");
    assert!(g.snapshot_for(human).me.protected > 2.0);
    // ...but cannot attack while protected
    for _ in 0..20 {
        g.tick(TICK_DT); // weapon raise after spawning
    }
    let (w, eye, mag) = {
        let p = g.p(human).unwrap();
        (p.weapon().id, p.eye(), p.weapon().mag)
    };
    g.fire(human, w, eye, &[glam::Vec3::NEG_Z]);
    g.throw_grenade(human, eye, glam::Vec3::NEG_Z, 0.0);
    let p = g.p(human).unwrap();
    assert_eq!(p.weapon().mag, mag, "a protected player fired");
    assert_eq!(p.shots_fired, 0);
    assert!(g.grenades.is_empty(), "a protected player threw a grenade");
    // the shield is visible to teammates only
    use toon_shared::protocol::stance_bits::PROTECTED;
    let bot = g.players.iter().flatten().find(|p| p.is_bot()).unwrap().id;
    let enemy_view = g.snapshot_for(bot);
    let me_seen = enemy_view.players.iter().find(|p| p.id == human).unwrap();
    assert_eq!(me_seen.stance & PROTECTED, 0, "enemies must not see the shield");
    let own_view = g.snapshot_for(human);
    assert_ne!(own_view.players.iter().find(|p| p.id == human).unwrap().stance & PROTECTED, 0);
    // giving the protection up makes us vulnerable and able to shoot
    g.handle(human, C2S::DropProtection);
    for _ in 0..10 {
        g.tick(TICK_DT);
    }
    let eye = g.p(human).unwrap().eye();
    g.fire(human, w, eye, &[glam::Vec3::NEG_Z]);
    assert_eq!(g.p(human).unwrap().weapon().mag, mag - 1);
    g.damage(human, None, 500.0, toon_shared::weapons::Cause::Grenade, false, pos);
    assert!(!g.p(human).unwrap().alive, "protection should be gone");
}
