//! Battle royale death boxes: everything a knocked-out player carried goes into one box
//! where they fell, and whoever comes by takes what they want from it.

use crate::br::Loot;
use crate::game::{Game, WState};
use crate::pickups::Pickup;
use glam::{vec3, Vec3};
use toon_shared::protocol::{BoxView, EntSnap, LootKind};

/// How close you must be to look inside.
pub const BOX_RADIUS: f32 = 2.6;

#[derive(Clone, Debug)]
pub struct DeathBox {
    pub id: u16,
    /// Whose things these were.
    pub owner: u8,
    pub name: String,
    pub pos: Vec3,
    pub items: Vec<(LootKind, u16)>,
    pub guns: Vec<WState>,
    /// When it was made (a death fills one box, guns and all).
    pub born: f64,
}

impl Game {
    /// The box of a player just knocked out (made on first use).
    pub fn br_box_of(&mut self, victim: u8) -> usize {
        if let Some(i) = self
            .br
            .boxes
            .iter()
            .position(|b| b.owner == victim && b.born == self.time)
        {
            return i;
        }
        let (pos, name) = self
            .p(victim)
            .map(|p| (p.body.pos, p.name.clone()))
            .unwrap_or_default();
        let floor = self
            .world
            .floor_below(pos + vec3(0.0, 0.5, 0.0), 4.0)
            .or_else(|| self.world.ground(pos.x, pos.z))
            .unwrap_or(pos.y);
        let id = self.new_ent_id();
        self.br.boxes.push(DeathBox {
            id,
            owner: victim,
            name,
            pos: vec3(pos.x, floor, pos.z),
            items: Vec::new(),
            guns: Vec::new(),
            born: self.time,
        });
        self.br.boxes.len() - 1
    }

    fn box_near(&self, id: u8, box_id: u16) -> Option<usize> {
        let p = self.p(id)?;
        if !p.alive || p.downed {
            return None;
        }
        let i = self.br.boxes.iter().position(|b| b.id == box_id)?;
        (self.br.boxes[i].pos.distance(p.body.pos) < BOX_RADIUS + 0.5).then_some(i)
    }

    /// Takes thing `idx` out of a box (supplies first, then guns). Supplies follow the
    /// usual rules (what does not fit stays in the box, a better vest is swapped and the
    /// old one goes in); a gun is swapped for the one in hand, which goes in the box.
    pub fn box_take(&mut self, id: u8, box_id: u16, idx: usize) {
        let Some(i) = self.box_near(id, box_id) else {
            return;
        };
        let at = self.p(id).map(|p| p.body.pos).unwrap_or_default();
        let n_items = self.br.boxes[i].items.len();
        if idx < n_items {
            let (kind, amount) = self.br.boxes[i].items[idx];
            let ent = self.new_ent_id();
            let before = self.br.loot.len();
            self.br.loot.push(Loot {
                id: ent,
                kind,
                amount,
                pos: at,
            });
            self.br_pick(id, ent);
            let remain = self.br.loot.iter().find(|l| l.id == ent).map(|l| l.amount);
            if remain == Some(amount) {
                // nothing taken (no room, or no better than what is on)
                self.br.loot.retain(|l| l.id != ent);
                return;
            }
            // what is left of it, and anything swapped out, back into the box
            let back: Vec<(LootKind, u16)> = self
                .br
                .loot
                .drain(before..)
                .map(|l| (l.kind, l.amount))
                .collect();
            let b = &mut self.br.boxes[i];
            b.items.remove(idx);
            b.items.extend(back);
        } else if let Some(&gun) = self.br.boxes[i].guns.get(idx - n_items) {
            let ent = self.new_ent_id();
            let before: Vec<u16> = self.pickups.iter().map(|p| p.id).collect();
            self.pickups.push(Pickup {
                id: ent,
                w: gun,
                pos: at,
                yaw: 0.0,
                expire: f64::INFINITY,
            });
            self.pick_up(id, ent);
            // not taken (the same gun in hand already): it stays in the box
            if let Some(k) = self.pickups.iter().position(|p| p.id == ent) {
                self.pickups.remove(k);
                return;
            }
            // the gun swapped out goes in the box instead of on the ground
            let mut out = Vec::new();
            self.pickups.retain(|p| {
                if before.contains(&p.id) {
                    true
                } else {
                    out.push(p.w);
                    false
                }
            });
            let b = &mut self.br.boxes[i];
            b.guns.remove(idx - n_items);
            b.guns.extend(out);
            // (and so do attachments that came off it)
            let mut k = 0;
            while k < self.br.loot.len() {
                let l = &self.br.loot[k];
                if l.pos.distance(at) < 0.5
                    && matches!(l.kind, LootKind::Mod(_) | LootKind::Sight(_))
                {
                    let l = self.br.loot.remove(k);
                    self.br.boxes[i].items.push((l.kind, l.amount));
                } else {
                    k += 1;
                }
            }
        }
        self.br
            .boxes
            .retain(|b| !b.items.is_empty() || !b.guns.is_empty());
    }

    /// Takes every supply (not the guns) out of a box.
    pub fn box_take_all(&mut self, id: u8, box_id: u16) {
        let Some(i) = self.box_near(id, box_id) else {
            return;
        };
        let mut idx = self.br.boxes[i].items.len();
        while idx > 0 {
            idx -= 1;
            self.box_take(id, box_id, idx);
            if !self.br.boxes.iter().any(|b| b.id == box_id) {
                break;
            }
        }
    }

    pub fn box_snaps(&self) -> impl Iterator<Item = EntSnap> + '_ {
        self.br.boxes.iter().map(|b| EntSnap::DeathBox {
            id: b.id,
            pos: b.pos.to_array(),
        })
    }

    /// What is in the box a player has open (while they stay next to it).
    pub fn box_view(&self, id: u8) -> Option<BoxView> {
        let p = self.p(id)?;
        let box_id = p.open_box?;
        let b = self.br.boxes.iter().find(|b| b.id == box_id)?;
        (b.pos.distance(p.body.pos) < BOX_RADIUS + 0.5).then(|| BoxView {
            id: b.id,
            name: b.name.clone(),
            items: b.items.clone(),
            guns: b.guns.iter().map(|g| (g.id, g.mag)).collect(),
        })
    }
}
