//! Looking for and installing game updates in the background (release builds only), and
//! the small banner in the main menu that offers them.

use crate::ui::*;
use macroquad::prelude::*;
use std::sync::{Arc, Mutex};
use toon_server::update::{self, Release};
use toon_shared::consts::GAME_VERSION;

const BIN: &str = "toon-warfare";

#[derive(Clone)]
pub enum State {
    /// Not looked yet (a build from source looks only when a server asks for an
    /// update), or turned off (`--no-update`).
    Off,
    Checking,
    UpToDate,
    /// The check failed (no internet, …): stay quiet unless a server asks for an update.
    CheckFailed(String),
    Available(Release),
    Downloading(String, f32),
    /// Installed: restart now.
    Installed(String),
    Failed(String),
}

pub struct Updater {
    state: Arc<Mutex<State>>,
    enabled: bool,
}

impl Updater {
    /// `enabled`: may look for updates at all; `at_start`: look right away (release
    /// builds), otherwise only when `check` is called.
    pub fn new(enabled: bool, at_start: bool) -> Updater {
        let u = Updater {
            state: Arc::new(Mutex::new(State::Off)),
            enabled,
        };
        if at_start {
            u.check();
        }
        u
    }

    pub fn state(&self) -> State {
        self.state.lock().map(|s| s.clone()).unwrap_or(State::Off)
    }

    fn set(state: &Arc<Mutex<State>>, s: State) {
        if let Ok(mut g) = state.lock() {
            *g = s;
        }
    }

    /// Asks GitHub for a newer release (in the background).
    pub fn check(&self) {
        if !self.enabled || matches!(self.state(), State::Checking | State::Downloading(..)) {
            return;
        }
        let state = self.state.clone();
        Self::set(&state, State::Checking);
        std::thread::spawn(move || {
            let s = match update::check(BIN) {
                Ok(Some(r)) => State::Available(r),
                Ok(None) => State::UpToDate,
                Err(e) => State::CheckFailed(e),
            };
            Self::set(&state, s);
        });
    }

    /// Downloads and installs `r` (in the background).
    pub fn install(&self, r: Release) {
        let state = self.state.clone();
        let v = r.version.clone();
        Self::set(&state, State::Downloading(v.clone(), 0.0));
        std::thread::spawn(move || {
            let st = state.clone();
            let res = update::install(&r, |p| Self::set(&st, State::Downloading(v.clone(), p)));
            Self::set(
                &state,
                match res {
                    Ok(()) => State::Installed(r.version.clone()),
                    Err(e) => State::Failed(e),
                },
            );
        });
    }

    /// The banner in the top right corner of the menu. `outdated` = a server just turned
    /// us away for an old version. Returns true when the game should restart now.
    pub fn draw(&self, ui: &mut Ui, outdated: bool) -> bool {
        let (x, y, pw) = (screen_width() - 356.0, 14.0, 342.0);
        let box_ = |_: &Ui, h: f32, warn: bool| {
            draw_rectangle(x, y, pw, h, Color::new(0.05, 0.07, 0.1, 0.85));
            draw_rectangle_lines(x, y, pw, h, 2.0, if warn { BAD } else { ACCENT });
        };
        match self.state() {
            State::Available(r) => {
                box_(ui, 92.0, outdated);
                ui.shadowed(
                    &format!("Доступне оновлення v{}", r.version),
                    x + 14.0,
                    y + 26.0,
                    18,
                    ACCENT,
                );
                let mb = r.size as f32 / 1_048_576.0;
                ui.shadowed(
                    &format!("у тебе v{GAME_VERSION} · {mb:.0} МБ"),
                    x + 14.0,
                    y + 46.0,
                    14,
                    MUTED,
                );
                if ui.button("Оновити", x + 14.0, y + 54.0, pw - 28.0, 32.0) {
                    self.install(r);
                }
            }
            State::Downloading(v, p) => {
                box_(ui, 64.0, false);
                ui.shadowed(
                    &format!("Завантаження v{v}… {:.0}%", p * 100.0),
                    x + 14.0,
                    y + 26.0,
                    18,
                    TEXT,
                );
                draw_rectangle(
                    x + 14.0,
                    y + 40.0,
                    pw - 28.0,
                    12.0,
                    Color::new(0.2, 0.2, 0.2, 0.9),
                );
                draw_rectangle(x + 14.0, y + 40.0, (pw - 28.0) * p, 12.0, ACCENT);
            }
            State::Installed(v) => {
                box_(ui, 44.0, false);
                ui.shadowed(
                    &format!("Оновлено до v{v}, перезапуск…"),
                    x + 14.0,
                    y + 28.0,
                    18,
                    GOOD,
                );
                return true;
            }
            State::Failed(e) => {
                box_(ui, 92.0, true);
                ui.shadowed("Не вдалося оновити", x + 14.0, y + 26.0, 18, BAD);
                ui.shadowed(&e, x + 14.0, y + 46.0, 13, MUTED);
                if ui.button2("Спробувати ще", x + 14.0, y + 54.0, pw - 28.0, 32.0) {
                    self.check();
                }
            }
            State::Checking if outdated => {
                box_(ui, 44.0, false);
                ui.shadowed("Шукаю оновлення…", x + 14.0, y + 28.0, 18, TEXT);
            }
            State::CheckFailed(e) if outdated => {
                box_(ui, 92.0, true);
                ui.shadowed("Не вдалося знайти оновлення", x + 14.0, y + 26.0, 18, BAD);
                ui.shadowed(&e, x + 14.0, y + 46.0, 13, MUTED);
                if ui.button2("Спробувати ще", x + 14.0, y + 54.0, pw - 28.0, 32.0) {
                    self.check();
                }
            }
            State::UpToDate if outdated => {
                box_(ui, 64.0, true);
                ui.shadowed(
                    "Новішої версії ще не опубліковано —",
                    x + 14.0,
                    y + 26.0,
                    16,
                    TEXT,
                );
                ui.shadowed("спробуй за кілька хвилин", x + 14.0, y + 48.0, 16, TEXT);
            }
            State::Off if outdated && !self.enabled => {
                box_(ui, 64.0, true);
                ui.shadowed(
                    "Оновлення вимкнено (--no-update):",
                    x + 14.0,
                    y + 26.0,
                    16,
                    TEXT,
                );
                ui.shadowed(
                    "завантаж гру з Releases на GitHub",
                    x + 14.0,
                    y + 48.0,
                    16,
                    TEXT,
                );
            }
            _ => {}
        }
        false
    }
}
