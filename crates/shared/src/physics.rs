//! Player movement and world collision. The same code runs on the client (prediction)
//! and on the server (bots, validation), so movement feels identical on both.

use crate::consts::*;
use crate::map::Map;
use crate::tuning::MoveTuning;
use crate::yaw_basis;
use glam::{vec3, Vec3};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Aabb {
    pub fn overlaps(&self, o: &Aabb) -> bool {
        self.min.x < o.max.x
            && self.max.x > o.min.x
            && self.min.y < o.max.y
            && self.max.y > o.min.y
            && self.min.z < o.max.z
            && self.max.z > o.min.z
    }
}

const CELL: f32 = 4.0;

/// Collision world: solid boxes plus a uniform grid for broad-phase queries.
pub struct World {
    pub boxes: Vec<Aabb>,
    origin: (f32, f32),
    dims: (i32, i32),
    cells: Vec<Vec<u32>>,
}

impl World {
    pub fn from_map(map: &Map) -> World {
        let boxes: Vec<Aabb> = map
            .boxes
            .iter()
            .filter(|b| b.solid)
            .map(|b| Aabb { min: b.min, max: b.max })
            .collect();
        let pad = 4.0;
        let origin = (map.min.x - pad, map.min.y - pad);
        let dims = (
            ((map.max.x - map.min.x + 2.0 * pad) / CELL).ceil() as i32,
            ((map.max.y - map.min.y + 2.0 * pad) / CELL).ceil() as i32,
        );
        let mut cells = vec![Vec::new(); (dims.0 * dims.1) as usize];
        let mut w = World { boxes, origin, dims, cells: Vec::new() };
        for (i, b) in w.boxes.iter().enumerate() {
            let (x0, z0) = w.cell_of(b.min.x, b.min.z);
            let (x1, z1) = w.cell_of(b.max.x, b.max.z);
            for cz in z0..=z1 {
                for cx in x0..=x1 {
                    cells[(cz * dims.0 + cx) as usize].push(i as u32);
                }
            }
        }
        w.cells = cells;
        w
    }

    fn cell_of(&self, x: f32, z: f32) -> (i32, i32) {
        let cx = (((x - self.origin.0) / CELL).floor() as i32).clamp(0, self.dims.0 - 1);
        let cz = (((z - self.origin.1) / CELL).floor() as i32).clamp(0, self.dims.1 - 1);
        (cx, cz)
    }

    /// Calls `f` for every box that may overlap the given XZ rectangle (may repeat boxes).
    fn for_candidates(&self, bb: &Aabb, mut f: impl FnMut(&Aabb) -> bool) -> bool {
        let (x0, z0) = self.cell_of(bb.min.x, bb.min.z);
        let (x1, z1) = self.cell_of(bb.max.x, bb.max.z);
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                for &i in &self.cells[(cz * self.dims.0 + cx) as usize] {
                    if f(&self.boxes[i as usize]) {
                        return true;
                    }
                }
            }
        }
        false
    }

    pub fn overlaps(&self, bb: &Aabb) -> bool {
        self.for_candidates(bb, |b| b.overlaps(bb))
    }

    /// Highest top among boxes overlapping `bb` (used for step-up).
    fn max_top(&self, bb: &Aabb) -> Option<f32> {
        let mut top: Option<f32> = None;
        self.for_candidates(bb, |b| {
            if b.overlaps(bb) {
                top = Some(top.map_or(b.max.y, |t: f32| t.max(b.max.y)));
            }
            false
        });
        top
    }

    /// Raycast against world geometry. Returns distance and surface normal.
    pub fn raycast(&self, o: Vec3, d: Vec3, max_dist: f32) -> Option<(f32, Vec3)> {
        let mut best: Option<(f32, Vec3)> = None;
        let inv = vec3(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let end = o + d * max_dist;
        let seg = Aabb { min: o.min(end), max: o.max(end) };
        let mut limit = max_dist;
        // Coarse: walk the grid cells overlapped by the segment's bounding box.
        // Cheap enough for our map sizes and keeps the code simple.
        self.for_candidates(&seg, |b| {
            if let Some((t, n)) = crate::ray::ray_aabb(o, inv, b.min, b.max) {
                if t <= limit {
                    limit = t;
                    best = Some((t, n));
                }
            }
            false
        });
        best
    }

    /// True if nothing blocks the segment a → b.
    pub fn line_of_sight(&self, a: Vec3, b: Vec3) -> bool {
        let d = b - a;
        let len = d.length();
        if len < 1e-4 {
            return true;
        }
        self.raycast(a, d / len, len).is_none()
    }

    /// Height of the floor below `p` (searching down to `p.y - max_drop`).
    pub fn floor_below(&self, p: Vec3, max_drop: f32) -> Option<f32> {
        self.raycast(p + vec3(0.0, 0.05, 0.0), vec3(0.0, -1.0, 0.0), max_drop + 0.05)
            .map(|(t, _)| p.y + 0.05 - t)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Stance {
    #[default]
    Stand,
    Crouch,
    Prone,
}

impl Stance {
    pub fn height(self) -> f32 {
        match self {
            Stance::Stand => STAND_HEIGHT,
            Stance::Crouch => CROUCH_HEIGHT,
            Stance::Prone => PRONE_HEIGHT,
        }
    }
    pub fn eye(self) -> f32 {
        match self {
            Stance::Stand => STAND_EYE,
            Stance::Crouch => CROUCH_EYE,
            Stance::Prone => PRONE_EYE,
        }
    }
    pub fn from_u8(v: u8) -> Stance {
        match v {
            1 => Stance::Crouch,
            2 => Stance::Prone,
            _ => Stance::Stand,
        }
    }
    pub fn to_u8(self) -> u8 {
        self as u8
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Body {
    pub pos: Vec3,
    pub vel: Vec3,
    pub on_ground: bool,
    pub stance: Stance,
    /// Sum of instant vertical snaps (stairs up/down) since last read — lets the
    /// camera smooth them out. See [`Body::take_snap`].
    pub snap: f32,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct MoveInput {
    /// -1..1 forward/back.
    pub forward: f32,
    /// -1..1 right/left.
    pub right: f32,
    pub yaw: f32,
    pub jump: bool,
    pub sprint: bool,
    pub ads: bool,
    pub want_stance: Stance,
    /// Weapon movement speed multiplier (0 = 1.0).
    pub speed_mult: f32,
    /// Weapon sprint speed multiplier (0 = 1.0).
    pub sprint_mult: f32,
    /// Weight of the carried weapon in kg (adds inertia).
    pub weight: f32,
}

pub fn body_box(pos: Vec3, h: f32) -> Aabb {
    Aabb {
        min: vec3(pos.x - PLAYER_RADIUS, pos.y, pos.z - PLAYER_RADIUS),
        max: vec3(pos.x + PLAYER_RADIUS, pos.y + h, pos.z + PLAYER_RADIUS),
    }
}

pub fn fits(world: &World, pos: Vec3, stance: Stance) -> bool {
    !world.overlaps(&body_box(pos, stance.height()))
}

impl Body {
    /// Returns and clears the accumulated vertical snap.
    pub fn take_snap(&mut self) -> f32 {
        std::mem::take(&mut self.snap)
    }

    pub fn is_sprinting(&self, inp: &MoveInput) -> bool {
        self.stance == Stance::Stand && inp.sprint && !inp.ads && inp.forward > 0.1
    }

    pub fn max_speed(&self, inp: &MoveInput, t: &MoveTuning) -> f32 {
        let or1 = |v: f32| if v > 0.0 { v } else { 1.0 };
        let base = match self.stance {
            Stance::Stand if self.is_sprinting(inp) => t.sprint_speed * or1(inp.sprint_mult),
            Stance::Stand => t.walk_speed,
            Stance::Crouch => t.crouch_speed,
            Stance::Prone => t.prone_speed,
        };
        let ads = if inp.ads { t.ads_speed_mult } else { 1.0 };
        base * ads * or1(inp.speed_mult)
    }

    /// Advances the body by `dt` seconds.
    pub fn step(&mut self, world: &World, inp: &MoveInput, t: &MoveTuning, dt: f32) {
        // stance changes (need head room to stand up)
        if inp.want_stance != self.stance {
            let ok = inp.want_stance.height() <= self.stance.height() || fits(world, self.pos, inp.want_stance);
            if ok {
                self.stance = inp.want_stance;
            } else if inp.want_stance == Stance::Stand && self.stance == Stance::Prone && fits(world, self.pos, Stance::Crouch) {
                self.stance = Stance::Crouch;
            }
        }

        let (fwd, right) = yaw_basis(inp.yaw);
        let mut f = inp.forward;
        let mut r = inp.right;
        let len = (f * f + r * r).sqrt();
        if len > 1.0 {
            f /= len;
            r /= len;
        }
        // moving backwards / sideways is slower than forwards
        let dir_mult = if len > 0.0 {
            let fm = if f < 0.0 { t.backpedal_mult } else { 1.0 };
            (f * f * fm + r * r * t.strafe_mult) / (f * f + r * r)
        } else {
            1.0
        };
        let wish = (fwd * f + right * r) * self.max_speed(inp, t) * dir_mult;

        let hv = vec3(self.vel.x, 0.0, self.vel.z);
        let new_hv = if self.on_ground {
            // heavier weapons = more inertia (slower to speed up and to stop)
            let inertia = 1.0 + inp.weight.max(0.0) * t.weight_inertia;
            let speeding_up = wish.length() > hv.length() + 0.01 && wish.dot(hv) >= 0.0;
            let rate = if !speeding_up {
                t.ground_decel / inertia.sqrt()
            } else if hv.length() >= t.walk_speed * 0.98 {
                // building up from walk to full sprint takes a moment
                t.sprint_accel / inertia
            } else {
                t.ground_accel / inertia
            };
            approach(hv, wish, rate * dt)
        } else {
            approach(hv, wish, t.air_accel * dt)
        };
        self.vel.x = new_hv.x;
        self.vel.z = new_hv.z;

        if inp.jump && self.on_ground && self.stance == Stance::Stand {
            self.vel.y = t.jump_speed;
            self.on_ground = false;
        } else if inp.jump && self.on_ground && self.stance != Stance::Stand {
            // jump key stands you up like in CoD
            if fits(world, self.pos, Stance::Stand) {
                self.stance = Stance::Stand;
            }
        }
        self.vel.y -= t.gravity * dt;
        self.vel.y = self.vel.y.max(-40.0);

        let delta = self.vel * dt;
        let steps = (delta.abs().max_element() / 0.2).ceil().max(1.0) as u32;
        let sub = delta / steps as f32;
        let was_on_ground = self.on_ground;
        self.on_ground = false;
        for _ in 0..steps {
            self.move_axis(world, 0, sub.x, was_on_ground);
            self.move_axis(world, 2, sub.z, was_on_ground);
            self.move_axis(world, 1, sub.y, was_on_ground);
        }
        // stick to the ground when walking down small steps
        if was_on_ground && !self.on_ground && self.vel.y <= 0.0 {
            if let Some(fy) = world.floor_below(self.pos, STEP_HEIGHT) {
                let np = vec3(self.pos.x, fy + 0.001, self.pos.z);
                if fits(world, np, self.stance) {
                    self.snap += np.y - self.pos.y;
                    self.pos = np;
                    self.on_ground = true;
                    self.vel.y = 0.0;
                }
            }
        }
    }

    fn move_axis(&mut self, world: &World, axis: usize, d: f32, grounded: bool) {
        if d == 0.0 {
            return;
        }
        let h = self.stance.height();
        let mut np = self.pos;
        np[axis] += d;
        let bb = body_box(np, h);
        if !world.overlaps(&bb) {
            self.pos = np;
            return;
        }
        if axis != 1 {
            // try stepping up
            if grounded || self.on_ground {
                if let Some(top) = world.max_top(&bb) {
                    let lift = top - self.pos.y;
                    if lift > 0.0 && lift <= STEP_HEIGHT {
                        let up = vec3(np.x, top + 0.001, np.z);
                        if fits(world, up, self.stance) && fits(world, vec3(self.pos.x, top + 0.001, self.pos.z), self.stance) {
                            self.snap += up.y - self.pos.y;
                            self.pos = up;
                            self.on_ground = true;
                            return;
                        }
                    }
                }
            }
            // clamp against the blocking boxes
            let mut lim = np[axis];
            world.for_candidates(&bb, |b| {
                if b.overlaps(&bb) {
                    if d > 0.0 {
                        lim = lim.min(b.min[axis] - PLAYER_RADIUS - 0.001);
                    } else {
                        lim = lim.max(b.max[axis] + PLAYER_RADIUS + 0.001);
                    }
                }
                false
            });
            let mut cp = self.pos;
            cp[axis] = if d > 0.0 { lim.max(self.pos[axis]).min(np[axis]) } else { lim.min(self.pos[axis]).max(np[axis]) };
            if fits(world, cp, self.stance) {
                self.pos = cp;
            }
            self.vel[axis] = 0.0;
        } else {
            let mut lim = np.y;
            world.for_candidates(&bb, |b| {
                if b.overlaps(&bb) {
                    if d < 0.0 {
                        lim = lim.max(b.max.y + 0.0005);
                    } else {
                        lim = lim.min(b.min.y - h - 0.0005);
                    }
                }
                false
            });
            let mut cp = self.pos;
            cp.y = if d < 0.0 { lim.min(self.pos.y) } else { lim.max(self.pos.y) };
            if fits(world, cp, self.stance) {
                self.pos = cp;
            }
            if d < 0.0 {
                self.on_ground = true;
            }
            self.vel.y = 0.0;
        }
    }
}

fn approach(cur: Vec3, target: Vec3, max_delta: f32) -> Vec3 {
    let diff = target - cur;
    let len = diff.length();
    if len <= max_delta || len < 1e-6 {
        target
    } else {
        cur + diff / len * max_delta
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::map::MapId;
    use crate::maps;
    use glam::vec2;

    fn mt() -> MoveTuning {
        MoveTuning::default()
    }

    #[test]
    fn heavy_weapon_accelerates_slower_and_sprints_slower() {
        let (_, w) = world();
        let run = |weight: f32, sprint_mult: f32| {
            let mut b = Body { pos: vec3(0.0, 0.0, 20.0), on_ground: true, ..Default::default() };
            let inp = MoveInput { forward: 1.0, sprint: true, weight, sprint_mult, ..Default::default() };
            let mut d02 = 0.0;
            for i in 0..90 {
                b.step(&w, &inp, &mt(), 1.0 / 60.0);
                if i == 11 {
                    d02 = 20.0 - b.pos.z;
                }
            }
            (d02, vec2(b.vel.x, b.vel.z).length())
        };
        let (smg_early, smg_top) = run(2.7, 1.0);
        let (lmg_early, lmg_top) = run(8.0, 0.84);
        assert!(lmg_early < smg_early, "{lmg_early} vs {smg_early}");
        assert!(lmg_top < smg_top - 0.5, "{lmg_top} vs {smg_top}");
    }

    #[test]
    fn backpedal_is_slower() {
        let (_, w) = world();
        let mut a = Body { pos: vec3(0.0, 0.0, 20.0), on_ground: true, ..Default::default() };
        let mut b = a;
        for _ in 0..60 {
            a.step(&w, &MoveInput { forward: 1.0, ..Default::default() }, &mt(), 1.0 / 60.0);
            b.step(&w, &MoveInput { forward: -1.0, ..Default::default() }, &mt(), 1.0 / 60.0);
        }
        assert!(b.vel.length() < a.vel.length());
    }

    fn world() -> (Map, World) {
        let m = maps::build(MapId::Depot);
        let w = World::from_map(&m);
        (m, w)
    }

    fn settle(w: &World, b: &mut Body) {
        for _ in 0..60 {
            b.step(w, &MoveInput { yaw: 0.0, ..Default::default() }, &mt(), 1.0 / 60.0);
        }
    }

    #[test]
    fn stands_on_ground_and_walks() {
        let (_, w) = world();
        let mut b = Body { pos: vec3(0.0, 2.0, 20.0), ..Default::default() };
        settle(&w, &mut b);
        assert!(b.on_ground);
        assert!(b.pos.y.abs() < 0.01, "y={}", b.pos.y);
        let start = b.pos;
        let inp = MoveInput { forward: 1.0, yaw: 0.0, ..Default::default() };
        for _ in 0..60 {
            b.step(&w, &inp, &mt(), 1.0 / 60.0);
        }
        let moved = start.z - b.pos.z;
        assert!(moved > 4.0 && moved < mt().walk_speed + 0.1, "moved {moved}");
    }

    #[test]
    fn walls_block() {
        let (_, w) = world();
        // walk west into the perimeter wall
        let mut b = Body { pos: vec3(-38.0, 0.0, 25.0), ..Default::default() };
        settle(&w, &mut b);
        let inp = MoveInput { forward: 1.0, yaw: std::f32::consts::FRAC_PI_2, ..Default::default() };
        for _ in 0..120 {
            b.step(&w, &inp, &mt(), 1.0 / 60.0);
        }
        assert!(b.pos.x >= -40.0 + PLAYER_RADIUS - 0.01, "x={}", b.pos.x);
    }

    #[test]
    fn climbs_office_stairs() {
        let (_, w) = world();
        // bottom of the office stairs, climbing towards -z
        let mut b = Body { pos: vec3(30.35, 0.0, -1.5), ..Default::default() };
        settle(&w, &mut b);
        let inp = MoveInput { forward: 1.0, yaw: 0.0, ..Default::default() };
        for _ in 0..150 {
            b.step(&w, &inp, &mt(), 1.0 / 60.0);
        }
        assert!(b.pos.y > 3.0, "y={} z={}", b.pos.y, b.pos.z);
        // every step-up is reported so the camera can smooth it
        assert!(b.take_snap() > 2.5);
        assert_eq!(b.snap, 0.0);
        // step west off the top onto the floor
        let inp = MoveInput { forward: 1.0, yaw: std::f32::consts::FRAC_PI_2, ..Default::default() };
        for _ in 0..60 {
            b.step(&w, &inp, &mt(), 1.0 / 60.0);
        }
        assert!(b.pos.y > 3.3 && b.pos.x < 29.0, "pos={:?}", b.pos);
    }

    #[test]
    fn jump_and_land() {
        let (_, w) = world();
        let mut b = Body { pos: vec3(0.0, 0.0, 20.0), ..Default::default() };
        settle(&w, &mut b);
        b.step(&w, &MoveInput { jump: true, ..Default::default() }, &mt(), 1.0 / 60.0);
        assert!(!b.on_ground);
        let mut peak: f32 = 0.0;
        for _ in 0..120 {
            b.step(&w, &MoveInput::default(), &mt(), 1.0 / 60.0);
            peak = peak.max(b.pos.y);
        }
        assert!(b.on_ground && peak > 0.9 && peak < 1.3, "peak {peak}");
    }

    #[test]
    fn crouch_needs_headroom_to_stand() {
        let (_, w) = world();
        let mut b = Body { pos: vec3(0.0, 0.0, 20.0), ..Default::default() };
        settle(&w, &mut b);
        b.step(&w, &MoveInput { want_stance: Stance::Prone, ..Default::default() }, &mt(), 1.0 / 60.0);
        assert_eq!(b.stance, Stance::Prone);
        b.step(&w, &MoveInput { want_stance: Stance::Stand, ..Default::default() }, &mt(), 1.0 / 60.0);
        assert_eq!(b.stance, Stance::Stand);
    }

    #[test]
    fn all_spawns_fit() {
        for id in MapId::ALL {
            let m = maps::build(id);
            let w = World::from_map(&m);
            for s in m.all_spawns() {
                assert!(fits(&w, s.pos + vec3(0.0, 0.01, 0.0), Stance::Stand), "{:?} spawn {:?}", id, s.pos);
            }
            for p in [m.bomb, m.sites[0], m.sites[1]] {
                assert!(fits(&w, p + vec3(0.0, 0.01, 0.0), Stance::Stand), "{:?} objective {:?}", id, p);
            }
        }
    }
}
