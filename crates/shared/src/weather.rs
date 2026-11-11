//! Battle royale weather and time of day, picked by the server for each round: how far
//! everyone (bots too) can see, and the colours the client paints the sky and light in.

use crate::lang::t;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Weather {
    /// A clear day: the map's own colours.
    #[default]
    Clear,
    /// The low warm sun of the evening.
    Evening,
    /// Grey clouds.
    Overcast,
    /// Rain: dark, wet and shorter sight.
    Rain,
    /// Thick fog: everything past a hundred metres or so is gone.
    Fog,
    /// A moonlit night: blue and dim, shorter sight.
    Night,
}

impl Weather {
    pub const ALL: [Weather; 6] = [
        Weather::Clear,
        Weather::Evening,
        Weather::Overcast,
        Weather::Rain,
        Weather::Fog,
        Weather::Night,
    ];

    /// How far you see, as a part of a clear day.
    pub fn visibility(self) -> f32 {
        match self {
            Weather::Clear => 1.0,
            Weather::Evening => 0.85,
            Weather::Overcast => 0.85,
            Weather::Rain => 0.65,
            Weather::Fog => 0.45,
            Weather::Night => 0.6,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Weather::Clear => t("Ясний день", "Clear day"),
            Weather::Evening => t("Вечір", "Evening"),
            Weather::Overcast => t("Хмарно", "Overcast"),
            Weather::Rain => t("Дощ", "Rain"),
            Weather::Fog => t("Туман", "Fog"),
            Weather::Night => t("Місячна ніч", "Moonlit night"),
        }
    }

    /// From a name in the server config ("clear", "evening", …).
    pub fn parse(s: &str) -> Option<Weather> {
        Weather::ALL
            .into_iter()
            .find(|w| format!("{w:?}").eq_ignore_ascii_case(s.trim()))
    }

    /// A round's weather from a random number 0..1 (clear days are the most common).
    pub fn pick(r: f32) -> Weather {
        match r {
            r if r < 0.32 => Weather::Clear,
            r if r < 0.50 => Weather::Evening,
            r if r < 0.64 => Weather::Overcast,
            r if r < 0.78 => Weather::Rain,
            r if r < 0.89 => Weather::Fog,
            _ => Weather::Night,
        }
    }

    /// The sky's top colour, the horizon / fog colour and the light's tint, from the
    /// map's own.
    pub fn colours(
        self,
        sky: [u8; 3],
        fog: [u8; 3],
        tint: [f32; 3],
    ) -> ([u8; 3], [u8; 3], [f32; 3]) {
        let mix = |a: [u8; 3], b: [u8; 3], k: f32| {
            [0, 1, 2].map(|i| (a[i] as f32 + (b[i] as f32 - a[i] as f32) * k) as u8)
        };
        let mul = |a: [f32; 3], b: [f32; 3]| [a[0] * b[0], a[1] * b[1], a[2] * b[2]];
        match self {
            Weather::Clear => (sky, fog, tint),
            Weather::Evening => (
                mix(sky, [96, 86, 150], 0.7),
                mix(fog, [236, 158, 112], 0.75),
                mul(tint, [1.02, 0.8, 0.66]),
            ),
            Weather::Overcast => (
                mix(sky, [150, 156, 166], 0.85),
                mix(fog, [182, 186, 190], 0.8),
                mul(tint, [0.84, 0.86, 0.9]),
            ),
            Weather::Rain => (
                mix(sky, [96, 102, 114], 0.9),
                mix(fog, [124, 130, 138], 0.9),
                mul(tint, [0.66, 0.69, 0.75]),
            ),
            Weather::Fog => (
                mix(sky, [196, 198, 200], 0.9),
                mix(fog, [204, 206, 208], 0.95),
                mul(tint, [0.86, 0.87, 0.89]),
            ),
            Weather::Night => (
                mix(sky, [20, 30, 66], 0.96),
                mix(fog, [44, 58, 96], 0.95),
                mul(tint, [0.5, 0.58, 0.82]),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_parse_back_and_picks_cover_all() {
        for w in Weather::ALL {
            assert_eq!(Weather::parse(&format!("{w:?}").to_lowercase()), Some(w));
            assert!(w.visibility() > 0.3 && w.visibility() <= 1.0);
        }
        let mut seen = std::collections::HashSet::new();
        for i in 0..100 {
            seen.insert(Weather::pick(i as f32 / 100.0) as u8);
        }
        assert_eq!(seen.len(), Weather::ALL.len());
        assert_eq!(Weather::parse("nope"), None);
    }
}
