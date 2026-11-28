//! The server's benchmark: the same matches every run (fixed seeds, the fixed 30 Hz tick,
//! no real clock in the game), measured system by system.
//!
//!   cargo run --release -p toon_server --example bench            # every scene
//!   cargo run --release -p toon_server --example bench -- br64    # one of them
//!
//! Scenes:
//! * `br64` — a battle royale on the Island with 64 players, 180 s from the plane to the
//!   first fights;
//! * `tdm12` — a team deathmatch on the Depot with 12 players, 120 s.
//!
//! Every player is treated as a real one (`Game::everyone_watches`): what they may see, the
//! loot near them, their snapshot and the messages for them are worked out every tick and
//! go through a real channel, so the traffic is what 64 people would get. Each player also
//! sends what a client sends (its position 30 times a second).
//!
//! Prints a Markdown report: per system mean / p50 / p99 / max of the tick, then the traffic
//! per client and the memory. docs/PERF.md keeps the results.

use std::time::Instant;
use toon_server::config::ServerConfig;
use toon_server::game::{Dest, Game};
use toon_shared::consts::{PROTOCOL_VERSION, TICK_DT};
use toon_shared::map::MapId;
use toon_shared::net::Channel;
use toon_shared::prof;
use toon_shared::protocol::{InputState, C2S, S2C};
use toon_shared::rng::Rng;
use toon_shared::Mode;

struct Scene {
    name: &'static str,
    map: MapId,
    mode: Mode,
    players: usize,
    secs: f32,
    seed: u64,
}

const SCENES: [Scene; 2] = [
    Scene {
        name: "br64",
        map: MapId::Island,
        mode: Mode::Br,
        players: 64,
        secs: 180.0,
        seed: 0xb464,
    },
    Scene {
        name: "tdm12",
        map: MapId::Depot,
        mode: Mode::Tdm,
        players: 12,
        secs: 120.0,
        seed: 0x7d312,
    },
];

fn main() {
    let only: Vec<String> = std::env::args().skip(1).collect();
    println!("# Server benchmark\n");
    println!(
        "`cargo run --release -p toon_server --example bench {}`, {} {}\n",
        only.join(" "),
        std::env::consts::OS,
        std::env::consts::ARCH
    );
    for scene in SCENES.iter() {
        if only.is_empty() || only.iter().any(|o| o == scene.name) {
            run(scene);
        }
    }
    if let Some(peak) = peak_mb() {
        println!("Peak memory of the process: {peak:.0} MB\n");
    }
}

fn run(s: &Scene) {
    let dir = std::env::temp_dir().join(format!("tw-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut cfg = ServerConfig {
        ready_check: false,
        data_dir: dir.to_string_lossy().into(),
        ..Default::default()
    };
    // one connected player (a battle royale starts only with somebody real), the rest bots
    if s.mode == Mode::Br {
        cfg.br.bots = s.players - 1;
        cfg.br.max_players = s.players;
    } else {
        cfg.bots = s.players - 1;
        cfg.max_players = s.players;
    }
    let mut g = Game::new(cfg);
    g.rng = Rng::new(s.seed);
    g.everyone_watches = true;
    g.change_map(s.map, s.mode);
    let me = g
        .connect(
            "127.0.0.1:7100".parse().unwrap(),
            "Bench",
            "",
            PROTOCOL_VERSION,
        )
        .unwrap();
    g.handle(me, C2S::JoinTeam(None));

    // a server channel to every player and a client channel back
    let mut down: Vec<Option<Channel>> = (0..256).map(|_| None).collect();
    let mut up: Vec<Option<Channel>> = (0..256).map(|_| None).collect();
    prof::enable();
    prof::reset();
    let ticks = (s.secs / TICK_DT) as usize;
    let started = Instant::now();
    for t in 0..ticks {
        g.tick(TICK_DT);
        let now = t as f64 * TICK_DT as f64;
        let ids: Vec<u8> = g.players.iter().flatten().map(|p| p.id).collect();
        for &id in &ids {
            down[id as usize].get_or_insert_with(Channel::new);
            up[id as usize].get_or_insert_with(Channel::new);
        }
        {
            let _p = prof::scope("net: messages");
            for (dest, msg, reliable) in std::mem::take(&mut g.outbox) {
                for &id in &ids {
                    let to = match dest {
                        Dest::All => true,
                        Dest::One(x) => x == id,
                        Dest::Team(team) => g.p(id).is_some_and(|p| p.team == team),
                    };
                    if to {
                        let c = down[id as usize].as_mut().unwrap();
                        if reliable {
                            c.send_reliable(&msg);
                        } else {
                            c.send_unreliable(&msg);
                        }
                    }
                }
            }
        }
        {
            let _p = prof::scope("net: snapshots");
            for &id in &ids {
                let snap = g.snapshot_for(id);
                down[id as usize]
                    .as_mut()
                    .unwrap()
                    .send_unreliable(&S2C::Snapshot(Box::new(snap)));
            }
        }
        g.shots.clear();
        let mut bytes_down = Vec::with_capacity(ids.len());
        {
            let _p = prof::scope("net: send");
            for &id in &ids {
                let c = down[id as usize].as_mut().unwrap();
                let first = c.build_packet(now);
                let n = first.len() + c.spill.drain(..).map(|p| p.len()).sum::<usize>();
                bytes_down.push(n);
                // the client's acks come back, so reliable messages do not pile up
                let back = up[id as usize].as_mut().unwrap();
                let _ = back.receive::<S2C>(&first);
            }
        }
        // what each client sends: its position, 30 times a second
        let seq = t as u32 + 1;
        let mut bytes_up = Vec::with_capacity(ids.len());
        for &id in &ids {
            let Some(p) = g.p(id) else { continue };
            let inp = InputState {
                seq,
                life: p.life,
                pos: p.body.pos.to_array(),
                vel: p.body.vel.to_array(),
                yaw: p.yaw,
                pitch: p.pitch,
                stance: p.body.stance.to_u8(),
                flags: p.flags,
                slot: p.slot,
            };
            let c = up[id as usize].as_mut().unwrap();
            c.send_unreliable(&C2S::Input(inp));
            let pkt = c.build_packet(now);
            bytes_up.push(pkt.len());
            let _ = down[id as usize].as_mut().unwrap().receive::<C2S>(&pkt);
        }
        // per client: the mean over the clients this tick, and the worst one
        let n = bytes_down.len().max(1) as f64;
        prof::value(
            "down per client (B/tick)",
            bytes_down.iter().sum::<usize>() as f64 / n,
        );
        prof::value(
            "down, the busiest client (B/tick)",
            bytes_down.iter().copied().max().unwrap_or(0) as f64,
        );
        prof::value(
            "up per client (B/tick)",
            bytes_up.iter().sum::<usize>() as f64 / n,
        );
        prof::value(
            "alive",
            g.players.iter().flatten().filter(|p| p.alive).count() as f64,
        );
        prof::frame_end();
    }
    let wall = started.elapsed().as_secs_f64();
    let (frames, lines) = prof::lines();
    let tps = 1.0 / TICK_DT as f64;
    println!(
        "## {} — {:?} on {:?}, {} players, {:.0} s ({} ticks), seed {:#x}\n",
        s.name, s.mode, s.map, s.players, s.secs, frames, s.seed
    );
    println!(
        "Ran in {wall:.1} s ({:.0}× real time).\n",
        s.secs as f64 / wall.max(1e-9)
    );
    let (times, values): (Vec<_>, Vec<_>) = lines.into_iter().partition(|l| l.timed);
    println!("Time per tick (the tick's budget at 30 Hz is 33.3 ms):\n");
    println!("{}", prof::table(&times));
    println!("Traffic (bytes per tick; × {tps:.0} for a second):\n");
    println!("{}", prof::table(&values));
    let mean = |name: &str| {
        let v = prof::series(name);
        v.iter().map(|x| *x as f64).sum::<f64>() / v.len().max(1) as f64
    };
    println!(
        "Per client: **{:.1} KB/s down** (the busiest: {:.1} KB/s), **{:.1} KB/s up**.\n",
        mean("down per client (B/tick)") * tps / 1000.0,
        mean("down, the busiest client (B/tick)") * tps / 1000.0,
        mean("up per client (B/tick)") * tps / 1000.0,
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// The most memory the process held (MB), where the system tells it (Linux).
fn peak_mb() -> Option<f64> {
    let s = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = s.lines().find(|l| l.starts_with("VmHWM:"))?;
    let kb: f64 = line.split_whitespace().nth(1)?.parse().ok()?;
    Some(kb / 1024.0)
}
