//! A powerful round goes on through a body into the next one, weaker each time (more so
//! through a vest); quick kills one after another make a double kill, a triple…

use glam::{vec3, Vec3};
use toon_server::config::ServerConfig;
use toon_server::game::{Game, WState};
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::protocol::{MatchState, C2S};
use toon_shared::weapons::{Cause, WeaponId};
use toon_shared::{yaw_of, Mode, Team};

fn setup(n: usize) -> (Game, u8, Vec<u8>) {
    let cfg = ServerConfig {
        ready_check: false,
        bots: 0,
        data_dir: std::env::temp_dir()
            .join("tw-pierce")
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
                "Гравець",
                "",
                toon_shared::consts::PROTOCOL_VERSION,
            )
            .unwrap();
        g.handle(id, C2S::JoinTeam(Some(team)));
        id
    };
    let a = join(&mut g, 5201, Team::Allies);
    let others: Vec<u8> = (0..n)
        .map(|k| join(&mut g, 5202 + k as u16, Team::Axis))
        .collect();
    // (the team balance would put some with the shooter)
    for &id in &others {
        g.pm(id).unwrap().team = Team::Axis;
    }
    for _ in 0..(30.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
        if g.state == MatchState::Playing && g.p(a).is_some_and(|p| p.alive) {
            break;
        }
    }
    assert_eq!(g.state, MatchState::Playing);
    (g, a, others)
}

/// The shooter high in the sky, the others in a row in front of him, 1.5 m apart; one
/// shot straight through their chests. Returns the health each lost.
fn row_shot(weapon: WeaponId, vest: u8) -> Vec<f32> {
    let (mut g, a, row) = setup(3);
    let t = g.time;
    let place = |g: &mut Game, id: u8, pos: Vec3| {
        let p = g.pm(id).unwrap();
        p.alive = true;
        p.hp = 1000.0;
        p.protected_until = 0.0;
        p.body.pos = pos;
        p.body.vel = Vec3::ZERO;
        p.history.clear();
        p.yaw = 0.0;
        p.slot = 0;
        p.weapons[0] = WState {
            id: weapon,
            mag: 5,
            reserve: 10,
        };
        p.reload_end = None;
        p.switch_end = t - 1.0;
        p.last_sprint = -100.0;
        p.shot_budget = 3.0;
        p.inv.armor = vest;
        p.inv.armor_hp = if vest > 0 { 100.0 } else { 0.0 };
        p.inv.helmet = vest;
        p.inv.helmet_hp = if vest > 0 { 100.0 } else { 0.0 };
    };
    place(&mut g, a, vec3(0.0, 300.0, 0.0));
    place(&mut g, row[0], vec3(8.0, 300.0, 0.0));
    // (the row a little higher: the shot goes level through their chests)
    let lift = g.p(a).unwrap().eye().y - g.p(row[0]).unwrap().chest().y;
    for (k, &id) in row.iter().enumerate() {
        place(&mut g, id, vec3(8.0 + k as f32 * 1.5, 300.0 + lift, 0.0));
    }
    let eye = g.p(a).unwrap().eye();
    let chest = g.p(row[0]).unwrap().chest();
    let d = (vec3(chest.x, eye.y, chest.z) - eye).normalize();
    {
        let p = g.pm(a).unwrap();
        p.yaw = yaw_of(d);
        p.pitch = 0.0;
    }
    g.handle(
        a,
        C2S::Fire {
            weapon,
            origin: eye.to_array(),
            dirs: vec![d.to_array()],
            draw: 1.0,
        },
    );
    for _ in 0..10 {
        for &id in &row {
            if let Some(p) = g.pm(id) {
                p.body.vel = Vec3::ZERO;
            }
        }
        g.tick(TICK_DT);
        g.outbox.clear();
        g.shots.clear();
    }
    row.iter()
        .map(|&id| 1000.0 - g.p(id).map_or(1000.0, |p| p.hp))
        .collect()
}

#[test]
fn a_rifle_round_goes_through_a_row() {
    let lost = row_shot(WeaponId::Awm, 0);
    assert!(lost.iter().all(|&l| l > 0.0), "all three hit: {lost:?}");
    assert!(
        lost[0] > lost[1] && lost[1] > lost[2],
        "weaker each time: {lost:?}"
    );
}

#[test]
fn a_pistol_round_stays_in_the_first() {
    let lost = row_shot(WeaponId::Fort17, 0);
    assert!(
        lost[0] > 0.0 && lost[1] == 0.0 && lost[2] == 0.0,
        "{lost:?}"
    );
}

#[test]
fn vests_and_helmets_take_more_of_it() {
    let bare = row_shot(WeaponId::Awm, 0);
    let vested = row_shot(WeaponId::Awm, 3);
    // (level 3 gear: the sniper round still gets through the first, but weaker)
    assert!(vested[1] < bare[1], "bare {bare:?} vested {vested:?}");
    assert_eq!(vested[2], 0.0, "stopped in the second: {vested:?}");
}

#[test]
fn quick_kills_make_a_double_and_a_triple() {
    let (mut g, a, row) = setup(3);
    for &v in &row {
        let p = g.pm(v).unwrap();
        p.alive = true;
        p.protected_until = 0.0;
    }
    for (k, &v) in row.iter().enumerate() {
        g.kill(v, Some(a), Cause::Weapon(WeaponId::Uar15), false);
        assert_eq!(g.p(a).unwrap().multi, k as u8 + 1);
        g.tick(TICK_DT);
    }
    // a while later: a new run
    for _ in 0..(5.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        g.outbox.clear();
    }
    let v = row[0];
    if let Some(p) = g.pm(v) {
        p.alive = true;
        p.hp = 100.0;
        p.protected_until = 0.0;
    }
    g.kill(v, Some(a), Cause::Weapon(WeaponId::Uar15), false);
    assert_eq!(g.p(a).unwrap().multi, 1);
}

/// One shot from `weapon` at the first of the row; `draw` as the client sends it, after
/// `wait` seconds of drawing since the gun was ready. Returns the health each lost.
fn single_shot(weapon: WeaponId, draw: f32, wait: f64) -> (Vec<f32>, bool) {
    let (mut g, a, row) = setup(2);
    let t = g.time;
    for (k, &id) in row.iter().enumerate() {
        let p = g.pm(id).unwrap();
        p.alive = true;
        p.hp = 1000.0;
        p.protected_until = 0.0;
        p.body.pos = vec3(14.0, 300.0, k as f32 * 3.0);
        p.body.vel = Vec3::ZERO;
        p.history.clear();
    }
    let lift = {
        let p = g.pm(a).unwrap();
        p.alive = true;
        p.protected_until = 0.0;
        p.body.pos = vec3(0.0, 300.0, 0.0);
        p.body.vel = Vec3::ZERO;
        p.slot = 0;
        p.weapons[0] = WState {
            id: weapon,
            mag: 1,
            reserve: 5,
        };
        p.reload_end = None;
        p.switch_end = t - 1.0;
        p.last_sprint = -100.0;
        p.last_shot = t - wait - weapon.def().reload_time as f64;
        p.shot_budget = 3.0;
        p.eye().y
    } - g.p(row[0]).unwrap().chest().y;
    for &id in &row {
        g.pm(id).unwrap().body.pos.y += lift;
    }
    let eye = g.p(a).unwrap().eye();
    let chest = g.p(row[0]).unwrap().chest();
    // (aim a touch high: the slow ones drop)
    let d = (chest + vec3(0.0, 0.15, 0.0) - eye).normalize();
    {
        let p = g.pm(a).unwrap();
        p.yaw = yaw_of(d);
        p.pitch = d.y.asin();
    }
    g.handle(
        a,
        C2S::Fire {
            weapon,
            origin: eye.to_array(),
            dirs: vec![d.to_array()],
            draw,
        },
    );
    let mut boom = false;
    for _ in 0..(1.0 / TICK_DT) as usize {
        for &id in &row {
            if let Some(p) = g.pm(id) {
                p.body.vel = Vec3::ZERO;
            }
        }
        g.tick(TICK_DT);
        boom |= g
            .outbox
            .iter()
            .any(|m| matches!(m.1, toon_shared::protocol::S2C::Explosion { .. }));
        g.outbox.clear();
        g.shots.clear();
    }
    let lost = row
        .iter()
        .map(|&id| 1000.0 - g.p(id).map_or(1000.0, |p| p.hp))
        .collect();
    (lost, boom)
}

#[test]
fn a_rocket_blows_up_on_whom_it_hits() {
    let (lost, boom) = single_shot(WeaponId::Rpg7, 1.0, 1.0);
    assert!(boom, "it went off");
    assert!(lost[0] > 100.0, "direct hit and the blast: {lost:?}");
    assert!(
        lost[1] > 0.0,
        "the one next to him caught the blast: {lost:?}"
    );
}

#[test]
fn a_bow_hits_harder_drawn_fully() {
    let (full, _) = single_shot(WeaponId::Bow, 1.0, 1.0);
    let (half, _) = single_shot(WeaponId::Bow, 0.3, 1.0);
    assert!(full[0] > 80.0, "{full:?}");
    assert!(
        half[0] > 0.0 && half[0] < full[0] * 0.75,
        "{half:?} vs {full:?}"
    );
    // claiming a full draw at once gets only what the time allowed
    let (cheat, _) = single_shot(WeaponId::Bow, 1.0, 0.0);
    assert!(cheat[0] < full[0] * 0.75, "{cheat:?} vs {full:?}");
}
