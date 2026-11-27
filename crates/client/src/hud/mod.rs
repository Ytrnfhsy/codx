//! 2D HUD and in-game menus.

mod br_inventory;
mod br_lobby;
mod br_status;
mod crosshair;
mod killfeed;
mod markers;
mod minimap;
mod scope;
mod scoreboard;
mod weapon_panel;

use crosshair::*;
use icons::*;
use markers::*;
pub use minimap::big_map_rect;
use overlay::*;
mod chat;
mod icons;
mod overlay;

use crate::binds::{key, Action};
use crate::game::{GameClient, Overlay, Pick};
use crate::render::{NameTag, View};
use crate::settings::Settings;
use crate::ui::*;
use macroquad::prelude::*;
use toon_shared::consts::*;
use toon_shared::lang::t;
use toon_shared::protocol::{flags, EntSnap, LootKind, MatchState};
use toon_shared::tr;
use toon_shared::weapons::{Cause, Killstreak, WeaponId};
use toon_shared::{look_dir, wrap_angle, yaw_of, Mode, Team};

pub enum HudAction {
    Leave,
}

fn fmt_time(t: f32) -> String {
    // (rounded up, as countdowns are: 0:00 only when it is over)
    let t = t.max(0.0).ceil() as u32;
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

        // between matches: the results, the best moments (Tab shows the table)
        let tab = crate::binds::down(Action::Scoreboard) && self.chat_input.is_none();
        if self.summary_active() && !tab {
            self.draw_summary(ui);
            self.draw_chat(ui);
            return self.draw_overlay(ui, settings);
        }
        if self.killcam.is_some() && !self.summary_active() {
            self.draw_killcam(ui);
            self.draw_killfeed(ui);
            self.draw_chat(ui);
            return self.draw_overlay(ui, settings);
        }
        // the battle royale lobby: its own screen until the plane takes off
        if self.in_br_lobby() {
            let leave = if self.overlay == Overlay::None {
                self.draw_br_lobby(ui, settings)
            } else {
                None
            };
            self.draw_killfeed(ui);
            self.draw_chat(ui);
            if leave.is_some() {
                return leave;
            }
            return self.draw_overlay(ui, settings);
        }

        if self.net_lost() {
            let (w, h) = (screen_width(), screen_height());
            let left = (crate::game::NET_GIVE_UP - self.net_silence())
                .max(0.0)
                .ceil();
            draw_rectangle(0.0, h * 0.6, w, 74.0, Color::new(0.0, 0.0, 0.0, 0.6));
            ui.centered(
                t(
                    "Зв'язок із сервером втрачено",
                    "Lost connection to the server",
                ),
                w / 2.0,
                h * 0.6 + 30.0,
                28,
                BAD,
            );
            let more = format!(
                "{} {left:.0} {}",
                t(
                    "Чекаємо на з'єднання… ще",
                    "Waiting for the connection… another"
                ),
                t("с, потім — головне меню", "s, then the main menu")
            );
            ui.centered(&more, w / 2.0, h * 0.6 + 58.0, 16, TEXT);
        }
        if self.scoped() && self.alive {
            self.draw_scope();
            self.draw_holdover(ui, view.fovy);
            // the rangefinder: how far off is what the crosshair rests on, over the
            // right arm of the cross
            {
                let (w, h) = (screen_width(), screen_height());
                let label = match self.scope_range() {
                    Some(d) => format!("{d:.0} {}", t("м", "m")),
                    None => format!("--- {}", t("м", "m")),
                };
                let (x, y) = (w / 2.0 + h * 0.46 * 0.42, h / 2.0 - 30.0);
                ui.centered(&label, x, y, 22, TEXT);
            }
            // a variable scope says what it is set to, and how to turn it
            if self.scope_turns() {
                let (w, h) = (screen_width(), screen_height());
                let z = self.ads_zoom();
                let label = format!("{z:.1}×");
                ui.centered(&label, w / 2.0 + h * 0.3, h / 2.0 + h * 0.36, 22, ACCENT);
                ui.centered(
                    t("колесо — кратність", "wheel — zoom"),
                    w / 2.0 + h * 0.3,
                    h / 2.0 + h * 0.36 + 20.0,
                    13,
                    MUTED,
                );
            }
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
                    ui.shadowed(&label, sp.x - tw / 2.0, sp.y, 15, self.tcol(self.team));
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
            self.draw_pings(ui, view);
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

        // a faint little cross always in the middle (to point at things and ping them)
        if self.alive && self.ads_t < 0.5 && self.piloting.is_none() && !self.big_map {
            let sh = Color::new(0.0, 0.0, 0.0, 0.45);
            let c = Color::new(1.0, 1.0, 1.0, 0.7);
            draw_rectangle(cx - 4.0, cy - 1.5, 8.0, 3.0, sh);
            draw_rectangle(cx - 1.5, cy - 4.0, 3.0, 8.0, sh);
            draw_rectangle(cx - 3.0, cy - 0.5, 6.0, 1.0, c);
            draw_rectangle(cx - 0.5, cy - 3.0, 1.0, 6.0, c);
        }
        // crosshair
        // (a bow has no sights: its crosshair stays when aiming)
        if self.alive
            && (self.ads_t < 0.5 || self.weapon().draw_time() > 0.0)
            && !self.sprinting
            && self.overlay == Overlay::None
            && self.piloting.is_none()
            && !self.hand_empty()
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
        // the bow being drawn: an arc under the crosshair fills up, gold when full
        if self.alive && self.draw_from.is_some() {
            let d = self.draw_now();
            let (bw, by) = (64.0, cy + 30.0);
            soft_rect(
                cx - bw / 2.0 - 2.0,
                by - 2.0,
                bw + 4.0,
                8.0,
                Color::new(0.0, 0.0, 0.0, 0.55),
            );
            let c = if d >= 1.0 {
                ACCENT
            } else {
                Color::new(1.0, 1.0, 1.0, 0.85)
            };
            draw_rectangle(cx - bw / 2.0, by, bw * d, 4.0, c);
            // where the arrow would come down: a small ring on the spot and its distance
            if let Some((at, dist)) = self.arrow_landing() {
                if let Some(sp) = view.project(at) {
                    let a = 0.35 + 0.55 * d;
                    let ring = Color::new(1.0, 0.85, 0.35, a);
                    draw_circle_lines(sp.x, sp.y, 7.0, 3.0, Color::new(0.0, 0.0, 0.0, a * 0.6));
                    draw_circle_lines(sp.x, sp.y, 7.0, 1.6, ring);
                    draw_circle(sp.x, sp.y, 1.6, ring);
                    ui.shadowed(
                        &format!("{dist:.0} {}", t("м", "m")),
                        sp.x + 11.0,
                        sp.y + 5.0,
                        14,
                        Color::new(1.0, 1.0, 1.0, a),
                    );
                }
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
            soft_rect(x, y, 220.0, 64.0, Color::new(0.0, 0.0, 0.0, 0.5));
            ui.bold(
                &format!("{kmh:.0} {}", t("км/год", "km/h")),
                x + 12.0,
                y + 30.0,
                24,
                TEXT,
            );
            ui.text(c.kind.name(), x + 140.0, y + 26.0, 15, MUTED);
            let f = c.hp as f32 / 100.0;
            soft_rect(
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
                if let Some((n, of, me, left)) = self.ready {
                    let (text, c) = if me {
                        (
                            tr!(
                                "Ти готовий ({n}/{of}) · {} — не готовий (перерва)",
                                "You are ready ({n}/{of}) · {} — not ready (a break)",
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
                    if left > 0 {
                        ui.centered(
                            &tr!(
                                "Матч почнеться за {left} с, навіть якщо не всі готові",
                                "The match starts in {left} s even if not everyone is ready",
                            ),
                            cx,
                            170.0,
                            16,
                            MUTED,
                        );
                    }
                }
                // the squad lobby: whom to land with
                if let Some(l) = &self.lobby {
                    let (text, c) = match l.asking_me.first() {
                        Some(&from) => (
                            tr!(
                                "{} хоче з тобою в загін — {}",
                                "{} wants to team up with you — {}",
                                self.name_of(from).unwrap_or("?"),
                                key(Action::Squad)
                            ),
                            GOOD,
                        ),
                        None => (
                            tr!(
                                "{} — загін: обери, з ким висаджуватися",
                                "{} — squad: pick whom to land with",
                                key(Action::Squad)
                            ),
                            TEXT,
                        ),
                    };
                    ui.centered(&text, cx, 196.0, 18, c);
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
        // the last ten seconds before respawns end: big, in the middle, ticking
        if let Some(left) = self.overtime_in() {
            let n = left.ceil().max(1.0);
            let frac = left - left.floor();
            // each second's number pops in big and settles
            let pop = 1.0 + 0.35 * frac.powi(3);
            let urgent = n <= 3.0;
            let c = if urgent { BAD } else { ACCENT };
            let size = (64.0 * pop) as u16;
            ui.centered(
                t("ВІДРОДЖЕННЯ ВИМКНЕТЬСЯ ЧЕРЕЗ", "RESPAWNS END IN"),
                cx,
                h * 0.22,
                24,
                TEXT,
            );
            ui.centered(&format!("{n:.0}"), cx, h * 0.22 + 70.0, size, c);
            if urgent {
                // the screen's edges pulse red
                let a = 0.25 * frac;
                for (x, y, ww, hh) in [
                    (0.0, 0.0, w, 14.0),
                    (0.0, h - 14.0, w, 14.0),
                    (0.0, 0.0, 14.0, h),
                    (w - 14.0, 0.0, 14.0, h),
                ] {
                    draw_rectangle(x, y, ww, hh, Color::new(0.9, 0.15, 0.1, a));
                }
            }
        }
        if self.overtime {
            let since = (self.now - self.overtime_at) as f32;
            if since < 4.0 {
                // the moment it starts: a banner nobody can miss
                let a = (1.0 - (since - 3.2).max(0.0) / 0.8).clamp(0.0, 1.0);
                draw_rectangle(
                    0.0,
                    h * 0.2,
                    w,
                    96.0,
                    Color::new(0.55, 0.05, 0.05, 0.75 * a),
                );
                ui.centered(
                    t(
                        "ОВЕРТАЙМ — ВІДРОДЖЕННЯ ВИМКНЕНО",
                        "OVERTIME — NO MORE RESPAWNS",
                    ),
                    cx,
                    h * 0.2 + 46.0,
                    36,
                    Color::new(1.0, 1.0, 1.0, a),
                );
                ui.centered(
                    t("одне життя — бережи його", "one life: keep it"),
                    cx,
                    h * 0.2 + 80.0,
                    18,
                    Color::new(1.0, 0.85, 0.8, a),
                );
            }
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
        // (once the round's summary is up, it says who got us)
        let summary_up = self.br.as_ref().is_some_and(|b| b.summary.is_some()) && self.br_out();
        if br_round && !self.alive && !summary_up && !self.br.as_ref().is_some_and(|b| b.in_plane) {
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
                t("Обери спорядження (Esc)", "Pick a loadout (Esc)").to_string()
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
            soft_rect(bx, 6.0, bw, 30.0, Color::new(0.0, 0.0, 0.0, 0.55));
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
        let tab = crate::binds::down(Action::Scoreboard) && self.chat_input.is_none();
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
            // the battle royale's round summary takes the title's place: the winner goes
            // under it, then the table
            if let Some(bottom) = self.br_summary_bottom() {
                ui.centered(&text, cx, bottom + 26.0, 20, c);
                if !self.final_killcam_waiting() {
                    self.draw_scoreboard(ui, bottom + 42.0);
                }
            } else {
                ui.centered(title, cx, 80.0, 48, c);
                ui.centered(&text, cx, 118.0, 22, TEXT);
                // the table comes after the final killcam
                if !self.final_killcam_waiting() {
                    self.draw_scoreboard(ui, 150.0);
                }
            }
        } else if tab {
            self.draw_scoreboard(ui, 90.0);
        }

        // overlays
        self.draw_overlay(ui, settings)
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
}

/// What a pick-up prompt calls a thing on the ground.
fn pick_label(p: Pick) -> String {
    match p {
        Pick::Weapon(w) => w.def().name().into(),
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
