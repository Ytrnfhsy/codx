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
            seq: rng.next_u64() as u32,
            proof: long.clone(),
            cmd: long,
        },
        15 => C2S::Pong(rng.next_u64() as u32),
        16 => C2S::DropProtection,
        17 => C2S::PickUp(rng.below(65536) as u16),
        _ => C2S::BrJump,
    }
}

/// The battle royale's own messages, aimed at real things on the map as often as not.
fn random_br_msg(rng: &mut Rng, g: &Game) -> C2S {
    use toon_shared::inventory::AmmoType;
    use toon_shared::protocol::{BoxCmd, CarCmd, Item, ItemAction};
    let any = |rng: &mut Rng, real: Vec<u16>| {
        if real.is_empty() || rng.chance(0.3) {
            rng.below(65536) as u16
        } else {
            real[rng.below(real.len())]
        }
    };
    let loot: Vec<u16> = g.br.loot.iter().map(|l| l.id).take(400).collect();
    let cars: Vec<u16> = g.cars.iter().map(|c| c.id).collect();
    let boxes: Vec<u16> = g.br.boxes.iter().map(|b| b.id).collect();
    let small = |rng: &mut Rng| {
        if rng.chance(0.7) {
            rng.below(6) as u8
        } else {
            rng.below(256) as u8
        }
    };
    let ammo = [
        AmmoType::Pistol,
        AmmoType::Smg,
        AmmoType::Rifle,
        AmmoType::Shotgun,
        AmmoType::Sniper,
        AmmoType::Lmg,
    ][rng.below(6)];
    match rng.below(22) {
        0 => C2S::UseMedkit,
        1 => C2S::UseBandage,
        2 => C2S::UseDrink,
        3 => C2S::Throw {
            origin: wild_v3(rng),
            dir: wild_v3(rng),
            kind: small(rng),
        },
        4 => C2S::Ready(rng.chance(0.5)),
        5 => C2S::Marker(rng.chance(0.8).then(|| [wild_f32(rng), wild_f32(rng)])),
        6 => C2S::Car(CarCmd::Enter(any(rng, cars))),
        7 => C2S::Car(CarCmd::Exit),
        8 => C2S::Car(CarCmd::Drive {
            throttle: rng.below(256) as u8 as i8,
            steer: rng.below(256) as u8 as i8,
            brake: rng.chance(0.3),
        }),
        9 => C2S::Box(BoxCmd::Open(any(rng, boxes))),
        10 => C2S::Box(BoxCmd::Take(any(rng, boxes), rng.below(256) as u8)),
        11 => C2S::Box(BoxCmd::TakeAll(any(rng, boxes))),
        12 => C2S::Box(BoxCmd::Close),
        13 => C2S::Spectate(small(rng)),
        14 => C2S::Door(if rng.chance(0.7) {
            rng.below(g.world.doors.len().max(1)) as u16
        } else {
            rng.below(65536) as u16
        }),
        15 => C2S::SetSkin(rng.below(256) as u8),
        16 => C2S::PickUp(any(rng, loot)),
        17 => C2S::Item(ItemAction::AttachSight {
            level: small(rng),
            slot: small(rng),
        }),
        18 => C2S::Item(ItemAction::AttachMod {
            kind: small(rng),
            slot: small(rng),
        }),
        19 => C2S::Item(if rng.chance(0.5) {
            ItemAction::DetachMods(small(rng))
        } else {
            ItemAction::DetachSight(small(rng))
        }),
        _ => C2S::Item(ItemAction::Drop(
            match rng.below(12) {
                0 => Item::Ammo(ammo),
                1 => Item::Medkit,
                2 => Item::Armor,
                3 => Item::Helmet,
                4 => Item::Backpack,
                5 => Item::Sight(small(rng)),
                6 => Item::Mod(small(rng)),
                7 => Item::Bandages,
                8 => Item::Drinks,
                9 => Item::Molotovs,
                10 => Item::Flashes,
                _ => Item::Weapon(small(rng)),
            },
            if rng.chance(0.5) {
                rng.below(5) as u16
            } else {
                rng.below(65536) as u16
            },
        )),
    }
}

fn run(map: MapId, mode: Mode, seed: u64) {
    let cfg = ServerConfig {
        // (tests start straight away: nobody presses «Ready»)
        ready_check: false,
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
                let msg = if mode == Mode::Br && rng.chance(0.5) {
                    random_br_msg(&mut rng, &g)
                } else {
                    random_msg(&mut rng, life, &mut seq)
                };
                g.handle(id, msg);
                worst = worst.max(t0.elapsed());
            }
        }
        // battle royale: now and then a player is put on the ground by some loot with a
        // full backpack, or loses a fight, so that the messages have something to work on
        if mode == Mode::Br && g.br.running && tick % 40 == 0 {
            let id = ids[rng.below(ids.len())];
            let spot = (!g.br.loot.is_empty()).then(|| g.br.loot[rng.below(g.br.loot.len())].pos);
            if let (Some(at), Some(p)) = (spot, g.pm(id)) {
                if p.alive || p.in_plane {
                    p.in_plane = false;
                    p.alive = true;
                    p.body.chute = toon_shared::physics::Chute::None;
                    p.body.pos = at;
                    p.inv.medkits = 3;
                    p.inv.bandages = 5;
                    p.inv.drinks = 2;
                    p.inv.molotovs = 2;
                    p.inv.flashes = 2;
                    p.inv.spare_sights = [1, 1, 1, 1];
                    p.inv.spare_mods = [1, 1, 1, 1];
                }
            }
            if rng.chance(0.3) {
                let victims: Vec<u8> = g
                    .players
                    .iter()
                    .flatten()
                    .filter(|p| p.alive && p.is_bot())
                    .map(|p| p.id)
                    .collect();
                if !victims.is_empty() {
                    g.kill(
                        victims[rng.below(victims.len())],
                        Some(id),
                        toon_shared::weapons::Cause::World,
                        false,
                    );
                }
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
