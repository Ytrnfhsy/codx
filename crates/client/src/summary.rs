//! The screen between matches: who won, the match's best player and the best at each
//! thing on the left, its best moments replayed on the right, the next map counting down.

use crate::game::GameClient;
use crate::ui::*;
use macroquad::prelude::*;
use toon_shared::lang::t;
use toon_shared::protocol::{Award, AwardKind};
use toon_shared::tr;
use toon_shared::Team;

/// The layout's unit: 1 at 1920×1080.
fn unit() -> f32 {
    (screen_width() / 1920.0).min(screen_height() / 1080.0)
}

/// The big replay window, in pixels (the 3D view is drawn into a target this size).
pub fn reel_size() -> (u32, u32) {
    let k = unit();
    (
        (560.0 * k).round().max(64.0) as u32,
        (315.0 * k).round().max(36.0) as u32,
    )
}

fn award_value(a: &Award) -> String {
    let v = a.value.round() as i64;
    match a.kind {
        AwardKind::Longest => tr!("{v} м", "{v} m"),
        AwardKind::Accuracy => format!("{v} %"),
        _ => format!("{v}"),
    }
}

/// A small drawn icon for an award, centred on (x, y).
fn award_icon(kind: AwardKind, x: f32, y: f32, s: f32) {
    let c = DARK;
    let w = (s * 0.12).max(2.0);
    match kind {
        AwardKind::Kills => {
            draw_line(x - s * 0.3, y - s * 0.3, x + s * 0.3, y + s * 0.3, w, c);
            draw_line(x - s * 0.3, y + s * 0.3, x + s * 0.3, y - s * 0.3, w, c);
        }
        AwardKind::Longest | AwardKind::Accuracy => {
            draw_circle_lines(x, y, s * 0.3, w * 0.8, c);
            if kind == AwardKind::Accuracy {
                draw_circle(x, y, s * 0.1, c);
            } else {
                for (dx, dy) in [(1.0f32, 0.0f32), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)] {
                    draw_line(
                        x + dx * s * 0.18,
                        y + dy * s * 0.18,
                        x + dx * s * 0.42,
                        y + dy * s * 0.42,
                        w * 0.7,
                        c,
                    );
                }
            }
        }
        AwardKind::Streak => {
            let p = [(0.1, -0.38), (-0.16, 0.04), (0.12, 0.0), (-0.1, 0.38)];
            for k in 0..3 {
                draw_line(
                    x + p[k].0 * s,
                    y + p[k].1 * s,
                    x + p[k + 1].0 * s,
                    y + p[k + 1].1 * s,
                    w,
                    c,
                );
            }
        }
        AwardKind::Headshots => {
            draw_circle(x, y - s * 0.06, s * 0.26, c);
            draw_rectangle(x - s * 0.15, y + s * 0.12, s * 0.3, s * 0.18, c);
        }
        AwardKind::Blasts => {
            for i in 0..8 {
                let a = i as f32 * std::f32::consts::FRAC_PI_4;
                let r = if i % 2 == 0 { 0.4 } else { 0.26 };
                draw_line(x, y, x + a.cos() * s * r, y + a.sin() * s * r, w, c);
            }
        }
        AwardKind::Assists => {
            draw_rectangle(x - s * 0.08, y - s * 0.32, s * 0.16, s * 0.64, c);
            draw_rectangle(x - s * 0.32, y - s * 0.08, s * 0.64, s * 0.16, c);
        }
        AwardKind::Arrows => {
            draw_line(x - s * 0.32, y + s * 0.32, x + s * 0.3, y - s * 0.3, w, c);
            draw_triangle(
                vec2(x + s * 0.36, y - s * 0.36),
                vec2(x + s * 0.08, y - s * 0.28),
                vec2(x + s * 0.28, y - s * 0.08),
                c,
            );
        }
        AwardKind::Knife => {
            draw_triangle(
                vec2(x + s * 0.34, y - s * 0.34),
                vec2(x - s * 0.1, y + s * 0.02),
                vec2(x + s * 0.02, y + s * 0.12),
                c,
            );
            draw_line(
                x - s * 0.1,
                y + s * 0.1,
                x - s * 0.3,
                y + s * 0.3,
                w * 1.4,
                c,
            );
        }
        AwardKind::Plants | AwardKind::Defuses => {
            draw_rectangle(x - s * 0.3, y - s * 0.18, s * 0.6, s * 0.36, c);
            draw_rectangle(x - s * 0.06, y - s * 0.34, s * 0.12, s * 0.16, c);
        }
    }
}

fn award_colour(kind: AwardKind) -> Color {
    let c = match kind {
        AwardKind::Kills => [255, 107, 91],
        AwardKind::Longest => [255, 201, 74],
        AwardKind::Streak => [155, 227, 122],
        AwardKind::Accuracy => [107, 184, 255],
        AwardKind::Headshots => [229, 139, 255],
        AwardKind::Blasts => [255, 161, 74],
        AwardKind::Assists => [126, 224, 208],
        AwardKind::Arrows => [201, 178, 122],
        AwardKind::Knife => [190, 196, 210],
        AwardKind::Plants => [240, 120, 90],
        AwardKind::Defuses => [120, 200, 240],
    };
    Color::from_rgba(c[0], c[1], c[2], 255)
}

const DARK: Color = Color::new(0.09, 0.11, 0.17, 1.0);
const CARD: Color = Color::new(0.1, 0.12, 0.18, 0.92);

impl GameClient {
    /// Our side blue, theirs red (spectators see the sides' own colours).
    fn side(&self, t: Team) -> Color {
        if !self.team.is_playing() {
            crate::models::team_color(t)
        } else if t == self.team {
            Color::from_rgba(107, 184, 255, 255)
        } else {
            Color::from_rgba(255, 107, 91, 255)
        }
    }

    pub fn draw_summary(&mut self, ui: &mut Ui) {
        let Some((sum, _)) = self.summary.clone() else {
            return;
        };
        let (w, h) = (screen_width(), screen_height());
        let k = unit();
        let fs = |px: f32| (px * k).round().max(9.0) as u16;
        // the whole screen at 1920×1080, centred
        let (ox, oy) = ((w - 1920.0 * k) / 2.0, (h - 1080.0 * k) / 2.0);
        let x = |v: f32| ox + v * k;
        let y = |v: f32| oy + v * k;
        clear_background(Color::new(0.06, 0.07, 0.11, 1.0));

        // ---- the top: who won, the score, the next map counting down
        round_rect(x(44.0), y(34.0), 1832.0 * k, 120.0 * k, 22.0 * k, CARD);
        let (title, tc) = match self.match_end.as_ref().and_then(|m| m.1) {
            Some(t) => (
                tr!("{} ПЕРЕМОГЛИ", "{} WIN", t.name().to_uppercase()),
                self.side(t),
            ),
            None => (t("НІЧИЯ", "DRAW").to_string(), TEXT),
        };
        ui.bold(&title, x(74.0), y(98.0), fs(46.0), tc);
        let tw = ui.width(&title, fs(46.0), true);
        let sub = format!(
            "{} · {} · {} {}",
            self.mode.name(),
            self.map.id.name(),
            self.scoreboard
                .iter()
                .filter(|r| r.team.is_playing())
                .count(),
            t("гравців", "players")
        );
        ui.text(&sub, x(76.0), y(134.0), fs(20.0), MUTED);
        let sx = x(74.0) + tw + 40.0 * k;
        let a = format!("{}", self.scores[0]);
        ui.bold(&a, sx, y(110.0), fs(46.0), self.side(Team::Allies));
        let aw = ui.width(&a, fs(46.0), true);
        ui.bold(":", sx + aw + 14.0 * k, y(110.0), fs(46.0), TEXT);
        ui.bold(
            &format!("{}", self.scores[1]),
            sx + aw + 44.0 * k,
            y(110.0),
            fs(46.0),
            self.side(Team::Axis),
        );
        // the next map and the time left on a ring
        let left = self.time_left.max(0.0);
        let (rx, ry, rr) = (x(1804.0), y(94.0), 42.0 * k);
        draw_circle(rx, ry, rr, Color::new(1.0, 1.0, 1.0, 0.12));
        let share = (left / sum.secs.max(1.0)).clamp(0.0, 1.0);
        let steps = 48;
        for i in 0..(steps as f32 * share) as i32 {
            let a0 = -std::f32::consts::FRAC_PI_2 + i as f32 / steps as f32 * std::f32::consts::TAU;
            let a1 = a0 + std::f32::consts::TAU / steps as f32;
            draw_triangle(
                vec2(rx, ry),
                vec2(rx + a0.cos() * rr, ry + a0.sin() * rr),
                vec2(rx + a1.cos() * rr, ry + a1.sin() * rr),
                ACCENT,
            );
        }
        draw_circle(rx, ry, rr * 0.79, Color::new(0.1, 0.12, 0.18, 1.0));
        let n = format!("{:.0}", left.ceil());
        let nw = ui.width(&n, fs(32.0), true);
        ui.bold(&n, rx - nw / 2.0, ry + 11.0 * k, fs(32.0), ACCENT);
        let next = format!("{} · {}", sum.next_map.name(), sum.next_mode.name());
        let nl = t("Наступна карта", "Next map");
        let (w1, w2) = (
            ui.width(nl, fs(20.0), false),
            ui.width(&next, fs(26.0), true),
        );
        ui.text(nl, rx - rr - 22.0 * k - w1, y(84.0), fs(20.0), MUTED);
        ui.bold(&next, rx - rr - 22.0 * k - w2, y(116.0), fs(26.0), TEXT);

        // ---- the best player
        let head = |s: &str, hx: f32, hy: f32| ui.bold(s, hx, hy, fs(22.0), ACCENT);
        head(
            t("НАЙКРАЩІ ГРАВЦІ МАТЧУ", "THE MATCH'S BEST"),
            x(48.0),
            y(196.0),
        );
        let (mx, my, mw, mh) = (x(44.0), y(215.0), 420.0 * k, 765.0 * k);
        if let Some(m) = &sum.mvp {
            round_rect(
                mx - 3.0 * k,
                my - 3.0 * k,
                mw + 6.0 * k,
                mh + 6.0 * k,
                24.0 * k,
                ACCENT,
            );
            round_rect(mx, my, mw, mh, 22.0 * k, Color::new(0.15, 0.18, 0.27, 1.0));
            let tag = t("MVP МАТЧУ", "MATCH MVP");
            let tgw = ui.width(tag, fs(20.0), true);
            round_rect(
                mx + 20.0 * k,
                my + 20.0 * k,
                tgw + 28.0 * k,
                38.0 * k,
                10.0 * k,
                ACCENT,
            );
            ui.bold(tag, mx + 34.0 * k, my + 47.0 * k, fs(20.0), DARK);
            // the character turns on its stand (drawn after the HUD, over this)
            self.mvp_preview = Some((
                Rect::new(mx + 40.0 * k, my + 70.0 * k, mw - 80.0 * k, 470.0 * k),
                m.team,
                m.weapon,
            ));
            let name = ui.fit(&m.name, fs(40.0), true, mw - 40.0 * k);
            let nw = ui.width(&name, fs(40.0), true);
            ui.bold(
                &name,
                mx + (mw - nw) / 2.0,
                my + 590.0 * k,
                fs(40.0),
                self.side(m.team),
            );
            let line = format!("{} · {}", m.team.name(), m.weapon.def().name());
            let lw = ui.width(&line, fs(18.0), false);
            ui.text(&line, mx + (mw - lw) / 2.0, my + 622.0 * k, fs(18.0), MUTED);
            let kd = if m.deaths == 0 {
                m.kills as f32
            } else {
                m.kills as f32 / m.deaths as f32
            };
            let stats = [
                (format!("{}", m.kills), t("вбивств", "kills")),
                (format!("{}", m.deaths), t("смертей", "deaths")),
                (format!("{kd:.1}"), "K/D"),
                (format!("{}", m.score), t("очок", "score")),
            ];
            let cw = (mw - 40.0 * k - 30.0 * k) / 4.0;
            for (i, (v, l)) in stats.iter().enumerate() {
                let cx0 = mx + 20.0 * k + i as f32 * (cw + 10.0 * k);
                let cy0 = my + 662.0 * k;
                round_rect(
                    cx0,
                    cy0,
                    cw,
                    76.0 * k,
                    12.0 * k,
                    Color::new(0.0, 0.0, 0.0, 0.25),
                );
                let vw = ui.width(v, fs(28.0), true);
                ui.bold(v, cx0 + (cw - vw) / 2.0, cy0 + 38.0 * k, fs(28.0), TEXT);
                let lw = ui.width(l, fs(14.0), false);
                ui.text(l, cx0 + (cw - lw) / 2.0, cy0 + 62.0 * k, fs(14.0), MUTED);
            }
        } else {
            self.mvp_preview = None;
            round_rect(mx, my, mw, mh, 22.0 * k, CARD);
            ui.centered(
                t("Без MVP цього разу", "No MVP this time"),
                mx + mw / 2.0,
                my + mh / 2.0,
                fs(22.0),
                MUTED,
            );
        }

        // ---- the awards, three by three
        let (gx, gy, gw, gh) = (x(484.0), y(215.0), 804.0 * k, 765.0 * k);
        let (cw, ch) = ((gw - 32.0 * k) / 3.0, (gh - 32.0 * k) / 3.0);
        for (i, a) in sum.awards.iter().take(9).enumerate() {
            let cx0 = gx + (i % 3) as f32 * (cw + 16.0 * k);
            let cy0 = gy + (i / 3) as f32 * (ch + 16.0 * k);
            let mine = a.id == self.my_id;
            if mine {
                round_rect(
                    cx0 - 3.0 * k,
                    cy0 - 3.0 * k,
                    cw + 6.0 * k,
                    ch + 6.0 * k,
                    20.0 * k,
                    ACCENT,
                );
            }
            round_rect(cx0, cy0, cw, ch, 18.0 * k, Color::new(0.1, 0.12, 0.18, 1.0));
            round_rect(
                cx0 + 20.0 * k,
                cy0 + 18.0 * k,
                46.0 * k,
                46.0 * k,
                12.0 * k,
                award_colour(a.kind),
            );
            award_icon(a.kind, cx0 + 43.0 * k, cy0 + 41.0 * k, 46.0 * k);
            if mine {
                let you = t("ТИ", "YOU");
                let yw = ui.width(you, fs(13.0), true);
                round_rect(
                    cx0 + cw - yw - 34.0 * k,
                    cy0 + 16.0 * k,
                    yw + 16.0 * k,
                    22.0 * k,
                    7.0 * k,
                    ACCENT,
                );
                ui.bold(
                    you,
                    cx0 + cw - yw - 26.0 * k,
                    cy0 + 32.0 * k,
                    fs(13.0),
                    DARK,
                );
            }
            ui.text(
                &ui.fit(a.kind.title(), fs(17.0), false, cw - 40.0 * k),
                cx0 + 20.0 * k,
                cy0 + 92.0 * k,
                fs(17.0),
                MUTED,
            );
            ui.bold(
                &award_value(a),
                cx0 + 20.0 * k,
                cy0 + ch - 56.0 * k,
                fs(34.0),
                TEXT,
            );
            let who = match a.weapon {
                Some(wp) => format!("{} · {}", a.name, wp.def().name()),
                None => a.name.clone(),
            };
            ui.bold(
                &ui.fit(&who, fs(19.0), true, cw - 40.0 * k),
                cx0 + 20.0 * k,
                cy0 + ch - 22.0 * k,
                fs(19.0),
                self.side(a.team),
            );
        }
        if sum.awards.is_empty() {
            ui.centered(
                t("Нагород цього разу немає", "No awards this time"),
                gx + gw / 2.0,
                gy + gh / 2.0,
                fs(22.0),
                MUTED,
            );
        }

        // ---- the best moments: the one playing big, the others under it
        head(t("КРАЩІ МОМЕНТИ", "BEST MOMENTS"), x(1320.0), y(196.0));
        let (bx, by) = (x(1316.0), y(215.0));
        let (rw, rh) = (560.0 * k, 315.0 * k);
        let clips = &self.highlights.clips;
        let n = clips.len();
        round_rect(bx, by, rw, rh + 70.0 * k, 18.0 * k, CARD);
        if n == 0 {
            ui.centered(
                t(
                    "Цього разу без яскравих моментів",
                    "No standout moments this time",
                ),
                bx + rw / 2.0,
                by + rh / 2.0,
                fs(20.0),
                MUTED,
            );
        } else {
            let cur = self.reel % n;
            if let (Some(tex), Some(_)) = (&self.reel_texture, &self.killcam) {
                draw_texture_ex(
                    tex,
                    bx,
                    by,
                    WHITE,
                    DrawTextureParams {
                        dest_size: Some(vec2(rw, rh)),
                        flip_y: true,
                        ..Default::default()
                    },
                );
            }
            // the badge, its number, how far it has played
            let rec = t("● ПОВТОР", "● REPLAY");
            let rcw = ui.width(rec, fs(15.0), true);
            round_rect(
                bx + 14.0 * k,
                by + 14.0 * k,
                rcw + 20.0 * k,
                28.0 * k,
                8.0 * k,
                BAD,
            );
            ui.bold(rec, bx + 24.0 * k, by + 34.0 * k, fs(15.0), TEXT);
            let num = format!("{}", cur + 1);
            round_rect(
                bx + rw - 48.0 * k,
                by + 12.0 * k,
                34.0 * k,
                34.0 * k,
                9.0 * k,
                Color::new(0.0, 0.0, 0.0, 0.6),
            );
            ui.bold(&num, bx + rw - 37.0 * k, by + 36.0 * k, fs(18.0), TEXT);
            if let Some(kc) = &self.killcam {
                let p = kc.progress();
                draw_rectangle(
                    bx,
                    by + rh - 7.0 * k,
                    rw,
                    7.0 * k,
                    Color::new(0.0, 0.0, 0.0, 0.5),
                );
                draw_rectangle(bx, by + rh - 7.0 * k, rw * p, 7.0 * k, ACCENT);
            }
            let c = &clips[cur];
            ui.bold(
                &ui.fit(&c.title, fs(22.0), true, rw - 36.0 * k),
                bx + 18.0 * k,
                by + rh + 32.0 * k,
                fs(22.0),
                TEXT,
            );
            let line = format!("{} · {}", c.sub, clock(c.clock));
            ui.text(
                &ui.fit(&line, fs(16.0), false, rw - 36.0 * k),
                bx + 18.0 * k,
                by + rh + 56.0 * k,
                fs(16.0),
                MUTED,
            );
            // the others, in the order they come next
            for j in 1..n.min(3) {
                let c = &clips[(cur + j) % n];
                let sy = by + rh + 70.0 * k + 16.0 * k + (j - 1) as f32 * 166.0 * k;
                let (tw, th) = (250.0 * k, 150.0 * k);
                round_rect(bx, sy, rw, th, 18.0 * k, CARD);
                match &c.thumb {
                    Some(tex) => draw_texture_ex(
                        tex,
                        bx,
                        sy,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(vec2(tw, th)),
                            flip_y: true,
                            ..Default::default()
                        },
                    ),
                    None => {
                        draw_rectangle(bx, sy, tw, th, Color::new(0.16, 0.19, 0.29, 1.0));
                        ui.centered(
                            "▶",
                            bx + tw / 2.0,
                            sy + th / 2.0 + 12.0 * k,
                            fs(34.0),
                            MUTED,
                        );
                    }
                }
                let num = format!("{}", (cur + j) % n + 1);
                round_rect(
                    bx + tw - 46.0 * k,
                    sy + 12.0 * k,
                    34.0 * k,
                    34.0 * k,
                    9.0 * k,
                    Color::new(0.0, 0.0, 0.0, 0.6),
                );
                ui.bold(&num, bx + tw - 35.0 * k, sy + 36.0 * k, fs(18.0), TEXT);
                let tx = bx + tw + 20.0 * k;
                let mw = rw - tw - 36.0 * k;
                ui.bold(
                    &ui.fit(&c.title, fs(21.0), true, mw),
                    tx,
                    sy + 52.0 * k,
                    fs(21.0),
                    TEXT,
                );
                ui.text(
                    &ui.fit(&c.sub, fs(15.0), false, mw),
                    tx,
                    sy + 80.0 * k,
                    fs(15.0),
                    MUTED,
                );
                let more = format!("{} · {}", clock(c.clock), t("далі ▸", "next ▸"));
                ui.text(&more, tx, sy + 104.0 * k, fs(15.0), MUTED);
            }
        }

        // ---- our own match, and the keys
        let mut mine = self
            .scoreboard
            .iter()
            .filter(|r| r.team.is_playing())
            .collect::<Vec<_>>();
        mine.sort_by_key(|r| std::cmp::Reverse(r.score));
        if let Some((place, me)) = mine.iter().enumerate().find(|(_, r)| r.id == self.my_id) {
            let s = tr!(
                "Твій матч:  {} / {}  ·  {} очок  ·  {}-й у таблиці",
                "Your match:  {} / {}  ·  {} points  ·  #{} on the table",
                me.kills,
                me.deaths,
                me.score,
                place + 1
            );
            let sw = ui.width(&s, fs(18.0), false);
            round_rect(x(44.0), y(1000.0), sw + 44.0 * k, 46.0 * k, 14.0 * k, CARD);
            ui.text(&s, x(66.0), y(1030.0), fs(18.0), TEXT);
        }
        let keys = tr!(
            "Tab — таблиця рахунку  ·  {} — наступний момент",
            "Tab — scoreboard  ·  {} — next moment",
            t("Пробіл", "Space")
        );
        let kw = ui.width(&keys, fs(18.0), false);
        ui.text(&keys, x(1876.0) - kw, y(1030.0), fs(18.0), MUTED);
    }
}

/// Match time as m:ss.
fn clock(s: f32) -> String {
    let s = s.max(0.0) as u32;
    format!("{}:{:02}", s / 60, s % 60)
}
