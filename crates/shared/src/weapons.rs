//! Weapons, loadouts and the loadouts bots and tests start from.

use crate::lang::t;
use crate::tuning::StreakTuning;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WeaponId {
    // assault rifles
    #[default]
    Uar15,
    Fort221,
    Ak12,
    ScarH,
    // submachine guns
    Vector,
    Mp7,
    Bizon,
    // marksman rifles
    Svd,
    Mk14,
    // machine guns
    Negev,
    Pkm,
    // shotguns
    Aa12,
    Ksg,
    // sniper rifles
    Awm,
    Barrett,
    Alligator,
    // pistols
    Fort17,
    Glock18,
    Revolver,
    // launchers (the second slot, like a pistol)
    Rpg7,
    Crossbow,
    Bow,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WeaponClass {
    Rifle,
    Smg,
    Marksman,
    Lmg,
    Shotgun,
    Sniper,
    Pistol,
    Launcher,
}

impl WeaponClass {
    pub fn name(self) -> &'static str {
        match self {
            WeaponClass::Rifle => t("Штурмова гвинтівка", "Assault rifle"),
            WeaponClass::Smg => t("Пістолет-кулемет", "SMG"),
            WeaponClass::Marksman => t("Марксманська гвинтівка", "Marksman rifle"),
            WeaponClass::Lmg => t("Кулемет", "LMG"),
            WeaponClass::Shotgun => t("Дробовик", "Shotgun"),
            WeaponClass::Sniper => t("Снайперська гвинтівка", "Sniper rifle"),
            WeaponClass::Pistol => t("Пістолет", "Pistol"),
            WeaponClass::Launcher => t("Пускова установка", "Launcher"),
        }
    }
    /// It goes in the second slot (with the pistols).
    pub fn is_sidearm(self) -> bool {
        matches!(self, WeaponClass::Pistol | WeaponClass::Launcher)
    }
}

/// What a gun shoots besides plain bullets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Shot {
    Bullet,
    /// A rocket: blows up where it hits (radius m, damage).
    Rocket,
    /// A crossbow bolt: silent, slow, falls in an arc.
    Bolt,
}

/// Static weapon parameters. Distances in meters, angles in degrees, times in seconds.
#[derive(Clone, Copy, Debug)]
pub struct WeaponDef {
    pub id: WeaponId,
    /// Its name in Ukrainian and in English.
    pub names: [&'static str; 2],
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
    pub fn name(&self) -> &'static str {
        t(self.names[0], self.names[1])
    }
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
        !self.class.is_sidearm()
    }
}

macro_rules! w {
    ($id:ident, $uk:expr, $en:expr, $class:ident, dmg $dmax:expr => $dmin:expr, range $rn:expr => $rf:expr,
     rpm $rpm:expr, auto $auto:expr, mag $mag:expr, res $res:expr, reload $rl:expr, pellets $pel:expr,
     spread $hip:expr, $ads:expr, $mv:expr, $shot:expr, ads $adst:expr, recoil $rv:expr, $rh:expr,
     speed $mm:expr, zoom $zoom:expr, head $head:expr, scoped $sc:expr, heft $heft:expr) => {
        WeaponDef {
            id: WeaponId::$id,
            names: [$uk, $en],
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

pub static WEAPONS: [WeaponDef; 22] = [
    // ---- assault rifles: an all-rounder, a quick bullpup, a hard hitter, a heavy one
    w!(Uar15, "UAR-15", "UAR-15", Rifle, dmg 30.0 => 22.0, range 30.0 => 60.0, rpm 780.0, auto true, mag 30, res 120, reload 2.2, pellets 1,
       spread 3.0, 0.15, 2.0, 0.35, ads 0.22, recoil 0.7, 0.35, speed 0.94, zoom 1.35, head 1.4, scoped false, heft 0.5),
    w!(Fort221, "Форт-221", "Fort-221", Rifle, dmg 30.0 => 20.0, range 22.0 => 48.0, rpm 860.0, auto true, mag 30, res 120, reload 2.6, pellets 1,
       spread 2.6, 0.18, 1.6, 0.32, ads 0.17, recoil 0.65, 0.4, speed 0.97, zoom 1.3, head 1.4, scoped false, heft 0.45),
    w!(Ak12, "АК-12", "AK-12", Rifle, dmg 38.0 => 28.0, range 25.0 => 55.0, rpm 650.0, auto true, mag 30, res 120, reload 2.4, pellets 1,
       spread 3.2, 0.2, 2.0, 0.45, ads 0.25, recoil 1.0, 0.5, speed 0.93, zoom 1.35, head 1.4, scoped false, heft 0.65),
    w!(ScarH, "SCAR-H", "SCAR-H", Rifle, dmg 45.0 => 34.0, range 35.0 => 70.0, rpm 560.0, auto true, mag 20, res 100, reload 2.5, pellets 1,
       spread 3.3, 0.12, 2.2, 0.55, ads 0.28, recoil 1.35, 0.45, speed 0.91, zoom 1.4, head 1.4, scoped false, heft 0.75),
    // ---- submachine guns: a buzzsaw, a precise one, a big magazine
    w!(Vector, "Vector", "Vector", Smg, dmg 26.0 => 14.0, range 8.0 => 22.0, rpm 1100.0, auto true, mag 25, res 125, reload 2.0, pellets 1,
       spread 2.4, 0.4, 1.0, 0.25, ads 0.17, recoil 0.45, 0.35, speed 1.02, zoom 1.25, head 1.4, scoped false, heft 0.3),
    w!(Mp7, "MP7", "MP7", Smg, dmg 30.0 => 20.0, range 14.0 => 32.0, rpm 900.0, auto true, mag 40, res 160, reload 2.1, pellets 1,
       spread 2.1, 0.3, 1.0, 0.25, ads 0.17, recoil 0.5, 0.3, speed 1.02, zoom 1.25, head 1.4, scoped false, heft 0.3),
    w!(Bizon, "Бізон", "Bizon", Smg, dmg 32.0 => 22.0, range 12.0 => 28.0, rpm 680.0, auto true, mag 64, res 128, reload 2.9, pellets 1,
       spread 2.5, 0.4, 1.0, 0.3, ads 0.2, recoil 0.55, 0.4, speed 0.99, zoom 1.25, head 1.4, scoped false, heft 0.4),
    // ---- marksman rifles: two hits to the body
    w!(Svd, "СВД", "SVD", Marksman, dmg 70.0 => 60.0, range 60.0 => 150.0, rpm 200.0, auto false, mag 10, res 40, reload 2.8, pellets 1,
       spread 6.0, 0.05, 5.0, 0.8, ads 0.32, recoil 2.4, 0.6, speed 0.9, zoom 3.0, head 1.5, scoped true, heft 0.85),
    w!(Mk14, "Mk 14 EBR", "Mk 14 EBR", Marksman, dmg 52.0 => 42.0, range 50.0 => 110.0, rpm 320.0, auto false, mag 20, res 80, reload 2.6, pellets 1,
       spread 5.0, 0.08, 4.0, 0.6, ads 0.3, recoil 1.8, 0.5, speed 0.91, zoom 2.0, head 1.5, scoped false, heft 0.8),
    // ---- machine guns: one spins up to a storm, one goes through thin walls
    w!(Negev, "Negev", "Negev", Lmg, dmg 28.0 => 20.0, range 35.0 => 65.0, rpm 950.0, auto true, mag 150, res 150, reload 6.5, pellets 1,
       spread 4.8, 0.35, 3.0, 0.3, ads 0.45, recoil 0.7, 0.5, speed 0.8, zoom 1.3, head 1.4, scoped false, heft 0.7),
    w!(Pkm, "ПКМ", "PKM", Lmg, dmg 42.0 => 32.0, range 45.0 => 80.0, rpm 650.0, auto true, mag 100, res 200, reload 7.0, pellets 1,
       spread 4.6, 0.3, 3.0, 0.45, ads 0.5, recoil 1.05, 0.55, speed 0.78, zoom 1.3, head 1.4, scoped false, heft 0.85),
    // ---- shotguns: a full-auto one and a pump with two tubes
    w!(Aa12, "AA-12", "AA-12", Shotgun, dmg 22.0 => 8.0, range 5.0 => 14.0, rpm 300.0, auto true, mag 8, res 32, reload 3.0, pellets 7,
       spread 5.0, 4.0, 1.0, 0.6, ads 0.25, recoil 1.8, 0.8, speed 0.94, zoom 1.15, head 1.0, scoped false, heft 0.9),
    w!(Ksg, "KSG", "KSG", Shotgun, dmg 40.0 => 10.0, range 7.0 => 20.0, rpm 75.0, auto false, mag 14, res 28, reload 3.4, pellets 8,
       spread 4.2, 3.2, 1.0, 0.0, ads 0.25, recoil 3.2, 1.0, speed 0.95, zoom 1.15, head 1.0, scoped false, heft 1.0),
    // ---- sniper rifles: the classic, the anti-materiel one, the monster
    w!(Awm, "AWM", "AWM", Sniper, dmg 110.0 => 110.0, range 500.0 => 600.0, rpm 45.0, auto false, mag 5, res 20, reload 3.4, pellets 1,
       spread 7.0, 0.0, 6.0, 0.0, ads 0.4, recoil 4.2, 0.8, speed 0.9, zoom 4.0, head 1.5, scoped true, heft 0.95),
    w!(Barrett, "Barrett M82", "Barrett M82", Sniper, dmg 140.0 => 140.0, range 600.0 => 800.0, rpm 120.0, auto false, mag 10, res 30, reload 3.8, pellets 1,
       spread 8.0, 0.02, 7.0, 1.5, ads 0.55, recoil 5.5, 1.2, speed 0.82, zoom 4.5, head 1.5, scoped true, heft 1.2),
    w!(Alligator, "«Алігатор»", "Alligator", Sniper, dmg 250.0 => 250.0, range 800.0 => 1000.0, rpm 30.0, auto false, mag 5, res 15, reload 4.5, pellets 1,
       spread 9.0, 0.0, 8.0, 0.0, ads 0.7, recoil 7.0, 1.5, speed 0.75, zoom 5.0, head 1.5, scoped true, heft 1.4),
    // ---- pistols: the plain one, the automatic one, the revolver
    w!(Fort17, "Форт-17", "Fort-17", Pistol, dmg 34.0 => 22.0, range 10.0 => 25.0, rpm 400.0, auto false, mag 12, res 48, reload 1.6, pellets 1,
       spread 2.4, 0.3, 1.0, 0.5, ads 0.14, recoil 1.0, 0.4, speed 1.05, zoom 1.2, head 1.4, scoped false, heft 0.25),
    w!(Glock18, "Glock 18", "Glock 18", Pistol, dmg 22.0 => 14.0, range 8.0 => 20.0, rpm 1100.0, auto true, mag 19, res 76, reload 1.7, pellets 1,
       spread 2.8, 0.5, 1.2, 0.4, ads 0.14, recoil 0.7, 0.5, speed 1.05, zoom 1.2, head 1.4, scoped false, heft 0.2),
    w!(Revolver, "Револьвер", "Revolver", Pistol, dmg 60.0 => 45.0, range 12.0 => 30.0, rpm 160.0, auto false, mag 6, res 24, reload 2.6, pellets 1,
       spread 2.8, 0.3, 1.2, 0.9, ads 0.16, recoil 2.8, 0.7, speed 1.04, zoom 1.2, head 2.0, scoped false, heft 0.6),
    // ---- launchers: the damage is the direct hit's (a rocket's blast comes on top)
    w!(Rpg7, "РПГ-7", "RPG-7", Launcher, dmg 60.0 => 60.0, range 400.0 => 500.0, rpm 30.0, auto false, mag 1, res 2, reload 3.2, pellets 1,
       spread 1.5, 0.2, 3.0, 0.0, ads 0.35, recoil 4.0, 1.0, speed 0.9, zoom 1.4, head 1.0, scoped false, heft 1.3),
    w!(Crossbow, "Арбалет", "Crossbow", Launcher, dmg 95.0 => 95.0, range 200.0 => 300.0, rpm 40.0, auto false, mag 1, res 12, reload 2.4, pellets 1,
       spread 3.0, 0.05, 3.0, 0.0, ads 0.3, recoil 1.5, 0.3, speed 1.0, zoom 2.0, head 1.5, scoped false, heft 0.3),
    // the bow: hold to draw, let go to shoot (the damage is a full draw's)
    w!(Bow, "Лук", "Bow", Launcher, dmg 90.0 => 90.0, range 150.0 => 250.0, rpm 70.0, auto false, mag 1, res 18, reload 0.6, pellets 1,
       spread 2.0, 0.05, 2.0, 0.0, ads 0.25, recoil 0.8, 0.2, speed 1.04, zoom 1.6, head 1.6, scoped false, heft 0.2),
];

impl WeaponId {
    /// Short stable key used in configs and admin commands (e.g. "pkm").
    pub fn key(self) -> &'static str {
        match self {
            WeaponId::Uar15 => "uar15",
            WeaponId::Fort221 => "fort221",
            WeaponId::Ak12 => "ak12",
            WeaponId::ScarH => "scarh",
            WeaponId::Vector => "vector",
            WeaponId::Mp7 => "mp7",
            WeaponId::Bizon => "bizon",
            WeaponId::Svd => "svd",
            WeaponId::Mk14 => "mk14",
            WeaponId::Negev => "negev",
            WeaponId::Pkm => "pkm",
            WeaponId::Aa12 => "aa12",
            WeaponId::Ksg => "ksg",
            WeaponId::Awm => "awm",
            WeaponId::Barrett => "barrett",
            WeaponId::Alligator => "alligator",
            WeaponId::Fort17 => "fort17",
            WeaponId::Glock18 => "glock18",
            WeaponId::Revolver => "revolver",
            WeaponId::Rpg7 => "rpg7",
            WeaponId::Crossbow => "crossbow",
            WeaponId::Bow => "bow",
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
    /// What it shoots.
    pub fn shot(self) -> Shot {
        match self {
            WeaponId::Rpg7 => Shot::Rocket,
            WeaponId::Crossbow | WeaponId::Bow => Shot::Bolt,
            _ => Shot::Bullet,
        }
    }
    /// How thick a brick or concrete wall its round still goes through (m); 0 for most.
    pub fn pierce(self) -> f32 {
        match self {
            WeaponId::ScarH => 0.12,
            WeaponId::Pkm => 0.35,
            WeaponId::Barrett => 0.5,
            WeaponId::Alligator => 0.8,
            _ => 0.0,
        }
    }
    /// It knocks a chunk out of the wall it hits (the power of that small blast; 0: no).
    pub fn chips(self) -> f32 {
        match self {
            WeaponId::Barrett => 0.4,
            WeaponId::Alligator => 0.55,
            _ => 0.0,
        }
    }
    /// Seconds of holding the trigger to reach its full rate of fire (0: at once).
    pub fn spin_up(self) -> f32 {
        match self {
            WeaponId::Negev => 0.8,
            _ => 0.0,
        }
    }
    /// Its share of the full rate of fire after the trigger has been held `held` seconds.
    pub fn spin(self, held: f32) -> f32 {
        let s = self.spin_up();
        if s <= 0.0 {
            1.0
        } else {
            0.5 + 0.5 * (held / s).clamp(0.0, 1.0)
        }
    }
    /// Makes no sound anybody far off hears (and shows nobody on a radar).
    pub fn silent(self) -> bool {
        matches!(self, WeaponId::Crossbow | WeaponId::Bow)
    }
    /// Seconds to draw it fully: it shoots when the trigger is let go, harder and further
    /// the longer it was held (0: an ordinary trigger).
    pub fn draw_time(self) -> f32 {
        if self == WeaponId::Bow {
            0.75
        } else {
            0.0
        }
    }
    /// Share of the speed and of the damage of a shot drawn `draw` (0..1) of the way.
    pub fn draw_power(draw: f32) -> (f32, f32) {
        let d = draw.clamp(0.0, 1.0);
        (0.35 + 0.65 * d, 0.3 + 0.7 * d)
    }
    /// A rocket's blast: radius (m) and damage at its middle.
    pub fn blast(self) -> Option<(f32, f32)> {
        (self == WeaponId::Rpg7).then_some((5.5, 125.0))
    }
}

/// The grenade thrown with `G` in the team modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Lethal {
    /// Two frags that can be cooked.
    #[default]
    Frag,
    /// One bottle that bursts into a patch of fire where it lands.
    Molotov,
}

/// The grenade thrown with `Q` in the team modes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tactical {
    /// A cloud nobody sees through.
    #[default]
    Smoke,
    /// Two flashbangs that blind whoever looks their way.
    Flash,
}

impl Lethal {
    pub const ALL: [Lethal; 2] = [Lethal::Frag, Lethal::Molotov];
    pub fn name(self) -> &'static str {
        match self {
            Lethal::Frag => crate::lang::t("Осколкова", "Frag"),
            Lethal::Molotov => crate::lang::t("Молотов", "Molotov"),
        }
    }
    pub fn count(self) -> u8 {
        match self {
            Lethal::Frag => 2,
            Lethal::Molotov => 1,
        }
    }
    pub fn about(self) -> &'static str {
        match self {
            Lethal::Frag => crate::lang::t("вибух, можна притримати", "a blast, can be cooked"),
            Lethal::Molotov => crate::lang::t("вогонь на місці падіння", "fire where it lands"),
        }
    }
}

impl Tactical {
    pub const ALL: [Tactical; 2] = [Tactical::Smoke, Tactical::Flash];
    pub fn name(self) -> &'static str {
        match self {
            Tactical::Smoke => crate::lang::t("Димова", "Smoke"),
            Tactical::Flash => crate::lang::t("Світлошумова", "Flashbang"),
        }
    }
    pub fn count(self) -> u8 {
        match self {
            Tactical::Smoke => 1,
            Tactical::Flash => 2,
        }
    }
    pub fn about(self) -> &'static str {
        match self {
            Tactical::Smoke => crate::lang::t("хмара на 18 с", "a cloud for 18 s"),
            Tactical::Flash => {
                crate::lang::t("засліплює тих, хто дивиться", "blinds who looks at it")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Loadout {
    pub primary: WeaponId,
    pub secondary: WeaponId,
    #[serde(default)]
    pub lethal: Lethal,
    #[serde(default)]
    pub tactical: Tactical,
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
    /// Lethal grenades to start a life with.
    pub fn grenades(&self) -> u8 {
        self.lethal.count()
    }
    /// Tactical grenades to start a life with.
    pub fn tacticals(&self) -> u8 {
        self.tactical.count()
    }
    pub fn reserve_for(&self, w: WeaponId, t: &crate::tuning::Tuning) -> u16 {
        t.weapon(w).reserve
    }
}

pub const DEFAULT_CLASSES: [(&str, Loadout); 5] = [
    (
        "Штурмовик",
        Loadout {
            primary: WeaponId::Uar15,
            secondary: WeaponId::Fort17,
            lethal: Lethal::Frag,
            tactical: Tactical::Smoke,
        },
    ),
    (
        "Спецназ",
        Loadout {
            primary: WeaponId::Mp7,
            secondary: WeaponId::Glock18,
            lethal: Lethal::Frag,
            tactical: Tactical::Smoke,
        },
    ),
    (
        "Кулеметник",
        Loadout {
            primary: WeaponId::Pkm,
            secondary: WeaponId::Fort17,
            lethal: Lethal::Frag,
            tactical: Tactical::Smoke,
        },
    ),
    (
        "Підривник",
        Loadout {
            primary: WeaponId::Ksg,
            secondary: WeaponId::Glock18,
            lethal: Lethal::Frag,
            tactical: Tactical::Smoke,
        },
    ),
    (
        "Снайпер",
        Loadout {
            primary: WeaponId::Awm,
            secondary: WeaponId::Revolver,
            lethal: Lethal::Frag,
            tactical: Tactical::Smoke,
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
            Cause::Weapon(w) => w.def().name(),
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
            assert_eq!(w.id as usize, i, "{}", w.name());
        }
    }

    #[test]
    fn falloff() {
        let m4 = WeaponId::Uar15.def();
        assert_eq!(m4.damage_at(5.0), 30.0);
        assert_eq!(m4.damage_at(100.0), 22.0);
        assert!((m4.damage_at(45.0) - 26.0).abs() < 0.01);
    }

    #[test]
    fn default_classes_valid() {
        for (_, l) in DEFAULT_CLASSES {
            assert_eq!(l.sanitized(), l);
        }
    }
}
