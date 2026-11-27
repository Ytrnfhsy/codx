//! The chat.

use super::*;

impl GameClient {
    pub(super) fn draw_chat(&mut self, ui: &mut Ui) {
        let h = screen_height();
        let open = self.chat_input.is_some();
        let mut y = h - 190.0;
        let lines: Vec<&crate::game::Line> = self
            .chat
            .iter()
            .rev()
            .filter(|l| open || self.now - l.time < 10.0)
            .take(if open { 14 } else { 6 })
            .collect();
        for l in lines {
            // a dark strip under each line, and the colour lifted towards white: chat
            // stays readable over the sky, sand and snow alike
            let fade = if open {
                1.0
            } else {
                (10.0 - (self.now - l.time) as f32).clamp(0.0, 1.0)
            };
            let w = ui.width(&l.text, 16, true);
            soft_rect(
                10.0,
                y - 16.0,
                w + 12.0,
                21.0,
                Color::new(0.0, 0.0, 0.0, 0.5 * fade),
            );
            let c = l.color;
            let lift = |v: f32| v + (1.0 - v) * 0.35;
            let c = Color::new(lift(c.r), lift(c.g), lift(c.b), c.a * fade);
            ui.shadowed(&l.text, 16.0, y, 16, c);
            y -= 21.0;
        }
        if let Some((text, team)) = &self.chat_input {
            let label = if *team {
                t("Команді: ", "Team: ")
            } else {
                t("Всім: ", "All: ")
            };
            soft_rect(10.0, h - 176.0, 560.0, 30.0, Color::new(0.0, 0.0, 0.0, 0.6));
            let cursor = if (self.now * 2.0).fract() < 0.5 {
                "_"
            } else {
                ""
            };
            ui.shadowed(&format!("{label}{text}{cursor}"), 16.0, h - 155.0, 18, TEXT);
        }
    }
}
