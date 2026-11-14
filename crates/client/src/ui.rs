//! A tiny immediate-mode UI with a chunky cartoon style.

use macroquad::prelude::*;

pub const PANEL: Color = Color::new(0.16, 0.19, 0.29, 0.94);
pub const PANEL2: Color = Color::new(0.22, 0.26, 0.38, 1.0);
pub const ACCENT: Color = Color::new(1.0, 0.79, 0.29, 1.0);
pub const TEXT: Color = Color::new(0.96, 0.95, 0.92, 1.0);
pub const MUTED: Color = Color::new(0.66, 0.69, 0.78, 1.0);
pub const BAD: Color = Color::new(1.0, 0.42, 0.37, 1.0);
pub const GOOD: Color = Color::new(0.37, 0.83, 0.55, 1.0);
pub const SHADOW: Color = Color::new(0.0, 0.0, 0.0, 0.45);

/// A filled rectangle with rounded corners (one fan of triangles, so that translucent
/// colours do not double up anywhere).
pub fn round_rect(x: f32, y: f32, w: f32, h: f32, r: f32, color: Color) {
    let r = r.min(w * 0.5).min(h * 0.5).max(0.0);
    if r < 1.0 {
        draw_rectangle(x, y, w, h, color);
        return;
    }
    const STEPS: usize = 4;
    let c = vec2(x + w * 0.5, y + h * 0.5);
    let mut rim: Vec<Vec2> = Vec::with_capacity(4 * (STEPS + 1));
    // corner centres clockwise from the top-right, each with the angle its arc starts at
    for (k, (cx, cy)) in [
        (x + w - r, y + r),
        (x + w - r, y + h - r),
        (x + r, y + h - r),
        (x + r, y + r),
    ]
    .into_iter()
    .enumerate()
    {
        let a0 = -std::f32::consts::FRAC_PI_2 + k as f32 * std::f32::consts::FRAC_PI_2;
        for s in 0..=STEPS {
            let a = a0 + s as f32 / STEPS as f32 * std::f32::consts::FRAC_PI_2;
            rim.push(vec2(cx + a.cos() * r, cy + a.sin() * r));
        }
    }
    for i in 0..rim.len() {
        draw_triangle(c, rim[i], rim[(i + 1) % rim.len()], color);
    }
}

/// A translucent backing for a piece of the HUD: softly rounded (bars across the whole
/// screen stay square).
pub fn soft_rect(x: f32, y: f32, w: f32, h: f32, color: Color) {
    if w >= screen_width() - 1.0 || h >= screen_height() - 1.0 {
        draw_rectangle(x, y, w, h, color);
    } else {
        round_rect(x, y, w, h, 7.0, color);
    }
}

pub struct Fonts {
    pub regular: Font,
    pub bold: Font,
}

impl Fonts {
    pub fn load() -> Fonts {
        let mut regular =
            load_ttf_font_from_bytes(include_bytes!("../../../assets/fonts/DejaVuSans.ttf"))
                .expect("font");
        let mut bold =
            load_ttf_font_from_bytes(include_bytes!("../../../assets/fonts/DejaVuSans-Bold.ttf"))
                .expect("font");
        regular.set_filter(FilterMode::Linear);
        bold.set_filter(FilterMode::Linear);
        let mut chars: Vec<char> = (32u8..127).map(|c| c as char).collect();
        chars.extend(
            "АБВГҐДЕЄЖЗИІЇЙКЛМНОПРСТУФХЦЧШЩЬЮЯабвгґдеєжзиіїйклмнопрстуфхцчшщьюя'—·«»…×°☠✦★☆▮▄▁"
                .chars(),
        );
        // Grow each atlas while it is still empty (growing later re-packs every glyph, which is
        // slow). A wide but *short* glyph does it: a tall one would leave a huge row in the
        // packer and make every later growth cascade until the texture size overflows.
        regular.populate_font_cache(&['—'], 1000);
        bold.populate_font_cache(&['—'], 1000);
        for size in [
            12u16, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26, 28, 30, 34,
        ] {
            regular.populate_font_cache(&chars, size);
            bold.populate_font_cache(&chars, size);
        }
        // the few big headings only need their own letters
        let big: [(u16, &str); 3] = [
            (64, "TOON WARFARE"),
            (48, "ПЕРЕМОГА!ПОРАЗКАМАТЧ ЗАВЕРШЕНОVICTORYDEFEATMATCH OVER"),
            (46, "0123456789"),
        ];
        for (size, text) in big {
            let chars: Vec<char> = text.chars().collect();
            regular.populate_font_cache(&chars, size);
            bold.populate_font_cache(&chars, size);
        }
        Fonts { regular, bold }
    }
}

pub struct Ui<'a> {
    pub fonts: &'a Fonts,
    pub mouse: Vec2,
    pub clicked: bool,
    pub down: bool,
    /// Id of the focused text field.
    pub focus: &'a mut Option<u32>,
    pub chars: &'a [char],
    pub backspace: bool,
    pub enter: bool,
    pub hot: bool,
}

impl<'a> Ui<'a> {
    pub fn text(&self, s: &str, x: f32, y: f32, size: u16, color: Color) {
        draw_text_ex(
            s,
            x,
            y,
            TextParams {
                font: Some(&self.fonts.regular),
                font_size: size,
                color,
                ..Default::default()
            },
        );
    }
    pub fn bold(&self, s: &str, x: f32, y: f32, size: u16, color: Color) {
        draw_text_ex(
            s,
            x,
            y,
            TextParams {
                font: Some(&self.fonts.bold),
                font_size: size,
                color,
                ..Default::default()
            },
        );
    }
    /// Bold text with a drop shadow (HUD style).
    pub fn shadowed(&self, s: &str, x: f32, y: f32, size: u16, color: Color) {
        self.bold(s, x + 2.0, y + 2.0, size, SHADOW);
        self.bold(s, x, y, size, color);
    }
    /// `s` cut short with «…» to fit in `max_w` pixels.
    pub fn fit(&self, s: &str, size: u16, bold: bool, max_w: f32) -> String {
        if self.width(s, size, bold) <= max_w {
            return s.to_string();
        }
        let chars: Vec<char> = s.chars().collect();
        let (mut lo, mut hi) = (0, chars.len());
        while lo < hi {
            let mid = (lo + hi).div_ceil(2);
            let t: String = chars[..mid].iter().collect::<String>() + "…";
            if self.width(&t, size, bold) <= max_w {
                lo = mid;
            } else {
                hi = mid - 1;
            }
        }
        chars[..lo]
            .iter()
            .collect::<String>()
            .trim_end()
            .to_string()
            + "…"
    }

    pub fn width(&self, s: &str, size: u16, bold: bool) -> f32 {
        measure_text(
            s,
            Some(if bold {
                &self.fonts.bold
            } else {
                &self.fonts.regular
            }),
            size,
            1.0,
        )
        .width
    }
    pub fn centered(&self, s: &str, cx: f32, y: f32, size: u16, color: Color) {
        let w = self.width(s, size, true);
        self.shadowed(s, cx - w / 2.0, y, size, color);
    }

    pub fn panel(&self, x: f32, y: f32, w: f32, h: f32) {
        round_rect(x + 5.0, y + 7.0, w, h, 14.0, SHADOW);
        round_rect(
            x - 2.0,
            y - 2.0,
            w + 4.0,
            h + 4.0,
            14.0,
            Color::new(0.04, 0.05, 0.09, 0.9),
        );
        round_rect(x, y, w, h, 12.0, PANEL);
    }

    fn inside(&self, x: f32, y: f32, w: f32, h: f32) -> bool {
        self.mouse.x >= x && self.mouse.x <= x + w && self.mouse.y >= y && self.mouse.y <= y + h
    }

    pub fn button_colored(
        &mut self,
        label: &str,
        x: f32,
        y: f32,
        w: f32,
        h: f32,
        color: Color,
    ) -> bool {
        let hover = self.inside(x, y, w, h);
        let press = hover && self.down;
        let off = if press { 3.0 } else { 0.0 };
        round_rect(x + 3.0, y + 5.0, w, h, 9.0, SHADOW);
        let c = if hover {
            Color::new(
                (color.r + 0.08).min(1.0),
                (color.g + 0.08).min(1.0),
                (color.b + 0.08).min(1.0),
                1.0,
            )
        } else {
            color
        };
        round_rect(
            x + off - 2.0,
            y + off - 2.0,
            w + 4.0,
            h + 4.0,
            10.0,
            Color::new(0.04, 0.05, 0.09, 0.85),
        );
        round_rect(x + off, y + off, w, h, 8.0, c);
        // (a lighter band along the top: the button catches the light)
        round_rect(
            x + off + 3.0,
            y + off + 2.0,
            w - 6.0,
            h * 0.42,
            6.0,
            Color::new(1.0, 1.0, 1.0, 0.07),
        );
        // a label too long for the button: smaller letters, then cut short
        let room = w - 16.0;
        let mut size = (h * 0.42).clamp(14.0, 26.0) as u16;
        let mut tw = self.width(label, size, true);
        while tw > room && size > 12 {
            size -= 1;
            tw = self.width(label, size, true);
        }
        let short;
        let label = if tw > room {
            short = self.fit(label, size, true, room);
            tw = self.width(&short, size, true);
            short.as_str()
        } else {
            label
        };
        let dark = color.r + color.g + color.b > 1.8;
        self.bold(
            label,
            x + off + (w - tw) / 2.0,
            y + off + h * 0.5 + size as f32 * 0.36,
            size,
            if dark {
                Color::new(0.12, 0.1, 0.1, 1.0)
            } else {
                TEXT
            },
        );
        if hover {
            self.hot = true;
        }
        hover && self.clicked
    }

    pub fn button(&mut self, label: &str, x: f32, y: f32, w: f32, h: f32) -> bool {
        self.button_colored(label, x, y, w, h, ACCENT)
    }

    pub fn button2(&mut self, label: &str, x: f32, y: f32, w: f32, h: f32) -> bool {
        self.button_colored(label, x, y, w, h, PANEL2)
    }

    /// Single-line text field. Returns true when Enter is pressed in it.
    #[allow(clippy::too_many_arguments)]
    pub fn field(
        &mut self,
        id: u32,
        label: &str,
        value: &mut String,
        x: f32,
        y: f32,
        w: f32,
        secret: bool,
        max: usize,
    ) -> bool {
        self.text(label, x, y - 6.0, 16, MUTED);
        let h = 36.0;
        let hover = self.inside(x, y, w, h);
        if self.clicked {
            if hover {
                *self.focus = Some(id);
            } else if *self.focus == Some(id) {
                *self.focus = None;
            }
        }
        let focused = *self.focus == Some(id);
        draw_rectangle(x, y, w, h, Color::new(0.07, 0.08, 0.13, 1.0));
        draw_rectangle_lines(
            x,
            y,
            w,
            h,
            3.0,
            if focused {
                ACCENT
            } else {
                Color::new(0.0, 0.0, 0.0, 0.7)
            },
        );
        let mut entered = false;
        if focused {
            for &c in self.chars {
                if !c.is_control() && value.chars().count() < max {
                    value.push(c);
                }
            }
            if self.backspace {
                value.pop();
            }
            if self.enter {
                entered = true;
            }
        }
        let shown: String = if secret {
            "•".repeat(value.chars().count())
        } else {
            value.clone()
        };
        // keep the tail visible
        let mut s = shown.as_str();
        while self.width(s, 20, false) > w - 16.0 && !s.is_empty() {
            let mut it = s.chars();
            it.next();
            s = it.as_str();
        }
        self.text(s, x + 8.0, y + 25.0, 20, TEXT);
        if focused && (get_time() * 2.0).fract() < 0.5 {
            let cx = x + 10.0 + self.width(s, 20, false);
            draw_line(cx, y + 7.0, cx, y + h - 7.0, 2.0, ACCENT);
        }
        entered
    }

    /// Horizontal slider. Returns the new value.
    #[allow(clippy::too_many_arguments)]
    pub fn slider(
        &mut self,
        label: &str,
        v: f32,
        min: f32,
        max: f32,
        x: f32,
        y: f32,
        w: f32,
        fmt: &str,
    ) -> f32 {
        self.text(
            &format!("{label}: {}", fmt.replace("{}", &format!("{:.1}", v))),
            x,
            y - 6.0,
            16,
            MUTED,
        );
        let h = 22.0;
        draw_rectangle(x, y + 6.0, w, 10.0, Color::new(0.07, 0.08, 0.13, 1.0));
        let k = ((v - min) / (max - min)).clamp(0.0, 1.0);
        draw_rectangle(x, y + 6.0, w * k, 10.0, ACCENT);
        draw_circle(x + w * k, y + 11.0, 11.0, TEXT);
        draw_circle_lines(
            x + w * k,
            y + 11.0,
            11.0,
            2.0,
            Color::new(0.0, 0.0, 0.0, 0.7),
        );
        if self.down && self.inside(x - 10.0, y - 4.0, w + 20.0, h + 8.0) {
            let k = ((self.mouse.x - x) / w).clamp(0.0, 1.0);
            return min + (max - min) * k;
        }
        v
    }

    pub fn toggle(&mut self, label: &str, v: &mut bool, x: f32, y: f32) {
        let s = 26.0;
        let hover = self.inside(x, y, s + 10.0 + self.width(label, 18, false), s);
        draw_rectangle(x, y, s, s, Color::new(0.07, 0.08, 0.13, 1.0));
        draw_rectangle_lines(x, y, s, s, 3.0, Color::new(0.0, 0.0, 0.0, 0.7));
        if *v {
            draw_rectangle(x + 5.0, y + 5.0, s - 10.0, s - 10.0, ACCENT);
        }
        self.text(label, x + s + 10.0, y + 19.0, 18, TEXT);
        if hover && self.clicked {
            *v = !*v;
        }
    }

    /// "< value >" selector. Returns -1/0/+1.
    pub fn stepper(&mut self, label: &str, value: &str, x: f32, y: f32, w: f32) -> i32 {
        self.text(label, x, y - 6.0, 16, MUTED);
        let mut r = 0;
        if self.button2("<", x, y, 36.0, 34.0) {
            r = -1;
        }
        if self.button2(">", x + w - 36.0, y, 36.0, 34.0) {
            r = 1;
        }
        draw_rectangle(
            x + 42.0,
            y,
            w - 84.0,
            34.0,
            Color::new(0.07, 0.08, 0.13, 1.0),
        );
        // a long value gets a smaller font instead of running over the arrows
        let room = w - 92.0;
        let mut size = 18u16;
        while size > 12 && self.width(value, size, true) > room {
            size -= 1;
        }
        let tw = self.width(value, size, true);
        self.bold(
            value,
            x + (w - tw) / 2.0,
            y + 17.0 + size as f32 / 3.0,
            size,
            TEXT,
        );
        r
    }
}
