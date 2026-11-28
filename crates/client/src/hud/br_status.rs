//! The battle royale's own status: the panel, alerts, and the screen once knocked out.

use super::*;

impl GameClient {
    /// Where the battle royale's round summary ends on the screen (None: not shown).
    pub(super) fn br_summary_bottom(&self) -> Option<f32> {
        let s = self.br.as_ref()?.summary.as_ref()?;
        if !(self.br_out() || self.match_end.is_some()) || self.killcam.is_some() {
            return None;
        }
        let killer = self.death_info.is_some() && s.place != 1;
        Some(screen_height() * 0.12 + 100.0 + if killer { 28.0 } else { 0.0 })
    }

    /// Out of the battle royale: whom we watch, and how the round went.
    pub(super) fn draw_br_out(&self, ui: &Ui) {
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
            soft_rect(0.0, h - 40.0, w, 40.0, Color::new(0.0, 0.0, 0.0, 0.45));
            ui.centered(&line, w / 2.0, h - 14.0, 16, TEXT);
        }
        // the round in numbers (once out, or when it is over)
        let Some(s) = self.br.as_ref().and_then(|b| b.summary.clone()) else {
            return;
        };
        if !(self.br_out() || self.match_end.is_some()) || self.killcam.is_some() {
            return;
        }
        let pw = 620.0f32.min(w - 20.0);
        let (x, y0) = ((w - pw) / 2.0, h * 0.12);
        // who got us, above the rest
        let killer = self
            .death_info
            .as_ref()
            .filter(|_| s.place != 1)
            .map(|(killer, cause, _)| {
                if killer.is_empty() {
                    tr!("Ти вибув ({})", "You are out ({})", cause.name())
                } else {
                    tr!(
                        "Тебе вибив {killer} — {}",
                        "Knocked out by {killer} — {}",
                        cause.name()
                    )
                }
            });
        let extra = if killer.is_some() { 28.0 } else { 0.0 };
        soft_rect(x, y0, pw, 100.0 + extra, Color::new(0.0, 0.0, 0.0, 0.55));
        if let Some(k) = &killer {
            ui.centered(k, w / 2.0, y0 + 26.0, 17, MUTED);
        }
        let y = y0 + extra;
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

    /// Battle royale: players left, kills, the zone clock, health and armour.
    pub(super) fn draw_br_panel(&self, ui: &Ui) {
        let Some(br) = &self.br else { return };
        let size = (screen_height() * 0.26).clamp(150.0, 240.0);
        let (x, y) = (16.0, 16.0 + size + 12.0);
        let w = size;
        let panel_h = if self.alive { 112.0 } else { 60.0 };
        soft_rect(
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
                round_rect(
                    x + 4.0,
                    yy,
                    w - 8.0,
                    10.0,
                    5.0,
                    Color::new(0.0, 0.0, 0.0, 0.5),
                );
                let fill = (w - 8.0) * k.clamp(0.0, 1.0);
                if fill > 1.0 {
                    round_rect(x + 4.0, yy, fill, 10.0, 5.0, c);
                }
            };
            ui.shadowed(t("Здоров'я", "Health"), x + 4.0, y + 66.0, 12, MUTED);
            // (health just lost lingers in white for a moment, then drains away)
            let hp = self.me.hp / MAX_HEALTH;
            let was = self.hud_hp / MAX_HEALTH;
            if was > hp + 0.005 {
                bar(y + 70.0, was, Color::new(1.0, 1.0, 1.0, 0.75));
            }
            let low = hp < 0.3;
            let pulse = 0.75 + 0.25 * ((self.now * 6.0).sin() as f32);
            bar(
                y + 70.0,
                hp,
                if low {
                    Color::new(BAD.r, BAD.g, BAD.b, pulse)
                } else {
                    GOOD
                },
            );
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

    /// Battle royale prompts: leaving the plane, the parachute, being outside the zone.
    pub(super) fn draw_br_alerts(&self, ui: &Ui) {
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
            let n = br.aboard.max(1);
            ui.centered(
                &tr!("У літаку: {n}", "In the plane: {n}"),
                w / 2.0,
                h * 0.8 - 36.0,
                20,
                TEXT,
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
}
