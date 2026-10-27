//! Character skins: every player picks how their soldier looks (skin tone, hair, hat, eyes,
//! face, clothes, pattern, accessory). Skins are small sets of palette indices, so they are
//! cheap to send and impossible to break: any byte value is valid (it wraps around).
//! The team colour always stays on the vest and shoulder pads, so teams remain readable.

use crate::rng::Rng;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(default)]
pub struct Skin {
    pub tone: u8,
    pub hair: u8,
    pub hair_color: u8,
    pub hat: u8,
    pub hat_color: u8,
    pub eyes: u8,
    pub face: u8,
    pub mouth: u8,
    pub shirt: u8,
    pub pants: u8,
    pub pattern: u8,
    pub extra: u8,
}

impl Default for Skin {
    fn default() -> Self {
        Skin {
            tone: 1,
            hair: 1,
            hair_color: 1,
            hat: 1,
            hat_color: 0,
            eyes: 0,
            face: 0,
            mouth: 1,
            shirt: 0,
            pants: 0,
            pattern: 0,
            extra: 0,
        }
    }
}

pub const TONES: &[(&str, [u8; 3])] = &[
    ("Світла", [255, 226, 196]),
    ("Персикова", [255, 214, 170]),
    ("Засмагла", [228, 176, 130]),
    ("Оливкова", [200, 150, 104]),
    ("Смаглява", [160, 108, 72]),
    ("Темна", [112, 74, 52]),
    ("Зелена (прибулець)", [150, 214, 120]),
    ("Блакитна", [150, 196, 240]),
];

pub const HAIR: &[&str] = &[
    "Лисий",
    "Коротке",
    "Їжачок",
    "Чубчик",
    "Довге",
    "Хвостик",
    "Ірокез",
    "Афро",
];

pub const HAIR_COLORS: &[(&str, [u8; 3])] = &[
    ("Чорне", [44, 40, 42]),
    ("Каштанове", [110, 68, 40]),
    ("Русяве", [214, 172, 96]),
    ("Руде", [214, 96, 46]),
    ("Сиве", [170, 170, 176]),
    ("Біле", [240, 238, 232]),
    ("Рожеве", [250, 130, 180]),
    ("Синє", [80, 120, 230]),
];

pub const HATS: &[&str] = &[
    "Без",
    "Каска",
    "Кепка",
    "Шапка",
    "Берет",
    "Бандана",
    "Панама",
];

pub const EYES: &[&str] = &["Великі", "Ґудзики", "Усміхнені", "Сердиті", "Сонні"];

pub const FACE: &[&str] = &[
    "Нічого",
    "Рум'янець",
    "Веснянки",
    "Вуса",
    "Борода",
    "Окуляри",
    "Темні окуляри",
    "Пов'язка",
];

pub const MOUTHS: &[&str] = &["Без рота", "Усмішка", "Ротик «о»", "Язик"];

/// Clothing colours; index 0 means "team colour".
pub const CLOTH: &[(&str, [u8; 3])] = &[
    ("Командний", [0, 0, 0]),
    ("Оливковий", [104, 120, 70]),
    ("Хакі", [196, 170, 120]),
    ("Чорний", [56, 56, 64]),
    ("Білий", [236, 234, 228]),
    ("Сірий", [140, 144, 152]),
    ("Темно-синій", [52, 64, 110]),
    ("Червоний", [214, 70, 60]),
    ("Помаранчевий", [240, 140, 50]),
    ("Жовтий", [244, 204, 70]),
    ("Зелений", [86, 170, 90]),
    ("Фіолетовий", [140, 90, 190]),
    ("Рожевий", [244, 140, 180]),
    ("Блакитний", [110, 190, 240]),
];

pub const PATTERNS: &[&str] = &["Однотонний", "Камуфляж", "Смужки", "Горошок"];

pub const EXTRAS: &[&str] = &["Нічого", "Рюкзак", "Шарф", "Навушники", "Ранець-радіо"];

/// Painted skins: a 128×128 PNG split into a 4×4 grid of 32×32 cells.
pub mod tex {
    pub const SIZE: u32 = 128;
    /// Largest PNG a player may upload (bytes).
    pub const MAX_PNG: usize = 48 * 1024;
    // cells (index = row * 4 + column)
    pub const HEAD_FRONT: u8 = 0;
    pub const HEAD_RIGHT: u8 = 1;
    pub const HEAD_BACK: u8 = 2;
    pub const HEAD_LEFT: u8 = 3;
    pub const HEAD_TOP: u8 = 4;
    pub const HEAD_BOTTOM: u8 = 5;
    pub const TORSO_FRONT: u8 = 6;
    pub const TORSO_BACK: u8 = 7;
    pub const TORSO_RIGHT: u8 = 8;
    pub const TORSO_LEFT: u8 = 9;
    /// Top half: shoulders (torso top), bottom half: torso bottom.
    pub const TORSO_ENDS: u8 = 10;
    pub const ARM_RIGHT: u8 = 11;
    pub const ARM_LEFT: u8 = 12;
    pub const LEG_RIGHT: u8 = 13;
    pub const LEG_LEFT: u8 = 14;
    /// Top half: hands, bottom half: boots.
    pub const HANDS_BOOTS: u8 = 15;

    /// UV rectangle (u0, v0, u1, v1) of a cell, v going down the image.
    pub fn cell(c: u8) -> [f32; 4] {
        let (x, y) = ((c % 4) as f32 * 0.25, (c / 4) as f32 * 0.25);
        [x, y, x + 0.25, y + 0.25]
    }
    /// Upper or lower half of a cell.
    pub fn half(c: u8, lower: bool) -> [f32; 4] {
        let r = cell(c);
        let m = (r[1] + r[3]) * 0.5;
        if lower {
            [r[0], m, r[2], r[3]]
        } else {
            [r[0], r[1], r[2], m]
        }
    }

    /// Width and height of a PNG from its header (None if it is not a PNG).
    pub fn png_size(data: &[u8]) -> Option<(u32, u32)> {
        if data.len() < 24 || &data[..8] != b"\x89PNG\r\n\x1a\n" || &data[12..16] != b"IHDR" {
            return None;
        }
        let w = u32::from_be_bytes([data[16], data[17], data[18], data[19]]);
        let h = u32::from_be_bytes([data[20], data[21], data[22], data[23]]);
        Some((w, h))
    }
}

fn pick(i: u8, len: usize) -> usize {
    i as usize % len
}

impl Skin {
    pub fn tone_rgb(&self) -> [u8; 3] {
        TONES[pick(self.tone, TONES.len())].1
    }
    pub fn hair_rgb(&self) -> [u8; 3] {
        HAIR_COLORS[pick(self.hair_color, HAIR_COLORS.len())].1
    }
    pub fn hair_style(&self) -> usize {
        pick(self.hair, HAIR.len())
    }
    pub fn hat_style(&self) -> usize {
        pick(self.hat, HATS.len())
    }
    pub fn eye_style(&self) -> usize {
        pick(self.eyes, EYES.len())
    }
    pub fn face_style(&self) -> usize {
        pick(self.face, FACE.len())
    }
    pub fn mouth_style(&self) -> usize {
        pick(self.mouth, MOUTHS.len())
    }
    pub fn pattern_style(&self) -> usize {
        pick(self.pattern, PATTERNS.len())
    }
    pub fn extra_style(&self) -> usize {
        pick(self.extra, EXTRAS.len())
    }
    /// Clothing colour for a slot (None = use the team colour).
    pub fn cloth(i: u8) -> Option<[u8; 3]> {
        match pick(i, CLOTH.len()) {
            0 => None,
            k => Some(CLOTH[k].1),
        }
    }

    /// Keeps every index inside its list.
    pub fn sanitized(mut self) -> Skin {
        self.tone = pick(self.tone, TONES.len()) as u8;
        self.hair = pick(self.hair, HAIR.len()) as u8;
        self.hair_color = pick(self.hair_color, HAIR_COLORS.len()) as u8;
        self.hat = pick(self.hat, HATS.len()) as u8;
        self.hat_color = pick(self.hat_color, CLOTH.len()) as u8;
        self.eyes = pick(self.eyes, EYES.len()) as u8;
        self.face = pick(self.face, FACE.len()) as u8;
        self.mouth = pick(self.mouth, MOUTHS.len()) as u8;
        self.shirt = pick(self.shirt, CLOTH.len()) as u8;
        self.pants = pick(self.pants, CLOTH.len()) as u8;
        self.pattern = pick(self.pattern, PATTERNS.len()) as u8;
        self.extra = pick(self.extra, EXTRAS.len()) as u8;
        self
    }

    /// A random but tasteful skin (bots, the "random" button).
    pub fn random(rng: &mut Rng) -> Skin {
        let mut r = |n: usize| (rng.f32() * n as f32) as u8;
        let tone = r(6);
        let hair = r(HAIR.len());
        let hair_color = r(6);
        let hat = r(HATS.len());
        let hat_color = r(CLOTH.len());
        let eyes = r(EYES.len());
        let face = r(FACE.len());
        let mouth = r(MOUTHS.len());
        let shirt = if r(2) == 0 { 0 } else { r(CLOTH.len()) };
        let pants = if r(2) == 0 { 0 } else { r(6) };
        let pattern = if r(3) == 0 { r(PATTERNS.len()) } else { 0 };
        let extra = if r(3) == 0 { r(EXTRAS.len()) } else { 0 };
        Skin {
            tone,
            hair,
            hair_color,
            hat,
            hat_color,
            eyes,
            face,
            mouth,
            shirt,
            pants,
            pattern,
            extra,
        }
        .sanitized()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_bytes_make_a_valid_skin() {
        let s = Skin {
            tone: 255,
            hair: 200,
            hair_color: 99,
            hat: 77,
            hat_color: 250,
            eyes: 13,
            face: 255,
            mouth: 9,
            shirt: 255,
            pants: 128,
            pattern: 31,
            extra: 254,
        }
        .sanitized();
        assert!((s.tone as usize) < TONES.len());
        assert!((s.shirt as usize) < CLOTH.len());
        assert_eq!(s, s.sanitized());
        let mut rng = Rng::new(3);
        for _ in 0..50 {
            let r = Skin::random(&mut rng);
            assert_eq!(r, r.sanitized());
        }
    }
}
