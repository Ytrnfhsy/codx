//! The game's two languages. Every text the player sees is written in both, side by side,
//! at the place it is used: `t("Під'єднатися", "Connect")`, or `tr!` for formatted text.
//!
//! The current language is per thread: the game's main thread uses the player's choice,
//! while a server (even one hosted inside the game) builds each message in its
//! recipient's language with `with`.

use serde::{Deserialize, Serialize};
use std::cell::Cell;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Lang {
    #[default]
    Uk,
    En,
}

impl Lang {
    pub const ALL: [Lang; 2] = [Lang::Uk, Lang::En];

    /// The language's own name.
    pub fn name(self) -> &'static str {
        match self {
            Lang::Uk => "Українська",
            Lang::En => "English",
        }
    }

    /// Picks a language from a locale name such as "uk_UA.UTF-8" or "en-US".
    pub fn from_locale(locale: &str) -> Lang {
        let l = locale.to_lowercase();
        if l.starts_with("uk") {
            Lang::Uk
        } else {
            Lang::En
        }
    }
}

thread_local! {
    static CURRENT: Cell<Lang> = const { Cell::new(Lang::Uk) };
}

pub fn set(l: Lang) {
    CURRENT.with(|c| c.set(l));
}

pub fn get() -> Lang {
    CURRENT.with(|c| c.get())
}

pub fn en() -> bool {
    get() == Lang::En
}

/// Runs `f` with `l` as the current language.
pub fn with<R>(l: Lang, f: impl FnOnce() -> R) -> R {
    let old = get();
    set(l);
    let r = f();
    set(old);
    r
}

/// A text in both languages, for passing around before it is shown.
pub type Text = (&'static str, &'static str);

/// `Text` in the current language.
pub fn pick(x: Text) -> &'static str {
    t(x.0, x.1)
}

/// The text in the current language.
pub fn t(uk: &'static str, en: &'static str) -> &'static str {
    if self::en() {
        en
    } else {
        uk
    }
}

/// `format!` in the current language: `tr!("{n} вбивств", "{n} kills")`.
#[macro_export]
macro_rules! tr {
    ($uk:literal, $en:literal $(, $arg:expr)* $(,)?) => {
        if $crate::lang::en() {
            format!($en $(, $arg)*)
        } else {
            format!($uk $(, $arg)*)
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_and_formats_per_thread() {
        let n = 3;
        assert_eq!(tr!("{n} вбивства", "{n} kills"), "3 вбивства");
        with(Lang::En, || {
            assert_eq!(tr!("{n} вбивства", "{n} kills"), "3 kills");
            assert_eq!(t("Так", "Yes"), "Yes");
            // another thread keeps its own language
            std::thread::spawn(|| assert_eq!(t("Так", "Yes"), "Так"))
                .join()
                .unwrap();
        });
        assert_eq!(t("Так", "Yes"), "Так");
        assert_eq!(Lang::from_locale("uk_UA.UTF-8"), Lang::Uk);
        assert_eq!(Lang::from_locale("en-US"), Lang::En);
        assert_eq!(Lang::from_locale("de_DE"), Lang::En);
    }
}
