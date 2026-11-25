//! The match's best moments: while the match goes on, kills worth seeing again (several in
//! a few seconds, a very long shot, an arrow, a knife…) are kept as short replays through
//! the killer's eyes; between matches the best three play on the results screen.

use crate::killcam::{has_killcam, KillInfo, Killcam, Recorder};
use macroquad::prelude::Texture2D;
use toon_shared::lang::t;
use toon_shared::tr;
use toon_shared::weapons::{Cause, WeaponId};

/// Moments kept (the best by score).
pub const KEEP: usize = 3;
/// Kills by one player this close together make one moment.
const CHAIN: f64 = 6.0;
/// A moment is cut this long after its last kill (the recorder has it by then).
const SETTLE: f64 = 2.0;
/// The replay starts this long before the first kill, and runs on after the last.
const LEAD: f64 = 2.8;
const TAIL: f64 = 1.3;
/// Below this a kill is nothing special.
const WORTH: f32 = 35.0;

#[derive(Clone, Copy)]
struct Kill {
    t: f64,
    cause: Cause,
    dist: u16,
    headshot: bool,
}

/// Kills in the making of a moment.
struct Run {
    info: KillInfo,
    kills: Vec<Kill>,
}

pub struct Clip {
    rec: Recorder,
    info: KillInfo,
    t0: f64,
    t_kill: f64,
    t1: f64,
    pub title: String,
    pub sub: String,
    pub score: f32,
    /// The match clock when it happened (seconds played).
    pub clock: f32,
    /// A still of it for the small windows (made the first time it is shown).
    pub thumb: Option<Texture2D>,
}

impl Clip {
    /// Its replay from the start.
    pub fn replay(&self) -> Option<Killcam> {
        Killcam::span(
            &self.rec,
            self.info.clone(),
            self.t0,
            self.t_kill,
            self.t1,
            false,
        )
    }
}

#[derive(Default)]
pub struct Highlights {
    runs: Vec<Run>,
    pub clips: Vec<Clip>,
}

fn arrow(c: Cause) -> bool {
    matches!(c, Cause::Weapon(WeaponId::Bow | WeaponId::Crossbow))
}

/// How good a run of kills is, its title and its line under it.
fn rate(r: &Run) -> (f32, String, String) {
    let n = r.kills.len();
    let mut score = match n {
        0 | 1 => 0.0,
        2 => 40.0,
        3 => 90.0,
        4 => 150.0,
        _ => 220.0,
    };
    let far = r.kills.iter().map(|k| k.dist).max().unwrap_or(0);
    for k in &r.kills {
        if k.dist >= 50 {
            score += (k.dist as f32 - 30.0) * 0.6;
        }
        if k.headshot {
            score += 8.0;
        }
        score += match k.cause {
            Cause::Knife => 30.0,
            c if arrow(c) => 25.0,
            Cause::Grenade => 10.0,
            _ => 0.0,
        };
    }
    let last = r.kills.last().copied();
    let cause = last.map_or(Cause::World, |k| k.cause);
    let title = match n {
        2 => t("Подвійне вбивство", "Double kill").to_string(),
        3 => t("Потрійне вбивство", "Triple kill").to_string(),
        4 => t("Четверне вбивство", "Quad kill").to_string(),
        n if n >= 5 => tr!("Мультивбивство ×{n}", "Multi kill ×{n}"),
        _ if cause == Cause::Knife => t("Ножем", "With a knife").to_string(),
        _ if arrow(cause) => tr!("Стріла з {far} м", "An arrow from {far} m"),
        _ if far >= 50 => tr!("Постріл з {far} м", "A {far} m shot"),
        _ => t("Хедшот", "Headshot").to_string(),
    };
    let heads = r.kills.iter().filter(|k| k.headshot).count();
    let mut sub = format!("{} · {}", r.info.killer_name, cause.name());
    if n >= 2 {
        let span = r.kills.last().map_or(0.0, |l| l.t - r.kills[0].t);
        sub += &tr!(" · за {:.0} с", " · in {:.0} s", span.max(1.0));
    }
    if heads > 0 {
        sub += &if heads == 1 {
            t(" · хедшот", " · headshot").to_string()
        } else {
            tr!(" · {heads} хедшоти", " · {heads} headshots")
        };
    }
    (score, title, sub)
}

impl Highlights {
    pub fn clear(&mut self) {
        self.runs.clear();
        self.clips.clear();
    }

    /// A kill at server time `t`.
    pub fn on_kill(&mut self, t: f64, info: KillInfo, dist: u16) {
        if info.killer == info.victim || !has_killcam(info.cause) {
            return;
        }
        let k = Kill {
            t,
            cause: info.cause,
            dist,
            headshot: info.headshot,
        };
        match self.runs.iter_mut().find(|r| {
            r.info.killer == info.killer && r.kills.last().is_some_and(|l| t - l.t < CHAIN)
        }) {
            Some(r) => {
                r.kills.push(k);
                r.info = info;
            }
            None => self.runs.push(Run {
                info,
                kills: vec![k],
            }),
        }
    }

    /// Cuts the runs that are over out of the recording (`clock`: seconds played).
    pub fn update(&mut self, rec: &Recorder, clock: f32) {
        let Some(now) = rec.newest() else { return };
        let mut k = 0;
        while k < self.runs.len() {
            let last = self.runs[k].kills.last().map_or(0.0, |l| l.t);
            if now - last < SETTLE {
                k += 1;
                continue;
            }
            let run = self.runs.swap_remove(k);
            let (score, title, sub) = rate(&run);
            if score < WORTH {
                continue;
            }
            let worst = self.clips.iter().map(|c| c.score).fold(f32::MAX, f32::min);
            if self.clips.len() >= KEEP && score <= worst {
                continue;
            }
            let t0 = run.kills[0].t - LEAD;
            let t1 = last + TAIL;
            let clip = Clip {
                rec: rec.slice(t0 - 0.3, t1 + 0.5),
                info: run.info,
                t0,
                t_kill: last,
                t1,
                title,
                sub,
                score,
                clock: clock - (now - last) as f32,
                thumb: None,
            };
            // (one that cannot be replayed, the killer out of sight, is no use)
            if clip.replay().is_none() {
                continue;
            }
            self.clips.push(clip);
            self.clips.sort_by(|a, b| b.score.total_cmp(&a.score));
            self.clips.truncate(KEEP);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toon_shared::protocol::Gear;
    use toon_shared::Team;

    fn info(killer: u8, victim: u8, cause: Cause, headshot: bool) -> KillInfo {
        KillInfo {
            killer,
            victim,
            killer_name: format!("p{killer}"),
            victim_name: format!("p{victim}"),
            killer_team: Team::Allies,
            cause,
            headshot,
            gear: Gear::default(),
        }
    }

    #[test]
    fn kills_close_together_make_one_moment() {
        let mut h = Highlights::default();
        let w = Cause::Weapon(WeaponId::Uar15);
        h.on_kill(10.0, info(1, 2, w, false), 20);
        h.on_kill(12.0, info(1, 3, w, true), 20);
        h.on_kill(15.0, info(1, 4, w, false), 20);
        // somebody else's, and one much later
        h.on_kill(13.0, info(5, 1, w, false), 20);
        h.on_kill(40.0, info(1, 6, w, false), 20);
        assert_eq!(h.runs.len(), 3);
        let (score, title, sub) = rate(&h.runs[0]);
        assert!(score >= 90.0, "{score}");
        assert_eq!(title, t("Потрійне вбивство", "Triple kill"));
        assert!(sub.contains("p1"), "{sub}");
        // a single plain kill is not worth a moment, a long shot is
        assert!(rate(&h.runs[1]).0 < WORTH);
        let mut far = Highlights::default();
        far.on_kill(1.0, info(1, 2, Cause::Weapon(WeaponId::Barrett), true), 142);
        assert!(rate(&far.runs[0]).0 >= WORTH);
    }
}
