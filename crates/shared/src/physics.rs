//! Player movement and world collision. The same code runs on the client (prediction)
//! and on the server (bots, validation), so movement feels identical on both.

use crate::consts::*;
use crate::map::{Ladder, Map, Mat};
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

/// Wood or cloth a bullet went through.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pierced {
    pub t: f32,
    pub mat: Mat,
    pub normal: Vec3,
}

/// Collision world: solid boxes plus a uniform grid for broad-phase queries.
pub struct World {
    pub boxes: Vec<Aabb>,
    /// What each box is made of (parallel to `boxes`).
    pub mats: Vec<Mat>,
    pub ladders: Vec<Ladder>,
    /// Hills and valleys under everything (big outdoor maps).
    pub terrain: Option<std::sync::Arc<crate::terrain::Heightmap>>,
    origin: (f32, f32),
    dims: (i32, i32),
    cells: Vec<Vec<u32>>,
}

impl World {
    pub fn from_map(map: &Map) -> World {
        let solid: Vec<_> = map.boxes.iter().filter(|b| b.solid).collect();
        let boxes: Vec<Aabb> = solid
            .iter()
            .map(|b| Aabb {
                min: b.min,
                max: b.max,
            })
            .collect();
        let mats = solid.iter().map(|b| b.mat).collect();
        let pad = 4.0;
        let origin = (map.min.x - pad, map.min.y - pad);
        let dims = (
            ((map.max.x - map.min.x + 2.0 * pad) / CELL).ceil() as i32,
            ((map.max.y - map.min.y + 2.0 * pad) / CELL).ceil() as i32,
        );
        let mut cells = vec![Vec::new(); (dims.0 * dims.1) as usize];
        let mut w = World {
            boxes,
            mats,
            ladders: map.ladders.clone(),
            terrain: map.terrain.clone(),
            origin,
            dims,
            cells: Vec::new(),
        };
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

    /// Calls `f` for every box that blocks movement and may overlap the given XZ rectangle
    /// (may repeat boxes). Cloth is left out: players walk through it.
    fn for_candidates(&self, bb: &Aabb, mut f: impl FnMut(&Aabb) -> bool) -> bool {
        let (x0, z0) = self.cell_of(bb.min.x, bb.min.z);
        let (x1, z1) = self.cell_of(bb.max.x, bb.max.z);
        for cz in z0..=z1 {
            for cx in x0..=x1 {
                for &i in &self.cells[(cz * self.dims.0 + cx) as usize] {
                    if self.mats[i as usize].blocks_movement() && f(&self.boxes[i as usize]) {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// Walks the grid cells a ray crosses, in order (2D, over XZ), calling
    /// `f(box indices of the cell, distance where the ray leaves the cell)` until it
    /// returns true or the ray is `max` long.
    fn walk_ray(&self, o: Vec3, d: Vec3, max: f32, mut f: impl FnMut(&[u32], f32) -> bool) {
        // start where the ray enters the grid
        let gx0 = self.origin.0;
        let gz0 = self.origin.1;
        let gx1 = gx0 + self.dims.0 as f32 * CELL;
        let gz1 = gz0 + self.dims.1 as f32 * CELL;
        let mut t0 = 0.0f32;
        let mut t1 = max;
        for (o, d, lo, hi) in [(o.x, d.x, gx0, gx1), (o.z, d.z, gz0, gz1)] {
            if d.abs() < 1e-9 {
                if o < lo || o > hi {
                    return;
                }
            } else {
                let (a, b) = ((lo - o) / d, (hi - o) / d);
                t0 = t0.max(a.min(b));
                t1 = t1.min(a.max(b));
            }
        }
        if t0 > t1 {
            return;
        }
        let p = o + d * (t0 + 1e-4);
        let (mut cx, mut cz) = self.cell_of(p.x, p.z);
        let step = |d: f32| if d > 0.0 { 1 } else { -1 };
        let next = |c: i32, org: f32, o: f32, d: f32| {
            if d.abs() < 1e-9 {
                f32::INFINITY
            } else {
                let edge = org + (c + if d > 0.0 { 1 } else { 0 }) as f32 * CELL;
                (edge - o) / d
            }
        };
        let (sx, sz) = (step(d.x), step(d.z));
        let dtx = if d.x.abs() < 1e-9 {
            f32::INFINITY
        } else {
            CELL / d.x.abs()
        };
        let dtz = if d.z.abs() < 1e-9 {
            f32::INFINITY
        } else {
            CELL / d.z.abs()
        };
        let mut tx = next(cx, gx0, o.x, d.x);
        let mut tz = next(cz, gz0, o.z, d.z);
        loop {
            let leave = tx.min(tz).min(t1);
            if f(&self.cells[(cz * self.dims.0 + cx) as usize], leave) || leave >= t1 {
                return;
            }
            if tx < tz {
                cx += sx;
                tx += dtx;
            } else {
                cz += sz;
                tz += dtz;
            }
            if cx < 0 || cz < 0 || cx >= self.dims.0 || cz >= self.dims.1 {
                return;
            }
        }
    }

    pub fn overlaps(&self, bb: &Aabb) -> bool {
        self.for_candidates(bb, |b| b.overlaps(bb))
    }

    /// The ladder a body standing at `p` is on, if any.
    pub fn ladder_at(&self, p: Vec3) -> Option<&Ladder> {
        self.ladders.iter().find(|l| l.holds(p))
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
    /// Everything counts here, cloth too (it hides what is behind it), and the ground.
    pub fn raycast(&self, o: Vec3, d: Vec3, max_dist: f32) -> Option<(f32, Vec3)> {
        let mut best = self.box_ray(o, d, max_dist);
        let limit = best.map_or(max_dist, |b| b.0);
        if let Some(t) = &self.terrain {
            if let Some((tt, n)) = t.raycast(o, d, limit) {
                if best.is_none_or(|b| tt < b.0) {
                    best = Some((tt, n));
                }
            }
        }
        best
    }

    /// `raycast` against the boxes only.
    fn box_ray(&self, o: Vec3, d: Vec3, max_dist: f32) -> Option<(f32, Vec3)> {
        let mut best: Option<(f32, Vec3)> = None;
        let inv = vec3(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut limit = max_dist;
        // the cells along the ray, nearest first: stop once a hit lies inside the cell
        self.walk_ray(o, d, max_dist, |cell, leave| {
            for &i in cell {
                let b = &self.boxes[i as usize];
                if let Some((t, n)) = crate::ray::ray_aabb(o, inv, b.min, b.max) {
                    if t <= limit {
                        limit = t;
                        best = Some((t, n));
                    }
                }
            }
            best.is_some() && limit <= leave
        });
        best
    }

    /// A bullet's path: where it stops (the first solid surface, or `max_dist`), and the
    /// wood and cloth it goes through on the way (distance, material, surface normal),
    /// nearest first.
    pub fn bullet_trace(&self, o: Vec3, d: Vec3, max_dist: f32) -> (f32, Vec3, Vec<Pierced>) {
        let inv = vec3(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
        let mut stop = max_dist;
        let mut normal = Vec3::ZERO;
        let mut soft: Vec<(u32, Pierced)> = Vec::new();
        self.walk_ray(o, d, max_dist, |cell, leave| {
            for &i in cell {
                let b = &self.boxes[i as usize];
                let Some((t, n)) = crate::ray::ray_aabb(o, inv, b.min, b.max) else {
                    continue;
                };
                if t > stop {
                    continue;
                }
                match self.mats[i as usize] {
                    Mat::Solid => {
                        stop = t;
                        normal = n;
                    }
                    mat => {
                        if !soft.iter().any(|(j, _)| *j == i) {
                            soft.push((i, Pierced { t, mat, normal: n }));
                        }
                    }
                }
            }
            stop <= leave
        });
        // the ground stops it too
        if let Some(t) = &self.terrain {
            if let Some((tt, n)) = t.raycast(o, d, stop) {
                stop = tt;
                normal = n;
            }
        }
        let mut pierced: Vec<Pierced> = soft
            .into_iter()
            .map(|(_, p)| p)
            .filter(|p| p.t < stop)
            .collect();
        pierced.sort_by(|a, b| a.t.total_cmp(&b.t));
        (stop, normal, pierced)
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

    /// Pushes a sphere out of every box it overlaps (smallest-penetration axis).
    /// Returns the corrected centre and the summed push normal, if anything was hit.
    pub fn push_out_sphere(&self, p: Vec3, r: f32) -> Option<(Vec3, Vec3)> {
        let mut c = p;
        let mut normal = Vec3::ZERO;
        let bb = Aabb {
            min: p - Vec3::splat(r),
            max: p + Vec3::splat(r),
        };
        self.for_candidates(&bb, |b| {
            let min = b.min - Vec3::splat(r);
            let max = b.max + Vec3::splat(r);
            if c.cmpgt(min).all() && c.cmplt(max).all() {
                let pen = [
                    (c.x - min.x, vec3(-1.0, 0.0, 0.0)),
                    (max.x - c.x, vec3(1.0, 0.0, 0.0)),
                    (c.y - min.y, vec3(0.0, -1.0, 0.0)),
                    (max.y - c.y, vec3(0.0, 1.0, 0.0)),
                    (c.z - min.z, vec3(0.0, 0.0, -1.0)),
                    (max.z - c.z, vec3(0.0, 0.0, 1.0)),
                ];
                let (d, n) =
                    pen.iter()
                        .copied()
                        .fold((f32::MAX, Vec3::ZERO), |a, b| if b.0 < a.0 { b } else { a });
                c += n * d;
                normal += n;
            }
            false
        });
        if let Some(t) = &self.terrain {
            let g = t.height(c.x, c.z);
            if c.y - r < g {
                c.y = g + r;
                normal += t.normal(c.x, c.z);
            }
        }
        (normal != Vec3::ZERO).then(|| (c, normal.normalize_or_zero()))
    }

    /// Height of the floor below `p` (searching down to `p.y - max_drop`).
    pub fn floor_below(&self, p: Vec3, max_drop: f32) -> Option<f32> {
        let boxes = self
            .box_ray(p + vec3(0.0, 0.05, 0.0), vec3(0.0, -1.0, 0.0), max_drop + 0.05)
            .map(|(t, _)| p.y + 0.05 - t);
        // the ground straight under p (if it is not above p)
        let ground = self.terrain.as_ref().and_then(|t| {
            let g = t.height(p.x, p.z);
            (g <= p.y + 0.05 && g >= p.y - max_drop).then_some(g)
        });
        match (boxes, ground) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        }
    }

    /// The ground's height at (x, z), if the map has terrain.
    pub fn ground(&self, x: f32, z: f32) -> Option<f32> {
        self.terrain.as_ref().map(|t| t.height(x, z))
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
    /// Holding on to a ladder (no gravity).
    pub climbing: bool,
    /// Seconds before a ladder can be grabbed again (after jumping off one).
    pub ladder_free: f32,
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
            let ok = inp.want_stance.height() <= self.stance.height()
                || fits(world, self.pos, inp.want_stance);
            if ok {
                self.stance = inp.want_stance;
            } else if inp.want_stance == Stance::Stand
                && self.stance == Stance::Prone
                && fits(world, self.pos, Stance::Crouch)
            {
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
        // wading through rivers and shallow sea is slow
        let wish = match &world.terrain {
            Some(m) if self.pos.y < m.water - 0.35 => wish * 0.55,
            _ => wish,
        };

        self.ladder_free = (self.ladder_free - dt).max(0.0);
        if self.climb(world, inp, t, dt) {
            return;
        }
        let start = self.pos;
        let was_grounded = self.on_ground;
        self.step_in_world(world, inp, t, dt, wish);
        self.stand_on_ground(world, start, was_grounded);
    }

    /// Keeps the body on the terrain: it cannot sink into the ground or walk up slopes
    /// steeper than `MAX_WALK_SLOPE`, and it lands on it when falling.
    fn stand_on_ground(&mut self, world: &World, start: Vec3, was_grounded: bool) {
        let Some(tr) = &world.terrain else { return };
        let g = tr.height(self.pos.x, self.pos.z);
        if self.pos.y >= g {
            return;
        }
        let moved = vec3(self.pos.x - start.x, 0.0, self.pos.z - start.z).length();
        let rise = g - start.y;
        if was_grounded && rise > moved * crate::terrain::MAX_WALK_SLOPE + 0.04 {
            // too steep to walk up: stay where we were
            self.pos.x = start.x;
            self.pos.z = start.z;
            let g0 = tr.height(start.x, start.z);
            if self.pos.y < g0 {
                self.pos.y = g0;
            }
        } else {
            self.pos.y = g;
        }
        self.on_ground = true;
        self.vel.y = self.vel.y.max(0.0);
    }

    fn step_in_world(&mut self, world: &World, inp: &MoveInput, t: &MoveTuning, dt: f32, wish: Vec3) {

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
            air_move(hv, wish, inp, t, dt)
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

    /// Ladder climbing. Moving towards the wall climbs, moving away from it (or along it)
    /// goes down, whichever way you look; jump lets go. You get on by walking at its foot
    /// or by stepping off the roof onto it (walking past its foot does not grab it), and
    /// off at the top by climbing on (you step onto the roof) or at the bottom by reaching
    /// the ground. Returns true if it moved the body.
    fn climb(&mut self, world: &World, inp: &MoveInput, t: &MoveTuning, dt: f32) -> bool {
        const SPEED: f32 = 3.2;
        let Some(l) = world.ladder_at(self.pos).copied() else {
            self.climbing = false;
            return false;
        };
        if self.ladder_free > 0.0 || self.stance == Stance::Prone {
            self.climbing = false;
            return false;
        }
        // which way the keys move you (not where you look: a bot fighting on a ladder
        // looks at its enemy): towards the wall climbs, away from it or along it goes down
        let (fwd, right) = yaw_basis(inp.yaw);
        let toward = -l.out;
        let wish = fwd * inp.forward + right * inp.right;
        let push = wish.length().min(1.0);
        let heading = if push > 0.1 {
            (wish / wish.length()).dot(toward)
        } else {
            0.0
        };
        let up = if push < 0.1 {
            0.0
        } else if heading > 0.3 {
            push
        } else {
            -push
        };
        if !self.climbing {
            // grab it: walking at it from below, stepping off the roof onto it, or falling
            // past it; walking past its foot does not
            let at_foot = self.pos.y < l.base.y + 0.5;
            let grab = (self.on_ground && at_foot && push > 0.1 && heading > 0.5)
                || (self.on_ground && !at_foot && push > 0.1 && heading < -0.3)
                || (!self.on_ground && self.vel.y < 0.0 && self.pos.y < l.top - 0.3);
            if !grab {
                return false;
            }
            self.climbing = true;
            self.stance = Stance::Stand;
        }
        if inp.jump {
            // let go, pushing off the wall
            self.climbing = false;
            self.ladder_free = 0.4;
            self.vel = l.out * 3.0 + vec3(0.0, t.jump_speed * 0.5, 0.0);
            return false;
        }
        let along = (self.pos - l.base).dot(l.out);
        self.vel = if up > 0.0 && self.pos.y >= l.top + 0.02 {
            // at the top: step onto the roof
            toward * 3.0
        } else if up < 0.0 && self.pos.y >= l.top - 0.1 && along < -0.05 {
            // getting on from the roof: out over the edge first, then down
            l.out * 2.5
        } else {
            vec3(0.0, up * SPEED, 0.0)
        };
        let delta = self.vel * dt;
        self.on_ground = false;
        self.move_axis(world, 0, delta.x, false);
        self.move_axis(world, 2, delta.z, false);
        self.move_axis(world, 1, delta.y, false);
        if self.on_ground && up <= 0.0 {
            // reached the ground
            self.climbing = false;
        }
        true
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
                        if fits(world, up, self.stance)
                            && fits(
                                world,
                                vec3(self.pos.x, top + 0.001, self.pos.z),
                                self.stance,
                            )
                        {
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
            cp[axis] = if d > 0.0 {
                lim.max(self.pos[axis]).min(np[axis])
            } else {
                lim.min(self.pos[axis]).max(np[axis])
            };
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
            cp.y = if d < 0.0 {
                lim.min(self.pos.y)
            } else {
                lim.max(self.pos.y)
            };
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

/// How fast along the keys' direction the strafe push still works (m/s).
const AIR_STRAFE_CAP: f32 = 0.8;

/// Steering in the air, CoD4/Quake style: the keys only push along their own direction,
/// up to the wanted speed *along that direction*, and nothing brakes. So holding A or D
/// while turning the mouse the same way keeps adding a little sideways push to a velocity
/// that points elsewhere — a strafe jump gains some speed, up to `strafe_jump` × sprint.
fn air_move(hv: Vec3, wish: Vec3, inp: &MoveInput, t: &MoveTuning, dt: f32) -> Vec3 {
    let wish_speed = wish.length();
    if wish_speed < 1e-4 {
        return hv;
    }
    let dir = wish / wish_speed;
    // the strafe push: strong, but only while the velocity barely points along the keys
    let along = hv.dot(dir);
    let mut v = hv + dir * (AIR_STRAFE_CAP - along).clamp(0.0, t.air_strafe_accel * dt);
    // plain steering, gentle: it turns the jump but does not speed it up
    let lim = v.length().max(wish_speed);
    let along = v.dot(dir);
    v += dir * (wish_speed - along).clamp(0.0, t.air_accel * dt);
    if v.length() > lim {
        v *= lim / v.length();
    }
    let or1 = |v: f32| if v > 0.0 { v } else { 1.0 };
    let cap = (t.sprint_speed * or1(inp.sprint_mult) * or1(inp.speed_mult) * t.strafe_jump)
        .max(hv.length());
    let len = v.length();
    if len > cap {
        v * (cap / len)
    } else {
        v
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

    /// The grid walk finds exactly what checking every box finds.
    #[test]
    fn raycast_matches_brute_force_on_every_map() {
        let mut rng = crate::rng::Rng::new(7);
        for id in MapId::ALL {
            let map = maps::build(id);
            let w = World::from_map(&map);
            for k in 0..3000 {
                let o = vec3(
                    rng.range(map.min.x - 3.0, map.max.x + 3.0),
                    rng.range(0.1, 12.0),
                    rng.range(map.min.y - 3.0, map.max.y + 3.0),
                );
                let d = if k % 10 == 0 {
                    vec3(0.0, -1.0, 0.0)
                } else if k % 10 == 1 {
                    vec3(1.0, 0.0, 0.0)
                } else {
                    vec3(
                        rng.range(-1.0, 1.0),
                        rng.range(-0.6, 0.6),
                        rng.range(-1.0, 1.0),
                    )
                    .normalize_or(vec3(0.0, 0.0, 1.0))
                };
                let max = rng.range(1.0, 160.0);
                let inv = vec3(1.0 / d.x, 1.0 / d.y, 1.0 / d.z);
                let brute = w
                    .boxes
                    .iter()
                    .filter_map(|b| crate::ray::ray_aabb(o, inv, b.min, b.max))
                    .map(|(t, _)| t)
                    .filter(|t| *t <= max)
                    .fold(None, |a: Option<f32>, t| Some(a.map_or(t, |a| a.min(t))));
                let got = w.raycast(o, d, max).map(|(t, _)| t);
                match (brute, got) {
                    (None, None) => {}
                    (Some(a), Some(b)) => {
                        assert!((a - b).abs() < 1e-3, "{id:?} {o} {d}: {a} vs {b}")
                    }
                    _ => panic!("{id:?} {o} {d} max {max}: {brute:?} vs {got:?}"),
                }
            }
        }
    }

    #[test]
    fn bullets_go_through_cloth_and_wood_but_not_walls() {
        let mut b = crate::map::MapBuilder::new();
        b.ground_and_bounds(vec2(-20.0, -20.0), vec2(20.0, 20.0), [0; 3], [0; 3], 5.0);
        b.cloth(2.0, 0.0, -2.0, 2.05, 3.0, 2.0, [255; 3]);
        b.wood(4.0, 0.0, -2.0, 4.1, 3.0, 2.0, [150, 100, 60]);
        b.block(6.0, 0.0, -2.0, 6.3, 3.0, 2.0, [200; 3]);
        let map = Map {
            id: MapId::Depot,
            name: "test",
            min: vec2(-20.0, -20.0),
            max: vec2(20.0, 20.0),
            boxes: b.boxes,
            ladders: b.ladders,
            spawns: [Vec::new(), Vec::new()],
            bomb: Vec3::ZERO,
            sites: [Vec3::ZERO; 2],
            sky: [0; 3],
            fog: [0; 3],
            tint: [1.0; 3],
            terrain: None,
        };
        let w = World::from_map(&map);
        let (stop, _, pierced) = w.bullet_trace(vec3(0.0, 1.5, 0.0), vec3(1.0, 0.0, 0.0), 50.0);
        assert!((stop - 6.0).abs() < 1e-3, "the brick wall stops it: {stop}");
        let mats: Vec<Mat> = pierced.iter().map(|p| p.mat).collect();
        assert_eq!(mats, vec![Mat::Cloth, Mat::Wood]);
        let left: f32 = pierced.iter().filter_map(|p| p.mat.bullet_pass()).product();
        assert!((left - 0.95 * 0.7).abs() < 1e-4);
        // eyes stop at the cloth, feet at the wood
        assert!(
            (w.raycast(vec3(0.0, 1.5, 0.0), vec3(1.0, 0.0, 0.0), 50.0)
                .unwrap()
                .0
                - 2.0)
                .abs()
                < 1e-3
        );
        assert!(
            fits(&w, vec3(2.0, 0.0, 0.0), Stance::Stand),
            "walk through cloth"
        );
        assert!(
            !fits(&w, vec3(4.05, 0.0, 0.0), Stance::Stand),
            "not through planks"
        );
    }

    #[test]
    fn climbs_a_ladder_onto_the_roof_and_back_down() {
        let mut b = crate::map::MapBuilder::new();
        b.ground_and_bounds(vec2(-20.0, -20.0), vec2(20.0, 20.0), [0; 3], [0; 3], 5.0);
        // a 4.2 m house from x = 0 to 8; the ladder on its west wall
        b.building(0.0, -4.0, 8.0, 4.0, 0.0, 4.2, &[], [200; 3], Some([150; 3]));
        let top = 4.2 + 0.35;
        b.ladder(vec3(0.0, 0.0, 0.0), vec3(-1.0, 0.0, 0.0), top);
        let map = Map {
            id: MapId::Depot,
            name: "test",
            min: vec2(-20.0, -20.0),
            max: vec2(20.0, 20.0),
            boxes: b.boxes,
            ladders: b.ladders,
            spawns: [Vec::new(), Vec::new()],
            bomb: Vec3::ZERO,
            sites: [Vec3::ZERO; 2],
            sky: [0; 3],
            fog: [0; 3],
            tint: [1.0; 3],
            terrain: None,
        };
        let w = World::from_map(&map);
        let t = mt();
        let east = -std::f32::consts::FRAC_PI_2; // yaw looking towards +x (the wall)
        let mut body = Body {
            pos: vec3(-1.5, 0.0, 0.0),
            on_ground: true,
            ..Default::default()
        };
        let run = |body: &mut Body, yaw: f32, forward: f32, secs: f32| {
            for _ in 0..(secs * 60.0) as usize {
                let inp = MoveInput {
                    forward,
                    yaw,
                    ..Default::default()
                };
                body.step(&w, &inp, &t, 1.0 / 60.0);
            }
        };
        // walking along the wall right past the ladder's foot does not grab it
        let mut passer = Body {
            pos: vec3(-0.9, 0.0, -3.0),
            on_ground: true,
            ..Default::default()
        };
        run(&mut passer, std::f32::consts::PI, 1.0, 1.2);
        assert!(
            passer.pos.z > 1.0 && passer.pos.y < 0.05,
            "walked past: {}",
            passer.pos
        );
        // climb until standing on the roof
        let mut secs = 0.0;
        while !(body.on_ground && body.pos.y > top - 0.05 && body.pos.x > 0.3) {
            run(&mut body, east, 1.0, 0.05);
            secs += 0.05;
            assert!(
                secs < 4.0,
                "stuck at {} (climbing {})",
                body.pos,
                body.climbing
            );
        }
        assert!(!body.climbing);
        assert!(secs > 1.0, "it takes a moment to climb {top} m: {secs} s");
        // walk back off the roof facing away from the wall: onto the ladder and down
        run(&mut body, east + std::f32::consts::PI, 1.0, 5.0);
        assert!(
            body.pos.y < 0.1 && body.pos.x < 0.0,
            "back on the ground: {}",
            body.pos
        );
    }

    /// A map with only terrain: a gentle ramp up to x = 20, then a cliff.
    fn slope_world() -> World {
        let mut hm = crate::terrain::Heightmap::flat(vec2(-30.0, -30.0), vec2(40.0, 30.0), 1.0, 0.0);
        for j in 0..hm.h {
            for i in 0..hm.w {
                let p = hm.point(i, j);
                let y = if p.x < 0.0 {
                    0.0
                } else if p.x < 20.0 {
                    p.x * 0.4 // 22°
                } else {
                    8.0 + (p.x - 20.0) * 3.0 // 72°: a cliff
                };
                hm.set(i, j, y);
            }
        }
        let map = Map {
            id: MapId::Depot,
            name: "slope",
            min: vec2(-30.0, -30.0),
            max: vec2(40.0, 30.0),
            boxes: Vec::new(),
            ladders: Vec::new(),
            spawns: [Vec::new(), Vec::new()],
            bomb: Vec3::ZERO,
            sites: [Vec3::ZERO; 2],
            sky: [0; 3],
            fog: [0; 3],
            tint: [1.0; 3],
            terrain: Some(std::sync::Arc::new(hm)),
        };
        World::from_map(&map)
    }

    #[test]
    fn walks_up_gentle_slopes_but_not_cliffs() {
        let w = slope_world();
        let t = mt();
        let east = -std::f32::consts::FRAC_PI_2;
        let mut b = Body {
            pos: vec3(-5.0, 0.0, 0.0),
            on_ground: true,
            ..Default::default()
        };
        let mut highest = 0.0f32;
        for _ in 0..(8.0 * 60.0) as usize {
            let inp = MoveInput {
                forward: 1.0,
                yaw: east,
                ..Default::default()
            };
            b.step(&w, &inp, &t, 1.0 / 60.0);
            let g = w.ground(b.pos.x, b.pos.z).unwrap();
            assert!(b.pos.y >= g - 1e-3, "never inside the ground: {} < {g}", b.pos.y);
            highest = highest.max(b.pos.y);
        }
        assert!(highest > 7.5, "walked up the ramp: {highest}");
        assert!(b.pos.x < 20.5 && b.pos.y < 9.0, "stopped at the cliff: {}", b.pos);
        // a jump lands back on the slope, and walking down it stays on the ground
        let mut b = Body {
            pos: vec3(10.0, 4.0, 0.0),
            on_ground: true,
            ..Default::default()
        };
        b.step(&w, &MoveInput { jump: true, ..Default::default() }, &t, 1.0 / 60.0);
        for _ in 0..60 {
            b.step(&w, &MoveInput::default(), &t, 1.0 / 60.0);
        }
        assert!(b.on_ground && (b.pos.y - 4.0).abs() < 0.05, "landed: {}", b.pos);
        let mut airborne = 0;
        for _ in 0..160 {
            let inp = MoveInput {
                forward: 1.0,
                yaw: -east,
                ..Default::default()
            };
            b.step(&w, &inp, &t, 1.0 / 60.0);
            if !b.on_ground {
                airborne += 1;
            }
        }
        assert!(airborne < 5, "ran down the slope without bouncing: {airborne} frames in the air");
        assert!(b.pos.x < 2.0, "{}", b.pos);
    }

    fn mt() -> MoveTuning {
        MoveTuning::default()
    }

    /// One jump's worth of air time (0.66 s), sprinting forward at the start.
    fn jump_speed_after(strafe: f32, turn_rate: f32) -> f32 {
        let t = mt();
        let mut yaw = 0.0f32;
        let (f, _) = yaw_basis(yaw);
        let mut hv = f * t.sprint_speed;
        let dt = 1.0 / 60.0;
        for _ in 0..40 {
            yaw += turn_rate * dt;
            let (fwd, right) = yaw_basis(yaw);
            let inp = MoveInput {
                forward: 0.0,
                right: strafe,
                yaw,
                sprint: true,
                ..Default::default()
            };
            let wish = (fwd * 0.0 + right * strafe) * t.walk_speed * t.strafe_mult;
            hv = air_move(hv, wish, &inp, &t, dt);
        }
        hv.length()
    }

    #[test]
    fn strafe_jump_gains_a_little_speed() {
        let t = mt();
        // no keys: momentum is kept, nothing brakes in the air
        let plain = jump_speed_after(0.0, 0.0);
        assert!((plain - t.sprint_speed).abs() < 1e-3, "{plain}");
        // holding D while turning right (yaw goes down) speeds up
        let strafe = jump_speed_after(1.0, -3.0);
        assert!(strafe > plain + 0.5, "{strafe} vs {plain}");
        // never past the cap, which stays inside the server's movement check
        let cap = t.sprint_speed * t.strafe_jump;
        assert!(strafe <= cap + 1e-3, "{strafe} > {cap}");
        assert!(cap < t.sprint_speed * 1.2);
        // just holding D, or turning the wrong way, gives next to nothing
        for turn in [0.0, 2.5] {
            let v = jump_speed_after(1.0, turn);
            assert!(v < plain + 0.1, "{turn}: {v}");
        }
    }

    #[test]
    fn heavy_weapon_accelerates_slower_and_sprints_slower() {
        let (_, w) = world();
        let run = |weight: f32, sprint_mult: f32| {
            let mut b = Body {
                pos: vec3(0.0, 0.0, 20.0),
                on_ground: true,
                ..Default::default()
            };
            let inp = MoveInput {
                forward: 1.0,
                sprint: true,
                weight,
                sprint_mult,
                ..Default::default()
            };
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
        let mut a = Body {
            pos: vec3(0.0, 0.0, 20.0),
            on_ground: true,
            ..Default::default()
        };
        let mut b = a;
        for _ in 0..60 {
            a.step(
                &w,
                &MoveInput {
                    forward: 1.0,
                    ..Default::default()
                },
                &mt(),
                1.0 / 60.0,
            );
            b.step(
                &w,
                &MoveInput {
                    forward: -1.0,
                    ..Default::default()
                },
                &mt(),
                1.0 / 60.0,
            );
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
            b.step(
                w,
                &MoveInput {
                    yaw: 0.0,
                    ..Default::default()
                },
                &mt(),
                1.0 / 60.0,
            );
        }
    }

    #[test]
    fn stands_on_ground_and_walks() {
        let (_, w) = world();
        let mut b = Body {
            pos: vec3(0.0, 2.0, 20.0),
            ..Default::default()
        };
        settle(&w, &mut b);
        assert!(b.on_ground);
        assert!(b.pos.y.abs() < 0.01, "y={}", b.pos.y);
        let start = b.pos;
        let inp = MoveInput {
            forward: 1.0,
            yaw: 0.0,
            ..Default::default()
        };
        for _ in 0..60 {
            b.step(&w, &inp, &mt(), 1.0 / 60.0);
        }
        let moved = start.z - b.pos.z;
        assert!(
            moved > 4.0 && moved < mt().walk_speed + 0.1,
            "moved {moved}"
        );
    }

    #[test]
    fn walls_block() {
        let (_, w) = world();
        // walk west into the perimeter wall
        let mut b = Body {
            pos: vec3(-38.0, 0.0, 25.0),
            ..Default::default()
        };
        settle(&w, &mut b);
        let inp = MoveInput {
            forward: 1.0,
            yaw: std::f32::consts::FRAC_PI_2,
            ..Default::default()
        };
        for _ in 0..120 {
            b.step(&w, &inp, &mt(), 1.0 / 60.0);
        }
        assert!(b.pos.x >= -40.0 + PLAYER_RADIUS - 0.01, "x={}", b.pos.x);
    }

    #[test]
    fn climbs_office_stairs() {
        let (_, w) = world();
        // bottom of the office stairs, climbing towards -z
        let mut b = Body {
            pos: vec3(30.35, 0.0, -1.5),
            ..Default::default()
        };
        settle(&w, &mut b);
        let inp = MoveInput {
            forward: 1.0,
            yaw: 0.0,
            ..Default::default()
        };
        for _ in 0..150 {
            b.step(&w, &inp, &mt(), 1.0 / 60.0);
        }
        assert!(b.pos.y > 3.0, "y={} z={}", b.pos.y, b.pos.z);
        // every step-up is reported so the camera can smooth it
        assert!(b.take_snap() > 2.5);
        assert_eq!(b.snap, 0.0);
        // step west off the top onto the floor
        let inp = MoveInput {
            forward: 1.0,
            yaw: std::f32::consts::FRAC_PI_2,
            ..Default::default()
        };
        for _ in 0..60 {
            b.step(&w, &inp, &mt(), 1.0 / 60.0);
        }
        assert!(b.pos.y > 3.3 && b.pos.x < 29.0, "pos={:?}", b.pos);
    }

    #[test]
    fn jump_and_land() {
        let (_, w) = world();
        let mut b = Body {
            pos: vec3(0.0, 0.0, 20.0),
            ..Default::default()
        };
        settle(&w, &mut b);
        b.step(
            &w,
            &MoveInput {
                jump: true,
                ..Default::default()
            },
            &mt(),
            1.0 / 60.0,
        );
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
        let mut b = Body {
            pos: vec3(0.0, 0.0, 20.0),
            ..Default::default()
        };
        settle(&w, &mut b);
        b.step(
            &w,
            &MoveInput {
                want_stance: Stance::Prone,
                ..Default::default()
            },
            &mt(),
            1.0 / 60.0,
        );
        assert_eq!(b.stance, Stance::Prone);
        b.step(
            &w,
            &MoveInput {
                want_stance: Stance::Stand,
                ..Default::default()
            },
            &mt(),
            1.0 / 60.0,
        );
        assert_eq!(b.stance, Stance::Stand);
    }

    #[test]
    fn all_spawns_fit() {
        for id in MapId::ALL {
            let m = maps::build(id);
            let w = World::from_map(&m);
            for s in m.all_spawns() {
                assert!(
                    fits(&w, s.pos + vec3(0.0, 0.01, 0.0), Stance::Stand),
                    "{:?} spawn {:?}",
                    id,
                    s.pos
                );
            }
            for p in [m.bomb, m.sites[0], m.sites[1]] {
                assert!(
                    fits(&w, p + vec3(0.0, 0.01, 0.0), Stance::Stand),
                    "{:?} objective {:?}",
                    id,
                    p
                );
            }
        }
    }
}
