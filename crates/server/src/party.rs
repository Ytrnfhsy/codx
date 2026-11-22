//! Squads in the main lobby, kept by the public list (`master.rs`): players who have not
//! picked a server yet find each other there, put a squad together and pick a mode; the
//! leader's game finds a server and tells the master where, and the others follow.
//!
//! Everybody talks to it with one call (`POST /party`) a second or two apart, which also
//! says they are still there; whoever has not called for a while drops out.
//! A player is known by a secret token their game made up (`token`); the others see only
//! an id made from it (`pid`), so nobody can act in their name.

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::IpAddr;
use std::time::{Duration, Instant};

/// Gone from the lobby after this long without a call.
const FORGET_AFTER: Duration = Duration::from_secs(20);
/// "Go to this server" is passed on for this long.
const GO_FOR: Duration = Duration::from_secs(30);
/// An invitation stands this long.
const INVITE_FOR: Duration = Duration::from_secs(60);
pub const PARTY_MAX: usize = 4;
const PER_IP: usize = 8;
const MAX_PEOPLE: usize = 4000;
/// How many of the others in the lobby a player is shown.
const ONLINE_SHOWN: usize = 30;

pub const MODES: [&str; 3] = ["br", "tdm", "sab"];

struct Person {
    pid: String,
    name: String,
    skin: u8,
    ready: bool,
    /// Playing on a server just now (still in the squad).
    busy: bool,
    party: u32,
    seen: Instant,
    ip: IpAddr,
    /// Who invited them (pid) and when.
    invites: Vec<(String, Instant)>,
}

struct Party {
    /// Tokens, the leader first.
    members: Vec<String>,
    mode: String,
    /// Where the leader took the squad: (address, a number that changes each time, when).
    go: Option<(String, u32, Instant)>,
    seq: u32,
}

#[derive(Default)]
pub struct Parties {
    people: HashMap<String, Person>,
    parties: HashMap<u32, Party>,
    next: u32,
}

/// The id others see a player by.
pub fn pid_of(token: &str) -> String {
    let h = Sha256::digest(token.as_bytes());
    h.iter().take(5).map(|b| format!("{b:02x}")).collect()
}

fn clean_name(s: &str) -> String {
    let n: String = s
        .chars()
        .filter(|c| !c.is_control())
        .take(20)
        .collect::<String>()
        .trim()
        .to_string();
    if n.is_empty() {
        "?".into()
    } else {
        n
    }
}

impl Parties {
    fn new_party(&mut self, token: &str) -> u32 {
        self.next = self.next.wrapping_add(1).max(1);
        while self.parties.contains_key(&self.next) {
            self.next = self.next.wrapping_add(1).max(1);
        }
        self.parties.insert(
            self.next,
            Party {
                members: vec![token.to_string()],
                mode: "br".into(),
                go: None,
                seq: 0,
            },
        );
        self.next
    }

    /// Takes a player out of their squad (the next one leads it if they did).
    fn leave_party(&mut self, token: &str) {
        let Some(p) = self.people.get(token) else {
            return;
        };
        let pid = p.party;
        if let Some(party) = self.parties.get_mut(&pid) {
            party.members.retain(|m| m != token);
            if party.members.is_empty() {
                self.parties.remove(&pid);
            }
        }
    }

    fn forget_old(&mut self) {
        let gone: Vec<String> = self
            .people
            .iter()
            .filter(|(_, p)| p.seen.elapsed() > FORGET_AFTER)
            .map(|(t, _)| t.clone())
            .collect();
        for t in gone {
            self.leave_party(&t);
            self.people.remove(&t);
        }
    }

    fn token_of(&self, pid: &str) -> Option<String> {
        self.people
            .iter()
            .find(|(_, p)| p.pid == pid)
            .map(|(t, _)| t.clone())
    }

    /// One call from a player: `v` has their `token`, `name`, `skin` and an `op` with its
    /// arguments; the answer is what they see of the lobby (or an error).
    pub fn call(&mut self, ip: IpAddr, v: &Value) -> Value {
        self.forget_old();
        let token = v["token"].as_str().unwrap_or("");
        if !(16..=64).contains(&token.len()) || !token.chars().all(|c| c.is_ascii_hexdigit()) {
            return json!({"error": "token"});
        }
        let token = token.to_string();
        if !self.people.contains_key(&token) {
            let from_ip = self.people.values().filter(|p| p.ip == ip).count();
            if from_ip >= PER_IP || self.people.len() >= MAX_PEOPLE {
                return json!({"error": "busy"});
            }
            let party = self.new_party(&token);
            self.people.insert(
                token.clone(),
                Person {
                    pid: pid_of(&token),
                    name: String::new(),
                    skin: 0,
                    ready: false,
                    busy: false,
                    party,
                    seen: Instant::now(),
                    ip,
                    invites: Vec::new(),
                },
            );
        }
        {
            let me = self.people.get_mut(&token).expect("just made");
            me.seen = Instant::now();
            me.ip = ip;
            if let Some(n) = v["name"].as_str() {
                me.name = clean_name(n);
            }
            if let Some(s) = v["skin"].as_u64() {
                me.skin = s.min(255) as u8;
            }
            me.busy = v["busy"].as_bool().unwrap_or(false);
            me.invites.retain(|i| i.1.elapsed() < INVITE_FOR);
        }
        let my_pid = self.people[&token].pid.clone();
        let my_party = self.people[&token].party;
        let arg = |k: &str| v[k].as_str().unwrap_or("").to_string();
        match v["op"].as_str().unwrap_or("poll") {
            "invite" => {
                if let Some(to) = self.token_of(&arg("to")).filter(|t| *t != token) {
                    let to_party = self.people[&to].party;
                    let p = self.people.get_mut(&to).expect("found");
                    if to_party != my_party && !p.invites.iter().any(|i| i.0 == my_pid) {
                        p.invites.push((my_pid.clone(), Instant::now()));
                        p.invites.truncate(8);
                    }
                }
            }
            "decline" => {
                let from = arg("from");
                if let Some(me) = self.people.get_mut(&token) {
                    me.invites.retain(|i| i.0 != from);
                }
            }
            "accept" => {
                let from = arg("from");
                let invited = self.people[&token].invites.iter().any(|i| i.0 == from);
                if let (true, Some(ft)) = (invited, self.token_of(&from)) {
                    let target = self.people[&ft].party;
                    let room = self
                        .parties
                        .get(&target)
                        .is_some_and(|p| p.members.len() < PARTY_MAX);
                    if room && target != my_party {
                        self.leave_party(&token);
                        if let Some(p) = self.parties.get_mut(&target) {
                            p.members.push(token.clone());
                        }
                        let me = self.people.get_mut(&token).expect("me");
                        me.party = target;
                        me.ready = false;
                        me.invites.retain(|i| i.0 != from);
                    }
                }
            }
            "leave" => {
                if self
                    .parties
                    .get(&my_party)
                    .is_some_and(|p| p.members.len() > 1)
                {
                    self.leave_party(&token);
                    let p = self.new_party(&token);
                    if let Some(me) = self.people.get_mut(&token) {
                        me.party = p;
                        me.ready = false;
                    }
                }
            }
            "ready" => {
                let r = v["ready"].as_bool().unwrap_or(false);
                if let Some(me) = self.people.get_mut(&token) {
                    me.ready = r;
                }
            }
            "mode" => {
                let m = arg("mode");
                if let Some(p) = self.parties.get_mut(&my_party) {
                    if p.members.first() == Some(&token) && MODES.contains(&m.as_str()) {
                        p.mode = m;
                    }
                }
            }
            "go" => {
                let addr = arg("addr");
                if let Some(p) = self.parties.get_mut(&my_party) {
                    if p.members.first() == Some(&token) && !addr.is_empty() && addr.len() < 128 {
                        p.seq = p.seq.wrapping_add(1);
                        p.go = Some((addr, p.seq, Instant::now()));
                    }
                }
                // (off they go: nobody is "ready" any more)
                let members = self.parties.get(&my_party).map(|p| p.members.clone());
                for m in members.into_iter().flatten() {
                    if let Some(p) = self.people.get_mut(&m) {
                        p.ready = false;
                    }
                }
            }
            _ => {}
        }
        self.view(&token)
    }

    /// What a player sees: themselves, their squad, invitations and who else is around.
    fn view(&self, token: &str) -> Value {
        let me = &self.people[token];
        let party = &self.parties[&me.party];
        let member = |t: &String| {
            let p = &self.people[t];
            json!({"pid": p.pid, "name": p.name, "skin": p.skin, "ready": p.ready, "busy": p.busy})
        };
        let go = party
            .go
            .as_ref()
            .filter(|g| g.2.elapsed() < GO_FOR)
            .map(|g| json!({"addr": g.0, "seq": g.1}));
        let invites: Vec<Value> = me
            .invites
            .iter()
            .filter_map(|(pid, _)| {
                let t = self.token_of(pid)?;
                Some(json!({"pid": pid, "name": self.people[&t].name}))
            })
            .collect();
        let mut online: Vec<&Person> = self
            .people
            .iter()
            .filter(|(t, p)| *t != token && p.party != me.party && !p.name.is_empty())
            .map(|(_, p)| p)
            .collect();
        online.sort_by_key(|p| p.name.to_lowercase());
        let online: Vec<Value> = online
            .iter()
            .take(ONLINE_SHOWN)
            .map(|p| {
                let size = self.parties.get(&p.party).map_or(1, |x| x.members.len());
                json!({"pid": p.pid, "name": p.name, "squad": size, "busy": p.busy})
            })
            .collect();
        json!({
            "me": me.pid,
            "party": {
                "leader": self.people[&party.members[0]].pid,
                "mode": party.mode,
                "members": party.members.iter().map(member).collect::<Vec<_>>(),
                "go": go,
                "tag": format!("{:x}", me.party as u64 * 2654435761 % 0xffff_ffff),
            },
            "invites": invites,
            "online": online,
            "in_lobby": self.people.len(),
        })
    }
}

/// The game's side: one call to the master's lobby (a few seconds at most).
pub fn call_master(master: &str, body: &Value) -> Result<Value, String> {
    let url = format!("{}/party", crate::master::base_url(master));
    let text = ureq::post(&url)
        .timeout(Duration::from_secs(4))
        .set("Content-Type", "application/json")
        .send_string(&body.to_string())
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&text).map_err(|e| e.to_string())
}

/// A new secret token for a player (32 hex digits).
pub fn new_token() -> String {
    let mut b = [0u8; 16];
    if getrandom::getrandom(&mut b).is_err() {
        // (no system randomness: the clock is still better than nothing)
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(1, |d| d.as_nanos());
        b = n.to_le_bytes();
    }
    b.iter().map(|x| format!("{x:02x}")).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(ps: &mut Parties, token: &str, name: &str, op: Value) -> Value {
        let mut v = op;
        v["token"] = json!(token);
        v["name"] = json!(name);
        ps.call("10.0.0.1".parse().unwrap(), &v)
    }

    #[test]
    fn a_squad_comes_together_and_goes() {
        let mut ps = Parties::default();
        let (a, b, c) = ("a".repeat(32), "b".repeat(32), "c".repeat(32));
        let va = call(&mut ps, &a, "Ann", json!({}));
        let vb = call(&mut ps, &b, "Bob", json!({}));
        call(&mut ps, &c, "Cid", json!({}));
        let (pa, pb) = (
            va["me"].as_str().unwrap(),
            vb["me"].as_str().unwrap().to_string(),
        );
        assert_eq!(va["party"]["members"].as_array().unwrap().len(), 1);
        // Ann asks Bob; Bob sees it and says yes
        call(&mut ps, &a, "Ann", json!({"op": "invite", "to": pb}));
        let vb = call(&mut ps, &b, "Bob", json!({}));
        assert_eq!(vb["invites"][0]["pid"], pa);
        let vb = call(&mut ps, &b, "Bob", json!({"op": "accept", "from": pa}));
        assert_eq!(vb["party"]["members"].as_array().unwrap().len(), 2);
        assert_eq!(vb["party"]["leader"], pa);
        // only the leader picks the mode and the server
        call(&mut ps, &b, "Bob", json!({"op": "mode", "mode": "tdm"}));
        let va = call(&mut ps, &a, "Ann", json!({"op": "mode", "mode": "sab"}));
        assert_eq!(va["party"]["mode"], "sab");
        call(&mut ps, &b, "Bob", json!({"op": "go", "addr": "1.2.3.4:5"}));
        assert!(call(&mut ps, &b, "Bob", json!({}))["party"]["go"].is_null());
        call(
            &mut ps,
            &a,
            "Ann",
            json!({"op": "go", "addr": "1.2.3.4:27015"}),
        );
        let vb = call(&mut ps, &b, "Bob", json!({}));
        assert_eq!(vb["party"]["go"]["addr"], "1.2.3.4:27015");
        // the third one only sees them in the list, as a squad of two
        let vc = call(&mut ps, &c, "Cid", json!({}));
        let online = vc["online"].as_array().unwrap();
        assert!(online.iter().all(|o| o["squad"] == 2), "{online:?}");
        // an invitation nobody sent cannot be taken
        let vc = call(&mut ps, &c, "Cid", json!({"op": "accept", "from": pa}));
        assert_eq!(vc["party"]["members"].as_array().unwrap().len(), 1);
        // Bob leaves: on his own again, Ann still leads hers
        let vb = call(&mut ps, &b, "Bob", json!({"op": "leave"}));
        assert_eq!(vb["party"]["members"].as_array().unwrap().len(), 1);
        let va = call(&mut ps, &a, "Ann", json!({}));
        assert_eq!(va["party"]["members"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn bad_tokens_are_refused() {
        let mut ps = Parties::default();
        assert!(call(&mut ps, "short", "X", json!({}))["error"].is_string());
        assert!(call(&mut ps, &"z".repeat(32), "X", json!({}))["error"].is_string());
        for k in 0..PER_IP {
            let t = format!("{k:032x}");
            assert!(call(&mut ps, &t, "X", json!({}))["error"].is_null());
        }
        assert!(call(&mut ps, &format!("{:032x}", 999), "X", json!({}))["error"].is_string());
    }
}
