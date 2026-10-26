//! End-to-end over real UDP: connect, join, spawn, move with client-side physics, shoot.

use std::net::UdpSocket;
use std::time::{Duration, Instant};
use toon_server::config::ServerConfig;
use toon_shared::consts::*;
use toon_shared::map::MapId;
use toon_shared::net::Channel;
use toon_shared::physics::{Body, MoveInput, Stance, World};
use toon_shared::protocol::*;
use toon_shared::weapons::DEFAULT_CLASSES;
use toon_shared::{look_dir, maps, Team};

struct Client {
    sock: UdpSocket,
    chan: Channel,
    start: Instant,
    port: u16,
}

impl Client {
    fn new(port: u16) -> Client {
        let sock = UdpSocket::bind("127.0.0.1:0").unwrap();
        sock.set_read_timeout(Some(Duration::from_millis(5))).unwrap();
        Client { sock, chan: Channel::new(), start: Instant::now(), port }
    }
    fn flush(&mut self) {
        let p = self.chan.build_packet(self.start.elapsed().as_secs_f64());
        self.sock.send_to(&p, ("127.0.0.1", self.port)).unwrap();
    }
    fn poll(&mut self) -> Vec<S2C> {
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        while let Ok((n, _)) = self.sock.recv_from(&mut buf) {
            if let Some(m) = self.chan.receive::<S2C>(&buf[..n]) {
                out.extend(m);
            }
        }
        out
    }
}

#[test]
fn join_move_and_shoot_over_udp() {
    let port = 27900 + (std::process::id() % 50) as u16;
    let cfg = ServerConfig {
        port,
        http_enabled: false,
        bots: 2,
        min_players: 1,
        data_dir: std::env::temp_dir().join("tw-e2e").to_string_lossy().into(),
        rotation: vec![toon_server::config::RotationEntry { map: "cargo".into(), mode: "tdm".into() }],
        ..Default::default()
    };
    let server = toon_server::start(cfg, None).expect("server starts");

    // info query
    let q = UdpSocket::bind("127.0.0.1:0").unwrap();
    q.set_read_timeout(Some(Duration::from_secs(2))).unwrap();
    q.send_to(QUERY_MAGIC, ("127.0.0.1", port)).unwrap();
    let mut buf = [0u8; 4096];
    let (n, _) = q.recv_from(&mut buf).expect("query reply");
    assert_eq!(&buf[..4], QUERY_REPLY_MAGIC);
    let info: serde_json::Value = serde_json::from_slice(&buf[4..n]).unwrap();
    assert_eq!(info["map"], "Вантаж");

    let mut c = Client::new(port);
    c.chan.send_reliable(&C2S::Hello { name: "E2E".into(), password: String::new(), version: PROTOCOL_VERSION });
    c.chan.send_reliable(&C2S::JoinTeam(Some(Team::Allies)));
    c.chan.send_reliable(&C2S::SetLoadout(DEFAULT_CLASSES[0].1));

    let map = maps::build(MapId::Cargo);
    let world = World::from_map(&map);
    let mut welcomed = false;
    let mut spawned: Option<(Body, f32, u16)> = None;
    let mut corrections = 0;
    let mut snapshots = 0;
    let mut seq = 0;
    let mut fired = false;
    let mut state = MatchState::Warmup;
    let mut walked = 0.0f32;
    let mut tuning: Option<toon_shared::tuning::Tuning> = None;
    let mut ammo_after_fire = None;
    let t0 = Instant::now();
    while t0.elapsed() < Duration::from_secs(20) {
        for m in c.poll() {
            match m {
                S2C::Welcome { map, .. } => {
                    assert_eq!(map, MapId::Cargo);
                    welcomed = true;
                }
                S2C::Spawned { pos, yaw, life, .. } => {
                    spawned = Some((Body { pos: glam::Vec3::from_array(pos), on_ground: true, ..Default::default() }, yaw, life));
                }
                S2C::Correction { pos, .. } => {
                    if state == MatchState::Playing {
                        corrections += 1;
                    }
                    if let Some((b, _, _)) = spawned.as_mut() {
                        b.pos = glam::Vec3::from_array(pos);
                    }
                }
                S2C::Snapshot(s) => {
                    snapshots += 1;
                    state = s.state;
                    if fired && s.me.alive {
                        ammo_after_fire = Some(s.me.ammo[0].0);
                    }
                }
                S2C::Tuning(mut t) => {
                    t.finalize();
                    tuning = Some(*t);
                }
                S2C::Ping(ms) => c.chan.send_unreliable(&C2S::Pong(ms)),
                S2C::Reject(r) => panic!("rejected: {r}"),
                _ => {}
            }
        }
        if let Some((body, yaw, life)) = spawned.as_mut() {
            // walk forward for a while using the shared physics, like the real client
            let can_move = matches!(state, MatchState::Playing | MatchState::Warmup);
            let walking = can_move && walked < 4.0;
            if walking {
                walked += 1.0 / 30.0;
            }
            let t = tuning.as_ref().expect("server sends tuning before spawning us");
            let ws = t.weapon(DEFAULT_CLASSES[0].1.primary);
            let inp = MoveInput {
                forward: if walking { 1.0 } else { 0.0 },
                yaw: *yaw,
                want_stance: Stance::Stand,
                speed_mult: ws.move_mult,
                sprint_mult: ws.sprint_mult,
                weight: ws.weight,
                ..Default::default()
            };
            for _ in 0..2 {
                body.step(&world, &inp, &t.movement, 1.0 / 60.0);
            }
            seq += 1;
            c.chan.send_unreliable(&C2S::Input(InputState {
                seq,
                life: *life,
                pos: body.pos.to_array(),
                vel: body.vel.to_array(),
                yaw: *yaw,
                pitch: 0.0,
                stance: 0,
                flags: flags::ON_GROUND,
                slot: 0,
            }));
            if can_move && !walking && !fired {
                let eye = body.pos + glam::vec3(0.0, STAND_EYE, 0.0);
                c.chan.send_reliable(&C2S::Fire { weapon: DEFAULT_CLASSES[0].1.primary, origin: eye.to_array(), dirs: vec![look_dir(*yaw, 0.0).to_array()] });
                fired = true;
            }
        }
        c.flush();
        std::thread::sleep(Duration::from_millis(33));
    }
    assert!(welcomed, "no welcome");
    assert!(spawned.is_some(), "never spawned");
    assert!(snapshots > 100, "only {snapshots} snapshots");
    assert_eq!(corrections, 0, "legit movement was corrected");
    assert!(walked >= 4.0, "never got to walk");
    assert!(fired);
    // the server consumed exactly one bullet (unless we died meanwhile)
    if let Some(a) = ammo_after_fire {
        assert!(a == 29 || a == 30, "ammo {a}");
    }
    let status = server.command("status");
    assert!(status.contains("E2E"), "{status}");
}
