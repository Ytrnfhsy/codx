//! Battle royale characters. Skin 0 is the soldier in team uniform; the others are
//! models from the character pack (`assets/characters`), by name.

/// (model name in the pack, Ukrainian name, English name)
pub const SKINS: [(&str, &str, &str); 11] = [
    ("", "Солдат", "Soldier"),
    ("Pirate_Male", "Пірат", "Pirate"),
    ("Ninja_Male", "Ніндзя", "Ninja"),
    ("Ninja_Sand", "Бандит", "Bandit"),
    ("Knight_Male", "Лицар", "Knight"),
    ("Knight_Golden_Male", "Золотий лицар", "Golden knight"),
    ("Goblin_Male", "Гоблін", "Goblin"),
    ("Elf", "Ельф", "Elf"),
    ("Kimono_Male", "Монах", "Monk"),
    ("Worker_Male", "Селянин", "Peasant"),
    ("Wizard", "Чарівник", "Wizard"),
];

pub fn count() -> u8 {
    SKINS.len() as u8
}

/// Name of a skin in the current language.
pub fn name(skin: u8) -> &'static str {
    let s = SKINS[(skin as usize).min(SKINS.len() - 1)];
    crate::lang::t(s.1, s.2)
}

/// The pack model a skin uses (None: the team uniform).
pub fn model(skin: u8) -> Option<&'static str> {
    SKINS
        .get(skin as usize)
        .map(|s| s.0)
        .filter(|m| !m.is_empty())
}
