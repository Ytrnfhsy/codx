//! The battle royale inventory, its full screen and a death box's contents.

use super::*;

impl GameClient {
    /// Battle royale: what we carry, bottom left — medkits, vest, helmet, backpack,
    /// grenades and the rounds of every calibre we have.
    pub(super) fn draw_br_inventory(&self, ui: &Ui) {
        use toon_shared::inventory::{armor_durability, helmet_durability, AmmoType, MEDKIT_TIME};
        let Some(br) = &self.br else { return };
        if !self.alive || br.in_plane {
            return;
        }
        let inv = &br.inv;
        let h = screen_height();
        let (x0, y0) = (16.0, h - 118.0);
        let s = 60.0;
        // a picture and the level as pips (or a count) instead of words and numerals
        let tile = |k: usize, icon: Icon, level: u8, value: &str, fill: f32, c: Color| {
            let x = x0 + k as f32 * (s + 6.0);
            let on = level > 0 || !value.is_empty();
            soft_rect(
                x,
                y0,
                s,
                s,
                Color::new(0.0, 0.0, 0.0, if on { 0.55 } else { 0.3 }),
            );
            if fill > 0.0 {
                draw_rectangle(x, y0 + s - 5.0, s * fill.clamp(0.0, 1.0), 5.0, c);
            }
            let ic = if on { c } else { with_alpha(MUTED, 0.6) };
            draw_icon(icon, x + s / 2.0, y0 + 21.0, 24.0, ic);
            if !value.is_empty() {
                let vw = ui.width(value, 15, true);
                ui.shadowed(value, x + (s - vw) / 2.0, y0 + 46.0, 15, TEXT);
            } else if k != 4 {
                level_pips(x + s / 2.0, y0 + 44.0, level);
            }
        };
        tile(
            0,
            Icon::Medkit,
            0,
            &format!("{}/{}", inv.medkits, inv.medkit_cap()),
            0.0,
            Color::new(1.0, 0.4, 0.4, 1.0),
        );
        // the key that uses one
        let hint = format!("[{}]", key(Action::Heal));
        let hw = ui.width(&hint, 11, false);
        ui.text(&hint, x0 + (s - hw) / 2.0, y0 + s - 3.0, 11, MUTED);
        let blue = Color::new(0.45, 0.65, 1.0, 1.0);
        tile(
            1,
            Icon::Vest,
            inv.armor,
            "",
            if inv.armor > 0 {
                inv.armor_hp / armor_durability(inv.armor)
            } else {
                0.0
            },
            blue,
        );
        tile(
            2,
            Icon::Helmet,
            inv.helmet,
            "",
            if inv.helmet > 0 {
                inv.helmet_hp / helmet_durability(inv.helmet)
            } else {
                0.0
            },
            blue,
        );
        tile(3, Icon::Backpack, inv.backpack, "", 0.0, ACCENT);
        {
            use toon_shared::inventory::sight_short;
            let l = self.sight();
            tile(
                4,
                if l == 1 { Icon::RedDot } else { Icon::Scope },
                0,
                if l > 0 { sight_short(l) } else { "" },
                0.0,
                Color::new(1.0, 0.45, 0.35, 1.0),
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
                soft_rect(x0, y0 - 8.0, bw, 4.0, Color::new(0.0, 0.0, 0.0, 0.5));
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
            soft_rect(
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
    pub(super) fn draw_inventory_screen(&mut self, ui: &mut Ui) {
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
        soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
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
            let empty = br.empty & (1 << k) != 0;
            soft_rect(lx, ly, cw, 84.0, Color::new(0.0, 0.0, 0.0, 0.3));
            if empty {
                ui.text(
                    t("— порожньо —", "— empty —"),
                    lx + 12.0,
                    ly + 38.0,
                    16,
                    MUTED,
                );
            } else {
                let title = format!("{}. {}", k + 1, wid.def().name());
                ui.bold(
                    &title,
                    lx + 12.0,
                    ly + 24.0,
                    18,
                    if self.slot as usize == k {
                        ACCENT
                    } else {
                        TEXT
                    },
                );
                // which sights this gun takes
                ui.text(
                    &tr!(
                        "приціли: {}",
                        "sights: {}",
                        toon_shared::inventory::sight_rule(wid)
                    ),
                    lx + 24.0 + ui.width(&title, 18, true),
                    ly + 23.0,
                    13,
                    MUTED,
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
                if small(ui, drop_txt, lx + cw - bw - 8.0, ly + 8.0, bw) {
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
            soft_rect(lx, ly, cw, 36.0, Color::new(0.0, 0.0, 0.0, 0.3));
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
                let ok = br.empty & (1 << k) == 0 && sight_fits(level, self.weapons[k].0);
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
            soft_rect(lx, ly, cw, 36.0, Color::new(0.0, 0.0, 0.0, 0.3));
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
                let ok = br.empty & (1 << k) == 0
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
            soft_rect(rx, ry, cw, 34.0, Color::new(0.0, 0.0, 0.0, 0.3));
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
    pub(super) fn draw_box_screen(&mut self, ui: &mut Ui) {
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
        soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.45));
        let rows = v.items.len() + v.guns.len();
        // with guns in the box: which hand a gun taken replaces (the one in hand, unless
        // the other is empty), switched here or with the usual keys
        let hands = !v.guns.is_empty();
        if hands {
            if crate::binds::pressed(Action::Primary) {
                self.switch_to(0);
            }
            if crate::binds::pressed(Action::Secondary) {
                self.switch_to(1);
            }
        }
        let hands_h = if hands { 52.0 } else { 0.0 };
        let pw = 460.0f32.min(w - 20.0);
        let ph = (130.0 + hands_h + rows as f32 * 38.0).min(h - 20.0);
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
            t(
                "Клацни, щоб узяти · ◆ або СКМ — позначити для своїх",
                "Click to take · ◆ or MMB — ping it for your side",
            ),
            x + 24.0,
            y + 62.0,
            14,
            MUTED,
        );
        let mut act = None;
        let mut ping: Option<u8> = None;
        // a row: take it (most of it), or ping it (the diamond at its end, or the middle
        // button anywhere on it)
        let row_w = pw - 48.0 - 42.0;
        let mine: Vec<toon_shared::protocol::PingWhat> = self
            .pings
            .iter()
            .filter(|p| {
                p.from == self.my_id && Vec3::from_array(p.pos).distance(self.body.pos) < 10.0
            })
            .map(|p| p.what)
            .collect();
        let pinged = |what: toon_shared::protocol::PingWhat| mine.contains(&what);
        let mut ry = y + 76.0;
        if hands {
            let empty = self.br.as_ref().map_or(0, |b| b.empty);
            let half = (pw - 48.0 - 10.0) / 2.0;
            for k in 0..2u8 {
                let name = if empty & (1 << k) != 0 {
                    t("порожньо", "empty").to_string()
                } else {
                    self.weapons[k as usize].0.def().name().to_string()
                };
                let label = format!("{}. {name}", k + 1);
                let bx = x + 24.0 + k as f32 * (half + 10.0);
                let hit = if k == self.slot {
                    ui.button(&label, bx, ry, half, 34.0)
                } else {
                    ui.button2(&label, bx, ry, half, 34.0)
                };
                if hit {
                    self.switch_to(k);
                }
            }
            ry += 40.0;
            ui.text(
                &tr!(
                    "Зброя піде в порожній слот або замінить ту, що в руках ({} / {})",
                    "A gun goes in an empty slot or replaces the one in hand ({} / {})",
                    key(Action::Primary),
                    key(Action::Secondary)
                ),
                x + 24.0,
                ry,
                13,
                MUTED,
            );
            ry += 12.0;
        }
        for (k, (kind, n)) in v.items.iter().enumerate() {
            let label = match kind {
                LootKind::Ammo(_) | LootKind::Medkit => format!("{} ×{n}", kind.name()),
                LootKind::Armor(_) | LootKind::Helmet(_) => format!("{} ({n}%)", kind.name()),
                _ => kind.name(),
            };
            if ui.button2(&label, x + 24.0, ry, row_w, 32.0) {
                act = Some(BoxCmd::Take(v.id, k as u8));
            }
            let on = pinged(toon_shared::protocol::PingWhat::Loot(*kind, *n));
            if ping_button(ui, x + 30.0 + row_w, ry, on) || middle_on(ui, x + 24.0, ry, pw - 48.0) {
                ping = Some(k as u8);
            }
            ry += 38.0;
        }
        for (k, (gun, mag)) in v.guns.iter().enumerate() {
            let label = format!("{} · {mag}", gun.def().name());
            if ui.button2(&label, x + 24.0, ry, row_w, 32.0) {
                act = Some(BoxCmd::Take(v.id, (v.items.len() + k) as u8));
            }
            let on = pinged(toon_shared::protocol::PingWhat::Weapon(*gun));
            if ping_button(ui, x + 30.0 + row_w, ry, on) || middle_on(ui, x + 24.0, ry, pw - 48.0) {
                ping = Some((v.items.len() + k) as u8);
            }
            ry += 38.0;
        }
        if let Some(k) = ping {
            self.net.send(toon_shared::protocol::C2S::Mark(
                toon_shared::protocol::MarkAt::BoxItem(v.id, k),
            ));
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
}
