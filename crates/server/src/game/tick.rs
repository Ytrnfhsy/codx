//! One server tick and the things lying on the ground that each player is told about.

use super::*;

impl Game {
    // ---------------------------------------------------------------- tick

    pub fn tick(&mut self, dt: f32) {
        self.time += dt as f64;
        self.tick = self.tick.wrapping_add(1);
        self.registry.tick(self.time);
        self.path_budget = PATHS_PER_TICK;
        self.update_state(dt);
        self.update_visibility();
        self.update_pings();
        self.update_bullets();
        self.update_pickups();

        // bots
        for id in self.ids() {
            if self.p(id).map(|p| p.is_bot()).unwrap_or(false) {
                self.update_bot(id, dt);
            }
        }

        let time = self.time;
        let mut fell = Vec::new();
        let mut auto_spawn = Vec::new();
        let tuning = &self.tuning;
        let max_speed = tuning.max_sprint_speed() * 1.2;
        let br_round = self.br.running;
        for p in self.players.iter_mut().flatten() {
            // gliding under a parachute is faster than running
            let speed = if p.body.chute != toon_shared::physics::Chute::None {
                toon_shared::physics::FREEFALL_GLIDE * 1.3
            } else {
                max_speed
            };
            p.move_budget = (p.move_budget + speed * dt).min(3.0);
            p.chat_budget = (p.chat_budget + dt * 0.5).min(5.0);
            p.item_budget = (p.item_budget + dt * 6.0).min(12.0);
            if !p.alive {
                if p.team.is_playing() && p.has_loadout {
                    let wait = if p.is_bot() { 0.0 } else { 3.0 };
                    if time >= p.respawn_at + wait {
                        auto_spawn.push(p.id);
                    }
                }
                continue;
            }
            let def = tuning.weapon(p.weapon().id);
            p.shot_budget = (p.shot_budget + dt * def.rpm / 60.0).min(2.0);
            let mag_max = p.mag_cap(def.mag);
            if let Some(end) = p.reload_end {
                if time >= end {
                    let w = p.weapon_mut();
                    let take = mag_max.saturating_sub(w.mag).min(w.reserve);
                    w.mag += take;
                    w.reserve -= take;
                    p.reload_end = None;
                }
            }
            // no healing by itself in the battle royale (medkits do that)
            if time - p.last_damage > REGEN_DELAY as f64 && p.hp < MAX_HEALTH && !br_round {
                p.hp = (p.hp + REGEN_RATE * dt).min(MAX_HEALTH);
            }
            if p.body.pos.y < -15.0 {
                fell.push(p.id);
            }
            p.history.push_back(HistEntry {
                t: time,
                pos: p.body.pos,
                yaw: p.yaw,
                stance: p.body.stance,
            });
            while p.history.len() > 40 {
                p.history.pop_front();
            }
        }
        for id in fell {
            self.kill(id, None, Cause::Fall, false);
        }
        for id in auto_spawn {
            if self.can_spawn(id) {
                self.spawn_player(id, false);
            }
        }

        self.update_grenades(dt);
        self.update_fires();
        let now = self.time;
        self.smokes.retain(|s| ((now - s.2) as f32) < SMOKE_TIME);
        self.update_blasts();
        self.update_helis(dt);
        self.update_vehicles(dt);
        self.update_cars(dt);
        self.bots_open_doors();
        self.sync_lying();

        if self.time >= self.next_housekeeping {
            self.next_housekeeping = self.time + 1.0;
            self.manage_bots();
            let rows = self.scoreboard();
            self.outbox.push((Dest::All, S2C::Scoreboard(rows), false));
            let ms = (self.time * 1000.0) as u32;
            self.outbox.push((Dest::All, S2C::Ping(ms), false));
        }
    }

    pub fn scoreboard(&self) -> Vec<ScoreRow> {
        self.players
            .iter()
            .flatten()
            .map(|p| ScoreRow {
                id: p.id,
                name: p.name.clone(),
                team: p.team,
                score: p.score,
                kills: p.kills,
                deaths: p.deaths,
                assists: p.assists,
                ping: if p.is_bot() {
                    0
                } else {
                    (p.ping * 1000.0) as u16
                },
                bot: p.is_bot(),
                alive: p.alive,
                place: p.br_place,
                squad: p.squad,
            })
            .collect()
    }

    /// What lies on the ground near a player (guns, supplies, death boxes): small things,
    /// a few pixels at most from further away.
    pub fn lying_near(&self, id: u8) -> Vec<EntSnap> {
        let focus = self.interest(id);
        let near = |q: Vec3| {
            focus.as_ref().is_none_or(|pts| {
                pts.iter().any(|f| {
                    vec3(q.x - f.x, 0.0, q.z - f.z).length_squared() < LOOT_RANGE * LOOT_RANGE
                })
            })
        };
        let mut out = Vec::new();
        for pk in &self.pickups {
            if near(pk.pos) {
                out.push(pk.snap());
            }
        }
        for l in &self.br.loot {
            if near(l.pos) {
                out.push(EntSnap::Loot {
                    id: l.id,
                    kind: l.kind,
                    amount: l.amount,
                    pos: l.pos.to_array(),
                });
            }
        }
        for b in &self.br.boxes {
            if near(b.pos) {
                out.push(EntSnap::DeathBox {
                    id: b.id,
                    pos: b.pos.to_array(),
                });
            }
        }
        out
    }

    /// Tells real players what appeared on the ground near them and what is gone, a few
    /// times a second (the things themselves are not in the snapshots).
    fn sync_lying(&mut self) {
        for id in 0..self.players.len() as u8 {
            if !(self.tick + id as u32).is_multiple_of(3) || self.p(id).is_none_or(|p| p.is_bot()) {
                continue;
            }
            let now = self.lying_near(id);
            let Some(p) = self.pm(id) else { continue };
            let mut remove: Vec<u16> = Vec::new();
            p.lying.retain(|k, _| {
                let keep = now.iter().any(|e| e.id() == *k);
                if !keep {
                    remove.push(*k);
                }
                keep
            });
            let mut add: Vec<EntSnap> = Vec::new();
            for e in now {
                if p.lying.get(&e.id()) != Some(&e) {
                    p.lying.insert(e.id(), e.clone());
                    add.push(e);
                }
            }
            // (in pieces that fit a datagram)
            let mut first = true;
            for part in add.chunks(36) {
                let remove = if first {
                    std::mem::take(&mut remove)
                } else {
                    Vec::new()
                };
                first = false;
                self.outbox.push((
                    Dest::One(id),
                    S2C::Ents {
                        add: part.to_vec(),
                        remove,
                    },
                    true,
                ));
            }
            for part in remove.chunks(300) {
                self.outbox.push((
                    Dest::One(id),
                    S2C::Ents {
                        add: Vec::new(),
                        remove: part.to_vec(),
                    },
                    true,
                ));
            }
        }
    }
}
