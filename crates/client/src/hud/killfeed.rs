//! The kill feed and the killcam's banner.

use super::*;

impl GameClient {
    pub(super) fn draw_killfeed(&self, ui: &Ui) {
        let w = screen_width();
        let mut y = 30.0;
        let ih = 16.0;
        for k in self.killfeed.iter().rev() {
            let age = (self.now - k.time) as f32;
            if age > 7.0 {
                continue;
            }
            let a = (1.0 - (age - 6.0).max(0.0)).clamp(0.0, 1.0);
            let suicide = k.killer.is_empty() || k.killer == k.victim;
            let icon_w = cause_icon_width(k.cause, ih) + if k.headshot { ih + 6.0 } else { 0.0 };
            let kw = if suicide {
                0.0
            } else {
                ui.width(&k.killer, 16, true)
            };
            let vw = ui.width(&k.victim, 16, true);
            let gap = 10.0;
            // (how far: worth showing once it is a real shot)
            let dist = (!suicide && k.dist >= 10).then(|| tr!("{} м", "{} m", k.dist));
            let dw = dist.as_ref().map_or(0.0, |d| ui.width(d, 13, false) + gap);
            let total = kw + vw + icon_w + dw + gap * if suicide { 1.0 } else { 2.0 };
            let mut x = w - 20.0 - total;
            soft_rect(
                x - 6.0,
                y - 17.0,
                total + 12.0,
                23.0,
                Color::new(0.0, 0.0, 0.0, 0.35 * a),
            );
            if !suicide {
                ui.shadowed(
                    &k.killer,
                    x,
                    y,
                    16,
                    with_alpha(self.feed_color(k.kteam, k.kme), a),
                );
                x += kw + gap;
            }
            let iw = draw_cause_icon(k.cause, x, y - 6.0, ih, a);
            x += iw;
            if k.headshot {
                draw_headshot_icon(x + 6.0 + ih * 0.5, y - 6.0, ih, a);
                x += ih + 6.0;
            }
            x += gap;
            ui.shadowed(
                &k.victim,
                x,
                y,
                16,
                with_alpha(self.feed_color(k.vteam, k.vme), a),
            );
            if let Some(d) = &dist {
                ui.shadowed(d, x + vw + gap, y - 1.0, 13, with_alpha(MUTED, a));
            }
            y += 26.0;
        }
    }

    /// Name colour in the kill feed: team colours, or in the battle royale (no teams) us in
    /// yellow and everybody else in red.
    fn feed_color(&self, team: Team, me: bool) -> Color {
        if !self.mode.solo() {
            self.tcol(team)
        } else if me {
            ACCENT
        } else {
            BAD
        }
    }

    /// Killcam overlay: letterbox, title, who killed whom and with what.
    pub(super) fn draw_killcam(&self, ui: &Ui) {
        let Some(k) = &self.killcam else { return };
        let (w, h) = (screen_width(), screen_height());
        let (cx, cy) = (w / 2.0, h / 2.0);
        if k.scoped() {
            self.draw_scope();
        }
        draw_hitmarker(cx, cy, (k.pt - k.hitmarker) as f32, k.hit_kill, false);
        let bar = (h * 0.09).max(48.0);
        let black = Color::new(0.0, 0.0, 0.0, 0.85);
        draw_rectangle(0.0, 0.0, w, bar, black);
        draw_rectangle(0.0, h - bar, w, bar, black);
        let title = if k.final_cam {
            t("ФІНАЛЬНИЙ КІЛКАМ", "FINAL KILLCAM")
        } else {
            t("КІЛКАМ", "KILLCAM")
        };
        // a slow blink on the record dot
        if ((self.now * 2.0) as i64) % 2 == 0 {
            draw_circle(
                cx - ui.width(title, 30, true) / 2.0 - 18.0,
                bar * 0.5 - 2.0,
                7.0,
                BAD,
            );
        }
        ui.centered(title, cx, bar * 0.5 + 10.0, 30, ACCENT);

        // killer · weapon · victim
        let info = &k.info;
        let ih = 20.0;
        let kw = ui.width(&info.killer_name, 22, true);
        let vw = ui.width(&info.victim_name, 22, true);
        let iw = cause_icon_width(info.cause, ih) + if info.headshot { ih + 8.0 } else { 0.0 };
        let total = kw + vw + iw + 40.0;
        let mut x = cx - total / 2.0;
        let y = h - bar * 0.5 + 8.0;
        ui.shadowed(&info.killer_name, x, y, 22, self.tcol(info.killer_team));
        x += kw + 20.0;
        x += draw_cause_icon(info.cause, x, y - 8.0, ih, 1.0);
        if info.headshot {
            draw_headshot_icon(x + 8.0 + ih * 0.5, y - 8.0, ih, 1.0);
            x += ih + 8.0;
        }
        x += 20.0;
        let vteam = self
            .scoreboard
            .iter()
            .find(|r| r.id == info.victim)
            .map(|r| r.team)
            .unwrap_or(Team::Spectator);
        ui.shadowed(&info.victim_name, x, y, 22, self.tcol(vteam));
        if !k.final_cam {
            let hint = if self.me.respawn_in > 0.0 {
                tr!(
                    "{} або Esc — пропустити · відродження через {:.0}",
                    "{} or Esc — skip · respawn in {:.0}",
                    key(Action::Fire),
                    self.me.respawn_in.ceil()
                )
            } else {
                tr!(
                    "{} або Esc — пропустити",
                    "{} or Esc — skip",
                    key(Action::Fire)
                )
            };
            ui.text(
                &hint,
                w - ui.width(&hint, 15, false) - 20.0,
                h - 12.0,
                15,
                MUTED,
            );
        }
    }
}
