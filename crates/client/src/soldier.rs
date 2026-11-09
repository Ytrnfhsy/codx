//! Third-person soldiers: an animated character (see `character.rs`) with the aim, the
//! stance and a gun held in both hands on top of the walk/run cycle.

use crate::character::{clip, rig, Rig, Xf, SCALE};
use crate::mesh::{trs, ToonMesh};
use crate::models::{add_gun_ex, fore_grip, gun_mag, muzzle, team_ink};
use macroquad::prelude::*;
use std::f32::consts::{PI, TAU};
use toon_shared::physics::Stance;
use toon_shared::weapons::WeaponId;
use toon_shared::Team;

/// Hips go down this much when crouching (metres).
pub const CROUCH_DROP: f32 = 0.2;
/// Gait cycles per walk-phase cycle (the phase turns 2.2 rad per metre). The chibi legs
/// are short, so matching their stride exactly would play the clips several times too
/// fast; the feet slide a little instead and the steps stay calm.
const GAIT_CYCLES: f32 = 0.75;
/// How much of the clips' up-and-down body bob is kept.
const BOB: f32 = 0.5;
/// How much of the stride's shoulder sway the gun ignores (the arms absorb it).
const STEADY: f32 = 0.75;
/// Silhouette width up close (metres).
pub const OUTLINE: f32 = 0.022;

/// Silhouette width for a soldier `dist` metres away: it widens with distance so the
/// team colour stays readable far away.
pub fn outline_at(dist: f32) -> f32 {
    (OUTLINE + dist * 0.0011).min(0.07)
}

const TONES: [[u8; 3]; 6] = [
    [255, 214, 178],
    [246, 196, 150],
    [222, 168, 120],
    [192, 138, 96],
    [150, 102, 70],
    [112, 76, 54],
];

pub struct SoldierPose {
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    pub stance: Stance,
    pub team: Team,
    pub weapon: WeaponId,
    /// Picks the model and skin tone (the player id).
    pub look: u8,
    /// Clock for the idle breathing (seconds).
    pub time: f32,
    /// Walk cycle phase (radians).
    pub phase: f32,
    /// 0..1 amount of walking.
    pub walk: f32,
    /// 0..1 how far into the sprint pose (gun lowered across the body).
    pub sprint: f32,
    pub has_bomb: bool,
    /// Third-person melee swing 0..1.
    pub melee: f32,
    /// Seconds into a reload (None = not reloading).
    pub reload: Option<f32>,
    /// Team silhouette width (metres).
    pub outline: f32,
}

thread_local! {
    /// Battle royale character of every player id (0: team uniform), from the snapshots.
    static SKIN_OF: std::cell::RefCell<[u8; 256]> = const { std::cell::RefCell::new([0; 256]) };
}

/// Remembers which character player `id` is (`toon_shared::skins`).
pub fn set_skin(id: u8, skin: u8) {
    SKIN_OF.with(|s| s.borrow_mut()[id as usize] = skin);
}

pub fn skin_of(id: u8) -> u8 {
    SKIN_OF.with(|s| s.borrow()[id as usize])
}

/// Model index of a skin's character (None for the uniform or if the pack lacks it).
pub fn skin_model(skin: u8) -> Option<usize> {
    let name = toon_shared::skins::model(skin)?;
    let r = rig();
    (0..r.models.len()).find(|&i| r.models[i].name == name)
}

/// Model index for a team and look (the player id): each team has its own uniforms,
/// unless the player wears a battle royale character.
pub fn model_for(team: Team, look: u8) -> usize {
    if let Some(m) = skin_model(skin_of(look)) {
        return m;
    }
    let r = rig();
    let prefix = match team {
        Team::Axis => "Soldier_",
        _ => "BlueSoldier_",
    };
    let list: Vec<usize> = (0..r.models.len())
        .filter(|&i| r.models[i].name.starts_with(prefix))
        .collect();
    if list.is_empty() {
        return 0;
    }
    list[look as usize % list.len()]
}

/// Colours of every material of a model.
pub fn colors(model: usize, look: u8) -> Vec<[u8; 3]> {
    let md = &rig().models[model];
    md.mats
        .iter()
        .map(|(name, c)| match name.as_str() {
            // the pack leaves human skin black for us to tint (a goblin is green already)
            "Skin" if (c[0] as u32 + c[1] as u32 + c[2] as u32) < 120 => {
                TONES[(look as usize / 2) % TONES.len()]
            }
            // the pack's eyes are white on dark skin; ours are dark on light skin
            "Face" => [44, 36, 46],
            // the pack is quite dark; lift it to the game's bright cartoon palette
            _ => {
                let lift = |v: u8| (v as f32 * 1.35 + 34.0).min(255.0) as u8;
                [lift(c[0]), lift(c[1]), lift(c[2])]
            }
        })
        .collect()
}

/// Sleeve and hand colours of a soldier (first-person arms).
pub fn arm_colors(team: Team, look: u8) -> ([u8; 3], [u8; 3]) {
    let model = model_for(team, look);
    let md = &rig().models[model];
    let c = colors(model, look);
    // the uniform's sleeve, or a character's clothes
    let sleeve = ["Main", "Clothes", "Armor", "Shirt", "Pants"]
        .iter()
        .find_map(|n| md.mat(n))
        .map(|i| c[i])
        .unwrap_or([120, 120, 120]);
    let hand = md
        .mat("Skin")
        .map(|i| c[i])
        .unwrap_or(TONES[(look as usize / 2) % TONES.len()]);
    (sleeve, hand)
}

/// Turns a bone around the model's X axis (positive leans forward) and refreshes `g`.
fn bend(r: &Rig, local: &mut [Xf], g: &mut Vec<Xf>, bone: usize, angle: f32) {
    if angle != 0.0 {
        r.turn(local, g, bone, Quat::from_rotation_x(angle));
        *g = r.globals(local);
    }
}

/// Distance between two bones in the bind pose (model units).
fn bind_dist(r: &Rig, a: usize, b: usize) -> f32 {
    let p = |i: usize| r.bones[i].inv_bind.inverse().w_axis.truncate();
    p(a).distance(p(b))
}

/// Builds a soldier. Returns the world-space muzzle position.
pub fn add_soldier(m: &mut ToonMesh, p: &SoldierPose) -> Vec3 {
    let r = rig();
    let ids = &r.ids;
    let model = model_for(p.team, p.look);

    // walk/run cycle over the idle breathing
    let mut local = r.sample(clip::IDLE, p.time + p.look as f32 * 0.37, true);
    let idle = local.clone();
    let walk = p.walk.clamp(0.0, 1.0);
    let idle_body = local[ids.body].t;
    if walk > 0.01 && p.stance != Stance::Prone {
        // one gait clock for both clips, so walking blends smoothly into running
        let gait = (p.phase / TAU * GAIT_CYCLES).fract();
        let walk_pose = r.sample(clip::WALK, gait * r.clips[clip::WALK].dur, true);
        let run_pose = r.sample(clip::RUN, gait * r.clips[clip::RUN].dur, true);
        let run = if p.stance == Stance::Crouch {
            0.0
        } else {
            let k = ((p.walk - 0.5) / 0.4).clamp(0.0, 1.0);
            k * k * (3.0 - 2.0 * k)
        };
        for ((a, w), rn) in local.iter_mut().zip(&walk_pose).zip(&run_pose) {
            let t = w.t.lerp(rn.t, run);
            let q = w.r.slerp(rn.r, run);
            a.t = a.t.lerp(t, walk);
            a.r = a.r.slerp(q, walk);
        }
    }
    // soften the bob: a jumpy body makes the whole soldier look jittery
    let bob = local[ids.body].t.y - idle_body.y;
    local[ids.body].t.y = idle_body.y + bob * BOB;
    // how far the stride moves the shoulders away from standing still
    let mid = |g: &[Xf]| (g[ids.upper_arm[0]].t + g[ids.upper_arm[1]].t) * 0.5;
    let sway = mid(&r.globals(&local)) - mid(&r.globals(&idle));

    let mut root = trs(p.pos, p.yaw, 0.0, 0.0);
    let mut lean = 0.0;
    match p.stance {
        Stance::Crouch => {
            local[ids.body].t.y -= CROUCH_DROP / SCALE;
            lean = 0.35;
        }
        Stance::Prone => {
            root =
                root * Mat4::from_translation(vec3(0.0, 0.16, 0.75)) * Mat4::from_rotation_x(-1.5);
        }
        Stance::Stand => {}
    }
    let world = root * Mat4::from_rotation_y(PI) * Mat4::from_scale(Vec3::splat(SCALE));
    let to_model = world.inverse();
    let mut g = r.globals(&local);

    // knees bend so the feet stay on the ground
    if p.stance == Stance::Crouch {
        for s in 0..2 {
            let side = if s == 0 { 1.0 } else { -1.0 };
            let len = bind_dist(r, ids.lower_leg[s], ids.foot[s]);
            let target = g[ids.foot[s]].t;
            r.reach(
                &mut local,
                &mut g,
                ids.upper_leg[s],
                ids.lower_leg[s],
                len,
                target,
                vec3(side * 0.3, 0.0, 1.0),
            );
        }
    }

    // aim: the spine and head follow the pitch
    let aim_pitch = p.pitch + (-0.7 - p.pitch) * p.sprint;
    let (reload_k, mag_drop) = match p.reload {
        Some(t) => {
            let k = (t / 0.25).min(1.0);
            let cyc = (t * 2.6).sin().abs();
            (k, cyc * 0.22)
        }
        None => (0.0, 0.0),
    };
    let aim_pitch = aim_pitch * (1.0 - reload_k) - 0.35 * reload_k;
    // lying down the body is already flat: the chest lifts to look ahead
    let prone_lift = if p.stance == Stance::Prone {
        -0.45
    } else {
        0.0
    };
    bend(
        r,
        &mut local,
        &mut g,
        ids.abdomen,
        lean * 0.6 - p.pitch * 0.25 + prone_lift * 0.5,
    );
    bend(
        r,
        &mut local,
        &mut g,
        ids.torso,
        lean * 0.4 - p.pitch * 0.25 + prone_lift * 0.5,
    );
    bend(
        r,
        &mut local,
        &mut g,
        ids.neck,
        -lean * 0.6 - p.pitch * 0.2 - prone_lift * 0.4,
    );

    // gun at the shoulders, pitched with the aim
    // the gun stays steady while the body bobs under it; the arms take up the difference
    let shoulders_model = mid(&g) - sway * STEADY;
    let shoulders = world.transform_point3(shoulders_model);
    let melee_push = (p.melee * PI).sin() * 0.35;
    let gun_base = trs(shoulders, p.yaw, aim_pitch, 0.0)
        * Mat4::from_translation(vec3(0.12, -0.13, -0.2 - melee_push))
        * Mat4::from_rotation_y(0.7 * p.sprint)
        * Mat4::from_rotation_z(0.5 * reload_k)
        * Mat4::from_scale(Vec3::splat(1.2));
    let mag_off = vec3(0.0, -mag_drop, 0.0);
    add_gun_ex(m, &gun_base, p.weapon, 0.02, mag_off);

    // hands on the grips
    let right_hand = gun_base.transform_point3(vec3(0.0, -0.03, 0.02));
    let left_hand = if reload_k > 0.0 {
        let mag = gun_mag(p.weapon)
            .map(|g| g.0)
            .unwrap_or(vec3(0.0, 0.0, -0.3));
        gun_base
            .transform_point3(mag + mag_off + vec3(0.0, -0.06, 0.0))
            .lerp(
                gun_base.transform_point3(fore_grip(p.weapon)),
                1.0 - reload_k,
            )
    } else {
        gun_base.transform_point3(fore_grip(p.weapon))
    };
    for (s, hand) in [(1usize, right_hand), (0, left_hand)] {
        let side = if s == 0 { 1.0 } else { -1.0 };
        let len = local[ids.fist[s]].t.length();
        let sh = g[ids.upper_arm[s]].t;
        let target = to_model.transform_point3(hand);
        // the wrist stops short of the grip so the fist wraps around it
        let target = target - (target - sh).normalize_or_zero() * 0.12;
        r.reach(
            &mut local,
            &mut g,
            ids.upper_arm[s],
            ids.lower_arm[s],
            len,
            target,
            vec3(side * 0.7, -1.0, -0.3),
        );
    }

    // characters have no side: a plain dark outline
    let ink = if skin_of(p.look) > 0 {
        crate::mesh::OUTLINE
    } else {
        team_ink(p.team)
    };
    let prev_ink = m.ink.replace(ink);
    r.draw(m, model, &g, world, &colors(model, p.look), p.outline);
    if p.has_bomb {
        let back = world * g[ids.torso].mat();
        m.rounded(
            &(back
                * Mat4::from_translation(vec3(0.0, 0.25, -0.42))
                * Mat4::from_scale(Vec3::splat(1.0 / SCALE))),
            vec3(0.18, 0.18, 0.08),
            0.05,
            [220, 50, 40],
            p.outline,
            3,
        );
    }
    m.ink = prev_ink;
    gun_base.transform_point3(muzzle(p.weapon))
}

/// Ragdoll joints in world space.
pub struct RagdollJoints {
    pub head: Vec3,
    pub chest: Vec3,
    pub pelvis: Vec3,
    /// [left, right]
    pub shoulder: [Vec3; 2],
    pub knee: [Vec3; 2],
    pub foot: [Vec3; 2],
    pub hand: [Vec3; 2],
}

/// Draws a limp soldier following ragdoll joints.
pub fn add_ragdoll(m: &mut ToonMesh, team: Team, look: u8, j: &RagdollJoints) {
    let r = rig();
    let ids = &r.ids;
    let model = model_for(team, look);
    // body frame: up along the spine, right across the shoulders; the model faces +Z
    let up = (j.chest - j.pelvis).normalize_or(Vec3::Y);
    let right = (j.shoulder[1] - j.shoulder[0]).normalize_or(Vec3::X);
    let fwd = up.cross(right).normalize_or(Vec3::NEG_Z);
    let right = fwd.cross(up).normalize();
    // model +X is the character's left
    let basis = Mat3::from_cols(-right, up, fwd);
    let mut local = r.sample(clip::IDLE, 0.0, true);
    let mut g = r.globals(&local);
    let hips_model = g[ids.hips].t;
    let world = Mat4::from_translation(j.pelvis)
        * Mat4::from_mat3(basis)
        * Mat4::from_scale(Vec3::splat(SCALE))
        * Mat4::from_translation(-hips_model);
    let to_model = world.inverse();
    let mp = |v: Vec3| to_model.transform_point3(v);

    r.aim(&mut local, &mut g, ids.neck, mp(j.head));
    for s in 0..2 {
        let side = if s == 0 { 1.0 } else { -1.0 };
        let arm_len = local[ids.fist[s]].t.length();
        r.reach(
            &mut local,
            &mut g,
            ids.upper_arm[s],
            ids.lower_arm[s],
            arm_len,
            mp(j.hand[s]),
            vec3(side, -1.0, -0.5),
        );
        let leg_len = bind_dist(r, ids.lower_leg[s], ids.foot[s]);
        // the foot bones are separate (IK controls in the original rig): they follow the
        // shin, keeping their angle to it
        let foot_rel = g[ids.lower_leg[s]].r.inverse() * g[ids.foot[s]].r;
        r.aim(&mut local, &mut g, ids.upper_leg[s], mp(j.knee[s]));
        r.aim(&mut local, &mut g, ids.lower_leg[s], mp(j.foot[s]));
        let shin = g[ids.lower_leg[s]];
        let foot = Xf {
            t: shin.t + shin.r * Vec3::Y * leg_len,
            r: shin.r * foot_rel,
        };
        r.set_global(&mut local, &mut g, ids.foot[s], foot);
    }
    let ink = if skin_of(look) > 0 {
        crate::mesh::OUTLINE
    } else {
        team_ink(team)
    };
    let prev_ink = m.ink.replace(ink);
    r.draw(m, model, &g, world, &colors(model, look), OUTLINE);
    m.ink = prev_ink;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn soldier_mesh_budget() {
        let mut m = ToonMesh::new();
        for round in 0..3 {
            m.clear();
            let t0 = std::time::Instant::now();
            for k in 0..16u8 {
                let pose = SoldierPose {
                    pos: Vec3::ZERO,
                    yaw: 0.3,
                    pitch: 0.1,
                    stance: Stance::Stand,
                    team: if k % 2 == 0 { Team::Allies } else { Team::Axis },
                    weapon: WeaponId::M4,
                    look: k,
                    time: 1.0,
                    phase: 0.5 + k as f32,
                    walk: 1.0,
                    sprint: 0.0,
                    has_bomb: false,
                    melee: 0.0,
                    reload: None,
                    outline: OUTLINE,
                };
                add_soldier(&mut m, &pose);
            }
            let verts: usize = m
                .fill
                .chunks
                .iter()
                .chain(m.outline.chunks.iter())
                .map(|c| c.verts.len())
                .sum();
            println!(
                "round {round}: 16 soldiers = {verts} verts in {:?}",
                t0.elapsed()
            );
        }
    }
}

#[cfg(test)]
mod fit {
    use super::*;

    /// The top of the model matches the top of the head hitbox in every stance.
    #[test]
    fn model_fits_hitboxes() {
        for stance in [Stance::Stand, Stance::Crouch, Stance::Prone] {
            let mut m = ToonMesh::new();
            let pose = SoldierPose {
                pos: Vec3::ZERO,
                yaw: 0.0,
                pitch: 0.0,
                stance,
                team: Team::Allies,
                weapon: WeaponId::M4,
                look: 0,
                time: 0.0,
                phase: 0.0,
                walk: 0.0,
                sprint: 0.0,
                has_bomb: false,
                melee: 0.0,
                reload: None,
                outline: OUTLINE,
            };
            add_soldier(&mut m, &pose);
            let top = m
                .fill
                .chunks
                .iter()
                .flat_map(|c| c.verts.iter())
                .map(|v| v.position.y)
                .fold(0.0f32, f32::max);
            let head = toon_shared::ray::hitboxes(stance)[0].2.y;
            assert!(
                (top - head).abs() < 0.08,
                "{stance:?}: model {top}, hitbox {head}"
            );
        }
    }
}

#[cfg(test)]
mod smooth {
    use super::*;

    /// At walking and sprinting speed the clips play at most about twice as fast as
    /// authored, and the gun moves smoothly from frame to frame.
    #[test]
    fn running_is_calm() {
        let r = rig();
        for (speed, sprint) in [(5.2f32, 0.0f32), (7.6, 1.0)] {
            let phase_rate = speed * 2.2;
            let loops = phase_rate / TAU * GAIT_CYCLES;
            let authored = 1.0 / r.clips[clip::RUN].dur;
            assert!(
                loops / authored < 2.0,
                "speed {speed}: {:.2}x",
                loops / authored
            );

            let dt = 1.0 / 60.0;
            let mut prev: Vec<Vec3> = Vec::new();
            let mut worst = 0.0f32;
            for f in 0..120 {
                let t = f as f32 * dt;
                let mut m = ToonMesh::new();
                let pose = SoldierPose {
                    pos: Vec3::ZERO,
                    yaw: 0.0,
                    pitch: 0.0,
                    stance: Stance::Stand,
                    team: Team::Allies,
                    weapon: WeaponId::M4,
                    look: 0,
                    time: t,
                    phase: t * phase_rate,
                    walk: speed / 5.2,
                    sprint,
                    has_bomb: false,
                    melee: 0.0,
                    reload: None,
                    outline: OUTLINE,
                };
                let muzzle = add_soldier(&mut m, &pose);
                prev.push(muzzle);
                if prev.len() >= 3 {
                    let n = prev.len();
                    // change of velocity between frames (jerkiness), metres per frame
                    let acc = (prev[n - 1] - prev[n - 2]) - (prev[n - 2] - prev[n - 3]);
                    worst = worst.max(acc.length());
                }
            }
            println!(
                "speed {speed}: {:.2}x authored, gun jerk {worst:.4} m",
                loops / authored
            );
            assert!(worst < 0.01, "gun jerk {worst}");
        }
    }
}
