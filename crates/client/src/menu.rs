//! Main menu: join a server, host one, browse the LAN, settings.

use crate::netclient::{Browser, ServerInfo};
use crate::settings::Settings;
use crate::ui::*;
use macroquad::prelude::*;
use toon_shared::map::MapId;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Main,
    Host,
    Browser,
    Settings,
}

pub enum MenuAction {
    Connect { address: String },
    Host,
    StopServer,
    Quit,
}

pub struct MenuState {
    pub page: Page,
    pub browser: Browser,
    pub error: Option<String>,
    pub host_info: Option<String>,
    pub lan_ip: Option<String>,
    pub updater: crate::updater::Updater,
    /// Already looked again for an update after a server said we are outdated.
    rechecked: bool,
}

pub fn lan_ip() -> Option<String> {
    let s = std::net::UdpSocket::bind("0.0.0.0:0").ok()?;
    s.connect("8.8.8.8:80").ok()?;
    Some(s.local_addr().ok()?.ip().to_string())
}

impl MenuState {
    /// `updates`: look for game updates (release builds only).
    pub fn new(updates: bool) -> MenuState {
        MenuState {
            page: Page::Main,
            browser: Browser::new(),
            error: None,
            host_info: None,
            lan_ip: lan_ip(),
            updater: crate::updater::Updater::new(updates, toon_shared::consts::RELEASE_BUILD),
            rechecked: false,
        }
    }
}

/// Settings controls shared by the main menu and the in-game pause menu.
pub fn settings_widgets(ui: &mut Ui, s: &mut Settings, x: f32, y: f32, w: f32) {
    s.sensitivity = ui.slider("Чутливість миші", s.sensitivity, 0.2, 8.0, x, y, w, "{}");
    s.fov = ui
        .slider("Поле зору (FOV)", s.fov, 65.0, 110.0, x, y + 56.0, w, "{}°")
        .round();
    s.volume = ui.slider("Гучність", s.volume, 0.0, 1.0, x, y + 112.0, w, "{}");
    ui.toggle("Інвертувати Y", &mut s.invert_y, x, y + 150.0);
    let col2 = x + w * 0.52;
    ui.toggle("Тіні", &mut s.shadows, col2, y + 150.0);
    ui.toggle("FPS і пінг", &mut s.show_fps, x, y + 186.0);
    ui.toggle("Текстури", &mut s.textures, col2, y + 186.0);
    ui.toggle("Контури", &mut s.outlines, x, y + 222.0);
    ui.toggle("Згладжування миші", &mut s.mouse_smoothing, col2, y + 222.0);
    s.render_scale = (ui.slider(
        "Масштаб рендеру (менше = швидше)",
        s.render_scale * 100.0,
        40.0,
        100.0,
        x,
        y + 280.0,
        w,
        "{}%",
    ) / 5.0)
        .round()
        * 5.0
        / 100.0;
    let was = s.vsync;
    ui.toggle("Вертикальна синхронізація", &mut s.vsync, x, y + 318.0);
    if s.vsync != was || s.vsync != crate::VSYNC_AT_START.load(std::sync::atomic::Ordering::Relaxed)
    {
        ui.text(
            "Зміниться після перезапуску гри",
            x + 36.0,
            y + 362.0,
            14,
            ACCENT,
        );
    }
    if !s.vsync {
        const LIMITS: [u32; 6] = [60, 120, 144, 165, 240, 0];
        let i = LIMITS.iter().position(|l| *l == s.fps_limit).unwrap_or(2);
        let label = if s.fps_limit == 0 {
            "без обмеження".to_string()
        } else {
            format!("{} FPS", s.fps_limit)
        };
        let d = ui.stepper("Обмеження FPS", &label, x + w - 260.0, y + 390.0, 260.0);
        if d != 0 {
            s.fps_limit = LIMITS[((i as i32 + d).rem_euclid(LIMITS.len() as i32)) as usize];
        }
    }
}

pub fn draw_menu(
    ui: &mut Ui,
    st: &mut MenuState,
    s: &mut Settings,
    server_running: Option<u16>,
) -> Option<MenuAction> {
    let (w, h) = (screen_width(), screen_height());
    // title
    let title = "TOON WARFARE";
    let tw = ui.width(title, 64, true);
    ui.bold(
        title,
        (w - tw) / 2.0 + 5.0,
        98.0,
        64,
        Color::new(0.0, 0.0, 0.0, 0.6),
    );
    ui.bold(title, (w - tw) / 2.0, 92.0, 64, ACCENT);
    ui.centered("мультяшний мультиплеєрний шутер", w / 2.0, 124.0, 20, TEXT);

    let pw = 560.0f32.min(w - 40.0);
    let x = (w - pw) / 2.0;
    // a little higher on short windows so the tallest page (hosting) still fits
    let y = (h - 575.0).clamp(134.0, 150.0);
    let mut action = None;
    match st.page {
        Page::Main => {
            // a running server adds a status line and its own row of buttons
            ui.panel(
                x,
                y,
                pw,
                if server_running.is_some() {
                    510.0
                } else {
                    470.0
                },
            );
            ui.field(
                1,
                "Твоє ім'я",
                &mut s.name,
                x + 24.0,
                y + 40.0,
                pw - 48.0,
                false,
                20,
            );
            let enter = ui.field(
                2,
                "Адреса сервера (IP:порт)",
                &mut s.address,
                x + 24.0,
                y + 110.0,
                pw - 48.0,
                false,
                64,
            );
            ui.field(
                3,
                "Пароль сервера (якщо є)",
                &mut s.password,
                x + 24.0,
                y + 180.0,
                pw - 48.0,
                true,
                32,
            );
            if ui.button("ПІД'ЄДНАТИСЯ", x + 24.0, y + 232.0, pw - 48.0, 54.0) || enter {
                action = Some(MenuAction::Connect {
                    address: s.address.clone(),
                });
            }
            let bw = (pw - 60.0) / 2.0;
            if ui.button2("Створити сервер", x + 24.0, y + 300.0, bw, 46.0) {
                st.page = Page::Host;
            }
            if ui.button2("Знайти сервери", x + 36.0 + bw, y + 300.0, bw, 46.0) {
                st.page = Page::Browser;
                let recent = s.recent.clone();
                st.browser.refresh(&recent);
            }
            if ui.button2("Налаштування", x + 24.0, y + 356.0, bw, 46.0) {
                st.page = Page::Settings;
            }
            if ui.button_colored("Вихід", x + 36.0 + bw, y + 356.0, bw, 46.0, BAD) {
                action = Some(MenuAction::Quit);
            }
            if let Some(port) = server_running {
                let ip = st.lan_ip.clone().unwrap_or_else(|| "твій-IP".into());
                ui.text(
                    &format!("Твій сервер працює: {ip}:{port}"),
                    x + 24.0,
                    y + 426.0,
                    16,
                    GOOD,
                );
                if let Some(i) = &st.host_info {
                    ui.text(i, x + 24.0, y + 446.0, 13, MUTED);
                }
                let bw = (pw - 60.0) / 2.0;
                if ui.button2("Грати на ньому", x + 24.0, y + 458.0, bw, 36.0) {
                    action = Some(MenuAction::Connect {
                        address: format!("127.0.0.1:{port}"),
                    });
                }
                if ui.button_colored("Зупинити", x + 36.0 + bw, y + 458.0, bw, 36.0, BAD) {
                    action = Some(MenuAction::StopServer);
                }
            } else {
                ui.text(
                    "Порада: створи сервер — друзі під'єднаються за твоїм IP.",
                    x + 24.0,
                    y + 440.0,
                    15,
                    MUTED,
                );
            }
        }
        Page::Host => {
            ui.panel(x, y, pw, 560.0);
            ui.bold("Створити сервер", x + 24.0, y + 40.0, 26, ACCENT);
            let hs = &mut s.host;
            ui.field(
                10,
                "Назва сервера",
                &mut hs.name,
                x + 24.0,
                y + 86.0,
                pw - 48.0,
                false,
                40,
            );
            let mut port = hs.port.to_string();
            ui.field(
                11,
                "Порт (UDP + TCP)",
                &mut port,
                x + 24.0,
                y + 150.0,
                200.0,
                false,
                5,
            );
            if let Ok(p) = port.parse::<u16>() {
                hs.port = p;
            } else if port.is_empty() {
                hs.port = 0;
            }
            ui.field(
                12,
                "RCON-пароль (адмінка)",
                &mut hs.rcon,
                x + 250.0,
                y + 150.0,
                pw - 274.0,
                true,
                32,
            );
            let modes = ["tdm", "sab"];
            let mi = modes.iter().position(|m| *m == hs.mode).unwrap_or(0);
            let mode_name = toon_shared::Mode::parse(modes[mi]).unwrap().name();
            let d = ui.stepper("Режим", mode_name, x + 24.0, y + 214.0, (pw - 60.0) / 2.0);
            if d != 0 {
                hs.mode = modes[((mi as i32 + d).rem_euclid(2)) as usize].into();
            }
            let maps = MapId::ALL;
            let cur = MapId::parse(&hs.map).unwrap_or(MapId::Depot);
            let ci = maps.iter().position(|m| *m == cur).unwrap_or(0);
            let d = ui.stepper(
                "Карта",
                toon_shared::maps::build(cur).name,
                x + 36.0 + (pw - 60.0) / 2.0,
                y + 214.0,
                (pw - 60.0) / 2.0,
            );
            if d != 0 {
                hs.map = maps[((ci as i32 + d).rem_euclid(maps.len() as i32)) as usize]
                    .short()
                    .into();
            }
            hs.bots = ui
                .slider(
                    "Боти (заповнення до)",
                    hs.bots as f32,
                    0.0,
                    16.0,
                    x + 24.0,
                    y + 280.0,
                    pw - 48.0,
                    "{}",
                )
                .round() as usize;
            let skill_label = format!(
                "Складність ботів ({})",
                toon_server::bots::skill_name(hs.bot_skill)
            );
            hs.bot_skill = ui.slider(
                &skill_label,
                hs.bot_skill,
                0.0,
                1.0,
                x + 24.0,
                y + 330.0,
                pw - 48.0,
                "{}",
            );
            hs.max_players = ui
                .slider(
                    "Макс. гравців",
                    hs.max_players as f32,
                    2.0,
                    24.0,
                    x + 24.0,
                    y + 380.0,
                    pw - 48.0,
                    "{}",
                )
                .round() as usize;
            ui.field(
                13,
                "Пароль для входу (порожньо = відкритий)",
                &mut hs.password,
                x + 24.0,
                y + 430.0,
                pw - 48.0,
                true,
                32,
            );
            if ui.button("ЗАПУСТИТИ", x + pw - 220.0, y + 490.0, 196.0, 50.0) {
                action = Some(MenuAction::Host);
            }
            if ui.button2("Назад", x + 24.0, y + 490.0, 140.0, 50.0) {
                st.page = Page::Main;
            }
        }
        Page::Browser => {
            ui.panel(x - 100.0, y, pw + 200.0, 470.0);
            let x = x - 100.0;
            let pw = pw + 200.0;
            ui.bold("Сервери", x + 24.0, y + 40.0, 26, ACCENT);
            ui.text(
                "Локальна мережа + нещодавні",
                x + 150.0,
                y + 40.0,
                15,
                MUTED,
            );
            st.browser.poll();
            let results: Vec<ServerInfo> = st.browser.results.clone();
            if results.is_empty() {
                ui.text(
                    "Шукаю… (переконайся, що сервер запущено і порт відкрито)",
                    x + 24.0,
                    y + 90.0,
                    16,
                    MUTED,
                );
            }
            for (i, r) in results.iter().take(7).enumerate() {
                let ry = y + 64.0 + i as f32 * 48.0;
                let label = format!(
                    "{}{}  ·  {}  ·  {}  ·  {}/{} (+{} ботів)  ·  {} мс",
                    if r.password { "[пароль] " } else { "" },
                    r.name,
                    r.map,
                    r.mode,
                    r.players,
                    r.max,
                    r.bots,
                    r.ping_ms
                );
                if ui.button2(&label, x + 24.0, ry, pw - 48.0, 40.0) {
                    s.address = r.addr.clone();
                    st.page = Page::Main;
                }
            }
            if ui.button("Оновити", x + pw - 200.0, y + 410.0, 176.0, 44.0) {
                let recent = s.recent.clone();
                st.browser.refresh(&recent);
            }
            if ui.button2("Назад", x + 24.0, y + 410.0, 140.0, 44.0) {
                st.page = Page::Main;
            }
        }
        Page::Settings => {
            ui.panel(x, y, pw, 550.0);
            ui.bold("Налаштування", x + 24.0, y + 40.0, 26, ACCENT);
            settings_widgets(ui, s, x + 24.0, y + 90.0, pw - 48.0);
            if ui.button("Зберегти", x + pw - 200.0, y + 490.0, 176.0, 44.0) {
                s.save();
                st.page = Page::Main;
            }
        }
    }
    if let Some(e) = &st.error {
        let ew = ui.width(e, 18, true) + 40.0;
        draw_rectangle(
            (w - ew) / 2.0,
            h - 58.0,
            ew,
            38.0,
            Color::new(0.5, 0.08, 0.08, 0.9),
        );
        ui.centered(e, w / 2.0, h - 32.0, 18, TEXT);
    }
    // a server turned us away for an old version: look for the update again if the
    // first look failed, and show what can be done
    let outdated = st
        .error
        .as_deref()
        .is_some_and(|e| e.starts_with(toon_shared::consts::OUTDATED));
    if outdated && !st.rechecked {
        st.rechecked = true;
        if matches!(
            st.updater.state(),
            crate::updater::State::Off
                | crate::updater::State::CheckFailed(_)
                | crate::updater::State::UpToDate
        ) {
            st.updater.check();
        }
    }
    if st.updater.draw(ui, outdated) {
        s.save();
        toon_server::update::restart();
    }
    ui.text(
        &format!(
            "v{}  ·  протокол {}",
            toon_shared::consts::GAME_VERSION,
            toon_shared::consts::PROTOCOL_VERSION
        ),
        10.0,
        h - 10.0,
        14,
        MUTED,
    );
    action
}
