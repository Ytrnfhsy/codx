//! Battle royale: everybody boards a plane that crosses the map, jumps wherever they like,
//! scavenges weapons and supplies, and fights everybody else inside a safe zone that keeps
//! shrinking. One life each; the last one standing wins.

use crate::game::{Dest, Game, WState};
use crate::pickups::Pickup;
use glam::{vec2, vec3, Vec2, Vec3};
use toon_shared::consts::MAX_HEALTH;
use toon_shared::inventory::{
    armor_durability, helmet_durability, AmmoType, MEDKIT_HEAL, MEDKIT_TIME,
};
use toon_shared::lang::t;
use toon_shared::physics::{Body, Chute, Stance};
use toon_shared::protocol::{BrSnap, EntSnap, LootKind, MatchState, S2C};
use toon_shared::tr;
use toon_shared::weapons::{Cause, WeaponId};
use toon_shared::{yaw_of, Mode};

/// The plane flies this high above the sea…
pub const PLANE_HEIGHT: f32 = 230.0;
/// …this fast (m/s).
pub const PLANE_SPEED: f32 = 75.0;
/// Walking this close to supplies picks them up.
const LOOT_RADIUS: f32 = 1.3;

/// How far sideways a skydiver gets from `height` metres above the ground: with the
/// parachute opened right away, or in free fall until it opens by itself.
pub fn glide_reach(height: f32, open_now: bool) -> f32 {
    use toon_shared::physics::{
        CHUTE_GLIDE, CHUTE_OPEN_HEIGHT, CHUTE_SINK, FREEFALL_GLIDE, FREEFALL_SPEED,
    };
    let h = height.max(0.0);
    if open_now {
        return h / CHUTE_SINK * CHUTE_GLIDE;
    }
    let fall = (h - CHUTE_OPEN_HEIGHT).max(0.0);
    fall / FREEFALL_SPEED * FREEFALL_GLIDE + h.min(CHUTE_OPEN_HEIGHT) / CHUTE_SINK * CHUTE_GLIDE
}

/// Zone phases: (wait before shrinking, shrinking time, radius after it, damage per second
/// outside). The first wait counts from the moment the plane takes off.
const PHASES: [(f32, f32, f32, f32); 6] = [
    (75.0, 50.0, 380.0, 1.0),
    (45.0, 40.0, 220.0, 2.0),
    (40.0, 35.0, 120.0, 4.0),
    (30.0, 30.0, 60.0, 7.0),
    (25.0, 25.0, 25.0, 10.0),
    (20.0, 20.0, 0.0, 15.0),
];

#[derive(Clone, Copy, Debug, Default)]
pub struct Zone {
    pub from_c: Vec2,
    pub from_r: f32,
    pub to_c: Vec2,
    pub to_r: f32,
    /// Index into `PHASES` of the shrink coming next (or under way).
    pub phase: usize,
    pub shrink_at: f64,
    pub shrink_end: f64,
    pub dps: f32,
}

impl Zone {
    /// Centre and radius at `time`.
    pub fn at(&self, time: f64) -> (Vec2, f32) {
        if time <= self.shrink_at || self.shrink_end <= self.shrink_at {
            return (self.from_c, self.from_r);
        }
        let k =
            ((time - self.shrink_at) / (self.shrink_end - self.shrink_at)).clamp(0.0, 1.0) as f32;
        (
            self.from_c.lerp(self.to_c, k),
            self.from_r + (self.to_r - self.from_r) * k,
        )
    }
    pub fn contains(&self, time: f64, p: Vec3) -> bool {
        let (c, r) = self.at(time);
        vec2(p.x, p.z).distance(c) <= r
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Loot {
    pub id: u16,
    pub kind: LootKind,
    /// Rounds, medkits, or per cent left of a vest or helmet.
    pub amount: u16,
    pub pos: Vec3,
}

#[derive(Clone, Debug, Default)]
pub struct BrState {
    /// A round is under way (the plane took off).
    pub running: bool,
    pub plane_from: Vec3,
    pub plane_dir: Vec3,
    pub plane_t0: f64,
    /// Seconds the plane spends over the map.
    pub plane_time: f32,
    pub zone: Zone,
    /// Players who boarded the plane.
    pub entrants: usize,
    pub loot: Vec<Loot>,
    /// Next whole second at which the zone hurts those outside it.
    pub next_burn: f64,
}

impl BrState {
    pub fn plane_pos(&self, time: f64) -> Vec3 {
        self.plane_from + self.plane_dir * PLANE_SPEED * (time - self.plane_t0) as f32
    }
    pub fn plane_flying(&self, time: f64) -> bool {
        self.running && ((time - self.plane_t0) as f32) < self.plane_time
    }
}

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
        let time = self.time;
        let half = self.map.size() * 0.5;
        let centre = self.map.center();
        // a random line across the map, passing near its middle
        let a = self.rng.range(0.0, std::f32::consts::TAU);
        let dir = vec3(a.cos(), 0.0, a.sin());
        let side = vec3(-dir.z, 0.0, dir.x) * self.rng.range(-0.25, 0.25) * half.x;
        let reach = half.max_element() * 1.05;
        let c3 = vec3(centre.x, PLANE_HEIGHT, centre.y);
        self.br = crate::br::BrState {
            running: true,
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
            p.body = Body {
                pos: start,
                ..Default::default()
            };
            p.inv = Default::default();
            p.empty_slot = None;
            p.driving = None;
            p.respawn_at = f64::MAX;
            if p.is_bot() {
                jumps.push(p.id);
            }
        }
        self.br.entrants = entrants;
        self.br_scatter_loot();
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

    /// Gives every bot its own landing spot, as far from the others as the flight path
    /// allows (by preference next to a gun), and the moment to jump: when the plane is
    /// closest to it.
    fn br_assign_drops(&mut self, bots: &[u8]) {
        if bots.is_empty() {
            return;
        }
        let from = self.br.plane_from;
        let dir = self.br.plane_dir;
        let run = PLANE_SPEED * self.br.plane_time;
        // what a bot can reach after leaving the plane (with some room to spare)
        let reach = glide_reach(PLANE_HEIGHT - 20.0, true) * 0.8;
        let (lo, hi) = (self.map.min, self.map.max);
        let margin = 25.0;
        let along_of = |q: Vec3| {
            let rel = vec3(q.x - from.x, 0.0, q.z - from.z);
            let along = rel.dot(dir);
            let side = (rel - dir * along).length();
            (along, side)
        };
        let fits = |q: Vec3| {
            let (along, side) = along_of(q);
            side < reach
                && along > run * 0.06
                && along < run * 0.94
                && q.x > lo.x + margin
                && q.x < hi.x - margin
                && q.z > lo.y + margin
                && q.z < hi.y - margin
        };
        // candidates: guns on the ground, then any walkable ground
        let mut cands: Vec<(Vec3, f32)> = self
            .pickups
            .iter()
            .map(|pk| pk.pos)
            .filter(|q| fits(*q))
            .map(|q| (q, 12.0))
            .collect();
        cands.retain(|c| self.reachable(c.0));
        for _ in 0..(bots.len() * 6).max(60) {
            let q = self.nav.random_point(&mut self.rng);
            if fits(q) {
                cands.push((q, 0.0));
            }
        }
        let mut chosen: Vec<Vec3> = Vec::new();
        for &id in bots {
            let pick = cands
                .iter()
                .enumerate()
                .map(|(i, (q, bonus))| {
                    let gap = chosen
                        .iter()
                        .map(|c| vec2(c.x - q.x, c.z - q.z).length())
                        .fold(400.0f32, f32::min);
                    (i, gap + bonus + self.rng.range(0.0, 6.0))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i);
            let time = self.time;
            let t0 = self.br.plane_t0;
            let window = self.br.plane_time as f64;
            let (land, at) = match pick {
                Some(i) => {
                    let (q, _) = cands.swap_remove(i);
                    let (along, _) = along_of(q);
                    // jump when the plane passes it
                    let at = t0 + (along / PLANE_SPEED) as f64;
                    (Some(q), at.clamp(time + 0.1 * window, time + 0.95 * window))
                }
                // nowhere left: anywhere along the way
                None => (None, time + self.rng.range(0.15, 0.8) as f64 * window),
            };
            if let Some(q) = land {
                chosen.push(q);
            }
            if let Some(b) = self.pm(id).and_then(|p| p.bot.as_mut()) {
                b.br_jump_at = at;
                b.br_land = land;
            }
        }
    }

    /// Picks where the zone shrinks to next.
    fn br_next_phase(&mut self, now: f64) {
        let z = self.br.zone;
        let (c, r) = z.at(now);
        let Some(&(wait, shrink, radius, dps)) = PHASES.get(z.phase) else {
            // the last circle stays put
            self.br.zone = Zone {
                from_c: c,
                from_r: r,
                to_c: c,
                to_r: r,
                shrink_at: f64::MAX,
                shrink_end: f64::MAX,
                dps: z.dps.max(15.0),
                phase: z.phase,
            };
            return;
        };
        // the next circle lies inside this one, preferably centred on dry land
        let mut to = c;
        for k in 0..24 {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let d = self.rng.range(0.0, (r - radius).max(0.0)) * if k < 20 { 1.0 } else { 0.3 };
            let cand = c + vec2(a.cos(), a.sin()) * d;
            let dry = self
                .world
                .terrain
                .as_ref()
                .is_none_or(|t| t.height(cand.x, cand.y) > t.water + 1.0);
            let inside = (cand - self.map.center()).abs().max_element()
                < (self.map.size().max_element() * 0.5 - radius * 0.5).max(0.0);
            if dry && inside {
                to = cand;
                break;
            }
        }
        self.br.zone = Zone {
            from_c: c,
            from_r: r,
            to_c: to,
            to_r: radius,
            phase: z.phase,
            shrink_at: now + wait as f64,
            shrink_end: now + (wait + shrink) as f64,
            dps,
        };
    }

    /// Weapons and supplies on the map's loot spots (or on random walkable ground).
    fn br_scatter_loot(&mut self) {
        self.pickups.clear();
        let mut spots = self.map.loot.clone();
        if spots.is_empty() {
            for _ in 0..80 {
                spots.push(self.nav.random_point(&mut self.rng) + vec3(0.0, 0.3, 0.0));
            }
        }
        for s in spots {
            let floor = self
                .world
                .floor_below(s + vec3(0.0, 0.2, 0.0), 2.0)
                .or_else(|| self.world.ground(s.x, s.z))
                .unwrap_or(s.y);
            let pos = vec3(s.x, floor, s.z);
            let roll = self.rng.f32();
            if roll < 0.12 {
                continue; // nothing here
            }
            if roll < 0.5 {
                // a gun with a loaded magazine, and a box of its rounds beside it
                let w = self.br_random_weapon();
                let mag = self.tuning.weapon(w).mag;
                let id = self.new_ent_id();
                let yaw = self.rng.range(0.0, std::f32::consts::TAU);
                self.pickups.push(Pickup {
                    id,
                    w: WState {
                        id: w,
                        mag,
                        reserve: 0,
                    },
                    pos,
                    yaw,
                    expire: f64::INFINITY,
                });
                let t = AmmoType::of(w);
                let id = self.new_ent_id();
                self.br.loot.push(Loot {
                    id,
                    kind: LootKind::Ammo(t),
                    amount: t.box_size() * 2,
                    pos: pos + vec3(0.7, 0.0, 0.2),
                });
            } else {
                let level = |rng: &mut toon_shared::rng::Rng| match rng.f32() {
                    k if k < 0.6 => 1,
                    k if k < 0.9 => 2,
                    _ => 3,
                };
                let (kind, amount) = match self.rng.f32() {
                    k if k < 0.36 => {
                        let t = AmmoType::ALL[self.rng.below(AmmoType::ALL.len())];
                        (LootKind::Ammo(t), t.box_size())
                    }
                    k if k < 0.56 => (LootKind::Medkit, 1 + self.rng.chance(0.25) as u16),
                    k if k < 0.7 => (LootKind::Armor(level(&mut self.rng)), 100),
                    k if k < 0.82 => (LootKind::Helmet(level(&mut self.rng)), 100),
                    k if k < 0.92 => (LootKind::Backpack(level(&mut self.rng)), 1),
                    _ => (LootKind::Grenades, 1),
                };
                let id = self.new_ent_id();
                self.br.loot.push(Loot {
                    id,
                    kind,
                    amount,
                    pos,
                });
            }
        }
    }

    fn br_random_weapon(&mut self) -> WeaponId {
        use WeaponId::*;
        // (weapon, weight): pistols and SMGs are common, snipers and machine guns rare
        const TABLE: [(WeaponId, f32); 15] = [
            (M9, 1.2),
            (Usp, 1.0),
            (Deagle, 0.5),
            (Mp5, 1.2),
            (P90, 0.9),
            (Ak74u, 0.9),
            (W1200, 0.8),
            (M1014, 0.5),
            (M4, 0.8),
            (Ak47, 0.8),
            (G3, 0.5),
            (M249, 0.3),
            (Rpd, 0.3),
            (M40, 0.35),
            (R700, 0.25),
        ];
        let total: f32 = TABLE.iter().map(|e| e.1).sum();
        let mut r = self.rng.range(0.0, total);
        for (w, k) in TABLE {
            if r < k {
                return w;
            }
            r -= k;
        }
        M9
    }

    /// Leaves the plane.
    pub fn br_jump(&mut self, id: u8) {
        let time = self.time;
        if self.mode != Mode::Br || !self.br.running {
            return;
        }
        // the plane starts and ends its run beyond the map's edge: whoever jumps there (or
        // is pushed out at the end) comes down just inside it
        let margin = 25.0;
        let p = self.br.plane_pos(time);
        let pos = vec3(
            p.x.clamp(self.map.min.x + margin, self.map.max.x - margin),
            p.y - 4.0,
            p.z.clamp(self.map.min.y + margin, self.map.max.y - margin),
        );
        let yaw = yaw_of(self.br.plane_dir);
        let tuning = &self.tuning;
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        if !p.in_plane {
            return;
        }
        let mag = tuning.weapon(WeaponId::M9).mag;
        p.in_plane = false;
        p.alive = true;
        p.hp = MAX_HEALTH;
        p.inv = Default::default();
        p.inv.ammo[AmmoType::Pistol.index()] = 15;
        p.empty_slot = Some(1);
        p.body = Body {
            pos,
            vel: p.body.vel,
            on_ground: false,
            stance: Stance::Stand,
            chute: Chute::FreeFall,
            ..Default::default()
        };
        p.yaw = yaw;
        p.pitch = 0.0;
        p.flags = 0;
        p.slot = 0;
        // a pistol to start with; the second slot waits for whatever you find
        p.weapons = [
            WState {
                id: WeaponId::M9,
                mag,
                reserve: mag,
            },
            WState {
                id: WeaponId::Usp,
                mag: 0,
                reserve: 0,
            },
        ];
        p.grenades = 0;
        p.smokes = 0;
        p.reload_end = None;
        p.switch_end = time;
        p.shot_budget = 1.0;
        p.attackers.clear();
        p.life = p.life.wrapping_add(1);
        p.protected_until = 0.0;
        p.history.clear();
        p.move_budget = 3.0;
        p.last_damage = -100.0;
        p.progress = 0.0;
        let life = p.life;
        let loadout = p.loadout;
        if let Some(b) = p.bot.as_mut() {
            b.on_spawn();
        }
        self.send(
            id,
            S2C::Spawned {
                pos: pos.to_array(),
                yaw,
                life,
                loadout,
            },
        );
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
        // medkits being used; reserves following the inventory
        for p in self.players.iter_mut().flatten() {
            if !p.alive {
                continue;
            }
            if p.inv.healing > 0.0 {
                p.inv.healing -= dt;
                if p.inv.healing <= 0.0 {
                    p.inv.healing = 0.0;
                    if p.inv.medkits > 0 {
                        p.inv.medkits -= 1;
                        p.hp = (p.hp + MEDKIT_HEAL).min(MAX_HEALTH);
                    }
                }
            }
            p.br_sync_ammo();
        }
        // no real player left in the round: nobody needs to watch the bots play it out
        let humans_in = self
            .players
            .iter()
            .flatten()
            .any(|p| !p.is_bot() && p.team.is_playing() && (p.alive || p.in_plane));
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
        // the end
        let alive = self.br_alive();
        if alive <= 1 && (self.br.entrants >= 2 || alive == 0) {
            let winner = self
                .players
                .iter()
                .flatten()
                .find(|p| p.team.is_playing() && (p.alive || p.in_plane))
                .map(|p| p.name.clone());
            self.br.running = false;
            let text = || match &winner {
                Some(n) => tr!("{n} — переможець!", "{n} wins!"),
                None => t("Переможця немає", "Nobody survived").into(),
            };
            self.end_match_text(text);
        }
    }

    /// Picks up battle royale supplies with the use key (returns false if `ent` is not
    /// supplies). What does not fit stays on the ground; a better vest, helmet or
    /// backpack is put on and the old one is left in its place.
    pub fn br_pick(&mut self, id: u8, ent: u16) -> bool {
        let Some(i) = self.br.loot.iter().position(|l| l.id == ent) else {
            return false;
        };
        let l = self.br.loot[i];
        let Some(p) = self.players[id as usize].as_mut() else {
            return true;
        };
        let d = l.pos - p.body.pos;
        if !p.alive || vec3(d.x, 0.0, d.z).length() > LOOT_RADIUS + 0.6 || d.y.abs() > 2.0 {
            return true;
        }
        let mut left = l.amount;
        // what it swaps out (dropped where the new one lay)
        let mut swapped: Option<(LootKind, u16)> = None;
        match l.kind {
            LootKind::Ammo(t) => {
                left -= p.inv.add_ammo(t, l.amount);
                p.br_refresh_reserve();
            }
            LootKind::Medkit => {
                let room = p.inv.medkit_cap().saturating_sub(p.inv.medkits) as u16;
                let take = room.min(l.amount);
                p.inv.medkits += take as u8;
                left -= take;
            }
            LootKind::Armor(lv) => {
                let hp = armor_durability(lv) * l.amount as f32 / 100.0;
                if lv > p.inv.armor || lv == p.inv.armor && hp > p.inv.armor_hp + 1.0 {
                    if p.inv.armor > 0 {
                        let pct = (p.inv.armor_hp / armor_durability(p.inv.armor) * 100.0) as u16;
                        swapped = Some((LootKind::Armor(p.inv.armor), pct.max(1)));
                    }
                    p.inv.armor = lv;
                    p.inv.armor_hp = hp;
                    left = 0;
                }
            }
            LootKind::Helmet(lv) => {
                let hp = helmet_durability(lv) * l.amount as f32 / 100.0;
                if lv > p.inv.helmet || lv == p.inv.helmet && hp > p.inv.helmet_hp + 1.0 {
                    if p.inv.helmet > 0 {
                        let pct =
                            (p.inv.helmet_hp / helmet_durability(p.inv.helmet) * 100.0) as u16;
                        swapped = Some((LootKind::Helmet(p.inv.helmet), pct.max(1)));
                    }
                    p.inv.helmet = lv;
                    p.inv.helmet_hp = hp;
                    left = 0;
                }
            }
            LootKind::Backpack(lv) => {
                if lv > p.inv.backpack {
                    if p.inv.backpack > 0 {
                        swapped = Some((LootKind::Backpack(p.inv.backpack), 1));
                    }
                    p.inv.backpack = lv;
                    left = 0;
                }
            }
            LootKind::Grenades => {
                let cap = p.inv.grenade_cap();
                if p.grenades < cap || p.smokes < 2 {
                    p.grenades = (p.grenades + 2).min(cap);
                    p.smokes = (p.smokes + 1).min(2);
                    left = 0;
                }
            }
        }
        let human = !p.is_bot();
        let gear = matches!(
            l.kind,
            LootKind::Armor(_) | LootKind::Helmet(_) | LootKind::Backpack(_)
        );
        let taken = left < l.amount || swapped.is_some() || left == 0;
        if left == 0 {
            self.br.loot.remove(i);
        } else {
            self.br.loot[i].amount = left;
        }
        if let Some((kind, amount)) = swapped {
            let id2 = self.new_ent_id();
            self.br.loot.push(Loot {
                id: id2,
                kind,
                amount,
                pos: l.pos,
            });
        }
        if human {
            let name = l.kind.name();
            self.say(Dest::One(id), || S2C::Center {
                text: if taken {
                    tr!("Підібрано: {name}", "Picked up: {name}")
                } else if gear {
                    t("На тобі вже не гірше", "What you wear is no worse").into()
                } else {
                    t(
                        "Більше не влазить — потрібен більший рюкзак",
                        "No room — get a bigger backpack",
                    )
                    .into()
                },
                secs: 1.2,
            });
        }
        true
    }

    /// Starts using a medkit (it takes a few seconds; shooting puts it away).
    pub fn br_use_medkit(&mut self, id: u8) {
        if self.mode != Mode::Br {
            return;
        }
        let Some(p) = self.pm(id) else { return };
        if p.alive && p.inv.medkits > 0 && p.inv.healing <= 0.0 && p.hp < MAX_HEALTH {
            p.inv.healing = MEDKIT_TIME;
        }
    }

    /// Everything a knocked out player carried goes on the ground around them.
    fn br_drop_inventory(&mut self, victim: u8) {
        let Some(v) = self.pm(victim) else { return };
        let inv = std::mem::take(&mut v.inv);
        let (grenades, smokes) = (
            std::mem::take(&mut v.grenades),
            std::mem::take(&mut v.smokes),
        );
        let pos = v.body.pos;
        let mut items: Vec<(LootKind, u16)> = Vec::new();
        for t in AmmoType::ALL {
            if inv.ammo(t) > 0 {
                items.push((LootKind::Ammo(t), inv.ammo(t)));
            }
        }
        if inv.medkits > 0 {
            items.push((LootKind::Medkit, inv.medkits as u16));
        }
        if inv.armor > 0 {
            let pct = (inv.armor_hp / armor_durability(inv.armor) * 100.0) as u16;
            items.push((LootKind::Armor(inv.armor), pct.max(1)));
        }
        if inv.helmet > 0 {
            let pct = (inv.helmet_hp / helmet_durability(inv.helmet) * 100.0) as u16;
            items.push((LootKind::Helmet(inv.helmet), pct.max(1)));
        }
        if inv.backpack > 0 {
            items.push((LootKind::Backpack(inv.backpack), 1));
        }
        if grenades > 0 || smokes > 0 {
            items.push((LootKind::Grenades, 1));
        }
        let n = items.len().max(1) as f32;
        for (k, (kind, amount)) in items.into_iter().enumerate() {
            // in a small ring round the body
            let a = k as f32 / n * std::f32::consts::TAU;
            let at = pos + vec3(a.cos() * 0.9, 0.3, a.sin() * 0.9);
            let floor = self
                .world
                .floor_below(at, 3.0)
                .or_else(|| self.world.ground(at.x, at.z))
                .unwrap_or(pos.y);
            let id = self.new_ent_id();
            self.br.loot.push(Loot {
                id,
                kind,
                amount,
                pos: vec3(at.x, floor, at.z),
            });
        }
    }

    /// A player is out: tell them their place.
    pub fn br_on_death(&mut self, victim: u8) {
        if !self.br.running {
            return;
        }
        self.br_drop_inventory(victim);
        let place = self.br_alive() + 1;
        self.say(Dest::One(victim), || S2C::Center {
            text: tr!("Ти вибув! Місце #{place}", "You are out! Place #{place}"),
            secs: 5.0,
        });
    }

    pub fn br_snap(&self, id: u8) -> Option<BrSnap> {
        if self.mode != Mode::Br {
            return None;
        }
        let time = self.time;
        let z = &self.br.zone;
        let (c, r) = z.at(time);
        let shrinking = time > z.shrink_at && time < z.shrink_end;
        let timer = if shrinking {
            z.shrink_end - time
        } else {
            z.shrink_at - time
        };
        let me = self.p(id);
        Some(BrSnap {
            alive: self.br_alive() as u16,
            zone: if self.br.running {
                [c.x, c.y, r]
            } else {
                [0.0, 0.0, 0.0]
            },
            next: [z.to_c.x, z.to_c.y, z.to_r],
            timer: if self.br.running {
                timer.clamp(0.0, 9999.0) as f32
            } else {
                0.0
            },
            shrinking,
            dps: z.dps,
            plane: self.br.plane_flying(time).then(|| {
                (
                    self.br.plane_pos(time).to_array(),
                    yaw_of(self.br.plane_dir),
                )
            }),
            in_plane: me.is_some_and(|p| p.in_plane),
            inv: me.map(|p| p.inv).unwrap_or_default(),
        })
    }

    pub fn br_loot_snaps(&self) -> impl Iterator<Item = EntSnap> + '_ {
        self.br.loot.iter().map(|l| EntSnap::Loot {
            id: l.id,
            kind: l.kind,
            amount: l.amount,
            pos: l.pos.to_array(),
        })
    }

    /// Where a battle royale bot heads: into the zone first, then to a gun if it has
    /// none worth the name, then to supplies it needs, else roaming inside the zone.
    pub fn br_bot_goal(
        &mut self,
        id: u8,
        bot: &mut crate::bots::Bot,
        pos: Vec3,
    ) -> (Option<Vec3>, bool, bool) {
        let time = self.time;
        let zone = self.br.zone;
        let flat = vec2(pos.x, pos.z);
        // the circle that matters: the next one when the zone is about to move
        let (c, r) = if time > zone.shrink_at - 25.0 {
            (zone.to_c, zone.to_r)
        } else {
            zone.at(time)
        };
        let Some(me) = self.p(id) else {
            return (None, false, false);
        };
        let skydiving = me.body.chute != Chute::None;
        if skydiving {
            if let Some(l) = bot.br_land {
                return (Some(l), false, false);
            }
        } else {
            bot.br_land = None;
        }
        if !skydiving && flat.distance(c) > r * 0.85 + 3.0 {
            let inside = c + (flat - c).normalize_or_zero() * r * 0.5;
            let g = self
                .nav
                .snap(vec3(inside.x, pos.y, inside.y))
                .unwrap_or(vec3(inside.x, pos.y, inside.y));
            return (Some(g), false, true);
        }
        let armed = me.weapons.iter().any(|w| {
            w.id.def().class != toon_shared::weapons::WeaponClass::Pistol && w.mag + w.reserve > 0
        });
        let (hp, inv) = (me.hp, me.inv);
        let carried: Vec<AmmoType> = me.weapons.iter().map(|w| AmmoType::of(w.id)).collect();
        let have: Vec<WeaponId> = me
            .weapons
            .iter()
            .filter(|w| w.mag + w.reserve > 0)
            .map(|w| w.id)
            .collect();
        // ammo is only worth a detour while some gun is running low
        let low_ammo = me
            .weapons
            .iter()
            .any(|w| w.mag + w.reserve > 0 && w.reserve < self.tuning.weapon(w.id).mag * 2);
        let near = |q: Vec3| vec2(q.x, q.z).distance(flat);
        if !armed {
            let range = if skydiving { 220.0 } else { 90.0 };
            let mut cands: Vec<(u16, Vec3)> = self
                .pickups
                .iter()
                // a real gun we do not have yet (a pickup we cannot take would hold us there)
                .filter(|pk| {
                    near(pk.pos) < range
                        && zone.contains(time, pk.pos)
                        && pk.w.id.def().class != toon_shared::weapons::WeaponClass::Pistol
                        && !have.contains(&pk.w.id)
                })
                .map(|pk| (pk.id, pk.pos))
                .collect();
            cands.sort_by(|a, b| near(a.1).total_cmp(&near(b.1)));
            // only what we can walk to (the coarse map of a big island does not lead up
            // every staircase)
            let best = cands
                .into_iter()
                .take(6)
                .find(|(_, at)| self.reachable(*at));
            if let Some((ent, at)) = best {
                if !skydiving
                    && near(at) < crate::pickups::PICKUP_RADIUS
                    && (at.y - pos.y).abs() < 1.8
                {
                    self.pick_up(id, ent);
                }
                return (Some(at), false, false);
            }
        }
        if !skydiving {
            // a medkit when hurt (and not using one already)
            if hp < 70.0 && inv.medkits > 0 && inv.healing <= 0.0 {
                self.br_use_medkit(id);
            }
            let want = |k: LootKind| match k {
                LootKind::Medkit => inv.medkits < inv.medkit_cap(),
                LootKind::Armor(l) => l > inv.armor,
                LootKind::Helmet(l) => l > inv.helmet,
                LootKind::Backpack(l) => l > inv.backpack,
                LootKind::Ammo(t) => {
                    carried.contains(&t) && inv.ammo(t) < inv.ammo_cap(t) / 2 && low_ammo
                        || carried.contains(&t) && inv.ammo(t) < t.box_size() * 2
                }
                LootKind::Grenades => false,
            };
            let mut cands: Vec<(u16, Vec3)> = self
                .br
                .loot
                .iter()
                .filter(|l| want(l.kind) && near(l.pos) < 45.0)
                .map(|l| (l.id, l.pos))
                .collect();
            cands.sort_by(|a, b| near(a.1).total_cmp(&near(b.1)));
            let best = cands.into_iter().take(4).find(|c| self.reachable(c.1));
            if let Some((ent, at)) = best {
                if near(at) < LOOT_RADIUS && (at.y - pos.y).abs() < 1.8 {
                    self.br_pick(id, ent);
                }
                return (Some(at), false, false);
            }
        }
        // roam somewhere inside the zone
        let reached = bot.roam_goal.map(|g| near(g) < 2.5).unwrap_or(true);
        let stale = bot
            .roam_goal
            .is_some_and(|g| vec2(g.x, g.z).distance(c) > r);
        if reached || stale || time > bot.roam_until {
            bot.roam_until = time + 25.0;
            let mut g = None;
            for _ in 0..12 {
                let q = self.nav.random_point(&mut self.rng);
                if vec2(q.x, q.z).distance(c) < r * 0.8 {
                    g = Some(q);
                    break;
                }
            }
            bot.roam_goal = Some(g.unwrap_or(vec3(c.x, pos.y, c.y)));
        }
        (bot.roam_goal, false, false)
    }

    /// A bot can walk to this point (the navigation map has ground right there).
    fn reachable(&self, at: Vec3) -> bool {
        self.nav.snap(at).is_some_and(|n| {
            (n.y - at.y).abs() < 0.8 && vec2(n.x - at.x, n.z - at.z).length() < 2.5
        })
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
        p.empty_slot = None;
        p.respawn_at = f64::MAX;
        if let Some(b) = p.bot.as_mut() {
            b.br_jump_at = jump;
        }
        self.br.entrants += 1;
    }
}
