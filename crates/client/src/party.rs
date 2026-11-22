//! The main lobby's squad, kept by the public list (see `toon_server::party`): a thread
//! calls the master a second and a half apart (and at once when we do something), and
//! the latest answer is what the lobby shows.

use serde_json::{json, Value};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default)]
pub struct Member {
    pub pid: String,
    pub name: String,
    pub skin: u8,
    pub ready: bool,
}

/// What the master last told us.
#[derive(Clone, Debug, Default)]
pub struct PartyView {
    /// Heard from the master at all (and the last error, if the last call failed).
    pub ok: bool,
    pub error: Option<String>,
    pub me: String,
    pub leader: String,
    pub mode: String,
    pub members: Vec<Member>,
    /// Where the leader took the squad, and the number of that order.
    pub go: Option<(String, u64)>,
    /// The squad's number for game servers (the same for all of it).
    pub tag: u64,
    /// Invitations: (pid, name).
    pub invites: Vec<(String, String)>,
    /// Others in the lobby: (pid, name, squad size).
    pub online: Vec<(String, String, u64)>,
    pub in_lobby: u64,
}

impl PartyView {
    pub fn leading(&self) -> bool {
        self.me == self.leader || self.members.len() <= 1
    }
}

/// What we tell about ourselves with every call.
#[derive(Clone, Debug, Default)]
pub struct Me {
    pub name: String,
    pub skin: u8,
    /// Playing on a server just now.
    pub busy: bool,
}

pub struct PartyClient {
    tx: Sender<Value>,
    pub view: Arc<Mutex<PartyView>>,
    pub me: Arc<Mutex<Me>>,
}

impl PartyClient {
    /// Starts talking to `master` (nothing happens when it is empty).
    pub fn start(master: &str, token: &str) -> Option<PartyClient> {
        let master = master.trim().to_string();
        if master.is_empty() {
            return None;
        }
        let (tx, rx) = channel::<Value>();
        let view: Arc<Mutex<PartyView>> = Default::default();
        let me: Arc<Mutex<Me>> = Default::default();
        let (v2, me2, token) = (view.clone(), me.clone(), token.to_string());
        let _ = std::thread::Builder::new()
            .name("party".into())
            .spawn(move || {
                let mut next = Instant::now();
                loop {
                    let wait = next.saturating_duration_since(Instant::now());
                    let op = match rx.recv_timeout(wait) {
                        Ok(op) => op,
                        Err(RecvTimeoutError::Timeout) => json!({"op": "poll"}),
                        Err(RecvTimeoutError::Disconnected) => return,
                    };
                    let mut body = op;
                    {
                        let m = me2.lock().unwrap_or_else(|e| e.into_inner());
                        body["token"] = json!(token);
                        body["name"] = json!(m.name);
                        body["skin"] = json!(m.skin);
                        body["busy"] = json!(m.busy);
                    }
                    let answer = toon_server::party::call_master(&master, &body);
                    let mut v = v2.lock().unwrap_or_else(|e| e.into_inner());
                    match answer {
                        Ok(a) if a["error"].is_null() => {
                            *v = parse(&a);
                            next = Instant::now() + Duration::from_millis(1500);
                        }
                        Ok(a) => {
                            v.error = Some(a["error"].as_str().unwrap_or("?").to_string());
                            next = Instant::now() + Duration::from_secs(5);
                        }
                        Err(e) => {
                            v.ok = false;
                            v.error = Some(e);
                            next = Instant::now() + Duration::from_secs(5);
                        }
                    }
                }
            });
        Some(PartyClient { tx, view, me })
    }

    pub fn send(&self, op: Value) {
        let _ = self.tx.send(op);
    }

    pub fn view(&self) -> PartyView {
        self.view.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub fn set_me(&self, name: &str, skin: u8, busy: bool) {
        let mut m = self.me.lock().unwrap_or_else(|e| e.into_inner());
        if m.name != name || m.skin != skin || m.busy != busy {
            m.name = name.to_string();
            m.skin = skin;
            m.busy = busy;
        }
    }
}

fn parse(a: &Value) -> PartyView {
    let s = |v: &Value| v.as_str().unwrap_or("").to_string();
    let p = &a["party"];
    PartyView {
        ok: true,
        error: None,
        me: s(&a["me"]),
        leader: s(&p["leader"]),
        mode: s(&p["mode"]),
        members: p["members"]
            .as_array()
            .map(|m| {
                m.iter()
                    .map(|m| Member {
                        pid: s(&m["pid"]),
                        name: s(&m["name"]),
                        skin: m["skin"].as_u64().unwrap_or(0).min(255) as u8,
                        ready: m["ready"].as_bool().unwrap_or(false),
                    })
                    .collect()
            })
            .unwrap_or_default(),
        go: p["go"]["addr"]
            .as_str()
            .map(|a| (a.to_string(), p["go"]["seq"].as_u64().unwrap_or(0))),
        tag: u64::from_str_radix(p["tag"].as_str().unwrap_or("0"), 16).unwrap_or(0),
        invites: a["invites"]
            .as_array()
            .map(|l| l.iter().map(|i| (s(&i["pid"]), s(&i["name"]))).collect())
            .unwrap_or_default(),
        online: a["online"]
            .as_array()
            .map(|l| {
                l.iter()
                    .map(|i| {
                        (
                            s(&i["pid"]),
                            s(&i["name"]),
                            i["squad"].as_u64().unwrap_or(1),
                        )
                    })
                    .collect()
            })
            .unwrap_or_default(),
        in_lobby: a["in_lobby"].as_u64().unwrap_or(0),
    }
}
