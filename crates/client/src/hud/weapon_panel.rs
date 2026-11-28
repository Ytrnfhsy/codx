//! The gun panel at the bottom right and the bomb badge.

use super::*;

impl GameClient {
    /// The bomb you carry, shown next to the weapon panel (pulses so it is hard to miss).
    fn draw_bomb_badge(&self, ui: &Ui) {
        let carrying = self.alive
            && self
                .bomb
                .as_ref()
                .is_some_and(|b| b.carrier == Some(self.my_id) && b.planted_at.is_none());
        if !carrying {
            return;
        }
        let (w, h) = (screen_width(), screen_height());
        let (s, x, y) = (84.0, w - 310.0 - 84.0 - 12.0, h - 130.0);
        let pulse = 0.5 + 0.5 * (self.now * 4.0).sin() as f32;
        soft_rect(x, y, s, s + 22.0, Color::new(0.0, 0.0, 0.0, 0.45));
        draw_rectangle_lines(
            x,
            y,
            s,
            s + 22.0,
            2.0 + pulse * 2.0,
            Color::new(1.0, 0.35 + pulse * 0.3, 0.25, 0.9),
        );
        // three red sticks taped together, a timer and a wire
        let (bx, by) = (x + 14.0, y + 18.0);
        for k in 0..3 {
            let sx = bx + k as f32 * 19.0;
            draw_rectangle(sx, by, 16.0, 46.0, Color::from_rgba(214, 60, 48, 255));
            draw_rectangle_lines(sx, by, 16.0, 46.0, 2.0, BLACK);
        }
        draw_rectangle(
            bx - 2.0,
            by + 17.0,
            59.0,
            9.0,
            Color::from_rgba(60, 60, 66, 255),
        );
        draw_rectangle(
            bx + 14.0,
            by + 6.0,
            27.0,
            16.0,
            Color::from_rgba(30, 32, 36, 255),
        );
        draw_rectangle_lines(bx + 14.0, by + 6.0, 27.0, 16.0, 2.0, BLACK);
        let led = if pulse > 0.5 {
            Color::from_rgba(255, 70, 50, 255)
        } else {
            Color::from_rgba(120, 30, 25, 255)
        };
        draw_circle(bx + 27.5, by + 14.0, 3.5, led);
        draw_line(bx + 41.0, by + 10.0, bx + 54.0, by - 6.0, 2.0, ACCENT);
        let label = t("БОМБА", "BOMB");
        let lw = ui.width(label, 16, true);
        ui.shadowed(label, x + (s - lw) / 2.0, y + s + 14.0, 16, ACCENT);
    }

    pub(super) fn draw_weapon_panel(&self, ui: &Ui) {
        self.draw_bomb_badge(ui);
        let (w, h) = (screen_width(), screen_height());
        let (wid, mag, res) = self.weapons[self.slot as usize & 1];
        let def = wid.def();
        let x = w - 300.0;
        let y = h - 120.0;
        soft_rect(
            x - 10.0,
            y - 10.0,
            300.0,
            110.0,
            Color::new(0.0, 0.0, 0.0, 0.35),
        );
        if self.hand_empty() {
            ui.shadowed(t("Ніж", "Knife"), x, y + 36.0, 30, TEXT);
            ui.shadowed(
                t("зброю знайдете в будинках", "find a gun in the houses"),
                x,
                y + 66.0,
                16,
                MUTED,
            );
        } else {
            let mag_max = self.mag_cap();
            let mag_c = if mag == 0 {
                BAD
            } else if (mag as f32) < mag_max as f32 * 0.3 {
                ACCENT
            } else {
                TEXT
            };
            ui.shadowed(&format!("{mag}"), x, y + 42.0, 46, mag_c);
            let mw = ui.width(&format!("{mag}"), 46, true);
            ui.shadowed(&format!("/ {res}"), x + mw + 8.0, y + 42.0, 24, MUTED);
            ui.shadowed(def.name(), x, y + 70.0, 18, TEXT);
        }
        // grenades: the lethal ones, then the tactical ones
        let (lethal, tactical) = (self.lethal_now(), self.tactical_now());
        for i in 0..self.grenades {
            crate::loadout_menu::draw_grenade(
                crate::loadout_menu::Nade::Lethal(lethal),
                x + 200.0 + i as f32 * 22.0,
                y + 30.0,
                1.0,
            );
        }
        for i in 0..self.smokes {
            crate::loadout_menu::draw_grenade(
                crate::loadout_menu::Nade::Tactical(tactical),
                x + 200.0 + (self.grenades + i) as f32 * 22.0,
                y + 30.0,
                1.0,
            );
        }
        if self.smoke_held {
            ui.centered(
                &tr!(
                    "{}: відпусти {}, щоб кинути",
                    "{}: release {} to throw",
                    tactical.name(),
                    key(Action::Smoke)
                ),
                w / 2.0,
                h * 0.62,
                20,
                TEXT,
            );
        }
        if self.cooking.is_some() && lethal == toon_shared::weapons::Lethal::Molotov {
            ui.centered(
                &tr!(
                    "Молотов: відпусти {}, щоб кинути",
                    "Molotov: release {} to throw",
                    key(Action::Grenade)
                ),
                w / 2.0,
                h * 0.62,
                20,
                TEXT,
            );
        } else if let Some(start) = self.cooking {
            let left = (self.tuning.grenade.fuse as f64 - (self.now - start)).max(0.0);
            ui.centered(
                &tr!("Граната: {left:.1}", "Grenade: {left:.1}"),
                w / 2.0,
                h * 0.62,
                22,
                BAD,
            );
        }
        // killstreak
        let (next, left) = Killstreak::next_after(self.me.streak as u32, &self.tuning.streaks);
        let streak_txt = tr!(
            "Серія: {} (до «{}» {})",
            "Streak: {} ({} in {})",
            self.me.streak,
            next.name(),
            left
        );
        // (no killstreaks in the battle royale)
        if self.mode != Mode::Br {
            ui.shadowed(&streak_txt, x, y + 92.0, 13, MUTED);
        }
        if let Some(r) = self.me.rewards.last() {
            let t = format!("[{}] {}", key(Action::Killstreak), r.name());
            let tw = ui.width(&t, 18, true);
            soft_rect(
                w - tw - 30.0,
                y - 48.0,
                tw + 16.0,
                30.0,
                Color::new(0.0, 0.0, 0.0, 0.5),
            );
            ui.shadowed(&t, w - tw - 22.0, y - 26.0, 18, ACCENT);
        }
        // stance + sprint
        let stance = match self.body.stance {
            toon_shared::physics::Stance::Stand => "▮",
            toon_shared::physics::Stance::Crouch => "▄",
            toon_shared::physics::Stance::Prone => "▁",
        };
        ui.shadowed(stance, x - 40.0, y + 60.0, 30, TEXT);
        if self.reload_until.is_some() {
            ui.centered(t("Перезарядка…", "Reloading…"), w / 2.0, h * 0.58, 18, TEXT);
        } else if mag == 0 && res == 0 && !self.hand_empty() {
            ui.centered(
                &tr!(
                    "Немає набоїв — {}/{} змінити зброю",
                    "Out of ammo — {}/{} to switch weapons",
                    key(Action::Primary),
                    key(Action::Secondary)
                ),
                w / 2.0,
                h * 0.58,
                18,
                BAD,
            );
        }
    }
}
