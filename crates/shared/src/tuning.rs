//! Server-tunable gameplay numbers: movement feel and per-weapon behaviour.
//!
//! The server owns a [`Tuning`] (from `server.json`, editable live by admins) and sends it to
//! every client, so client-side movement prediction and weapon handling use exactly the same
//! numbers as the authoritative simulation.

use crate::weapons::{WeaponClass, WeaponDef, WeaponId, WEAPONS};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::OnceLock;

trait ClampVal: Sized {
    fn clamp_to(self, lo: f32, hi: f32) -> Self;
}
impl ClampVal for f32 {
    fn clamp_to(self, lo: f32, hi: f32) -> f32 {
        if self.is_finite() {
            self.clamp(lo, hi)
        } else {
            lo
        }
    }
}
impl ClampVal for u16 {
    fn clamp_to(self, lo: f32, hi: f32) -> u16 {
        self.clamp(lo as u16, hi as u16)
    }
}
impl ClampVal for u8 {
    fn clamp_to(self, lo: f32, hi: f32) -> u8 {
        self.clamp(lo as u8, hi as u8)
    }
}
impl ClampVal for bool {
    fn clamp_to(self, _: f32, _: f32) -> bool {
        self
    }
}

/// Field metadata: (key, Ukrainian label, min, max). Used for validation, help and the web panel.
pub type FieldInfo = (&'static str, &'static str, f32, f32);

macro_rules! tuned {
    ($(#[$m:meta])* pub struct $name:ident { $( $field:ident : $ty:ty = $def:expr, [$min:expr, $max:expr], $label:expr; )* }) => {
        $(#[$m])*
        pub struct $name { $( pub $field: $ty, )* }
        impl Default for $name {
            fn default() -> Self { $name { $( $field: $def, )* } }
        }
        impl $name {
            pub const FIELDS: &'static [FieldInfo] = &[ $( (stringify!($field), $label, $min as f32, $max as f32), )* ];
            /// Clamps every field into its allowed range.
            pub fn clamp_all(&mut self) { $( self.$field = ClampVal::clamp_to(self.$field, $min as f32, $max as f32); )* }
        }
    };
}

tuned! {
    /// Movement feel.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    pub struct MoveTuning {
        walk_speed: f32 = 5.2, [0.5, 20.0], "Швидкість ходьби, м/с";
        sprint_speed: f32 = 7.6, [0.5, 25.0], "Швидкість спринту, м/с";
        crouch_speed: f32 = 2.7, [0.2, 15.0], "Швидкість у присіді, м/с";
        prone_speed: f32 = 1.1, [0.1, 10.0], "Швидкість лежачи, м/с";
        ads_speed_mult: f32 = 0.6, [0.1, 1.5], "Множник швидкості під час прицілювання";
        backpedal_mult: f32 = 0.8, [0.1, 1.5], "Множник швидкості руху назад";
        strafe_mult: f32 = 0.9, [0.1, 1.5], "Множник швидкості вбік";
        ground_accel: f32 = 38.0, [1.0, 200.0], "Розгін на землі, м/с²";
        ground_decel: f32 = 32.0, [1.0, 200.0], "Гальмування на землі, м/с²";
        sprint_accel: f32 = 12.0, [1.0, 200.0], "Розгін від ходьби до спринту, м/с²";
        air_accel: f32 = 5.0, [0.0, 100.0], "Керування в повітрі, м/с²";
        weight_inertia: f32 = 0.05, [0.0, 1.0], "Інерція від ваги зброї (на кг)";
        gravity: f32 = 20.0, [1.0, 60.0], "Гравітація, м/с²";
        jump_speed: f32 = 6.6, [0.0, 20.0], "Сила стрибка, м/с";
        sprint_time: f32 = 4.0, [0.5, 60.0], "Тривалість спринту, с";
        sprint_recover: f32 = 0.6, [0.05, 10.0], "Відновлення витривалості, с за с";
        view_bob: f32 = 1.0, [0.0, 3.0], "Розгойдування камери/зброї при ходьбі";
    }
}

tuned! {
    /// Per-weapon behaviour.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    pub struct WeaponStats {
        damage_max: f32 = 30.0, [1.0, 500.0], "Урон зблизька";
        damage_min: f32 = 20.0, [1.0, 500.0], "Урон здалеку";
        range_near: f32 = 30.0, [0.0, 1000.0], "Дистанція без спаду урону, м";
        range_far: f32 = 60.0, [1.0, 1000.0], "Дистанція мінімального урону, м";
        head_mult: f32 = 1.4, [0.5, 5.0], "Множник урону в голову";
        rpm: f32 = 750.0, [10.0, 2000.0], "Темп стрільби, постр./хв";
        auto: bool = true, [0.0, 1.0], "Автоматичний вогонь (true/false)";
        pellets: u8 = 1, [1.0, 20.0], "Дробин за постріл";
        mag: u16 = 30, [1.0, 500.0], "Набоїв у магазині";
        reserve: u16 = 120, [0.0, 2000.0], "Запасні набої";
        reload_time: f32 = 2.2, [0.2, 15.0], "Перезарядка, с";
        swap_time: f32 = 0.55, [0.1, 3.0], "Час діставання зброї, с";
        hip_spread: f32 = 3.0, [0.0, 20.0], "Розкид від стегна, °";
        ads_spread: f32 = 0.15, [0.0, 10.0], "Розкид у прицілі, °";
        move_spread: f32 = 2.0, [0.0, 20.0], "Додатковий розкид у русі, °";
        shot_spread: f32 = 0.35, [0.0, 5.0], "Розкид за кожен постріл, °";
        spread_recover: f32 = 8.0, [0.5, 50.0], "Відновлення розкиду, °/с";
        ads_time: f32 = 0.22, [0.05, 2.0], "Час прицілювання (ADS), с";
        ads_zoom: f32 = 1.35, [1.0, 10.0], "Збільшення в прицілі";
        sway: f32 = 1.0, [0.0, 5.0], "Коливання прицілу";
        recoil_v: f32 = 0.7, [0.0, 15.0], "Віддача вгору, °";
        recoil_h: f32 = 0.35, [0.0, 10.0], "Віддача вбік, °";
        first_shot_mult: f32 = 1.0, [0.0, 3.0], "Віддача першого пострілу (множник)";
        recoil_ramp: f32 = 0.03, [0.0, 0.5], "Наростання віддачі за постріл у черзі";
        recoil_max_ramp: f32 = 1.6, [1.0, 5.0], "Максимальне наростання віддачі";
        recoil_recover: f32 = 3.0, [0.0, 30.0], "Повернення прицілу після черги, 1/с";
        crouch_recoil: f32 = 0.8, [0.1, 1.0], "Множник віддачі в присіді";
        prone_recoil: f32 = 0.6, [0.1, 1.0], "Множник віддачі лежачи (сошки)";
        weight: f32 = 3.5, [0.0, 30.0], "Вага, кг (інерція руху)";
        move_mult: f32 = 0.94, [0.3, 1.5], "Множник швидкості руху";
        sprint_mult: f32 = 0.97, [0.3, 1.5], "Множник швидкості спринту";
        sprint_out_time: f32 = 0.25, [0.0, 2.0], "Затримка пострілу після спринту, с";
    }
}

tuned! {
    /// Frag grenades.
    #[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
    #[serde(default)]
    pub struct GrenadeTuning {
        throw_speed: f32 = 17.0, [3.0, 60.0], "Швидкість кидка, м/с (більше = далі)";
        throw_arc: f32 = 10.0, [-10.0, 45.0], "Додатковий кут кидка вгору, °";
        inherit_velocity: f32 = 0.5, [0.0, 1.0], "Частка швидкості бігу, що передається гранаті";
        fuse: f32 = 3.5, [0.5, 10.0], "Запал, с";
        radius: f32 = 6.5, [1.0, 30.0], "Радіус вибуху, м";
        damage: f32 = 150.0, [1.0, 1000.0], "Урон в епіцентрі";
        bounce: f32 = 0.4, [0.0, 0.95], "Пружність відскоку (0 — прилипає)";
    }
}

impl GrenadeTuning {
    /// Approximate max throw distance on flat ground (45°), metres.
    pub fn max_range(&self, gravity: f32) -> f32 {
        self.throw_speed * self.throw_speed / gravity.max(0.1)
    }
}

impl WeaponStats {
    pub fn fire_interval(&self) -> f32 {
        60.0 / self.rpm.max(1.0)
    }

    pub fn damage_at(&self, dist: f32) -> f32 {
        if dist <= self.range_near {
            self.damage_max
        } else if dist >= self.range_far {
            self.damage_min
        } else {
            let t = (dist - self.range_near) / (self.range_far - self.range_near).max(0.01);
            self.damage_max + (self.damage_min - self.damage_max) * t
        }
    }

    /// Factory defaults: the base table plus class-specific handling ("heavy LMG", "snappy pistol").
    pub fn from_def(d: &WeaponDef) -> WeaponStats {
        // (weight kg, sprint mult, sprint-out s, swap s, ramp, max ramp, recover, first shot, sway, crouch, prone, spread recover)
        let (
            weight,
            sprint,
            sprint_out,
            swap,
            ramp,
            max_ramp,
            recover,
            first,
            sway,
            crouch,
            prone,
            srec,
        ) = match d.class {
            WeaponClass::Rifle => (
                3.6, 0.96, 0.25, 0.55, 0.03, 1.5, 3.0, 1.0, 1.0, 0.8, 0.6, 8.0,
            ),
            WeaponClass::Smg => (
                2.7, 1.0, 0.15, 0.45, 0.02, 1.4, 4.0, 1.0, 0.8, 0.85, 0.7, 10.0,
            ),
            // heavy: slow to move and aim, recoil builds over a long burst, steady on a bipod
            WeaponClass::Lmg => (
                8.0, 0.84, 0.5, 0.95, 0.04, 1.8, 1.5, 0.7, 1.5, 0.65, 0.35, 5.0,
            ),
            WeaponClass::Shotgun => (
                3.6, 0.96, 0.3, 0.6, 0.0, 1.0, 5.0, 1.0, 1.0, 0.85, 0.7, 12.0,
            ),
            WeaponClass::Sniper => (6.0, 0.9, 0.5, 0.8, 0.0, 1.0, 2.0, 1.0, 1.6, 0.75, 0.5, 6.0),
            WeaponClass::Pistol => (
                1.0, 1.05, 0.1, 0.35, 0.05, 1.5, 6.0, 1.0, 0.7, 0.9, 0.8, 12.0,
            ),
        };
        WeaponStats {
            damage_max: d.damage_max,
            damage_min: d.damage_min,
            range_near: d.range_near,
            range_far: d.range_far,
            head_mult: d.head_mult,
            rpm: d.rpm,
            auto: d.auto,
            pellets: d.pellets,
            mag: d.mag,
            reserve: d.reserve,
            reload_time: d.reload_time,
            swap_time: swap,
            hip_spread: d.hip_spread,
            ads_spread: d.ads_spread,
            move_spread: d.move_spread,
            shot_spread: d.shot_spread,
            spread_recover: srec,
            ads_time: d.ads_time,
            ads_zoom: d.ads_zoom,
            sway,
            recoil_v: d.recoil_v,
            recoil_h: d.recoil_h,
            first_shot_mult: first,
            recoil_ramp: ramp,
            recoil_max_ramp: max_ramp,
            recoil_recover: recover,
            crouch_recoil: crouch,
            prone_recoil: prone,
            weight,
            move_mult: d.move_mult,
            sprint_mult: sprint,
            sprint_out_time: sprint_out,
        }
    }

    /// Recoil multiplier for the `n`-th consecutive shot (0 = first).
    pub fn recoil_scale(&self, n: u32) -> f32 {
        if n == 0 {
            self.first_shot_mult
        } else {
            (1.0 + self.recoil_ramp * n as f32).min(self.recoil_max_ramp)
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Tuning {
    pub movement: MoveTuning,
    pub grenade: GrenadeTuning,
    /// Keyed by [`WeaponId::key`] for readable configs.
    pub weapons: BTreeMap<String, WeaponStats>,
    /// Fast lookup by weapon id, rebuilt by [`Tuning::finalize`].
    #[serde(skip)]
    cache: Vec<WeaponStats>,
}

fn default_weapons() -> &'static Vec<WeaponStats> {
    static D: OnceLock<Vec<WeaponStats>> = OnceLock::new();
    D.get_or_init(|| WEAPONS.iter().map(WeaponStats::from_def).collect())
}

impl Default for Tuning {
    fn default() -> Self {
        let weapons = WEAPONS
            .iter()
            .map(|d| (d.id.key().to_string(), WeaponStats::from_def(d)))
            .collect();
        let mut t = Tuning {
            movement: MoveTuning::default(),
            grenade: GrenadeTuning::default(),
            weapons,
            cache: Vec::new(),
        };
        t.finalize();
        t
    }
}

impl Tuning {
    /// Validates everything (ranges, missing/unknown weapons) and rebuilds the lookup cache.
    /// Must be called after deserializing.
    pub fn finalize(&mut self) {
        self.movement.clamp_all();
        self.grenade.clamp_all();
        self.weapons.retain(|k, _| WeaponId::from_key(k).is_some());
        for d in WEAPONS.iter() {
            self.weapons
                .entry(d.id.key().to_string())
                .or_insert_with(|| WeaponStats::from_def(d));
        }
        for w in self.weapons.values_mut() {
            w.clamp_all();
            if w.range_far < w.range_near {
                w.range_far = w.range_near;
            }
        }
        self.cache = WeaponId::all()
            .map(|id| self.weapons[id.key()].clone())
            .collect();
    }

    pub fn weapon(&self, id: WeaponId) -> &WeaponStats {
        if self.cache.len() == WEAPONS.len() {
            &self.cache[id as usize]
        } else {
            &default_weapons()[id as usize]
        }
    }

    /// Movement speed multiplier for a weapon (and the max sprint speed it allows).
    pub fn max_sprint_speed(&self) -> f32 {
        let best = self
            .cache
            .iter()
            .map(|w| w.move_mult * w.sprint_mult)
            .fold(1.0f32, f32::max);
        self.movement.sprint_speed * best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_cover_all_weapons() {
        let t = Tuning::default();
        for id in WeaponId::all() {
            assert_eq!(t.weapon(id).mag, id.def().mag);
        }
        // the LMG is heavier and slower than an SMG
        let lmg = t.weapon(WeaponId::M249);
        let smg = t.weapon(WeaponId::Mp5);
        assert!(
            lmg.weight > smg.weight && lmg.move_mult < smg.move_mult && lmg.ads_time > smg.ads_time
        );
    }

    #[test]
    fn finalize_clamps_and_repairs() {
        let mut t = Tuning::default();
        t.movement.walk_speed = -5.0;
        t.weapons.get_mut("m4").unwrap().rpm = f32::NAN;
        t.weapons.remove("ak47");
        t.weapons.insert("laser".into(), WeaponStats::default());
        t.finalize();
        assert_eq!(t.movement.walk_speed, 0.5);
        assert_eq!(t.weapon(WeaponId::M4).rpm, 10.0);
        assert_eq!(t.weapon(WeaponId::Ak47).mag, 30);
        assert!(!t.weapons.contains_key("laser"));
    }

    #[test]
    fn bincode_roundtrip() {
        let t = Tuning::default();
        let bytes = bincode::serialize(&t).unwrap();
        let mut back: Tuning = bincode::deserialize(&bytes).unwrap();
        back.finalize();
        assert_eq!(back.weapon(WeaponId::Rpd), t.weapon(WeaponId::Rpd));
    }

    #[test]
    fn recoil_ramps_up_for_lmg() {
        let t = Tuning::default();
        let l = t.weapon(WeaponId::M249);
        assert!(l.recoil_scale(20) > l.recoil_scale(1));
        assert!(l.recoil_scale(1000) <= l.recoil_max_ramp);
    }
}
