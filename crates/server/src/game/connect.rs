//! Players coming and going: names, admission, the welcome, leaving.

use super::*;

impl Game {
    /// A real player's installation number, as its client gave it.
    pub fn set_key(&mut self, id: u8, key: u64) {
        if let Some(p) = self.pm(id) {
            p.key = key;
        }
        // (the visit starts here: the key is what it is kept under)
        if let Some(p) = self.p(id).filter(|p| !p.is_bot()) {
            let (k, name, ip) = (p.stats_key(), p.name.clone(), p.ip.clone());
            self.registry.join(&k, &name, &ip);
        }
    }
}

impl Game {
    // ---------------------------------------------------------------- connections

    pub fn sanitize_name(&self, raw: &str, skip: Option<u8>) -> String {
        let mut name: String = raw
            .chars()
            .filter(|c| !c.is_control())
            .take(20)
            .collect::<String>()
            .trim()
            .to_string();
        if name.is_empty() {
            name = "Гравець".into();
        }
        let taken = |n: &str| {
            self.players
                .iter()
                .flatten()
                .any(|p| Some(p.id) != skip && p.name == n)
        };
        if !taken(&name) {
            return name;
        }
        for i in 2..100 {
            let n = format!("{name}({i})");
            if !taken(&n) {
                return n;
            }
        }
        name
    }

    /// Handles a `Hello` from a new address. Returns the player id or a reject reason.
    pub fn connect(
        &mut self,
        addr: SocketAddr,
        name: &str,
        password: &str,
        version: u16,
    ) -> Result<u8, String> {
        self.connect_as(addr, name, password, version, GAME_VERSION, Lang::Uk)
    }

    /// `connect` for a client of game version `game`.
    pub fn connect_as(
        &mut self,
        addr: SocketAddr,
        name: &str,
        password: &str,
        version: u16,
        game: &str,
        lang: Lang,
    ) -> Result<u8, String> {
        // a refusal is written in the newcomer's language
        let id = toon_shared::lang::with(lang, || self.admit(addr, name, password, version, game))?;
        if let Some(p) = self.pm(id) {
            p.lang = lang;
        }
        self.welcome(id);
        Ok(id)
    }

    fn admit(
        &mut self,
        addr: SocketAddr,
        name: &str,
        password: &str,
        version: u16,
        game: &str,
    ) -> Result<u8, String> {
        if version != PROTOCOL_VERSION || (self.cfg.require_same_version && game != GAME_VERSION) {
            if crate::update::newer(game, GAME_VERSION) {
                return Err(tr!(
                    "{} На сервері v{GAME_VERSION}, у тебе v{game}: адміну треба оновити сервер (toon-server --update).",
                    "{} The server has v{GAME_VERSION}, you have v{game}: the admin needs to update the server (toon-server --update).",
                    pick(SERVER_OUTDATED)
                ));
            }
            return Err(tr!(
                "{} На сервері гра v{GAME_VERSION}, у тебе v{game}. Онови гру: кнопка «Оновити» справа вгорі в меню (або завантаж її з Releases на GitHub).",
                "{} The server has v{GAME_VERSION}, you have v{game}. Update the game: the Update button in the top right of the menu (or download it from GitHub Releases).",
                pick(OUTDATED)
            ));
        }
        let ip = addr.ip().to_string();
        if let Some(b) = self.bans.find(&ip) {
            return Err(tr!(
                "Тебе заблоковано на цьому сервері: {}",
                "You are banned from this server: {}",
                b.reason
            ));
        }
        if !self.cfg.password.is_empty() && password != self.cfg.password {
            return Err(t("Невірний пароль сервера", "Wrong server password").into());
        }
        if self.humans() >= self.max_players() {
            return Err(t("Сервер заповнений", "The server is full").into());
        }
        // make room by removing a bot if needed
        if self.players.iter().flatten().count() >= self.max_players() {
            if let Some(bot) = self
                .players
                .iter()
                .flatten()
                .find(|p| p.is_bot())
                .map(|p| p.id)
            {
                self.remove_player(bot, None);
            }
        }
        let Some(slot) = self.players.iter().position(|p| p.is_none()) else {
            return Err(t("Сервер заповнений", "The server is full").into());
        };
        let id = slot as u8;
        let name = self.sanitize_name(name, None);
        self.players[slot] = Some(Player::new(id, name.clone(), Some(addr), self.time));
        self.logf(format!("+ {name} ({ip}) підключився як #{id}"));
        Ok(id)
    }

    /// First messages to a player who just got in, and the news for everyone.
    fn welcome(&mut self, id: u8) {
        let name = self.p(id).map(|p| p.name.clone()).unwrap_or_default();
        self.send(
            id,
            S2C::Welcome {
                id,
                server_name: self.cfg.name.clone(),
                motd: self.cfg.motd.clone(),
                map: self.map.id,
                mode: self.mode,
                settings: self.settings(),
                nonce: self.p(id).map_or(0, |p| p.nonce),
            },
        );
        self.send(id, S2C::Tuning(Box::new(self.tuning.clone())));
        if !self.world.doors.is_empty() {
            let doors = self.doors_msg();
            self.send(id, doors);
        }
        if !self.wreck.log.is_empty() {
            self.send(id, S2C::Blasts(self.wreck.log.clone()));
        }
        self.say(Dest::All, || S2C::Chat {
            from: String::new(),
            team: Team::Spectator,
            text: tr!("{name} приєднався до гри", "{name} joined the game"),
            team_only: false,
        });
    }

    pub fn remove_player(&mut self, id: u8, reason: Option<&str>) {
        // leaving a battle royale round counts as being knocked out: the things stay in
        // a box, the place is recorded, whoever knocked them down gets the kill
        if self.mode == Mode::Br && self.br.running && self.p(id).is_some_and(|p| p.alive) {
            self.kill(id, None, Cause::World, false);
        }
        let Some(p) = self.players[id as usize].take() else {
            return;
        };
        self.party_forget(id);
        if self.sab.carrier == Some(id) {
            self.sab.carrier = None;
            self.sab.pos = p.body.pos;
            self.sab.dropped_since = self.time;
        }
        if !p.is_bot() {
            self.registry.leave(&p.stats_key());
            let text = match reason {
                Some(r) => format!("{} покинув гру ({r})", p.name),
                None => format!("{} покинув гру", p.name),
            };
            let name = p.name.clone();
            self.say(Dest::All, || S2C::Chat {
                from: String::new(),
                team: Team::Spectator,
                text: tr!("{name} покинув гру", "{name} left the game"),
                team_only: false,
            });
            self.logf(format!("- {text}"));
        }
    }
}
