//! Playing sounds: positions, loops (engines, rain, drones), queued cues.

use super::*;

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
