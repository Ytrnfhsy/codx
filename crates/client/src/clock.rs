//! The game's clock. Normally macroquad's (seconds since start, and the last frame's
//! length); with `--fixed-clock` (screenshots that must come out the same every run) every
//! frame lasts exactly `FIXED_STEP`, whatever the machine does meanwhile.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

/// One frame of the fixed clock (s).
pub const FIXED_STEP: f64 = 1.0 / 30.0;

static FIXED: AtomicBool = AtomicBool::new(false);
static FRAMES: AtomicU64 = AtomicU64::new(0);

/// From now on every frame lasts `FIXED_STEP`.
pub fn fix() {
    FIXED.store(true, Ordering::Relaxed);
}

/// The fixed clock is on (screenshots): randomness should be fixed too.
pub fn is_fixed() -> bool {
    FIXED.load(Ordering::Relaxed)
}

/// Called once a frame, after it is shown.
pub fn frame_done() {
    FRAMES.fetch_add(1, Ordering::Relaxed);
}

/// Seconds since the game started.
pub fn now() -> f64 {
    if FIXED.load(Ordering::Relaxed) {
        // (not from zero: some timers treat 0 as «never»)
        1.0 + FRAMES.load(Ordering::Relaxed) as f64 * FIXED_STEP
    } else {
        macroquad::time::get_time()
    }
}

/// How long the last frame took (s).
pub fn frame_time() -> f32 {
    if FIXED.load(Ordering::Relaxed) {
        FIXED_STEP as f32
    } else {
        macroquad::time::get_frame_time()
    }
}
