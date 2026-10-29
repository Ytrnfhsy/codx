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

/// Centre of the magazine (where the off hand goes when reloading).
pub fn gun_mag(w: WeaponId) -> Option<(Vec3, Vec3, [u8; 3])> {
    gun_parts(w)
        .iter()
        .find(|p| p.mag)
        .map(|p| (p.c, p.h, p.col))
}

/// Height of the gun's highest point (sights) in gun space.
pub fn sight_height(w: WeaponId) -> f32 {
    gun_parts(w)
        .iter()
        .map(|p| p.c.y + p.h.y)
        .fold(0.1, f32::max)
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
