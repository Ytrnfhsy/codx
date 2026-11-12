//! Battle royale inventory: everything a player carries besides the two guns in their
//! hands — ammunition by calibre, medkits, grenades, the vest, the helmet and the
//! backpack that decides how much of it all fits.

use crate::lang::t;
use crate::weapons::{WeaponClass, WeaponId};
use serde::{Deserialize, Serialize};

/// Ammunition kinds; every gun takes one of them (by its class).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AmmoType {
    Pistol,
    Smg,
    Rifle,
    Shotgun,
    Sniper,
    Lmg,
}

impl AmmoType {
    pub const ALL: [AmmoType; 6] = [
        AmmoType::Pistol,
        AmmoType::Smg,
        AmmoType::Rifle,
        AmmoType::Shotgun,
        AmmoType::Sniper,
        AmmoType::Lmg,
    ];

    pub fn of(w: WeaponId) -> AmmoType {
        match w.def().class {
            WeaponClass::Pistol => AmmoType::Pistol,
            WeaponClass::Smg => AmmoType::Smg,
            WeaponClass::Rifle => AmmoType::Rifle,
            WeaponClass::Shotgun => AmmoType::Shotgun,
            WeaponClass::Sniper => AmmoType::Sniper,
            WeaponClass::Lmg => AmmoType::Lmg,
        }
    }

    pub fn index(self) -> usize {
        self as usize
    }

    pub fn name(self) -> &'static str {
        match self {
            AmmoType::Pistol => t("Пістолетні набої", "Pistol ammo"),
            AmmoType::Smg => t("Набої для ПП", "SMG ammo"),
            AmmoType::Rifle => t("Гвинтівкові набої", "Rifle ammo"),
            AmmoType::Shotgun => t("Дріб", "Shotgun shells"),
            AmmoType::Sniper => t("Снайперські набої", "Sniper ammo"),
            AmmoType::Lmg => t("Кулеметна стрічка", "Machine gun belt"),
        }
    }

    /// A few letters for the HUD.
    pub fn short(self) -> &'static str {
        match self {
            AmmoType::Pistol => t("ПІСТ", "PST"),
            AmmoType::Smg => t("ПП", "SMG"),
            AmmoType::Rifle => t("ГВИНТ", "RFL"),
            AmmoType::Shotgun => t("ДРІБ", "SHT"),
            AmmoType::Sniper => t("СНАЙП", "SNP"),
            AmmoType::Lmg => t("КУЛ", "LMG"),
        }
    }

    /// The guns that take it (for the pick-up prompt).
    pub fn guns(self) -> Vec<&'static str> {
        WeaponId::all()
            .filter(|w| AmmoType::of(*w) == self)
            .map(|w| w.def().name)
            .collect()
    }

    /// What fits with no backpack.
    pub fn base_cap(self) -> u16 {
        match self {
            AmmoType::Pistol => 45,
            AmmoType::Smg => 120,
            AmmoType::Rifle => 120,
            AmmoType::Shotgun => 24,
            AmmoType::Sniper => 25,
            AmmoType::Lmg => 200,
        }
    }

    /// How much one box on the ground holds.
    pub fn box_size(self) -> u16 {
        match self {
            AmmoType::Pistol => 15,
            AmmoType::Smg => 30,
            AmmoType::Rifle => 30,
            AmmoType::Shotgun => 8,
            AmmoType::Sniper => 5,
            AmmoType::Lmg => 50,
        }
    }
}

/// Highest level of vests, helmets and backpacks.
pub const MAX_LEVEL: u8 = 3;

/// Points of damage a vest of each level can take before it is worn out.
pub fn armor_durability(level: u8) -> f32 {
    [0.0, 60.0, 100.0, 150.0][level.min(MAX_LEVEL) as usize]
}
/// Share of a body hit a vest takes on.
pub fn armor_absorb(level: u8) -> f32 {
    [0.0, 0.3, 0.4, 0.55][level.min(MAX_LEVEL) as usize]
}
pub fn helmet_durability(level: u8) -> f32 {
    [0.0, 40.0, 70.0, 100.0][level.min(MAX_LEVEL) as usize]
}
/// Share of a head hit a helmet takes on.
pub fn helmet_absorb(level: u8) -> f32 {
    [0.0, 0.3, 0.45, 0.6][level.min(MAX_LEVEL) as usize]
}

/// Roman numerals for the levels.
pub fn level_name(level: u8) -> &'static str {
    ["", "I", "II", "III"][level.min(MAX_LEVEL) as usize]
}

/// Sights found as loot: 0 iron sights, 1 red dot, 2 a 2× scope, 3 a 4× scope, 4 an 8× scope.
pub const MAX_SIGHT: u8 = 4;

/// How much a sight magnifies at most.
pub fn sight_zoom(level: u8) -> f32 {
    sight_zoom_range(level).1
}

/// The magnification a sight can be set to: the two strongest scopes are variable (the
/// mouse wheel turns them while aiming), the others fixed.
pub fn sight_zoom_range(level: u8) -> (f32, f32) {
    match level.min(MAX_SIGHT) {
        0 => (1.0, 1.0),
        1 => (1.5, 1.5),
        2 => (2.0, 2.0),
        3 => (2.0, 4.0),
        _ => (3.0, 8.0),
    }
}

/// The magnification can be turned.
pub fn sight_variable(level: u8) -> bool {
    let (a, b) = sight_zoom_range(level);
    b > a
}

/// A scope you look through (a black ring round it), not just a dot over the iron sights.
pub fn sight_magnifies(level: u8) -> bool {
    level >= 2
}

pub fn sight_name(level: u8) -> &'static str {
    match level.min(MAX_SIGHT) {
        0 => t("Мушка", "Iron sights"),
        1 => t("Коліматор", "Red dot"),
        2 => t("Приціл 2×", "2× scope"),
        3 => t("Приціл 2–4×", "2–4× scope"),
        _ => t("Приціл 3–8×", "3–8× scope"),
    }
}

/// Short label for the HUD.
pub fn sight_short(level: u8) -> &'static str {
    match level.min(MAX_SIGHT) {
        0 => "—",
        1 => t("КОЛ", "DOT"),
        2 => "2×",
        3 => "2-4×",
        _ => "3-8×",
    }
}

/// Which sights go on which gun (0, iron sights, always does):
/// pistols take only a red dot; shotguns up to a 2× scope; SMGs and machine guns up to
/// the 2–4×; assault rifles anything; sniper rifles only scopes (no red dot).
pub fn sight_fits(level: u8, w: WeaponId) -> bool {
    if level == 0 {
        return true;
    }
    match w.def().class {
        WeaponClass::Pistol => level <= 1,
        WeaponClass::Shotgun => level <= 2,
        WeaponClass::Smg | WeaponClass::Lmg => level <= 3,
        WeaponClass::Rifle => true,
        WeaponClass::Sniper => level >= 2,
    }
}

/// What sights a gun takes, for the inventory screen ("red dot", "up to 2–4×"…).
pub fn sight_rule(w: WeaponId) -> &'static str {
    match w.def().class {
        WeaponClass::Pistol => t("лише коліматор", "red dot only"),
        WeaponClass::Shotgun => t("коліматор або 2×", "red dot or 2×"),
        WeaponClass::Smg | WeaponClass::Lmg => t("до 2–4×", "up to 2–4×"),
        WeaponClass::Rifle => t("будь-який", "any"),
        WeaponClass::Sniper => t("лише оптика", "scopes only"),
    }
}

/// Gun attachments found as loot (besides sights): kinds 1..4.
pub const MOD_SUPPRESSOR: u8 = 1;
pub const MOD_COMPENSATOR: u8 = 2;
pub const MOD_EXTMAG: u8 = 3;
pub const MOD_GRIP: u8 = 4;
pub const MODS: [u8; 4] = [MOD_SUPPRESSOR, MOD_COMPENSATOR, MOD_EXTMAG, MOD_GRIP];

pub fn mod_name(kind: u8) -> &'static str {
    match kind {
        MOD_SUPPRESSOR => t("Глушник", "Suppressor"),
        MOD_COMPENSATOR => t("Компенсатор", "Compensator"),
        MOD_EXTMAG => t("Збільшений магазин", "Extended magazine"),
        _ => t("Руків'я", "Vertical grip"),
    }
}

/// Where on a gun it goes: 0 the muzzle (suppressor or compensator), 1 the magazine,
/// 2 under the barrel.
pub fn mod_place(kind: u8) -> usize {
    match kind {
        MOD_SUPPRESSOR | MOD_COMPENSATOR => 0,
        MOD_EXTMAG => 1,
        _ => 2,
    }
}

/// Which attachments go on which gun.
pub fn mod_fits(kind: u8, w: WeaponId) -> bool {
    let c = w.def().class;
    match kind {
        MOD_SUPPRESSOR => c != WeaponClass::Shotgun,
        MOD_COMPENSATOR => !matches!(c, WeaponClass::Shotgun | WeaponClass::Pistol),
        MOD_EXTMAG => c != WeaponClass::Shotgun,
        MOD_GRIP => matches!(c, WeaponClass::Rifle | WeaponClass::Smg | WeaponClass::Lmg),
        _ => false,
    }
}

/// A magazine's size with or without the extended one.
pub fn mag_size(base: u16, extended: bool) -> u16 {
    if extended {
        base + base / 2
    } else {
        base
    }
}

/// Seconds a medkit takes to use, and what it heals.
pub const MEDKIT_TIME: f32 = 3.0;
pub const MEDKIT_HEAL: f32 = 60.0;
/// Bandages: quick, a little, and only up to `BANDAGE_MAX`.
pub const BANDAGE_TIME: f32 = 4.0;
pub const BANDAGE_HEAL: f32 = 15.0;
pub const BANDAGE_MAX: f32 = 75.0;
/// An energy drink: a few seconds to drink, then the boost heals one point a second.
pub const DRINK_TIME: f32 = 3.0;
pub const DRINK_BOOST: f32 = 40.0;

/// What is being used (`Inventory::using`).
pub const USE_MEDKIT: u8 = 1;
pub const USE_BANDAGE: u8 = 2;
pub const USE_DRINK: u8 = 3;

/// Thrown things besides frag and smoke grenades (`kind` of a thrown grenade).
pub const THROW_MOLOTOV: u8 = 1;
pub const THROW_FLASH: u8 = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Inventory {
    pub medkits: u8,
    /// Vest level (0 = none) and what is left of it.
    pub armor: u8,
    pub armor_hp: f32,
    pub helmet: u8,
    pub helmet_hp: f32,
    pub backpack: u8,
    /// Rounds of each `AmmoType`.
    pub ammo: [u16; 6],
    /// Seconds left of using a medkit, bandage or drink (0 = not healing), and which.
    pub healing: f32,
    pub using: u8,
    pub bandages: u8,
    pub drinks: u8,
    /// Boost from energy drinks still to heal.
    pub boost: f32,
    pub molotovs: u8,
    pub flashes: u8,
    /// Sight on the gun in each hand slot (see `sight_zoom`).
    pub sights: [u8; 2],
    /// Spare sights in the backpack, by level (index 0 = red dot … 3 = 8×).
    pub spare_sights: [u8; 4],
    /// Attachments on the gun in each hand slot, by place (`mod_place`): the kind there
    /// (0 = none).
    pub mods: [[u8; 3]; 2],
    /// Spare attachments in the backpack, by kind (index kind - 1).
    pub spare_mods: [u8; 4],
}

impl Inventory {
    fn bag(&self) -> f32 {
        [1.0, 1.5, 2.0, 2.5][self.backpack.min(MAX_LEVEL) as usize]
    }
    pub fn ammo_cap(&self, t: AmmoType) -> u16 {
        (t.base_cap() as f32 * self.bag()) as u16
    }
    pub fn bandage_cap(&self) -> u8 {
        5 + self.backpack * 3
    }
    pub fn drink_cap(&self) -> u8 {
        2 + self.backpack
    }
    /// Molotovs and flashbangs, each.
    pub fn throwable_cap(&self) -> u8 {
        1 + self.backpack
    }
    pub fn medkit_cap(&self) -> u8 {
        2 + self.backpack * 2
    }
    /// How many spare sights the backpack holds.
    pub fn sight_cap(&self) -> u8 {
        2 + self.backpack
    }
    /// Spare sights and attachments (they share the room).
    pub fn spare_count(&self) -> u8 {
        self.spare_sights.iter().sum::<u8>() + self.spare_mods.iter().sum::<u8>()
    }
    /// Puts an attachment in the backpack; false when there is no room.
    pub fn stash_mod(&mut self, kind: u8) -> bool {
        if !MODS.contains(&kind) || self.spare_count() >= self.sight_cap() {
            return false;
        }
        self.spare_mods[kind as usize - 1] += 1;
        true
    }
    /// The gun in `slot` has this kind of attachment on.
    pub fn has_mod(&self, slot: usize, kind: u8) -> bool {
        self.mods[slot & 1][mod_place(kind)] == kind
    }
    /// Puts a sight in the backpack; false when there is no room.
    pub fn stash_sight(&mut self, level: u8) -> bool {
        if level == 0 || level > MAX_SIGHT || self.spare_count() >= self.sight_cap() {
            return false;
        }
        self.spare_sights[level as usize - 1] += 1;
        true
    }
    /// Takes the best spare sight that fits `w` out of the backpack (0 if none does).
    pub fn take_best_sight(&mut self, w: WeaponId) -> u8 {
        for level in (1..=MAX_SIGHT).rev() {
            let k = level as usize - 1;
            if self.spare_sights[k] > 0 && sight_fits(level, w) {
                self.spare_sights[k] -= 1;
                return level;
            }
        }
        0
    }
    pub fn grenade_cap(&self) -> u8 {
        2 + self.backpack
    }
    pub fn ammo(&self, t: AmmoType) -> u16 {
        self.ammo[t.index()]
    }
    /// Adds up to `n` rounds; returns how many fitted.
    pub fn add_ammo(&mut self, t: AmmoType, n: u16) -> u16 {
        let room = self.ammo_cap(t).saturating_sub(self.ammo[t.index()]);
        let take = room.min(n);
        self.ammo[t.index()] += take;
        take
    }
    /// Damage after the vest or the helmet took its share (and wore down).
    pub fn soak(&mut self, amount: f32, head: bool) -> f32 {
        let absorb = if head {
            helmet_absorb(self.helmet)
        } else {
            armor_absorb(self.armor)
        };
        let (level, hp) = if head {
            (&mut self.helmet, &mut self.helmet_hp)
        } else {
            (&mut self.armor, &mut self.armor_hp)
        };
        if *level == 0 || *hp <= 0.0 {
            return amount;
        }
        let taken = (amount * absorb).min(*hp);
        *hp -= taken;
        if *hp <= 0.01 {
            *level = 0;
            *hp = 0.0;
        }
        amount - taken
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vests_and_helmets_soak_and_wear_out() {
        let mut inv = Inventory {
            armor: 2,
            armor_hp: armor_durability(2),
            helmet: 1,
            helmet_hp: helmet_durability(1),
            ..Default::default()
        };
        assert!((inv.soak(50.0, false) - 30.0).abs() < 1e-3);
        assert!((inv.soak(100.0, true) - 70.0).abs() < 1e-3);
        // a worn vest gives up
        for _ in 0..10 {
            inv.soak(100.0, false);
        }
        assert_eq!(inv.armor, 0);
        assert_eq!(inv.soak(40.0, false), 40.0);
    }

    #[test]
    fn the_backpack_decides_what_fits() {
        let mut inv = Inventory::default();
        assert_eq!(inv.add_ammo(AmmoType::Sniper, 100), 25);
        inv.backpack = 3;
        assert_eq!(inv.add_ammo(AmmoType::Sniper, 100), 37);
        assert_eq!(inv.medkit_cap(), 8);
        assert_eq!(AmmoType::of(WeaponId::M40), AmmoType::Sniper);
        assert!(AmmoType::Rifle.guns().contains(&"M4A1"));
    }

    #[test]
    fn attachments_fit_and_share_the_backpack() {
        assert!(mod_fits(MOD_SUPPRESSOR, WeaponId::M9) && !mod_fits(MOD_GRIP, WeaponId::M9));
        assert!(!mod_fits(MOD_SUPPRESSOR, WeaponId::W1200));
        assert!(mod_fits(MOD_GRIP, WeaponId::M4) && !mod_fits(MOD_GRIP, WeaponId::M40));
        assert_eq!(mag_size(30, true), 45);
        let mut inv = Inventory::default();
        assert!(inv.stash_sight(2) && inv.stash_mod(MOD_GRIP) && !inv.stash_mod(MOD_EXTMAG));
        assert_eq!(mod_place(MOD_COMPENSATOR), mod_place(MOD_SUPPRESSOR));
    }

    #[test]
    fn sights_fit_their_guns() {
        assert!(sight_fits(4, WeaponId::M40) && sight_fits(3, WeaponId::M4));
        assert!(sight_fits(1, WeaponId::M9) && !sight_fits(2, WeaponId::M9));
        assert!(sight_fits(2, WeaponId::W1200) && !sight_fits(3, WeaponId::W1200));
        assert!(sight_fits(3, WeaponId::Mp5) && !sight_fits(4, WeaponId::Mp5));
        assert!(sight_fits(3, WeaponId::M249) && !sight_fits(4, WeaponId::Rpd));
        assert!(sight_fits(4, WeaponId::Ak47) && sight_fits(1, WeaponId::M4));
        assert!(!sight_fits(1, WeaponId::M40) && sight_fits(2, WeaponId::R700));
        assert!(!sight_fits(2, WeaponId::Deagle) && !sight_fits(4, WeaponId::Usp));
        assert!((0..=MAX_SIGHT).all(|l| sight_fits(0, WeaponId::all().nth(l as usize).unwrap())));
        assert!(sight_zoom(4) > sight_zoom(3) && !sight_magnifies(1));
        assert!(sight_variable(4) && sight_variable(3) && !sight_variable(2));
        assert_eq!(sight_zoom_range(4), (3.0, 8.0));
        // spares: as many as the backpack holds, the best fitting one comes out first
        let mut inv = Inventory::default();
        assert!(inv.stash_sight(4) && inv.stash_sight(1) && !inv.stash_sight(2));
        assert_eq!(inv.take_best_sight(WeaponId::M9), 1);
        assert_eq!(inv.take_best_sight(WeaponId::M9), 0);
        assert_eq!(inv.take_best_sight(WeaponId::M4), 4);
    }
}
