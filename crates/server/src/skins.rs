//! Painted skins. Anyone can draw a skin in the web editor (`/skins`) and submit it; the
//! admin reviews submissions in the admin panel and approves them (for example after a
//! donation), which creates a redeem code. Players enter the code in the game and everyone
//! on the server sees their skin. Bots and players without a code keep the standard look.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toon_shared::rng::Rng;
use toon_shared::skin::{tex, Skin};

/// Most submissions waiting for review at once.
const MAX_PENDING: usize = 300;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Approved,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SkinEntry {
    pub id: u32,
    pub name: String,
    pub author: String,
    /// How to reach the author (for the donation).
    pub contact: String,
    pub status: Status,
    pub skin: Skin,
    #[serde(with = "b64")]
    pub png: Vec<u8>,
    pub codes: Vec<String>,
    /// Unix seconds.
    pub created: u64,
}

#[derive(Default, Serialize, Deserialize)]
struct File {
    next_id: u32,
    skins: Vec<SkinEntry>,
}

pub struct SkinStore {
    path: Option<PathBuf>,
    next_id: u32,
    pub entries: Vec<SkinEntry>,
    rng: Rng,
}

fn now_secs() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn clean(s: &str, max: usize) -> String {
    s.chars()
        .filter(|c| !c.is_control())
        .take(max)
        .collect::<String>()
        .trim()
        .to_string()
}

/// Checks an uploaded skin picture.
pub fn validate_png(png: &[u8]) -> Result<(), String> {
    if png.len() > tex::MAX_PNG {
        return Err(format!(
            "Картинка завелика (макс. {} КБ)",
            tex::MAX_PNG / 1024
        ));
    }
    match tex::png_size(png) {
        Some((w, h)) if w == tex::SIZE && h == tex::SIZE => Ok(()),
        Some((w, h)) => Err(format!("Потрібно {0}×{0}, а не {w}×{h}", tex::SIZE)),
        None => Err("Це не PNG".into()),
    }
}

impl SkinStore {
    pub fn load(data_dir: &str) -> SkinStore {
        let path = PathBuf::from(data_dir).join("skins.json");
        let file: File = std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        SkinStore {
            path: Some(path),
            next_id: file.next_id.max(1),
            entries: file.skins,
            rng: Rng::from_time(),
        }
    }

    /// In-memory store (tests).
    pub fn memory() -> SkinStore {
        SkinStore {
            path: None,
            next_id: 1,
            entries: Vec::new(),
            rng: Rng::new(9),
        }
    }

    pub fn save(&self) {
        let Some(path) = &self.path else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let file = File {
            next_id: self.next_id,
            skins: self.entries.clone(),
        };
        if let Ok(s) = serde_json::to_string_pretty(&file) {
            let tmp = path.with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, path);
            }
        }
    }

    pub fn get(&self, id: u32) -> Option<&SkinEntry> {
        self.entries.iter().find(|e| e.id == id)
    }

    fn get_mut(&mut self, id: u32) -> Option<&mut SkinEntry> {
        self.entries.iter_mut().find(|e| e.id == id)
    }

    /// A player's submission from the web editor (waits for the admin).
    pub fn submit(
        &mut self,
        name: &str,
        author: &str,
        contact: &str,
        skin: Skin,
        png: Vec<u8>,
    ) -> Result<u32, String> {
        validate_png(&png)?;
        let pending = self
            .entries
            .iter()
            .filter(|e| e.status == Status::Pending)
            .count();
        if pending >= MAX_PENDING {
            return Err("Черга на розгляд переповнена, спробуй пізніше".into());
        }
        let id = self.next_id;
        self.next_id += 1;
        self.entries.push(SkinEntry {
            id,
            name: clean(name, 32),
            author: clean(author, 32),
            contact: clean(contact, 80),
            status: Status::Pending,
            skin: skin.sanitized(),
            png,
            codes: Vec::new(),
            created: now_secs(),
        });
        self.save();
        Ok(id)
    }

    /// Admin edit (or creation with `id = None`); edited skins stay approved.
    pub fn put(
        &mut self,
        id: Option<u32>,
        name: &str,
        skin: Skin,
        png: Vec<u8>,
    ) -> Result<u32, String> {
        validate_png(&png)?;
        let id = match id.and_then(|i| self.get(i).map(|e| e.id)) {
            Some(i) => i,
            None => {
                let i = self.next_id;
                self.next_id += 1;
                self.entries.push(SkinEntry {
                    id: i,
                    name: String::new(),
                    author: "адмін".into(),
                    contact: String::new(),
                    status: Status::Approved,
                    skin,
                    png: Vec::new(),
                    codes: Vec::new(),
                    created: now_secs(),
                });
                i
            }
        };
        let e = self.get_mut(id).unwrap();
        e.name = clean(name, 32);
        e.skin = skin.sanitized();
        e.png = png;
        self.save();
        Ok(id)
    }

    fn new_code(&mut self) -> String {
        const A: &[u8] = b"ABCDEFGHJKLMNPQRSTUVWXYZ23456789";
        loop {
            let mut s = String::from("TW-");
            for k in 0..8 {
                if k == 4 {
                    s.push('-');
                }
                s.push(A[(self.rng.next_u64() % A.len() as u64) as usize] as char);
            }
            if self.find_code(&s).is_none() {
                return s;
            }
        }
    }

    /// Approves a skin and returns a new redeem code for it.
    pub fn approve(&mut self, id: u32) -> Option<String> {
        self.get(id)?;
        let code = self.new_code();
        let e = self.get_mut(id)?;
        e.status = Status::Approved;
        e.codes.push(code.clone());
        self.save();
        Some(code)
    }

    pub fn revoke(&mut self, code: &str) -> bool {
        let code = code.trim().to_uppercase();
        let mut found = false;
        for e in &mut self.entries {
            let before = e.codes.len();
            e.codes.retain(|c| *c != code);
            found |= e.codes.len() != before;
        }
        if found {
            self.save();
        }
        found
    }

    pub fn delete(&mut self, id: u32) -> bool {
        let before = self.entries.len();
        self.entries.retain(|e| e.id != id);
        let removed = self.entries.len() != before;
        if removed {
            self.save();
        }
        removed
    }

    /// The approved skin a code unlocks.
    pub fn find_code(&self, code: &str) -> Option<u32> {
        let code = code.trim().to_uppercase();
        self.entries
            .iter()
            .find(|e| e.status == Status::Approved && e.codes.contains(&code))
            .map(|e| e.id)
    }
}

/// Minimal base64 (standard alphabet, with padding) for PNG data in JSON.
pub mod b64 {
    use serde::{Deserialize, Deserializer, Serializer};

    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(data: &[u8]) -> String {
        let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
        for c in data.chunks(3) {
            let n = (c[0] as u32) << 16
                | (*c.get(1).unwrap_or(&0) as u32) << 8
                | *c.get(2).unwrap_or(&0) as u32;
            for k in 0..4 {
                if k <= c.len() {
                    out.push(T[(n >> (18 - 6 * k) & 63) as usize] as char);
                } else {
                    out.push('=');
                }
            }
        }
        out
    }

    pub fn decode(s: &str) -> Option<Vec<u8>> {
        let s = s.trim();
        let s = s.split_once("base64,").map(|x| x.1).unwrap_or(s);
        let mut out = Vec::with_capacity(s.len() / 4 * 3);
        let mut buf = 0u32;
        let mut bits = 0;
        for ch in s.bytes() {
            if ch == b'=' {
                break;
            }
            if ch.is_ascii_whitespace() {
                continue;
            }
            let v = T.iter().position(|t| *t == ch)? as u32;
            buf = buf << 6 | v;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buf >> bits) as u8);
                buf &= (1 << bits) - 1;
            }
        }
        Some(out)
    }

    pub fn serialize<S: Serializer>(v: &[u8], s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&encode(v))
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<u8>, D::Error> {
        let s = String::deserialize(d)?;
        decode(&s).ok_or_else(|| serde::de::Error::custom("bad base64"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_png() -> Vec<u8> {
        let mut v = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        v.extend_from_slice(&128u32.to_be_bytes());
        v.extend_from_slice(&128u32.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v
    }

    #[test]
    fn base64_round_trip() {
        for n in 0..40 {
            let data: Vec<u8> = (0..n).map(|i| (i * 37 + 11) as u8).collect();
            assert_eq!(b64::decode(&b64::encode(&data)).unwrap(), data);
        }
        assert_eq!(b64::encode(b"hi"), "aGk=");
        assert_eq!(
            b64::decode("data:image/png;base64,aGk=").unwrap(),
            b"hi".to_vec()
        );
    }

    #[test]
    fn submit_approve_redeem_revoke() {
        let mut st = SkinStore::memory();
        assert!(st
            .submit("x", "a", "", Skin::default(), b"nope".to_vec())
            .is_err());
        let id = st
            .submit("Котик", "Тарас", "tg @taras", Skin::default(), fake_png())
            .unwrap();
        assert_eq!(st.get(id).unwrap().status, Status::Pending);
        let code = st.approve(id).unwrap();
        assert!(code.starts_with("TW-") && code.len() == 12);
        assert_eq!(st.find_code(&code.to_lowercase()), Some(id));
        assert!(st.revoke(&code));
        assert_eq!(st.find_code(&code), None);
        assert!(st.delete(id));
        assert!(st.get(id).is_none());
    }
}
