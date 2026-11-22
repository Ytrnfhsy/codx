//! Talking without words: pings (pointing something out to our side) and the squad
//! lobby before a battle royale match (who plays with whom).

use crate::game::{Dest, Game, SEEN_LINGER};
use glam::Vec3;
use toon_shared::lang::t;
use toon_shared::protocol::{
    EntSnap, LobbySnap, MarkAt, MatchState, PartyCmd, PingSnap, PingWhat, S2C,
};
use toon_shared::Mode;

/// How long a ping stays up, seconds.
const SPOT_LIFE: f64 = 25.0;
const DANGER_LIFE: f64 = 15.0;
const ENEMY_LIFE: f64 = 6.0;
const THING_LIFE: f64 = 45.0;
/// Pings one player may have up at once (the oldest goes).
const PER_PLAYER: usize = 3;
/// Not more often than this, seconds.
const PING_GAP: f64 = 0.2;
/// How far from the player a ping may be put (the island is 2 km across).
const PING_REACH: f32 = 2500.0;

#[derive(Clone, Debug)]
pub struct Ping {
    pub id: u16,
    pub from: u8,
    pub what: PingWhat,
    pub pos: Vec3,
    pub at: f64,
    pub until: f64,
    /// The thing it points at (it goes with it, and goes away once it is taken).
    pub ent: Option<u16>,
    /// One thing inside the box `ent` (not the whole box): it goes once that is taken.
    pub in_box: bool,
}

impl Game {
    /// Something lying on the ground, a crate or a car, by entity id: what it is and where.
    fn markable(&self, ent: u16) -> Option<(PingWhat, Vec3)> {
        if let Some(pk) = self.pickups.iter().find(|pk| pk.snap().id() == ent) {
            if let EntSnap::Weapon { weapon, .. } = pk.snap() {
                return Some((PingWhat::Weapon(weapon), pk.pos));
            }
        }
        if let Some(l) = self.br.loot.iter().find(|l| l.id == ent) {
            return Some((PingWhat::Loot(l.kind, l.amount), l.pos));
        }
        if let Some(b) = self.br.boxes.iter().find(|b| b.id == ent) {
            return Some((PingWhat::DeathBox, b.pos));
        }
        if let Some(d) = self.br.drops.iter().find(|d| d.id == ent) {
            return Some((PingWhat::AirDrop, d.pos));
        }
        if let Some(c) = self.cars.iter().find(|c| c.id == ent) {
            return Some((PingWhat::Car(c.kind), c.state.pos));
        }
        None
    }

    pub fn on_mark(&mut self, id: u8, at: MarkAt) {
        let time = self.time;
        let Some(me) = self.p(id) else { return };
        if !me.alive || !me.team.is_playing() || time - me.last_ping < PING_GAP {
            return;
        }
        let eye = me.body.pos;
        let mut in_box = false;
        let (what, pos, ent, life) = match at {
            MarkAt::BoxItem(bid, k) => {
                let Some(b) = self.br.boxes.iter().find(|b| b.id == bid) else {
                    return;
                };
                let k = k as usize;
                let what = match b.items.get(k) {
                    Some(&(kind, n)) => PingWhat::Loot(kind, n),
                    None => match b.guns.get(k - b.items.len()) {
                        Some(g) => PingWhat::Weapon(g.id),
                        None => return,
                    },
                };
                if b.pos.distance(eye) > PING_REACH {
                    return;
                }
                in_box = true;
                (what, b.pos, Some(bid), THING_LIFE)
            }
            MarkAt::Remove(pid) => {
                self.pings.retain(|p| !(p.id == pid && p.from == id));
                return;
            }
            MarkAt::Clear => {
                if let Some(k) = self.pings.iter().rposition(|p| p.from == id) {
                    self.pings.remove(k);
                }
                return;
            }
            MarkAt::Spot { pos, danger } => {
                let pos = Vec3::from_array(pos);
                if !pos.is_finite() || pos.distance(eye) > PING_REACH {
                    return;
                }
                // a second press on the same place: "enemy seen here"
                if danger {
                    (PingWhat::Danger, pos, None, DANGER_LIFE)
                } else {
                    (PingWhat::Spot, pos, None, SPOT_LIFE)
                }
            }
            MarkAt::Thing(ent) => {
                let Some((what, pos)) = self.markable(ent) else {
                    return;
                };
                if pos.distance(eye) > PING_REACH {
                    return;
                }
                (what, pos, Some(ent), THING_LIFE)
            }
            MarkAt::Enemy(t) => {
                let Some(other) = self.p(t) else { return };
                // only somebody they can actually see (the server knows who that is)
                let seen = !self.hideable(me, other) || time - me.seen_at[t as usize] < SEEN_LINGER;
                if t == id || !other.alive || self.allied(me, other) || !seen {
                    return;
                }
                (PingWhat::Enemy, other.body.pos, None, ENEMY_LIFE)
            }
        };
        if let Some(p) = self.pm(id) {
            p.last_ping = time;
        }
        // one ping per thing: pointing at it again only freshens it
        self.pings.retain(|p| {
            let same = ent.is_some() && p.ent == ent && (p.what == what || !in_box && !p.in_box)
                || p.what == what && p.pos.distance(pos) < 2.0
                // ("enemy here" takes the place of the plain ping it was pressed over)
                || what == PingWhat::Danger && p.what == PingWhat::Spot && p.pos.distance(pos) < 3.0;
            !(p.from == id && same)
        });
        let mine: Vec<usize> = (0..self.pings.len())
            .filter(|&k| self.pings[k].from == id)
            .collect();
        if mine.len() >= PER_PLAYER {
            self.pings.remove(mine[0]);
        }
        self.next_ping = self.next_ping.wrapping_add(1).max(1);
        self.pings.push(Ping {
            id: self.next_ping,
            from: id,
            what,
            pos,
            at: time,
            until: time + life,
            ent,
            in_box,
        });
    }

    /// Pings run out, follow what they point at, and go with whoever put them.
    pub fn update_pings(&mut self) {
        let time = self.time;
        let mut pings = std::mem::take(&mut self.pings);
        pings.retain_mut(|p| {
            if time >= p.until || self.p(p.from).is_none_or(|o| !o.alive) {
                return false;
            }
            match p.ent {
                // a thing in a box: as long as the box still has one like it
                Some(e) if p.in_box => {
                    let what = p.what;
                    self.br.boxes.iter().any(|b| {
                        b.id == e
                            && match what {
                                PingWhat::Loot(kind, _) => b.items.iter().any(|i| i.0 == kind),
                                PingWhat::Weapon(w) => b.guns.iter().any(|g| g.id == w),
                                _ => false,
                            }
                    })
                }
                Some(e) => match self.markable(e) {
                    Some((what, pos)) => {
                        p.what = what;
                        p.pos = pos;
                        true
                    }
                    // taken
                    None => false,
                },
                None => true,
            }
        });
        self.pings = pings;
    }

    /// The pings player `id` sees: their own and their side's.
    pub fn pings_for(&self, id: u8) -> Vec<PingSnap> {
        let Some(me) = self.p(id) else {
            return Vec::new();
        };
        self.pings
            .iter()
            .filter(|p| {
                p.from == id
                    || self.p(p.from).is_some_and(|o| {
                        self.allied(me, o)
                            // (before the round the squads are only the lobby's)
                            || !self.br.running && me.party != 0 && me.party == o.party
                    })
            })
            .map(|p| PingSnap {
                id: p.id,
                from: p.from,
                what: p.what,
                pos: p.pos.to_array(),
                age: (self.time - p.at) as f32,
            })
            .collect()
    }

    // ---- the squad lobby

    /// The lobby is open: a battle royale in squads that has not started yet.
    pub fn lobby_open(&self) -> bool {
        self.mode == Mode::Br && matches!(self.state, MatchState::Warmup | MatchState::PreRound)
    }

    /// Squads can be put together in the lobby.
    fn squads_on(&self) -> bool {
        self.lobby_open() && self.cfg.br.squad >= 2
    }

    fn party_size(&self, party: u8) -> usize {
        if party == 0 {
            return 0;
        }
        self.players
            .iter()
            .flatten()
            .filter(|p| !p.is_bot() && p.party == party)
            .count()
    }

    /// A party left with one player is no party.
    fn tidy_parties(&mut self) {
        let mut count = [0u8; 256];
        for p in self.players.iter().flatten() {
            count[p.party as usize] += 1;
        }
        for p in self.players.iter_mut().flatten() {
            if p.party != 0 && count[p.party as usize] < 2 {
                p.party = 0;
            }
        }
        let alive: Vec<u8> = self.players.iter().flatten().map(|p| p.id).collect();
        self.party_asks
            .retain(|(a, b)| alive.contains(a) && alive.contains(b));
    }

    fn tell(&mut self, id: u8, uk: &'static str, en: &'static str) {
        self.say(Dest::One(id), || S2C::Center {
            text: t(uk, en).into(),
            secs: 3.0,
        });
    }

    pub fn on_party(&mut self, id: u8, cmd: PartyCmd) {
        let size = self.cfg.br.squad.clamp(1, 4) as usize;
        match cmd {
            PartyCmd::Leave => {
                if let Some(p) = self.pm(id) {
                    p.party = 0;
                }
                self.tidy_parties();
            }
            _ if !self.squads_on() => {}
            PartyCmd::Ask(to) => {
                let (Some(me), Some(other)) = (self.p(id), self.p(to)) else {
                    return;
                };
                if to == id || other.is_bot() || me.party != 0 && me.party == other.party {
                    return;
                }
                if self.party_asks.iter().filter(|a| a.0 == id).count() >= 8
                    || self.party_asks.contains(&(id, to))
                {
                    return;
                }
                // they asked us already: that is a yes from both
                if self.party_asks.contains(&(to, id)) {
                    return self.on_party(id, PartyCmd::Accept(to));
                }
                let name = me.name.clone();
                self.party_asks.push((id, to));
                self.say(Dest::One(to), || S2C::Chat {
                    from: String::new(),
                    team: toon_shared::Team::Spectator,
                    text: toon_shared::tr!(
                        "{} хоче грати з тобою в загоні (вікно «Загін»)",
                        "{} wants to be in your squad (the «Squad» window)",
                        name
                    ),
                    team_only: false,
                });
            }
            PartyCmd::Decline(from) => {
                self.party_asks.retain(|a| *a != (from, id));
            }
            PartyCmd::Accept(from) => {
                if !self.party_asks.contains(&(from, id)) {
                    return;
                }
                self.party_asks.retain(|a| *a != (from, id));
                let (Some(me), Some(other)) = (self.p(id), self.p(from)) else {
                    return;
                };
                let (mine, theirs) = (me.party, other.party);
                // the one who asked comes over; with no squad yet, ours goes to theirs
                let (party, mover) = if mine != 0 {
                    (mine, from)
                } else if theirs != 0 {
                    (theirs, id)
                } else {
                    let used: Vec<u8> = self.players.iter().flatten().map(|p| p.party).collect();
                    let new = (1..=255u8).find(|k| !used.contains(k)).unwrap_or(1);
                    if let Some(p) = self.pm(id) {
                        p.party = new;
                    }
                    (new, from)
                };
                if self.party_size(party) + 1 > size {
                    self.tidy_parties();
                    self.tell(id, "Загін уже повний", "The squad is full");
                    self.tell(from, "Загін уже повний", "The squad is full");
                    return;
                }
                if let Some(p) = self.pm(mover) {
                    p.party = party;
                }
                // asks inside one squad are answered
                let members: Vec<u8> = self
                    .players
                    .iter()
                    .flatten()
                    .filter(|p| p.party == party)
                    .map(|p| p.id)
                    .collect();
                self.party_asks
                    .retain(|(a, b)| !(members.contains(a) && members.contains(b)));
                self.tidy_parties();
                let names: Vec<String> = self
                    .players
                    .iter()
                    .flatten()
                    .filter(|p| p.party == party)
                    .map(|p| p.name.clone())
                    .collect();
                let list = names.join(", ");
                for m in members {
                    let list = list.clone();
                    self.say(Dest::One(m), || S2C::Center {
                        text: toon_shared::tr!("Загін: {}", "Squad: {}", list),
                        secs: 3.0,
                    });
                }
            }
        }
    }

    /// A player is gone: their squad and asks with them.
    pub fn party_forget(&mut self, id: u8) {
        self.party_asks.retain(|(a, b)| *a != id && *b != id);
        self.pings.retain(|p| p.from != id);
        self.tidy_parties();
    }

    /// Back to the lobby in the middle of a battle royale match: out of the round (as if
    /// knocked out: the things stay in a box), waiting there for the next match.
    pub fn to_lobby(&mut self, id: u8) {
        if self.mode != Mode::Br || self.lobby_open() {
            return;
        }
        let Some(p) = self.p(id) else { return };
        if !p.team.is_playing() || p.lobby_wait {
            return;
        }
        if p.alive {
            self.kill(id, None, toon_shared::weapons::Cause::World, false);
        }
        if let Some(p) = self.pm(id) {
            p.in_plane = false;
            p.alive = false;
            p.spectating = None;
            p.ready = false;
            p.lobby_wait = true;
        }
    }

    pub fn lobby_for(&self, id: u8) -> Option<LobbySnap> {
        let waiting = self.mode == Mode::Br && self.p(id).is_some_and(|p| p.lobby_wait);
        if !self.lobby_open() && !waiting {
            return None;
        }
        let mut parties: Vec<(u8, Vec<u8>)> = Vec::new();
        for p in self
            .players
            .iter()
            .flatten()
            .filter(|p| !p.is_bot() && p.team.is_playing())
        {
            if p.party == 0 {
                parties.push((0, vec![p.id]));
            } else if let Some(g) = parties.iter_mut().find(|g| g.0 == p.party) {
                g.1.push(p.id);
            } else {
                parties.push((p.party, vec![p.id]));
            }
        }
        Some(LobbySnap {
            parties: parties.into_iter().map(|g| g.1).collect(),
            asking_me: self
                .party_asks
                .iter()
                .filter(|a| a.1 == id)
                .map(|a| a.0)
                .collect(),
            i_asked: self
                .party_asks
                .iter()
                .filter(|a| a.0 == id)
                .map(|a| a.1)
                .collect(),
            size: self.cfg.br.squad.clamp(1, 4),
            skins: self
                .players
                .iter()
                .flatten()
                .filter(|p| !p.is_bot())
                .map(|p| (p.id, p.skin))
                .collect(),
            ready: self
                .players
                .iter()
                .flatten()
                .filter(|p| !p.is_bot() && p.ready)
                .map(|p| p.id)
                .collect(),
            running: !self.lobby_open(),
        })
    }

    /// The squads for the round: the lobby's squads first (too many for the size: the
    /// rest play on their own), then players without one fill the gaps or make new ones
    /// in the order they came, then bots.
    pub fn squads_from_parties(&self, humans: &[u8], bots: &[u8], size: usize) -> Vec<Vec<u8>> {
        let mut groups: Vec<(u8, Vec<u8>)> = Vec::new();
        let mut alone: Vec<u8> = Vec::new();
        for &h in humans {
            let party = self.p(h).map_or(0, |p| p.party);
            if party == 0 {
                alone.push(h);
                continue;
            }
            match groups.iter_mut().find(|g| g.0 == party) {
                Some(g) if g.1.len() < size => g.1.push(h),
                Some(_) => alone.push(h),
                None => groups.push((party, vec![h])),
            }
        }
        let mut groups: Vec<Vec<u8>> = groups.into_iter().map(|g| g.1).collect();
        for id in alone.into_iter().chain(bots.iter().copied()) {
            match groups.iter_mut().find(|g| g.len() < size) {
                Some(g) => g.push(id),
                None => groups.push(vec![id]),
            }
        }
        groups
    }
}
