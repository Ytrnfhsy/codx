//! A hostile or broken client must not be able to crash the server, freeze it, make it
//! grow without bound, or break the game for everybody else: random and malformed
//! messages with extreme values are thrown at it while a match is running.

use glam::Vec3;
use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::TICK_DT;
use toon_shared::map::MapId;
use toon_shared::net::Channel;
use toon_shared::protocol::{InputState, C2S};
use toon_shared::rng::Rng;
use toon_shared::weapons::{Loadout, WeaponId, DEFAULT_CLASSES};
use toon_shared::{Mode, Team};

fn wild_f32(rng: &mut Rng) -> f32 {
    match rng.below(8) {
        0 => f32::NAN,
        1 => f32::INFINITY,
        2 => -f32::INFINITY,
        3 => 1.0e30,
        4 => -1.0e30,
        5 => 0.0,
        _ => rng.range(-600.0, 600.0),
    }
}

fn wild_v3(rng: &mut Rng) -> [f32; 3] {
    [wild_f32(rng), wild_f32(rng), wild_f32(rng)]
}

fn random_msg(rng: &mut Rng, life: u16, seq: &mut u32) -> C2S {
    let weapons: Vec<WeaponId> = WeaponId::all().collect();
    let w = weapons[rng.below(weapons.len())];
    let long = "Ж".repeat(rng.below(5000));
    match rng.below(19) {
        0 => C2S::JoinTeam(match rng.below(4) {
            0 => None,
            1 => Some(Team::Allies),
            2 => Some(Team::Axis),
            _ => Some(Team::Spectator),
        }),
        1 => C2S::SetLoadout(Loadout {
            primary: w,
            secondary: weapons[rng.below(weapons.len())],
            ..DEFAULT_CLASSES[rng.below(DEFAULT_CLASSES.len())].1
        }),
        2 => C2S::Spawn,
        3 | 4 | 5 => {
            *seq = seq.wrapping_add(rng.below(3) as u32);
            C2S::Input(InputState {
                seq: if rng.chance(0.1) {
                    u32::MAX - rng.below(5) as u32
                } else {
                    *seq
                },
                life: if rng.chance(0.8) {
                    life
                } else {
                    rng.below(70000) as u16
                },
                pos: wild_v3(rng),
                vel: wild_v3(rng),
                yaw: wild_f32(rng),
                pitch: wild_f32(rng),
                stance: rng.below(256) as u8,
                flags: rng.below(256) as u8,
                slot: rng.below(256) as u8,
            })
        }
        6 => C2S::Fire {
            weapon: w,
            origin: wild_v3(rng),
            dirs: (0..rng.below(300)).map(|_| wild_v3(rng)).collect(),
        },
        7 => C2S::Reload,
        8 => C2S::Switch(rng.below(256) as u8),
        9 => C2S::Grenade {
            origin: wild_v3(rng),
            dir: wild_v3(rng),
            cook: wild_f32(rng),
            smoke: rng.chance(0.5),
        },
        10 => C2S::Melee,
        11 => C2S::UseKillstreak {
            target: rng.chance(0.7).then(|| [wild_f32(rng), wild_f32(rng)]),
            pilot: rng.chance(0.5),
        },
        12 => C2S::Drive {
            pos: wild_v3(rng),
            yaw: wild_f32(rng),
            pitch: wild_f32(rng),
            keys: rng.below(256) as u8,
        },
        13 => C2S::Chat {
            text: long,
            team_only: rng.chance(0.5),
        },
        14 => C2S::Rcon {
            password: long.clone(),
            cmd: long,
        },
        15 => C2S::Pong(rng.next_u64() as u32),
        16 => C2S::DropProtection,
        17 => C2S::PickUp(rng.below(65536) as u16),
        _ => C2S::BrJump,
    }
}

fn run(map: MapId, mode: Mode, seed: u64) {
    let cfg = ServerConfig {
        bots: 6,
        data_dir: std::env::temp_dir()
            .join("tw-fuzz")
            .to_string_lossy()
            .into(),
        ..Default::default()
    };
    let mut g = Game::new(cfg);
    g.change_map(map, mode);
    let mut rng = Rng::new(seed);
    let mut ids = Vec::new();
    for k in 0..3 {
        let id = g
            .connect(
                format!("127.0.0.1:{}", 6000 + k).parse().unwrap(),
                &"Б".repeat(k * 40 + 1),
                "",
                toon_shared::consts::PROTOCOL_VERSION,
            )
            .unwrap();
        g.handle(id, C2S::JoinTeam(None));
        g.handle(id, C2S::SetLoadout(DEFAULT_CLASSES[0].1));
        ids.push(id);
    }
    let mut seq = 1u32;
    let mut worst = std::time::Duration::ZERO;
    for tick in 0..(90.0 / TICK_DT) as usize {
        for &id in &ids {
            let life = g.p(id).map(|p| p.life).unwrap_or(0);
            for _ in 0..rng.below(6) {
                let t0 = std::time::Instant::now();
                g.handle(id, random_msg(&mut rng, life, &mut seq));
                worst = worst.max(t0.elapsed());
            }
        }
        let t0 = std::time::Instant::now();
        let before = g.map.id;
        g.tick(TICK_DT);
        // (a map rotation builds a whole new map: slow on purpose, and not a message)
        if g.map.id == before {
            worst = worst.max(t0.elapsed());
        }
        for &id in &ids {
            let _ = g.snapshot_for(id);
        }
        g.outbox.clear();
        g.shots.clear();
        // nobody may end up at a NaN or infinite position
        for p in g.players.iter().flatten() {
            assert!(
                p.body.pos.is_finite()
                    && p.hp.is_finite()
                    && p.yaw.is_finite()
                    && p.pitch.is_finite(),
                "{} broke at tick {tick}: pos {} hp {} yaw {} pitch {}",
                p.name,
                p.body.pos,
                p.hp,
                p.yaw,
                p.pitch
            );
        }
        for v in &g.vehicles {
            assert!(v.pos.is_finite(), "vehicle at {}", v.pos);
        }
        for gr in &g.grenades {
            assert!(
                gr.pos.is_finite() && gr.vel.is_finite(),
                "grenade at {}",
                gr.pos
            );
        }
    }
    eprintln!(
        "{} {}: worst message or tick {worst:?}",
        map.short(),
        mode.short()
    );
    assert!(worst.as_millis() < 100, "a message took {worst:?}");
    let _ = Vec3::ZERO;
}

#[test]
fn hostile_messages_do_not_break_the_server() {
    run(MapId::Depot, Mode::Tdm, 1);
    run(MapId::Market, Mode::Sab, 2);
    run(MapId::Island, Mode::Br, 3);
}

/// Garbage datagrams and reliable ids from the far future must not crash the network
/// layer or make it hold on to memory forever.
#[test]
fn garbage_datagrams() {
    let mut rng = Rng::new(9);
    let mut ch = Channel::new();
    let mut real = Channel::new();
    for _ in 0..20000 {
        let mut data: Vec<u8> = match rng.below(3) {
            0 => (0..rng.below(1500)).map(|_| rng.below(256) as u8).collect(),
            _ => {
                // a real packet with a few bytes flipped
                real.send_reliable(&C2S::Chat {
                    text: "x".repeat(rng.below(100)),
                    team_only: false,
                });
                real.send_unreliable(&C2S::Pong(rng.next_u64() as u32));
                real.build_packet(rng.f32() as f64)
            }
        };
        for _ in 0..rng.below(4) {
            if !data.is_empty() {
                let i = rng.below(data.len());
                data[i] = rng.below(256) as u8;
            }
        }
        let _ = ch.receive::<C2S>(&data);
    }
    assert!(
        ch.held_len() < 4096,
        "{} messages held for later",
        ch.held_len()
    );
}
