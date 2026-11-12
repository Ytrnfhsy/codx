//! Cartoon particles: tracers, dust puffs, "pow" stars, fireballs and smoke.

use crate::mesh::MeshBuilder;
use macroquad::prelude::*;
use toon_shared::rng::Rng;

#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Puff,
    Spark,
    Fire,
    Smoke,
    Star,
    Flash,
}

struct Particle {
    pos: Vec3,
    vel: Vec3,
    life: f32,
    max: f32,
    size: f32,
    grow: f32,
    color: [u8; 3],
    kind: Kind,
    grav: f32,
}

struct Tracer {
    a: Vec3,
    b: Vec3,
    t: f32,
}

/// What a bullet hit (how it looks when it does).
#[derive(Clone, Copy, PartialEq)]
pub enum Surface {
    /// Earth, grass, sand: clods fly up.
    Dirt,
    /// Walls, concrete, metal: dust, chips and sparks.
    Hard,
}

/// A bullet mark left on a surface for a while.
struct Mark {
    pos: Vec3,
    normal: Vec3,
    age: f32,
    size: f32,
}

const MARK_LIFE: f32 = 18.0;
const MAX_MARKS: usize = 120;

pub struct Effects {
    marks: std::collections::VecDeque<Mark>,
    /// Most particles at once (the effects setting).
    pub cap: usize,
    parts: Vec<Particle>,
    tracers: Vec<Tracer>,
    rng: Rng,
}

impl Effects {
    pub fn new() -> Effects {
        Effects {
            cap: 1500,
            marks: Default::default(),
            parts: Vec::new(),
            tracers: Vec::new(),
            rng: Rng::new(99),
        }
    }

    fn rv(&mut self, s: f32) -> Vec3 {
        vec3(
            self.rng.range(-s, s),
            self.rng.range(-s, s),
            self.rng.range(-s, s),
        )
    }

    fn push(&mut self, p: Particle) {
        if self.parts.len() < self.cap {
            self.parts.push(p);
        }
    }

    pub fn tracer(&mut self, a: Vec3, b: Vec3) {
        if self.tracers.len() < 200 {
            self.tracers.push(Tracer { a, b, t: 0.0 });
        }
    }

    /// A bullet striking the world at `pos` on a surface facing `normal`: what flies off
    /// depends on what it hit, and a mark stays behind for a while.
    pub fn hit(&mut self, pos: Vec3, normal: Vec3, surface: Surface) {
        match surface {
            Surface::Dirt => {
                // clods thrown up and a low puff of dust, no sparks
                for _ in 0..5 {
                    let v = self.rv(1.4) + normal * self.rng.range(2.0, 4.0);
                    let c = self.rng.range(0.8, 1.1);
                    let (max, size) = (self.rng.range(0.35, 0.6), self.rng.range(0.03, 0.06));
                    self.push(Particle {
                        pos,
                        vel: v,
                        life: 0.0,
                        max,
                        size,
                        grow: 0.0,
                        color: [(112.0 * c) as u8, (90.0 * c) as u8, (60.0 * c) as u8],
                        kind: Kind::Spark,
                        grav: 11.0,
                    });
                }
                let v = self.rv(0.5) + normal * 0.9;
                self.push(Particle {
                    pos,
                    vel: v,
                    life: 0.0,
                    max: 0.6,
                    size: 0.1,
                    grow: 0.5,
                    color: [176, 158, 122],
                    kind: Kind::Puff,
                    grav: 0.6,
                });
            }
            Surface::Hard => {
                self.impact(pos);
                // chips of the wall bounce off it
                for _ in 0..3 {
                    let v = self.rv(1.6) + normal * self.rng.range(1.5, 3.5);
                    let (max, size) = (self.rng.range(0.3, 0.5), self.rng.range(0.02, 0.04));
                    self.push(Particle {
                        pos,
                        vel: v,
                        life: 0.0,
                        max,
                        size,
                        grow: 0.0,
                        color: [150, 146, 138],
                        kind: Kind::Spark,
                        grav: 10.0,
                    });
                }
            }
        }
        if self.marks.len() >= MAX_MARKS {
            self.marks.pop_front();
        }
        self.marks.push_back(Mark {
            pos: pos + normal * 0.012,
            normal,
            age: 0.0,
            size: self.rng.range(0.035, 0.06) * if surface == Surface::Dirt { 1.5 } else { 1.0 },
        });
    }

    /// A raindrop hitting the ground: a small pale splash.
    pub fn splash(&mut self, pos: Vec3) {
        let v = self.rv(0.5) + vec3(0.0, 1.1, 0.0);
        self.push(Particle {
            pos,
            vel: v,
            life: 0.0,
            max: 0.22,
            size: 0.035,
            grow: 0.25,
            color: [210, 222, 236],
            kind: Kind::Puff,
            grav: 6.0,
        });
    }

    pub fn impact(&mut self, pos: Vec3) {
        for _ in 0..3 {
            let v = self.rv(1.2) + vec3(0.0, 1.2, 0.0);
            self.push(Particle {
                pos,
                vel: v,
                life: 0.0,
                max: 0.45,
                size: 0.08,
                grow: 0.35,
                color: [214, 196, 160],
                kind: Kind::Puff,
                grav: 1.0,
            });
        }
        let v = self.rv(3.0) + vec3(0.0, 2.0, 0.0);
        self.push(Particle {
            pos,
            vel: v,
            life: 0.0,
            max: 0.2,
            size: 0.04,
            grow: 0.0,
            color: [255, 230, 120],
            kind: Kind::Spark,
            grav: 9.0,
        });
    }

    /// A bullet going through planks (splinters) or cloth (a few shreds).
    pub fn pierce(&mut self, pos: Vec3, wood: bool) {
        let (n, color, size) = if wood {
            (4, [150, 104, 60], 0.05)
        } else {
            (2, [235, 225, 205], 0.07)
        };
        for _ in 0..n {
            let v = self.rv(2.0) + vec3(0.0, 0.8, 0.0);
            self.push(Particle {
                pos,
                vel: v,
                life: 0.0,
                max: 0.5,
                size,
                grow: 0.0,
                color,
                kind: Kind::Spark,
                grav: 9.0,
            });
        }
    }

    pub fn flesh(&mut self, pos: Vec3) {
        self.push(Particle {
            pos,
            vel: Vec3::ZERO,
            life: 0.0,
            max: 0.22,
            size: 0.18,
            grow: 1.2,
            color: [255, 80, 60],
            kind: Kind::Star,
            grav: 0.0,
        });
        for _ in 0..4 {
            let v = self.rv(2.5) + vec3(0.0, 1.5, 0.0);
            self.push(Particle {
                pos,
                vel: v,
                life: 0.0,
                max: 0.4,
                size: 0.05,
                grow: 0.0,
                color: [255, 220, 80],
                kind: Kind::Spark,
                grav: 6.0,
            });
        }
    }

    pub fn muzzle(&mut self, pos: Vec3) {
        self.push(Particle {
            pos,
            vel: Vec3::ZERO,
            life: 0.0,
            max: 0.05,
            size: 0.14,
            grow: 1.0,
            color: [255, 220, 110],
            kind: Kind::Flash,
            grav: 0.0,
        });
        // a wisp of smoke drifting up from the barrel
        if self.parts.len() < self.cap / 2 {
            let v = self.rv(0.15) + vec3(0.0, 0.5, 0.0);
            self.push(Particle {
                pos,
                vel: v,
                life: 0.0,
                max: 0.55,
                size: 0.05,
                grow: 0.35,
                color: [196, 196, 200],
                kind: Kind::Smoke,
                grav: 0.0,
            });
        }
    }

    pub fn explosion(&mut self, pos: Vec3, radius: f32) {
        let s = (radius / 6.0).clamp(0.6, 2.2);
        self.push(Particle {
            pos,
            vel: Vec3::ZERO,
            life: 0.0,
            max: 0.45,
            size: 0.6 * s,
            grow: 5.0 * s,
            color: [255, 170, 50],
            kind: Kind::Fire,
            grav: 0.0,
        });
        self.push(Particle {
            pos: pos + vec3(0.0, 0.3, 0.0),
            vel: Vec3::ZERO,
            life: 0.0,
            max: 0.3,
            size: 0.5 * s,
            grow: 4.0 * s,
            color: [255, 240, 140],
            kind: Kind::Fire,
            grav: 0.0,
        });
        let n = (10.0 * s) as usize;
        for _ in 0..n {
            let v = self.rv(3.0 * s) + vec3(0.0, 2.5 * s, 0.0);
            let max = self.rng.range(1.2, 2.2);
            self.push(Particle {
                pos,
                vel: v,
                life: 0.0,
                max,
                size: 0.5 * s,
                grow: 1.2 * s,
                color: [120, 120, 128],
                kind: Kind::Smoke,
                grav: -0.6,
            });
        }
        for _ in 0..n {
            let v = self.rv(9.0 * s) + vec3(0.0, 5.0 * s, 0.0);
            self.push(Particle {
                pos,
                vel: v,
                life: 0.0,
                max: 0.7,
                size: 0.08,
                grow: 0.0,
                color: [255, 200, 80],
                kind: Kind::Spark,
                grav: 12.0,
            });
        }
    }

    /// Flames licking up from a patch of fire.
    pub fn flames(&mut self, c: Vec3, r: f32) {
        for _ in 0..3 {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let d = self.rng.range(0.0, r);
            let pos = c + vec3(a.cos() * d, 0.1, a.sin() * d);
            let hot = self.rng.range(0.0, 1.0);
            let up = self.rng.range(1.5, 3.0);
            self.push(Particle {
                pos,
                vel: vec3(0.0, up, 0.0),
                life: 0.0,
                max: 0.5,
                size: 0.35,
                grow: -0.4,
                color: [255, (120.0 + hot * 110.0) as u8, 30],
                kind: Kind::Spark,
                grav: -1.0,
            });
        }
    }

    /// Red signal smoke rising from a landed supply crate.
    pub fn flare_smoke(&mut self, pos: Vec3) {
        let v = self.rv(0.4) + vec3(0.0, 2.6, 0.0);
        self.push(Particle {
            pos,
            vel: v,
            life: 0.0,
            max: 4.0,
            size: 0.5,
            grow: 0.9,
            color: [230, 70, 60],
            kind: Kind::Smoke,
            grav: -0.3,
        });
    }

    /// A thin, short-lived trail behind a flying grenade: just enough to show its arc.
    pub fn smoke_trail(&mut self, pos: Vec3) {
        self.push(Particle {
            pos,
            vel: Vec3::ZERO,
            life: 0.0,
            max: 0.6,
            size: 0.07,
            grow: 0.05,
            color: [235, 235, 240],
            kind: Kind::Smoke,
            grav: 0.0,
        });
    }

    pub fn update(&mut self, dt: f32) {
        for p in &mut self.parts {
            p.life += dt;
            p.vel.y -= p.grav * dt;
            if p.kind == Kind::Smoke || p.kind == Kind::Puff {
                p.vel *= 1.0 - (dt * 1.5).min(1.0);
            }
            p.pos += p.vel * dt;
            p.size += p.grow * dt;
        }
        self.parts.retain(|p| p.life < p.max);
        for m in &mut self.marks {
            m.age += dt;
        }
        while self.marks.front().is_some_and(|m| m.age > MARK_LIFE) {
            self.marks.pop_front();
        }
        for t in &mut self.tracers {
            t.t += dt;
        }
        self.tracers.retain(|t| t.t < 0.07);
    }

    /// Adds particles to an (alpha-blended) mesh.
    pub fn build(&self, m: &mut MeshBuilder, cam: Vec3) {
        // bullet marks: dark spots lying on what was hit, fading at the end
        for k in &self.marks {
            if k.pos.distance_squared(cam) > 60.0 * 60.0 {
                continue;
            }
            let side = if k.normal.y.abs() > 0.9 {
                Vec3::X
            } else {
                Vec3::Y
            };
            let t = k.normal.cross(side).normalize_or_zero() * k.size;
            let b = k.normal.cross(t);
            let a = ((MARK_LIFE - k.age) / 3.0).clamp(0.0, 1.0);
            m.add_quad(
                [k.pos - t - b, k.pos + t - b, k.pos + t + b, k.pos - t + b],
                [26, 22, 20, (a * 210.0) as u8],
            );
        }
        for p in &self.parts {
            let k = p.life / p.max;
            let alpha = match p.kind {
                Kind::Smoke => ((1.0 - k) * 0.7 * 255.0) as u8,
                Kind::Fire => ((1.0 - k * k) * 255.0) as u8,
                Kind::Flash | Kind::Star => ((1.0 - k) * 255.0) as u8,
                _ => ((1.0 - k) * 230.0) as u8,
            };
            let col = if p.kind == Kind::Fire && k > 0.5 {
                [255, 120, 40]
            } else {
                p.color
            };
            match p.kind {
                Kind::Star | Kind::Flash => {
                    // camera-facing 4-point star
                    let to = (cam - p.pos).normalize_or_zero();
                    let r = to.cross(Vec3::Y).normalize_or_zero();
                    let u = r.cross(to);
                    let s = p.size;
                    for (a, b) in [(r, u), (u, -r), (-r, -u), (-u, r)] {
                        m.add_quad(
                            [
                                p.pos,
                                p.pos + (a + b) * s * 0.35,
                                p.pos + a * s * 1.6,
                                p.pos + (a - b) * s * 0.35,
                            ],
                            [col[0], col[1], col[2], alpha],
                        );
                    }
                }
                _ => {
                    let mm = Mat4::from_translation(p.pos);
                    m.add_sphere_ex(&mm, p.size, col, 7, 4, alpha, p.kind != Kind::Smoke);
                }
            }
        }
    }

    pub fn draw_tracers(&self) {
        for t in &self.tracers {
            let k = 1.0 - t.t / 0.07;
            let d = t.b - t.a;
            let a = t.a + d * (t.t / 0.07) * 0.6;
            draw_line_3d(
                a,
                t.b.lerp(a, 0.3 * (1.0 - k)),
                Color::new(1.0, 0.92, 0.5, k),
            );
        }
    }
}
