//! Tiny HTTP server: public status page, `/api/info`, and the admin web panel (`/admin`).

use crate::skins::{b64, SkinStore};
use crate::AdminMsg;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::Read;
use std::sync::mpsc::{channel, Sender};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tiny_http::{Header, Method, Request, Response, Server};
use toon_shared::rng::Rng;
use toon_shared::skin::Skin;

const ADMIN_HTML: &str = include_str!("../web/admin.html");
const INDEX_HTML: &str = include_str!("../web/index.html");
const SKINS_HTML: &str = include_str!("../web/skins.html");
const THREE_JS: &str = include_str!("../web/vendor/three.min.js");
const ORBIT_JS: &str = include_str!("../web/vendor/OrbitControls.js");
/// Skin submissions allowed per address per hour.
const SUBMITS_PER_HOUR: usize = 6;
const TOKEN_TTL: Duration = Duration::from_secs(12 * 3600);

type Store = Arc<Mutex<SkinStore>>;

struct Ctx {
    admin: Sender<AdminMsg>,
    skins: Store,
    submits: HashMap<String, Vec<Instant>>,
    tokens: HashMap<String, Instant>,
    fails: HashMap<String, (u32, Instant)>,
    rng: Rng,
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
    body_json_max(req, 64 * 1024)
}

fn body_json_max(req: &mut Request, max: u64) -> Value {
    let mut s = String::new();
    let _ = req.as_reader().take(max).read_to_string(&mut s);
    serde_json::from_str(&s).unwrap_or(Value::Null)
}

fn query(full_url: &str, key: &str) -> Option<String> {
    let q = full_url.split_once('?')?.1;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=')?;
        (k == key).then(|| v.to_string())
    })
}

fn str_of(v: &Value, k: &str) -> String {
    v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string()
}

/// Skin options + picture from an editor request.
fn skin_payload(v: &Value) -> Result<(Skin, Vec<u8>), String> {
    let skin: Skin = v
        .get("skin")
        .and_then(|s| serde_json::from_value(s.clone()).ok())
        .unwrap_or_default();
    let png = b64::decode(&str_of(v, "png")).ok_or("Пошкоджена картинка")?;
    Ok((skin.sanitized(), png))
}

fn skin_list(store: &SkinStore) -> Value {
    let list: Vec<Value> = store
        .entries
        .iter()
        .rev()
        .map(|e| {
            json!({
                "id": e.id,
                "name": e.name,
                "author": e.author,
                "contact": e.contact,
                "status": e.status,
                "codes": e.codes,
                "created": e.created,
                "skin": e.skin,
            })
        })
        .collect();
    json!({ "skins": list })
}

/// Admin-only skin endpoints (the caller has checked the token).
fn skin_admin(ctx: &mut Ctx, mut req: Request, path: &str, full: &str) {
    let mut store = ctx.skins.lock().unwrap();
    match path {
        "/api/skins" => json_resp(req, 200, skin_list(&store)),
        "/api/skins/get" => {
            let id = query(full, "id").and_then(|s| s.parse().ok()).unwrap_or(0);
            match store.get(id) {
                Some(e) => json_resp(
                    req,
                    200,
                    json!({"id": e.id, "name": e.name, "skin": e.skin,
                        "png": format!("data:image/png;base64,{}", b64::encode(&e.png))}),
                ),
                None => json_resp(req, 404, json!({"error": "Скін не знайдено"})),
            }
        }
        "/api/skins/approve" => {
            let b = body_json(&mut req);
            let id = b.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            match store.approve(id) {
                Some(code) => json_resp(req, 200, json!({ "code": code })),
                None => json_resp(req, 404, json!({"error": "Скін не знайдено"})),
            }
        }
        "/api/skins/revoke" => {
            let b = body_json(&mut req);
            let ok = store.revoke(&str_of(&b, "code"));
            json_resp(req, 200, json!({ "ok": ok }))
        }
        "/api/skins/delete" => {
            let b = body_json(&mut req);
            let id = b.get("id").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            let ok = store.delete(id);
            json_resp(req, 200, json!({ "ok": ok }))
        }
        "/api/skins/save" => {
            let b = body_json_max(&mut req, 256 * 1024);
            let id = b.get("id").and_then(|v| v.as_u64()).map(|v| v as u32);
            match skin_payload(&b)
                .and_then(|(skin, png)| store.put(id, &str_of(&b, "name"), skin, png))
            {
                Ok(id) => json_resp(req, 200, json!({ "id": id })),
                Err(e) => json_resp(req, 400, json!({ "error": e })),
            }
        }
        _ => respond(req, 404, "text/plain; charset=utf-8", "404".into()),
    }
}

fn handle(ctx: &mut Ctx, mut req: Request) {
    let full = req.url().to_string();
    let url = full.split('?').next().unwrap_or("/").to_string();
    let ip = req
        .remote_addr()
        .map(|a| a.ip().to_string())
        .unwrap_or_default();
    match (req.method().clone(), url.as_str()) {
        (Method::Get, "/") => respond(req, 200, "text/html; charset=utf-8", INDEX_HTML.to_string()),
        (Method::Get, "/admin") | (Method::Get, "/admin/") => {
            respond(req, 200, "text/html; charset=utf-8", ADMIN_HTML.to_string())
        }
        (Method::Get, "/skins") | (Method::Get, "/skins/") => {
            respond(req, 200, "text/html; charset=utf-8", SKINS_HTML.to_string())
        }
        (Method::Get, "/skins/three.min.js") | (Method::Get, "/skins/OrbitControls.js") => {
            let js = if url.ends_with("three.min.js") {
                THREE_JS
            } else {
                ORBIT_JS
            };
            let r = Response::from_string(js)
                .with_header(header(
                    "Content-Type",
                    "application/javascript; charset=utf-8",
                ))
                .with_header(header("Cache-Control", "public, max-age=86400"));
            let _ = req.respond(r);
        }
        (Method::Get, "/api/skins/png") => {
            let id = query(&full, "id").and_then(|s| s.parse().ok()).unwrap_or(0);
            let png = ctx.skins.lock().unwrap().get(id).map(|e| e.png.clone());
            match png {
                Some(png) => {
                    let r = Response::from_data(png)
                        .with_header(header("Content-Type", "image/png"))
                        .with_header(header("Cache-Control", "no-store"));
                    let _ = req.respond(r);
                }
                None => respond(req, 404, "text/plain; charset=utf-8", "404".into()),
            }
        }
        (Method::Post, "/api/skins/submit") => {
            let now = Instant::now();
            let recent = ctx.submits.entry(ip.clone()).or_default();
            recent.retain(|t| now.duration_since(*t) < Duration::from_secs(3600));
            if recent.len() >= SUBMITS_PER_HOUR {
                return json_resp(
                    req,
                    429,
                    json!({"error": "Забагато скінів за годину, спробуй пізніше"}),
                );
            }
            let b = body_json_max(&mut req, 256 * 1024);
            let res = skin_payload(&b).and_then(|(skin, png)| {
                ctx.skins.lock().unwrap().submit(
                    &str_of(&b, "name"),
                    &str_of(&b, "author"),
                    &str_of(&b, "contact"),
                    skin,
                    png,
                )
            });
            match res {
                Ok(id) => {
                    ctx.submits.entry(ip.clone()).or_default().push(now);
                    println!("[http] новий скін #{id} на розгляд (з {ip})");
                    json_resp(req, 200, json!({ "id": id }))
                }
                Err(e) => json_resp(req, 400, json!({ "error": e })),
            }
        }
        (_, p) if p.starts_with("/api/skins") => {
            if !token_ok(ctx, &req) {
                return json_resp(req, 401, json!({"error": "unauthorized"}));
            }
            let p = p.to_string();
            skin_admin(ctx, req, &p, &full);
        }
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
                let token = format!("{:016x}{:016x}", ctx.rng.next_u64(), ctx.rng.next_u64());
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

pub fn spawn(
    bind: &str,
    port: u16,
    admin: Sender<AdminMsg>,
    skins: Store,
) -> Result<JoinHandle<()>, String> {
    let server = Server::http(format!("{bind}:{port}")).map_err(|e| e.to_string())?;
    Ok(std::thread::Builder::new()
        .name("http".into())
        .spawn(move || {
            let mut ctx = Ctx {
                admin,
                skins,
                submits: HashMap::new(),
                tokens: HashMap::new(),
                fails: HashMap::new(),
                rng: Rng::from_time(),
            };
            for req in server.incoming_requests() {
                handle(&mut ctx, req);
            }
        })
        .expect("spawn http thread"))
}
