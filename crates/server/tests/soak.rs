//! Long bot matches on every map and mode, checking the game's invariants every tick:
//! nobody ends up inside walls, out of the world or at a NaN position, health stays in
//! range, bots keep moving, matches finish and the server keeps its tick budget.
//!
//! Slow, so ignored by default: `SOAK_SECS=600 cargo test --release --test soak -- --ignored --nocapture`

use glam::{vec2, Vec3};
use std::collections::HashMap;
use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::physics::{fits, Chute};
use toon_shared::protocol::{MatchState, C2S, S2C};
use toon_shared::weapons::DEFAULT_CLASSES;
use toon_shared::{Mode, Team};

#[derive(Default, Debug)]
struct Report {
    kills: usize,
    matches_ended: usize,
    maps_changed: usize,
    worst_tick_ms: f32,
    avg_tick_us: f32,
    biggest_snapshot: usize,
    problems: Vec<String>,
    /// (bot name, where) for bots that stood still for a long time while alive
    stuck: Vec<(String, Vec3)>,
    inside_walls: usize,
}

fn soak(map: MapId, mode: Mode, bots: usize, squad: u8, secs: f32) -> Report {
    let mut cfg = ServerConfig {
        // (tests start straight away: nobody presses «Ready»)
        ready_check: false,
        bots,
        max_players: bots.max(2),
        data_dir: std::env::temp_dir()
            .join("tw-soak")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    cfg.br.bots = bots;
    cfg.br.max_players = bots.max(2);
    cfg.br.squad = squad;
    let mut g = Game::new(cfg);
    g.rng = toon_shared::rng::Rng::new(map as u64 * 31 + mode as u64);
    g.change_map(map, mode);
    let me = g
        .connect(
            "127.0.0.1:5003".parse().unwrap(),
            "Тестер",
            "",
            toon_shared::consts::PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(Some(Team::Allies)));
    g.handle(me, C2S::SetLoadout(DEFAULT_CLASSES[0].1));

    let mut r = Report::default();
    let ticks = (secs / TICK_DT) as usize;
    let start_map = g.map.id;
    // per bot: where it was and since when it has not moved 1.5 m
    let mut still: HashMap<u8, (Vec3, f64)> = HashMap::new();
    let mut reported_stuck: HashMap<u8, f64> = HashMap::new();
    let mut inside: HashMap<u8, f64> = HashMap::new();
    let t_all = std::time::Instant::now();
    for _ in 0..ticks {
        let t0 = std::time::Instant::now();
        g.tick(TICK_DT);
        // the network layer would build a snapshot for everyone
        let ids: Vec<u8> = g.players.iter().flatten().map(|p| p.id).collect();
        for id in &ids {
            let s = g.snapshot_for(*id);
            if *id == me {
                let n = toon_shared::net::encode(&S2C::Snapshot(Box::new(s))).len();
                r.biggest_snapshot = r.biggest_snapshot.max(n);
            }
        }
        r.worst_tick_ms = r.worst_tick_ms.max(t0.elapsed().as_secs_f32() * 1000.0);
        g.shots.clear();
        for (_, m, _) in g.outbox.drain(..) {
            match m {
                S2C::Kill { .. } => r.kills += 1,
                S2C::MatchEnd { .. } => r.matches_ended += 1,
                S2C::MapChange { .. } => r.maps_changed += 1,
                _ => {}
            }
        }
        if g.map.id != start_map {
            break; // the rotation moved on: this map is done
        }
        let time = g.time;
        let playing = g.state == MatchState::Playing;
        let (lo, hi) = (g.map.min, g.map.max);
        for p in g.players.iter().flatten() {
            let pos = p.body.pos;
            if !pos.is_finite() || !p.hp.is_finite() {
                r.problems
                    .push(format!("{}: NaN state at {pos} hp {}", p.name, p.hp));
                continue;
            }
            if !p.alive {
                still.remove(&p.id);
                inside.remove(&p.id);
                continue;
            }
            if p.hp > 100.01 || p.hp < 0.0 {
                r.problems.push(format!("{}: hp {}", p.name, p.hp));
            }
            if pos.x < lo.x - 2.0
                || pos.x > hi.x + 2.0
                || pos.y < -30.0
                || pos.z < lo.y - 2.0
                || pos.z > hi.y + 2.0
            {
                r.problems
                    .push(format!("{}: out of the map at {pos:.1}", p.name));
            }
            let floor = g.world.ground(pos.x, pos.z).unwrap_or(0.0);
            if pos.y < floor - 0.5 {
                r.problems.push(format!(
                    "{}: under the ground at {pos:.1} (ground {floor:.1})",
                    p.name
                ));
            }
            // inside geometry for more than a second (a moment can happen on corrections)
            // (on a ladder the climber may touch the roof's edge)
            if p.body.chute == Chute::None
                && !p.body.climbing
                && !fits(&g.world, pos + Vec3::Y * 0.02, p.body.stance)
            {
                let since = *inside.entry(p.id).or_insert(time);
                if time - since > 1.0 {
                    r.inside_walls += 1;
                    if r.inside_walls < 6 {
                        r.problems.push(format!(
                            "{}: inside a wall at {pos:.2} ({:?})",
                            p.name, p.body.stance
                        ));
                    }
                    inside.insert(p.id, time + 1000.0);
                }
            } else {
                inside.remove(&p.id);
            }
            // (a bot holding a spot stands still on purpose)
            // (a bot holding a spot, or trading shots from cover, stands still on purpose)
            let busy = p
                .bot
                .as_ref()
                .is_some_and(|b| b.holding() || b.target().is_some());
            if busy {
                still.remove(&p.id);
            }
            if p.is_bot() && playing && !busy {
                let e = still.entry(p.id).or_insert((pos, time));
                if vec2(pos.x - e.0.x, pos.z - e.0.z).length() > 1.5 || (pos.y - e.0.y).abs() > 1.0
                {
                    *e = (pos, time);
                } else if time - e.1 > 25.0
                    && reported_stuck.get(&p.id).is_none_or(|t| time - t > 60.0)
                {
                    r.stuck.push((p.name.clone(), pos));
                    if let Some(b) = &p.bot {
                        eprintln!(
                            "      {} at {pos:.1}, t {time:.0}: {}",
                            p.name,
                            b.describe()
                        );
                    }
                    reported_stuck.insert(p.id, time);
                }
            }
        }
    }
    r.avg_tick_us = t_all.elapsed().as_secs_f32() * 1e6 / ticks as f32;
    if mode == Mode::Br {
        let alive: Vec<String> = g
            .players
            .iter()
            .flatten()
            .filter(|p| p.alive)
            .map(|p| {
                format!(
                    "{} {:?} {:.0}hp at {:.1}",
                    p.name,
                    p.weapons
                        .iter()
                        .map(|w| (w.id, w.mag + w.reserve))
                        .collect::<Vec<_>>(),
                    p.hp,
                    p.body.pos
                )
            })
            .collect();
        eprintln!("    alive at the end ({:?}): {alive:?}", g.state);
        let all: Vec<String> = g
            .players
            .iter()
            .flatten()
            .map(|p| format!("{} t{:?} a{} pl{}", p.name, p.team, p.alive, p.in_plane))
            .collect();
        eprintln!("    entrants {} everyone: {all:?}", g.br.entrants);
    }
    r
}

#[test]
#[ignore]
fn soak_every_map_and_mode() {
    let secs: f32 = std::env::var("SOAK_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(300.0);
    let mut cases: Vec<(MapId, Mode, usize, u8)> = Vec::new();
    for m in MapId::ALL {
        cases.push((m, Mode::Tdm, 10, 1));
        cases.push((m, Mode::Sab, 10, 1));
    }
    cases.push((MapId::Island, Mode::Tdm, 24, 1));
    cases.push((MapId::Island, Mode::Br, 30, 1));
    cases.push((MapId::Island, Mode::Br, 32, 4));
    cases.push((MapId::Market, Mode::Br, 12, 2));
    let mut bad = 0;
    for (m, mode, bots, squad) in cases {
        let r = soak(m, mode, bots, squad, secs);
        eprintln!(
            "{:>7} {:>3} ×{squad}: {} kills, {} matches ended, tick avg {:.0} µs worst {:.1} ms, snapshot ≤ {} B, {} stuck, {} in walls",
            m.short(),
            mode.short(),
            r.kills,
            r.matches_ended,
            r.avg_tick_us,
            r.worst_tick_ms,
            r.biggest_snapshot,
            r.stuck.len(),
            r.inside_walls
        );
        for p in r.problems.iter().take(8) {
            eprintln!("    PROBLEM {p}");
        }
        for (n, at) in r.stuck.iter().take(8) {
            eprintln!("    stuck: {n} at {at:.1}");
        }
        if !r.problems.is_empty() || r.kills == 0 {
            bad += 1;
        }
    }
    assert_eq!(bad, 0, "problems found (see above)");
}
