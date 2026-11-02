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

pub struct Effects {
    parts: Vec<Particle>,
    tracers: Vec<Tracer>,
    rng: Rng,
}

impl Effects {
    pub fn new() -> Effects {
        Effects {
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
        if self.parts.len() < 1500 {
            self.parts.push(p);
        }
    }

    pub fn tracer(&mut self, a: Vec3, b: Vec3) {
        if self.tracers.len() < 200 {
            self.tracers.push(Tracer { a, b, t: 0.0 });
        }
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

    pub fn smoke_trail(&mut self, pos: Vec3) {
        let v = self.rv(0.3);
        self.push(Particle {
            pos,
            vel: v,
            life: 0.0,
            max: 0.8,
            size: 0.2,
            grow: 1.0,
            color: [200, 200, 210],
            kind: Kind::Smoke,
            grav: -0.3,
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
        for t in &mut self.tracers {
            t.t += dt;
        }
        self.tracers.retain(|t| t.t < 0.07);
    }

    /// Adds particles to an (alpha-blended) mesh.
    pub fn build(&self, m: &mut MeshBuilder, cam: Vec3) {
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
