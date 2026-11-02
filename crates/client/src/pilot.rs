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
use toon_shared::tr;
use toon_shared::vehicles::*;

pub struct Piloting {
    pub id: u16,
    pub kind: VehicleKind,
    pub pos: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    /// FPV speed for the HUD.
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
    /// FPV camera bank (radians), from sliding sideways and turning.
    pub roll: f32,
    /// FPV video link quality, 1 = clear; it drops with distance and with buildings
    /// between the drone and the soldier holding the controller.
    pub signal: f32,
    /// Yaw last frame, to bank into turns.
    last_yaw: f32,
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
                VehicleKind::Fpv => pitch.clamp(-0.4, 0.4),
                VehicleKind::Bomber => -1.45,
                VehicleKind::Nrk => 0.0,
            },
            speed: 0.0,
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
            roll: 0.0,
            signal: 1.0,
            last_yaw: h.yaw,
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
        let link = self.eye();
        let st = self.tuning.streaks.clone();
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
        // left mouse button, or Space where Space does not mean "up"
        if can_act
            && (is_mouse_button_down(MouseButton::Left)
                || (p.kind != VehicleKind::Fpv && is_key_down(KeyCode::Space)))
        {
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
                let mut up = 0.0;
                if key(KeyCode::Space) {
                    up += 1.0;
                }
                if key(KeyCode::LeftControl) || key(KeyCode::C) {
                    up -= 1.0;
                }
                let boost = key(KeyCode::LeftShift);
                if boost {
                    keys |= keys::BOOST;
                }
                let target = fpv_target(&st, p.yaw, p.pitch, fwd, right, up, boost);
                let vel = fpv_accel(p.vel, target, dt);
                let (to, left, impact) = fpv_move(&self.world, p.pos, vel, dt);
                p.pos = to;
                p.vel = left;
                p.speed = left.length();
                // hitting something hard ends it: a blast once armed, a broken drone before
                if impact >= st.fpv_crash_speed {
                    keys |= keys::CRASH;
                }
                // bank into slides and turns like a real quad
                let (_, r) = toon_shared::yaw_basis(p.yaw);
                let turn = toon_shared::wrap_angle(p.yaw - p.last_yaw) / dt.max(1e-3);
                p.last_yaw = p.yaw;
                let want_roll = (p.vel.dot(r) * 0.03 - turn * 0.04).clamp(-0.45, 0.45);
                p.roll += (want_roll - p.roll) * (dt * 6.0).min(1.0);
                // the video link: the further and the more walls in between, the worse
                let d = p.pos - link;
                let dist = d.length();
                let mut want = 1.0 - (dist / 160.0).min(1.0) * 0.3;
                if dist > 0.5 {
                    let dir = d / dist;
                    if let Some((t1, _)) = self.world.raycast(link, dir, dist) {
                        // from the first wall the signal enters to the last one it leaves
                        let t2 = self
                            .world
                            .raycast(p.pos, -dir, dist)
                            .map_or(0.0, |(t, _)| t);
                        let solid = (dist - t1 - t2).max(0.0);
                        want -= 0.3 + (solid * 0.025).min(0.35);
                    }
                }
                p.signal += (want.clamp(0.1, 1.0) - p.signal) * (dt * 2.5).min(1.0);
            }
            VehicleKind::Bomber => {
                p.pos = bomber_step(
                    st.bomber_speed,
                    p.pos,
                    p.yaw,
                    fwd,
                    right,
                    dt,
                    self.map.min,
                    self.map.max,
                );
            }
            VehicleKind::Nrk => {
                let mut t = dt;
                while t > 0.0 {
                    let h = t.min(1.0 / 60.0);
                    nrk_step(
                        st.nrk_speed,
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
        if dt > 0.0 && p.kind != VehicleKind::Fpv {
            p.vel = (p.pos - before) / dt;
        }
        // the exit press and a crash must reach the server even between two sends
        p.keys = (p.keys & (keys::EXIT | keys::CRASH)) | keys;
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
        if p.keys & (keys::EXIT | keys::CRASH) != 0 {
            // say it twice, reliably, so leaving or crashing is never lost
            self.net.send(C2S::Drive {
                pos: p.pos.to_array(),
                yaw: p.yaw,
                pitch: p.pitch,
                keys: p.keys,
            });
            p.keys &= !(keys::EXIT | keys::CRASH);
        }
    }

    /// Camera while steering: (position, yaw, pitch, horizontal field of view in degrees).
    pub fn pilot_camera(&self) -> Option<(Vec3, f32, f32, Option<f32>)> {
        let p = self.piloting.as_ref()?;
        Some(match p.kind {
            VehicleKind::Fpv => {
                // a little propeller shake, more at speed
                let s = 0.002 + p.speed * 0.00015;
                let t = self.now as f32;
                let shake = vec2((t * 31.0).sin() + (t * 47.0).sin(), (t * 37.0).cos()) * s;
                (p.pos, p.yaw + shake.x, p.pitch + shake.y, Some(105.0))
            }
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
        let st = &self.tuning.streaks;
        let dur = p.kind.duration(st);
        let left = p.time_left / dur;
        match p.kind {
            VehicleKind::Fpv => self.draw_fpv_video(ui, p, left),
            VehicleKind::Bomber => {
                // the red ring on the ground is the sight; bombs left under the top band
                for k in 0..st.bomber_bombs {
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
        let t = tr!("{:.0} с", "{:.0} s", p.time_left.ceil());
        ui.shadowed(&t, bx + bw - 12.0 - ui.width(&t, 18, true), 66.0, 18, tc);
        let hp = (p.hp / p.kind.health(st)).clamp(0.0, 1.0);
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
            VehicleKind::Fpv => {
                toon_shared::lang::t("W/S — вперед/назад · A/D — вбік · Пробіл/Ctrl — вгору/вниз · Shift — газ · ЛКМ або E — підірвати",
                  "W/S — forward/back · A/D — sideways · Space/Ctrl — up/down · Shift — throttle · LMB or E — detonate")
            }
            VehicleKind::Bomber => {
                toon_shared::lang::t("WASD — летіти · миша — розворот · ЛКМ/Пробіл — скинути · E — вийти",
                  "WASD — fly · mouse — turn · LMB/Space — drop · E — leave")
            }
            VehicleKind::Nrk => toon_shared::lang::t("WASD — їхати · ЛКМ/Пробіл — кулемет · E — вийти", "WASD — drive · LMB/Space — machine gun · E — leave"),
        };
        ui.centered(help, cx, h - 24.0, 16, TEXT);
    }

    /// The FPV goggles: scanlines and the reticle, plus interference that grows as the
    /// link weakens (behind buildings, far away, flat battery). Kept light: static, a few
    /// torn lines, a grey wash and now and then a short dropout, never a blank screen.
    fn draw_fpv_video(&self, ui: &Ui, p: &Piloting, left: f32) {
        let (w, h) = (screen_width(), screen_height());
        let (cx, cy) = (w / 2.0, h / 2.0);
        let bad = (1.0 - p.signal).clamp(0.0, 1.0);
        let battery = (1.0 - left).clamp(0.0, 1.0);
        let frame = (self.now * 24.0) as u64;
        for k in 0..(h as i32 / 4) {
            draw_rectangle(0.0, k as f32 * 4.0, w, 1.0, Color::new(0.0, 0.0, 0.0, 0.10));
        }
        if bad > 0.05 {
            draw_rectangle(0.0, 0.0, w, h, Color::new(0.45, 0.45, 0.45, bad * 0.22));
        }
        // snow
        let n = (40.0 + 120.0 * battery + 650.0 * bad) as u64;
        for k in 0..n {
            let x = hash(frame * 7919 + k * 2) * w;
            let y = hash(frame * 7919 + k * 2 + 1) * h;
            let a = 0.15 + 0.3 * hash(k + frame);
            draw_rectangle(
                x,
                y,
                2.0 + (k % 3) as f32,
                1.0,
                Color::new(1.0, 1.0, 1.0, a),
            );
        }
        // torn lines, and a slow dark band rolling down the picture
        let tears = (bad * 7.0) as u64;
        for k in 0..tears {
            let y = hash(frame * 31 + k * 977) * h;
            let th = 2.0 + hash(frame + k * 13) * 10.0 * bad;
            let shift = (hash(frame * 3 + k) - 0.5) * 80.0 * bad;
            draw_rectangle(shift, y, w, th, Color::new(0.8, 0.8, 0.8, 0.08 + 0.2 * bad));
        }
        if bad > 0.2 {
            let roll_y = ((self.now * 0.35).fract() as f32) * (h + 120.0) - 120.0;
            draw_rectangle(0.0, roll_y, w, 90.0, Color::new(0.0, 0.0, 0.0, 0.12 * bad));
        }
        // a short dropout every now and then on a weak link
        let blink = (self.now * 6.0) as u64;
        if bad > 0.35 && hash(blink * 101) < (bad - 0.35) * 0.6 {
            draw_rectangle(0.0, 0.0, w, h, Color::new(0.5, 0.5, 0.5, 0.35));
        }

        draw_circle_lines(cx, cy, 18.0, 2.0, Color::new(1.0, 1.0, 1.0, 0.8));
        draw_line(cx - 30.0, cy, cx - 10.0, cy, 2.0, WHITE);
        draw_line(cx + 10.0, cy, cx + 30.0, cy, 2.0, WHITE);
        // horizon: shows the bank and the nose angle
        let (sr, cr) = (-p.roll).sin_cos();
        let hy = cy + p.pitch * h * 0.35;
        let c = Color::new(1.0, 1.0, 1.0, 0.35);
        for side in [-1.0f32, 1.0] {
            let (x0, x1) = (side * 60.0, side * 130.0);
            draw_line(
                cx + x0 * cr,
                hy + x0 * sr,
                cx + x1 * cr,
                hy + x1 * sr,
                2.0,
                c,
            );
        }
        ui.shadowed(
            &tr!("{:.0} км/год", "{:.0} km/h", p.speed * 3.6),
            cx + 40.0,
            cy + 6.0,
            18,
            TEXT,
        );
        if p.age < self.tuning.streaks.fpv_arm_time {
            ui.centered(
                toon_shared::lang::t("ЗАРЯД ЗВОДИТЬСЯ…", "ARMING…"),
                cx,
                cy + 50.0,
                16,
                TEXT,
            );
        }
        // link strength
        let bars = (p.signal * 5.0).round() as i32;
        let (bx, by) = (w - 120.0, 140.0);
        ui.shadowed(
            toon_shared::lang::t("СИГНАЛ", "SIGNAL"),
            bx - 6.0,
            by - 26.0,
            14,
            TEXT,
        );
        for k in 0..5 {
            let bh = 6.0 + k as f32 * 4.0;
            let on = k < bars;
            let col = if !on {
                Color::new(0.3, 0.3, 0.3, 0.6)
            } else if bars <= 2 {
                BAD
            } else {
                GOOD
            };
            draw_rectangle(bx + k as f32 * 10.0, by - bh, 7.0, bh, col);
        }
        if p.signal < 0.5 && (self.now * 2.5).fract() < 0.6 {
            ui.centered(
                toon_shared::lang::t("СЛАБКИЙ СИГНАЛ", "WEAK SIGNAL"),
                cx,
                cy - 60.0,
                20,
                BAD,
            );
        }
    }
}

/// A repeatable pseudo-random number in 0..1.
fn hash(n: u64) -> f32 {
    let mut x = n.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    x ^= x >> 29;
    x = x.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    x ^= x >> 32;
    (x >> 40) as f32 / (1u64 << 24) as f32
}
