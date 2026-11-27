//! Picking things up and using the inventory: medkits, bandages, sights, attachments.

use super::*;

impl Game {
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
                    !p.is_empty(k) && sight_fits(lv, w.id) && lv > p.inv.sights[k]
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
                    !p.is_empty(k) && mod_fits(kind, p.weapons[k].id) && p.inv.mods[k][place] == 0
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
                Item::Weapon(k) if k < 2 => {
                    let (k, other) = (k as usize, 1 - k as usize);
                    if !p.is_empty(k) {
                        let w = p.weapons[k];
                        gun = Some(WState { reserve: 0, ..w });
                        p.weapons[k] = WState {
                            id: WeaponId::Glock18,
                            mag: 0,
                            reserve: 0,
                        };
                        p.empty |= 1 << k;
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
                    && !p.is_empty(k)
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
                    && !p.is_empty(k)
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
}
