//! Messages from the server, chat and the connection itself.

use super::*;

impl GameClient {
    /// Seconds since the server last said anything.
    pub fn net_silence(&self) -> f32 {
        self.net.last_recv.elapsed().as_secs_f32()
    }

    /// The server has gone quiet: we stand where we are (nobody else moves either) and
    /// wait for it, up to [`NET_GIVE_UP`] seconds.
    pub fn net_lost(&self) -> bool {
        self.net_silence() > NET_LOST_AFTER
    }

    pub fn handle_msg(&mut self, m: S2C, audio: &mut Option<&Audio>) {
        let now = crate::clock::now();
        match m {
            S2C::Welcome { .. } => {}
            S2C::Reject(reason) => self.leave = Some(reason),
            S2C::MapChange {
                map,
                mode,
                settings,
            } => {
                self.load_map(map, mode, settings);
                self.marker = None;
                self.big_map = false;
                let name = self.map.id.name();
                self.center_msg(format!("{name} — {}", mode.name()), 4.0, crate::ui::ACCENT);
            }
            S2C::Snapshot(s) => self.on_snapshot(*s, audio),
            S2C::Scoreboard(rows) => self.scoreboard = rows,
            S2C::Spawned {
                pos,
                yaw,
                life,
                loadout,
            } => {
                self.body = Body {
                    pos: Vec3::from_array(pos),
                    vel: Vec3::ZERO,
                    on_ground: true,
                    stance: Stance::Stand,
                    ..Default::default()
                };
                // battle royale: out of the plane, falling
                let p = Vec3::from_array(pos);
                let ground = self
                    .world
                    .floor_below(p, 400.0)
                    .into_iter()
                    .chain(self.world.ground(p.x, p.z))
                    .fold(f32::MIN, f32::max);
                if self.mode == Mode::Br && p.y > ground + 20.0 {
                    self.body.chute = toon_shared::physics::Chute::FreeFall;
                    self.body.on_ground = false;
                }
                self.cam_smooth = Vec3::ZERO;
                self.want_stance = Stance::Stand;
                self.yaw = yaw;
                self.pitch = 0.0;
                self.life = life;
                self.alive = true;
                self.loadout = loadout;
                self.weapons = [
                    (
                        loadout.primary,
                        self.tuning.weapon(loadout.primary).mag,
                        loadout.reserve_for(loadout.primary, &self.tuning),
                    ),
                    (
                        loadout.secondary,
                        self.tuning.weapon(loadout.secondary).mag,
                        loadout.reserve_for(loadout.secondary, &self.tuning),
                    ),
                ];
                self.slot = 0;
                self.grenades = loadout.grenades();
                self.smokes = loadout.tacticals();
                self.kit = loadout;
                self.reload_until = None;
                self.switch_until = now + 0.3;
                self.switch_dur = 0.3;
                self.ads_t = 0.0;
                self.shot_streak = 0;
                self.recoil_accum = 0.0;
                self.protection_dropped = false;
                self.stamina = self.sprint_max();
                self.cooking = None;
                self.death_info = None;
                self.has_class = true;
                if !self.killcam.as_ref().is_some_and(|k| k.final_cam) {
                    self.killcam = None;
                }
                self.pending_killcam = None;
                self.reload_cue = 0;
                self.spawn_time = now;
                if matches!(self.overlay, Overlay::Class | Overlay::Team) {
                    self.overlay = Overlay::None;
                }
            }
            S2C::TeamSet(t) => {
                self.team = t;
                if t.is_playing() && self.mode == Mode::Br && !self.has_class {
                    // no classes in the battle royale (you find your weapons): pick who you
                    // are instead
                    self.choose_class(DEFAULT_CLASSES[0].1);
                    // (the lobby has the character chooser)
                    self.overlay = Overlay::None;
                } else if t.is_playing() {
                    if !self.has_class {
                        self.overlay = Overlay::Class;
                    }
                } else {
                    self.alive = false;
                    if self.overlay == Overlay::Team {
                        self.overlay = Overlay::None;
                    }
                }
            }
            S2C::Correction { pos, life } => {
                if life == self.life {
                    let new = Vec3::from_array(pos);
                    // small corrections glide instead of teleporting the view
                    let err = self.body.pos - new;
                    if err.length() < 2.0 {
                        self.cam_smooth += err;
                    }
                    self.body.pos = new;
                    self.body.vel = Vec3::ZERO;
                }
            }
            S2C::Kill {
                killer,
                victim,
                cause,
                headshot,
                killer_name,
                victim_name,
                killer_team,
                victim_team,
                dist,
                gear,
            } => {
                let st = self.server_now();
                self.recorder.event(
                    st,
                    Event::Kill {
                        killer,
                        victim,
                        cause,
                        headshot,
                        gear,
                    },
                );
                if let Some(k) = killer {
                    if self.state == MatchState::Playing {
                        self.highlights.on_kill(
                            st,
                            KillInfo {
                                killer: k,
                                victim,
                                killer_name: killer_name.clone(),
                                victim_name: victim_name.clone(),
                                killer_team,
                                cause,
                                headshot,
                                gear,
                            },
                            dist,
                        );
                    }
                }
                let blow = DeathBlow {
                    cause,
                    headshot,
                    from: self.blow_source(killer, cause),
                };
                self.blows.insert(victim, (blow, now));
                if victim == self.my_id {
                    let was_alive = self.alive;
                    self.alive = false;
                    self.death_time = now;
                    self.death_pos = self.eye();
                    self.spec_pos = self.death_pos + vec3(0.0, 4.0, 0.0);
                    self.death_info =
                        Some((killer_name.clone(), cause, killer.filter(|k| *k != victim)));
                    self.cooking = None;
                    if was_alive && self.team.is_playing() {
                        let r = Ragdoll::new(
                            self.body.pos,
                            self.yaw,
                            self.body.stance,
                            self.body.vel,
                            self.team,
                            blow,
                            &mut self.rng,
                        );
                        self.add_ragdoll(victim, r);
                    }
                    // killcam a moment later, once the replay has the whole kill
                    if let Some(k) = killer.filter(|k| *k != victim && has_killcam(cause)) {
                        self.pending_killcam = Some((
                            now + KILLCAM_DELAY,
                            KillInfo {
                                killer: k,
                                victim,
                                killer_name: killer_name.clone(),
                                victim_name: victim_name.clone(),
                                killer_team,
                                cause,
                                headshot,
                                gear,
                            },
                            false,
                        ));
                    }
                }
                if killer == Some(self.my_id) && victim != self.my_id {
                    if let Some(a) = audio {
                        a.play(Sfx::Kill, 0.6);
                    }
                }
                self.killfeed.push(KillLine {
                    killer: killer_name,
                    victim: victim_name,
                    kteam: killer_team,
                    vteam: victim_team,
                    cause,
                    headshot,
                    time: now,
                    kme: killer == Some(self.my_id),
                    vme: victim == self.my_id,
                    dist,
                });
                if self.killfeed.len() > 6 {
                    self.killfeed.remove(0);
                }
            }
            S2C::HitConfirm { kill, headshot } => {
                self.hitmarker = now;
                self.hit_kill = kill;
                self.hit_head = headshot;
                if let Some(a) = audio {
                    a.play(Sfx::Hit, 0.55);
                    if headshot {
                        a.play(Sfx::HitHead, 0.55);
                    }
                }
            }
            S2C::Flashed(k) => {
                let k = k.clamp(0.0, 1.0);
                self.flashed = (now + 0.6 + 3.5 * k as f64, k);
                if let Some(a) = audio {
                    a.play(Sfx::Explosion, 0.6 * k);
                }
            }
            S2C::Damaged { from, amount } => {
                self.dmg_dirs.push((Vec3::from_array(from), now));
                self.dmg_flash = now;
                self.shake = (self.shake + amount * 0.01).min(0.6);
                if let Some(a) = audio {
                    a.play(Sfx::Hurt, 0.5);
                }
            }
            S2C::Explosion { pos, radius, big } => {
                let p = Vec3::from_array(pos);
                self.recorder
                    .event(self.server_now(), Event::Explosion { pos: p, radius });
                self.explosions.push((p, now));
                self.fx.explosion(p, radius);
                let d = p.distance(self.eye());
                self.shake = (self.shake
                    + (1.0 - d / (radius * 4.0)).max(0.0) * if big { 1.0 } else { 0.6 })
                .min(1.2);
                if let Some(a) = audio {
                    a.play_at(Sfx::Explosion, p, self.camera().0, 1.0, 260.0);
                }
            }
            S2C::Blast(b) => {
                let now = crate::clock::now();
                let ear = self.camera().0;
                self.ruins.blast(
                    b,
                    &mut self.map,
                    &mut self.world,
                    &mut self.level,
                    &mut self.fx,
                    *audio,
                    ear,
                    now,
                );
            }
            S2C::Blasts(list) => {
                self.ruins
                    .replay(list, &mut self.map, &mut self.world, &mut self.level);
            }
            S2C::Melee { id } => {
                self.recorder.event(self.server_now(), Event::Melee(id));
                if let Some(r) = self.remotes.get_mut(&id) {
                    r.melee_t = now;
                    if let Some(a) = audio {
                        a.play_at(Sfx::Knife, r.pos, self.body.pos, 0.8, 25.0);
                    }
                }
            }
            S2C::Doors(open) => {
                for k in 0..self.world.doors.len() {
                    let state = open.iter().find(|o| o.0 as usize == k).map_or(0, |o| o.1);
                    self.world.set_door_state(k, state);
                }
            }
            S2C::Leaderboards(boards) => {
                self.leaderboards = Some(boards);
            }
            S2C::Door(k, open) => {
                let k = k as usize;
                if k < self.world.doors.len() && self.world.door_state(k) != open {
                    self.world.set_door_state(k, open);
                    let eye = self.eye();
                    let c = self.world.doors[k].centre();
                    if let Some(a) = audio.filter(|_| c.distance(eye) < 30.0) {
                        let s = if open != 0 { Sfx::Door } else { Sfx::DoorShut };
                        a.play_at(s, c, eye, 0.8, 30.0);
                    }
                }
            }
            S2C::Ents { add, remove } => {
                for id in remove {
                    self.lying.remove(&id);
                }
                for e in add {
                    self.lying.insert(e.id(), e);
                }
            }
            S2C::ScorePopup { points, text } => {
                self.popups.push((format!("+{points} {text}"), now));
            }
            S2C::Chat {
                from,
                team,
                text,
                team_only,
            } => {
                let line = if from.is_empty() {
                    text
                } else if team_only {
                    tr!("(команда) {from}: {text}", "(team) {from}: {text}")
                } else {
                    format!("{from}: {text}")
                };
                let color = if from.is_empty() {
                    crate::ui::MUTED
                } else if from == "Адмін" || from == "Admin" {
                    crate::ui::ACCENT
                } else {
                    self.tcol(team)
                };
                self.push_chat(line, color);
            }
            S2C::Center { text, secs } => self.center_msg(text, secs as f64, crate::ui::TEXT),
            S2C::StreakReady(k) => {
                self.center_msg(
                    tr!(
                        "{} готовий! Натисни {}",
                        "{} ready! Press {}",
                        k.name(),
                        binds::key(Action::Killstreak)
                    ),
                    3.0,
                    crate::ui::ACCENT,
                );
                self.streak_hint = Some(k);
                if let Some(a) = audio {
                    a.play(Sfx::Streak, 0.7);
                }
            }
            S2C::StreakUsed { by, team, streak } => {
                let friendly = team == self.team;
                let text = if friendly {
                    tr!("{by}: {} — дружній!", "{by}: {} — friendly!", streak.name())
                } else {
                    tr!("Ворожий {}!", "Enemy {}!", streak.name().to_lowercase())
                };
                self.center_msg(
                    text,
                    2.5,
                    if friendly {
                        crate::ui::GOOD
                    } else {
                        crate::ui::BAD
                    },
                );
            }
            S2C::Artillery { target, radius } => {
                let t = Vec3::from_array(target);
                self.artillery.push((t, radius, now));
                // shells whistle in over a few seconds
                if let Some(a) = audio {
                    let d = t.distance(self.eye());
                    let v = (1.0 - d / 140.0).clamp(0.25, 1.0);
                    for k in 0..6 {
                        a.play_later(Sfx::Whistle, 2.2 + k as f32 * 0.7, v * 0.8);
                    }
                }
            }
            S2C::Airstrike { from, to } => {
                self.airstrikes
                    .push((Vec3::from_array(from), Vec3::from_array(to), now));
                if let Some(a) = audio {
                    a.play(Sfx::Jet, 0.9);
                }
            }
            S2C::MatchSummary(sum) => {
                self.summary = Some((sum, now));
                self.reel = 0;
            }
            S2C::MatchEnd { winner, text } => {
                self.match_end = Some((text, winner, now));
                self.queue_final_killcam(now);
            }
            S2C::RoundEnd { winner, text } => {
                self.queue_final_killcam(now);
                let c = match winner {
                    Some(t) if t == self.team => crate::ui::GOOD,
                    Some(_) => crate::ui::BAD,
                    None => crate::ui::TEXT,
                };
                self.center_msg(text, 5.0, c);
            }
            S2C::BombEvent { text } => {
                self.center_msg(text, 2.5, crate::ui::ACCENT);
                if let Some(a) = audio {
                    a.play(Sfx::Bomb, 0.6);
                }
            }
            S2C::RconReply(text) => {
                for l in text.lines() {
                    self.push_chat(format!("[rcon] {l}"), crate::ui::ACCENT);
                }
            }
            S2C::Ping(ms) => self.net.send_unreliable(C2S::Pong(ms)),
            S2C::Tuning(mut t) => {
                t.finalize();
                if (self.tuning.movement != t.movement || self.tuning.weapons != t.weapons)
                    && self.have_clock
                {
                    self.push_chat(
                        toon_shared::lang::t(
                            "Сервер оновив налаштування руху/зброї",
                            "The server changed movement/weapon settings",
                        )
                        .into(),
                        crate::ui::MUTED,
                    );
                }
                self.tuning = *t;
            }
        }
    }

    pub fn send_chat(&mut self, text: String, team_only: bool) {
        let t = text.trim().to_string();
        if t.is_empty() {
            return;
        }
        if let Some(rest) = t.strip_prefix("/rcon") {
            let rest = rest.trim();
            if let Some(p) = rest.strip_prefix("login") {
                self.rcon_pass = p.trim().to_string();
                self.push_chat(
                    toon_shared::lang::t(
                        "[rcon] пароль збережено — тепер: /rcon <команда>",
                        "[rcon] password saved — now: /rcon <command>",
                    )
                    .into(),
                    crate::ui::ACCENT,
                );
            } else if self.rcon_pass.is_empty() {
                self.push_chat(
                    toon_shared::lang::t(
                        "[rcon] спершу: /rcon login <пароль>",
                        "[rcon] first: /rcon login <password>",
                    )
                    .into(),
                    crate::ui::BAD,
                );
            } else {
                let cmd = if rest.is_empty() {
                    "help".to_string()
                } else {
                    rest.to_string()
                };
                self.rcon_seq += 1;
                let proof = rcon_proof(&self.rcon_pass, self.rcon_nonce, self.rcon_seq, &cmd);
                self.net.send(C2S::Rcon {
                    seq: self.rcon_seq,
                    proof,
                    cmd,
                });
            }
            return;
        }
        self.net.send(C2S::Chat { text: t, team_only });
    }

    pub fn join_team(&mut self, t: Option<Team>) {
        self.net.send(C2S::JoinTeam(t));
        self.overlay = if t == Some(Team::Spectator) || self.mode == Mode::Br {
            Overlay::None
        } else {
            Overlay::Class
        };
    }

    pub fn disconnect(&mut self) {
        self.net.disconnect();
    }
}
