//! Procedurally synthesized sound effects (no asset files needed).

use macroquad::audio::{
    load_sound_from_bytes, play_sound, set_sound_volume, stop_sound, PlaySoundParams, Sound,
};
use macroquad::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use toon_shared::rng::Rng;
use toon_shared::weapons::{WeaponClass, WeaponId};

const RATE: u32 = 22050;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Sfx {
    Rifle,
    Smg,
    Lmg,
    Shotgun,
    Sniper,
    Pistol,
    Explosion,
    Hit,
    Kill,
    Step,
    Reload,
    Empty,
    Knife,
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
    /// Gunshot: filtered noise crack + low thump.
    fn shot(&mut self, len: f32, decay: f32, cutoff: f32, thump: f32, gain: f32) -> Vec<f32> {
        let mut b = Self::buf(len);
        let mut lp = 0.0f32;
        let a = (cutoff / RATE as f32).min(0.99);
        for (i, s) in b.iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            let env = (-t * decay).exp();
            lp += (self.noise() - lp) * a;
            let crack = if t < 0.004 { self.noise() } else { 0.0 };
            let th = (t * thump * std::f32::consts::TAU * (1.0 - t * 2.0).max(0.3)).sin()
                * (-t * decay * 0.6).exp();
            *s = (lp * 1.6 * env + crack * 0.6 + th * 0.8) * gain;
        }
        b
    }
    fn explosion(&mut self) -> Vec<f32> {
        let mut b = Self::buf(1.8);
        let (mut lp, mut lp2) = (0.0f32, 0.0f32);
        for (i, s) in b.iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            let env = if t < 0.02 {
                t / 0.02
            } else {
                (-(t - 0.02) * 2.6).exp()
            };
            lp += (self.noise() - lp) * 0.08;
            lp2 += (lp - lp2) * 0.2;
            let rumble = (t * std::f32::consts::TAU * (55.0 - t * 20.0)).sin() * (-t * 2.0).exp();
            *s = (lp2 * 3.5 + rumble * 0.7) * env;
        }
        b
    }
    fn tone(&mut self, len: f32, f0: f32, f1: f32, decay: f32, square: bool) -> Vec<f32> {
        let mut b = Self::buf(len);
        let mut ph = 0.0f32;
        for (i, s) in b.iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            let f = f0 + (f1 - f0) * (t / len);
            ph += f / RATE as f32;
            let w = (ph * std::f32::consts::TAU).sin();
            let w = if square { w.signum() * 0.5 } else { w };
            let attack = (t / 0.003).min(1.0);
            *s = w * (-t * decay).exp() * attack * 0.6;
        }
        b
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
    fn step(&mut self) -> Vec<f32> {
        let mut b = Self::buf(0.09);
        let mut lp = 0.0;
        for (i, s) in b.iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            lp += (self.noise() - lp) * 0.15;
            *s = lp * (-t * 45.0).exp() * 1.2;
        }
        b
    }
    fn whoosh(&mut self, len: f32, peak: f32, cutoff: f32) -> Vec<f32> {
        let mut b = Self::buf(len);
        let mut lp = 0.0;
        for (i, s) in b.iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            let x = t / len;
            let env = (-((x - peak) * (x - peak)) * 18.0).exp();
            lp += (self.noise() - lp) * cutoff;
            *s = lp * env * 1.6;
        }
        b
    }
    fn heli(&mut self) -> Vec<f32> {
        let mut b = Self::buf(0.6);
        let mut lp = 0.0;
        for (i, s) in b.iter_mut().enumerate() {
            let t = i as f32 / RATE as f32;
            let blade = ((t * 10.0).fract() * std::f32::consts::TAU)
                .cos()
                .max(0.0)
                .powi(6);
            lp += (self.noise() - lp) * 0.06;
            *s = lp * (0.25 + blade * 1.5) + (t * 70.0 * std::f32::consts::TAU).sin() * 0.15;
        }
        b
    }
}

pub struct Audio {
    sounds: HashMap<Sfx, Sound>,
    pub volume: f32,
    last: RefCell<HashMap<Sfx, f64>>,
    heli_on: RefCell<bool>,
}

impl Audio {
    pub async fn load(volume: f32, enabled: bool) -> Audio {
        let mut sounds = HashMap::new();
        if enabled {
            let mut s = Synth { rng: Rng::new(7) };
            let mut list: Vec<(Sfx, Vec<f32>)> = vec![
                (Sfx::Rifle, s.shot(0.32, 18.0, 6000.0, 120.0, 0.9)),
                (Sfx::Smg, s.shot(0.22, 26.0, 7000.0, 160.0, 0.75)),
                (Sfx::Lmg, s.shot(0.36, 15.0, 4500.0, 95.0, 0.95)),
                (Sfx::Shotgun, s.shot(0.55, 9.0, 3000.0, 70.0, 1.1)),
                (Sfx::Sniper, s.shot(0.8, 7.0, 3800.0, 60.0, 1.1)),
                (Sfx::Pistol, s.shot(0.2, 30.0, 8000.0, 200.0, 0.7)),
                (Sfx::Explosion, s.explosion()),
                (Sfx::Hit, s.tone(0.06, 2200.0, 1800.0, 50.0, true)),
                (Sfx::Step, s.step()),
                (Sfx::Empty, s.tone(0.04, 1500.0, 1200.0, 90.0, true)),
                (Sfx::Knife, s.whoosh(0.25, 0.4, 0.35)),
                (Sfx::Beep, s.tone(0.12, 1760.0, 1760.0, 12.0, false)),
                (Sfx::Jet, s.whoosh(2.6, 0.5, 0.05)),
                (Sfx::Click, s.tone(0.05, 900.0, 600.0, 60.0, false)),
                (Sfx::Hurt, s.tone(0.18, 180.0, 90.0, 18.0, false)),
                (Sfx::Heli, s.heli()),
            ];
            let mut kill = s.tone(0.12, 1320.0, 1320.0, 20.0, false);
            Synth::mix(
                &mut kill,
                &s.tone(0.18, 1760.0, 1760.0, 14.0, false),
                0.07,
                1.0,
            );
            list.push((Sfx::Kill, kill));
            let mut reload = s.tone(0.05, 600.0, 400.0, 60.0, true);
            Synth::mix(
                &mut reload,
                &s.tone(0.06, 900.0, 700.0, 60.0, true),
                0.22,
                1.0,
            );
            list.push((Sfx::Reload, reload));
            let mut streak = Vec::new();
            for (k, f) in [523.0f32, 659.0, 784.0, 1046.0].iter().enumerate() {
                Synth::mix(
                    &mut streak,
                    &s.tone(0.25, *f, *f, 8.0, true),
                    k as f32 * 0.09,
                    0.6,
                );
            }
            list.push((Sfx::Streak, streak));
            let mut bomb = s.tone(0.3, 330.0, 330.0, 6.0, true);
            Synth::mix(&mut bomb, &s.tone(0.3, 247.0, 247.0, 6.0, true), 0.25, 1.0);
            list.push((Sfx::Bomb, bomb));
            for (k, v) in list {
                if let Ok(snd) = load_sound_from_bytes(&wav(&v)).await {
                    sounds.insert(k, snd);
                }
            }
        }
        Audio {
            sounds,
            volume,
            last: RefCell::new(HashMap::new()),
            heli_on: RefCell::new(false),
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
            if now - *l < 0.025 {
                return;
            }
            *l = now;
        }
        if let Some(snd) = self.sounds.get(&s) {
            play_sound(
                snd,
                PlaySoundParams {
                    looped: false,
                    volume: (vol * self.volume).min(1.0),
                },
            );
        }
    }

    /// Distance-attenuated sound.
    pub fn play_at(&self, s: Sfx, pos: Vec3, listener: Vec3, vol: f32, range: f32) {
        let d = pos.distance(listener);
        let k = (1.0 - d / range).clamp(0.0, 1.0);
        self.play(s, vol * (0.08 + 0.92 * k * k));
    }

    /// Helicopter rotor loop with distance-based volume (None = stop).
    pub fn heli(&self, dist: Option<f32>) {
        let Some(snd) = self.sounds.get(&Sfx::Heli) else {
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
