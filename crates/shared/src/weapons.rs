//! Weapons, perks, loadouts and default classes in the spirit of CoD4.

use crate::lang::t;
use crate::tuning::StreakTuning;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeaponId {
    #[default]
    M4,
    Ak47,
    G3,
    Mp5,
    P90,
    Ak74u,
    M249,
    Rpd,
    W1200,
    M1014,
    M40,
    R700,
    M9,
    Usp,
    Deagle,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponClass {
    Rifle,
    Smg,
    Lmg,
    Shotgun,
    Sniper,
    Pistol,
}

impl WeaponClass {
    pub fn name(self) -> &'static str {
        match self {
            WeaponClass::Rifle => t("Штурмова гвинтівка", "Assault rifle"),
            WeaponClass::Smg => t("Пістолет-кулемет", "SMG"),
            WeaponClass::Lmg => t("Кулемет", "LMG"),
            WeaponClass::Shotgun => t("Дробовик", "Shotgun"),
            WeaponClass::Sniper => t("Снайперська гвинтівка", "Sniper rifle"),
            WeaponClass::Pistol => t("Пістолет", "Pistol"),
        }
    }
}

/// Static weapon parameters. Distances in meters, angles in degrees, times in seconds.
#[derive(Clone, Copy, Debug)]
pub struct WeaponDef {
    pub id: WeaponId,
    pub name: &'static str,
    pub class: WeaponClass,
    pub damage_max: f32,
    pub damage_min: f32,
    /// Full damage up to `range_near`, linear falloff to `damage_min` at `range_far`.
    pub range_near: f32,
    pub range_far: f32,
    pub rpm: f32,
    pub auto: bool,
    pub mag: u16,
    /// Spare ammo at spawn.
    pub reserve: u16,
    pub reload_time: f32,
    pub pellets: u8,
    pub hip_spread: f32,
    pub ads_spread: f32,
    /// Extra spread while moving at full speed.
    pub move_spread: f32,
    /// Spread added per shot (recovers quickly).
    pub shot_spread: f32,
    pub ads_time: f32,
    pub recoil_v: f32,
    pub recoil_h: f32,
    pub move_mult: f32,
    /// FOV divisor while aiming down sights.
    pub ads_zoom: f32,
    pub head_mult: f32,
    pub scoped: bool,
    /// Visual/audio "body" of the shot: 0 pistol … 1 heavy.
    pub heft: f32,
}

impl WeaponDef {
    pub fn fire_interval(&self) -> f32 {
        60.0 / self.rpm
    }
    pub fn damage_at(&self, dist: f32) -> f32 {
        if dist <= self.range_near {
            self.damage_max
        } else if dist >= self.range_far {
            self.damage_min
        } else {
            let t = (dist - self.range_near) / (self.range_far - self.range_near);
            self.damage_max + (self.damage_min - self.damage_max) * t
        }
    }
    pub fn is_primary(&self) -> bool {
        self.class != WeaponClass::Pistol
    }
}

macro_rules! w {
    ($id:ident, $name:expr, $class:ident, dmg $dmax:expr => $dmin:expr, range $rn:expr => $rf:expr,
     rpm $rpm:expr, auto $auto:expr, mag $mag:expr, res $res:expr, reload $rl:expr, pellets $pel:expr,
     spread $hip:expr, $ads:expr, $mv:expr, $shot:expr, ads $adst:expr, recoil $rv:expr, $rh:expr,
     speed $mm:expr, zoom $zoom:expr, head $head:expr, scoped $sc:expr, heft $heft:expr) => {
        WeaponDef {
            id: WeaponId::$id,
            name: $name,
            class: WeaponClass::$class,
            damage_max: $dmax,
            damage_min: $dmin,
            range_near: $rn,
            range_far: $rf,
            rpm: $rpm,
            auto: $auto,
            mag: $mag,
            reserve: $res,
            reload_time: $rl,
            pellets: $pel,
            hip_spread: $hip,
            ads_spread: $ads,
            move_spread: $mv,
            shot_spread: $shot,
            ads_time: $adst,
            recoil_v: $rv,
            recoil_h: $rh,
            move_mult: $mm,
            ads_zoom: $zoom,
            head_mult: $head,
            scoped: $sc,
            heft: $heft,
        }
    };
}

pub static WEAPONS: [WeaponDef; 15] = [
    w!(M4, "M4A1", Rifle, dmg 30.0 => 20.0, range 30.0 => 60.0, rpm 750.0, auto true, mag 30, res 120, reload 2.2, pellets 1,
       spread 3.0, 0.15, 2.0, 0.35, ads 0.22, recoil 0.7, 0.35, speed 0.94, zoom 1.35, head 1.4, scoped false, heft 0.5),
    w!(Ak47, "AK-47", Rifle, dmg 40.0 => 30.0, range 25.0 => 55.0, rpm 600.0, auto true, mag 30, res 120, reload 2.4, pellets 1,
       spread 3.2, 0.2, 2.0, 0.45, ads 0.25, recoil 1.1, 0.5, speed 0.93, zoom 1.35, head 1.4, scoped false, heft 0.65),
    w!(G3, "G3", Rifle, dmg 40.0 => 30.0, range 40.0 => 70.0, rpm 500.0, auto false, mag 20, res 100, reload 2.3, pellets 1,
       spread 3.2, 0.1, 2.0, 0.6, ads 0.27, recoil 1.4, 0.4, speed 0.92, zoom 1.4, head 1.4, scoped false, heft 0.7),
    w!(Mp5, "MP5", Smg, dmg 40.0 => 20.0, range 12.0 => 30.0, rpm 800.0, auto true, mag 30, res 120, reload 2.0, pellets 1,
       spread 2.2, 0.35, 1.0, 0.3, ads 0.18, recoil 0.6, 0.35, speed 1.0, zoom 1.25, head 1.4, scoped false, heft 0.35),
    w!(P90, "P90", Smg, dmg 40.0 => 20.0, range 12.0 => 30.0, rpm 900.0, auto true, mag 50, res 150, reload 2.6, pellets 1,
       spread 2.4, 0.45, 1.0, 0.3, ads 0.18, recoil 0.55, 0.45, speed 1.0, zoom 1.25, head 1.4, scoped false, heft 0.35),
    w!(Ak74u, "AK-74u", Smg, dmg 40.0 => 30.0, range 12.0 => 30.0, rpm 700.0, auto true, mag 30, res 120, reload 2.2, pellets 1,
       spread 2.3, 0.35, 1.0, 0.35, ads 0.2, recoil 0.85, 0.45, speed 1.0, zoom 1.25, head 1.4, scoped false, heft 0.45),
    w!(M249, "M249 SAW", Lmg, dmg 30.0 => 20.0, range 40.0 => 70.0, rpm 750.0, auto true, mag 100, res 200, reload 6.0, pellets 1,
       spread 4.5, 0.3, 3.0, 0.3, ads 0.45, recoil 0.75, 0.45, speed 0.8, zoom 1.3, head 1.4, scoped false, heft 0.7),
    w!(Rpd, "RPD", Lmg, dmg 40.0 => 30.0, range 40.0 => 70.0, rpm 600.0, auto true, mag 100, res 200, reload 6.5, pellets 1,
       spread 4.5, 0.35, 3.0, 0.4, ads 0.45, recoil 1.0, 0.55, speed 0.8, zoom 1.3, head 1.4, scoped false, heft 0.8),
    w!(W1200, "W1200", Shotgun, dmg 40.0 => 10.0, range 6.0 => 18.0, rpm 75.0, auto false, mag 7, res 28, reload 3.0, pellets 8,
       spread 4.5, 3.5, 1.0, 0.0, ads 0.25, recoil 3.5, 1.0, speed 0.95, zoom 1.15, head 1.0, scoped false, heft 1.0),
    w!(M1014, "M1014", Shotgun, dmg 30.0 => 10.0, range 6.0 => 16.0, rpm 210.0, auto false, mag 4, res 24, reload 2.7, pellets 6,
       spread 4.5, 3.5, 1.0, 0.5, ads 0.25, recoil 2.6, 0.9, speed 0.95, zoom 1.15, head 1.0, scoped false, heft 0.95),
    w!(M40, "M40A3", Sniper, dmg 98.0 => 98.0, range 500.0 => 600.0, rpm 50.0, auto false, mag 5, res 20, reload 3.0, pellets 1,
       spread 7.0, 0.0, 6.0, 0.0, ads 0.4, recoil 4.0, 0.8, speed 0.9, zoom 4.0, head 1.5, scoped true, heft 0.95),
    w!(R700, "R700", Sniper, dmg 98.0 => 98.0, range 500.0 => 600.0, rpm 48.0, auto false, mag 4, res 16, reload 3.2, pellets 1,
       spread 7.0, 0.0, 6.0, 0.0, ads 0.4, recoil 4.2, 0.8, speed 0.9, zoom 4.0, head 1.5, scoped true, heft 0.95),
    w!(M9, "M9", Pistol, dmg 40.0 => 20.0, range 10.0 => 25.0, rpm 400.0, auto false, mag 15, res 45, reload 1.6, pellets 1,
       spread 2.4, 0.3, 1.0, 0.5, ads 0.14, recoil 1.0, 0.4, speed 1.05, zoom 1.2, head 1.4, scoped false, heft 0.2),
    w!(Usp, "USP .45", Pistol, dmg 40.0 => 30.0, range 10.0 => 25.0, rpm 360.0, auto false, mag 12, res 36, reload 1.6, pellets 1,
       spread 2.4, 0.3, 1.0, 0.55, ads 0.14, recoil 1.2, 0.45, speed 1.05, zoom 1.2, head 1.4, scoped false, heft 0.25),
    w!(Deagle, "Desert Eagle", Pistol, dmg 55.0 => 45.0, range 12.0 => 30.0, rpm 240.0, auto false, mag 7, res 21, reload 1.8, pellets 1,
       spread 2.8, 0.35, 1.2, 0.9, ads 0.16, recoil 2.6, 0.7, speed 1.05, zoom 1.2, head 1.4, scoped false, heft 0.55),
];

impl WeaponId {
    /// Short stable key used in configs and admin commands (e.g. "m249").
    pub fn key(self) -> &'static str {
        match self {
            WeaponId::M4 => "m4",
            WeaponId::Ak47 => "ak47",
            WeaponId::G3 => "g3",
            WeaponId::Mp5 => "mp5",
            WeaponId::P90 => "p90",
            WeaponId::Ak74u => "ak74u",
            WeaponId::M249 => "m249",
            WeaponId::Rpd => "rpd",
            WeaponId::W1200 => "w1200",
            WeaponId::M1014 => "m1014",
            WeaponId::M40 => "m40",
            WeaponId::R700 => "r700",
            WeaponId::M9 => "m9",
            WeaponId::Usp => "usp",
            WeaponId::Deagle => "deagle",
        }
    }
    pub fn from_key(k: &str) -> Option<WeaponId> {
        let k = k.to_ascii_lowercase();
        WeaponId::all().find(|w| w.key() == k)
    }
    pub fn def(self) -> &'static WeaponDef {
        &WEAPONS[self as usize]
    }
    pub fn all() -> impl Iterator<Item = WeaponId> {
        WEAPONS.iter().map(|w| w.id)
    }
    pub fn primaries() -> impl Iterator<Item = WeaponId> {
        WEAPONS.iter().filter(|w| w.is_primary()).map(|w| w.id)
    }
    pub fn secondaries() -> impl Iterator<Item = WeaponId> {
        WEAPONS.iter().filter(|w| !w.is_primary()).map(|w| w.id)
    }
}

/// Perk tier 1 — equipment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Perk1 {
    /// Three frag grenades instead of one.
    Frag3,
    /// Extra magazines.
    Bandolier,
}

/// Perk tier 2.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Perk2 {
    StoppingPower,
    Juggernaut,
    SleightOfHand,
    UavJammer,
}

/// Perk tier 3.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Perk3 {
    SteadyAim,
    ExtremeConditioning,
    Martyrdom,
    DeadSilence,
}

impl Perk1 {
    pub const ALL: [Perk1; 2] = [Perk1::Frag3, Perk1::Bandolier];
    pub fn name(self) -> &'static str {
        match self {
            Perk1::Frag3 => t("Фраг ×3", "Frag ×3"),
            Perk1::Bandolier => t("Бандольєр", "Bandolier"),
        }
    }
    pub fn desc(self) -> &'static str {
        match self {
            Perk1::Frag3 => t("Три осколкові гранати", "Three frag grenades"),
            Perk1::Bandolier => t("Більше запасних набоїв", "More spare ammo"),
        }
    }
}

impl Perk2 {
    pub const ALL: [Perk2; 4] = [
        Perk2::StoppingPower,
        Perk2::Juggernaut,
        Perk2::SleightOfHand,
        Perk2::UavJammer,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Perk2::StoppingPower => t("Зупиняюча сила", "Stopping Power"),
            Perk2::Juggernaut => t("Джаггернаут", "Juggernaut"),
            Perk2::SleightOfHand => t("Спритність рук", "Sleight of Hand"),
            Perk2::UavJammer => t("Глушилка БПЛА", "UAV Jammer"),
        }
    }
    pub fn desc(self) -> &'static str {
        match self {
            Perk2::StoppingPower => t("+40% урону від куль", "+40% bullet damage"),
            Perk2::Juggernaut => t("Отримуєш на 25% менше урону", "Take 25% less damage"),
            Perk2::SleightOfHand => t("Перезарядка вдвічі швидша", "Reload twice as fast"),
            Perk2::UavJammer => t("Невидимий для ворожого БПЛА", "Invisible to enemy UAV"),
        }
    }
}

impl Perk3 {
    pub const ALL: [Perk3; 4] = [
        Perk3::SteadyAim,
        Perk3::ExtremeConditioning,
        Perk3::Martyrdom,
        Perk3::DeadSilence,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Perk3::SteadyAim => t("Стійкий приціл", "Steady Aim"),
            Perk3::ExtremeConditioning => t("Екстремальна підготовка", "Extreme Conditioning"),
            Perk3::Martyrdom => t("Мученик", "Martyrdom"),
            Perk3::DeadSilence => t("Мертва тиша", "Dead Silence"),
        }
    }
    pub fn desc(self) -> &'static str {
        match self {
            Perk3::SteadyAim => t("Точніша стрільба від стегна", "Tighter hip fire"),
            Perk3::ExtremeConditioning => t("Спринт удвічі довший", "Sprint twice as long"),
            Perk3::Martyrdom => t("Після смерті падає граната", "Drop a grenade when you die"),
            Perk3::DeadSilence => t("Беззвучні кроки", "Silent footsteps"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loadout {
    pub primary: WeaponId,
    pub secondary: WeaponId,
    pub perk1: Perk1,
    pub perk2: Perk2,
    pub perk3: Perk3,
}

impl Loadout {
    /// Ensures weapons are in the right slots; falls back to the default class otherwise.
    pub fn sanitized(self) -> Loadout {
        if self.primary.def().is_primary() && !self.secondary.def().is_primary() {
            self
        } else {
            DEFAULT_CLASSES[0].1
        }
    }
    pub fn grenades(&self) -> u8 {
        if self.perk1 == Perk1::Frag3 {
            3
        } else {
            1
        }
    }
    pub fn reserve_for(&self, w: WeaponId, t: &crate::tuning::Tuning) -> u16 {
        let base = t.weapon(w).reserve;
        if self.perk1 == Perk1::Bandolier {
            base + base / 2
        } else {
            base
        }
    }
}

pub const DEFAULT_CLASSES: [(&str, Loadout); 5] = [
    (
        "Штурмовик",
        Loadout {
            primary: WeaponId::M4,
            secondary: WeaponId::M9,
            perk1: Perk1::Frag3,
            perk2: Perk2::StoppingPower,
            perk3: Perk3::SteadyAim,
        },
    ),
    (
        "Спецназ",
        Loadout {
            primary: WeaponId::Mp5,
            secondary: WeaponId::Usp,
            perk1: Perk1::Frag3,
            perk2: Perk2::UavJammer,
            perk3: Perk3::ExtremeConditioning,
        },
    ),
    (
        "Кулеметник",
        Loadout {
            primary: WeaponId::M249,
            secondary: WeaponId::M9,
            perk1: Perk1::Bandolier,
            perk2: Perk2::Juggernaut,
            perk3: Perk3::DeadSilence,
        },
    ),
    (
        "Підривник",
        Loadout {
            primary: WeaponId::W1200,
            secondary: WeaponId::Usp,
            perk1: Perk1::Frag3,
            perk2: Perk2::SleightOfHand,
            perk3: Perk3::Martyrdom,
        },
    ),
    (
        "Снайпер",
        Loadout {
            primary: WeaponId::M40,
            secondary: WeaponId::Deagle,
            perk1: Perk1::Bandolier,
            perk2: Perk2::StoppingPower,
            perk3: Perk3::SteadyAim,
        },
    ),
];

/// Killstreak rewards, earned at 3, 6, 9 … 21 kills in a row.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Killstreak {
    /// Radar: enemies show on the minimap.
    Uav,
    Airstrike,
    /// A barrage of shells over a wider area.
    Artillery,
    Helicopter,
    /// A kamikaze drone you fly yourself.
    Fpv,
    /// A big drone you steer from above, dropping bombs.
    Bomber,
    /// A ground robot with a machine gun (piloted or on its own).
    Nrk,
}

impl Killstreak {
    pub const ALL: [Killstreak; 7] = [
        Killstreak::Uav,
        Killstreak::Airstrike,
        Killstreak::Artillery,
        Killstreak::Helicopter,
        Killstreak::Fpv,
        Killstreak::Bomber,
        Killstreak::Nrk,
    ];
    pub fn name(self) -> &'static str {
        match self {
            Killstreak::Uav => t("Радар", "Radar"),
            Killstreak::Airstrike => t("Авіаудар", "Airstrike"),
            Killstreak::Artillery => t("Артилерія", "Artillery"),
            Killstreak::Helicopter => t("Гелікоптер", "Helicopter"),
            Killstreak::Fpv => t("FPV-дрон", "FPV drone"),
            Killstreak::Bomber => t("Дрон-бомбер", "Bomber drone"),
            Killstreak::Nrk => t("НРК", "Ground robot"),
        }
    }
    /// Kills in a row needed.
    /// Kills in a row it takes.
    pub fn kills(self, t: &StreakTuning) -> u32 {
        (match self {
            Killstreak::Uav => t.uav_kills,
            Killstreak::Airstrike => t.airstrike_kills,
            Killstreak::Artillery => t.artillery_kills,
            Killstreak::Helicopter => t.heli_kills,
            Killstreak::Fpv => t.fpv_kills,
            Killstreak::Bomber => t.bomber_kills,
            Killstreak::Nrk => t.nrk_kills,
        }) as u32
    }
    /// Kills for one full round of rewards; after the last one they start over.
    pub fn cycle(t: &StreakTuning) -> u32 {
        Killstreak::ALL
            .iter()
            .map(|k| k.kills(t))
            .max()
            .unwrap_or(1)
            .max(1)
    }
    /// The reward for reaching exactly `streak` kills (going round: with the usual numbers
    /// 24 kills is the radar again, 27 the airstrike, …).
    pub fn for_streak(streak: u32, t: &StreakTuning) -> Option<Killstreak> {
        if streak == 0 {
            return None;
        }
        let n = (streak - 1) % Self::cycle(t) + 1;
        Killstreak::ALL.into_iter().find(|k| k.kills(t) == n)
    }
    /// The next reward after `streak` kills and how many more kills it takes.
    pub fn next_after(streak: u32, t: &StreakTuning) -> (Killstreak, u32) {
        let n = streak % Self::cycle(t);
        let k = Killstreak::ALL
            .into_iter()
            .filter(|k| k.kills(t) > n)
            .min_by_key(|k| k.kills(t))
            .unwrap_or_else(|| {
                Killstreak::ALL
                    .into_iter()
                    .min_by_key(|k| k.kills(t))
                    .unwrap_or(Killstreak::Uav)
            });
        let left = if k.kills(t) > n {
            k.kills(t) - n
        } else {
            Self::cycle(t) - n + k.kills(t)
        };
        (k, left)
    }
    /// Called in on a point picked on the map.
    pub fn needs_target(self) -> bool {
        matches!(self, Killstreak::Airstrike | Killstreak::Artillery)
    }
    /// A vehicle the player drives (and the kind it is).
    pub fn vehicle(self) -> Option<crate::vehicles::VehicleKind> {
        use crate::vehicles::VehicleKind;
        match self {
            Killstreak::Fpv => Some(VehicleKind::Fpv),
            Killstreak::Bomber => Some(VehicleKind::Bomber),
            Killstreak::Nrk => Some(VehicleKind::Nrk),
            _ => None,
        }
    }
}

/// What killed a player (for the kill feed).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Cause {
    Weapon(WeaponId),
    Grenade,
    Knife,
    Airstrike,
    Helicopter,
    Bomb,
    Fall,
    World,
    Artillery,
    Fpv,
    /// A bomb dropped by a drone.
    Drop,
    Nrk,
    /// Outside the battle royale zone.
    Zone,
    /// Knocked down in the battle royale and not picked up in time.
    Bleed,
    /// A molotov's fire.
    Fire,
}

impl Cause {
    pub fn name(self) -> &'static str {
        match self {
            Cause::Weapon(w) => w.def().name,
            Cause::Grenade => t("Граната", "Grenade"),
            Cause::Knife => t("Ніж", "Knife"),
            Cause::Airstrike => t("Авіаудар", "Airstrike"),
            Cause::Helicopter => t("Гелікоптер", "Helicopter"),
            Cause::Bomb => t("Бомба", "Bomb"),
            Cause::Fall => t("Падіння", "Fall"),
            Cause::World => t("Світ", "World"),
            Cause::Artillery => t("Артилерія", "Artillery"),
            Cause::Fpv => t("FPV-дрон", "FPV drone"),
            Cause::Drop => t("Скид з дрона", "Drone drop"),
            Cause::Nrk => t("НРК", "Ground robot"),
            Cause::Zone => t("Зона", "The zone"),
            Cause::Bleed => t("Кровотеча", "Bled out"),
            Cause::Fire => t("Вогонь", "Fire"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn table_order_matches_ids() {
        for (i, w) in WEAPONS.iter().enumerate() {
            assert_eq!(w.id as usize, i, "{}", w.name);
        }
    }

    #[test]
    fn falloff() {
        let m4 = WeaponId::M4.def();
        assert_eq!(m4.damage_at(5.0), 30.0);
        assert_eq!(m4.damage_at(100.0), 20.0);
        assert!((m4.damage_at(45.0) - 25.0).abs() < 0.01);
    }

    #[test]
    fn default_classes_valid() {
        for (_, l) in DEFAULT_CLASSES {
            assert_eq!(l.sanitized(), l);
        }
    }
}
