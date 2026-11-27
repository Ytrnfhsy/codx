//! Sounds made from scratch: guns, explosions, steps, the rest; filters.

use super::*;

/// Second-order filter (RBJ cookbook), transposed direct form II.
#[derive(Clone, Copy, Default)]
pub(super) struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    z1: f32,
    z2: f32,
}

#[derive(Clone, Copy)]
pub(super) enum Kind {
    Low,
    High,
    /// Band-pass with 0 dB at the peak.
    Band,
    /// Peaking EQ (gain in dB).
    Peak(f32),
}

impl Biquad {
    pub(super) fn new(kind: Kind, f: f32, q: f32) -> Biquad {
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
    pub(super) fn run(&mut self, x: f32) -> f32 {
        let y = self.b0 * x + self.z1;
        self.z1 = self.b1 * x - self.a1 * y + self.z2;
        self.z2 = self.b2 * x - self.a2 * y;
        y
    }
}

/// Runs a whole buffer through a chain of filters.
pub(super) fn filter(b: &mut [f32], chain: &[(Kind, f32, f32)]) {
    for &(k, f, q) in chain {
        let mut bq = Biquad::new(k, f, q);
        for v in b.iter_mut() {
            *v = bq.run(*v);
        }
    }
}

/// Parameters of a gunshot.
#[derive(Clone, Copy)]
pub(super) struct Gun {
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

pub(super) fn gun_params(w: WeaponId) -> Gun {
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

pub(super) struct Synth {
    pub(super) rng: Rng,
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

    pub(super) fn make(&mut self, s: Sfx, variant: usize) -> Vec<[f32; 2]> {
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
