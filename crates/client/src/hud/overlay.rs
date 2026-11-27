//! Full-screen pages over the game: the Esc menu and the rest, achievements.

use super::*;

impl GameClient {
    /// What we have achieved on this server, worked out from our places on its
    /// leaderboards: a card per achievement, lit once reached, with how far there is to go.
    fn draw_achievements(&mut self, ui: &mut Ui) {
        use toon_shared::protocol::BoardKind as B;
        let (w, h) = (screen_width(), screen_height());
        soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.6));
        let pw = (w - 40.0).min(980.0);
        let ph = (h - 40.0).min(600.0);
        let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
        ui.panel(x, y, pw, ph);
        ui.bold(
            t("Досягнення", "Achievements"),
            x + 24.0,
            y + 42.0,
            26,
            ACCENT,
        );
        if ui.button2(t("Назад", "Back"), x + pw - 164.0, y + 18.0, 140.0, 40.0) {
            self.overlay = self.menu_back();
            return;
        }
        let Some(boards) = &self.leaderboards else {
            ui.centered(t("Завантаження…", "Loading…"), w / 2.0, h / 2.0, 18, MUTED);
            return;
        };
        let mine = |k: B| -> f32 {
            boards
                .iter()
                .find(|b| b.kind == k)
                .and_then(|b| b.mine)
                .map_or(0.0, |m| m.1)
        };
        let list = achievements();
        let got = list.iter().filter(|a| mine(a.0) >= a.1).count();
        ui.text(
            &tr!("Відкрито {got} з {}", "{got} of {} unlocked", list.len()),
            x + 24.0,
            y + 66.0,
            14,
            MUTED,
        );
        let cols = if pw > 860.0 { 4 } else { 3 };
        let cw = (pw - 48.0 - (cols - 1) as f32 * 12.0) / cols as f32;
        let ch = 84.0;
        for (i, (kind, goal, uk, en, what_uk, what_en)) in list.iter().enumerate() {
            let cx = x + 24.0 + (i % cols) as f32 * (cw + 12.0);
            let cy = y + 86.0 + (i / cols) as f32 * (ch + 10.0);
            if cy + ch > y + ph - 10.0 {
                break;
            }
            let v = mine(*kind);
            let done = v >= *goal;
            soft_rect(
                cx,
                cy,
                cw,
                ch,
                if done {
                    Color::new(0.35, 0.28, 0.05, 0.55)
                } else {
                    Color::new(0.0, 0.0, 0.0, 0.35)
                },
            );
            // a medal
            let (mx, my) = (cx + 26.0, cy + 30.0);
            draw_circle(mx, my, 16.0, BLACK);
            draw_circle(
                mx,
                my,
                14.0,
                if done {
                    ACCENT
                } else {
                    Color::new(0.3, 0.32, 0.38, 1.0)
                },
            );
            if done {
                draw_circle(mx, my, 7.0, Color::new(1.0, 0.95, 0.6, 1.0));
            }
            let name = t(uk, en);
            ui.bold(
                name,
                cx + 50.0,
                cy + 26.0,
                15,
                if done { ACCENT } else { TEXT },
            );
            ui.text(t(what_uk, what_en), cx + 50.0, cy + 44.0, 12, MUTED);
            // progress
            let k = (v / goal).clamp(0.0, 1.0);
            draw_rectangle(
                cx + 12.0,
                cy + ch - 18.0,
                cw - 24.0,
                6.0,
                Color::new(1.0, 1.0, 1.0, 0.12),
            );
            draw_rectangle(
                cx + 12.0,
                cy + ch - 18.0,
                (cw - 24.0) * k,
                6.0,
                if done { ACCENT } else { GOOD },
            );
            let label = format!("{} / {}", kind.format(v.min(*goal)), kind.format(*goal));
            let lw = ui.width(&label, 11, false);
            ui.text(&label, cx + cw - 12.0 - lw, cy + ch - 24.0, 11, MUTED);
        }
    }

    pub(super) fn draw_overlay(
        &mut self,
        ui: &mut Ui,
        settings: &mut Settings,
    ) -> Option<HudAction> {
        let (w, h) = (screen_width(), screen_height());
        match self.overlay {
            Overlay::None => None,
            // the battle royale has no teams: you play (alone or in a squad) or you watch
            Overlay::Team if self.mode == Mode::Br => {
                soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
                let pw = 520.0;
                let (x, y) = ((w - pw) / 2.0, h * 0.2);
                ui.panel(x, y, pw, 300.0);
                ui.bold(
                    t("Королівська битва", "Battle royale"),
                    x + 24.0,
                    y + 44.0,
                    28,
                    ACCENT,
                );
                ui.text(
                    &format!("{} · {}", self.server_name, self.map.id.name()),
                    x + 24.0,
                    y + 72.0,
                    15,
                    MUTED,
                );
                let playing = self.team.is_playing();
                if ui.button(
                    if playing {
                        t("Грати далі", "Keep playing")
                    } else {
                        t("ГРАТИ", "PLAY")
                    },
                    x + 24.0,
                    y + 100.0,
                    pw - 48.0,
                    56.0,
                ) {
                    if playing {
                        self.overlay = Overlay::None;
                    } else {
                        self.join_team(None);
                    }
                }
                if ui.button2(
                    if playing {
                        t("Вийти в глядачі", "Go and watch")
                    } else {
                        t("Лише дивитися", "Just watch")
                    },
                    x + 24.0,
                    y + 168.0,
                    pw - 48.0,
                    46.0,
                ) {
                    self.join_team(Some(Team::Spectator));
                    self.overlay = Overlay::None;
                }
                if ui.button2(
                    t("Відключитися", "Disconnect"),
                    x + 24.0,
                    y + 230.0,
                    pw - 48.0,
                    44.0,
                ) {
                    return Some(HudAction::Leave);
                }
                None
            }
            Overlay::Team => {
                soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
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
                    self.tcol(Team::Allies),
                ) {
                    self.join_team(Some(Team::Allies));
                }
                if ui.button_colored(
                    &format!("{} ({b})", Team::Axis.name()),
                    x + 24.0,
                    y + 160.0,
                    pw - 48.0,
                    50.0,
                    self.tcol(Team::Axis),
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
            Overlay::Class => self.draw_loadout(ui, settings),
            Overlay::Pause => {
                soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
                let pw = 420.0;
                let (x, y) = ((w - pw) / 2.0, h * 0.22);
                ui.panel(x, y, pw, 384.0);
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
                let half = (bw - 12.0) / 2.0;
                // before the match: ready or taking a break, right next to «Resume»
                let resume_w = if self.ready.is_some() { half } else { bw };
                if ui.button(
                    t("Продовжити", "Resume"),
                    x + 24.0,
                    y + 90.0,
                    resume_w,
                    46.0,
                ) {
                    self.overlay = self.after_pause();
                }
                if let Some((_, _, me, _)) = self.ready {
                    let (label, c) = if me {
                        (t("Не готовий (перерва)", "Not ready (a break)"), MUTED)
                    } else {
                        (t("Готовий!", "Ready!"), GOOD)
                    };
                    if ui.button_colored(label, x + 36.0 + half, y + 90.0, half, 46.0, c) {
                        self.net.send(toon_shared::protocol::C2S::Ready(!me));
                    }
                }
                // (no classes or sides in the battle royale: characters instead)
                if self.mode == Mode::Br {
                    if ui.button2(t("Персонаж", "Character"), x + 24.0, y + 146.0, bw, 46.0)
                    {
                        self.overlay = Overlay::Skin;
                    }
                } else if ui.button2(t("Спорядження", "Loadout"), x + 24.0, y + 146.0, bw, 46.0)
                {
                    self.overlay = Overlay::Class;
                }
                let team_label = if self.mode != Mode::Br {
                    t("Змінити команду", "Change team")
                } else if self.team.is_playing() {
                    t("Грати / дивитися", "Play / watch")
                } else {
                    t("Грати", "Play")
                };
                // before a battle royale in squads: the lobby, next to it
                let team_w = if self.lobby.is_some() { half } else { bw };
                if ui.button2(team_label, x + 24.0, y + 202.0, team_w, 46.0) {
                    self.overlay = Overlay::Team;
                }
                if self.lobby.is_some() {
                    let asks = self.lobby.as_ref().map_or(0, |l| l.asking_me.len());
                    let label = if asks > 0 {
                        tr!("Загін ({asks} запит)", "Squad ({asks} asking)")
                    } else {
                        t("Загін", "Squad").to_string()
                    };
                    let c = if asks > 0 { GOOD } else { ACCENT };
                    if ui.button_colored(&label, x + 36.0 + half, y + 202.0, half, 46.0, c) {
                        self.overlay = Overlay::Squad;
                    }
                }
                if ui.button2(
                    t("Налаштування", "Settings"),
                    x + 24.0,
                    y + 258.0,
                    half,
                    46.0,
                ) {
                    self.overlay = Overlay::Settings;
                }
                if ui.button2(
                    t("Рейтинг", "Leaderboards"),
                    x + 36.0 + half,
                    y + 258.0,
                    half,
                    46.0,
                ) {
                    self.leaderboards = None;
                    self.net.send(toon_shared::protocol::C2S::Leaderboards);
                    self.overlay = Overlay::Leaders;
                }
                // a battle royale match on: back to the lobby to wait for the next one
                let to_lobby =
                    self.mode == Mode::Br && self.team.is_playing() && self.lobby.is_none();
                if to_lobby
                    && ui.button_colored(
                        t("Вийти в лоббі", "Back to the lobby"),
                        x + 24.0,
                        y + 314.0,
                        half,
                        46.0,
                        ACCENT,
                    )
                {
                    self.net.send(toon_shared::protocol::C2S::ToLobby);
                    self.overlay = Overlay::None;
                    self.killcam = None;
                    self.pending_killcam = None;
                }
                let (dx, dw) = if to_lobby {
                    (x + 36.0 + half, half)
                } else {
                    (x + 24.0, bw)
                };
                if ui.button_colored(
                    t("Відключитися", "Disconnect"),
                    dx,
                    y + 314.0,
                    dw,
                    46.0,
                    BAD,
                ) {
                    return Some(HudAction::Leave);
                }
                None
            }
            Overlay::Squad => {
                self.draw_squad_lobby(ui);
                None
            }
            Overlay::Achievements => {
                self.draw_achievements(ui);
                None
            }
            Overlay::Leaders => {
                soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.55));
                let pw = (w - 40.0).min(1040.0);
                let ph = (h - 40.0).min(640.0);
                let (x, y) = ((w - pw) / 2.0, (h - ph) / 2.0);
                ui.panel(x, y, pw, ph);
                ui.bold(
                    t("Рейтинг сервера", "Server leaderboards"),
                    x + 24.0,
                    y + 42.0,
                    26,
                    ACCENT,
                );
                if ui.button2(t("Назад", "Back"), x + pw - 164.0, y + 18.0, 140.0, 40.0) {
                    self.overlay = self.menu_back();
                    return None;
                }
                ui.text(
                    t(
                        "Рахуються всі режими; K/D — після 5 матчів",
                        "Every mode counts; K/D after 5 matches",
                    ),
                    x + 24.0,
                    y + 66.0,
                    14,
                    MUTED,
                );
                match &self.leaderboards {
                    None => {
                        ui.centered(t("Завантаження…", "Loading…"), w / 2.0, h / 2.0, 18, MUTED)
                    }
                    Some(boards) => {
                        let cols = if pw > 900.0 { 3 } else { 2 };
                        let rows = boards.len().div_ceil(cols);
                        let cw = (pw - 48.0 - (cols - 1) as f32 * 16.0) / cols as f32;
                        let ch = (ph - 100.0 - (rows - 1) as f32 * 14.0) / rows as f32;
                        let line = ((ch - 52.0) / 11.0).clamp(14.0, 20.0);
                        for (i, b) in boards.iter().enumerate() {
                            let bx = x + 24.0 + (i % cols) as f32 * (cw + 16.0);
                            let by = y + 86.0 + (i / cols) as f32 * (ch + 14.0);
                            soft_rect(bx, by, cw, ch, Color::new(0.0, 0.0, 0.0, 0.3));
                            ui.bold(b.kind.name(), bx + 12.0, by + 22.0, 16, ACCENT);
                            if b.rows.is_empty() {
                                ui.text(
                                    t("поки нікого", "nobody yet"),
                                    bx + 12.0,
                                    by + 46.0,
                                    14,
                                    MUTED,
                                );
                            }
                            let shown = (((ch - 52.0) / line).floor() as usize).min(b.rows.len());
                            for (k, (name, v)) in b.rows.iter().take(shown).enumerate() {
                                let ly = by + 44.0 + k as f32 * line;
                                let c = if k == 0 { ACCENT } else { TEXT };
                                let mut nm = format!("{}. {name}", k + 1);
                                let vs = b.kind.format(*v);
                                let vw = ui.width(&vs, 14, false);
                                while nm.chars().count() > 4
                                    && ui.width(&nm, 14, false) > cw - vw - 34.0
                                {
                                    nm.pop();
                                }
                                ui.text(&nm, bx + 12.0, ly, 14, c);
                                ui.text(&vs, bx + cw - 12.0 - vw, ly, 14, c);
                            }
                            if let Some((place, v)) = b.mine {
                                let me = tr!(
                                    "Ти: #{place} ({})",
                                    "You: #{place} ({})",
                                    b.kind.format(v)
                                );
                                ui.text(&me, bx + 12.0, by + ch - 10.0, 13, GOOD);
                            }
                        }
                    }
                }
                None
            }
            Overlay::RobotMode => {
                soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
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
                soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
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
                soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
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
                soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
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
                soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
                let pw = 900.0f32.min(w - 20.0);
                let (x, y) = (
                    (w - pw) / 2.0,
                    (h - crate::menu::controls_h()).max(0.0) / 2.0,
                );
                if crate::menu::controls_page(ui, &mut self.binds_editor, settings, x, y, pw) {
                    self.overlay = Overlay::Settings;
                }
                None
            }
            Overlay::Airstrike => {
                soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.6));
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
                                self.tcol(self.team)
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

/// The achievements: the leaderboard a value comes from, the value to reach, the name and
/// what it takes (Ukrainian, English).
pub(super) type Achievement = (
    toon_shared::protocol::BoardKind,
    f32,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
);

pub(super) fn achievements() -> Vec<Achievement> {
    use toon_shared::protocol::BoardKind as B;
    vec![
        (
            B::Matches,
            1.0,
            "Новобранець",
            "Recruit",
            "Зіграти матч",
            "Play a match",
        ),
        (
            B::Kills,
            1.0,
            "Перша кров",
            "First blood",
            "Перше вбивство",
            "Your first kill",
        ),
        (
            B::Wins,
            1.0,
            "Переможець",
            "Winner",
            "Виграти матч",
            "Win a match",
        ),
        (
            B::Kills,
            25.0,
            "Стрілець",
            "Marksman",
            "25 вбивств",
            "25 kills",
        ),
        (
            B::Longest,
            100.0,
            "Влучне око",
            "Sharp eye",
            "Вбивство за 100 м",
            "A kill from 100 m",
        ),
        (
            B::Matches,
            10.0,
            "Ветеран",
            "Veteran",
            "10 матчів",
            "10 matches",
        ),
        (
            B::Damage,
            1000.0,
            "Руйнівник",
            "Wrecker",
            "1000 урону в BR",
            "1000 damage in BR",
        ),
        (
            B::Kills,
            100.0,
            "Мисливець",
            "Hunter",
            "100 вбивств",
            "100 kills",
        ),
        (B::Wins, 5.0, "Чемпіон", "Champion", "5 перемог", "5 wins"),
        (
            B::Longest,
            300.0,
            "Снайпер",
            "Sniper",
            "Вбивство за 300 м",
            "A kill from 300 m",
        ),
        (
            B::KillDeath,
            1.0,
            "Рівний бій",
            "Even fight",
            "K/D від 1,0",
            "K/D of 1.0",
        ),
        (
            B::KillDeath,
            2.0,
            "Майстер",
            "Master",
            "K/D від 2,0",
            "K/D of 2.0",
        ),
        (
            B::Matches,
            50.0,
            "Старожил",
            "Old hand",
            "50 матчів",
            "50 matches",
        ),
        (
            B::Kills,
            500.0,
            "Легенда",
            "Legend",
            "500 вбивств",
            "500 kills",
        ),
        (
            B::Wins,
            25.0,
            "Непереможний",
            "Unbeatable",
            "25 перемог",
            "25 wins",
        ),
        (
            B::Longest,
            600.0,
            "Соколине око",
            "Hawk eye",
            "Вбивство за 600 м",
            "A kill from 600 m",
        ),
    ]
}

/// The little ping button at the end of a row (lit when that thing is pinged already).
pub(super) fn ping_button(ui: &mut Ui, x: f32, y: f32, on: bool) -> bool {
    let hit = ui.button2("", x, y, 36.0, 32.0);
    let c = if on {
        ACCENT
    } else {
        Color::new(0.75, 0.8, 0.9, 1.0)
    };
    ping_diamond(vec2(x + 18.0, y + 16.0), 9.0, c);
    hit
}

/// The middle mouse button pressed over a row.
pub(super) fn middle_on(ui: &Ui, x: f32, y: f32, w: f32) -> bool {
    let m = ui.mouse;
    is_mouse_button_pressed(MouseButton::Middle)
        && m.x >= x
        && m.x <= x + w
        && m.y >= y
        && m.y <= y + 32.0
}
