//! Killstreaks: rewards at 3…21 kills, artillery, the FPV drone, the bomber drone and the
//! ground robot (piloted and autonomous).

use glam::{vec3, Vec3};
use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::TICK_DT;
use toon_shared::look_dir;
use toon_shared::map::MapId;
use toon_shared::protocol::{MatchState, C2S};
use toon_shared::vehicles::{keys, VehicleKind, BOMBER_ALT};
use toon_shared::weapons::{Cause, Killstreak, DEFAULT_CLASSES};
use toon_shared::{Mode, Team};

const ME_AT: Vec3 = Vec3::new(-18.0, 0.0, 0.0);
const FOE_AT: Vec3 = Vec3::new(-10.0, 0.0, 0.0);

/// A Depot match with two players facing each other on open ground.
fn duel() -> (Game, u8, u8) {
    let cfg = ServerConfig {
        bots: 0,
        data_dir: std::env::temp_dir()
            .join("tw-test-streaks")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    g.rng = toon_shared::rng::Rng::new(99);
    g.change_map(MapId::Depot, Mode::Tdm);
    let mut ids = Vec::new();
    for (k, team) in [Team::Allies, Team::Axis].into_iter().enumerate() {
        let id = g
            .connect(
                format!("127.0.0.1:{}", 6000 + k).parse().unwrap(),
                ["Я", "Ворог"][k],
                "",
                toon_shared::consts::PROTOCOL_VERSION,
            )
            .unwrap();
        g.handle(id, C2S::JoinTeam(Some(team)));
        g.handle(id, C2S::SetLoadout(DEFAULT_CLASSES[0].1));
        ids.push(id);
    }
    for _ in 0..1200 {
        tick(&mut g, 0.5);
        if g.state == MatchState::Playing {
            break;
        }
    }
    assert_eq!(g.state, MatchState::Playing);
    let (me, foe) = (ids[0], ids[1]);
    place(&mut g, me, ME_AT);
    place(&mut g, foe, FOE_AT);
    (g, me, foe)
}

fn place(g: &mut Game, id: u8, at: Vec3) {
    let p = g.pm(id).unwrap();
    p.alive = true;
    p.hp = 100.0;
    p.protected_until = 0.0;
    p.body.pos = at;
    p.body.vel = Vec3::ZERO;
}

fn tick(g: &mut Game, secs: f32) {
    for _ in 0..(secs / TICK_DT).round() as usize {
        g.tick(TICK_DT);
        let _ = g.snapshot_for(0);
        g.shots.clear();
        g.outbox.clear();
    }
}

fn give(g: &mut Game, id: u8, k: Killstreak) {
    let p = g.pm(id).unwrap();
    p.rewards.retain(|r| *r != k);
    p.rewards.push(k);
}

fn deaths(g: &Game, id: u8) -> u16 {
    g.p(id).unwrap().deaths
}

#[test]
fn rewards_come_every_three_kills() {
    let (mut g, me, foe) = duel();
    let mut got = Vec::new();
    for n in 1..=21 {
        place(&mut g, foe, FOE_AT);
        g.kill(foe, Some(me), Cause::World, false);
        if let Some(k) = Killstreak::for_streak(n, &g.tuning.streaks) {
            got.push(k);
        }
        assert_eq!(g.p(me).unwrap().streak, n);
    }
    assert_eq!(got, Killstreak::ALL.to_vec());
    assert_eq!(g.p(me).unwrap().rewards, Killstreak::ALL.to_vec());
    // and round again: 24 kills is the radar, 27 the airstrike
    g.pm(me).unwrap().rewards.clear();
    for _ in 22..=27 {
        place(&mut g, foe, FOE_AT);
        g.kill(foe, Some(me), Cause::World, false);
    }
    assert_eq!(
        g.p(me).unwrap().rewards,
        vec![Killstreak::Uav, Killstreak::Airstrike]
    );
}

#[test]
fn next_reward_goes_round() {
    let d = toon_shared::tuning::StreakTuning::default();
    assert_eq!(Killstreak::next_after(0, &d), (Killstreak::Uav, 3));
    assert_eq!(Killstreak::next_after(20, &d), (Killstreak::Nrk, 1));
    assert_eq!(Killstreak::next_after(21, &d), (Killstreak::Uav, 3));
    assert_eq!(Killstreak::next_after(25, &d), (Killstreak::Airstrike, 2));
    assert_eq!(Killstreak::for_streak(42, &d), Some(Killstreak::Nrk));
    assert_eq!(Killstreak::for_streak(43, &d), None);
}

#[test]
fn artillery_shells_the_target() {
    let (mut g, me, foe) = duel();
    give(&mut g, me, Killstreak::Artillery);
    g.handle(
        me,
        C2S::UseKillstreak {
            target: Some([FOE_AT.x, FOE_AT.z]),
            pilot: false,
        },
    );
    assert!(g.p(me).unwrap().rewards.is_empty(), "used up");
    let before = deaths(&g, foe);
    for _ in 0..90 {
        place_if_alive(&mut g, foe, FOE_AT);
        tick(&mut g, 0.1);
    }
    assert!(deaths(&g, foe) > before, "the barrage hit");
}

fn place_if_alive(g: &mut Game, id: u8, at: Vec3) {
    if let Some(p) = g.pm(id) {
        if p.alive {
            p.body.pos = at;
        }
    }
}

/// Flies the pilot's vehicle towards `goal` like a client would, holding `held` keys.
fn drive_towards(g: &mut Game, me: u8, goal: Vec3, held: u8, secs: f32) {
    let steps = (secs / TICK_DT) as usize;
    for _ in 0..steps {
        let Some(v) = g.vehicle_hud(me) else { return };
        let pos = Vec3::from_array(v.pos);
        let speed = v.kind.max_speed(&g.tuning.streaks) * 0.8;
        let d = goal - pos;
        let flat = vec3(d.x, if v.kind == VehicleKind::Fpv { d.y } else { 0.0 }, d.z);
        let step = if flat.length() > speed * TICK_DT {
            flat.normalize() * speed * TICK_DT
        } else {
            flat
        };
        let yaw = toon_shared::yaw_of(d);
        let pitch = (d.y / d.length().max(0.1)).asin();
        g.handle(
            me,
            C2S::Drive {
                pos: (pos + step).to_array(),
                yaw,
                pitch,
                keys: held,
            },
        );
        g.tick(TICK_DT);
        let _ = g.snapshot_for(0);
        g.shots.clear();
        g.outbox.clear();
    }
}

#[test]
fn fpv_drone_flies_into_the_enemy() {
    let (mut g, me, foe) = duel();
    give(&mut g, me, Killstreak::Fpv);
    g.handle(
        me,
        C2S::UseKillstreak {
            target: None,
            pilot: true,
        },
    );
    assert!(g.p(me).unwrap().driving.is_some(), "piloting");
    let before = deaths(&g, foe);
    drive_towards(&mut g, me, FOE_AT + vec3(0.0, 1.2, 0.0), 0, 6.0);
    assert!(g.p(me).unwrap().driving.is_none(), "it blew up");
    assert!(g.vehicles.is_empty());
    assert!(deaths(&g, foe) > before, "the enemy is down");
    assert!(g.p(me).unwrap().alive, "the pilot stood far enough away");
}

#[test]
fn fpv_cannot_fly_through_walls() {
    let (mut g, me, _) = duel();
    give(&mut g, me, Killstreak::Fpv);
    g.handle(
        me,
        C2S::UseKillstreak {
            target: None,
            pilot: true,
        },
    );
    let v = g.vehicle_hud(me).unwrap();
    let start = Vec3::from_array(v.pos);
    assert!(start.y > 2.0, "it takes off above the head");
    // it hovers until the pilot moves it
    tick(&mut g, 1.0);
    assert_eq!(Vec3::from_array(g.vehicle_hud(me).unwrap().pos), start);
    // straight down: it stops on the ground instead of going through
    let below = start - vec3(0.0, 10.0, 0.0);
    drive_towards(&mut g, me, below, 0, 1.0);
    let v = g
        .vehicle_hud(me)
        .expect("bumping the ground does not end it");
    assert!(v.pos[1] > 0.2 && v.pos[1] < 1.0, "{:?}", v.pos);
    // the pilot's client reports the crash: it blows up there
    drive_towards(&mut g, me, below, keys::CRASH, 0.1);
    assert!(g.vehicles.is_empty(), "it blew up");
}

#[test]
fn fpv_breaks_without_a_blast_before_it_is_armed() {
    let (mut g, me, _) = duel();
    give(&mut g, me, Killstreak::Fpv);
    g.handle(
        me,
        C2S::UseKillstreak {
            target: None,
            pilot: true,
        },
    );
    let hp = g.p(me).unwrap().hp;
    let pos = g.vehicle_hud(me).unwrap().pos;
    g.handle(
        me,
        C2S::Drive {
            pos,
            yaw: 0.0,
            pitch: 0.0,
            keys: keys::CRASH,
        },
    );
    tick(&mut g, 0.2);
    assert!(g.vehicles.is_empty(), "it broke");
    assert!(g.p(me).unwrap().driving.is_none());
    assert_eq!(g.p(me).unwrap().hp, hp, "no blast");
}

#[test]
fn bomber_drops_bombs_on_the_enemy() {
    let (mut g, me, foe) = duel();
    give(&mut g, me, Killstreak::Bomber);
    g.handle(
        me,
        C2S::UseKillstreak {
            target: None,
            pilot: true,
        },
    );
    let v = g.vehicle_hud(me).unwrap();
    assert_eq!(v.pos[1], BOMBER_ALT);
    assert_eq!(v.ammo, 6);
    let before = deaths(&g, foe);
    // get over the enemy, then drop while hovering
    drive_towards(&mut g, me, vec3(FOE_AT.x, BOMBER_ALT, FOE_AT.z), 0, 3.0);
    drive_towards(
        &mut g,
        me,
        vec3(FOE_AT.x, BOMBER_ALT, FOE_AT.z),
        keys::FIRE,
        4.0,
    );
    assert!(deaths(&g, foe) > before, "a bomb landed on the enemy");
    let ammo = g.vehicle_hud(me).map(|v| v.ammo).unwrap_or(0);
    assert!(ammo < 6, "bombs were used");
}

#[test]
fn piloted_robot_guns_down_the_enemy() {
    let (mut g, me, foe) = duel();
    give(&mut g, me, Killstreak::Nrk);
    g.handle(
        me,
        C2S::UseKillstreak {
            target: None,
            pilot: true,
        },
    );
    assert!(g.p(me).unwrap().driving.is_some());
    let before = deaths(&g, foe);
    for _ in 0..90 {
        let Some(v) = g.vehicle_hud(me) else { break };
        let gun = Vec3::from_array(v.pos) + vec3(0.0, 1.05, 0.0);
        let to = FOE_AT + vec3(0.0, 1.0, 0.0) - gun;
        place_if_alive(&mut g, foe, FOE_AT);
        g.handle(
            me,
            C2S::Drive {
                pos: v.pos,
                yaw: toon_shared::yaw_of(to),
                pitch: (to.y / to.length()).asin(),
                keys: keys::FIRE,
            },
        );
        tick(&mut g, TICK_DT);
        if deaths(&g, foe) > before {
            break;
        }
    }
    assert!(deaths(&g, foe) > before, "the machine gun hit");
}

#[test]
fn autonomous_robot_finds_and_shoots_the_enemy() {
    let (mut g, me, foe) = duel();
    give(&mut g, me, Killstreak::Nrk);
    g.handle(
        me,
        C2S::UseKillstreak {
            target: None,
            pilot: false,
        },
    );
    assert!(g.p(me).unwrap().driving.is_none(), "nobody steers it");
    assert_eq!(g.vehicles.len(), 1);
    assert!(g.vehicles[0].auto);
    let before = deaths(&g, foe);
    for _ in 0..80 {
        place_if_alive(&mut g, foe, FOE_AT);
        tick(&mut g, 0.1);
        if deaths(&g, foe) > before {
            break;
        }
    }
    assert!(deaths(&g, foe) > before, "it shot the enemy on its own");
}

#[test]
fn vehicles_end_with_their_pilot_and_can_be_destroyed() {
    let (mut g, me, foe) = duel();
    give(&mut g, me, Killstreak::Bomber);
    g.handle(
        me,
        C2S::UseKillstreak {
            target: None,
            pilot: true,
        },
    );
    g.kill(me, Some(foe), Cause::World, false);
    tick(&mut g, 0.1);
    assert!(g.vehicles.is_empty(), "the drone falls with its pilot");

    place(&mut g, me, ME_AT);
    give(&mut g, me, Killstreak::Nrk);
    g.handle(
        me,
        C2S::UseKillstreak {
            target: None,
            pilot: false,
        },
    );
    let at = g.vehicles[0].pos;
    // grenades hurt it, enough of them destroy it
    for _ in 0..8 {
        g.explode(at, 6.0, 150.0, Some(foe), Cause::Grenade, false);
    }
    tick(&mut g, 0.1);
    assert!(g.vehicles.is_empty(), "the robot was destroyed");
    let _ = look_dir(0.0, 0.0);
}

#[test]
fn own_killstreaks_do_not_hurt_the_pilot() {
    let (mut g, me, _) = duel();
    let hp = g.p(me).unwrap().hp;
    let at = g.p(me).unwrap().body.pos;
    for cause in [Cause::Drop, Cause::Artillery, Cause::Fpv] {
        g.explode(at, 6.0, 300.0, Some(me), cause, false);
    }
    assert!(g.p(me).unwrap().alive);
    assert_eq!(g.p(me).unwrap().hp, hp);
    // a grenade of your own still hurts
    g.explode(at, 6.0, 300.0, Some(me), Cause::Grenade, false);
    assert!(!g.p(me).unwrap().alive || g.p(me).unwrap().hp < hp);
}

#[test]
fn helicopter_guns_down_enemies_in_the_open() {
    let (mut g, me, foe) = duel();
    give(&mut g, me, Killstreak::Helicopter);
    g.handle(
        me,
        C2S::UseKillstreak {
            target: None,
            pilot: false,
        },
    );
    assert!(!g.helis.is_empty(), "the helicopter is up");
    let before = deaths(&g, foe);
    // keep the enemy standing in the open under its circle
    let c = g.map.center();
    let spot = vec3(c.x, 0.0, c.y);
    for _ in 0..200 {
        place_if_alive(&mut g, foe, spot);
        tick(&mut g, 0.1);
        if deaths(&g, foe) > before {
            break;
        }
    }
    assert!(deaths(&g, foe) > before, "the door gun killed the enemy");
}

#[test]
fn killstreaks_follow_the_tuning() {
    let (mut g, me, foe) = duel();
    // the radar after 2 kills instead of 3, and a weaker, quicker artillery
    let mut t = g.tuning.clone();
    t.streaks.uav_kills = 2;
    t.streaks.artillery_shells = 2;
    g.set_tuning(t);
    for _ in 0..2 {
        place(&mut g, foe, FOE_AT);
        g.kill(foe, Some(me), Cause::World, false);
    }
    assert_eq!(g.p(me).unwrap().rewards, vec![Killstreak::Uav]);
    give(&mut g, me, Killstreak::Artillery);
    let before = g.blasts.len();
    g.handle(
        me,
        C2S::UseKillstreak {
            target: Some([FOE_AT.x, FOE_AT.z]),
            pilot: false,
        },
    );
    assert_eq!(g.blasts.len() - before, 2, "two shells");
}
