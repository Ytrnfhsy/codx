//! Snapshots: the clock against the server's, other players, cars, ragdolls.

use super::*;

impl GameClient {
    /// Server time we render remote players at.
    pub fn render_time(&self) -> f64 {
        self.now + self.clock_offset - INTERP_DELAY as f64
    }

    pub(super) fn on_snapshot(&mut self, s: Snapshot, audio: &mut Option<&Audio>) {
        if s.tick <= self.latest_tick && self.latest_tick - s.tick < 1000 {
            return;
        }
        self.latest_tick = s.tick;
        let st = s.tick as f64 * TICK_DT as f64;
        let target = st - self.now;
        if !self.have_clock || (target - self.clock_offset).abs() > 0.25 {
            self.clock_offset = target;
            self.have_clock = true;
        } else {
            self.clock_offset += (target - self.clock_offset) * 0.05;
        }
        if s.state != self.state {
            self.prev_state = self.state;
            if s.state == MatchState::Playing && self.state == MatchState::PreRound {
                self.match_end = None;
            }
        }
        // what lies on the ground comes separately (`S2C::Ents`)
        let mut s = s;
        s.ents.extend(self.lying.values().cloned());
        self.recorder.push(Frame {
            t: st,
            players: s.players.clone(),
            ents: s.ents.clone(),
            shots: s.shots.clone(),
            bomb: s.bomb.clone(),
        });
        if self.state != s.state {
            // a new match: the last one's moments and results go
            if matches!(self.state, MatchState::PostMatch | MatchState::Warmup)
                && matches!(s.state, MatchState::PreRound | MatchState::Playing)
            {
                self.highlights.clear();
                self.summary = None;
                self.reel = 0;
                self.match_t0 = self.now;
            }
        }
        self.state = s.state;
        self.time_left = s.time_left;
        self.scores = s.scores;
        self.overtime = s.overtime;

        let mut seen = Vec::new();
        for p in &s.players {
            crate::soldier::set_skin(p.id, p.skin);
        }
        for p in s.players {
            if p.id == self.my_id {
                self.team = p.team;
                self.squad = p.squad;
                continue;
            }
            seen.push(p.id);
            let r = self.remotes.entry(p.id).or_insert_with(|| Remote {
                buf: VecDeque::new(),
                team: p.team,
                pos: Vec3::from_array(p.pos),
                yaw: p.yaw,
                pitch: p.pitch,
                stance: Stance::from_u8(p.stance & 3),
                weapon: p.weapon,
                flags: p.flags,
                silent: false,
                chute: 0,
                protected: false,
                squad: p.squad,
                downed: false,
                alive: p.alive,
                phase: 0.0,
                walk: 0.0,
                sprint: 0.0,
                death_time: None,
                muzzle: Vec3::from_array(p.pos),
                melee_t: -10.0,
                step_acc: 0.0,
                reload_since: None,
                vel: Vec3::ZERO,
            });
            r.buf.push_back((st, p));
            while r.buf.len() > 40 {
                r.buf.pop_front();
            }
        }
        self.remotes.retain(|id, _| seen.contains(id));

        // our own state
        let me = s.me;
        let now = self.now;
        if !me.alive && self.alive {
            self.alive = false;
            self.death_time = now;
            self.death_pos = self.eye();
        }
        let quiet = now - self.last_fire > 0.5
            && self.reload_until.is_none()
            && now > self.switch_until + 0.3;
        // weapons picked up from the ground (the server is the authority)
        if me.alive && self.alive && now - self.spawn_time > 0.6 {
            for i in 0..2 {
                if me.weapons[i] != self.weapons[i].0 {
                    self.weapons[i] = (me.weapons[i], me.ammo[i].0, me.ammo[i].1);
                    if i == self.slot as usize {
                        let swap = self.tuning.weapon(me.weapons[i]).swap_time;
                        self.reload_until = None;
                        self.switch_until = now + swap as f64;
                        self.switch_dur = swap;
                        self.ads_t = 0.0;
                        self.shot_streak = 0;
                        if let Some(a) = audio {
                            a.play(Sfx::Raise, 0.5);
                        }
                    }
                }
            }
        }
        if quiet && self.alive {
            for i in 0..2 {
                self.weapons[i].1 = me.ammo[i].0;
                self.weapons[i].2 = me.ammo[i].1;
            }
            if me.slot != self.slot {
                self.slot = me.slot;
            }
        }
        if now - self.last_throw > 0.6 {
            self.grenades = me.grenades;
            self.smokes = me.smokes;
        }
        self.sync_piloting(if self.alive { me.vehicle.clone() } else { None });
        self.me = me;
        self.bomb = s.bomb;
        if s.br.as_ref().is_some_and(|b| b.in_plane) && self.br.as_ref().is_none_or(|b| !b.in_plane)
        {
            // boarding: look along the plane's course and down at the island
            if let Some((_, yaw)) = s.br.as_ref().and_then(|b| b.plane) {
                self.yaw = yaw;
                self.pitch = -0.7;
            }
        }
        self.br = s.br;
        self.ready = s.ready;
        // somebody on our side pointed something out: a note and a line in the chat
        let mut heard = false;
        for p in &s.pings {
            // somebody said «I need that» on a ping
            let before = self
                .pings
                .iter()
                .find(|o| o.id == p.id)
                .and_then(|o| o.claimed);
            if let Some(c) = p
                .claimed
                .filter(|c| Some(*c) != before && self.seen_pings.contains(&p.id))
            {
                heard = true;
                let who = self.name_of(c).unwrap_or("?").to_string();
                let col = self.ping_color(p);
                self.push_chat(
                    tr!(
                        "{who}: мені потрібно — {}",
                        "{who}: I need that — {}",
                        ping_label(p.what)
                    ),
                    col,
                );
            }
            if self.seen_pings.insert(p.id) && p.age < 1.5 {
                heard = true;
                if p.from != self.my_id {
                    let who = self.name_of(p.from).unwrap_or("?").to_string();
                    let c = self.ping_color(p);
                    self.push_chat(format!("{who}: {}", ping_label(p.what)), c);
                }
            }
        }
        if heard {
            if let Some(a) = audio {
                a.play(Sfx::Ping, 0.5);
            }
        }
        self.seen_pings
            .retain(|id| s.pings.iter().any(|p| p.id == *id));
        self.pings = s.pings;
        self.lobby = s.lobby;
        let w = self.br.as_ref().map_or(Weather::Clear, |b| b.weather);
        if w != self.weather {
            self.weather = w;
            self.sky_mesh = build_sky_in(&self.map, w);
            self.sun_mesh = build_sun(w);
            if w != Weather::Clear {
                self.center_msg(
                    tr!("Погода: {}", "Weather: {}", w.name()),
                    3.0,
                    crate::ui::TEXT,
                );
            }
        }
        // cars: the latest of each
        let now = self.now;
        let mut seen_cars = Vec::new();
        for e in &s.ents {
            if let EntSnap::Car {
                id,
                kind,
                pos,
                yaw,
                speed,
                hp,
                seats,
            } = e
            {
                seen_cars.push(*id);
                let target = toon_shared::cars::CarState {
                    pos: Vec3::from_array(*pos),
                    yaw: *yaw,
                    speed: *speed,
                    ..Default::default()
                };
                let c = self.cars.entry(*id).or_insert(CarView {
                    kind: *kind,
                    state: target,
                    target,
                    at: now,
                    hp: *hp,
                    seats: *seats,
                });
                c.target = target;
                c.at = now;
                c.hp = *hp;
                c.seats = *seats;
            }
        }
        self.cars.retain(|id, _| seen_cars.contains(id));
        self.ents = s.ents;
        let mut present = Vec::new();
        for e in &self.ents {
            match e {
                EntSnap::Weapon { id, .. } => {
                    self.pickup_seen.entry(*id).or_insert(now);
                    present.push(*id);
                }
                // a smoke cloud just popped: the hiss
                EntSnap::Smoke { id, pos, .. } => {
                    if !self.pickup_seen.contains_key(id) {
                        self.pickup_seen.insert(*id, now);
                        if let Some(a) = audio.as_ref() {
                            a.play_at(Sfx::Smoke, Vec3::from_array(*pos), self.eye(), 0.8, 50.0);
                        }
                    }
                    present.push(*id);
                }
                _ => {}
            }
        }
        self.pickup_seen.retain(|id, _| present.contains(id));

        // effects for other players' shots
        let eye = self.eye();
        for shot in s.shots {
            if shot.id == self.my_id {
                continue;
            }
            let to = Vec3::from_array(shot.to);
            let from = if shot.id == 255 {
                Vec3::from_array(shot.from)
            } else {
                self.remotes
                    .get(&shot.id)
                    .map(|r| r.muzzle)
                    .unwrap_or(Vec3::from_array(shot.from))
            };
            let path = to - from;
            let len = path.length();
            let kind = shot.weapon.shot();
            if shot.id != 255 && kind != toon_shared::weapons::Shot::Bullet {
                let speed = toon_shared::ballistics::Ballistics::of(shot.weapon).velocity;
                let rocket = kind == toon_shared::weapons::Shot::Rocket;
                self.fx.fly(from, to, speed, rocket, shot.flesh);
                if let Some(a) = audio {
                    a.play_shot(
                        Sfx::for_weapon(shot.weapon),
                        from,
                        eye,
                        if shot.quiet { 0.3 } else { 0.75 },
                    );
                }
                continue;
            }
            self.fx.tracer(from, to);
            if len > 0.1 {
                let (_, _, pierced) = self.world.bullet_trace(from, path / len, len);
                for p in &pierced {
                    self.fx.pierce(
                        from + path / len * p.t,
                        p.mat == toon_shared::map::Mat::Wood,
                    );
                }
            }
            if shot.flesh {
                self.fx.flesh(to);
            } else if len > 0.1 {
                self.bullet_hit(to, path / len);
            }
            // (a suppressor: no flash, and hardly heard)
            if shot.id != 255 && !shot.quiet {
                self.fx.muzzle(from);
            }
            if let Some(a) = audio {
                let sfx = if shot.id == 255 {
                    Sfx::HeliGun
                } else {
                    Sfx::for_weapon(shot.weapon)
                };
                a.play_shot(sfx, from, eye, if shot.quiet { 0.2 } else { 0.75 });
                if shot.flesh {
                    a.play_at(Sfx::Flesh, to, eye, 0.5, 30.0);
                }
                // bullets cracking past your head
                if self.alive {
                    let d = to - from;
                    let len = d.length();
                    if len > 1.0 {
                        let k = ((eye - from).dot(d) / (len * len)).clamp(0.0, 1.0);
                        let miss = (from + d * k).distance(eye);
                        if miss < 2.5 && k > 0.02 && k < 0.98 {
                            a.play(Sfx::Whiz, 0.75 * (1.0 - miss / 2.5) + 0.15);
                        }
                    }
                }
            }
        }
    }

    /// Server time matching the newest snapshot.
    pub fn server_now(&self) -> f64 {
        self.now + self.clock_offset
    }

    /// Where the blow that killed somebody came from.
    pub(super) fn blow_source(&self, killer: Option<u8>, cause: Cause) -> Option<Vec3> {
        match cause {
            Cause::Grenade | Cause::Airstrike | Cause::Bomb => self
                .explosions
                .iter()
                .rev()
                .find(|e| self.now - e.1 < 0.6)
                .map(|e| e.0),
            _ => match killer {
                Some(k) if k == self.my_id => Some(self.eye()),
                Some(k) => self.remotes.get(&k).map(|r| r.pos + vec3(0.0, 1.3, 0.0)),
                None => None,
            },
        }
    }

    pub(super) fn add_ragdoll(&mut self, id: u8, r: Ragdoll) {
        self.ragdolls.push((id, r));
        if self.ragdolls.len() > 24 {
            self.ragdolls.remove(0);
        }
    }

    /// Living remote players, interpolated for this frame (those in cars sit in them;
    /// in a car we see ourselves too, from behind).
    pub fn live_actors(&self) -> Vec<Actor> {
        let now = self.now;
        let mut out: Vec<Actor> = self.live_actors_raw();
        // (watching through somebody's eyes: their own head is not in the way)
        if self.br_out() {
            if let Some(t) = self.spectate {
                out.retain(|a| a.id != t);
            }
        }
        for a in out.iter_mut() {
            if let Some((pos, yaw)) = self.seated_at(a.id) {
                a.pos = pos;
                a.yaw = yaw;
                a.stance = Stance::Crouch;
                a.walk = 0.0;
                a.sprint = 0.0;
            }
        }
        if let Some((pos, yaw)) = self.seat().and_then(|_| self.seated_at(self.my_id)) {
            out.push(Actor {
                id: self.my_id,
                pos,
                yaw,
                pitch: self.pitch,
                stance: Stance::Crouch,
                team: self.team,
                weapon: self.weapon(),
                flags: 0,
                phase: 0.0,
                walk: 0.0,
                sprint: 0.0,
                melee: 0.0,
                reload: None,
                protected: false,
                chute: 0,
            });
        }
        let _ = now;
        out
    }

    fn live_actors_raw(&self) -> Vec<Actor> {
        let now = self.now;
        self.remotes
            .iter()
            .filter(|(_, r)| r.alive)
            .map(|(id, r)| Actor {
                id: *id,
                pos: r.pos,
                yaw: r.yaw,
                pitch: r.pitch,
                stance: r.stance,
                team: r.team,
                weapon: r.weapon,
                flags: r.flags,
                phase: r.phase,
                walk: r.walk,
                sprint: r.sprint,
                melee: if now - r.melee_t < 0.4 {
                    ((now - r.melee_t) as f32 / 0.4).clamp(0.0, 1.0)
                } else {
                    0.0
                },
                reload: r.reload_since.map(|t| (now - t) as f32),
                protected: r.protected,
                chute: r.chute,
            })
            .collect()
    }

    pub(super) fn update_remotes(&mut self, dt: f32, audio: Option<&Audio>) {
        let rt = self.render_time();
        let walk_speed = self.tuning.movement.walk_speed;
        let now = self.now;
        let listener = self.camera().0;
        let mut deaths = Vec::new();
        for (id, r) in self.remotes.iter_mut() {
            // find the two snapshots around rt
            let mut a = None;
            let mut b = None;
            for (i, (t, _)) in r.buf.iter().enumerate() {
                if *t <= rt {
                    a = Some(i);
                } else {
                    b = Some(i);
                    break;
                }
            }
            let (pa, pb, k) = match (a, b) {
                (Some(i), Some(j)) => {
                    let (ta, tb) = (r.buf[i].0, r.buf[j].0);
                    (i, j, ((rt - ta) / (tb - ta).max(1e-6)) as f32)
                }
                (Some(i), None) => (i, i, 0.0),
                (None, Some(j)) => (j, j, 0.0),
                (None, None) => continue,
            };
            let sa = &r.buf[pa].1;
            let sb = &r.buf[pb].1;
            let new_pos = Vec3::from_array(sa.pos).lerp(Vec3::from_array(sb.pos), k);
            let old_pos = r.pos;
            let moved = vec2(new_pos.x - r.pos.x, new_pos.z - r.pos.z).length();
            let teleport = moved > 4.0;
            r.pos = new_pos;
            r.yaw = sa.yaw + wrap_angle(sb.yaw - sa.yaw) * k;
            r.pitch = sa.pitch + (sb.pitch - sa.pitch) * k;
            r.stance = Stance::from_u8(sb.stance & 3);
            r.silent = sb.stance & stance_bits::SILENT != 0;
            r.chute = if sb.stance & stance_bits::CHUTE != 0 {
                2
            } else if sb.stance & stance_bits::FREEFALL != 0 {
                1
            } else {
                0
            };
            r.protected = sb.alive && sb.stance & stance_bits::PROTECTED != 0;
            r.downed = sb.alive && sb.stance & stance_bits::DOWNED != 0;
            r.squad = sb.squad;
            r.weapon = sb.weapon;
            r.flags = sb.flags;
            r.team = sb.team;
            let was_alive = r.alive;
            r.alive = sb.alive;
            if !teleport && dt > 0.0 {
                let v = ((new_pos - old_pos) / dt).clamp_length_max(9.0);
                r.vel += (v - r.vel) * (dt * 12.0).min(1.0);
            }
            if was_alive && !r.alive {
                r.death_time = Some(now);
                deaths.push((*id, r.pos, r.yaw, r.stance, r.vel, r.team, r.weapon));
            }
            match (r.flags & flags::RELOADING != 0 && r.alive, r.reload_since) {
                (true, None) => r.reload_since = Some(now),
                (false, Some(_)) => r.reload_since = None,
                _ => {}
            }
            if r.alive {
                r.death_time = None;
            }
            if !teleport && dt > 0.0 {
                let speed = moved / dt;
                r.walk += ((speed / walk_speed).min(1.4) - r.walk) * (dt * 6.0).min(1.0);
                let sprint = if r.flags & flags::SPRINT != 0 {
                    1.0
                } else {
                    0.0
                };
                r.sprint += (sprint - r.sprint) * (dt * 7.0).min(1.0);
                r.phase += moved * 2.2;
                if r.alive
                    && !r.silent
                    && r.flags & flags::ON_GROUND != 0
                    && r.stance == Stance::Stand
                    && speed > 2.0
                {
                    r.step_acc += moved;
                    if r.step_acc > 2.2 {
                        r.step_acc = 0.0;
                        if let Some(a) = audio {
                            // running is heard further than walking
                            let step = step_sfx(&self.world, r.pos);
                            let (vol, range) = if r.sprint > 0.5 {
                                (0.45, 32.0)
                            } else {
                                (0.32, 20.0)
                            };
                            a.play_at(step, r.pos, listener, vol, range);
                        }
                    }
                }
            }
            // drop very old samples
            while r.buf.len() > 2 && r.buf[1].0 < rt - 0.5 {
                r.buf.pop_front();
            }
        }
        for (id, pos, yaw, stance, vel, team, weapon) in deaths {
            let blow = self.blows.get(&id).map(|b| b.0).unwrap_or(DeathBlow {
                cause: Cause::World,
                headshot: false,
                from: None,
            });
            let _ = weapon;
            let r = Ragdoll::new(pos, yaw, stance, vel, team, blow, &mut self.rng);
            self.add_ragdoll(id, r);
        }
    }

    /// Follows the server's cars smoothly (our own one a little ahead, for the delay).
    pub(super) fn update_cars(&mut self, dt: f32) {
        let now = self.now;
        let mine = self.me.seat.map(|s| s.0);
        // (our own car: about the one-way delay ahead)
        let lead = 0.06;
        let k = 1.0 - (-14.0 * dt).exp();
        for (id, c) in self.cars.iter_mut() {
            let ahead = (now - c.at) + if Some(*id) == mine { lead } else { 0.0 };
            let ahead = ahead.min(0.3) as f32;
            let t = &c.target;
            let want = t.pos + t.forward() * t.speed * ahead;
            if c.state.pos.distance(want) > 8.0 {
                c.state.pos = want;
                c.state.yaw = t.yaw;
            } else {
                c.state.pos = c.state.pos.lerp(want, k);
                c.state.yaw = wrap_angle(c.state.yaw + wrap_angle(t.yaw - c.state.yaw) * k);
            }
            c.state.speed = t.speed;
        }
    }
}
