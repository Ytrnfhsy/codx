//! Loot on the ground: scattered at the start and dropped by the dead.

use super::*;

impl Game {
    /// Weapons and supplies on the map's loot spots (or on random walkable ground).
    pub(super) fn br_scatter_loot(&mut self) {
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
                        // (rockets and bolts are rare: the launchers are)
                        let t = if self.rng.chance(0.1) {
                            [AmmoType::Rocket, AmmoType::Bolt, AmmoType::Bolt][self.rng.below(3)]
                        } else {
                            AmmoType::ALL[self.rng.below(6)]
                        };
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
        const TABLE: [(WeaponId, f32); 20] = [
            (Fort17, 1.2),
            (Glock18, 0.9),
            (Revolver, 0.6),
            (Mp7, 1.1),
            (Vector, 0.9),
            (Bizon, 1.0),
            (Ksg, 0.8),
            (Aa12, 0.4),
            (Uar15, 0.9),
            (Fort221, 0.8),
            (Ak12, 0.8),
            (ScarH, 0.45),
            (Mk14, 0.45),
            (Svd, 0.35),
            (Pkm, 0.3),
            (Negev, 0.25),
            (Awm, 0.3),
            (Crossbow, 0.35),
            (Bow, 0.5),
            (Rpg7, 0.2),
        ];
        let total: f32 = TABLE.iter().map(|e| e.1).sum();
        let mut r = self.rng.range(0.0, total);
        for (w, k) in TABLE {
            if r < k {
                return w;
            }
            r -= k;
        }
        Fort17
    }

    /// Everything a knocked out player carried goes on the ground around them.
    pub(super) fn br_drop_inventory(&mut self, victim: u8) {
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
}
