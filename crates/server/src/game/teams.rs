//! Bots, player limits and who plays on which side.

use super::*;

impl Game {
    pub fn add_bot(&mut self) -> Option<u8> {
        let total = self.players.iter().flatten().count();
        if total >= self.max_players() {
            return None;
        }
        let slot = self.players.iter().position(|p| p.is_none())?;
        let id = slot as u8;
        let base = crate::bots::BOT_NAMES[self.rng.below(crate::bots::BOT_NAMES.len())];
        let name = self.sanitize_name(&format!("[Бот] {base}"), None);
        let mut p = Player::new(id, name, None, self.time);
        p.bot = Some(Box::new(Bot::new(&mut self.rng)));
        // bots dress up as somebody in the battle royale
        p.skin = 1 + self.rng.below(toon_shared::skins::count() as usize - 1) as u8;
        let class = DEFAULT_CLASSES[self.rng.below(DEFAULT_CLASSES.len())].1;
        p.loadout = class;
        p.has_loadout = true;
        self.players[slot] = Some(p);
        let team = self.auto_team(id);
        self.set_team(id, team, false);
        Some(id)
    }

    /// Player limit of the mode being played (the battle royale has its own).
    pub fn max_players(&self) -> usize {
        if self.mode == Mode::Br {
            self.cfg.br.max_players
        } else {
            self.cfg.max_players
        }
    }

    /// Bots fill the server up to this many players (per mode, like the limit).
    pub fn bot_fill(&self) -> usize {
        if self.mode == Mode::Br {
            self.cfg.br.bots
        } else {
            self.cfg.bots
        }
    }

    /// The current mode's bot setting and player limit, to change them.
    pub fn limits_mut(&mut self) -> (&mut usize, &mut usize) {
        if self.mode == Mode::Br {
            (&mut self.cfg.br.bots, &mut self.cfg.br.max_players)
        } else {
            (&mut self.cfg.bots, &mut self.cfg.max_players)
        }
    }

    pub fn auto_team(&self, id: u8) -> Team {
        // battle royale: no sides, everybody goes on the same list
        if self.mode.solo() {
            return Team::Allies;
        }
        // with the squad they came with
        let tag = self.p(id).map_or(0, |p| p.party_tag);
        if let Some(t) = self
            .players
            .iter()
            .flatten()
            .find(|p| tag != 0 && p.id != id && p.party_tag == tag && p.team.is_playing())
            .map(|p| p.team)
        {
            return t;
        }
        let count = |t: Team| {
            self.players
                .iter()
                .flatten()
                .filter(|p| p.id != id && p.team == t)
                .count()
        };
        let (a, b) = (count(Team::Allies), count(Team::Axis));
        if a < b {
            Team::Allies
        } else if b < a {
            Team::Axis
        } else {
            let score = |t: Team| self.scores[t.index()];
            if score(Team::Allies) <= score(Team::Axis) {
                Team::Allies
            } else {
                Team::Axis
            }
        }
    }

    pub fn set_team(&mut self, id: u8, team: Team, notify: bool) {
        let Some(p) = self.pm(id) else { return };
        if p.team == team {
            return;
        }
        let was_alive = p.alive;
        p.team = team;
        p.lobby_wait = false;
        p.alive = false;
        p.streak = 0;
        p.respawn_at = 0.0;
        let name = p.name.clone();
        if was_alive {
            self.drop_bomb_if_carrier(id);
        }
        self.send(id, S2C::TeamSet(team));
        let br = self.mode == Mode::Br;
        if notify {
            self.say(Dest::All, || S2C::Chat {
                from: String::new(),
                team,
                text: if br {
                    if team.is_playing() {
                        toon_shared::tr!("{name} грає", "{name} plays")
                    } else {
                        toon_shared::tr!("{name} дивиться", "{name} watches")
                    }
                } else {
                    format!("{name} → {}", team.name())
                },
                team_only: false,
            });
        }
        // battle royale: whoever joins while the plane is still over the map boards it;
        // later on, they wait in the lobby for the next match
        if team.is_playing() {
            self.br_board(id);
            let running = self.mode == Mode::Br && self.br.running;
            if let Some(p) = self.pm(id).filter(|p| running && !p.in_plane && !p.alive) {
                p.lobby_wait = true;
            }
        }
    }

    /// Bots fill the server up to `cfg.bots` players, keeping teams balanced.
    pub(super) fn manage_bots(&mut self) {
        let humans_playing = self
            .players
            .iter()
            .flatten()
            .filter(|p| !p.is_bot() && p.team.is_playing())
            .count();
        let bots: Vec<u8> = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.is_bot())
            .map(|p| p.id)
            .collect();
        let want = self.bot_fill().saturating_sub(humans_playing);
        if bots.len() < want {
            // a few per second (big servers fill up quickly, without a burst of joins)
            for _ in 0..(want - bots.len()).min(8) {
                self.add_bot();
            }
        } else if bots.len() > want {
            // too many (say, after a battle royale with fifty): a few leave every second,
            // from the bigger team
            let mut left = bots.clone();
            for _ in 0..(bots.len() - want).min(8) {
                let big = if self.team_count(Team::Allies) >= self.team_count(Team::Axis) {
                    Team::Allies
                } else {
                    Team::Axis
                };
                let Some(k) = left
                    .iter()
                    .position(|&b| self.p(b).map(|p| p.team) == Some(big))
                    .or((!left.is_empty()).then_some(0))
                else {
                    break;
                };
                let victim = left.remove(k);
                self.remove_player(victim, None);
            }
        }
        // balance bots between teams (there are none in the battle royale: moving a bot
        // there would knock it out of the round)
        let (a, b) = (self.team_count(Team::Allies), self.team_count(Team::Axis));
        if a.abs_diff(b) > 1 && !self.mode.solo() {
            let (big, small) = if a > b {
                (Team::Allies, Team::Axis)
            } else {
                (Team::Axis, Team::Allies)
            };
            if let Some(bot) = self
                .players
                .iter()
                .flatten()
                .find(|p| p.is_bot() && p.team == big && !p.alive)
                .map(|p| p.id)
            {
                self.set_team(bot, small, false);
            } else if let Some(bot) = self
                .players
                .iter()
                .flatten()
                .find(|p| p.is_bot() && p.team == big)
                .map(|p| p.id)
            {
                self.set_team(bot, small, false);
            }
        }
    }
}
