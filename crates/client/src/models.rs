//! Procedural toon models: weapons, helicopter, jets, bomb, grenade (soldiers: `soldier.rs`).

use crate::mesh::{trs, ToonMesh};
use macroquad::prelude::*;
use toon_shared::weapons::WeaponId;
use toon_shared::Team;

const METAL: [u8; 3] = [58, 62, 72];
const WOOD: [u8; 3] = [170, 104, 52];
const TAN: [u8; 3] = [196, 170, 120];
const OLIVE: [u8; 3] = [104, 120, 70];

/// Team colours: a bright one (bubbles, markers) and a deeper one (helicopter paint).
pub struct TeamLook {
    pub helmet: [u8; 3],
    pub vest: [u8; 3],
}

pub fn team_look(t: Team) -> TeamLook {
    match t {
        Team::Allies => TeamLook {
            helmet: [72, 142, 250],
            vest: [52, 96, 170],
        },
        Team::Axis => TeamLook {
            helmet: [240, 92, 68],
            vest: [176, 70, 56],
        },
        Team::Spectator => TeamLook {
            helmet: [200, 200, 200],
            vest: [150, 150, 150],
        },
    }
}

pub fn team_color(t: Team) -> Color {
    match t {
        Team::Allies => Color::from_rgba(90, 168, 255, 255),
        Team::Axis => Color::from_rgba(255, 107, 94, 255),
        Team::Spectator => Color::from_rgba(200, 200, 200, 255),
    }
}

/// One box of a gun model.
#[derive(Clone, Copy)]
pub struct GunPart {
    pub c: Vec3,
    pub h: Vec3,
    pub col: [u8; 3],
    /// Detachable magazine / ammo box / drum (moves during reloads).
    pub mag: bool,
    /// Shotgun pump (slides back when racked).
    pub pump: bool,
}

const BLACK: [u8; 3] = [42, 44, 50];
const SILVER: [u8; 3] = [196, 200, 208];
const DARK_WOOD: [u8; 3] = [120, 72, 40];
const BROWN: [u8; 3] = [150, 108, 70];
const GREEN_BOX: [u8; 3] = [92, 120, 60];
const BAKELITE: [u8; 3] = [170, 90, 50];

/// Builds the chunky toy-like guns, one recognisable silhouette per weapon.
/// Gun space: barrel along -Z, origin at the pistol grip, Y up.
struct Gun(Vec<GunPart>);

impl Gun {
    fn new() -> Gun {
        Gun(Vec::new())
    }
    fn b(&mut self, c: [f32; 3], h: [f32; 3], col: [u8; 3]) -> &mut Self {
        self.0.push(GunPart {
            c: Vec3::from_array(c),
            h: Vec3::from_array(h),
            col,
            mag: false,
            pump: false,
        });
        self
    }
    fn mag(&mut self, c: [f32; 3], h: [f32; 3], col: [u8; 3]) -> &mut Self {
        self.b(c, h, col);
        self.0.last_mut().unwrap().mag = true;
        self
    }
    fn pump(&mut self, c: [f32; 3], h: [f32; 3], col: [u8; 3]) -> &mut Self {
        self.b(c, h, col);
        self.0.last_mut().unwrap().pump = true;
        self
    }
    /// Pistol grip with a trigger guard.
    fn grip(&mut self, col: [u8; 3]) -> &mut Self {
        self.b([0.0, -0.035, 0.02], [0.026, 0.06, 0.03], col).b(
            [0.0, 0.0, -0.05],
            [0.008, 0.016, 0.03],
            BLACK,
        )
    }
    /// Folding bipod under the barrel.
    fn bipod(&mut self, z: f32) -> &mut Self {
        self.b([0.035, -0.02, z], [0.008, 0.075, 0.008], BLACK)
            .b([-0.035, -0.02, z], [0.008, 0.075, 0.008], BLACK)
            .b([0.0, 0.05, z], [0.035, 0.008, 0.012], BLACK)
    }
    /// Telescopic sight above the receiver.
    fn scope(&mut self, z: f32, col: [u8; 3]) -> &mut Self {
        self.b([0.0, 0.155, z], [0.022, 0.022, 0.16], col)
            .b([0.0, 0.155, z - 0.17], [0.032, 0.032, 0.03], col)
            .b([0.0, 0.155, z + 0.15], [0.028, 0.028, 0.025], col)
            .b([0.0, 0.115, z - 0.07], [0.012, 0.02, 0.012], BLACK)
            .b([0.0, 0.115, z + 0.07], [0.012, 0.02, 0.012], BLACK)
    }
}

fn gun_def(w: WeaponId) -> Gun {
    let mut g = Gun::new();
    match w {
        WeaponId::M4 => {
            g.b([0.0, 0.07, -0.1], [0.04, 0.05, 0.17], BLACK) // receiver
                .b([0.0, 0.135, -0.08], [0.015, 0.02, 0.1], BLACK) // carry handle
                .b([0.0, 0.15, 0.0], [0.015, 0.025, 0.015], BLACK)
                .b([0.0, 0.07, -0.38], [0.046, 0.045, 0.12], METAL) // handguard
                .b([0.0, 0.14, -0.46], [0.012, 0.04, 0.012], BLACK) // front sight
                .b([0.0, 0.07, -0.6], [0.016, 0.016, 0.12], BLACK) // barrel
                .b([0.0, 0.07, -0.73], [0.023, 0.023, 0.02], BLACK)
                .b([0.0, 0.065, 0.12], [0.018, 0.018, 0.08], BLACK) // buffer tube
                .b([0.0, 0.04, 0.23], [0.035, 0.07, 0.045], METAL) // stock
                .grip(BLACK)
                .mag([0.0, -0.06, -0.15], [0.026, 0.07, 0.035], METAL)
                .mag([0.0, -0.14, -0.17], [0.026, 0.025, 0.035], METAL);
        }
        WeaponId::Ak47 => {
            g.b([0.0, 0.07, -0.1], [0.04, 0.045, 0.17], METAL)
                .b([0.0, 0.118, -0.33], [0.018, 0.016, 0.11], METAL) // gas tube
                .b([0.0, 0.065, -0.35], [0.043, 0.036, 0.1], WOOD) // handguard
                .b([0.0, 0.07, -0.56], [0.015, 0.015, 0.12], METAL)
                .b([0.0, 0.12, -0.62], [0.012, 0.035, 0.012], METAL)
                .b([0.0, 0.07, -0.69], [0.019, 0.019, 0.015], METAL)
                .b([0.0, 0.035, 0.16], [0.03, 0.05, 0.12], WOOD) // stock
                .b([0.0, 0.0, 0.27], [0.032, 0.08, 0.03], WOOD)
                .grip(WOOD)
                .mag([0.0, -0.05, -0.16], [0.026, 0.06, 0.035], BAKELITE)
                .mag([0.0, -0.12, -0.2], [0.026, 0.035, 0.035], BAKELITE);
        }
        WeaponId::G3 => {
            g.b([0.0, 0.075, -0.1], [0.04, 0.05, 0.2], METAL)
                .b([0.0, 0.14, 0.03], [0.015, 0.02, 0.02], BLACK) // drum sight
                .b([0.0, 0.065, -0.42], [0.046, 0.042, 0.13], OLIVE)
                .b([0.0, 0.075, -0.64], [0.015, 0.015, 0.1], BLACK)
                .b([0.0, 0.13, -0.62], [0.016, 0.03, 0.016], BLACK)
                .b([0.0, 0.05, 0.2], [0.032, 0.06, 0.15], OLIVE) // long stock
                .b([0.0, 0.04, 0.35], [0.034, 0.07, 0.012], BLACK)
                .grip(OLIVE)
                .mag([0.0, -0.065, -0.16], [0.026, 0.08, 0.035], BLACK);
        }
        WeaponId::Mp5 => {
            g.b([0.0, 0.07, -0.05], [0.04, 0.045, 0.15], BLACK)
                .b([0.0, 0.06, -0.26], [0.046, 0.042, 0.07], BLACK) // fat handguard
                .b([0.0, 0.07, -0.37], [0.016, 0.016, 0.05], BLACK)
                .b([0.0, 0.135, -0.3], [0.016, 0.03, 0.016], BLACK) // front sight ring
                .b([0.0, 0.135, 0.06], [0.016, 0.025, 0.016], BLACK)
                .b([0.0, 0.085, 0.13], [0.008, 0.008, 0.1], METAL) // retractable stock
                .b([0.0, 0.035, 0.13], [0.008, 0.008, 0.1], METAL)
                .b([0.0, 0.06, 0.23], [0.03, 0.045, 0.012], BLACK)
                .grip(BLACK)
                .mag([0.0, -0.05, -0.12], [0.02, 0.06, 0.025], METAL)
                .mag([0.0, -0.12, -0.15], [0.02, 0.035, 0.025], METAL);
        }
        WeaponId::P90 => {
            g.b([0.0, 0.03, -0.06], [0.045, 0.075, 0.22], TAN) // bullpup body
                .b([0.0, -0.06, -0.13], [0.03, 0.04, 0.025], TAN) // thumb-hole front
                .b([0.0, 0.17, -0.12], [0.024, 0.024, 0.06], BLACK) // optic
                .b([0.0, 0.04, -0.3], [0.016, 0.016, 0.04], BLACK)
                .b([0.0, -0.035, 0.02], [0.026, 0.05, 0.03], BLACK)
                .mag([0.0, 0.12, -0.08], [0.038, 0.02, 0.17], [70, 72, 80]);
        }
        WeaponId::Ak74u => {
            g.b([0.0, 0.07, -0.08], [0.04, 0.045, 0.15], METAL)
                .b([0.0, 0.065, -0.29], [0.043, 0.036, 0.07], WOOD)
                .b([0.0, 0.07, -0.4], [0.02, 0.02, 0.05], METAL)
                .b([0.0, 0.07, -0.47], [0.028, 0.028, 0.03], METAL) // flash cone
                .b([0.032, 0.06, 0.12], [0.008, 0.008, 0.11], METAL) // folding stock
                .b([0.0, 0.03, 0.22], [0.032, 0.05, 0.012], METAL)
                .grip(WOOD)
                .mag([0.0, -0.05, -0.15], [0.026, 0.06, 0.035], BAKELITE)
                .mag([0.0, -0.12, -0.19], [0.026, 0.035, 0.035], BAKELITE);
        }
        WeaponId::M249 => {
            g.b([0.0, 0.08, -0.08], [0.05, 0.06, 0.2], METAL)
                .b([0.0, 0.15, -0.05], [0.045, 0.016, 0.12], BLACK) // feed cover
                .b([0.0, 0.07, -0.38], [0.046, 0.04, 0.1], BLACK)
                .b([0.0, 0.075, -0.62], [0.017, 0.017, 0.14], BLACK)
                .b([0.0, 0.075, -0.77], [0.022, 0.022, 0.016], BLACK)
                .b([0.0, 0.17, -0.3], [0.012, 0.016, 0.06], BLACK) // carry handle
                .bipod(-0.56)
                .b([0.0, 0.06, 0.2], [0.03, 0.06, 0.13], BLACK)
                .grip(BLACK)
                .mag([0.0, -0.05, -0.1], [0.045, 0.07, 0.07], GREEN_BOX);
        }
        WeaponId::Rpd => {
            g.b([0.0, 0.07, -0.06], [0.04, 0.05, 0.16], METAL)
                .b([0.0, 0.06, -0.32], [0.042, 0.04, 0.1], WOOD)
                .b([0.0, 0.07, -0.62], [0.016, 0.016, 0.2], METAL)
                .b([0.0, 0.12, -0.78], [0.012, 0.035, 0.012], METAL)
                .bipod(-0.62)
                .b([0.0, 0.02, 0.2], [0.032, 0.07, 0.14], WOOD)
                .grip(WOOD)
                .mag([0.0, -0.07, -0.12], [0.032, 0.075, 0.075], BLACK); // drum
        }
        WeaponId::W1200 => {
            g.b([0.0, 0.06, -0.05], [0.04, 0.045, 0.12], METAL)
                .b([0.0, 0.085, -0.45], [0.018, 0.018, 0.28], METAL) // barrel
                .b([0.0, 0.045, -0.42], [0.015, 0.015, 0.22], METAL) // tube magazine
                .pump([0.0, 0.04, -0.32], [0.032, 0.03, 0.09], WOOD)
                .b([0.0, 0.025, 0.19], [0.03, 0.06, 0.14], WOOD)
                .b([0.0, -0.025, 0.04], [0.025, 0.05, 0.03], WOOD);
        }
        WeaponId::M1014 => {
            g.b([0.0, 0.06, -0.05], [0.04, 0.05, 0.13], BLACK)
                .b([0.0, 0.09, -0.42], [0.018, 0.018, 0.24], BLACK)
                .b([0.0, 0.05, -0.38], [0.016, 0.016, 0.2], BLACK)
                .b([0.0, 0.05, -0.27], [0.034, 0.032, 0.08], BLACK) // forend
                .b([0.0, 0.135, 0.0], [0.016, 0.02, 0.01], BLACK) // ghost ring
                .b([0.0, 0.075, 0.18], [0.012, 0.012, 0.13], METAL) // skeleton stock
                .b([0.0, 0.0, 0.18], [0.012, 0.012, 0.12], METAL)
                .b([0.0, 0.035, 0.31], [0.022, 0.065, 0.015], BLACK)
                .grip(BLACK)
                .mag([0.0, -0.0, -0.1], [0.02, 0.02, 0.03], BLACK);
        }
        WeaponId::M40 => {
            g.b([0.0, 0.035, -0.06], [0.036, 0.045, 0.26], BROWN) // stock & forend
                .b([0.0, 0.0, 0.25], [0.036, 0.075, 0.07], BROWN) // butt
                .b([0.0, 0.085, -0.05], [0.03, 0.025, 0.12], METAL) // action
                .b([0.05, 0.085, 0.04], [0.03, 0.01, 0.01], METAL) // bolt handle
                .b([0.075, 0.085, 0.04], [0.012, 0.014, 0.014], METAL)
                .b([0.0, 0.085, -0.5], [0.014, 0.014, 0.25], METAL)
                .scope(-0.06, BLACK)
                .mag([0.0, -0.01, -0.08], [0.02, 0.025, 0.035], METAL);
        }
        WeaponId::R700 => {
            g.b([0.0, 0.035, -0.04], [0.036, 0.045, 0.24], DARK_WOOD)
                .b([0.0, 0.0, 0.24], [0.036, 0.075, 0.06], DARK_WOOD)
                .b([0.0, 0.085, -0.04], [0.03, 0.025, 0.11], SILVER)
                .b([0.05, 0.085, 0.04], [0.03, 0.01, 0.01], SILVER)
                .b([0.0, 0.085, -0.46], [0.015, 0.015, 0.23], BLACK)
                .scope(-0.04, [52, 56, 66])
                .mag([0.0, -0.01, -0.06], [0.02, 0.025, 0.03], METAL);
        }
        WeaponId::M9 => {
            g.b([0.0, 0.07, -0.08], [0.025, 0.03, 0.12], BLACK) // slide
                .b([0.0, 0.035, -0.07], [0.023, 0.016, 0.1], METAL)
                .b([0.0, 0.11, -0.19], [0.006, 0.012, 0.006], BLACK)
                .b([0.0, -0.03, 0.02], [0.025, 0.06, 0.03], BROWN)
                .b([0.0, 0.0, -0.06], [0.008, 0.016, 0.028], BLACK)
                .mag([0.0, -0.095, 0.02], [0.022, 0.012, 0.028], BLACK);
        }
        WeaponId::Usp => {
            g.b([0.0, 0.07, -0.08], [0.027, 0.033, 0.125], BLACK)
                .b([0.0, 0.035, -0.07], [0.025, 0.018, 0.1], BLACK)
                .b([0.0, -0.03, 0.02], [0.027, 0.062, 0.032], BLACK)
                .b([0.0, 0.0, -0.06], [0.008, 0.016, 0.028], BLACK)
                .mag([0.0, -0.1, 0.02], [0.024, 0.012, 0.03], METAL);
        }
        WeaponId::Deagle => {
            g.b([0.0, 0.08, -0.1], [0.028, 0.037, 0.15], SILVER) // big slide
                .b([0.0, 0.035, -0.08], [0.026, 0.02, 0.11], SILVER)
                .b([0.0, 0.125, -0.23], [0.007, 0.012, 0.007], BLACK)
                .b([0.0, -0.035, 0.025], [0.028, 0.065, 0.033], BLACK)
                .b([0.0, 0.0, -0.07], [0.009, 0.018, 0.03], SILVER)
                .mag([0.0, -0.105, 0.025], [0.025, 0.012, 0.03], SILVER);
        }
    }
    g
}

/// Gun parts in gun space: barrel along -Z, origin at the grip.
pub fn gun_parts(w: WeaponId) -> Vec<GunPart> {
    gun_def(w).0
}

/// A sight found in battle royale (1 red dot, 2..4 scopes), standing on `y`.
pub fn sight_parts(level: u8, y: f32) -> Vec<GunPart> {
    let mut g = Gun::new();
    match level {
        0 => {}
        1 => {
            // a mount and an open frame: you look through it (the dot is drawn on the HUD)
            let (cy, half, bar) = (y + 0.042, 0.022, 0.0035);
            g.b([0.0, y + 0.01, -0.06], [0.016, 0.01, 0.035], BLACK)
                .b([-half, cy, -0.07], [bar, half, 0.006], BLACK)
                .b([half, cy, -0.07], [bar, half, 0.006], BLACK)
                .b([0.0, cy + half, -0.07], [half + bar, bar, 0.006], BLACK)
                .b([0.0, cy - half, -0.07], [half + bar, bar, 0.006], BLACK);
        }
        l => {
            // longer and thicker the stronger it is
            let r = 0.017 + 0.004 * l as f32;
            let len = 0.06 + 0.025 * l as f32;
            let cy = y + 0.022 + r;
            g.b([0.0, y + 0.012, -0.11], [0.01, 0.014, 0.01], BLACK)
                .b([0.0, y + 0.012, 0.0], [0.01, 0.014, 0.01], BLACK)
                .b([0.0, cy, -0.055], [r, r, len], BLACK)
                .b(
                    [0.0, cy, -0.055 - len],
                    [r + 0.008, r + 0.008, 0.018],
                    BLACK,
                )
                .b(
                    [0.0, cy, -0.055 + len],
                    [r + 0.004, r + 0.004, 0.012],
                    BLACK,
                )
                .b(
                    [0.0, cy, -0.055 - len - 0.019],
                    [r, r, 0.002],
                    [90, 140, 200],
                );
        }
    }
    g.0
}

/// A gun with the sight it carries (snipers keep their own scope).
pub fn gun_parts_with(w: WeaponId, sight: u8) -> Vec<GunPart> {
    let mut parts = gun_parts(w);
    if sight == 0 || w.def().scoped {
        return parts;
    }
    // on top of the receiver (iron sights stick out further forward)
    let top = parts
        .iter()
        .filter(|p| p.c.z > -0.25 && p.c.z < 0.1 && !p.mag)
        .map(|p| p.c.y + p.h.y)
        .fold(0.06, f32::max)
        .min(0.13);
    parts.extend(sight_parts(sight, top));
    parts
}

/// Centre of the magazine (where the off hand goes when reloading).
pub fn gun_mag(w: WeaponId) -> Option<(Vec3, Vec3, [u8; 3])> {
    gun_parts(w)
        .iter()
        .find(|p| p.mag)
        .map(|p| (p.c, p.h, p.col))
}

/// Where the off hand holds the gun.
pub fn fore_grip(w: WeaponId) -> Vec3 {
    match w {
        WeaponId::P90 => vec3(0.0, -0.06, -0.14),
        WeaponId::Mp5 | WeaponId::Ak74u => vec3(0.0, 0.03, -0.27),
        WeaponId::W1200 => vec3(0.0, 0.02, -0.32),
        WeaponId::M1014 => vec3(0.0, 0.02, -0.27),
        WeaponId::M40 | WeaponId::R700 => vec3(0.0, 0.0, -0.26),
        WeaponId::M9 | WeaponId::Usp | WeaponId::Deagle => vec3(-0.02, -0.03, 0.03),
        _ => vec3(0.0, 0.03, -0.36),
    }
}

/// Muzzle position in gun space.
pub fn muzzle(w: WeaponId) -> Vec3 {
    match w {
        WeaponId::M4 => vec3(0.0, 0.07, -0.75),
        WeaponId::Ak47 => vec3(0.0, 0.07, -0.71),
        WeaponId::G3 => vec3(0.0, 0.075, -0.75),
        WeaponId::Mp5 => vec3(0.0, 0.07, -0.43),
        WeaponId::P90 => vec3(0.0, 0.04, -0.35),
        WeaponId::Ak74u => vec3(0.0, 0.07, -0.51),
        WeaponId::M249 => vec3(0.0, 0.075, -0.79),
        WeaponId::Rpd => vec3(0.0, 0.07, -0.83),
        WeaponId::W1200 => vec3(0.0, 0.085, -0.74),
        WeaponId::M1014 => vec3(0.0, 0.09, -0.67),
        WeaponId::M40 => vec3(0.0, 0.085, -0.76),
        WeaponId::R700 => vec3(0.0, 0.085, -0.7),
        WeaponId::M9 | WeaponId::Usp => vec3(0.0, 0.07, -0.21),
        WeaponId::Deagle => vec3(0.0, 0.08, -0.26),
    }
}

pub fn add_gun(m: &mut ToonMesh, base: &Mat4, w: WeaponId, outline: f32) {
    add_gun_ex(m, base, w, outline, Vec3::ZERO);
}

/// Gun with its magazine moved by `mag_off` (gun space) — used by reload animations.
pub fn add_gun_ex(m: &mut ToonMesh, base: &Mat4, w: WeaponId, outline: f32, mag_off: Vec3) {
    for p in gun_parts(w) {
        let c = if p.mag { p.c + mag_off } else { p.c };
        m.boxed(&(*base * Mat4::from_translation(c)), p.h, p.col, outline);
    }
}

/// A weapon lying on the ground. `age` drives the short fall when it was just dropped;
/// `highlight` makes it bob a little when it can be picked up.
pub fn add_dropped_gun(
    m: &mut ToonMesh,
    pos: Vec3,
    yaw: f32,
    w: WeaponId,
    age: f32,
    highlight: bool,
) {
    let fall = (1.0 - age / 0.4).max(0.0);
    let lift = 1.0 * fall * fall
        + if highlight {
            0.06 + (age * 4.0).sin().abs() * 0.05
        } else {
            0.0
        };
    let base = trs(
        pos + vec3(0.0, 0.07 + lift, 0.0),
        yaw + fall * 3.0,
        0.0,
        std::f32::consts::FRAC_PI_2 * (1.0 - fall * 0.6),
    ) * Mat4::from_scale(Vec3::splat(1.25));
    add_gun(m, &base, w, if highlight { 0.03 } else { 0.016 });
}

/// Team colour of a character's silhouette outline.
pub fn team_ink(t: Team) -> [u8; 3] {
    match t {
        Team::Allies => [70, 150, 255],
        Team::Axis => [255, 78, 64],
        Team::Spectator => [24, 22, 30],
    }
}

pub fn add_grenade(m: &mut ToonMesh, pos: Vec3, t: f32) {
    let base = trs(pos, t * 5.0, t * 7.0, 0.0);
    m.sphere(&base, 0.11, [70, 110, 60], 0.012);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.12, 0.0))),
        vec3(0.03, 0.03, 0.03),
        METAL,
        0.01,
    );
}

/// Smoke grenade: a grey can with a white band.
pub fn add_smoke_grenade(m: &mut ToonMesh, pos: Vec3, t: f32) {
    let base = trs(pos, t * 5.0, t * 7.0, 0.0);
    m.boxed(&base, vec3(0.07, 0.12, 0.07), [120, 126, 132], 0.012);
    m.boxed(&base, vec3(0.075, 0.025, 0.075), [235, 235, 230], 0.0);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.14, 0.0))),
        vec3(0.03, 0.03, 0.03),
        METAL,
        0.01,
    );
}

/// A cartoon smoke cloud: a heap of soft grey puffs that swell, drift and thin out.
/// Opaque on purpose: it must really hide whoever is inside or behind it.
pub fn add_smoke_cloud(m: &mut ToonMesh, centre: Vec3, age: f32, id: u16, now: f32) {
    let r = toon_shared::consts::smoke_radius(age);
    if r < 0.05 {
        return;
    }
    let prev = (m.detail, m.ink);
    m.detail = 5;
    // a soft grey line around each puff gives the cartoon-cloud look
    m.ink = Some([150, 156, 166]);
    let mut rng = toon_shared::rng::Rng::new(id as u64 * 7919 + 13);
    for k in 0..24 {
        // puffs spread through the cloud, flattened a little, never below the ground
        let dir = vec3(
            rng.range(-1.0, 1.0),
            rng.range(-0.5, 0.9),
            rng.range(-1.0, 1.0),
        );
        let dir = dir.normalize_or_zero() * rng.range(0.25, 0.7);
        let drift = vec3(
            (now * 0.21 + k as f32).sin(),
            (now * 0.17 + k as f32 * 1.7).sin() * 0.5,
            (now * 0.19 + k as f32 * 2.3).cos(),
        ) * 0.1
            * r;
        let mut c = centre + dir * r + drift;
        let pr = r * rng.range(0.3, 0.45) * (1.0 + 0.06 * (now * 0.9 + k as f32).sin());
        c.y =
            c.y.max(centre.y - toon_shared::consts::SMOKE_LIFT + pr * 0.6);
        let g = rng.range(222.0, 242.0) as u8;
        m.sphere(
            &Mat4::from_translation(c),
            pr,
            [g, g, g.saturating_add(4)],
            0.05,
        );
    }
    (m.detail, m.ink) = prev;
}

pub fn add_bomb(m: &mut ToonMesh, pos: Vec3, t: f32, planted: bool) {
    let base = trs(
        pos + vec3(0.0, 0.22, 0.0),
        t * if planted { 0.0 } else { 1.2 },
        0.0,
        0.0,
    );
    m.boxed(&base, vec3(0.32, 0.2, 0.22), [214, 54, 46], 0.02);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.0, 0.0))),
        vec3(0.33, 0.06, 0.23),
        [250, 220, 70],
        0.0,
    );
    let light = if planted && (t * 4.0).fract() < 0.5 {
        [255, 255, 120]
    } else {
        [120, 30, 30]
    };
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.24, 0.0))),
        vec3(0.06, 0.05, 0.06),
        light,
        0.015,
    );
}

pub fn add_heli(m: &mut ToonMesh, pos: Vec3, yaw: f32, team: Team, t: f32) {
    let look = team_look(team);
    let base = trs(pos, yaw, 0.08, 0.0);
    m.boxed(&base, vec3(1.2, 1.0, 2.0), look.vest, 0.06);
    m.sphere(
        &(base
            * Mat4::from_translation(vec3(0.0, 0.1, -1.8))
            * Mat4::from_scale(vec3(1.0, 0.8, 0.9))),
        1.0,
        [160, 220, 250],
        0.06,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.3, 3.2))),
        vec3(0.25, 0.3, 1.8),
        look.vest,
        0.05,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.8, 4.8))),
        vec3(0.1, 0.7, 0.4),
        look.helmet,
        0.05,
    );
    for s in [-1.0f32, 1.0] {
        m.boxed(
            &(base * Mat4::from_translation(vec3(s * 0.9, -1.3, 0.0))),
            vec3(0.08, 0.08, 1.6),
            METAL,
            0.04,
        );
        m.boxed(
            &(base * Mat4::from_translation(vec3(s * 1.5, -0.3, -0.3))),
            vec3(0.3, 0.25, 0.7),
            METAL,
            0.04,
        );
    }
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 1.2, 0.0))),
        vec3(0.2, 0.25, 0.2),
        METAL,
        0.04,
    );
    for k in 0..2 {
        let r = base
            * Mat4::from_translation(vec3(0.0, 1.45, 0.0))
            * Mat4::from_rotation_y(t * 25.0 + k as f32 * std::f32::consts::FRAC_PI_2);
        m.boxed(&r, vec3(0.18, 0.03, 4.2), [40, 40, 46], 0.03);
    }
    let tr = base * Mat4::from_translation(vec3(0.25, 0.9, 4.8)) * Mat4::from_rotation_x(t * 30.0);
    m.boxed(&tr, vec3(0.03, 0.9, 0.12), [40, 40, 46], 0.0);
}

pub fn add_jet(m: &mut ToonMesh, pos: Vec3, dir: Vec3) {
    let yaw = toon_shared::yaw_of(dir);
    let base = trs(pos, yaw, 0.0, 0.0);
    let grey = [150, 160, 176];
    m.boxed(&base, vec3(0.7, 0.6, 4.0), grey, 0.08);
    m.sphere(
        &(base
            * Mat4::from_translation(vec3(0.0, 0.4, -2.2))
            * Mat4::from_scale(vec3(0.6, 0.5, 1.2))),
        1.0,
        [120, 200, 250],
        0.06,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.0, 0.5))),
        vec3(4.5, 0.12, 1.2),
        grey,
        0.06,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 1.0, 3.4))),
        vec3(0.1, 0.9, 0.6),
        grey,
        0.05,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.1, 3.6))),
        vec3(1.8, 0.1, 0.5),
        grey,
        0.05,
    );
    m.fill.add_sphere_ex(
        &(base * Mat4::from_translation(vec3(0.0, 0.0, 4.3))),
        0.5,
        [255, 170, 60],
        8,
        5,
        255,
        true,
    );
}

/// FPV kamikaze drone: an X frame with four rotors, a camera and a charge strapped
/// underneath. Drawn about twice life size so it can be spotted (and shot).
pub fn add_fpv(m: &mut ToonMesh, pos: Vec3, yaw: f32, pitch: f32, team: Team, t: f32) {
    let prev = m.ink.replace(team_ink(team));
    let base = trs(pos, yaw, pitch, 0.0) * Mat4::from_scale(Vec3::splat(1.6));
    let dark = [52, 54, 60];
    m.boxed(&base, vec3(0.07, 0.035, 0.11), dark, 0.012);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, -0.06, 0.01))),
        vec3(0.05, 0.04, 0.12),
        [104, 120, 70],
        0.01,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.02, -0.12))),
        vec3(0.025, 0.025, 0.02),
        [30, 30, 34],
        0.0,
    );
    for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
        let arm = base
            * Mat4::from_translation(vec3(sx * 0.07, 0.0, sz * 0.07))
            * Mat4::from_rotation_y(if sx * sz > 0.0 { -0.785 } else { 0.785 });
        m.boxed(&arm, vec3(0.012, 0.01, 0.1), dark, 0.0);
        let tip = base * Mat4::from_translation(vec3(sx * 0.14, 0.02, sz * 0.14));
        m.boxed(&tip, vec3(0.02, 0.02, 0.02), METAL, 0.0);
        // a spinning rotor reads as a pale disc
        let spin =
            tip * Mat4::from_translation(vec3(0.0, 0.025, 0.0)) * Mat4::from_rotation_y(t * 40.0);
        m.sphere(
            &(spin * Mat4::from_scale(vec3(1.0, 0.08, 1.0))),
            0.075,
            [205, 210, 218],
            0.0,
        );
    }
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.04, 0.0))),
        vec3(0.04, 0.01, 0.06),
        team_look(team).helmet,
        0.0,
    );
    m.ink = prev;
}

/// Bomber drone: a big hexacopter with a rack of bombs (`ammo` still hanging).
pub fn add_bomber(m: &mut ToonMesh, pos: Vec3, yaw: f32, team: Team, t: f32, ammo: u8) {
    let prev = m.ink.replace(team_ink(team));
    let base = trs(pos, yaw, 0.0, 0.0);
    let body = [70, 76, 70];
    m.rounded(&base, vec3(0.42, 0.16, 0.42), 0.12, body, 0.04, 2);
    m.rounded(
        &(base * Mat4::from_translation(vec3(0.0, 0.16, 0.0))),
        vec3(0.24, 0.08, 0.24),
        0.06,
        team_look(team).vest,
        0.03,
        2,
    );
    for k in 0..6 {
        let a = k as f32 * std::f32::consts::TAU / 6.0 + 0.52;
        let dir = vec3(a.sin(), 0.0, a.cos());
        let mid = base * Mat4::from_translation(dir * 0.65) * Mat4::from_rotation_y(a);
        m.boxed(&mid, vec3(0.04, 0.035, 0.38), body, 0.02);
        let tip = base * Mat4::from_translation(dir * 1.05 + vec3(0.0, 0.08, 0.0));
        m.boxed(&tip, vec3(0.07, 0.07, 0.07), METAL, 0.02);
        let spin = tip
            * Mat4::from_translation(vec3(0.0, 0.1, 0.0))
            * Mat4::from_rotation_y(t * 35.0 + k as f32);
        m.sphere(
            &(spin * Mat4::from_scale(vec3(1.0, 0.06, 1.0))),
            0.34,
            [200, 205, 212],
            0.0,
        );
    }
    for s in [-1.0f32, 1.0] {
        m.boxed(
            &(base * Mat4::from_translation(vec3(s * 0.3, -0.34, 0.0))),
            vec3(0.02, 0.2, 0.02),
            METAL,
            0.0,
        );
        m.boxed(
            &(base * Mat4::from_translation(vec3(s * 0.3, -0.54, 0.0))),
            vec3(0.03, 0.02, 0.3),
            METAL,
            0.0,
        );
    }
    for k in 0..ammo.min(6) {
        let x = (k as f32 - 2.5) * 0.11;
        let at = (base * Mat4::from_translation(vec3(x, -0.3, 0.0)))
            .w_axis
            .truncate();
        add_drop(m, at, 0.0);
    }
    m.ink = prev;
}

/// A bomb as dropped from the drone (nose down, with fins).
pub fn add_drop(m: &mut ToonMesh, pos: Vec3, t: f32) {
    let base = Mat4::from_translation(pos) * Mat4::from_rotation_y(t * 3.0);
    let olive = [96, 110, 66];
    m.sphere(
        &(base
            * Mat4::from_translation(vec3(0.0, -0.05, 0.0))
            * Mat4::from_scale(vec3(1.0, 1.4, 1.0))),
        0.07,
        olive,
        0.01,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.08, 0.0))),
        vec3(0.05, 0.06, 0.05),
        olive,
        0.01,
    );
    for r in [0.0f32, std::f32::consts::FRAC_PI_2] {
        m.boxed(
            &(base * Mat4::from_rotation_y(r) * Mat4::from_translation(vec3(0.0, 0.15, 0.0))),
            vec3(0.08, 0.04, 0.008),
            [60, 66, 50],
            0.0,
        );
    }
}

/// Ground robot (НРК): a small tracked chassis with a machine-gun turret.
/// `aim_pitch` tilts the gun.
pub fn add_nrk(
    m: &mut ToonMesh,
    pos: Vec3,
    yaw: f32,
    aim_pitch: f32,
    team: Team,
    t: f32,
    auto: bool,
) {
    let prev = m.ink.replace(team_ink(team));
    let base = trs(pos, yaw, 0.0, 0.0);
    let hull = [104, 112, 84];
    let dark = [44, 46, 50];
    for s in [-1.0f32, 1.0] {
        m.rounded(
            &(base * Mat4::from_translation(vec3(s * 0.42, 0.2, 0.0))),
            vec3(0.13, 0.2, 0.62),
            0.12,
            dark,
            0.03,
            2,
        );
        for k in 0..4 {
            let z = -0.42 + k as f32 * 0.28;
            m.sphere(
                &(base
                    * Mat4::from_translation(vec3(s * 0.56, 0.18, z))
                    * Mat4::from_rotation_x(t * 6.0)
                    * Mat4::from_scale(vec3(0.4, 1.0, 1.0))),
                0.11,
                [120, 124, 130],
                0.0,
            );
        }
    }
    m.rounded(
        &(base * Mat4::from_translation(vec3(0.0, 0.42, 0.0))),
        vec3(0.33, 0.16, 0.55),
        0.07,
        hull,
        0.03,
        2,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.5, 0.0))),
        vec3(0.335, 0.035, 0.4),
        team_look(team).vest,
        0.0,
    );
    let turret = base * Mat4::from_translation(vec3(0.0, 0.72, 0.05));
    m.rounded(&turret, vec3(0.2, 0.12, 0.22), 0.06, hull, 0.03, 2);
    let gun = turret
        * Mat4::from_translation(vec3(0.0, NRK_GUN_LOCAL_Y, -0.05))
        * Mat4::from_rotation_x(aim_pitch);
    m.boxed(
        &(gun * Mat4::from_translation(vec3(0.0, 0.0, -0.25))),
        vec3(0.07, 0.07, 0.2),
        dark,
        0.02,
    );
    m.boxed(
        &(gun * Mat4::from_translation(vec3(0.0, 0.0, -0.6))),
        vec3(0.025, 0.025, 0.2),
        dark,
        0.015,
    );
    m.boxed(
        &(gun * Mat4::from_translation(vec3(0.09, -0.04, -0.2))),
        vec3(0.05, 0.07, 0.09),
        [104, 120, 70],
        0.0,
    );
    m.boxed(
        &(turret * Mat4::from_translation(vec3(-0.13, 0.18, -0.1))),
        vec3(0.06, 0.05, 0.06),
        dark,
        0.015,
    );
    // an autonomous robot blinks red
    let led = if auto && (t * 3.0).fract() < 0.5 {
        [255, 80, 60]
    } else {
        [120, 200, 255]
    };
    m.fill.add_box_ex(
        &(turret * Mat4::from_translation(vec3(-0.13, 0.18, -0.165))),
        vec3(0.03, 0.025, 0.01),
        led,
        255,
        true,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.22, 0.9, 0.4))),
        vec3(0.01, 0.35, 0.01),
        dark,
        0.0,
    );
    m.ink = prev;
}

/// Gun height above the turret ring, so the muzzle sits where the server shoots from.
const NRK_GUN_LOCAL_Y: f32 = toon_shared::vehicles::NRK_GUN_Y - 0.72;

/// Battle royale supplies on the ground: a medkit, an ammo box, a vest or a grenade crate.
pub fn add_loot(m: &mut ToonMesh, pos: Vec3, kind: toon_shared::protocol::LootKind, t: f32) {
    use toon_shared::protocol::LootKind;
    // a slow bob and turn so they catch the eye
    let base = trs(
        pos + vec3(0.0, 0.25 + (t * 2.0).sin() * 0.05, 0.0),
        t * 0.8,
        0.0,
        0.0,
    );
    match kind {
        LootKind::Medkit => {
            m.boxed(&base, vec3(0.3, 0.18, 0.22), [240, 240, 236], 0.02);
            let cross = [220, 40, 40];
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.181, 0.0))),
                vec3(0.16, 0.005, 0.05),
                cross,
                0.0,
            );
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.181, 0.0))),
                vec3(0.05, 0.005, 0.16),
                cross,
                0.0,
            );
        }
        LootKind::Ammo(a) => {
            use toon_shared::inventory::AmmoType;
            // the box's band tells the calibre
            let band = match a {
                AmmoType::Pistol => [220, 200, 120],
                AmmoType::Smg => [120, 200, 230],
                AmmoType::Rifle => [230, 150, 60],
                AmmoType::Shotgun => [220, 70, 60],
                AmmoType::Sniper => [170, 110, 220],
                AmmoType::Lmg => [120, 210, 110],
            };
            m.boxed(&base, vec3(0.3, 0.15, 0.2), [86, 100, 60], 0.02);
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.0, 0.0))),
                vec3(0.31, 0.05, 0.21),
                band,
                0.0,
            );
        }
        LootKind::Armor(l) => {
            let c = level_color(l);
            m.boxed(&base, vec3(0.3, 0.34, 0.1), c, 0.02);
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.05, 0.11))),
                vec3(0.2, 0.18, 0.02),
                toon_shared::map::shade(c, 1.2),
                0.0,
            );
        }
        LootKind::Helmet(l) => {
            let c = level_color(l);
            m.sphere(
                &(base * Mat4::from_scale(vec3(1.0, 0.75, 1.1))),
                0.24,
                c,
                0.02,
            );
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, -0.06, 0.0))),
                vec3(0.27, 0.03, 0.29),
                toon_shared::map::shade(c, 0.8),
                0.0,
            );
        }
        LootKind::Backpack(l) => {
            let c = [[0, 0, 0], [110, 120, 80], [90, 110, 140], [70, 70, 80]][l.min(3) as usize];
            m.boxed(&base, vec3(0.24, 0.3, 0.14), c, 0.02);
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, -0.08, 0.15))),
                vec3(0.18, 0.12, 0.05),
                toon_shared::map::shade(c, 0.85),
                0.0,
            );
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, 0.32, 0.0))),
                vec3(0.18, 0.04, 0.12),
                level_color(l),
                0.0,
            );
        }
        LootKind::Sight(l) => {
            // the sight itself, a little bigger than on a gun, on a small olive case
            m.boxed(
                &(base * Mat4::from_translation(vec3(0.0, -0.08, 0.0))),
                vec3(0.2, 0.05, 0.14),
                [96, 104, 70],
                0.02,
            );
            let at = base * Mat4::from_scale(Vec3::splat(2.2));
            for p in sight_parts(l, 0.0) {
                m.boxed(&(at * Mat4::from_translation(p.c)), p.h, p.col, 0.01);
            }
        }
        LootKind::Grenades => {
            m.boxed(&base, vec3(0.3, 0.14, 0.24), [130, 100, 60], 0.02);
            for dx in [-0.12f32, 0.12] {
                m.sphere(
                    &(base * Mat4::from_translation(vec3(dx, 0.2, 0.0))),
                    0.09,
                    [70, 110, 60],
                    0.01,
                );
            }
        }
    }
}

/// A parachute canopy over a falling player.
pub fn add_parachute(m: &mut ToonMesh, pos: Vec3, yaw: f32, t: f32) {
    let sway = (t * 1.3).sin() * 0.06;
    let base = trs(pos + vec3(0.0, 4.6, 0.0), yaw, 0.0, sway);
    let colours = [[230, 90, 60], [250, 236, 210]];
    for k in 0..5 {
        let x = (k as f32 - 2.0) * 0.9;
        let y = -(k as f32 - 2.0).abs() * 0.25;
        m.boxed(
            &(base * Mat4::from_translation(vec3(x, y, 0.0))),
            vec3(0.46, 0.12, 1.1),
            colours[k % 2],
            0.03,
        );
    }
    // lines down to the harness
    for x in [-1.9f32, 1.9] {
        for z in [-0.9f32, 0.9] {
            let top = vec3(x, -0.4, z);
            let bottom = vec3(0.0, -3.0, 0.0);
            let mid = (top + bottom) * 0.5;
            let d = top - bottom;
            let rot = Quat::from_rotation_arc(Vec3::Y, d.normalize());
            m.fill.add_box_ex(
                &(base * Mat4::from_rotation_translation(rot, mid)),
                vec3(0.015, d.length() * 0.5, 0.015),
                [40, 40, 44],
                255,
                true,
            );
        }
    }
}

/// The battle royale transport plane (big, slow, propellers).
pub fn add_transport(m: &mut ToonMesh, pos: Vec3, yaw: f32, t: f32) {
    let base = trs(pos, yaw, 0.0, 0.0);
    let body = [176, 182, 168];
    let dark = [96, 104, 92];
    // fuselage along -z (forward)
    m.boxed(&base, vec3(2.2, 2.2, 13.0), body, 0.08);
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 0.4, -13.6))),
        vec3(1.6, 1.4, 1.2),
        dark,
        0.06,
    );
    // wings and tail
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 1.6, -2.0))),
        vec3(18.0, 0.3, 2.6),
        body,
        0.06,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 1.8, 11.5))),
        vec3(6.0, 0.25, 1.6),
        body,
        0.05,
    );
    m.boxed(
        &(base * Mat4::from_translation(vec3(0.0, 4.2, 11.8))),
        vec3(0.25, 2.4, 1.6),
        dark,
        0.05,
    );
    // engines with spinning propellers
    for x in [-7.0f32, -3.6, 3.6, 7.0] {
        let e = base * Mat4::from_translation(vec3(x, 1.0, -3.4));
        m.boxed(&e, vec3(0.7, 0.7, 1.6), dark, 0.04);
        let prop =
            e * Mat4::from_translation(vec3(0.0, 0.0, -1.7)) * Mat4::from_rotation_z(t * 25.0 + x);
        m.boxed(&prop, vec3(2.0, 0.15, 0.05), [40, 40, 40], 0.0);
    }
}

/// Colour of an equipment level: grey, blue, black with gold.
pub fn level_color(level: u8) -> [u8; 3] {
    match level {
        3 => [50, 50, 58],
        2 => [60, 96, 160],
        _ => [130, 136, 120],
    }
}

/// A battle royale jeep or boat (`hp` in per cent: a battered one looks it).
pub fn add_car(
    m: &mut ToonMesh,
    kind: toon_shared::cars::CarKind,
    s: &toon_shared::cars::CarState,
    hp: u8,
    t: f32,
) {
    use toon_shared::cars::CarKind;
    let rot = Mat4::from_rotation_y(s.yaw + std::f32::consts::PI);
    let worn = if hp < 35 { 0.7 } else { 1.0 };
    let mut part = |local: Vec3, half: Vec3, c: [u8; 3], outline: f32| {
        let at = s.to_world(local);
        let c = toon_shared::map::shade(c, worn);
        m.boxed(&(Mat4::from_translation(at) * rot), half, c, outline);
    };
    let o = 0.03;
    match kind {
        CarKind::Jeep => {
            let body = [104, 124, 72];
            let dark = [70, 84, 50];
            part(vec3(0.0, 0.62, -0.1), vec3(1.0, 0.3, 1.95), body, o);
            // the hood, a little higher in front
            part(vec3(0.0, 0.98, 1.3), vec3(0.95, 0.08, 0.6), body, o);
            part(vec3(0.0, 0.7, 2.0), vec3(0.9, 0.2, 0.08), dark, o);
            // windshield frame and roll bar
            part(vec3(0.0, 1.35, 0.72), vec3(0.95, 0.04, 0.04), dark, 0.0);
            part(vec3(-0.92, 1.15, 0.72), vec3(0.04, 0.25, 0.04), dark, 0.0);
            part(vec3(0.92, 1.15, 0.72), vec3(0.04, 0.25, 0.04), dark, 0.0);
            part(vec3(0.0, 1.6, -0.95), vec3(0.95, 0.04, 0.04), dark, 0.0);
            part(vec3(-0.92, 1.25, -0.95), vec3(0.04, 0.35, 0.04), dark, 0.0);
            part(vec3(0.92, 1.25, -0.95), vec3(0.04, 0.35, 0.04), dark, 0.0);
            // seats and the steering wheel (on the driver's side)
            for (x, z) in [(-0.45, 0.35), (0.45, 0.35), (-0.45, -0.85), (0.45, -0.85)] {
                part(
                    vec3(x, 1.0, z - 0.25),
                    vec3(0.3, 0.3, 0.06),
                    [60, 50, 40],
                    0.0,
                );
            }
            part(
                vec3(-0.45, 1.25, 0.62),
                vec3(0.18, 0.18, 0.03),
                [30, 30, 30],
                0.0,
            );
            // spare wheel and wheels (turning with the speed)
            part(
                vec3(0.0, 0.85, -2.1),
                vec3(0.32, 0.32, 0.12),
                [36, 36, 36],
                o,
            );
            let spin = (t * s.speed * 1.5).sin() * 0.02;
            for (x, z) in [(-1.05, 1.3), (1.05, 1.3), (-1.05, -1.35), (1.05, -1.35)] {
                part(
                    vec3(x, 0.4 + spin, z),
                    vec3(0.16, 0.38, 0.38),
                    [36, 36, 36],
                    o,
                );
            }
            // headlights
            part(
                vec3(-0.6, 0.85, 2.09),
                vec3(0.14, 0.1, 0.02),
                [250, 240, 170],
                0.0,
            );
            part(
                vec3(0.6, 0.85, 2.09),
                vec3(0.14, 0.1, 0.02),
                [250, 240, 170],
                0.0,
            );
        }
        CarKind::Boat => {
            let hull = [236, 236, 228];
            let stripe = [200, 60, 50];
            part(vec3(0.0, 0.15, -0.2), vec3(1.0, 0.32, 2.1), hull, o);
            part(vec3(0.0, 0.2, 2.15), vec3(0.6, 0.27, 0.35), hull, o);
            part(vec3(0.0, 0.3, 2.6), vec3(0.25, 0.18, 0.2), hull, o);
            part(vec3(0.0, 0.32, -0.2), vec3(1.02, 0.06, 2.12), stripe, 0.0);
            // seats, the console and the outboard motor
            for (x, z) in [(-0.45, 0.35), (0.45, 0.35), (-0.45, -0.85), (0.45, -0.85)] {
                part(
                    vec3(x, 0.55, z - 0.2),
                    vec3(0.3, 0.08, 0.25),
                    [70, 90, 120],
                    0.0,
                );
            }
            part(
                vec3(-0.45, 0.75, 0.75),
                vec3(0.25, 0.25, 0.12),
                [80, 80, 86],
                0.0,
            );
            part(
                vec3(0.0, 0.55, -2.45),
                vec3(0.22, 0.4, 0.22),
                [40, 40, 44],
                o,
            );
            // a wake when it moves
            if s.speed.abs() > 2.0 {
                let k = (s.speed.abs() / 16.0).min(1.0);
                part(
                    vec3(0.0, -0.1, -2.9),
                    vec3(0.6 * k, 0.04, 0.5 * k),
                    [240, 250, 255],
                    0.0,
                );
            }
        }
    }
}

/// A supply crate (under its parachute while it falls).
pub fn add_airdrop(m: &mut ToonMesh, pos: Vec3, landed: bool, t: f32) {
    let base = Mat4::from_translation(pos + vec3(0.0, 0.7, 0.0));
    m.boxed(&base, vec3(0.75, 0.7, 0.75), [60, 110, 170], 0.04);
    for (dx, dz) in [(0.0, 0.76), (0.0, -0.76), (0.76, 0.0), (-0.76, 0.0)] {
        m.boxed(
            &(base * Mat4::from_translation(vec3(dx, 0.0, dz))),
            vec3(
                if dx == 0.0 { 0.78 } else { 0.02 },
                0.16,
                if dz == 0.0 { 0.78 } else { 0.02 },
            ),
            [240, 150, 40],
            0.0,
        );
    }
    if !landed {
        add_parachute(m, pos - vec3(0.0, 1.0, 0.0), t * 0.2, t);
    }
}
