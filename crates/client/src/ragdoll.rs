//! Cartoon ragdolls: a dead soldier becomes a handful of Verlet particles held together by
//! sticks, tumbles from the hit that killed him and flops against the level geometry.

use crate::mesh::{trs, ToonMesh};
use crate::models::{self, team_look, BOOT, SKIN};
use macroquad::prelude::*;
use toon_shared::physics::{Stance, World};
use toon_shared::rng::Rng;
use toon_shared::weapons::{Cause, WeaponId};
use toon_shared::Team;

const HEAD: usize = 0;
const CHEST: usize = 1;
const PELVIS: usize = 2;
const LSH: usize = 3;
const RSH: usize = 4;
const LHIP: usize = 5;
const RHIP: usize = 6;
const LKNEE: usize = 7;
const RKNEE: usize = 8;
const LFOOT: usize = 9;
const RFOOT: usize = 10;
const LHAND: usize = 11;
const RHAND: usize = 12;
const GRIP: usize = 13;
const MUZZLE: usize = 14;
const N: usize = 15;

const STEP: f32 = 1.0 / 60.0;
const GRAVITY: f32 = 20.0;
/// How long a body stays before it is removed.
pub const LIFETIME: f32 = 8.0;

/// Local joint positions of a standing soldier (forward is -Z, like the model).
fn standing() -> [Vec3; N] {
    [
        vec3(0.0, 1.62, 0.0),
        vec3(0.0, 1.3, 0.0),
        vec3(0.0, 0.85, 0.0),
        vec3(-0.28, 1.3, 0.0),
        vec3(0.28, 1.3, 0.0),
        vec3(-0.13, 0.8, 0.0),
        vec3(0.13, 0.8, 0.0),
        vec3(-0.13, 0.42, 0.0),
        vec3(0.13, 0.42, 0.0),
        vec3(-0.13, 0.06, -0.05),
        vec3(0.13, 0.06, -0.05),
        vec3(0.1, 1.2, -0.85),
        vec3(0.12, 1.18, -0.4),
        vec3(0.12, 1.18, -0.4),
        vec3(0.12, 1.25, -1.35),
    ]
}

/// How much of the killing blow each joint takes (upper body takes most, so he topples).
const PUSH: [f32; N] = [
    1.35, 1.0, 0.55, 1.0, 1.0, 0.45, 0.45, 0.25, 0.25, 0.12, 0.12, 1.1, 1.1, 1.2, 1.2,
];

#[derive(Clone, Copy)]
struct Stick {
    a: usize,
    b: usize,
    len: f32,
    /// Only keeps the joints apart (lets knees and hips fold).
    min_only: bool,
}

#[derive(Clone, Copy)]
pub struct DeathBlow {
    pub cause: Cause,
    pub headshot: bool,
    /// Where the damage came from (killer or explosion), if known.
    pub from: Option<Vec3>,
}

pub struct Ragdoll {
    p: [Vec3; N],
    prev: [Vec3; N],
    sticks: Vec<Stick>,
    pub team: Team,
    pub weapon: WeaponId,
    pub age: f32,
    acc: f32,
    still: f32,
    /// Explosion victims get a few circling cartoon stars.
    dizzy: bool,
}

impl Ragdoll {
    pub fn new(
        pos: Vec3,
        yaw: f32,
        stance: Stance,
        vel: Vec3,
        team: Team,
        weapon: WeaponId,
        blow: DeathBlow,
        rng: &mut Rng,
    ) -> Ragdoll {
        let mut local = standing();
        let rest = local;
        let root = match stance {
            Stance::Prone => {
                trs(pos, yaw, 0.0, 0.0)
                    * Mat4::from_translation(vec3(0.0, 0.22, 0.78))
                    * Mat4::from_rotation_x(-1.5)
            }
            _ => trs(pos, yaw, 0.0, 0.0),
        };
        if stance == Stance::Crouch {
            for (i, l) in local.iter_mut().enumerate() {
                if !matches!(i, LKNEE | RKNEE | LFOOT | RFOOT) {
                    l.y -= 0.5;
                }
            }
            local[LKNEE] = vec3(-0.13, 0.32, -0.38);
            local[RKNEE] = vec3(0.13, 0.32, -0.38);
        }
        let p: [Vec3; N] = std::array::from_fn(|i| root.transform_point3(local[i]));

        // the killing blow
        let centre = p[CHEST];
        let (dir, strength, lift) = match blow.cause {
            Cause::Grenade | Cause::Airstrike | Cause::Bomb | Cause::Helicopter => {
                let d = blow
                    .from
                    .map(|f| (centre - f).normalize_or_zero())
                    .unwrap_or(Vec3::ZERO);
                (vec3(d.x, 0.0, d.z).normalize_or_zero(), 7.5, 6.5)
            }
            Cause::Weapon(w) => {
                let d = blow
                    .from
                    .map(|f| (centre - f).normalize_or_zero())
                    .unwrap_or(-root.z_axis.truncate());
                let k = if matches!(
                    w.def().class,
                    toon_shared::weapons::WeaponClass::Shotgun
                        | toon_shared::weapons::WeaponClass::Sniper
                ) {
                    4.2
                } else {
                    2.6
                };
                (d, k, 0.8)
            }
            Cause::Knife => {
                let d = blow
                    .from
                    .map(|f| (centre - f).normalize_or_zero())
                    .unwrap_or(Vec3::ZERO);
                (d, 3.0, 1.0)
            }
            _ => (Vec3::ZERO, 0.0, 0.0),
        };
        let explosive = lift > 3.0;
        let mut prev = p;
        for i in 0..N {
            let jitter = vec3(
                rng.range(-0.4, 0.4),
                rng.range(-0.2, 0.4),
                rng.range(-0.4, 0.4),
            );
            let mut v = vel + jitter;
            if explosive {
                v +=
                    dir * strength * (0.8 + PUSH[i] * 0.3) + Vec3::Y * lift * (0.7 + PUSH[i] * 0.3);
            } else {
                v += dir * strength * PUSH[i] + Vec3::Y * lift * PUSH[i];
            }
            if blow.headshot && i == HEAD {
                v += dir * 3.0 + Vec3::Y * 1.5;
            }
            prev[i] = p[i] - v * STEP;
        }
        // the gun gets flung a little on its own
        let toss = vec3(
            rng.range(-1.5, 1.5),
            rng.range(1.0, 2.5),
            rng.range(-1.5, 1.5),
        );
        prev[GRIP] -= toss * STEP;
        prev[MUZZLE] -= toss * STEP * 1.4;

        let mut sticks = Vec::new();
        let mut link = |a: usize, b: usize, min_only: bool, k: f32| {
            sticks.push(Stick {
                a,
                b,
                len: rest[a].distance(rest[b]) * k,
                min_only,
            })
        };
        // torso box (cross-braced so it stays rigid)
        for (a, b) in [
            (CHEST, PELVIS),
            (LSH, RSH),
            (LSH, CHEST),
            (RSH, CHEST),
            (LSH, PELVIS),
            (RSH, PELVIS),
            (LHIP, RHIP),
            (LHIP, PELVIS),
            (RHIP, PELVIS),
            (LHIP, CHEST),
            (RHIP, CHEST),
            (LSH, LHIP),
            (RSH, RHIP),
            (LSH, RHIP),
            (RSH, LHIP),
        ] {
            link(a, b, false, 1.0);
        }
        // neck
        for (a, b) in [(HEAD, CHEST), (HEAD, LSH), (HEAD, RSH)] {
            link(a, b, false, 1.0);
        }
        link(HEAD, PELVIS, true, 0.9);
        // limbs
        for (a, b) in [
            (LHIP, LKNEE),
            (LKNEE, LFOOT),
            (RHIP, RKNEE),
            (RKNEE, RFOOT),
            (LSH, LHAND),
            (RSH, RHAND),
            (GRIP, MUZZLE),
        ] {
            link(a, b, false, 1.0);
        }
        // knees bend but don't fold flat; feet stay away from the chest
        link(LHIP, LFOOT, true, 0.55);
        link(RHIP, RFOOT, true, 0.55);
        link(LFOOT, CHEST, true, 0.6);
        link(RFOOT, CHEST, true, 0.6);
        link(LHAND, RHAND, true, 0.25);

        Ragdoll {
            p,
            prev,
            sticks,
            team,
            weapon,
            age: 0.0,
            acc: 0.0,
            still: 0.0,
            dizzy: explosive,
        }
    }

    fn radius(i: usize) -> f32 {
        match i {
            HEAD => 0.27,
            CHEST | PELVIS => 0.16,
            LFOOT | RFOOT | GRIP | MUZZLE => 0.06,
            _ => 0.09,
        }
    }

    pub fn update(&mut self, world: &World, dt: f32) {
        self.age += dt;
        if self.still > 0.6 {
            return;
        }
        self.acc = (self.acc + dt).min(STEP * 4.0);
        while self.acc >= STEP {
            self.acc -= STEP;
            self.step(world);
        }
    }

    fn step(&mut self, world: &World) {
        let mut max_v: f32 = 0.0;
        for i in 0..N {
            let v = (self.p[i] - self.prev[i]) * 0.995;
            max_v = max_v.max(v.length());
            self.prev[i] = self.p[i];
            self.p[i] += v + Vec3::NEG_Y * GRAVITY * STEP * STEP;
        }
        for _ in 0..5 {
            for s in &self.sticks {
                let d = self.p[s.b] - self.p[s.a];
                let len = d.length().max(1e-5);
                if s.min_only && len >= s.len {
                    continue;
                }
                let corr = d * ((len - s.len) / len * 0.5);
                self.p[s.a] += corr;
                self.p[s.b] -= corr;
            }
            for i in 0..N {
                let r = Self::radius(i);
                if let Some((c, n)) = world.push_out_sphere(self.p[i], r) {
                    self.p[i] = c;
                    // friction and a little bounce
                    let v = self.p[i] - self.prev[i];
                    let vn = n * v.dot(n);
                    if v.dot(n) < 0.0 {
                        let nv = (v - vn) * 0.82 - vn * 0.15;
                        self.prev[i] = self.p[i] - nv;
                    }
                }
                if self.p[i].y < -2.0 {
                    self.p[i].y = -2.0;
                    self.prev[i].y = -2.0;
                }
            }
        }
        // settle: stop simulating once the body has come to rest
        if max_v < 0.004 && self.age > 0.8 {
            self.still += STEP;
        } else {
            self.still = 0.0;
        }
    }

    /// Centre of the body (for the death camera).
    pub fn centre(&self) -> Vec3 {
        self.p[CHEST]
    }

    pub fn draw(&self, m: &mut ToonMesh, outlines: bool, now: f32) {
        let look = team_look(self.team);
        let ow = if outlines { 0.025 } else { 0.0 };
        let sink = ((self.age - (LIFETIME - 1.5)) / 1.5).clamp(0.0, 1.0) * 0.7;
        let p: [Vec3; N] = std::array::from_fn(|i| self.p[i] - vec3(0.0, sink, 0.0));

        // torso frame
        let up = (p[CHEST] - p[PELVIS]).normalize_or(Vec3::Y);
        let r0 = p[RSH] - p[LSH];
        let right = (r0 - up * r0.dot(up)).normalize_or(Vec3::X);
        let back = right.cross(up);
        let torso = |c: Vec3| {
            Mat4::from_cols(
                right.extend(0.0),
                up.extend(0.0),
                back.extend(0.0),
                c.extend(1.0),
            )
        };
        let tc = p[PELVIS].lerp(p[CHEST], 0.53);
        m.boxed(&torso(tc), vec3(0.27, 0.3, 0.17), look.shirt, ow);
        m.boxed(
            &torso(tc + up * 0.01),
            vec3(0.285, 0.2, 0.185),
            look.vest,
            0.0,
        );
        m.boxed(
            &torso(p[PELVIS] - up * 0.03),
            vec3(0.28, 0.04, 0.18),
            BOOT,
            0.0,
        );

        // head
        let hu = (p[HEAD] - p[CHEST]).normalize_or(up);
        let hr = (right - hu * right.dot(hu)).normalize_or(Vec3::X);
        let hb = hr.cross(hu);
        let head = Mat4::from_cols(
            hr.extend(0.0),
            hu.extend(0.0),
            hb.extend(0.0),
            p[HEAD].extend(1.0),
        );
        models::add_head(m, &head, self.team, false, true, ow);
        if self.dizzy && self.age < 3.5 {
            for k in 0..3 {
                let a = now * 5.0 + k as f32 * std::f32::consts::TAU / 3.0;
                let sp = p[HEAD] + hu * 0.38 + (hr * a.cos() + hb * a.sin()) * 0.32;
                m.fill
                    .add_sphere(&Mat4::from_translation(sp), 0.05, [255, 230, 60], 5, 3);
            }
        }

        // limbs
        let seg = |m: &mut ToonMesh, a: Vec3, b: Vec3, half: f32, col: [u8; 3]| {
            let d = b - a;
            let len = d.length();
            let dir = if len > 1e-3 { d / len } else { Vec3::NEG_Y };
            let rot = Quat::from_rotation_arc(Vec3::NEG_Y, dir);
            m.boxed(
                &Mat4::from_rotation_translation(rot, (a + b) * 0.5),
                vec3(half, len * 0.5, half * 1.1),
                col,
                ow,
            );
            rot
        };
        for (hip, knee, foot) in [(LHIP, LKNEE, LFOOT), (RHIP, RKNEE, RFOOT)] {
            seg(m, p[hip], p[knee], 0.1, look.pants);
            let rot = seg(m, p[knee], p[foot], 0.095, look.pants);
            m.boxed(
                &(Mat4::from_rotation_translation(rot, p[foot])
                    * Mat4::from_translation(vec3(0.0, 0.0, -0.05))),
                vec3(0.11, 0.06, 0.15),
                BOOT,
                ow,
            );
        }
        for (sh, hand) in [(LSH, LHAND), (RSH, RHAND)] {
            seg(m, p[sh], p[hand], 0.075, look.shirt);
            m.sphere(&Mat4::from_translation(p[hand]), 0.08, SKIN, ow);
        }

        // the dropped gun
        let gd = (p[MUZZLE] - p[GRIP]).normalize_or(Vec3::NEG_Z);
        let rot = Quat::from_rotation_arc(Vec3::NEG_Z, gd);
        let base =
            Mat4::from_rotation_translation(rot, p[GRIP]) * Mat4::from_scale(Vec3::splat(1.25));
        models::add_gun(m, &base, self.weapon, if outlines { 0.016 } else { 0.0 });
    }
}
