//! Battle royale: everybody boards a plane that crosses the map, jumps wherever they like,
//! scavenges weapons and supplies, and fights everybody else inside a safe zone that keeps
//! shrinking. One life each; the last one standing wins.

use crate::game::{Dest, Game, WState};
use crate::pickups::Pickup;
use glam::{vec2, vec3, Vec2, Vec3};
use toon_shared::consts::MAX_HEALTH;
use toon_shared::inventory::{
    armor_durability, helmet_durability, mag_size, mod_fits, mod_place, sight_fits, AmmoType,
    BANDAGE_HEAL, BANDAGE_MAX, BANDAGE_TIME, DRINK_BOOST, DRINK_TIME, MAX_SIGHT, MEDKIT_HEAL,
    MEDKIT_TIME, MODS, MOD_EXTMAG, MOD_SUPPRESSOR, USE_BANDAGE, USE_DRINK, USE_MEDKIT,
};
use toon_shared::lang::t;
use toon_shared::physics::{Body, Chute, Stance};
use toon_shared::protocol::{flags, BrSnap, EntSnap, Item, ItemAction, LootKind, MatchState, S2C};
use toon_shared::tr;
use toon_shared::weapons::{Cause, WeaponId};
use toon_shared::{look_dir, yaw_of, Mode};

/// The plane flies this high above the sea…
pub const PLANE_HEIGHT: f32 = 230.0;
/// …this fast (m/s).
pub const PLANE_SPEED: f32 = 75.0;
/// Walking this close to supplies picks them up.
const LOOT_RADIUS: f32 = 1.3;
/// A knocked-down player's pool of health: it bleeds out over `BLEED_TIME` seconds and
/// takes hits like normal health (no vest or helmet).
pub const DOWNED_HP: f32 = 100.0;
pub const BLEED_TIME: f32 = 40.0;
/// Seconds of holding the use key next to a downed squadmate to pick them up, and the
/// health they get back.
pub const REVIVE_TIME: f32 = 5.0;
pub const REVIVE_HP: f32 = 30.0;

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
    /// Supply crates coming down (or down) and when the next one comes.
    pub drops: Vec<AirDrop>,
    pub next_drop: f64,
    /// Crates sent this round.
    pub launched: usize,
    /// What knocked-out players left.
    pub boxes: Vec<crate::boxes::DeathBox>,
    /// The bombarded area, if any, and when the next one comes.
    pub red: Option<RedZone>,
    /// This round's weather and time of day.
    pub weather: toon_shared::weather::Weather,
    pub next_red: f64,
}

/// An area shelled for a while (after a warning): buildings and cover protect.
#[derive(Clone, Copy, Debug)]
pub struct RedZone {
    pub c: Vec2,
    pub r: f32,
    /// Shells fall from this moment until `end`.
    pub start: f64,
    pub end: f64,
    pub next_shell: f64,
}

const RED_FIRST: f64 = 130.0;
const RED_EVERY: f64 = 90.0;
const RED_WARNING: f64 = 8.0;
const RED_TIME: f64 = 20.0;

/// A supply crate: it comes down under a parachute inside the next circle and spills rare
/// gear around it when it lands.
#[derive(Clone, Debug)]
pub struct AirDrop {
    pub id: u16,
    pub pos: Vec3,
    pub ground: f32,
    pub landed: bool,
    /// The loot it left (gone once all of it is taken).
    pub items: Vec<u16>,
}

/// Supply crates per round, the first one this long after the plane leaves, then apart.
const DROPS: usize = 3;
const FIRST_DROP: f64 = 100.0;
const DROP_EVERY: f64 = 120.0;
/// How fast a crate sinks under its parachute.
const DROP_SINK: f32 = 5.0;

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
        self.br = crate::br::BrState {
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
            p.body = Body {
                pos: start,
                ..Default::default()
            };
            p.inv = Default::default();
            p.empty_slot = None;
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
        // squadmates come down together: the first of a squad picks, the others follow
        let mut squad_spot: Vec<(u8, Vec3, f64)> = Vec::new();
        for &id in bots {
            let squad = self.p(id).map_or(0, |p| p.squad);
            if let Some(&(_, spot, at)) = squad_spot.iter().find(|s| s.0 == squad && squad != 0) {
                let n = squad_spot.len() as f32;
                let off = vec3((n * 2.4).cos() * 7.0, 0.0, (n * 2.4).sin() * 7.0);
                let land = self.nav.snap(spot + off).unwrap_or(spot);
                if let Some(b) = self.pm(id).and_then(|p| p.bot.as_mut()) {
                    b.br_jump_at = at + 0.3;
                    b.br_land = Some(land);
                }
                squad_spot.push((squad, spot, at));
                continue;
            }
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
                if squad != 0 {
                    squad_spot.push((squad, q, at));
                }
            }
            if let Some(b) = self.pm(id).and_then(|p| p.bot.as_mut()) {
                b.br_jump_at = at;
                b.br_land = land;
            }
        }
    }

    /// Splits the players into squads (real players first, in the order they came, so that
    /// friends who joined together play together; bots fill up the rest).
    fn br_make_squads(&mut self) {
        let size = self.cfg.br.squad.clamp(1, 4) as usize;
        if size < 2 {
            return;
        }
        let mut ids: Vec<u8> = self
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
        ids.extend(bots);
        for (k, id) in ids.into_iter().enumerate() {
            if let Some(p) = self.pm(id) {
                p.squad = (k / size) as u8 + 1;
            }
        }
    }

    /// Whom a player out of the round may watch: their squadmates while any is still in
    /// (no peeking at the enemy for them), otherwise anybody.
    fn br_watchable(&self, id: u8) -> Vec<u8> {
        let squad = self.p(id).map_or(0, |p| p.squad);
        let alive = |p: &&crate::game::Player| p.team.is_playing() && p.alive && p.id != id;
        let mates: Vec<u8> = self
            .players
            .iter()
            .flatten()
            .filter(alive)
            .filter(|p| squad != 0 && p.squad == squad)
            .map(|p| p.id)
            .collect();
        if !mates.is_empty() {
            return mates;
        }
        self.players
            .iter()
            .flatten()
            .filter(alive)
            .map(|p| p.id)
            .collect()
    }

    /// The next (or previous) player to watch.
    pub fn br_spectate_step(&mut self, id: u8, step: i32) {
        if self.mode != Mode::Br || self.p(id).is_none_or(|p| p.alive) {
            return;
        }
        let list = self.br_watchable(id);
        let cur = self.p(id).and_then(|p| p.spectating);
        let next = if list.is_empty() {
            None
        } else {
            let i = cur.and_then(|c| list.iter().position(|x| *x == c));
            let k = match i {
                Some(i) => (i as i32 + step).rem_euclid(list.len() as i32) as usize,
                None => 0,
            };
            Some(list[k])
        };
        if let Some(p) = self.pm(id) {
            p.spectating = next;
            p.spec_free = false;
        }
    }

    /// Players out of the round always have somebody to watch: the one who got them, a
    /// squadmate, or whoever is nearest.
    fn br_spectators(&mut self) {
        for id in 0..self.players.len() as u8 {
            let Some(p) = self.p(id) else { continue };
            if p.is_bot() || p.alive || p.in_plane || p.spec_free || !p.team.is_playing() {
                continue;
            }
            let list = self.br_watchable(id);
            if p.spectating.is_some_and(|t| list.contains(&t)) {
                continue;
            }
            let at = p.body.pos;
            let pick = p.killed_by.filter(|k| list.contains(k)).or_else(|| {
                list.iter().copied().min_by(|a, b| {
                    let d = |x: &u8| self.p(*x).map_or(f32::MAX, |q| q.body.pos.distance(at));
                    d(a).total_cmp(&d(b))
                })
            });
            if let Some(p) = self.pm(id) {
                p.spectating = pick;
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

    /// Knocked-down players bleed and are picked up by squadmates holding the use key next
    /// to them; a squad with nobody standing is out.
    fn br_downed(&mut self, dt: f32) {
        let mut out: Vec<u8> = Vec::new();
        let ids: Vec<u8> = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.alive && p.squad != 0)
            .map(|p| p.id)
            .collect();
        for &id in &ids {
            if let Some(p) = self.pm(id) {
                p.progress = 0.0;
            }
        }
        for &id in &ids {
            let Some(v) = self.p(id) else { continue };
            if !v.downed {
                continue;
            }
            let (squad, pos) = (v.squad, v.body.pos);
            let standing = self
                .players
                .iter()
                .flatten()
                .any(|o| o.squad == squad && o.alive && !o.downed);
            if !standing {
                out.push(id);
                continue;
            }
            // a squadmate holding the use key right next to them
            let reviver = self
                .players
                .iter()
                .flatten()
                .find(|o| {
                    o.squad == squad
                        && o.alive
                        && !o.downed
                        && o.flags & flags::USE != 0
                        && o.body.pos.distance(pos) < 2.0
                })
                .map(|o| o.id);
            let now = self.time;
            let Some(v) = self.pm(id) else { continue };
            match reviver {
                Some(_) => v.revive += dt / REVIVE_TIME,
                None => {
                    v.revive = 0.0;
                    v.hp -= DOWNED_HP / BLEED_TIME * dt;
                }
            }
            let revive = v.revive;
            if revive >= 1.0 {
                v.downed = false;
                v.downed_by = None;
                v.revive = 0.0;
                v.hp = REVIVE_HP;
                v.last_damage = now;
                let name = v.name.clone();
                if let Some(r) = reviver {
                    self.say(Dest::One(r), || S2C::Center {
                        text: tr!("Піднято: {name}", "Picked up: {name}"),
                        secs: 1.5,
                    });
                }
                continue;
            }
            v.progress = revive;
            if v.hp <= 0.0 {
                out.push(id);
            }
            if let Some(r) = reviver.and_then(|r| self.pm(r)) {
                r.progress = revive;
            }
        }
        for id in out {
            self.kill(id, None, Cause::Bleed, false);
        }
    }

    /// The red zone: marked inside the circle (away from the next one), then shelled.
    fn br_red_zone(&mut self) {
        let time = self.time;
        if let Some(rz) = self.br.red {
            if time >= rz.end {
                self.br.red = None;
            } else if time >= rz.start && time >= rz.next_shell {
                let mut z = rz;
                z.next_shell = time + self.rng.range(0.25, 0.6) as f64;
                self.br.red = Some(z);
                let a = self.rng.range(0.0, std::f32::consts::TAU);
                let d = self.rng.f32().sqrt() * rz.r;
                let q = rz.c + vec2(a.cos(), a.sin()) * d;
                // the shell lands on whatever is highest there (a roof shields those below)
                let top = self
                    .world
                    .floor_below(vec3(q.x, 300.0, q.y), 400.0)
                    .unwrap_or(0.0);
                self.explode(
                    vec3(q.x, top + 0.3, q.y),
                    6.0,
                    95.0,
                    None,
                    Cause::Artillery,
                    true,
                );
            }
            return;
        }
        if time < self.br.next_red || self.state != MatchState::Playing {
            return;
        }
        self.br.next_red = time + RED_EVERY;
        let (c, r) = self.br.zone.at(time);
        if r < 160.0 {
            return; // the end game is tight enough
        }
        let next = vec2(self.br.zone.to_c.x, self.br.zone.to_c.y);
        let size = self.rng.range(80.0, 110.0);
        for _ in 0..20 {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let d = self.rng.range(0.0, (r - size).max(0.0));
            let q = c + vec2(a.cos(), a.sin()) * d;
            if q.distance(next) > self.br.zone.to_r + size * 0.5 {
                self.br.red = Some(RedZone {
                    c: q,
                    r: size,
                    start: time + RED_WARNING,
                    end: time + RED_WARNING + RED_TIME,
                    next_shell: time + RED_WARNING,
                });
                self.center_all(
                    || {
                        t(
                            "Червона зона! Сховайся в будинку або тікай з неї",
                            "Red zone! Get indoors or get out of it",
                        )
                        .into()
                    },
                    3.5,
                );
                return;
            }
        }
    }

    /// Supply crates: a new one now and then, falling ones come down, landed ones spill
    /// their gear; a crate emptied of it goes away.
    fn br_airdrops(&mut self, dt: f32) {
        let time = self.time;
        if time >= self.br.next_drop {
            self.br.next_drop = time + DROP_EVERY;
            if self.br.launched < DROPS {
                self.br_launch_drop();
            }
        }
        let mut landed = Vec::new();
        for (i, d) in self.br.drops.iter_mut().enumerate() {
            if !d.landed {
                d.pos.y -= DROP_SINK * dt;
                if d.pos.y <= d.ground {
                    d.pos.y = d.ground;
                    d.landed = true;
                    landed.push(i);
                }
            }
        }
        for i in landed {
            self.br_fill_drop(i);
        }
        let loot = &self.br.loot;
        let pickups = &self.pickups;
        self.br.drops.retain(|d| {
            !d.landed
                || d.items.iter().any(|id| {
                    loot.iter().any(|l| l.id == *id) || pickups.iter().any(|p| p.id == *id)
                })
        });
    }

    /// Where the next crate comes down: dry, walkable ground inside the next circle.
    fn br_launch_drop(&mut self) {
        let z = self.br.zone;
        let mut spot = None;
        for _ in 0..40 {
            let a = self.rng.range(0.0, std::f32::consts::TAU);
            let r = self.rng.range(0.0, z.to_r.max(30.0) * 0.7);
            let q = z.to_c + vec2(a.cos(), a.sin()) * r;
            let Some(n) = self.nav.snap(vec3(q.x, 400.0, q.y)) else {
                continue;
            };
            let dry = self
                .world
                .terrain
                .as_ref()
                .is_none_or(|t| t.height(n.x, n.z) > t.water + 0.5);
            // in the open (not on a roof or inside a house)
            let open = self
                .world
                .ground(n.x, n.z)
                .is_none_or(|g| (n.y - g).abs() < 0.5);
            if dry && open {
                spot = Some(n);
                break;
            }
        }
        let Some(at) = spot else { return };
        let id = self.new_ent_id();
        self.br.drops.push(AirDrop {
            id,
            pos: vec3(at.x, at.y + 170.0, at.z),
            ground: at.y,
            landed: false,
            items: Vec::new(),
        });
        self.br.launched += 1;
        self.center_all(
            || {
                t(
                    "Скидання припасів! Позначено на карті",
                    "Supply drop! Marked on the map",
                )
                .into()
            },
            3.0,
        );
    }

    /// A crate has landed: the rare gear inside goes round it.
    fn br_fill_drop(&mut self, i: usize) {
        let at = self.br.drops[i].pos;
        let mut items: Vec<(LootKind, u16)> = vec![
            (LootKind::Sight(4), 1),
            (LootKind::Medkit, 3),
            (LootKind::Mod(MOD_SUPPRESSOR), 1),
        ];
        let gear = [
            LootKind::Armor(3),
            LootKind::Helmet(3),
            LootKind::Backpack(3),
        ];
        let skip = self.rng.below(3);
        for (k, g) in gear.into_iter().enumerate() {
            if k != skip {
                items.push((
                    g,
                    if matches!(g, LootKind::Backpack(_)) {
                        1
                    } else {
                        100
                    },
                ));
            }
        }
        let guns = [WeaponId::M40, WeaponId::R700, WeaponId::M249, WeaponId::G3];
        let gun = guns[self.rng.below(guns.len())];
        let ammo = AmmoType::of(gun);
        items.push((LootKind::Ammo(ammo), ammo.box_size() * 3));
        let mut ids = Vec::new();
        let n = items.len() as f32 + 1.0;
        for (k, (kind, amount)) in items.into_iter().enumerate() {
            let a = k as f32 / n * std::f32::consts::TAU;
            let p = at + vec3(a.cos() * 1.8, 0.3, a.sin() * 1.8);
            let id = self.new_ent_id();
            let floor = self
                .world
                .floor_below(p + vec3(0.0, 1.0, 0.0), 3.0)
                .unwrap_or(at.y);
            self.br.loot.push(Loot {
                id,
                kind,
                amount,
                pos: vec3(p.x, floor, p.z),
            });
            ids.push(id);
        }
        // the gun on top of the crate
        let id = self.new_ent_id();
        let mag = self.tuning.weapon(gun).mag;
        let yaw = self.rng.range(0.0, std::f32::consts::TAU);
        self.pickups.push(Pickup {
            id,
            w: WState {
                id: gun,
                mag,
                reserve: 0,
            },
            pos: at + vec3(0.0, 1.45, 0.0),
            yaw,
            expire: f64::INFINITY,
        });
        ids.push(id);
        self.br.drops[i].items = ids;
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
        // the circles were made for a 1 km island: bigger maps get bigger ones, and a
        // little more time to cross them
        let k = (self.map.size().max_element() / 1000.0).max(1.0);
        let (wait, shrink, radius) = (wait * k.sqrt(), shrink * k.sqrt(), radius * k);
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
                    k if k < 0.30 => {
                        let t = AmmoType::ALL[self.rng.below(AmmoType::ALL.len())];
                        (LootKind::Ammo(t), t.box_size())
                    }
                    k if k < 0.40 => (LootKind::Medkit, 1 + self.rng.chance(0.25) as u16),
                    k if k < 0.47 => (LootKind::Bandages, 3 + self.rng.below(3) as u16),
                    k if k < 0.51 => (LootKind::Drink, 1),
                    k if k < 0.62 => (LootKind::Armor(level(&mut self.rng)), 100),
                    k if k < 0.72 => (LootKind::Helmet(level(&mut self.rng)), 100),
                    k if k < 0.79 => (LootKind::Backpack(level(&mut self.rng)), 1),
                    // other attachments
                    k if k < 0.85 => (LootKind::Mod(MODS[self.rng.below(MODS.len())]), 1),
                    // sights: red dots and 2× common, 8× rare
                    k if k < 0.91 => {
                        let l = match self.rng.f32() {
                            r if r < 0.35 => 1,
                            r if r < 0.65 => 2,
                            r if r < 0.9 => 3,
                            _ => 4,
                        };
                        (LootKind::Sight(l), 1)
                    }
                    k if k < 0.94 => (LootKind::Molotov, 1),
                    k if k < 0.96 => (LootKind::Flash, 1),
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
        // and one medkit to start with
        p.inv.medkits = 1;
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
        if !p.alive
            || p.downed
            || vec3(d.x, 0.0, d.z).length() > crate::pickups::PICKUP_RADIUS + 0.3
            || d.y.abs() > 2.0
        {
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
            LootKind::Sight(lv) => {
                // on the gun in hand if it fits and is better, otherwise on the other one
                let slot = p.slot as usize;
                let fits = |k: usize| {
                    let w = p.weapons[k];
                    (w.mag + w.reserve > 0 || Some(k as u8) != p.empty_slot)
                        && sight_fits(lv, w.id)
                        && lv > p.inv.sights[k]
                };
                if let Some(k) = [slot, 1 - slot].into_iter().find(|k| fits(*k)) {
                    // the old one goes in the backpack, or on the ground if it is full
                    let old = p.inv.sights[k];
                    p.inv.sights[k] = lv;
                    if old > 0 && !p.inv.stash_sight(old) {
                        swapped = Some((LootKind::Sight(old), 1));
                    }
                    left = 0;
                } else if p.inv.stash_sight(lv) {
                    // for a gun found later
                    left = 0;
                }
            }
            LootKind::Mod(kind) => {
                // on a gun in hand that takes it and has its place free, otherwise in the
                // backpack
                let slot = p.slot as usize;
                let place = mod_place(kind);
                let fits = |k: usize| {
                    Some(k as u8) != p.empty_slot
                        && mod_fits(kind, p.weapons[k].id)
                        && p.inv.mods[k][place] == 0
                };
                if let Some(k) = [slot, 1 - slot].into_iter().find(|k| fits(*k)) {
                    p.inv.mods[k][place] = kind;
                    left = 0;
                } else if p.inv.stash_mod(kind) {
                    left = 0;
                }
            }
            LootKind::Bandages | LootKind::Drink | LootKind::Molotov | LootKind::Flash => {
                let (have, cap) = match l.kind {
                    LootKind::Bandages => (&mut p.inv.bandages, 5 + p.inv.backpack * 3),
                    LootKind::Drink => (&mut p.inv.drinks, 2 + p.inv.backpack),
                    LootKind::Molotov => (&mut p.inv.molotovs, 1 + p.inv.backpack),
                    _ => (&mut p.inv.flashes, 1 + p.inv.backpack),
                };
                let take = (cap.saturating_sub(*have) as u16).min(l.amount);
                *have += take as u8;
                left -= take;
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
            LootKind::Armor(_)
                | LootKind::Helmet(_)
                | LootKind::Backpack(_)
                | LootKind::Sight(_)
                | LootKind::Mod(_)
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
                    if matches!(l.kind, LootKind::Sight(_)) {
                        t(
                            "Не підходить або вже є не гірший",
                            "Does not fit, or yours is no worse",
                        )
                    } else {
                        t("На тобі вже не гірше", "What you wear is no worse")
                    }
                    .into()
                } else {
                    t(
                        "Більше не влазить — потрібен більший рюкзак або викинь щось (I)",
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
        self.br_use(id, USE_MEDKIT);
    }

    /// Starts using a medkit, a bandage or an energy drink.
    pub fn br_use(&mut self, id: u8, what: u8) {
        if self.mode != Mode::Br || self.seat_of(id).is_some_and(|s| s.1 == 0) {
            return;
        }
        let Some(p) = self.pm(id) else { return };
        if !p.alive || p.downed || p.inv.healing > 0.0 {
            return;
        }
        let (ok, time) = match what {
            USE_MEDKIT => (p.inv.medkits > 0 && p.hp < MAX_HEALTH, MEDKIT_TIME),
            USE_BANDAGE => (p.inv.bandages > 0 && p.hp < BANDAGE_MAX, BANDAGE_TIME),
            USE_DRINK => (p.inv.drinks > 0 && p.inv.boost < 100.0, DRINK_TIME),
            _ => (false, 0.0),
        };
        if ok {
            p.inv.healing = time;
            p.inv.using = what;
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
        for l in inv.sights {
            if l > 0 {
                items.push((LootKind::Sight(l), 1));
            }
        }
        for (k, n) in inv.spare_sights.iter().enumerate() {
            for _ in 0..*n {
                items.push((LootKind::Sight(k as u8 + 1), 1));
            }
        }
        for (n, kind) in [
            (inv.bandages, LootKind::Bandages),
            (inv.drinks, LootKind::Drink),
            (inv.molotovs, LootKind::Molotov),
            (inv.flashes, LootKind::Flash),
        ] {
            if n > 0 {
                items.push((kind, n as u16));
            }
        }
        for m in inv.mods.iter().flatten() {
            if *m != 0 {
                items.push((LootKind::Mod(*m), 1));
            }
        }
        for (k, n) in inv.spare_mods.iter().enumerate() {
            for _ in 0..*n {
                items.push((LootKind::Mod(k as u8 + 1), 1));
            }
        }
        // all of it in one box where they fell
        let _ = pos;
        let b = self.br_box_of(victim);
        self.br.boxes[b].items.extend(items);
        self.br
            .boxes
            .retain(|b| !b.items.is_empty() || !b.guns.is_empty());
    }

    /// The inventory screen: throwing things away and moving sights between the guns
    /// and the backpack.
    pub fn br_item(&mut self, id: u8, action: ItemAction) {
        if self.mode != Mode::Br || !self.br.running {
            return;
        }
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        if !p.alive || p.in_plane || p.item_budget < 1.0 {
            return;
        }
        p.item_budget -= 1.0;
        p.br_sync_ammo();
        // things land a step in front
        let at = p.body.pos + look_dir(p.yaw, 0.0) * 0.9;
        let mut out: Vec<(LootKind, u16)> = Vec::new();
        let mut gun: Option<WState> = None;
        match action {
            ItemAction::Drop(item, n) => match item {
                Item::Ammo(t) => {
                    let n = n.min(p.inv.ammo(t));
                    if n > 0 {
                        p.inv.ammo[t.index()] -= n;
                        out.push((LootKind::Ammo(t), n));
                    }
                }
                Item::Medkit => {
                    let n = (n as u8).min(p.inv.medkits);
                    if n > 0 {
                        p.inv.medkits -= n;
                        p.inv.healing = 0.0;
                        out.push((LootKind::Medkit, n as u16));
                    }
                }
                Item::Armor if p.inv.armor > 0 => {
                    let pct = (p.inv.armor_hp / armor_durability(p.inv.armor) * 100.0) as u16;
                    out.push((LootKind::Armor(p.inv.armor), pct.max(1)));
                    p.inv.armor = 0;
                    p.inv.armor_hp = 0.0;
                }
                Item::Helmet if p.inv.helmet > 0 => {
                    let pct = (p.inv.helmet_hp / helmet_durability(p.inv.helmet) * 100.0) as u16;
                    out.push((LootKind::Helmet(p.inv.helmet), pct.max(1)));
                    p.inv.helmet = 0;
                    p.inv.helmet_hp = 0.0;
                }
                Item::Backpack if p.inv.backpack > 0 => {
                    out.push((LootKind::Backpack(p.inv.backpack), 1));
                    p.inv.backpack = 0;
                    // what no longer fits falls out with it
                    for t in AmmoType::ALL {
                        let over = p.inv.ammo(t).saturating_sub(p.inv.ammo_cap(t));
                        if over > 0 {
                            p.inv.ammo[t.index()] -= over;
                            out.push((LootKind::Ammo(t), over));
                        }
                    }
                    let over = p.inv.medkits.saturating_sub(p.inv.medkit_cap());
                    if over > 0 {
                        p.inv.medkits -= over;
                        out.push((LootKind::Medkit, over as u16));
                    }
                    while p.inv.spare_count() > p.inv.sight_cap() {
                        if let Some(k) = p.inv.spare_mods.iter().position(|n| *n > 0) {
                            p.inv.spare_mods[k] -= 1;
                            out.push((LootKind::Mod(k as u8 + 1), 1));
                        } else {
                            let k = p.inv.spare_sights.iter().position(|n| *n > 0).unwrap_or(0);
                            p.inv.spare_sights[k] -= 1;
                            out.push((LootKind::Sight(k as u8 + 1), 1));
                        }
                    }
                    p.grenades = p.grenades.min(p.inv.grenade_cap());
                    for (have, cap, kind) in [
                        (&mut p.inv.bandages, 5u8, LootKind::Bandages),
                        (&mut p.inv.drinks, 2, LootKind::Drink),
                        (&mut p.inv.molotovs, 1, LootKind::Molotov),
                        (&mut p.inv.flashes, 1, LootKind::Flash),
                    ] {
                        if *have > cap {
                            out.push((kind, (*have - cap) as u16));
                            *have = cap;
                        }
                    }
                }
                Item::Sight(l) if (1..=MAX_SIGHT).contains(&l) => {
                    let k = l as usize - 1;
                    if p.inv.spare_sights[k] > 0 {
                        p.inv.spare_sights[k] -= 1;
                        out.push((LootKind::Sight(l), 1));
                    }
                }
                Item::Bandages | Item::Drinks | Item::Molotovs | Item::Flashes => {
                    let (have, kind) = match item {
                        Item::Bandages => (&mut p.inv.bandages, LootKind::Bandages),
                        Item::Drinks => (&mut p.inv.drinks, LootKind::Drink),
                        Item::Molotovs => (&mut p.inv.molotovs, LootKind::Molotov),
                        _ => (&mut p.inv.flashes, LootKind::Flash),
                    };
                    let n = (n as u8).min(*have);
                    if n > 0 {
                        *have -= n;
                        p.inv.healing = 0.0;
                        out.push((kind, n as u16));
                    }
                }
                Item::Mod(m) if MODS.contains(&m) => {
                    let k = m as usize - 1;
                    if p.inv.spare_mods[k] > 0 {
                        p.inv.spare_mods[k] -= 1;
                        out.push((LootKind::Mod(m), 1));
                    }
                }
                // a gun can go while the other hand still holds one
                Item::Weapon(k) if k < 2 => {
                    let (k, other) = (k as usize, 1 - k as usize);
                    let real = |j: usize| p.empty_slot != Some(j as u8);
                    if real(k) && real(other) {
                        let w = p.weapons[k];
                        gun = Some(WState { reserve: 0, ..w });
                        p.weapons[k] = WState {
                            id: WeaponId::Usp,
                            mag: 0,
                            reserve: 0,
                        };
                        p.empty_slot = Some(k as u8);
                        let l = std::mem::take(&mut p.inv.sights[k]);
                        if l > 0 && !p.inv.stash_sight(l) {
                            out.push((LootKind::Sight(l), 1));
                        }
                        for m in std::mem::take(&mut p.inv.mods[k]) {
                            if m != 0 && !p.inv.stash_mod(m) {
                                out.push((LootKind::Mod(m), 1));
                            }
                        }
                        if p.slot as usize == k {
                            p.slot = other as u8;
                            p.reload_end = None;
                            p.switch_end = self.time + 0.4;
                        }
                    }
                }
                _ => {}
            },
            ItemAction::AttachSight { level, slot } => {
                let (k, l) = (slot as usize, level as usize);
                if k < 2
                    && (1..=MAX_SIGHT as usize).contains(&l)
                    && p.inv.spare_sights[l - 1] > 0
                    && p.empty_slot != Some(slot)
                    && sight_fits(level, p.weapons[k].id)
                {
                    p.inv.spare_sights[l - 1] -= 1;
                    let old = std::mem::replace(&mut p.inv.sights[k], level);
                    if old > 0 {
                        p.inv.spare_sights[old as usize - 1] += 1;
                    }
                }
            }
            ItemAction::AttachMod { kind, slot } => {
                let k = slot as usize;
                if k < 2
                    && MODS.contains(&kind)
                    && p.inv.spare_mods[kind as usize - 1] > 0
                    && p.empty_slot != Some(slot)
                    && mod_fits(kind, p.weapons[k].id)
                {
                    let place = mod_place(kind);
                    p.inv.spare_mods[kind as usize - 1] -= 1;
                    let old = std::mem::replace(&mut p.inv.mods[k][place], kind);
                    if old != 0 {
                        p.inv.spare_mods[old as usize - 1] += 1;
                    }
                }
            }
            ItemAction::DetachMods(slot) => {
                let k = slot as usize;
                if k < 2 {
                    for place in 0..3 {
                        let m = p.inv.mods[k][place];
                        if m != 0 && p.inv.stash_mod(m) {
                            p.inv.mods[k][place] = 0;
                        }
                    }
                }
            }
            ItemAction::DetachSight(slot) => {
                let k = slot as usize;
                if k < 2 && p.inv.sights[k] > 0 && p.inv.stash_sight(p.inv.sights[k]) {
                    p.inv.sights[k] = 0;
                }
            }
        }
        // a magazine holding more than it may now: the extra rounds go back in the bag
        for k in 0..2 {
            let w = p.weapons[k];
            let cap = mag_size(self.tuning.weapon(w.id).mag, p.inv.has_mod(k, MOD_EXTMAG));
            if w.mag > cap {
                p.weapons[k].mag = cap;
                p.inv.add_ammo(AmmoType::of(w.id), w.mag - cap);
            }
        }
        p.br_refresh_reserve();
        for (kind, n) in out {
            self.br_drop_loot(kind, n, at);
        }
        if let Some(w) = gun {
            self.drop_weapon(w, at);
        }
    }

    /// Puts supplies on the floor at `pos`.
    pub fn br_drop_loot(&mut self, kind: LootKind, amount: u16, pos: Vec3) {
        let floor = self
            .world
            .floor_below(pos + vec3(0.0, 0.3, 0.0), 3.0)
            .or_else(|| self.world.ground(pos.x, pos.z))
            .unwrap_or(pos.y);
        let pos = vec3(pos.x, floor, pos.z);
        // the same thing already lying right here: one pile, not a hundred
        let stacks = matches!(
            kind,
            LootKind::Ammo(_)
                | LootKind::Medkit
                | LootKind::Bandages
                | LootKind::Drink
                | LootKind::Molotov
                | LootKind::Flash
        );
        if stacks {
            if let Some(l) = self
                .br
                .loot
                .iter_mut()
                .find(|l| l.kind == kind && l.pos.distance(pos) < 1.2)
            {
                l.amount = l.amount.saturating_add(amount);
                return;
            }
        }
        let id = self.new_ent_id();
        self.br.loot.push(Loot {
            id,
            kind,
            amount,
            pos,
        });
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
            weather: self.br.weather,
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
            empty_slot: me.and_then(|p| p.empty_slot),
            squad: me
                .filter(|m| m.squad != 0)
                .map(|m| {
                    self.players
                        .iter()
                        .flatten()
                        .filter(|p| p.squad == m.squad)
                        .map(|p| toon_shared::protocol::SquadMate {
                            id: p.id,
                            name: p.name.clone(),
                            hp: p.hp,
                            alive: p.alive || p.in_plane,
                            downed: p.downed,
                            pos: p.body.pos.to_array(),
                            marker: p.marker,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            drops: self.br.drops.iter().map(|d| d.pos.to_array()).collect(),
            red: self
                .br
                .red
                .map(|z| [z.c.x, z.c.y, z.r, (z.start - self.time).max(0.0) as f32]),
            open_box: self.box_view(id),
            watching: me
                .filter(|m| !m.alive && !m.spec_free)
                .and_then(|m| m.spectating),
            summary: me.filter(|m| m.br_place != 0).map(|m| {
                let rec = self.br_stats.get(&m.stats_key());
                toon_shared::protocol::BrSummary {
                    place: m.br_place,
                    of: self.br.entrants as u16,
                    kills: m.kills,
                    damage: m.br_dmg,
                    longest: m.br_longest,
                    survived: (m.br_out_at - self.br.plane_t0).max(0.0) as f32,
                    matches: rec.matches,
                    wins: rec.wins,
                    total_kills: rec.kills,
                    best: rec.best,
                }
            }),
        })
    }

    pub fn br_drop_snaps(&self) -> impl Iterator<Item = EntSnap> + '_ {
        self.br.drops.iter().map(|d| EntSnap::AirDrop {
            id: d.id,
            pos: d.pos.to_array(),
            landed: d.landed,
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
        // out of a red zone (marked or shelled), towards the circle's middle
        if let Some(rz) = self.br.red.filter(|_| !skydiving) {
            if flat.distance(rz.c) < rz.r + 6.0 {
                let away = (flat - rz.c).normalize_or(vec2(1.0, 0.0));
                let out = rz.c + away * (rz.r + 20.0);
                let towards = out.lerp(c, 0.2);
                let g = self
                    .nav
                    .snap(vec3(towards.x, pos.y, towards.y))
                    .unwrap_or(vec3(towards.x, pos.y, towards.y));
                return (Some(g), false, true);
            }
        }
        // squads: crawl to a squadmate when down; pick up a squadmate who is down; keep up
        // with the squad
        let squad = me.squad;
        if squad != 0 && !skydiving {
            let mates: Vec<(u8, Vec3, bool, bool)> = self
                .players
                .iter()
                .flatten()
                .filter(|p| p.squad == squad && p.id != id && p.alive)
                .map(|p| (p.id, p.body.pos, p.downed, p.is_bot()))
                .collect();
            if me.downed {
                let to = mates
                    .iter()
                    .filter(|m| !m.2)
                    .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)));
                return (to.map(|m| m.1), false, true);
            }
            if let Some(m) = mates
                .iter()
                .filter(|m| m.2 && m.1.distance(pos) < 90.0)
                .min_by(|a, b| a.1.distance(pos).total_cmp(&b.1.distance(pos)))
            {
                let close = m.1.distance(pos) < 1.6;
                return (Some(m.1), close, true);
            }
            // the leader: a real player in the squad, otherwise its first member
            let leader = mates
                .iter()
                .filter(|m| !m.2)
                .min_by_key(|m| (m.3, m.0))
                .filter(|m| !m.3 || m.0 < id);
            if let Some(l) = leader {
                if l.1.distance(pos) > 45.0 {
                    let off = vec3(((id % 3) as f32 - 1.0) * 4.0, 0.0, ((id % 2) as f32) * 4.0);
                    let g = self.nav.snap(l.1 + off).unwrap_or(l.1);
                    return (Some(g), false, false);
                }
            }
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
        let have_ids: Vec<WeaponId> = me.weapons.iter().map(|w| w.id).collect();
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
        bot.skip.retain(|s| s.1 > time);
        let skipped: Vec<u16> = bot.skip.iter().map(|s| s.0).collect();
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
                        && !skipped.contains(&pk.id)
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
                if skydiving || !Self::br_bot_gave_up(bot, ent, at, pos, time) {
                    return (Some(at), false, false);
                }
            }
            // a death box with a real gun in it
            if !skydiving {
                let found = self.br.boxes.iter().find_map(|b| {
                    let k = b.guns.iter().position(|g| {
                        g.id.def().class != toon_shared::weapons::WeaponClass::Pistol
                            && !have.contains(&g.id)
                    })?;
                    (near(b.pos) < 60.0 && !skipped.contains(&b.id)).then_some((
                        b.id,
                        b.pos,
                        b.items.len() + k,
                    ))
                });
                if let Some((bid, at, k)) = found {
                    if near(at) < crate::boxes::BOX_RADIUS {
                        self.box_take(id, bid, k);
                    }
                    if !Self::br_bot_gave_up(bot, bid, at, pos, time) {
                        return (Some(at), false, false);
                    }
                }
            }
        }
        // a landed supply crate not too far away (inside the zone): the best gear there is
        if !skydiving && armed {
            if let Some(d) = self.br.drops.iter().find(|d| {
                d.landed && zone.contains(time, d.pos) && (25.0..220.0).contains(&near(d.pos))
            }) {
                let g = self.nav.snap(d.pos).unwrap_or(d.pos);
                return (Some(g), false, false);
            }
        }
        if !skydiving {
            // a medkit when hurt (and not using one already), else a bandage or a drink
            if hp < 70.0 && inv.healing <= 0.0 {
                if inv.medkits > 0 {
                    self.br_use_medkit(id);
                } else if inv.bandages > 0 {
                    self.br_use(id, USE_BANDAGE);
                } else if inv.drinks > 0 {
                    self.br_use(id, USE_DRINK);
                }
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
                LootKind::Grenades | LootKind::Molotov | LootKind::Flash => false,
                LootKind::Bandages => inv.bandages < 5,
                LootKind::Drink => inv.drinks < inv.drink_cap(),
                LootKind::Mod(m) => (0..2).any(|k| {
                    have_ids.get(k).is_some_and(|w| {
                        w.def().class != toon_shared::weapons::WeaponClass::Pistol
                            && mod_fits(m, *w)
                            && inv.mods[k][mod_place(m)] == 0
                    })
                }),
                LootKind::Sight(l) => (0..2).any(|k| {
                    have_ids.get(k).is_some_and(|w| {
                        w.def().class != toon_shared::weapons::WeaponClass::Pistol
                            && sight_fits(l, *w)
                            && l > inv.sights[k]
                    })
                }),
            };
            let mut cands: Vec<(u16, Vec3)> = self
                .br
                .loot
                .iter()
                .filter(|l| want(l.kind) && near(l.pos) < 45.0 && !skipped.contains(&l.id))
                .map(|l| (l.id, l.pos))
                .collect();
            cands.sort_by(|a, b| near(a.1).total_cmp(&near(b.1)));
            let best = cands.into_iter().take(4).find(|c| self.reachable(c.1));
            if let Some((ent, at)) = best {
                if near(at) < LOOT_RADIUS && (at.y - pos.y).abs() < 1.8 {
                    self.br_pick(id, ent);
                }
                if !Self::br_bot_gave_up(bot, ent, at, pos, time) {
                    return (Some(at), false, false);
                }
            }
            // death boxes with something we want
            let found = self.br.boxes.iter().find_map(|b| {
                let wanted: Vec<usize> = b
                    .items
                    .iter()
                    .enumerate()
                    .filter(|(_, i)| want(i.0))
                    .map(|(k, _)| k)
                    .collect();
                (!wanted.is_empty() && near(b.pos) < 45.0 && !skipped.contains(&b.id))
                    .then_some((b.id, b.pos, wanted))
            });
            if let Some((bid, at, wanted)) = found {
                if near(at) < crate::boxes::BOX_RADIUS {
                    for k in wanted.into_iter().rev() {
                        self.box_take(id, bid, k);
                    }
                }
                if !Self::br_bot_gave_up(bot, bid, at, pos, time) {
                    return (Some(at), false, false);
                }
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

    /// A bot heading for item `ent` at `at`: true once it has stood next to it for a few
    /// seconds without getting it (on another floor, behind something); it is then left
    /// alone for a while.
    fn br_bot_gave_up(
        bot: &mut crate::bots::Bot,
        ent: u16,
        at: Vec3,
        pos: Vec3,
        time: f64,
    ) -> bool {
        let close = vec2(at.x - pos.x, at.z - pos.z).length() < 2.5;
        match bot.fetch {
            Some((e, since)) if e == ent && close => {
                if time - since > 4.0 {
                    bot.skip.push((ent, time + 90.0));
                    bot.fetch = None;
                    return true;
                }
            }
            _ if close => bot.fetch = Some((ent, time)),
            _ => {}
        }
        false
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
