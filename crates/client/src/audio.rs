//! Sound effects: synthesized at start-up (no asset files needed), or real recordings
//! dropped into `assets/sounds/` (see `assets/sounds/README.md`), which take precedence.
//!
//! The synthesis follows how real recordings are built up. A gunshot is the supersonic
//! crack (an N-shaped pressure wave), the muzzle blast (a noise burst shaped by resonant
//! filters), a low punch, the clack of the action, then the town answering: early
//! reflections off nearby walls and a stereo reverb tail where the highs die first.
//! Mechanical sounds ring through banks of resonators tuned like small steel parts.
//! Frequently heard sounds come in several random variants so bursts never loop audibly.

use macroquad::audio::{
    load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound,
};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::{PI, TAU};
use toon_shared::rng::Rng;
use toon_shared::weapons::{WeaponClass, WeaponId};

const RATE: u32 = 44100;
const SR: f32 = RATE as f32;

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
    fn fallback(self) -> Option<Sfx> {
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
    fn placed(self) -> bool {
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
    fn looped_in_world(self) -> bool {
        matches!(
            self,
            Sfx::Heli | Sfx::Drone | Sfx::EngineIdle | Sfx::EngineRun
        )
    }

    /// Loud enough to be heard across the map.
    fn loud(self) -> bool {
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
fn weapon_stem(w: WeaponId) -> String {
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

/// 16-bit stereo WAV.
fn wav(samples: &[[f32; 2]]) -> Vec<u8> {
    let data_len = (samples.len() * 4) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&2u16.to_le_bytes()); // stereo
    v.extend_from_slice(&RATE.to_le_bytes());
    v.extend_from_slice(&(RATE * 4).to_le_bytes());
    v.extend_from_slice(&4u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        for c in s {
            let i = (c.clamp(-1.0, 1.0) * 32000.0) as i16;
            v.extend_from_slice(&i.to_le_bytes());
        }
    }
    v
}

// ---------------------------------------------------------------- DSP building blocks

/// Second-order filter (RBJ cookbook), transposed direct form II.
#[derive(Clone, Copy, Default)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

#[derive(Clone, Copy)]
enum Kind {
    Low,
    High,
    /// Band-pass with 0 dB at the peak.
    Band,
    /// Peaking EQ (gain in dB).
    Peak(f32),
}

impl Biquad {
    fn new(kind: Kind, f: f32, q: f32) -> Biquad {
        let mut b = Biquad::default();
        b.set(kind, f, q);
        b
    }
    fn set(&mut self, kind: Kind, f: f32, q: f32) {
        let w0 = TAU * f.clamp(10.0, SR * 0.45) / SR;
        let (s, c) = w0.sin_cos();
        let alpha = s / (2.0 * q.max(0.05));
        let (b0, b1, b2, a0, a1, a2) = match kind {
            Kind::Low => (
                (1.0 - c) / 2.0,
                1.0 - c,
                (1.0 - c) / 2.0,
                1.0 + alpha,
                -2.0 * c,
                1.0 - alpha,
            ),
            Kind::High => (
                (1.0 + c) / 2.0,
                -(1.0 + c),
                (1.0 + c) / 2.0,
                1.0 + alpha,
                -2.0 * c,
                1.0 - alpha,
            ),
            Kind::Band => (alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * c, 1.0 - alpha),
            Kind::Peak(db) => {
                let a = 10f32.powf(db / 40.0);
                (
                    1.0 + alpha * a,
                    -2.0 * c,
                    1.0 - alpha * a,
                    1.0 + alpha / a,
                    -2.0 * c,
                    1.0 - alpha / a,
                )
            }
        };
        self.b0 = b0 / a0;
        self.b1 = b1 / a0;
        self.b2 = b2 / a0;
        self.a1 = a1 / a0;
        self.a2 = a2 / a0;
    }
    fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Runs a whole buffer through a chain of filters.
fn filter(b: &mut [f32], chain: &[(Kind, f32, f32)]) {
    for &(k, f, q) in chain {
        let mut bq = Biquad::new(k, f, q);
        for v in b.iter_mut() {
            *v = bq.run(*v);
        }
    }
}

/// How a place sounds. The reverb is a convolution with velvet noise — sparse random
/// clicks that fade away — split into a low band that rings long and a high band that dies
/// fast. Unlike comb-filter reverbs it has no resonances, so nothing sounds like a pipe.
#[derive(Clone, Copy)]
struct Space {
    /// Gap before the first reflection (s).
    predelay: f32,
    /// Reflections per second at first (sparse, distinct walls) and later (diffuse).
    early: f32,
    density: f32,
    /// How long the early, sparse part lasts (s).
    early_len: f32,
    /// Decay to -60 dB of the lows and of the highs (s).
    rt_low: f32,
    rt_high: f32,
    /// Level of the reverb against the direct sound.
    wet: f32,
    /// Low-pass on the reverb (air absorbs the highs).
    air: f32,
}

/// A town street: houses all around, hard walls.
const TOWN: Space = Space {
    predelay: 0.012,
    early: 220.0,
    density: 1100.0,
    early_len: 0.09,
    rt_low: 1.2,
    rt_high: 0.45,
    wet: 0.5,
    air: 5000.0,
};

/// Explosions: the blast rolls around the whole map.
const VALLEY: Space = Space {
    predelay: 0.03,
    early: 120.0,
    density: 900.0,
    early_len: 0.2,
    rt_low: 2.4,
    rt_high: 0.8,
    wet: 0.6,
    air: 3000.0,
};

/// Close handling sounds: a hint of space only.
const NEAR: Space = Space {
    predelay: 0.003,
    early: 600.0,
    density: 1500.0,
    early_len: 0.02,
    rt_low: 0.2,
    rt_high: 0.1,
    wet: 0.18,
    air: 7000.0,
};

/// Velvet-noise impulse response: (offset in samples, gain), energy normalized to 1.
fn velvet(sp: &Space, rt: f32, rng: &mut Rng) -> Vec<(usize, f32)> {
    let len = rt * 1.1;
    let mut taps = Vec::new();
    let mut t = sp.predelay;
    while t < sp.predelay + len {
        let dens = if t < sp.predelay + sp.early_len {
            sp.early
        } else {
            sp.density
        };
        let period = 1.0 / dens;
        let at = t + rng.f32() * period;
        let sign = if rng.f32() < 0.5 { -1.0 } else { 1.0 };
        // the first reflections are a bit louder: those are the nearest walls
        let g = (-6.9 * (at - sp.predelay) / rt).exp() * if dens == sp.early { 1.8 } else { 1.0 };
        taps.push(((at * SR) as usize, sign * g * rng.range(0.6, 1.0)));
        t += period;
    }
    let e: f32 = taps.iter().map(|t| t.1 * t.1).sum::<f32>().sqrt().max(1e-6);
    for t in &mut taps {
        t.1 /= e;
    }
    taps
}

/// Sparse convolution of `x` with velvet taps, added into one channel of `out`.
fn convolve(out: &mut [[f32; 2]], ch: usize, x: &[f32], taps: &[(usize, f32)], gain: f32) {
    for &(off, g) in taps {
        let g = g * gain;
        if off >= out.len() {
            continue;
        }
        let n = x.len().min(out.len() - off);
        for (o, v) in out[off..off + n].iter_mut().zip(&x[..n]) {
            o[ch] += v * g;
        }
    }
}

/// Places a mono dry sound into a space; returns stereo with `tail` seconds of decay.
fn spatialize(dry: &[f32], sp: Space, tail: f32, rng: &mut Rng) -> Vec<[f32; 2]> {
    let n = dry.len() + (tail * SR) as usize;
    let mut out = vec![[0.0f32; 2]; n];
    for (i, v) in dry.iter().enumerate() {
        out[i] = [*v, *v];
    }
    // only the audible part of the dry sound feeds the reverb (keeps this fast)
    let peak = dry.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let end = dry
        .iter()
        .rposition(|v| v.abs() > peak * 0.002)
        .map_or(0, |i| i + 1)
        // a long rumble is diffuse already: its start is enough to excite the room
        .min((0.35 * SR) as usize);
    let mut low = dry[..end].to_vec();
    filter(
        &mut low,
        &[(Kind::Low, 900.0, 0.7), (Kind::Low, 900.0, 0.7)],
    );
    let mut high: Vec<f32> = dry[..end].iter().zip(&low).map(|(d, l)| d - l).collect();
    filter(&mut high, &[(Kind::Low, sp.air, 0.7)]);
    // the highs of a shot die within milliseconds: trim them on their own
    let hp = high.iter().fold(0.0f32, |m, v| m.max(v.abs()));
    let hend = high
        .iter()
        .rposition(|v| v.abs() > hp * 0.003)
        .map_or(0, |i| i + 1);
    high.truncate(hend);
    // the lows need no fine time resolution: reverberate them at an eighth of the rate
    const DEC: usize = 8;
    let low_d: Vec<f32> = low.iter().step_by(DEC).copied().collect();
    let nd = n / DEC + 2;
    let mut wet = vec![[0.0f32; 2]; n];
    let mut wet_d = vec![[0.0f32; 2]; nd];
    for ch in 0..2 {
        let lo: Vec<(usize, f32)> = velvet(&sp, sp.rt_low, rng)
            .into_iter()
            .map(|(o, g)| (o / DEC, g))
            .collect();
        let hi = velvet(&sp, sp.rt_high, rng);
        convolve(&mut wet_d, ch, &low_d, &lo, 1.0);
        convolve(&mut wet, ch, &high, &hi, 1.0);
    }
    for ch in 0..2 {
        let mut smooth = Biquad::new(Kind::Low, 1100.0, 0.7);
        for (i, w) in wet.iter_mut().enumerate() {
            let x = i as f32 / DEC as f32;
            let k = x.fract();
            let a = wet_d[x as usize][ch];
            let b = wet_d[(x as usize + 1).min(nd - 1)][ch];
            w[ch] += smooth.run(a + (b - a) * k);
        }
    }
    // the rumble below 80 Hz would only make the tail boomy
    for ch in 0..2 {
        let mut hp = Biquad::new(Kind::High, 80.0, 0.7);
        for (o, w) in out.iter_mut().zip(&wet) {
            o[ch] += hp.run(w[ch]) * sp.wet;
        }
    }
    out
}

/// Where around the listener the prepared versions of a sound in the world come from,
/// in degrees (0 ahead, 90 to the right, -90 to the left): straight ahead plays the sound
/// as it is; the rest are made once at start. Six of them (every 45° or so, the back
/// shared by the two back-quarter ones) keep the memory down on weak machines.
pub const DIRS: [f32; 6] = [-140.0, -90.0, -45.0, 45.0, 90.0, 140.0];

/// The sound as heard from `az` degrees, through headphones: the far ear hears it a moment
/// later (up to 0.66 ms), quieter and duller (the head in the way); from behind both ears
/// hear it a little muffled (the ears point forward). `looped`: the sound is a loop, so it
/// is processed as one (no click where it starts again).
fn binaural(pcm: &[[f32; 2]], az: f32, looped: bool) -> Vec<[f32; 2]> {
    let r = az.to_radians();
    let (lat, front) = (r.sin(), r.cos());
    let side = lat.abs();
    // 0: left ear, 1: right ear; the sound to the right reaches the left ear last
    let far = if lat >= 0.0 { 0 } else { 1 };
    let near = 1 - far;
    let delay = (0.00066 * side * SR).round() as usize;
    let far_gain = 1.0 - 0.55 * side;
    let near_gain = 1.0 + 0.12 * side;
    let shadow = 1400.0 + (1.0 - side) * 9000.0;
    let back = (-front).max(0.0);
    let pinna = 14000.0 - 10500.0 * back;
    let n = pcm.len();
    // a loop goes round twice: filters warmed up, the delay wraps
    let passes = if looped { 2 } else { 1 };
    let mut out = vec![[0.0f32; 2]; n];
    let mut sh = Biquad::new(Kind::Low, shadow, 0.7);
    let mut pin = [
        Biquad::new(Kind::Low, pinna, 0.7),
        Biquad::new(Kind::Low, pinna, 0.7),
    ];
    for pass in 0..passes {
        for i in 0..n {
            let get = |ch: usize, back_by: usize| -> f32 {
                if looped {
                    pcm[(i + n - back_by % n) % n][ch]
                } else if i >= back_by {
                    pcm[i - back_by][ch]
                } else {
                    0.0
                }
            };
            let mut v = [0.0f32; 2];
            v[near] = get(near, 0) * near_gain;
            v[far] = sh.run(get(far, delay)) * far_gain;
            if back > 0.05 {
                for ch in 0..2 {
                    v[ch] = pin[ch].run(v[ch]) * (1.0 - 0.12 * back);
                }
            }
            if pass + 1 == passes {
                out[i] = v;
            }
        }
    }
    out
}

/// The prepared versions of a sound (one list of variants per direction in `DIRS`): short
/// sounds get all their variants (footsteps should not repeat), long ones the first.
async fn add_dirs(dirs: &mut HashMap<Sfx, Vec<Vec<Sound>>>, k: Sfx, pcm: &[[f32; 2]]) {
    let e = dirs
        .entry(k)
        .or_insert_with(|| vec![Vec::new(); DIRS.len()]);
    let short = pcm.len() < (0.4 * SR) as usize;
    let have = e[0].len();
    if have >= if short { 4 } else { 1 } {
        return;
    }
    for (d, list) in e.iter_mut().enumerate() {
        if let Ok(snd) = load_sound_from_bytes(&wav(&binaural(pcm, DIRS[d], false))).await {
            list.push(snd);
        }
    }
}

/// Every direction of a looped sound: the sound itself (ahead), then one per `DIRS`.
async fn loop_dirs(pcm: &[[f32; 2]]) -> Vec<Sound> {
    let mut v = Vec::new();
    for pcm in std::iter::once(pcm.to_vec()).chain(DIRS.iter().map(|&az| binaural(pcm, az, true))) {
        if let Ok(snd) = load_sound_from_bytes(&wav(&pcm)).await {
            v.push(snd);
        }
    }
    v
}

fn mono(b: &[f32]) -> Vec<[f32; 2]> {
    b.iter().map(|v| [*v, *v]).collect()
}

/// How loud a sound `d` metres away is, out of 1: falling off with distance and gone at
/// `range` (no floor: what is far away is not heard).
pub fn falloff(d: f32, range: f32) -> f32 {
    let k = (1.0 - d / range.max(0.1)).clamp(0.0, 1.0);
    // close by it barely changes, then fades out smoothly
    k * k * (3.0 - 2.0 * k)
}

/// Volumes of a looped sound's versions (ahead, then `DIRS`) for a sound at `az` degrees:
/// the two directions either side of it, equal power.
fn pan_weights(az: f32) -> [f32; DIRS.len() + 1] {
    // every direction round the circle, with its version
    let mut ring: Vec<(f32, usize)> = std::iter::once((0.0, 0))
        .chain(DIRS.iter().enumerate().map(|(k, d)| (*d, k + 1)))
        .collect();
    ring.sort_by(|a, b| a.0.total_cmp(&b.0));
    let az = (az + 540.0).rem_euclid(360.0) - 180.0;
    let mut w = [0.0f32; DIRS.len() + 1];
    let n = ring.len();
    for i in 0..n {
        let (a, ka) = ring[i];
        let (b, kb) = ring[(i + 1) % n];
        let b = if i + 1 == n { b + 360.0 } else { b };
        let x = if az < a && i + 1 == n { az + 360.0 } else { az };
        if x >= a && x <= b {
            let t = (x - a) / (b - a).max(1e-3);
            w[ka] = (t * std::f32::consts::FRAC_PI_2).cos();
            w[kb] = (t * std::f32::consts::FRAC_PI_2).sin();
            return w;
        }
    }
    // behind, between the last and the first round the back
    let (a, ka) = ring[n - 1];
    let (b, kb) = ring[0];
    let x = if az < a { az + 360.0 } else { az };
    let t = ((x - a) / (b + 360.0 - a)).clamp(0.0, 1.0);
    w[ka] = (t * std::f32::consts::FRAC_PI_2).cos();
    w[kb] = (t * std::f32::consts::FRAC_PI_2).sin();
    w
}

/// Pans a mono sound (-1 = left, 1 = right), equal power.
fn pan(b: &[f32], p: f32) -> Vec<[f32; 2]> {
    let a = (p.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
    let (l, r) = (a.cos() * 1.41, a.sin() * 1.41);
    b.iter().map(|v| [v * l, v * r]).collect()
}

fn normalize(b: &mut [[f32; 2]], peak: f32) {
    let m = b
        .iter()
        .fold(0.0f32, |m, v| m.max(v[0].abs()).max(v[1].abs()));
    if m > 1e-6 {
        for v in b {
            v[0] *= peak / m;
            v[1] *= peak / m;
        }
    }
}

/// Fades the last `secs` out so nothing ends in a click.
fn fade_out(b: &mut [[f32; 2]], secs: f32) {
    let n = ((secs * SR) as usize).min(b.len());
    let start = b.len() - n;
    for (k, v) in b[start..].iter_mut().enumerate() {
        let g = 1.0 - k as f32 / n as f32;
        v[0] *= g;
        v[1] *= g;
    }
}

// ---------------------------------------------------------------- synthesis

/// Parameters of a gunshot.
#[derive(Clone, Copy)]
struct Gun {
    /// Supersonic crack strength (0 for subsonic rounds).
    crack: f32,
    /// Where the blast's body sits (Hz) — lower for bigger calibres.
    body: f32,
    /// Upper limit of the blast (Hz).
    bright: f32,
    /// Fast and slow decay times of the blast (s).
    snap: f32,
    boom: f32,
    /// Low punch frequency and gain.
    punch: f32,
    punch_gain: f32,
    /// Clack of the action cycling (0 = bolt/pump: no clack).
    mech: f32,
    /// How much of the street answers.
    wet: f32,
}

fn gun_params(w: WeaponId) -> Gun {
    let g = |crack, body, bright, snap, boom, punch, punch_gain, mech, wet| Gun {
        crack,
        body,
        bright,
        snap,
        boom,
        punch,
        punch_gain,
        mech,
        wet,
    };
    use WeaponId::*;
    match w {
        Uar15 => g(0.8, 320.0, 7500.0, 0.006, 0.045, 70.0, 0.7, 0.25, 1.0),
        Fort221 => g(0.8, 300.0, 7800.0, 0.006, 0.048, 68.0, 0.75, 0.25, 1.0),
        Ak12 => g(0.8, 250.0, 6200.0, 0.007, 0.058, 60.0, 0.9, 0.3, 1.0),
        ScarH => g(0.9, 210.0, 6500.0, 0.008, 0.070, 52.0, 1.0, 0.3, 1.1),
        Vector => g(0.15, 480.0, 7200.0, 0.004, 0.030, 95.0, 0.5, 0.35, 0.8),
        Mp7 => g(0.6, 520.0, 9000.0, 0.004, 0.030, 95.0, 0.45, 0.25, 0.8),
        Bizon => g(0.25, 420.0, 7000.0, 0.005, 0.035, 85.0, 0.5, 0.3, 0.8),
        Svd => g(0.95, 200.0, 6500.0, 0.009, 0.085, 50.0, 1.15, 0.25, 1.2),
        Mk14 => g(0.9, 215.0, 6800.0, 0.008, 0.075, 52.0, 1.05, 0.3, 1.15),
        Negev => g(0.8, 290.0, 7000.0, 0.007, 0.055, 62.0, 0.85, 0.35, 1.1),
        Pkm => g(0.85, 220.0, 6000.0, 0.008, 0.068, 54.0, 1.0, 0.35, 1.15),
        Aa12 => g(0.0, 185.0, 5200.0, 0.009, 0.090, 48.0, 1.2, 0.25, 1.2),
        Ksg => g(0.0, 170.0, 4800.0, 0.010, 0.100, 45.0, 1.3, 0.0, 1.2),
        Awm => g(1.0, 180.0, 6200.0, 0.011, 0.120, 44.0, 1.35, 0.0, 1.4),
        Barrett => g(1.0, 130.0, 5200.0, 0.013, 0.160, 36.0, 1.6, 0.2, 1.6),
        Alligator => g(1.0, 110.0, 4800.0, 0.015, 0.190, 32.0, 1.8, 0.0, 1.7),
        Fort17 => g(0.15, 600.0, 8000.0, 0.004, 0.025, 110.0, 0.45, 0.3, 0.7),
        Glock18 => g(0.15, 620.0, 8200.0, 0.004, 0.024, 112.0, 0.42, 0.3, 0.7),
        Revolver => g(0.6, 280.0, 7000.0, 0.008, 0.070, 60.0, 1.1, 0.0, 1.0),
        // made by their own synthesis (`launch`, `twang`); these only feed far-away echoes
        Rpg7 => g(0.0, 120.0, 3000.0, 0.02, 0.2, 40.0, 1.4, 0.0, 1.5),
        Crossbow | Bow => g(0.0, 900.0, 4000.0, 0.003, 0.02, 120.0, 0.1, 0.0, 0.3),
    }
}

struct Synth {
    rng: Rng,
}

impl Synth {
    fn buf(secs: f32) -> Vec<f32> {
        vec![0.0; (secs * SR) as usize]
    }
    fn noise(&mut self) -> f32 {
        self.rng.f32() * 2.0 - 1.0
    }
    fn t(i: usize) -> f32 {
        i as f32 / SR
    }
    fn mix(a: &mut Vec<f32>, b: &[f32], at: f32, gain: f32) {
        let off = (at * SR) as usize;
        if a.len() < off + b.len() {
            a.resize(off + b.len(), 0.0);
        }
        for (i, v) in b.iter().enumerate() {
            a[off + i] += v * gain;
        }
    }
    fn peak(b: &[f32]) -> f32 {
        b.iter().fold(0.0f32, |m, v| m.max(v.abs()))
    }
    fn norm(b: &mut [f32], peak: f32) {
        let m = Synth::peak(b);
        if m > 1e-6 {
            for v in b {
                *v *= peak / m;
            }
        }
    }
    /// Soft clipping: loud and punchy without harsh distortion.
    fn saturate(b: &mut [f32], drive: f32) {
        for v in b {
            *v = (*v * drive).tanh();
        }
    }

    /// N-shaped pressure wave: a sharp crack (supersonic bullet, blast front).
    fn n_wave(len: f32) -> Vec<f32> {
        let n = (len * SR).max(4.0) as usize;
        (0..n)
            .map(|i| {
                let x = i as f32 / n as f32;
                (1.0 - 2.0 * x) * (1.0 - (x - 0.5).abs() * 0.3)
            })
            .collect()
    }

    /// A small steel part struck: noise excitation ringing through tuned resonators.
    fn clack(&mut self, freqs: &[f32], q: f32, len: f32, gain: f32) -> Vec<f32> {
        let mut b = Self::buf(len);
        let mut bank: Vec<Biquad> = freqs
            .iter()
            .map(|f| Biquad::new(Kind::Band, f * self.rng.range(0.96, 1.04), q))
            .collect();
        let exc = (0.0015 * SR) as usize;
        for (i, s) in b.iter_mut().enumerate() {
            let x = if i < exc {
                self.noise() * (1.0 - i as f32 / exc as f32)
            } else {
                0.0
            };
            let mut y = 0.0;
            for (k, r) in bank.iter_mut().enumerate() {
                y += r.run(x) / (1.0 + k as f32 * 0.5);
            }
            *s = y + x * 0.15;
        }
        Synth::norm(&mut b, gain);
        b
    }

    /// Filtered noise with an attack and exponential decay.
    fn burst(&mut self, len: f32, chain: &[(Kind, f32, f32)], attack: f32, decay: f32) -> Vec<f32> {
        let mut b: Vec<f32> = (0..(len * SR) as usize).map(|_| self.noise()).collect();
        filter(&mut b, chain);
        for (i, v) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            *v *= (t / attack.max(1e-4)).min(1.0) * (-t / decay).exp();
        }
        b
    }

    /// Sine with a falling pitch (thumps, punches, booms).
    fn drop(len: f32, f0: f32, f1: f32, fall: f32, decay: f32) -> Vec<f32> {
        let mut ph = 0.0f32;
        (0..(len * SR) as usize)
            .map(|i| {
                let t = Self::t(i);
                let f = f1 + (f0 - f1) * (-t / fall).exp();
                ph += f / SR;
                (ph * TAU).sin() * (-t / decay).exp() * (t / 0.001).min(1.0)
            })
            .collect()
    }

    /// The dry gunshot, before the street answers.
    fn shot_dry(&mut self, g: Gun) -> Vec<f32> {
        let mut b = Self::buf(0.4);
        // supersonic crack
        if g.crack > 0.0 {
            let n = Synth::n_wave(0.00045);
            Synth::mix(&mut b, &n, 0.0, 1.6 * g.crack);
        }
        // muzzle blast: two layers, a snappy bright one and a darker boom
        let at = if g.crack > 0.0 { 0.0012 } else { 0.0 };
        let bright = self.burst(
            0.15,
            &[
                (Kind::High, g.body * 0.6, 0.7),
                (Kind::Low, g.bright, 0.7),
                (Kind::Peak(3.0), 2800.0, 0.9),
            ],
            0.0002,
            g.snap,
        );
        Synth::mix(&mut b, &bright, at, 3.0);
        let boom = self.burst(
            0.4,
            // a broad hump around the body frequency (a narrow band-pass rings like a tube)
            &[
                (Kind::High, g.body * 0.45, 0.6),
                (Kind::Low, g.body * 3.0, 0.6),
            ],
            0.0006,
            g.boom,
        );
        Synth::mix(&mut b, &boom, at, 4.0);
        // low punch in the chest: short, felt rather than heard as a tone
        let p = Synth::drop(0.15, g.punch * 2.5, g.punch, 0.008, 0.022 + g.boom * 0.15);
        let level = Synth::peak(&b);
        Synth::mix(&mut b, &p, at, 0.35 * g.punch_gain * level);
        // gentle saturation, like a recording that is loud but not broken
        Synth::norm(&mut b, 1.0);
        Synth::saturate(&mut b, 1.5);
        Synth::norm(&mut b, 1.0);
        // the action cycling
        if g.mech > 0.0 {
            let c = self.clack(&[1650.0, 2900.0, 4300.0, 6100.0], 12.0, 0.05, 1.0);
            let at = 0.022 + self.rng.range(0.0, 0.01);
            Synth::mix(&mut b, &c, at, g.mech * 0.25);
        }
        b
    }

    fn shot(&mut self, g: Gun) -> Vec<[f32; 2]> {
        let dry = self.shot_dry(g);
        let sp = Space {
            wet: TOWN.wet * g.wet,
            ..TOWN
        };
        let mut out = spatialize(&dry, sp, 0.6 + g.wet * 0.5, &mut self.rng);
        fade_out(&mut out, 0.2);
        normalize(&mut out, 0.98);
        out
    }

    /// A rocket leaving the tube: a hollow thump, then the motor's roar fading away.
    fn launch(&mut self) -> Vec<[f32; 2]> {
        let mut b = self.burst(0.25, &[(Kind::Low, 260.0, 0.8)], 0.002, 0.07);
        Synth::norm(&mut b, 1.0);
        let len = 1.3;
        let mut roar = Self::buf(len);
        let mut bp = Biquad::default();
        for i in 0..roar.len() {
            let t = Self::t(i);
            if i % 64 == 0 {
                bp.set(Kind::Band, 1400.0 - 700.0 * (t / len), 0.6);
            }
            let env = (t / 0.03).min(1.0) * (1.0 - t / len).powf(2.0);
            roar[i] = bp.run(self.noise()) * env;
        }
        Synth::norm(&mut roar, 0.7);
        Synth::mix(&mut b, &roar, 0.02, 1.0);
        Synth::saturate(&mut b, 1.4);
        let sp = Space {
            wet: TOWN.wet * 1.3,
            ..TOWN
        };
        let mut out = spatialize(&b, sp, 1.2, &mut self.rng);
        fade_out(&mut out, 0.2);
        normalize(&mut out, 0.95);
        out
    }

    /// A bowstring let go: a low thrum that dies fast, a whip of air. `tone` 1 is the bow,
    /// lower is the crossbow's shorter, sharper string.
    fn twang(&mut self, tone: f32) -> Vec<[f32; 2]> {
        let len = 0.35;
        let mut b = Self::buf(len);
        let f = 95.0 / tone.max(0.3) * self.rng.range(0.95, 1.05);
        for (i, v) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            let env = (-t * (14.0 / tone)).exp();
            let wob = 1.0 + 0.04 * (-t * 30.0).exp();
            *v = ((t * f * wob * 2.0 * PI).sin() + 0.35 * (t * f * 3.0 * 2.0 * PI).sin()) * env;
        }
        let mut whip = self.burst(
            0.12,
            &[(Kind::Band, 2600.0 / tone.max(0.5), 0.8)],
            0.001,
            0.03,
        );
        Synth::norm(&mut whip, 0.5);
        Synth::mix(&mut b, &whip, 0.0, 1.0);
        let mut clack = self.clack(&[1800.0, 3100.0], 6.0, 0.05, 0.4 * (1.0 - tone).max(0.1));
        Synth::norm(&mut clack, 0.3);
        Synth::mix(&mut b, &clack, 0.0, 1.0);
        Synth::norm(&mut b, 0.9);
        let mut out = spatialize(&b, NEAR, 0.2, &mut self.rng);
        fade_out(&mut out, 0.08);
        out
    }

    /// Gunfire heard from far away: no crack, the highs lost on the way, mostly echo.
    fn far(&mut self) -> Vec<[f32; 2]> {
        let mut dry = self.shot_dry(gun_params(WeaponId::Ak12));
        filter(
            &mut dry,
            &[(Kind::Low, 900.0, 0.7), (Kind::Low, 1400.0, 0.7)],
        );
        let mut pre = Self::buf(0.02);
        pre.extend(dry);
        let sp = Space {
            wet: 0.6,
            air: 1400.0,
            ..TOWN
        };
        let mut out = spatialize(&pre, sp, 1.3, &mut self.rng);
        fade_out(&mut out, 0.3);
        normalize(&mut out, 0.85);
        out
    }

    fn explosion(&mut self) -> Vec<[f32; 2]> {
        let mut b = Self::buf(2.2);
        // blast front
        let n = Synth::n_wave(0.0025);
        Synth::mix(&mut b, &n, 0.0, 2.0);
        // roar: noise whose brightness falls as the fireball cools
        let mut lp = Biquad::default();
        let mut lp2 = Biquad::default();
        for i in 0..b.len() {
            let t = Self::t(i);
            if i % 32 == 0 {
                let f = 180.0 + 5200.0 * (-t / 0.18).exp();
                lp.set(Kind::Low, f, 0.8);
                lp2.set(Kind::Low, f * 1.3, 0.6);
            }
            let env = (t / 0.004).min(1.0) * ((-t / 0.22).exp() * 0.8 + (-t / 0.9).exp() * 0.35);
            b[i] += lp2.run(lp.run(self.noise())) * env * 3.0;
        }
        // deep boom: a short thump and a long rumble of low noise
        Synth::norm(&mut b, 1.0);
        let thump = Synth::drop(0.4, 110.0, 40.0, 0.04, 0.09);
        Synth::mix(&mut b, &thump, 0.002, 0.8);
        let mut sub = self.burst(
            2.0,
            &[(Kind::Low, 120.0, 0.7), (Kind::Low, 160.0, 0.7)],
            0.008,
            0.5,
        );
        Synth::norm(&mut sub, 1.0);
        Synth::mix(&mut b, &sub, 0.0, 0.9);
        Synth::norm(&mut b, 1.0);
        Synth::saturate(&mut b, 1.6);
        Synth::norm(&mut b, 1.0);
        // crackle and debris raining down
        for _ in 0..40 {
            let at = 0.05 + self.rng.f32().powi(2) * 1.6;
            let f = self.rng.range(1500.0, 5000.0);
            let c = self.burst(0.02, &[(Kind::Band, f, 2.0)], 0.0002, 0.004);
            Synth::mix(&mut b, &c, at, self.rng.range(0.05, 0.2));
        }
        for _ in 0..10 {
            let at = self.rng.range(0.35, 1.8);
            let f = self.rng.range(700.0, 2000.0);
            let c = self.clack(&[f, f * 1.7, f * 2.6], 12.0, 0.08, 1.0);
            Synth::mix(&mut b, &c, at, self.rng.range(0.03, 0.08));
        }
        let mut out = spatialize(&b, VALLEY, 1.8, &mut self.rng);
        fade_out(&mut out, 0.6);
        normalize(&mut out, 1.0);
        out
    }

    fn step(&mut self) -> Vec<f32> {
        // heel then toe: a dull thud and gritty scuff
        let mut b = Vec::new();
        let grit_f = self.rng.range(800.0, 1400.0);
        for (at, g) in [(0.0f32, 1.0f32), (self.rng.range(0.035, 0.055), 0.55)] {
            let thud = Synth::drop(0.06, 160.0, 80.0, 0.01, 0.018);
            let grit = self.burst(
                0.07,
                &[(Kind::Band, grit_f, 1.2), (Kind::Low, 3500.0, 0.7)],
                0.001,
                0.015,
            );
            Synth::mix(&mut b, &thud, at, 0.5 * g);
            Synth::mix(&mut b, &grit, at, 1.4 * g);
        }
        Synth::norm(&mut b, 0.7);
        b
    }

    /// Bullet fly-by from one side: a snap and a falling hiss.
    fn whiz(&mut self, side: f32) -> Vec<[f32; 2]> {
        let len = 0.24;
        let mut b = Self::buf(len);
        let snap = Synth::n_wave(0.0003);
        Synth::mix(&mut b, &snap, 0.03, 1.2);
        let mut bp = Biquad::default();
        for i in 0..b.len() {
            let t = Self::t(i);
            let x = t / len;
            if i % 32 == 0 {
                bp.set(Kind::Band, 6500.0 - 4000.0 * x, 2.5);
            }
            let env = (-((x - 0.2) * (x - 0.2)) * 40.0).exp();
            b[i] += bp.run(self.noise()) * env * 2.5;
        }
        Synth::norm(&mut b, 0.8);
        let mut out = pan(&b, side);
        normalize(&mut out, 0.8);
        out
    }

    /// Jet flying over: a roar sweeping down in pitch as it passes.
    fn jet(&mut self) -> Vec<[f32; 2]> {
        let len = 3.0;
        let n = (len * SR) as usize;
        let mut l = vec![0.0f32; n];
        let mut r = vec![0.0f32; n];
        let (mut bp, mut lp) = (Biquad::default(), Biquad::default());
        let mut ph = 0.0f32;
        for i in 0..n {
            let t = Self::t(i);
            let x = t / len;
            if i % 32 == 0 {
                let f = 2200.0 - 1600.0 / (1.0 + (-(x - 0.45) * 14.0).exp());
                bp.set(Kind::Band, f, 1.2);
                lp.set(Kind::Low, f * 0.4, 0.7);
            }
            let env = (-((x - 0.45) * (x - 0.45)) * 9.0).exp();
            let nse = self.noise();
            ph += (3200.0 - 1800.0 * x) / SR;
            let v = (bp.run(nse) * 1.5 + lp.run(nse) * 2.0 + (ph * TAU).sin() * 0.04) * env;
            // passes from left to right
            let p = (x * 2.0 - 1.0).clamp(-1.0, 1.0);
            let a = (p + 1.0) * PI / 4.0;
            l[i] = v * a.cos();
            r[i] = v * a.sin();
        }
        let mut out: Vec<[f32; 2]> = l.into_iter().zip(r).map(|(a, b)| [a, b]).collect();
        fade_out(&mut out, 0.3);
        normalize(&mut out, 0.85);
        out
    }

    /// Helicopter rotor: one seamless second (ten blade slaps) to loop.
    /// Drone rotors: a buzzing whine with a slow beat, one seamless second to loop.
    fn drone(&mut self) -> Vec<[f32; 2]> {
        let period = 1.0;
        let xf = 0.15;
        let n = ((period + xf) * SR) as usize;
        let mut b = vec![0.0f32; n];
        let mut bp = Biquad::new(Kind::Band, 900.0, 1.2);
        for (i, v) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            // two motors slightly apart give the beat; 180 and 186 Hz loop in one second
            let saw = |f: f32| 2.0 * (t * f).fract() - 1.0;
            let buzz = saw(180.0) + saw(186.0) * 0.8 + saw(360.0) * 0.3;
            *v = buzz * 0.35 + bp.run(self.noise()) * 0.5;
        }
        let mut lp = Biquad::new(Kind::Low, 2600.0, 0.7);
        for v in b.iter_mut() {
            *v = lp.run(*v);
        }
        let p = (period * SR) as usize;
        let x = n - p;
        let mut out: Vec<f32> = b[..p].to_vec();
        for i in 0..x {
            let k = i as f32 / x as f32;
            out[i] = b[i] * k + b[p + i] * (1.0 - k);
        }
        Synth::norm(&mut out, 0.7);
        mono(&out)
    }

    /// An engine as a one-second loop: cylinders firing `f` times a second (a whole
    /// number, so the loop closes), a rumble under it and more rasp when it pulls.
    fn engine(&mut self, f: f32, rough: f32) -> Vec<[f32; 2]> {
        let n = SR as usize;
        let mut b = vec![0.0f32; 2 * n];
        let mut body = Biquad::new(Kind::Low, 160.0 + 260.0 * rough, 0.8);
        let mut rasp = Biquad::new(Kind::Band, 900.0 + 900.0 * rough, 1.1);
        let mut lp = Biquad::new(Kind::Low, 2400.0, 0.7);
        for (i, v) in b.iter_mut().enumerate() {
            let t = (i % n) as f32 / SR;
            // a pulse per firing, each a little different in strength
            let ph = (t * f).fract();
            let k = (t * f) as usize % 4;
            let strength = [1.0, 0.86, 0.95, 0.8][k];
            let pulse = (-ph * 9.0).exp() * strength;
            let nse = self.noise();
            let tone = (t * f * TAU).sin() * 0.6 + (t * f * 2.0 * TAU).sin() * 0.35;
            *v = lp.run(
                body.run(pulse * 2.0 + nse * 0.15) * 1.4
                    + tone * 0.3
                    + rasp.run(nse) * pulse * rough * 1.2,
            );
        }
        // the second pass, filters warmed up: the loop starts where it ends
        let mut out = b[n..].to_vec();
        Synth::norm(&mut out, 0.75);
        mono(&out)
    }

    fn heli(&mut self) -> Vec<[f32; 2]> {
        let period = 1.0;
        let xf = 0.15;
        let n = ((period + xf) * SR) as usize;
        let mut b = vec![0.0f32; n];
        let (mut wash, mut slap_f) = (
            Biquad::new(Kind::Low, 500.0, 0.7),
            Biquad::new(Kind::Band, 260.0, 1.2),
        );
        let mut slap_lp = Biquad::new(Kind::Low, 1200.0, 0.7);
        let mut ph = 0.0f32;
        for (i, v) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            let blade = ((t * 10.0).fract() * TAU).cos().max(0.0).powi(10);
            let nse = self.noise();
            ph += 1900.0 / SR;
            *v = wash.run(nse) * 1.2
                + slap_lp.run(slap_f.run(nse) * blade) * 7.0
                + (t * 20.0 * TAU).sin() * blade * 0.6
                + (ph * TAU).sin() * 0.015;
        }
        // crossfade the tail into the head so the loop has no seam
        let p = (period * SR) as usize;
        let x = n - p;
        let mut out: Vec<f32> = b[..p].to_vec();
        for i in 0..x {
            let k = i as f32 / x as f32;
            out[i] = b[i] * k + b[p + i] * (1.0 - k);
        }
        Synth::norm(&mut out, 0.8);
        mono(&out)
    }

    /// Steady rain: hiss with a little patter in it, different in each ear, two seconds
    /// that loop without a seam.
    fn rain(&mut self) -> Vec<[f32; 2]> {
        let period = 2.0;
        let xf = 0.25;
        let n = ((period + xf) * SR) as usize;
        let p = (period * SR) as usize;
        let side = |me: &mut Self| {
            let mut b = vec![0.0f32; n];
            let mut hiss = Biquad::new(Kind::High, 1800.0, 0.7);
            let mut top = Biquad::new(Kind::Low, 7000.0, 0.7);
            let mut patter = Biquad::new(Kind::Band, 900.0, 0.9);
            let mut drop = 0.0f32;
            for v in b.iter_mut() {
                let nse = me.noise();
                if me.rng.chance(0.004) {
                    drop = me.rng.range(0.5, 1.0);
                }
                drop *= 0.994;
                *v = top.run(hiss.run(nse)) * 0.6 + patter.run(nse) * drop * 0.8;
            }
            let x = n - p;
            let mut out: Vec<f32> = b[..p].to_vec();
            for i in 0..x {
                let k = i as f32 / x as f32;
                out[i] = b[i] * k + b[p + i] * (1.0 - k);
            }
            Synth::norm(&mut out, 0.6);
            out
        };
        let (l, r) = (side(self), side(self));
        l.iter().zip(&r).map(|(a, b)| [*a, *b]).collect()
    }

    /// A short "ugh": a buzzing voice through the formants of an "uh" vowel.
    fn hurt(&mut self) -> Vec<f32> {
        let len = 0.28;
        let f0 = self.rng.range(130.0, 175.0);
        let mut b = Self::buf(len);
        let mut ph = 0.0f32;
        let mut formants = [
            Biquad::new(Kind::Band, 650.0, 5.0),
            Biquad::new(Kind::Band, 1150.0, 7.0),
            Biquad::new(Kind::Band, 2450.0, 9.0),
        ];
        for (i, s) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            let f = f0 * (1.15 - 0.35 * t / len) * (1.0 + 0.01 * (t * 33.0).sin());
            ph = (ph + f / SR).fract();
            let glottal = 1.0 - 2.0 * ph + self.noise() * 0.15;
            let mut y = 0.0;
            for (k, fm) in formants.iter_mut().enumerate() {
                y += fm.run(glottal) * [1.0, 0.6, 0.25][k];
            }
            let env = (t / 0.025).min(1.0) * (-(t / len) * 3.0).exp();
            *s = y * env;
        }
        let thud = Synth::drop(0.1, 140.0, 70.0, 0.02, 0.03);
        let g = Synth::peak(&b) * 0.6;
        Synth::mix(&mut b, &thud, 0.0, g);
        Synth::norm(&mut b, 0.7);
        b
    }

    fn tone(len: f32, f: f32, decay: f32, harmonics: &[f32]) -> Vec<f32> {
        (0..(len * SR) as usize)
            .map(|i| {
                let t = Self::t(i);
                let mut v = 0.0;
                for (k, h) in harmonics.iter().enumerate() {
                    v += (t * f * (k + 1) as f32 * TAU).sin() * h;
                }
                v * (t / 0.004).min(1.0) * (-t * decay).exp()
            })
            .collect()
    }

    fn chord(notes: &[(f32, f32)], len: f32, decay: f32) -> Vec<f32> {
        let mut out = Vec::new();
        for (at, f) in notes {
            let t = Synth::tone(len, *f, decay, &[1.0, 0.35, 0.12]);
            Synth::mix(&mut out, &t, *at, 0.6);
        }
        Synth::norm(&mut out, 0.6);
        out
    }

    /// A bell-like ring (headshot ding).
    fn bell(f: f32, len: f32) -> Vec<f32> {
        let partials = [(1.0f32, 1.0f32), (2.76, 0.5), (5.4, 0.25), (8.93, 0.12)];
        (0..(len * SR) as usize)
            .map(|i| {
                let t = Self::t(i);
                let mut v = 0.0;
                for (k, (p, g)) in partials.iter().enumerate() {
                    v += (t * f * p * TAU).sin() * g * (-t * (7.0 + k as f32 * 6.0)).exp();
                }
                v * (t / 0.001).min(1.0)
            })
            .collect()
    }

    fn make(&mut self, s: Sfx, variant: usize) -> Vec<[f32; 2]> {
        let mut out = self.make_raw(s, variant);
        // the rotor loop must stay seamless; everything else ends softly
        if s != Sfx::Heli && s != Sfx::Drone {
            // drop the silent end, then fade what is left
            let end = out
                .iter()
                .rposition(|v| v[0].abs().max(v[1].abs()) > 2e-4)
                .map_or(1, |i| i + 1);
            out.truncate(end.max(1));
            fade_out(&mut out, 0.01);
        }
        out
    }

    fn make_raw(&mut self, s: Sfx, variant: usize) -> Vec<[f32; 2]> {
        let room = |b: Vec<f32>, rng: &mut Rng| {
            let mut o = spatialize(&b, NEAR, 0.25, rng);
            fade_out(&mut o, 0.1);
            o
        };
        match s {
            // recordings only (never asked for: they have no synthesized variants)
            Sfx::MagOutOf(_) => self.make(Sfx::MagOut, variant),
            Sfx::StepSoft => self.make(Sfx::Step, variant),
            Sfx::Door | Sfx::DoorShut => self.make(Sfx::Click, variant),
            Sfx::MagInOf(_) => self.make(Sfx::MagIn, variant),
            Sfx::BoltOf(_) => self.make(Sfx::Bolt, variant),
            Sfx::Gun(WeaponId::Rpg7) => self.launch(),
            Sfx::Gun(WeaponId::Crossbow) => self.twang(0.7),
            Sfx::Gun(WeaponId::Bow) => self.twang(1.0),
            Sfx::Gun(w) => self.shot(gun_params(w)),
            Sfx::HeliGun => self.shot(Gun {
                body: 200.0,
                boom: 0.08,
                punch_gain: 1.2,
                mech: 0.0,
                wet: 1.3,
                ..gun_params(WeaponId::Negev)
            }),
            Sfx::Far => self.far(),
            Sfx::Explosion => self.explosion(),
            Sfx::Smoke => {
                // a dull pop, then gas rushing out for a couple of seconds
                let mut b = self.burst(0.08, &[(Kind::Low, 700.0, 0.7)], 0.001, 0.02);
                Synth::norm(&mut b, 0.8);
                let len = 2.6;
                let mut hiss = Self::buf(len);
                let mut bp = Biquad::default();
                for i in 0..hiss.len() {
                    let t = Self::t(i);
                    if i % 64 == 0 {
                        bp.set(Kind::Band, 3800.0 - 1600.0 * (t / len), 0.9);
                    }
                    let env = (t / 0.12).min(1.0) * (1.0 - t / len).powf(1.5);
                    hiss[i] = bp.run(self.noise()) * env;
                }
                Synth::norm(&mut hiss, 0.5);
                Synth::mix(&mut b, &hiss, 0.03, 1.0);
                let mut o = spatialize(&b, NEAR, 0.3, &mut self.rng);
                normalize(&mut o, 0.75);
                o
            }
            Sfx::Hit => {
                // hitmarker: a crisp, dry tick
                let mut b = self.clack(&[3200.0, 5100.0], 30.0, 0.05, 1.0);
                let tick = self.burst(0.01, &[(Kind::High, 6000.0, 0.7)], 0.0001, 0.002);
                Synth::mix(&mut b, &tick, 0.0, 1.5);
                Synth::norm(&mut b, 0.6);
                mono(&b)
            }
            Sfx::HitHead => {
                let mut b = Synth::bell(1650.0, 0.45);
                let tick = self.clack(&[3200.0, 5100.0], 30.0, 0.05, 1.0);
                Synth::mix(&mut b, &tick, 0.0, 0.6);
                Synth::norm(&mut b, 0.7);
                mono(&b)
            }
            Sfx::Kill => {
                let mut b = Synth::chord(&[(0.0, 1320.0), (0.07, 1760.0)], 0.25, 12.0);
                let thud = Synth::drop(0.12, 160.0, 70.0, 0.02, 0.04);
                Synth::mix(&mut b, &thud, 0.0, 0.5);
                Synth::norm(&mut b, 0.6);
                mono(&b)
            }
            Sfx::Step => {
                let b = self.step();
                mono(&b)
            }
            Sfx::Crumble => {
                // a low rumble under it all
                let mut b = self.burst(
                    1.6,
                    &[(Kind::Low, 260.0, 0.7), (Kind::Low, 380.0, 0.7)],
                    0.02,
                    0.6,
                );
                Synth::norm(&mut b, 0.9);
                let thump = Synth::drop(0.35, 90.0, 40.0, 0.05, 0.12);
                Synth::mix(&mut b, &thump, 0.0, 0.9);
                // grit pouring and chunks hitting chunks
                let grit = self.burst(1.3, &[(Kind::Band, 1400.0, 0.7)], 0.03, 0.45);
                Synth::mix(&mut b, &grit, 0.05, 0.35);
                for _ in 0..34 {
                    let at = self.rng.f32().powf(1.6) * 1.3;
                    let f = self.rng.range(500.0, 1900.0);
                    let c = self.clack(&[f, f * 1.5, f * 2.3], 9.0, 0.07, 1.0);
                    Synth::mix(&mut b, &c, at, self.rng.range(0.06, 0.2));
                    if self.rng.chance(0.4) {
                        let t = Synth::drop(0.12, 140.0, 70.0, 0.02, 0.04);
                        Synth::mix(&mut b, &t, at, 0.3);
                    }
                }
                Synth::norm(&mut b, 0.9);
                let mut o = spatialize(&b, VALLEY, 1.0, &mut self.rng);
                fade_out(&mut o, 0.3);
                normalize(&mut o, 0.85);
                o
            }
            Sfx::TreeFall => {
                // wood giving way: a creak of rising crackles...
                let mut b = Self::buf(0.1);
                for k in 0..26 {
                    let at = k as f32 * 0.028 + self.rng.range(0.0, 0.02);
                    let f = self.rng.range(300.0, 900.0) + k as f32 * 25.0;
                    let c = self.clack(&[f, f * 2.1], 22.0, 0.05, 1.0);
                    Synth::mix(&mut b, &c, at, 0.1 + k as f32 * 0.008);
                }
                // ...a sharp crack, leaves rushing through the air...
                let crack = self.burst(0.08, &[(Kind::Band, 2200.0, 1.2)], 0.0005, 0.02);
                Synth::mix(&mut b, &crack, 0.75, 1.0);
                let rush = self.burst(1.0, &[(Kind::Band, 3000.0, 0.6)], 0.4, 0.3);
                Synth::mix(&mut b, &rush, 0.8, 0.35);
                // ...and the trunk hitting the ground
                let thud = Synth::drop(0.45, 85.0, 38.0, 0.05, 0.14);
                Synth::mix(&mut b, &thud, 1.55, 1.3);
                let dirt = self.burst(0.4, &[(Kind::Band, 900.0, 0.8)], 0.002, 0.1);
                Synth::mix(&mut b, &dirt, 1.56, 0.5);
                Synth::norm(&mut b, 0.9);
                let mut o = spatialize(&b, VALLEY, 0.8, &mut self.rng);
                fade_out(&mut o, 0.2);
                normalize(&mut o, 0.8);
                o
            }
            Sfx::Land => {
                let mut b = Synth::drop(0.2, 120.0, 55.0, 0.03, 0.06);
                let grit = self.burst(0.15, &[(Kind::Band, 1800.0, 0.8)], 0.002, 0.03);
                Synth::mix(&mut b, &grit, 0.0, 1.2);
                for k in 0..3 {
                    let f = self.rng.range(1200.0, 2600.0);
                    let c = self.clack(&[f, f * 1.6], 15.0, 0.05, 1.0);
                    Synth::mix(&mut b, &c, 0.01 + k as f32 * 0.03, 0.12);
                }
                Synth::norm(&mut b, 0.75);
                room(b, &mut self.rng)
            }
            Sfx::MagOut => {
                // release click, then the magazine sliding out
                let mut b = self.clack(&[2300.0, 3700.0, 5200.0], 30.0, 0.04, 1.0);
                let mut slide = Self::buf(0.12);
                let mut bp = Biquad::default();
                for i in 0..slide.len() {
                    let t = Self::t(i);
                    if i % 32 == 0 {
                        bp.set(Kind::Band, 900.0 + t * 18000.0, 2.0);
                    }
                    slide[i] = bp.run(self.noise()) * (t / 0.01).min(1.0) * (-t / 0.05).exp();
                }
                Synth::mix(&mut b, &slide, 0.025, 2.5);
                Synth::norm(&mut b, 0.6);
                room(b, &mut self.rng)
            }
            Sfx::MagIn => {
                // magazine seated: a dull thud and a sharp latch
                let mut b = self.burst(0.08, &[(Kind::Low, 500.0, 0.8)], 0.001, 0.02);
                Synth::norm(&mut b, 0.8);
                let latch = self.clack(&[1250.0, 2450.0, 3950.0, 5600.0], 35.0, 0.08, 1.0);
                Synth::mix(&mut b, &latch, 0.012, 0.9);
                Synth::norm(&mut b, 0.7);
                room(b, &mut self.rng)
            }
            Sfx::Bolt => {
                // charging handle back, spring, slamming forward
                let mut b = self.clack(&[1800.0, 3100.0, 4700.0], 25.0, 0.05, 0.7);
                let spring = self.burst(0.08, &[(Kind::Band, 3500.0, 3.0)], 0.005, 0.03);
                Synth::mix(&mut b, &spring, 0.03, 0.4);
                let fwd = self.clack(&[1300.0, 2200.0, 3600.0, 5200.0], 14.0, 0.08, 1.0);
                Synth::mix(&mut b, &fwd, 0.1, 1.0);
                let thud = self.burst(0.05, &[(Kind::Low, 400.0, 0.8)], 0.001, 0.012);
                Synth::mix(&mut b, &thud, 0.1, 1.5);
                Synth::norm(&mut b, 0.75);
                room(b, &mut self.rng)
            }
            Sfx::Pump => {
                let mut b = self.burst(0.09, &[(Kind::Band, 1500.0, 1.5)], 0.01, 0.04);
                Synth::norm(&mut b, 0.5);
                let back = self.clack(&[900.0, 1700.0, 2900.0], 20.0, 0.07, 1.0);
                let fwd = self.clack(&[1100.0, 2100.0, 3400.0], 22.0, 0.08, 1.0);
                Synth::mix(&mut b, &back, 0.06, 0.9);
                let slide = self.burst(0.08, &[(Kind::Band, 1800.0, 1.5)], 0.01, 0.04);
                Synth::mix(&mut b, &slide, 0.14, 0.5);
                Synth::mix(&mut b, &fwd, 0.21, 1.0);
                Synth::norm(&mut b, 0.8);
                room(b, &mut self.rng)
            }
            Sfx::ShellIn => {
                let mut b = self.clack(&[1500.0, 2700.0], 18.0, 0.05, 1.0);
                let push = self.burst(0.06, &[(Kind::Band, 1200.0, 1.2)], 0.004, 0.02);
                Synth::mix(&mut b, &push, 0.01, 0.8);
                Synth::norm(&mut b, 0.5);
                room(b, &mut self.rng)
            }
            Sfx::Shell => {
                // brass bouncing: bright high-Q rings, each bounce softer
                let mut b = Vec::new();
                let mut at = 0.0;
                for k in 0..3 {
                    let f = self.rng.range(4000.0, 5200.0);
                    let c = self.clack(&[f, f * 1.83, f * 2.41], 90.0, 0.18, 1.0);
                    Synth::mix(&mut b, &c, at, 0.7 / (1.0 + k as f32 * 1.2));
                    at += self.rng.range(0.07, 0.12) / (1.0 + k as f32 * 0.3);
                }
                Synth::norm(&mut b, 0.35);
                room(b, &mut self.rng)
            }
            Sfx::Raise => {
                let mut b = self.burst(0.14, &[(Kind::Band, 2500.0, 0.8)], 0.03, 0.05);
                Synth::norm(&mut b, 0.4);
                let c = self.clack(&[1900.0, 3300.0], 25.0, 0.05, 1.0);
                Synth::mix(&mut b, &c, 0.09, 0.7);
                Synth::norm(&mut b, 0.5);
                room(b, &mut self.rng)
            }
            Sfx::Empty => {
                let mut b = self.clack(&[2600.0, 4900.0, 7200.0], 40.0, 0.04, 0.6);
                Synth::norm(&mut b, 0.6);
                mono(&b)
            }
            Sfx::Knife | Sfx::KnifeHit => {
                let len = 0.22;
                let mut b = Self::buf(len);
                let mut bp = Biquad::default();
                for i in 0..b.len() {
                    let t = Self::t(i);
                    let x = t / len;
                    if i % 32 == 0 {
                        bp.set(Kind::Band, 1200.0 + 3500.0 * x, 1.8);
                    }
                    let env = (-((x - 0.45) * (x - 0.45)) * 20.0).exp();
                    b[i] = bp.run(self.noise()) * env;
                }
                Synth::norm(&mut b, 0.6);
                if s == Sfx::KnifeHit {
                    let thud = self.burst(0.12, &[(Kind::Low, 500.0, 0.8)], 0.001, 0.03);
                    let mut thud = thud;
                    Synth::norm(&mut thud, 1.0);
                    Synth::mix(&mut b, &thud, 0.09, 0.9);
                    Synth::norm(&mut b, 0.8);
                }
                mono(&b)
            }
            Sfx::Flesh => {
                let mut b = self.burst(0.1, &[(Kind::Low, 650.0, 0.8)], 0.0005, 0.022);
                Synth::norm(&mut b, 1.0);
                let pop = Synth::drop(0.08, 190.0, 70.0, 0.015, 0.025);
                Synth::mix(&mut b, &pop, 0.0, 0.7);
                let crunch = self.burst(0.03, &[(Kind::Band, 2200.0, 1.0)], 0.0002, 0.006);
                Synth::mix(&mut b, &crunch, 0.002, 0.8);
                Synth::norm(&mut b, 0.6);
                mono(&b)
            }
            Sfx::Whiz => self.whiz(if variant.is_multiple_of(2) { -0.7 } else { 0.7 }),
            Sfx::Beep => mono(&Synth::tone(0.12, 1760.0, 14.0, &[0.7, 0.15])),
            Sfx::Jet => self.jet(),
            Sfx::Click => mono(&Synth::tone(0.04, 1100.0, 80.0, &[0.6, 0.2])),
            Sfx::Ping => {
                let mut b = Synth::tone(0.22, 1320.0, 18.0, &[0.5, 0.12]);
                let up = Synth::tone(0.16, 1980.0, 20.0, &[0.45, 0.1]);
                Synth::mix(&mut b, &up, 0.07, 1.0);
                Synth::norm(&mut b, 0.5);
                mono(&b)
            }
            Sfx::Hurt => {
                let b = self.hurt();
                mono(&b)
            }
            Sfx::Heli => self.heli(),
            Sfx::Whistle => {
                // a falling pitch that grows louder, then cuts off before the blast
                let len = 1.4;
                let mut b = Self::buf(len);
                let mut ph = 0.0f32;
                let mut bp = Biquad::new(Kind::Band, 2500.0, 2.0);
                for (i, v) in b.iter_mut().enumerate() {
                    let t = Self::t(i);
                    let x = t / len;
                    let f = 1900.0 - 1100.0 * x + (t * 30.0).sin() * 12.0;
                    ph += f / SR;
                    let env = x.powf(1.6) * (1.0 - ((x - 0.93) / 0.07).clamp(0.0, 1.0));
                    *v = ((ph * TAU).sin() * 0.7 + bp.run(self.noise()) * 0.5) * env;
                }
                Synth::norm(&mut b, 0.7);
                mono(&b)
            }
            Sfx::Drone => self.drone(),
            Sfx::EngineIdle => self.engine(36.0, 0.35),
            Sfx::EngineRun => self.engine(72.0, 1.0),
            Sfx::Rain => self.rain(),
            Sfx::Streak => mono(&Synth::chord(
                &[(0.0, 523.0), (0.09, 659.0), (0.18, 784.0), (0.27, 1046.0)],
                0.3,
                7.0,
            )),
            Sfx::Bomb => mono(&Synth::chord(&[(0.0, 330.0), (0.25, 247.0)], 0.35, 5.0)),
            Sfx::Tick => mono(&Synth::chord(&[(0.0, 880.0)], 0.18, 16.0)),
            Sfx::TickHigh => mono(&Synth::chord(&[(0.0, 1175.0), (0.0, 1760.0)], 0.22, 12.0)),
            Sfx::Siren => {
                // two rising and falling wails, then a low horn under them
                let len = 2.2;
                let mut b = Self::buf(len);
                let mut ph = 0.0f32;
                for (i, v) in b.iter_mut().enumerate() {
                    let t = Self::t(i);
                    let f = 620.0 + 260.0 * (t * PI * 1.8).sin().abs();
                    ph += f / SR;
                    let saw = 2.0 * (ph - ph.floor()) - 1.0;
                    let env = (t / 0.05).min(1.0) * (1.0 - (t / len).powf(3.0));
                    *v = (saw * 0.35 + (ph * TAU).sin() * 0.65) * env;
                }
                let horn = Synth::chord(&[(0.0, 165.0), (0.0, 220.0)], 1.6, 1.5);
                Synth::mix(&mut b, &horn, 0.0, 0.6);
                Synth::norm(&mut b, 0.8);
                mono(&b)
            }
        }
    }
}

/// Number of random variants generated per sound.
fn variants(s: Sfx) -> usize {
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

fn all() -> Vec<Sfx> {
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

/// Decodes a recording (WAV or Ogg Vorbis, mono or stereo) into 44.1 kHz stereo, or says
/// why it cannot. Files are checked here because the audio backend aborts on bad data.
fn decode(data: &[u8]) -> Result<Vec<[f32; 2]>, String> {
    let mut r = audrey::Reader::new(std::io::Cursor::new(data)).map_err(|e| format!("{e:?}"))?;
    let d = r.description();
    let ch = d.channel_count() as usize;
    if !(1..=2).contains(&ch) {
        return Err(format!("{ch} channels (need mono or stereo)"));
    }
    let rate = d.sample_rate() as f32;
    if !(8000.0..=192000.0).contains(&rate) {
        return Err(format!("sample rate {rate}"));
    }
    let mut raw = Vec::new();
    for v in r.samples::<f32>() {
        raw.push(v.map_err(|e| format!("{e:?}"))?);
        if raw.len() > 30 * 192000 * 2 {
            return Err("longer than 30 s".into());
        }
    }
    let frames: Vec<[f32; 2]> = raw
        .chunks_exact(ch)
        .map(|f| if ch == 1 { [f[0], f[0]] } else { [f[0], f[1]] })
        .collect();
    if frames.is_empty() {
        return Err("no audio".into());
    }
    if rate == SR {
        return Ok(frames);
    }
    // linear resampling to 44.1 kHz
    let n = (frames.len() as f32 * SR / rate) as usize;
    Ok((0..n)
        .map(|i| {
            let x = i as f32 * rate / SR;
            let a = frames[(x as usize).min(frames.len() - 1)];
            let b = frames[(x as usize + 1).min(frames.len() - 1)];
            let k = x.fract();
            [a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k]
        })
        .collect())
}

/// Recorded replacements for a sound: `<name>.wav|ogg`, `<name>_2.wav`, ... found in
/// `assets/sounds/` next to the game or in the current directory.
fn recordings(s: Sfx) -> Vec<(String, Vec<u8>)> {
    let mut dirs = vec![std::path::PathBuf::from("assets/sounds")];
    if let Some(d) = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|p| p.to_path_buf()))
    {
        dirs.insert(0, d.join("assets/sounds"));
    }
    let name = s.file_name();
    for dir in dirs {
        let mut found = Vec::new();
        for k in 1..=8 {
            let stem = if k == 1 {
                name.clone()
            } else {
                format!("{name}_{k}")
            };
            for ext in ["ogg", "wav"] {
                let path = dir.join(format!("{stem}.{ext}"));
                if let Ok(data) = std::fs::read(&path) {
                    found.push((path.display().to_string(), data));
                    break;
                }
            }
        }
        if !found.is_empty() {
            return found;
        }
    }
    BUNDLED
        .iter()
        .filter(|(n, _)| *n == name)
        .map(|(n, d)| (format!("{n} (built in)"), d.to_vec()))
        .collect()
}

/// Recordings shipped inside the game (used unless a file in `assets/sounds/` replaces
/// them): (sound name, file contents), variants of a sound in order.
const BUNDLED: &[(&str, &[u8])] = &[
    ("step", include_bytes!("../../../assets/sounds/step.ogg")),
    ("step", include_bytes!("../../../assets/sounds/step_2.ogg")),
    ("step", include_bytes!("../../../assets/sounds/step_3.ogg")),
    ("step", include_bytes!("../../../assets/sounds/step_4.ogg")),
    ("step", include_bytes!("../../../assets/sounds/step_5.ogg")),
    ("step", include_bytes!("../../../assets/sounds/step_6.ogg")),
    (
        "step_soft",
        include_bytes!("../../../assets/sounds/step_soft.ogg"),
    ),
    (
        "step_soft",
        include_bytes!("../../../assets/sounds/step_soft_2.ogg"),
    ),
    (
        "step_soft",
        include_bytes!("../../../assets/sounds/step_soft_3.ogg"),
    ),
    (
        "step_soft",
        include_bytes!("../../../assets/sounds/step_soft_4.ogg"),
    ),
    (
        "step_soft",
        include_bytes!("../../../assets/sounds/step_soft_5.ogg"),
    ),
    (
        "step_soft",
        include_bytes!("../../../assets/sounds/step_soft_6.ogg"),
    ),
    ("door", include_bytes!("../../../assets/sounds/door.ogg")),
    ("door", include_bytes!("../../../assets/sounds/door_2.ogg")),
    ("door", include_bytes!("../../../assets/sounds/door_3.ogg")),
    (
        "door_shut",
        include_bytes!("../../../assets/sounds/door_shut.ogg"),
    ),
    (
        "door_shut",
        include_bytes!("../../../assets/sounds/door_shut_2.ogg"),
    ),
    (
        "door_shut",
        include_bytes!("../../../assets/sounds/door_shut_3.ogg"),
    ),
    (
        "explosion",
        include_bytes!("../../../assets/sounds/explosion.ogg"),
    ),
    ("rain", include_bytes!("../../../assets/sounds/rain.ogg")),
    ("heli", include_bytes!("../../../assets/sounds/heli.ogg")),
    (
        "bolt_ak47",
        include_bytes!("../../../assets/sounds/bolt_ak47.ogg"),
    ),
    (
        "bolt_ak74u",
        include_bytes!("../../../assets/sounds/bolt_ak74u.ogg"),
    ),
    (
        "bolt_m249",
        include_bytes!("../../../assets/sounds/bolt_m249.ogg"),
    ),
    (
        "bolt_m249",
        include_bytes!("../../../assets/sounds/bolt_m249_2.ogg"),
    ),
    (
        "bolt_mp5",
        include_bytes!("../../../assets/sounds/bolt_mp5.ogg"),
    ),
    (
        "bolt_p90",
        include_bytes!("../../../assets/sounds/bolt_p90.ogg"),
    ),
    (
        "bolt_rpd",
        include_bytes!("../../../assets/sounds/bolt_rpd.ogg"),
    ),
    (
        "bolt_rpd",
        include_bytes!("../../../assets/sounds/bolt_rpd_2.ogg"),
    ),
    (
        "bolt_usp",
        include_bytes!("../../../assets/sounds/bolt_usp.ogg"),
    ),
    (
        "gun_ak47",
        include_bytes!("../../../assets/sounds/gun_ak47.wav"),
    ),
    (
        "gun_ak47",
        include_bytes!("../../../assets/sounds/gun_ak47_2.wav"),
    ),
    (
        "gun_ak47",
        include_bytes!("../../../assets/sounds/gun_ak47_3.ogg"),
    ),
    (
        "gun_deagle",
        include_bytes!("../../../assets/sounds/gun_deagle.ogg"),
    ),
    (
        "gun_m249",
        include_bytes!("../../../assets/sounds/gun_m249.ogg"),
    ),
    (
        "gun_m249",
        include_bytes!("../../../assets/sounds/gun_m249_2.ogg"),
    ),
    (
        "gun_m4",
        include_bytes!("../../../assets/sounds/gun_m4.ogg"),
    ),
    (
        "gun_m4",
        include_bytes!("../../../assets/sounds/gun_m4_2.ogg"),
    ),
    (
        "gun_m4",
        include_bytes!("../../../assets/sounds/gun_m4_3.ogg"),
    ),
    (
        "gun_m4",
        include_bytes!("../../../assets/sounds/gun_m4_4.ogg"),
    ),
    (
        "gun_m4",
        include_bytes!("../../../assets/sounds/gun_m4_5.ogg"),
    ),
    (
        "gun_m40",
        include_bytes!("../../../assets/sounds/gun_m40.ogg"),
    ),
    (
        "gun_m40",
        include_bytes!("../../../assets/sounds/gun_m40_2.ogg"),
    ),
    (
        "gun_m40",
        include_bytes!("../../../assets/sounds/gun_m40_3.ogg"),
    ),
    (
        "gun_m9",
        include_bytes!("../../../assets/sounds/gun_m9.ogg"),
    ),
    (
        "gun_mp5",
        include_bytes!("../../../assets/sounds/gun_mp5.ogg"),
    ),
    (
        "gun_mp5",
        include_bytes!("../../../assets/sounds/gun_mp5_2.ogg"),
    ),
    (
        "gun_mp5",
        include_bytes!("../../../assets/sounds/gun_mp5_3.ogg"),
    ),
    (
        "gun_mp5",
        include_bytes!("../../../assets/sounds/gun_mp5_4.ogg"),
    ),
    (
        "gun_mp5",
        include_bytes!("../../../assets/sounds/gun_mp5_5.ogg"),
    ),
    (
        "gun_p90",
        include_bytes!("../../../assets/sounds/gun_p90.ogg"),
    ),
    (
        "gun_p90",
        include_bytes!("../../../assets/sounds/gun_p90_2.ogg"),
    ),
    (
        "gun_p90",
        include_bytes!("../../../assets/sounds/gun_p90_3.ogg"),
    ),
    (
        "gun_r700",
        include_bytes!("../../../assets/sounds/gun_r700.ogg"),
    ),
    (
        "gun_r700",
        include_bytes!("../../../assets/sounds/gun_r700_2.ogg"),
    ),
    (
        "gun_r700",
        include_bytes!("../../../assets/sounds/gun_r700_3.ogg"),
    ),
    (
        "gun_r700",
        include_bytes!("../../../assets/sounds/gun_r700_4.ogg"),
    ),
    (
        "gun_r700",
        include_bytes!("../../../assets/sounds/gun_r700_5.ogg"),
    ),
    (
        "gun_rpd",
        include_bytes!("../../../assets/sounds/gun_rpd.ogg"),
    ),
    (
        "gun_rpd",
        include_bytes!("../../../assets/sounds/gun_rpd_2.ogg"),
    ),
    (
        "gun_rpd",
        include_bytes!("../../../assets/sounds/gun_rpd_3.ogg"),
    ),
    (
        "gun_usp",
        include_bytes!("../../../assets/sounds/gun_usp.ogg"),
    ),
    (
        "mag_in_ak47",
        include_bytes!("../../../assets/sounds/mag_in_ak47.ogg"),
    ),
    (
        "mag_in_ak74u",
        include_bytes!("../../../assets/sounds/mag_in_ak74u.ogg"),
    ),
    (
        "mag_in_m249",
        include_bytes!("../../../assets/sounds/mag_in_m249.ogg"),
    ),
    (
        "mag_in_mp5",
        include_bytes!("../../../assets/sounds/mag_in_mp5.ogg"),
    ),
    (
        "mag_in_mp5",
        include_bytes!("../../../assets/sounds/mag_in_mp5_2.ogg"),
    ),
    (
        "mag_in_p90",
        include_bytes!("../../../assets/sounds/mag_in_p90.ogg"),
    ),
    (
        "mag_in_rpd",
        include_bytes!("../../../assets/sounds/mag_in_rpd.ogg"),
    ),
    (
        "mag_out_m249",
        include_bytes!("../../../assets/sounds/mag_out_m249.ogg"),
    ),
    (
        "mag_out_mp5",
        include_bytes!("../../../assets/sounds/mag_out_mp5.ogg"),
    ),
    (
        "mag_out_p90",
        include_bytes!("../../../assets/sounds/mag_out_p90.ogg"),
    ),
    (
        "mag_out_rpd",
        include_bytes!("../../../assets/sounds/mag_out_rpd.ogg"),
    ),
];

/// Synthesizes sounds in parallel; each job gets its own seeded generator, so the result
/// does not depend on the number of threads.
fn synthesize(jobs: &[(Sfx, usize)]) -> Vec<(Sfx, Vec<[f32; 2]>)> {
    let threads = std::thread::available_parallelism()
        .map_or(2, |n| n.get())
        .clamp(1, 8);
    // round robin, so the few heavy sounds (explosions) land on different threads
    std::thread::scope(|sc| {
        let handles: Vec<_> = (0..threads)
            .map(|t| {
                sc.spawn(move || {
                    jobs.iter()
                        .enumerate()
                        .skip(t)
                        .step_by(threads)
                        .map(|(j, &(k, i))| {
                            let mut s = Synth {
                                rng: Rng::new(7 + j as u64 * 7919),
                            };
                            (k, s.make(k, i))
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().unwrap_or_default())
            .collect()
    })
}

pub struct Audio {
    sounds: HashMap<Sfx, Vec<Sound>>,
    /// The same, heard from each of `DIRS` (sounds out in the world).
    dirs: HashMap<Sfx, Vec<Vec<Sound>>>,
    /// Looped sounds in the world (rotors, engines): ahead, then each of `DIRS`; all of
    /// them play together and their volumes pan the sound round the listener.
    loops: HashMap<Sfx, Vec<Sound>>,
    loop_on: RefCell<HashMap<Sfx, bool>>,
    /// Where the listener looks (which way is left).
    listener_yaw: std::cell::Cell<f32>,
    /// Master volume (the settings slider changes it while the game runs).
    volume: std::cell::Cell<f32>,
    last: RefCell<HashMap<Sfx, f64>>,
    rain_on: RefCell<bool>,
    pending: RefCell<Vec<(f64, Sfx, f32)>>,
    rng: RefCell<Rng>,
}

impl Audio {
    pub async fn load(volume: f32, enabled: bool) -> Audio {
        let mut sounds: HashMap<Sfx, Vec<Sound>> = HashMap::new();
        let mut dirs: HashMap<Sfx, Vec<Vec<Sound>>> = HashMap::new();
        let mut loops: HashMap<Sfx, Vec<Sound>> = HashMap::new();
        if enabled {
            // recordings first; everything else is synthesized on all CPU cores
            let mut jobs = Vec::new();
            for k in all() {
                let mut v = Vec::new();
                for (file, data) in recordings(k) {
                    match decode(&data) {
                        Ok(pcm) => match load_sound_from_bytes(&wav(&pcm)).await {
                            Ok(snd) => {
                                v.push(snd);
                                if k.placed() {
                                    add_dirs(&mut dirs, k, &pcm).await;
                                }
                                if k.looped_in_world() && !loops.contains_key(&k) {
                                    loops.insert(k, loop_dirs(&pcm).await);
                                }
                            }
                            Err(e) => eprintln!("sound {file}: {e:?}"),
                        },
                        Err(e) => eprintln!("sound {file}: skipped, {e}"),
                    }
                }
                if v.is_empty() {
                    jobs.extend((0..variants(k)).map(|i| (k, i)));
                }
                sounds.insert(k, v);
            }
            for (k, pcm) in synthesize(&jobs) {
                if let Ok(snd) = load_sound_from_bytes(&wav(&pcm)).await {
                    sounds.entry(k).or_default().push(snd);
                    if k.placed() {
                        add_dirs(&mut dirs, k, &pcm).await;
                    }
                    if k.looped_in_world() && !loops.contains_key(&k) {
                        loops.insert(k, loop_dirs(&pcm).await);
                    }
                }
            }
        }
        Audio {
            sounds,
            dirs,
            loops,
            loop_on: RefCell::new(HashMap::new()),
            listener_yaw: std::cell::Cell::new(0.0),
            volume: std::cell::Cell::new(volume),
            last: RefCell::new(HashMap::new()),
            rain_on: RefCell::new(false),
            pending: RefCell::new(Vec::new()),
            rng: RefCell::new(Rng::from_time()),
        }
    }

    /// Master volume 0..1, applied at once (the next sounds play at it).
    pub fn set_volume(&self, v: f32) {
        self.volume.set(v.clamp(0.0, 1.0));
    }

    pub fn volume(&self) -> f32 {
        self.volume.get()
    }

    /// Which way the listener faces (call once a frame, before placed sounds).
    pub fn set_listener_yaw(&self, yaw: f32) {
        self.listener_yaw.set(yaw);
    }

    /// Where `pos` is around the listener, in degrees: 0 ahead, 90 to the right, -90 to
    /// the left, ±180 behind (0 when it is right here).
    fn azimuth(&self, pos: Vec3, listener: Vec3) -> f32 {
        let d = pos - listener;
        let flat = vec3(d.x, 0.0, d.z);
        if flat.length_squared() < 0.25 {
            return 0.0;
        }
        let (fwd, right) = toon_shared::yaw_basis(self.listener_yaw.get());
        flat.dot(right).atan2(flat.dot(fwd)).to_degrees()
    }

    /// A sound from a place: the version made for the nearest direction.
    fn play_placed(&self, s: Sfx, vol: f32, az: f32) {
        let mut best = (az.abs(), None);
        for (k, d) in DIRS.iter().enumerate() {
            let diff = (az - d + 540.0).rem_euclid(360.0) - 180.0;
            if diff.abs() < best.0 {
                best = (diff.abs(), Some(k));
            }
        }
        let Some(k) = best.1 else {
            return self.play(s, vol);
        };
        match self.dirs.get(&s).map(|v| &v[k]).filter(|l| !l.is_empty()) {
            Some(list) => {
                let i = (self.rng.borrow_mut().f32() * list.len() as f32) as usize;
                self.play_one(s, &list[i.min(list.len() - 1)], vol);
            }
            None => self.play(s, vol),
        }
    }

    fn play_one(&self, s: Sfx, snd: &Sound, vol: f32) {
        if vol <= 0.01 || self.volume.get() <= 0.0 {
            return;
        }
        let now = crate::clock::now();
        {
            let mut last = self.last.borrow_mut();
            let l = last.entry(s).or_insert(-1.0);
            if now - *l < 0.02 {
                return;
            }
            *l = now;
        }
        play_sound(
            snd,
            PlaySoundParams {
                looped: false,
                volume: (vol * self.volume.get()).min(1.0),
            },
        );
    }

    pub fn play(&self, s: Sfx, vol: f32) {
        if vol <= 0.01 || self.volume.get() <= 0.0 {
            return;
        }
        let now = crate::clock::now();
        {
            let mut last = self.last.borrow_mut();
            let l = last.entry(s).or_insert(-1.0);
            if now - *l < 0.02 {
                return;
            }
            *l = now;
        }
        let s = match (self.sounds.get(&s), s.fallback()) {
            (Some(l), Some(f)) if l.is_empty() => f,
            (None, Some(f)) => f,
            _ => s,
        };
        if let Some(list) = self.sounds.get(&s) {
            if list.is_empty() {
                return;
            }
            let i = (self.rng.borrow_mut().f32() * list.len() as f32) as usize;
            play_sound(
                &list[i.min(list.len() - 1)],
                PlaySoundParams {
                    looped: false,
                    volume: (vol * self.volume.get()).min(1.0),
                },
            );
        }
    }

    /// Plays a sound after a delay (shell casings, reload cues).
    pub fn play_later(&self, s: Sfx, delay: f32, vol: f32) {
        self.pending
            .borrow_mut()
            .push((crate::clock::now() + delay as f64, s, vol));
    }

    /// Fires delayed sounds that are due. Call once per frame.
    pub fn tick(&self) {
        let now = crate::clock::now();
        let due: Vec<(Sfx, f32)> = {
            let mut p = self.pending.borrow_mut();
            let due = p
                .iter()
                .filter(|e| e.0 <= now)
                .map(|e| (e.1, e.2))
                .collect();
            p.retain(|e| e.0 > now);
            due
        };
        for (s, v) in due {
            self.play(s, v);
        }
    }

    /// A sound from a place in the world: quieter with distance and gone beyond `range`
    /// (a footstep 22 m off is not heard at all), from the direction it comes from.
    pub fn play_at(&self, s: Sfx, pos: Vec3, listener: Vec3, vol: f32, range: f32) {
        let g = falloff(pos.distance(listener), range);
        if g <= 0.0 {
            return;
        }
        self.play_placed(s, vol * g, self.azimuth(pos, listener));
    }

    /// Gunfire with distance: close shots are sharp, far ones turn into a muffled boom
    /// rolling through the streets; both from where the shot was.
    pub fn play_shot(&self, s: Sfx, pos: Vec3, listener: Vec3, vol: f32) {
        let d = pos.distance(listener);
        let az = self.azimuth(pos, listener);
        let near = falloff(d, 60.0);
        if near > 0.0 {
            self.play_placed(s, vol * near, az);
        }
        if d > 15.0 {
            let far = ((d - 15.0) / 20.0).clamp(0.0, 1.0) * falloff(d, 260.0).sqrt();
            let loud = if s.loud() { 0.7 } else { 0.45 };
            self.play_placed(Sfx::Far, vol * far * loud, az);
        }
    }

    /// A looped sound at `at` (None: stop it): every direction of it plays at once and
    /// the two nearest the sound's direction are heard, so it moves smoothly round the
    /// listener; quieter with distance, silent beyond `range`.
    pub fn loop_at(&self, s: Sfx, at: Option<(Vec3, Vec3)>, range: f32, gain: f32) {
        let Some(list) = self.loops.get(&s).filter(|l| l.len() == DIRS.len() + 1) else {
            return;
        };
        let mut on = self.loop_on.borrow_mut();
        let playing = on.entry(s).or_insert(false);
        let level = at.map_or(0.0, |(p, l)| falloff(p.distance(l), range));
        if level <= 0.0 {
            if *playing {
                for snd in list {
                    stop_sound(snd);
                }
                *playing = false;
            }
            return;
        }
        let (pos, listener) = at.expect("checked above");
        let w = pan_weights(self.azimuth(pos, listener));
        let master = (level * gain * self.volume.get()).min(1.0);
        for (k, snd) in list.iter().enumerate() {
            let v = master * w[k];
            if *playing {
                set_sound_volume(snd, v);
            } else {
                play_sound(
                    snd,
                    PlaySoundParams {
                        looped: true,
                        volume: v,
                    },
                );
            }
        }
        *playing = true;
    }

    /// Helicopter rotors where the nearest helicopter is (None = stop).
    pub fn heli(&self, at: Option<(Vec3, Vec3)>) {
        self.loop_at(Sfx::Heli, at, 120.0, 0.7);
    }

    /// Drone buzz of the nearest drone (None = stop).
    pub fn drone(&self, at: Option<(Vec3, Vec3)>) {
        self.loop_at(Sfx::Drone, at, 50.0, 0.55);
    }

    /// The nearest engine running (None = none): `pull` 0 ticking over .. 1 flat out.
    pub fn engine(&self, at: Option<(Vec3, Vec3)>, pull: f32) {
        let pull = pull.clamp(0.0, 1.0);
        let (idle, run) = ((1.0 - pull).sqrt(), pull.sqrt());
        self.loop_at(
            Sfx::EngineIdle,
            at.filter(|_| idle > 0.05),
            70.0,
            0.45 * idle,
        );
        self.loop_at(Sfx::EngineRun, at.filter(|_| run > 0.05), 90.0, 0.55 * run);
    }

    /// Rain all around: 1 out in it, less under a roof (None = it does not rain).
    pub fn rain(&self, level: Option<f32>) {
        self.looped(
            Sfx::Rain,
            &self.rain_on,
            level.map(|l| 1.0 - l.clamp(0.0, 1.0)),
            1.0,
            0.5,
        );
    }

    fn looped(&self, s: Sfx, on: &RefCell<bool>, dist: Option<f32>, range: f32, gain: f32) {
        let Some(snd) = self.sounds.get(&s).and_then(|v| v.first()) else {
            return;
        };
        let mut on = on.borrow_mut();
        match dist {
            Some(d) => {
                let v = ((1.0 - d / range).clamp(0.05, 1.0) * self.volume.get() * gain).min(1.0);
                if !*on {
                    play_sound(
                        snd,
                        PlaySoundParams {
                            looped: true,
                            volume: v,
                        },
                    );
                    *on = true;
                } else {
                    set_sound_volume(snd, v);
                }
            }
            None => {
                if *on {
                    stop_sound(snd);
                    *on = false;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every sound comes out of the synthesizer the same as when the numbers in
    /// `golden/synth.txt` were taken (a refactor must not change a sample). When a sound
    /// is meant to change, run `GOLDEN_UPDATE=1 cargo test --release -p toon_client
    /// synthesis_is_unchanged` and commit the file.
    #[test]
    fn synthesis_is_unchanged() {
        // FNV-1a over the samples' bits: the same on every machine
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut s = Synth { rng: Rng::new(11) };
        for k in all() {
            for i in 0..variants(k) {
                for frame in s.make(k, i) {
                    for x in frame {
                        for b in x.to_bits().to_le_bytes() {
                            h ^= b as u64;
                            h = h.wrapping_mul(0x0100_0000_01b3);
                        }
                    }
                }
            }
        }
        let got = format!("{h:016x}");
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("golden/synth.txt");
        if std::env::var_os("GOLDEN_UPDATE").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, format!("{got}\n")).unwrap();
            return;
        }
        let want = std::fs::read_to_string(&path).unwrap_or_default();
        assert_eq!(
            want.trim(),
            got,
            "a synthesized sound changed (see the comment above)"
        );
    }

    #[test]
    fn every_sound_synthesizes_cleanly() {
        let t0 = std::time::Instant::now();
        let mut s = Synth { rng: Rng::new(7) };
        let mut total = 0;
        for k in all() {
            for i in 0..variants(k) {
                let b = s.make(k, i);
                assert!(!b.is_empty(), "{k:?} is empty");
                assert!(
                    b.iter().all(|v| v[0].is_finite() && v[1].is_finite()),
                    "{k:?} has NaN"
                );
                let peak = b
                    .iter()
                    .fold(0.0f32, |m, v| m.max(v[0].abs()).max(v[1].abs()));
                assert!(peak > 0.05 && peak <= 1.0, "{k:?} peak {peak}");
                // no click at the end
                let last = b.last().unwrap();
                assert!(
                    k == Sfx::Heli
                        || k == Sfx::Drone
                        || (last[0].abs() < 0.05 && last[1].abs() < 0.05),
                    "{k:?} ends abruptly"
                );
                total += b.len();
            }
        }
        let secs = total as f32 / SR;
        println!("{secs:.1} s of stereo audio in {:?}", t0.elapsed());
        let limit = if cfg!(debug_assertions) { 30.0 } else { 3.0 };
        assert!(t0.elapsed().as_secs_f32() < limit);
    }

    #[test]
    fn far_sounds_are_not_heard() {
        assert_eq!(falloff(0.0, 20.0), 1.0);
        assert!(falloff(5.0, 20.0) > falloff(15.0, 20.0));
        assert_eq!(falloff(20.0, 20.0), 0.0, "nothing at the edge of the range");
        assert_eq!(falloff(300.0, 20.0), 0.0, "nor beyond it");
    }

    #[test]
    fn loops_pan_between_the_nearest_directions() {
        let total = |w: &[f32]| w.iter().map(|v| v * v).sum::<f32>();
        for az in [
            -180.0f32, -160.0, -100.0, -45.0, -10.0, 0.0, 30.0, 90.0, 150.0, 179.0,
        ] {
            let w = pan_weights(az);
            assert!((total(&w) - 1.0).abs() < 1e-3, "{az}: {w:?}");
            assert!(w.iter().filter(|v| **v > 1e-3).count() <= 2, "{az}: {w:?}");
        }
        // straight ahead: the sound as it is; to the right: the 90° version
        assert!(pan_weights(0.0)[0] > 0.99);
        let right = 1 + DIRS.iter().position(|d| *d == 90.0).unwrap();
        assert!(pan_weights(90.0)[right] > 0.99);
        // right behind: the two back versions
        let w = pan_weights(180.0);
        let back: f32 = DIRS
            .iter()
            .enumerate()
            .filter(|(_, d)| d.abs() > 120.0)
            .map(|(k, _)| w[k + 1])
            .sum();
        assert!(back > 1.3, "{w:?}");
    }

    #[test]
    fn a_sound_on_the_right_reaches_the_left_ear_later_and_quieter() {
        // a click
        let mut pcm = vec![[0.0f32; 2]; 400];
        pcm[10] = [1.0, 1.0];
        let out = binaural(&pcm, 90.0, false);
        let peak = |ch: usize| {
            out.iter()
                .enumerate()
                .max_by(|a, b| a.1[ch].abs().total_cmp(&b.1[ch].abs()))
                .unwrap()
        };
        let (ti_r, r) = peak(1);
        let (ti_l, l) = peak(0);
        assert_eq!(ti_r, 10, "the near ear at once");
        assert!(ti_l > ti_r + 20, "the far ear ~0.6 ms later: {ti_l}");
        assert!(l[0].abs() < r[1].abs() * 0.6, "and quieter");
        // and the mirror image from the left
        let out = binaural(&pcm, -90.0, false);
        assert!(out[10][0].abs() > 0.9 && out[10][1].abs() < 0.1);
    }

    #[test]
    fn engine_loops() {
        let mut s = Synth { rng: Rng::new(5) };
        for k in [Sfx::EngineIdle, Sfx::EngineRun] {
            let b = s.make(k, 0);
            assert_eq!(b.len(), SR as usize, "one second");
            let jump = (b[0][0] - b[b.len() - 1][0]).abs();
            let typical = b
                .windows(2)
                .map(|w| (w[1][0] - w[0][0]).abs())
                .fold(0.0, f32::max);
            assert!(jump <= typical * 1.5, "{k:?}: seam {jump} vs {typical}");
            // as a loop in a direction it still closes
            let d = binaural(&b, 90.0, true);
            let jump = (d[0][0] - d[d.len() - 1][0]).abs();
            assert!(jump <= typical * 1.5, "{k:?} at 90°: seam {jump}");
        }
    }

    #[test]
    fn heli_loop_has_no_seam() {
        let mut s = Synth { rng: Rng::new(3) };
        let b = s.make(Sfx::Heli, 0);
        let jump = (b[0][0] - b[b.len() - 1][0]).abs();
        let typical = b
            .windows(2)
            .map(|w| (w[1][0] - w[0][0]).abs())
            .fold(0.0, f32::max);
        assert!(jump <= typical * 1.5, "seam {jump} vs {typical}");
    }

    #[test]
    fn recordings_are_checked() {
        let mut s = Synth { rng: Rng::new(1) };
        let b = s.make(Sfx::Click, 0);
        let back = decode(&wav(&b)).unwrap();
        assert_eq!(back.len(), b.len());
        assert!(decode(b"not audio").is_err());
        let mut broken = wav(&b);
        broken.truncate(30);
        assert!(decode(&broken).is_err());
    }

    #[test]
    fn parallel_synthesis_is_quick() {
        let jobs: Vec<(Sfx, usize)> = all()
            .into_iter()
            .flat_map(|k| (0..variants(k)).map(move |i| (k, i)))
            .collect();
        let t0 = std::time::Instant::now();
        let out = synthesize(&jobs);
        println!("parallel synthesis: {:?}", t0.elapsed());
        assert_eq!(out.len(), jobs.len());
    }

    /// Every recording shipped in the game decodes, is not silent and does not clip.
    #[test]
    fn bundled_recordings_are_sound() {
        for (name, data) in BUNDLED {
            let pcm = decode(data).unwrap_or_else(|e| panic!("{name}: {e}"));
            let peak = pcm
                .iter()
                .fold(0.0f32, |m, v| m.max(v[0].abs()).max(v[1].abs()));
            assert!(pcm.len() > 2000, "{name}: {} samples", pcm.len());
            assert!(peak > 0.05 && peak < 1.3, "{name}: peak {peak}");
        }
        // the new ones are found under their names
        for s in [
            Sfx::Step,
            Sfx::StepSoft,
            Sfx::Door,
            Sfx::DoorShut,
            Sfx::Rain,
            Sfx::Heli,
        ] {
            assert!(
                BUNDLED.iter().any(|(n, _)| *n == s.file_name()),
                "{} has no recording",
                s.file_name()
            );
        }
        assert_eq!(Sfx::StepSoft.fallback(), Some(Sfx::Step));
    }

    #[test]
    fn file_names() {
        assert_eq!(Sfx::Gun(WeaponId::Ak12).file_name(), "gun_ak47");
        assert_eq!(Sfx::MagIn.file_name(), "mag_in");
        assert_eq!(Sfx::Explosion.file_name(), "explosion");
        assert_eq!(Sfx::MagIn.of(WeaponId::Bizon).file_name(), "mag_in_mp5");
        assert_eq!(Sfx::Bolt.of(WeaponId::Glock18).file_name(), "bolt_usp");
        assert_eq!(Sfx::Pump.of(WeaponId::Ksg), Sfx::Pump);
    }

    #[test]
    fn bundled_recordings_decode() {
        // every recording shipped in the game must decode (the audio backend aborts on
        // bad data), and each weapon in the pack has its shots
        for (name, data) in BUNDLED {
            let pcm = decode(data).unwrap_or_else(|e| panic!("{name}: {e}"));
            assert!(pcm.len() > 4410, "{name} too short");
            assert!(pcm.iter().all(|f| f[0].is_finite() && f[1].is_finite()));
        }
        for stem in [
            "gun_m4", "gun_mp5", "gun_p90", "gun_m249", "gun_rpd", "gun_r700", "gun_m40",
        ] {
            assert!(
                BUNDLED.iter().filter(|(n, _)| *n == stem).count() >= 2,
                "{stem}"
            );
        }
        for stem in [
            "mag_in_ak47",
            "bolt_mp5",
            "mag_out_p90",
            "bolt_m249",
            "mag_in_rpd",
            "bolt_usp",
        ] {
            assert!(BUNDLED.iter().any(|(n, _)| *n == stem), "{stem}");
        }
        assert_eq!(recordings(Sfx::Gun(WeaponId::Ak12)).len(), 3);
    }
}

#[cfg(test)]
mod dump {
    use super::*;

    /// `TW_SOUND_DUMP=<dir> cargo test dump_sounds` writes every sound as a WAV to listen to.
    #[test]
    fn dump_sounds() {
        let Ok(dir) = std::env::var("TW_SOUND_DUMP") else {
            return;
        };
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Synth { rng: Rng::new(7) };
        for k in all() {
            for i in 0..variants(k) {
                let b = s.make(k, i);
                let name = if i == 0 {
                    k.file_name()
                } else {
                    format!("{}_{}", k.file_name(), i + 1)
                };
                std::fs::write(format!("{dir}/{name}.wav"), wav(&b)).unwrap();
            }
        }
    }
}
