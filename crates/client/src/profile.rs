//! `--profile SECS`: the client measures its frames (after a warm-up on the screen being
//! measured), writes a report and quits. The sections come from `toon_shared::prof`
//! scopes around the frame's parts; the whole frame and the wait for it to be shown are
//! added here.

use crate::settings::Settings;
use std::time::Instant;
use toon_shared::prof;

/// Seconds on the screen being measured before measuring starts (loading, the server
/// starting, the first fall of the battle royale's frames are not counted).
const WARM_UP: f32 = 5.0;

pub struct ProfileRun {
    secs: f32,
    out: String,
    /// Measuring a game (`--host` or `--connect`), not the menu.
    game: bool,
    on_screen_since: Option<Instant>,
    started: Option<Instant>,
}

impl ProfileRun {
    pub fn new(secs: f32, out: Option<String>, game: bool) -> ProfileRun {
        prof::enable();
        ProfileRun {
            secs,
            out: out.unwrap_or_else(|| "profile.md".into()),
            game,
            on_screen_since: None,
            started: None,
        }
    }

    /// Once a frame, after it is shown: true once the measuring is over and the report
    /// written.
    pub fn frame(&mut self, in_game: bool, present_ms: f64, frame_ms: f64, s: &Settings) -> bool {
        prof::time("present (swap, GPU, vsync)", present_ms);
        prof::time("frame", frame_ms);
        prof::frame_end();
        if in_game != self.game {
            self.on_screen_since = None;
            return false;
        }
        let since = *self.on_screen_since.get_or_insert_with(Instant::now);
        match self.started {
            None if since.elapsed().as_secs_f32() >= WARM_UP => {
                prof::reset();
                self.started = Some(Instant::now());
                false
            }
            Some(t) if t.elapsed().as_secs_f32() >= self.secs => {
                let report = report(s);
                match std::fs::write(&self.out, &report) {
                    Ok(()) => println!("profile written: {}", self.out),
                    Err(e) => println!("profile: cannot write {}: {e}\n{report}", self.out),
                }
                true
            }
            _ => false,
        }
    }
}

/// The report: the command line, the video settings, the frame's time (mean, p99, the 1%
/// low) and every section.
fn report(s: &Settings) -> String {
    let (frames, lines) = prof::lines();
    let mut ft = prof::series("frame");
    ft.sort_by(|a, b| b.total_cmp(a));
    let n = ft.len().max(1);
    let mean = ft.iter().map(|x| *x as f64).sum::<f64>() / n as f64;
    // the 1% low: the frame rate of the slowest 1% of frames, on average
    let worst = &ft[..(n / 100).max(1).min(ft.len())];
    let worst_mean = worst.iter().map(|x| *x as f64).sum::<f64>() / worst.len().max(1) as f64;
    let mut sorted = ft.clone();
    sorted.reverse();
    let p99 = prof::percentile(&sorted, 0.99);
    let fps = |ms: f64| if ms > 0.0 { 1000.0 / ms } else { 0.0 };
    let args: Vec<String> = std::env::args().skip(1).collect();
    format!(
        "# Client profile\n\n`toon-warfare {}`\n\n\
         {} × {} px, render scale {:.2}, shadows {} (quality {}), textures {}, outlines {}, \
         MSAA {}, FXAA {}, bloom {}, grass {}, effects {}, view distance {:.1}, vsync {}\n\n\
         {frames} frames: **{:.1} FPS on average ({mean:.2} ms), p99 {p99:.2} ms, \
         1% low {:.1} FPS ({worst_mean:.2} ms)**\n\n{}",
        args.join(" "),
        macroquad::window::screen_width(),
        macroquad::window::screen_height(),
        s.render_scale,
        s.shadows,
        s.shadow_quality,
        s.textures,
        s.outlines,
        s.msaa,
        s.fxaa,
        s.bloom,
        s.grass,
        s.effects,
        s.view_distance,
        crate::VSYNC_AT_START.load(std::sync::atomic::Ordering::Relaxed),
        fps(mean),
        fps(worst_mean),
        prof::table(&lines)
    )
}
