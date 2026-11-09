//! Animated characters from the Quaternius "Ultimate Animated Character Pack" (CC0),
//! converted by `tools/convert_characters.py`. One skeleton is shared by every model;
//! clips are resampled at 30 fps. Skinning runs on the CPU and bakes the toon light into
//! vertex colours, like the rest of the game, so weak GPUs only draw coloured triangles.

use crate::mesh::{shade, ToonMesh};
use macroquad::models::Vertex;
use macroquad::prelude::*;
use std::sync::OnceLock;

const DATA: &[u8] = include_bytes!("../../../assets/characters/characters.bin");
const FPS: f32 = 30.0;

/// Model units to metres (the models are 3.27 units tall).
pub const SCALE: f32 = 0.566;

/// A bone transform (the pack never scales bones).
#[derive(Clone, Copy, Debug)]
pub struct Xf {
    pub t: Vec3,
    pub r: Quat,
}

impl Xf {
    pub const IDENTITY: Xf = Xf {
        t: Vec3::ZERO,
        r: Quat::IDENTITY,
    };
    pub fn mul(self, c: Xf) -> Xf {
        Xf {
            t: self.t + self.r * c.t,
            r: (self.r * c.r).normalize(),
        }
    }
    pub fn mat(self) -> Mat4 {
        Mat4::from_rotation_translation(self.r, self.t)
    }
}

pub struct Bone {
    pub name: String,
    pub parent: Option<usize>,
    pub inv_bind: Mat4,
}

pub struct Clip {
    pub dur: f32,
    frames: usize,
    keys: Vec<Xf>,
}

/// A spot on the body: skinned once per frame, shared by the vertices of every material
/// and normal that meet there.
struct Spot {
    p: Vec3,
    /// Normal averaged over the spot (crack-free outline hull).
    on: Vec3,
    joints: [u8; 4],
    weights: [f32; 4],
}

struct Vert {
    spot: u16,
    n: Vec3,
    mat: u8,
}

pub struct Model {
    pub name: String,
    pub mats: Vec<(String, [u8; 3])>,
    spots: Vec<Spot>,
    verts: Vec<Vert>,
    idx: Vec<u16>,
    /// Triangles over spots (the outline hull).
    hull_idx: Vec<u16>,
}

impl Model {
    pub fn mat(&self, name: &str) -> Option<usize> {
        self.mats.iter().position(|m| m.0 == name)
    }
}

/// Indices of the bones the game moves by hand. `[left, right]` pairs.
pub struct Ids {
    pub body: usize,
    pub hips: usize,
    pub abdomen: usize,
    pub torso: usize,
    pub neck: usize,
    pub upper_arm: [usize; 2],
    pub lower_arm: [usize; 2],
    pub fist: [usize; 2],
    pub upper_leg: [usize; 2],
    pub lower_leg: [usize; 2],
    pub foot: [usize; 2],
}

pub struct Rig {
    pub bones: Vec<Bone>,
    pub clips: Vec<Clip>,
    pub models: Vec<Model>,
    pub ids: Ids,
}

struct Reader<'a> {
    d: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn bytes(&mut self, n: usize) -> &[u8] {
        let b = &self.d[self.at..self.at + n];
        self.at += n;
        b
    }
    fn u8(&mut self) -> u8 {
        self.bytes(1)[0]
    }
    fn i8(&mut self) -> f32 {
        self.bytes(1)[0] as i8 as f32 / 127.0
    }
    fn u16(&mut self) -> u16 {
        let b = self.bytes(2);
        u16::from_le_bytes([b[0], b[1]])
    }
    fn u32(&mut self) -> u32 {
        let b = self.bytes(4);
        u32::from_le_bytes([b[0], b[1], b[2], b[3]])
    }
    fn f32(&mut self) -> f32 {
        f32::from_bits(self.u32())
    }
    fn vec3(&mut self) -> Vec3 {
        vec3(self.f32(), self.f32(), self.f32())
    }
    fn quat(&mut self) -> Quat {
        Quat::from_xyzw(self.f32(), self.f32(), self.f32(), self.f32()).normalize()
    }
    fn str(&mut self) -> String {
        let n = self.u8() as usize;
        String::from_utf8_lossy(self.bytes(n)).into_owned()
    }
}

fn parse(d: &[u8]) -> Rig {
    let mut r = Reader { d, at: 0 };
    assert_eq!(r.bytes(4), b"TWC1", "characters.bin: bad header");
    let nb = r.u8() as usize;
    let mut bones = Vec::with_capacity(nb);
    for i in 0..nb {
        let name = r.str();
        let parent = match r.u8() {
            255 => None,
            p => Some(p as usize),
        };
        assert!(
            parent.is_none_or(|p| p < i),
            "bones must come after their parents"
        );
        let mut m = [0.0f32; 16];
        for v in &mut m {
            *v = r.f32();
        }
        // rest pose (translation, rotation, scale): the clips key every bone
        r.bytes(40);
        bones.push(Bone {
            name,
            parent,
            inv_bind: Mat4::from_cols_array(&m),
        });
    }
    let nc = r.u8() as usize;
    let mut clips = Vec::with_capacity(nc);
    for _ in 0..nc {
        let name = r.str();
        let dur = r.f32();
        let frames = r.u16() as usize;
        let mut keys = Vec::with_capacity(frames * nb);
        for _ in 0..frames * nb {
            let t = r.vec3();
            let q = r.quat();
            keys.push(Xf { t, r: q });
        }
        assert_eq!(
            clip::NAMES.get(clips.len()),
            Some(&name.as_str()),
            "characters.bin: clips"
        );
        clips.push(Clip { dur, frames, keys });
    }
    let nm = r.u8() as usize;
    let mut models = Vec::with_capacity(nm);
    for _ in 0..nm {
        let name = r.str();
        let nmat = r.u8() as usize;
        let mats = (0..nmat)
            .map(|_| {
                let n = r.str();
                let c = r.bytes(3);
                (n, [c[0], c[1], c[2]])
            })
            .collect();
        let ns = r.u16() as usize;
        let mut spots = Vec::with_capacity(ns);
        for _ in 0..ns {
            let p = r.vec3();
            let on = vec3(r.i8(), r.i8(), r.i8()).normalize_or_zero();
            let j = r.bytes(4);
            let joints = [j[0], j[1], j[2], j[3]];
            let w = r.bytes(4);
            let weights = [
                w[0] as f32 / 255.0,
                w[1] as f32 / 255.0,
                w[2] as f32 / 255.0,
                w[3] as f32 / 255.0,
            ];
            spots.push(Spot {
                p,
                on,
                joints,
                weights,
            });
        }
        let nv = r.u16() as usize;
        let mut verts = Vec::with_capacity(nv);
        for _ in 0..nv {
            let spot = r.u16();
            let n = vec3(r.i8(), r.i8(), r.i8()).normalize_or_zero();
            let mat = r.u8();
            verts.push(Vert { spot, n, mat });
        }
        let ni = r.u32() as usize;
        let idx: Vec<u16> = (0..ni).map(|_| r.u16()).collect();
        let hull_idx = idx.iter().map(|&i| verts[i as usize].spot).collect();
        models.push(Model {
            name,
            mats,
            spots,
            verts,
            idx,
            hull_idx,
        });
    }
    let find = |n: &str| {
        bones
            .iter()
            .position(|b| b.name == n)
            .unwrap_or_else(|| panic!("characters.bin: no bone {n}"))
    };
    let pair = |n: &str| [find(&format!("{n}.L")), find(&format!("{n}.R"))];
    let ids = Ids {
        body: find("Body"),
        hips: find("Hips"),
        abdomen: find("Abdomen"),
        torso: find("Torso"),
        neck: find("Neck"),
        upper_arm: pair("UpperArm"),
        lower_arm: pair("LowerArm"),
        fist: pair("Fist"),
        upper_leg: pair("UpperLeg"),
        lower_leg: pair("LowerLeg"),
        foot: pair("Foot"),
    };
    Rig {
        bones,
        clips,
        models,
        ids,
    }
}

pub fn rig() -> &'static Rig {
    static RIG: OnceLock<Rig> = OnceLock::new();
    RIG.get_or_init(|| parse(DATA))
}

/// Clip numbers (same order as `CLIPS` in the converter).
pub mod clip {
    pub const IDLE: usize = 0;
    pub const WALK: usize = 1;
    pub const RUN: usize = 2;
    pub const NAMES: [&str; 3] = ["Idle", "Walk", "Run"];
}

impl Rig {
    /// Local bone transforms of `clip` at time `t` (seconds).
    pub fn sample(&self, clip: usize, t: f32, looped: bool) -> Vec<Xf> {
        let c = &self.clips[clip];
        let t = if looped {
            t.rem_euclid(c.dur.max(1e-3))
        } else {
            t.clamp(0.0, c.dur)
        };
        let f = t * FPS;
        let i0 = (f.floor() as usize).min(c.frames - 1);
        let i1 = (i0 + 1).min(c.frames - 1);
        let k = f - i0 as f32;
        let nb = self.bones.len();
        (0..nb)
            .map(|b| {
                let a = c.keys[i0 * nb + b];
                let z = c.keys[i1 * nb + b];
                Xf {
                    t: a.t.lerp(z.t, k),
                    r: a.r.slerp(z.r, k),
                }
            })
            .collect()
    }

    /// Model-space bone transforms.
    pub fn globals(&self, local: &[Xf]) -> Vec<Xf> {
        let mut g: Vec<Xf> = Vec::with_capacity(local.len());
        for (i, b) in self.bones.iter().enumerate() {
            let x = match b.parent {
                Some(p) => g[p].mul(local[i]),
                None => local[i],
            };
            g.push(x);
        }
        g
    }

    fn parent_rot(&self, g: &[Xf], bone: usize) -> Quat {
        self.bones[bone].parent.map_or(Quat::IDENTITY, |p| g[p].r)
    }

    /// Rotates a bone (and everything below it) by `q`, given in model space.
    pub fn turn(&self, local: &mut [Xf], g: &[Xf], bone: usize, q: Quat) {
        let pr = self.parent_rot(g, bone);
        local[bone].r = (pr.inverse() * q * g[bone].r).normalize();
    }

    /// Moves a bone (and everything below it) to a model-space transform.
    pub fn set_global(&self, local: &mut [Xf], g: &mut Vec<Xf>, bone: usize, x: Xf) {
        let parent = self.bones[bone].parent.map_or(Xf::IDENTITY, |p| g[p]);
        let inv = parent.r.inverse();
        local[bone] = Xf {
            t: inv * (x.t - parent.t),
            r: (inv * x.r).normalize(),
        };
        *g = self.globals(local);
    }

    /// Points a bone (local +Y runs along it) at `target` (model space) with the
    /// smallest turn, keeping its twist.
    pub fn aim(&self, local: &mut [Xf], g: &mut Vec<Xf>, bone: usize, target: Vec3) {
        let cur = g[bone].r * Vec3::Y;
        let dir = (target - g[bone].t).normalize_or_zero();
        if dir == Vec3::ZERO {
            return;
        }
        let q = Quat::from_rotation_arc(cur, dir);
        self.turn(local, g, bone, q);
        *g = self.globals(local);
    }

    /// Two-bone IK: bends `upper`/`lower` so the end of `lower` (the head of its child,
    /// `lower_len` away) reaches `target`; the middle joint bends towards `pole`.
    #[allow(clippy::too_many_arguments)]
    pub fn reach(
        &self,
        local: &mut [Xf],
        g: &mut Vec<Xf>,
        upper: usize,
        lower: usize,
        lower_len: f32,
        target: Vec3,
        pole: Vec3,
    ) {
        let a = local[lower].t.length();
        let b = lower_len;
        let s = g[upper].t;
        let to = target - s;
        let d = to.length().clamp(0.05, (a + b) * 0.999);
        let dir = to.normalize_or_zero();
        if dir == Vec3::ZERO {
            return;
        }
        let cos_a = ((a * a + d * d - b * b) / (2.0 * a * d)).clamp(-1.0, 1.0);
        let side = (pole - dir * pole.dot(dir)).normalize_or_zero();
        let mid = s + dir * (cos_a * a) + side * (1.0 - cos_a * cos_a).sqrt() * a;
        let end = s + dir * d;
        self.aim(local, g, upper, mid);
        self.aim(local, g, lower, end);
    }

    /// Adds a skinned model to `m`. `world` maps model space to the world (scale
    /// included); `colors` gives one colour per material; `outline` is the silhouette
    /// width in metres (0 = none), drawn in `m.ink` or black.
    pub fn draw(
        &self,
        m: &mut ToonMesh,
        model: usize,
        g: &[Xf],
        world: Mat4,
        colors: &[[u8; 3]],
        outline: f32,
    ) {
        let md = &self.models[model];
        let skin: Vec<Mat4> = g
            .iter()
            .zip(&self.bones)
            .map(|(x, b)| world * x.mat() * b.inv_bind)
            .collect();
        // every spot is skinned once; its blended matrix also turns the normals there
        let spots: Vec<(Mat4, Vec3)> = md
            .spots
            .iter()
            .map(|v| {
                let mut mm = skin[v.joints[0] as usize] * v.weights[0];
                for k in 1..4 {
                    if v.weights[k] > 0.0 {
                        mm += skin[v.joints[k] as usize] * v.weights[k];
                    }
                }
                let p = mm.transform_point3(v.p);
                (mm, p)
            })
            .collect();
        let fill: Vec<Vertex> = md
            .verts
            .iter()
            .map(|v| {
                let (mm, p) = &spots[v.spot as usize];
                let n = mm.transform_vector3(v.n).normalize_or_zero();
                let c = colors.get(v.mat as usize).copied().unwrap_or([255, 0, 255]);
                Vertex {
                    position: *p,
                    uv: Vec2::ZERO,
                    color: shade(c, n),
                    normal: n.extend(0.0),
                }
            })
            .collect();
        m.fill.add_indexed(&fill, &md.idx);
        if outline > 0.0 {
            let ink = m.ink.unwrap_or(crate::mesh::OUTLINE);
            let ink = [ink[0], ink[1], ink[2], 255];
            let hull: Vec<Vertex> = md
                .spots
                .iter()
                .zip(&spots)
                .map(|(v, (mm, p))| Vertex {
                    position: *p + mm.transform_vector3(v.on).normalize_or_zero() * outline,
                    uv: Vec2::ZERO,
                    color: ink,
                    normal: Vec4::ZERO,
                })
                .collect();
            m.outline.add_indexed(&hull, &md.hull_idx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn characters_load() {
        let r = rig();
        assert_eq!(r.clips.len(), clip::NAMES.len());
        // the two soldiers and every battle royale character
        assert_eq!(r.models.len(), 2 + toon_shared::skins::count() as usize - 1);
        for k in 1..toon_shared::skins::count() {
            let name = toon_shared::skins::model(k).unwrap();
            assert!(r.models.iter().any(|m| m.name == name), "{name} missing");
        }
        // clip bone lengths agree with the bind pose the meshes were skinned in
        let local = r.sample(clip::IDLE, 0.0, true);
        for i in [
            r.ids.lower_arm[0],
            r.ids.fist[1],
            r.ids.lower_leg[0],
            r.ids.neck,
        ] {
            let p = r.bones[i].parent.unwrap();
            let bind = |b: usize| r.bones[b].inv_bind.inverse().w_axis.truncate();
            let len = bind(i).distance(bind(p));
            assert!(
                (len - local[i].t.length()).abs() < 1e-3,
                "{}",
                r.bones[i].name
            );
        }
        for md in &r.models {
            assert!(md.idx.iter().all(|i| (*i as usize) < md.verts.len()));
            assert!(md.verts.iter().all(|v| (v.spot as usize) < md.spots.len()));
            assert!(md.mat("Skin").is_some());
        }
    }

    #[test]
    fn reach_puts_the_hand_on_the_target() {
        let r = rig();
        let ids = &r.ids;
        let mut local = r.sample(clip::IDLE, 0.3, true);
        let mut g = r.globals(&local);
        let s = g[ids.upper_arm[1]].t;
        let target = s + vec3(0.1, -0.4, 0.6);
        let len = local[ids.fist[1]].t.length();
        r.reach(
            &mut local,
            &mut g,
            ids.upper_arm[1],
            ids.lower_arm[1],
            len,
            target,
            Vec3::NEG_Y,
        );
        assert!(g[ids.fist[1]].t.distance(target) < 0.01);
    }
}
