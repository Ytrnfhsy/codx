//! Weapons dropped by killed players. Anyone can swap their current gun for one (press the
//! use key next to it); walking over the same gun you carry just takes its ammo.

use crate::game::{Dest, Game, WState};
use glam::{vec3, Vec3};
use toon_shared::protocol::{EntSnap, S2C};
use toon_shared::tr;

/// How long a dropped weapon stays on the ground.
pub const PICKUP_LIFETIME: f64 = 40.0;
/// Most dropped weapons on the map at once (oldest disappear first).
pub const MAX_PICKUPS: usize = 32;
/// How close you must be to pick one up.
pub const PICKUP_RADIUS: f32 = 2.3;
/// Walking this close to a gun of the same type takes its ammo.
const AMMO_RADIUS: f32 = 1.1;

#[derive(Clone, Copy, Debug)]
pub struct Pickup {
    pub id: u16,
    pub w: WState,
    pub pos: Vec3,
    pub yaw: f32,
    pub expire: f64,
}

impl Pickup {
    pub fn snap(&self) -> EntSnap {
        EntSnap::Weapon {
            id: self.id,
            weapon: self.w.id,
            pos: self.pos.to_array(),
            yaw: self.yaw,
        }
    }
}

fn near(a: Vec3, b: Vec3, r: f32) -> bool {
    let d = a - b;
    vec3(d.x, 0.0, d.z).length() < r && d.y.abs() < 2.0
}

impl Game {
    /// Drops a weapon on the floor below `pos`.
    pub fn drop_weapon(&mut self, w: WState, pos: Vec3) {
        if w.mag == 0 && w.reserve == 0 {
            return;
        }
        let floor = self
            .world
            .floor_below(pos + vec3(0.0, 0.5, 0.0), 30.0)
            .unwrap_or(pos.y);
        let id = self.new_ent_id();
        let yaw = self.rng.range(0.0, std::f32::consts::TAU);
        self.pickups.push(Pickup {
            id,
            w,
            pos: vec3(pos.x, floor, pos.z),
            yaw,
            expire: self.time + PICKUP_LIFETIME,
        });
        // too many dropped guns: the oldest goes (battle royale supplies never do)
        if self.pickups.iter().filter(|p| p.expire.is_finite()).count() > MAX_PICKUPS {
            if let Some(i) = self.pickups.iter().position(|p| p.expire.is_finite()) {
                self.pickups.remove(i);
            }
        }
    }

    /// Swaps the player's current weapon for a dropped one.
    pub fn pick_up(&mut self, id: u8, ent: u16) {
        // battle royale supplies are picked up with the same key
        if self.mode == toon_shared::Mode::Br && self.br_pick(id, ent) {
            return;
        }
        let br = self.mode == toon_shared::Mode::Br;
        let time = self.time;
        let Some(i) = self.pickups.iter().position(|p| p.id == ent) else {
            return;
        };
        let pk = self.pickups[i];
        let swap = self.tuning.weapon(pk.w.id).swap_time;
        let Some(p) = self.pm(id) else { return };
        if !p.alive || p.downed || !near(p.body.pos, pk.pos, PICKUP_RADIUS + 0.3) {
            return;
        }
        if p.weapons
            .iter()
            .any(|w| w.id == pk.w.id && (w.mag > 0 || w.reserve > 0))
        {
            return;
        }
        // an empty slot is filled first (battle royale: you start with a pistol only)
        let empty = if br {
            p.empty_slot.map(|s| s as usize)
        } else {
            p.weapons.iter().position(|w| w.mag == 0 && w.reserve == 0)
        };
        if br {
            p.empty_slot = None;
        }
        if let Some(e) = empty {
            p.slot = e as u8;
        }
        let old = *p.weapon();
        *p.weapon_mut() = pk.w;
        // battle royale: rounds live in the inventory, not in the guns
        let old = if br {
            p.br_refresh_reserve();
            WState { reserve: 0, ..old }
        } else {
            old
        };
        // a sight that does not go on the new gun comes off into the backpack (or the
        // ground, if it is full); a bare gun gets the best spare that fits it
        let slot = p.slot as usize;
        let mut loose = None;
        let mut loose_mods: Vec<u8> = Vec::new();
        if br {
            let l = p.inv.sights[slot];
            if l > 0 && !toon_shared::inventory::sight_fits(l, pk.w.id) {
                p.inv.sights[slot] = 0;
                if !p.inv.stash_sight(l) {
                    loose = Some(l);
                }
            }
            if p.inv.sights[slot] == 0 {
                p.inv.sights[slot] = p.inv.take_best_sight(pk.w.id);
            }
            // attachments the new gun cannot take go in the backpack (or the ground); free
            // places get spares that fit
            use toon_shared::inventory::{mod_fits, MODS};
            for place in 0..3 {
                let m = p.inv.mods[slot][place];
                if m != 0 && !mod_fits(m, pk.w.id) {
                    p.inv.mods[slot][place] = 0;
                    if !p.inv.stash_mod(m) {
                        loose_mods.push(m);
                    }
                }
            }
            for m in MODS {
                let place = toon_shared::inventory::mod_place(m);
                if p.inv.mods[slot][place] == 0
                    && mod_fits(m, pk.w.id)
                    && p.inv.spare_mods[m as usize - 1] > 0
                {
                    p.inv.spare_mods[m as usize - 1] -= 1;
                    p.inv.mods[slot][place] = m;
                }
            }
        }
        p.reload_end = None;
        p.switch_end = time + swap as f64;
        p.shot_budget = p.shot_budget.min(1.0);
        let pos = p.body.pos;
        let name = pk.w.id.def().name;
        self.pickups.remove(i);
        self.drop_weapon(old, pos);
        if let Some(l) = loose {
            self.br_drop_loot(toon_shared::protocol::LootKind::Sight(l), 1, pos);
        }
        for m in loose_mods {
            self.br_drop_loot(toon_shared::protocol::LootKind::Mod(m), 1, pos);
        }
        self.say(Dest::One(id), || S2C::Center {
            text: tr!("Підібрано: {name}", "Picked up: {name}"),
            secs: 1.5,
        });
    }

    /// Expires old pickups and lets players scavenge ammo from guns they already carry.
    pub fn update_pickups(&mut self) {
        let time = self.time;
        self.pickups.retain(|p| p.expire > time);
        if self.pickups.is_empty() {
            return;
        }
        let mut emptied = Vec::new();
        for pl in self.players.iter_mut().flatten() {
            if !pl.alive {
                continue;
            }
            for pk in self.pickups.iter_mut() {
                if pk.w.mag == 0 && pk.w.reserve == 0 {
                    continue;
                }
                if !near(pl.body.pos, pk.pos, AMMO_RADIUS) {
                    continue;
                }
                let Some(w) = pl.weapons.iter_mut().find(|w| w.id == pk.w.id) else {
                    continue;
                };
                let cap = self.tuning.weapon(w.id).mag * 6;
                let room = cap.saturating_sub(w.reserve);
                let take = room.min(pk.w.mag + pk.w.reserve);
                if take == 0 {
                    continue;
                }
                w.reserve += take;
                let from_mag = take.min(pk.w.mag);
                pk.w.mag -= from_mag;
                pk.w.reserve -= take - from_mag;
                if pk.w.mag == 0 && pk.w.reserve == 0 {
                    emptied.push(pk.id);
                }
            }
        }
        self.pickups.retain(|p| !emptied.contains(&p.id));
    }
}
