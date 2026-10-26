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
    let with_port = if a.contains(':') && !a.ends_with(']') { a.to_string() } else { format!("{a}:{DEFAULT_PORT}") };
    with_port
        .to_socket_addrs()
        .map_err(|e| format!("Невірна адреса «{a}»: {e}"))?
        .find(|s| s.is_ipv4())
        .or_else(|| with_port.to_socket_addrs().ok().and_then(|mut i| i.next()))
        .ok_or_else(|| format!("Не вдалося знайти «{a}»"))
}

impl NetClient {
    pub fn connect(addr: &str, name: &str, password: &str) -> Result<NetClient, String> {
        let server = resolve(addr)?;
        let bind = if server.is_ipv4() { "0.0.0.0:0" } else { "[::]:0" };
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
        c.chan.send_reliable(&C2S::Hello { name: name.to_string(), password: password.to_string(), version: PROTOCOL_VERSION });
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
    pub players: u64,
    pub bots: u64,
    pub max: u64,
    pub password: bool,
    pub ping_ms: u32,
}

/// Non-blocking server browser: sends info queries and collects replies.
pub struct Browser {
    sock: Option<UdpSocket>,
    sent: Instant,
    pub results: Vec<ServerInfo>,
}

impl Browser {
    pub fn new() -> Browser {
        let sock = UdpSocket::bind("0.0.0.0:0").ok();
        if let Some(s) = &sock {
            let _ = s.set_nonblocking(true);
            let _ = s.set_broadcast(true);
        }
        Browser { sock, sent: Instant::now(), results: Vec::new() }
    }

    /// Queries the given addresses and broadcasts on the LAN.
    pub fn refresh(&mut self, addrs: &[String]) {
        self.results.clear();
        self.sent = Instant::now();
        let Some(s) = &self.sock else { return };
        for a in addrs {
            if let Ok(sa) = resolve(a) {
                let _ = s.send_to(QUERY_MAGIC, sa);
            }
        }
        let _ = s.send_to(QUERY_MAGIC, ("255.255.255.255", DEFAULT_PORT));
    }

    pub fn poll(&mut self) {
        let Some(s) = &self.sock else { return };
        let mut buf = [0u8; 4096];
        while let Ok((n, from)) = s.recv_from(&mut buf) {
            if n < 4 || &buf[..4] != QUERY_REPLY_MAGIC {
                continue;
            }
            let Ok(v) = serde_json::from_slice::<serde_json::Value>(&buf[4..n]) else { continue };
            let port = v["port"].as_u64().unwrap_or(from.port() as u64);
            let addr = format!("{}:{}", from.ip(), port);
            if self.results.iter().any(|r| r.addr == addr) {
                continue;
            }
            self.results.push(ServerInfo {
                addr,
                name: v["name"].as_str().unwrap_or("?").to_string(),
                map: v["map"].as_str().unwrap_or("?").to_string(),
                mode: v["mode"].as_str().unwrap_or("?").to_string(),
                players: v["players"].as_u64().unwrap_or(0),
                bots: v["bots"].as_u64().unwrap_or(0),
                max: v["max_players"].as_u64().unwrap_or(0),
                password: v["password"].as_bool().unwrap_or(false),
                ping_ms: self.sent.elapsed().as_millis() as u32,
            });
        }
    }
}
