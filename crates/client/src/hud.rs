//! 2D HUD and in-game menus.

use crate::game::{GameClient, Overlay};
use crate::models::team_color;
use crate::render::{NameTag, View};
use crate::settings::Settings;
use crate::ui::*;
use macroquad::prelude::*;
use toon_shared::consts::*;
use toon_shared::protocol::{flags, EntSnap, MatchState};
use toon_shared::weapons::{Killstreak, Perk1, Perk2, Perk3, WeaponId};
use toon_shared::{wrap_angle, yaw_of, Mode, Team};

pub enum HudAction {
    Leave,
}

fn fmt_time(t: f32) -> String {
    let t = t.max(0.0) as u32;
    format!("{}:{:02}", t / 60, t % 60)
}

fn with_alpha(c: Color, a: f32) -> Color {
    Color::new(c.r, c.g, c.b, a)
}

impl GameClient {
    pub fn draw_hud(
        &mut self,
        ui: &mut Ui,
        view: &View,
        tags: &[NameTag],
        settings: &mut Settings,
        fps: f32,
    ) -> Option<HudAction> {
        let (w, h) = (screen_width(), screen_height());
        let (cx, cy) = (w / 2.0, h / 2.0);
        let now = self.now;

        if self.scoped() && self.alive {
            self.draw_scope();
        }

        // low health / damage flash
        if self.alive {
            let k = (1.0 - self.me.hp / MAX_HEALTH).clamp(0.0, 1.0);
            let flash = (1.0 - (now - self.dmg_flash) as f32 / 0.35).clamp(0.0, 1.0) * 0.35;
            let a = (k * 0.6 + flash).min(0.85);
            if a > 0.01 {
                for i in 0..10 {
                    let t = i as f32 / 10.0;
                    let band = (w.min(h) * 0.18) * (1.0 - t);
                    let c = Color::new(0.85, 0.05, 0.05, a * 0.12);
                    draw_rectangle(0.0, 0.0, w, band, c);
                    draw_rectangle(0.0, h - band, w, band, c);
                    draw_rectangle(0.0, 0.0, band, h, c);
                    draw_rectangle(w - band, 0.0, band, h, c);
                }
            }
            // damage direction arcs
            for (from, t) in &self.dmg_dirs {
                let rel = wrap_angle(yaw_of(*from - view.pos) - self.camera().1);
                let alpha = (1.0 - (now - t) as f32 / 1.6).clamp(0.0, 1.0);
                let r = 150.0;
                let p = vec2(cx - rel.sin() * r, cy - rel.cos() * r);
                let dir = (p - vec2(cx, cy)).normalize_or_zero();
                let perp = vec2(-dir.y, dir.x);
                let c = Color::new(1.0, 0.15, 0.1, alpha * 0.85);
                draw_triangle(p + dir * 22.0, p + perp * 26.0, p - perp * 26.0, c);
            }
        }

        // name tags
        for t in tags {
            let Some(sp) = view.project(t.pos) else {
                continue;
            };
            if t.friendly {
                let d = t.pos.distance(view.pos);
                if d < 60.0 {
                    let label = if t.bomb {
                        format!("[Б] {}", t.name)
                    } else {
                        t.name.clone()
                    };
                    let tw = ui.width(&label, 15, true);
                    ui.shadowed(&label, sp.x - tw / 2.0, sp.y, 15, team_color(self.team));
                }
            } else if (sp - vec2(cx, cy)).length() < 50.0
                && self
                    .world
                    .line_of_sight(view.pos, t.pos - vec3(0.0, 0.4, 0.0))
            {
                let tw = ui.width(&t.name, 15, true);
                ui.shadowed(&t.name, sp.x - tw / 2.0, sp.y, 15, BAD);
            }
        }

        if self.mode == Mode::Sab {
            self.draw_sab_markers(ui, view);
        }

        // crosshair
        if self.alive && self.ads_t < 0.5 && !self.sprinting && self.overlay == Overlay::None {
            let gap = self.spread_px(view.fovy) + 4.0;
            let len = 10.0;
            let c = Color::new(1.0, 1.0, 1.0, 0.9);
            let sh = Color::new(0.0, 0.0, 0.0, 0.6);
            for (dx, dy) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                let a = vec2(cx + dx * gap, cy + dy * gap);
                let b = vec2(cx + dx * (gap + len), cy + dy * (gap + len));
                draw_line(a.x + 1.0, a.y + 1.0, b.x + 1.0, b.y + 1.0, 3.0, sh);
                draw_line(a.x, a.y, b.x, b.y, 2.0, c);
            }
        }
        // hitmarker
        let hk = (now - self.hitmarker) as f32;
        if hk < 0.25 {
            let c = if self.hit_kill { BAD } else { WHITE };
            let a = 1.0 - hk / 0.25;
            for (dx, dy) in [(1.0f32, 1.0f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
                draw_line(
                    cx + dx * 6.0,
                    cy + dy * 6.0,
                    cx + dx * 14.0,
                    cy + dy * 14.0,
                    3.0,
                    with_alpha(c, a),
                );
            }
        }

        self.draw_minimap(ui);
        self.draw_score_panel(ui);
        self.draw_killfeed(ui);
        self.draw_chat(ui);
        if self.alive {
            self.draw_weapon_panel(ui);
        }

        // score popups
        for (i, (text, t)) in self.popups.iter().enumerate() {
            let k = ((now - t) as f32 / 1.6).clamp(0.0, 1.0);
            ui.shadowed(
                text,
                cx + 40.0,
                cy - 30.0 - i as f32 * 24.0 - k * 30.0,
                20,
                with_alpha(ACCENT, 1.0 - k * k),
            );
        }

        // centre message
        if let Some((text, _, color)) = &self.center {
            ui.centered(text, cx, h * 0.3, 30, *color);
        }

        // state banners
        match self.state {
            MatchState::Warmup => {
                ui.centered("Розминка — очікування гравців", cx, 110.0, 22, MUTED)
            }
            MatchState::PreRound => {
                ui.centered(
                    &format!("Бій почнеться через {}", self.time_left.ceil() as i32),
                    cx,
                    h * 0.38,
                    34,
                    ACCENT,
                );
            }
            _ => {}
        }
        if self.overtime {
            ui.centered("ОВЕРТАЙМ — без відродження", cx, 110.0, 22, BAD);
        }

        // own spawn protection
        if self.is_protected()
            && self.overlay == Overlay::None
            && matches!(self.state, MatchState::Playing | MatchState::Warmup)
        {
            let a = (self.me.protected / 3.0).clamp(0.25, 1.0);
            let c = Color::new(0.45, 0.75, 1.0, 0.18 * a);
            for i in 0..6 {
                let band = 40.0 - i as f32 * 6.0;
                draw_rectangle(0.0, 0.0, w, band, c);
                draw_rectangle(0.0, h - band, w, band, c);
            }
            ui.centered(
                &format!("Захист після відродження: {:.1} с", self.me.protected),
                cx,
                h * 0.78,
                20,
                Color::new(0.6, 0.85, 1.0, 1.0),
            );
            ui.centered(
                "із захистом атакувати не можна · ЛКМ — зняти захист і битися",
                cx,
                h * 0.78 + 22.0,
                14,
                MUTED,
            );
        }

        // death screen
        if !self.alive
            && self.team.is_playing()
            && self.state != MatchState::PostMatch
            && self.overlay == Overlay::None
        {
            if let Some((killer, cause, _)) = &self.death_info {
                let text = if killer.is_empty() {
                    format!("Ти загинув ({})", cause.name())
                } else {
                    format!("Тебе вбив {killer} — {}", cause.name())
                };
                ui.centered(&text, cx, h * 0.62, 26, TEXT);
            }
            let text = if self.overtime && self.state == MatchState::Playing {
                "Чекай наступного раунду".to_string()
            } else if self.me.respawn_in > 0.0 {
                format!("Відродження через {:.0}", self.me.respawn_in.ceil())
            } else if self.has_class {
                "Клацни ЛКМ, щоб відродитися".to_string()
            } else {
                "Обери клас (Esc)".to_string()
            };
            ui.centered(&text, cx, h * 0.62 + 34.0, 20, ACCENT);
            ui.centered("Esc — змінити клас/команду", cx, h * 0.62 + 62.0, 16, MUTED);
        }
        if self.team == Team::Spectator && self.overlay == Overlay::None {
            ui.centered(
                "Режим глядача — WASD, Space/C, Shift. Esc — меню",
                cx,
                h - 40.0,
                18,
                MUTED,
            );
        }

        // fps / ping
        if settings.show_fps {
            let ping = self
                .scoreboard
                .iter()
                .find(|r| r.id == self.my_id)
                .map(|r| r.ping)
                .unwrap_or(0);
            ui.text(
                &format!("{fps:.0} FPS  {ping} мс"),
                w - 140.0,
                h - 8.0,
                14,
                MUTED,
            );
        }

        // match end / scoreboard
        let tab = is_key_down(KeyCode::Tab) && self.chat_input.is_none();
        if let Some((text, winner, _)) = self.match_end.clone() {
            let c = match winner {
                Some(t) if t == self.team => GOOD,
                Some(_) => BAD,
                None => TEXT,
            };
            let title = match winner {
                Some(t) if t == self.team => "ПЕРЕМОГА!",
                Some(_) if self.team.is_playing() => "ПОРАЗКА",
                _ => "МАТЧ ЗАВЕРШЕНО",
            };
            ui.centered(title, cx, 80.0, 48, c);
            ui.centered(&text, cx, 118.0, 22, TEXT);
            self.draw_scoreboard(ui, 150.0);
        } else if tab {
            self.draw_scoreboard(ui, 90.0);
        }

        // overlays
        self.draw_overlay(ui, settings)
    }

    fn draw_scope(&self) {
        let (w, h) = (screen_width(), screen_height());
        let r = h * 0.46;
        let (cx, cy) = (w / 2.0, h / 2.0);
        let black = Color::new(0.02, 0.02, 0.03, 1.0);
        draw_rectangle(0.0, 0.0, cx - r, h, black);
        draw_rectangle(cx + r, 0.0, w - cx - r, h, black);
        // ring made of thick circle lines
        for i in 0..14 {
            draw_circle_lines(cx, cy, r + i as f32 * 8.0, 9.0, black);
        }
        draw_rectangle(cx - r, 0.0, 2.0 * r, cy - r + 2.0, black);
        draw_rectangle(cx - r, cy + r - 2.0, 2.0 * r, h - cy - r + 2.0, black);
        draw_line(cx - r, cy, cx + r, cy, 2.0, black);
        draw_line(cx, cy - r, cx, cy + r, 2.0, black);
        draw_line(cx - r, cy, cx - 30.0, cy, 5.0, black);
        draw_line(cx + 30.0, cy, cx + r, cy, 5.0, black);
        draw_line(cx, cy + 30.0, cx, cy + r, 5.0, black);
        draw_circle(cx, cy, 2.0, BAD);
    }

    fn draw_sab_markers(&self, ui: &Ui, view: &View) {
        let Some(b) = &self.bomb else { return };
        if self.state == MatchState::Warmup {
            return;
        }
        let me = self.team;
        for t in [Team::Allies, Team::Axis] {
            let site = self.map.sites[t.index()];
            let Some(sp) = view.project(site + vec3(0.0, 2.2, 0.0)) else {
                continue;
            };
            let ours = t == me;
            let label = if ours {
                "ЗАХИСТИТИ"
            } else {
                "ПІДІРВАТИ"
            };
            let c = if ours { team_color(me) } else { BAD };
            let planted_here = b.planted_at == Some(t);
            let r = if planted_here && (self.now * 4.0).fract() < 0.5 {
                20.0
            } else {
                16.0
            };
            draw_circle(sp.x, sp.y, r + 3.0, Color::new(0.0, 0.0, 0.0, 0.6));
            draw_circle(sp.x, sp.y, r, c);
            let letter = if t == Team::Allies { "A" } else { "B" };
            let lw = ui.width(letter, 18, true);
            ui.bold(letter, sp.x - lw / 2.0, sp.y + 7.0, 18, WHITE);
            let d = site.distance(self.body.pos);
            let txt = format!("{label} {d:.0}м");
            let tw = ui.width(&txt, 13, true);
            ui.shadowed(&txt, sp.x - tw / 2.0, sp.y + r + 16.0, 13, c);
        }
        // the bomb itself when lying around
        if b.carrier.is_none() && b.planted_at.is_none() {
            if let Some(sp) = view.project(Vec3::from_array(b.pos) + vec3(0.0, 1.2, 0.0)) {
                draw_circle(sp.x, sp.y - 8.0, 11.0, Color::new(0.0, 0.0, 0.0, 0.7));
                draw_circle(sp.x, sp.y - 8.0, 8.0, BAD);
                let d = Vec3::from_array(b.pos).distance(self.body.pos);
                ui.centered(&format!("бомба {d:.0}м"), sp.x, sp.y + 18.0, 13, ACCENT);
            }
        }
        // planted timer
        if b.planted_at.is_some() {
            ui.centered(
                &format!("БОМБА {:.1}", b.timer),
                screen_width() / 2.0,
                70.0,
                34,
                BAD,
            );
        }
        // plant / defuse hints & progress
        let (w, h) = (screen_width(), screen_height());
        if self.me.progress > 0.0 {
            let bw = 300.0;
            draw_rectangle(
                w / 2.0 - bw / 2.0 - 3.0,
                h * 0.7 - 3.0,
                bw + 6.0,
                22.0,
                Color::new(0.0, 0.0, 0.0, 0.7),
            );
            draw_rectangle(
                w / 2.0 - bw / 2.0,
                h * 0.7,
                bw * self.me.progress,
                16.0,
                ACCENT,
            );
            let label = if b.planted_at.is_some() {
                "Знешкодження…"
            } else {
                "Закладання…"
            };
            ui.centered(label, w / 2.0, h * 0.7 - 10.0, 18, TEXT);
        } else if self.alive {
            let carrying = b.carrier == Some(self.my_id);
            let enemy_site = self.map.sites[me.enemy().index()];
            if carrying && self.body.pos.distance(enemy_site) < SAB_SITE_RADIUS {
                ui.centered("Тримай F, щоб закласти бомбу", w / 2.0, h * 0.7, 22, ACCENT);
            } else if b.planted_at == Some(me)
                && self.body.pos.distance(Vec3::from_array(b.pos)) < SAB_DEFUSE_RADIUS
            {
                ui.centered(
                    "Тримай F, щоб знешкодити бомбу",
                    w / 2.0,
                    h * 0.7,
                    22,
                    ACCENT,
                );
            } else if carrying {
                ui.centered(
                    "У тебе бомба — неси її на ворожу точку!",
                    w / 2.0,
                    h - 150.0,
                    18,
                    ACCENT,
                );
            }
        }
    }

    fn draw_minimap(&self, ui: &Ui) {
        let size = (screen_height() * 0.26).clamp(150.0, 240.0);
        let (x0, y0) = (16.0, 16.0);
        let scale = size / 55.0; // pixels per meter
        let c = vec2(x0 + size / 2.0, y0 + size / 2.0);
        draw_rectangle(
            x0 - 4.0,
            y0 - 4.0,
            size + 8.0,
            size + 8.0,
            Color::new(0.0, 0.0, 0.0, 0.7),
        );
        draw_rectangle(x0, y0, size, size, Color::new(0.16, 0.22, 0.18, 0.85));
        let (pos, yaw) = if self.alive || !self.team.is_playing() {
            let (p, y, _) = self.camera();
            (p, y)
        } else {
            (self.death_pos, self.yaw)
        };
        let to_map = |p: Vec3| -> Vec2 {
            let rx = p.x - pos.x;
            let rz = p.z - pos.z;
            let sx = rx * yaw.cos() - rz * yaw.sin();
            let sy = rx * yaw.sin() + rz * yaw.cos();
            c + vec2(sx, sy) * scale
        };
        let gl = unsafe { get_internal_gl() }.quad_gl;
        let dpi = screen_dpi_scale();
        gl.scissor(Some((
            (x0 * dpi) as i32,
            (y0 * dpi) as i32,
            (size * dpi) as i32,
            (size * dpi) as i32,
        )));
        for b in &self.map.boxes {
            if !b.visible || b.min.y < -0.5 || b.max.y < 0.3 && !b.solid {
                if b.visible && !b.solid && b.max.y < 0.3 {
                    // floor paint (roads, pads)
                } else {
                    continue;
                }
            }
            let ht = (b.max.y / 7.0).clamp(0.0, 1.0);
            let col = if b.solid {
                Color::new(0.55 + ht * 0.35, 0.58 + ht * 0.32, 0.6 + ht * 0.3, 0.95)
            } else {
                Color::from_rgba(b.color[0], b.color[1], b.color[2], 90)
            };
            let p = [
                to_map(vec3(b.min.x, 0.0, b.min.z)),
                to_map(vec3(b.max.x, 0.0, b.min.z)),
                to_map(vec3(b.max.x, 0.0, b.max.z)),
                to_map(vec3(b.min.x, 0.0, b.max.z)),
            ];
            draw_triangle(p[0], p[1], p[2], col);
            draw_triangle(p[0], p[2], p[3], col);
        }
        // sites & bomb
        if self.mode == Mode::Sab {
            for t in [Team::Allies, Team::Axis] {
                let p = to_map(self.map.sites[t.index()]);
                draw_circle(
                    p.x,
                    p.y,
                    8.0,
                    if t == self.team { team_color(t) } else { BAD },
                );
                let l = if t == Team::Allies { "A" } else { "B" };
                ui.bold(l, p.x - 5.0, p.y + 5.0, 13, WHITE);
            }
            if let Some(b) = &self.bomb {
                let visible = b.carrier.is_none()
                    || b.carrier
                        .map(|c| {
                            self.remotes
                                .get(&c)
                                .map(|r| r.team == self.team)
                                .unwrap_or(c == self.my_id)
                        })
                        .unwrap_or(false);
                if visible {
                    let p = to_map(Vec3::from_array(b.pos));
                    draw_circle(p.x, p.y, 6.0, Color::new(0.1, 0.1, 0.1, 1.0));
                    draw_circle(p.x, p.y, 4.0, ACCENT);
                }
            }
        }
        // players
        for r in self.remotes.values() {
            if !r.alive {
                continue;
            }
            let p = to_map(r.pos);
            if r.team == self.team {
                let a = r.yaw - yaw;
                let d = vec2(-a.sin(), -a.cos());
                let perp = vec2(-d.y, d.x);
                draw_triangle(
                    p + d * 7.0,
                    p - d * 4.0 + perp * 5.0,
                    p - d * 4.0 - perp * 5.0,
                    team_color(self.team),
                );
            } else if r.flags & flags::ON_RADAR != 0 {
                let pulse = 5.0 + ((self.now * 6.0).sin() as f32) * 1.0;
                draw_circle(p.x, p.y, pulse + 1.5, BLACK);
                draw_circle(p.x, p.y, pulse, BAD);
            }
        }
        for e in &self.ents {
            if let EntSnap::Heli { pos, team, .. } = e {
                let p = to_map(Vec3::from_array(*pos));
                draw_rectangle(
                    p.x - 6.0,
                    p.y - 6.0,
                    12.0,
                    12.0,
                    if *team == self.team {
                        team_color(*team)
                    } else {
                        BAD
                    },
                );
            }
        }
        gl.scissor(None);
        // me
        draw_triangle(
            c + vec2(0.0, -9.0),
            c + vec2(6.0, 6.0),
            c + vec2(-6.0, 6.0),
            ACCENT,
        );
        // compass letters around the edge
        for (label, a) in [
            ("Пн", 0.0f32),
            ("Сх", -std::f32::consts::FRAC_PI_2),
            ("Пд", std::f32::consts::PI),
            ("Зх", std::f32::consts::FRAC_PI_2),
        ] {
            let rel = a - yaw;
            let d = vec2(-rel.sin(), -rel.cos());
            let p = c + d * (size / 2.0 - 10.0);
            let tw = ui.width(label, 12, true);
            ui.shadowed(label, p.x - tw / 2.0, p.y + 5.0, 12, TEXT);
        }
        if self.me.uav {
            ui.shadowed("БПЛА", x0 + 6.0, y0 + size - 8.0, 14, GOOD);
        }
    }

    fn draw_score_panel(&self, ui: &Ui) {
        let size = (screen_height() * 0.26).clamp(150.0, 240.0);
        let y = 16.0 + size + 12.0;
        let x = 16.0;
        let my = self.team;
        let (mine, theirs) = if my == Team::Axis {
            (self.scores[1], self.scores[0])
        } else {
            (self.scores[0], self.scores[1])
        };
        let my_c = if my.is_playing() {
            team_color(my)
        } else {
            team_color(Team::Allies)
        };
        let en_c = if my.is_playing() {
            team_color(my.enemy())
        } else {
            team_color(Team::Axis)
        };
        let limit = match self.mode {
            Mode::Tdm => self.gs.score_limit,
            Mode::Sab => self.gs.rounds_to_win as i32,
        };
        let bw = size;
        draw_rectangle(
            x - 4.0,
            y - 4.0,
            bw + 8.0,
            64.0,
            Color::new(0.0, 0.0, 0.0, 0.55),
        );
        for (i, (val, col)) in [(mine, my_c), (theirs, en_c)].iter().enumerate() {
            let yy = y + i as f32 * 22.0;
            let k = (*val as f32 / limit.max(1) as f32).clamp(0.0, 1.0);
            draw_rectangle(x + 40.0, yy + 4.0, (bw - 44.0) * k, 14.0, *col);
            draw_rectangle_lines(
                x + 40.0,
                yy + 4.0,
                bw - 44.0,
                14.0,
                2.0,
                Color::new(0.0, 0.0, 0.0, 0.6),
            );
            ui.shadowed(&val.to_string(), x + 2.0, yy + 18.0, 18, *col);
        }
        let t = match self.state {
            MatchState::Warmup => "Розминка".to_string(),
            _ if self.overtime => "Овертайм".to_string(),
            _ => fmt_time(self.time_left),
        };
        ui.shadowed(
            &format!("{}  ·  {}", t, self.mode.name()),
            x + 2.0,
            y + 58.0,
            15,
            TEXT,
        );
    }

    fn draw_killfeed(&self, ui: &Ui) {
        let w = screen_width();
        let mut y = 30.0;
        for k in self.killfeed.iter().rev() {
            let age = (self.now - k.time) as f32;
            if age > 7.0 {
                continue;
            }
            let a = (1.0 - (age - 6.0).max(0.0)).clamp(0.0, 1.0);
            let mid = format!(
                "  [{}{}]  ",
                k.cause.name(),
                if k.headshot { " ✦" } else { "" }
            );
            let parts: Vec<(String, Color)> = if k.killer.is_empty() || k.killer == k.victim {
                vec![
                    (mid.trim_start().to_string(), MUTED),
                    (k.victim.clone(), team_color(k.vteam)),
                ]
            } else {
                vec![
                    (k.killer.clone(), team_color(k.kteam)),
                    (mid, MUTED),
                    (k.victim.clone(), team_color(k.vteam)),
                ]
            };
            let total: f32 = parts.iter().map(|p| ui.width(&p.0, 16, true)).sum();
            let mut x = w - 20.0 - total;
            draw_rectangle(
                x - 6.0,
                y - 17.0,
                total + 12.0,
                23.0,
                Color::new(0.0, 0.0, 0.0, 0.35 * a),
            );
            for (s, c) in parts {
                ui.shadowed(&s, x, y, 16, with_alpha(c, a));
                x += ui.width(&s, 16, true);
            }
            y += 26.0;
        }
    }

    fn draw_chat(&mut self, ui: &mut Ui) {
        let h = screen_height();
        let open = self.chat_input.is_some();
        let mut y = h - 190.0;
        let lines: Vec<&crate::game::Line> = self
            .chat
            .iter()
            .rev()
            .filter(|l| open || self.now - l.time < 10.0)
            .take(if open { 14 } else { 6 })
            .collect();
        for l in lines {
            ui.shadowed(&l.text, 16.0, y, 16, l.color);
            y -= 21.0;
        }
        if let Some((text, team)) = &self.chat_input {
            let label = if *team {
                "Команді: "
            } else {
                "Всім: "
            };
            draw_rectangle(10.0, h - 176.0, 560.0, 30.0, Color::new(0.0, 0.0, 0.0, 0.6));
            let cursor = if (self.now * 2.0).fract() < 0.5 {
                "_"
            } else {
                ""
            };
            ui.shadowed(&format!("{label}{text}{cursor}"), 16.0, h - 155.0, 18, TEXT);
        }
    }

    fn draw_weapon_panel(&self, ui: &Ui) {
        let (w, h) = (screen_width(), screen_height());
        let (wid, mag, res) = self.weapons[self.slot as usize & 1];
        let def = wid.def();
        let x = w - 300.0;
        let y = h - 120.0;
        draw_rectangle(
            x - 10.0,
            y - 10.0,
            300.0,
            110.0,
            Color::new(0.0, 0.0, 0.0, 0.35),
        );
        let mag_max = self.tuning.weapon(wid).mag;
        let mag_c = if mag == 0 {
            BAD
        } else if (mag as f32) < mag_max as f32 * 0.3 {
            ACCENT
        } else {
            TEXT
        };
        ui.shadowed(&format!("{mag}"), x, y + 42.0, 46, mag_c);
        let mw = ui.width(&format!("{mag}"), 46, true);
        ui.shadowed(&format!("/ {res}"), x + mw + 8.0, y + 42.0, 24, MUTED);
        ui.shadowed(def.name, x, y + 70.0, 18, TEXT);
        // grenades
        for i in 0..self.grenades {
            draw_circle(
                x + 200.0 + i as f32 * 22.0,
                y + 30.0,
                8.0,
                Color::from_rgba(90, 140, 70, 255),
            );
            draw_circle_lines(x + 200.0 + i as f32 * 22.0, y + 30.0, 8.0, 2.0, BLACK);
        }
        if let Some(start) = self.cooking {
            let left = (self.tuning.grenade.fuse as f64 - (self.now - start)).max(0.0);
            ui.centered(&format!("Граната: {left:.1}"), w / 2.0, h * 0.62, 22, BAD);
        }
        // killstreak
        let next = match self.me.streak as u32 {
            s if s < UAV_KILLS => Some((Killstreak::Uav, UAV_KILLS)),
            s if s < AIRSTRIKE_KILLS => Some((Killstreak::Airstrike, AIRSTRIKE_KILLS)),
            s if s < HELI_KILLS => Some((Killstreak::Helicopter, HELI_KILLS)),
            _ => None,
        };
        let streak_txt = match next {
            Some((k, n)) => format!(
                "Серія: {} (до «{}» {})",
                self.me.streak,
                k.name(),
                n - self.me.streak as u32
            ),
            None => format!("Серія: {}", self.me.streak),
        };
        ui.shadowed(&streak_txt, x, y + 92.0, 13, MUTED);
        if let Some(r) = self.me.rewards.last() {
            let t = format!("[5] {}", r.name());
            let tw = ui.width(&t, 18, true);
            draw_rectangle(
                w - tw - 30.0,
                y - 48.0,
                tw + 16.0,
                30.0,
                Color::new(0.0, 0.0, 0.0, 0.5),
            );
            ui.shadowed(&t, w - tw - 22.0, y - 26.0, 18, ACCENT);
        }
        // stance + sprint
        let stance = match self.body.stance {
            toon_shared::physics::Stance::Stand => "▮",
            toon_shared::physics::Stance::Crouch => "▄",
            toon_shared::physics::Stance::Prone => "▁",
        };
        ui.shadowed(stance, x - 40.0, y + 60.0, 30, TEXT);
        if self.reload_until.is_some() {
            ui.centered("Перезарядка…", w / 2.0, h * 0.58, 18, TEXT);
        } else if mag == 0 && res == 0 {
            ui.centered(
                "Немає набоїв — 1/2 змінити зброю",
                w / 2.0,
                h * 0.58,
                18,
                BAD,
            );
        }
    }

    fn draw_scoreboard(&self, ui: &Ui, top: f32) {
        let w = screen_width();
        let pw = (w - 80.0).min(980.0);
        let x0 = (w - pw) / 2.0;
        let rows_per = self.scoreboard.len().max(4) as f32;
        ui.panel(x0, top, pw, 90.0 + rows_per * 24.0);
        ui.bold(&self.server_name, x0 + 16.0, top + 30.0, 20, ACCENT);
        ui.text(
            &format!("{} · {}", self.map.name, self.mode.name()),
            x0 + 16.0,
            top + 52.0,
            15,
            MUTED,
        );
        let col_w = pw / 2.0 - 20.0;
        for (i, team) in [Team::Allies, Team::Axis].iter().enumerate() {
            let x = x0 + 12.0 + i as f32 * (col_w + 16.0);
            let y = top + 80.0;
            let c = team_color(*team);
            ui.bold(
                &format!("{}  {}", team.name(), self.scores[team.index()]),
                x,
                y,
                18,
                c,
            );
            ui.text("Очки   В   С   Д   Пінг", x + col_w - 210.0, y, 14, MUTED);
            let mut rows: Vec<_> = self.scoreboard.iter().filter(|r| r.team == *team).collect();
            rows.sort_by(|a, b| b.score.cmp(&a.score));
            for (k, r) in rows.iter().enumerate() {
                let yy = y + 24.0 + k as f32 * 24.0;
                if r.id == self.my_id {
                    draw_rectangle(
                        x - 4.0,
                        yy - 17.0,
                        col_w,
                        22.0,
                        Color::new(1.0, 1.0, 1.0, 0.08),
                    );
                }
                let name = if r.alive {
                    r.name.clone()
                } else {
                    format!("☠ {}", r.name)
                };
                ui.text(&name, x, yy, 16, if r.alive { TEXT } else { MUTED });
                let ping = if r.bot {
                    "БОТ".to_string()
                } else {
                    r.ping.to_string()
                };
                ui.text(
                    &format!(
                        "{:>4} {:>3} {:>3} {:>3} {:>5}",
                        r.score, r.kills, r.deaths, r.assists, ping
                    ),
                    x + col_w - 210.0,
                    yy,
                    15,
                    TEXT,
                );
            }
        }
        let specs: Vec<String> = self
            .scoreboard
            .iter()
            .filter(|r| r.team == Team::Spectator)
            .map(|r| r.name.clone())
            .collect();
        if !specs.is_empty() {
            ui.text(
                &format!("Глядачі: {}", specs.join(", ")),
                x0 + 16.0,
                top + 80.0 + rows_per * 24.0,
                14,
                MUTED,
            );
        }
    }

    fn draw_overlay(&mut self, ui: &mut Ui, settings: &mut Settings) -> Option<HudAction> {
        let (w, h) = (screen_width(), screen_height());
        match self.overlay {
            Overlay::None => None,
            Overlay::Team => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
                let pw = 520.0;
                let (x, y) = ((w - pw) / 2.0, h * 0.2);
                ui.panel(x, y, pw, 360.0);
                ui.bold("Обери команду", x + 24.0, y + 44.0, 28, ACCENT);
                ui.text(
                    &format!(
                        "{} · {} · {}",
                        self.server_name,
                        self.map.name,
                        self.mode.name()
                    ),
                    x + 24.0,
                    y + 72.0,
                    15,
                    MUTED,
                );
                let count = |t: Team| self.scoreboard.iter().filter(|r| r.team == t).count();
                let (a, b) = (count(Team::Allies), count(Team::Axis));
                if ui.button_colored(
                    &format!("{} ({a})", Team::Allies.name()),
                    x + 24.0,
                    y + 100.0,
                    pw - 48.0,
                    50.0,
                    team_color(Team::Allies),
                ) {
                    self.join_team(Some(Team::Allies));
                }
                if ui.button_colored(
                    &format!("{} ({b})", Team::Axis.name()),
                    x + 24.0,
                    y + 160.0,
                    pw - 48.0,
                    50.0,
                    team_color(Team::Axis),
                ) {
                    self.join_team(Some(Team::Axis));
                }
                if ui.button("Автовибір", x + 24.0, y + 220.0, (pw - 60.0) / 2.0, 46.0) {
                    self.join_team(None);
                }
                if ui.button2(
                    "Глядач",
                    x + 36.0 + (pw - 60.0) / 2.0,
                    y + 220.0,
                    (pw - 60.0) / 2.0,
                    46.0,
                ) {
                    self.join_team(Some(Team::Spectator));
                }
                if ui.button2("Відключитися", x + 24.0, y + 290.0, pw - 48.0, 44.0) {
                    return Some(HudAction::Leave);
                }
                None
            }
            Overlay::Class => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
                let classes = settings.classes();
                let pw = 860.0f32.min(w - 40.0);
                let ph = 520.0;
                let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
                ui.panel(x, y, pw, ph);
                ui.bold("Обери клас", x + 24.0, y + 44.0, 28, ACCENT);
                let list_w = 300.0;
                let mut hover_class = self.class_choice.min(classes.len() - 1);
                for (i, (name, _)) in classes.iter().enumerate() {
                    let by = y + 70.0 + i as f32 * 50.0;
                    let label = if i >= 5 {
                        format!("★ {name}")
                    } else {
                        name.clone()
                    };
                    let selected = i == self.class_choice;
                    let color = if selected { ACCENT } else { PANEL2 };
                    if ui.button_colored(&label, x + 24.0, by, list_w, 42.0, color) {
                        if self.class_choice == i {
                            settings.last_class = i;
                            self.choose_class(classes[i].1);
                            settings.save();
                            return None;
                        }
                        self.class_choice = i;
                    }
                    if ui.mouse.x > x + 24.0
                        && ui.mouse.x < x + 24.0 + list_w
                        && ui.mouse.y > by
                        && ui.mouse.y < by + 42.0
                    {
                        hover_class = i;
                    }
                }
                // details of the hovered/selected class
                let (name, l) = &classes[hover_class];
                let dx = x + 50.0 + list_w;
                ui.bold(name, dx, y + 96.0, 24, TEXT);
                let lines = [
                    (
                        format!("Основна: {}", l.primary.def().name),
                        l.primary.def().class.name().to_string(),
                    ),
                    (
                        format!("Додаткова: {}", l.secondary.def().name),
                        l.secondary.def().class.name().to_string(),
                    ),
                    (
                        format!("Перк 1: {}", l.perk1.name()),
                        l.perk1.desc().to_string(),
                    ),
                    (
                        format!("Перк 2: {}", l.perk2.name()),
                        l.perk2.desc().to_string(),
                    ),
                    (
                        format!("Перк 3: {}", l.perk3.name()),
                        l.perk3.desc().to_string(),
                    ),
                ];
                for (k, (a, b)) in lines.iter().enumerate() {
                    ui.bold(a, dx, y + 140.0 + k as f32 * 50.0, 18, TEXT);
                    ui.text(b, dx, y + 160.0 + k as f32 * 50.0, 15, MUTED);
                }
                let d = self.tuning.weapon(l.primary).clone();
                let stats = [
                    ("Урон", d.damage_max / 100.0),
                    ("Скорострільність", d.rpm / 1000.0),
                    ("Дальність", d.range_far / 100.0),
                    (
                        "Контроль віддачі",
                        1.0 - (d.recoil_v * d.recoil_max_ramp / 6.0),
                    ),
                    ("Мобільність", (d.move_mult - 0.6) / 0.45),
                ];
                ui.text(
                    &format!(
                        "Вага {:.1} кг · приціл {:.2} с · магазин {}",
                        d.weight, d.ads_time, d.mag
                    ),
                    dx,
                    y + 388.0,
                    14,
                    TEXT,
                );
                for (k, (label, v)) in stats.iter().enumerate() {
                    let sy = y + 408.0 + k as f32 * 18.0;
                    ui.text(label, dx, sy, 14, MUTED);
                    draw_rectangle(
                        dx + 140.0,
                        sy - 10.0,
                        200.0,
                        10.0,
                        Color::new(0.0, 0.0, 0.0, 0.5),
                    );
                    draw_rectangle(
                        dx + 140.0,
                        sy - 10.0,
                        200.0 * v.clamp(0.0, 1.0),
                        10.0,
                        ACCENT,
                    );
                }
                if ui.button("В БІЙ", x + pw - 200.0, y + ph - 64.0, 176.0, 46.0) {
                    let i = self.class_choice.min(classes.len() - 1);
                    settings.last_class = i;
                    settings.save();
                    self.choose_class(classes[i].1);
                }
                if self.class_choice >= 5
                    && ui.button2("Редагувати", x + pw - 390.0, y + ph - 64.0, 176.0, 46.0)
                {
                    self.overlay = Overlay::Edit(self.class_choice - 5);
                }
                if ui.button2("Команда", x + 24.0, y + ph - 64.0, 140.0, 46.0) {
                    self.overlay = Overlay::Team;
                }
                ui.text(
                    "Клацни клас двічі або «В БІЙ»",
                    x + 180.0,
                    y + ph - 34.0,
                    14,
                    MUTED,
                );
                None
            }
            Overlay::Edit(i) => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
                let pw = 620.0;
                let ph = 520.0;
                let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
                ui.panel(x, y, pw, ph);
                ui.bold("Редактор класу", x + 24.0, y + 44.0, 26, ACCENT);
                let c = &mut settings.custom[i];
                ui.field(
                    100 + i as u32,
                    "Назва",
                    &mut c.name,
                    x + 24.0,
                    y + 90.0,
                    pw - 48.0,
                    false,
                    24,
                );
                let fw = pw - 48.0;
                let prim: Vec<WeaponId> = WeaponId::primaries().collect();
                let sec: Vec<WeaponId> = WeaponId::secondaries().collect();
                let cyc = |list: &[WeaponId], cur: WeaponId, d: i32| -> WeaponId {
                    let i = list.iter().position(|w| *w == cur).unwrap_or(0) as i32;
                    list[((i + d).rem_euclid(list.len() as i32)) as usize]
                };
                let d = ui.stepper(
                    "Основна зброя",
                    &format!(
                        "{} ({})",
                        c.loadout.primary.def().name,
                        c.loadout.primary.def().class.name()
                    ),
                    x + 24.0,
                    y + 160.0,
                    fw,
                );
                if d != 0 {
                    c.loadout.primary = cyc(&prim, c.loadout.primary, d);
                }
                let d = ui.stepper(
                    "Додаткова зброя",
                    c.loadout.secondary.def().name,
                    x + 24.0,
                    y + 220.0,
                    fw,
                );
                if d != 0 {
                    c.loadout.secondary = cyc(&sec, c.loadout.secondary, d);
                }
                let d = ui.stepper(
                    "Перк 1",
                    &format!("{} — {}", c.loadout.perk1.name(), c.loadout.perk1.desc()),
                    x + 24.0,
                    y + 280.0,
                    fw,
                );
                if d != 0 {
                    let i = Perk1::ALL
                        .iter()
                        .position(|p| *p == c.loadout.perk1)
                        .unwrap_or(0) as i32;
                    c.loadout.perk1 =
                        Perk1::ALL[(i + d).rem_euclid(Perk1::ALL.len() as i32) as usize];
                }
                let d = ui.stepper(
                    "Перк 2",
                    &format!("{} — {}", c.loadout.perk2.name(), c.loadout.perk2.desc()),
                    x + 24.0,
                    y + 340.0,
                    fw,
                );
                if d != 0 {
                    let i = Perk2::ALL
                        .iter()
                        .position(|p| *p == c.loadout.perk2)
                        .unwrap_or(0) as i32;
                    c.loadout.perk2 =
                        Perk2::ALL[(i + d).rem_euclid(Perk2::ALL.len() as i32) as usize];
                }
                let d = ui.stepper(
                    "Перк 3",
                    &format!("{} — {}", c.loadout.perk3.name(), c.loadout.perk3.desc()),
                    x + 24.0,
                    y + 400.0,
                    fw,
                );
                if d != 0 {
                    let i = Perk3::ALL
                        .iter()
                        .position(|p| *p == c.loadout.perk3)
                        .unwrap_or(0) as i32;
                    c.loadout.perk3 =
                        Perk3::ALL[(i + d).rem_euclid(Perk3::ALL.len() as i32) as usize];
                }
                if ui.button("Зберегти", x + pw - 200.0, y + ph - 60.0, 176.0, 44.0) {
                    settings.save();
                    self.overlay = Overlay::Class;
                }
                None
            }
            Overlay::Pause => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
                let pw = 420.0;
                let (x, y) = ((w - pw) / 2.0, h * 0.22);
                ui.panel(x, y, pw, 400.0);
                ui.bold("Пауза", x + 24.0, y + 44.0, 28, ACCENT);
                ui.text("(гра на сервері триває)", x + 24.0, y + 70.0, 14, MUTED);
                let bw = pw - 48.0;
                if ui.button("Продовжити", x + 24.0, y + 90.0, bw, 46.0) {
                    self.overlay = Overlay::None;
                }
                if ui.button2("Змінити клас", x + 24.0, y + 146.0, bw, 46.0) {
                    self.overlay = Overlay::Class;
                }
                if ui.button2("Змінити команду", x + 24.0, y + 202.0, bw, 46.0) {
                    self.overlay = Overlay::Team;
                }
                if ui.button2("Налаштування", x + 24.0, y + 258.0, bw, 46.0) {
                    self.overlay = Overlay::Settings;
                }
                if ui.button_colored("Відключитися", x + 24.0, y + 330.0, bw, 46.0, BAD)
                {
                    return Some(HudAction::Leave);
                }
                None
            }
            Overlay::Settings => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
                let pw = 480.0;
                let (x, y) = ((w - pw) / 2.0, h * 0.18);
                ui.panel(x, y, pw, 570.0);
                ui.bold("Налаштування", x + 24.0, y + 44.0, 26, ACCENT);
                crate::menu::settings_widgets(ui, settings, x + 24.0, y + 90.0, pw - 48.0);
                if ui.button("Готово", x + pw - 180.0, y + 510.0, 156.0, 44.0) {
                    settings.save();
                    self.overlay = Overlay::Pause;
                }
                None
            }
            Overlay::Airstrike => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.6));
                let size = (h - 160.0).min(w - 80.0);
                let (x0, y0) = ((w - size) / 2.0, 90.0);
                ui.centered(
                    "Авіаудар: клацни ціль на карті (Esc — скасувати)",
                    w / 2.0,
                    60.0,
                    24,
                    ACCENT,
                );
                let msz = self.map.size();
                let scale = size / msz.x.max(msz.y);
                let ox = x0 + (size - msz.x * scale) / 2.0;
                let oy = y0 + (size - msz.y * scale) / 2.0;
                let to = |x: f32, z: f32| {
                    vec2(
                        ox + (x - self.map.min.x) * scale,
                        oy + (z - self.map.min.y) * scale,
                    )
                };
                draw_rectangle(
                    ox,
                    oy,
                    msz.x * scale,
                    msz.y * scale,
                    Color::new(0.16, 0.22, 0.18, 1.0),
                );
                for b in &self.map.boxes {
                    if !b.visible || b.min.y < -0.5 {
                        continue;
                    }
                    let a = to(b.min.x, b.min.z);
                    let c = to(b.max.x, b.max.z);
                    let col = if b.solid {
                        Color::new(0.75, 0.77, 0.8, 1.0)
                    } else {
                        Color::from_rgba(b.color[0], b.color[1], b.color[2], 110)
                    };
                    draw_rectangle(a.x, a.y, c.x - a.x, c.y - a.y, col);
                }
                for r in self.remotes.values() {
                    if r.alive && (r.team == self.team || r.flags & flags::ON_RADAR != 0) {
                        let p = to(r.pos.x, r.pos.z);
                        draw_circle(
                            p.x,
                            p.y,
                            6.0,
                            if r.team == self.team {
                                team_color(self.team)
                            } else {
                                BAD
                            },
                        );
                    }
                }
                let me = to(self.body.pos.x, self.body.pos.z);
                draw_circle(me.x, me.y, 7.0, ACCENT);
                let m = ui.mouse;
                let inside =
                    m.x > ox && m.y > oy && m.x < ox + msz.x * scale && m.y < oy + msz.y * scale;
                if inside {
                    draw_circle_lines(m.x, m.y, 7.0 * scale, 3.0, BAD);
                    draw_line(m.x - 12.0, m.y, m.x + 12.0, m.y, 2.0, BAD);
                    draw_line(m.x, m.y - 12.0, m.x, m.y + 12.0, 2.0, BAD);
                    if ui.clicked {
                        let wx = self.map.min.x + (m.x - ox) / scale;
                        let wz = self.map.min.y + (m.y - oy) / scale;
                        self.call_airstrike(wx, wz);
                    }
                }
                None
            }
        }
    }
}
