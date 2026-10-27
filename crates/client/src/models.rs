//! Procedural toon models: soldiers, weapons, helicopter, jets, bomb, grenade.

use crate::mesh::{trs, ToonMesh};
use macroquad::prelude::*;
use toon_shared::physics::Stance;
use toon_shared::skin::Skin;
use toon_shared::weapons::{WeaponClass, WeaponId};
use toon_shared::Team;

pub const BOOT: [u8; 3] = [60, 52, 48];
const METAL: [u8; 3] = [58, 62, 72];
const METAL_L: [u8; 3] = [96, 102, 116];
const WOOD: [u8; 3] = [170, 104, 52];
const TAN: [u8; 3] = [196, 170, 120];
const OLIVE: [u8; 3] = [104, 120, 70];

pub struct TeamLook {
    pub helmet: [u8; 3],
    pub shirt: [u8; 3],
    pub pants: [u8; 3],
    pub vest: [u8; 3],
}

pub fn team_look(t: Team) -> TeamLook {
    match t {
        Team::Allies => TeamLook {
            helmet: [72, 142, 250],
            shirt: [92, 128, 196],
            pants: [60, 74, 120],
            vest: [52, 96, 170],
        },
        Team::Axis => TeamLook {
            helmet: [240, 92, 68],
            shirt: [204, 150, 96],
            pants: [122, 82, 58],
            vest: [176, 70, 56],
        },
        Team::Spectator => TeamLook {
            helmet: [200, 200, 200],
            shirt: [180, 180, 180],
            pants: [120, 120, 120],
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
    /// Detachable magazine / ammo box (moves during reloads).
    pub mag: bool,
}

/// Gun parts in gun space: barrel along -Z, origin at the grip.
pub fn gun_parts(w: WeaponId) -> Vec<GunPart> {
    gun_boxes(w)
        .into_iter()
        .map(|(c, h, col)| GunPart {
            c,
            h,
            col,
            mag: false,
        })
        .chain(gun_mag(w).map(|(c, h, col)| GunPart {
            c,
            h,
            col,
            mag: true,
        }))
        .collect()
}

/// Magazine box of a gun (None for tube-fed / internal magazines).
pub fn gun_mag(w: WeaponId) -> Option<(Vec3, Vec3, [u8; 3])> {
    match w.def().class {
        WeaponClass::Pistol => Some((vec3(0.0, -0.075, 0.0), vec3(0.022, 0.02, 0.028), METAL)),
        WeaponClass::Lmg => Some((vec3(0.0, -0.03, -0.12), vec3(0.06, 0.07, 0.07), OLIVE)),
        WeaponClass::Shotgun => None,
        WeaponClass::Sniper => Some((vec3(0.0, -0.01, -0.14), vec3(0.022, 0.035, 0.04), METAL)),
        _ => Some((vec3(0.0, -0.04, -0.17), vec3(0.025, 0.07, 0.035), METAL)),
    }
}

fn gun_boxes(w: WeaponId) -> Vec<(Vec3, Vec3, [u8; 3])> {
    let d = w.def();
    let accent = match w {
        WeaponId::Ak47 | WeaponId::Rpd | WeaponId::Ak74u => WOOD,
        WeaponId::G3 | WeaponId::M40 => OLIVE,
        WeaponId::P90 | WeaponId::M1014 | WeaponId::Deagle => TAN,
        WeaponId::W1200 | WeaponId::R700 => WOOD,
        _ => METAL_L,
    };
    let mut v = Vec::new();
    match d.class {
        WeaponClass::Pistol => {
            v.push((vec3(0.0, 0.07, -0.08), vec3(0.03, 0.04, 0.12), METAL));
            v.push((vec3(0.0, 0.0, 0.0), vec3(0.028, 0.07, 0.035), accent));
            v.push((
                vec3(0.0, 0.075, -0.2),
                vec3(0.012, 0.012, 0.01),
                [255, 220, 90],
            ));
        }
        _ => {
            let (len, h) = match d.class {
                WeaponClass::Smg => (0.42, 0.1),
                WeaponClass::Lmg => (0.62, 0.13),
                WeaponClass::Shotgun => (0.62, 0.09),
                WeaponClass::Sniper => (0.72, 0.09),
                _ => (0.55, 0.1),
            };
            // receiver
            v.push((vec3(0.0, 0.07, -0.12), vec3(0.04, h * 0.5, 0.22), METAL));
            // barrel
            v.push((
                vec3(0.0, 0.08, -0.34 - len * 0.35),
                vec3(0.016, 0.016, len * 0.38),
                METAL,
            ));
            // handguard
            v.push((vec3(0.0, 0.065, -0.4), vec3(0.042, 0.04, 0.14), accent));
            // stock
            v.push((vec3(0.0, 0.04, 0.2), vec3(0.03, 0.055, 0.13), accent));
            // grip
            v.push((vec3(0.0, -0.02, 0.0), vec3(0.025, 0.06, 0.03), METAL));
            match d.class {
                WeaponClass::Lmg => {
                    v.push((vec3(0.0, 0.0, -0.6), vec3(0.04, 0.008, 0.008), METAL));
                }
                WeaponClass::Sniper => {
                    v.push((vec3(0.0, 0.16, -0.12), vec3(0.03, 0.03, 0.16), [30, 30, 36]));
                    v.push((vec3(0.0, 0.16, -0.29), vec3(0.04, 0.04, 0.02), [30, 30, 36]));
                }
                WeaponClass::Shotgun => {
                    v.push((vec3(0.0, 0.035, -0.38), vec3(0.03, 0.025, 0.1), WOOD));
                }
                _ => {
                    // front sight
                    v.push((vec3(0.0, 0.13, -0.5), vec3(0.008, 0.02, 0.008), METAL));
                    v.push((vec3(0.0, 0.13, -0.02), vec3(0.015, 0.015, 0.012), METAL));
                }
            }
            // a fun orange tip so guns read as toys
            v.push((
                vec3(0.0, 0.08, -0.34 - len * 0.73),
                vec3(0.02, 0.02, 0.012),
                [255, 150, 40],
            ));
        }
    }
    v
}

/// Muzzle offset in gun space.
pub fn muzzle(w: WeaponId) -> Vec3 {
    let d = w.def();
    let len = match d.class {
        WeaponClass::Pistol => return vec3(0.0, 0.075, -0.22),
        WeaponClass::Smg => 0.42,
        WeaponClass::Lmg => 0.62,
        WeaponClass::Shotgun => 0.62,
        WeaponClass::Sniper => 0.72,
        _ => 0.55,
    };
    vec3(0.0, 0.08, -0.34 - len * 0.75)
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

fn mul(c: [u8; 3], k: f32) -> [u8; 3] {
    [
        (c[0] as f32 * k).min(255.0) as u8,
        (c[1] as f32 * k).min(255.0) as u8,
        (c[2] as f32 * k).min(255.0) as u8,
    ]
}

const INK: [u8; 3] = [24, 22, 30];

/// Team colour of a character's silhouette outline.
pub fn team_ink(t: Team) -> [u8; 3] {
    match t {
        Team::Allies => [70, 150, 255],
        Team::Axis => [255, 78, 64],
        Team::Spectator => [24, 22, 30],
    }
}

fn head_rects() -> [[f32; 4]; 6] {
    use toon_shared::skin::tex::*;
    [
        cell(HEAD_RIGHT),
        cell(HEAD_LEFT),
        cell(HEAD_TOP),
        cell(HEAD_BOTTOM),
        cell(HEAD_BACK),
        cell(HEAD_FRONT),
    ]
}

fn torso_rects() -> [[f32; 4]; 6] {
    use toon_shared::skin::tex::*;
    [
        cell(TORSO_RIGHT),
        cell(TORSO_LEFT),
        half(TORSO_ENDS, false),
        half(TORSO_ENDS, true),
        cell(TORSO_BACK),
        cell(TORSO_FRONT),
    ]
}

/// A rounded body part: painted (texture + outline) or plain colour.
#[allow(clippy::too_many_arguments)]
fn part_box(
    m: &mut ToonMesh,
    painted: &mut Option<&mut ToonMesh>,
    mat: &Mat4,
    half: Vec3,
    r: f32,
    color: [u8; 3],
    ow: f32,
    lod: u32,
    rects: &[[f32; 4]; 6],
) {
    match painted {
        Some(pm) => {
            pm.fill.add_rounded_box_painted(mat, half, r, lod, rects);
            if ow > 0.0 {
                let ink = m.ink.unwrap_or(crate::mesh::OUTLINE);
                m.outline.add_rounded_box_ex(
                    mat,
                    half + Vec3::splat(ow),
                    r + ow,
                    ink,
                    255,
                    true,
                    lod.min(3),
                );
            }
        }
        None => m.rounded(mat, half, r, color, ow, lod),
    }
}

/// A limb: `top` is where the texture's upper edge goes (shoulder / hip).
#[allow(clippy::too_many_arguments)]
fn part_capsule(
    m: &mut ToonMesh,
    painted: &mut Option<&mut ToonMesh>,
    bottom: Vec3,
    top: Vec3,
    r: f32,
    color: [u8; 3],
    ow: f32,
    seg: u32,
    right: Vec3,
    rect: [f32; 4],
) {
    match painted {
        Some(pm) => {
            let rings = if m.detail >= 3 || m.detail == 0 { 3 } else { 2 };
            pm.fill
                .add_capsule_painted(bottom, top, r, seg.max(6), rings, right, rect);
            if ow > 0.0 {
                let ink = m.ink.unwrap_or(crate::mesh::OUTLINE);
                m.outline
                    .add_capsule_ex(bottom, top, r + ow, ink, 255, true, seg.min(8), 2);
            }
        }
        None => m.capsule(top, bottom, r, color, ow, seg),
    }
}

fn part_sphere(
    m: &mut ToonMesh,
    painted: &mut Option<&mut ToonMesh>,
    mat: &Mat4,
    r: f32,
    color: [u8; 3],
    ow: f32,
    rect: [f32; 4],
) {
    match painted {
        Some(pm) => {
            pm.fill.add_sphere_painted(mat, r, 8, 6, rect);
            if ow > 0.0 {
                let ink = m.ink.unwrap_or(crate::mesh::OUTLINE);
                m.outline.add_sphere_ex(mat, r + ow, ink, 8, 6, 255, true);
            }
        }
        None => m.sphere(mat, r, color, ow),
    }
}

/// Resolved colours of a soldier (skin choices + team colours).
pub struct Palette {
    pub tone: [u8; 3],
    pub hair: [u8; 3],
    pub hat: [u8; 3],
    pub shirt: [u8; 3],
    pub pants: [u8; 3],
    pub vest: [u8; 3],
    pub team: [u8; 3],
}

pub fn palette(skin: &Skin, team: Team) -> Palette {
    let look = team_look(team);
    Palette {
        tone: skin.tone_rgb(),
        hair: skin.hair_rgb(),
        hat: Skin::cloth(skin.hat_color).unwrap_or(look.helmet),
        shirt: Skin::cloth(skin.shirt).unwrap_or(look.shirt),
        pants: Skin::cloth(skin.pants).unwrap_or(look.pants),
        vest: look.vest,
        team: look.helmet,
    }
}

/// Cute rounded head with hair/hat, eyes, mouth and face extras. `head` is the centre
/// transform (the face looks along -Z). `lod` = rounded-box subdivisions.
#[allow(clippy::too_many_arguments)]
pub fn add_head(
    m: &mut ToonMesh,
    mut painted: Option<&mut ToonMesh>,
    head: &Mat4,
    skin: &Skin,
    team: Team,
    blink: bool,
    dead: bool,
    ow: f32,
    lod: u32,
) {
    let pal = palette(skin, team);
    let at = |x: f32, y: f32, z: f32| *head * Mat4::from_translation(vec3(x, y, z));
    let small = lod.min(3);
    let is_painted = painted.is_some();
    part_box(
        m,
        &mut painted,
        head,
        vec3(0.29, 0.27, 0.27),
        0.17,
        pal.tone,
        ow,
        lod,
        &head_rects(),
    );
    // ears (a painted skin draws its own)
    if !is_painted {
        for x in [-0.29f32, 0.29] {
            m.sphere(&at(x, -0.03, 0.03), 0.06, mul(pal.tone, 0.95), ow * 0.6);
        }
    }
    let hat = skin.hat_style();
    let style = skin.hair_style();

    // ---- hair
    let cap = |m: &mut ToonMesh| {
        m.rounded(
            &at(0.0, 0.13, 0.03),
            vec3(0.305, 0.17, 0.285),
            0.16,
            pal.hair,
            ow,
            small,
        );
    };
    let fringe = |m: &mut ToonMesh| {
        m.rounded(
            &(at(0.0, 0.2, -0.235) * Mat4::from_rotation_x(0.25)),
            vec3(0.24, 0.06, 0.05),
            0.045,
            pal.hair,
            0.0,
            2,
        );
    };
    let hatless = hat == 0;
    match style {
        1 if hatless => {
            cap(m);
            fringe(m);
        }
        2 if hatless => {
            cap(m);
            for (k, x) in [-0.18f32, -0.09, 0.0, 0.09, 0.18].iter().enumerate() {
                let tilt = *x * 1.6;
                let spike = at(*x, 0.31, -0.02 + (k % 2) as f32 * 0.08)
                    * Mat4::from_rotation_z(-tilt)
                    * Mat4::from_rotation_x(0.2);
                m.rounded(&spike, vec3(0.05, 0.09, 0.05), 0.04, pal.hair, ow * 0.6, 2);
            }
        }
        3 if hatless => {
            cap(m);
            m.rounded(
                &(at(0.0, 0.3, -0.15) * Mat4::from_rotation_x(-0.45)),
                vec3(0.2, 0.08, 0.13),
                0.07,
                pal.hair,
                ow,
                small,
            );
        }
        4 => {
            if hatless {
                cap(m);
                fringe(m);
            }
            m.rounded(
                &at(0.0, -0.1, 0.2),
                vec3(0.3, 0.26, 0.1),
                0.09,
                pal.hair,
                ow,
                small,
            );
            for x in [-0.29f32, 0.29] {
                m.rounded(
                    &at(x, -0.1, -0.03),
                    vec3(0.04, 0.2, 0.12),
                    0.04,
                    pal.hair,
                    ow * 0.6,
                    2,
                );
            }
        }
        5 => {
            if hatless {
                cap(m);
                fringe(m);
            }
            let a = head.transform_point3(vec3(0.0, 0.12, 0.27));
            let b = head.transform_point3(vec3(0.0, -0.22, 0.42));
            m.capsule(a, b, 0.07, pal.hair, ow * 0.6, 8);
            m.sphere(&at(0.0, 0.1, 0.3), 0.045, pal.hat, 0.0);
        }
        6 if hatless => {
            m.rounded(
                &at(0.0, 0.32, 0.03),
                vec3(0.05, 0.1, 0.25),
                0.04,
                pal.hair,
                ow,
                small,
            );
        }
        7 if hatless => {
            m.sphere(
                &(at(0.0, 0.12, 0.08) * Mat4::from_scale(vec3(1.25, 1.0, 1.15))),
                0.3,
                pal.hair,
                ow,
            );
        }
        _ => {}
    }

    // ---- hats
    match hat {
        1 => {
            m.rounded(
                &at(0.0, 0.15, 0.02),
                vec3(0.33, 0.17, 0.31),
                0.16,
                pal.hat,
                ow,
                lod,
            );
            m.rounded(
                &at(0.0, 0.03, -0.31),
                vec3(0.3, 0.025, 0.06),
                0.02,
                pal.hat,
                ow * 0.6,
                2,
            );
            m.rounded(
                &at(0.0, 0.2, -0.3),
                vec3(0.07, 0.05, 0.03),
                0.02,
                pal.team,
                0.0,
                2,
            );
        }
        2 => {
            m.rounded(
                &at(0.0, 0.17, 0.02),
                vec3(0.315, 0.14, 0.295),
                0.14,
                pal.hat,
                ow,
                lod,
            );
            m.rounded(
                &at(0.0, 0.06, -0.37),
                vec3(0.24, 0.02, 0.13),
                0.02,
                mul(pal.hat, 0.85),
                ow * 0.6,
                2,
            );
            m.sphere(&at(0.0, 0.32, 0.02), 0.03, mul(pal.hat, 0.8), 0.0);
        }
        3 => {
            m.rounded(
                &at(0.0, 0.18, 0.02),
                vec3(0.315, 0.16, 0.295),
                0.16,
                pal.hat,
                ow,
                lod,
            );
            m.rounded(
                &at(0.0, 0.05, 0.02),
                vec3(0.325, 0.055, 0.305),
                0.05,
                mul(pal.hat, 0.85),
                0.0,
                small,
            );
            m.sphere(&at(0.0, 0.37, 0.02), 0.08, [245, 245, 240], ow * 0.6);
        }
        4 => {
            m.sphere(
                &(at(0.05, 0.25, 0.02)
                    * Mat4::from_rotation_z(-0.2)
                    * Mat4::from_scale(vec3(1.2, 0.35, 1.1))),
                0.3,
                pal.hat,
                ow,
            );
            m.sphere(&at(0.08, 0.36, 0.02), 0.025, mul(pal.hat, 0.7), 0.0);
        }
        5 => {
            m.rounded(
                &at(0.0, 0.13, 0.0),
                vec3(0.305, 0.05, 0.29),
                0.05,
                pal.hat,
                ow * 0.6,
                small,
            );
            m.sphere(&at(0.0, 0.12, 0.3), 0.05, pal.hat, 0.0);
            let k = head.transform_point3(vec3(0.0, 0.12, 0.31));
            for x in [-0.06f32, 0.06] {
                m.capsule(
                    k,
                    head.transform_point3(vec3(x, -0.02, 0.38)),
                    0.025,
                    pal.hat,
                    0.0,
                    6,
                );
            }
        }
        6 => {
            m.rounded(
                &at(0.0, 0.2, 0.02),
                vec3(0.31, 0.12, 0.29),
                0.12,
                pal.hat,
                ow,
                lod,
            );
            m.sphere(
                &(at(0.0, 0.1, 0.02) * Mat4::from_scale(vec3(1.0, 0.1, 1.0))),
                0.48,
                mul(pal.hat, 0.92),
                ow * 0.6,
            );
        }
        _ => {}
    }

    // ---- face (front at z = -0.27); painted skins have the face in their picture
    if !is_painted {
        add_face(m, head, skin, &pal, blink, dead, ow);
    }
    // headphones
    if skin.extra_style() == 3 {
        let l = head.transform_point3(vec3(-0.31, 0.0, 0.02));
        let r = head.transform_point3(vec3(0.31, 0.0, 0.02));
        let top = head.transform_point3(vec3(0.0, 0.36, 0.02));
        let ml = head.transform_point3(vec3(-0.27, 0.28, 0.02));
        let mr = head.transform_point3(vec3(0.27, 0.28, 0.02));
        for (a, b) in [(l, ml), (ml, top), (top, mr), (mr, r)] {
            m.capsule(a, b, 0.025, [50, 50, 60], 0.0, 6);
        }
        for x in [-0.32f32, 0.32] {
            m.rounded(
                &at(x, -0.01, 0.02),
                vec3(0.05, 0.085, 0.085),
                0.04,
                [50, 50, 60],
                ow * 0.6,
                2,
            );
            m.rounded(
                &at(x * 1.12, -0.01, 0.02),
                vec3(0.02, 0.06, 0.06),
                0.02,
                pal.hat,
                0.0,
                1,
            );
        }
    }
}

/// Eyes, nose, mouth and face extras of the standard (unpainted) look.
fn add_face(
    m: &mut ToonMesh,
    head: &Mat4,
    skin: &Skin,
    pal: &Palette,
    blink: bool,
    dead: bool,
    ow: f32,
) {
    let at = |x: f32, y: f32, z: f32| *head * Mat4::from_translation(vec3(x, y, z));
    let face = skin.face_style();
    let ez = if face == 5 { -0.3 } else { -0.27 };
    let disc = |m: &mut ToonMesh, x: f32, y: f32, z: f32, r: f32, c: [u8; 3]| {
        m.fill.add_sphere(
            &(at(x, y, z) * Mat4::from_scale(vec3(1.0, 1.0, 0.35))),
            r,
            c,
            6,
            4,
        );
    };
    let bar = |m: &mut ToonMesh, x: f32, y: f32, z: f32, w: f32, rot: f32, c: [u8; 3]| {
        m.fill.add_box(
            &(at(x, y, z) * Mat4::from_rotation_z(rot)),
            vec3(w, 0.012, 0.01),
            c,
        );
    };
    if face == 5 {
        // glasses: frame + light lenses, eyes drawn in front
        for x in [-0.11f32, 0.11] {
            m.rounded(
                &at(x, -0.03, -0.275),
                vec3(0.085, 0.07, 0.01),
                0.02,
                INK,
                0.0,
                1,
            );
            m.rounded(
                &at(x, -0.03, -0.282),
                vec3(0.07, 0.055, 0.01),
                0.02,
                [210, 235, 255],
                0.0,
                1,
            );
        }
        bar(m, 0.0, -0.01, -0.28, 0.04, 0.0, INK);
    }
    let covered = face == 6;
    for (side, x) in [(-1.0f32, -0.11f32), (1.0, 0.11)] {
        if covered || (face == 7 && side > 0.0) {
            continue;
        }
        if dead {
            for r in [0.7f32, -0.7] {
                bar(m, x, -0.03, ez - 0.03, 0.06, r, INK);
            }
            continue;
        }
        if blink {
            bar(m, x, -0.03, ez - 0.02, 0.06, 0.0, INK);
            continue;
        }
        match skin.eye_style() {
            0 | 3 | 4 => {
                disc(m, x, -0.03, ez + 0.01, 0.078, [255, 255, 255]);
                disc(m, x + side * -0.01, -0.035, ez - 0.012, 0.048, INK);
                disc(m, x + 0.018, -0.012, ez - 0.03, 0.016, [255, 255, 255]);
                if skin.eye_style() == 3 {
                    bar(m, x, 0.07, ez - 0.02, 0.06, side * 0.35, INK);
                }
                if skin.eye_style() == 4 {
                    m.fill
                        .add_box(&at(x, 0.02, ez - 0.035), vec3(0.085, 0.035, 0.01), pal.tone);
                    bar(m, x, -0.012, ez - 0.045, 0.075, 0.0, INK);
                }
            }
            1 => {
                disc(m, x, -0.03, ez, 0.058, INK);
                disc(m, x + 0.018, -0.012, ez - 0.02, 0.016, [255, 255, 255]);
            }
            _ => {
                // happy "^" eyes
                bar(m, x - 0.028, -0.025, ez - 0.01, 0.04, 0.6, INK);
                bar(m, x + 0.028, -0.025, ez - 0.01, 0.04, -0.6, INK);
            }
        }
    }
    // nose
    m.fill
        .add_sphere(&at(0.0, -0.08, -0.275), 0.03, mul(pal.tone, 0.88), 6, 4);
    // face extras
    match face {
        1 => {
            for x in [-0.17f32, 0.17] {
                disc(m, x, -0.1, -0.255, 0.045, [255, 150, 150]);
            }
        }
        2 => {
            for (x, y) in [
                (-0.17f32, -0.09f32),
                (-0.2, -0.11),
                (-0.15, -0.12),
                (0.17, -0.09),
                (0.2, -0.11),
                (0.15, -0.12),
            ] {
                disc(m, x, y, -0.262, 0.012, mul(pal.tone, 0.7));
            }
        }
        3 => {
            m.rounded(
                &at(0.0, -0.115, -0.28),
                vec3(0.09, 0.022, 0.02),
                0.018,
                pal.hair,
                0.0,
                1,
            );
        }
        4 => {
            m.rounded(
                &at(0.0, -0.17, -0.1),
                vec3(0.28, 0.11, 0.2),
                0.1,
                pal.hair,
                ow,
                3,
            );
        }
        6 => {
            for x in [-0.11f32, 0.11] {
                m.rounded(
                    &at(x, -0.03, -0.285),
                    vec3(0.1, 0.065, 0.015),
                    0.03,
                    INK,
                    0.0,
                    2,
                );
            }
            bar(m, 0.0, -0.01, -0.29, 0.04, 0.0, INK);
        }
        7 => {
            disc(m, 0.11, -0.03, -0.275, 0.075, INK);
            m.rounded(
                &(at(0.0, 0.02, 0.0) * Mat4::from_rotation_z(0.35)),
                vec3(0.295, 0.015, 0.28),
                0.015,
                INK,
                0.0,
                1,
            );
        }
        _ => {}
    }
    // mouth (hidden by a beard)
    if face != 4 {
        let my = -0.155;
        match skin.mouth_style() {
            1 | 3 => {
                bar(m, -0.035, my + 0.008, -0.273, 0.025, -0.45, INK);
                bar(m, 0.0, my - 0.003, -0.275, 0.02, 0.0, INK);
                bar(m, 0.035, my + 0.008, -0.273, 0.025, 0.45, INK);
                if skin.mouth_style() == 3 {
                    disc(m, 0.012, my - 0.025, -0.268, 0.025, [250, 120, 140]);
                }
            }
            2 => disc(m, 0.0, my, -0.27, 0.028, INK),
            _ => {}
        }
    }
}

pub struct SoldierPose {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub stance: Stance,
    pub team: Team,
    pub weapon: WeaponId,
    pub skin: Skin,
    /// Rounded-box subdivisions (level of detail by distance).
    pub lod: u32,
    /// Walk cycle phase (radians).
    pub phase: f32,
    /// 0..1 amount of walking.
    pub walk: f32,
    pub sprint: bool,
    /// Seconds since death (None = alive).
    pub dead: Option<f32>,
    pub has_bomb: bool,
    pub blink: bool,
    /// Third-person melee swing 0..1.
    pub melee: f32,
    /// Seconds into a reload (None = not reloading).
    pub reload: Option<f32>,
}

/// Decorations for a clothing pattern on a body part (`at` maps local points of a unit
/// part to the world, `c` is the cloth colour).
fn pattern_marks(m: &mut ToonMesh, pattern: usize, c: [u8; 3], spots: &[Mat4]) {
    let col = match pattern {
        1 => mul(c, 0.7),
        2 => mul(c, 0.78),
        _ => [245, 245, 240],
    };
    for (k, s) in spots.iter().enumerate() {
        match pattern {
            1 => {
                let c2 = if k % 2 == 0 { col } else { mul(c, 1.18) };
                m.fill.add_sphere(
                    &(*s * Mat4::from_scale(vec3(1.4, 0.8, 0.35))),
                    0.06,
                    c2,
                    6,
                    4,
                );
            }
            3 => m.fill.add_sphere(
                &(*s * Mat4::from_scale(vec3(1.0, 1.0, 0.35))),
                0.028,
                col,
                6,
                4,
            ),
            _ => {}
        }
    }
}

/// Builds a cute rounded soldier. Returns the world-space muzzle position.
pub fn add_soldier(m: &mut ToonMesh, mut painted: Option<&mut ToonMesh>, p: &SoldierPose) -> Vec3 {
    use toon_shared::skin::tex;
    let pal = palette(&p.skin, p.team);
    let is_painted = painted.is_some();
    // a team-coloured silhouette tells friend from foe whatever the skin looks like
    let ow = 0.03;
    let lod = p.lod.max(1);
    let prev_detail = m.detail;
    m.detail = lod;
    m.ink = Some(team_ink(p.team));
    let small = lod.min(3);
    let cap_seg = (lod * 2 + 2).min(10);
    // root transform (death/prone rotations around the feet)
    let mut root = trs(p.pos, p.yaw, 0.0, 0.0);
    let mut crouch = 0.0;
    match (p.dead, p.stance) {
        (Some(t), _) => {
            let k = (t / 0.45).min(1.0);
            let k = 1.0 - (1.0 - k) * (1.0 - k);
            let sink = ((t - 4.0) / 2.0).clamp(0.0, 1.0) * 0.6;
            root = root
                * Mat4::from_translation(vec3(0.0, 0.15 * k - sink, 0.6 * k))
                * Mat4::from_rotation_x(1.5 * k);
        }
        (None, Stance::Prone) => {
            root =
                root * Mat4::from_translation(vec3(0.0, 0.22, 0.78)) * Mat4::from_rotation_x(-1.5);
        }
        (None, Stance::Crouch) => crouch = 0.5,
        _ => {}
    }
    let hip = 0.8 - crouch;
    let swing = (p.phase).sin() * 0.7 * p.walk;
    let pt = |mat: &Mat4, v: Vec3| mat.transform_point3(v);
    let pattern = p.skin.pattern_style();

    // legs
    let body_right = root.transform_vector3(Vec3::X);
    let boot_rects = [tex::half(tex::HANDS_BOOTS, true); 6];
    for (side, s) in [(-1.0f32, 1.0f32), (1.0, -1.0)] {
        let x = side * 0.13;
        let rect = tex::cell(if side > 0.0 {
            tex::LEG_RIGHT
        } else {
            tex::LEG_LEFT
        });
        let (hip_p, foot_p, boot) = if crouch > 0.0 && p.dead.is_none() {
            let knee = pt(&root, vec3(x, hip + 0.02, -0.36 + swing * 0.04 * s));
            let hip_p = pt(&root, vec3(x, hip + 0.04, 0.0));
            let upper = [rect[0], rect[1], rect[2], (rect[1] + rect[3]) * 0.5];
            let lower = [rect[0], upper[3], rect[2], rect[3]];
            part_capsule(
                m,
                &mut painted,
                knee,
                hip_p,
                0.11,
                pal.pants,
                ow,
                cap_seg,
                body_right,
                upper,
            );
            let foot = pt(&root, vec3(x, 0.12, -0.4));
            part_capsule(
                m,
                &mut painted,
                foot,
                knee,
                0.105,
                pal.pants,
                ow,
                cap_seg,
                body_right,
                lower,
            );
            (
                hip_p,
                foot,
                root * Mat4::from_translation(vec3(x, 0.075, -0.44)),
            )
        } else {
            let leg =
                root * Mat4::from_translation(vec3(x, hip, 0.0)) * Mat4::from_rotation_x(swing * s);
            let a = pt(&leg, vec3(0.0, -0.04, 0.0));
            let b = pt(&leg, vec3(0.0, -0.6, 0.0));
            part_capsule(
                m,
                &mut painted,
                b,
                a,
                0.125,
                pal.pants,
                ow,
                cap_seg,
                body_right,
                rect,
            );
            (a, b, leg * Mat4::from_translation(vec3(0.0, -0.71, -0.04)))
        };
        part_box(
            m,
            &mut painted,
            &boot,
            vec3(0.13, 0.085, 0.18),
            0.07,
            BOOT,
            ow,
            small,
            &boot_rects,
        );
        if is_painted {
            continue;
        }
        if pattern == 2 {
            m.capsule(
                hip_p.lerp(foot_p, 0.45),
                hip_p.lerp(foot_p, 0.55),
                0.116,
                mul(pal.pants, 0.78),
                0.0,
                cap_seg,
            );
        } else if pattern != 0 && lod > 2 {
            let dir = (foot_p - hip_p).normalize_or_zero();
            let fwd = root.transform_vector3(Vec3::NEG_Z);
            let spots: Vec<Mat4> = [0.3f32, 0.7]
                .iter()
                .map(|t| {
                    let c = hip_p.lerp(foot_p, *t) + fwd * 0.105;
                    Mat4::from_rotation_translation(Quat::from_rotation_arc(Vec3::Y, dir), c)
                })
                .collect();
            pattern_marks(m, pattern, pal.pants, &spots);
        }
    }

    // torso
    let torso_base = root
        * Mat4::from_translation(vec3(0.0, hip, 0.0))
        * Mat4::from_rotation_x(if p.sprint { -0.18 } else { 0.0 });
    let tat = |x: f32, y: f32, z: f32| torso_base * Mat4::from_translation(vec3(x, y, z));
    part_box(
        m,
        &mut painted,
        &tat(0.0, 0.3, 0.0),
        vec3(0.27, 0.3, 0.18),
        0.13,
        pal.shirt,
        ow,
        lod,
        &torso_rects(),
    );
    if !is_painted {
        m.rounded(
            &tat(0.0, 0.33, 0.0),
            vec3(0.29, 0.19, 0.2),
            0.1,
            pal.vest,
            ow * 0.5,
            small,
        );
        let pocket = mul(pal.vest, 0.82);
        for x in [-0.12f32, 0.12] {
            m.rounded(
                &tat(x, 0.27, -0.205),
                vec3(0.075, 0.06, 0.025),
                0.02,
                pocket,
                0.0,
                1,
            );
        }
        m.rounded(
            &tat(0.0, 0.06, 0.0),
            vec3(0.28, 0.045, 0.19),
            0.04,
            BOOT,
            0.0,
            small,
        );
        m.rounded(
            &tat(0.0, 0.06, -0.195),
            vec3(0.04, 0.03, 0.01),
            0.01,
            [230, 190, 70],
            0.0,
            1,
        );
        for x in [-0.31f32, 0.31] {
            m.rounded(
                &tat(x, 0.55, 0.0),
                vec3(0.1, 0.06, 0.13),
                0.05,
                pal.team,
                ow * 0.6,
                small,
            );
        }
        match pattern {
            2 => {
                for y in [0.13f32, 0.47] {
                    m.rounded(
                        &tat(0.0, y, 0.0),
                        vec3(0.275, 0.025, 0.185),
                        0.025,
                        mul(pal.shirt, 0.78),
                        0.0,
                        small,
                    );
                }
            }
            1 | 3 if lod > 2 => {
                let spots: Vec<Mat4> = [
                    (-0.15f32, 0.12f32, -0.185f32),
                    (0.14, 0.15, -0.185),
                    (0.0, 0.1, -0.185),
                    (-0.12, 0.25, 0.205),
                    (0.12, 0.35, 0.205),
                    (0.0, 0.14, 0.185),
                ]
                .iter()
                .map(|(x, y, z)| {
                    let flip = if *z > 0.0 {
                        Mat4::from_rotation_y(std::f32::consts::PI)
                    } else {
                        Mat4::IDENTITY
                    };
                    tat(*x, *y, *z) * flip
                })
                .collect();
                pattern_marks(m, pattern, pal.shirt, &spots);
            }
            _ => {}
        }
    }
    match p.skin.extra_style() {
        1 | 4 => {
            let c = [92, 108, 66];
            m.rounded(
                &tat(0.0, 0.32, 0.27),
                vec3(0.21, 0.23, 0.1),
                0.08,
                c,
                ow,
                small,
            );
            m.rounded(
                &tat(0.0, 0.22, 0.37),
                vec3(0.14, 0.09, 0.03),
                0.03,
                mul(c, 0.85),
                0.0,
                1,
            );
            if p.skin.extra_style() == 4 {
                let a = pt(&torso_base, vec3(0.14, 0.5, 0.3));
                m.capsule(
                    a,
                    a + torso_base.transform_vector3(vec3(0.05, 0.55, 0.08)),
                    0.012,
                    INK,
                    0.0,
                    4,
                );
            }
        }
        2 => {
            m.rounded(
                &tat(0.0, 0.6, 0.0),
                vec3(0.24, 0.06, 0.2),
                0.06,
                pal.hat,
                ow * 0.6,
                small,
            );
            m.rounded(
                &tat(0.12, 0.44, -0.2),
                vec3(0.06, 0.14, 0.03),
                0.03,
                pal.hat,
                0.0,
                1,
            );
        }
        _ => {}
    }
    if p.has_bomb {
        m.rounded(
            &tat(0.0, 0.3, 0.27),
            vec3(0.2, 0.2, 0.09),
            0.05,
            [220, 50, 40],
            ow,
            small,
        );
    }

    // head — follows pitch a little
    let neck = torso_base
        * Mat4::from_translation(vec3(0.0, 0.62, 0.0))
        * Mat4::from_rotation_x(p.pitch * 0.35);
    let head = neck * Mat4::from_translation(vec3(0.0, 0.2, 0.0));
    add_head(
        m,
        painted.as_deref_mut(),
        &head,
        &p.skin,
        p.team,
        p.blink,
        p.dead.is_some(),
        ow,
        lod,
    );

    // arms + gun pivot at the shoulders, pitched with the aim
    let aim_pitch = if p.sprint { -0.7 } else { p.pitch };
    let melee_push = (p.melee * std::f32::consts::PI).sin() * 0.35;
    // reload: gun tilts, the off hand pulls the magazine down and slaps it back in
    let (reload_k, mag_drop) = match p.reload {
        Some(t) if p.dead.is_none() => {
            let k = (t / 0.25).min(1.0);
            let cyc = (t * 2.6).sin().abs();
            (k, cyc * 0.22)
        }
        _ => (0.0, 0.0),
    };
    let aim_pitch = aim_pitch * (1.0 - reload_k) - 0.35 * reload_k;
    let shoulder =
        torso_base * Mat4::from_translation(vec3(0.0, 0.5, 0.0)) * Mat4::from_rotation_x(aim_pitch);
    let gun_base = shoulder
        * Mat4::from_translation(vec3(0.1, -0.08, -0.32 - melee_push))
        * Mat4::from_rotation_y(if p.sprint { 0.7 } else { 0.0 })
        * Mat4::from_rotation_z(0.5 * reload_k)
        * Mat4::from_scale(Vec3::splat(1.25));
    let mag_off = vec3(0.0, -mag_drop, 0.0);
    add_gun_ex(m, &gun_base, p.weapon, 0.016, mag_off);
    let right_hand = gun_base.transform_point3(vec3(0.0, -0.02, 0.0));
    let left_hand = if reload_k > 0.0 {
        let mag = gun_mag(p.weapon)
            .map(|g| g.0)
            .unwrap_or(vec3(0.0, 0.0, -0.3));
        gun_base
            .transform_point3(mag + mag_off + vec3(0.0, -0.06, 0.0))
            .lerp(
                gun_base.transform_point3(vec3(0.0, 0.03, -0.38)),
                1.0 - reload_k,
            )
    } else {
        gun_base.transform_point3(vec3(0.0, 0.03, -0.38))
    };
    let torso_right = torso_base.transform_vector3(Vec3::X);
    for (sx, hand) in [(0.29f32, right_hand), (-0.29, left_hand)] {
        let sh = torso_base.transform_point3(vec3(sx, 0.5, 0.0));
        let d = hand - sh;
        let end = hand - d.normalize_or_zero() * 0.06;
        let rect = tex::cell(if sx > 0.0 {
            tex::ARM_RIGHT
        } else {
            tex::ARM_LEFT
        });
        part_capsule(
            m,
            &mut painted,
            end,
            sh,
            0.08,
            pal.shirt,
            ow,
            cap_seg,
            torso_right,
            rect,
        );
        if pattern == 2 && !is_painted {
            m.capsule(
                sh.lerp(end, 0.45),
                sh.lerp(end, 0.55),
                0.086,
                mul(pal.shirt, 0.78),
                0.0,
                cap_seg,
            );
        }
        part_sphere(
            m,
            &mut painted,
            &Mat4::from_translation(hand),
            0.09,
            pal.tone,
            ow,
            tex::half(tex::HANDS_BOOTS, false),
        );
    }
    m.detail = prev_detail;
    m.ink = None;
    gun_base.transform_point3(muzzle(p.weapon))
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soldier_mesh_budget() {
        let mut rng = toon_shared::rng::Rng::new(5);
        let mut m = ToonMesh::new();
        for lod in [5u32, 3, 2, 5] {
            m.clear();
            let t0 = std::time::Instant::now();
            for _ in 0..16 {
                let pose = SoldierPose {
                    pos: Vec3::ZERO,
                    yaw: 0.3,
                    pitch: 0.1,
                    stance: Stance::Stand,
                    team: Team::Allies,
                    weapon: WeaponId::M4,
                    skin: Skin::random(&mut rng),
                    lod,
                    phase: 0.5,
                    walk: 1.0,
                    sprint: false,
                    dead: None,
                    has_bomb: false,
                    blink: false,
                    melee: 0.0,
                    reload: None,
                };
                add_soldier(&mut m, None, &pose);
            }
            let verts: usize = m
                .fill
                .chunks
                .iter()
                .chain(m.outline.chunks.iter())
                .map(|c| c.verts.len())
                .sum();
            println!(
                "lod {lod}: 16 soldiers = {verts} verts in {:?}",
                t0.elapsed()
            );
        }
    }
}
