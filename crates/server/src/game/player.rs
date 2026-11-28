//! A player on the server: their state, gear and the history the hit checks rewind.

use super::*;

pub struct Player {
    pub id: u8,
    pub name: String,
    /// Language the server writes to this player in.
    pub lang: Lang,
    pub addr: Option<SocketAddr>,
    pub ip: String,
    pub bot: Option<Box<Bot>>,
    pub team: Team,
    pub alive: bool,
    pub hp: f32,
    pub body: Body,
    pub yaw: f32,
    pub pitch: f32,
    pub flags: u8,
    pub slot: u8,
    pub loadout: Loadout,
    pub has_loadout: bool,
    pub next_loadout: Option<Loadout>,
    pub weapons: [WState; 2],
    pub grenades: u8,
    pub smokes: u8,
    /// The killstreak vehicle this player is steering.
    pub driving: Option<u16>,
    pub shot_budget: f32,
    pub reload_end: Option<f64>,
    pub switch_end: f64,
    pub melee_ready: f64,
    pub last_damage: f64,
    pub last_shot: f64,
    /// When this player could last see each other player (by id): enemies they cannot
    /// see are left out of their snapshots (see `Game::update_visibility`).
    pub seen_at: [f64; MAX_PLAYERS],
    /// Last time an input had the sprint flag (weapon raise delay).
    pub last_sprint: f64,
    /// Invulnerable until this time (spawn protection).
    pub protected_until: f64,
    /// Accuracy stats for this match.
    pub shots_fired: u32,
    pub shots_hit: u32,
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    pub score: i32,
    pub streak: u32,
    pub rewards: Vec<Killstreak>,
    pub attackers: Vec<(u8, f64)>,
    pub respawn_at: f64,
    pub life: u16,
    pub ping: f32,
    /// Pressed «Ready» (the match waits for everybody).
    pub ready: bool,
    /// Battle royale squad (0 = on your own).
    pub squad: u8,
    /// The squad chosen in the lobby (0 = none): kept from match to match.
    pub party: u8,
    /// Battle royale: went back to the lobby during a match (waits for the next one).
    pub lobby_wait: bool,
    /// The squad they came with from the main lobby (0 = none).
    pub party_tag: u64,
    /// When they last pinged.
    pub last_ping: f64,
    /// Battle royale: knocked down (crawling, `hp` bleeds out) and who did it.
    pub downed: bool,
    pub downed_by: Option<u8>,
    /// Revive progress 0..1 (on the one being revived).
    pub revive: f32,
    /// The map marker this player shares with the squad.
    pub marker: Option<[f32; 2]>,
    /// The death box they look into.
    pub open_box: Option<u16>,
    /// Out of the battle royale: whom they watch (or a free camera), and who got them.
    pub spectating: Option<u8>,
    pub spec_free: bool,
    pub killed_by: Option<u8>,
    /// How far the one who knocked them down was (for the kill feed).
    pub downed_dist: u16,
    /// This battle royale round: damage dealt, longest kill, place once out, when out.
    pub br_dmg: f32,
    pub br_longest: f32,
    pub br_place: u16,
    pub br_out_at: f64,
    pub last_seq: u32,
    pub move_budget: f32,
    pub history: VecDeque<HistEntry>,
    pub rcon: bool,
    pub rcon_fails: u32,
    /// This connection's number for admin command proofs, and the last command's count.
    pub nonce: u64,
    pub rcon_seq: u32,
    /// The installation's own number (0: none): records are kept under it.
    pub key: u64,
    pub progress: f32,
    pub joined_at: f64,
    pub chat_budget: f32,
    /// The things on the ground this player's client knows of.
    pub lying: std::collections::HashMap<u16, EntSnap>,
    /// The height of the last place they stood on, and the highest point of the fall
    /// they are in (None: not falling, or it does not count — a parachute, a car).
    pub support_y: f32,
    /// Been by a wall since the feet left the ground (kicks off walls lift further).
    pub by_wall: bool,
    pub fall_top: Option<f32>,
    pub(super) fall_life: u16,
    /// Kicks off walls since the feet left the ground, and whether the last input said
    /// it had just kicked (a kick stops a fall: it counts from there).
    pub(super) kicks: u8,
    pub(super) kicking: bool,
    /// Backpack actions left (refills; nobody needs to drop things a hundred times a second).
    pub item_budget: f32,
    /// Battle royale: still riding the plane.
    pub in_plane: bool,
    /// Battle royale: everything carried besides the guns.
    pub inv: toon_shared::inventory::Inventory,
    /// Battle royale: each slot's reserve as last matched with the inventory (a reload
    /// takes rounds out of the reserve, and so out of the inventory).
    pub ammo_seen: [u16; 2],
    /// Battle royale: the slots that hold no gun yet, one bit each (the next gun picked up
    /// goes in one of them).
    pub empty: u8,
    /// Battle royale character.
    pub skin: u8,
    /// Battle royale: what the variable scope on each gun magnifies (0 = its weakest).
    pub scope_zoom: [f32; 2],
    /// Kills in a quick run (each within `MULTI_KILL_WINDOW` of the last) and when the
    /// last one was.
    pub multi: u8,
    /// What this match counts for the awards at its end.
    pub ms: MatchStats,
    pub multi_at: f64,
}

impl Player {
    /// How many rounds the gun in hand holds (an extended magazine holds more).
    pub fn mag_cap(&self, base: u16) -> u16 {
        toon_shared::inventory::mag_size(
            base,
            self.inv
                .has_mod(self.slot as usize, toon_shared::inventory::MOD_EXTMAG),
        )
    }

    /// Battle royale: what is on the gun in hand, for the killcam.
    pub fn gear(&self) -> Gear {
        use toon_shared::inventory::{sight_fits, sight_zoom_range};
        let k = self.slot as usize & 1;
        if self.is_empty(k) {
            return Default::default();
        }
        let id = self.weapons[k].id;
        let sight = Some(self.inv.sights[k])
            .filter(|&l| sight_fits(l, id))
            .unwrap_or(0);
        let (lo, hi) = sight_zoom_range(sight);
        let zoom = if sight >= 2 {
            self.scope_zoom[k].clamp(lo, hi)
        } else {
            0.0
        };
        Gear {
            sight,
            mods: self.inv.mods[k],
            zoom,
        }
    }

    /// The gun in hand has a suppressor on.
    pub fn suppressed(&self) -> bool {
        self.inv
            .has_mod(self.slot as usize, toon_shared::inventory::MOD_SUPPRESSOR)
    }

    /// Battle royale: slot `k` holds no gun.
    pub fn is_empty(&self, k: usize) -> bool {
        self.empty & (1 << k) != 0
    }

    /// Battle royale: every gun's reserve is the inventory's ammo of its calibre.
    pub fn br_refresh_reserve(&mut self) {
        let empty = self.empty;
        for (k, w) in self.weapons.iter_mut().enumerate() {
            if empty & (1 << k) != 0 {
                w.reserve = 0;
            } else {
                w.reserve = self.inv.ammo(toon_shared::inventory::AmmoType::of(w.id));
            }
            self.ammo_seen[k] = w.reserve;
        }
    }

    /// Battle royale: rounds a reload moved from a reserve into a magazine leave the
    /// inventory too.
    pub fn br_sync_ammo(&mut self) {
        for k in 0..2 {
            let w = self.weapons[k];
            if self.is_empty(k) {
                continue;
            }
            let used = self.ammo_seen[k].saturating_sub(w.reserve);
            if used > 0 {
                let t = toon_shared::inventory::AmmoType::of(w.id);
                self.inv.ammo[t.index()] = self.inv.ammo[t.index()].saturating_sub(used);
            }
        }
        self.br_refresh_reserve();
    }

    pub fn new(id: u8, name: String, addr: Option<SocketAddr>, now: f64) -> Player {
        let l = DEFAULT_CLASSES[0].1;
        Player {
            id,
            name,
            lang: Lang::Uk,
            ip: addr
                .map(|a| a.ip().to_string())
                .unwrap_or_else(|| "bot".into()),
            addr,
            bot: None,
            team: Team::Spectator,
            alive: false,
            hp: MAX_HEALTH,
            body: Body::default(),
            yaw: 0.0,
            pitch: 0.0,
            flags: 0,
            slot: 0,
            loadout: l,
            has_loadout: false,
            next_loadout: None,
            weapons: [
                WState {
                    id: l.primary,
                    mag: 0,
                    reserve: 0,
                },
                WState {
                    id: l.secondary,
                    mag: 0,
                    reserve: 0,
                },
            ],
            grenades: 0,
            smokes: 0,
            driving: None,
            shot_budget: 1.0,
            reload_end: None,
            switch_end: 0.0,
            melee_ready: 0.0,
            last_damage: -100.0,
            last_shot: -100.0,
            seen_at: [f64::NEG_INFINITY; MAX_PLAYERS],
            last_sprint: -100.0,
            protected_until: 0.0,
            shots_fired: 0,
            shots_hit: 0,
            kills: 0,
            deaths: 0,
            assists: 0,
            score: 0,
            streak: 0,
            rewards: Vec::new(),
            attackers: Vec::new(),
            respawn_at: 0.0,
            life: 0,
            ping: 0.05,
            ready: false,
            party: 0,
            lobby_wait: false,
            party_tag: 0,
            last_ping: f64::NEG_INFINITY,
            squad: 0,
            downed: false,
            downed_by: None,
            revive: 0.0,
            marker: None,
            open_box: None,
            spectating: None,
            spec_free: false,
            killed_by: None,
            downed_dist: 0,
            br_dmg: 0.0,
            br_longest: 0.0,
            br_place: 0,
            br_out_at: 0.0,
            last_seq: 0,
            move_budget: 2.0,
            history: VecDeque::new(),
            rcon: false,
            rcon_fails: 0,
            nonce: random_u64(),
            rcon_seq: 0,
            key: 0,
            progress: 0.0,
            joined_at: now,
            chat_budget: CHAT_BUDGET_MAX,
            lying: Default::default(),
            support_y: f32::MAX,
            by_wall: false,
            fall_top: None,
            kicks: 0,
            kicking: false,
            fall_life: 0,
            item_budget: ITEM_BUDGET_MAX,
            in_plane: false,
            inv: Default::default(),
            ammo_seen: [0; 2],
            empty: 0,
            scope_zoom: [0.0; 2],
            multi: 0,
            multi_at: f64::MIN,
            ms: MatchStats::default(),
            skin: 0,
        }
    }
    pub fn is_bot(&self) -> bool {
        self.bot.is_some()
    }
    pub fn eye(&self) -> Vec3 {
        self.body.pos + vec3(0.0, self.body.stance.eye(), 0.0)
    }
    pub fn chest(&self) -> Vec3 {
        self.body.pos + vec3(0.0, self.body.stance.height() * 0.62, 0.0)
    }
    pub fn weapon(&self) -> &WState {
        &self.weapons[self.slot as usize & 1]
    }
    pub fn weapon_mut(&mut self) -> &mut WState {
        &mut self.weapons[self.slot as usize & 1]
    }
    /// Position/yaw/stance at a past time (lag compensation).
    pub fn rewound(&self, t: f64) -> (Vec3, f32, Stance) {
        let h = &self.history;
        if h.is_empty() {
            return (self.body.pos, self.yaw, self.body.stance);
        }
        if t >= h.back().unwrap().t {
            return (self.body.pos, self.yaw, self.body.stance);
        }
        for i in (0..h.len()).rev() {
            if h[i].t <= t {
                let a = h[i];
                let b = if i + 1 < h.len() {
                    h[i + 1]
                } else {
                    return (a.pos, a.yaw, a.stance);
                };
                let k = ((t - a.t) / (b.t - a.t).max(1e-6)) as f32;
                return (
                    a.pos.lerp(b.pos, k),
                    a.yaw,
                    if k < 0.5 { a.stance } else { b.stance },
                );
            }
        }
        let a = h.front().unwrap();
        (a.pos, a.yaw, a.stance)
    }
}

impl Player {
    /// What this player's records are kept under: the installation's number, or the name
    /// for those without one.
    pub fn stats_key(&self) -> String {
        if self.key != 0 {
            format!("#{:016x}", self.key)
        } else {
            self.name.clone()
        }
    }
}
