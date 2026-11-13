//! UDP networking and the fixed-rate server loop.

use crate::game::{Dest, Game};
use crate::AdminMsg;
use serde_json::json;
use std::collections::HashMap;
use std::io::ErrorKind;
use std::net::{SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Receiver;
use std::sync::Arc;
use std::time::{Duration, Instant};
use toon_shared::consts::*;
use toon_shared::net::Channel;
use toon_shared::protocol::{C2S, S2C};

const TIMEOUT: f64 = 12.0;

struct Conn {
    chan: Channel,
    id: Option<u8>,
    last_recv: f64,
    closing: Option<f64>,
    /// Language announced before `Hello`.
    lang: toon_shared::lang::Lang,
}

pub fn info_json(g: &Game) -> serde_json::Value {
    json!({
        "name": g.cfg.name,
        "map": g.map.name,
        "map_id": g.map.id.short(),
        "mode": g.mode.name(),
        "mode_id": g.mode.short(),
        "players": g.players.iter().flatten().filter(|p| !p.is_bot()).count(),
        "bots": g.players.iter().flatten().filter(|p| p.is_bot()).count(),
        "max_players": g.max_players(),
        "password": !g.cfg.password.is_empty(),
        "state": format!("{:?}", g.state),
        "scores": g.scores,
        "time_left": g.time_left(),
        "version": PROTOCOL_VERSION,
        "game_version": GAME_VERSION,
        "port": g.cfg.port,
    })
}

pub fn state_json(g: &Game) -> serde_json::Value {
    let mut v = state_json_raw(g);
    crate::config::round_json(&mut v);
    v
}

fn state_json_raw(g: &Game) -> serde_json::Value {
    let players: Vec<_> = g
        .players
        .iter()
        .flatten()
        .map(|p| {
            json!({
                "id": p.id, "name": p.name, "team": p.team.name(), "team_id": p.team.index(),
                "score": p.score, "kills": p.kills, "deaths": p.deaths, "assists": p.assists,
                "ping": if p.is_bot() { 0 } else { (p.ping * 1000.0) as u32 },
                "ip": p.ip, "bot": p.is_bot(), "alive": p.alive,
            })
        })
        .collect();
    json!({
        "info": info_json(g),
        "players": players,
        "settings": {
            "score_limit": g.cfg.tdm.score_limit,
            "time_limit": g.cfg.tdm.time_limit,
            "sab_time": g.cfg.sab.time_limit,
            "rounds": g.cfg.sab.rounds_to_win,
            "bomb_timer": g.cfg.sab.bomb_timer,
            "friendly_fire": g.cfg.friendly_fire,
            "auto_balance": g.cfg.auto_balance,
            "bots": g.cfg.bots,
            "bot_skill": g.cfg.bot_skill,
            "spawn_protection": g.cfg.spawn_protection,
            "max_players": g.cfg.max_players,
            "br_bots": g.cfg.br.bots,
            "br_max_players": g.cfg.br.max_players,
            "motd": g.cfg.motd,
            "has_password": !g.cfg.password.is_empty(),
        },
        "rotation": g.cfg.rotation.iter().map(|r| format!("{} {}", r.map, r.mode)).collect::<Vec<_>>(),
        "rotation_idx": g.rotation_idx,
        "bans": g.bans.list,
        "log": g.log.iter().rev().take(150).collect::<Vec<_>>(),
        "tuning": g.tuning,
        "tuning_fields": {
            "movement": toon_shared::tuning::MoveTuning::FIELDS,
            "grenade": toon_shared::tuning::GrenadeTuning::FIELDS,
            "streaks": toon_shared::tuning::StreakTuning::FIELDS,
            "weapon": toon_shared::tuning::WeaponStats::FIELDS,
        },
        "weapon_names": toon_shared::weapons::WeaponId::all().map(|w| (w.key(), w.def().name)).collect::<Vec<_>>(),
    })
}

pub fn run(socket: UdpSocket, mut game: Game, admin_rx: Receiver<AdminMsg>, stop: Arc<AtomicBool>) {
    socket.set_nonblocking(true).expect("nonblocking socket");
    let mut conns: HashMap<SocketAddr, Conn> = HashMap::new();
    let start = Instant::now();
    let tick = Duration::from_secs_f64(1.0 / TICK_RATE as f64);
    let mut next = Instant::now();
    let mut buf = vec![0u8; 8192];

    while !stop.load(Ordering::Relaxed) {
        let now = start.elapsed().as_secs_f64();

        // ---- receive
        loop {
            match socket.recv_from(&mut buf) {
                Ok((n, addr)) => {
                    let data = &buf[..n];
                    if data.starts_with(QUERY_MAGIC) {
                        let mut reply = QUERY_REPLY_MAGIC.to_vec();
                        reply.extend_from_slice(info_json(&game).to_string().as_bytes());
                        let _ = socket.send_to(&reply, addr);
                        continue;
                    }
                    let fresh = !conns.contains_key(&addr);
                    if fresh {
                        if conns.len() >= 128 {
                            continue;
                        }
                        conns.insert(
                            addr,
                            Conn {
                                chan: Channel::new(),
                                lang: Default::default(),
                                id: None,
                                last_recv: now,
                                closing: None,
                            },
                        );
                    }
                    let conn = conns.get_mut(&addr).unwrap();
                    let Some(msgs) = conn.chan.receive::<C2S>(data) else {
                        // junk from an unknown address takes no connection slot
                        if fresh {
                            conns.remove(&addr);
                        }
                        continue;
                    };
                    conn.last_recv = now;
                    for m in msgs {
                        match (conn.id, m) {
                            (
                                None,
                                C2S::Hello {
                                    name,
                                    password,
                                    version,
                                    game: client_game,
                                    key,
                                },
                            ) => {
                                if conn.closing.is_some() {
                                    continue;
                                }
                                match game.connect_as(
                                    addr,
                                    &name,
                                    &password,
                                    version,
                                    &client_game,
                                    conn.lang,
                                ) {
                                    Ok(id) => {
                                        conn.id = Some(id);
                                        game.set_key(id, key);
                                    }
                                    Err(reason) => {
                                        println!("[net] відмова {addr}: {reason}");
                                        conn.chan.send_reliable(&S2C::Reject(reason));
                                        conn.closing = Some(now + 1.0);
                                    }
                                }
                            }
                            (None, C2S::Lang(l)) => conn.lang = l,
                            (None, _) => {}
                            (Some(id), C2S::Disconnect) => {
                                game.remove_player(id, None);
                                conn.id = None;
                                conn.closing = Some(now);
                            }
                            (Some(id), m) => game.handle(id, m),
                        }
                    }
                }
                Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                Err(e)
                    if e.kind() == ErrorKind::ConnectionReset
                        || e.kind() == ErrorKind::Interrupted =>
                {
                    continue
                }
                Err(e) => {
                    eprintln!("[net] помилка сокета: {e}");
                    break;
                }
            }
        }

        // ---- admin requests
        while let Ok(req) = admin_rx.try_recv() {
            match req {
                AdminMsg::Cmd(cmd, reply) => {
                    game.logf(format!("Адмін-команда: {cmd}"));
                    let out = crate::admin::exec(&mut game, &cmd);
                    let _ = reply.send(out);
                }
                AdminMsg::State(reply) => {
                    let _ = reply.send(state_json(&game));
                }
                AdminMsg::Info(reply) => {
                    let _ = reply.send(info_json(&game));
                }
                AdminMsg::Top(reply) => {
                    let boards: Vec<serde_json::Value> = game
                        .br_stats
                        .boards(None)
                        .iter()
                        .map(|b| {
                            json!({
                                "title": b.kind.name(),
                                "rows": b.rows.iter().map(|(n, v)| json!([n, b.kind.format(*v)])).collect::<Vec<_>>(),
                            })
                        })
                        .collect();
                    let _ = reply.send(json!(boards));
                }
                AdminMsg::CheckRcon(pass, reply) => {
                    let _ = reply
                        .send(!game.cfg.rcon_password.is_empty() && pass == game.cfg.rcon_password);
                }
            }
        }

        // ---- simulate
        game.tick(TICK_DT);

        // ---- kicks
        for (id, reason) in std::mem::take(&mut game.kicked) {
            if let Some(c) = conns.values_mut().find(|c| c.id == Some(id)) {
                c.chan.send_reliable(&S2C::Reject(reason.clone()));
                c.id = None;
                c.closing = Some(now + 0.6);
            }
            game.remove_player(id, Some(&reason));
        }

        // ---- route messages
        let outbox = std::mem::take(&mut game.outbox);
        for (dest, msg, reliable) in &outbox {
            for c in conns.values_mut() {
                let Some(id) = c.id else { continue };
                let ok = match dest {
                    Dest::All => true,
                    Dest::One(t) => *t == id,
                    Dest::Team(t) => game.p(id).map(|p| p.team == *t).unwrap_or(false),
                };
                if ok {
                    if *reliable {
                        c.chan.send_reliable(msg);
                    } else {
                        c.chan.send_unreliable(msg);
                    }
                }
            }
        }
        for c in conns.values_mut() {
            if let Some(id) = c.id {
                if game.p(id).is_some() {
                    c.chan
                        .send_unreliable(&S2C::Snapshot(Box::new(game.snapshot_for(id))));
                }
            }
        }
        game.shots.clear();
        for (addr, c) in conns.iter_mut() {
            let pkt = c.chan.build_packet(now);
            let _ = socket.send_to(&pkt, addr);
            for more in c.chan.spill.drain(..) {
                let _ = socket.send_to(&more, addr);
            }
        }

        // ---- timeouts / closing
        let mut dead = Vec::new();
        for (addr, c) in conns.iter() {
            if c.closing.map(|t| now >= t).unwrap_or(false) || now - c.last_recv > TIMEOUT {
                dead.push(*addr);
            }
        }
        for addr in dead {
            if let Some(c) = conns.remove(&addr) {
                if let Some(id) = c.id {
                    game.remove_player(id, Some("втрачено з'єднання"));
                }
            }
        }

        // ---- sleep until the next tick
        next += tick;
        let now_i = Instant::now();
        if next > now_i {
            std::thread::sleep(next - now_i);
        } else if now_i - next > Duration::from_millis(250) {
            next = now_i; // we fell behind; don't try to catch up
        }
    }

    // shutting down: tell everyone
    for c in conns.values_mut() {
        let text = toon_shared::lang::with(c.lang, || {
            toon_shared::lang::t("Сервер вимкнено", "The server shut down")
        });
        c.chan.send_reliable(&S2C::Reject(text.into()));
    }
    for _ in 0..3 {
        for (addr, c) in conns.iter_mut() {
            let pkt = c.chan.build_packet(f64::MAX / 2.0);
            let _ = socket.send_to(&pkt, addr);
        }
        std::thread::sleep(Duration::from_millis(30));
    }
}
