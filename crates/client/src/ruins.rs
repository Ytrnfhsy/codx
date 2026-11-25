//! Things breaking, as the player sees it: the map is broken the same way the server
//! broke it (`toon_shared::destruct`), then the pieces that fell are shown falling and
//! the trees toppling before they join the level's meshes; chunks fly, dust rises.

use crate::audio::{Audio, Sfx};
use crate::effects::Effects;
use crate::mesh::ToonMesh;
use macroquad::prelude::*;
use toon_shared::destruct::{Blast, Change, Topple, Wreck};
use toon_shared::map::Map;
use toon_shared::physics::World;

/// How long a tree takes to go over (s).
const TOPPLE_TIME: f32 = 1.5;
const GRAVITY: f32 = 16.0;

struct Fall {
    i: usize,
    /// Starts this high above where it lands, after `delay`.
    drop: f32,
    delay: f32,
}

struct Going {
    t0: f64,
    falls: Vec<Fall>,
    topples: Vec<Topple>,
}

impl Going {
    fn length(&self) -> f32 {
        let fall = self
            .falls
            .iter()
            .map(|f| f.delay + (2.0 * f.drop / GRAVITY).sqrt())
            .fold(0.0, f32::max);
        if self.topples.is_empty() {
            fall
        } else {
            fall.max(TOPPLE_TIME)
        }
    }
}

pub struct Ruins {
    pub wreck: Wreck,
    going: Vec<Going>,
    /// Boxes still on their way down (not in the level's meshes yet).
    hidden: std::collections::HashSet<usize>,
}

impl Ruins {
    pub fn new(map: &Map) -> Ruins {
        Ruins {
            wreck: Wreck::new(map),
            going: Vec::new(),
            hidden: Default::default(),
        }
    }

    /// A blast went off now: breaks the map and starts showing it.
    #[allow(clippy::too_many_arguments)]
    pub fn blast(
        &mut self,
        b: Blast,
        map: &mut Map,
        world: &mut World,
        level: &mut crate::level::Level,
        fx: &mut Effects,
        audio: Option<&Audio>,
        ear: Vec3,
        now: f64,
    ) {
        let ch = self.wreck.blast(map, world, b);
        self.show(ch, map, level, fx, audio, ear, now);
    }

    #[allow(clippy::too_many_arguments)]
    fn show(
        &mut self,
        ch: Change,
        map: &Map,
        level: &mut crate::level::Level,
        fx: &mut Effects,
        audio: Option<&Audio>,
        ear: Vec3,
        now: f64,
    ) {
        if ch.is_empty() {
            return;
        }
        let from = (ch.min + ch.max) * 0.5;
        for b in ch.blown.iter().take(24) {
            fx.debris(b.min, b.max, b.color, from);
        }
        // the pieces fall top first a moment after the blast, roughly in order of height
        let mut falls: Vec<Fall> = ch
            .falls
            .iter()
            .map(|&(i, drop)| Fall {
                i,
                drop,
                delay: 0.05 + (map.boxes[i].min.y * 13.7).fract() * 0.25,
            })
            .collect();
        falls.retain(|f| f.drop > 0.05);
        for f in &falls {
            self.hidden.insert(f.i);
        }
        for t in &ch.topples {
            self.hidden.extend(t.boxes.iter().copied());
        }
        if let Some(a) = audio {
            if !falls.is_empty() {
                a.play_at(Sfx::Crumble, from, ear, 1.0, 120.0);
            }
            if let Some(t) = ch.topples.first() {
                a.play_at(Sfx::TreeFall, t.pivot, ear, 0.9, 90.0);
            }
        }
        if let Some((c, r)) = ch.crater {
            level.rebuild_ground(map, c, r);
        }
        let hidden = &self.hidden;
        level.rebuild(map, &ch.touched, &|i| hidden.contains(&i));
        if !falls.is_empty() || !ch.topples.is_empty() {
            self.going.push(Going {
                t0: now,
                falls,
                topples: ch.topples,
            });
        }
    }

    /// Every blast so far (to someone who just joined, or the map mended: none).
    pub fn replay(
        &mut self,
        blasts: Vec<Blast>,
        map: &mut Map,
        world: &mut World,
        level: &mut crate::level::Level,
    ) {
        self.wreck.reset(map, world);
        self.going.clear();
        self.hidden.clear();
        for b in blasts {
            self.wreck.blast(map, world, b);
        }
        *level = crate::level::Level::build(map);
    }

    /// The pieces that landed join the level; their dust rises.
    pub fn update(
        &mut self,
        map: &Map,
        level: &mut crate::level::Level,
        fx: &mut Effects,
        now: f64,
    ) {
        let mut k = 0;
        while k < self.going.len() {
            let g = &self.going[k];
            let t = (now - g.t0) as f32;
            if t < g.length() {
                k += 1;
                continue;
            }
            let g = self.going.remove(k);
            for f in &g.falls {
                self.hidden.remove(&f.i);
            }
            for tp in &g.topples {
                for i in &tp.boxes {
                    self.hidden.remove(i);
                }
            }
            // dust where the heap settled
            let mut puffs = 0;
            for f in g.falls.iter().filter(|f| f.drop > 1.0) {
                if puffs >= 6 {
                    break;
                }
                let b = &map.boxes[f.i];
                if (f.i * 7) % 5 == 0 {
                    fx.dust((b.min + b.max) * 0.5, 1.5, [170, 162, 150]);
                    puffs += 1;
                }
            }
            for tp in &g.topples {
                // (a crane or a tower raises dust all along where it hit)
                let step = (tp.boxes.len() / 6).max(1);
                for &i in tp.boxes.iter().skip(step).step_by(step).take(6) {
                    let b = &map.boxes[i];
                    fx.dust(
                        vec3(
                            (b.min.x + b.max.x) * 0.5,
                            b.min.y,
                            (b.min.z + b.max.z) * 0.5,
                        ),
                        2.0,
                        [170, 160, 140],
                    );
                }
                if let Some(&i) = tp.boxes.first() {
                    let b = &map.boxes[i];
                    fx.dust(
                        vec3(
                            (b.min.x + b.max.x) * 0.5,
                            b.min.y,
                            (b.min.z + b.max.z) * 0.5,
                        ),
                        1.6,
                        [150, 140, 110],
                    );
                }
            }
            let landed: Vec<Vec3> = g
                .falls
                .iter()
                .map(|f| f.i)
                .chain(g.topples.iter().flat_map(|t| t.boxes.iter().copied()))
                .map(|i| (map.boxes[i].min + map.boxes[i].max) * 0.5)
                .collect();
            let hidden = &self.hidden;
            level.rebuild(map, &landed, &|i| hidden.contains(&i));
        }
    }

    /// The pieces on their way down, this frame.
    pub fn build(&self, map: &Map, now: f64) -> ToonMesh {
        let mut m = ToonMesh::new();
        for g in &self.going {
            let t = (now - g.t0) as f32;
            for f in &g.falls {
                let b = &map.boxes[f.i];
                let tt = (t - f.delay).max(0.0);
                let up = (f.drop - 0.5 * GRAVITY * tt * tt).max(0.0);
                let lift = vec3(0.0, up, 0.0);
                m.fill.add_aabb(
                    b.min + lift,
                    b.max + lift,
                    b.color,
                    crate::textures::material_for(b.color),
                );
                let s = b.max - b.min;
                if s.min_element() > 0.15 {
                    m.outline.add_outline_box(
                        (b.min + b.max) * 0.5 + lift,
                        s * 0.5 + Vec3::splat(0.045),
                        crate::mesh::OUTLINE,
                    );
                }
            }
            // a tree turns from upright to lying, faster as it goes
            let k = (t / TOPPLE_TIME).clamp(0.0, 1.0);
            let angle = (1.0 - k * k) * std::f32::consts::FRAC_PI_2;
            for tp in &g.topples {
                // lying → upright is a quarter turn back about the horizontal axis across `dir`
                let axis = vec3(tp.dir.z, 0.0, -tp.dir.x);
                let rot = Mat4::from_translation(tp.pivot)
                    * Mat4::from_axis_angle(axis, -angle)
                    * Mat4::from_translation(-tp.pivot);
                for &i in &tp.boxes {
                    let b = &map.boxes[i];
                    let mm = rot * Mat4::from_translation((b.min + b.max) * 0.5);
                    m.fill.add_box_full(
                        &mm,
                        (b.max - b.min) * 0.5,
                        b.color,
                        255,
                        false,
                        crate::textures::material_for(b.color),
                    );
                }
            }
        }
        m
    }

    pub fn busy(&self) -> bool {
        !self.going.is_empty()
    }
}
