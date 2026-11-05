//! Main menu: join a server, host one, browse the LAN, settings.

use crate::netclient::{Browser, ServerInfo};
use crate::settings::{server_key, Settings};
use crate::ui::*;
use macroquad::prelude::*;
use toon_shared::lang::{t, Lang};
use toon_shared::map::MapId;
use toon_shared::tr;

#[derive(Clone, Copy, PartialEq)]
pub enum Page {
    Main,
    Host,
    Browser,
    Settings,
    Controls,
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
    /// The "add a favourite" field on the server list.
    pub fav_input: String,
    pub binds_editor: crate::binds::Editor,
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
            fav_input: String::new(),
            binds_editor: Default::default(),
        }
    }
}

/// Settings controls shared by the main menu and the in-game pause menu.
pub fn settings_widgets(ui: &mut Ui, s: &mut Settings, x: f32, y: f32, w: f32) {
    s.sensitivity = ui.slider(
        t("Чутливість миші", "Mouse sensitivity"),
        s.sensitivity,
        0.2,
        8.0,
        x,
        y,
        w,
        "{}",
    );
    s.fov = ui
        .slider(
            t("Поле зору (FOV)", "Field of view (FOV)"),
            s.fov,
            65.0,
            110.0,
            x,
            y + 56.0,
            w,
            "{}°",
        )
        .round();
    s.volume = ui.slider(
        t("Гучність", "Volume"),
        s.volume,
        0.0,
        1.0,
        x,
        y + 112.0,
        w,
        "{}",
    );
    ui.toggle(
        t("Інвертувати Y", "Invert Y"),
        &mut s.invert_y,
        x,
        y + 150.0,
    );
    let col2 = x + w * 0.52;
    ui.toggle(t("Тіні", "Shadows"), &mut s.shadows, col2, y + 150.0);
    ui.toggle(
        t("FPS і пінг", "FPS and ping"),
        &mut s.show_fps,
        x,
        y + 186.0,
    );
    ui.toggle(t("Текстури", "Textures"), &mut s.textures, col2, y + 186.0);
    ui.toggle(t("Контури", "Outlines"), &mut s.outlines, x, y + 222.0);
    ui.toggle(
        t("Згладжування миші", "Mouse smoothing"),
        &mut s.mouse_smoothing,
        col2,
        y + 222.0,
    );
    s.render_scale = (ui.slider(
        t(
            "Масштаб рендеру (менше = швидше)",
            "Render scale (lower = faster)",
        ),
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
    ui.toggle(
        t("Вертикальна синхронізація", "Vertical sync"),
        &mut s.vsync,
        x,
        y + 318.0,
    );
    if s.vsync != was || s.vsync != crate::VSYNC_AT_START.load(std::sync::atomic::Ordering::Relaxed)
    {
        ui.text(
            t(
                "Зміниться після перезапуску гри",
                "Takes effect after restarting the game",
            ),
            x + 36.0,
            y + 362.0,
            14,
            ACCENT,
        );
    }
    let li = Lang::ALL.iter().position(|l| *l == s.lang).unwrap_or(0);
    // in the panel's title row, top right
    let d = ui.stepper(
        "Мова / Language",
        s.lang.name(),
        x + w - 230.0,
        y - 58.0,
        230.0,
    );
    if d != 0 {
        s.lang = Lang::ALL[((li as i32 + d).rem_euclid(Lang::ALL.len() as i32)) as usize];
        toon_shared::lang::set(s.lang);
    }
    if !s.vsync {
        const LIMITS: [u32; 6] = [60, 120, 144, 165, 240, 0];
        let i = LIMITS.iter().position(|l| *l == s.fps_limit).unwrap_or(2);
        let label = if s.fps_limit == 0 {
            t("без обмеження", "unlimited").to_string()
        } else {
            format!("{} FPS", s.fps_limit)
        };
        let d = ui.stepper(t("Обмеження FPS", "FPS limit"), &label, x, y + 390.0, 260.0);
        if d != 0 {
            s.fps_limit = LIMITS[((i as i32 + d).rem_euclid(LIMITS.len() as i32)) as usize];
        }
    }
}

/// A ★ button: gold when the server is a favourite, a hollow ☆ when not.
fn star_button(ui: &mut Ui, fav: bool, x: f32, y: f32, w: f32, h: f32) -> bool {
    let clicked = ui.button2("", x, y, w, h);
    let (star, color) = if fav { ("★", ACCENT) } else { ("☆", MUTED) };
    ui.centered(star, x + w / 2.0, y + h / 2.0 + 9.0, 26, color);
    clicked
}

/// The Controls page in a panel at (x, y), `pw` wide (its height is `CONTROLS_H`).
/// Returns true when the player is done with it.
pub fn controls_page(
    ui: &mut Ui,
    ed: &mut crate::binds::Editor,
    s: &mut Settings,
    x: f32,
    y: f32,
    pw: f32,
) -> bool {
    ui.panel(x, y, pw, CONTROLS_H);
    ui.bold(t("Клавіші", "Controls"), x + 24.0, y + 40.0, 26, ACCENT);
    if ed.draw(ui, &mut s.binds, x + 24.0, y + 64.0, pw - 48.0) {
        crate::binds::use_binds(&s.binds);
        s.save();
    }
    let by = y + CONTROLS_H - 60.0;
    if !ed.listening() && ui.button2(t("Скинути все", "Reset all"), x + 24.0, by, 176.0, 44.0)
    {
        s.binds = Default::default();
        crate::binds::use_binds(&s.binds);
        s.save();
    }
    if !ed.listening() && ui.button(t("Готово", "Done"), x + pw - 200.0, by, 176.0, 44.0) {
        ed.close();
        s.save();
        return true;
    }
    false
}

/// Height of the Controls panel.
pub const CONTROLS_H: f32 = 580.0;

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
    ui.centered(
        t(
            "мультяшний мультиплеєрний шутер",
            "a cartoon multiplayer shooter",
        ),
        w / 2.0,
        124.0,
        20,
        TEXT,
    );

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
                t("Твоє ім'я", "Your name"),
                &mut s.name,
                x + 24.0,
                y + 40.0,
                pw - 48.0,
                false,
                20,
            );
            let enter = ui.field(
                2,
                t("Адреса сервера (IP:порт)", "Server address (IP:port)"),
                &mut s.address,
                x + 24.0,
                y + 110.0,
                pw - 48.0 - 54.0,
                false,
                64,
            );
            // ★ keeps this address among the favourites
            let fav = s.is_favorite(&s.address);
            if star_button(ui, fav, x + pw - 24.0 - 44.0, y + 110.0, 44.0, 36.0) {
                let a = s.address.clone();
                s.toggle_favorite(&a);
                s.save();
            }
            ui.field(
                3,
                t("Пароль сервера (якщо є)", "Server password (if any)"),
                &mut s.password,
                x + 24.0,
                y + 180.0,
                pw - 48.0,
                true,
                32,
            );
            if ui.button(
                t("ПІД'ЄДНАТИСЯ", "CONNECT"),
                x + 24.0,
                y + 232.0,
                pw - 48.0,
                54.0,
            ) || enter
            {
                action = Some(MenuAction::Connect {
                    address: s.address.clone(),
                });
            }
            let bw = (pw - 60.0) / 2.0;
            if ui.button2(
                t("Створити сервер", "Host a server"),
                x + 24.0,
                y + 300.0,
                bw,
                46.0,
            ) {
                st.page = Page::Host;
            }
            if ui.button2(
                t("Знайти сервери", "Find servers"),
                x + 36.0 + bw,
                y + 300.0,
                bw,
                46.0,
            ) {
                st.page = Page::Browser;
                st.browser.refresh(&s.servers_to_ask());
            }
            if ui.button2(t("Налаштування", "Settings"), x + 24.0, y + 356.0, bw, 46.0)
            {
                st.page = Page::Settings;
            }
            if ui.button_colored(t("Вихід", "Quit"), x + 36.0 + bw, y + 356.0, bw, 46.0, BAD) {
                action = Some(MenuAction::Quit);
            }
            if let Some(port) = server_running {
                let ip = st
                    .lan_ip
                    .clone()
                    .unwrap_or_else(|| t("твій-IP", "your-IP").into());
                ui.text(
                    &tr!(
                        "Твій сервер працює: {ip}:{port}",
                        "Your server is running: {ip}:{port}"
                    ),
                    x + 24.0,
                    y + 426.0,
                    16,
                    GOOD,
                );
                if let Some(i) = &st.host_info {
                    ui.text(i, x + 24.0, y + 446.0, 13, MUTED);
                }
                let bw = (pw - 60.0) / 2.0;
                if ui.button2(
                    t("Грати на ньому", "Play on it"),
                    x + 24.0,
                    y + 458.0,
                    bw,
                    36.0,
                ) {
                    action = Some(MenuAction::Connect {
                        address: format!("127.0.0.1:{port}"),
                    });
                }
                if ui.button_colored(
                    t("Зупинити", "Stop"),
                    x + 36.0 + bw,
                    y + 458.0,
                    bw,
                    36.0,
                    BAD,
                ) {
                    action = Some(MenuAction::StopServer);
                }
            } else {
                ui.text(
                    t(
                        "Порада: створи сервер — друзі під'єднаються за твоїм IP.",
                        "Tip: host a server and friends can join by your IP.",
                    ),
                    x + 24.0,
                    y + 440.0,
                    15,
                    MUTED,
                );
            }
        }
        Page::Host => {
            ui.panel(x, y, pw, 560.0);
            ui.bold(
                t("Створити сервер", "Host a server"),
                x + 24.0,
                y + 40.0,
                26,
                ACCENT,
            );
            let hs = &mut s.host;
            ui.field(
                10,
                t("Назва сервера", "Server name"),
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
                t("Порт (UDP + TCP)", "Port (UDP + TCP)"),
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
                t("RCON-пароль (адмінка)", "RCON password (admin)"),
                &mut hs.rcon,
                x + 250.0,
                y + 150.0,
                pw - 274.0,
                true,
                32,
            );
            let modes = ["tdm", "sab", "br"];
            let mi = modes.iter().position(|m| *m == hs.mode).unwrap_or(0);
            let mode_name = toon_shared::Mode::parse(modes[mi]).unwrap().name();
            let d = ui.stepper(
                t("Режим", "Mode"),
                mode_name,
                x + 24.0,
                y + 214.0,
                (pw - 60.0) / 2.0,
            );
            if d != 0 {
                hs.mode = modes[((mi as i32 + d).rem_euclid(modes.len() as i32)) as usize].into();
                // the battle royale is played on the big island
                if hs.mode == "br" {
                    hs.map = MapId::Island.short().into();
                }
            }
            let maps = MapId::EVERY;
            let cur = MapId::parse(&hs.map).unwrap_or(MapId::Depot);
            let ci = maps.iter().position(|m| *m == cur).unwrap_or(0);
            let d = ui.stepper(
                t("Карта", "Map"),
                cur.name(),
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
                    t("Боти (заповнення до)", "Bots (fill up to)"),
                    hs.bots as f32,
                    0.0,
                    16.0,
                    x + 24.0,
                    y + 280.0,
                    pw - 48.0,
                    "{}",
                )
                .round() as usize;
            let skill = toon_server::bots::skill_name(hs.bot_skill);
            let skill_label = tr!("Складність ботів ({skill})", "Bot difficulty ({skill})");
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
                    t("Макс. гравців", "Max players"),
                    hs.max_players as f32,
                    2.0,
                    toon_shared::consts::MAX_PLAYERS as f32,
                    x + 24.0,
                    y + 380.0,
                    pw - 48.0,
                    "{}",
                )
                .round() as usize;
            ui.field(
                13,
                t(
                    "Пароль для входу (порожньо = відкритий)",
                    "Join password (empty = open)",
                ),
                &mut hs.password,
                x + 24.0,
                y + 430.0,
                pw - 48.0,
                true,
                32,
            );
            if ui.button(
                t("ЗАПУСТИТИ", "START"),
                x + pw - 220.0,
                y + 490.0,
                196.0,
                50.0,
            ) {
                action = Some(MenuAction::Host);
            }
            if ui.button2(t("Назад", "Back"), x + 24.0, y + 490.0, 140.0, 50.0) {
                st.page = Page::Main;
            }
        }
        Page::Browser => {
            ui.panel(x - 100.0, y, pw + 200.0, 530.0);
            let x = x - 100.0;
            let pw = pw + 200.0;
            ui.bold(t("Сервери", "Servers"), x + 24.0, y + 40.0, 26, ACCENT);
            ui.text(
                t(
                    "★ обрані · локальна мережа · нещодавні",
                    "★ favourites · local network · recent",
                ),
                x + 190.0,
                y + 40.0,
                15,
                MUTED,
            );
            st.browser.poll();
            let results: Vec<ServerInfo> = st.browser.results.clone();
            // favourites first (also those that do not answer, so they can be removed)
            let mut rows: Vec<(String, Option<ServerInfo>)> = Vec::new();
            for f in &s.favorites {
                let found = results
                    .iter()
                    .find(|r| server_key(&r.addr) == server_key(f));
                if found.is_some() || st.browser.age() > 1.5 {
                    rows.push((f.clone(), found.cloned()));
                }
            }
            for r in &results {
                if !s.is_favorite(&r.addr) {
                    rows.push((r.addr.clone(), Some(r.clone())));
                }
            }
            if rows.is_empty() {
                ui.text(
                    t(
                        "Шукаю… (переконайся, що сервер запущено і порт відкрито)",
                        "Searching… (make sure the server is running and the port is open)",
                    ),
                    x + 24.0,
                    y + 90.0,
                    16,
                    MUTED,
                );
            }
            let mut toggle = None;
            for (i, (addr, info)) in rows.iter().take(7).enumerate() {
                let ry = y + 64.0 + i as f32 * 48.0;
                let label = match info {
                    Some(r) => {
                        let lock = if r.password {
                            t("[пароль] ", "[password] ")
                        } else {
                            ""
                        };
                        let (name, map, mode) = (&r.name, r.map_name(), r.mode_name());
                        let (players, max, bots, ping) = (r.players, r.max, r.bots, r.ping_ms);
                        tr!(
                            "{lock}{name}  ·  {map}  ·  {mode}  ·  {players}/{max} (+{bots} ботів)  ·  {ping} мс",
                            "{lock}{name}  ·  {map}  ·  {mode}  ·  {players}/{max} (+{bots} bots)  ·  {ping} ms"
                        )
                    }
                    None => tr!("{addr}  ·  не відповідає", "{addr}  ·  not answering"),
                };
                if ui.button2(&label, x + 24.0, ry, pw - 48.0 - 54.0, 40.0) {
                    s.address = addr.clone();
                    st.page = Page::Main;
                }
                if star_button(
                    ui,
                    s.is_favorite(addr),
                    x + pw - 24.0 - 44.0,
                    ry,
                    44.0,
                    40.0,
                ) {
                    toggle = Some(addr.clone());
                }
            }
            if let Some(a) = toggle {
                s.toggle_favorite(&a);
                s.save();
            }
            // add one by address (an IP or a domain)
            let add_w = 170.0;
            let enter = ui.field(
                30,
                t(
                    "Додати сервер в обрані (IP або домен)",
                    "Add a favourite server (IP or domain)",
                ),
                &mut st.fav_input,
                x + 24.0,
                y + 428.0,
                pw - 48.0 - add_w - 12.0,
                false,
                64,
            );
            let add = ui.button2(
                t("★ Додати", "★ Add"),
                x + pw - 24.0 - add_w,
                y + 428.0,
                add_w,
                36.0,
            );
            if (add || enter) && !st.fav_input.trim().is_empty() {
                let a = st.fav_input.trim().to_string();
                if !s.is_favorite(&a) {
                    s.toggle_favorite(&a);
                    s.save();
                }
                st.fav_input.clear();
                st.browser.refresh(&s.servers_to_ask());
            }
            if ui.button(
                t("Оновити", "Refresh"),
                x + pw - 200.0,
                y + 474.0,
                176.0,
                44.0,
            ) {
                st.browser.refresh(&s.servers_to_ask());
            }
            if ui.button2(t("Назад", "Back"), x + 24.0, y + 474.0, 140.0, 44.0) {
                st.page = Page::Main;
            }
        }
        Page::Settings => {
            ui.panel(x, y, pw, 550.0);
            ui.bold(
                t("Налаштування", "Settings"),
                x + 24.0,
                y + 40.0,
                26,
                ACCENT,
            );
            settings_widgets(ui, s, x + 24.0, y + 90.0, pw - 48.0);
            if ui.button2(t("Клавіші", "Controls"), x + 24.0, y + 490.0, 176.0, 44.0) {
                st.page = Page::Controls;
            }
            if ui.button(
                t("Зберегти", "Save"),
                x + pw - 200.0,
                y + 490.0,
                176.0,
                44.0,
            ) {
                s.save();
                st.page = Page::Main;
            }
        }
        Page::Controls => {
            let pw = 760.0f32.min(w - 20.0);
            let (cx, cy) = ((w - pw) / 2.0, (h - CONTROLS_H).max(0.0) / 2.0);
            // Esc leaves the page unless it was only cancelling a key being chosen
            let esc = is_key_pressed(KeyCode::Escape) && !st.binds_editor.listening();
            let back = controls_page(ui, &mut st.binds_editor, s, cx, cy, pw) || esc;
            if back {
                st.binds_editor.close();
                st.page = Page::Settings;
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
    let outdated = st.error.as_deref().is_some_and(|e| {
        let (uk, en) = toon_shared::consts::OUTDATED;
        e.starts_with(uk) || e.starts_with(en)
    });
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
            "v{}  ·  {} {}",
            toon_shared::consts::GAME_VERSION,
            t("протокол", "protocol"),
            toon_shared::consts::PROTOCOL_VERSION
        ),
        10.0,
        h - 10.0,
        14,
        MUTED,
    );
    action
}
