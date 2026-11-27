//! Looking through a scope: the mask, the reticle and the holdover ladder.

use super::*;

impl GameClient {
    pub(super) fn draw_scope(&self) {
        let (w, h) = (screen_width(), screen_height());
        let r = h * 0.46;
        let (cx, cy) = (w / 2.0, h / 2.0);
        let black = Color::new(0.02, 0.02, 0.03, 1.0);
        draw_rectangle(0.0, 0.0, cx - r, h, black);
        draw_rectangle(cx + r, 0.0, w - cx - r, h, black);
        // everything outside the round view is black (a ring of quads out past the
        // square's corners)
        let n = 72;
        let far = r * 1.5;
        for i in 0..n {
            let a0 = i as f32 / n as f32 * std::f32::consts::TAU;
            let a1 = (i + 1) as f32 / n as f32 * std::f32::consts::TAU;
            let (p0, p1) = (
                vec2(cx + a0.cos() * r, cy + a0.sin() * r),
                vec2(cx + a1.cos() * r, cy + a1.sin() * r),
            );
            let (q0, q1) = (
                vec2(cx + a0.cos() * far, cy + a0.sin() * far),
                vec2(cx + a1.cos() * far, cy + a1.sin() * far),
            );
            draw_triangle(p0, q0, q1, black);
            draw_triangle(p0, q1, p1, black);
        }
        draw_circle_lines(cx, cy, r, 4.0, black);
        draw_rectangle(cx - r, 0.0, 2.0 * r, cy - r + 2.0, black);
        draw_rectangle(cx - r, cy + r - 2.0, 2.0 * r, h - cy - r + 2.0, black);
        draw_line(cx - r, cy, cx + r, cy, 2.0, black);
        draw_line(cx, cy - r, cx, cy + r, 2.0, black);
        draw_line(cx - r, cy, cx - 30.0, cy, 5.0, black);
        draw_line(cx + 30.0, cy, cx + r, cy, 5.0, black);
        // (the thick post starts low: the holdover ladder sits on the thin line above it)
        draw_line(cx, cy + r * 0.62, cx, cy + r, 5.0, black);
        draw_circle(cx, cy, 2.0, BAD);
    }

    /// The scope's holdover: a ladder under the centre, one mark per hundred metres (where
    /// to put the target for the bullet to drop onto it, worked out from the gun's real
    /// ballistics and this zoom), hundreds written by every other one; a yellow mark at
    /// the hold for the range found, and the same in words.
    pub(super) fn draw_holdover(&self, ui: &Ui, fovy: f32) {
        let (w, h) = (screen_width(), screen_height());
        let (cx, cy) = (w / 2.0, h / 2.0);
        let r = h * 0.46;
        let v = toon_shared::ballistics::Ballistics::of(self.weapon());
        // pixels per radian at the centre of this view
        let px = (h * 0.5) / (fovy * 0.5).tan();
        let drop_px = |m: f32| -> f32 { (v.drop_at(m) / m).atan() * px };
        let ink = Color::new(0.04, 0.04, 0.06, 0.95);
        let most = v.max_range.min(1500.0);
        let mut m = 100.0;
        while m <= most {
            let y = cy + drop_px(m);
            if y > cy + r * 0.8 {
                break;
            }
            let hundreds = (m / 100.0).round() as i32;
            let long = hundreds % 2 == 0;
            let hw = if long { 14.0 } else { 7.0 };
            draw_line(cx - hw, y, cx + hw, y, if long { 2.0 } else { 1.5 }, ink);
            if long {
                // the numbers left and right by turns: they do not crowd one another
                let s = format!("{hundreds}");
                let left = (hundreds / 2) % 2 == 1;
                let tw = ui.width(&s, 12, true);
                let x = if left {
                    cx - hw - 6.0 - tw
                } else {
                    cx + hw + 6.0
                };
                ui.bold(&s, x, y + 4.5, 12, ink);
            }
            m += 100.0;
        }
        // the hold for the range found
        let Some(d) = self.hold_range() else { return };
        let y = cy + if d > 100.0 { drop_px(d) } else { 0.0 };
        let gold = Color::new(1.0, 0.79, 0.29, 1.0);
        draw_triangle(
            vec2(cx + 18.0, y),
            vec2(cx + 28.0, y - 5.5),
            vec2(cx + 28.0, y + 5.5),
            gold,
        );
        let n = d / 100.0;
        let hold = if d <= 120.0 {
            t("у центр", "dead centre").to_string()
        } else if (n - n.round()).abs() < 0.2 {
            tr!("на риску {:.0}", "on mark {:.0}", n.round())
        } else {
            tr!(
                "між {:.0} і {:.0}",
                "between {:.0} and {:.0}",
                n.floor(),
                n.floor() + 1.0
            )
        };
        let s = format!("{d:.0} {} · {}", t("м", "m"), hold);
        ui.centered(&s, cx, cy + r - 40.0, 20, gold);
    }
}
