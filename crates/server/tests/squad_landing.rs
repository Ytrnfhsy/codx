//! A squad from the main lobby jumps out of the plane and lands: both stay in the round.

use glam::Vec3;
use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::physics::{Body, MoveInput, Stance};
use toon_shared::protocol::{flags, InputState, C2S, S2C};
use toon_shared::Mode;

#[test]
fn a_squad_from_the_lobby_lands_and_plays() {
    land_after(0.0);
}

/// A dedicated server runs for weeks: its clock is far from zero.
#[test]
fn a_squad_lands_on_a_server_up_for_weeks() {
    for days in [1.0, 7.0, 40.0] {
        land_after(days * 86400.0);
    }
}

fn land_after(uptime: f64) {
    let mut cfg = ServerConfig {
        data_dir: std::env::temp_dir()
            .join("tw-squadland")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    cfg.br.squad = 2;
    cfg.br.bots = 6;
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Br);
    g.time += uptime;
    let v = toon_shared::consts::PROTOCOL_VERSION;
    let mut ids = Vec::new();
    for k in 0..2u16 {
        let id = g
            .connect(
                format!("127.0.0.1:{}", 5800 + k).parse().unwrap(),
                &format!("P{k}"),
                "",
                v,
            )
            .unwrap();
        // what the client sends when it comes with a squad from the main lobby
        g.handle(id, C2S::PartyTag(0xabcdef));
        g.handle(id, C2S::JoinTeam(None));
        g.handle(id, C2S::Ready(true));
        ids.push(id);
        for _ in 0..(1.0 / TICK_DT) as usize {
            g.tick(TICK_DT);
        }
    }
    for _ in 0..(20.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
        if g.br.running {
            break;
        }
    }
    assert!(g.br.running);
    for &id in &ids {
        let p = g.p(id).unwrap();
        assert!(p.in_plane, "{} not on the plane: team {:?}", p.name, p.team);
    }
    for _ in 0..(5.0 / TICK_DT) as usize {
        g.tick(TICK_DT);
    }
    let mut bodies = Vec::new();
    for &id in &ids {
        g.handle(id, C2S::BrJump);
        let p = g.p(id).unwrap();
        assert!(p.alive, "jumped but not alive");
        bodies.push(p.body);
    }
    g.outbox.clear();
    let mut seq = 0u32;
    let mut landed = [None::<f64>; 2];
    for step in 0..(150.0 / TICK_DT) as usize {
        for (k, &id) in ids.iter().enumerate() {
            let b: &mut Body = &mut bodies[k];
            let inp = MoveInput {
                forward: if step % 300 < 150 { 1.0 } else { 0.0 },
                yaw: g.p(id).unwrap().yaw,
                want_stance: Stance::Stand,
                ..Default::default()
            };
            b.step(&g.world, &inp, &g.tuning.movement, TICK_DT);
            if b.on_ground && landed[k].is_none() {
                landed[k] = Some(g.time);
            }
            seq += 1;
            let life = g.p(id).unwrap().life;
            g.handle(
                id,
                C2S::Input(InputState {
                    seq,
                    life,
                    pos: b.pos.to_array(),
                    vel: b.vel.to_array(),
                    yaw: inp.yaw,
                    pitch: 0.0,
                    stance: b.stance.to_u8(),
                    flags: if b.on_ground { flags::ON_GROUND } else { 0 },
                    slot: 0,
                }),
            );
        }
        g.tick(TICK_DT);
        for m in g.outbox.drain(..) {
            if let S2C::Correction { pos, .. } = m.1 {
                for (k, &id) in ids.iter().enumerate() {
                    if matches!(m.0, toon_server::game::Dest::One(x) if x == id) {
                        bodies[k].pos = Vec3::from_array(pos);
                        bodies[k].vel = Vec3::ZERO;
                    }
                }
            }
            if !matches!(m.1, S2C::Snapshot(..) | S2C::Correction { .. }) {
                let s = format!("{:?}", m.1);
                if s.len() < 300 && !s.starts_with("Ents") {
                    println!("{:.1} {}", g.time, s);
                }
            }
        }
        if landed.iter().all(|l| l.is_some_and(|t| g.time > t + 10.0)) {
            break;
        }
    }
    for (k, &id) in ids.iter().enumerate() {
        let p = g.p(id).unwrap();
        println!(
            "{} landed {:?} alive {} hp {} downed {} team {:?} pos {:?}",
            p.name, landed[k], p.alive, p.hp, p.downed, p.team, p.body.pos
        );
        assert!(landed[k].is_some(), "never landed");
        assert!(p.alive && !p.downed, "{} out after landing", p.name);
    }
}
