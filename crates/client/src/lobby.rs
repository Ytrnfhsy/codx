//! The main lobby: where the game starts. Our character (and our squad's) on podiums, the
//! mode to play, the squad from the public list's lobby, and «Play»: the game finds the
//! nearest server with that mode and room for the squad, and the squad follows the leader.

use crate::menu::{MenuAction, MenuState, Page};
use crate::netclient::ServerInfo;
use crate::settings::Settings;
use crate::ui::*;
use macroquad::prelude::*;
use serde_json::json;
use std::time::Instant;
use toon_shared::lang::t;
use toon_shared::tr;

/// The modes, as the public list and the servers name them.
pub const MODES: [&str; 3] = ["br", "tdm", "sab"];

pub fn mode_name(m: &str) -> &'static str {
    match m {
        "tdm" => t("Командний бій", "Team deathmatch"),
        "sab" => t("Саботаж", "Sabotage"),
        _ => t("Королівська битва", "Battle royale"),
    }
}

/// A player's tag: the start of the id the public list knows them by (the same for an
/// installation of the game), so two of the same name can be told apart.
pub fn short(pid: &str) -> &str {
    &pid[..pid.len().min(4)]
}

/// The tag as it follows a name («#a3f9»).
pub fn tag_of(pid: &str) -> String {
    format!(" #{}", short(pid))
}

/// The colour tags are drawn in: quiet, so they do not catch the eye.
const TAG: Color = Color::new(0.6, 0.63, 0.7, 0.55);

/// A line of text and, when `show`, the player's tag after it, smaller and quieter.
#[allow(clippy::too_many_arguments)]
fn text_tagged(ui: &Ui, text: &str, pid: &str, show: bool, x: f32, y: f32, size: u16, c: Color) {
    ui.text(text, x, y, size, c);
    if show {
        let w = ui.width(text, size, false);
        ui.text(&tag_of(pid), x + w, y, size.saturating_sub(3), TAG);
    }
}

/// The names (in lower case) more than one player in the lobby has: their tags show
/// without pointing at them.
fn clashing(v: &crate::party::PartyView) -> Vec<String> {
    let all: Vec<String> = v
        .members
        .iter()
        .map(|m| m.name.to_lowercase())
        .chain(v.invites.iter().map(|i| i.1.to_lowercase()))
        .chain(v.online.iter().map(|o| o.1.to_lowercase()))
        .collect();
    let mut out: Vec<String> = all
        .iter()
        .filter(|n| all.iter().filter(|m| m == n).count() > 1)
        .cloned()
        .collect();
    out.dedup();
    out
}

/// The mouse over this rectangle.
fn hovered(ui: &Ui, x: f32, y: f32, w: f32, h: f32) -> bool {
    let m = ui.mouse;
    m.x >= x && m.x <= x + w && m.y >= y && m.y <= y + h
}

/// How long to wait for the servers' answers before picking.
const SEARCH_MIN: f32 = 1.5;
const SEARCH_MAX: f32 = 3.5;

/// Who stands on the podiums: we in the middle (look id, character), then the squad.
pub fn cast(st: &MenuState, s: &Settings) -> Vec<(u8, u8)> {
    let mut c = vec![(250u8, s.skin)];
    if let Some(p) = &st.party {
        let v = p.view();
        for (k, m) in v.members.iter().filter(|m| m.pid != v.me).enumerate() {
            c.push((251 + k as u8, m.skin));
        }
    }
    c
}

/// The best server for the mode: same version, no password, room for the squad; the
/// closest first, those with people on them before empty ones, and (battle royale) those
/// still in the lobby before ones with a match on.
pub fn pick_server(list: &[ServerInfo], mode: &str, squad: u64) -> Option<String> {
    list.iter()
        .filter(|s| s.mode_id == mode)
        .filter(|s| s.version == toon_shared::consts::PROTOCOL_VERSION as u64)
        .filter(|s| !s.password)
        .filter(|s| s.max == 0 || s.players + squad <= s.max)
        .map(|s| {
            let mut score = s.ping_ms as i64 - (s.players.min(12) as i64) * 15;
            if s.players == 0 {
                score += 150;
            }
            if mode == "br" && s.state != "Warmup" && s.state != "PreRound" {
                score += 400;
            }
            (score, s)
        })
        .min_by_key(|(score, _)| *score)
        .map(|(_, s)| s.addr.clone())
}

pub fn main_lobby(ui: &mut Ui, st: &mut MenuState, s: &mut Settings) -> Option<MenuAction> {
    let (w, h) = (screen_width(), screen_height());
    if st.party.is_none() && !s.master.trim().is_empty() {
        st.party = crate::party::PartyClient::start(&s.master, &s.party_token);
    }
    let view = st.party.as_ref().map(|p| p.view()).unwrap_or_default();
    let online = view.ok;
    let squad: Vec<crate::party::Member> = view.members.clone();
    let leading = !online || view.leading();
    let in_squad = online && squad.len() > 1;
    // the mode: the leader's
    let mode = if in_squad && !view.mode.is_empty() {
        view.mode.clone()
    } else {
        s.play_mode.clone()
    };
    let mut action = None;

    // the title
    ui.centered("TOON WARFARE", w / 2.0, 52.0, 40, ACCENT);
    // the modes
    let tw = 230.0;
    let x0 = w / 2.0 - tw * 1.5 - 8.0;
    for (k, m) in MODES.iter().enumerate() {
        let x = x0 + k as f32 * (tw + 8.0);
        let hit = if *m == mode {
            ui.button(mode_name(m), x, 72.0, tw, 42.0)
        } else {
            ui.button2(mode_name(m), x, 72.0, tw, 42.0)
        };
        if hit && leading && st.searching.is_none() {
            s.play_mode = m.to_string();
            s.save();
            st.nothing_found = None;
            if let Some(p) = &st.party {
                p.send(json!({"op": "mode", "mode": m}));
            }
        }
    }
    if in_squad && !leading {
        ui.centered(
            t(
                "Режим обирає лідер загону",
                "The squad's leader picks the mode",
            ),
            w / 2.0,
            132.0,
            13,
            MUTED,
        );
    }

    // our character: drag it round, the wheel brings it closer
    let side = (w * 0.27).clamp(260.0, 360.0);
    let m = ui.mouse;
    let over = m.x > side + 30.0 && m.x < w - side - 30.0 && m.y > 140.0 && m.y < h - 150.0;
    if is_mouse_button_down(MouseButton::Left) {
        if let Some(last) = st.drag {
            st.turn += (m.x - last.x) * 0.012;
        }
        if st.drag.is_some() || over && is_mouse_button_pressed(MouseButton::Left) {
            st.drag = Some(m);
        }
    } else {
        st.drag = None;
    }
    let wheel = mouse_wheel().1;
    if over && wheel.abs() > 0.01 {
        st.zoom = (st.zoom + wheel.signum() * 0.12).clamp(0.0, 1.0);
    }
    // names over the podiums
    // (a tag shows pointing at the name, or when somebody else has the same name)
    let clash = clashing(&view);
    let mut names: Vec<(String, bool, bool, String)> =
        vec![(s.name.clone(), true, false, String::new())];
    for m in squad.iter().filter(|m| m.pid != view.me) {
        names.push((m.name.clone(), m.ready, m.pid == view.leader, m.pid.clone()));
    }
    if in_squad && view.leader == view.me {
        names[0].2 = true;
    }
    for (k, (name, ready, lead, pid)) in names.iter().enumerate().take(4) {
        let (x, z) = crate::render::LOBBY_SPOTS[k];
        let p = crate::render::lobby_screen_pos(vec3(x, 2.25, z), st.zoom) - vec2(0.0, 46.0);
        let p = vec2(p.x, p.y.max(150.0));
        let show = !pid.is_empty()
            && (hovered(ui, p.x - 90.0, p.y, 180.0, 46.0) || clash.contains(&name.to_lowercase()));
        let tag = if show { tag_of(pid) } else { String::new() };
        let label = if *lead {
            format!("★ {name}")
        } else {
            name.clone()
        };
        let sz = if k == 0 { 20 } else { 16 };
        let nw = ui.width(&label, sz, true);
        let gw = if tag.is_empty() {
            0.0
        } else {
            ui.width(&tag, sz - 4, false)
        };
        let tw = nw + gw;
        soft_rect(
            p.x - tw / 2.0 - 10.0,
            p.y,
            tw + 20.0,
            46.0,
            Color::new(0.0, 0.0, 0.0, 0.55),
        );
        ui.bold(
            &label,
            p.x - tw / 2.0,
            p.y + 22.0,
            sz,
            if k == 0 { ACCENT } else { TEXT },
        );
        if !tag.is_empty() {
            ui.text(&tag, p.x - tw / 2.0 + nw, p.y + 22.0, sz - 4, TAG);
        }
        if k > 0 {
            let (rt, rc) = if *ready {
                (t("ГОТОВИЙ", "READY"), GOOD)
            } else {
                (t("не готовий", "not ready"), MUTED)
            };
            let rw = ui.width(rt, 13, true);
            ui.bold(rt, p.x - rw / 2.0, p.y + 40.0, 13, rc);
        }
    }

    // the squad, on the left
    let (px, py) = (16.0, 150.0);
    let ph = (h - 330.0).max(240.0);
    ui.panel(px, py, side, ph);
    ui.bold(t("Загін", "Squad"), px + 18.0, py + 32.0, 22, ACCENT);
    if let Some(p) = &st.party {
        squad_panel(ui, p, &view, px, py + 50.0, side, py + ph - 10.0);
    } else {
        ui.text(
            t(
                "Каталог не задано в налаштуваннях",
                "No public list in the settings",
            ),
            px + 18.0,
            py + 66.0,
            13,
            MUTED,
        );
    }

    // the rest, on the right
    let rx = w - side - 16.0;
    ui.panel(rx, py, side, 380.0);
    let bw = side - 36.0;
    ui.field(
        41,
        t("Твоє ім'я", "Your name"),
        &mut s.name,
        rx + 18.0,
        py + 42.0,
        bw,
        false,
        20,
    );
    let n_sk = toon_shared::skins::count();
    let d = ui.stepper(
        t("Персонаж", "Character"),
        toon_shared::skins::name(s.skin),
        rx + 18.0,
        py + 112.0,
        bw,
    );
    if d != 0 {
        s.skin = ((s.skin as i32 + d).rem_euclid(n_sk as i32)) as u8;
        s.save();
    }
    let mut y = py + 164.0;
    if ui.button2(
        t("Сервери вручну", "Servers by hand"),
        rx + 18.0,
        y,
        bw,
        42.0,
    ) {
        st.page = Page::Main;
    }
    y += 52.0;
    if ui.button2(
        t("Створити сервер", "Host a server"),
        rx + 18.0,
        y,
        bw,
        42.0,
    ) {
        st.page = Page::Host;
        st.back = Page::Lobby;
    }
    y += 52.0;
    let half = (bw - 10.0) / 2.0;
    if ui.button2(t("Налаштування", "Settings"), rx + 18.0, y, half, 42.0) {
        st.page = Page::Settings;
        st.back = Page::Lobby;
    }
    if ui.button_colored(t("Вихід", "Quit"), rx + 28.0 + half, y, half, 42.0, BAD) {
        action = Some(MenuAction::Quit);
    }
    if online {
        // our tag: the same for this installation, to tell friends so they find us
        ui.text(
            &tr!(
                "Твій тег: #{} — скажи його друзям",
                "Your tag: #{} — tell it to your friends",
                short(&view.me)
            ),
            rx + 18.0,
            py + 346.0,
            13,
            ACCENT,
        );
        ui.text(
            &tr!("У лоббі зараз: {}", "In the lobby now: {}", view.in_lobby),
            rx + 18.0,
            py + 366.0,
            13,
            MUTED,
        );
    }

    // following the leader to the server they found
    if let (Some((addr, seq)), true) = (&view.go, online) {
        match st.followed {
            None => st.followed = Some(*seq),
            Some(f) if f != *seq && !leading => {
                st.followed = Some(*seq);
                crate::netclient::PARTY_TAG.store(view.tag, std::sync::atomic::Ordering::Relaxed);
                return Some(MenuAction::Matched {
                    address: addr.clone(),
                });
            }
            _ => {}
        }
    } else if online && st.followed.is_none() {
        st.followed = Some(0);
    }

    // «Play» (the leader) or «Ready» (the others)
    let (bw, bh) = (340.0, 62.0);
    let (bx, by) = ((w - bw) / 2.0, h - 132.0);
    if let Some((since, m)) = st.searching.clone() {
        st.browser.poll();
        let age = since.elapsed().as_secs_f32();
        let dots = ".".repeat(1 + (age * 3.0) as usize % 3);
        ui.centered(
            &tr!("Шукаємо сервер{dots}", "Looking for a server{dots}"),
            w / 2.0,
            by + 38.0,
            26,
            ACCENT,
        );
        let enough = age > SEARCH_MIN && !st.browser.results.is_empty() || age > SEARCH_MAX;
        if enough {
            st.searching = None;
            let n = squad.len().max(1) as u64;
            match pick_server(&st.browser.results, &m, n) {
                Some(addr) => {
                    let tag = if in_squad { view.tag } else { 0 };
                    crate::netclient::PARTY_TAG.store(tag, std::sync::atomic::Ordering::Relaxed);
                    if let Some(p) = &st.party {
                        p.send(json!({"op": "go", "addr": addr}));
                    }
                    action = Some(MenuAction::Matched { address: addr });
                }
                None => st.nothing_found = Some(m),
            }
        }
    } else if leading {
        let others_ready = squad.iter().filter(|m| m.pid != view.me).all(|m| m.ready);
        let label = if others_ready {
            t("ГРАТИ", "PLAY").to_string()
        } else {
            let r = squad.iter().filter(|m| m.pid != view.me && m.ready).count();
            tr!(
                "ГРАТИ (готові {r}/{})",
                "PLAY ({r}/{} ready)",
                squad.len().saturating_sub(1)
            )
        };
        if ui.button_colored(
            &label,
            bx,
            by,
            bw,
            bh,
            if others_ready { GOOD } else { ACCENT },
        ) {
            st.nothing_found = None;
            st.error = None;
            st.browser.refresh(&s.servers_to_ask(), &s.master);
            st.searching = Some((Instant::now(), mode.clone()));
        }
        ui.centered(mode_name(&mode), w / 2.0, by + bh + 22.0, 15, TEXT);
    } else {
        let me_ready = squad.iter().any(|m| m.pid == view.me && m.ready);
        let (label, c) = if me_ready {
            (t("НЕ ГОТОВИЙ", "NOT READY"), MUTED)
        } else {
            (t("ГОТОВИЙ", "READY"), GOOD)
        };
        if ui.button_colored(label, bx, by, bw, bh, c) {
            if let Some(p) = &st.party {
                p.send(json!({"op": "ready", "ready": !me_ready}));
            }
        }
        ui.centered(
            &tr!(
                "{} · гру почне лідер загону",
                "{} · the squad's leader starts the game",
                mode_name(&mode)
            ),
            w / 2.0,
            by + bh + 22.0,
            15,
            TEXT,
        );
    }
    // nothing for that mode: a game of our own, with bots
    if let Some(m) = st.nothing_found.clone() {
        let text = tr!(
            "Немає вільного сервера з режимом «{}»",
            "No server with room for «{}»",
            mode_name(&m)
        );
        ui.centered(&text, w / 2.0, by - 46.0, 16, BAD);
        if ui.button2(
            t(
                "Грати з ботами на своєму сервері",
                "Play with bots on a server of your own",
            ),
            (w - 380.0) / 2.0,
            by - 38.0,
            380.0,
            32.0,
        ) {
            st.nothing_found = None;
            s.host.mode = m.clone();
            if m == "br" {
                s.host.map = "island".into();
            } else if s.host.map == "island" {
                s.host.map = "depot".into();
            }
            action = Some(MenuAction::Host);
        }
    }
    action
}

/// The squad panel: who is in it (and ready), invitations, and the others in the lobby
/// with «Invite».
fn squad_panel(
    ui: &mut Ui,
    p: &crate::party::PartyClient,
    v: &crate::party::PartyView,
    x: f32,
    top: f32,
    pw: f32,
    bottom: f32,
) {
    let inner = pw - 36.0;
    let mut y = top;
    let clash = clashing(v);
    if !v.ok {
        let e = v.error.clone().unwrap_or_default();
        ui.text(
            t(
                "Каталог недоступний — грай через «Сервери вручну»",
                "The public list is down — use «Servers by hand»",
            ),
            x + 18.0,
            y + 14.0,
            12,
            BAD,
        );
        if !e.is_empty() {
            let mut e = e;
            e.truncate(60);
            ui.text(&e, x + 18.0, y + 30.0, 11, MUTED);
        }
        return;
    }
    for m in &v.members {
        soft_rect(x + 18.0, y, inner, 34.0, Color::new(0.0, 0.0, 0.0, 0.3));
        let show = hovered(ui, x + 18.0, y, inner, 34.0) || clash.contains(&m.name.to_lowercase());
        let mut name = m.name.clone();
        if m.pid == v.leader && v.members.len() > 1 {
            name = format!("★ {name}");
        }
        if m.pid == v.me {
            name = tr!("{name} (ти)", "{name} (you)");
        }
        let c = if m.pid == v.me { GOOD } else { TEXT };
        text_tagged(ui, &name, &m.pid, show, x + 28.0, y + 23.0, 15, c);
        if m.pid != v.leader {
            let (rt, rc) = if m.ready {
                (t("готовий", "ready"), GOOD)
            } else {
                (t("ні", "no"), MUTED)
            };
            let rw = ui.width(rt, 13, true);
            ui.bold(rt, x + 18.0 + inner - rw - 10.0, y + 22.0, 13, rc);
        }
        y += 40.0;
    }
    if v.members.len() > 1
        && ui.button_colored(
            t("Вийти із загону", "Leave the squad"),
            x + 18.0,
            y,
            inner,
            32.0,
            BAD,
        )
    {
        p.send(json!({"op": "leave"}));
    }
    if v.members.len() > 1 {
        y += 40.0;
    }
    let small = (inner * 0.3).min(100.0);
    if !v.invites.is_empty() {
        ui.bold(
            t("Кличуть у загін", "Invitations"),
            x + 18.0,
            y + 16.0,
            15,
            GOOD,
        );
        y += 24.0;
        for (pid, name) in v.invites.iter().take(3) {
            soft_rect(x + 18.0, y, inner, 34.0, Color::new(0.1, 0.3, 0.1, 0.35));
            let show =
                hovered(ui, x + 18.0, y, inner, 34.0) || clash.contains(&name.to_lowercase());
            text_tagged(ui, name, pid, show, x + 28.0, y + 23.0, 14, TEXT);
            if ui.button_colored(
                "✓",
                x + 18.0 + inner - 2.0 * small - 6.0,
                y + 2.0,
                small,
                30.0,
                GOOD,
            ) {
                p.send(json!({"op": "accept", "from": pid}));
            }
            if ui.button_colored("✕", x + 18.0 + inner - small, y + 2.0, small, 30.0, MUTED) {
                p.send(json!({"op": "decline", "from": pid}));
            }
            y += 40.0;
        }
    }
    ui.bold(t("У лоббі", "In the lobby"), x + 18.0, y + 18.0, 15, TEXT);
    y += 26.0;
    if v.online.is_empty() {
        ui.text(
            t("Поки нікого", "Nobody yet"),
            x + 28.0,
            y + 16.0,
            13,
            MUTED,
        );
    }
    let full = v.members.len() >= toon_server::party::PARTY_MAX;
    for (pid, name, size) in &v.online {
        if y + 34.0 > bottom {
            break;
        }
        soft_rect(x + 18.0, y, inner, 34.0, Color::new(0.0, 0.0, 0.0, 0.3));
        let show = hovered(ui, x + 18.0, y, inner, 34.0) || clash.contains(&name.to_lowercase());
        let label = if *size > 1 {
            tr!("{name} (загін {size})", "{name} (squad of {size})")
        } else {
            name.clone()
        };
        text_tagged(ui, &label, pid, show, x + 28.0, y + 23.0, 14, TEXT);
        let bw = (inner * 0.4).min(130.0);
        if !full
            && ui.button_colored(
                t("Запросити", "Invite"),
                x + 18.0 + inner - bw,
                y + 2.0,
                bw,
                30.0,
                ACCENT,
            )
        {
            p.send(json!({"op": "invite", "to": pid}));
        }
        y += 40.0;
    }
}
