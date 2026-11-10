//! The static level, ready to draw: small maps are a single mesh; big terrain maps are cut
//! into 64 m tiles, each with its share of the boxes and the ground at several levels of
//! detail, so a frame only draws what is in view and close enough to see through the fog.

use crate::mesh::{MeshBuilder, ToonMesh, OUTLINE};
use macroquad::models::Vertex;
use macroquad::prelude::*;
use toon_shared::map::{Map, MapBox};
use toon_shared::rng::Rng;
use toon_shared::terrain::{fbm, paint, Heightmap};

/// Tile size in grid cells (on terrain maps).
const TILE_CELLS: usize = 32;
/// Grid steps of the ground's levels of detail.
const LODS: [usize; 4] = [1, 2, 4, 8];
/// Fog on terrain maps: (starts, opaque).
pub const TERRAIN_FOG: (f32, f32) = (90.0, 330.0);
/// The level's outlines fade out between these distances (m): only where the fog
/// swallows everything anyway.
const OUTLINE_FADE: (f32, f32) = (170.0, 260.0);
/// Fog on the small maps.
pub const ARENA_FOG: (f32, f32) = (55.0, 230.0);

/// Mesh of a set of map boxes (with outlines if `outlines`).
pub fn boxes_mesh<'a>(
    boxes: impl Iterator<Item = &'a MapBox>,
    seed: u64,
    outlines: bool,
) -> ToonMesh {
    let mut m = ToonMesh::new();
    let mut rng = Rng::new(seed);
    let mut decal_layer = 0;
    for b in boxes {
        if !b.visible {
            continue;
        }
        // tiny colour variation per box for a hand-painted feel
        let j = rng.range(-10.0, 10.0);
        let c = [
            (b.color[0] as f32 + j).clamp(0.0, 255.0) as u8,
            (b.color[1] as f32 + j).clamp(0.0, 255.0) as u8,
            (b.color[2] as f32 + j * 0.8).clamp(0.0, 255.0) as u8,
        ];
        let size = b.max - b.min;
        if !b.solid && size.y <= 0.035 && b.min.y < 0.3 {
            // flat paint on the ground: just the top face, drawn as a decal
            decal_layer += 1;
            let y = b.max.y + decal_layer as f32 * 0.004;
            let col = crate::mesh::shade(c, Vec3::Y);
            m.decals.add_quad_ex(
                [
                    vec3(b.min.x, y, b.min.z),
                    vec3(b.min.x, y, b.max.z),
                    vec3(b.max.x, y, b.max.z),
                    vec3(b.max.x, y, b.min.z),
                ],
                col,
                Vec3::Y,
                crate::textures::material_for(b.color),
            );
            continue;
        }
        m.fill
            .add_aabb(b.min, b.max, c, crate::textures::material_for(b.color));
        let is_ground = b.min.y < -0.5 && size.x > 20.0;
        if outlines && b.solid && !is_ground && size.min_element() > 0.15 {
            let w = Vec3::splat(0.045);
            m.outline
                .add_outline_box((b.min + b.max) * 0.5, size * 0.5 + w, OUTLINE);
        }
    }
    m
}

struct Tile {
    min: Vec3,
    max: Vec3,
    /// Boxes up close (with outlines and small details).
    near: ToonMesh,
    /// The same boxes far away: big shapes only, no outlines.
    far: ToonMesh,
    /// The ground, one mesh per level of detail.
    ground: Vec<MeshBuilder>,
    /// Water surface over this tile.
    water: Option<MeshBuilder>,
}

/// A painted top-down map of a big terrain map (for the minimap).
pub struct MapImage {
    pub tex: Texture2D,
    /// World (x, z) of the image's top-left corner and its size in metres.
    pub origin: Vec2,
    pub size: f32,
}

pub struct Level {
    tiles: Vec<Tile>,
    pub terrain: bool,
    pub image: Option<MapImage>,
    /// Bounds of everything drawn.
    pub min: Vec3,
    pub max: Vec3,
}

/// Colour of the ground at grid point (i, j).
fn ground_color(m: &Heightmap, i: usize, j: usize) -> [u8; 3] {
    let p = m.point(i, j);
    let n = m.normal(p.x, p.z);
    let slope = (1.0 - n.y * n.y).max(0.0).sqrt() / n.y.max(1e-3);
    let grass = [118.0, 172.0, 82.0];
    let base: [f32; 3] = match m.paint_at(i as i64, j as i64) {
        paint::ROAD => [178.0, 150.0, 106.0],
        paint::DIRT => [170.0, 158.0, 116.0],
        paint::PAVED => [170.0, 166.0, 158.0],
        paint::SAND => [228.0, 208.0, 152.0],
        paint::FIELD_A => [212.0, 184.0, 92.0],
        paint::FIELD_B => [160.0, 172.0, 78.0],
        paint::FOREST => [88.0, 140.0, 72.0],
        paint::MEADOW => [146.0, 190.0, 92.0],
        _ => grass,
    };
    let natural = matches!(
        m.paint_at(i as i64, j as i64),
        paint::AUTO | paint::FOREST | paint::MEADOW
    );
    let mut c = base;
    let mix = |c: &mut [f32; 3], to: [f32; 3], k: f32| {
        for q in 0..3 {
            c[q] += (to[q] - c[q]) * k.clamp(0.0, 1.0);
        }
    };
    if natural {
        // bare rock on steep slopes, snow on the peaks
        mix(&mut c, [134.0, 128.0, 120.0], (slope - 0.55) / 0.35);
        if slope < 0.8 {
            mix(&mut c, [236.0, 240.0, 244.0], (p.y - 52.0) / 6.0);
        }
        // greener in the valleys, drier up high
        mix(&mut c, [150.0, 160.0, 96.0], (p.y - 18.0) / 40.0 * 0.6);
    }
    if p.y < m.water + 0.2 {
        // wet sand and mud under the water
        mix(&mut c, [150.0, 140.0, 104.0], 0.8);
    }
    let v = 0.92 + 0.16 * fbm(91, p.x / 9.0, p.z / 9.0, 2);
    [
        (c[0] * v).min(255.0) as u8,
        (c[1] * v).min(255.0) as u8,
        (c[2] * v).min(255.0) as u8,
    ]
}

/// Ground of one tile at grid step `s`: the same triangle split as `Heightmap::height`,
/// with a skirt round the edge that hides cracks against coarser neighbours.
fn ground_mesh(m: &Heightmap, colors: &[[u8; 3]], i0: usize, j0: usize, s: usize) -> MeshBuilder {
    let n = TILE_CELLS / s;
    let mut verts = Vec::with_capacity((n + 1) * (n + 1) + 8 * n);
    let mut idx: Vec<u16> = Vec::with_capacity(n * n * 6 + 24 * n);
    // no detail texture: on long slopes it turns into contour-line moiré
    let detail = (0u8, 1.0f32);
    let vert = |i: usize, j: usize, drop: f32| -> Vertex {
        let (i, j) = (i.min(m.w - 1), j.min(m.h - 1));
        let p = m.point(i, j);
        let nrm = m.normal(p.x, p.z);
        let mut col = crate::mesh::shade(colors[j * m.w + i], nrm);
        col[3] = 255;
        Vertex {
            position: p - Vec3::Y * drop,
            uv: vec2(detail.0 as f32, detail.1),
            color: col,
            normal: nrm.extend(0.0),
        }
    };
    for b in 0..=n {
        for a in 0..=n {
            verts.push(vert(i0 + a * s, j0 + b * s, 0.0));
        }
    }
    let at = |a: usize, b: usize| (b * (n + 1) + a) as u16;
    for b in 0..n {
        for a in 0..n {
            let (v00, v10, v01, v11) = (at(a, b), at(a + 1, b), at(a, b + 1), at(a + 1, b + 1));
            idx.extend_from_slice(&[v00, v10, v11, v00, v11, v01]);
        }
    }
    // skirts
    let drop = 1.5 * s as f32;
    let edges: [Box<dyn Fn(usize) -> (usize, usize)>; 4] = [
        Box::new(|k| (k, 0)),
        Box::new(|k| (k, n)),
        Box::new(|k| (0, k)),
        Box::new(|k| (n, k)),
    ];
    for e in edges.iter() {
        let start = verts.len() as u16;
        for k in 0..=n {
            let (a, b) = e(k);
            let mut v = vert(i0 + a * s, j0 + b * s, drop);
            v.color = verts[at(a, b) as usize].color;
            verts.push(v);
        }
        for k in 0..n {
            let (a0, b0) = e(k);
            let (a1, b1) = e(k + 1);
            let (t0, t1) = (at(a0, b0), at(a1, b1));
            let (d0, d1) = (start + k as u16, start + k as u16 + 1);
            idx.extend_from_slice(&[t0, t1, d1, t0, d1, d0]);
        }
    }
    let mut mb = MeshBuilder::new();
    mb.add_indexed(&verts, &idx);
    mb
}

/// Paints the minimap: ground colours with hill shading, water, buildings and woods.
fn map_image(map: &Map, m: &Heightmap, colors: &[[u8; 3]]) -> MapImage {
    const N: usize = 512;
    let (lo, hi) = m.bounds();
    let size = (hi - lo).max_element();
    let px = size / N as f32;
    let mut img = vec![0u8; N * N * 4];
    let light = vec3(-0.5, 0.75, -0.45).normalize();
    for y in 0..N {
        for x in 0..N {
            let wx = lo.x + (x as f32 + 0.5) * px;
            let wz = lo.y + (y as f32 + 0.5) * px;
            let (i, j) = m.index_of(wx, wz).unwrap_or((0, 0));
            let h = m.height(wx, wz);
            let mut c = colors[j * m.w + i].map(|v| v as f32);
            if h < m.water {
                let deep = ((m.water - h) / 1.2).clamp(0.3, 1.0);
                c = [
                    c[0] + (70.0 - c[0]) * deep,
                    c[1] + (130.0 - c[1]) * deep,
                    c[2] + (180.0 - c[2]) * deep,
                ];
            } else {
                let k = 0.8 + 0.35 * m.normal(wx, wz).dot(light);
                c = c.map(|v| v * k);
            }
            let o = (y * N + x) * 4;
            img[o] = c[0].min(255.0) as u8;
            img[o + 1] = c[1].min(255.0) as u8;
            img[o + 2] = c[2].min(255.0) as u8;
            img[o + 3] = 255;
        }
    }
    let mut fill = |b: &MapBox, col: [u8; 3]| {
        let x0 = ((b.min.x - lo.x) / px).floor().max(0.0) as usize;
        let x1 = ((b.max.x - lo.x) / px).ceil().min(N as f32) as usize;
        let y0 = ((b.min.z - lo.y) / px).floor().max(0.0) as usize;
        let y1 = ((b.max.z - lo.y) / px).ceil().min(N as f32) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let o = (y * N + x) * 4;
                img[o..o + 3].copy_from_slice(&col);
            }
        }
    };
    // tree crowns first, then the buildings over them
    for b in map.boxes.iter().filter(|b| b.visible && !b.solid) {
        let s = b.max - b.min;
        if s.x > 1.5 && s.y > 0.5 {
            fill(b, toon_shared::map::shade(b.color, 0.7));
        }
    }
    for b in map.boxes.iter().filter(|b| b.visible && b.solid) {
        let s = b.max - b.min;
        if s.x.min(s.z) > 0.25 && s.y > 0.3 && s.x.max(s.z) > 2.0 {
            let k = 0.75 + ((b.max.y - m.height(b.min.x, b.min.z)) / 12.0).clamp(0.0, 0.35);
            fill(b, toon_shared::map::shade([180, 178, 172], k));
        }
    }
    let tex = Texture2D::from_rgba8(N as u16, N as u16, &img);
    tex.set_filter(FilterMode::Linear);
    MapImage {
        tex,
        origin: lo,
        size,
    }
}

impl Level {
    pub fn build(map: &Map) -> Level {
        let Some(m) = map.terrain.as_deref() else {
            let mesh = crate::level::boxes_mesh(map.boxes.iter(), map.id as u64 + 3, true);
            let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
            for b in map.boxes.iter().filter(|b| b.visible) {
                lo = lo.min(b.min);
                hi = hi.max(b.max);
            }
            lo.y = lo.y.max(-1.0);
            return Level {
                tiles: vec![Tile {
                    min: lo,
                    max: hi,
                    near: mesh,
                    far: ToonMesh::new(),
                    ground: Vec::new(),
                    water: None,
                }],
                terrain: false,
                image: None,
                min: lo,
                max: hi,
            };
        };
        let colors: Vec<[u8; 3]> = (0..m.h)
            .flat_map(|j| (0..m.w).map(move |i| (i, j)))
            .map(|(i, j)| ground_color(m, i, j))
            .collect();
        let tw = (m.w - 1).div_ceil(TILE_CELLS);
        let th = (m.h - 1).div_ceil(TILE_CELLS);
        let size = TILE_CELLS as f32 * m.cell;
        // boxes go to the tile under their centre
        let mut groups: Vec<Vec<&MapBox>> = vec![Vec::new(); tw * th];
        for b in map.boxes.iter().filter(|b| b.visible) {
            let c = (b.min + b.max) * 0.5;
            let ti = (((c.x - m.origin.x) / size).floor() as i64).clamp(0, tw as i64 - 1) as usize;
            let tj = (((c.z - m.origin.y) / size).floor() as i64).clamp(0, th as i64 - 1) as usize;
            groups[tj * tw + ti].push(b);
        }
        let mut tiles = Vec::with_capacity(tw * th);
        let (mut all_lo, mut all_hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for tj in 0..th {
            for ti in 0..tw {
                let (i0, j0) = (ti * TILE_CELLS, tj * TILE_CELLS);
                let ground: Vec<MeshBuilder> = LODS
                    .iter()
                    .map(|&s| ground_mesh(m, &colors, i0, j0, s))
                    .collect();
                let mut lo = vec3(
                    m.origin.x + i0 as f32 * m.cell,
                    f32::MAX,
                    m.origin.y + j0 as f32 * m.cell,
                );
                let mut hi = vec3(lo.x + size, f32::MIN, lo.z + size);
                let mut wet = false;
                for j in j0..=(j0 + TILE_CELLS).min(m.h - 1) {
                    for i in i0..=(i0 + TILE_CELLS).min(m.w - 1) {
                        let y = m.at(i as i64, j as i64);
                        lo.y = lo.y.min(y);
                        hi.y = hi.y.max(y);
                        wet |= y < m.water;
                    }
                }
                let group = &groups[tj * tw + ti];
                for b in group {
                    lo = lo.min(b.min);
                    hi = hi.max(b.max);
                }
                let seed = (tj * tw + ti) as u64 + 3;
                let near = boxes_mesh(group.iter().copied(), seed, true);
                let far = boxes_mesh(
                    group.iter().copied().filter(|b| {
                        let s = b.max - b.min;
                        s.x * s.y * s.z > 1.0 && s.min_element() > 0.2
                    }),
                    seed,
                    false,
                );
                let water = wet.then(|| {
                    let y = m.water;
                    hi.y = hi.y.max(y);
                    let mut w = MeshBuilder::new();
                    w.add_quad(
                        [
                            vec3(lo.x, y, lo.z),
                            vec3(lo.x, y, lo.z + size),
                            vec3(lo.x + size, y, lo.z + size),
                            vec3(lo.x + size, y, lo.z),
                        ],
                        [86, 156, 200, 190],
                    );
                    w
                });
                all_lo = all_lo.min(lo);
                all_hi = all_hi.max(hi);
                tiles.push(Tile {
                    min: lo,
                    max: hi,
                    near,
                    far,
                    ground,
                    water,
                });
            }
        }
        let image = Some(map_image(map, m, &colors));
        Level {
            tiles,
            terrain: true,
            image,
            min: all_lo,
            max: all_hi,
        }
    }

    /// Horizontal distance from `p` to a tile.
    fn dist(t: &Tile, p: Vec3) -> f32 {
        let dx = (t.min.x - p.x).max(p.x - t.max.x).max(0.0);
        let dz = (t.min.z - p.z).max(p.z - t.max.z).max(0.0);
        (dx * dx + dz * dz).sqrt()
    }

    /// Whether a box is (partly) inside the view frustum.
    fn visible(matrix: &Mat4, min: Vec3, max: Vec3) -> bool {
        let mut out = [0u8; 6];
        for k in 0..8 {
            let c = vec3(
                if k & 1 == 0 { min.x } else { max.x },
                if k & 2 == 0 { min.y } else { max.y },
                if k & 4 == 0 { min.z } else { max.z },
            );
            let v = *matrix * c.extend(1.0);
            let tests = [
                v.x < -v.w,
                v.x > v.w,
                v.y < -v.w,
                v.y > v.w,
                v.z < -v.w,
                v.z > v.w,
            ];
            for (o, t) in out.iter_mut().zip(tests) {
                *o += t as u8;
            }
        }
        out.iter().all(|&o| o < 8)
    }

    /// Draws the opaque level as seen from `cam` (with the view-projection `matrix`).
    /// The black outlines come last and fade out with distance: far away they would be
    /// thinner than a pixel and break up into flickering dashes.
    /// `outline_grow`: metres per metre of distance that keep an outline about one pixel
    /// wide (0 = no outlines).
    pub fn draw(
        &self,
        mats: &crate::mesh::Materials,
        cam: Vec3,
        matrix: &Mat4,
        outline_grow: f32,
        fog_far: f32,
    ) {
        let outlines = outline_grow > 0.0;
        let fade = (OUTLINE_FADE.0, OUTLINE_FADE.1, outline_grow);
        if !self.terrain {
            let t = &self.tiles[0];
            gl_use_material(&mats.world);
            t.near.fill.draw_tex(Some(&mats.atlas));
            t.near.decals.draw_tex(Some(&mats.atlas));
            if outlines {
                mats.set_outline_fade(Some(fade));
                gl_use_material(&mats.outline);
                t.near.outline.draw();
                mats.set_outline_fade(None);
            }
            gl_use_default_material();
            return;
        }
        let far = fog_far + 10.0;
        let shown: Vec<(&Tile, f32)> = self
            .tiles
            .iter()
            .map(|t| (t, Self::dist(t, cam)))
            .filter(|(t, d)| *d < far && (*d < 1.0 || Self::visible(matrix, t.min, t.max)))
            .collect();
        gl_use_material(&mats.world);
        for (t, d) in &shown {
            let lod = match *d {
                d if d < 80.0 => 0,
                d if d < 160.0 => 1,
                d if d < 250.0 => 2,
                _ => 3,
            };
            t.ground[lod].draw_tex(Some(&mats.atlas));
            if *d < 150.0 {
                t.near.fill.draw_tex(Some(&mats.atlas));
                t.near.decals.draw_tex(Some(&mats.atlas));
            } else {
                t.far.fill.draw_tex(Some(&mats.atlas));
            }
        }
        if outlines {
            mats.set_outline_fade(Some(fade));
            gl_use_material(&mats.outline);
            for (t, d) in &shown {
                if *d < OUTLINE_FADE.1 {
                    t.near.outline.draw();
                }
            }
            mats.set_outline_fade(None);
        }
        gl_use_default_material();
    }

    /// Draws the water surfaces (after everything opaque).
    pub fn draw_water(&self, mats: &crate::mesh::Materials, cam: Vec3, fog_far: f32) {
        if !self.terrain {
            return;
        }
        gl_use_material(&mats.fx);
        for t in &self.tiles {
            if let Some(w) = &t.water {
                if Self::dist(t, cam) < fog_far + 10.0 {
                    w.draw();
                }
            }
        }
        gl_use_default_material();
    }

    /// Everything that casts sun shadows inside `min..max` (x/z).
    pub fn casters(&self, min: Vec3, max: Vec3) -> Vec<&MeshBuilder> {
        let mut out = Vec::new();
        for t in &self.tiles {
            if t.max.x < min.x || t.min.x > max.x || t.max.z < min.z || t.min.z > max.z {
                continue;
            }
            // the ground only receives shadows: on long gentle slopes it would shadow
            // itself in stripes
            out.push(&t.near.fill);
        }
        out
    }
}
