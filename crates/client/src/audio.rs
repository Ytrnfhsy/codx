//! Procedurally synthesized sound effects (no asset files needed).
//!
//! Gunshots are layered like real recordings: a sharp transient, a filtered noise body,
//! a low "thump", the mechanical clack of the action and an outdoor echo tail. Every
//! frequently heard sound has a few random variants so bursts never sound like a loop.

use macroquad::audio::{
    load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound,
};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::f32::consts::TAU;
use toon_shared::rng::Rng;
use toon_shared::weapons::{WeaponClass, WeaponId};

const RATE: u32 = 44100;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    Rifle,
    Smg,
    Lmg,
    Shotgun,
    Sniper,
    Pistol,
    /// Muffled gunfire far away.
    Far,
    Explosion,
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
}

impl Sfx {
    pub fn for_weapon(w: WeaponId) -> Sfx {
        match w.def().class {
            WeaponClass::Rifle => Sfx::Rifle,
            WeaponClass::Smg => Sfx::Smg,
            WeaponClass::Lmg => Sfx::Lmg,
            WeaponClass::Shotgun => Sfx::Shotgun,
            WeaponClass::Sniper => Sfx::Sniper,
            WeaponClass::Pistol => Sfx::Pistol,
        }
    }
}

/// Reload sound cues: (fraction of the reload, sound).
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

fn wav(samples: &[f32]) -> Vec<u8> {
    let data_len = (samples.len() * 2) as u32;
    let mut v = Vec::with_capacity(44 + data_len as usize);
    v.extend_from_slice(b"RIFF");
    v.extend_from_slice(&(36 + data_len).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes()); // PCM
    v.extend_from_slice(&1u16.to_le_bytes()); // mono
    v.extend_from_slice(&RATE.to_le_bytes());
    v.extend_from_slice(&(RATE * 2).to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&data_len.to_le_bytes());
    for s in samples {
        let i = (s.clamp(-1.0, 1.0) * 32000.0) as i16;
        v.extend_from_slice(&i.to_le_bytes());
    }
    v
}

/// One-pole low-pass coefficient for a cutoff in Hz.
fn lp_k(hz: f32) -> f32 {
    (1.0 - (-TAU * hz / RATE as f32).exp()).clamp(0.0, 1.0)
}

/// Parameters of a gunshot.
#[derive(Clone, Copy)]
struct Gun {
    /// Body decay (1/s) — bigger is shorter.
    decay: f32,
    /// Body brightness (Hz).
    bright: f32,
    /// Thump frequency (Hz).
    thump: f32,
    thump_gain: f32,
    /// Mechanical clack after the shot.
    mech: f32,
    /// Echo tail amount.
    tail: f32,
    gain: f32,
}

struct Synth {
    rng: Rng,
}

impl Synth {
    fn buf(secs: f32) -> Vec<f32> {
        vec![0.0; (secs * RATE as f32) as usize]
    }
    fn noise(&mut self) -> f32 {
        self.rng.f32() * 2.0 - 1.0
    }
    fn t(i: usize) -> f32 {
        i as f32 / RATE as f32
    }
    fn mix(a: &mut Vec<f32>, b: &[f32], at: f32, gain: f32) {
        let off = (at * RATE as f32) as usize;
        if a.len() < off + b.len() {
            a.resize(off + b.len(), 0.0);
        }
        for (i, v) in b.iter().enumerate() {
            a[off + i] += v * gain;
        }
    }
    fn normalize(b: &mut [f32], peak: f32) {
        let m = b.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        if m > 1e-6 {
            for v in b {
                *v *= peak / m;
            }
        }
    }
    /// Soft clipping adds punch without harsh distortion.
    fn saturate(b: &mut [f32], drive: f32) {
        for v in b {
            *v = (*v * drive).tanh();
        }
    }
    /// Short metallic hit: a few inharmonic partials.
    fn metal(&mut self, len: f32, base: f32, decay: f32) -> Vec<f32> {
        let mut b = Self::buf(len);
        let partials = [1.0f32, 2.76, 5.40, 8.93];
        let jit: Vec<f32> = partials
            .iter()
            .map(|_| self.rng.range(0.97, 1.03))
            .collect();
        for (i, s) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            let mut v = 0.0;
            for (k, p) in partials.iter().enumerate() {
                let f = base * p * jit[k];
                v += (t * f * TAU).sin() * (-t * decay * (1.0 + k as f32 * 0.6)).exp()
                    / (1.0 + k as f32);
            }
            let click = if t < 0.0015 { self.noise() * 0.6 } else { 0.0 };
            *s = (v + click) * (t / 0.0008).min(1.0);
        }
        Synth::normalize(&mut b, 0.9);
        b
    }
    fn shot(&mut self, g: Gun) -> Vec<f32> {
        let len = 0.25 + g.tail * 1.1;
        let mut b = Self::buf(len);
        let (mut lp1, mut lp2, mut hp) = (0.0f32, 0.0f32, 0.0f32);
        let k1 = lp_k(g.bright);
        let k2 = lp_k(g.bright * 0.5);
        let mut ph = 0.0f32;
        for (i, s) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            let n = self.noise();
            // transient: broadband crack in the first couple of ms
            let crack = if t < 0.0025 {
                n * (1.0 - t / 0.0025) * 1.4
            } else {
                0.0
            };
            // body: band-limited noise
            lp1 += (n - lp1) * k1;
            lp2 += (lp1 - lp2) * k2;
            hp += (lp2 - hp) * lp_k(180.0);
            let body = (lp2 - hp) * 2.8 * (-t * g.decay).exp();
            // thump: pitch drops quickly like a pressure wave
            let f = g.thump * (1.0 + 1.5 * (-t * 40.0).exp());
            ph += f / RATE as f32;
            let thump = (ph * TAU).sin() * (-t * g.decay * 0.55).exp() * g.thump_gain;
            *s = crack + body + thump;
        }
        // the action cycling
        if g.mech > 0.0 {
            let base = self.rng.range(1900.0, 2400.0);
            let m = self.metal(0.05, base, 70.0);
            Synth::mix(&mut b, &m, 0.018 + self.rng.range(0.0, 0.01), g.mech);
        }
        // outdoor slap-back echoes and a soft reverb tail
        if g.tail > 0.0 {
            let src: Vec<f32> = b[..(0.12 * RATE as f32) as usize].to_vec();
            let mut lp = 0.0f32;
            let echo: Vec<f32> = src
                .iter()
                .map(|v| {
                    lp += (v - lp) * lp_k(900.0);
                    lp
                })
                .collect();
            for (at, gain) in [(0.085f32, 0.32f32), (0.19, 0.2), (0.33, 0.12), (0.52, 0.07)] {
                let at = at * self.rng.range(0.9, 1.1);
                Synth::mix(&mut b, &echo, at, gain * g.tail);
            }
            let mut lp = 0.0f32;
            let start = (0.03 * RATE as f32) as usize;
            for i in start..b.len() {
                let t = Self::t(i - start);
                lp += (self.noise() - lp) * lp_k(700.0);
                b[i] += lp * 0.5 * g.tail * (-t * 4.5).exp() * (t / 0.05).min(1.0);
            }
        }
        Synth::saturate(&mut b, 1.3);
        Synth::normalize(&mut b, g.gain);
        b
    }
    /// Gunfire heard from far away: no crack, just a low boom and a long tail.
    fn far(&mut self) -> Vec<f32> {
        let mut b = Self::buf(1.2);
        let (mut lp, mut lp2) = (0.0f32, 0.0f32);
        for (i, s) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            lp += (self.noise() - lp) * lp_k(500.0);
            lp2 += (lp - lp2) * lp_k(300.0);
            let env = (t / 0.004).min(1.0) * ((-t * 9.0).exp() + 0.25 * (-t * 3.0).exp());
            *s = lp2 * env * 5.0 + (t * 70.0 * TAU).sin() * (-t * 14.0).exp() * 0.5;
        }
        Synth::normalize(&mut b, 0.8);
        b
    }
    fn explosion(&mut self) -> Vec<f32> {
        let mut b = Self::buf(2.4);
        let (mut lp, mut lp2) = (0.0f32, 0.0f32);
        let mut ph = 0.0f32;
        for (i, s) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            let env = if t < 0.012 {
                t / 0.012
            } else {
                (-(t - 0.012) * 2.4).exp()
            };
            let n = self.noise();
            lp += (n - lp) * lp_k(1400.0 * (-t * 2.0).exp() + 120.0);
            lp2 += (lp - lp2) * lp_k(600.0);
            ph += (48.0 - t * 14.0).max(25.0) / RATE as f32;
            let rumble = (ph * TAU).sin() * (-t * 1.6).exp();
            let crack = if t < 0.004 { n * 1.5 } else { 0.0 };
            *s = lp2 * 4.0 * env + rumble * 0.9 + crack;
        }
        // debris rattling down
        for _ in 0..14 {
            let at = self.rng.range(0.25, 1.4);
            let f = self.rng.range(900.0, 2600.0);
            let m = self.metal(0.04, f, 90.0);
            Synth::mix(&mut b, &m, at, self.rng.range(0.03, 0.09));
        }
        Synth::saturate(&mut b, 1.5);
        Synth::normalize(&mut b, 0.95);
        b
    }
    fn tone(&mut self, len: f32, f0: f32, f1: f32, decay: f32, square: bool) -> Vec<f32> {
        let mut b = Self::buf(len);
        let mut ph = 0.0f32;
        for (i, s) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            let f = f0 + (f1 - f0) * (t / len);
            ph += f / RATE as f32;
            let w = (ph * TAU).sin();
            let w = if square { w.signum() * 0.5 } else { w };
            let attack = (t / 0.003).min(1.0);
            *s = w * (-t * decay).exp() * attack * 0.6;
        }
        b
    }
    fn noise_burst(&mut self, len: f32, cutoff: f32, decay: f32, gain: f32) -> Vec<f32> {
        let mut b = Self::buf(len);
        let mut lp = 0.0;
        let k = lp_k(cutoff);
        for (i, s) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            lp += (self.noise() - lp) * k;
            *s = lp * (-t * decay).exp() * (t / 0.002).min(1.0) * gain;
        }
        b
    }
    fn step(&mut self) -> Vec<f32> {
        let f = self.rng.range(700.0, 1100.0);
        let mut b = self.noise_burst(0.1, f, 40.0, 2.2);
        let grit = self.noise_burst(0.05, 4000.0, 70.0, 0.25);
        Synth::mix(&mut b, &grit, 0.01, 1.0);
        Synth::normalize(&mut b, 0.7);
        b
    }
    fn whoosh(&mut self, len: f32, peak: f32, cutoff: f32) -> Vec<f32> {
        let mut b = Self::buf(len);
        let mut lp = 0.0;
        let k = lp_k(cutoff);
        for (i, s) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            let x = t / len;
            let env = (-((x - peak) * (x - peak)) * 18.0).exp();
            lp += (self.noise() - lp) * k;
            *s = lp * env * 2.0;
        }
        Synth::normalize(&mut b, 0.8);
        b
    }
    /// Bullet fly-by: a quick swelling hiss with a doppler drop.
    fn whiz(&mut self) -> Vec<f32> {
        let len = 0.22;
        let mut b = Self::buf(len);
        let (mut lp, mut hp) = (0.0f32, 0.0f32);
        let mut ph = 0.0f32;
        for (i, s) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            let x = t / len;
            let env = (-((x - 0.3) * (x - 0.3)) * 30.0).exp();
            let n = self.noise();
            lp += (n - lp) * lp_k(5000.0 - x * 3000.0);
            hp += (lp - hp) * lp_k(1200.0);
            ph += (2400.0 - x * 1500.0) / RATE as f32;
            let crack = if (0.055..0.058).contains(&t) {
                n * 2.0
            } else {
                0.0
            };
            *s = ((lp - hp) * 3.0 + (ph * TAU).sin() * 0.25) * env + crack;
        }
        Synth::normalize(&mut b, 0.8);
        b
    }
    fn heli(&mut self) -> Vec<f32> {
        let mut b = Self::buf(0.6);
        let mut lp = 0.0;
        for (i, s) in b.iter_mut().enumerate() {
            let t = Self::t(i);
            let blade = ((t * 10.0).fract() * TAU).cos().max(0.0).powi(6);
            lp += (self.noise() - lp) * lp_k(400.0);
            *s = lp * (0.25 + blade * 1.5) * 2.0 + (t * 70.0 * TAU).sin() * 0.15;
        }
        Synth::normalize(&mut b, 0.8);
        b
    }
    fn chord(&mut self, notes: &[(f32, f32)], len: f32, decay: f32, square: bool) -> Vec<f32> {
        let mut out = Vec::new();
        for (at, f) in notes {
            let t = self.tone(len, *f, *f, decay, square);
            Synth::mix(&mut out, &t, *at, 0.6);
        }
        Synth::normalize(&mut out, 0.7);
        out
    }

    fn make(&mut self, s: Sfx) -> Vec<f32> {
        let g = |decay, bright, thump, thump_gain, mech, tail, gain| Gun {
            decay,
            bright,
            thump,
            thump_gain,
            mech,
            tail,
            gain,
        };
        match s {
            Sfx::Rifle => self.shot(g(22.0, 5200.0, 95.0, 0.9, 0.25, 0.7, 0.9)),
            Sfx::Smg => self.shot(g(32.0, 6500.0, 140.0, 0.6, 0.3, 0.45, 0.78)),
            Sfx::Lmg => self.shot(g(18.0, 4200.0, 80.0, 1.1, 0.3, 0.8, 0.95)),
            Sfx::Shotgun => self.shot(g(11.0, 3200.0, 62.0, 1.4, 0.0, 0.9, 1.0)),
            Sfx::Sniper => self.shot(g(9.0, 3800.0, 55.0, 1.5, 0.0, 1.0, 1.0)),
            Sfx::Pistol => self.shot(g(38.0, 7000.0, 170.0, 0.5, 0.35, 0.35, 0.7)),
            Sfx::Far => self.far(),
            Sfx::Explosion => self.explosion(),
            Sfx::Hit => {
                let mut b = self.metal(0.05, 3100.0, 90.0);
                let tick = self.noise_burst(0.01, 9000.0, 300.0, 1.0);
                Synth::mix(&mut b, &tick, 0.0, 0.8);
                Synth::normalize(&mut b, 0.6);
                b
            }
            Sfx::HitHead => {
                let mut b = self.metal(0.35, 1650.0, 9.0);
                let tick = self.metal(0.05, 3100.0, 90.0);
                Synth::mix(&mut b, &tick, 0.0, 0.7);
                Synth::normalize(&mut b, 0.7);
                b
            }
            Sfx::Kill => {
                let mut b = self.chord(&[(0.0, 1320.0), (0.06, 1760.0)], 0.2, 14.0, false);
                let thud = self.noise_burst(0.12, 400.0, 30.0, 1.0);
                Synth::mix(&mut b, &thud, 0.0, 0.6);
                Synth::normalize(&mut b, 0.65);
                b
            }
            Sfx::Step => self.step(),
            Sfx::Land => {
                let mut b = self.noise_burst(0.18, 450.0, 22.0, 2.0);
                let clank = self.metal(0.06, 900.0, 50.0);
                Synth::mix(&mut b, &clank, 0.01, 0.15);
                Synth::normalize(&mut b, 0.75);
                b
            }
            Sfx::MagOut => {
                let mut b = self.metal(0.07, 1300.0, 55.0);
                let slide = self.noise_burst(0.08, 2500.0, 35.0, 0.6);
                Synth::mix(&mut b, &slide, 0.02, 1.0);
                Synth::normalize(&mut b, 0.6);
                b
            }
            Sfx::MagIn => {
                let mut b = self.metal(0.06, 1700.0, 60.0);
                let snap = self.metal(0.05, 2600.0, 90.0);
                Synth::mix(&mut b, &snap, 0.035, 0.9);
                Synth::normalize(&mut b, 0.7);
                b
            }
            Sfx::Bolt => {
                let mut b = self.metal(0.05, 2200.0, 80.0);
                let back = self.metal(0.06, 1500.0, 60.0);
                Synth::mix(&mut b, &back, 0.09, 1.0);
                Synth::normalize(&mut b, 0.7);
                b
            }
            Sfx::Pump => {
                let mut b = self.noise_burst(0.07, 1800.0, 40.0, 1.0);
                let a = self.metal(0.05, 1200.0, 70.0);
                let c = self.metal(0.06, 1600.0, 70.0);
                Synth::mix(&mut b, &a, 0.0, 0.8);
                Synth::mix(&mut b, &c, 0.13, 1.0);
                Synth::normalize(&mut b, 0.75);
                b
            }
            Sfx::ShellIn => {
                let mut b = self.metal(0.05, 1100.0, 70.0);
                let push = self.noise_burst(0.04, 1500.0, 60.0, 0.6);
                Synth::mix(&mut b, &push, 0.01, 1.0);
                Synth::normalize(&mut b, 0.5);
                b
            }
            Sfx::Shell => {
                // brass bouncing: two or three tinkles
                let mut b = Vec::new();
                let mut at = 0.0;
                for k in 0..3 {
                    let f = self.rng.range(3600.0, 4600.0);
                    let m = self.metal(0.08, f, 45.0);
                    Synth::mix(&mut b, &m, at, 0.7 / (1.0 + k as f32));
                    at += self.rng.range(0.06, 0.11);
                }
                Synth::normalize(&mut b, 0.35);
                b
            }
            Sfx::Raise => {
                let mut b = self.noise_burst(0.12, 1800.0, 25.0, 0.8);
                let c = self.metal(0.05, 1900.0, 80.0);
                Synth::mix(&mut b, &c, 0.08, 0.9);
                Synth::normalize(&mut b, 0.5);
                b
            }
            Sfx::Empty => {
                let mut b = self.metal(0.04, 2400.0, 120.0);
                Synth::normalize(&mut b, 0.6);
                b
            }
            Sfx::Knife => self.whoosh(0.25, 0.4, 3000.0),
            Sfx::KnifeHit => {
                let mut b = self.noise_burst(0.14, 600.0, 26.0, 2.0);
                let w = self.whoosh(0.12, 0.2, 3500.0);
                Synth::mix(&mut b, &w, 0.0, 0.5);
                Synth::normalize(&mut b, 0.8);
                b
            }
            Sfx::Flesh => {
                let mut b = self.noise_burst(0.09, 700.0, 45.0, 2.0);
                let pop = self.tone(0.06, 260.0, 120.0, 40.0, false);
                Synth::mix(&mut b, &pop, 0.0, 0.8);
                Synth::normalize(&mut b, 0.6);
                b
            }
            Sfx::Whiz => self.whiz(),
            Sfx::Beep => self.tone(0.12, 1760.0, 1760.0, 12.0, false),
            Sfx::Jet => self.whoosh(2.6, 0.5, 600.0),
            Sfx::Click => self.tone(0.05, 900.0, 600.0, 60.0, false),
            Sfx::Hurt => {
                let mut b = self.tone(0.18, 180.0, 90.0, 18.0, false);
                let thud = self.noise_burst(0.1, 350.0, 30.0, 1.0);
                Synth::mix(&mut b, &thud, 0.0, 0.7);
                Synth::normalize(&mut b, 0.7);
                b
            }
            Sfx::Heli => self.heli(),
            Sfx::Streak => self.chord(
                &[(0.0, 523.0), (0.09, 659.0), (0.18, 784.0), (0.27, 1046.0)],
                0.25,
                8.0,
                true,
            ),
            Sfx::Bomb => self.chord(&[(0.0, 330.0), (0.25, 247.0)], 0.3, 6.0, true),
        }
    }
}

/// Number of random variants generated per sound.
fn variants(s: Sfx) -> usize {
    match s {
        Sfx::Rifle
        | Sfx::Smg
        | Sfx::Lmg
        | Sfx::Pistol
        | Sfx::Step
        | Sfx::Whiz
        | Sfx::Shell
        | Sfx::Flesh => 3,
        Sfx::Shotgun | Sfx::Sniper | Sfx::Far | Sfx::Explosion | Sfx::Hit => 2,
        _ => 1,
    }
}

const ALL: &[Sfx] = &[
    Sfx::Rifle,
    Sfx::Smg,
    Sfx::Lmg,
    Sfx::Shotgun,
    Sfx::Sniper,
    Sfx::Pistol,
    Sfx::Far,
    Sfx::Explosion,
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
];

pub struct Audio {
    sounds: HashMap<Sfx, Vec<Sound>>,
    pub volume: f32,
    last: RefCell<HashMap<Sfx, f64>>,
    heli_on: RefCell<bool>,
    pending: RefCell<Vec<(f64, Sfx, f32)>>,
    rng: RefCell<Rng>,
}

impl Audio {
    pub async fn load(volume: f32, enabled: bool) -> Audio {
        let mut sounds: HashMap<Sfx, Vec<Sound>> = HashMap::new();
        if enabled {
            let mut s = Synth { rng: Rng::new(7) };
            for &k in ALL {
                if sounds.contains_key(&k) {
                    continue;
                }
                let mut v = Vec::new();
                for _ in 0..variants(k) {
                    let samples = s.make(k);
                    if let Ok(snd) = load_sound_from_bytes(&wav(&samples)).await {
                        v.push(snd);
                    }
                }
                sounds.insert(k, v);
            }
        }
        Audio {
            sounds,
            volume,
            last: RefCell::new(HashMap::new()),
            heli_on: RefCell::new(false),
            pending: RefCell::new(Vec::new()),
            rng: RefCell::new(Rng::from_time()),
        }
    }

    pub fn play(&self, s: Sfx, vol: f32) {
        if vol <= 0.01 || self.volume <= 0.0 {
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
        if let Some(list) = self.sounds.get(&s) {
            if list.is_empty() {
                return;
            }
            let i = (self.rng.borrow_mut().f32() * list.len() as f32) as usize;
            play_sound(
                &list[i.min(list.len() - 1)],
                PlaySoundParams {
                    looped: false,
                    volume: (vol * self.volume).min(1.0),
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
        self.play(s, vol * (0.08 + 0.92 * k * k));
    }

    /// Gunfire with distance: close shots are sharp, far ones turn into a muffled boom.
    pub fn play_shot(&self, s: Sfx, pos: Vec3, listener: Vec3, vol: f32) {
        let d = pos.distance(listener);
        let near = (1.0 - d / 60.0).clamp(0.0, 1.0);
        if near > 0.0 {
            self.play(s, vol * near * near);
        }
        let loud = matches!(s, Sfx::Sniper | Sfx::Lmg | Sfx::Shotgun);
        if d > 18.0 {
            let far = ((d - 18.0) / 20.0).clamp(0.0, 1.0) * (1.0 - (d - 140.0).max(0.0) / 120.0);
            self.play(Sfx::Far, vol * far.max(0.0) * if loud { 0.6 } else { 0.4 });
        }
    }

    /// Helicopter rotor loop with distance-based volume (None = stop).
    pub fn heli(&self, dist: Option<f32>) {
        let Some(snd) = self.sounds.get(&Sfx::Heli).and_then(|v| v.first()) else {
            return;
        };
        let mut on = self.heli_on.borrow_mut();
        match dist {
            Some(d) => {
                let v = ((1.0 - d / 90.0).clamp(0.05, 1.0) * self.volume * 0.7).min(1.0);
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
        for &k in ALL {
            for _ in 0..variants(k) {
                let b = s.make(k);
                assert!(!b.is_empty(), "{k:?} is empty");
                assert!(b.iter().all(|v| v.is_finite()), "{k:?} has NaN");
                let peak = b.iter().fold(0.0f32, |m, v| m.max(v.abs()));
                assert!(peak > 0.05 && peak <= 1.0, "{k:?} peak {peak}");
                total += b.len();
            }
        }
        let secs = total as f32 / RATE as f32;
        println!("{secs:.1} s of audio in {:?}", t0.elapsed());
        assert!(t0.elapsed().as_secs_f32() < 3.0);
    }
}
