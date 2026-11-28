//! Warm-up, rounds, the end of a match and the next map.

use super::*;

impl Game {
    // ---------------------------------------------------------------- match flow

    fn ready_to_start(&self) -> bool {
        let playing = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.team.is_playing())
            .count();
        // a battle royale round ends as soon as no real player is in it, so one only
        // starts with a real player on board (bots alone, or bots watched by a spectator,
        // would start and end it over and over); other modes run for whoever watches
        let humans = if self.mode == Mode::Br {
            self.humans_playing()
        } else {
            self.players.iter().flatten().any(|p| !p.is_bot())
        };
        self.can_start(playing, humans)
            && (!self.cfg.ready_check
                || self.unready() == 0
                || self.ready_deadline.is_some_and(|t| self.time >= t))
    }

    fn can_start(&self, playing: usize, humans: bool) -> bool {
        playing >= self.cfg.min_players
            && (self.mode.solo()
                || self.team_count(Team::Allies) > 0 && self.team_count(Team::Axis) > 0)
            && humans
    }

    /// When the match starts whether or not everybody pressed «Ready»: the countdown
    /// begins once the match could start, shorter as soon as somebody is ready.
    fn update_ready_deadline(&mut self) {
        let timeout = self.cfg.ready_timeout as f64;
        // (the battle royale lobby simply waits for everybody to be ready)
        if self.state != MatchState::Warmup
            || !self.cfg.ready_check
            || timeout <= 0.0
            || self.mode == Mode::Br
        {
            self.ready_deadline = None;
            return;
        }
        let playing = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.team.is_playing())
            .count();
        let humans = if self.mode == Mode::Br {
            self.humans_playing()
        } else {
            self.players.iter().flatten().any(|p| !p.is_bot())
        };
        if !self.can_start(playing, humans) {
            self.ready_deadline = None;
            return;
        }
        let someone_ready = self
            .players
            .iter()
            .flatten()
            .any(|p| !p.is_bot() && p.team.is_playing() && p.ready);
        let wait = if someone_ready {
            timeout
        } else {
            timeout * 2.0
        };
        let latest = self.time + wait;
        self.ready_deadline = Some(match self.ready_deadline {
            Some(t) => t.min(latest),
            None => latest,
        });
    }

    /// What side a player is on, as the admin sees it: the team, or in the battle royale
    /// playing (in which squad) or watching.
    pub fn side_name(&self, p: &Player) -> String {
        if self.mode != Mode::Br || !p.team.is_playing() {
            return p.team.name().to_string();
        }
        if p.squad != 0 {
            toon_shared::tr!("загін {}", "squad {}", p.squad)
        } else {
            t("грає", "plays").to_string()
        }
    }

    /// A real player has joined a side (not just watching).
    pub fn humans_playing(&self) -> bool {
        self.players
            .iter()
            .flatten()
            .any(|p| !p.is_bot() && p.team.is_playing())
    }

    /// On the same side: the same team, or in the battle royale the same squad.
    pub fn allied(&self, a: &Player, b: &Player) -> bool {
        if self.mode.solo() {
            a.squad != 0 && a.squad == b.squad
        } else {
            a.team == b.team
        }
    }

    /// `allied` by ids (false for anybody missing).
    pub fn allied_ids(&self, a: u8, b: u8) -> bool {
        match (self.p(a), self.p(b)) {
            (Some(a), Some(b)) => self.allied(a, b),
            _ => false,
        }
    }

    /// Real players in the game who have not pressed «Ready» yet.
    pub fn unready(&self) -> usize {
        self.players
            .iter()
            .flatten()
            .filter(|p| !p.is_bot() && p.team.is_playing() && !p.ready)
            .count()
    }

    pub fn start_match(&mut self) {
        self.scores = [0, 0];
        self.round = 0;
        self.match_started = true;
        for p in self.players.iter_mut().flatten() {
            p.br_longest = 0.0;
            p.ms = MatchStats {
                fired_at_start: p.shots_fired,
                hit_at_start: p.shots_hit,
                ..Default::default()
            };
            p.kills = 0;
            p.deaths = 0;
            p.assists = 0;
            p.score = 0;
            p.streak = 0;
            p.rewards.clear();
        }
        self.start_round(PREROUND_FIRST);
    }

    fn start_round(&mut self, pre: f64) {
        self.round += 1;
        self.mend_map();
        self.state = MatchState::PreRound;
        self.state_end = self.time + pre;
        self.overtime = false;
        self.grenades.clear();
        self.fires.clear();
        self.bullets.clear();
        self.lobby_spots = None;
        self.base_spots = [None, None];
        self.smokes.clear();
        self.vehicles.clear();
        self.cars.clear();
        self.drops.clear();
        for p in self.players.iter_mut().flatten() {
            p.driving = None;
        }
        self.helis.clear();
        self.pickups.clear();
        self.blasts.clear();
        self.uav_until = [0.0; 2];
        self.reset_bomb();
        for id in self.ids() {
            if let Some(p) = self.pm(id) {
                p.alive = false;
                p.respawn_at = 0.0;
                p.progress = 0.0;
            }
            if self.can_spawn(id) {
                self.spawn_player(id, true);
            }
        }
        let (mode, map, round) = (self.mode, self.map.id, self.round);
        self.center_all(
            || match mode {
                Mode::Tdm | Mode::Br => format!("{} — {}", mode.name(), map.name()),
                Mode::Sab => tr!("{} — раунд {round}", "{} — round {round}", mode.name()),
            },
            pre as f32,
        );
    }

    pub fn reset_bomb(&mut self) {
        self.sab.pos = self.map.bomb;
        self.sab.carrier = None;
        self.sab.owner = None;
        self.sab.planted_at = None;
        self.sab.plant = None;
        self.sab.defuse = None;
        self.sab.dropped_since = self.time;
    }

    /// Ends the match with a free-form text (the battle royale's winner).
    pub fn end_match_text(&mut self, text: impl Fn() -> String) {
        if self.state == MatchState::PostMatch {
            return;
        }
        self.state = MatchState::PostMatch;
        self.state_end = self.time + self.cfg.postmatch_time as f64;
        self.logf(format!("Матч завершено: {}", text()));
        self.say(Dest::All, || S2C::MatchEnd {
            winner: None,
            text: text(),
        });
    }

    pub fn end_match(&mut self, winner: Option<Team>, reason: Text) {
        if self.state == MatchState::PostMatch {
            return;
        }
        self.state = MatchState::PostMatch;
        self.state_end = self.time + self.cfg.postmatch_time as f64;
        let text = || match winner {
            Some(t) => tr!("{} перемагають! {}", "{} win! {}", t.name(), pick(reason)),
            None => tr!("Нічия! {}", "Draw! {}", pick(reason)),
        };
        self.logf(format!(
            "Матч завершено: {} ({}:{})",
            text(),
            self.scores[0],
            self.scores[1]
        ));
        self.say(Dest::All, || S2C::MatchEnd {
            winner,
            text: text(),
        });
        // the match goes into everybody's records (the battle royale records its own, as
        // each player is out)
        if self.mode != Mode::Br {
            let rows: Vec<(String, String, bool, u32, u32, f32)> = self
                .players
                .iter()
                .flatten()
                .filter(|p| !p.is_bot() && p.team.is_playing())
                .map(|p| {
                    (
                        p.stats_key(),
                        p.name.clone(),
                        winner == Some(p.team),
                        p.kills as u32,
                        p.deaths as u32,
                        p.br_longest,
                    )
                })
                .collect();
            for (key, name, won, kills, deaths, longest) in rows {
                let f = self.registry.fight(&key);
                f.matches += 1;
                f.wins += won as u32;
                let news = self
                    .br_stats
                    .record_match(&key, &name, won, kills, deaths, longest);
                self.announce(news);
            }
        }
        self.send_summary();
    }

    /// Leaderboard news in everybody's chat.
    pub fn announce(&mut self, news: Vec<crate::stats::News>) {
        for n in news {
            self.logf(n.0.clone());
            self.say(Dest::All, || S2C::Chat {
                from: String::new(),
                team: Team::Spectator,
                text: n.0.clone(),
                team_only: false,
            });
        }
    }

    pub fn round_win(&mut self, winner: Option<Team>, reason: Text) {
        if self.state != MatchState::Playing {
            return;
        }
        if let Some(t) = winner {
            self.scores[t.index()] += 1;
        }
        let text = || match winner {
            Some(t) => tr!(
                "{} виграли раунд — {}",
                "{} win the round — {}",
                t.name(),
                pick(reason)
            ),
            None => tr!("Раунд нічийний — {}", "Round drawn — {}", pick(reason)),
        };
        self.logf(text());
        if let Some(t) = winner {
            if self.scores[t.index()] >= self.cfg.sab.rounds_to_win as i32 {
                self.end_match(Some(t), reason);
                return;
            }
        }
        self.state = MatchState::RoundEnd;
        self.state_end = self.time + ROUND_END_TIME;
        self.say(Dest::All, || S2C::RoundEnd {
            winner,
            text: text(),
        });
    }

    /// Plays on a map given ready-made (tests use the small fixed yard, which never
    /// changes when the real maps do).
    pub fn play_on(&mut self, map: Map, mode: Mode) {
        self.world = World::from_map(&map);
        self.wreck = toon_shared::destruct::Wreck::new(&map);
        self.nav = Nav::build(&map, &self.world);
        self.map = map;
        self.spare_map = None;
        self.change_map(self.map.id, mode);
    }

    /// Switches map/mode immediately (admin or rotation).
    pub fn change_map(&mut self, map: MapId, mode: Mode) {
        // building a big map (and its routes) takes seconds: the same map again keeps what
        // is built, and the one before is kept aside for a rotation that goes back to it
        // (the map put aside is kept whole)
        self.wreck.reset(&mut self.map, &mut self.world);
        if self.map.id != map || self.map.boxes.is_empty() {
            let built = match self.spare_map.take() {
                Some(b) if b.0.id == map => b,
                _ => {
                    let m = maps::build(map);
                    let w = World::from_map(&m);
                    let n = Nav::build(&m, &w);
                    (m, w, n)
                }
            };
            let old_map = std::mem::replace(&mut self.map, built.0);
            let old_world = std::mem::replace(&mut self.world, built.1);
            let old_nav = std::mem::replace(&mut self.nav, built.2);
            self.spare_map = Some((old_map, old_world, old_nav));
            self.wreck = toon_shared::destruct::Wreck::new(&self.map);
        }
        self.mode = mode;
        self.state = MatchState::Warmup;
        self.match_started = false;
        self.scores = [0, 0];
        self.overtime = false;
        self.grenades.clear();
        self.smokes.clear();
        self.vehicles.clear();
        self.drops.clear();
        for p in self.players.iter_mut().flatten() {
            p.driving = None;
        }
        self.helis.clear();
        self.pickups.clear();
        self.blasts.clear();
        self.reset_bomb();
        self.br = Default::default();
        self.intel.clear();
        self.pings.clear();
        for p in self.players.iter_mut().flatten() {
            p.in_plane = false;
            p.alive = false;
            p.respawn_at = 0.0;
            p.ms = MatchStats {
                fired_at_start: p.shots_fired,
                hit_at_start: p.shots_hit,
                ..Default::default()
            };
            p.kills = 0;
            p.deaths = 0;
            p.assists = 0;
            p.score = 0;
            p.streak = 0;
            p.rewards.clear();
            if let Some(b) = p.bot.as_mut() {
                b.on_spawn();
            }
        }
        let settings = self.settings();
        // (a new map: the clients forget what lay on the old one)
        for p in self.players.iter_mut().flatten() {
            p.lying.clear();
        }
        self.broadcast(S2C::MapChange {
            map,
            mode,
            settings,
        });
        self.logf(format!("Зміна карти: {} ({})", self.map.name, mode.name()));
        self.shut_doors();
        for id in self.ids() {
            if self.can_spawn(id) {
                self.spawn_player(id, true);
            }
        }
    }

    /// The match's best players to everybody, for the screen between matches.
    pub fn send_summary(&mut self) {
        use toon_shared::protocol::{Award, AwardKind, MatchSummary, MvpLine};
        let players: Vec<&Player> = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.team.is_playing())
            .collect();
        let mvp = players
            .iter()
            .filter(|p| p.score > 0 || p.kills > 0)
            .max_by_key(|p| (p.score, p.kills, std::cmp::Reverse(p.deaths)))
            .map(|p| MvpLine {
                id: p.id,
                name: p.name.clone(),
                team: p.team,
                kills: p.kills,
                deaths: p.deaths,
                score: p.score,
                weapon: p.loadout.primary,
            });
        // the best at each thing (above a bar worth showing); nobody gets more than two
        // while somebody else also earned it, so more players are named
        let mut given: Vec<u8> = Vec::new();
        let mut best = |kind: AwardKind, min: f32, f: &dyn Fn(&Player) -> f32| -> Option<Award> {
            let mut ranked: Vec<&&Player> = players.iter().filter(|p| f(p) >= min).collect();
            ranked.sort_by(|a, b| f(b).total_cmp(&f(a)).then(a.deaths.cmp(&b.deaths)));
            let p = ranked
                .iter()
                .find(|p| given.iter().filter(|g| **g == p.id).count() < 2)
                .or(ranked.first())?;
            given.push(p.id);
            Some(Award {
                kind,
                id: p.id,
                name: p.name.clone(),
                team: p.team,
                value: f(p),
                weapon: (kind == AwardKind::Longest)
                    .then_some(p.ms.longest_with)
                    .flatten(),
            })
        };
        let accuracy = |p: &Player| {
            let fired = p.shots_fired.saturating_sub(p.ms.fired_at_start);
            let hit = p.shots_hit.saturating_sub(p.ms.hit_at_start);
            if fired >= 25 {
                (hit as f32 / fired as f32 * 100.0).round()
            } else {
                0.0
            }
        };
        let awards: Vec<Award> = [
            best(AwardKind::Kills, 1.0, &|p| p.kills as f32),
            best(AwardKind::Longest, 20.0, &|p| p.ms.longest.round()),
            best(AwardKind::Streak, 3.0, &|p| p.ms.best_streak as f32),
            best(AwardKind::Accuracy, 1.0, &accuracy),
            best(AwardKind::Headshots, 1.0, &|p| p.ms.headshots as f32),
            best(AwardKind::Blasts, 1.0, &|p| p.ms.blast_kills as f32),
            best(AwardKind::Assists, 1.0, &|p| p.assists as f32),
            best(AwardKind::Arrows, 1.0, &|p| p.ms.arrow_kills as f32),
            best(AwardKind::Knife, 1.0, &|p| p.ms.knife_kills as f32),
            best(AwardKind::Plants, 1.0, &|p| p.ms.plants as f32),
            best(AwardKind::Defuses, 1.0, &|p| p.ms.defuses as f32),
        ]
        .into_iter()
        .flatten()
        .take(9)
        .collect();
        let (next_map, next_mode) = self.peek_next();
        let summary = MatchSummary {
            mvp,
            awards,
            next_map,
            next_mode,
            secs: self.cfg.postmatch_time,
        };
        self.broadcast(S2C::MatchSummary(summary));
    }

    /// The map and mode that come after this match.
    pub fn peek_next(&self) -> (MapId, Mode) {
        let i = (self.rotation_idx + 1) % self.cfg.rotation.len().max(1);
        self.cfg
            .rotation
            .get(i)
            .map(|r| r.resolve())
            .unwrap_or((self.map.id, self.mode))
    }

    pub fn next_map(&mut self) {
        self.rotation_idx = (self.rotation_idx + 1) % self.cfg.rotation.len();
        let (m, mode) = self.cfg.rotation[self.rotation_idx].resolve();
        self.change_map(m, mode);
    }

    pub(super) fn update_state(&mut self, dt: f32) {
        match self.state {
            MatchState::Warmup => {
                self.update_ready_deadline();
                if self.ready_to_start() {
                    self.start_match();
                }
            }
            MatchState::PreRound => {
                // (the last real player went to watch during the countdown: back to the
                // warm-up instead of a round with nobody to play it)
                if self.mode == Mode::Br && !self.humans_playing() {
                    self.state = MatchState::Warmup;
                    self.match_started = false;
                } else if self.time >= self.state_end {
                    self.state = MatchState::Playing;
                    let limit = match self.mode {
                        Mode::Tdm => self.cfg.tdm.time_limit,
                        Mode::Sab => self.cfg.sab.time_limit,
                        // the zone decides when it ends
                        Mode::Br => 1.0e6,
                    };
                    self.round_end = self.time + limit as f64 * 60.0;
                    if self.mode == Mode::Br {
                        self.br_start();
                    } else {
                        self.center_all(|| t("В БІЙ!", "FIGHT!").into(), 1.5);
                    }
                }
            }
            MatchState::Playing => {
                if self.mode == Mode::Sab && self.sab.planted_at.is_some() {
                    // the round clock stops while the bomb ticks
                    self.round_end += dt as f64;
                }
                if self.time >= self.round_end && !self.overtime {
                    match self.mode {
                        Mode::Br => {}
                        Mode::Tdm => {
                            let w = if self.scores[0] > self.scores[1] {
                                Some(Team::Allies)
                            } else if self.scores[1] > self.scores[0] {
                                Some(Team::Axis)
                            } else {
                                None
                            };
                            self.end_match(w, ("Час вийшов", "Time is up"));
                        }
                        Mode::Sab => {
                            self.overtime = true;
                            self.sab.overtime_end =
                                self.time + self.cfg.sab.overtime_limit as f64 * 60.0;
                            self.center_all(
                                || {
                                    t("ОВЕРТАЙМ! Відродження вимкнено", "OVERTIME! No respawns")
                                        .into()
                                },
                                4.0,
                            );
                        }
                    }
                }
                if self.mode == Mode::Sab {
                    self.update_sab();
                }
                if self.mode == Mode::Br {
                    self.update_br(dt);
                }
            }
            MatchState::RoundEnd => {
                if self.time >= self.state_end {
                    self.start_round(PREROUND_NEXT);
                }
            }
            MatchState::PostMatch => {
                if self.time >= self.state_end {
                    self.next_map();
                }
            }
        }
    }

    pub fn time_left(&self) -> f32 {
        match self.state {
            MatchState::Playing => {
                if self.overtime {
                    0.0
                } else {
                    (self.round_end - self.time).max(0.0) as f32
                }
            }
            MatchState::Warmup => 0.0,
            _ => (self.state_end - self.time).max(0.0) as f32,
        }
    }
}
