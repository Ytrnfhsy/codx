//! Bullets take time to fly and drop on the way: a far target is hit only by aiming
//! above it, and the damage lands when the bullet arrives, not when it is fired.

use glam::{vec3, Vec3};
use toon_server::config::ServerConfig;
use toon_server::game::{Game, WState};
use toon_shared::ballistics::{lead, Ballistics};
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::protocol::{MatchState, C2S};
use toon_shared::weapons::WeaponId;
use toon_shared::{yaw_of, Mode, Team};

fn setup() -> (Game, u8, u8) {
    let cfg = ServerConfig {
        // (tests start straight away: nobody presses «Ready»)
        ready_check: false,
        bots: 0,
        data_dir: std::env::temp_dir()
            .join("tw-ballistics")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Tdm);
    let join = |g: &mut Game, port: u16, team: Team| {
        let id = g
            .connect(
                format!("127.0.0.1:{port}").parse().unwrap(),
                "Стрілець",
                "",
                toon_shared::consts::PROTOCOL_VERSION,
            )
            .unwrap();
        g.handle(id, C2S::JoinTeam(Some(team)));
        id
    };
    let a = join(&mut g, 5101, Team::Allies);
    let b = join(&mut g, 5102, Team::Axis);
    for _ in 0..(30.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
        if g.state == MatchState::Playing && g.p(a).is_some_and(|p| p.alive) {
            break;
        }
    }
    assert_eq!(g.state, MatchState::Playing);
    (g, a, b)
}

/// Puts both high in the sky (nothing in the way), 450 m apart.
fn place(g: &mut Game, a: u8, b: u8) -> (Vec3, Vec3) {
    let t = g.time;
    for (id, pos) in [(a, vec3(0.0, 300.0, 0.0)), (b, vec3(450.0, 300.0, 0.0))] {
        let p = g.pm(id).unwrap();
        p.alive = true;
        p.hp = 100.0;
        p.protected_until = 0.0;
        p.body.pos = pos;
        p.body.vel = Vec3::ZERO;
        p.history.clear();
        p.slot = 0;
        p.weapons[0] = WState {
            id: WeaponId::M40,
            mag: 5,
            reserve: 10,
        };
        p.reload_end = None;
        p.switch_end = t - 1.0;
        p.last_sprint = -100.0;
        p.shot_budget = 3.0;
    }
    let eye = g.p(a).unwrap().eye();
    let chest = g.p(b).unwrap().chest();
    (eye, chest)
}

fn shoot(g: &mut Game, a: u8, eye: Vec3, at: Vec3) {
    let d = (at - eye).normalize();
    let p = g.pm(a).unwrap();
    p.yaw = yaw_of(d);
    p.pitch = d.y.asin();
    g.handle(
        a,
        C2S::Fire {
            weapon: WeaponId::M40,
            origin: eye.to_array(),
            dirs: vec![d.to_array()],
        },
    );
}

fn run(g: &mut Game, secs: f32, b: u8) -> Vec<(f32, f32)> {
    let mut hp = Vec::new();
    let t0 = g.time;
    for _ in 0..(secs / TICK_DT) as usize {
        // (the target hangs in the air: keep it there)
        if let Some(p) = g.pm(b) {
            p.body.vel = Vec3::ZERO;
        }
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
        hp.push(((g.time - t0) as f32, g.p(b).map_or(0.0, |p| p.hp)));
    }
    hp
}

#[test]
fn a_far_shot_needs_holdover_and_arrives_late() {
    let (mut g, a, b) = setup();
    let speed = Ballistics::of(WeaponId::M40).velocity;

    // straight at the chest: the bullet drops ~1.4 m over 450 m and passes under the feet
    let (eye, chest) = place(&mut g, a, b);
    shoot(&mut g, a, eye, chest);
    let hp = run(&mut g, 1.0, b);
    assert_eq!(hp.last().unwrap().1, 100.0, "a flat shot should fall short");

    // held over by the drop: a hit, but only once the bullet got there
    let (eye, chest) = place(&mut g, a, b);
    let (aim, flight) = lead(eye, chest, Vec3::ZERO, speed);
    assert!(flight > 0.5 && flight < 0.56, "flight {flight}");
    shoot(&mut g, a, eye, aim);
    assert_eq!(g.p(b).unwrap().hp, 100.0, "hit before the bullet arrived");
    let hp = run(&mut g, 1.0, b);
    let when = hp
        .iter()
        .find(|h| h.1 < 100.0)
        .map(|h| h.0)
        .expect("the held-over shot missed");
    assert!(
        (when - flight).abs() < 0.08,
        "damage at {when:.2} s, flight {flight:.2} s"
    );
}

/// A good bot with a sniper rifle opens fire far beyond its old sight range and, leading a
/// target that runs sideways and holding over for the drop, hits it.
#[test]
fn a_bot_leads_a_far_runner() {
    let cfg = ServerConfig {
        // (tests start straight away: nobody presses «Ready»)
        ready_check: false,
        // (bots fill up to this many players, us included)
        bots: 2,
        bot_skill: 1.0,
        data_dir: std::env::temp_dir()
            .join("tw-ballistics2")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    // (the same aim errors every run; seeds 1..8 all give 5..11 hits of 12..14 shots)
    g.rng = toon_shared::rng::Rng::new(1);
    g.change_map(MapId::Island, Mode::Tdm);
    let me = g
        .connect(
            "127.0.0.1:5103".parse().unwrap(),
            "Мішень",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(Some(Team::Axis)));
    for _ in 0..(30.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
        if g.state == MatchState::Playing && g.p(me).is_some_and(|p| p.alive) {
            break;
        }
    }
    let bot = g
        .players
        .iter()
        .flatten()
        .find(|p| p.is_bot())
        .map(|p| p.id)
        .unwrap();
    assert_ne!(g.p(bot).unwrap().team, Team::Axis);
    let from = vec3(0.0, 300.0, 0.0);
    let run_vel = vec3(0.0, 0.0, 5.5);
    let mut target = vec3(250.0, 300.0, -30.0);
    {
        let p = g.pm(bot).unwrap();
        p.body.pos = from;
        p.slot = 0;
        p.weapons[0] = WState {
            id: WeaponId::M40,
            mag: 5,
            reserve: 100,
        };
        p.yaw = yaw_of(target - from);
        p.protected_until = 0.0;
    }
    // (the runner soaks every hit, so that the bot keeps shooting at it)
    const TOUGH: f32 = 100_000.0;
    g.pm(me).unwrap().hp = TOUGH;
    let mut hits = 0;
    let mut shots = 0;
    for k in 0..(25.0 / TICK_DT) as usize {
        // the runner goes back and forth along a line; the bot hangs in the air
        if (k / 300) % 2 == 1 {
            target -= run_vel * TICK_DT;
        } else {
            target += run_vel * TICK_DT;
        }
        let dir = if (k / 300) % 2 == 1 {
            -run_vel
        } else {
            run_vel
        };
        for (id, pos, vel) in [(bot, from, Vec3::ZERO), (me, target, dir)] {
            let p = g.pm(id).unwrap();
            p.alive = true;
            p.body.pos = pos;
            p.body.vel = vel;
            p.protected_until = 0.0;
        }
        // (it looks our way: noticing someone behind you is another matter)
        let p = g.pm(bot).unwrap();
        if p.bot.as_ref().unwrap().target().is_none() {
            p.yaw = yaw_of(target - from);
        }
        g.tick(TICK_DT);
        g.outbox.clear();
        shots += g.shots.iter().filter(|s| s.id == bot).count();
        g.shots.clear();
        let p = g.pm(me).unwrap();
        if p.hp < TOUGH {
            hits += 1;
            p.hp = TOUGH;
        }
    }
    eprintln!("{hits} hits of {shots} shots");
    assert!(
        shots >= 6,
        "the bot fired only {shots} shots at a far runner"
    );
    assert!(
        hits * 3 >= shots,
        "the bot hit a far runner only {hits} times of {shots}"
    );
}
