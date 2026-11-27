//! Where a sound comes from: the room's echo, distance, panning and the binaural directions.

use super::*;

/// How a place sounds. The reverb is a convolution with velvet noise — sparse random
/// clicks that fade away — split into a low band that rings long and a high band that dies
/// fast. Unlike comb-filter reverbs it has no resonances, so nothing sounds like a pipe.
#[derive(Clone, Copy)]
pub(super) struct Space {
    /// Gap before the first reflection (s).
    pub(super) predelay: f32,
    /// Reflections per second at first (sparse, distinct walls) and later (diffuse).
    pub(super) early: f32,
    pub(super) density: f32,
    /// How long the early, sparse part lasts (s).
    pub(super) early_len: f32,
    /// Decay to -60 dB of the lows and of the highs (s).
    pub(super) rt_low: f32,
    pub(super) rt_high: f32,
    /// Level of the reverb against the direct sound.
    pub(super) wet: f32,
    /// Low-pass on the reverb (air absorbs the highs).
    pub(super) air: f32,
}

/// A town street: houses all around, hard walls.
pub(super) const TOWN: Space = Space {
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
pub(super) const VALLEY: Space = Space {
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
pub(super) const NEAR: Space = Space {
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
pub(super) fn velvet(sp: &Space, rt: f32, rng: &mut Rng) -> Vec<(usize, f32)> {
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
pub(super) fn convolve(
    out: &mut [[f32; 2]],
    ch: usize,
    x: &[f32],
    taps: &[(usize, f32)],
    gain: f32,
) {
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
pub(super) fn spatialize(dry: &[f32], sp: Space, tail: f32, rng: &mut Rng) -> Vec<[f32; 2]> {
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
pub(super) fn binaural(pcm: &[[f32; 2]], az: f32, looped: bool) -> Vec<[f32; 2]> {
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

pub(super) fn mono(b: &[f32]) -> Vec<[f32; 2]> {
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
pub(super) fn pan_weights(az: f32) -> [f32; DIRS.len() + 1] {
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
pub(super) fn pan(b: &[f32], p: f32) -> Vec<[f32; 2]> {
    let a = (p.clamp(-1.0, 1.0) + 1.0) * PI / 4.0;
    let (l, r) = (a.cos() * 1.41, a.sin() * 1.41);
    b.iter().map(|v| [v * l, v * r]).collect()
}

pub(super) fn normalize(b: &mut [[f32; 2]], peak: f32) {
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
pub(super) fn fade_out(b: &mut [[f32; 2]], secs: f32) {
    let n = ((secs * SR) as usize).min(b.len());
    let start = b.len() - n;
    for (k, v) in b[start..].iter_mut().enumerate() {
        let g = 1.0 - k as f32 / n as f32;
        v[0] *= g;
        v[1] *= g;
    }
}

// ---------------------------------------------------------------- synthesis
