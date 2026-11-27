//! Getting the sounds ready: recordings shipped with the game, synthesis, WAV, every direction.

use super::*;

/// 16-bit stereo WAV.
pub(super) fn wav(samples: &[[f32; 2]]) -> Vec<u8> {
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

/// The prepared versions of a sound (one list of variants per direction in `DIRS`): short
/// sounds get all their variants (footsteps should not repeat), long ones the first.
pub(super) async fn add_dirs(dirs: &mut HashMap<Sfx, Vec<Vec<Sound>>>, k: Sfx, pcm: &[[f32; 2]]) {
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
pub(super) async fn loop_dirs(pcm: &[[f32; 2]]) -> Vec<Sound> {
    let mut v = Vec::new();
    for pcm in std::iter::once(pcm.to_vec()).chain(DIRS.iter().map(|&az| binaural(pcm, az, true))) {
        if let Ok(snd) = load_sound_from_bytes(&wav(&pcm)).await {
            v.push(snd);
        }
    }
    v
}

/// Decodes a recording (WAV or Ogg Vorbis, mono or stereo) into 44.1 kHz stereo, or says
/// why it cannot. Files are checked here because the audio backend aborts on bad data.
pub(super) fn decode(data: &[u8]) -> Result<Vec<[f32; 2]>, String> {
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
pub(super) fn recordings(s: Sfx) -> Vec<(String, Vec<u8>)> {
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
pub(super) const BUNDLED: &[(&str, &[u8])] = &[
    ("step", include_bytes!("../../../../assets/sounds/step.ogg")),
    (
        "step",
        include_bytes!("../../../../assets/sounds/step_2.ogg"),
    ),
    (
        "step",
        include_bytes!("../../../../assets/sounds/step_3.ogg"),
    ),
    (
        "step",
        include_bytes!("../../../../assets/sounds/step_4.ogg"),
    ),
    (
        "step",
        include_bytes!("../../../../assets/sounds/step_5.ogg"),
    ),
    (
        "step",
        include_bytes!("../../../../assets/sounds/step_6.ogg"),
    ),
    (
        "step_soft",
        include_bytes!("../../../../assets/sounds/step_soft.ogg"),
    ),
    (
        "step_soft",
        include_bytes!("../../../../assets/sounds/step_soft_2.ogg"),
    ),
    (
        "step_soft",
        include_bytes!("../../../../assets/sounds/step_soft_3.ogg"),
    ),
    (
        "step_soft",
        include_bytes!("../../../../assets/sounds/step_soft_4.ogg"),
    ),
    (
        "step_soft",
        include_bytes!("../../../../assets/sounds/step_soft_5.ogg"),
    ),
    (
        "step_soft",
        include_bytes!("../../../../assets/sounds/step_soft_6.ogg"),
    ),
    ("door", include_bytes!("../../../../assets/sounds/door.ogg")),
    (
        "door",
        include_bytes!("../../../../assets/sounds/door_2.ogg"),
    ),
    (
        "door",
        include_bytes!("../../../../assets/sounds/door_3.ogg"),
    ),
    (
        "door_shut",
        include_bytes!("../../../../assets/sounds/door_shut.ogg"),
    ),
    (
        "door_shut",
        include_bytes!("../../../../assets/sounds/door_shut_2.ogg"),
    ),
    (
        "door_shut",
        include_bytes!("../../../../assets/sounds/door_shut_3.ogg"),
    ),
    (
        "explosion",
        include_bytes!("../../../../assets/sounds/explosion.ogg"),
    ),
    ("rain", include_bytes!("../../../../assets/sounds/rain.ogg")),
    ("heli", include_bytes!("../../../../assets/sounds/heli.ogg")),
    (
        "bolt_ak47",
        include_bytes!("../../../../assets/sounds/bolt_ak47.ogg"),
    ),
    (
        "bolt_ak74u",
        include_bytes!("../../../../assets/sounds/bolt_ak74u.ogg"),
    ),
    (
        "bolt_m249",
        include_bytes!("../../../../assets/sounds/bolt_m249.ogg"),
    ),
    (
        "bolt_m249",
        include_bytes!("../../../../assets/sounds/bolt_m249_2.ogg"),
    ),
    (
        "bolt_mp5",
        include_bytes!("../../../../assets/sounds/bolt_mp5.ogg"),
    ),
    (
        "bolt_p90",
        include_bytes!("../../../../assets/sounds/bolt_p90.ogg"),
    ),
    (
        "bolt_rpd",
        include_bytes!("../../../../assets/sounds/bolt_rpd.ogg"),
    ),
    (
        "bolt_rpd",
        include_bytes!("../../../../assets/sounds/bolt_rpd_2.ogg"),
    ),
    (
        "bolt_usp",
        include_bytes!("../../../../assets/sounds/bolt_usp.ogg"),
    ),
    (
        "gun_ak47",
        include_bytes!("../../../../assets/sounds/gun_ak47.wav"),
    ),
    (
        "gun_ak47",
        include_bytes!("../../../../assets/sounds/gun_ak47_2.wav"),
    ),
    (
        "gun_ak47",
        include_bytes!("../../../../assets/sounds/gun_ak47_3.ogg"),
    ),
    (
        "gun_deagle",
        include_bytes!("../../../../assets/sounds/gun_deagle.ogg"),
    ),
    (
        "gun_m249",
        include_bytes!("../../../../assets/sounds/gun_m249.ogg"),
    ),
    (
        "gun_m249",
        include_bytes!("../../../../assets/sounds/gun_m249_2.ogg"),
    ),
    (
        "gun_m4",
        include_bytes!("../../../../assets/sounds/gun_m4.ogg"),
    ),
    (
        "gun_m4",
        include_bytes!("../../../../assets/sounds/gun_m4_2.ogg"),
    ),
    (
        "gun_m4",
        include_bytes!("../../../../assets/sounds/gun_m4_3.ogg"),
    ),
    (
        "gun_m4",
        include_bytes!("../../../../assets/sounds/gun_m4_4.ogg"),
    ),
    (
        "gun_m4",
        include_bytes!("../../../../assets/sounds/gun_m4_5.ogg"),
    ),
    (
        "gun_m40",
        include_bytes!("../../../../assets/sounds/gun_m40.ogg"),
    ),
    (
        "gun_m40",
        include_bytes!("../../../../assets/sounds/gun_m40_2.ogg"),
    ),
    (
        "gun_m40",
        include_bytes!("../../../../assets/sounds/gun_m40_3.ogg"),
    ),
    (
        "gun_m9",
        include_bytes!("../../../../assets/sounds/gun_m9.ogg"),
    ),
    (
        "gun_mp5",
        include_bytes!("../../../../assets/sounds/gun_mp5.ogg"),
    ),
    (
        "gun_mp5",
        include_bytes!("../../../../assets/sounds/gun_mp5_2.ogg"),
    ),
    (
        "gun_mp5",
        include_bytes!("../../../../assets/sounds/gun_mp5_3.ogg"),
    ),
    (
        "gun_mp5",
        include_bytes!("../../../../assets/sounds/gun_mp5_4.ogg"),
    ),
    (
        "gun_mp5",
        include_bytes!("../../../../assets/sounds/gun_mp5_5.ogg"),
    ),
    (
        "gun_p90",
        include_bytes!("../../../../assets/sounds/gun_p90.ogg"),
    ),
    (
        "gun_p90",
        include_bytes!("../../../../assets/sounds/gun_p90_2.ogg"),
    ),
    (
        "gun_p90",
        include_bytes!("../../../../assets/sounds/gun_p90_3.ogg"),
    ),
    (
        "gun_r700",
        include_bytes!("../../../../assets/sounds/gun_r700.ogg"),
    ),
    (
        "gun_r700",
        include_bytes!("../../../../assets/sounds/gun_r700_2.ogg"),
    ),
    (
        "gun_r700",
        include_bytes!("../../../../assets/sounds/gun_r700_3.ogg"),
    ),
    (
        "gun_r700",
        include_bytes!("../../../../assets/sounds/gun_r700_4.ogg"),
    ),
    (
        "gun_r700",
        include_bytes!("../../../../assets/sounds/gun_r700_5.ogg"),
    ),
    (
        "gun_rpd",
        include_bytes!("../../../../assets/sounds/gun_rpd.ogg"),
    ),
    (
        "gun_rpd",
        include_bytes!("../../../../assets/sounds/gun_rpd_2.ogg"),
    ),
    (
        "gun_rpd",
        include_bytes!("../../../../assets/sounds/gun_rpd_3.ogg"),
    ),
    (
        "gun_usp",
        include_bytes!("../../../../assets/sounds/gun_usp.ogg"),
    ),
    (
        "mag_in_ak47",
        include_bytes!("../../../../assets/sounds/mag_in_ak47.ogg"),
    ),
    (
        "mag_in_ak74u",
        include_bytes!("../../../../assets/sounds/mag_in_ak74u.ogg"),
    ),
    (
        "mag_in_m249",
        include_bytes!("../../../../assets/sounds/mag_in_m249.ogg"),
    ),
    (
        "mag_in_mp5",
        include_bytes!("../../../../assets/sounds/mag_in_mp5.ogg"),
    ),
    (
        "mag_in_mp5",
        include_bytes!("../../../../assets/sounds/mag_in_mp5_2.ogg"),
    ),
    (
        "mag_in_p90",
        include_bytes!("../../../../assets/sounds/mag_in_p90.ogg"),
    ),
    (
        "mag_in_rpd",
        include_bytes!("../../../../assets/sounds/mag_in_rpd.ogg"),
    ),
    (
        "mag_out_m249",
        include_bytes!("../../../../assets/sounds/mag_out_m249.ogg"),
    ),
    (
        "mag_out_mp5",
        include_bytes!("../../../../assets/sounds/mag_out_mp5.ogg"),
    ),
    (
        "mag_out_p90",
        include_bytes!("../../../../assets/sounds/mag_out_p90.ogg"),
    ),
    (
        "mag_out_rpd",
        include_bytes!("../../../../assets/sounds/mag_out_rpd.ogg"),
    ),
];

/// Synthesizes sounds in parallel; each job gets its own seeded generator, so the result
/// does not depend on the number of threads.
pub(super) fn synthesize(jobs: &[(Sfx, usize)]) -> Vec<(Sfx, Vec<[f32; 2]>)> {
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
