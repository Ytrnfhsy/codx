//! Steering a killstreak vehicle: the FPV drone (first person), the bomber drone (looking
//! down, with a marker where a bomb would land) and the ground robot (third person).
//!
//! Like the soldier, the vehicle moves on this client at once and the server checks it;
//! `C2S::Drive` carries the position and the held keys.

use crate::game::GameClient;
use crate::ui::{Ui, ACCENT, BAD, GOOD, TEXT};
use macroquad::prelude::*;
use toon_shared::look_dir;
use toon_shared::physics::{Body, Stance};
use toon_shared::protocol::{VehicleHud, C2S};
use toon_shared::vehicles::*;

pub struct Piloting {
    pub id: u16,
    pub kind: VehicleKind,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// FPV forward speed.
    pub speed: f32,
    pub vel: Vec3,
    /// Robot physics.
    pub body: Body,
    pub keys: u8,
    pub hp: f32,
    pub time_left: f32,
    pub ammo: u8,
    /// Seconds since we took control.
    pub age: f32,
}

impl Piloting {
    fn new(h: &VehicleHud, pitch: f32) -> Piloting {
        let pos = Vec3::from_array(h.pos);
        Piloting {
            id: h.id,
            kind: h.kind,
            pos,
            yaw: h.yaw,
            pitch: match h.kind {
                VehicleKind::Fpv => pitch.max(0.0),
                VehicleKind::Bomber => -1.45,
                VehicleKind::Nrk => 0.0,
            },
            speed: FPV_SPEED,
            vel: Vec3::ZERO,
            body: Body {
                pos,
                stance: Stance::Crouch,
                on_ground: true,
                ..Default::default()
            },
            keys: 0,
            hp: h.hp,
            time_left: h.time_left,
            ammo: h.ammo,
            age: 0.0,
        }
    }

    /// Where a bomb dropped now would land (bomber).
    pub fn bomb_landing(&self, g: &GameClient) -> Vec3 {
        let from = self.pos - vec3(0.0, 0.8, 0.0);
        let ground = g
            .world
            .floor_below(vec3(from.x, BOMBER_ALT - 1.0, from.z), BOMBER_ALT + 2.0)
            .unwrap_or(0.0);
        let mut p = drop_landing(
            from,
            vec3(self.vel.x, -1.5, self.vel.z),
            g.tuning.movement.gravity,
            ground,
        );
        p.y = g
            .world
            .floor_below(vec3(p.x, BOMBER_ALT - 1.0, p.z), BOMBER_ALT + 2.0)
            .unwrap_or(ground);
        p
    }
}

impl GameClient {
    /// Follows what the server says about our vehicle: start steering, stop, or pull the
    /// local copy back when it drifted too far.
    pub fn sync_piloting(&mut self, hud: Option<VehicleHud>) {
        let Some(h) = hud else {
            self.piloting = None;
            return;
        };
        match &mut self.piloting {
            Some(p) if p.id == h.id => {
                p.hp = h.hp;
                p.time_left = h.time_left;
                p.ammo = h.ammo;
                let server = Vec3::from_array(h.pos);
                let slack = match p.kind {
                    VehicleKind::Fpv => 8.0,
                    _ => 4.0,
                };
                if server.distance(p.pos) > slack {
                    p.pos = server;
                    p.body.pos = server;
                }
            }
            _ => self.piloting = Some(Piloting::new(&h, self.pitch)),
        }
    }

    /// Mouse look while steering.
    pub fn pilot_look(&mut self, md: Vec2, sens: f32, invert: bool) {
        let Some(p) = &mut self.piloting else { return };
        p.yaw = toon_shared::wrap_angle(p.yaw - md.x * sens);
        let dy = md.y * sens * if invert { -1.0 } else { 1.0 };
        match p.kind {
            VehicleKind::Fpv => p.pitch = (p.pitch - dy).clamp(-1.45, 1.45),
            VehicleKind::Nrk => p.pitch = (p.pitch - dy).clamp(-0.6, 0.7),
            // the bomber camera tilts between straight down and ahead
            VehicleKind::Bomber => p.pitch = (p.pitch - dy).clamp(-1.5, -0.6),
        }
    }

    /// Keys and movement of the vehicle for this frame.
    pub fn update_piloting(&mut self, dt: f32, can_act: bool) {
        let key = |k: KeyCode| can_act && is_key_down(k);
        let Some(p) = &mut self.piloting else { return };
        let mut fwd = 0.0;
        let mut right = 0.0;
        if key(KeyCode::W) {
            fwd += 1.0;
        }
        if key(KeyCode::S) {
            fwd -= 1.0;
        }
        if key(KeyCode::D) {
            right += 1.0;
        }
        if key(KeyCode::A) {
            right -= 1.0;
        }
        let mut keys = 0;
        // left mouse button or Space
        if can_act && (is_mouse_button_down(MouseButton::Left) || is_key_down(KeyCode::Space)) {
            keys |= keys::FIRE;
        }
        if can_act && is_key_pressed(KeyCode::E) {
            keys |= keys::EXIT;
        }
        let dt = dt.min(0.1);
        p.age += dt;
        let before = p.pos;
        match p.kind {
            VehicleKind::Fpv => {
                let want = if fwd > 0.0 {
                    keys |= keys::BOOST;
                    FPV_BOOST
                } else if fwd < 0.0 {
                    keys |= keys::BRAKE;
                    FPV_SLOW
                } else {
                    FPV_SPEED
                };
                p.speed += (want - p.speed) * (dt * FPV_ACCEL).min(1.0);
                let dir = look_dir(p.yaw, p.pitch);
                let (to, hit) = fpv_step(&self.world, p.pos, dir, p.speed, dt);
                p.pos = to;
                // touching anything sets the charge off
                if hit && p.age >= FPV_ARM_TIME {
                    keys |= keys::FIRE;
                }
            }
            VehicleKind::Bomber => {
                p.pos = bomber_step(p.pos, p.yaw, fwd, right, dt, self.map.min, self.map.max);
            }
            VehicleKind::Nrk => {
                let mut t = dt;
                while t > 0.0 {
                    let h = t.min(1.0 / 60.0);
                    nrk_step(
                        &self.world,
                        &mut p.body,
                        fwd,
                        right,
                        p.yaw,
                        &self.tuning.movement,
                        h,
                    );
                    t -= h;
                }
                p.pos = p.body.pos;
            }
        }
        if dt > 0.0 {
            p.vel = (p.pos - before) / dt;
        }
        // the exit press must reach the server even between two sends
        p.keys = (p.keys & keys::EXIT) | keys;
    }

    /// Sends the vehicle state (called at the input rate).
    pub fn send_drive(&mut self) {
        let Some(p) = &mut self.piloting else { return };
        self.net.send_unreliable(C2S::Drive {
            pos: p.pos.to_array(),
            yaw: p.yaw,
            pitch: p.pitch,
            keys: p.keys,
        });
        if p.keys & keys::EXIT != 0 {
            // say it twice, reliably, so leaving is never lost
            self.net.send(C2S::Drive {
                pos: p.pos.to_array(),
                yaw: p.yaw,
                pitch: p.pitch,
                keys: p.keys,
            });
            p.keys &= !keys::EXIT;
        }
    }

    /// Camera while steering: (position, yaw, pitch, horizontal field of view in degrees).
    pub fn pilot_camera(&self) -> Option<(Vec3, f32, f32, Option<f32>)> {
        let p = self.piloting.as_ref()?;
        Some(match p.kind {
            VehicleKind::Fpv => (p.pos, p.yaw, p.pitch, Some(105.0)),
            VehicleKind::Bomber => (p.pos - vec3(0.0, 1.0, 0.0), p.yaw, p.pitch, Some(80.0)),
            VehicleKind::Nrk => {
                // over the shoulder, pulled in when a wall is behind
                let pivot = p.pos + vec3(0.0, 1.7, 0.0);
                let back = -look_dir(p.yaw, p.pitch);
                let want = 3.4;
                let dist = self
                    .world
                    .raycast(pivot, back, want)
                    .map(|(t, _)| (t - 0.25).max(0.3))
                    .unwrap_or(want);
                (pivot + back * dist, p.yaw, p.pitch - 0.12, None)
            }
        })
    }

    /// The vehicle's own HUD: what it is, time and health left, what the keys do.
    pub fn draw_pilot_hud(&self, ui: &Ui) {
        let Some(p) = &self.piloting else { return };
        let (w, h) = (screen_width(), screen_height());
        let (cx, cy) = (w / 2.0, h / 2.0);
        let dur = p.kind.duration();
        let left = p.time_left / dur;
        match p.kind {
            VehicleKind::Fpv => {
                // a cheap video link: scanlines, noise and a vignette that worsen with time
                let noise = (1.0 - left).clamp(0.0, 1.0);
                for k in 0..(h as i32 / 4) {
                    draw_rectangle(0.0, k as f32 * 4.0, w, 1.0, Color::new(0.0, 0.0, 0.0, 0.10));
                }
                let n = (40.0 + 260.0 * noise) as i32;
                for k in 0..n {
                    let s = ((self.now * 977.0) as i64 + k as i64 * 7919) as f32;
                    let x = (s * 12.9898).sin().abs().fract() * w;
                    let y = (s * 78.233).sin().abs().fract() * h;
                    draw_rectangle(
                        x,
                        y,
                        2.0 + (k % 3) as f32,
                        1.0,
                        Color::new(1.0, 1.0, 1.0, 0.25),
                    );
                }
                draw_circle_lines(cx, cy, 18.0, 2.0, Color::new(1.0, 1.0, 1.0, 0.8));
                draw_line(cx - 30.0, cy, cx - 10.0, cy, 2.0, WHITE);
                draw_line(cx + 10.0, cy, cx + 30.0, cy, 2.0, WHITE);
                ui.shadowed(
                    &format!("{:.0} км/год", p.speed * 3.6),
                    cx + 40.0,
                    cy + 6.0,
                    18,
                    TEXT,
                );
            }
            VehicleKind::Bomber => {
                // the red ring on the ground is the sight; bombs left under the top band
                for k in 0..BOMBER_DROPS {
                    let c = if k < p.ammo {
                        Color::from_rgba(150, 170, 100, 255)
                    } else {
                        Color::new(0.3, 0.3, 0.3, 0.6)
                    };
                    let x = cx - 60.0 + k as f32 * 24.0;
                    draw_circle(x, 118.0, 8.0, c);
                    draw_circle_lines(x, 118.0, 8.0, 2.0, BLACK);
                }
            }
            VehicleKind::Nrk => {
                let c = Color::new(1.0, 1.0, 1.0, 0.85);
                for (dx, dy) in [(-1.0f32, 0.0f32), (1.0, 0.0), (0.0, -1.0), (0.0, 1.0)] {
                    draw_line(
                        cx + dx * 6.0,
                        cy + dy * 6.0,
                        cx + dx * 16.0,
                        cy + dy * 16.0,
                        2.0,
                        c,
                    );
                }
            }
        }
        // top band: name, battery/time, health
        let bw = 360.0;
        let bx = cx - bw / 2.0;
        draw_rectangle(bx, 44.0, bw, 58.0, Color::new(0.0, 0.0, 0.0, 0.5));
        ui.shadowed(p.kind.name(), bx + 12.0, 66.0, 18, ACCENT);
        let tc = if left < 0.25 { BAD } else { TEXT };
        let t = format!("{:.0} с", p.time_left.ceil());
        ui.shadowed(&t, bx + bw - 12.0 - ui.width(&t, 18, true), 66.0, 18, tc);
        let hp = (p.hp / p.kind.health()).clamp(0.0, 1.0);
        draw_rectangle(
            bx + 12.0,
            78.0,
            bw - 24.0,
            8.0,
            Color::new(0.2, 0.2, 0.2, 0.8),
        );
        draw_rectangle(
            bx + 12.0,
            78.0,
            (bw - 24.0) * hp,
            8.0,
            if hp > 0.35 { GOOD } else { BAD },
        );
        draw_rectangle(bx + 12.0, 90.0, (bw - 24.0) * left, 4.0, ACCENT);
        let help = match p.kind {
            VehicleKind::Fpv => "W/S — швидше/повільніше · ЛКМ/Пробіл або E — підірвати",
            VehicleKind::Bomber => {
                "WASD — летіти · миша — розворот · ЛКМ/Пробіл — скинути · E — вийти"
            }
            VehicleKind::Nrk => "WASD — їхати · ЛКМ/Пробіл — кулемет · E — вийти",
        };
        ui.centered(help, cx, h - 24.0, 16, TEXT);
    }
}
