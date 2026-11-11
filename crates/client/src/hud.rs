//! 2D HUD and in-game menus.

use crate::binds::{key, Action};
use crate::game::{GameClient, Overlay, Pick};
use crate::models::team_color;
use crate::render::{NameTag, View};
use crate::settings::Settings;
use crate::ui::*;
use macroquad::prelude::*;
use toon_shared::consts::*;
use toon_shared::lang::t;
use toon_shared::protocol::{flags, EntSnap, LootKind, MatchState};
use toon_shared::tr;
use toon_shared::weapons::{Cause, Killstreak, Perk1, Perk2, Perk3, WeaponId};
use toon_shared::{look_dir, wrap_angle, yaw_of, Mode, Team};

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

        if self.killcam.is_some() {
            self.draw_killcam(ui);
            self.draw_killfeed(ui);
            return self.draw_overlay(ui, settings);
        }

        if self.scoped() && self.alive {
            self.draw_scope();
        }
        // blinded by a flashbang: white, fading
        if now < self.flashed.0 {
            let left = ((self.flashed.0 - now) as f32 / 2.0).min(1.0);
            draw_rectangle(
                0.0,
                0.0,
                w,
                h,
                Color::new(1.0, 1.0, 0.97, left * (0.5 + 0.5 * self.flashed.1)),
            );
        }
        // a red dot sight: the dot floats in the middle while aiming
        if self.alive && self.sight() == 1 && self.ads_t > 0.8 {
            draw_circle(cx, cy, 2.6, Color::new(1.0, 0.15, 0.1, 0.95));
            draw_circle_lines(cx, cy, 5.0, 1.0, Color::new(1.0, 0.2, 0.1, 0.35));
        }

        self.draw_rain(view.pos);

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
                        tr!("[Б] {}", "[B] {}", t.name)
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
        if !self.big_map {
            self.draw_world_marker(ui, view);
            self.draw_squad_world(ui, view);
        }
        self.draw_squad_panel(ui);
        self.draw_br_out(ui);
        if self.alive && self.body.swimming(&self.world) {
            let (w, h) = (screen_width(), screen_height());
            ui.centered(
                t("Пливеш — стріляти не можна", "Swimming — you cannot shoot"),
                w / 2.0,
                h * 0.72,
                16,
                MUTED,
            );
        }

        // crosshair
        if self.alive
            && self.ads_t < 0.5
            && !self.sprinting
            && self.overlay == Overlay::None
            && self.piloting.is_none()
        {
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
        draw_hitmarker(
            cx,
            cy,
            (now - self.hitmarker) as f32,
            self.hit_kill,
            self.hit_head,
        );

        if self.alive {
            self.draw_pilot_hud(ui);
        }
        self.draw_minimap(ui);
        self.draw_score_panel(ui);
        self.draw_br_alerts(ui);
        self.draw_br_inventory(ui);
        self.draw_killfeed(ui);
        self.draw_chat(ui);
        if self.big_map && self.mode == Mode::Br && self.overlay == Overlay::None {
            self.draw_big_map(ui);
        }
        if self.alive && self.piloting.is_none() {
            self.draw_weapon_panel(ui);
        }
        // in a car: speed, its health, how to get out
        if let Some((c, k)) = self.seat() {
            let (w, h) = (screen_width(), screen_height());
            let kmh = c.state.speed.abs() * 3.6;
            let x = w / 2.0 - 110.0;
            let y = h - 92.0;
            draw_rectangle(x, y, 220.0, 64.0, Color::new(0.0, 0.0, 0.0, 0.5));
            ui.bold(
                &format!("{kmh:.0} {}", t("км/год", "km/h")),
                x + 12.0,
                y + 30.0,
                24,
                TEXT,
            );
            ui.text(c.kind.name(), x + 140.0, y + 26.0, 15, MUTED);
            let f = c.hp as f32 / 100.0;
            draw_rectangle(
                x + 12.0,
                y + 40.0,
                196.0,
                7.0,
                Color::new(0.0, 0.0, 0.0, 0.6),
            );
            draw_rectangle(
                x + 12.0,
                y + 40.0,
                196.0 * f,
                7.0,
                if f > 0.35 { GOOD } else { BAD },
            );
            let hint = if k == 0 {
                tr!(
                    "{} — вийти · Пробіл — ручник",
                    "{} — get out · Space — handbrake",
                    key(Action::Use)
                )
            } else {
                tr!("{} — вийти", "{} — get out", key(Action::Use))
            };
            ui.text(&hint, x + 12.0, y + 60.0, 13, MUTED);
        }

        // dropped weapon prompt
        if self.alive && self.overlay == Overlay::None {
            if let Some((_, pick)) = self.pickup_target {
                let what = pick_label(pick);
                let text = if self.near_objective() {
                    tr!(
                        "{} — відійди від бомби, щоб підібрати",
                        "{} — step off the bomb to pick it up",
                        what
                    )
                } else if let Pick::Door(open) = pick {
                    if open {
                        tr!(
                            "{} — зачинити двері",
                            "{} — shut the door",
                            key(Action::Use)
                        )
                    } else {
                        tr!(
                            "{} — відчинити двері",
                            "{} — open the door",
                            key(Action::Use)
                        )
                    }
                } else if let Pick::DeathBox = pick {
                    tr!(
                        "{} — обшукати ящик",
                        "{} — search the box",
                        key(Action::Use)
                    )
                } else if let Pick::Car(_) = pick {
                    tr!("{} — сісти: {}", "{} — get in: {}", key(Action::Use), what)
                } else {
                    tr!(
                        "{} — підібрати {}",
                        "{} — pick up {}",
                        key(Action::Use),
                        what
                    )
                };
                ui.centered(&text, cx, cy + 90.0, 20, TEXT);
                // which guns take these rounds
                if let Pick::Loot(LootKind::Ammo(a), _) = pick {
                    ui.centered(
                        &tr!("для: {}", "for: {}", a.guns().join(", ")),
                        cx,
                        cy + 114.0,
                        15,
                        MUTED,
                    );
                }
            }
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
                ui.centered(
                    t(
                        "Розминка — очікування гравців",
                        "Warmup — waiting for players",
                    ),
                    cx,
                    110.0,
                    22,
                    MUTED,
                );
                if let Some((n, of, me)) = self.ready {
                    let (text, c) = if me {
                        (
                            tr!(
                                "Ти готовий ({n}/{of}) · {} — ще ні",
                                "You are ready ({n}/{of}) · {} — not yet",
                                key(Action::Ready)
                            ),
                            GOOD,
                        )
                    } else {
                        (
                            tr!(
                                "{} — ГОТОВИЙ ({n}/{of} готові)",
                                "{} — READY ({n}/{of} ready)",
                                key(Action::Ready)
                            ),
                            ACCENT,
                        )
                    };
                    ui.centered(&text, cx, 142.0, 26, c);
                }
            }
            MatchState::PreRound => {
                ui.centered(
                    &tr!(
                        "Бій почнеться через {}",
                        "The fight starts in {}",
                        self.time_left.ceil() as i32
                    ),
                    cx,
                    h * 0.38,
                    34,
                    ACCENT,
                );
            }
            _ => {}
        }
        if self.overtime {
            ui.centered(
                t("ОВЕРТАЙМ — без відродження", "OVERTIME — no respawns"),
                cx,
                110.0,
                22,
                BAD,
            );
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
                &tr!(
                    "Захист після відродження: {:.1} с",
                    "Spawn protection: {:.1} s",
                    self.me.protected
                ),
                cx,
                h * 0.78,
                20,
                Color::new(0.6, 0.85, 1.0, 1.0),
            );
            ui.centered(
                &tr!(
                    "із захистом атакувати не можна · {} — зняти захист і битися",
                    "you cannot attack while protected · {} — drop protection and fight",
                    key(Action::Fire)
                ),
                cx,
                h * 0.78 + 22.0,
                14,
                MUTED,
            );
        }

        // death screen
        let br_round = self.mode == Mode::Br && self.state == MatchState::Playing;
        if br_round && !self.alive && !self.br.as_ref().is_some_and(|b| b.in_plane) {
            if let Some((killer, cause, _)) = &self.death_info {
                let text = if killer.is_empty() {
                    tr!("Ти вибув ({})", "You are out ({})", cause.name())
                } else {
                    tr!(
                        "Тебе вибив {killer} — {}",
                        "Knocked out by {killer} — {}",
                        cause.name()
                    )
                };
                ui.centered(&text, cx, h * 0.18, 22, TEXT);
            }
            ui.centered(
                t(
                    "Можеш літати й дивитися, як закінчиться раунд",
                    "Fly around and watch how the round ends",
                ),
                cx,
                h * 0.18 + 26.0,
                16,
                MUTED,
            );
        } else if !self.alive
            && !br_round
            && self.team.is_playing()
            && self.state != MatchState::PostMatch
            && self.overlay == Overlay::None
        {
            if let Some((killer, cause, _)) = &self.death_info {
                let text = if killer.is_empty() {
                    tr!("Ти загинув ({})", "You died ({})", cause.name())
                } else {
                    tr!(
                        "Тебе вбив {killer} — {}",
                        "Killed by {killer} — {}",
                        cause.name()
                    )
                };
                ui.centered(&text, cx, h * 0.62, 26, TEXT);
            }
            let text = if self.overtime && self.state == MatchState::Playing {
                t("Чекай наступного раунду", "Wait for the next round").to_string()
            } else if self.me.respawn_in > 0.0 {
                tr!(
                    "Відродження через {:.0}",
                    "Respawn in {:.0}",
                    self.me.respawn_in.ceil()
                )
            } else if self.has_class {
                tr!("{} — відродитися", "{} — respawn", key(Action::Fire))
            } else {
                t("Обери клас (Esc)", "Choose a class (Esc)").to_string()
            };
            ui.centered(&text, cx, h * 0.62 + 34.0, 20, ACCENT);
            ui.centered(
                t("Esc — змінити клас/команду", "Esc — change class/team"),
                cx,
                h * 0.62 + 62.0,
                16,
                MUTED,
            );
        }
        if self.team == Team::Spectator && self.overlay == Overlay::None {
            ui.centered(
                &tr!(
                    "Режим глядача — {}{}{}{}, {}/{}, {}. Esc — меню",
                    "Spectating — {}{}{}{}, {}/{}, {}. Esc — menu",
                    key(Action::Forward),
                    key(Action::Left),
                    key(Action::Back),
                    key(Action::Right),
                    key(Action::Jump),
                    key(Action::Crouch),
                    key(Action::Sprint)
                ),
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
            // a badge at the top centre, coloured by how smooth the game runs
            let fps_c = if fps >= 55.0 {
                GOOD
            } else if fps >= 30.0 {
                ACCENT
            } else {
                BAD
            };
            let fps_s = format!("{fps:.0} FPS");
            let ping_s = tr!("  {ping} мс", "  {ping} ms");
            let (fw, pw) = (ui.width(&fps_s, 20, true), ui.width(&ping_s, 16, true));
            let bw = fw + pw + 20.0;
            let bx = (w - bw) / 2.0;
            draw_rectangle(bx, 6.0, bw, 30.0, Color::new(0.0, 0.0, 0.0, 0.55));
            draw_rectangle_lines(
                bx,
                6.0,
                bw,
                30.0,
                2.0,
                Color::new(fps_c.r, fps_c.g, fps_c.b, 0.8),
            );
            ui.shadowed(&fps_s, bx + 10.0, 28.0, 20, fps_c);
            let ping_c = if ping < 80 {
                TEXT
            } else if ping < 150 {
                ACCENT
            } else {
                BAD
            };
            ui.shadowed(&ping_s, bx + 10.0 + fw, 27.0, 16, ping_c);
        }

        // match end / scoreboard
        let tab = crate::binds::down(crate::binds::Action::Scoreboard) && self.chat_input.is_none();
        if let Some((text, winner, _)) = self.match_end.clone() {
            let c = match winner {
                Some(t) if t == self.team => GOOD,
                Some(_) => BAD,
                None => TEXT,
            };
            let title = match winner {
                Some(t) if t == self.team => toon_shared::lang::t("ПЕРЕМОГА!", "VICTORY!"),
                Some(_) if self.team.is_playing() => t("ПОРАЗКА", "DEFEAT"),
                _ => t("МАТЧ ЗАВЕРШЕНО", "MATCH OVER"),
            };
            ui.centered(title, cx, 80.0, 48, c);
            ui.centered(&text, cx, 118.0, 22, TEXT);
            // the table comes after the final killcam
            if !self.final_killcam_waiting() {
                self.draw_scoreboard(ui, 150.0);
            }
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

    /// The map marker over the world, with how far it is (on the screen's edge when it
    /// is behind or to the side).
    fn draw_world_marker(&self, ui: &Ui, view: &View) {
        let Some(mk) = self.marker else { return };
        let (w, h) = (screen_width(), screen_height());
        let ground = self.world.ground(mk.x, mk.y).unwrap_or(0.0);
        let at = vec3(mk.x, ground + 2.0, mk.y);
        let (eye, yaw, _) = self.camera();
        let dist = vec2(mk.x - eye.x, mk.y - eye.z).length();
        let label = if dist >= 1000.0 {
            format!("{:.1} {}", dist / 1000.0, t("км", "km"))
        } else {
            format!("{:.0} {}", dist, t("м", "m"))
        };
        let on_screen = view
            .project(at)
            .filter(|p| p.x > 20.0 && p.x < w - 20.0 && p.y > 30.0 && p.y < h - 20.0);
        let p = match on_screen {
            Some(p) => p,
            None => {
                // off screen: along the top edge, left or right by where it lies
                let to = vec3(mk.x - eye.x, 0.0, mk.y - eye.z);
                let side = toon_shared::wrap_angle(toon_shared::yaw_of(to) - yaw);
                let x = w / 2.0 - (side / std::f32::consts::PI) * (w / 2.0 - 30.0);
                vec2(x.clamp(30.0, w - 30.0), 70.0)
            }
        };
        draw_marker_pin(p, 0.9);
        let tw = ui.width(&label, 15, true);
        ui.shadowed(
            &label,
            p.x - tw / 2.0,
            p.y + 16.0,
            15,
            Color::new(1.0, 0.85, 0.15, 1.0),
        );
    }

    /// Colour of a squadmate by their place in the squad (we are yellow, like our marker).
    pub fn mate_color(&self, id: u8) -> Color {
        const C: [Color; 4] = [
            Color::new(1.0, 0.85, 0.15, 1.0),
            Color::new(0.35, 0.8, 1.0, 1.0),
            Color::new(0.5, 1.0, 0.4, 1.0),
            Color::new(1.0, 0.55, 0.3, 1.0),
        ];
        if id == self.my_id {
            return C[0];
        }
        let others = self
            .br
            .as_ref()
            .map(|b| {
                b.squad
                    .iter()
                    .filter(|m| m.id != self.my_id)
                    .position(|m| m.id == id)
                    .unwrap_or(0)
            })
            .unwrap_or(0);
        C[1 + others % 3]
    }

    /// Rain streaking down the screen (not under a roof, not high up in the plane).
    fn draw_rain(&self, eye: Vec3) {
        if self.weather != toon_shared::weather::Weather::Rain || self.killcam.is_some() {
            return;
        }
        if !self.world.line_of_sight(eye, eye + vec3(0.0, 25.0, 0.0)) {
            return;
        }
        let ground = self.world.ground(eye.x, eye.z).unwrap_or(0.0);
        let fade = (1.0 - (eye.y - ground - 60.0).max(0.0) / 60.0).clamp(0.0, 1.0);
        if fade <= 0.0 {
            return;
        }
        let (w, h) = (screen_width(), screen_height());
        // looking up the drops come at you, looking down they fall away
        let len = 14.0 + 10.0 * (1.0 - self.camera().2.abs().min(1.0));
        let n = 140;
        let t = self.now as f32;
        draw_rectangle(0.0, 0.0, w, h, Color::new(0.2, 0.24, 0.3, 0.08 * fade));
        for i in 0..n {
            let k = i as f32;
            let speed = 0.9 + (k * 0.731).fract() * 0.6;
            let x0 = (k * 0.618_034).fract() * (w + 80.0) - 40.0;
            let y = ((k * 0.377).fract() + t * speed).fract() * (h + len) - len;
            let x = x0 - (y / h) * 30.0;
            let a = (0.18 + 0.2 * (k * 0.913).fract()) * fade;
            draw_line(x, y, x - 4.0, y + len, 1.2, Color::new(0.8, 0.85, 0.95, a));
        }
    }

    /// Out of the battle royale: whom we watch, and how the round went.
    fn draw_br_out(&self, ui: &Ui) {
        let (w, h) = (screen_width(), screen_height());
        if self.br_out() {
            let name = self
                .spectate
                .and_then(|t| self.name_of(t))
                .map(|n| n.to_string());
            let line = match name {
                Some(n) => tr!(
                    "Спостерігаєш: {n}  ·  ЛКМ / ПКМ — наступний / попередній  ·  Пробіл — вільна камера",
                    "Watching: {n}  ·  LMB / RMB — next / previous  ·  Space — free camera"
                ),
                None => t(
                    "Вільна камера  ·  Пробіл — дивитися очима гравців",
                    "Free camera  ·  Space — watch through players' eyes",
                )
                .into(),
            };
            draw_rectangle(0.0, h - 40.0, w, 40.0, Color::new(0.0, 0.0, 0.0, 0.45));
            ui.centered(&line, w / 2.0, h - 14.0, 16, TEXT);
        }
        // the round in numbers (once out, or when it is over)
        let Some(s) = self.br.as_ref().and_then(|b| b.summary.clone()) else {
            return;
        };
        if !(self.br_out() || self.match_end.is_some()) || self.killcam.is_some() {
            return;
        }
        let pw = 520.0f32.min(w - 20.0);
        let (x, y) = ((w - pw) / 2.0, 150.0);
        draw_rectangle(x, y, pw, 96.0, Color::new(0.0, 0.0, 0.0, 0.55));
        let title = if s.place == 1 {
            t("ПЕРЕМОГА!", "WINNER!").to_string()
        } else {
            tr!("Місце #{} з {}", "Place #{} of {}", s.place, s.of)
        };
        ui.centered(
            &title,
            w / 2.0,
            y + 30.0,
            26,
            if s.place == 1 { ACCENT } else { TEXT },
        );
        let mins = (s.survived / 60.0) as u32;
        let secs = (s.survived % 60.0) as u32;
        let line = tr!(
            "Вбивства {} · Урон {:.0} · Найдальше {:.0} м · Протримався {}:{:02}",
            "Kills {} · Damage {:.0} · Longest {:.0} m · Survived {}:{:02}",
            s.kills,
            s.damage,
            s.longest,
            mins,
            secs
        );
        ui.centered(&line, w / 2.0, y + 58.0, 16, TEXT);
        let total = tr!(
            "Усього на сервері: матчів {} · перемог {} · вбивств {} · найкраще місце #{}",
            "On this server: matches {} · wins {} · kills {} · best place #{}",
            s.matches,
            s.wins,
            s.total_kills,
            s.best.max(1)
        );
        ui.centered(&total, w / 2.0, y + 82.0, 14, MUTED);
    }

    /// The squad on the left: names, health, who is down.
    fn draw_squad_panel(&self, ui: &Ui) {
        let Some(br) = &self.br else { return };
        if br.squad.len() < 2 {
            return;
        }
        let x = 16.0;
        let mut y = screen_height() * 0.5 - 20.0;
        for m in &br.squad {
            let c = self.mate_color(m.id);
            draw_rectangle(x, y, 190.0, 30.0, Color::new(0.0, 0.0, 0.0, 0.45));
            draw_rectangle(x, y, 5.0, 30.0, c);
            let name: String = m.name.chars().take(16).collect();
            ui.text(
                &name,
                x + 12.0,
                y + 14.0,
                14,
                if m.alive { TEXT } else { MUTED },
            );
            let (frac, col) = if !m.alive {
                (0.0, MUTED)
            } else if m.downed {
                (m.hp / 100.0, BAD)
            } else {
                (m.hp / MAX_HEALTH, GOOD)
            };
            draw_rectangle(
                x + 12.0,
                y + 20.0,
                168.0,
                5.0,
                Color::new(0.0, 0.0, 0.0, 0.5),
            );
            draw_rectangle(x + 12.0, y + 20.0, 168.0 * frac.clamp(0.0, 1.0), 5.0, col);
            if m.downed {
                ui.text(t("ПОРАНЕНИЙ", "DOWN"), x + 120.0, y + 14.0, 12, BAD);
            } else if !m.alive {
                ui.text(t("вибув", "out"), x + 140.0, y + 14.0, 12, MUTED);
            }
            y += 34.0;
        }
    }

    /// Squadmates' markers and the ones who are down, over the world; knocked down
    /// ourselves: what is left of us; next to a squadmate who is down: how to pick them up.
    fn draw_squad_world(&self, ui: &Ui, view: &View) {
        let Some(br) = &self.br else { return };
        let (w, h) = (screen_width(), screen_height());
        let (eye, _, _) = self.camera();
        for m in br.squad.iter().filter(|m| m.id != self.my_id) {
            let c = self.mate_color(m.id);
            if let Some(mk) = m.marker {
                let ground = self.world.ground(mk[0], mk[1]).unwrap_or(0.0);
                if let Some(p) = view.project(vec3(mk[0], ground + 2.0, mk[1])) {
                    if p.x > 0.0 && p.x < w && p.y > 0.0 && p.y < h {
                        draw_circle(p.x, p.y - 14.0, 7.0, c);
                        draw_circle_lines(p.x, p.y - 14.0, 7.0, 2.0, BLACK);
                        let d = vec2(mk[0] - eye.x, mk[1] - eye.z).length();
                        let label = format!("{:.0} {}", d, t("м", "m"));
                        let tw = ui.width(&label, 13, true);
                        ui.shadowed(&label, p.x - tw / 2.0, p.y + 6.0, 13, c);
                    }
                }
            }
            if m.alive && m.downed {
                let pos = Vec3::from_array(m.pos) + vec3(0.0, 1.0, 0.0);
                if let Some(p) = view.project(pos) {
                    if p.x > 0.0 && p.x < w && p.y > 0.0 && p.y < h {
                        draw_circle(p.x, p.y, 11.0, BAD);
                        draw_rectangle(p.x - 2.0, p.y - 7.0, 4.0, 14.0, WHITE);
                        draw_rectangle(p.x - 7.0, p.y - 2.0, 14.0, 4.0, WHITE);
                        let d = pos.distance(eye);
                        let label = format!("{} · {:.0} {}", m.name, d, t("м", "m"));
                        let tw = ui.width(&label, 13, true);
                        ui.shadowed(&label, p.x - tw / 2.0, p.y + 26.0, 13, BAD);
                    }
                }
            }
        }
        // knocked down ourselves
        if self.downed() {
            draw_rectangle(0.0, 0.0, w, h, Color::new(0.5, 0.0, 0.0, 0.18));
            ui.centered(
                t("ТЕБЕ ПОРАНЕНО", "YOU ARE DOWN"),
                w / 2.0,
                h * 0.62,
                30,
                BAD,
            );
            ui.centered(
                t(
                    "Повзи в укриття — напарник може підняти (тримаючи дію)",
                    "Crawl to cover — a squadmate can pick you up (holding use)",
                ),
                w / 2.0,
                h * 0.62 + 26.0,
                16,
                TEXT,
            );
            let bw = 260.0;
            let (bx, by) = ((w - bw) / 2.0, h * 0.62 + 40.0);
            draw_rectangle(bx, by, bw, 8.0, Color::new(0.0, 0.0, 0.0, 0.6));
            draw_rectangle(bx, by, bw * (self.me.hp / 100.0).clamp(0.0, 1.0), 8.0, BAD);
            if self.me.progress > 0.0 {
                draw_rectangle(
                    bx,
                    by + 12.0,
                    bw * self.me.progress.clamp(0.0, 1.0),
                    8.0,
                    GOOD,
                );
            }
            return;
        }
        // a squadmate down right next to us
        if self.alive {
            if let Some(m) = br.squad.iter().find(|m| {
                m.id != self.my_id
                    && m.alive
                    && m.downed
                    && Vec3::from_array(m.pos).distance(self.body.pos) < 2.0
            }) {
                ui.centered(
                    &tr!(
                        "Тримай {} — підняти {}",
                        "Hold {} — pick up {}",
                        key(Action::Use),
                        m.name
                    ),
                    w / 2.0,
                    h * 0.62,
                    20,
                    GOOD,
                );
                if self.me.progress > 0.0 {
                    let bw = 220.0;
                    let (bx, by) = ((w - bw) / 2.0, h * 0.62 + 12.0);
                    draw_rectangle(bx, by, bw, 8.0, Color::new(0.0, 0.0, 0.0, 0.6));
                    draw_rectangle(bx, by, bw * self.me.progress.clamp(0.0, 1.0), 8.0, GOOD);
                }
            }
        }
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
                toon_shared::lang::t("ЗАХИСТИТИ", "DEFEND")
            } else {
                toon_shared::lang::t("ПІДІРВАТИ", "DESTROY")
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
            let txt = tr!("{label} {d:.0}м", "{label} {d:.0}m");
            let tw = ui.width(&txt, 13, true);
            ui.shadowed(&txt, sp.x - tw / 2.0, sp.y + r + 16.0, 13, c);
        }
        // a teammate carrying the bomb: an icon over their head, seen through walls, so
        // everyone knows whom to cover
        if let (Some(c), None) = (b.carrier, b.planted_at) {
            let ours = self.remotes.get(&c).filter(|r| r.alive && r.team == me);
            if let Some(r) = ours {
                if let Some(sp) = view.project(r.pos + vec3(0.0, 3.0, 0.0)) {
                    let pulse = 0.5 + 0.5 * (self.now * 4.0).sin() as f32;
                    draw_bomb_icon(sp.x, sp.y, 1.2, pulse);
                    let d = r.pos.distance(self.body.pos);
                    ui.centered(
                        &tr!("ПРИКРИЙ {d:.0}м", "COVER {d:.0}m"),
                        sp.x,
                        sp.y - 24.0,
                        13,
                        team_color(me),
                    );
                }
            }
        }
        // the bomb itself when lying around
        if b.carrier.is_none() && b.planted_at.is_none() {
            if let Some(sp) = view.project(Vec3::from_array(b.pos) + vec3(0.0, 1.2, 0.0)) {
                draw_circle(sp.x, sp.y - 8.0, 11.0, Color::new(0.0, 0.0, 0.0, 0.7));
                draw_circle(sp.x, sp.y - 8.0, 8.0, BAD);
                let d = Vec3::from_array(b.pos).distance(self.body.pos);
                ui.centered(
                    &tr!("бомба {d:.0}м", "bomb {d:.0}m"),
                    sp.x,
                    sp.y + 18.0,
                    13,
                    ACCENT,
                );
            }
        }
        // planted timer
        if b.planted_at.is_some() {
            ui.centered(
                &tr!("БОМБА {:.1}", "BOMB {:.1}", b.timer),
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
                t("Знешкодження…", "Defusing…")
            } else {
                t("Закладання…", "Planting…")
            };
            ui.centered(label, w / 2.0, h * 0.7 - 10.0, 18, TEXT);
        } else if self.alive {
            let carrying = b.carrier == Some(self.my_id);
            let enemy_site = self.map.sites[me.enemy().index()];
            if carrying && self.body.pos.distance(enemy_site) < SAB_SITE_RADIUS {
                ui.centered(
                    &tr!(
                        "Тримай {}, щоб закласти бомбу",
                        "Hold {} to plant the bomb",
                        key(Action::Use)
                    ),
                    w / 2.0,
                    h * 0.7,
                    22,
                    ACCENT,
                );
            } else if b.planted_at == Some(me)
                && self.body.pos.distance(Vec3::from_array(b.pos)) < SAB_DEFUSE_RADIUS
            {
                ui.centered(
                    &tr!(
                        "Тримай {}, щоб знешкодити бомбу",
                        "Hold {} to defuse the bomb",
                        key(Action::Use)
                    ),
                    w / 2.0,
                    h * 0.7,
                    22,
                    ACCENT,
                );
            } else if carrying {
                ui.centered(
                    t(
                        "У тебе бомба — неси її на ворожу точку!",
                        "You have the bomb — take it to the enemy site!",
                    ),
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
        // pixels per metre: big maps show a wider area
        let scale = size / if self.level.terrain { 200.0 } else { 55.0 };
        let c = vec2(x0 + size / 2.0, y0 + size / 2.0);
        draw_rectangle(
            x0 - 4.0,
            y0 - 4.0,
            size + 8.0,
            size + 8.0,
            Color::new(0.0, 0.0, 0.0, 0.7),
        );
        draw_rectangle(x0, y0, size, size, Color::new(0.16, 0.22, 0.18, 0.85));
        let plane = self
            .br
            .as_ref()
            .filter(|b| b.in_plane)
            .and_then(|b| b.plane);
        let (pos, yaw) = if let Some((p, _)) = plane {
            // riding the plane: where it is now
            (Vec3::from_array(p), self.yaw)
        } else if self.alive || !self.team.is_playing() {
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
        if let Some(img) = &self.level.image {
            // big maps: the painted map, turned with the view
            let corner = |u: f32, v: f32| {
                let w = img.origin + vec2(u, v) * img.size;
                Vertex::new2(to_map(vec3(w.x, 0.0, w.y)).extend(0.0), vec2(u, v), WHITE)
            };
            let mesh = Mesh {
                vertices: vec![
                    corner(0.0, 0.0),
                    corner(1.0, 0.0),
                    corner(1.0, 1.0),
                    corner(0.0, 1.0),
                ],
                indices: vec![0, 1, 2, 0, 2, 3],
                texture: Some(img.tex.clone()),
            };
            draw_mesh(&mesh);
        }
        if let Some(br) = self.br.as_ref().filter(|b| b.zone[2] > 0.0) {
            let c = to_map(vec3(br.zone[0], 0.0, br.zone[1]));
            draw_circle_lines(
                c.x,
                c.y,
                br.zone[2] * scale,
                3.0,
                Color::new(0.3, 0.55, 1.0, 1.0),
            );
            let n = to_map(vec3(br.next[0], 0.0, br.next[1]));
            draw_circle_lines(n.x, n.y, br.next[2] * scale, 2.0, WHITE);
        }
        // the red zone
        if let Some(rz) = self.br.as_ref().and_then(|b| b.red) {
            let p = to_map(vec3(rz[0], 0.0, rz[1]));
            draw_circle(p.x, p.y, rz[2] * scale, Color::new(0.9, 0.1, 0.1, 0.22));
            draw_circle_lines(
                p.x,
                p.y,
                rz[2] * scale,
                2.0,
                Color::new(0.9, 0.15, 0.1, 0.9),
            );
        }
        // supply crates (on the edge when further away)
        for d in self.br.iter().flat_map(|b| b.drops.iter()) {
            let p = to_map(vec3(d[0], 0.0, d[2]));
            let r = size / 2.0 - 8.0;
            let dd = p - c;
            let p = if dd.length() > r {
                c + dd.normalize() * r
            } else {
                p
            };
            draw_crate_icon(p, 0.7);
        }
        // squadmates' markers
        for m in self
            .br
            .iter()
            .flat_map(|b| b.squad.iter())
            .filter(|m| m.id != self.my_id)
        {
            if let Some(mk) = m.marker {
                let p = to_map(vec3(mk[0], 0.0, mk[1]));
                let r = size / 2.0 - 8.0;
                let d = p - c;
                let p = if d.length() > r {
                    c + d.normalize() * r
                } else {
                    p
                };
                draw_circle(p.x, p.y, 5.0, self.mate_color(m.id));
                draw_circle_lines(p.x, p.y, 5.0, 1.5, BLACK);
            }
        }
        // the marker (kept on the edge when it is further than the minimap shows)
        if let Some(mk) = self.marker {
            let p = to_map(vec3(mk.x, 0.0, mk.y));
            let r = size / 2.0 - 8.0;
            let d = p - c;
            let p = if d.length() > r {
                c + d.normalize() * r
            } else {
                p
            };
            draw_marker_pin(p, 0.7);
        }
        for b in &self.map.boxes {
            if self.level.terrain {
                break;
            }
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
            // battle royale: only the squad is on our side
            if self.is_friend(r) && r.downed {
                draw_circle(p.x, p.y, 6.0, BLACK);
                draw_circle(p.x, p.y, 5.0, BAD);
                draw_rectangle(p.x - 1.0, p.y - 3.5, 2.0, 7.0, WHITE);
                draw_rectangle(p.x - 3.5, p.y - 1.0, 7.0, 2.0, WHITE);
            } else if self.is_friend(r) {
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
        // artillery about to land: a pulsing red ring
        for (t, r, at) in &self.artillery {
            let p = to_map(*t);
            let pulse = 0.5 + 0.5 * ((self.now - at) * 6.0).sin() as f32;
            draw_circle_lines(p.x, p.y, r * scale, 2.0 + pulse * 2.0, BAD);
        }
        // jeeps and boats
        for c in self.cars.values() {
            let p = to_map(c.state.pos);
            let col = if c.seats.contains(&self.my_id) {
                ACCENT
            } else {
                Color::new(0.85, 0.85, 0.8, 0.9)
            };
            draw_rectangle(p.x - 4.0, p.y - 3.0, 8.0, 6.0, BLACK);
            draw_rectangle(p.x - 3.0, p.y - 2.0, 6.0, 4.0, col);
        }
        // vehicles: ours always, enemy ones like enemies (on radar)
        for e in &self.ents {
            if let EntSnap::Vehicle { pos, team, .. } = e {
                let friendly = *team == self.team;
                if !friendly && !self.me.uav {
                    continue;
                }
                let p = to_map(Vec3::from_array(*pos));
                let c = if friendly { team_color(*team) } else { BAD };
                draw_rectangle(p.x - 4.0, p.y - 4.0, 8.0, 8.0, BLACK);
                draw_rectangle(p.x - 3.0, p.y - 3.0, 6.0, 6.0, c);
            }
        }
        for e in &self.ents {
            if let EntSnap::Smoke { pos, age, .. } = e {
                let p = to_map(Vec3::from_array(*pos));
                let r = toon_shared::consts::smoke_radius(*age) * scale;
                draw_circle(p.x, p.y, r, Color::new(0.85, 0.86, 0.88, 0.75));
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
            (t("Пн", "N"), 0.0f32),
            (t("Сх", "E"), -std::f32::consts::FRAC_PI_2),
            (t("Пд", "S"), std::f32::consts::PI),
            (t("Зх", "W"), std::f32::consts::FRAC_PI_2),
        ] {
            let rel = a - yaw;
            let d = vec2(-rel.sin(), -rel.cos());
            let p = c + d * (size / 2.0 - 10.0);
            let tw = ui.width(label, 12, true);
            ui.shadowed(label, p.x - tw / 2.0, p.y + 5.0, 12, TEXT);
        }
        if self.me.uav {
            ui.shadowed(t("БПЛА", "UAV"), x0 + 6.0, y0 + size - 8.0, 14, GOOD);
        }
    }

    /// Battle royale: players left, kills, the zone clock, health and armour.
    fn draw_br_panel(&self, ui: &Ui) {
        let Some(br) = &self.br else { return };
        let size = (screen_height() * 0.26).clamp(150.0, 240.0);
        let (x, y) = (16.0, 16.0 + size + 12.0);
        let w = size;
        let panel_h = if self.alive { 112.0 } else { 60.0 };
        draw_rectangle(
            x - 4.0,
            y - 4.0,
            w + 8.0,
            panel_h,
            Color::new(0.0, 0.0, 0.0, 0.55),
        );
        let kills = self
            .scoreboard
            .iter()
            .find(|r| r.id == self.my_id)
            .map(|r| r.kills)
            .unwrap_or(0);
        let at = tr!("Живих: {}", "Alive: {}", br.alive);
        ui.shadowed(&at, x + 4.0, y + 20.0, 18, TEXT);
        let kt = tr!("Вбивств: {kills}", "Kills: {kills}");
        let kw = ui.width(&kt, 15, true);
        if ui.width(&at, 18, true) + kw + 16.0 < w {
            ui.shadowed(&kt, x + w - kw - 4.0, y + 20.0, 15, ACCENT);
        }
        let secs = br.timer.max(0.0) as u32;
        let clock = format!("{}:{:02}", secs / 60, secs % 60);
        let (zt, zc) = if self.state != MatchState::Playing || br.zone[2] <= 0.0 {
            (String::new(), MUTED)
        } else if br.shrinking {
            (
                tr!("Зона звужується! {clock}", "Zone closing! {clock}"),
                BAD,
            )
        } else if br.next[2] < br.zone[2] - 0.5 {
            (
                tr!("Зона рушить через {clock}", "Zone moves in {clock}"),
                TEXT,
            )
        } else {
            (t("Остання зона", "Final zone").into(), BAD)
        };
        ui.shadowed(&zt, x + 4.0, y + 46.0, 16, zc);
        // health and armour
        if self.alive {
            let bar = |yy: f32, k: f32, c: Color| {
                draw_rectangle(x + 4.0, yy, w - 8.0, 10.0, Color::new(0.0, 0.0, 0.0, 0.5));
                draw_rectangle(x + 4.0, yy, (w - 8.0) * k.clamp(0.0, 1.0), 10.0, c);
            };
            ui.shadowed(t("Здоров'я", "Health"), x + 4.0, y + 66.0, 12, MUTED);
            bar(y + 70.0, self.me.hp / toon_shared::consts::MAX_HEALTH, GOOD);
            ui.shadowed(t("Броня", "Armour"), x + 4.0, y + 94.0, 12, MUTED);
            let vest = toon_shared::inventory::armor_durability(br.inv.armor);
            bar(
                y + 96.0,
                if vest > 0.0 {
                    br.inv.armor_hp / vest
                } else {
                    0.0
                },
                Color::new(0.4, 0.6, 1.0, 1.0),
            );
        }
    }

    /// Battle royale: what we carry, bottom left — medkits, vest, helmet, backpack,
    /// grenades and the rounds of every calibre we have.
    fn draw_br_inventory(&self, ui: &Ui) {
        use toon_shared::inventory::{
            armor_durability, helmet_durability, level_name, AmmoType, MEDKIT_TIME,
        };
        let Some(br) = &self.br else { return };
        if !self.alive || br.in_plane {
            return;
        }
        let inv = &br.inv;
        let h = screen_height();
        let (x0, y0) = (16.0, h - 118.0);
        let s = 60.0;
        let tile = |k: usize, label: &str, value: &str, fill: f32, c: Color, on: bool| {
            let x = x0 + k as f32 * (s + 6.0);
            draw_rectangle(
                x,
                y0,
                s,
                s,
                Color::new(0.0, 0.0, 0.0, if on { 0.55 } else { 0.3 }),
            );
            if fill > 0.0 {
                draw_rectangle(x, y0 + s - 5.0, s * fill.clamp(0.0, 1.0), 5.0, c);
            }
            let col = if on { TEXT } else { MUTED };
            let lw = ui.width(label, 12, false);
            ui.text(label, x + (s - lw) / 2.0, y0 + 15.0, 12, col);
            let vw = ui.width(value, 20, true);
            ui.shadowed(
                value,
                x + (s - vw) / 2.0,
                y0 + 40.0,
                20,
                if on { c } else { MUTED },
            );
        };
        tile(
            0,
            t("Аптечки", "Medkits"),
            &format!("{}/{}", inv.medkits, inv.medkit_cap()),
            0.0,
            Color::new(1.0, 0.4, 0.4, 1.0),
            inv.medkits > 0,
        );
        // the key that uses one
        let hint = format!("[{}]", key(Action::Heal));
        let hw = ui.width(&hint, 12, false);
        ui.text(&hint, x0 + (s - hw) / 2.0, y0 + s - 3.0, 12, MUTED);
        let blue = Color::new(0.45, 0.65, 1.0, 1.0);
        tile(
            1,
            t("Броня", "Vest"),
            if inv.armor > 0 {
                level_name(inv.armor)
            } else {
                "—"
            },
            if inv.armor > 0 {
                inv.armor_hp / armor_durability(inv.armor)
            } else {
                0.0
            },
            blue,
            inv.armor > 0,
        );
        tile(
            2,
            t("Шолом", "Helmet"),
            if inv.helmet > 0 {
                level_name(inv.helmet)
            } else {
                "—"
            },
            if inv.helmet > 0 {
                inv.helmet_hp / helmet_durability(inv.helmet)
            } else {
                0.0
            },
            blue,
            inv.helmet > 0,
        );
        tile(
            3,
            t("Рюкзак", "Pack"),
            if inv.backpack > 0 {
                level_name(inv.backpack)
            } else {
                "—"
            },
            0.0,
            ACCENT,
            inv.backpack > 0,
        );
        {
            use toon_shared::inventory::sight_short;
            let l = self.sight();
            tile(
                4,
                t("Приціл", "Sight"),
                sight_short(l),
                0.0,
                Color::new(1.0, 0.45, 0.35, 1.0),
                l > 0,
            );
        }
        // rounds of every calibre carried
        let mut x = x0;
        let y = y0 + s + 18.0;
        for a in AmmoType::ALL {
            let n = inv.ammo(a);
            if n == 0 {
                continue;
            }
            let txt = format!("{} {}", a.short(), n);
            ui.shadowed(&txt, x, y, 14, TEXT);
            x += ui.width(&txt, 14, true) + 14.0;
        }
        // bandages, drinks, what G throws
        {
            use toon_shared::inventory::{THROW_FLASH, THROW_MOLOTOV};
            let mut parts: Vec<String> = Vec::new();
            if inv.bandages > 0 {
                parts.push(format!(
                    "{} {} [{}]",
                    t("Бинти", "Bandages"),
                    inv.bandages,
                    key(Action::Bandage)
                ));
            }
            if inv.drinks > 0 {
                parts.push(format!(
                    "{} {} [{}]",
                    t("Енергетик", "Drink"),
                    inv.drinks,
                    key(Action::Drink)
                ));
            }
            let throw = match self.throw_kind {
                THROW_MOLOTOV => format!("{} {}", t("Молотов", "Molotov"), inv.molotovs),
                THROW_FLASH => format!("{} {}", t("Світлошумова", "Flashbang"), inv.flashes),
                _ => format!("{} {}", t("Граната", "Grenade"), self.grenades),
            };
            if inv.molotovs > 0 || inv.flashes > 0 || self.throw_kind != 0 {
                parts.push(format!(
                    "{}: {throw} [{}]",
                    key(Action::Grenade),
                    key(Action::NextThrow)
                ));
            }
            if !parts.is_empty() {
                ui.shadowed(&parts.join("  ·  "), x0, y + 18.0, 13, MUTED);
            }
            // the energy drink's boost: a thin yellow bar along the panel
            if inv.boost > 0.0 {
                let bw = 4.0 * s + 18.0;
                draw_rectangle(x0, y0 - 8.0, bw, 4.0, Color::new(0.0, 0.0, 0.0, 0.5));
                draw_rectangle(
                    x0,
                    y0 - 8.0,
                    bw * inv.boost / 100.0,
                    4.0,
                    Color::new(1.0, 0.8, 0.2, 1.0),
                );
            }
        }
        // using a medkit
        if inv.healing > 0.0 {
            let (w, hh) = (screen_width(), h);
            let total = match inv.using {
                toon_shared::inventory::USE_BANDAGE => toon_shared::inventory::BANDAGE_TIME,
                toon_shared::inventory::USE_DRINK => toon_shared::inventory::DRINK_TIME,
                _ => MEDKIT_TIME,
            };
            let k = 1.0 - inv.healing / total;
            let bw = 220.0;
            draw_rectangle(
                (w - bw) / 2.0,
                hh * 0.66,
                bw,
                10.0,
                Color::new(0.0, 0.0, 0.0, 0.6),
            );
            draw_rectangle((w - bw) / 2.0, hh * 0.66, bw * k, 10.0, GOOD);
            let what = match inv.using {
                toon_shared::inventory::USE_BANDAGE => t("Бинт…", "Bandage…"),
                toon_shared::inventory::USE_DRINK => t("Енергетик…", "Energy drink…"),
                _ => t("Аптечка…", "Medkit…"),
            };
            ui.centered(what, w / 2.0, hh * 0.66 - 8.0, 16, TEXT);
        }
    }

    /// The battle royale inventory screen (I): guns and their sights, spare sights, gear,
    /// medkits and rounds, each with a button to throw it away.
    fn draw_inventory_screen(&mut self, ui: &mut Ui) {
        use toon_shared::inventory::{
            armor_durability, helmet_durability, level_name, sight_fits, sight_name, AmmoType,
            MAX_SIGHT,
        };
        use toon_shared::protocol::{Item, ItemAction};
        let Some(br) = self.br.clone() else {
            self.overlay = Overlay::None;
            return;
        };
        if !self.alive || br.in_plane {
            self.overlay = Overlay::None;
            return;
        }
        let again = crate::binds::pressed(Action::Inventory) && self.now > self.inventory_opened;
        if is_key_pressed(KeyCode::Escape) || again {
            self.overlay = Overlay::None;
            return;
        }
        let inv = br.inv;
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
        let pw = 820.0f32.min(w - 20.0);
        let ph = 560.0f32.min(h - 20.0);
        let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
        ui.panel(x, y, pw, ph);
        ui.bold(t("Інвентар", "Inventory"), x + 24.0, y + 42.0, 26, ACCENT);
        ui.text(
            &tr!(
                "{} / Esc — закрити. Викинуте падає перед тобою.",
                "{} / Esc — close. What you throw away lands in front of you.",
                key(Action::Inventory)
            ),
            x + 190.0,
            y + 38.0,
            14,
            MUTED,
        );
        let mut act: Option<ItemAction> = None;
        let small = |ui: &mut Ui, label: &str, bx: f32, by: f32, bw: f32| -> bool {
            ui.button2(label, bx, by, bw, 28.0)
        };
        let drop_txt = t("Викинути", "Drop");

        // ---- left: guns and sights
        let lx = x + 24.0;
        let cw = pw / 2.0 - 36.0;
        let mut ly = y + 80.0;
        ui.bold(t("Зброя", "Guns"), lx, ly, 18, TEXT);
        ly += 12.0;
        for k in 0..2usize {
            let (wid, mag, res) = self.weapons[k];
            let empty = br.empty_slot == Some(k as u8);
            draw_rectangle(lx, ly, cw, 84.0, Color::new(0.0, 0.0, 0.0, 0.3));
            if empty {
                ui.text(
                    t("— порожньо —", "— empty —"),
                    lx + 12.0,
                    ly + 38.0,
                    16,
                    MUTED,
                );
            } else {
                ui.bold(
                    &format!("{}. {}", k + 1, wid.def().name),
                    lx + 12.0,
                    ly + 24.0,
                    18,
                    if self.slot as usize == k {
                        ACCENT
                    } else {
                        TEXT
                    },
                );
                let mut what = vec![if inv.sights[k] > 0 {
                    sight_name(inv.sights[k]).to_string()
                } else {
                    t("без прицілу", "no sight").to_string()
                }];
                what.extend(
                    inv.mods[k]
                        .iter()
                        .filter(|m| **m != 0)
                        .map(|m| toon_shared::inventory::mod_name(*m).to_string()),
                );
                let line = format!("{mag} / {res} · {}", what.join(" · "));
                let mut size = 15u16;
                while size > 11 && ui.width(&line, size, false) > cw - 24.0 {
                    size -= 1;
                }
                ui.text(&line, lx + 12.0, ly + 74.0, size, MUTED);
                if inv.mods[k].iter().any(|m| *m != 0)
                    && small(
                        ui,
                        t("Зняти обвіси", "Mods off"),
                        lx + cw - 2.0 * 104.0 - 16.0,
                        ly + 40.0,
                        104.0,
                    )
                {
                    act = Some(ItemAction::DetachMods(k as u8));
                }
                let bw = 104.0;
                if inv.sights[k] > 0
                    && small(
                        ui,
                        t("Зняти приціл", "Sight off"),
                        lx + cw - 2.0 * bw - 16.0,
                        ly + 8.0,
                        bw,
                    )
                {
                    act = Some(ItemAction::DetachSight(k as u8));
                }
                let other_real = br.empty_slot != Some(1 - k as u8);
                if other_real && small(ui, drop_txt, lx + cw - bw - 8.0, ly + 8.0, bw) {
                    act = Some(ItemAction::Drop(Item::Weapon(k as u8), 1));
                }
            }
            ly += 92.0;
        }
        ly += 16.0;
        ui.bold(
            &format!(
                "{} ({}/{})",
                t(
                    "Приціли й обвіси в рюкзаку",
                    "Sights and mods in the backpack"
                ),
                inv.spare_count(),
                inv.sight_cap()
            ),
            lx,
            ly,
            18,
            TEXT,
        );
        ly += 12.0;
        let mut any = false;
        for level in 1..=MAX_SIGHT {
            let n = inv.spare_sights[level as usize - 1];
            if n == 0 {
                continue;
            }
            any = true;
            draw_rectangle(lx, ly, cw, 36.0, Color::new(0.0, 0.0, 0.0, 0.3));
            let label = if n > 1 {
                format!("{} ×{n}", sight_name(level))
            } else {
                sight_name(level).to_string()
            };
            ui.text(&label, lx + 12.0, ly + 24.0, 16, TEXT);
            let mut bx = lx + cw - 8.0;
            bx -= 84.0;
            if small(ui, drop_txt, bx, ly + 4.0, 84.0) {
                act = Some(ItemAction::Drop(Item::Sight(level), 1));
            }
            for k in (0..2usize).rev() {
                let ok = br.empty_slot != Some(k as u8) && sight_fits(level, self.weapons[k].0);
                if ok {
                    bx -= 58.0;
                    if small(ui, &tr!("На {}", "On {}", k + 1), bx, ly + 4.0, 54.0) {
                        act = Some(ItemAction::AttachSight {
                            level,
                            slot: k as u8,
                        });
                    }
                }
            }
            ly += 42.0;
        }
        for kind in toon_shared::inventory::MODS {
            let n = inv.spare_mods[kind as usize - 1];
            if n == 0 {
                continue;
            }
            any = true;
            draw_rectangle(lx, ly, cw, 36.0, Color::new(0.0, 0.0, 0.0, 0.3));
            let name = toon_shared::inventory::mod_name(kind);
            let label = if n > 1 {
                format!("{name} ×{n}")
            } else {
                name.to_string()
            };
            ui.text(&label, lx + 12.0, ly + 24.0, 16, TEXT);
            let mut bx = lx + cw - 8.0 - 84.0;
            if small(ui, drop_txt, bx, ly + 4.0, 84.0) {
                act = Some(ItemAction::Drop(Item::Mod(kind), 1));
            }
            for k in (0..2usize).rev() {
                let ok = br.empty_slot != Some(k as u8)
                    && toon_shared::inventory::mod_fits(kind, self.weapons[k].0);
                if ok {
                    bx -= 58.0;
                    if small(ui, &tr!("На {}", "On {}", k + 1), bx, ly + 4.0, 54.0) {
                        act = Some(ItemAction::AttachMod {
                            kind,
                            slot: k as u8,
                        });
                    }
                }
            }
            ly += 42.0;
        }
        if !any {
            ui.text(t("немає", "none"), lx + 12.0, ly + 22.0, 15, MUTED);
        }

        // ---- right: gear, medkits and rounds
        let rx = x + pw / 2.0 + 12.0;
        let mut ry = y + 80.0;
        ui.bold(t("Спорядження", "Gear"), rx, ry, 18, TEXT);
        ry += 12.0;
        let row = |ui: &mut Ui, label: &str, on: bool, ry: f32| {
            draw_rectangle(rx, ry, cw, 34.0, Color::new(0.0, 0.0, 0.0, 0.3));
            ui.text(
                label,
                rx + 12.0,
                ry + 23.0,
                16,
                if on { TEXT } else { MUTED },
            );
        };
        let gear: [(String, bool, Item); 3] = [
            (
                if inv.armor > 0 {
                    format!(
                        "{} {} ({:.0}%)",
                        t("Бронежилет", "Vest"),
                        level_name(inv.armor),
                        inv.armor_hp / armor_durability(inv.armor) * 100.0
                    )
                } else {
                    t("Бронежилет: немає", "Vest: none").into()
                },
                inv.armor > 0,
                Item::Armor,
            ),
            (
                if inv.helmet > 0 {
                    format!(
                        "{} {} ({:.0}%)",
                        t("Шолом", "Helmet"),
                        level_name(inv.helmet),
                        inv.helmet_hp / helmet_durability(inv.helmet) * 100.0
                    )
                } else {
                    t("Шолом: немає", "Helmet: none").into()
                },
                inv.helmet > 0,
                Item::Helmet,
            ),
            (
                if inv.backpack > 0 {
                    format!("{} {}", t("Рюкзак", "Backpack"), level_name(inv.backpack))
                } else {
                    t("Рюкзак: немає", "Backpack: none").into()
                },
                inv.backpack > 0,
                Item::Backpack,
            ),
        ];
        for (label, on, item) in gear {
            row(ui, &label, on, ry);
            if on && small(ui, drop_txt, rx + cw - 92.0, ry + 3.0, 84.0) {
                act = Some(ItemAction::Drop(item, 1));
            }
            ry += 40.0;
        }
        row(
            ui,
            &format!(
                "{}: {}/{}",
                t("Аптечки", "Medkits"),
                inv.medkits,
                inv.medkit_cap()
            ),
            inv.medkits > 0,
            ry,
        );
        if inv.medkits > 0 && small(ui, "−1", rx + cw - 92.0, ry + 3.0, 84.0) {
            act = Some(ItemAction::Drop(Item::Medkit, 1));
        }
        for (label, n, item) in [
            (t("Бинти", "Bandages"), inv.bandages, Item::Bandages),
            (t("Енергетики", "Energy drinks"), inv.drinks, Item::Drinks),
            (
                t("Коктейлі Молотова", "Molotovs"),
                inv.molotovs,
                Item::Molotovs,
            ),
            (t("Світлошумові", "Flashbangs"), inv.flashes, Item::Flashes),
        ] {
            if n == 0 {
                continue;
            }
            ry += 40.0;
            row(ui, &format!("{label}: {n}"), true, ry);
            if small(ui, "−1", rx + cw - 92.0, ry + 3.0, 84.0) {
                act = Some(ItemAction::Drop(item, 1));
            }
        }
        ry += 52.0;
        ui.bold(t("Набої", "Rounds"), rx, ry, 18, TEXT);
        ry += 12.0;
        let mut shown = false;
        for a in AmmoType::ALL {
            let n = inv.ammo(a);
            if n == 0 {
                continue;
            }
            shown = true;
            row(
                ui,
                &format!("{}: {n}/{}", a.name(), inv.ammo_cap(a)),
                true,
                ry,
            );
            if small(ui, t("Усе", "All"), rx + cw - 66.0, ry + 3.0, 58.0) {
                act = Some(ItemAction::Drop(Item::Ammo(a), n));
            }
            let part = a.box_size().min(n);
            if small(ui, &format!("−{part}"), rx + cw - 130.0, ry + 3.0, 58.0) {
                act = Some(ItemAction::Drop(Item::Ammo(a), part));
            }
            ry += 40.0;
        }
        if !shown {
            ui.text(t("немає", "none"), rx + 12.0, ry + 22.0, 15, MUTED);
        }
        if let Some(a) = act {
            self.net.send(toon_shared::protocol::C2S::Item(a));
        }
    }

    /// A death box: what is in it, a click takes a thing, a button takes all the supplies.
    fn draw_box_screen(&mut self, ui: &mut Ui) {
        use toon_shared::protocol::BoxCmd;
        let close = |g: &mut Self| {
            g.overlay = Overlay::None;
            g.net.send(toon_shared::protocol::C2S::Box(BoxCmd::Close));
        };
        let again = crate::binds::pressed(Action::Use) && self.now > self.inventory_opened;
        if is_key_pressed(KeyCode::Escape) || again || !self.alive {
            close(self);
            return;
        }
        let view = self.br.as_ref().and_then(|b| b.open_box.clone());
        let Some(v) = view else {
            // (the server has not answered yet, or we walked away)
            if self.now - self.inventory_opened > 0.6 {
                close(self);
            }
            return;
        };
        let (w, h) = (screen_width(), screen_height());
        draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
        let rows = v.items.len() + v.guns.len();
        let pw = 460.0f32.min(w - 20.0);
        let ph = (130.0 + rows as f32 * 38.0).min(h - 20.0);
        let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
        ui.panel(x, y, pw, ph);
        ui.bold(
            &tr!("Речі: {}", "Belongings: {}", v.name),
            x + 24.0,
            y + 40.0,
            22,
            ACCENT,
        );
        ui.text(
            t("Клацни, щоб узяти", "Click to take"),
            x + 24.0,
            y + 62.0,
            14,
            MUTED,
        );
        let mut act = None;
        let mut ry = y + 76.0;
        for (k, (kind, n)) in v.items.iter().enumerate() {
            let label = match kind {
                LootKind::Ammo(_) | LootKind::Medkit => format!("{} ×{n}", kind.name()),
                LootKind::Armor(_) | LootKind::Helmet(_) => format!("{} ({n}%)", kind.name()),
                _ => kind.name(),
            };
            if ui.button2(&label, x + 24.0, ry, pw - 48.0, 32.0) {
                act = Some(BoxCmd::Take(v.id, k as u8));
            }
            ry += 38.0;
        }
        for (k, (gun, mag)) in v.guns.iter().enumerate() {
            let label = format!("{} · {mag}", gun.def().name);
            if ui.button2(&label, x + 24.0, ry, pw - 48.0, 32.0) {
                act = Some(BoxCmd::Take(v.id, (v.items.len() + k) as u8));
            }
            ry += 38.0;
        }
        if !v.items.is_empty()
            && ui.button(
                t("Взяти все", "Take all"),
                x + pw - 184.0,
                y + ph - 46.0,
                160.0,
                36.0,
            )
        {
            act = Some(BoxCmd::TakeAll(v.id));
        }
        ui.text(
            &tr!("{} / Esc — закрити", "{} / Esc — close", key(Action::Use)),
            x + 24.0,
            y + ph - 22.0,
            14,
            MUTED,
        );
        if let Some(a) = act {
            self.net.send(toon_shared::protocol::C2S::Box(a));
        }
    }

    /// Battle royale prompts: leaving the plane, the parachute, being outside the zone.
    fn draw_br_alerts(&self, ui: &Ui) {
        let Some(br) = &self.br else { return };
        let (w, h) = (screen_width(), screen_height());
        if let Some(rz) = br.red.filter(|_| self.alive && !br.in_plane) {
            let me = self.body.pos;
            let inside = vec2(me.x - rz[0], me.z - rz[1]).length() < rz[2];
            let text = match (inside, rz[3] > 0.0) {
                (true, true) => tr!(
                    "ЧЕРВОНА ЗОНА: обстріл через {} с — в укриття!",
                    "RED ZONE: shelling in {} s — take cover!",
                    rz[3].ceil() as i32
                ),
                (true, false) => t("ОБСТРІЛ! Сховайся в будинку", "SHELLING! Get indoors").into(),
                (false, true) => tr!(
                    "Червона зона позначена (обстріл через {} с)",
                    "Red zone marked (shelling in {} s)",
                    rz[3].ceil() as i32
                ),
                (false, false) => t("Червона зона: обстріл", "Red zone: shelling").into(),
            };
            let c = if inside {
                BAD
            } else {
                Color::new(1.0, 0.6, 0.5, 0.9)
            };
            ui.centered(&text, w / 2.0, h * 0.16, if inside { 22 } else { 16 }, c);
        }
        if br.in_plane {
            ui.centered(
                &tr!(
                    "{} — стрибнути з літака",
                    "{} — jump out of the plane",
                    key(Action::Jump)
                ),
                w / 2.0,
                h * 0.8,
                26,
                ACCENT,
            );
            ui.centered(
                &tr!("{} — карта", "{} — map", key(Action::Class)),
                w / 2.0,
                h * 0.8 + 30.0,
                18,
                MUTED,
            );
            return;
        }
        if !self.alive {
            return;
        }
        if self.body.chute != toon_shared::physics::Chute::None {
            let ground = self
                .world
                .floor_below(self.body.pos, 400.0)
                .into_iter()
                .chain(self.world.ground(self.body.pos.x, self.body.pos.z))
                .fold(f32::MIN, f32::max);
            let alt = (self.body.pos.y - ground).max(0.0);
            let text = if self.body.chute == toon_shared::physics::Chute::FreeFall {
                tr!(
                    "Висота {alt:.0} м — парашут розкриється сам ({} — раніше)",
                    "Altitude {alt:.0} m — the parachute opens by itself ({} — sooner)",
                    key(Action::Jump)
                )
            } else {
                tr!("Висота {alt:.0} м", "Altitude {alt:.0} m")
            };
            ui.centered(&text, w / 2.0, h * 0.8, 20, TEXT);
        }
        let (c, r) = (vec2(br.zone[0], br.zone[1]), br.zone[2]);
        let me = vec2(self.body.pos.x, self.body.pos.z);
        let out = me.distance(c) - r;
        if self.state == MatchState::Playing && r > 0.0 && out > 0.0 {
            let pulse = 0.6 + 0.4 * ((self.now * 6.0).sin() as f32).abs();
            ui.centered(
                &tr!(
                    "ТИ ПОЗА ЗОНОЮ! −{:.0} HP/с · до зони {:.0} м",
                    "OUTSIDE THE ZONE! −{:.0} HP/s · {:.0} m to safety",
                    br.dps,
                    out
                ),
                w / 2.0,
                110.0,
                24,
                Color::new(1.0, 0.35, 0.3, pulse),
            );
            // an arrow towards the zone
            let to = c - me;
            let rel = wrap_angle(yaw_of(vec3(to.x, 0.0, to.y)) - self.yaw);
            let d = vec2(-rel.sin(), -rel.cos());
            let p = vec2(w / 2.0, 150.0) + d * 22.0;
            let perp = vec2(-d.y, d.x);
            draw_triangle(p + d * 16.0, p + perp * 12.0, p - perp * 12.0, BAD);
        }
    }

    /// Battle royale: the whole island, north up, with the zone and the plane's course.
    pub fn draw_big_map(&self, ui: &Ui) {
        let (Some(br), Some(img)) = (&self.br, &self.level.image) else {
            return;
        };
        let (w, h) = (screen_width(), screen_height());
        let (x0, y0, side) = big_map_rect();
        draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
        draw_texture_ex(
            &img.tex,
            x0,
            y0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(side, side)),
                ..Default::default()
            },
        );
        let k = side / img.size;
        // circles and the plane's course stay inside the map
        let gl = unsafe { get_internal_gl() }.quad_gl;
        let dpi = screen_dpi_scale();
        gl.scissor(Some((
            (x0 * dpi) as i32,
            (y0 * dpi) as i32,
            (side * dpi) as i32,
            (side * dpi) as i32,
        )));
        let to_map =
            |x: f32, z: f32| vec2(x0 + (x - img.origin.x) * k, y0 + (z - img.origin.y) * k);
        if br.zone[2] > 0.0 {
            let c = to_map(br.zone[0], br.zone[1]);
            draw_circle_lines(
                c.x,
                c.y,
                br.zone[2] * k,
                3.0,
                Color::new(0.3, 0.55, 1.0, 1.0),
            );
            let n = to_map(br.next[0], br.next[1]);
            draw_circle_lines(n.x, n.y, br.next[2] * k, 2.0, WHITE);
        }
        if let Some((p, yaw)) = br.plane {
            let d = look_dir(yaw, 0.0);
            let a = to_map(p[0] - d.x * 1500.0, p[2] - d.z * 1500.0);
            let b = to_map(p[0] + d.x * 1500.0, p[2] + d.z * 1500.0);
            draw_line(a.x, a.y, b.x, b.y, 2.0, Color::new(1.0, 1.0, 1.0, 0.5));
            let pp = to_map(p[0], p[2]);
            draw_circle(pp.x, pp.y, 6.0, ACCENT);
        }
        // me
        let pos = if br.in_plane {
            br.plane
                .map(|p| Vec3::from_array(p.0))
                .unwrap_or(self.body.pos)
        } else {
            self.body.pos
        };
        let m = to_map(pos.x, pos.z);
        let d = look_dir(self.yaw, 0.0);
        let f = vec2(d.x, d.z);
        let perp = vec2(-f.y, f.x);
        draw_triangle(
            m + f * 12.0,
            m - f * 7.0 + perp * 7.0,
            m - f * 7.0 - perp * 7.0,
            ACCENT,
        );
        draw_triangle_lines(
            m + f * 12.0,
            m - f * 7.0 + perp * 7.0,
            m - f * 7.0 - perp * 7.0,
            2.0,
            BLACK,
        );
        if let Some(rz) = br.red {
            let c = to_map(rz[0], rz[1]);
            draw_circle(c.x, c.y, rz[2] * k, Color::new(0.9, 0.1, 0.1, 0.25));
            draw_circle_lines(c.x, c.y, rz[2] * k, 2.0, Color::new(0.9, 0.15, 0.1, 0.9));
        }
        for d in &br.drops {
            let p = to_map(d[0], d[2]);
            draw_crate_icon(p, 1.0);
        }
        if let Some(mk) = self.marker {
            let p = to_map(mk.x, mk.y);
            draw_marker_pin(p, 1.0);
        }
        for m in br.squad.iter().filter(|m| m.id != self.my_id) {
            if let Some(mk) = m.marker {
                let p = to_map(mk[0], mk[1]);
                draw_circle(p.x, p.y - 12.0, 7.0, self.mate_color(m.id));
                draw_circle_lines(p.x, p.y - 12.0, 7.0, 2.0, BLACK);
            }
            if m.alive {
                let p = to_map(m.pos[0], m.pos[2]);
                draw_circle(p.x, p.y, 6.0, BLACK);
                draw_circle(
                    p.x,
                    p.y,
                    4.5,
                    if m.downed { BAD } else { self.mate_color(m.id) },
                );
            }
        }
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.scissor(None);
        draw_rectangle_lines(x0, y0, side, side, 3.0, BLACK);
        ui.centered(
            &tr!(
                "{} — закрити карту · ЛКМ — поставити мітку, ПКМ — прибрати",
                "{} — close the map · LMB — put a marker, RMB — remove it",
                key(Action::Class)
            ),
            w / 2.0,
            y0 + side + 24.0,
            16,
            TEXT,
        );
    }

    fn draw_score_panel(&self, ui: &Ui) {
        if self.mode == Mode::Br {
            self.draw_br_panel(ui);
            return;
        }
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
            Mode::Tdm | Mode::Br => self.gs.score_limit,
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
            MatchState::Warmup => t("Розминка", "Warmup").to_string(),
            _ if self.overtime => t("Овертайм", "Overtime").to_string(),
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
        let ih = 16.0;
        for k in self.killfeed.iter().rev() {
            let age = (self.now - k.time) as f32;
            if age > 7.0 {
                continue;
            }
            let a = (1.0 - (age - 6.0).max(0.0)).clamp(0.0, 1.0);
            let suicide = k.killer.is_empty() || k.killer == k.victim;
            let icon_w = cause_icon_width(k.cause, ih) + if k.headshot { ih + 6.0 } else { 0.0 };
            let kw = if suicide {
                0.0
            } else {
                ui.width(&k.killer, 16, true)
            };
            let vw = ui.width(&k.victim, 16, true);
            let gap = 10.0;
            // (how far: worth showing once it is a real shot)
            let dist = (!suicide && k.dist >= 10).then(|| tr!("{} м", "{} m", k.dist));
            let dw = dist.as_ref().map_or(0.0, |d| ui.width(d, 13, false) + gap);
            let total = kw + vw + icon_w + dw + gap * if suicide { 1.0 } else { 2.0 };
            let mut x = w - 20.0 - total;
            draw_rectangle(
                x - 6.0,
                y - 17.0,
                total + 12.0,
                23.0,
                Color::new(0.0, 0.0, 0.0, 0.35 * a),
            );
            if !suicide {
                ui.shadowed(
                    &k.killer,
                    x,
                    y,
                    16,
                    with_alpha(self.feed_color(k.kteam, k.kme), a),
                );
                x += kw + gap;
            }
            let iw = draw_cause_icon(k.cause, x, y - 6.0, ih, a);
            x += iw;
            if k.headshot {
                draw_headshot_icon(x + 6.0 + ih * 0.5, y - 6.0, ih, a);
                x += ih + 6.0;
            }
            x += gap;
            ui.shadowed(
                &k.victim,
                x,
                y,
                16,
                with_alpha(self.feed_color(k.vteam, k.vme), a),
            );
            if let Some(d) = &dist {
                ui.shadowed(d, x + vw + gap, y - 1.0, 13, with_alpha(MUTED, a));
            }
            y += 26.0;
        }
    }

    /// Name colour in the kill feed: team colours, or in the battle royale (no teams) us in
    /// yellow and everybody else in red.
    fn feed_color(&self, team: Team, me: bool) -> Color {
        if !self.mode.solo() {
            team_color(team)
        } else if me {
            ACCENT
        } else {
            BAD
        }
    }

    /// Killcam overlay: letterbox, title, who killed whom and with what.
    fn draw_killcam(&self, ui: &Ui) {
        let Some(k) = &self.killcam else { return };
        let (w, h) = (screen_width(), screen_height());
        let (cx, cy) = (w / 2.0, h / 2.0);
        if k.scoped() {
            self.draw_scope();
        }
        draw_hitmarker(cx, cy, (k.pt - k.hitmarker) as f32, k.hit_kill, false);
        let bar = (h * 0.09).max(48.0);
        let black = Color::new(0.0, 0.0, 0.0, 0.85);
        draw_rectangle(0.0, 0.0, w, bar, black);
        draw_rectangle(0.0, h - bar, w, bar, black);
        let title = if k.final_cam {
            t("ФІНАЛЬНИЙ КІЛКАМ", "FINAL KILLCAM")
        } else {
            t("КІЛКАМ", "KILLCAM")
        };
        // a slow blink on the record dot
        if ((self.now * 2.0) as i64) % 2 == 0 {
            draw_circle(
                cx - ui.width(title, 30, true) / 2.0 - 18.0,
                bar * 0.5 - 2.0,
                7.0,
                BAD,
            );
        }
        ui.centered(title, cx, bar * 0.5 + 10.0, 30, ACCENT);

        // killer · weapon · victim
        let info = &k.info;
        let ih = 20.0;
        let kw = ui.width(&info.killer_name, 22, true);
        let vw = ui.width(&info.victim_name, 22, true);
        let iw = cause_icon_width(info.cause, ih) + if info.headshot { ih + 8.0 } else { 0.0 };
        let total = kw + vw + iw + 40.0;
        let mut x = cx - total / 2.0;
        let y = h - bar * 0.5 + 8.0;
        ui.shadowed(&info.killer_name, x, y, 22, team_color(info.killer_team));
        x += kw + 20.0;
        x += draw_cause_icon(info.cause, x, y - 8.0, ih, 1.0);
        if info.headshot {
            draw_headshot_icon(x + 8.0 + ih * 0.5, y - 8.0, ih, 1.0);
            x += ih + 8.0;
        }
        x += 20.0;
        let vteam = self
            .scoreboard
            .iter()
            .find(|r| r.id == info.victim)
            .map(|r| r.team)
            .unwrap_or(Team::Spectator);
        ui.shadowed(&info.victim_name, x, y, 22, team_color(vteam));
        if !k.final_cam {
            let hint = if self.me.respawn_in > 0.0 {
                tr!(
                    "{} або Esc — пропустити · відродження через {:.0}",
                    "{} or Esc — skip · respawn in {:.0}",
                    key(Action::Fire),
                    self.me.respawn_in.ceil()
                )
            } else {
                tr!(
                    "{} або Esc — пропустити",
                    "{} or Esc — skip",
                    key(Action::Fire)
                )
            };
            ui.text(
                &hint,
                w - ui.width(&hint, 15, false) - 20.0,
                h - 12.0,
                15,
                MUTED,
            );
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
                t("Команді: ", "Team: ")
            } else {
                t("Всім: ", "All: ")
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

    /// The bomb you carry, shown next to the weapon panel (pulses so it is hard to miss).
    fn draw_bomb_badge(&self, ui: &Ui) {
        let carrying = self.alive
            && self
                .bomb
                .as_ref()
                .is_some_and(|b| b.carrier == Some(self.my_id) && b.planted_at.is_none());
        if !carrying {
            return;
        }
        let (w, h) = (screen_width(), screen_height());
        let (s, x, y) = (84.0, w - 310.0 - 84.0 - 12.0, h - 130.0);
        let pulse = 0.5 + 0.5 * (self.now * 4.0).sin() as f32;
        draw_rectangle(x, y, s, s + 22.0, Color::new(0.0, 0.0, 0.0, 0.45));
        draw_rectangle_lines(
            x,
            y,
            s,
            s + 22.0,
            2.0 + pulse * 2.0,
            Color::new(1.0, 0.35 + pulse * 0.3, 0.25, 0.9),
        );
        // three red sticks taped together, a timer and a wire
        let (bx, by) = (x + 14.0, y + 18.0);
        for k in 0..3 {
            let sx = bx + k as f32 * 19.0;
            draw_rectangle(sx, by, 16.0, 46.0, Color::from_rgba(214, 60, 48, 255));
            draw_rectangle_lines(sx, by, 16.0, 46.0, 2.0, BLACK);
        }
        draw_rectangle(
            bx - 2.0,
            by + 17.0,
            59.0,
            9.0,
            Color::from_rgba(60, 60, 66, 255),
        );
        draw_rectangle(
            bx + 14.0,
            by + 6.0,
            27.0,
            16.0,
            Color::from_rgba(30, 32, 36, 255),
        );
        draw_rectangle_lines(bx + 14.0, by + 6.0, 27.0, 16.0, 2.0, BLACK);
        let led = if pulse > 0.5 {
            Color::from_rgba(255, 70, 50, 255)
        } else {
            Color::from_rgba(120, 30, 25, 255)
        };
        draw_circle(bx + 27.5, by + 14.0, 3.5, led);
        draw_line(bx + 41.0, by + 10.0, bx + 54.0, by - 6.0, 2.0, ACCENT);
        let label = t("БОМБА", "BOMB");
        let lw = ui.width(label, 16, true);
        ui.shadowed(label, x + (s - lw) / 2.0, y + s + 14.0, 16, ACCENT);
    }

    fn draw_weapon_panel(&self, ui: &Ui) {
        self.draw_bomb_badge(ui);
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
        let mag_max = self.mag_cap();
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
        // smoke grenades: grey cans after the frags
        for i in 0..self.smokes {
            let sx = x + 200.0 + (self.grenades + i) as f32 * 22.0;
            draw_rectangle(
                sx - 6.0,
                y + 21.0,
                12.0,
                18.0,
                Color::from_rgba(150, 156, 162, 255),
            );
            draw_rectangle(sx - 6.0, y + 28.0, 12.0, 4.0, WHITE);
            draw_rectangle_lines(sx - 6.0, y + 21.0, 12.0, 18.0, 2.0, BLACK);
        }
        if self.smoke_held {
            ui.centered(
                &tr!(
                    "Димова: відпусти {}, щоб кинути",
                    "Smoke: release {} to throw",
                    key(Action::Smoke)
                ),
                w / 2.0,
                h * 0.62,
                20,
                TEXT,
            );
        }
        if let Some(start) = self.cooking {
            let left = (self.tuning.grenade.fuse as f64 - (self.now - start)).max(0.0);
            ui.centered(
                &tr!("Граната: {left:.1}", "Grenade: {left:.1}"),
                w / 2.0,
                h * 0.62,
                22,
                BAD,
            );
        }
        // killstreak
        let (next, left) = Killstreak::next_after(self.me.streak as u32, &self.tuning.streaks);
        let streak_txt = tr!(
            "Серія: {} (до «{}» {})",
            "Streak: {} ({} in {})",
            self.me.streak,
            next.name(),
            left
        );
        // (no killstreaks in the battle royale)
        if self.mode != Mode::Br {
            ui.shadowed(&streak_txt, x, y + 92.0, 13, MUTED);
        }
        if let Some(r) = self.me.rewards.last() {
            let t = format!("[{}] {}", key(Action::Killstreak), r.name());
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
            ui.centered(t("Перезарядка…", "Reloading…"), w / 2.0, h * 0.58, 18, TEXT);
        } else if mag == 0 && res == 0 {
            ui.centered(
                &tr!(
                    "Немає набоїв — {}/{} змінити зброю",
                    "Out of ammo — {}/{} to switch weapons",
                    key(Action::Primary),
                    key(Action::Secondary)
                ),
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
            &format!("{} · {}", self.map.id.name(), self.mode.name()),
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
            ui.text(
                t("Очки   В   С   Д   Пінг", "Score  K   D   A   Ping"),
                x + col_w - 210.0,
                y,
                14,
                MUTED,
            );
            let mut rows: Vec<_> = self.scoreboard.iter().filter(|r| r.team == *team).collect();
            rows.sort_by_key(|r| std::cmp::Reverse(r.score));
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
                    t("БОТ", "BOT").to_string()
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
                &tr!("Глядачі: {}", "Spectators: {}", specs.join(", ")),
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
                ui.bold(
                    t("Обери команду", "Choose a team"),
                    x + 24.0,
                    y + 44.0,
                    28,
                    ACCENT,
                );
                ui.text(
                    &format!(
                        "{} · {} · {}",
                        self.server_name,
                        self.map.id.name(),
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
                if ui.button(
                    t("Автовибір", "Auto-assign"),
                    x + 24.0,
                    y + 220.0,
                    (pw - 60.0) / 2.0,
                    46.0,
                ) {
                    self.join_team(None);
                }
                if ui.button2(
                    t("Глядач", "Spectator"),
                    x + 36.0 + (pw - 60.0) / 2.0,
                    y + 220.0,
                    (pw - 60.0) / 2.0,
                    46.0,
                ) {
                    self.join_team(Some(Team::Spectator));
                }
                if ui.button2(
                    t("Відключитися", "Disconnect"),
                    x + 24.0,
                    y + 290.0,
                    pw - 48.0,
                    44.0,
                ) {
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
                ui.bold(
                    t("Обери клас", "Choose a class"),
                    x + 24.0,
                    y + 44.0,
                    28,
                    ACCENT,
                );
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
                        tr!("Основна: {}", "Primary: {}", l.primary.def().name),
                        l.primary.def().class.name().to_string(),
                    ),
                    (
                        tr!("Додаткова: {}", "Secondary: {}", l.secondary.def().name),
                        l.secondary.def().class.name().to_string(),
                    ),
                    (
                        tr!("Перк 1: {}", "Perk 1: {}", l.perk1.name()),
                        l.perk1.desc().to_string(),
                    ),
                    (
                        tr!("Перк 2: {}", "Perk 2: {}", l.perk2.name()),
                        l.perk2.desc().to_string(),
                    ),
                    (
                        tr!("Перк 3: {}", "Perk 3: {}", l.perk3.name()),
                        l.perk3.desc().to_string(),
                    ),
                ];
                for (k, (a, b)) in lines.iter().enumerate() {
                    ui.bold(a, dx, y + 140.0 + k as f32 * 50.0, 18, TEXT);
                    ui.text(b, dx, y + 160.0 + k as f32 * 50.0, 15, MUTED);
                }
                let d = self.tuning.weapon(l.primary).clone();
                let stats = [
                    (t("Урон", "Damage"), d.damage_max / 100.0),
                    (t("Скорострільність", "Fire rate"), d.rpm / 1000.0),
                    (t("Дальність", "Range"), d.range_far / 100.0),
                    (
                        t("Контроль віддачі", "Recoil control"),
                        1.0 - (d.recoil_v * d.recoil_max_ramp / 6.0),
                    ),
                    (t("Мобільність", "Mobility"), (d.move_mult - 0.6) / 0.45),
                ];
                ui.text(
                    &tr!(
                        "Вага {:.1} кг · приціл {:.2} с · магазин {}",
                        "Weight {:.1} kg · aim {:.2} s · magazine {}",
                        d.weight,
                        d.ads_time,
                        d.mag
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
                if ui.button(
                    t("В БІЙ", "FIGHT"),
                    x + pw - 200.0,
                    y + ph - 64.0,
                    176.0,
                    46.0,
                ) {
                    let i = self.class_choice.min(classes.len() - 1);
                    settings.last_class = i;
                    settings.save();
                    self.choose_class(classes[i].1);
                }
                if self.class_choice >= 5
                    && ui.button2(
                        t("Редагувати", "Edit"),
                        x + pw - 390.0,
                        y + ph - 64.0,
                        176.0,
                        46.0,
                    )
                {
                    self.overlay = Overlay::Edit(self.class_choice - 5);
                }
                if ui.button2(t("Команда", "Team"), x + 24.0, y + ph - 64.0, 140.0, 46.0) {
                    self.overlay = Overlay::Team;
                }
                ui.text(
                    t(
                        "Клацни клас двічі або «В БІЙ»",
                        "Double-click a class or press FIGHT",
                    ),
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
                ui.bold(
                    t("Редактор класу", "Class editor"),
                    x + 24.0,
                    y + 44.0,
                    26,
                    ACCENT,
                );
                let c = &mut settings.custom[i];
                ui.field(
                    100 + i as u32,
                    t("Назва", "Name"),
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
                    t("Основна зброя", "Primary weapon"),
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
                    t("Додаткова зброя", "Secondary weapon"),
                    c.loadout.secondary.def().name,
                    x + 24.0,
                    y + 220.0,
                    fw,
                );
                if d != 0 {
                    c.loadout.secondary = cyc(&sec, c.loadout.secondary, d);
                }
                let d = ui.stepper(
                    t("Перк 1", "Perk 1"),
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
                    t("Перк 2", "Perk 2"),
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
                    t("Перк 3", "Perk 3"),
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
                if ui.button(
                    t("Зберегти", "Save"),
                    x + pw - 200.0,
                    y + ph - 60.0,
                    176.0,
                    44.0,
                ) {
                    settings.save();
                    self.overlay = Overlay::Class;
                }
                None
            }
            Overlay::Pause => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
                let pw = 420.0;
                let (x, y) = ((w - pw) / 2.0, h * 0.22);
                ui.panel(x, y, pw, 460.0);
                ui.bold(t("Пауза", "Pause"), x + 24.0, y + 44.0, 28, ACCENT);
                ui.text(
                    t(
                        "(гра на сервері триває)",
                        "(the game on the server goes on)",
                    ),
                    x + 24.0,
                    y + 70.0,
                    14,
                    MUTED,
                );
                let bw = pw - 48.0;
                if ui.button(t("Продовжити", "Resume"), x + 24.0, y + 90.0, bw, 46.0) {
                    self.overlay = self.after_pause();
                }
                // (no classes or sides in the battle royale: characters instead)
                if self.mode == Mode::Br {
                    if ui.button2(t("Персонаж", "Character"), x + 24.0, y + 146.0, bw, 46.0)
                    {
                        self.overlay = Overlay::Skin;
                    }
                } else if ui.button2(
                    t("Змінити клас", "Change class"),
                    x + 24.0,
                    y + 146.0,
                    bw,
                    46.0,
                ) {
                    self.overlay = Overlay::Class;
                }
                if self.mode != Mode::Br
                    && ui.button2(
                        t("Змінити команду", "Change team"),
                        x + 24.0,
                        y + 202.0,
                        bw,
                        46.0,
                    )
                {
                    self.overlay = Overlay::Team;
                }
                if ui.button2(t("Налаштування", "Settings"), x + 24.0, y + 258.0, bw, 46.0)
                {
                    self.overlay = Overlay::Settings;
                }
                // before the match: ready or taking a break
                if let Some((_, _, me)) = self.ready {
                    let label = if me {
                        t("Не готовий (перерва)", "Not ready (a break)")
                    } else {
                        t("Готовий!", "Ready!")
                    };
                    let hit = if me {
                        ui.button2(label, x + 24.0, y + 314.0, bw, 46.0)
                    } else {
                        ui.button(label, x + 24.0, y + 314.0, bw, 46.0)
                    };
                    if hit {
                        self.net.send(toon_shared::protocol::C2S::Ready(!me));
                    }
                }
                if ui.button_colored(
                    t("Відключитися", "Disconnect"),
                    x + 24.0,
                    y + 392.0,
                    bw,
                    46.0,
                    BAD,
                ) {
                    return Some(HudAction::Leave);
                }
                None
            }
            Overlay::RobotMode => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
                let pw = 460.0;
                let (x, y) = ((w - pw) / 2.0, h * 0.28);
                ui.panel(x, y, pw, 250.0);
                ui.bold(
                    t("НРК — наземний робот", "Ground robot"),
                    x + 24.0,
                    y + 44.0,
                    26,
                    ACCENT,
                );
                ui.text(
                    t(
                        "Кулемет, броня, 60 секунд. Хто ним керує?",
                        "Machine gun, armour, 60 seconds. Who drives it?",
                    ),
                    x + 24.0,
                    y + 72.0,
                    15,
                    MUTED,
                );
                let bw = pw - 48.0;
                if ui.button(
                    t("Керувати самому", "Drive it myself"),
                    x + 24.0,
                    y + 96.0,
                    bw,
                    50.0,
                ) {
                    self.call_robot(true);
                }
                if ui.button2(
                    t("Автономний (воює сам)", "Autonomous (fights on its own)"),
                    x + 24.0,
                    y + 156.0,
                    bw,
                    50.0,
                ) {
                    self.call_robot(false);
                }
                ui.text(
                    t("Esc — скасувати", "Esc — cancel"),
                    x + 24.0,
                    y + 232.0,
                    14,
                    MUTED,
                );
                None
            }
            Overlay::Settings => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
                let pw = 560.0f32.min(w - 20.0);
                let ph = crate::menu::SETTINGS_H + 110.0;
                let (x, y) = ((w - pw) / 2.0, ((h - ph) / 2.0).max(0.0));
                ui.panel(x, y, pw, ph);
                ui.bold(
                    t("Налаштування", "Settings"),
                    x + 24.0,
                    y + 44.0,
                    26,
                    ACCENT,
                );
                crate::menu::settings_widgets(ui, settings, x + 24.0, y + 90.0, pw - 48.0);
                let by = y + ph - 70.0;
                if ui.button2(t("Відео", "Video"), x + 24.0, by, 130.0, 44.0) {
                    self.overlay = Overlay::Video;
                }
                if ui.button2(t("Клавіші", "Controls"), x + 162.0, by, 130.0, 44.0) {
                    self.overlay = Overlay::Controls;
                }
                if ui.button(t("Готово", "Done"), x + pw - 160.0, by, 136.0, 44.0) {
                    settings.save();
                    self.overlay = Overlay::Pause;
                }
                None
            }
            Overlay::Video => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
                let pw = 600.0f32.min(w - 20.0);
                let (x, y) = ((w - pw) / 2.0, (h - crate::menu::VIDEO_H).max(0.0) / 2.0);
                if crate::menu::video_page(ui, settings, x, y, pw) {
                    self.overlay = Overlay::Settings;
                }
                None
            }
            Overlay::Inventory => {
                self.draw_inventory_screen(ui);
                None
            }
            Overlay::DeathBox => {
                self.draw_box_screen(ui);
                None
            }
            Overlay::Skin => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
                let pw = 440.0f32.min(w - 20.0);
                let ph = 520.0f32.min(h - 20.0);
                let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
                ui.panel(x, y, pw, ph);
                ui.bold(
                    t("Твій персонаж", "Your character"),
                    x + 24.0,
                    y + 44.0,
                    26,
                    ACCENT,
                );
                ui.text(
                    t(
                        "Так тебе бачитимуть у королівській битві",
                        "How others see you in the battle royale",
                    ),
                    x + 24.0,
                    y + 70.0,
                    14,
                    MUTED,
                );
                // the turning model is drawn into this box after the HUD
                let bx = Rect::new(x + 24.0, y + 84.0, pw - 48.0, ph - 230.0);
                draw_rectangle(bx.x, bx.y, bx.w, bx.h, Color::new(0.1, 0.12, 0.18, 1.0));
                self.skin_preview = Some(bx);
                let n = toon_shared::skins::count();
                let d = ui.stepper(
                    "",
                    toon_shared::skins::name(settings.skin),
                    x + 24.0,
                    y + ph - 130.0,
                    pw - 48.0,
                );
                if d != 0 {
                    settings.skin = ((settings.skin as i32 + d).rem_euclid(n as i32)) as u8;
                }
                if is_key_pressed(KeyCode::Left) || is_key_pressed(KeyCode::A) {
                    settings.skin = (settings.skin + n - 1) % n;
                }
                if is_key_pressed(KeyCode::Right) || is_key_pressed(KeyCode::D) {
                    settings.skin = (settings.skin + 1) % n;
                }
                if ui.button(
                    t("Готово", "Done"),
                    x + pw - 200.0,
                    y + ph - 70.0,
                    176.0,
                    46.0,
                ) || is_key_pressed(KeyCode::Enter)
                {
                    settings.save();
                    self.overlay = Overlay::None;
                }
                None
            }
            Overlay::Controls => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
                let pw = 760.0f32.min(w - 20.0);
                let (x, y) = ((w - pw) / 2.0, (h - crate::menu::CONTROLS_H).max(0.0) / 2.0);
                if crate::menu::controls_page(ui, &mut self.binds_editor, settings, x, y, pw) {
                    self.overlay = Overlay::Settings;
                }
                None
            }
            Overlay::Airstrike => {
                draw_rectangle(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.6));
                let size = (h - 160.0).min(w - 80.0);
                let (x0, y0) = ((w - size) / 2.0, 90.0);
                let what = match self.me.rewards.last() {
                    Some(Killstreak::Artillery) => t("Артилерія", "Artillery"),
                    _ => t("Авіаудар", "Airstrike"),
                };
                ui.centered(
                    &tr!(
                        "{what}: клацни ціль на карті (Esc — скасувати)",
                        "{what}: click a target on the map (Esc — cancel)"
                    ),
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

/// Hitmarker: pops out on impact, red and bigger for kills, with a gold ring for headshots.
fn draw_hitmarker(cx: f32, cy: f32, age: f32, kill: bool, head: bool) {
    let life = if kill { 0.4 } else { 0.25 };
    if !(0.0..life).contains(&age) {
        return;
    }
    let a = 1.0 - age / life;
    let pop = 1.0 + 0.5 * (1.0 - (age / 0.07).min(1.0));
    let size = if kill { 1.35 } else { 1.0 } * pop;
    let c = if kill { BAD } else { WHITE };
    for (dx, dy) in [(1.0f32, 1.0f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        let (x0, y0) = (cx + dx * 6.0 * size, cy + dy * 6.0 * size);
        let (x1, y1) = (cx + dx * 14.0 * size, cy + dy * 14.0 * size);
        draw_line(
            x0 + 1.0,
            y0 + 1.0,
            x1 + 1.0,
            y1 + 1.0,
            3.5,
            Color::new(0.0, 0.0, 0.0, a * 0.5),
        );
        draw_line(
            x0,
            y0,
            x1,
            y1,
            if kill { 3.5 } else { 3.0 },
            with_alpha(c, a),
        );
    }
    if head {
        draw_circle_lines(cx, cy, 20.0 * size, 2.0, with_alpha(ACCENT, a));
    }
}

fn icon_color(a: f32) -> (Color, Color) {
    (
        Color::new(1.0, 1.0, 1.0, 0.95 * a),
        Color::new(0.0, 0.0, 0.0, 0.7 * a),
    )
}

/// Side view of a gun built from its 3D model: (x, y, w, h) rectangles, barrel to the right.
fn gun_rects(w: WeaponId, h: f32) -> (Vec<Rect>, f32) {
    let parts = crate::models::gun_parts(w);
    let (mut z0, mut z1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for p in &parts {
        z0 = z0.min(-(p.c.z + p.h.z));
        z1 = z1.max(-(p.c.z - p.h.z));
        y0 = y0.min(p.c.y - p.h.y);
        y1 = y1.max(p.c.y + p.h.y);
    }
    let s = (h / (y1 - y0)).min(64.0 / (z1 - z0));
    let rects = parts
        .iter()
        .map(|p| {
            Rect::new(
                (-(p.c.z + p.h.z) - z0) * s,
                (y1 - (p.c.y + p.h.y)) * s + (h - (y1 - y0) * s) * 0.5,
                (p.h.z * 2.0 * s).max(1.5),
                (p.h.y * 2.0 * s).max(1.5),
            )
        })
        .collect();
    (rects, (z1 - z0) * s)
}

fn cause_icon_width(c: Cause, h: f32) -> f32 {
    match c {
        Cause::Weapon(w) => gun_rects(w, h).1,
        Cause::Knife | Cause::Airstrike | Cause::Helicopter => h * 1.8,
        _ => h * 1.1,
    }
}

/// Draws a small white silhouette for a cause of death; `cy` is the icon's vertical centre.
/// Returns its width.
fn draw_cause_icon(c: Cause, x: f32, cy: f32, h: f32, a: f32) -> f32 {
    let (fg, bg) = icon_color(a);
    let top = cy - h * 0.5;
    match c {
        Cause::Weapon(w) => {
            let (rects, width) = gun_rects(w, h);
            for r in &rects {
                draw_rectangle(x + r.x - 1.0, top + r.y - 1.0, r.w + 2.0, r.h + 2.0, bg);
            }
            for r in &rects {
                draw_rectangle(x + r.x, top + r.y, r.w, r.h, fg);
            }
            width
        }
        Cause::Grenade => {
            let r = h * 0.36;
            let (gx, gy) = (x + h * 0.5, cy + h * 0.1);
            draw_circle(gx, gy, r + 1.2, bg);
            draw_circle(gx, gy, r, fg);
            draw_rectangle(gx - r * 0.3, gy - r - h * 0.18, r * 0.6, h * 0.2, fg);
            draw_circle_lines(gx + r * 0.6, gy - r - h * 0.05, h * 0.12, 1.5, fg);
            h * 1.1
        }
        Cause::Knife => {
            let w = h * 1.8;
            draw_rectangle(
                x - 1.0,
                cy - h * 0.18 - 1.0,
                w * 0.4 + 2.0,
                h * 0.36 + 2.0,
                bg,
            );
            draw_rectangle(x, cy - h * 0.18, w * 0.4, h * 0.36, fg);
            draw_rectangle(x + w * 0.38, cy - h * 0.3, w * 0.06, h * 0.6, fg);
            draw_triangle(
                vec2(x + w * 0.44, cy - h * 0.2),
                vec2(x + w * 0.44, cy + h * 0.15),
                vec2(x + w, cy - h * 0.12),
                fg,
            );
            w
        }
        Cause::Airstrike => {
            let w = h * 1.8;
            draw_triangle(
                vec2(x, cy - h * 0.4),
                vec2(x, cy + h * 0.4),
                vec2(x + w, cy),
                fg,
            );
            draw_rectangle(x + w * 0.1, cy - h * 0.08, w * 0.6, h * 0.16, bg);
            w
        }
        Cause::Helicopter => {
            let w = h * 1.8;
            draw_circle(x + w * 0.65, cy + h * 0.05, h * 0.3, fg);
            draw_rectangle(x, cy - h * 0.02, w * 0.6, h * 0.14, fg);
            draw_rectangle(x + w * 0.15, cy - h * 0.42, w * 0.85, h * 0.08, fg);
            draw_rectangle(x + w * 0.62, cy - h * 0.42, w * 0.05, h * 0.25, fg);
            w
        }
        Cause::Bomb => {
            draw_rectangle(x - 1.0, top + h * 0.25 - 1.0, h + 2.0, h * 0.65 + 2.0, bg);
            draw_rectangle(x, top + h * 0.25, h, h * 0.65, fg);
            draw_rectangle(x, top + h * 0.45, h, h * 0.12, bg);
            draw_line(x + h * 0.8, top + h * 0.25, x + h, top, 2.0, fg);
            h * 1.1
        }
        Cause::Artillery => {
            // a shell, nose to the right
            let w = h * 1.8;
            draw_rectangle(
                x - 1.0,
                cy - h * 0.22 - 1.0,
                w * 0.65 + 2.0,
                h * 0.44 + 2.0,
                bg,
            );
            draw_rectangle(x, cy - h * 0.22, w * 0.65, h * 0.44, fg);
            draw_triangle(
                vec2(x + w * 0.65, cy - h * 0.22),
                vec2(x + w * 0.65, cy + h * 0.22),
                vec2(x + w, cy),
                fg,
            );
            draw_rectangle(x + w * 0.12, cy - h * 0.22, w * 0.06, h * 0.44, bg);
            w
        }
        Cause::Fire => {
            // a flame
            let w = h * 0.7;
            let c = vec2(x + w * 0.5, cy + h * 0.15);
            draw_circle(c.x, c.y, w * 0.42 + 1.0, bg);
            draw_triangle(
                vec2(c.x - w * 0.42, c.y),
                vec2(c.x + w * 0.42, c.y),
                vec2(c.x + w * 0.05, cy - h * 0.5),
                fg,
            );
            draw_circle(c.x, c.y, w * 0.42, fg);
            w
        }
        Cause::Fpv => {
            // a quad drone from above
            let s = h * 0.9;
            let (cx, cy2) = (x + s * 0.6, cy);
            draw_line(
                cx - s * 0.45,
                cy2 - s * 0.45,
                cx + s * 0.45,
                cy2 + s * 0.45,
                3.0,
                fg,
            );
            draw_line(
                cx - s * 0.45,
                cy2 + s * 0.45,
                cx + s * 0.45,
                cy2 - s * 0.45,
                3.0,
                fg,
            );
            for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                draw_circle_lines(cx + dx * s * 0.45, cy2 + dy * s * 0.45, s * 0.2, 1.5, fg);
            }
            draw_rectangle(cx - s * 0.14, cy2 - s * 0.14, s * 0.28, s * 0.28, fg);
            s * 1.25
        }
        Cause::Drop => {
            // a falling bomb with fins
            let (bx, w) = (x + h * 0.35, h * 0.5);
            draw_circle(bx, cy + h * 0.15, w * 0.5 + 1.0, bg);
            draw_circle(bx, cy + h * 0.15, w * 0.5, fg);
            draw_rectangle(bx - w * 0.5, cy - h * 0.2, w, h * 0.35, fg);
            draw_triangle(
                vec2(bx - w * 0.6, cy - h * 0.45),
                vec2(bx + w * 0.6, cy - h * 0.45),
                vec2(bx, cy - h * 0.15),
                fg,
            );
            h * 0.8
        }
        Cause::Nrk => {
            // a tracked robot with a gun
            let w = h * 1.8;
            draw_rectangle(x, cy + h * 0.1, w * 0.62, h * 0.3, fg);
            draw_rectangle(x + w * 0.08, cy - h * 0.2, w * 0.45, h * 0.3, fg);
            draw_rectangle(x + w * 0.5, cy - h * 0.12, w * 0.5, h * 0.1, fg);
            for k in 0..3 {
                draw_circle(x + w * (0.1 + k as f32 * 0.21), cy + h * 0.25, h * 0.08, bg);
            }
            w
        }
        Cause::Fall | Cause::World | Cause::Zone | Cause::Bleed => {
            // cartoon skull
            let (sx, sy) = (x + h * 0.5, cy);
            draw_circle(sx, sy - h * 0.05, h * 0.42, fg);
            draw_rectangle(sx - h * 0.22, sy + h * 0.15, h * 0.44, h * 0.25, fg);
            draw_circle(sx - h * 0.16, sy - h * 0.05, h * 0.11, bg);
            draw_circle(sx + h * 0.16, sy - h * 0.05, h * 0.11, bg);
            h * 1.1
        }
    }
}

/// Head with a crosshair — shown after the weapon for headshots.
fn draw_headshot_icon(cx: f32, cy: f32, h: f32, a: f32) {
    let (fg, bg) = icon_color(a);
    let r = h * 0.38;
    draw_circle(cx, cy, r + 1.2, bg);
    draw_circle(cx, cy, r, fg);
    let red = Color::new(0.95, 0.25, 0.2, a);
    draw_circle_lines(cx, cy, r * 0.75, 1.5, red);
    draw_line(cx - r * 1.2, cy, cx + r * 1.2, cy, 1.5, red);
    draw_line(cx, cy - r * 1.2, cx, cy + r * 1.2, 1.5, red);
}

/// A small bomb (three red sticks and a blinking timer) centred on `(x, y)`.
fn draw_bomb_icon(x: f32, y: f32, scale: f32, pulse: f32) {
    let (w, h) = (30.0 * scale, 22.0 * scale);
    let (x0, y0) = (x - w / 2.0, y - h / 2.0);
    draw_rectangle(
        x0 - 4.0,
        y0 - 4.0,
        w + 8.0,
        h + 8.0,
        Color::new(0.0, 0.0, 0.0, 0.55),
    );
    let sw = w / 3.0;
    for k in 0..3 {
        let sx = x0 + k as f32 * sw;
        draw_rectangle(sx, y0, sw - 1.0, h, Color::from_rgba(214, 60, 48, 255));
        draw_rectangle_lines(sx, y0, sw - 1.0, h, 1.5, BLACK);
    }
    draw_rectangle(
        x0 - 1.0,
        y0 + h * 0.4,
        w + 1.0,
        h * 0.2,
        Color::from_rgba(60, 60, 66, 255),
    );
    let led = if pulse > 0.5 {
        Color::from_rgba(255, 70, 50, 255)
    } else {
        Color::from_rgba(120, 30, 25, 255)
    };
    draw_circle(x, y, 3.0 * scale, led);
    draw_line(
        x0 + w,
        y0 + 3.0,
        x0 + w + 7.0 * scale,
        y0 - 6.0 * scale,
        2.0,
        ACCENT,
    );
}

/// What a pick-up prompt calls a thing on the ground.
fn pick_label(p: Pick) -> String {
    match p {
        Pick::Weapon(w) => w.def().name.into(),
        Pick::Car(k) => k.name().into(),
        Pick::DeathBox => t("ящик", "box").into(),
        Pick::Door(_) => t("двері", "door").into(),
        Pick::Loot(k, n) => match k {
            LootKind::Ammo(_) | LootKind::Medkit if n > 1 => format!("{} ×{n}", k.name()),
            LootKind::Armor(_) | LootKind::Helmet(_) if n < 100 => format!("{} ({n}%)", k.name()),
            _ => k.name(),
        },
    }
}

/// Where the big map is on the screen: (x, y, side).
pub fn big_map_rect() -> (f32, f32, f32) {
    let (w, h) = (screen_width(), screen_height());
    let side = (h * 0.86).min(w * 0.9);
    ((w - side) / 2.0, (h - side) / 2.0, side)
}

/// The map marker: a yellow pin whose point is at `p`.
fn draw_marker_pin(p: Vec2, k: f32) {
    let c = Color::new(1.0, 0.85, 0.15, 1.0);
    let top = p - vec2(0.0, 20.0 * k);
    draw_triangle(p, top + vec2(-7.0 * k, 0.0), top + vec2(7.0 * k, 0.0), c);
    draw_circle(top.x, top.y, 8.0 * k, c);
    draw_circle_lines(top.x, top.y, 8.0 * k, 2.0, BLACK);
    draw_circle(top.x, top.y, 3.0 * k, BLACK);
}

/// A supply crate on a map: a blue box with an orange band.
fn draw_crate_icon(p: Vec2, k: f32) {
    let s = 7.0 * k;
    draw_rectangle(
        p.x - s - 1.5,
        p.y - s - 1.5,
        2.0 * s + 3.0,
        2.0 * s + 3.0,
        BLACK,
    );
    draw_rectangle(
        p.x - s,
        p.y - s,
        2.0 * s,
        2.0 * s,
        Color::new(0.24, 0.43, 0.67, 1.0),
    );
    draw_rectangle(
        p.x - s,
        p.y - 1.5 * k,
        2.0 * s,
        3.0 * k,
        Color::new(0.94, 0.59, 0.16, 1.0),
    );
}
