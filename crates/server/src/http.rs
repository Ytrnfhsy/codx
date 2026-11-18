//! Tiny HTTP server: public status page, `/api/info`, and the admin web panel (`/admin`).

use crate::AdminMsg;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::sync::mpsc::{channel, Sender};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};

const ADMIN_HTML: &str = include_str!("../web/admin.html");
const INDEX_HTML: &str = include_str!("../web/index.html");
const PLAYERS_HTML: &str = include_str!("../web/players.html");

const TOKEN_TTL: Duration = Duration::from_secs(12 * 3600);

struct Ctx {
    admin: Sender<AdminMsg>,
    tokens: HashMap<String, Instant>,
    fails: HashMap<String, (u32, Instant)>,
}

/// A session token: 128 bits from the operating system's secure random source (not the
/// game's generator, whose state could be worked out from when the server started).
fn new_token() -> Option<String> {
    let mut b = [0u8; 16];
    getrandom::getrandom(&mut b).ok()?;
    Some(b.iter().map(|x| format!("{x:02x}")).collect())
}

fn header(k: &str, v: &str) -> Header {
    Header::from_bytes(k.as_bytes(), v.as_bytes()).unwrap()
}

fn respond(req: Request, code: u16, ctype: &str, body: String) {
    let r = Response::from_string(body)
        .with_status_code(code)
        .with_header(header("Content-Type", ctype))
        .with_header(header("Cache-Control", "no-store"))
        .with_header(header("X-Content-Type-Options", "nosniff"));
    let _ = req.respond(r);
}

fn json_resp(req: Request, code: u16, v: Value) {
    respond(req, code, "application/json; charset=utf-8", v.to_string());
}

fn ask<T>(ctx: &Ctx, make: impl FnOnce(Sender<T>) -> AdminMsg) -> Option<T> {
    let (tx, rx) = channel();
    ctx.admin.send(make(tx)).ok()?;
    rx.recv_timeout(Duration::from_secs(3)).ok()
}

fn token_ok(ctx: &mut Ctx, req: &Request) -> bool {
    let tok = req
        .headers()
        .iter()
        .find(|h| h.field.equiv("X-Token"))
        .map(|h| h.value.as_str().to_string());
    let now = Instant::now();
    ctx.tokens.retain(|_, t| now.duration_since(*t) < TOKEN_TTL);
    tok.map(|t| ctx.tokens.contains_key(&t)).unwrap_or(false)
}

fn body_json(req: &mut Request) -> Value {
    let mut s = String::new();
    let _ = req.as_reader().take(64 * 1024).read_to_string(&mut s);
    serde_json::from_str(&s).unwrap_or(Value::Null)
}

/// A query string parameter, percent-decoded (`?key=%23ab12` gives "#ab12").
fn query(url: &str, name: &str) -> Option<String> {
    let q = url.split_once('?')?.1;
    let raw = q.split('&').find_map(|kv| {
        kv.split_once('=')
            .filter(|(k, _)| *k == name)
            .map(|(_, v)| v)
    })?;
    let b = raw.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(std::str::from_utf8(&b[i + 1..i + 3]).ok()?, 16) {
                    Ok(v) => out.push(v),
                    Err(_) => return None,
                }
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}

fn handle(ctx: &mut Ctx, mut req: Request) {
    let full_url = req.url().to_string();
    let url = full_url.split('?').next().unwrap_or("/").to_string();
    let ip = req
        .remote_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();
    // HEAD (curl -I, health checks) is answered like GET; the body is left out
    let method = match req.method() {
        Method::Head => Method::Get,
        m => m.clone(),
    };
    match (method, url.as_str()) {
        (Method::Get, "/") => respond(req, 200, "text/html; charset=utf-8", INDEX_HTML.to_string()),
        (Method::Get, "/admin") | (Method::Get, "/admin/") => {
            respond(req, 200, "text/html; charset=utf-8", ADMIN_HTML.to_string())
        }
        (Method::Get, "/admin/players") | (Method::Get, "/admin/players/") => respond(
            req,
            200,
            "text/html; charset=utf-8",
            PLAYERS_HTML.to_string(),
        ),
        (Method::Get, "/api/players") => {
            if !token_ok(ctx, &req) {
                return json_resp(req, 401, json!({"error": "unauthorized"}));
            }
            match ask(ctx, AdminMsg::Players) {
                Some(v) => json_resp(req, 200, v),
                None => json_resp(req, 503, json!({"error": "server busy"})),
            }
        }
        (Method::Get, "/api/player") => {
            if !token_ok(ctx, &req) {
                return json_resp(req, 401, json!({"error": "unauthorized"}));
            }
            let Some(key) = query(&full_url, "key") else {
                return json_resp(req, 400, json!({"error": "key?"}));
            };
            match ask(ctx, |tx| AdminMsg::Player(key, tx)) {
                Some(v) => json_resp(req, 200, v),
                None => json_resp(req, 503, json!({"error": "server busy"})),
            }
        }
        (Method::Get, "/api/top") => match ask(ctx, AdminMsg::Top) {
            Some(v) => json_resp(req, 200, v),
            None => json_resp(req, 503, json!({"error": "server busy"})),
        },
        (Method::Get, "/api/info") => match ask(ctx, AdminMsg::Info) {
            Some(v) => json_resp(req, 200, v),
            None => json_resp(req, 503, json!({"error": "server busy"})),
        },
        (Method::Post, "/api/login") => {
            let now = Instant::now();
            if let Some((n, t)) = ctx.fails.get(&ip) {
                if *n >= 5 && now.duration_since(*t) < Duration::from_secs(60) {
                    return json_resp(
                        req,
                        429,
                        json!({"error": "Забагато спроб. Зачекай хвилину."}),
                    );
                }
            }
            let body = body_json(&mut req);
            let pass = body
                .get("password")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let ok = ask(ctx, |tx| AdminMsg::CheckRcon(pass, tx)).unwrap_or(false);
            if ok {
                ctx.fails.remove(&ip);
                let Some(token) = new_token() else {
                    return json_resp(req, 500, json!({"error": "Немає джерела випадкових чисел"}));
                };
                ctx.tokens.insert(token.clone(), now);
                println!("[http] адмін увійшов з {ip}");
                json_resp(req, 200, json!({ "token": token }))
            } else {
                let e = ctx.fails.entry(ip.clone()).or_insert((0, now));
                if now.duration_since(e.1) > Duration::from_secs(60) {
                    *e = (0, now);
                }
                e.0 += 1;
                e.1 = now;
                std::thread::sleep(Duration::from_millis(400));
                json_resp(
                    req,
                    401,
                    json!({"error": "Невірний пароль (або rcon_password не задано в конфігурації)"}),
                )
            }
        }
        (Method::Post, "/api/logout") => {
            if let Some(t) = req
                .headers()
                .iter()
                .find(|h| h.field.equiv("X-Token"))
                .map(|h| h.value.as_str().to_string())
            {
                ctx.tokens.remove(&t);
            }
            json_resp(req, 200, json!({"ok": true}))
        }
        (Method::Get, "/api/state") => {
            if !token_ok(ctx, &req) {
                return json_resp(req, 401, json!({"error": "unauthorized"}));
            }
            match ask(ctx, AdminMsg::State) {
                Some(v) => json_resp(req, 200, v),
                None => json_resp(req, 503, json!({"error": "server busy"})),
            }
        }
        (Method::Post, "/api/cmd") => {
            if !token_ok(ctx, &req) {
                return json_resp(req, 401, json!({"error": "unauthorized"}));
            }
            let body = body_json(&mut req);
            let cmd = body
                .get("cmd")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            match ask(ctx, |tx| AdminMsg::Cmd(cmd, tx)) {
                Some(out) => json_resp(req, 200, json!({ "output": out })),
                None => json_resp(req, 503, json!({"error": "server busy"})),
            }
        }
        _ => respond(req, 404, "text/plain; charset=utf-8", "404".into()),
    }
}

pub fn spawn(bind: &str, port: u16, admin: Sender<AdminMsg>) -> Result<JoinHandle<()>, String> {
    let server = Server::http(format!("{bind}:{port}")).map_err(|e| e.to_string())?;
    Ok(std::thread::Builder::new()
        .name("http".into())
        .spawn(move || {
            let mut ctx = Ctx {
                admin,
                tokens: HashMap::new(),
                fails: HashMap::new(),
            };
            for req in server.incoming_requests() {
                handle(&mut ctx, req);
            }
        })
        .expect("spawn http thread"))
}
