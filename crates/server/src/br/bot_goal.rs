//! Where a bot heads in the battle royale.

use super::*;

impl Game {
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
        // empty-handed (as everybody lands): any gun will do for a start, a pistol too
        let bare = have.is_empty();
        if !armed {
            let range = if skydiving {
                220.0
            } else if bare {
                160.0
            } else {
                90.0
            };
            let mut cands: Vec<(u16, Vec3)> = self
                .pickups
                .iter()
                // a real gun we do not have yet (a pickup we cannot take would hold us there)
                .filter(|pk| {
                    near(pk.pos) < range
                        && zone.contains(time, pk.pos)
                        && (bare
                            || pk.w.id.def().class != toon_shared::weapons::WeaponClass::Pistol)
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
    pub(super) fn reachable(&self, at: Vec3) -> bool {
        self.nav.snap(at).is_some_and(|n| {
            (n.y - at.y).abs() < 0.8 && vec2(n.x - at.x, n.z - at.z).length() < 2.5
        })
    }
}
