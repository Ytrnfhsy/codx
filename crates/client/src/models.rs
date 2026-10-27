//! Procedural toon models: soldiers, weapons, helicopter, jets, bomb, grenade.

use crate::mesh::{trs, ToonMesh};
use macroquad::prelude::*;
use toon_shared::physics::Stance;
use toon_shared::weapons::{WeaponClass, WeaponId};
use toon_shared::Team;

pub const SKIN: [u8; 3] = [255, 214, 170];
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

/// Big cartoon head with helmet, eyes and nose. `head` is the head centre transform.
pub fn add_head(m: &mut ToonMesh, head: &Mat4, team: Team, blink: bool, dead: bool, ow: f32) {
    let look = team_look(team);
    m.sphere(head, 0.3, SKIN, ow);
    let helmet = *head
        * Mat4::from_translation(vec3(0.0, 0.1, 0.02))
        * Mat4::from_scale(vec3(1.08, 0.78, 1.08));
    m.sphere(&helmet, 0.3, look.helmet, ow);
    m.boxed(
        &(*head * Mat4::from_translation(vec3(0.0, 0.07, -0.27))),
        vec3(0.3, 0.025, 0.07),
        look.helmet,
        0.0,
    );
    let eye_h = if blink && !dead { 0.012 } else { 0.075 };
    for x in [-0.11f32, 0.11] {
        let e = *head
            * Mat4::from_translation(vec3(x, -0.02, -0.25))
            * Mat4::from_scale(vec3(1.0, eye_h / 0.075, 1.0));
        m.fill.add_sphere(&e, 0.075, [255, 255, 255], 8, 5);
        if dead {
            // cartoon "X" eyes
            for r in [0.7f32, -0.7] {
                let xm = *head
                    * Mat4::from_translation(vec3(x, -0.02, -0.31))
                    * Mat4::from_rotation_z(r);
                m.fill.add_box(&xm, vec3(0.06, 0.012, 0.01), [20, 20, 30]);
            }
        } else if !blink {
            m.fill.add_sphere(
                &(e * Mat4::from_translation(vec3(0.0, 0.0, -0.05))),
                0.038,
                [20, 20, 30],
                6,
                4,
            );
        }
    }
    m.fill.add_sphere(
        &(*head * Mat4::from_translation(vec3(0.0, -0.1, -0.29))),
        0.05,
        [255, 180, 150],
        6,
        4,
    );
}

pub struct SoldierPose {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub stance: Stance,
    pub team: Team,
    pub weapon: WeaponId,
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

/// Builds a soldier with big cartoon proportions. Returns the world-space muzzle position.
pub fn add_soldier(m: &mut ToonMesh, p: &SoldierPose) -> Vec3 {
    let look = team_look(p.team);
    let ow = 0.025;
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

    // legs
    for (side, s) in [(-1.0f32, 1.0f32), (1.0, -1.0)] {
        let x = side * 0.13;
        if crouch > 0.0 && p.dead.is_none() {
            // thigh forward, shin down
            let thigh = root
                * Mat4::from_translation(vec3(x, hip, 0.0))
                * Mat4::from_rotation_x(1.25 + swing * 0.2 * s);
            m.boxed(
                &(thigh * Mat4::from_translation(vec3(0.0, -0.2, 0.0))),
                vec3(0.1, 0.22, 0.11),
                look.pants,
                ow,
            );
            let knee = root * Mat4::from_translation(vec3(x, hip - 0.05, -0.38));
            m.boxed(
                &(knee * Mat4::from_translation(vec3(0.0, -0.13, 0.0))),
                vec3(0.095, 0.17, 0.1),
                look.pants,
                ow,
            );
            m.boxed(
                &(knee * Mat4::from_translation(vec3(0.0, -0.29, -0.05))),
                vec3(0.11, 0.06, 0.15),
                BOOT,
                ow,
            );
        } else {
            let leg =
                root * Mat4::from_translation(vec3(x, hip, 0.0)) * Mat4::from_rotation_x(swing * s);
            m.boxed(
                &(leg * Mat4::from_translation(vec3(0.0, -0.36, 0.0))),
                vec3(0.1, 0.36, 0.11),
                look.pants,
                ow,
            );
            m.boxed(
                &(leg * Mat4::from_translation(vec3(0.0, -0.74, -0.05))),
                vec3(0.11, 0.06, 0.15),
                BOOT,
                ow,
            );
        }
    }

    // torso
    let torso_base = root
        * Mat4::from_translation(vec3(0.0, hip, 0.0))
        * Mat4::from_rotation_x(if p.sprint { -0.18 } else { 0.0 });
    m.boxed(
        &(torso_base * Mat4::from_translation(vec3(0.0, 0.29, 0.0))),
        vec3(0.27, 0.3, 0.17),
        look.shirt,
        ow,
    );
    m.boxed(
        &(torso_base * Mat4::from_translation(vec3(0.0, 0.3, 0.0))),
        vec3(0.285, 0.2, 0.185),
        look.vest,
        0.0,
    );
    m.boxed(
        &(torso_base * Mat4::from_translation(vec3(0.0, 0.03, 0.0))),
        vec3(0.28, 0.04, 0.18),
        BOOT,
        0.0,
    );
    if p.has_bomb {
        m.boxed(
            &(torso_base * Mat4::from_translation(vec3(0.0, 0.3, 0.26))),
            vec3(0.2, 0.2, 0.09),
            [220, 50, 40],
            ow,
        );
    }

    // head (big!) — follows pitch a little
    let neck = torso_base
        * Mat4::from_translation(vec3(0.0, 0.62, 0.0))
        * Mat4::from_rotation_x(p.pitch * 0.35);
    let head = neck * Mat4::from_translation(vec3(0.0, 0.2, 0.0));
    add_head(m, &head, p.team, p.blink, p.dead.is_some(), ow);

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
    // arms: boxes from shoulders to the hands
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
    for (sx, hand) in [(0.28f32, right_hand), (-0.28, left_hand)] {
        let sh = torso_base.transform_point3(vec3(sx, 0.5, 0.0));
        let mid = (sh + hand) * 0.5;
        let dir = hand - sh;
        let len = dir.length();
        let rot = Quat::from_rotation_arc(Vec3::NEG_Z, dir / len.max(1e-4));
        let am = Mat4::from_rotation_translation(rot, mid);
        m.boxed(&am, vec3(0.075, 0.075, len * 0.5), look.shirt, ow);
        m.sphere(&Mat4::from_translation(hand), 0.08, SKIN, ow);
    }
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
