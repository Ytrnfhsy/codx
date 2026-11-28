//! The score at the top and the scoreboards.

use super::*;

impl GameClient {
    pub(super) fn draw_score_panel(&self, ui: &Ui) {
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
            self.tcol(my)
        } else {
            self.tcol(Team::Allies)
        };
        let en_c = if my.is_playing() {
            self.tcol(my.enemy())
        } else {
            self.tcol(Team::Axis)
        };
        let limit = match self.mode {
            Mode::Tdm | Mode::Br => self.gs.score_limit,
            Mode::Sab => self.gs.rounds_to_win as i32,
        };
        let bw = size;
        soft_rect(
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

    pub(super) fn draw_scoreboard(&self, ui: &Ui, top: f32) {
        if self.mode == Mode::Br {
            return self.draw_br_scoreboard(ui, top);
        }
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
            let c = self.tcol(*team);
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
                let c = match (r.bot, r.alive) {
                    (false, true) => HUMAN,
                    (false, false) => with_alpha(HUMAN, 0.6),
                    (true, true) => TEXT,
                    (true, false) => MUTED,
                };
                ui.text(&name, x, yy, 16, c);
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

    /// The battle royale's table: no teams, everybody by place (those still in first, by
    /// kills), in as many columns as it takes to fit the screen.
    fn draw_br_scoreboard(&self, ui: &Ui, top: f32) {
        let (w, h) = (screen_width(), screen_height());
        let mut rows: Vec<_> = self
            .scoreboard
            .iter()
            .filter(|r| r.team.is_playing())
            .collect();
        rows.sort_by_key(|r| {
            (
                if r.place == 0 { 0 } else { r.place },
                std::cmp::Reverse(r.kills),
            )
        });
        let row_h = 22.0;
        let room = (h - top - 110.0).max(row_h * 4.0);
        let per_col = ((room / row_h).floor() as usize).max(4);
        let cols = rows.len().div_ceil(per_col).clamp(1, 4);
        let per_col = rows.len().div_ceil(cols).max(1);
        let pw = (w - 40.0).min(330.0 * cols as f32 + 32.0);
        let x0 = (w - pw) / 2.0;
        let ph = 96.0 + per_col as f32 * row_h;
        ui.panel(x0, top, pw, ph);
        ui.bold(&self.server_name, x0 + 16.0, top + 30.0, 20, ACCENT);
        let alive = rows.iter().filter(|r| r.place == 0).count();
        ui.text(
            &tr!(
                "{} · {} · живих {} з {}",
                "{} · {} · {} of {} still in",
                self.map.id.name(),
                self.mode.name(),
                alive,
                rows.len()
            ),
            x0 + 16.0,
            top + 52.0,
            15,
            MUTED,
        );
        let col_w = (pw - 32.0) / cols as f32;
        for c in 0..cols {
            let x = x0 + 16.0 + c as f32 * col_w;
            ui.text(t("#   Гравець", "#   Player"), x, top + 80.0, 13, MUTED);
            ui.text(t("Вб.", "K"), x + col_w - 40.0, top + 80.0, 13, MUTED);
        }
        for (i, r) in rows.iter().enumerate() {
            let (c, k) = (i / per_col, i % per_col);
            let x = x0 + 16.0 + c as f32 * col_w;
            let yy = top + 100.0 + k as f32 * row_h;
            if r.id == self.my_id {
                draw_rectangle(
                    x - 4.0,
                    yy - 16.0,
                    col_w - 8.0,
                    20.0,
                    Color::new(1.0, 1.0, 1.0, 0.1),
                );
            }
            let place = if r.place == 0 {
                "—".to_string()
            } else {
                r.place.to_string()
            };
            let colour = if r.place == 1 {
                ACCENT
            } else if r.place == 0 {
                TEXT
            } else {
                MUTED
            };
            ui.text(&place, x, yy, 14, colour);
            // (names cut to the column)
            let mut name = r.name.clone();
            if r.squad != 0 && self.br.as_ref().is_some_and(|b| b.squad.len() > 1) {
                name = format!("{name} ·{}", r.squad);
            }
            while name.chars().count() > 3 && ui.width(&name, 15, false) > col_w - 80.0 {
                name.pop();
            }
            // real players stand out from the bots
            let name_c = if r.bot {
                colour
            } else if r.place == 0 {
                HUMAN
            } else {
                with_alpha(HUMAN, 0.6)
            };
            ui.text(&name, x + 30.0, yy, 15, name_c);
            ui.text(&r.kills.to_string(), x + col_w - 40.0, yy, 15, colour);
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
                top + ph - 10.0,
                14,
                MUTED,
            );
        }
    }
}
