//! The hit marker over the crosshair.

use super::*;

/// Hitmarker: pops out on impact, red and bigger for kills, with a gold ring for headshots.
pub(super) fn draw_hitmarker(cx: f32, cy: f32, age: f32, kill: bool, head: bool) {
    let life = if kill { 0.4 } else { 0.25 };
    if !(0.0..life).contains(&age) {
        return;
    }
    let a = 1.0 - age / life;
    let pop = 1.0 + 0.5 * (1.0 - (age / 0.07).min(1.0));
    let size = if kill { 1.35 } else { 1.0 } * pop;
    let c = if kill { BAD } else { WHITE };
    for (dx, dy) in [(1.0f32, 1.0f32), (-1.0, 1.0), (1.0, -1.0), (-1.0, -1.0)] {
        let (x0, y0) = (cx + dx * 6.0 * size, cy + dy * 6.0 * size);
        let (x1, y1) = (cx + dx * 14.0 * size, cy + dy * 14.0 * size);
        draw_line(
            x0 + 1.0,
            y0 + 1.0,
            x1 + 1.0,
            y1 + 1.0,
            3.5,
            Color::new(0.0, 0.0, 0.0, a * 0.5),
        );
        draw_line(
            x0,
            y0,
            x1,
            y1,
            if kill { 3.5 } else { 3.0 },
            with_alpha(c, a),
        );
    }
    if head {
        draw_circle_lines(cx, cy, 20.0 * size, 2.0, with_alpha(ACCENT, a));
    }
}
