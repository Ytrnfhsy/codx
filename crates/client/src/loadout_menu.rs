//! The loadout screen (team deathmatch and sabotage): every gun at once on a grid, one
//! column per kind, a pistol to go with it, the perks below; one click picks, «FIGHT»
//! (or Enter) sends it. What was picked is remembered for next time.

use crate::game::GameClient;
use crate::hud::HudAction;
use crate::settings::Settings;
use crate::ui::*;
use macroquad::prelude::*;
use toon_shared::lang::t;
use toon_shared::tr;
use toon_shared::weapons::{Perk1, Perk2, Perk3, WeaponClass, WeaponId};

/// The columns: a kind of gun and its title.
fn columns() -> [(WeaponClass, &'static str); 6] {
    [
        (
            WeaponClass::Rifle,
            t("Штурмові гвинтівки", "Assault rifles"),
        ),
        (WeaponClass::Smg, t("Пістолети-кулемети", "SMGs")),
        (WeaponClass::Lmg, t("Кулемети", "LMGs")),
        (WeaponClass::Shotgun, t("Дробовики", "Shotguns")),
        (WeaponClass::Sniper, t("Снайперські", "Sniper rifles")),
        (WeaponClass::Pistol, t("Пістолети", "Pistols")),
    ]
}

fn guns_of(c: WeaponClass) -> Vec<WeaponId> {
    WeaponId::all().filter(|w| w.def().class == c).collect()
}

fn mouse_in(m: Vec2, x: f32, y: f32, w: f32, h: f32) -> bool {
    m.x >= x && m.x <= x + w && m.y >= y && m.y <= y + h
}

/// The gun seen from the side in its own colours, with a dark outline, fitted into a box
/// centred on (cx, cy).
pub fn draw_gun(w: WeaponId, cx: f32, cy: f32, max_w: f32, max_h: f32) {
    let parts = crate::models::gun_parts(w);
    let (mut z0, mut z1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for p in &parts {
        z0 = z0.min(-(p.c.z + p.h.z));
        z1 = z1.max(-(p.c.z - p.h.z));
        y0 = y0.min(p.c.y - p.h.y);
        y1 = y1.max(p.c.y + p.h.y);
    }
    let s = (max_w / (z1 - z0)).min(max_h / (y1 - y0));
    let (ox, oy) = (cx - (z1 - z0) * s * 0.5, cy - (y1 - y0) * s * 0.5);
    let rects: Vec<(Rect, [u8; 3], f32)> = parts
        .iter()
        .map(|p| {
            (
                Rect::new(
                    ox + (-(p.c.z + p.h.z) - z0) * s,
                    oy + (y1 - (p.c.y + p.h.y)) * s,
                    (p.h.z * 2.0 * s).max(1.5),
                    (p.h.y * 2.0 * s).max(1.5),
                ),
                p.col,
                p.c.x,
            )
        })
        .collect();
    let ink = (s * 0.012).clamp(1.5, 3.0);
    for (r, _, _) in &rects {
        draw_rectangle(
            r.x - ink,
            r.y - ink,
            r.w + 2.0 * ink,
            r.h + 2.0 * ink,
            Color::from_rgba(12, 12, 18, 255),
        );
    }
    let mut order: Vec<&(Rect, [u8; 3], f32)> = rects.iter().collect();
    order.sort_by(|a, b| a.2.total_cmp(&b.2));
    for (r, c, _) in order {
        // a touch brighter than in the hand: the screen is dark
        let lit = c.map(|v| (v as f32 * 1.35 + 18.0).min(255.0) as u8);
        draw_rectangle(
            r.x,
            r.y,
            r.w,
            r.h,
            Color::from_rgba(lit[0], lit[1], lit[2], 255),
        );
        let top = lit.map(|v| (v as f32 * 1.12 + 20.0).min(255.0) as u8);
        draw_rectangle(
            r.x,
            r.y,
            r.w,
            (r.h * 0.25).max(1.0),
            Color::from_rgba(top[0], top[1], top[2], 255),
        );
    }
}

impl GameClient {
    /// The loadout screen. Returns an action for the HUD (none so far).
    pub fn draw_loadout(&mut self, ui: &mut Ui, settings: &mut Settings) -> Option<HudAction> {
        let (w, h) = (screen_width(), screen_height());
        let k = (w / 1920.0).min(h / 1080.0).clamp(0.55, 1.6);
        let fs = |px: f32| (px * k).round().max(10.0) as u16;
        let mut pick = self.pick.unwrap_or_else(|| settings.loadout());
        soft_rect(0.0, 0.0, w, h, Color::new(0.05, 0.06, 0.1, 0.93));

        // ---- the header: title, what this is, the loadout so far and the fight button
        let mx = 60.0 * k;
        ui.bold(t("СПОРЯДЖЕННЯ", "LOADOUT"), mx, 88.0 * k, fs(44.0), ACCENT);
        ui.text(
            &format!(
                "{} · {} · {}",
                self.map.id.name(),
                self.mode.name(),
                t(
                    "обери основну зброю, пістолет і перки",
                    "pick a gun, a pistol and perks"
                )
            ),
            mx + 2.0,
            122.0 * k,
            fs(20.0),
            MUTED,
        );
        let (bx, by, bw, bh) = (w - mx - 720.0 * k, 36.0 * k, 720.0 * k, 84.0 * k);
        round_rect(bx, by, bw, bh, 16.0 * k, PANEL);
        ui.text(
            t("Твій набір", "Your loadout"),
            bx + 24.0 * k,
            by + 28.0 * k,
            fs(16.0),
            MUTED,
        );
        let summary = format!(
            "{} · {}",
            pick.primary.def().name,
            pick.secondary.def().name
        );
        ui.bold(
            &ui.fit(&summary, fs(21.0), true, bw - 300.0 * k),
            bx + 24.0 * k,
            by + 60.0 * k,
            fs(21.0),
            TEXT,
        );
        let fight = ui.button(
            t("В БІЙ ⏎", "FIGHT ⏎"),
            bx + bw - 226.0 * k,
            by + 14.0 * k,
            210.0 * k,
            56.0 * k,
        );
        let team = ui.button2(
            t("Команда", "Team"),
            bx + bw - 390.0 * k,
            by + 14.0 * k,
            150.0 * k,
            56.0 * k,
        );

        // ---- the grid
        let top = 150.0 * k;
        let gap = 18.0 * k;
        let colw = (w - 2.0 * mx - gap * 5.0) / 6.0;
        let perk_h = 150.0 * k;
        let foot = 44.0 * k;
        let head_h = 52.0 * k;
        let cards_h = h - top - head_h - 14.0 * k - perk_h - gap - foot;
        let cardh = ((cards_h - 2.0 * 14.0 * k) / 3.0).min(220.0 * k);
        // stats against the best of every gun on this server
        let stat = |w: WeaponId| {
            let d = self.tuning.weapon(w);
            (d.damage_max * d.pellets.max(1) as f32, d.rpm, d.range_far)
        };
        let all: Vec<(f32, f32, f32)> = WeaponId::all().map(stat).collect();
        let most = all.iter().fold((1.0f32, 1.0f32, 1.0f32), |m, s| {
            (m.0.max(s.0), m.1.max(s.1), m.2.max(s.2))
        });
        // keys: a column's number, then a gun's number in it
        let digit = [
            KeyCode::Key1,
            KeyCode::Key2,
            KeyCode::Key3,
            KeyCode::Key4,
            KeyCode::Key5,
            KeyCode::Key6,
        ]
        .iter()
        .position(|&key| is_key_pressed(key));
        let mut chosen_by_key = None;
        if let Some(d) = digit {
            match self.pick_col {
                None => self.pick_col = Some(d),
                Some(c) => {
                    chosen_by_key = guns_of(columns()[c].0).get(d).copied();
                    self.pick_col = None;
                }
            }
        }
        for (ci, (class, title)) in columns().iter().enumerate() {
            let x = mx + ci as f32 * (colw + gap);
            let pistols = *class == WeaponClass::Pistol;
            let keyed = self.pick_col == Some(ci);
            round_rect(
                x,
                top,
                colw,
                head_h,
                12.0 * k,
                if keyed { PANEL2 } else { PANEL },
            );
            let badge = if pistols {
                Color::new(0.47, 0.75, 1.0, 1.0)
            } else {
                ACCENT
            };
            round_rect(
                x + 10.0 * k,
                top + 10.0 * k,
                32.0 * k,
                32.0 * k,
                8.0 * k,
                badge,
            );
            let n = format!("{}", ci + 1);
            let nw = ui.width(&n, fs(20.0), true);
            ui.bold(
                &n,
                x + 26.0 * k - nw / 2.0,
                top + 33.0 * k,
                fs(20.0),
                DARK_TEXT,
            );
            ui.bold(
                &ui.fit(title, fs(19.0), true, colw - 64.0 * k),
                x + 54.0 * k,
                top + 33.0 * k,
                fs(19.0),
                TEXT,
            );
            for (gi, gun) in guns_of(*class).into_iter().enumerate() {
                let y = top + head_h + 14.0 * k + gi as f32 * (cardh + 14.0 * k);
                let chosen = if pistols {
                    pick.secondary == gun
                } else {
                    pick.primary == gun
                };
                let hover = mouse_in(ui.mouse, x, y, colw, cardh);
                if chosen {
                    round_rect(
                        x - 4.0 * k,
                        y - 4.0 * k,
                        colw + 8.0 * k,
                        cardh + 8.0 * k,
                        18.0 * k,
                        Color::new(1.0, 0.79, 0.29, 0.3),
                    );
                    round_rect(x, y, colw, cardh, 14.0 * k, ACCENT);
                    round_rect(
                        x + 4.0 * k,
                        y + 4.0 * k,
                        colw - 8.0 * k,
                        cardh - 8.0 * k,
                        11.0 * k,
                        Color::new(0.25, 0.28, 0.36, 1.0),
                    );
                } else {
                    if hover {
                        round_rect(
                            x - 2.0,
                            y - 2.0,
                            colw + 4.0,
                            cardh + 4.0,
                            15.0 * k,
                            Color::new(1.0, 1.0, 1.0, 0.25),
                        );
                    }
                    round_rect(
                        x,
                        y,
                        colw,
                        cardh,
                        14.0 * k,
                        if hover { PANEL2 } else { PANEL },
                    );
                }
                let d = gun.def();
                if chosen {
                    let tag = if pistols {
                        t("ДОДАТКОВА", "SECONDARY")
                    } else {
                        t("ОСНОВНА", "PRIMARY")
                    };
                    let tw = ui.width(tag, fs(13.0), true);
                    round_rect(
                        x + 12.0 * k,
                        y + 12.0 * k,
                        tw + 14.0 * k,
                        22.0 * k,
                        6.0 * k,
                        ACCENT,
                    );
                    ui.bold(tag, x + 19.0 * k, y + 28.0 * k, fs(13.0), DARK_TEXT);
                } else {
                    ui.bold(
                        &format!("{}", gi + 1),
                        x + 16.0 * k,
                        y + 30.0 * k,
                        fs(18.0),
                        MUTED,
                    );
                }
                let name = ui.fit(d.name, fs(20.0), true, colw * 0.62);
                let nw = ui.width(&name, fs(20.0), true);
                ui.bold(
                    &name,
                    x + colw - 16.0 * k - nw,
                    y + 31.0 * k,
                    fs(20.0),
                    if chosen { ACCENT } else { TEXT },
                );
                let icon_h = (cardh - 130.0 * k).max(30.0 * k);
                draw_gun(
                    gun,
                    x + colw / 2.0,
                    y + 44.0 * k + icon_h / 2.0,
                    colw - 60.0 * k,
                    icon_h,
                );
                // three bars: damage, rate of fire, range
                let (dm, rpm, rg) = stat(gun);
                let bars = [
                    (t("Урон", "Damage"), (dm / most.0).sqrt()),
                    (t("Темп", "Rate"), rpm / most.1),
                    // (a sniper rifle reaches several times further than the rest: on a
                    // square root scale the others still show how they compare)
                    (t("Дальн.", "Range"), (rg / most.2).sqrt()),
                ];
                for (j, (label, v)) in bars.iter().enumerate() {
                    let sy = y + cardh - 66.0 * k + j as f32 * 18.0 * k;
                    ui.text(label, x + 16.0 * k, sy + 10.0 * k, fs(13.0), MUTED);
                    let (bx, bw) = (x + 82.0 * k, colw - 100.0 * k);
                    round_rect(
                        bx,
                        sy + 3.0 * k,
                        bw,
                        8.0 * k,
                        4.0 * k,
                        Color::new(0.08, 0.09, 0.15, 1.0),
                    );
                    let fill = if chosen {
                        ACCENT
                    } else {
                        Color::new(0.51, 0.59, 0.78, 1.0)
                    };
                    round_rect(
                        bx,
                        sy + 3.0 * k,
                        (bw * v.clamp(0.04, 1.0)).max(8.0 * k),
                        8.0 * k,
                        4.0 * k,
                        fill,
                    );
                }
                let clicked = hover && ui.clicked;
                if clicked || chosen_by_key == Some(gun) {
                    if pistols {
                        pick.secondary = gun;
                    } else {
                        pick.primary = gun;
                    }
                }
            }
        }

        // ---- the perks
        let py = h - foot - perk_h;
        let pw = (w - 2.0 * mx - gap * 2.0) / 3.0;
        let groups: [(&str, Vec<(&str, bool)>, &str); 3] = [
            (
                t("Перк 1", "Perk 1"),
                Perk1::ALL
                    .iter()
                    .map(|p| (p.name(), *p == pick.perk1))
                    .collect(),
                pick.perk1.desc(),
            ),
            (
                t("Перк 2", "Perk 2"),
                Perk2::ALL
                    .iter()
                    .map(|p| (p.name(), *p == pick.perk2))
                    .collect(),
                pick.perk2.desc(),
            ),
            (
                t("Перк 3", "Perk 3"),
                Perk3::ALL
                    .iter()
                    .map(|p| (p.name(), *p == pick.perk3))
                    .collect(),
                pick.perk3.desc(),
            ),
        ];
        for (i, (label, opts, desc)) in groups.iter().enumerate() {
            let x = mx + i as f32 * (pw + gap);
            round_rect(x, py, pw, perk_h, 14.0 * k, PANEL);
            ui.bold(label, x + 18.0 * k, py + 30.0 * k, fs(18.0), ACCENT);
            let lw = ui.width(label, fs(18.0), true);
            ui.text(
                &ui.fit(desc, fs(15.0), false, pw - lw - 50.0 * k),
                x + 30.0 * k + lw,
                py + 30.0 * k,
                fs(15.0),
                MUTED,
            );
            let (mut cx, mut ry) = (x + 16.0 * k, py + 46.0 * k);
            for (j, (name, on)) in opts.iter().enumerate() {
                let tw = ui.width(name, fs(15.0), true) + 28.0 * k;
                if cx + tw > x + pw - 12.0 * k {
                    cx = x + 16.0 * k;
                    ry += 48.0 * k;
                }
                let hover = mouse_in(ui.mouse, cx, ry, tw, 40.0 * k);
                let c = if *on {
                    ACCENT
                } else if hover {
                    Color::new(0.3, 0.35, 0.5, 1.0)
                } else {
                    PANEL2
                };
                round_rect(cx, ry, tw, 40.0 * k, 10.0 * k, c);
                ui.bold(
                    name,
                    cx + 14.0 * k,
                    ry + 26.0 * k,
                    fs(15.0),
                    if *on { DARK_TEXT } else { TEXT },
                );
                if hover && ui.clicked {
                    match i {
                        0 => pick.perk1 = Perk1::ALL[j],
                        1 => pick.perk2 = Perk2::ALL[j],
                        _ => pick.perk3 = Perk3::ALL[j],
                    }
                }
                cx += tw + 10.0 * k;
            }
        }
        let hint = match self.pick_col {
            Some(c) => tr!(
                "{}: натисни номер зброї (1–{})",
                "{}: press the gun's number (1–{})",
                columns()[c].1,
                guns_of(columns()[c].0).len()
            ),
            None => t(
                "Клік — вибрати · 1–6, потім номер — вибір з клавіатури · Enter — в бій · Esc — закрити · набір запам'ятовується",
                "Click to pick · 1–6 then a number from the keyboard · Enter to fight · Esc to close · the loadout is remembered",
            )
            .to_string(),
        };
        let hw = ui.width(&hint, fs(17.0), false);
        ui.text(&hint, (w - hw) / 2.0, h - 16.0 * k, fs(17.0), MUTED);

        self.pick = Some(pick);
        if team {
            self.overlay = crate::game::Overlay::Team;
        } else if fight || is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter) {
            let l = pick.sanitized();
            settings.loadout = Some(l);
            settings.save();
            self.pick = None;
            self.pick_col = None;
            self.choose_class(l);
        }
        None
    }
}

/// The dark text on light (accent) buttons.
const DARK_TEXT: Color = Color::new(0.09, 0.11, 0.17, 1.0);
