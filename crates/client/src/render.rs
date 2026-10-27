//! 3D rendering of a frame: sky, level, characters, entities, effects and the first-person weapon.

use crate::game::GameClient;
use crate::mesh::{trs, Materials, ToonMesh};
use crate::models::{self, SoldierPose};
use crate::settings::Settings;
use macroquad::prelude::*;
use toon_shared::look_dir;
use toon_shared::physics::Stance;
use toon_shared::protocol::{flags, EntSnap, MatchState};
use toon_shared::weapons::WeaponClass;

pub struct View {
    pub cam: Camera3D,
    pub fovy: f32,
    pub pos: Vec3,
    pub matrix: Mat4,
}

impl View {
    /// World → screen pixels (None if behind the camera).
    pub fn project(&self, p: Vec3) -> Option<Vec2> {
        let c = self.matrix * p.extend(1.0);
        if c.w <= 0.05 {
            return None;
        }
        let ndc = c.truncate() / c.w;
        Some(vec2(
            (ndc.x + 1.0) * 0.5 * screen_width(),
            (1.0 - ndc.y) * 0.5 * screen_height(),
        ))
    }
}

/// Camera for the first-person weapon: its depth range is squashed into the very front of
/// the depth buffer, so the gun never clips into walls and no depth clear is needed.
struct ViewmodelCamera(Camera3D);

impl macroquad::camera::Camera for ViewmodelCamera {
    fn matrix(&self) -> Mat4 {
        let squash = Mat4::from_cols(
            vec4(1.0, 0.0, 0.0, 0.0),
            vec4(0.0, 1.0, 0.0, 0.0),
            vec4(0.0, 0.0, 0.01, 0.0),
            vec4(0.0, 0.0, -0.99, 1.0),
        );
        squash * self.0.matrix()
    }
    fn depth_enabled(&self) -> bool {
        true
    }
    fn render_pass(&self) -> Option<macroquad::texture::RenderPass> {
        self.0.render_pass()
    }
    fn viewport(&self) -> Option<(i32, i32, i32, i32)> {
        None
    }
}

pub struct NameTag {
    pub pos: Vec3,
    pub name: String,
    pub friendly: bool,
    pub bomb: bool,
}

fn fog_color(g: &GameClient) -> Color {
    Color::from_rgba(g.map.fog[0], g.map.fog[1], g.map.fog[2], 255)
}

pub fn make_view(g: &GameClient, settings: &Settings, target: Option<RenderTarget>) -> View {
    let (pos, yaw, pitch) = g.camera();
    let aspect = screen_width() / screen_height().max(1.0);
    let hfov = settings.fov.clamp(60.0, 120.0).to_radians();
    let mut fovy = 2.0 * ((hfov * 0.5).tan() / aspect).atan();
    let zoom = 1.0 + (g.ws().ads_zoom - 1.0) * g.ads_t;
    let zoom = if g.weapon().def().scoped && !g.scoped() {
        1.0 + (zoom - 1.0) * 0.25
    } else {
        zoom
    };
    if g.alive {
        fovy = 2.0 * ((fovy * 0.5).tan() / zoom).atan();
    }
    let dir = look_dir(yaw, pitch);
    let cam = Camera3D {
        position: pos,
        target: pos + dir,
        up: Vec3::Y,
        fovy,
        aspect: None,
        projection: Projection::Perspective,
        render_target: target,
        viewport: None,
        // a near plane this far keeps the depth buffer precise at range (no shimmering)
        z_near: 0.15,
        z_far: 420.0,
    };
    let matrix = cam.matrix();
    View {
        cam,
        fovy,
        pos,
        matrix,
    }
}

/// Renders the world. Returns name tags for the HUD.
pub fn render_world(
    g: &mut GameClient,
    mats: &Materials,
    settings: &Settings,
    view: &View,
) -> Vec<NameTag> {
    let fog = fog_color(g);
    set_camera(&view.cam);
    clear_background(fog);
    mats.set_view(view.pos, fog, 55.0, 230.0);

    // sky (follows the camera, no fog)
    let sky_model = Mat4::from_translation(vec3(view.pos.x, 0.0, view.pos.z));
    {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.push_model_matrix(sky_model);
    }
    mats.set_view(vec3(0.0, 0.0, 0.0), fog, 10_000.0, 20_000.0);
    gl_use_material(&mats.world);
    g.sky_mesh.draw();
    gl_use_default_material();
    {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.pop_model_matrix();
    }
    mats.set_view(view.pos, fog, 55.0, 230.0);

    // far ground outside the playable area
    {
        let c = g.map.ground_color();
        let mut far = crate::mesh::MeshBuilder::new();
        let s = 600.0;
        let y = -0.02;
        far.add_quad(
            [
                vec3(-s, y, -s),
                vec3(-s, y, s),
                vec3(s, y, s),
                vec3(s, y, -s),
            ],
            [c[0], c[1], c[2], 255],
        );
        gl_use_material(&mats.world);
        far.draw();
        gl_use_default_material();
    }

    // level
    mats.draw_toon(&g.map_mesh);

    // dynamic objects
    let mut m = std::mem::take(&mut g.frame_mesh);
    m.clear();
    let mut shadows = std::mem::take(&mut g.fx_mesh);
    shadows.clear();
    let mut tags = Vec::new();
    let now = g.now;
    let my_team = g.team;
    let ids: Vec<u8> = g.remotes.keys().copied().collect();
    for id in ids {
        let r = g.remotes.get(&id).unwrap();
        let dead = r.death_time.map(|t| (now - t) as f32);
        if !r.alive && dead.map(|d| d > 6.0).unwrap_or(true) {
            continue;
        }
        let has_bomb = r.flags & flags::HAS_BOMB != 0;
        let pose = SoldierPose {
            pos: r.pos,
            yaw: r.yaw,
            pitch: r.pitch,
            stance: r.stance,
            team: r.team,
            weapon: r.weapon,
            phase: r.phase,
            walk: r.walk,
            sprint: r.flags & flags::SPRINT != 0,
            dead,
            has_bomb,
            blink: ((now * 0.37 + id as f64 * 0.31).fract()) < 0.035,
            melee: ((now - r.melee_t) as f32 / 0.4).clamp(0.0, 1.0)
                * if now - r.melee_t < 0.4 { 1.0 } else { 0.0 },
        };
        let muzzle = models::add_soldier(&mut m, &pose);
        let floor = g
            .world
            .floor_below(r.pos + vec3(0.0, 0.3, 0.0), 3.0)
            .unwrap_or(r.pos.y);
        blob_shadow(&mut shadows, vec3(r.pos.x, floor + 0.03, r.pos.z), 0.5);
        if r.protected && r.team == my_team {
            // spawn protection: a soft pulsing bubble in team colour
            let c = crate::models::team_look(r.team).helmet;
            let pulse = 0.5 + 0.5 * ((now * 5.0) as f32).sin();
            let centre = r.pos + vec3(0.0, r.stance.height() * 0.5, 0.0);
            let m = Mat4::from_translation(centre) * Mat4::from_scale(vec3(1.0, 1.25, 1.0));
            shadows.add_sphere_ex(
                &m,
                0.95 + pulse * 0.05,
                c,
                14,
                9,
                (45.0 + pulse * 35.0) as u8,
                true,
            );
        }
        if r.alive {
            let head = r.pos + vec3(0.0, r.stance.height() + 0.45, 0.0);
            let friendly = r.team == my_team;
            let name = g.name_of(id).unwrap_or("?").to_string();
            tags.push(NameTag {
                pos: head,
                name,
                friendly,
                bomb: has_bomb,
            });
        }
        if let Some(rr) = g.remotes.get_mut(&id) {
            rr.muzzle = muzzle;
        }
    }
    // entities
    for e in &g.ents {
        match e {
            EntSnap::Grenade { id, pos } => {
                models::add_grenade(&mut m, Vec3::from_array(*pos), now as f32 + *id as f32)
            }
            EntSnap::Heli { pos, yaw, team, .. } => {
                models::add_heli(&mut m, Vec3::from_array(*pos), *yaw, *team, now as f32)
            }
            EntSnap::Plane { pos, yaw, .. } => {
                models::add_jet(&mut m, Vec3::from_array(*pos), look_dir(*yaw, 0.0))
            }
        }
    }
    // airstrike jets (purely visual)
    for (from, to, t0) in &g.airstrikes {
        let k = ((now - t0) / 3.6) as f32;
        if (0.0..=1.0).contains(&k) {
            let dir = (*to - *from).normalize();
            let side = vec3(-dir.z, 0.0, dir.x);
            for (i, off) in [-1.0f32, 0.0, 1.0].iter().enumerate() {
                let p = from.lerp(*to, k) + side * (*off * 9.0) - dir * (i as f32 * 4.0)
                    + vec3(0.0, *off * 2.0, 0.0);
                models::add_jet(&mut m, p, dir);
            }
        }
    }
    // bomb
    if let Some(b) = &g.bomb {
        if b.carrier.is_none() && g.state != MatchState::Warmup {
            models::add_bomb(
                &mut m,
                Vec3::from_array(b.pos),
                now as f32,
                b.planted_at.is_some(),
            );
        }
    }
    if !settings.outlines {
        m.outline.clear();
    }
    mats.draw_toon(&m);

    // shadows + particles (blended)
    g.fx.build(&mut shadows, view.pos);
    gl_use_material(&mats.fx);
    shadows.draw();
    gl_use_default_material();
    g.fx.draw_tracers();

    g.frame_mesh = m;
    g.fx_mesh = shadows;
    tags
}

fn blob_shadow(m: &mut crate::mesh::MeshBuilder, c: Vec3, r: f32) {
    let n = 10;
    for i in 0..n {
        let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
        let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
        m.add_quad(
            [
                c,
                c + vec3(a0.cos() * r, 0.0, a0.sin() * r),
                c + vec3(a1.cos() * r, 0.0, a1.sin() * r),
                c,
            ],
            [0, 0, 0, 80],
        );
    }
}

/// First-person weapon and hands, drawn after clearing depth so it never clips into walls.
pub fn render_viewmodel(
    g: &mut GameClient,
    mats: &Materials,
    settings: &Settings,
    target: Option<RenderTarget>,
) {
    if !g.alive || g.scoped() {
        return;
    }
    let cam = ViewmodelCamera(Camera3D {
        position: Vec3::ZERO,
        target: vec3(0.0, 0.0, -1.0),
        up: Vec3::Y,
        fovy: 62f32.to_radians(),
        aspect: None,
        projection: Projection::Perspective,
        render_target: target,
        viewport: None,
        z_near: 0.01,
        z_far: 10.0,
    });
    set_camera(&cam);
    mats.set_view(Vec3::ZERO, Color::new(0.0, 0.0, 0.0, 1.0), 100.0, 200.0);

    let now = g.now;
    let w = g.weapon();
    let def = w.def();
    let ads = g.ads_t;
    let hip = vec3(0.26, -0.27, -0.66);
    let ads_pos = vec3(0.0, -0.132, -0.5);
    let mut pos = hip.lerp(ads_pos, ads);
    let mut pitch = 0.0f32;
    let mut yaw = 0.0f32;
    let mut roll = 0.0f32;
    // bob & sway
    let bob_k = g.walk_amount * (1.0 - ads * 0.85) * g.tuning.movement.view_bob;
    pos += vec3(
        g.bob_phase.sin() * 0.012,
        -(g.bob_phase.cos().abs()) * 0.016,
        0.0,
    ) * bob_k;
    pos += vec3(-g.sway.x, g.sway.y, 0.0) * (1.0 - ads * 0.7);
    // recoil
    pos.z += g.recoil_vis * 0.035;
    pitch += g.recoil_vis * 0.06;
    // sprint
    if g.sprinting {
        pos += vec3(-0.04, -0.06, 0.04);
        yaw += 0.7;
        roll += 0.25;
        pitch -= 0.15;
    }
    // reload
    if let Some((end, dur)) = g.reload_until {
        let t = (1.0 - ((end - now) / dur) as f32).clamp(0.0, 1.0);
        let k = (t * std::f32::consts::PI).sin();
        pitch -= k * 0.7;
        roll += k * 0.5;
        pos.y -= k * 0.08;
    }
    // weapon switch
    if now < g.switch_until {
        let k = ((g.switch_until - now) as f32 / g.switch_dur.max(0.05)).clamp(0.0, 1.0);
        pos.y -= k * 0.35;
        pitch -= k * 0.8;
    }
    // knife / grenade: lower the gun
    let knife = now < g.melee_until;
    let throwing = g.cooking.is_some() || now < g.throw_until;
    if knife || throwing {
        pos.y -= 0.25;
        pitch -= 0.6;
    }
    let base = trs(pos, yaw, pitch, roll) * Mat4::from_scale(Vec3::splat(0.85));
    let mut m = ToonMesh::new();
    let ow = if settings.outlines { 0.004 } else { 0.0 };
    for (c, h, col) in models::gun_parts(w) {
        m.boxed(&(base * Mat4::from_translation(c)), h, col, ow);
    }
    // arms
    let sleeve = models::team_look(g.team).shirt;
    let grip = base.transform_point3(vec3(0.0, -0.03, 0.02));
    let fore = base.transform_point3(if def.class == WeaponClass::Pistol {
        vec3(-0.02, -0.03, 0.03)
    } else {
        vec3(0.0, 0.02, -0.36)
    });
    for (hand, shoulder) in [
        (grip, vec3(0.38, -0.55, -0.1)),
        (fore, vec3(-0.3, -0.6, -0.2)),
    ] {
        let mid = (hand + shoulder) * 0.5;
        let d = hand - shoulder;
        let rot = Quat::from_rotation_arc(Vec3::NEG_Z, d.normalize());
        m.boxed(
            &Mat4::from_rotation_translation(rot, mid),
            vec3(0.036, 0.036, d.length() * 0.5),
            sleeve,
            ow,
        );
        m.sphere(&Mat4::from_translation(hand), 0.04, [240, 236, 228], ow);
    }
    // knife swing
    if knife {
        let t = 1.0 - ((g.melee_until - now) / 0.45) as f32;
        let k = (t * std::f32::consts::PI).sin();
        let kb = trs(
            vec3(0.12 - k * 0.1, -0.12, -0.25 - k * 0.25),
            -0.3 + k * 0.4,
            0.0,
            0.0,
        );
        m.boxed(&kb, vec3(0.012, 0.025, 0.12), [220, 225, 235], ow);
        m.boxed(
            &(kb * Mat4::from_translation(vec3(0.0, 0.0, 0.14))),
            vec3(0.018, 0.03, 0.05),
            [60, 50, 40],
            ow,
        );
        m.sphere(
            &(kb * Mat4::from_translation(vec3(0.0, 0.0, 0.16))),
            0.045,
            [240, 236, 228],
            ow,
        );
    }
    // grenade in hand
    if throwing {
        let gpos = vec3(0.16, -0.15, -0.3);
        m.sphere(&Mat4::from_translation(gpos), 0.05, [70, 110, 60], ow);
        m.sphere(
            &Mat4::from_translation(gpos + vec3(0.02, -0.03, 0.03)),
            0.045,
            [240, 236, 228],
            ow,
        );
    }
    mats.draw_toon(&m);
    // muzzle flash
    if now - g.muzzle_flash < 0.05 {
        let mp = base.transform_point3(models::muzzle(w));
        let mut f = crate::mesh::MeshBuilder::new();
        let s = 0.06 + def.heft * 0.05;
        for (a, b) in [
            (Vec3::X, Vec3::Y),
            (Vec3::Y, Vec3::NEG_X),
            (Vec3::NEG_X, Vec3::NEG_Y),
            (Vec3::NEG_Y, Vec3::X),
        ] {
            f.add_quad(
                [
                    mp,
                    mp + (a + b) * s * 0.4,
                    mp + a * s * 1.8,
                    mp + (a - b) * s * 0.4,
                ],
                [255, 220, 110, 230],
            );
        }
        f.add_sphere_ex(
            &Mat4::from_translation(mp),
            s * 0.6,
            [255, 250, 200],
            6,
            4,
            220,
            true,
        );
        gl_use_material(&mats.fx);
        f.draw();
        gl_use_default_material();
    }
    let _ = Stance::Stand;
}
