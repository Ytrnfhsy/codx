//! Supply drops: launching, landing and filling them.

use super::*;

impl Game {
    /// Supply crates: a new one now and then, falling ones come down, landed ones spill
    /// their gear; a crate emptied of it goes away.
    pub(super) fn br_airdrops(&mut self, dt: f32) {
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
        // (the rare and heavy ones: the anti-materiel rifles only come from the sky)
        let guns = [
            WeaponId::Barrett,
            WeaponId::Alligator,
            WeaponId::Negev,
            WeaponId::Rpg7,
            WeaponId::Awm,
        ];
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

    pub fn br_drop_snaps(&self) -> impl Iterator<Item = EntSnap> + '_ {
        self.br.drops.iter().map(|d| EntSnap::AirDrop {
            id: d.id,
            pos: d.pos.to_array(),
            landed: d.landed,
        })
    }
}
