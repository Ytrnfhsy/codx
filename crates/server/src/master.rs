//! The public server list.
//!
//! * The master (`toon-server --master`) keeps the list: game servers announce themselves
//!   to it over HTTP every minute, and it puts one on the list only after asking it over
//!   UDP itself (so it is a real Toon Warfare server and can be reached from outside).
//!   A server it has not heard from for three minutes drops off.
//! * A game server with `"public": true` announces itself to its `master`.
//! * The game's server browser asks the master for the list and then each server for its
//!   name, map and players, as it does any other.

use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::net::{IpAddr, SocketAddr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};
use toon_shared::consts::{PROTOCOL_VERSION, QUERY_MAGIC, QUERY_REPLY_MAGIC};

/// How often a game server says it is still there.
const ANNOUNCE_EVERY: Duration = Duration::from_secs(60);
/// A server not heard from for this long drops off the list.
const FORGET_AFTER: Duration = Duration::from_secs(180);
/// At most this many servers from one address.
const PER_IP: usize = 8;

struct Listed {
    last: Instant,
    protocol: u16,
}

type List = Arc<Mutex<HashMap<SocketAddr, Listed>>>;

/// The master: serves the list on `bind:port` (TCP) until the process ends.
/// (`bind` "::" listens on IPv6 and, where the system allows it, IPv4 at once: a tunnel or
/// proxy that reaches "localhost" over either finds it; on a system without IPv6 it falls
/// back to IPv4 alone.)
pub fn run_master(bind: &str, port: u16) -> Result<(), String> {
    let at = |b: &str| {
        if b.contains(':') {
            format!("[{b}]:{port}")
        } else {
            format!("{b}:{port}")
        }
    };
    let (server, used) = match Server::http(at(bind)) {
        Ok(s) => (s, at(bind)),
        Err(_) if bind == "::" => (
            Server::http(at("0.0.0.0")).map_err(|e| e.to_string())?,
            at("0.0.0.0"),
        ),
        Err(e) => return Err(e.to_string()),
    };
    println!("[master] каталог серверів слухає TCP {used}");
    let list: List = Default::default();
    for req in server.incoming_requests() {
        let list = list.clone();
        // (each on its own thread: checking a server can take a couple of seconds)
        let _ = std::thread::Builder::new()
            .name("master-req".into())
            .spawn(move || handle(&list, req));
    }
    Ok(())
}

fn json_resp(req: Request, code: u16, v: Value) {
    let h = Header::from_bytes("Content-Type", "application/json; charset=utf-8").unwrap();
    let _ = req.respond(
        Response::from_string(v.to_string())
            .with_status_code(code)
            .with_header(h),
    );
}

/// The list's address as a URL: "https://host" or "http://host:port" as given, a bare
/// "host:port" over plain HTTP.
pub fn base_url(master: &str) -> String {
    let m = master.trim().trim_end_matches('/');
    if m.contains("://") {
        m.to_string()
    } else {
        format!("http://{m}")
    }
}

/// Where a request came from. Behind a proxy or a Cloudflare tunnel on this machine, the
/// address it passes on (Cloudflare's own header first: it cannot be set by the sender).
fn client_ip(req: &Request) -> Option<IpAddr> {
    let direct = req.remote_addr()?.ip();
    if direct.is_loopback() {
        let header = |name: &'static str| -> Option<IpAddr> {
            req.headers()
                .iter()
                .find(|h| h.field.equiv(name))
                .and_then(|h| h.value.as_str().split(',').next()?.trim().parse().ok())
        };
        if let Some(ip) = header("CF-Connecting-IP")
            .or_else(|| header("X-Real-IP"))
            .or_else(|| header("X-Forwarded-For"))
        {
            return Some(ip);
        }
    }
    Some(direct)
}

fn handle(list: &List, mut req: Request) {
    let url = req.url().split('?').next().unwrap_or("/").to_string();
    match (req.method().clone(), url.as_str()) {
        (Method::Post, "/announce") => {
            let Some(ip) = client_ip(&req) else {
                return json_resp(req, 400, json!({"error": "no address"}));
            };
            let mut body = String::new();
            let _ = req.as_reader().take(4096).read_to_string(&mut body);
            let v: Value = serde_json::from_str(&body).unwrap_or(Value::Null);
            let Some(port) = v["port"].as_u64().filter(|p| (1..=65535).contains(p)) else {
                return json_resp(req, 400, json!({"error": "port?"}));
            };
            let protocol = v["protocol"].as_u64().unwrap_or(0) as u16;
            let addr = SocketAddr::new(ip, port as u16);
            {
                let mut l = list.lock().unwrap();
                l.retain(|_, s| s.last.elapsed() < FORGET_AFTER);
                let mine = l.keys().filter(|a| a.ip() == ip && **a != addr).count();
                if mine >= PER_IP {
                    return json_resp(
                        req,
                        429,
                        json!({"listed": false, "error": "too many servers from this address"}),
                    );
                }
            }
            // a real server, reachable from outside?
            if !answers(addr) {
                return json_resp(
                    req,
                    200,
                    json!({"listed": false, "addr": addr.to_string(), "error": "no answer over UDP"}),
                );
            }
            list.lock().unwrap().insert(
                addr,
                Listed {
                    last: Instant::now(),
                    protocol,
                },
            );
            json_resp(req, 200, json!({"listed": true, "addr": addr.to_string()}))
        }
        (Method::Get, "/servers") | (Method::Head, "/servers") => {
            let mut l = list.lock().unwrap();
            l.retain(|_, s| s.last.elapsed() < FORGET_AFTER);
            let servers: Vec<Value> = l
                .iter()
                .map(|(a, s)| json!({"addr": a.to_string(), "protocol": s.protocol}))
                .collect();
            json_resp(req, 200, json!({ "servers": servers }))
        }
        (Method::Get, "/") => {
            let n = list.lock().unwrap().len();
            json_resp(
                req,
                200,
                json!({"service": "Toon Warfare server list", "servers": n, "list": "/servers"}),
            )
        }
        _ => json_resp(req, 404, json!({"error": "not found"})),
    }
}

/// Asks a game server over UDP, as the game's browser does.
fn answers(addr: SocketAddr) -> bool {
    let bind = if addr.is_ipv4() {
        "0.0.0.0:0"
    } else {
        "[::]:0"
    };
    let Ok(s) = UdpSocket::bind(bind) else {
        return false;
    };
    let _ = s.set_read_timeout(Some(Duration::from_millis(700)));
    let mut buf = [0u8; 4096];
    // (three tries: one lost datagram is no reason to leave a server off)
    for _ in 0..3 {
        if s.send_to(QUERY_MAGIC, addr).is_err() {
            return false;
        }
        while let Ok((n, from)) = s.recv_from(&mut buf) {
            if from == addr && n >= 4 && &buf[..4] == QUERY_REPLY_MAGIC {
                return true;
            }
        }
    }
    false
}

/// A game server's announcements: every minute while `on`, to `master` ("host:port").
/// It says in the console when it gets on the list, and when the master cannot reach it.
pub fn announcer(master: String, port: u16, on: Arc<AtomicBool>) {
    let _ = std::thread::Builder::new()
        .name("announce".into())
        .spawn(move || {
            // (the first time, give the game a moment to answer the master's check)
            std::thread::sleep(Duration::from_secs(2));
            let mut last: Option<bool> = None;
            loop {
                if on.load(Ordering::Relaxed) {
                    let url = format!("{}/announce", base_url(&master));
                    let body = json!({ "port": port, "protocol": PROTOCOL_VERSION }).to_string();
                    let result = ureq::post(&url)
                        .timeout(Duration::from_secs(10))
                        .set("Content-Type", "application/json")
                        .send_string(&body)
                        .ok()
                        .and_then(|r| r.into_string().ok())
                        .and_then(|t| serde_json::from_str::<Value>(&t).ok());
                    let listed = result.as_ref().and_then(|v| v["listed"].as_bool());
                    if listed != last {
                        match (&result, listed) {
                            (_, Some(true)) => println!(
                                "[master] ✓ сервер у загальному каталозі ({master}) як {}",
                                result.as_ref().and_then(|v| v["addr"].as_str()).unwrap_or("?")
                            ),
                            (Some(v), _) => println!(
                                "[master] ⚠ каталог не може достукатися до сервера ({}): відкрий UDP-порт {port} у брандмауері / на роутері",
                                v["error"].as_str().unwrap_or("?")
                            ),
                            (None, _) => println!(
                                "[master] ⚠ каталог {master} недоступний, спробую ще"
                            ),
                        }
                        last = listed;
                    }
                } else {
                    last = None;
                }
                std::thread::sleep(ANNOUNCE_EVERY);
            }
        });
}

/// The game's browser: the servers the master lists (for this game's protocol).
pub fn fetch_list(master: &str) -> Result<Vec<String>, String> {
    let url = format!("{}/servers", base_url(master));
    let text = ureq::get(&url)
        .timeout(Duration::from_secs(5))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let v: Value = serde_json::from_str(&text).map_err(|e| e.to_string())?;
    Ok(v["servers"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter(|s| s["protocol"].as_u64() == Some(PROTOCOL_VERSION as u64))
                .filter_map(|s| s["addr"].as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default())
}
