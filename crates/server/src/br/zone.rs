//! The shrinking zone and the shelled red zone.

use super::*;

impl Game {
    /// The red zone: marked inside the circle (away from the next one), then shelled.
    pub(super) fn br_red_zone(&mut self) {
        let time = self.time;
        if let Some(rz) = self.br.red {
            if time >= rz.end {
                self.br.red = None;
            } else if time >= rz.start && time >= rz.next_shell {
                let mut z = rz;
                z.next_shell = time + self.rng.range(0.25, 0.6) as f64;
                self.br.red = Some(z);
                let a = self.rng.range(0.0, std::f32::consts::TAU);
                let d = self.rng.f32().sqrt() * rz.r;
                let q = rz.c + vec2(a.cos(), a.sin()) * d;
                // the shell lands on whatever is highest there (a roof shields those below)
                let top = self
                    .world
                    .floor_below(vec3(q.x, 300.0, q.y), 400.0)
                    .unwrap_or(0.0);
                self.explode(
                    vec3(q.x, top + 0.3, q.y),
                    6.0,
                    95.0,
                    None,
                    Cause::Artillery,
                    true,
                );
            }
            return;
        }
        if time < self.br.next_red || self.state != MatchState::Playing {
            return;
        }
        self.br.next_red = time + RED_EVERY;
        let (c, r) = self.br.zone.at(time);
        if r < 160.0 {
            return; // the end game is tight enough
        }
        let next = vec2(self.br.zone.to_c.x, self.br.zone.to_c.y);
        let size = self.rng.range(80.0, 110.0);
        for _ in 0..20 {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let d = self.rng.range(0.0, (r - size).max(0.0));
            let q = c + vec2(a.cos(), a.sin()) * d;
            if q.distance(next) > self.br.zone.to_r + size * 0.5 {
                self.br.red = Some(RedZone {
                    c: q,
                    r: size,
                    start: time + RED_WARNING,
                    end: time + RED_WARNING + RED_TIME,
                    next_shell: time + RED_WARNING,
                });
                self.center_all(
                    || {
                        t(
                            "Червона зона! Сховайся в будинку або тікай з неї",
                            "Red zone! Get indoors or get out of it",
                        )
                        .into()
                    },
                    3.5,
                );
                return;
            }
        }
    }

    /// Picks where the zone shrinks to next.
    pub(super) fn br_next_phase(&mut self, now: f64) {
        let z = self.br.zone;
        let (c, r) = z.at(now);
        let Some(&(wait, shrink, radius, dps)) = PHASES.get(z.phase) else {
            // the last circle stays put
            self.br.zone = Zone {
                from_c: c,
                from_r: r,
                to_c: c,
                to_r: r,
                shrink_at: f64::MAX,
                shrink_end: f64::MAX,
                dps: z.dps.max(15.0),
                phase: z.phase,
            };
            return;
        };
        // the circles were made for a 1 km island: bigger maps get bigger ones, and a
        // little more time to cross them
        let k = (self.map.size().max_element() / 1000.0).max(1.0);
        let (wait, shrink, radius) = (wait * k.sqrt(), shrink * k.sqrt(), radius * k);
        // the next circle lies inside this one, preferably centred on dry land
        let mut to = c;
        for k in 0..24 {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let d = self.rng.range(0.0, (r - radius).max(0.0)) * if k < 20 { 1.0 } else { 0.3 };
            let cand = c + vec2(a.cos(), a.sin()) * d;
            let dry = self
                .world
                .terrain
                .as_ref()
                .is_none_or(|t| t.height(cand.x, cand.y) > t.water + 1.0);
            let inside = (cand - self.map.center()).abs().max_element()
                < (self.map.size().max_element() * 0.5 - radius * 0.5).max(0.0);
            if dry && inside {
                to = cand;
                break;
            }
        }
        self.br.zone = Zone {
            from_c: c,
            from_r: r,
            to_c: to,
            to_r: radius,
            phase: z.phase,
            shrink_at: now + wait as f64,
            shrink_end: now + (wait + shrink) as f64,
            dps,
        };
    }
}
