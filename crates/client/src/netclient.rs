//! UDP client side of the protocol, plus server info queries (LAN discovery / favourites).

use std::net::{SocketAddr, ToSocketAddrs, UdpSocket};
use std::time::Instant;
use toon_shared::consts::*;
use toon_shared::net::Channel;
use toon_shared::protocol::{C2S, S2C};

pub struct NetClient {
    sock: UdpSocket,
    pub server: SocketAddr,
    chan: Channel,
    pub last_recv: Instant,
    last_send: Instant,
    started: Instant,
}

pub fn resolve(addr: &str) -> Result<SocketAddr, String> {
    let a = addr.trim();
    let with_port = if a.contains(':') && !a.ends_with(']') {
        a.to_string()
    } else {
        format!("{a}:{DEFAULT_PORT}")
    };
    with_port
        .to_socket_addrs()
        .map_err(|e| toon_shared::tr!("Невірна адреса «{a}»: {e}", "Bad address \"{a}\": {e}"))?
        .find(|s| s.is_ipv4())
        .or_else(|| with_port.to_socket_addrs().ok().and_then(|mut i| i.next()))
        .ok_or_else(|| toon_shared::tr!("Не вдалося знайти «{a}»", "Could not find \"{a}\""))
}

/// This installation's own number (from the settings), sent when connecting.
pub static PLAYER_KEY: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

impl NetClient {
    pub fn connect(addr: &str, name: &str, password: &str) -> Result<NetClient, String> {
        let server = resolve(addr)?;
        let bind = if server.is_ipv4() {
            "0.0.0.0:0"
        } else {
            "[::]:0"
        };
        let sock = UdpSocket::bind(bind).map_err(|e| e.to_string())?;
        sock.set_nonblocking(true).map_err(|e| e.to_string())?;
        let mut c = NetClient {
            sock,
            server,
            chan: Channel::new(),
            last_recv: Instant::now(),
            last_send: Instant::now(),
            started: Instant::now(),
        };
        c.chan.send_reliable(&C2S::Lang(toon_shared::lang::get()));
        c.chan.send_reliable(&C2S::Hello {
            name: name.to_string(),
            password: password.to_string(),
            version: PROTOCOL_VERSION,
            game: toon_shared::consts::GAME_VERSION.to_string(),
            key: PLAYER_KEY.load(std::sync::atomic::Ordering::Relaxed),
        });
        c.flush();
        Ok(c)
    }

    pub fn send(&mut self, m: C2S) {
        self.chan.send_reliable(&m);
    }

    pub fn send_unreliable(&mut self, m: C2S) {
        self.chan.send_unreliable(&m);
    }

    /// Sends a datagram now.
    pub fn flush(&mut self) {
        let now = self.started.elapsed().as_secs_f64();
        let pkt = self.chan.build_packet(now);
        let _ = self.sock.send_to(&pkt, self.server);
        for more in self.chan.spill.drain(..) {
            let _ = self.sock.send_to(&more, self.server);
        }
        self.last_send = Instant::now();
    }

    /// Sends at most every `interval` seconds.
    pub fn maybe_flush(&mut self, interval: f32) {
        if self.last_send.elapsed().as_secs_f32() >= interval {
            self.flush();
        }
    }

    pub fn poll(&mut self) -> Vec<S2C> {
        let mut out = Vec::new();
        let mut buf = [0u8; 8192];
        loop {
            match self.sock.recv_from(&mut buf) {
                Ok((n, from)) => {
                    if from != self.server {
                        continue;
                    }
                    if let Some(msgs) = self.chan.receive::<S2C>(&buf[..n]) {
                        self.last_recv = Instant::now();
                        out.extend(msgs);
                    }
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => break,
            }
        }
        out
    }

    pub fn disconnect(&mut self) {
        self.chan.send_reliable(&C2S::Disconnect);
        for _ in 0..3 {
            self.flush();
        }
    }
}

#[derive(Clone, Debug)]
pub struct ServerInfo {
    pub addr: String,
    pub name: String,
    pub map: String,
    pub mode: String,
    /// Short ids ("wreck", "tdm"), to show the names in our own language.
    pub map_id: String,
    pub mode_id: String,
    pub players: u64,
    pub bots: u64,
    pub max: u64,
    pub password: bool,
    pub ping_ms: u32,
    /// Where the answer came from.
    from: Option<SocketAddr>,
}

impl ServerInfo {
    /// Map name in our language (the server's own name for maps we do not know).
    pub fn map_name(&self) -> &str {
        toon_shared::map::MapId::parse(&self.map_id).map_or(self.map.as_str(), |m| m.name())
    }
    pub fn mode_name(&self) -> &str {
        toon_shared::Mode::parse(&self.mode_id).map_or(self.mode.as_str(), |m| m.name())
    }
}

/// Non-blocking server browser: sends info queries and collects replies.
pub struct Browser {
    sock: Option<UdpSocket>,
    sent: Instant,
    pub results: Vec<ServerInfo>,
    /// Addresses we asked (as typed) and where they resolved to, so a reply is listed
    /// under the name the player knows it by (a domain stays a domain). Filled by
    /// background threads: looking up a domain can take seconds.
    asked: std::sync::Arc<std::sync::Mutex<Vec<(SocketAddr, String)>>>,
}

impl Browser {
    pub fn new() -> Browser {
        let sock = UdpSocket::bind("0.0.0.0:0").ok();
        if let Some(s) = &sock {
            let _ = s.set_nonblocking(true);
            let _ = s.set_broadcast(true);
        }
        Browser {
            sock,
            sent: Instant::now(),
            results: Vec::new(),
            asked: Default::default(),
        }
    }

    /// Queries the given addresses and broadcasts on the LAN.
    pub fn refresh(&mut self, addrs: &[String]) {
        self.results.clear();
        // a fresh list: answers to an earlier refresh still being looked up are dropped
        self.asked = Default::default();
        self.sent = Instant::now();
        let Some(s) = &self.sock else { return };
        for a in addrs {
            let (Ok(sock), asked, a) = (s.try_clone(), self.asked.clone(), a.clone()) else {
                continue;
            };
            std::thread::spawn(move || {
                if let Ok(sa) = resolve(&a) {
                    if let Ok(mut l) = asked.lock() {
                        l.push((sa, a.trim().to_string()));
                    }
                    let _ = sock.send_to(QUERY_MAGIC, sa);
                }
            });
        }
        let _ = s.send_to(QUERY_MAGIC, ("255.255.255.255", DEFAULT_PORT));
    }

    /// Seconds since the last refresh (servers that have not answered by now are down).
    pub fn age(&self) -> f32 {
        self.sent.elapsed().as_secs_f32()
    }

    pub fn poll(&mut self) {
        let Some(s) = &self.sock else { return };
        let mut buf = [0u8; 4096];
        while let Ok((n, from)) = s.recv_from(&mut buf) {
            if n < 4 || &buf[..4] != QUERY_REPLY_MAGIC {
                continue;
            }
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf[4..n]) else {
                continue;
            };
            let port = v["port"].as_u64().unwrap_or(from.port() as u64);
            let typed = self
                .asked
                .lock()
                .ok()
                .and_then(|l| l.iter().find(|(sa, _)| *sa == from).map(|(_, t)| t.clone()));
            // one row per server: a LAN broadcast answer may come before the direct one
            if let Some(r) = self.results.iter_mut().find(|r| r.from == Some(from)) {
                if let Some(t) = typed {
                    r.addr = t;
                }
                continue;
            }
            let addr = typed.unwrap_or_else(|| format!("{}:{}", from.ip(), port));
            self.results.push(ServerInfo {
                addr,
                name: v["name"].as_str().unwrap_or("?").to_string(),
                map: v["map"].as_str().unwrap_or("?").to_string(),
                mode: v["mode"].as_str().unwrap_or("?").to_string(),
                map_id: v["map_id"].as_str().unwrap_or("").to_string(),
                mode_id: v["mode_id"].as_str().unwrap_or("").to_string(),
                players: v["players"].as_u64().unwrap_or(0),
                bots: v["bots"].as_u64().unwrap_or(0),
                max: v["max_players"].as_u64().unwrap_or(0),
                password: v["password"].as_bool().unwrap_or(false),
                ping_ms: self.sent.elapsed().as_millis() as u32,
                from: Some(from),
            });
        }
    }
}
