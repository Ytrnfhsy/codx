//! Sound effects: synthesized at start-up (no asset files needed), or real recordings
//! dropped into `assets/sounds/` (see `assets/sounds/README.md`), which take precedence.
//!
//! The synthesis follows how real recordings are built up. A gunshot is the supersonic
//! crack (an N-shaped pressure wave), the muzzle blast (a noise burst shaped by resonant
//! filters), a low punch, the clack of the action, then the town answering: early
//! reflections off nearby walls and a stereo reverb tail where the highs die first.
//! Mechanical sounds ring through banks of resonators tuned like small steel parts.
//! Frequently heard sounds come in several random variants so bursts never loop audibly.

mod sfx;
mod synth;

use bank::*;
pub use mixer::*;
pub use sfx::*;
pub use spatial::*;
use synth::*;
mod bank;
mod mixer;
mod spatial;

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
