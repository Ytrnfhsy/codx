//! 3D rendering of a frame: sky, level, characters, entities, effects and the first-person weapon.

use crate::game::GameClient;
use crate::mesh::{trs, Materials, ToonMesh};
use crate::models::{self, SoldierPose};
use crate::settings::Settings;
use macroquad::prelude::*;
use toon_shared::look_dir;
use toon_shared::physics::Stance;
use toon_shared::protocol::{flags, EntSnap, MatchState};
use toon_shared::weapons::{WeaponClass, WeaponId};
use toon_shared::Team;

/// A living soldier to draw (from live interpolation or from a killcam replay).
#[derive(Clone)]
pub struct Actor {
    pub id: u8,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub stance: Stance,
    pub team: Team,
    pub weapon: WeaponId,
    pub flags: u8,
    pub phase: f32,
    pub walk: f32,
    /// Melee swing progress 0..1.
    pub melee: f32,
    /// Seconds into a reload.
    pub reload: Option<f32>,
    /// Spawn protection bubble (shown to teammates only).
    pub protected: bool,
}

/// Everything the first-person weapon animation needs.
pub struct VmState {
    pub weapon: WeaponId,
    pub team: Team,
    pub ads: f32,
    pub bob_phase: f32,
    /// Bob amount (walk speed × view bob setting).
    pub bob: f32,
    pub sway: Vec2,
    pub recoil: f32,
    pub sprint: bool,
    /// Reload progress 0..1.
    pub reload: Option<f32>,
    /// Weapon being raised: 1 = fully down, 0 = ready.
    pub raise: f32,
    /// Knife swing progress 0..1.
    pub knife: Option<f32>,
    pub throwing: bool,
    pub flash: bool,
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

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
    let (weapon, ads, scoped, first_person) = match &g.killcam {
        Some(k) => (k.weapon, k.ads_t, k.scoped(), true),
        None => (g.weapon(), g.ads_t, g.scoped(), g.alive),
    };
    let zoom = 1.0 + (g.tuning.weapon(weapon).ads_zoom - 1.0) * ads;
    let zoom = if weapon.def().scoped && !scoped {
        1.0 + (zoom - 1.0) * 0.25
    } else {
        zoom
    };
    if first_person {
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
    if settings.shadows
        && g.shadow
            .as_ref()
            .is_none_or(|s| s.res != shadow_res(settings))
    {
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for b in g.map.boxes.iter().filter(|b| b.visible) {
            lo = lo.min(b.min);
            hi = hi.max(b.max);
        }
        lo.y = lo.y.max(-1.0);
        g.shadow = Some(crate::mesh::ShadowMap::render(
            mats,
            &g.map_mesh,
            lo,
            hi,
            shadow_res(settings),
        ));
    }
    mats.set_lighting(&crate::mesh::Lighting {
        shadow: g.shadow.as_ref().filter(|_| settings.shadows),
        textures: settings.textures,
        tint: Vec3::ONE,
    });
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
    let kc = g.killcam.as_ref();
    let actors: Vec<Actor> = match kc {
        Some(k) => k.actors.clone(),
        None => g.live_actors(),
    };
    let mut muzzles = Vec::with_capacity(actors.len());
    for a in &actors {
        let has_bomb = a.flags & flags::HAS_BOMB != 0;
        let pose = SoldierPose {
            pos: a.pos,
            yaw: a.yaw,
            pitch: a.pitch,
            stance: a.stance,
            team: a.team,
            weapon: a.weapon,
            phase: a.phase,
            walk: a.walk,
            sprint: a.flags & flags::SPRINT != 0,
            dead: None,
            has_bomb,
            blink: ((now * 0.37 + a.id as f64 * 0.31).fract()) < 0.035,
            melee: a.melee,
            reload: a.reload,
        };
        let muzzle = models::add_soldier(&mut m, &pose);

        let floor = g
            .world
            .floor_below(a.pos + vec3(0.0, 0.3, 0.0), 3.0)
            .unwrap_or(a.pos.y);
        blob_shadow(&mut shadows, vec3(a.pos.x, floor + 0.03, a.pos.z), 0.5);
        if a.protected && a.team == my_team && kc.is_none() {
            // spawn protection: a soft pulsing bubble in team colour
            let c = crate::models::team_look(a.team).helmet;
            let pulse = 0.5 + 0.5 * ((now * 5.0) as f32).sin();
            let centre = a.pos + vec3(0.0, a.stance.height() * 0.5, 0.0);
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
        if kc.is_none() {
            let head = a.pos + vec3(0.0, a.stance.height() + 0.45, 0.0);
            let friendly = a.team == my_team;
            let name = g.name_of(a.id).unwrap_or("?").to_string();
            tags.push(NameTag {
                pos: head,
                name,
                friendly,
                bomb: has_bomb,
            });
        }
        muzzles.push((a.id, muzzle));
    }
    // ragdolls
    let ragdolls = match kc {
        Some(k) => &k.ragdolls,
        None => &g.ragdolls,
    };
    for (_, r) in ragdolls {
        r.draw(&mut m, true, now as f32);

        let c = r.centre();
        if let Some(f) = g.world.floor_below(c + vec3(0.0, 0.2, 0.0), 2.5) {
            blob_shadow(&mut shadows, vec3(c.x, f + 0.03, c.z), 0.7);
        }
    }
    let ents = kc.map(|k| &k.ents).unwrap_or(&g.ents);
    let bomb = match kc {
        Some(k) => k.bomb.as_ref(),
        None => g.bomb.as_ref(),
    };
    // entities
    for e in ents {
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
            EntSnap::Weapon {
                id,
                weapon,
                pos,
                yaw,
            } => {
                // freshly dropped guns fall out of the hands first
                let age = match kc {
                    Some(_) => 1.0,
                    None => g
                        .pickup_seen
                        .get(id)
                        .map(|t| (now - t) as f32)
                        .unwrap_or(1.0),
                };
                let target = g.pickup_target.map(|t| t.0) == Some(*id) && kc.is_none();
                models::add_dropped_gun(&mut m, Vec3::from_array(*pos), *yaw, *weapon, age, target);
            }
        }
    }
    // airstrike jets (purely visual)
    let jets = if kc.is_some() {
        &[][..]
    } else {
        &g.airstrikes[..]
    };
    for (from, to, t0) in jets {
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
    if let Some(b) = bomb {
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
    let fx = kc.map(|k| &k.fx).unwrap_or(&g.fx);
    fx.build(&mut shadows, view.pos);
    gl_use_material(&mats.fx);
    shadows.draw();
    gl_use_default_material();
    fx.draw_tracers();

    if g.killcam.is_none() {
        for (id, mz) in muzzles {
            if let Some(r) = g.remotes.get_mut(&id) {
                r.muzzle = mz;
            }
        }
    }
    g.frame_mesh = m;
    g.fx_mesh = shadows;
    tags
}

fn shadow_res(settings: &Settings) -> u32 {
    if settings.render_scale < 0.75 {
        1024
    } else {
        2048
    }
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
    let vm = match &g.killcam {
        Some(k) if !k.scoped() => k.vm_state(&g.tuning),
        Some(_) => return,
        None if g.alive && !g.scoped() => g.vm_state(),
        None => return,
    };
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
    // the gun takes the shade of where the player stands
    let eye = g.camera().0;
    let in_shade = settings.shadows
        && !g
            .world
            .line_of_sight(eye, eye + crate::mesh::LIGHT_DIR.normalize() * 120.0);
    mats.set_lighting(&crate::mesh::Lighting {
        shadow: None,
        textures: false,
        tint: if in_shade {
            vec3(0.74, 0.77, 0.9)
        } else {
            Vec3::ONE
        },
    });
    set_camera(&cam);
    mats.set_view(Vec3::ZERO, Color::new(0.0, 0.0, 0.0, 1.0), 100.0, 200.0);
    draw_viewmodel(&vm, mats, settings);
    mats.set_lighting(&crate::mesh::Lighting {
        shadow: None,
        textures: false,
        tint: Vec3::ONE,
    });
}

fn draw_viewmodel(vm: &VmState, mats: &Materials, settings: &Settings) {
    use std::f32::consts::PI;
    let w = vm.weapon;
    let def = w.def();
    let ads = vm.ads;
    let hip = vec3(0.26, -0.27, -0.66);
    let ads_pos = vec3(0.0, -0.132, -0.5);
    let mut pos = hip.lerp(ads_pos, ads);
    let mut pitch = 0.0f32;
    let mut yaw = 0.0f32;
    let mut roll = 0.0f32;
    // bob & sway
    pos += vec3(
        vm.bob_phase.sin() * 0.012,
        -(vm.bob_phase.cos().abs()) * 0.016,
        0.0,
    ) * vm.bob;
    pos += vec3(-vm.sway.x, vm.sway.y, 0.0) * (1.0 - ads * 0.7);
    // recoil: kick back and up, with a little random-looking roll
    pos.z += vm.recoil * 0.035;
    pitch += vm.recoil * 0.06;
    roll += (vm.recoil * 9.0).sin() * vm.recoil * 0.02;
    // sprint: gun across the chest, bouncing with the stride
    if vm.sprint {
        pos += vec3(-0.04, -0.06, 0.04);
        pos += vec3(
            vm.bob_phase.sin() * 0.025,
            -(vm.bob_phase.cos().abs()) * 0.03,
            0.0,
        );
        yaw += 0.7;
        roll += 0.25 + vm.bob_phase.sin() * 0.05;
        pitch -= 0.15;
    }
    // reload: tilt the gun, pull the magazine, slap a new one in, rack the bolt
    let mut mag_off = Vec3::ZERO;
    let mut hand_to_mag = 0.0f32;
    let mut pump = 0.0f32;
    let shotgun = def.class == WeaponClass::Shotgun;
    if let Some(t) = vm.reload {
        let tilt = smooth(t / 0.12) * (1.0 - smooth((t - 0.86) / 0.14));
        pitch += tilt * 0.22;
        roll += tilt * 0.55;
        yaw += tilt * 0.12;
        pos += vec3(-0.05, -0.03, 0.03) * tilt;
        if shotgun {
            // feed shells one by one into the loading port, then pump
            if (0.08..0.82).contains(&t) {
                let ph = ((t - 0.08) / 0.74 * 4.0).fract();
                hand_to_mag = (ph * PI).sin();
            }
            if t > 0.84 {
                pump = ((t - 0.84) / 0.14 * PI).sin().max(0.0);
            }
        } else {
            let out = smooth((t - 0.14) / 0.18);
            let back_in = smooth((t - 0.44) / 0.2);
            let drop = out * (1.0 - back_in);
            mag_off = vec3(0.02 * drop, -0.5 * drop, 0.1 * drop);
            hand_to_mag = smooth((t - 0.08) / 0.1) * (1.0 - smooth((t - 0.66) / 0.1));
            // magazine seats with a jolt
            if (0.64..0.74).contains(&t) {
                let k = ((t - 0.64) / 0.1 * PI).sin();
                pos.y += 0.014 * k;
                pitch += 0.04 * k;
            }
            // charging handle / bolt: the gun kicks back
            if (0.78..0.92).contains(&t) {
                let k = ((t - 0.78) / 0.14 * PI).sin();
                pos.z += 0.035 * k;
                roll -= 0.08 * k;
            }
        }
    }
    // weapon raise / switch
    if vm.raise > 0.0 {
        let k = smooth(vm.raise);
        pos.y -= k * 0.35;
        pitch -= k * 0.8;
        roll += k * 0.3;
    }
    // knife / grenade: lower the gun out of the way
    let knife = vm.knife;
    if knife.is_some() || vm.throwing {
        let k = knife
            .map(|t| (t * PI).sin().min(1.0) * 1.4)
            .unwrap_or(1.0)
            .min(1.0);
        pos.y -= 0.25 * k;
        pos.x -= 0.08 * k;
        pitch -= 0.6 * k;
    }
    let base = trs(pos, yaw, pitch, roll) * Mat4::from_scale(Vec3::splat(0.85));
    let mut m = ToonMesh::new();
    let ow = if settings.outlines { 0.004 } else { 0.0 };
    for p in models::gun_parts(w) {
        let c = if p.mag { p.c + mag_off } else { p.c };
        m.boxed(&(base * Mat4::from_translation(c)), p.h, p.col, ow);
    }
    // shotgun pump slides back
    if pump > 0.0 {
        m.boxed(
            &(base * Mat4::from_translation(vec3(0.0, 0.035, -0.38 + pump * 0.1))),
            vec3(0.034, 0.03, 0.1),
            [170, 104, 52],
            ow,
        );
    }
    // arms
    let sleeve = models::team_look(vm.team).shirt;
    let grip = base.transform_point3(vec3(0.0, -0.03, 0.02));
    let fore_rest = if def.class == WeaponClass::Pistol {
        vec3(-0.02, -0.03, 0.03)
    } else {
        vec3(0.0, 0.02, -0.36 + pump * 0.1)
    };
    let mag_spot = if shotgun {
        vec3(0.02, 0.0, -0.12)
    } else {
        models::gun_mag(w)
            .map(|g| g.0 + mag_off - vec3(0.0, 0.06, 0.0))
            .unwrap_or(fore_rest)
    };
    let fore = base.transform_point3(fore_rest.lerp(mag_spot, hand_to_mag));
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
    // a shell in the off hand while loading the shotgun
    if shotgun && hand_to_mag > 0.05 {
        m.boxed(
            &(Mat4::from_translation(fore + vec3(0.0, 0.03, 0.0))),
            vec3(0.012, 0.012, 0.03),
            [220, 50, 40],
            ow,
        );
    }
    // knife slash: a sweeping arc from right to left
    let mut swoosh = crate::mesh::MeshBuilder::new();
    if let Some(t) = knife {
        let knife_at = |t: f32| {
            let e = smooth(t * 1.15);
            let s = (e * PI).sin();
            (
                vec3(0.32 - 0.6 * e, -0.06 - 0.06 * s, -0.32 - 0.18 * s),
                0.9 - 1.8 * e,
            )
        };
        let (kp, ky) = knife_at(t);
        let kb = trs(kp, ky, -0.15, -0.5);
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
        // arm to the knife hand
        let hand = kb.transform_point3(vec3(0.0, 0.0, 0.16));
        let shoulder = vec3(0.38, -0.55, -0.1);
        let d = hand - shoulder;
        m.boxed(
            &Mat4::from_rotation_translation(
                Quat::from_rotation_arc(Vec3::NEG_Z, d.normalize()),
                (hand + shoulder) * 0.5,
            ),
            vec3(0.036, 0.036, d.length() * 0.5),
            sleeve,
            ow,
        );
        // motion trail
        if (0.1..0.75).contains(&t) {
            let n = 8;
            for i in 0..n {
                let a = t - 0.2 * (i as f32 / n as f32);
                let b = t - 0.2 * ((i + 1) as f32 / n as f32);
                let (pa, ya) = knife_at(a.max(0.0));
                let (pb, yb) = knife_at(b.max(0.0));
                let tip = |p: Vec3, y: f32| {
                    trs(p, y, -0.15, -0.5).transform_point3(vec3(0.0, 0.0, -0.12))
                };
                let alpha = (150.0 * (1.0 - i as f32 / n as f32)) as u8;
                swoosh.add_quad([pa, tip(pa, ya), tip(pb, yb), pb], [255, 255, 255, alpha]);
            }
        }
    }
    // grenade in hand
    if vm.throwing {
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
    let mut f = swoosh;
    if vm.flash {
        let mp = base.transform_point3(models::muzzle(w));
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
    }
    gl_use_material(&mats.fx);
    f.draw();
    gl_use_default_material();
}
