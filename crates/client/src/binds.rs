//! Key bindings: every game action has two slots, each a keyboard key or a mouse button,
//! chosen by the player on the Controls page and saved with the settings.
//!
//! The game asks `binds::down(Action::Fire)` (or `pressed`, `released`) instead of looking
//! at a fixed key. Escape, Enter and Backspace stay fixed: they drive the menus.

use macroquad::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};
use toon_shared::lang::t;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Action {
    Forward,
    Back,
    Left,
    Right,
    Jump,
    Crouch,
    Prone,
    Sprint,
    Fire,
    Aim,
    Reload,
    Primary,
    Secondary,
    Grenade,
    Smoke,
    Knife,
    Use,
    Killstreak,
    Scoreboard,
    ChatAll,
    ChatTeam,
    Class,
    Leave,
}

impl Action {
    pub const ALL: [Action; 23] = [
        Action::Forward,
        Action::Back,
        Action::Left,
        Action::Right,
        Action::Jump,
        Action::Crouch,
        Action::Prone,
        Action::Sprint,
        Action::Fire,
        Action::Aim,
        Action::Reload,
        Action::Primary,
        Action::Secondary,
        Action::Grenade,
        Action::Smoke,
        Action::Knife,
        Action::Use,
        Action::Killstreak,
        Action::Scoreboard,
        Action::ChatAll,
        Action::ChatTeam,
        Action::Class,
        Action::Leave,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Action::Forward => t("Вперед", "Forward"),
            Action::Back => t("Назад", "Back"),
            Action::Left => t("Ліворуч", "Left"),
            Action::Right => t("Праворуч", "Right"),
            Action::Jump => t("Стрибок / вгору", "Jump / up"),
            Action::Crouch => t("Присісти", "Crouch"),
            Action::Prone => t("Лягти / вниз", "Prone / down"),
            Action::Sprint => t("Біг / дихання", "Sprint / breath"),
            Action::Fire => t("Вогонь", "Fire"),
            Action::Aim => t("Приціл", "Aim"),
            Action::Reload => t("Перезарядка", "Reload"),
            Action::Primary => t("Основна зброя", "Primary"),
            Action::Secondary => t("Додаткова зброя", "Secondary"),
            Action::Grenade => t("Граната", "Grenade"),
            Action::Smoke => t("Димова граната", "Smoke grenade"),
            Action::Knife => t("Ніж", "Knife"),
            Action::Use => t("Дія (бомба, зброя)", "Use (bomb, gun)"),
            Action::Killstreak => t("Кілстрік", "Killstreak"),
            Action::Scoreboard => t("Рахунок", "Scoreboard"),
            Action::ChatAll => t("Чат: всім", "Chat: all"),
            Action::ChatTeam => t("Чат: команді", "Chat: team"),
            Action::Class => t("Змінити клас", "Change class"),
            Action::Leave => t("Вийти з техніки", "Leave vehicle"),
        }
    }

    /// The factory keys: (main, extra).
    fn defaults(self) -> [Bind; 2] {
        use KeyCode as K;
        let k = Bind::Key;
        match self {
            Action::Forward => [k(K::W), k(K::Up)],
            Action::Back => [k(K::S), k(K::Down)],
            Action::Left => [k(K::A), k(K::Left)],
            Action::Right => [k(K::D), k(K::Right)],
            Action::Jump => [k(K::Space), Bind::None],
            Action::Crouch => [k(K::C), Bind::None],
            Action::Prone => [k(K::LeftControl), k(K::Z)],
            Action::Sprint => [k(K::LeftShift), Bind::None],
            Action::Fire => [Bind::Mouse(0), Bind::None],
            Action::Aim => [Bind::Mouse(1), Bind::None],
            Action::Reload => [k(K::R), Bind::None],
            Action::Primary => [k(K::Key1), Bind::None],
            Action::Secondary => [k(K::Key2), Bind::None],
            Action::Grenade => [k(K::G), Bind::None],
            Action::Smoke => [k(K::Q), Bind::None],
            Action::Knife => [k(K::V), Bind::None],
            Action::Use => [k(K::F), Bind::None],
            Action::Killstreak => [k(K::Key5), k(K::Key4)],
            Action::Scoreboard => [k(K::Tab), Bind::None],
            Action::ChatAll => [k(K::T), Bind::None],
            Action::ChatTeam => [k(K::Y), Bind::None],
            Action::Class => [k(K::M), Bind::None],
            Action::Leave => [k(K::E), Bind::None],
        }
    }
}

/// One slot: a key, a mouse button (0 left, 1 right, 2 middle) or nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Bind {
    None,
    Key(KeyCode),
    Mouse(u8),
}

fn mouse(b: u8) -> MouseButton {
    match b {
        0 => MouseButton::Left,
        1 => MouseButton::Right,
        _ => MouseButton::Middle,
    }
}

impl Bind {
    fn down(self) -> bool {
        match self {
            Bind::None => false,
            Bind::Key(k) => is_key_down(k),
            Bind::Mouse(b) => is_mouse_button_down(mouse(b)),
        }
    }
    fn pressed(self) -> bool {
        match self {
            Bind::None => false,
            Bind::Key(k) => is_key_pressed(k),
            Bind::Mouse(b) => is_mouse_button_pressed(mouse(b)),
        }
    }

    /// How it reads on screen ("W", "Shift", "ЛКМ").
    pub fn label(self) -> String {
        match self {
            Bind::None => "—".into(),
            Bind::Mouse(0) => t("ЛКМ", "LMB").into(),
            Bind::Mouse(1) => t("ПКМ", "RMB").into(),
            Bind::Mouse(_) => t("СКМ", "MMB").into(),
            Bind::Key(k) => match k {
                KeyCode::Space => t("Пробіл", "Space").into(),
                KeyCode::LeftShift => "Shift".into(),
                KeyCode::RightShift => t("Пр. Shift", "R Shift").into(),
                KeyCode::LeftControl => "Ctrl".into(),
                KeyCode::RightControl => t("Пр. Ctrl", "R Ctrl").into(),
                KeyCode::LeftAlt => "Alt".into(),
                KeyCode::RightAlt => t("Пр. Alt", "R Alt").into(),
                KeyCode::Up => "↑".into(),
                KeyCode::Down => "↓".into(),
                KeyCode::Left => "←".into(),
                KeyCode::Right => "→".into(),
                KeyCode::CapsLock => "Caps".into(),
                other => {
                    let n = key_name(other);
                    n.strip_prefix("Key").unwrap_or(&n).to_string()
                }
            },
        }
    }

    /// Saved form: "W", "LeftShift", "Mouse0", "".
    fn save(self) -> String {
        match self {
            Bind::None => String::new(),
            Bind::Key(k) => key_name(k),
            Bind::Mouse(b) => format!("Mouse{b}"),
        }
    }
    fn load(s: &str) -> Bind {
        if s.is_empty() {
            return Bind::None;
        }
        if let Some(b) = s.strip_prefix("Mouse").and_then(|n| n.parse().ok()) {
            return Bind::Mouse(b);
        }
        KEYS.iter()
            .find(|k| key_name(**k) == s)
            .map(|k| Bind::Key(*k))
            .unwrap_or(Bind::None)
    }
}

fn key_name(k: KeyCode) -> String {
    format!("{k:?}")
}

/// Keys a player may bind (Escape, Enter and Backspace stay with the menus).
const KEYS: &[KeyCode] = {
    use KeyCode::*;
    &[
        A,
        B,
        C,
        D,
        E,
        F,
        G,
        H,
        I,
        J,
        K,
        L,
        M,
        N,
        O,
        P,
        Q,
        R,
        S,
        T,
        U,
        V,
        W,
        X,
        Y,
        Z,
        Key0,
        Key1,
        Key2,
        Key3,
        Key4,
        Key5,
        Key6,
        Key7,
        Key8,
        Key9,
        Space,
        Tab,
        LeftShift,
        RightShift,
        LeftControl,
        RightControl,
        LeftAlt,
        RightAlt,
        CapsLock,
        Up,
        Down,
        Left,
        Right,
        Insert,
        Delete,
        Home,
        End,
        PageUp,
        PageDown,
        F1,
        F2,
        F3,
        F4,
        F5,
        F6,
        F7,
        F8,
        F9,
        F10,
        F11,
        F12,
        Kp0,
        Kp1,
        Kp2,
        Kp3,
        Kp4,
        Kp5,
        Kp6,
        Kp7,
        Kp8,
        Kp9,
        KpAdd,
        KpSubtract,
        KpMultiply,
        KpDivide,
        KpDecimal,
        Apostrophe,
        Comma,
        Minus,
        Period,
        Slash,
        Semicolon,
        Equal,
        LeftBracket,
        Backslash,
        RightBracket,
        GraveAccent,
    ]
};

/// The player's bindings (saved as `{"Fire": ["Mouse0", ""], …}`; missing actions get
/// the factory keys).
#[derive(Clone, Debug, PartialEq)]
pub struct Binds {
    map: BTreeMap<Action, [Bind; 2]>,
}

impl Default for Binds {
    fn default() -> Self {
        Binds {
            map: Action::ALL.iter().map(|a| (*a, a.defaults())).collect(),
        }
    }
}

impl Serialize for Binds {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let m: BTreeMap<Action, [String; 2]> = self
            .map
            .iter()
            .map(|(a, b)| (*a, [b[0].save(), b[1].save()]))
            .collect();
        m.serialize(s)
    }
}

impl<'de> Deserialize<'de> for Binds {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        // unknown action names are skipped, missing ones keep their factory keys
        let raw: BTreeMap<String, [String; 2]> = BTreeMap::deserialize(d)?;
        let mut b = Binds::default();
        for (name, slots) in raw {
            if let Ok(a) = serde_json::from_value::<Action>(serde_json::Value::String(name)) {
                b.map
                    .insert(a, [Bind::load(&slots[0]), Bind::load(&slots[1])]);
            }
        }
        Ok(b)
    }
}

impl Binds {
    pub fn get(&self, a: Action) -> [Bind; 2] {
        self.map.get(&a).copied().unwrap_or_else(|| a.defaults())
    }

    /// Puts `bind` into slot `slot` of `a`, taking it off any other action first.
    pub fn set(&mut self, a: Action, slot: usize, bind: Bind) {
        if bind != Bind::None {
            for v in self.map.values_mut() {
                for s in v.iter_mut() {
                    if *s == bind {
                        *s = Bind::None;
                    }
                }
            }
        }
        self.map.entry(a).or_insert_with(|| a.defaults())[slot.min(1)] = bind;
    }
}

fn current() -> &'static Mutex<Binds> {
    static B: OnceLock<Mutex<Binds>> = OnceLock::new();
    B.get_or_init(|| Mutex::new(Binds::default()))
}

/// Makes `b` the bindings the game reads from now on.
pub fn use_binds(b: &Binds) {
    if let Ok(mut c) = current().lock() {
        *c = b.clone();
    }
}

fn slots(a: Action) -> [Bind; 2] {
    current()
        .lock()
        .map(|b| b.get(a))
        .unwrap_or_else(|_| a.defaults())
}

/// The action's key or button is held.
pub fn down(a: Action) -> bool {
    slots(a).iter().any(|b| b.down())
}

/// It went down this frame.
pub fn pressed(a: Action) -> bool {
    slots(a).iter().any(|b| b.pressed())
}

/// Its main key as shown in hints ("F", "ЛКМ").
pub fn key(a: Action) -> String {
    let s = slots(a);
    if s[0] != Bind::None { s[0] } else { s[1] }.label()
}

/// While the Controls page waits for a key, the game and the menus ignore input.
static CAPTURING: Mutex<bool> = Mutex::new(false);

pub fn capturing() -> bool {
    CAPTURING.lock().map(|c| *c).unwrap_or(false)
}

fn set_capturing(v: bool) {
    if let Ok(mut c) = CAPTURING.lock() {
        *c = v;
    }
}

/// The Controls page's state: which slot waits for a key.
#[derive(Default)]
pub struct Editor {
    listening: Option<(Action, usize)>,
}

impl Editor {
    /// Draws the bindings in two columns at (x, y) across `w`. Click a slot, then press a
    /// key or a mouse button; Escape cancels, Backspace clears. Returns true when `b`
    /// changed.
    pub fn draw(&mut self, ui: &mut crate::ui::Ui, b: &mut Binds, x: f32, y: f32, w: f32) -> bool {
        use crate::ui::*;
        let mut changed = false;
        // a click that binds a mouse button must not also press a slot
        let was_listening = self.listening.is_some();
        // a key or a button for the slot that is waiting
        if let Some((a, slot)) = self.listening {
            let got = if is_key_pressed(KeyCode::Escape) {
                Some(None)
            } else if is_key_pressed(KeyCode::Backspace) {
                Some(Some(Bind::None))
            } else if let Some(k) = get_last_key_pressed().filter(|k| KEYS.contains(k)) {
                Some(Some(Bind::Key(k)))
            } else if is_mouse_button_pressed(MouseButton::Left) {
                Some(Some(Bind::Mouse(0)))
            } else if is_mouse_button_pressed(MouseButton::Right) {
                Some(Some(Bind::Mouse(1)))
            } else if is_mouse_button_pressed(MouseButton::Middle) {
                Some(Some(Bind::Mouse(2)))
            } else {
                None
            };
            if let Some(r) = got {
                if let Some(bind) = r {
                    b.set(a, slot, bind);
                    changed = true;
                }
                self.listening = None;
            }
        }
        let rows = Action::ALL.len().div_ceil(2);
        let col_w = (w - 20.0) / 2.0;
        let slot_w = 78.0;
        for (i, a) in Action::ALL.iter().enumerate() {
            let (col, row) = (i / rows, i % rows);
            let cx = x + col as f32 * (col_w + 20.0);
            let cy = y + row as f32 * 34.0;
            ui.text(a.name(), cx, cy + 21.0, 15, TEXT);
            for slot in 0..2 {
                let bx = cx + col_w - (2 - slot) as f32 * (slot_w + 6.0);
                let waiting = self.listening == Some((*a, slot));
                let label = if waiting {
                    "…".to_string()
                } else {
                    b.get(*a)[slot].label()
                };
                // while a slot waits for a key, clicks go to it, not to other buttons
                let clicked = ui.button2(&label, bx, cy, slot_w, 30.0) && !was_listening;
                if waiting {
                    draw_rectangle_lines(bx, cy, slot_w, 30.0, 3.0, ACCENT);
                }
                if clicked {
                    self.listening = Some((*a, slot));
                }
            }
        }
        let hint = if self.listening.is_some() {
            t(
                "Натисни клавішу або кнопку миші · Esc — скасувати · Backspace — очистити",
                "Press a key or a mouse button · Esc — cancel · Backspace — clear",
            )
        } else {
            t(
                "Клацни клавішу, щоб змінити. У кожної дії дві клавіші.",
                "Click a key to change it. Every action has two.",
            )
        };
        ui.text(hint, x, y + rows as f32 * 34.0 + 22.0, 14, MUTED);
        set_capturing(self.listening.is_some());
        changed
    }

    pub fn listening(&self) -> bool {
        self.listening.is_some()
    }

    /// Stops waiting (the page was closed).
    pub fn close(&mut self) {
        self.listening = None;
        set_capturing(false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_and_loads_and_moves_a_key() {
        let mut b = Binds::default();
        // F to fire: it leaves "use"
        b.set(Action::Fire, 1, Bind::Key(KeyCode::F));
        assert_eq!(b.get(Action::Fire), [Bind::Mouse(0), Bind::Key(KeyCode::F)]);
        assert_eq!(b.get(Action::Use)[0], Bind::None);
        let json = serde_json::to_string(&b).unwrap();
        let back: Binds = serde_json::from_str(&json).unwrap();
        assert_eq!(back, b);
        // an old or partial file keeps the factory keys for what it lacks
        let partial: Binds =
            serde_json::from_str(r#"{"Jump": ["J", ""], "Nonsense": ["X", ""]}"#).unwrap();
        assert_eq!(partial.get(Action::Jump)[0], Bind::Key(KeyCode::J));
        assert_eq!(partial.get(Action::Fire)[0], Bind::Mouse(0));
        assert_eq!(Bind::Key(KeyCode::Key5).label(), "5");
    }
}
