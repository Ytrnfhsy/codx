//! Who has played here, for the admin's eyes only: kept in `players.json` in the server's
//! data directory under the same key as the records (the installation's number, `#…`,
//! or the name for a client without one). For each player: the names used, when they
//! came and went, from which addresses (and, when `geoip` is on, which country), what
//! they wrote in the chat and how they fought.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::net::IpAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// How much is kept per player (the oldest goes first).
const MAX_NAMES: usize = 12;
const MAX_IPS: usize = 24;
const MAX_VISITS: usize = 120;
const MAX_CHAT: usize = 400;

/// Seconds since 1970 (the admin page shows them in the browser's own time zone).
pub fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct IpSeen {
    pub ip: String,
    pub first: u64,
    pub last: u64,
    pub times: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Visit {
    pub join: u64,
    /// 0 while still here (or when the server stopped before they left).
    pub leave: u64,
    pub ip: String,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct ChatLine {
    pub at: u64,
    pub text: String,
    pub team: bool,
    /// The map and mode it was said on.
    pub map: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Fight {
    pub kills: u32,
    pub deaths: u32,
    pub headshots: u32,
    /// Killed by their own hand (a grenade, a fall, the zone).
    pub suicides: u32,
    pub knife_kills: u32,
    pub grenade_kills: u32,
    pub streak_kills: u32,
    /// Longest kill, metres.
    pub longest: f32,
    pub best_streak: u32,
    /// Double, triple, quad and five-or-more kills.
    pub multi: [u32; 4],
    /// Matches finished (team modes) and won.
    pub matches: u32,
    pub wins: u32,
    pub by_weapon: BTreeMap<String, u32>,
    /// What killed them.
    pub killed_by: BTreeMap<String, u32>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub name: String,
    /// Names used before (the latest last).
    pub names: Vec<String>,
    pub first_seen: u64,
    pub last_seen: u64,
    pub sessions: u32,
    /// Seconds spent on the server.
    pub play_secs: u64,
    pub ips: Vec<IpSeen>,
    pub visits: VecDeque<Visit>,
    pub chat: VecDeque<ChatLine>,
    pub fight: Fight,
    /// On the server since then (not kept: a restart ends every visit).
    #[serde(skip)]
    pub online_since: Option<u64>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Geo {
    /// Two letters ("UA"); empty when unknown.
    pub cc: String,
    pub country: String,
}

#[derive(Default, Serialize, Deserialize)]
#[serde(default)]
struct File {
    players: HashMap<String, Entry>,
    geo: HashMap<String, Geo>,
}

pub struct Registry {
    pub players: HashMap<String, Entry>,
    /// Countries of addresses, filled in by lookups in the background.
    pub geo: Arc<Mutex<HashMap<String, Geo>>>,
    asked: Arc<Mutex<HashSet<String>>>,
    path: Option<PathBuf>,
    dirty: bool,
    saved_at: f64,
    /// Look addresses up at ip-api.com (the server's `geoip` setting).
    pub lookup: bool,
}

impl Registry {
    pub fn load(dir: &str, lookup: bool) -> Registry {
        let path = PathBuf::from(dir).join("players.json");
        let f: File = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        let mut players = f.players;
        // the server stopped while they were here: that visit has no end
        for e in players.values_mut() {
            e.online_since = None;
        }
        Registry {
            players,
            geo: Arc::new(Mutex::new(f.geo)),
            asked: Default::default(),
            path: Some(path),
            dirty: false,
            saved_at: 0.0,
            lookup,
        }
    }

    /// A registry that is never written (tests).
    pub fn in_memory() -> Registry {
        Registry {
            players: HashMap::new(),
            geo: Default::default(),
            asked: Default::default(),
            path: None,
            dirty: false,
            saved_at: 0.0,
            lookup: false,
        }
    }

    /// A real player came in.
    pub fn join(&mut self, key: &str, name: &str, ip: &str) {
        let now = unix_now();
        let e = self.players.entry(key.to_string()).or_default();
        if e.first_seen == 0 {
            e.first_seen = now;
        }
        e.last_seen = now;
        e.sessions += 1;
        e.online_since = Some(now);
        rename(e, name);
        if !ip.is_empty() {
            match e.ips.iter_mut().find(|s| s.ip == ip) {
                Some(s) => {
                    s.last = now;
                    s.times += 1;
                }
                None => {
                    e.ips.push(IpSeen {
                        ip: ip.to_string(),
                        first: now,
                        last: now,
                        times: 1,
                    });
                    if e.ips.len() > MAX_IPS {
                        // (the one least recently seen goes)
                        if let Some(i) = (0..e.ips.len()).min_by_key(|&i| e.ips[i].last) {
                            e.ips.remove(i);
                        }
                    }
                }
            }
        }
        e.visits.push_back(Visit {
            join: now,
            leave: 0,
            ip: ip.to_string(),
            name: name.to_string(),
        });
        while e.visits.len() > MAX_VISITS {
            e.visits.pop_front();
        }
        self.dirty = true;
        self.locate(ip);
    }

    /// They left (or were dropped).
    pub fn leave(&mut self, key: &str) {
        let now = unix_now();
        let Some(e) = self.players.get_mut(key) else {
            return;
        };
        if let Some(since) = e.online_since.take() {
            e.play_secs += now.saturating_sub(since);
        }
        e.last_seen = now;
        if let Some(v) = e.visits.back_mut().filter(|v| v.leave == 0) {
            v.leave = now;
        }
        self.dirty = true;
        // (a visit is worth keeping straight away)
        self.save();
    }

    pub fn chat(&mut self, key: &str, name: &str, text: &str, team: bool, map: &str) {
        let e = self.players.entry(key.to_string()).or_default();
        rename(e, name);
        e.chat.push_back(ChatLine {
            at: unix_now(),
            text: text.to_string(),
            team,
            map: map.to_string(),
        });
        while e.chat.len() > MAX_CHAT {
            e.chat.pop_front();
        }
        self.dirty = true;
    }

    pub fn fight(&mut self, key: &str) -> &mut Fight {
        self.dirty = true;
        &mut self.players.entry(key.to_string()).or_default().fight
    }

    /// Writes the file now and then (at most once a minute) when something changed.
    pub fn tick(&mut self, now: f64) {
        if self.dirty && now - self.saved_at > 60.0 {
            self.saved_at = now;
            self.save();
        }
    }

    pub fn save(&mut self) {
        let Some(path) = &self.path else { return };
        self.dirty = false;
        let f = File {
            players: self.players.clone(),
            geo: self.geo.lock().map(|g| g.clone()).unwrap_or_default(),
        };
        let Ok(text) = serde_json::to_string(&f) else {
            return;
        };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // (written aside and moved over: a crash mid-write leaves the old file whole)
        let tmp = path.with_extension("json.tmp");
        if std::fs::write(&tmp, text).is_ok() {
            let _ = std::fs::rename(&tmp, path);
        }
    }

    /// The country of an address, if known.
    pub fn country(&self, ip: &str) -> Option<Geo> {
        self.geo.lock().ok().and_then(|g| g.get(ip).cloned())
    }

    /// Asks ip-api.com for an address's country, once, in the background (never for
    /// local or private addresses).
    fn locate(&self, ip: &str) {
        if !self.lookup || !public(ip) {
            return;
        }
        if self.geo.lock().map(|g| g.contains_key(ip)).unwrap_or(true) {
            return;
        }
        let first = self
            .asked
            .lock()
            .map(|mut a| a.insert(ip.to_string()))
            .unwrap_or(false);
        if !first {
            return;
        }
        let (geo, ip) = (self.geo.clone(), ip.to_string());
        let _ = std::thread::Builder::new()
            .name("geoip".into())
            .spawn(move || {
                let url = format!("http://ip-api.com/json/{ip}?fields=status,country,countryCode");
                let found = ureq::get(&url)
                    .timeout(std::time::Duration::from_secs(6))
                    .call()
                    .ok()
                    .and_then(|r| r.into_string().ok())
                    .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
                    .filter(|v| v["status"] == "success")
                    .map(|v| Geo {
                        cc: v["countryCode"].as_str().unwrap_or("").to_string(),
                        country: v["country"].as_str().unwrap_or("").to_string(),
                    });
                if let (Some(g), Ok(mut map)) = (found, geo.lock()) {
                    map.insert(ip, g);
                }
            });
    }
}

fn rename(e: &mut Entry, name: &str) {
    if name.is_empty() || e.name == name {
        return;
    }
    if !e.name.is_empty() && !e.names.contains(&e.name) {
        e.names.push(e.name.clone());
        if e.names.len() > MAX_NAMES {
            e.names.remove(0);
        }
    }
    e.name = name.to_string();
}

/// An address on the internet (not this machine, not a home or office network).
fn public(ip: &str) -> bool {
    match ip.parse::<IpAddr>() {
        Ok(IpAddr::V4(a)) => {
            !(a.is_private()
                || a.is_loopback()
                || a.is_link_local()
                || a.is_unspecified()
                || a.is_broadcast()
                || a.octets()[0] == 100 && (64..128).contains(&a.octets()[1]))
        }
        Ok(IpAddr::V6(a)) => {
            !(a.is_loopback()
                || a.is_unspecified()
                || (a.segments()[0] & 0xfe00) == 0xfc00
                || (a.segments()[0] & 0xffc0) == 0xfe80)
        }
        Err(_) => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn visits_names_and_chat_are_kept() {
        let mut r = Registry::in_memory();
        r.join("#1", "Петро", "10.0.0.5");
        r.chat("#1", "Петро", "привіт", false, "depot tdm");
        r.leave("#1");
        r.join("#1", "Петрик", "10.0.0.6");
        let e = &r.players["#1"];
        assert_eq!(e.sessions, 2);
        assert_eq!(e.name, "Петрик");
        assert_eq!(e.names, vec!["Петро".to_string()]);
        assert_eq!(e.ips.len(), 2);
        assert_eq!(e.visits.len(), 2);
        assert!(e.visits[0].leave != 0 && e.visits[1].leave == 0);
        assert_eq!(e.chat.len(), 1);
        assert!(!public("192.168.1.2") && !public("127.0.0.1") && public("8.8.8.8"));
    }
}
