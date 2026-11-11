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
    /// Rain falling all around (looped).
    Rain,
    /// A weapon's own reload sounds, where recordings exist (`mag_out_ak47`, …); the
    /// generic `MagOut`/`MagIn`/`Bolt` play for the rest.
    MagOutOf(WeaponId),
    MagInOf(WeaponId),
    BoltOf(WeaponId),
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
                | Sfx::Flesh
                | Sfx::Knife
                | Sfx::Beep
                | Sfx::Smoke
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

fn weapon_stem(w: WeaponId) -> String {
    format!("{w:?}").to_lowercase()
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

/// The left- and right-side versions of a sound in the world (one of each per sound,
/// to keep memory down on weak machines): the far ear hears it quieter and duller.
async fn add_sides(sides: &mut HashMap<Sfx, [Vec<Sound>; 2]>, k: Sfx, pcm: &[[f32; 2]]) {
    let e = sides.entry(k).or_default();
    if !e[0].is_empty() {
        return;
    }
    for (ch, list) in e.iter_mut().enumerate() {
        // ch 0: from the left, so the right ear (1) is the far one
        let far = 1 - ch;
        let mut lp = Biquad::new(Kind::Low, 2500.0, 0.7);
        let side: Vec<[f32; 2]> = pcm
            .iter()
            .map(|v| {
                let mut o = *v;
                o[far] = lp.run(v[far]) * 0.4;
                o
            })
            .collect();
        if let Ok(snd) = load_sound_from_bytes(&wav(&side)).await {
            list.push(snd);
        }
    }
}

fn mono(b: &[f32]) -> Vec<[f32; 2]> {
    b.iter().map(|v| [*v, *v]).collect()
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
        M4 => g(0.8, 320.0, 7500.0, 0.006, 0.045, 70.0, 0.7, 0.25, 1.0),
        Ak47 => g(0.8, 240.0, 6000.0, 0.007, 0.060, 58.0, 0.9, 0.3, 1.0),
        G3 => g(0.9, 210.0, 6500.0, 0.008, 0.070, 52.0, 1.0, 0.3, 1.1),
        Mp5 => g(0.25, 420.0, 7000.0, 0.005, 0.035, 85.0, 0.5, 0.3, 0.8),
        P90 => g(0.6, 520.0, 9000.0, 0.004, 0.030, 95.0, 0.45, 0.25, 0.8),
        Ak74u => g(0.7, 360.0, 8500.0, 0.006, 0.050, 75.0, 0.7, 0.3, 1.0),
        M249 => g(0.8, 290.0, 7000.0, 0.007, 0.055, 62.0, 0.85, 0.35, 1.1),
        Rpd => g(0.85, 230.0, 6000.0, 0.008, 0.065, 55.0, 0.95, 0.35, 1.1),
        W1200 => g(0.0, 170.0, 4800.0, 0.010, 0.100, 45.0, 1.3, 0.0, 1.2),
        M1014 => g(0.0, 185.0, 5200.0, 0.009, 0.090, 48.0, 1.2, 0.2, 1.2),
        M40 => g(1.0, 190.0, 6500.0, 0.010, 0.110, 46.0, 1.3, 0.0, 1.4),
        R700 => g(1.0, 175.0, 6000.0, 0.011, 0.120, 44.0, 1.35, 0.0, 1.4),
        M9 => g(0.15, 600.0, 8000.0, 0.004, 0.025, 110.0, 0.45, 0.3, 0.7),
        Usp => g(0.0, 460.0, 7000.0, 0.004, 0.030, 95.0, 0.55, 0.3, 0.7),
        Deagle => g(0.6, 280.0, 7000.0, 0.008, 0.070, 60.0, 1.1, 0.35, 1.0),
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

    /// Gunfire heard from far away: no crack, the highs lost on the way, mostly echo.
    fn far(&mut self) -> Vec<[f32; 2]> {
        let mut dry = self.shot_dry(gun_params(WeaponId::Ak47));
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
            Sfx::MagInOf(_) => self.make(Sfx::MagIn, variant),
            Sfx::BoltOf(_) => self.make(Sfx::Bolt, variant),
            Sfx::Gun(w) => self.shot(gun_params(w)),
            Sfx::HeliGun => self.shot(Gun {
                body: 200.0,
                boom: 0.08,
                punch_gain: 1.2,
                mech: 0.0,
                wet: 1.3,
                ..gun_params(WeaponId::M249)
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
            Sfx::Rain => self.rain(),
            Sfx::Streak => mono(&Synth::chord(
                &[(0.0, 523.0), (0.09, 659.0), (0.18, 784.0), (0.27, 1046.0)],
                0.3,
                7.0,
            )),
            Sfx::Bomb => mono(&Synth::chord(&[(0.0, 330.0), (0.25, 247.0)], 0.35, 5.0)),
        }
    }
}

/// Number of random variants generated per sound.
fn variants(s: Sfx) -> usize {
    match s {
        Sfx::Gun(_) | Sfx::Far | Sfx::Explosion | Sfx::Hit | Sfx::Hurt => 2,
        Sfx::Step | Sfx::Whiz | Sfx::Shell | Sfx::Flesh => 4,
        // recordings only; without one the generic sound plays
        Sfx::MagOutOf(_) | Sfx::MagInOf(_) | Sfx::BoltOf(_) => 0,
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
        Sfx::Rain,
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
    /// The same, heard from the left and from the right (sounds out in the world).
    sides: HashMap<Sfx, [Vec<Sound>; 2]>,
    /// Where the listener looks (which way is left).
    listener_yaw: std::cell::Cell<f32>,
    /// Master volume (the settings slider changes it while the game runs).
    volume: std::cell::Cell<f32>,
    last: RefCell<HashMap<Sfx, f64>>,
    heli_on: RefCell<bool>,
    drone_on: RefCell<bool>,
    rain_on: RefCell<bool>,
    pending: RefCell<Vec<(f64, Sfx, f32)>>,
    rng: RefCell<Rng>,
}

impl Audio {
    pub async fn load(volume: f32, enabled: bool) -> Audio {
        let mut sounds: HashMap<Sfx, Vec<Sound>> = HashMap::new();
        let mut sides: HashMap<Sfx, [Vec<Sound>; 2]> = HashMap::new();
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
                                    add_sides(&mut sides, k, &pcm).await;
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
                        add_sides(&mut sides, k, &pcm).await;
                    }
                }
            }
        }
        Audio {
            sounds,
            sides,
            listener_yaw: std::cell::Cell::new(0.0),
            volume: std::cell::Cell::new(volume),
            last: RefCell::new(HashMap::new()),
            heli_on: RefCell::new(false),
            drone_on: RefCell::new(false),
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

    /// -1 well to the left .. 1 well to the right, as heard from `listener`.
    fn side_of(&self, pos: Vec3, listener: Vec3) -> f32 {
        let d = pos - listener;
        let flat = vec3(d.x, 0.0, d.z);
        if flat.length_squared() < 1.0 {
            return 0.0;
        }
        let (_, right) = toon_shared::yaw_basis(self.listener_yaw.get());
        flat.normalize().dot(right)
    }

    /// Sounds from behind are a little quieter than those in front.
    fn behind(&self, pos: Vec3, listener: Vec3) -> f32 {
        let d = pos - listener;
        let flat = vec3(d.x, 0.0, d.z);
        if flat.length_squared() < 1.0 {
            return 1.0;
        }
        let (fwd, _) = toon_shared::yaw_basis(self.listener_yaw.get());
        let back = (-flat.normalize().dot(fwd)).max(0.0);
        1.0 - 0.3 * back
    }

    /// A sound from a place: the left or right version when it is off to one side.
    fn play_placed(&self, s: Sfx, vol: f32, side: f32) {
        let k = if side < -0.35 {
            0
        } else if side > 0.35 {
            1
        } else {
            return self.play(s, vol);
        };
        match self.sides.get(&s).map(|v| &v[k]).filter(|l| !l.is_empty()) {
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
        let now = get_time();
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
        let now = get_time();
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
            .push((get_time() + delay as f64, s, vol));
    }

    /// Fires delayed sounds that are due. Call once per frame.
    pub fn tick(&self) {
        let now = get_time();
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

    /// Distance-attenuated sound.
    pub fn play_at(&self, s: Sfx, pos: Vec3, listener: Vec3, vol: f32, range: f32) {
        let d = pos.distance(listener);
        let k = (1.0 - d / range).clamp(0.0, 1.0);
        let side = self.side_of(pos, listener);
        let vol = vol * self.behind(pos, listener);
        self.play_placed(s, vol * (0.08 + 0.92 * k * k), side);
    }

    /// Gunfire with distance: close shots are sharp, far ones turn into a muffled boom
    /// rolling through the streets.
    pub fn play_shot(&self, s: Sfx, pos: Vec3, listener: Vec3, vol: f32) {
        let d = pos.distance(listener);
        let side = self.side_of(pos, listener);
        let vol = vol * self.behind(pos, listener);
        let near = (1.0 - d / 60.0).clamp(0.0, 1.0);
        if near > 0.0 {
            self.play_placed(s, vol * near * near, side);
        }
        if d > 15.0 {
            let far = ((d - 15.0) / 20.0).clamp(0.0, 1.0) * (1.0 - (d - 140.0).max(0.0) / 120.0);
            let loud = if s.loud() { 0.7 } else { 0.45 };
            self.play_placed(Sfx::Far, vol * far.max(0.0) * loud, side);
        }
    }

    /// Helicopter rotor loop with distance-based volume (None = stop).
    pub fn heli(&self, dist: Option<f32>) {
        self.looped(Sfx::Heli, &self.heli_on, dist, 90.0, 0.7);
    }

    /// Drone buzz of the nearest drone (None = stop).
    pub fn drone(&self, dist: Option<f32>) {
        self.looped(Sfx::Drone, &self.drone_on, dist, 45.0, 0.55);
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

    #[test]
    fn file_names() {
        assert_eq!(Sfx::Gun(WeaponId::Ak47).file_name(), "gun_ak47");
        assert_eq!(Sfx::MagIn.file_name(), "mag_in");
        assert_eq!(Sfx::Explosion.file_name(), "explosion");
        assert_eq!(Sfx::MagIn.of(WeaponId::Mp5).file_name(), "mag_in_mp5");
        assert_eq!(Sfx::Bolt.of(WeaponId::Usp).file_name(), "bolt_usp");
        assert_eq!(Sfx::Pump.of(WeaponId::W1200), Sfx::Pump);
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
        assert_eq!(recordings(Sfx::Gun(WeaponId::Ak47)).len(), 3);
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
