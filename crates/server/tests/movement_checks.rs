//! The server's checks on where clients say they are: honest running and jumping passes,
//! flying does not, and long falls hurt.

use glam::{vec3, Vec3};
use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::{MAX_HEALTH, TICK_DT};
use toon_shared::map::MapId;
use toon_shared::physics::{Body, MoveInput, Stance};
use toon_shared::protocol::{flags, InputState, C2S, S2C};
use toon_shared::Mode;

fn game() -> (Game, u8) {
    let cfg = ServerConfig {
        ready_check: false,
        bots: 0,
        data_dir: std::env::temp_dir()
            .join("tw-move")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    g.change_map(MapId::Island, Mode::Tdm);
    let v = toon_shared::consts::PROTOCOL_VERSION;
    let me = g
        .connect("127.0.0.1:5501".parse().unwrap(), "Гравець", "", v)
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));
    for _ in 0..10 {
        g.tick(TICK_DT);
    }
    let at = g.map.spawns[0][0].pos;
    let p = g.pm(me).unwrap();
    p.alive = true;
    p.hp = MAX_HEALTH;
    p.protected_until = 0.0;
    p.body.pos = at;
    g.state = toon_shared::protocol::MatchState::Playing;
    g.outbox.clear();
    (g, me)
}

fn send(g: &mut Game, me: u8, seq: &mut u32, pos: Vec3, on_ground: bool) {
    *seq += 1;
    // (whatever the match is doing, nobody is frozen in place here)
    g.state = toon_shared::protocol::MatchState::Playing;
    let life = g.p(me).unwrap().life;
    g.handle(
        me,
        C2S::Input(InputState {
            seq: *seq,
            life,
            pos: pos.to_array(),
            vel: [0.0; 3],
            yaw: 0.0,
            pitch: 0.0,
            stance: Stance::Stand.to_u8(),
            flags: if on_ground { flags::ON_GROUND } else { 0 },
            slot: 0,
        }),
    );
}

fn corrections(g: &Game) -> usize {
    g.outbox
        .iter()
        .filter(|m| matches!(m.1, S2C::Correction { .. }))
        .count()
}

#[test]
fn honest_running_and_jumping_passes() {
    let (mut g, me) = game();
    let mut b = Body {
        pos: g.p(me).unwrap().body.pos,
        ..Default::default()
    };
    let mut seq = 0;
    let dt = 1.0 / 60.0;
    for k in 0..60 * 12 {
        let inp = MoveInput {
            forward: 1.0,
            sprint: true,
            yaw: k as f32 * 0.004,
            jump: k % 50 == 0,
            want_stance: Stance::Stand,
            ..Default::default()
        };
        b.step(&g.world, &inp, &g.tuning.movement, dt);
        send(&mut g, me, &mut seq, b.pos, b.on_ground);
        if k % 2 == 0 {
            g.tick(TICK_DT);
        }
    }
    assert_eq!(
        corrections(&g),
        0,
        "the server pulled an honest player back"
    );
    let p = g.p(me).unwrap();
    assert!(p.body.pos.distance(b.pos) < 0.01);
    assert_eq!(p.hp, MAX_HEALTH, "running and jumping hurt");
}

#[test]
fn flying_is_refused() {
    let (mut g, me) = game();
    let start = g.p(me).unwrap().body.pos;
    let mut seq = 0;
    send(&mut g, me, &mut seq, start, true);
    for k in 1..60 {
        send(
            &mut g,
            me,
            &mut seq,
            start + vec3(0.0, k as f32 * 0.4, 0.0),
            false,
        );
        g.tick(TICK_DT);
    }
    let y = g.p(me).unwrap().body.pos.y;
    assert!(y < start.y + 2.6, "flew up to {} from {}", y, start.y);
    assert!(corrections(&g) > 10);
}

#[test]
fn long_falls_hurt_and_short_ones_do_not() {
    for (height, hurt) in [(4.0f32, false), (12.0, true), (40.0, true)] {
        let (mut g, me) = game();
        let ground = g.p(me).unwrap().body.pos;
        let mut seq = 0;
        send(&mut g, me, &mut seq, ground, true);
        // (as if walked off something this high)
        g.pm(me).unwrap().body.pos = ground + vec3(0.0, height, 0.0);
        g.pm(me).unwrap().support_y = ground.y + height;
        let mut y = height;
        while y > 0.0 {
            y = (y - 0.5).max(0.0);
            send(&mut g, me, &mut seq, ground + vec3(0.0, y, 0.0), y == 0.0);
            g.tick(TICK_DT);
        }
        let p = g.p(me).unwrap();
        if hurt {
            assert!(
                p.hp < MAX_HEALTH - 20.0 || !p.alive,
                "{height} m: hp {}",
                p.hp
            );
        } else {
            assert_eq!(p.hp, MAX_HEALTH, "{height} m hurt");
        }
        if height > 30.0 {
            assert!(!p.alive, "survived {height} m");
        }
    }
}

/// Admin commands carry a proof made from the password, never the password: the right
/// proof runs the command once, and neither a repeat of it nor a changed command passes.
#[test]
fn rcon_needs_a_fresh_proof() {
    use toon_shared::protocol::rcon_proof;
    let (mut g, me) = game();
    g.cfg.rcon_password = "таємниця".into();
    let nonce = g.p(me).unwrap().nonce;
    let replies = |g: &mut Game| -> Vec<String> {
        g.outbox
            .drain(..)
            .filter_map(|m| match m.1 {
                S2C::RconReply(t) => Some(t),
                _ => None,
            })
            .collect()
    };
    let wrong = |r: &[String]| r.iter().any(|t| t.contains("RCON:"));
    let cmd = "help".to_string();
    let proof = rcon_proof("таємниця", nonce, 1, &cmd);
    g.handle(
        me,
        C2S::Rcon {
            seq: 1,
            proof: proof.clone(),
            cmd: cmd.clone(),
        },
    );
    let r = replies(&mut g);
    assert!(
        !r.is_empty() && !wrong(&r),
        "the right proof was refused: {r:?}"
    );
    // the same datagram again
    g.handle(
        me,
        C2S::Rcon {
            seq: 1,
            proof: proof.clone(),
            cmd: cmd.clone(),
        },
    );
    assert!(wrong(&replies(&mut g)), "a repeat passed");
    // the proof of one command on another, and a proof for another connection
    g.handle(
        me,
        C2S::Rcon {
            seq: 2,
            proof,
            cmd: "status".into(),
        },
    );
    assert!(wrong(&replies(&mut g)));
    let other = rcon_proof("таємниця", nonce ^ 1, 3, &cmd);
    g.handle(
        me,
        C2S::Rcon {
            seq: 3,
            proof: other,
            cmd: cmd.clone(),
        },
    );
    assert!(wrong(&replies(&mut g)));
    // the password itself is no proof
    g.handle(
        me,
        C2S::Rcon {
            seq: 4,
            proof: "таємниця".into(),
            cmd,
        },
    );
    assert!(wrong(&replies(&mut g)));
}
