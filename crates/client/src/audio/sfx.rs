//! The game's sounds by name, which gun makes which, reload cues, variants.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    /// A shot of this weapon.
    Gun(WeaponId),
    /// The helicopter's door gun.
    HeliGun,
    /// Muffled gunfire far away.
    Far,
    Explosion,
    /// A smoke grenade popping and hissing.
    Smoke,
    /// Hitmarker tick.
    Hit,
    /// Headshot "ding".
    HitHead,
    Kill,
    Step,
    Land,
    MagOut,
    MagIn,
    Bolt,
    Pump,
    ShellIn,
    /// Brass casing hitting the floor.
    Shell,
    /// Weapon raised / switched.
    Raise,
    Empty,
    Knife,
    KnifeHit,
    /// Bullet impact on a body.
    Flesh,
    /// A bullet passing close by.
    Whiz,
    Beep,
    Jet,
    Click,
    Streak,
    Hurt,
    Bomb,
    Heli,
    /// A shell whistling in (artillery).
    Whistle,
    /// Drone rotors (looped).
    Drone,
    /// A car or boat engine ticking over, and pulling (looped).
    EngineIdle,
    EngineRun,
    /// Rain falling all around (looped).
    Rain,
    /// A footstep on earth, grass or sand (recordings only; `Step` otherwise).
    StepSoft,
    /// A door opening / shutting (recordings only; a click otherwise).
    Door,
    DoorShut,
    /// A weapon's own reload sounds, where recordings exist (`mag_out_ak47`, …); the
    /// generic `MagOut`/`MagIn`/`Bolt` play for the rest.
    MagOutOf(WeaponId),
    MagInOf(WeaponId),
    BoltOf(WeaponId),
    /// Somebody on our side pointed something out: two quick rising notes.
    Ping,
    /// Something coming down: a rumble and chunks of wall clattering into a heap.
    Crumble,
    /// A tree going over: a creak, a crack and the thump of it hitting the ground.
    TreeFall,
    /// A countdown second (the last three a note higher).
    Tick,
    TickHigh,
    /// The alarm when respawns end (sabotage overtime).
    Siren,
}

impl Sfx {
    pub fn for_weapon(w: WeaponId) -> Sfx {
        Sfx::Gun(w)
    }

    /// This reload sound as made by weapon `w` (falls back to the generic one when the
    /// weapon has no recording of its own).
    pub fn of(self, w: WeaponId) -> Sfx {
        match self {
            Sfx::MagOut => Sfx::MagOutOf(w),
            Sfx::MagIn => Sfx::MagInOf(w),
            Sfx::Bolt => Sfx::BoltOf(w),
            other => other,
        }
    }

    /// What to play when this sound has nothing loaded.
    pub(super) fn fallback(self) -> Option<Sfx> {
        match self {
            Sfx::MagOutOf(_) => Some(Sfx::MagOut),
            Sfx::MagInOf(_) => Some(Sfx::MagIn),
            Sfx::BoltOf(_) => Some(Sfx::Bolt),
            Sfx::StepSoft => Some(Sfx::Step),
            Sfx::Door | Sfx::DoorShut => Some(Sfx::Click),
            _ => None,
        }
    }

    /// File name stem for a recorded replacement in `assets/sounds/`.
    pub fn file_name(self) -> String {
        match self {
            Sfx::Gun(w) => format!("gun_{}", weapon_stem(w)),
            Sfx::MagOutOf(w) => format!("mag_out_{}", weapon_stem(w)),
            Sfx::MagInOf(w) => format!("mag_in_{}", weapon_stem(w)),
            Sfx::BoltOf(w) => format!("bolt_{}", weapon_stem(w)),
            Sfx::HeliGun => "heli_gun".into(),
            Sfx::StepSoft => "step_soft".into(),
            Sfx::DoorShut => "door_shut".into(),
            Sfx::HitHead => "hit_head".into(),
            Sfx::MagOut => "mag_out".into(),
            Sfx::MagIn => "mag_in".into(),
            Sfx::ShellIn => "shell_in".into(),
            Sfx::KnifeHit => "knife_hit".into(),
            other => format!("{other:?}").to_lowercase(),
        }
    }

    /// Comes from somewhere in the world, so it gets a left and a right version too.
    pub(super) fn placed(self) -> bool {
        matches!(
            self,
            Sfx::Gun(_)
                | Sfx::HeliGun
                | Sfx::Far
                | Sfx::Explosion
                | Sfx::Step
                | Sfx::StepSoft
                | Sfx::Door
                | Sfx::DoorShut
                | Sfx::Flesh
                | Sfx::Knife
                | Sfx::Beep
                | Sfx::Smoke
                | Sfx::Crumble
                | Sfx::TreeFall
        )
    }

    /// A loop that comes from somewhere in the world (it pans round the listener).
    pub(super) fn looped_in_world(self) -> bool {
        matches!(
            self,
            Sfx::Heli | Sfx::Drone | Sfx::EngineIdle | Sfx::EngineRun
        )
    }

    /// Loud enough to be heard across the map.
    pub(super) fn loud(self) -> bool {
        match self {
            Sfx::Gun(w) => matches!(
                w.def().class,
                WeaponClass::Sniper | WeaponClass::Lmg | WeaponClass::Shotgun
            ),
            Sfx::HeliGun => true,
            _ => false,
        }
    }
}

/// The recordings each gun uses (several guns share one family's sounds; a stem with no
/// recordings is synthesized).
pub(super) fn weapon_stem(w: WeaponId) -> String {
    use WeaponId::*;
    match w {
        Uar15 | Fort221 => "m4",
        Ak12 | ScarH => "ak47",
        Vector | Mp7 => "p90",
        Bizon => "mp5",
        Svd | Mk14 | Barrett | Alligator => "m40",
        Negev => "m249",
        Pkm => "rpd",
        Awm => "r700",
        Fort17 => "m9",
        Glock18 => "usp",
        Revolver => "deagle",
        Aa12 => "aa12",
        Ksg => "ksg",
        Rpg7 => "rpg7",
        Crossbow => "crossbow",
        Bow => "bow",
    }
    .into()
}

/// Reload sound cues: (fraction of the reload, sound). Play them with `Sfx::of` so each
/// weapon uses its own recordings.
pub fn reload_cues(class: WeaponClass) -> &'static [(f32, Sfx)] {
    match class {
        WeaponClass::Shotgun => &[
            (0.15, Sfx::ShellIn),
            (0.35, Sfx::ShellIn),
            (0.55, Sfx::ShellIn),
            (0.75, Sfx::ShellIn),
            (0.9, Sfx::Pump),
        ],
        WeaponClass::Pistol => &[(0.2, Sfx::MagOut), (0.6, Sfx::MagIn), (0.85, Sfx::Bolt)],
        WeaponClass::Sniper => &[
            (0.15, Sfx::Bolt),
            (0.4, Sfx::MagOut),
            (0.68, Sfx::MagIn),
            (0.88, Sfx::Bolt),
        ],
        WeaponClass::Lmg => &[
            (0.15, Sfx::Bolt),
            (0.32, Sfx::MagOut),
            (0.62, Sfx::MagIn),
            (0.85, Sfx::Bolt),
        ],
        _ => &[(0.22, Sfx::MagOut), (0.62, Sfx::MagIn), (0.82, Sfx::Bolt)],
    }
}

/// Number of random variants generated per sound.
pub(super) fn variants(s: Sfx) -> usize {
    match s {
        Sfx::Gun(_) | Sfx::Far | Sfx::Explosion | Sfx::Hit | Sfx::Hurt => 2,
        Sfx::Step | Sfx::Whiz | Sfx::Shell | Sfx::Flesh => 4,
        Sfx::EngineIdle | Sfx::EngineRun => 1,
        // recordings only; without one the generic sound plays
        Sfx::MagOutOf(_) | Sfx::MagInOf(_) | Sfx::BoltOf(_) => 0,
        Sfx::StepSoft | Sfx::Door | Sfx::DoorShut => 0,
        _ => 1,
    }
}

pub(super) fn all() -> Vec<Sfx> {
    let mut v: Vec<Sfx> = WeaponId::all().map(Sfx::Gun).collect();
    v.extend([
        Sfx::HeliGun,
        Sfx::Far,
        Sfx::Explosion,
        Sfx::Smoke,
        Sfx::Hit,
        Sfx::HitHead,
        Sfx::Kill,
        Sfx::Step,
        Sfx::Land,
        Sfx::MagOut,
        Sfx::MagIn,
        Sfx::Bolt,
        Sfx::Pump,
        Sfx::ShellIn,
        Sfx::Shell,
        Sfx::Raise,
        Sfx::Empty,
        Sfx::Knife,
        Sfx::KnifeHit,
        Sfx::Flesh,
        Sfx::Whiz,
        Sfx::Beep,
        Sfx::Jet,
        Sfx::Click,
        Sfx::Streak,
        Sfx::Hurt,
        Sfx::Bomb,
        Sfx::Heli,
        Sfx::Whistle,
        Sfx::Drone,
        Sfx::EngineIdle,
        Sfx::EngineRun,
        Sfx::Rain,
        Sfx::StepSoft,
        Sfx::Door,
        Sfx::DoorShut,
        Sfx::Ping,
        Sfx::Crumble,
        Sfx::TreeFall,
        Sfx::Tick,
        Sfx::TickHigh,
        Sfx::Siren,
    ]);
    for w in WeaponId::all() {
        v.extend([Sfx::MagOutOf(w), Sfx::MagInOf(w), Sfx::BoltOf(w)]);
    }
    v
}
