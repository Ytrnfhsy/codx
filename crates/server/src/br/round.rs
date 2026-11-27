//! The round: boarding, squads, places, the per-tick update, deaths and late joiners.

use super::*;

impl Game {
    /// Players still in the game: in the plane or alive on the ground.
    pub fn br_alive(&self) -> usize {
        self.players
            .iter()
            .flatten()
            .filter(|p| p.team.is_playing() && (p.alive || p.in_plane))
            .count()
    }

    /// The plane takes off: everyone boards, supplies are scattered, the zone is drawn.
    pub fn br_start(&mut self) {
        self.shut_doors();
        let time = self.time;
        let half = self.map.size() * 0.5;
        let centre = self.map.center();
        // a random line across the map, passing near its middle
        let a = self.rng.range(0.0, std::f32::consts::TAU);
        let dir = vec3(a.cos(), 0.0, a.sin());
        let side = vec3(-dir.z, 0.0, dir.x) * self.rng.range(-0.25, 0.25) * half.x;
        let reach = half.max_element() * 1.05;
        let c3 = vec3(centre.x, PLANE_HEIGHT, centre.y);
        let weather =
            toon_shared::weather::Weather::parse(&self.cfg.br.weather).unwrap_or_else(|| {
                // (its own dice: the rest of the round's randomness stays as it was)
                let seed = (time * 1000.0) as u64 ^ self.rng.clone().next_u64().rotate_left(17);
                toon_shared::weather::Weather::pick(toon_shared::rng::Rng::new(seed).f32())
            });
        self.br = BrState {
            running: true,
            weather,
            plane_from: c3 - dir * reach + side,
            plane_dir: dir,
            plane_t0: time,
            plane_time: 2.0 * reach / PLANE_SPEED,
            zone: Zone {
                from_c: centre,
                from_r: half.length() * 1.02,
                ..Default::default()
            },
            entrants: 0,
            loot: Vec::new(),
            next_burn: time + 1.0,
            drops: Vec::new(),
            next_drop: time + FIRST_DROP,
            launched: 0,
            boxes: Vec::new(),
            red: None,
            next_red: time + RED_FIRST,
        };
        self.br_next_phase(time);
        // everyone boards
        let start = self.br.plane_pos(time);
        let mut entrants = 0;
        let mut jumps = Vec::new();
        for p in self.players.iter_mut().flatten() {
            if !p.team.is_playing() {
                continue;
            }
            entrants += 1;
            p.team = toon_shared::Team::Allies;
            p.alive = false;
            p.in_plane = true;
            p.lobby_wait = false;
            p.body = Body {
                pos: start,
                ..Default::default()
            };
            p.inv = Default::default();
            p.empty = 0;
            p.driving = None;
            p.respawn_at = f64::MAX;
            p.downed = false;
            p.downed_by = None;
            p.revive = 0.0;
            p.marker = None;
            p.squad = 0;
            p.br_dmg = 0.0;
            p.br_longest = 0.0;
            p.br_place = 0;
            p.br_out_at = 0.0;
            p.spectating = None;
            p.spec_free = false;
            p.killed_by = None;
            p.downed_dist = 0;
            if p.is_bot() {
                jumps.push(p.id);
            }
        }
        self.br.entrants = entrants;
        self.br_make_squads();
        self.br_scatter_loot();
        self.spawn_cars();
        self.br_assign_drops(&jumps);
        self.center_all(
            || {
                t(
                    "ПРОБІЛ — СТРИБНУТИ З ЛІТАКА",
                    "SPACE — JUMP OUT OF THE PLANE",
                )
                .into()
            },
            4.0,
        );
    }

    /// Splits the players into squads (real players first, in the order they came, so that
    /// friends who joined together play together; bots fill up the rest).
    fn br_make_squads(&mut self) {
        let size = self.cfg.br.squad.clamp(1, 4) as usize;
        if size < 2 {
            return;
        }
        let ids: Vec<u8> = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.in_plane && !p.is_bot())
            .map(|p| p.id)
            .collect();
        let mut bots: Vec<u8> = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.in_plane && p.is_bot())
            .map(|p| p.id)
            .collect();
        for k in (1..bots.len()).rev() {
            let j = self.rng.below(k + 1);
            bots.swap(k, j);
        }
        let groups = self.squads_from_parties(&ids, &bots, size);
        for (k, g) in groups.into_iter().enumerate() {
            for id in g {
                if let Some(p) = self.pm(id) {
                    p.squad = (k + 1).min(255) as u8;
                }
            }
        }
    }

    /// A player's place in the round is settled: kept for the summary, and for a real
    /// player added to their records.
    fn br_set_place(&mut self, id: u8, place: u16) {
        let now = self.time;
        let Some(p) = self.pm(id) else { return };
        if p.br_place != 0 {
            return;
        }
        p.br_place = place;
        if p.br_out_at <= 0.0 || p.alive {
            p.br_out_at = now;
        }
        if p.is_bot() {
            return;
        }
        let (key, name, kills, dmg, longest) = (
            p.stats_key(),
            p.name.clone(),
            p.kills as u32,
            p.br_dmg,
            p.br_longest,
        );
        let news = self
            .br_stats
            .record(&key, &name, place as u32, kills, dmg, longest);
        self.announce(news);
    }

    /// Sides still in the round: squads, and players on their own.
    pub fn br_sides(&self) -> usize {
        let mut squads: Vec<u8> = Vec::new();
        let mut solo = 0;
        for p in self.players.iter().flatten() {
            if !(p.team.is_playing() && (p.alive || p.in_plane)) {
                continue;
            }
            if p.squad == 0 {
                solo += 1;
            } else if !squads.contains(&p.squad) {
                squads.push(p.squad);
            }
        }
        solo + squads.len()
    }

    /// Plane, zone, supplies and the end of the round; called every tick while playing.
    pub fn update_br(&mut self, dt: f32) {
        if !self.br.running || self.state != MatchState::Playing {
            return;
        }
        let time = self.time;
        // the plane: passengers ride along; at the far edge everybody left is pushed out
        let plane = self.br.plane_pos(time);
        let flying = self.br.plane_flying(time);
        let mut jumpers = Vec::new();
        for p in self.players.iter_mut().flatten() {
            if p.in_plane {
                p.body.pos = plane;
                let bot_ready = p.bot.as_ref().is_some_and(|b| time >= b.br_jump_at);
                if !flying || bot_ready {
                    jumpers.push(p.id);
                }
            }
        }
        for id in jumpers {
            self.br_jump(id);
        }
        // the zone
        if time >= self.br.zone.shrink_end {
            self.br.zone.phase += 1;
            self.br_next_phase(time);
            let r = self.br.zone.to_r;
            self.center_all(
                || {
                    if r > 0.0 {
                        t(
                            "Зону позначено на карті — рушай!",
                            "New zone marked — move!",
                        )
                        .into()
                    } else {
                        t("Остання зона!", "Final zone!").into()
                    }
                },
                3.0,
            );
        }
        if time >= self.br.next_burn {
            self.br.next_burn = time + 1.0;
            let zone = self.br.zone;
            let burn: Vec<(u8, Vec3)> = self
                .players
                .iter()
                .flatten()
                .filter(|p| p.alive && !zone.contains(time, p.body.pos))
                .map(|p| (p.id, p.body.pos))
                .collect();
            for (id, pos) in burn {
                self.damage(id, None, zone.dps, Cause::Zone, false, pos);
            }
        }
        self.br_downed(dt);
        self.br_airdrops(dt);
        self.br_red_zone();
        // medkits being used; reserves following the inventory
        for p in self.players.iter_mut().flatten() {
            if !p.alive {
                continue;
            }
            if p.inv.healing > 0.0 {
                p.inv.healing -= dt;
                if p.inv.healing <= 0.0 {
                    p.inv.healing = 0.0;
                    match p.inv.using {
                        USE_BANDAGE if p.inv.bandages > 0 => {
                            p.inv.bandages -= 1;
                            p.hp = (p.hp + BANDAGE_HEAL).min(BANDAGE_MAX.max(p.hp));
                        }
                        USE_DRINK if p.inv.drinks > 0 => {
                            p.inv.drinks -= 1;
                            p.inv.boost = (p.inv.boost + DRINK_BOOST).min(100.0);
                        }
                        _ if p.inv.medkits > 0
                            && p.inv.using != USE_BANDAGE
                            && p.inv.using != USE_DRINK =>
                        {
                            p.inv.medkits -= 1;
                            p.hp = (p.hp + MEDKIT_HEAL).min(MAX_HEALTH);
                        }
                        _ => {}
                    }
                    p.inv.using = 0;
                }
            }
            // energy drinks: the boost heals slowly
            if p.inv.boost > 0.0 && !p.downed {
                let k = dt.min(p.inv.boost);
                p.inv.boost -= k;
                p.hp = (p.hp + k).min(MAX_HEALTH);
            }
            p.br_sync_ammo();
        }
        // no real player left in the round: nobody needs to watch the bots play it out
        // (a real player whose squad still fights is still in it: they watch their mates)
        let humans_in = self.players.iter().flatten().any(|p| {
            !p.is_bot()
                && p.team.is_playing()
                && (p.alive
                    || p.in_plane
                    || p.squad != 0
                        && p.br_place == 0
                        && self
                            .players
                            .iter()
                            .flatten()
                            .any(|m| m.squad == p.squad && (m.alive || m.in_plane)))
        });
        self.br_spectators();
        if !humans_in {
            self.br.running = false;
            self.end_match_text(|| {
                t(
                    "Усі гравці вибули — раунд завершено",
                    "Every player is out — round over",
                )
                .into()
            });
            return;
        }
        // the end: one player, or one squad, left
        let sides = self.br_sides();
        if sides <= 1 && (self.br.entrants >= 2 || sides == 0) {
            let last = self
                .players
                .iter()
                .flatten()
                .find(|p| p.team.is_playing() && (p.alive || p.in_plane))
                .map(|p| (p.name.clone(), p.squad));
            let last_squad = last.as_ref().map_or(0, |l| l.1);
            let winner = last.map(|(name, squad)| {
                if squad == 0 {
                    return name;
                }
                // the whole squad, those knocked out on the way included
                self.players
                    .iter()
                    .flatten()
                    .filter(|p| p.squad == squad)
                    .map(|p| p.name.clone())
                    .collect::<Vec<_>>()
                    .join(", ")
            });
            let winners: Vec<u8> = self
                .players
                .iter()
                .flatten()
                .filter(|p| {
                    p.team.is_playing() && (p.alive || p.in_plane) && p.br_place == 0
                        || last_squad != 0 && p.squad == last_squad
                })
                .map(|p| p.id)
                .collect();
            for id in winners {
                self.br_set_place(id, 1);
            }
            self.br.running = false;
            let text = || match &winner {
                Some(n) => tr!("{n} — переможці!", "{n} win!"),
                None => t("Переможця немає", "Nobody survived").into(),
            };
            self.end_match_text(text);
        }
    }

    /// A player is out: tell them their place.
    pub fn br_on_death(&mut self, victim: u8) {
        if !self.br.running {
            return;
        }
        self.br_drop_inventory(victim);
        let now = self.time;
        if let Some(v) = self.pm(victim) {
            v.br_out_at = now;
        }
        let squad = self.p(victim).map_or(0, |p| p.squad);
        let mates_in = squad != 0
            && self
                .players
                .iter()
                .flatten()
                .any(|p| p.squad == squad && p.alive);
        if mates_in {
            self.say(Dest::One(victim), || S2C::Center {
                text: t(
                    "Ти вибув — твій загін ще бореться",
                    "You are out — your squad fights on",
                )
                .into(),
                secs: 5.0,
            });
            return;
        }
        let place = self.br_sides() + 1;
        // the whole side is out: its place (squadmates out earlier share it)
        let side: Vec<u8> = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.id == victim || squad != 0 && p.squad == squad)
            .map(|p| p.id)
            .collect();
        for id in side {
            self.br_set_place(id, place as u16);
        }
        self.say(Dest::One(victim), || S2C::Center {
            text: tr!("Ти вибув! Місце #{place}", "You are out! Place #{place}"),
            secs: 5.0,
        });
    }

    /// Puts a player who joined late onto the plane, if it is still flying.
    pub fn br_board(&mut self, id: u8) {
        let time = self.time;
        if self.mode != Mode::Br || self.state != MatchState::Playing || !self.br.plane_flying(time)
        {
            return;
        }
        let pos = self.br.plane_pos(time);
        let left = self.br.plane_time as f64 - (time - self.br.plane_t0);
        let jump = time + self.rng.range(0.1, 0.8) as f64 * left;
        let Some(p) = self.pm(id) else { return };
        if p.in_plane || p.alive {
            return;
        }
        p.in_plane = true;
        p.body = Body {
            pos,
            ..Default::default()
        };
        p.inv = Default::default();
        p.empty = 0;
        p.respawn_at = f64::MAX;
        p.downed = false;
        p.squad = 0;
        if let Some(b) = p.bot.as_mut() {
            b.br_jump_at = jump;
        }
        self.br.entrants += 1;
        // a squad with room for one more, or a new one
        let size = self.cfg.br.squad.clamp(1, 4);
        if size >= 2 {
            let mut count = [0u8; 256];
            for o in self
                .players
                .iter()
                .flatten()
                .filter(|o| o.in_plane || o.alive)
            {
                count[o.squad as usize] += 1;
            }
            let squad = (1..=255u8)
                .find(|s| count[*s as usize] > 0 && count[*s as usize] < size)
                .or_else(|| (1..=255u8).find(|s| count[*s as usize] == 0))
                .unwrap_or(0);
            if let Some(p) = self.pm(id) {
                p.squad = squad;
            }
        }
    }
}
