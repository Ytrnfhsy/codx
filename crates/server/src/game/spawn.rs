//! Where and when players come (back) into the match.

use super::*;

impl Game {
    // ---------------------------------------------------------------- spawning

    pub fn can_spawn(&self, id: u8) -> bool {
        let Some(p) = self.p(id) else { return false };
        if p.alive || !p.team.is_playing() || !p.has_loadout {
            return false;
        }
        match self.state {
            MatchState::PostMatch | MatchState::RoundEnd => false,
            MatchState::Playing => {
                !(self.mode == Mode::Sab && self.overtime || self.mode == Mode::Br)
            }
            // the battle royale waits in the lobby, not on the island
            _ => self.mode != Mode::Br,
        }
    }

    pub fn respawn_delay(&self) -> f32 {
        match self.mode {
            Mode::Tdm | Mode::Br => self.cfg.tdm.respawn_delay,
            Mode::Sab => self.cfg.sab.respawn_delay,
        }
    }

    /// Free spots on rings around the given spawn points (worked out once a map).
    fn ring_spots(&self, spawns: &[Spawn]) -> Vec<Spawn> {
        let mut out = Vec::new();
        for s in spawns {
            for ring in 1..=3 {
                let r = ring as f32 * 2.2;
                let n = 6 * ring;
                for k in 0..n {
                    let a = k as f32 / n as f32 * std::f32::consts::TAU + ring as f32;
                    let at = s.pos + vec3(a.cos() * r, 0.0, a.sin() * r);
                    let Some(y) = self.world.floor_below(at + vec3(0.0, 1.0, 0.0), 2.5) else {
                        continue;
                    };
                    let at = vec3(at.x, y, at.z);
                    // on the same level as the spawn, inside the map, with nothing in
                    // between (not over a railing, behind a wall or down on the quay)
                    let [x0, z0, x1, z1] = self.world.bounds;
                    let inside =
                        at.x > x0 + 1.0 && at.x < x1 - 1.0 && at.z > z0 + 1.0 && at.z < z1 - 1.0;
                    let chest = vec3(0.0, 1.0, 0.0);
                    if inside
                        && (y - s.pos.y).abs() < 0.6
                        && self.world.line_of_sight(s.pos + chest, at + chest)
                        && toon_shared::physics::fits(
                            &self.world,
                            at + vec3(0.0, 0.05, 0.0),
                            Stance::Stand,
                        )
                    {
                        out.push(Spawn {
                            pos: at,
                            yaw: s.yaw,
                        });
                    }
                }
            }
        }
        out
    }

    /// Picks a spawn point. Rounds and matches start at the team bases; during play anyone
    /// may appear anywhere on the ground floor: away from enemies and out of their sight,
    /// not where someone just died, and (in team deathmatch) not far from teammates.
    fn pick_spawn(&mut self, id: u8, side_only: bool) -> (Vec3, f32) {
        let team = self.p(id).map(|p| p.team).unwrap_or(Team::Allies);
        let time = self.time;
        self.recent_deaths.retain(|d| time - d.1 < 12.0);
        self.recent_spawns.retain(|d| time - d.1 < 4.0);
        let base = side_only || self.state == MatchState::PreRound || !team.is_playing();
        let mut cands: Vec<Spawn> = if base && team.is_playing() && !self.mode.solo() {
            self.map.spawns[team.index()].clone()
        } else {
            self.map.all_spawns().copied().collect()
        };
        // battle royale lobby: dozens of players wait on a handful of spawn points, so
        // each one gets rings of spots around it (nobody ends up inside someone else)
        if self.mode == Mode::Br && base {
            if self.lobby_spots.is_none() {
                self.lobby_spots = Some(self.ring_spots(&cands));
            }
            cands.extend(self.lobby_spots.iter().flatten().copied());
        } else if base && team.is_playing() && !self.mode.solo() {
            let k = team.index();
            if self.base_spots[k].is_none() {
                self.base_spots[k] = Some(self.ring_spots(&cands));
            }
            cands.extend(self.base_spots[k].iter().flatten().copied());
        }
        if !base {
            let centre = self.map.center();
            let centre = vec3(centre.x, 0.0, centre.y);
            for _ in 0..48 {
                let p = self.nav.random_point(&mut self.rng);
                // ground floor only (no spawning on roofs or balconies), with room to stand
                let floor = self.world.ground(p.x, p.z).unwrap_or(0.0);
                if p.y > floor + 0.6
                    || !toon_shared::physics::fits(
                        &self.world,
                        p + vec3(0.0, 0.05, 0.0),
                        Stance::Stand,
                    )
                {
                    continue;
                }
                let to = centre - p;
                let yaw = if to.length() > 1.0 {
                    toon_shared::yaw_of(to)
                } else {
                    self.rng.range(-3.1, 3.1)
                };
                cands.push(Spawn { pos: p, yaw });
            }
        }
        // never inside somebody: spots where a living player stands are out, as long as
        // any other is left
        let taken = |s: &Spawn| {
            self.players.iter().flatten().any(|p| {
                p.alive
                    && p.id != id
                    && vec2(p.body.pos.x - s.pos.x, p.body.pos.z - s.pos.z).length() < 1.1
                    && (p.body.pos.y - s.pos.y).abs() < 2.0
            })
        };
        if cands.iter().any(|s| !taken(s)) {
            cands.retain(|s| !taken(s));
        }
        let mut best = (f32::NEG_INFINITY, cands[0]);
        for s in &cands {
            let mut score = self.spawn_rng.range(0.0, 6.0);
            let mut min_enemy = 60.0f32;
            let mut min_friend = 60.0f32;
            for p in self.players.iter().flatten() {
                if !p.alive || p.id == id {
                    continue;
                }
                let d = p.body.pos.distance(s.pos);
                if d < 1.5 {
                    score -= 200.0;
                } else if d < 2.5 {
                    score -= 40.0;
                }
                if p.team == team {
                    min_friend = min_friend.min(d);
                } else {
                    min_enemy = min_enemy.min(d);
                    if d < 12.0 {
                        score -= 100.0;
                    }
                    // (the battle royale lobby: nobody is fighting yet, and hundreds of
                    // spots would each need a look over the hills)
                    if d < 55.0
                        && !(self.mode == Mode::Br && base)
                        && self
                            .world
                            .line_of_sight(p.eye(), s.pos + vec3(0.0, 1.5, 0.0))
                    {
                        score -= 60.0;
                    }
                }
            }
            // beyond ~28 m every spot is equally safe: let chance and teammates decide
            score += min_enemy.min(28.0);
            for (dp, _) in &self.recent_deaths {
                if dp.distance(s.pos) < 10.0 {
                    score -= 25.0;
                }
            }
            for (sp, _) in &self.recent_spawns {
                if sp.distance(s.pos) < 3.0 {
                    score -= 30.0;
                }
            }
            if !base {
                match self.mode {
                    // near the team, but not on top of it
                    Mode::Tdm if min_friend < 60.0 => {
                        score -= (min_friend - 15.0).max(0.0) * 0.3;
                    }
                    // Sabotage: stay on your own half of the map
                    Mode::Sab if team.is_playing() => {
                        let home = self.map.spawns[team.index()][0].pos;
                        let away = self.map.spawns[team.enemy().index()][0].pos;
                        if s.pos.distance(home) > s.pos.distance(away) {
                            score -= 40.0;
                        }
                    }
                    _ => {}
                }
            }
            if score > best.0 {
                best = (score, *s);
            }
        }
        self.recent_spawns.push((best.1.pos, time));
        (best.1.pos + vec3(0.0, 0.02, 0.0), best.1.yaw)
    }

    pub fn spawn_player(&mut self, id: u8, side_only: bool) {
        let (pos, yaw) = self.pick_spawn(id, side_only);
        let time = self.time;
        // protection counts from the moment fighting is possible
        let protect_from = if self.state == MatchState::PreRound {
            self.state_end
        } else {
            time
        };
        let protected_until = protect_from + self.cfg.spawn_protection as f64;
        let tuning = &self.tuning;
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        if let Some(l) = p.next_loadout.take() {
            p.loadout = l;
        }
        let l = p.loadout;
        p.alive = true;
        p.hp = MAX_HEALTH;
        p.body = Body {
            pos,
            vel: Vec3::ZERO,
            on_ground: true,
            stance: Stance::Stand,
            ..Default::default()
        };
        p.yaw = yaw;
        p.pitch = 0.0;
        p.flags = 0;
        p.slot = 0;
        p.weapons = [
            WState {
                id: l.primary,
                mag: tuning.weapon(l.primary).mag,
                reserve: l.reserve_for(l.primary, tuning),
            },
            WState {
                id: l.secondary,
                mag: tuning.weapon(l.secondary).mag,
                reserve: l.reserve_for(l.secondary, tuning),
            },
        ];
        p.grenades = l.grenades();
        p.smokes = l.tacticals();
        p.driving = None;
        p.reload_end = None;
        p.switch_end = time + 0.3;
        p.shot_budget = 1.0;
        p.attackers.clear();
        p.life = p.life.wrapping_add(1);
        p.protected_until = protected_until;
        p.history.clear();
        p.move_budget = 1.0;
        p.last_damage = -100.0;
        p.progress = 0.0;
        let life = p.life;
        if let Some(b) = p.bot.as_mut() {
            b.on_spawn();
        }
        self.send(
            id,
            S2C::Spawned {
                pos: pos.to_array(),
                yaw,
                life,
                loadout: l,
            },
        );
    }
}
