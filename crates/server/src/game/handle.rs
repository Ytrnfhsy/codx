//! Messages from clients: what each one does, chat and remote administration.

use super::*;

impl Game {
    // ---------------------------------------------------------------- client messages

    pub fn handle(&mut self, id: u8, msg: C2S) {
        match msg {
            C2S::Hello { .. } => {}
            C2S::JoinTeam(choice) => self.on_join_team(id, choice),
            C2S::SetLoadout(l) => {
                let l = l.sanitized();
                if let Some(p) = self.pm(id) {
                    p.next_loadout = Some(l);
                    if !p.has_loadout {
                        p.has_loadout = true;
                        p.loadout = l;
                    }
                }
                if self.can_spawn(id)
                    && self
                        .p(id)
                        .map(|p| self.time >= p.respawn_at)
                        .unwrap_or(false)
                {
                    self.spawn_player(id, false);
                }
            }
            C2S::Spawn => {
                if self.can_spawn(id)
                    && self
                        .p(id)
                        .map(|p| self.time >= p.respawn_at)
                        .unwrap_or(false)
                {
                    self.spawn_player(id, false);
                }
            }
            C2S::Input(inp) => self.on_input(id, inp),
            C2S::Fire {
                weapon,
                origin,
                dirs,
                draw,
            } => {
                let dirs: Vec<Vec3> = dirs.iter().map(|d| Vec3::from_array(*d)).collect();
                self.fire_drawn(id, weapon, Vec3::from_array(origin), &dirs, draw);
            }
            C2S::Reload => self.start_reload(id),
            C2S::Switch(slot) => self.switch_weapon(id, slot),
            C2S::ScopeZoom { slot, zoom } => {
                if let Some(p) = self.pm(id) {
                    if slot < 2 {
                        p.scope_zoom[slot as usize] = (zoom as f32 / 10.0).clamp(0.0, 12.0);
                    }
                }
            }
            C2S::Grenade {
                origin,
                dir,
                cook,
                smoke,
            } => self.throw_grenade(
                id,
                Vec3::from_array(origin),
                Vec3::from_array(dir),
                cook,
                smoke,
            ),
            C2S::Melee => self.melee(id),
            C2S::PickUp(ent) => self.pick_up(id, ent),
            C2S::BrJump => self.br_jump(id),
            C2S::UseMedkit => self.br_use_medkit(id),
            C2S::UseBandage => self.br_use(id, toon_shared::inventory::USE_BANDAGE),
            C2S::UseDrink => self.br_use(id, toon_shared::inventory::USE_DRINK),
            C2S::Throw { origin, dir, kind } => {
                self.throw_special(id, Vec3::from_array(origin), Vec3::from_array(dir), kind)
            }
            C2S::Item(a) => self.br_item(id, a),
            C2S::Leaderboards => {
                // (not more often than the backpack's budget allows)
                let key = match self.pm(id) {
                    Some(p) if p.item_budget >= 1.0 => {
                        p.item_budget -= 1.0;
                        p.stats_key()
                    }
                    _ => return,
                };
                let boards = self.br_stats.boards(Some(&key));
                self.send(id, S2C::Leaderboards(boards));
            }
            C2S::Door(k) => {
                self.use_door(id, k, None);
            }
            C2S::Spectate(cmd) => match cmd {
                0 => self.br_spectate_step(id, 1),
                1 => self.br_spectate_step(id, -1),
                2 | 3 => {
                    if let Some(p) = self.pm(id) {
                        p.spec_free = cmd == 2;
                        if p.spec_free {
                            p.spectating = None;
                        }
                    }
                }
                _ => {}
            },
            C2S::Box(cmd) => match cmd {
                BoxCmd::Open(b) => {
                    if let Some(p) = self.pm(id) {
                        p.open_box = Some(b);
                    }
                }
                BoxCmd::Take(b, k) => self.box_take(id, b, k as usize),
                BoxCmd::TakeAll(b) => self.box_take_all(id, b),
                BoxCmd::Close => {
                    if let Some(p) = self.pm(id) {
                        p.open_box = None;
                    }
                }
            },
            C2S::Car(cmd) => match cmd {
                CarCmd::Enter(c) => {
                    self.car_enter(id, c);
                    self.car_hint(id);
                }
                CarCmd::Exit => self.car_exit(id),
                CarCmd::Drive {
                    throttle,
                    steer,
                    brake,
                } => self.car_drive(id, throttle, steer, brake),
            },
            C2S::Mark(at) => self.on_mark(id, at),
            C2S::Party(cmd) => self.on_party(id, cmd),
            C2S::ToLobby => self.to_lobby(id),
            C2S::PartyTag(tag) => self.on_party_tag(id, tag),
            C2S::Marker(m) => {
                if let Some(p) = self.pm(id) {
                    p.marker = m.filter(|m| m[0].is_finite() && m[1].is_finite());
                }
            }
            C2S::Ready(r) => {
                if let Some(p) = self.pm(id) {
                    p.ready = r;
                }
            }
            C2S::SetSkin(s) => {
                if let Some(p) = self.pm(id) {
                    p.skin = s.min(toon_shared::skins::count() - 1);
                }
            }
            C2S::UseKillstreak { target, pilot } => self.use_killstreak(id, target, pilot),
            C2S::Drive {
                pos,
                yaw,
                pitch,
                keys,
            } => self.on_drive(id, Vec3::from_array(pos), yaw, pitch, keys),
            C2S::Chat { text, team_only } => self.on_chat(id, text, team_only),
            C2S::Rcon { seq, proof, cmd } => self.on_rcon(id, seq, &proof, &cmd),
            C2S::Pong(ms) => {
                let now_ms = (self.time * 1000.0) as u32;
                if let Some(p) = self.pm(id) {
                    let rtt = now_ms.wrapping_sub(ms) as f32 / 1000.0;
                    if rtt < 2.0 {
                        p.ping = p.ping * 0.7 + rtt * 0.3;
                    }
                }
            }
            C2S::DropProtection => {
                let time = self.time;
                if let Some(p) = self.pm(id) {
                    if p.alive && p.protected_until > time {
                        p.protected_until = 0.0;
                        // the weapon needs a moment to come up
                        p.switch_end = p.switch_end.max(time + 0.2);
                    }
                }
            }
            C2S::Disconnect => {}
            C2S::Lang(l) => {
                if let Some(p) = self.pm(id) {
                    p.lang = l;
                }
            }
        }
    }

    fn on_join_team(&mut self, id: u8, choice: Option<Team>) {
        let target = match choice {
            None => self.auto_team(id),
            // battle royale: any side means "play"
            Some(t) if t.is_playing() && self.mode.solo() => self.auto_team(id),
            Some(t) => t,
        };
        if target.is_playing() && self.cfg.auto_balance && choice.is_some() {
            let other = target.enemy();
            let count = |t: Team| {
                self.players
                    .iter()
                    .flatten()
                    .filter(|p| p.id != id && p.team == t)
                    .count()
            };
            let mine = count(target) + 1;
            let theirs = count(other);
            if mine > theirs + 1 {
                // try to swap a bot out instead of refusing
                if let Some(bot) = self
                    .players
                    .iter()
                    .flatten()
                    .find(|p| p.is_bot() && p.team == target)
                    .map(|p| p.id)
                {
                    self.set_team(bot, other, false);
                } else {
                    self.say(Dest::One(id), || S2C::Center {
                        text: t(
                            "Команда переповнена — автобаланс",
                            "Team is full — auto-balance",
                        )
                        .into(),
                        secs: 3.0,
                    });
                    let t = self.auto_team(id);
                    self.set_team(id, t, true);
                    return;
                }
            }
        }
        if self.p(id).map(|p| p.team) == Some(target) {
            self.send(id, S2C::TeamSet(target));
            return;
        }
        self.set_team(id, target, true);
    }

    fn on_chat(&mut self, id: u8, text: String, team_only: bool) {
        let text: String = text.chars().filter(|c| !c.is_control()).take(120).collect();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        let Some(p) = self.pm(id) else { return };
        if p.chat_budget < 1.0 {
            return;
        }
        p.chat_budget -= 1.0;
        let (from, team, key, squad) = (p.name.clone(), p.team, p.stats_key(), p.squad);
        let place = format!("{} · {}", self.map.name, self.mode.name());
        self.registry.chat(&key, &from, &text, team_only, &place);
        self.logf(format!(
            "{}{from}: {text}",
            if team_only { "(команда) " } else { "" }
        ));
        let msg = S2C::Chat {
            from,
            team,
            text,
            team_only,
        };
        if team_only && self.mode.solo() {
            // the battle royale: "team" is the squad (everybody there is on one side), and
            // somebody on their own only talks to themselves
            let to: Vec<u8> = self
                .players
                .iter()
                .flatten()
                .filter(|o| o.id == id || squad != 0 && o.squad == squad && !o.is_bot())
                .map(|o| o.id)
                .collect();
            for t in to {
                self.send(t, msg.clone());
            }
        } else if team_only {
            self.send_team(team, msg);
        } else {
            self.broadcast(msg);
        }
    }

    fn on_rcon(&mut self, id: u8, seq: u32, proof: &str, cmd: &str) {
        let ok = {
            let Some(p) = self.p(id) else { return };
            // (each command counts up: one overheard cannot be sent again)
            !self.cfg.rcon_password.is_empty()
                && p.rcon_fails < 5
                && seq > p.rcon_seq
                && cmd.len() <= 512
                && proof == rcon_proof(&self.cfg.rcon_password, p.nonce, seq, cmd)
        };
        if ok {
            if let Some(p) = self.pm(id) {
                p.rcon_seq = seq;
            }
        }
        if !ok {
            if let Some(p) = self.pm(id) {
                p.rcon_fails += 1;
            }
            self.say(Dest::One(id), || {
                S2C::RconReply(
                    t(
                        "RCON: невірний пароль або адміністрування вимкнено",
                        "RCON: wrong password, or administration is off",
                    )
                    .into(),
                )
            });
            return;
        }
        let name = self.p(id).map(|p| p.name.clone()).unwrap_or_default();
        self.logf(format!("RCON від {name}: {cmd}"));
        let reply = crate::admin::exec(self, cmd);
        self.send(id, S2C::RconReply(reply));
    }
}
