//! Little drawings: what killed whom, guns, the bomb, inventory icons and level pips.

use super::*;

pub(super) fn icon_color(a: f32) -> (Color, Color) {
    (
        Color::new(1.0, 1.0, 1.0, 0.95 * a),
        Color::new(0.0, 0.0, 0.0, 0.7 * a),
    )
}

/// Side view of a gun built from its 3D model: (x, y, w, h) rectangles, barrel to the right.
pub(super) fn gun_rects(w: WeaponId, h: f32) -> (Vec<Rect>, f32) {
    let parts = crate::models::gun_parts(w);
    let (mut z0, mut z1, mut y0, mut y1) = (f32::MAX, f32::MIN, f32::MAX, f32::MIN);
    for p in &parts {
        z0 = z0.min(-(p.c.z + p.h.z));
        z1 = z1.max(-(p.c.z - p.h.z));
        y0 = y0.min(p.c.y - p.h.y);
        y1 = y1.max(p.c.y + p.h.y);
    }
    let s = (h / (y1 - y0)).min(64.0 / (z1 - z0));
    let rects = parts
        .iter()
        .map(|p| {
            Rect::new(
                (-(p.c.z + p.h.z) - z0) * s,
                (y1 - (p.c.y + p.h.y)) * s + (h - (y1 - y0) * s) * 0.5,
                (p.h.z * 2.0 * s).max(1.5),
                (p.h.y * 2.0 * s).max(1.5),
            )
        })
        .collect();
    (rects, (z1 - z0) * s)
}

pub(super) fn cause_icon_width(c: Cause, h: f32) -> f32 {
    match c {
        Cause::Weapon(w) => gun_rects(w, h).1,
        Cause::Knife | Cause::Airstrike | Cause::Helicopter => h * 1.8,
        _ => h * 1.1,
    }
}

/// Draws a small white silhouette for a cause of death; `cy` is the icon's vertical centre.
/// Returns its width.
pub(super) fn draw_cause_icon(c: Cause, x: f32, cy: f32, h: f32, a: f32) -> f32 {
    let (fg, bg) = icon_color(a);
    let top = cy - h * 0.5;
    match c {
        Cause::Weapon(w) => {
            let (rects, width) = gun_rects(w, h);
            for r in &rects {
                draw_rectangle(x + r.x - 1.0, top + r.y - 1.0, r.w + 2.0, r.h + 2.0, bg);
            }
            for r in &rects {
                draw_rectangle(x + r.x, top + r.y, r.w, r.h, fg);
            }
            width
        }
        Cause::Grenade => {
            let r = h * 0.36;
            let (gx, gy) = (x + h * 0.5, cy + h * 0.1);
            draw_circle(gx, gy, r + 1.2, bg);
            draw_circle(gx, gy, r, fg);
            draw_rectangle(gx - r * 0.3, gy - r - h * 0.18, r * 0.6, h * 0.2, fg);
            draw_circle_lines(gx + r * 0.6, gy - r - h * 0.05, h * 0.12, 1.5, fg);
            h * 1.1
        }
        Cause::Knife => {
            let w = h * 1.8;
            draw_rectangle(
                x - 1.0,
                cy - h * 0.18 - 1.0,
                w * 0.4 + 2.0,
                h * 0.36 + 2.0,
                bg,
            );
            draw_rectangle(x, cy - h * 0.18, w * 0.4, h * 0.36, fg);
            draw_rectangle(x + w * 0.38, cy - h * 0.3, w * 0.06, h * 0.6, fg);
            draw_triangle(
                vec2(x + w * 0.44, cy - h * 0.2),
                vec2(x + w * 0.44, cy + h * 0.15),
                vec2(x + w, cy - h * 0.12),
                fg,
            );
            w
        }
        Cause::Airstrike => {
            let w = h * 1.8;
            draw_triangle(
                vec2(x, cy - h * 0.4),
                vec2(x, cy + h * 0.4),
                vec2(x + w, cy),
                fg,
            );
            draw_rectangle(x + w * 0.1, cy - h * 0.08, w * 0.6, h * 0.16, bg);
            w
        }
        Cause::Helicopter => {
            let w = h * 1.8;
            draw_circle(x + w * 0.65, cy + h * 0.05, h * 0.3, fg);
            draw_rectangle(x, cy - h * 0.02, w * 0.6, h * 0.14, fg);
            draw_rectangle(x + w * 0.15, cy - h * 0.42, w * 0.85, h * 0.08, fg);
            draw_rectangle(x + w * 0.62, cy - h * 0.42, w * 0.05, h * 0.25, fg);
            w
        }
        Cause::Bomb => {
            draw_rectangle(x - 1.0, top + h * 0.25 - 1.0, h + 2.0, h * 0.65 + 2.0, bg);
            draw_rectangle(x, top + h * 0.25, h, h * 0.65, fg);
            draw_rectangle(x, top + h * 0.45, h, h * 0.12, bg);
            draw_line(x + h * 0.8, top + h * 0.25, x + h, top, 2.0, fg);
            h * 1.1
        }
        Cause::Artillery => {
            // a shell, nose to the right
            let w = h * 1.8;
            draw_rectangle(
                x - 1.0,
                cy - h * 0.22 - 1.0,
                w * 0.65 + 2.0,
                h * 0.44 + 2.0,
                bg,
            );
            draw_rectangle(x, cy - h * 0.22, w * 0.65, h * 0.44, fg);
            draw_triangle(
                vec2(x + w * 0.65, cy - h * 0.22),
                vec2(x + w * 0.65, cy + h * 0.22),
                vec2(x + w, cy),
                fg,
            );
            draw_rectangle(x + w * 0.12, cy - h * 0.22, w * 0.06, h * 0.44, bg);
            w
        }
        Cause::Fire => {
            // a flame
            let w = h * 0.7;
            let c = vec2(x + w * 0.5, cy + h * 0.15);
            draw_circle(c.x, c.y, w * 0.42 + 1.0, bg);
            draw_triangle(
                vec2(c.x - w * 0.42, c.y),
                vec2(c.x + w * 0.42, c.y),
                vec2(c.x + w * 0.05, cy - h * 0.5),
                fg,
            );
            draw_circle(c.x, c.y, w * 0.42, fg);
            w
        }
        Cause::Fpv => {
            // a quad drone from above
            let s = h * 0.9;
            let (cx, cy2) = (x + s * 0.6, cy);
            draw_line(
                cx - s * 0.45,
                cy2 - s * 0.45,
                cx + s * 0.45,
                cy2 + s * 0.45,
                3.0,
                fg,
            );
            draw_line(
                cx - s * 0.45,
                cy2 + s * 0.45,
                cx + s * 0.45,
                cy2 - s * 0.45,
                3.0,
                fg,
            );
            for (dx, dy) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
                draw_circle_lines(cx + dx * s * 0.45, cy2 + dy * s * 0.45, s * 0.2, 1.5, fg);
            }
            draw_rectangle(cx - s * 0.14, cy2 - s * 0.14, s * 0.28, s * 0.28, fg);
            s * 1.25
        }
        Cause::Drop => {
            // a falling bomb with fins
            let (bx, w) = (x + h * 0.35, h * 0.5);
            draw_circle(bx, cy + h * 0.15, w * 0.5 + 1.0, bg);
            draw_circle(bx, cy + h * 0.15, w * 0.5, fg);
            draw_rectangle(bx - w * 0.5, cy - h * 0.2, w, h * 0.35, fg);
            draw_triangle(
                vec2(bx - w * 0.6, cy - h * 0.45),
                vec2(bx + w * 0.6, cy - h * 0.45),
                vec2(bx, cy - h * 0.15),
                fg,
            );
            h * 0.8
        }
        Cause::Nrk => {
            // a tracked robot with a gun
            let w = h * 1.8;
            draw_rectangle(x, cy + h * 0.1, w * 0.62, h * 0.3, fg);
            draw_rectangle(x + w * 0.08, cy - h * 0.2, w * 0.45, h * 0.3, fg);
            draw_rectangle(x + w * 0.5, cy - h * 0.12, w * 0.5, h * 0.1, fg);
            for k in 0..3 {
                draw_circle(x + w * (0.1 + k as f32 * 0.21), cy + h * 0.25, h * 0.08, bg);
            }
            w
        }
        Cause::Fall | Cause::World | Cause::Zone | Cause::Bleed => {
            // cartoon skull
            let (sx, sy) = (x + h * 0.5, cy);
            draw_circle(sx, sy - h * 0.05, h * 0.42, fg);
            draw_rectangle(sx - h * 0.22, sy + h * 0.15, h * 0.44, h * 0.25, fg);
            draw_circle(sx - h * 0.16, sy - h * 0.05, h * 0.11, bg);
            draw_circle(sx + h * 0.16, sy - h * 0.05, h * 0.11, bg);
            h * 1.1
        }
    }
}

/// Head with a crosshair — shown after the weapon for headshots.
pub(super) fn draw_headshot_icon(cx: f32, cy: f32, h: f32, a: f32) {
    let (fg, bg) = icon_color(a);
    let r = h * 0.38;
    draw_circle(cx, cy, r + 1.2, bg);
    draw_circle(cx, cy, r, fg);
    let red = Color::new(0.95, 0.25, 0.2, a);
    draw_circle_lines(cx, cy, r * 0.75, 1.5, red);
    draw_line(cx - r * 1.2, cy, cx + r * 1.2, cy, 1.5, red);
    draw_line(cx, cy - r * 1.2, cx, cy + r * 1.2, 1.5, red);
}

/// A small bomb (three red sticks and a blinking timer) centred on `(x, y)`.
pub(super) fn draw_bomb_icon(x: f32, y: f32, scale: f32, pulse: f32) {
    let (w, h) = (30.0 * scale, 22.0 * scale);
    let (x0, y0) = (x - w / 2.0, y - h / 2.0);
    soft_rect(
        x0 - 4.0,
        y0 - 4.0,
        w + 8.0,
        h + 8.0,
        Color::new(0.0, 0.0, 0.0, 0.55),
    );
    let sw = w / 3.0;
    for k in 0..3 {
        let sx = x0 + k as f32 * sw;
        draw_rectangle(sx, y0, sw - 1.0, h, Color::from_rgba(214, 60, 48, 255));
        draw_rectangle_lines(sx, y0, sw - 1.0, h, 1.5, BLACK);
    }
    draw_rectangle(
        x0 - 1.0,
        y0 + h * 0.4,
        w + 1.0,
        h * 0.2,
        Color::from_rgba(60, 60, 66, 255),
    );
    let led = if pulse > 0.5 {
        Color::from_rgba(255, 70, 50, 255)
    } else {
        Color::from_rgba(120, 30, 25, 255)
    };
    draw_circle(x, y, 3.0 * scale, led);
    draw_line(
        x0 + w,
        y0 + 3.0,
        x0 + w + 7.0 * scale,
        y0 - 6.0 * scale,
        2.0,
        ACCENT,
    );
}

/// Pictures for the battle royale's equipment.
#[derive(Clone, Copy)]
pub(super) enum Icon {
    Medkit,
    Vest,
    Helmet,
    Backpack,
    Scope,
    RedDot,
}

/// One of the equipment pictures, `sz` pixels across, centred on (cx, cy).
pub(super) fn draw_icon(icon: Icon, cx: f32, cy: f32, sz: f32, c: Color) {
    let u = sz / 24.0;
    let dark = Color::new(0.05, 0.05, 0.08, 0.9);
    match icon {
        Icon::Medkit => {
            round_rect(cx - 11.0 * u, cy - 8.0 * u, 22.0 * u, 17.0 * u, 3.0 * u, c);
            draw_rectangle_lines(cx - 4.0 * u, cy - 11.0 * u, 8.0 * u, 4.0 * u, 2.0 * u, c);
            draw_rectangle(cx - 1.8 * u, cy - 5.5 * u, 3.6 * u, 12.0 * u, WHITE);
            draw_rectangle(cx - 6.0 * u, cy - 1.3 * u, 12.0 * u, 3.6 * u, WHITE);
        }
        Icon::Vest => {
            // the body, the arm holes cut out and the neck between the shoulder straps
            draw_rectangle(cx - 9.0 * u, cy - 6.0 * u, 18.0 * u, 17.0 * u, c);
            draw_rectangle(cx - 9.0 * u, cy - 11.0 * u, 5.0 * u, 6.0 * u, c);
            draw_rectangle(cx + 4.0 * u, cy - 11.0 * u, 5.0 * u, 6.0 * u, c);
            draw_triangle(
                vec2(cx - 4.0 * u, cy - 6.0 * u),
                vec2(cx + 4.0 * u, cy - 6.0 * u),
                vec2(cx, cy - 1.0 * u),
                dark,
            );
            draw_line(cx, cy - 1.0 * u, cx, cy + 11.0 * u, 1.5 * u, dark);
            draw_line(
                cx - 9.0 * u,
                cy + 4.0 * u,
                cx + 9.0 * u,
                cy + 4.0 * u,
                1.5 * u,
                dark,
            );
        }
        Icon::Helmet => {
            // a dome over a brim
            let n = 12;
            for i in 0..n {
                let a0 = std::f32::consts::PI * i as f32 / n as f32;
                let a1 = std::f32::consts::PI * (i + 1) as f32 / n as f32;
                draw_triangle(
                    vec2(cx, cy + 3.0 * u),
                    vec2(cx + a0.cos() * 10.0 * u, cy + 3.0 * u - a0.sin() * 11.0 * u),
                    vec2(cx + a1.cos() * 10.0 * u, cy + 3.0 * u - a1.sin() * 11.0 * u),
                    c,
                );
            }
            draw_rectangle(cx - 12.0 * u, cy + 3.0 * u, 24.0 * u, 3.5 * u, c);
            draw_line(
                cx - 7.0 * u,
                cy - 2.0 * u,
                cx + 7.0 * u,
                cy - 2.0 * u,
                1.5 * u,
                dark,
            );
        }
        Icon::Backpack => {
            draw_circle_lines(cx, cy - 9.0 * u, 3.5 * u, 2.0 * u, c);
            round_rect(cx - 8.0 * u, cy - 7.0 * u, 16.0 * u, 18.0 * u, 4.0 * u, c);
            draw_rectangle(cx - 8.0 * u, cy - 1.5 * u, 16.0 * u, 1.5 * u, dark);
            round_rect(cx - 5.0 * u, cy + 2.0 * u, 10.0 * u, 6.0 * u, 1.5 * u, dark);
        }
        Icon::Scope => {
            // a tube with its two bells, on a mount
            draw_rectangle(cx - 9.0 * u, cy - 2.5 * u, 18.0 * u, 5.0 * u, c);
            draw_rectangle(cx - 12.0 * u, cy - 4.5 * u, 5.0 * u, 9.0 * u, c);
            draw_rectangle(cx + 6.0 * u, cy - 4.0 * u, 6.0 * u, 8.0 * u, c);
            draw_rectangle(cx - 1.5 * u, cy - 6.5 * u, 3.0 * u, 4.0 * u, c);
            draw_rectangle(cx - 6.0 * u, cy + 2.5 * u, 3.0 * u, 4.0 * u, c);
            draw_rectangle(cx + 3.0 * u, cy + 2.5 * u, 3.0 * u, 4.0 * u, c);
        }
        Icon::RedDot => {
            draw_circle_lines(cx, cy, 9.0 * u, 2.5 * u, c);
            draw_circle(cx, cy, 2.5 * u, Color::new(1.0, 0.2, 0.15, 1.0));
        }
    }
}

/// A level as three pips, as many lit as the level, coloured by it.
pub(super) fn level_pips(cx: f32, cy: f32, level: u8) {
    let lit = match level {
        1 => Color::new(0.85, 0.85, 0.85, 1.0),
        2 => Color::new(0.4, 0.65, 1.0, 1.0),
        _ => Color::new(0.8, 0.45, 1.0, 1.0),
    };
    for i in 0..3u8 {
        let x = cx + (i as f32 - 1.0) * 11.0;
        if i < level {
            draw_circle(x, cy, 4.0, lit);
        } else {
            draw_circle_lines(x, cy, 3.5, 1.5, Color::new(1.0, 1.0, 1.0, 0.25));
        }
    }
}
