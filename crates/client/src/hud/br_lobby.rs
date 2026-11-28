//! The battle royale lobby: the waiting room, squads and the squad panel.

use super::*;

impl GameClient {
    /// The squad on the left: names, health, who is down.
    /// The battle royale lobby, Apex-style: we and our squad on podiums in the middle (drawn
    /// before the HUD), names and «Ready» under them, the squad on the left, the character,
    /// achievements and the rest on the right, a big «Ready» at the bottom.
    pub(super) fn draw_br_lobby(
        &mut self,
        ui: &mut Ui,
        settings: &mut Settings,
    ) -> Option<HudAction> {
        use toon_shared::protocol::C2S;
        let (w, h) = (screen_width(), screen_height());
        let lobby = self.lobby.clone()?;
        let size = lobby.size as usize;
        // the title
        ui.centered(
            t("КОРОЛІВСЬКА БИТВА", "BATTLE ROYALE"),
            w / 2.0,
            50.0,
            32,
            ACCENT,
        );
        let kind = match size {
            1 => t("Соло", "Solo"),
            2 => t("Дуо", "Duos"),
            _ => t("Четвірки", "Fours"),
        };
        ui.centered(
            &format!("{kind} · {} · {}", self.map.id.name(), self.server_name),
            w / 2.0,
            76.0,
            16,
            MUTED,
        );
        // our character: drag it round with the mouse, the wheel brings it closer
        let side_w = (w * 0.27).clamp(260.0, 360.0);
        let m = ui.mouse;
        let over_stage =
            m.x > side_w + 30.0 && m.x < w - side_w - 30.0 && m.y > 100.0 && m.y < h - 140.0;
        if is_mouse_button_down(MouseButton::Left) {
            if let Some(last) = self.lobby_drag {
                self.lobby_turn += (m.x - last.x) * 0.012;
            }
            if self.lobby_drag.is_some() || over_stage && is_mouse_button_pressed(MouseButton::Left)
            {
                self.lobby_drag = Some(m);
            }
        } else {
            self.lobby_drag = None;
        }
        let wheel = mouse_wheel().1;
        if over_stage && wheel.abs() > 0.01 {
            self.lobby_zoom = (self.lobby_zoom + wheel.signum() * 0.12).clamp(0.0, 1.0);
        }
        if self.lobby_zoom < 0.05 && self.lobby_turn.abs() < 0.01 {
            ui.centered(
                t(
                    "Тягни персонажа мишею, щоб покрутити; коліщатко — ближче",
                    "Drag your character to turn it round; the wheel brings it closer",
                ),
                w / 2.0,
                108.0,
                13,
                MUTED,
            );
        }
        // names (and who is ready) over the players
        for (k, (id, _)) in self.lobby_cast(settings.skin).iter().enumerate().take(4) {
            let (x, z) = crate::render::LOBBY_SPOTS[k];
            // (over their heads: under them is the «Ready» button)
            let p = crate::render::lobby_screen_pos(vec3(x, 2.25, z), self.lobby_zoom)
                - vec2(0.0, 46.0);
            // (below the title, however close the camera came)
            let p = vec2(p.x, p.y.max(118.0));
            let name = if *id == self.my_id {
                self.name_of(*id).unwrap_or(t("Ти", "You")).to_string()
            } else {
                self.name_of(*id).unwrap_or("?").to_string()
            };
            let ready = lobby.ready.contains(id);
            let size_t = if k == 0 { 20 } else { 16 };
            let tw = ui.width(&name, size_t, true);
            soft_rect(
                p.x - tw / 2.0 - 10.0,
                p.y,
                tw + 20.0,
                46.0,
                Color::new(0.0, 0.0, 0.0, 0.55),
            );
            ui.bold(
                &name,
                p.x - tw / 2.0,
                p.y + 22.0,
                size_t,
                if k == 0 { ACCENT } else { TEXT },
            );
            let (rt, rc) = if ready {
                (t("ГОТОВИЙ", "READY"), GOOD)
            } else {
                (t("не готовий", "not ready"), MUTED)
            };
            let rw = ui.width(rt, 13, true);
            ui.bold(rt, p.x - rw / 2.0, p.y + 40.0, 13, rc);
        }
        if size > 1 && self.lobby_cast(settings.skin).len() < size && self.lobby_zoom < 0.3 {
            ui.centered(
                t(
                    "Вільні місця в загоні займуть інші гравці або боти",
                    "Free places in the squad go to other players or bots",
                ),
                w / 2.0,
                h - 150.0,
                14,
                MUTED,
            );
        }
        // the big «Ready»
        let (n, of, me, left) = self.ready.unwrap_or((0, 0, false, 0));
        let (bw, bh) = (320.0, 62.0);
        let (bx, by) = ((w - bw) / 2.0, h - 128.0);
        let pre = self.state == MatchState::PreRound;
        if lobby.running {
            ui.centered(
                t(
                    "Матч триває — ти почнеш у наступному",
                    "A match is on — you join the next one",
                ),
                w / 2.0,
                by + 40.0,
                24,
                ACCENT,
            );
        } else if pre {
            ui.centered(
                &tr!(
                    "Висадка через {}",
                    "Boarding in {}",
                    self.time_left.ceil() as i32
                ),
                w / 2.0,
                by + 40.0,
                30,
                ACCENT,
            );
        } else {
            let (label, c) = if me {
                (t("НЕ ГОТОВИЙ", "NOT READY"), MUTED)
            } else {
                (t("ГОТОВИЙ", "READY"), GOOD)
            };
            if ui.button_colored(label, bx, by, bw, bh, c) {
                self.net.send(C2S::Ready(!me));
            }
            let _ = left;
            let status = if of > 0 && n >= of {
                t("Усі готові — починаємо", "Everyone is ready — here we go").to_string()
            } else if me {
                tr!(
                    "Ти готовий · {n}/{of} — чекаємо на інших",
                    "You are ready · {n}/{of} — waiting for the others"
                )
            } else if of > 0 {
                tr!(
                    "{n}/{of} готові — матч почнеться, коли всі будуть готові",
                    "{n}/{of} ready — the match starts once everyone is"
                )
            } else {
                t("Очікування гравців…", "Waiting for players…").to_string()
            };
            ui.centered(&status, w / 2.0, by + bh + 24.0, 15, TEXT);
        }
        // the squad, on the left
        let side = (w * 0.27).clamp(260.0, 360.0);
        if size > 1 {
            // (ends above the chat in the corner)
            let (px, py, ph) = (16.0, 110.0, (h - 360.0).max(220.0));
            ui.panel(px, py, side, ph);
            ui.bold(t("Загін", "Squad"), px + 18.0, py + 32.0, 22, ACCENT);
            self.squad_list(ui, &lobby, px - 6.0, py + 64.0, side + 12.0, py + ph - 10.0);
        }
        // the character and the rest, on the right
        let (px, py) = (w - side - 16.0, 110.0);
        ui.panel(px, py, side, 330.0);
        ui.bold(t("Персонаж", "Character"), px + 18.0, py + 32.0, 22, ACCENT);
        let n_sk = toon_shared::skins::count();
        let d = ui.stepper(
            "",
            toon_shared::skins::name(settings.skin),
            px + 18.0,
            py + 52.0,
            side - 36.0,
        );
        if d != 0 {
            settings.skin = ((settings.skin as i32 + d).rem_euclid(n_sk as i32)) as u8;
            settings.save();
        }
        let bw2 = side - 36.0;
        let mut y = py + 104.0;
        if ui.button2(t("Досягнення", "Achievements"), px + 18.0, y, bw2, 42.0) {
            self.leaderboards = None;
            self.net.send(C2S::Leaderboards);
            self.overlay = Overlay::Achievements;
        }
        y += 52.0;
        if ui.button2(t("Рейтинг", "Leaderboards"), px + 18.0, y, bw2, 42.0) {
            self.leaderboards = None;
            self.net.send(C2S::Leaderboards);
            self.overlay = Overlay::Leaders;
        }
        y += 52.0;
        if ui.button2(t("Налаштування", "Settings"), px + 18.0, y, bw2, 42.0) {
            self.overlay = Overlay::Settings;
        }
        y += 52.0;
        let half = (bw2 - 10.0) / 2.0;
        if ui.button2(t("Дивитися", "Watch"), px + 18.0, y, half, 42.0) {
            self.join_team(Some(Team::Spectator));
        }
        if ui.button_colored(t("Вийти", "Leave"), px + 28.0 + half, y, half, 42.0, BAD) {
            return Some(HudAction::Leave);
        }
        None
    }

    /// The squad lobby: who plays with whom in the coming battle royale, requests to and
    /// from other players, and «Ready».
    pub(super) fn draw_squad_lobby(&mut self, ui: &mut Ui) {
        use toon_shared::protocol::C2S;
        let (w, h) = (screen_width(), screen_height());
        soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.55));
        let Some(lobby) = self.lobby.clone() else {
            self.overlay = Overlay::None;
            return;
        };
        let pw = (w - 40.0).min(640.0);
        let ph = (h - 40.0).min(620.0);
        let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
        ui.panel(x, y, pw, ph);
        let size = lobby.size as usize;
        let title = match size {
            2 => t("Загін — дуо", "Squad — duos"),
            _ => t("Загін — по 4", "Squad — fours"),
        };
        ui.bold(title, x + 24.0, y + 42.0, 26, ACCENT);
        for (k, line) in [
            t(
                "Запроси гравця — коли він погодиться, ви висадитеся разом.",
                "Ask a player — once they say yes, you land together.",
            ),
            t(
                "Вільні місця займуть інші гравці або боти.",
                "Free places go to other players or bots.",
            ),
        ]
        .iter()
        .enumerate()
        {
            ui.text(line, x + 24.0, y + 64.0 + k as f32 * 16.0, 13, MUTED);
        }
        self.squad_list(ui, &lobby, x, y + 112.0, pw, y + ph - 76.0);
        // ready, and back
        let by = y + ph - 64.0;
        let half = (pw - 60.0) / 2.0;
        if let Some((n, of, me, _)) = self.ready {
            let (label, c) = if me {
                (tr!("Готовий ✓ ({n}/{of})", "Ready ✓ ({n}/{of})"), GOOD)
            } else {
                (tr!("Готовий ({n}/{of})", "Ready ({n}/{of})"), ACCENT)
            };
            if ui.button_colored(&label, x + 24.0, by, half, 44.0, c) {
                self.net.send(C2S::Ready(!me));
            }
        }
        if ui.button2(
            t("Закрити (Esc)", "Close (Esc)"),
            x + 36.0 + half,
            by,
            half,
            44.0,
        ) {
            self.overlay = Overlay::None;
        }
    }

    /// The squad part of the lobby: our squad, who asks to join us, and the others with
    /// «Ask to team up», inside a panel at `x` that is `pw` wide, from `top` to `bottom`.
    fn squad_list(
        &mut self,
        ui: &mut Ui,
        lobby: &toon_shared::protocol::LobbySnap,
        x: f32,
        top: f32,
        pw: f32,
        bottom: f32,
    ) {
        use toon_shared::protocol::{PartyCmd, C2S};
        let size = lobby.size as usize;
        // buttons fit narrow panels too
        let small = ((pw - 48.0) * 0.27).min(112.0);
        let wide = ((pw - 48.0) * 0.42).min(166.0);
        let names_of: std::collections::HashMap<u8, String> = lobby
            .parties
            .iter()
            .flatten()
            .chain(lobby.asking_me.iter())
            .map(|&id| {
                let n = if id == self.my_id {
                    t("Ти", "You").to_string()
                } else {
                    self.name_of(id).unwrap_or("?").to_string()
                };
                (id, n)
            })
            .collect();
        let name = |id: u8| -> String { names_of.get(&id).cloned().unwrap_or_else(|| "?".into()) };
        let mine: Vec<u8> = lobby
            .parties
            .iter()
            .find(|g| g.contains(&self.my_id))
            .cloned()
            .unwrap_or_else(|| vec![self.my_id]);
        let mut cy = top;
        // our squad
        ui.bold(
            &tr!("Мій загін ({}/{size})", "My squad ({}/{size})", mine.len()),
            x + 24.0,
            cy,
            17,
            TEXT,
        );
        cy += 10.0;
        let names: Vec<String> = mine.iter().map(|&id| name(id)).collect();
        soft_rect(
            x + 24.0,
            cy,
            pw - 48.0,
            40.0,
            Color::new(0.0, 0.0, 0.0, 0.3),
        );
        ui.text(&names.join(", "), x + 36.0, cy + 26.0, 16, GOOD);
        if mine.len() > 1
            && ui.button_colored(
                t("Вийти", "Leave"),
                x + pw - 24.0 - 110.0,
                cy + 4.0,
                106.0,
                32.0,
                BAD,
            )
        {
            self.net.send(C2S::Party(PartyCmd::Leave));
        }
        cy += 60.0;
        // asking us
        if !lobby.asking_me.is_empty() {
            ui.bold(
                t("Хочуть до тебе в загін", "Asking to join you"),
                x + 24.0,
                cy,
                17,
                GOOD,
            );
            cy += 10.0;
            for &from in lobby.asking_me.iter().take(4) {
                soft_rect(
                    x + 24.0,
                    cy,
                    pw - 48.0,
                    40.0,
                    Color::new(0.1, 0.3, 0.1, 0.35),
                );
                ui.text(&name(from), x + 36.0, cy + 26.0, 16, TEXT);
                if ui.button_colored(
                    t("Прийняти", "Accept"),
                    x + pw - 30.0 - 2.0 * small,
                    cy + 4.0,
                    small,
                    32.0,
                    GOOD,
                ) {
                    self.net.send(C2S::Party(PartyCmd::Accept(from)));
                }
                if ui.button_colored(
                    t("Відхилити", "Decline"),
                    x + pw - 24.0 - small,
                    cy + 4.0,
                    small,
                    32.0,
                    MUTED,
                ) {
                    self.net.send(C2S::Party(PartyCmd::Decline(from)));
                }
                cy += 46.0;
            }
            cy += 14.0;
        }
        // everybody else
        ui.bold(t("Гравці", "Players"), x + 24.0, cy, 17, TEXT);
        cy += 10.0;
        let others: Vec<&Vec<u8>> = lobby
            .parties
            .iter()
            .filter(|g| !g.contains(&self.my_id))
            .collect();
        if others.is_empty() {
            ui.text(
                t(
                    "Поки нікого: зачекай, доки зайдуть інші",
                    "Nobody yet: wait for others to come",
                ),
                x + 36.0,
                cy + 24.0,
                14,
                MUTED,
            );
        }
        for g in others {
            if cy + 40.0 > bottom {
                break;
            }
            soft_rect(
                x + 24.0,
                cy,
                pw - 48.0,
                40.0,
                Color::new(0.0, 0.0, 0.0, 0.3),
            );
            let label = if g.len() > 1 {
                let all: Vec<String> = g.iter().map(|&id| name(id)).collect();
                tr!(
                    "{} (загін {}/{size})",
                    "{} (squad {}/{size})",
                    all.join(", "),
                    g.len()
                )
            } else {
                name(g[0])
            };
            ui.text(&label, x + 36.0, cy + 26.0, 15, TEXT);
            // one button for the whole squad: asking its first player is enough
            let to = g[0];
            let full = g.len() + mine.len() > size;
            let asked = g.iter().any(|id| lobby.i_asked.contains(id));
            let asking = g.iter().find(|id| lobby.asking_me.contains(id)).copied();
            if let Some(from) = asking {
                if ui.button_colored(
                    t("Прийняти", "Accept"),
                    x + pw - 24.0 - wide,
                    cy + 4.0,
                    wide,
                    32.0,
                    GOOD,
                ) {
                    self.net.send(C2S::Party(PartyCmd::Accept(from)));
                }
                cy += 46.0;
                continue;
            }
            let (text, c) = if asked {
                (t("Запит надіслано", "Asked"), MUTED)
            } else if full {
                (t("Немає місця", "No room"), MUTED)
            } else {
                (t("Запросити", "Ask to team up"), ACCENT)
            };
            if ui.button_colored(text, x + pw - 24.0 - wide, cy + 4.0, wide, 32.0, c)
                && !asked
                && !full
            {
                self.net.send(C2S::Party(PartyCmd::Ask(to)));
            }
            cy += 46.0;
        }
    }

    pub(super) fn draw_squad_panel(&self, ui: &Ui) {
        let Some(br) = &self.br else { return };
        if br.squad.len() < 2 {
            return;
        }
        let x = 16.0;
        let mut y = screen_height() * 0.5 - 20.0;
        for m in &br.squad {
            let c = self.mate_color(m.id);
            soft_rect(x, y, 190.0, 30.0, Color::new(0.0, 0.0, 0.0, 0.45));
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
            soft_rect(
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
}
