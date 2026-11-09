//! 3D rendering of a frame: sky, level, characters, entities, effects and the first-person weapon.

use crate::game::GameClient;
use crate::mesh::{trs, Materials, ToonMesh};
use crate::models;
use crate::settings::Settings;
use crate::soldier::{self, SoldierPose};
use macroquad::prelude::*;
use toon_shared::look_dir;
use toon_shared::physics::Stance;
use toon_shared::protocol::{flags, EntSnap, MatchState};
use toon_shared::vehicles::VehicleKind;
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
    /// 0..1 how far into the sprint pose (smoothed).
    pub sprint: f32,
    /// Melee swing progress 0..1.
    pub melee: f32,
    /// Seconds into a reload.
    pub reload: Option<f32>,
    /// Spawn protection bubble (shown to teammates only).
    pub protected: bool,
    /// Skydiving: 0 no, 1 free fall, 2 under the parachute.
    pub chute: u8,
}

/// Everything the first-person weapon animation needs.
pub struct VmState {
    pub weapon: WeaponId,
    /// Sleeve and hand colours.
    pub sleeve: [u8; 3],
    pub hand: [u8; 3],
    pub ads: f32,
    pub bob_phase: f32,
    /// Bob amount (walk speed × view bob setting).
    pub bob: f32,
    pub sway: Vec2,
    pub recoil: f32,
    /// 0..1 into the sprint pose.
    pub sprint: f32,
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
    let pilot_fov = g
        .pilot_camera()
        .and_then(|c| c.3)
        .filter(|_| g.killcam.is_none() && g.alive);
    let hfov = pilot_fov
        .unwrap_or(settings.fov)
        .clamp(60.0, 120.0)
        .to_radians();
    let mut fovy = 2.0 * ((hfov * 0.5).tan() / aspect).atan();
    let (weapon, ads, scoped, first_person) = match &g.killcam {
        Some(k) => (k.weapon, k.ads_t, k.scoped(), true),
        None => (
            g.weapon(),
            g.ads_t,
            g.scoped(),
            g.alive && g.piloting.is_none(),
        ),
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
    // the FPV camera banks with the drone
    let roll = match (&g.piloting, &g.killcam) {
        (Some(p), None) if g.alive && p.kind == VehicleKind::Fpv => p.roll,
        _ => 0.0,
    };
    // the near plane as far out as it can be without the screen's corners dipping into a
    // wall the player stands against (the body is 0.35 m round the eye): fewer depth bits
    // (some GPUs, multisampled windows) then still keep far surfaces from fighting
    let tv = (fovy * 0.5).tan();
    let diag = (tv * tv * (1.0 + aspect * aspect)).sqrt();
    let z_near = (0.32 / (1.0 + diag * diag).sqrt()).clamp(0.1, 0.3);
    let cam = Camera3D {
        position: pos,
        target: pos + dir,
        up: Quat::from_axis_angle(dir, roll) * Vec3::Y,
        fovy,
        aspect: None,
        projection: Projection::Perspective,
        render_target: target,
        viewport: None,
        // a near plane this far keeps the depth buffer precise at range (no shimmering)
        z_near,
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
    let (fog_near, fog_far) = if g.level.terrain {
        crate::level::TERRAIN_FOG
    } else {
        crate::level::ARENA_FOG
    };
    if settings.shadows {
        // big maps: the shadow map covers the area around the camera and follows it
        let (lo, hi) = if g.level.terrain {
            // a smaller area means finer shadows (and no striped walls)
            let snap = 32.0;
            let c = vec2(
                (view.pos.x / snap).round() * snap,
                (view.pos.z / snap).round() * snap,
            );
            let r = 112.0;
            (
                vec3(c.x - r, g.level.min.y, c.y - r),
                vec3(c.x + r, g.level.max.y, c.y + r),
            )
        } else {
            (g.level.min, g.level.max)
        };
        let centre = vec2((lo.x + hi.x) * 0.5, (lo.z + hi.z) * 0.5);
        if g.shadow
            .as_ref()
            .is_none_or(|s| s.res != shadow_res(settings) || (s.centre - centre).length() > 1.0)
        {
            let casters = g.level.casters(lo, hi);
            g.shadow = Some(crate::mesh::ShadowMap::render(
                mats,
                &casters,
                lo,
                hi,
                shadow_res(settings),
            ));
        }
    }
    mats.set_lighting(&crate::mesh::Lighting {
        shadow: g.shadow.as_ref().filter(|_| settings.shadows),
        textures: settings.textures,
        // every map has its own light: warm evening, dusty noon, grey morning…
        tint: Vec3::from_array(g.map.tint),
    });
    set_camera(&view.cam);
    clear_background(fog);
    mats.set_view(view.pos, fog, fog_near, fog_far);

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
    mats.set_view(view.pos, fog, fog_near, fog_far);

    // far ground outside the playable area (the sea around an island)
    if g.level.terrain {
        let y = g.map.terrain.as_ref().map(|t| t.water).unwrap_or(0.0) - 0.05;
        let mut far = crate::mesh::MeshBuilder::new();
        // a frame round the terrain (inside it the tiles have their own water)
        let s = 3000.0;
        let (a, b) = (g.level.min, g.level.max);
        let sea = [70, 132, 176, 255];
        for (x0, z0, x1, z1) in [
            (-s, -s, s, a.z),
            (-s, b.z, s, s),
            (-s, a.z, a.x, b.z),
            (b.x, a.z, s, b.z),
        ] {
            far.add_quad(
                [
                    vec3(x0, y, z0),
                    vec3(x0, y, z1),
                    vec3(x1, y, z1),
                    vec3(x1, y, z0),
                ],
                sea,
            );
        }
        gl_use_material(&mats.world);
        far.draw();
        gl_use_default_material();
    } else {
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
    // outlines about a pixel wide whatever the distance (in this frame's resolution)
    let px = 2.0 * (view.fovy * 0.5).tan()
        / (screen_height() * settings.render_scale.clamp(0.4, 1.0)).max(1.0);
    let grow = if settings.outlines { px * 0.9 } else { 0.0 };
    g.level.draw(mats, view.pos, &view.matrix, grow);

    // dynamic objects
    let mut m = std::mem::take(&mut g.frame_mesh);
    m.clear();
    let mut shadows = std::mem::take(&mut g.fx_mesh);
    shadows.clear();
    let mut tags = Vec::new();
    let now = g.now;
    // in the battle royale nobody is on your side
    let my_team = if g.mode.solo() {
        Team::Spectator
    } else {
        g.team
    };
    let kc = g.killcam.as_ref();
    let ents = kc.map(|k| &k.ents).unwrap_or(&g.ents);
    let bomb = match kc {
        Some(k) => k.bomb.as_ref(),
        None => g.bomb.as_ref(),
    };
    // entities
    for e in ents {
        match e {
            EntSnap::Grenade { id, pos, smoke } => {
                let t = now as f32 + *id as f32;
                if *smoke {
                    models::add_smoke_grenade(&mut m, Vec3::from_array(*pos), t)
                } else {
                    models::add_grenade(&mut m, Vec3::from_array(*pos), t)
                }
            }
            EntSnap::Vehicle {
                id,
                kind,
                team,
                pos,
                yaw,
                pitch,
                auto,
                ..
            } => {
                // our own vehicle is drawn where we steer it (below), not where the
                // server last saw it
                if kc.is_none() && g.piloting.as_ref().is_some_and(|p| p.id == *id) {
                    continue;
                }
                let at = Vec3::from_array(*pos);
                match kind {
                    VehicleKind::Fpv => {
                        models::add_fpv(&mut m, at, *yaw, *pitch, *team, now as f32)
                    }
                    VehicleKind::Bomber => {
                        models::add_bomber(&mut m, at, *yaw, *team, now as f32, 6)
                    }
                    VehicleKind::Nrk => {
                        models::add_nrk(&mut m, at, *yaw, *pitch, *team, now as f32, *auto)
                    }
                }
            }
            EntSnap::Drop { id, pos } => {
                models::add_drop(&mut m, Vec3::from_array(*pos), now as f32 + *id as f32)
            }
            EntSnap::Smoke { id, pos, age } => {
                models::add_smoke_cloud(&mut m, Vec3::from_array(*pos), *age, *id, now as f32)
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
            EntSnap::Loot { id, kind, pos } => models::add_loot(
                &mut m,
                Vec3::from_array(*pos),
                *kind,
                now as f32 + *id as f32,
            ),
        }
    }
    // the battle royale plane
    if let Some((p, yaw)) = g.br.as_ref().and_then(|b| b.plane) {
        models::add_transport(&mut m, Vec3::from_array(p), yaw, now as f32);
    }
    // our own vehicle; the bomber also marks where a bomb would land
    if let (Some(p), None) = (&g.piloting, kc) {
        match p.kind {
            VehicleKind::Nrk => {
                models::add_nrk(&mut m, p.pos, p.yaw, p.pitch, g.team, now as f32, false)
            }
            VehicleKind::Bomber => {
                let at = p.bomb_landing(g);
                let pulse = 1.0 + 0.15 * ((now * 5.0).sin() as f32);
                for k in 0..12 {
                    let a = k as f32 * std::f32::consts::TAU / 12.0;
                    let r = g.tuning.streaks.bomber_drop_radius * 0.5 * pulse;
                    let c = at + vec3(a.cos() * r, 0.08, a.sin() * r);
                    m.fill.add_box_ex(
                        &(Mat4::from_translation(c) * Mat4::from_rotation_y(-a)),
                        vec3(0.08, 0.04, 0.5),
                        [255, 80, 60],
                        255,
                        true,
                    );
                }
                m.fill.add_box_ex(
                    &Mat4::from_translation(at + vec3(0.0, 0.08, 0.0)),
                    vec3(0.25, 0.04, 0.25),
                    [255, 80, 60],
                    255,
                    true,
                );
            }
            VehicleKind::Fpv => {}
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
    // the black cartoon outlines can be turned off; characters always keep their
    // team-coloured silhouette
    if !settings.outlines {
        m.outline.clear();
    }
    let actors: Vec<Actor> = match kc {
        Some(k) => k.actors.clone(),
        None => g.live_actors(),
    };
    let mut muzzles = Vec::with_capacity(actors.len());
    let clouds: Vec<(Vec3, f32)> = ents
        .iter()
        .filter_map(|e| match e {
            EntSnap::Smoke { pos, age, .. } => Some((
                Vec3::from_array(*pos),
                toon_shared::consts::smoke_radius(*age),
            )),
            _ => None,
        })
        .collect();
    for a in &actors {
        let has_bomb = a.flags & flags::HAS_BOMB != 0;
        let pose = SoldierPose {
            pos: a.pos,
            yaw: a.yaw,
            pitch: a.pitch,
            stance: a.stance,
            team: a.team,
            weapon: a.weapon,
            look: a.id,
            time: now as f32,
            phase: a.phase,
            walk: a.walk,
            sprint: a.sprint,
            has_bomb,
            melee: a.melee,
            reload: a.reload,
            outline: soldier::outline_at(a.pos.distance(view.pos)),
        };
        let muzzle = soldier::add_soldier(&mut m, &pose);
        if a.chute == 2 {
            models::add_parachute(&mut m, a.pos, a.yaw, now as f32 + a.id as f32);
        }

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
        let head = a.pos + vec3(0.0, a.stance.height() + 0.45, 0.0);
        // no enemy names through smoke
        let hidden = a.team != my_team
            && toon_shared::ray::smoke_blocks(view.pos, head - vec3(0.0, 0.5, 0.0), &clouds);
        // battle royale: no names over anybody (they would give players away from afar)
        if kc.is_none() && !hidden && !g.mode.solo() {
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
    for (id, r) in ragdolls {
        r.draw(&mut m, *id, now as f32);

        let c = r.centre();
        if let Some(f) = g.world.floor_below(c + vec3(0.0, 0.2, 0.0), 2.5) {
            blob_shadow(&mut shadows, vec3(c.x, f + 0.03, c.z), 0.7);
        }
    }
    mats.draw_toon(&m);

    // rivers, lakes and the sea
    g.level.draw_water(mats, view.pos);
    // the battle royale zone: a shimmering wall round the safe circle
    if let Some(br) = g.br.as_ref().filter(|b| b.zone[2] > 0.0 && kc.is_none()) {
        let (c, r) = (vec2(br.zone[0], br.zone[1]), br.zone[2]);
        let mut wall = crate::mesh::MeshBuilder::new();
        let n = 96;
        let a = ((now * 0.6).sin() * 0.15 + 0.35) as f32;
        for k in 0..n {
            let a0 = k as f32 / n as f32 * std::f32::consts::TAU;
            let a1 = (k + 1) as f32 / n as f32 * std::f32::consts::TAU;
            let p0 = c + vec2(a0.cos(), a0.sin()) * r;
            let p1 = c + vec2(a1.cos(), a1.sin()) * r;
            // only the part that is not lost in the fog
            let mid = (p0 + p1) * 0.5;
            if mid.distance(vec2(view.pos.x, view.pos.z)) > crate::level::TERRAIN_FOG.1 + 60.0 {
                continue;
            }
            wall.add_quad(
                [
                    vec3(p0.x, -10.0, p0.y),
                    vec3(p0.x, 180.0, p0.y),
                    vec3(p1.x, 180.0, p1.y),
                    vec3(p1.x, -10.0, p1.y),
                ],
                [90, 150, 255, (a * 255.0) as u8],
            );
        }
        gl_use_material(&mats.fx);
        wall.draw();
        gl_use_default_material();
    }

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
        Some(k) if !k.scoped() => {
            let mut vm = k.vm_state(&g.tuning);
            (vm.sleeve, vm.hand) = g.vm_colors(k.info.killer, k.team);
            vm
        }
        Some(_) => return,
        None if g.alive && !g.scoped() && g.piloting.is_none() => g.vm_state(),
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
        tint: Vec3::from_array(g.map.tint)
            * if in_shade {
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
    // line the top of the sights up just under the screen centre
    let ads_pos = vec3(0.0, -models::sight_height(w) * 0.85 - 0.006, -0.5);
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
    if vm.sprint > 0.001 {
        let k = vm.sprint * vm.sprint * (3.0 - 2.0 * vm.sprint);
        pos += vec3(-0.04, -0.06, 0.04) * k;
        pos += vec3(
            vm.bob_phase.sin() * 0.025,
            -(vm.bob_phase.cos().abs()) * 0.03,
            0.0,
        ) * k;
        yaw += 0.7 * k;
        roll += (0.25 + vm.bob_phase.sin() * 0.05) * k;
        pitch -= 0.15 * k;
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
        let c = if p.mag {
            p.c + mag_off
        } else if p.pump {
            p.c + vec3(0.0, 0.0, pump * 0.1)
        } else {
            p.c
        };
        m.boxed(&(base * Mat4::from_translation(c)), p.h, p.col, ow);
    }
    // arms
    let sleeve = vm.sleeve;
    let grip = base.transform_point3(vec3(0.0, -0.03, 0.02));
    let fore_rest = models::fore_grip(w) + vec3(0.0, 0.0, pump * 0.1);
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
        m.sphere(&Mat4::from_translation(hand), 0.045, vm.hand, ow);
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

/// Developer view (`--model-gallery`): every gun and a few soldiers side by side.
pub fn draw_model_gallery(mats: &Materials) {
    clear_background(Color::from_rgba(236, 230, 214, 255));
    let cam = Camera3D {
        position: vec3(0.0, 2.2, -9.5),
        target: vec3(0.0, 1.0, 0.0),
        up: Vec3::Y,
        fovy: 0.62,
        projection: Projection::Perspective,
        z_near: 0.1,
        z_far: 60.0,
        ..Default::default()
    };
    set_camera(&cam);
    mats.set_lighting(&crate::mesh::Lighting {
        shadow: None,
        textures: false,
        tint: Vec3::ONE,
    });
    mats.set_view(cam.position, Color::new(0.0, 0.0, 0.0, 1.0), 100.0, 200.0);
    let mut m = ToonMesh::new();
    // guns: 3 rows of 5, seen from the side, barrels pointing right
    for (i, w) in WeaponId::all().enumerate() {
        let (col, row) = ((i % 5) as f32, (i / 5) as f32);
        let pos = vec3(4.4 - col * 2.2, 3.55 - row * 0.75, 2.0);
        let base =
            trs(pos, std::f32::consts::FRAC_PI_2, 0.0, 0.0) * Mat4::from_scale(Vec3::splat(1.7));
        models::add_gun(&mut m, &base, w, 0.012);
    }
    // soldiers of both teams
    let looks = [
        (
            Team::Allies,
            0u8,
            WeaponId::M4,
            Stance::Stand,
            vec3(2.7, 0.0, -1.5),
        ),
        (
            Team::Allies,
            5,
            WeaponId::P90,
            Stance::Crouch,
            vec3(0.9, 0.0, -1.5),
        ),
        (
            Team::Axis,
            2,
            WeaponId::Ak47,
            Stance::Stand,
            vec3(-0.9, 0.0, -1.5),
        ),
        (
            Team::Axis,
            9,
            WeaponId::M249,
            Stance::Stand,
            vec3(-2.7, 0.0, -1.5),
        ),
        (
            Team::Allies,
            6,
            WeaponId::M40,
            Stance::Prone,
            vec3(2.6, 0.0, -3.6),
        ),
    ];
    for (k, (team, look, weapon, stance, pos)) in looks.iter().enumerate() {
        let pose = SoldierPose {
            pos: *pos,
            yaw: if *stance == Stance::Prone {
                -1.3
            } else {
                -0.45
            },
            pitch: -0.15,
            stance: *stance,
            team: *team,
            weapon: *weapon,
            look: *look,
            time: 0.0,
            // one walks, one sprints
            phase: k as f32 * 0.9,
            walk: if (2..=3).contains(&k) { 1.0 } else { 0.0 },
            sprint: if k == 3 { 1.0 } else { 0.0 },
            has_bomb: k == 3,
            melee: 0.0,
            reload: None,
            outline: soldier::OUTLINE,
        };
        soldier::add_soldier(&mut m, &pose);
    }
    // a smoke cloud and a smoke grenade
    models::add_smoke_cloud(
        &mut m,
        vec3(-4.2, toon_shared::consts::SMOKE_LIFT, 3.5),
        6.0,
        1,
        0.0,
    );
    models::add_smoke_grenade(&mut m, vec3(-1.6, 0.15, -4.0), 0.3);
    // a fallen soldier: a ragdoll simulated for a moment on flat ground
    let mut flat = toon_shared::maps::build(toon_shared::map::MapId::Wreck);
    flat.boxes.retain(|b| b.max.y <= 0.01);
    let world = toon_shared::physics::World::from_map(&flat);
    let mut rng = toon_shared::rng::Rng::new(7);
    let blow = crate::ragdoll::DeathBlow {
        cause: toon_shared::weapons::Cause::Weapon(WeaponId::M4),
        headshot: false,
        from: Some(vec3(5.0, 1.0, -4.6)),
    };
    let mut rd = crate::ragdoll::Ragdoll::new(
        vec3(0.6, 0.0, -4.6),
        2.6,
        Stance::Stand,
        Vec3::ZERO,
        Team::Axis,
        blow,
        &mut rng,
    );
    for _ in 0..90 {
        rd.update(&world, 1.0 / 60.0);
    }
    rd.draw(&mut m, 3, 0.0);
    mats.draw_toon(&m);
    set_default_camera();
}

/// A character turning slowly inside `rect` (screen pixels): the chooser's preview.
pub fn draw_skin_preview(mats: &Materials, skin: u8, rect: Rect, t: f32) {
    let dpi = screen_dpi_scale();
    let vp = (
        (rect.x * dpi) as i32,
        ((screen_height() - rect.y - rect.h) * dpi) as i32,
        (rect.w * dpi) as i32,
        (rect.h * dpi) as i32,
    );
    let cam = Camera3D {
        position: vec3(0.0, 1.2, -4.8),
        target: vec3(0.0, 0.92, 0.0),
        up: Vec3::Y,
        fovy: 0.55,
        aspect: Some(rect.w / rect.h.max(1.0)),
        projection: Projection::Perspective,
        render_target: None,
        viewport: Some(vp),
        z_near: 0.1,
        z_far: 20.0,
    };
    set_camera(&cam);
    mats.set_lighting(&crate::mesh::Lighting {
        shadow: None,
        textures: false,
        tint: Vec3::ONE,
    });
    mats.set_view(cam.position, Color::new(0.0, 0.0, 0.0, 1.0), 100.0, 200.0);
    // a spare player id carries the chosen character
    const PREVIEW: u8 = 255;
    soldier::set_skin(PREVIEW, skin);
    let mut m = ToonMesh::new();
    soldier::add_soldier(
        &mut m,
        &soldier::SoldierPose {
            pos: Vec3::ZERO,
            yaw: std::f32::consts::PI + t * 0.7,
            pitch: 0.0,
            stance: Stance::Stand,
            team: Team::Allies,
            weapon: WeaponId::M4,
            look: PREVIEW,
            time: t,
            phase: 0.0,
            walk: 0.0,
            sprint: 0.0,
            has_bomb: false,
            melee: 0.0,
            reload: None,
            outline: soldier::OUTLINE,
        },
    );
    mats.draw_toon(&m);
    set_default_camera();
}
