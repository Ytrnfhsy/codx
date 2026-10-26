//! Sabotage: one neutral bomb in the middle, plant it on the enemy site.

use crate::game::Game;
use toon_shared::consts::*;
use toon_shared::protocol::{flags, S2C};
use toon_shared::weapons::Cause;
use toon_shared::{Mode, Team};

impl Game {
    pub fn drop_bomb_if_carrier(&mut self, id: u8) {
        if self.mode != Mode::Sab || self.sab.carrier != Some(id) {
            return;
        }
        let pos = self.p(id).map(|p| p.body.pos).unwrap_or(self.map.bomb);
        let floor = self.world.floor_below(pos + glam::vec3(0.0, 0.5, 0.0), 30.0).unwrap_or(pos.y);
        self.sab.carrier = None;
        self.sab.plant = None;
        self.sab.pos = glam::vec3(pos.x, floor, pos.z);
        self.sab.dropped_since = self.time;
        let name = self.p(id).map(|p| p.name.clone()).unwrap_or_default();
        self.broadcast(S2C::BombEvent { text: format!("{name} впустив бомбу!") });
    }

    pub fn check_elimination(&mut self) {
        let alive = |t: Team| self.players.iter().flatten().filter(|p| p.team == t && p.alive).count();
        let present = |t: Team| self.players.iter().flatten().filter(|p| p.team == t).count();
        let (a, b) = (alive(Team::Allies), alive(Team::Axis));
        // a planted bomb still decides the round even if the planters died
        if self.sab.planted_at.is_some() {
            if let Some(site) = self.sab.planted_at {
                let defenders = site;
                if alive(defenders) == 0 && present(defenders) > 0 {
                    self.round_win(Some(defenders.enemy()), "Захисників знищено");
                }
            }
            return;
        }
        if a == 0 && b > 0 && present(Team::Allies) > 0 {
            self.round_win(Some(Team::Axis), "Ворога знищено");
        } else if b == 0 && a > 0 && present(Team::Axis) > 0 {
            self.round_win(Some(Team::Allies), "Ворога знищено");
        } else if a == 0 && b == 0 {
            self.round_win(None, "Усі загинули");
        }
    }

    pub fn update_sab(&mut self) {
        let time = self.time;
        let cfg = self.cfg.sab.clone();
        for p in self.players.iter_mut().flatten() {
            p.progress = 0.0;
        }

        // carrier left / died → drop
        if let Some(c) = self.sab.carrier {
            match self.p(c) {
                Some(p) if p.alive => self.sab.pos = p.body.pos,
                _ => {
                    self.sab.carrier = None;
                    self.sab.dropped_since = time;
                }
            }
        }

        if let Some(site) = self.sab.planted_at {
            // defusing by the site's team
            let pos = self.sab.pos;
            let defuser = self
                .players
                .iter()
                .flatten()
                .filter(|p| p.alive && p.team == site && p.flags & flags::USE != 0 && p.body.pos.distance(pos) < SAB_DEFUSE_RADIUS)
                .map(|p| p.id)
                .next();
            match defuser {
                Some(d) => {
                    let start = match self.sab.defuse {
                        Some((who, s)) if who == d => s,
                        _ => {
                            self.sab.defuse = Some((d, time));
                            time
                        }
                    };
                    let k = ((time - start) as f32 / cfg.defuse_time).min(1.0);
                    if let Some(p) = self.pm(d) {
                        p.progress = k;
                    }
                    if k >= 1.0 {
                        self.sab.planted_at = None;
                        self.sab.defuse = None;
                        self.sab.carrier = Some(d);
                        self.sab.owner = Some(site);
                        let name = self.p(d).map(|p| p.name.clone()).unwrap_or_default();
                        if let Some(p) = self.pm(d) {
                            p.score += SCORE_DEFUSE;
                        }
                        self.send(d, S2C::ScorePopup { points: SCORE_DEFUSE, text: "Знешкоджено".into() });
                        self.broadcast(S2C::BombEvent { text: format!("{name} знешкодив бомбу!") });
                        self.logf(format!("{name} знешкодив бомбу"));
                        if self.overtime {
                            self.round_win(Some(site), "Бомбу знешкоджено в овертаймі");
                        }
                        return;
                    }
                }
                None => self.sab.defuse = None,
            }
            if time >= self.sab.explode_at {
                let pos = self.sab.pos;
                self.sab.planted_at = None;
                self.explode(pos + glam::vec3(0.0, 0.5, 0.0), SAB_EXPLOSION_RADIUS, 500.0, None, Cause::Bomb, true);
                self.round_win(Some(site.enemy()), "Ціль знищено");
            }
            return;
        }

        match self.sab.carrier {
            None => {
                // pick up
                let pos = self.sab.pos;
                let owner = self.sab.owner;
                let picker = self
                    .players
                    .iter()
                    .flatten()
                    .filter(|p| p.alive && p.team.is_playing() && (owner.is_none() || owner == Some(p.team)))
                    .filter(|p| p.body.pos.distance(pos) < SAB_PICKUP_RADIUS)
                    .map(|p| (p.id, p.team, p.name.clone()))
                    .next();
                if let Some((id, team, name)) = picker {
                    self.sab.carrier = Some(id);
                    self.sab.owner = Some(team);
                    self.send_team(team, S2C::BombEvent { text: format!("{name} несе бомбу — супроводжуйте!") });
                    self.send_team(team.enemy(), S2C::BombEvent { text: "Ворог підібрав бомбу!".into() });
                } else if time - self.sab.dropped_since > 60.0 && self.sab.pos.distance(self.map.bomb) > 1.0 {
                    self.reset_bomb();
                    self.broadcast(S2C::BombEvent { text: "Бомбу повернуто в центр".into() });
                }
            }
            Some(c) => {
                // planting on the enemy site
                let (team, pos, using, name) = match self.p(c) {
                    Some(p) => (p.team, p.body.pos, p.flags & flags::USE != 0, p.name.clone()),
                    None => return,
                };
                let site = self.map.sites[team.enemy().index()];
                if using && pos.distance(site) < SAB_SITE_RADIUS {
                    let start = match self.sab.plant {
                        Some((who, s)) if who == c => s,
                        _ => {
                            self.sab.plant = Some((c, time));
                            time
                        }
                    };
                    let k = ((time - start) as f32 / cfg.plant_time).min(1.0);
                    if let Some(p) = self.pm(c) {
                        p.progress = k;
                    }
                    if k >= 1.0 {
                        self.sab.plant = None;
                        self.sab.carrier = None;
                        self.sab.planted_at = Some(team.enemy());
                        self.sab.pos = site;
                        self.sab.explode_at = time + cfg.bomb_timer as f64;
                        if let Some(p) = self.pm(c) {
                            p.score += SCORE_PLANT;
                        }
                        self.send(c, S2C::ScorePopup { points: SCORE_PLANT, text: "Бомбу закладено".into() });
                        self.broadcast(S2C::BombEvent { text: format!("{name} заклав бомбу!") });
                        self.logf(format!("{name} заклав бомбу"));
                    }
                } else {
                    self.sab.plant = None;
                }
            }
        }

        if self.overtime {
            self.check_elimination();
            if self.state == toon_shared::protocol::MatchState::Playing && time >= self.sab.overtime_end {
                self.round_win(None, "Овертайм завершився");
            }
        }
    }
}
