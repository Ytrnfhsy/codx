//! Killcam: the client keeps the last seconds of snapshots and events, and after a death
//! replays them through the killer's eyes — first person, with his gun, shots, hitmarkers
//! and sounds. The final kill of a match is replayed for everybody in slow motion.

use crate::audio::{reload_cues, Audio, Sfx};
use crate::effects::Effects;
use crate::ragdoll::{DeathBlow, Ragdoll};
use crate::render::{Actor, VmState};
use macroquad::prelude::*;
use std::collections::{HashMap, VecDeque};
use toon_shared::physics::{Stance, World};
use toon_shared::protocol::{flags, stance_bits, BombSnap, EntSnap, PlayerSnap, ShotFx};
use toon_shared::rng::Rng;
use toon_shared::tuning::Tuning;
use toon_shared::weapons::{Cause, WeaponId};
use toon_shared::{wrap_angle, Team};

/// How much history the client keeps (seconds).
const KEEP: f64 = 14.0;
/// Replay starts this long before the kill...
const LEAD: f64 = 3.4;
const LEAD_FINAL: f64 = 4.0;
/// ...and ends this long after it.
const TAIL: f64 = 1.0;
const TAIL_FINAL: f64 = 1.6;

pub struct Frame {
    pub t: f64,
    pub players: Vec<PlayerSnap>,
    pub ents: Vec<EntSnap>,
    pub shots: Vec<ShotFx>,
    pub bomb: Option<BombSnap>,
}

#[derive(Clone, Copy)]
pub enum Event {
    Melee(u8),
    Explosion {
        pos: Vec3,
        radius: f32,
    },
    Kill {
        killer: Option<u8>,
        victim: u8,
        cause: Cause,
        headshot: bool,
    },
}

#[derive(Default)]
pub struct Recorder {
    frames: VecDeque<Frame>,
    events: VecDeque<(f64, Event)>,
}

/// Deaths worth a killcam (a player you can look through did it).
pub fn has_killcam(c: Cause) -> bool {
    matches!(c, Cause::Weapon(_) | Cause::Grenade | Cause::Knife)
}

impl Recorder {
    pub fn push(&mut self, f: Frame) {
        let t = f.t;
        self.frames.push_back(f);
        while self.frames.front().is_some_and(|f| f.t < t - KEEP) {
            self.frames.pop_front();
        }
        while self.events.front().is_some_and(|e| e.0 < t - KEEP) {
            self.events.pop_front();
        }
    }

    pub fn event(&mut self, t: f64, e: Event) {
        self.events.push_back((t, e));
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.events.clear();
    }

    /// Server time of the first frame where `victim` is dead after being alive.
    pub fn death_time(&self, victim: u8, after: f64) -> Option<f64> {
        let mut was_alive = false;
        for f in self.frames.iter().filter(|f| f.t >= after) {
            if let Some(p) = f.players.iter().find(|p| p.id == victim) {
                if p.alive {
                    was_alive = true;
                } else if was_alive {
                    return Some(f.t);
                }
            }
        }
        None
    }

    /// The most recent kill (after `since`) that can be shown as a killcam.
    pub fn last_kill(&self, since: f64) -> Option<(f64, u8, u8, Cause, bool)> {
        self.events.iter().rev().find_map(|(t, e)| match *e {
            Event::Kill {
                killer: Some(k),
                victim,
                cause,
                headshot,
            } if *t >= since && k != victim && has_killcam(cause) => {
                Some((*t, k, victim, cause, headshot))
            }
            _ => None,
        })
    }
}

#[derive(Clone, Copy)]
struct Track {
    phase: f32,
    walk: f32,
    last: Option<Vec3>,
    alive: bool,
    melee_t: f64,
    reload_since: Option<f64>,
}

pub struct KillInfo {
    pub killer: u8,
    pub victim: u8,
    pub killer_name: String,
    pub victim_name: String,
    pub killer_team: Team,
    pub cause: Cause,
    pub headshot: bool,
}

pub struct Killcam {
    pub info: KillInfo,
    pub final_cam: bool,
    frames: Vec<Frame>,
    events: Vec<(f64, Event)>,
    t1: f64,
    pub t_kill: f64,
    /// Playback time (server clock).
    pub pt: f64,
    tracks: HashMap<u8, Track>,
    pub ragdolls: Vec<(u8, Ragdoll)>,
    pub fx: Effects,
    pub actors: Vec<Actor>,
    pub ents: Vec<EntSnap>,
    pub bomb: Option<BombSnap>,
    // the killer's first-person view
    pub eye: Vec3,
    pub yaw: f32,
    pub pitch: f32,
    eye_h: Option<f32>,
    pub ads_t: f32,
    pub weapon: WeaponId,
    pub team: Team,
    sprint: bool,
    recoil: f32,
    muzzle_at: f64,
    pub hitmarker: f64,
    pub hit_kill: bool,
    bob_phase: f32,
    walk_amount: f32,
    switch_at: f64,
    knife_at: f64,
    reload_cue: usize,
    pub shake: f32,
    rng: Rng,
    pub done: bool,
    /// Real seconds since the replay started.
    pub age: f32,
}

impl Killcam {
    pub fn new(rec: &Recorder, info: KillInfo, t_kill: f64, final_cam: bool) -> Option<Killcam> {
        let t0 = t_kill - if final_cam { LEAD_FINAL } else { LEAD };
        let t1 = t_kill + if final_cam { TAIL_FINAL } else { TAIL };
        let first = rec.frames.front()?.t;
        let t0 = t0.max(first);
        if t_kill - t0 < 1.0 {
            return None;
        }
        let frames: Vec<Frame> = rec
            .frames
            .iter()
            .filter(|f| f.t >= t0 - 0.2 && f.t <= t1 + 0.4)
            .map(|f| Frame {
                t: f.t,
                players: f.players.clone(),
                ents: f.ents.clone(),
                shots: f.shots.clone(),
                bomb: f.bomb.clone(),
            })
            .collect();
        if !frames
            .iter()
            .any(|f| f.players.iter().any(|p| p.id == info.killer))
        {
            return None;
        }
        let events = rec
            .events
            .iter()
            .filter(|e| e.0 >= t0 - 1.0 && e.0 <= t1 + 0.5)
            .copied()
            .collect();
        let weapon = match info.cause {
            Cause::Weapon(w) => w,
            _ => WeaponId::M4,
        };
        let team = info.killer_team;
        let mut kc = Killcam {
            info,
            final_cam,
            frames,
            events,
            t1,
            t_kill,
            pt: t0,
            tracks: HashMap::new(),
            ragdolls: Vec::new(),
            fx: Effects::new(),
            actors: Vec::new(),
            ents: Vec::new(),
            bomb: None,
            eye: Vec3::ZERO,
            yaw: 0.0,
            pitch: 0.0,
            eye_h: None,
            ads_t: 0.0,
            weapon,
            team,
            sprint: false,
            recoil: 0.0,
            muzzle_at: -10.0,
            hitmarker: -10.0,
            hit_kill: false,
            bob_phase: 0.0,
            walk_amount: 0.0,
            switch_at: -10.0,
            knife_at: -10.0,
            reload_cue: 0,
            shake: 0.0,
            rng: Rng::new(t_kill.to_bits()),
            done: false,
            age: 0.0,
        };
        // settle the initial state without spawning bodies for people already dead
        kc.sample(None, None, 0.0, true);
        kc.switch_at = -10.0;
        Some(kc)
    }

    /// Playback speed: the final killcam slows down around the killing shot.
    fn speed(&self) -> f64 {
        if self.final_cam && (self.t_kill - 0.35..self.t_kill + 0.45).contains(&self.pt) {
            0.3
        } else {
            1.0
        }
    }

    pub fn update(&mut self, dt: f32, world: &World, tuning: &Tuning, audio: Option<&Audio>) {
        if self.done {
            return;
        }
        self.age += dt;
        let speed = self.speed();
        let prev = self.pt;
        self.pt += dt as f64 * speed;
        let sdt = dt * speed as f32;

        // shots fired in this slice of time
        let eye = self.eye;
        let shots: Vec<ShotFx> = self
            .frames
            .iter()
            .filter(|f| f.t > prev && f.t <= self.pt)
            .flat_map(|f| f.shots.iter().cloned())
            .collect();
        for s in shots {
            let to = Vec3::from_array(s.to);
            let mut from = Vec3::from_array(s.from);
            if s.id == self.info.killer {
                // from the killer's gun, just below the view
                let d = (to - from).normalize_or_zero();
                let (_, r) = toon_shared::yaw_basis(self.yaw);
                from += d * 0.9 + r * 0.14 * (1.0 - self.ads_t) - Vec3::Y * 0.1;
                self.muzzle_at = self.pt;
                let heft = s.weapon.def().heft;
                self.recoil = (self.recoil + 0.5 + heft * 0.5).min(1.4);
                self.shake = (self.shake + heft * 0.05).min(0.3);
                if let Some(a) = audio {
                    a.play(Sfx::for_weapon(s.weapon), 0.85);
                    a.play_later(Sfx::Shell, 0.35 + self.rng.range(0.0, 0.15), 0.3);
                }
                if s.flesh {
                    self.hitmarker = self.pt;
                    if let Some(a) = audio {
                        a.play(Sfx::Hit, 0.5);
                    }
                }
            } else {
                self.fx.muzzle(from);
                if let Some(a) = audio {
                    let sfx = if s.id == 255 {
                        Sfx::Lmg
                    } else {
                        Sfx::for_weapon(s.weapon)
                    };
                    a.play_shot(sfx, from, eye, 0.7);
                }
            }
            self.fx.tracer(from, to);
            if s.flesh {
                self.fx.flesh(to);
            } else {
                self.fx.impact(to);
            }
        }
        // events
        let evs: Vec<Event> = self
            .events
            .iter()
            .filter(|e| e.0 > prev && e.0 <= self.pt)
            .map(|e| e.1)
            .collect();
        for e in evs {
            match e {
                Event::Melee(id) => {
                    if let Some(t) = self.tracks.get_mut(&id) {
                        t.melee_t = self.pt;
                    }
                    if id == self.info.killer {
                        self.knife_at = self.pt;
                    }
                    if let Some(a) = audio {
                        a.play(Sfx::Knife, 0.6);
                    }
                }
                Event::Explosion { pos, radius } => {
                    self.fx.explosion(pos, radius);
                    let d = pos.distance(self.eye);
                    self.shake = (self.shake + (1.0 - d / (radius * 4.0)).max(0.0)).min(1.2);
                    if let Some(a) = audio {
                        a.play_at(Sfx::Explosion, pos, self.eye, 1.0, 120.0);
                    }
                }
                Event::Kill {
                    killer, headshot, ..
                } => {
                    if killer == Some(self.info.killer) {
                        self.hitmarker = self.pt;
                        self.hit_kill = true;
                        if let Some(a) = audio {
                            a.play(if headshot { Sfx::HitHead } else { Sfx::Kill }, 0.6);
                        }
                    }
                }
            }
        }

        self.sample(Some(world), audio, sdt, false);

        // the killer's weapon handling
        let ws = tuning.weapon(self.weapon);
        let ads_speed = 1.0 / ws.ads_time.max(0.05);
        let want_ads = self
            .killer_snap()
            .map(|p| p.flags & flags::ADS != 0)
            .unwrap_or(false);
        self.ads_t = if want_ads {
            (self.ads_t + sdt * ads_speed).min(1.0)
        } else {
            (self.ads_t - sdt * ads_speed * 1.5).max(0.0)
        };
        self.recoil = (self.recoil - sdt * 7.0).max(0.0);
        self.shake = (self.shake - sdt * 2.5).max(0.0);
        if let (Some(a), Some(p)) = (audio, self.reload_progress(tuning)) {
            let cues = reload_cues(self.weapon.def().class);
            while self.reload_cue < cues.len() && p >= cues[self.reload_cue].0 {
                a.play(cues[self.reload_cue].1, 0.45);
                self.reload_cue += 1;
            }
        }
        self.fx.update(sdt);
        for (_, r) in &mut self.ragdolls {
            r.update(world, sdt);
        }
        if self.pt >= self.t1 {
            self.done = true;
        }
    }

    fn killer_snap(&self) -> Option<&PlayerSnap> {
        let i = self.frame_index(self.pt);
        self.frames[i]
            .players
            .iter()
            .find(|p| p.id == self.info.killer)
    }

    fn frame_index(&self, t: f64) -> usize {
        let i = self.frames.partition_point(|f| f.t <= t);
        i.saturating_sub(1).min(self.frames.len() - 1)
    }

    /// Interpolates every player at the playback time and updates bodies and the view.
    fn sample(&mut self, world: Option<&World>, audio: Option<&Audio>, dt: f32, init: bool) {
        let i = self.frame_index(self.pt);
        let j = (i + 1).min(self.frames.len() - 1);
        let (fa, fb) = (&self.frames[i], &self.frames[j]);
        let k = if j > i {
            ((self.pt - fa.t) / (fb.t - fa.t).max(1e-6)).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        self.ents = fb.ents.clone();
        // grenades fly smoothly
        for e in &mut self.ents {
            if let EntSnap::Grenade { id, pos } = e {
                if let Some(EntSnap::Grenade { pos: pa, .. }) = fa
                    .ents
                    .iter()
                    .find(|x| matches!(x, EntSnap::Grenade { id: i2, .. } if i2 == id))
                {
                    *pos = Vec3::from_array(*pa)
                        .lerp(Vec3::from_array(*pos), k)
                        .to_array();
                }
            }
        }
        self.bomb = fb.bomb.clone();
        let mut actors = Vec::new();
        let mut deaths = Vec::new();
        let killer = self.info.killer;
        for b in &fb.players {
            let a = fa.players.iter().find(|p| p.id == b.id).unwrap_or(b);
            let pos = Vec3::from_array(a.pos).lerp(Vec3::from_array(b.pos), k);
            let yaw = a.yaw + wrap_angle(b.yaw - a.yaw) * k;
            let pitch = a.pitch + (b.pitch - a.pitch) * k;
            let alive = if k < 0.5 { a.alive } else { b.alive };
            let tr = self.tracks.entry(b.id).or_insert(Track {
                phase: 0.0,
                walk: 0.0,
                last: None,
                alive,
                melee_t: -10.0,
                reload_since: None,
            });
            let moved = tr.last.map(|l| vec2(pos.x - l.x, pos.z - l.z).length());
            let vel = match tr.last {
                Some(l) if dt > 0.0 => ((pos - l) / dt).clamp_length_max(8.0),
                _ => Vec3::ZERO,
            };
            if let Some(m) = moved.filter(|m| *m < 4.0) {
                if dt > 0.0 {
                    let walk_k = ((m / dt) / 5.2).min(1.4);
                    tr.walk += (walk_k - tr.walk) * (dt * 10.0).min(1.0);
                }
                tr.phase += m * 2.2;
            }
            tr.last = Some(pos);
            if tr.alive && !alive && !init && b.id != killer {
                deaths.push((b.clone(), pos, vel));
            }
            tr.alive = alive;
            let reloading = b.flags & flags::RELOADING != 0;
            match (reloading, tr.reload_since) {
                (true, None) => tr.reload_since = Some(self.pt),
                (false, Some(_)) => tr.reload_since = None,
                _ => {}
            }
            if b.id == killer {
                let eh = Stance::from_u8(b.stance & 3).eye();
                let h = match self.eye_h {
                    Some(h) if dt > 0.0 => h + (eh - h) * (dt * 10.0).min(1.0),
                    _ => eh,
                };
                self.eye_h = Some(h);
                self.eye = pos + vec3(0.0, h, 0.0);
                self.yaw = yaw;
                self.pitch = pitch;
                self.team = b.team;
                self.sprint = b.flags & flags::SPRINT != 0;
                if b.weapon != self.weapon && b.alive {
                    self.weapon = b.weapon;
                    self.switch_at = self.pt;
                    if let Some(a) = audio {
                        a.play(Sfx::Raise, 0.4);
                    }
                }
                if reloading && tr.reload_since == Some(self.pt) {
                    self.reload_cue = 0;
                }
                let speed = moved
                    .map(|m| if dt > 0.0 { m / dt } else { 0.0 })
                    .unwrap_or(0.0);
                self.walk_amount +=
                    ((speed / 5.2).min(1.5) - self.walk_amount) * (dt * 8.0).min(1.0);
                self.bob_phase += speed * dt * 1.8;
                continue;
            }
            if !alive {
                continue;
            }
            actors.push(Actor {
                id: b.id,
                pos,
                yaw,
                pitch,
                stance: Stance::from_u8(b.stance & 3),
                team: b.team,
                weapon: b.weapon,
                flags: b.flags,
                phase: tr.phase,
                walk: tr.walk,
                melee: {
                    let m = (self.pt - tr.melee_t) as f32;
                    if m < 0.4 {
                        (m / 0.4).clamp(0.0, 1.0)
                    } else {
                        0.0
                    }
                },
                reload: tr.reload_since.map(|s| (self.pt - s) as f32),
                protected: false,
            });
            let _ = stance_bits::PROTECTED;
        }
        self.actors = actors;
        if let Some(world) = world {
            for (snap, pos, vel) in deaths {
                let blow = self.blow_for(snap.id);
                let r = Ragdoll::new(
                    pos,
                    snap.yaw,
                    Stance::from_u8(snap.stance & 3),
                    vel,
                    snap.team,
                    blow,
                    &mut self.rng,
                );
                self.ragdolls.push((snap.id, r));
                let _ = world;
            }
        }
    }

    /// How the player `victim` died (from the recorded kill and the killer's position).
    fn blow_for(&self, victim: u8) -> DeathBlow {
        let kill = self.events.iter().rev().find_map(|(t, e)| match *e {
            Event::Kill {
                killer,
                victim: v,
                cause,
                headshot,
            } if v == victim && (*t - self.pt).abs() < 1.5 => Some((killer, cause, headshot)),
            _ => None,
        });
        let Some((killer, cause, headshot)) = kill else {
            return DeathBlow {
                cause: Cause::World,
                headshot: false,
                from: None,
            };
        };
        let from = match cause {
            Cause::Grenade | Cause::Airstrike | Cause::Bomb => {
                self.events.iter().rev().find_map(|(t, e)| match *e {
                    Event::Explosion { pos, .. } if (*t - self.pt).abs() < 1.0 => Some(pos),
                    _ => None,
                })
            }
            _ => killer.and_then(|k| {
                if k == self.info.killer {
                    Some(self.eye)
                } else {
                    self.actors
                        .iter()
                        .find(|a| a.id == k)
                        .map(|a| a.pos + vec3(0.0, 1.3, 0.0))
                }
            }),
        };
        DeathBlow {
            cause,
            headshot,
            from,
        }
    }

    fn reload_progress(&self, tuning: &Tuning) -> Option<f32> {
        let t = self.tracks.get(&self.info.killer)?;
        let since = t.reload_since?;
        let dur = tuning.weapon(self.weapon).reload_time.max(0.1);
        Some(((self.pt - since) as f32 / dur).clamp(0.0, 1.0))
    }

    pub fn camera(&self, now: f64) -> (Vec3, f32, f32) {
        let s = self.shake;
        (
            self.eye,
            self.yaw + ((now * 53.0).sin() as f32) * s * 0.02,
            self.pitch + ((now * 61.0).cos() as f32) * s * 0.02,
        )
    }

    pub fn scoped(&self) -> bool {
        self.weapon.def().scoped && self.ads_t > 0.85
    }

    pub fn vm_state(&self, tuning: &Tuning) -> VmState {
        let ws = tuning.weapon(self.weapon);
        let since = |t: f64| (self.pt - t) as f32;
        VmState {
            weapon: self.weapon,
            sleeve: [200, 200, 200],
            hand: [255, 214, 170],
            ads: self.ads_t,
            bob_phase: self.bob_phase,
            bob: self.walk_amount * (1.0 - self.ads_t * 0.85) * tuning.movement.view_bob,
            sway: Vec2::ZERO,
            recoil: self.recoil,
            sprint: self.sprint && self.ads_t < 0.1,
            reload: self.reload_progress(tuning),
            raise: (1.0 - since(self.switch_at) / ws.swap_time.max(0.05)).clamp(0.0, 1.0),
            knife: Some(since(self.knife_at) / 0.45).filter(|k| (0.0..1.0).contains(k)),
            throwing: false,
            flash: since(self.muzzle_at) < 0.05,
        }
    }
}
