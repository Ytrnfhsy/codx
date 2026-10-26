//! Immediate-mode mesh building with baked toon lighting, plus the fog/outline materials.
//!
//! Lighting is baked into vertex colours (three flat bands), so the GPU work per pixel is
//! trivial — this keeps the game fast on weak integrated graphics.

use macroquad::miniquad::{BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation, PipelineParams};
use macroquad::models::Vertex;
use macroquad::prelude::*;

/// Vertices per submitted chunk (macroquad batches up to its draw-call capacity).
const CHUNK_VERTS: usize = 8000;

pub const LIGHT_DIR: Vec3 = Vec3::new(0.42, 0.82, 0.38);

/// Toon shading: quantize the diffuse term into three bands.
pub fn shade(c: [u8; 3], n: Vec3) -> [u8; 4] {
    let l = LIGHT_DIR.normalize();
    let d = n.dot(l);
    let k = if d > 0.55 {
        1.0
    } else if d > 0.1 {
        0.84
    } else {
        0.68
    };
    [
        (c[0] as f32 * k).min(255.0) as u8,
        (c[1] as f32 * k).min(255.0) as u8,
        (c[2] as f32 * k).min(255.0) as u8,
        255,
    ]
}

#[derive(Default, Clone)]
pub struct Chunk {
    pub verts: Vec<Vertex>,
    pub idx: Vec<u16>,
}

/// A growing list of triangle chunks.
#[derive(Default, Clone)]
pub struct MeshBuilder {
    pub chunks: Vec<Chunk>,
}

const BOX_FACES: [(Vec3, [Vec3; 4]); 6] = [
    // normal, 4 corners (CCW seen from outside) in unit cube [-1,1]
    (Vec3::X, [Vec3::new(1.0, -1.0, 1.0), Vec3::new(1.0, -1.0, -1.0), Vec3::new(1.0, 1.0, -1.0), Vec3::new(1.0, 1.0, 1.0)]),
    (Vec3::NEG_X, [Vec3::new(-1.0, -1.0, -1.0), Vec3::new(-1.0, -1.0, 1.0), Vec3::new(-1.0, 1.0, 1.0), Vec3::new(-1.0, 1.0, -1.0)]),
    (Vec3::Y, [Vec3::new(-1.0, 1.0, 1.0), Vec3::new(1.0, 1.0, 1.0), Vec3::new(1.0, 1.0, -1.0), Vec3::new(-1.0, 1.0, -1.0)]),
    (Vec3::NEG_Y, [Vec3::new(-1.0, -1.0, -1.0), Vec3::new(1.0, -1.0, -1.0), Vec3::new(1.0, -1.0, 1.0), Vec3::new(-1.0, -1.0, 1.0)]),
    (Vec3::Z, [Vec3::new(-1.0, -1.0, 1.0), Vec3::new(1.0, -1.0, 1.0), Vec3::new(1.0, 1.0, 1.0), Vec3::new(-1.0, 1.0, 1.0)]),
    (Vec3::NEG_Z, [Vec3::new(1.0, -1.0, -1.0), Vec3::new(-1.0, -1.0, -1.0), Vec3::new(-1.0, 1.0, -1.0), Vec3::new(1.0, 1.0, -1.0)]),
];

impl MeshBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn clear(&mut self) {
        for c in &mut self.chunks {
            c.verts.clear();
            c.idx.clear();
        }
    }

    pub fn is_empty(&self) -> bool {
        self.chunks.iter().all(|c| c.verts.is_empty())
    }

    fn chunk_for(&mut self, n: usize) -> &mut Chunk {
        let need_new = match self.chunks.iter().position(|c| c.verts.len() + n <= CHUNK_VERTS) {
            Some(i) => return &mut self.chunks[i],
            None => true,
        };
        if need_new {
            self.chunks.push(Chunk::default());
        }
        self.chunks.last_mut().unwrap()
    }

    /// Box with transform `m` (applied to a cube with half extents `half`).
    pub fn add_box(&mut self, m: &Mat4, half: Vec3, color: [u8; 3]) {
        self.add_box_ex(m, half, color, 255, false);
    }

    /// `flat_color`: skip lighting (outlines, glowing things).
    pub fn add_box_ex(&mut self, m: &Mat4, half: Vec3, color: [u8; 3], alpha: u8, flat_color: bool) {
        let c = self.chunk_for(24);
        for (n, corners) in BOX_FACES.iter() {
            let wn = m.transform_vector3(*n).normalize_or_zero();
            let mut col = if flat_color { [color[0], color[1], color[2], 255] } else { shade(color, wn) };
            col[3] = alpha;
            let base = c.verts.len() as u16;
            for k in corners {
                let p = m.transform_point3(*k * half);
                c.verts.push(Vertex { position: p, uv: Vec2::ZERO, color: col, normal: Vec4::ZERO });
            }
            c.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    /// Axis-aligned box from min/max (static level geometry).
    pub fn add_aabb(&mut self, min: Vec3, max: Vec3, color: [u8; 3]) {
        let m = Mat4::from_translation((min + max) * 0.5);
        self.add_box(&m, (max - min) * 0.5, color);
    }

    /// Low-poly UV sphere (ellipsoid via `m`).
    pub fn add_sphere(&mut self, m: &Mat4, r: f32, color: [u8; 3], seg: u32, rings: u32) {
        self.add_sphere_ex(m, r, color, seg, rings, 255, false);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_sphere_ex(&mut self, m: &Mat4, r: f32, color: [u8; 3], seg: u32, rings: u32, alpha: u8, flat_color: bool) {
        let n_verts = ((seg + 1) * (rings + 1)) as usize;
        let c = self.chunk_for(n_verts);
        let base = c.verts.len() as u16;
        for j in 0..=rings {
            let v = j as f32 / rings as f32;
            let phi = v * std::f32::consts::PI;
            for i in 0..=seg {
                let u = i as f32 / seg as f32;
                let th = u * std::f32::consts::TAU;
                let n = Vec3::new(phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin());
                let p = m.transform_point3(n * r);
                let wn = m.transform_vector3(n).normalize_or_zero();
                let mut col = if flat_color { [color[0], color[1], color[2], 255] } else { shade(color, wn) };
                col[3] = alpha;
                c.verts.push(Vertex { position: p, uv: Vec2::ZERO, color: col, normal: Vec4::ZERO });
            }
        }
        for j in 0..rings {
            for i in 0..seg {
                let a = base + (j * (seg + 1) + i) as u16;
                let b = a + (seg + 1) as u16;
                // CCW from outside
                c.idx.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
            }
        }
    }

    /// Flat quad (decals, shadows, sprites). Corners in order.
    pub fn add_quad(&mut self, p: [Vec3; 4], color: [u8; 4]) {
        let c = self.chunk_for(4);
        let base = c.verts.len() as u16;
        for v in p {
            c.verts.push(Vertex { position: v, uv: Vec2::ZERO, color, normal: Vec4::ZERO });
        }
        c.idx.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Submits all chunks with the currently bound material.
    pub fn draw(&self) {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.texture(None);
        gl.draw_mode(DrawMode::Triangles);
        for c in &self.chunks {
            if !c.verts.is_empty() {
                gl.geometry(&c.verts, &c.idx);
            }
        }
    }
}

/// Both the coloured mesh and its black inverted-hull outline.
#[derive(Default, Clone)]
pub struct ToonMesh {
    pub fill: MeshBuilder,
    pub outline: MeshBuilder,
}

pub const OUTLINE: [u8; 3] = [24, 22, 30];

impl ToonMesh {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.fill.clear();
        self.outline.clear();
    }
    /// Box plus outline of width `w` (0 = no outline).
    pub fn boxed(&mut self, m: &Mat4, half: Vec3, color: [u8; 3], w: f32) {
        self.fill.add_box(m, half, color);
        if w > 0.0 {
            self.outline.add_box_ex(m, half + Vec3::splat(w), OUTLINE, 255, true);
        }
    }
    pub fn sphere(&mut self, m: &Mat4, r: f32, color: [u8; 3], w: f32) {
        self.fill.add_sphere(m, r, color, 10, 7);
        if w > 0.0 {
            self.outline.add_sphere_ex(m, r + w, OUTLINE, 10, 7, 255, true);
        }
    }
}

pub struct Materials {
    pub world: Material,
    pub outline: Material,
    /// Unlit, blended, no depth write (particles, glows).
    pub fx: Material,
}

const VS: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;
varying lowp vec4 color;
varying highp float fogd;
uniform mat4 Model;
uniform mat4 Projection;
uniform vec3 CamPos;
void main() {
    vec4 wp = Model * vec4(position, 1.0);
    gl_Position = Projection * wp;
    color = color0 / 255.0;
    fogd = length(wp.xyz - CamPos);
}
"#;

const FS: &str = r#"#version 100
precision mediump float;
varying lowp vec4 color;
varying highp float fogd;
uniform vec4 FogColor;
uniform vec2 FogRange;
void main() {
    float f = clamp((fogd - FogRange.x) / (FogRange.y - FogRange.x), 0.0, 1.0);
    gl_FragColor = vec4(mix(color.rgb, FogColor.rgb, f), color.a);
}
"#;

fn mat(cull: CullFace, depth_write: bool) -> Material {
    let blend = Some(BlendState::new(
        Equation::Add,
        BlendFactor::Value(BlendValue::SourceAlpha),
        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
    ));
    load_material(
        ShaderSource::Glsl { vertex: VS, fragment: FS },
        MaterialParams {
            pipeline_params: PipelineParams {
                cull_face: cull,
                depth_test: Comparison::LessOrEqual,
                depth_write,
                color_blend: blend,
                ..Default::default()
            },
            uniforms: vec![
                UniformDesc::new("CamPos", UniformType::Float3),
                UniformDesc::new("FogColor", UniformType::Float4),
                UniformDesc::new("FogRange", UniformType::Float2),
            ],
            textures: vec![],
        },
    )
    .expect("toon material")
}

impl Materials {
    pub fn new() -> Materials {
        Materials {
            world: mat(CullFace::Nothing, true),
            outline: mat(CullFace::Front, true),
            fx: mat(CullFace::Nothing, false),
        }
    }

    pub fn set_view(&self, cam: Vec3, fog: Color, near: f32, far: f32) {
        for m in [&self.world, &self.outline, &self.fx] {
            m.set_uniform("CamPos", cam);
            m.set_uniform("FogColor", vec4(fog.r, fog.g, fog.b, 1.0));
            m.set_uniform("FogRange", vec2(near, far));
        }
    }

    /// Draws a toon mesh: outline pass then fill.
    pub fn draw_toon(&self, m: &ToonMesh) {
        if !m.outline.is_empty() {
            gl_use_material(&self.outline);
            m.outline.draw();
        }
        gl_use_material(&self.world);
        m.fill.draw();
        gl_use_default_material();
    }
}

/// Model matrix helper: translation * yaw * pitch * roll.
pub fn trs(t: Vec3, yaw: f32, pitch: f32, roll: f32) -> Mat4 {
    Mat4::from_translation(t) * Mat4::from_rotation_y(yaw) * Mat4::from_rotation_x(pitch) * Mat4::from_rotation_z(roll)
}
