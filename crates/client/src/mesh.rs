//! Immediate-mode mesh building with baked toon lighting, plus the fog/outline materials.
//!
//! Lighting is baked into vertex colours (three flat bands), so the GPU work per pixel is
//! trivial — this keeps the game fast on weak integrated graphics.

use macroquad::miniquad::{
    BlendFactor, BlendState, BlendValue, Comparison, CullFace, Equation, PipelineParams,
};
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

/// One lit vertex (`flat_color`: unlit and unshadowed, for outlines and glows).
fn vert(p: Vec3, wn: Vec3, color: [u8; 3], alpha: u8, flat_color: bool) -> Vertex {
    let mut col = if flat_color {
        [color[0], color[1], color[2], 255]
    } else {
        shade(color, wn)
    };
    col[3] = alpha;
    Vertex {
        position: p,
        uv: Vec2::ZERO,
        color: col,
        normal: if flat_color {
            Vec4::ZERO
        } else {
            wn.extend(0.0)
        },
    }
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
    (
        Vec3::X,
        [
            Vec3::new(1.0, -1.0, 1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(1.0, 1.0, -1.0),
            Vec3::new(1.0, 1.0, 1.0),
        ],
    ),
    (
        Vec3::NEG_X,
        [
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(-1.0, -1.0, 1.0),
            Vec3::new(-1.0, 1.0, 1.0),
            Vec3::new(-1.0, 1.0, -1.0),
        ],
    ),
    (
        Vec3::Y,
        [
            Vec3::new(-1.0, 1.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(1.0, 1.0, -1.0),
            Vec3::new(-1.0, 1.0, -1.0),
        ],
    ),
    (
        Vec3::NEG_Y,
        [
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(1.0, -1.0, 1.0),
            Vec3::new(-1.0, -1.0, 1.0),
        ],
    ),
    (
        Vec3::Z,
        [
            Vec3::new(-1.0, -1.0, 1.0),
            Vec3::new(1.0, -1.0, 1.0),
            Vec3::new(1.0, 1.0, 1.0),
            Vec3::new(-1.0, 1.0, 1.0),
        ],
    ),
    (
        Vec3::NEG_Z,
        [
            Vec3::new(1.0, -1.0, -1.0),
            Vec3::new(-1.0, -1.0, -1.0),
            Vec3::new(-1.0, 1.0, -1.0),
            Vec3::new(1.0, 1.0, -1.0),
        ],
    ),
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
        let need_new = match self
            .chunks
            .iter()
            .position(|c| c.verts.len() + n <= CHUNK_VERTS)
        {
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
    pub fn add_box_ex(
        &mut self,
        m: &Mat4,
        half: Vec3,
        color: [u8; 3],
        alpha: u8,
        flat_color: bool,
    ) {
        self.add_box_full(m, half, color, alpha, flat_color, (0, 1.0));
    }

    /// Box with a detail texture `(tile, metres per repeat)` (see `textures.rs`).
    pub fn add_box_full(
        &mut self,
        m: &Mat4,
        half: Vec3,
        color: [u8; 3],
        alpha: u8,
        flat_color: bool,
        detail: (u8, f32),
    ) {
        let uv = vec2(detail.0 as f32, detail.1);
        let c = self.chunk_for(24);
        for (n, corners) in BOX_FACES.iter() {
            let wn = m.transform_vector3(*n).normalize_or_zero();
            let mut col = if flat_color {
                [color[0], color[1], color[2], 255]
            } else {
                shade(color, wn)
            };
            col[3] = alpha;
            let base = c.verts.len() as u16;
            // flat-coloured things (outlines, glows) never receive sun shadows
            let normal = if flat_color {
                Vec4::ZERO
            } else {
                wn.extend(0.0)
            };
            for k in corners {
                let p = m.transform_point3(*k * half);
                c.verts.push(Vertex {
                    position: p,
                    uv,
                    color: col,
                    normal,
                });
            }
            c.idx
                .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
        }
    }

    /// Axis-aligned box from min/max (static level geometry).
    pub fn add_aabb(&mut self, min: Vec3, max: Vec3, color: [u8; 3], detail: (u8, f32)) {
        let m = Mat4::from_translation((min + max) * 0.5);
        self.add_box_full(&m, (max - min) * 0.5, color, 255, false, detail);
    }

    /// Low-poly UV sphere (ellipsoid via `m`).
    pub fn add_sphere(&mut self, m: &Mat4, r: f32, color: [u8; 3], seg: u32, rings: u32) {
        self.add_sphere_ex(m, r, color, seg, rings, 255, false);
    }

    #[allow(clippy::too_many_arguments)]
    pub fn add_sphere_ex(
        &mut self,
        m: &Mat4,
        r: f32,
        color: [u8; 3],
        seg: u32,
        rings: u32,
        alpha: u8,
        flat_color: bool,
    ) {
        self.sphere_impl(m, r, color, seg, rings, alpha, flat_color);
    }

    #[allow(clippy::too_many_arguments)]
    fn sphere_impl(
        &mut self,
        m: &Mat4,
        r: f32,
        color: [u8; 3],
        seg: u32,
        rings: u32,
        alpha: u8,
        flat_color: bool,
    ) {
        let n_verts = ((seg + 1) * (rings + 1)) as usize;
        let c = self.chunk_for(n_verts);
        let base = c.verts.len() as u16;
        for j in 0..=rings {
            let phi = j as f32 / rings as f32 * std::f32::consts::PI;
            for i in 0..=seg {
                let th = i as f32 / seg as f32 * std::f32::consts::TAU;
                let n = Vec3::new(phi.sin() * th.cos(), phi.cos(), phi.sin() * th.sin());
                let p = m.transform_point3(n * r);
                let wn = m.transform_vector3(n).normalize_or_zero();
                c.verts.push(vert(p, wn, color, alpha, flat_color));
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

    /// Box with rounded edges and corners (radius `r`), `n` subdivisions per face.
    #[allow(clippy::too_many_arguments)]
    pub fn add_rounded_box_ex(
        &mut self,
        m: &Mat4,
        half: Vec3,
        r: f32,
        color: [u8; 3],
        alpha: u8,
        flat_color: bool,
        n: u32,
    ) {
        self.rounded_impl(m, half, r, color, alpha, flat_color, n);
    }

    #[allow(clippy::too_many_arguments)]
    fn rounded_impl(
        &mut self,
        m: &Mat4,
        half: Vec3,
        r: f32,
        color: [u8; 3],
        alpha: u8,
        flat_color: bool,
        n: u32,
    ) {
        let n = n.max(1);
        let r = r.min(half.min_element()).max(0.0);
        let inner = half - Vec3::splat(r);
        let verts = 6 * ((n + 1) * (n + 1)) as usize;
        let c = self.chunk_for(verts);
        // more vertices near the edges, where the surface curves
        let warp = |t: f32| (((t * 2.0 - 1.0) * std::f32::consts::FRAC_PI_2).sin() + 1.0) * 0.5;
        for (fnorm, corners) in BOX_FACES.iter() {
            let base = c.verts.len() as u16;
            for j in 0..=n {
                let v = warp(j as f32 / n as f32);
                for i in 0..=n {
                    let u = warp(i as f32 / n as f32);
                    let a = corners[0].lerp(corners[1], u);
                    let b = corners[3].lerp(corners[2], u);
                    let q = a.lerp(b, v) * half;
                    let core = q.clamp(-inner, inner);
                    let d = q - core;
                    let dir = if d.length_squared() > 1e-10 {
                        d.normalize()
                    } else {
                        *fnorm
                    };
                    let p = m.transform_point3(core + dir * r);
                    let wn = m.transform_vector3(dir).normalize_or_zero();
                    c.verts.push(vert(p, wn, color, alpha, flat_color));
                }
            }
            let row = (n + 1) as u16;
            for j in 0..n as u16 {
                for i in 0..n as u16 {
                    let a = base + j * row + i;
                    let b = a + row;
                    c.idx.extend_from_slice(&[a, a + 1, b + 1, a, b + 1, b]);
                }
            }
        }
    }

    /// Flat quad (decals, shadows, sprites). Corners in order. Unlit and unshadowed.
    pub fn add_quad(&mut self, p: [Vec3; 4], color: [u8; 4]) {
        self.add_quad_ex(p, color, Vec3::ZERO, (0, 1.0));
    }

    /// Quad with a normal (receives sun shadows) and a detail texture.
    pub fn add_quad_ex(&mut self, p: [Vec3; 4], color: [u8; 4], n: Vec3, detail: (u8, f32)) {
        let c = self.chunk_for(4);
        let base = c.verts.len() as u16;
        for v in p {
            c.verts.push(Vertex {
                position: v,
                uv: vec2(detail.0 as f32, detail.1),
                color,
                normal: n.extend(0.0),
            });
        }
        c.idx
            .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    }

    /// Adds ready-made vertices and triangles (skinned characters), split across chunks
    /// when they do not fit into one.
    pub fn add_indexed(&mut self, verts: &[Vertex], idx: &[u16]) {
        if verts.len() <= CHUNK_VERTS {
            let c = self.chunk_for(verts.len());
            let base = c.verts.len() as u16;
            c.verts.extend_from_slice(verts);
            c.idx.extend(idx.iter().map(|i| base + i));
            return;
        }
        let mut remap = vec![u16::MAX; verts.len()];
        self.chunks.push(Chunk::default());
        for tri in idx.chunks_exact(3) {
            if self.chunks.last().unwrap().verts.len() + 3 > CHUNK_VERTS {
                self.chunks.push(Chunk::default());
                remap.fill(u16::MAX);
            }
            let c = self.chunks.last_mut().unwrap();
            for &i in tri {
                let r = &mut remap[i as usize];
                if *r == u16::MAX {
                    *r = c.verts.len() as u16;
                    c.verts.push(verts[i as usize]);
                }
                c.idx.push(*r);
            }
        }
    }

    /// Submits all chunks with the currently bound material.
    pub fn draw(&self) {
        self.draw_tex(None);
    }

    /// Submits all chunks with `tex` bound as the shader's `Texture` (detail atlas).
    pub fn draw_tex(&self, tex: Option<&Texture2D>) {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.texture(tex);
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
    /// Flat ground paint (roads, pads). Each decal is lifted a few millimetres above the
    /// previous one in creation order, so overlapping paint never z-fights.
    pub decals: MeshBuilder,
    /// Level of detail for spheres (1 = coarse … 5+ = smooth).
    pub detail: u32,
    /// Outline colour override (characters get a team-coloured silhouette).
    pub ink: Option<[u8; 3]>,
}

pub const OUTLINE: [u8; 3] = [24, 22, 30];

impl ToonMesh {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn clear(&mut self) {
        self.fill.clear();
        self.outline.clear();
        self.decals.clear();
    }
    /// Box plus outline of width `w` (0 = no outline).
    pub fn boxed(&mut self, m: &Mat4, half: Vec3, color: [u8; 3], w: f32) {
        self.fill.add_box(m, half, color);
        if w > 0.0 {
            self.outline.add_box_ex(
                m,
                half + Vec3::splat(w),
                self.ink.unwrap_or(OUTLINE),
                255,
                true,
            );
        }
    }
    /// Rounded box plus outline. `n` = subdivisions per face (level of detail).
    pub fn rounded(&mut self, m: &Mat4, half: Vec3, r: f32, color: [u8; 3], w: f32, n: u32) {
        self.fill
            .add_rounded_box_ex(m, half, r, color, 255, false, n);
        if w > 0.0 {
            self.outline.add_rounded_box_ex(
                m,
                half + Vec3::splat(w),
                r + w,
                self.ink.unwrap_or(OUTLINE),
                255,
                true,
                n.min(3),
            );
        }
    }
    /// Level of detail for spheres.
    fn lod(&self) -> u32 {
        if self.detail == 0 {
            5
        } else {
            self.detail
        }
    }
    pub fn sphere(&mut self, m: &Mat4, r: f32, color: [u8; 3], w: f32) {
        let (seg, rings) = match self.lod() {
            5.. => (10, 7),
            3 | 4 => (8, 5),
            _ => (6, 4),
        };
        self.fill.add_sphere(m, r, color, seg, rings);
        // tiny far-away bits don't need outlines
        if w > 0.0 && (self.lod() > 2 || r > 0.07) {
            self.outline.add_sphere_ex(
                m,
                r + w,
                self.ink.unwrap_or(OUTLINE),
                seg,
                rings,
                255,
                true,
            );
        }
    }
}

pub struct Materials {
    pub world: Material,
    pub outline: Material,
    /// Unlit, blended (particles, glows, shadows). Note: miniquad's GL backend turns the
    /// depth *test* off together with depth writes, so this must keep depth writes on —
    /// otherwise muzzle flashes and tracers would be visible through walls.
    pub fx: Material,
    /// Writes packed depth for the sun's shadow map.
    pub depth: Material,
    /// Detail texture atlas (see `textures.rs`).
    pub atlas: Texture2D,
    white: Texture2D,
}

const VS: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;
varying lowp vec4 color;
varying highp float fogd;
varying highp vec3 vWorld;
varying mediump vec3 vN;
varying mediump vec2 vTile;
varying highp vec3 vShadow;
uniform mat4 Model;
uniform mat4 Projection;
uniform vec3 CamPos;
uniform mat4 SunMatrix;
uniform vec4 Params;
void main() {
    vec4 wp = Model * vec4(position, 1.0);
    gl_Position = Projection * wp;
    color = color0 / 255.0;
    fogd = length(wp.xyz - CamPos);
    vWorld = wp.xyz;
    vN = normal.xyz;
    vTile = texcoord;
    // offset along the normal so surfaces don't shadow themselves
    vec4 sp = SunMatrix * vec4(wp.xyz + normal.xyz * 0.08, 1.0);
    vShadow = sp.xyz * 0.5 + 0.5;
}
"#;

const FS: &str = r#"#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
varying lowp vec4 color;
varying highp float fogd;
varying highp vec3 vWorld;
varying mediump vec3 vN;
varying mediump vec2 vTile;
varying highp vec3 vShadow;
uniform sampler2D Texture;
uniform sampler2D ShadowMap;
uniform vec4 FogColor;
uniform vec2 FogRange;
uniform vec3 SunDir;
// x: shadows on, y: textures on, z: shadow texel size, w: depth bias
uniform vec4 Params;
uniform vec3 Tint;

float occluder(vec2 uv) {
    vec4 c = texture2D(ShadowMap, uv);
    return c.r + c.g / 255.0;
}

void main() {
    vec3 col = color.rgb;
    if (vTile.x > 0.5 && Params.y > 0.5) {
        // triplanar detail texture from the 4x4 atlas, faded out with distance
        vec3 an = abs(vN);
        vec2 p = an.y > an.x && an.y > an.z ? vWorld.xz : (an.x > an.z ? vWorld.zy : vWorld.xy);
        p = p / vTile.y;
        float t = floor(vTile.x + 0.5) - 1.0;
        vec2 cell = vec2(mod(t, 4.0), floor(t / 4.0));
        vec2 uv = (cell + 0.02 + fract(p) * 0.96) * 0.25;
        float d = texture2D(Texture, uv).r;
        float fade = 1.0 - clamp((fogd - 25.0) / 50.0, 0.0, 1.0);
        col *= mix(1.0, 0.55 + d * 0.9, fade);
    }
    float ndl = dot(vN, SunDir);
    if (Params.x > 0.5 && ndl > 0.08) {
        float lit = 1.0;
        if (vShadow.x > 0.0 && vShadow.x < 1.0 && vShadow.y > 0.0 && vShadow.y < 1.0) {
            float z = vShadow.z - Params.w;
            float s = Params.z;
            lit = step(z, occluder(vShadow.xy));
            lit += step(z, occluder(vShadow.xy + vec2(s, 0.0)));
            lit += step(z, occluder(vShadow.xy - vec2(s, 0.0)));
            lit += step(z, occluder(vShadow.xy + vec2(0.0, s)));
            lit += step(z, occluder(vShadow.xy - vec2(0.0, s)));
            lit *= 0.2;
        }
        // cool, bluish toon shade (sun-facing faces drop more, like the baked bands)
        vec3 shade = ndl > 0.55 ? vec3(0.66, 0.69, 0.84) : vec3(0.8, 0.82, 0.93);
        col *= mix(shade, vec3(1.0), lit);
    }
    col *= Tint;
    float f = clamp((fogd - FogRange.x) / (FogRange.y - FogRange.x), 0.0, 1.0);
    gl_FragColor = vec4(mix(col, FogColor.rgb, f), color.a);
}
"#;

const DEPTH_VS: &str = r#"#version 100
attribute vec3 position;
attribute vec2 texcoord;
attribute vec4 color0;
attribute vec4 normal;
uniform mat4 Model;
uniform mat4 Projection;
void main() {
    gl_Position = Projection * Model * vec4(position, 1.0);
}
"#;

const DEPTH_FS: &str = r#"#version 100
#ifdef GL_FRAGMENT_PRECISION_HIGH
precision highp float;
#else
precision mediump float;
#endif
void main() {
    // 16-bit depth packed into red (high) and green (low)
    float d = gl_FragCoord.z;
    float hi = floor(d * 255.0) / 255.0;
    gl_FragColor = vec4(hi, (d - hi) * 255.0, 0.0, 1.0);
}
"#;

fn mat(cull: CullFace, depth_write: bool) -> Material {
    let blend = Some(BlendState::new(
        Equation::Add,
        BlendFactor::Value(BlendValue::SourceAlpha),
        BlendFactor::OneMinusValue(BlendValue::SourceAlpha),
    ));
    load_material(
        ShaderSource::Glsl {
            vertex: VS,
            fragment: FS,
        },
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
                UniformDesc::new("SunMatrix", UniformType::Mat4),
                UniformDesc::new("SunDir", UniformType::Float3),
                UniformDesc::new("Params", UniformType::Float4),
                UniformDesc::new("Tint", UniformType::Float3),
            ],
            textures: vec!["ShadowMap".to_string()],
        },
    )
    .expect("toon material")
}

/// The sun's view of a level, rendered once per map into a depth texture.
pub struct ShadowMap {
    pub target: RenderTarget,
    pub matrix: Mat4,
    /// Depth units per metre along the sun direction.
    pub depth_per_m: f32,
    pub res: u32,
}

struct SunCamera<'a> {
    matrix: Mat4,
    target: &'a RenderTarget,
}

impl macroquad::camera::Camera for SunCamera<'_> {
    fn matrix(&self) -> Mat4 {
        self.matrix
    }
    fn depth_enabled(&self) -> bool {
        true
    }
    fn render_pass(&self) -> Option<macroquad::texture::RenderPass> {
        Some(self.target.render_pass.clone())
    }
    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        None
    }
}

impl ShadowMap {
    /// Renders the static level from the sun. `bounds` is the level's AABB.
    pub fn render(mats: &Materials, level: &ToonMesh, min: Vec3, max: Vec3, res: u32) -> ShadowMap {
        let l = LIGHT_DIR.normalize();
        let centre = (min + max) * 0.5;
        let view = Mat4::look_at_rh(centre + l * 200.0, centre, Vec3::Y);
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for i in 0..8 {
            let c = vec3(
                if i & 1 == 0 { min.x } else { max.x },
                if i & 2 == 0 { min.y } else { max.y },
                if i & 4 == 0 { min.z } else { max.z },
            );
            let v = view.transform_point3(c);
            lo = lo.min(v);
            hi = hi.max(v);
        }
        let (near, far) = (-hi.z - 2.0, -lo.z + 2.0);
        let proj = Mat4::orthographic_rh_gl(lo.x, hi.x, lo.y, hi.y, near, far);
        let matrix = proj * view;
        let target = render_target_ex(
            res,
            res,
            RenderTargetParams {
                sample_count: 1,
                depth: true,
            },
        );
        target.texture.set_filter(FilterMode::Nearest);
        set_camera(&SunCamera {
            matrix,
            target: &target,
        });
        clear_background(WHITE);
        gl_use_material(&mats.depth);
        level.fill.draw();
        gl_use_default_material();
        set_default_camera();
        ShadowMap {
            target,
            matrix,
            depth_per_m: 1.0 / (far - near),
            res,
        }
    }
}

/// How the world is lit this frame.
pub struct Lighting<'a> {
    pub shadow: Option<&'a ShadowMap>,
    pub textures: bool,
    /// Colour multiplier (the first-person gun takes the shade of where you stand).
    pub tint: Vec3,
}

impl Materials {
    pub fn new() -> Materials {
        let depth = load_material(
            ShaderSource::Glsl {
                vertex: DEPTH_VS,
                fragment: DEPTH_FS,
            },
            MaterialParams {
                pipeline_params: PipelineParams {
                    cull_face: CullFace::Nothing,
                    depth_test: Comparison::LessOrEqual,
                    depth_write: true,
                    ..Default::default()
                },
                uniforms: vec![],
                textures: vec![],
            },
        )
        .expect("depth material");
        let white = Texture2D::from_rgba8(1, 1, &[255, 255, 255, 255]);
        let m = Materials {
            world: mat(CullFace::Nothing, true),
            outline: mat(CullFace::Front, true),
            fx: mat(CullFace::Nothing, true),
            depth,
            atlas: crate::textures::detail_atlas(),
            white,
        };
        m.set_lighting(&Lighting {
            shadow: None,
            textures: false,
            tint: Vec3::ONE,
        });
        m
    }

    pub fn set_view(&self, cam: Vec3, fog: Color, near: f32, far: f32) {
        for m in [&self.world, &self.outline, &self.fx] {
            m.set_uniform("CamPos", cam);
            m.set_uniform("FogColor", vec4(fog.r, fog.g, fog.b, 1.0));
            m.set_uniform("FogRange", vec2(near, far));
        }
    }

    pub fn set_lighting(&self, l: &Lighting) {
        let (tex, matrix, params) = match l.shadow {
            Some(s) => (
                &s.target.texture,
                s.matrix,
                vec4(
                    1.0,
                    l.textures as u8 as f32,
                    1.0 / s.res as f32,
                    0.03 * s.depth_per_m,
                ),
            ),
            None => (
                &self.white,
                Mat4::IDENTITY,
                vec4(0.0, l.textures as u8 as f32, 0.0, 0.0),
            ),
        };
        for m in [&self.world, &self.outline, &self.fx] {
            m.set_texture("ShadowMap", tex.clone());
            m.set_uniform("SunMatrix", matrix);
            m.set_uniform("SunDir", LIGHT_DIR.normalize());
            m.set_uniform("Params", params);
            m.set_uniform("Tint", l.tint);
        }
    }

    /// Draws a toon mesh: outline pass then fill.
    pub fn draw_toon(&self, m: &ToonMesh) {
        if !m.outline.is_empty() {
            gl_use_material(&self.outline);
            m.outline.draw();
        }
        gl_use_material(&self.world);
        m.fill.draw_tex(Some(&self.atlas));
        m.decals.draw_tex(Some(&self.atlas));
        gl_use_default_material();
    }
}

/// Model matrix helper: translation * yaw * pitch * roll.
pub fn trs(t: Vec3, yaw: f32, pitch: f32, roll: f32) -> Mat4 {
    Mat4::from_translation(t)
        * Mat4::from_rotation_y(yaw)
        * Mat4::from_rotation_x(pitch)
        * Mat4::from_rotation_z(roll)
}
