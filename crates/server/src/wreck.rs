//! Blasts break the map (see `toon_shared::destruct`): the server breaks its own copy,
//! tells every client which blast went off, and moves what lay on a floor that is gone.

use crate::game::{Dest, Game};
use glam::{vec3, Vec3};
use toon_shared::destruct::Blast;
use toon_shared::protocol::S2C;

impl Game {
    /// An explosion breaks what is around it.
    pub fn break_things(&mut self, pos: Vec3, radius: f32, big: bool) {
        let blast = Blast::of_explosion(pos, radius, big);
        let ch = self.wreck.blast(&mut self.map, &mut self.world, blast);
        self.outbox.push((Dest::All, S2C::Blast(blast), true));
        if ch.is_empty() {
            return;
        }
        // things lying where it broke: onto whatever is under them now
        let (lo, hi) = (ch.min - Vec3::splat(1.0), ch.max + Vec3::splat(1.0));
        let inside = |p: Vec3| p.cmpge(lo).all() && p.cmple(hi).all();
        let world = &self.world;
        let settle = |p: &mut Vec3| {
            if inside(*p) {
                if let Some(y) = world.floor_below(vec3(p.x, p.y + 1.5, p.z), 80.0) {
                    p.y = y;
                }
            }
        };
        for k in self.pickups.iter_mut() {
            settle(&mut k.pos);
        }
        for l in self.br.loot.iter_mut() {
            settle(&mut l.pos);
        }
        for b in self.br.boxes.iter_mut() {
            settle(&mut b.pos);
        }
        if self.sab.carrier.is_none() && self.sab.planted_at.is_none() {
            settle(&mut self.sab.pos);
        }
    }

    /// The map whole again (a new round).
    pub fn mend_map(&mut self) {
        if self.wreck.log.is_empty() {
            return;
        }
        self.wreck.reset(&mut self.map, &mut self.world);
        self.outbox.push((Dest::All, S2C::Blasts(Vec::new()), true));
    }
}
