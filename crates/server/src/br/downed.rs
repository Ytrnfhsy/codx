//! Knocked down in a squad: bleeding out and being picked up.

use super::*;

impl Game {
    /// Knocked-down players bleed and are picked up by squadmates holding the use key next
    /// to them; a squad with nobody standing is out.
    pub(super) fn br_downed(&mut self, dt: f32) {
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
            let (bleed, revive_time, revive_hp) = (
                self.cfg.br.bleed_time.max(1.0),
                self.cfg.br.revive_time.max(0.1),
                self.cfg.br.revive_hp,
            );
            let Some(v) = self.pm(id) else { continue };
            match reviver {
                Some(_) => v.revive += dt / revive_time,
                None => {
                    v.revive = 0.0;
                    v.hp -= DOWNED_HP / bleed * dt;
                }
            }
            let revive = v.revive;
            if revive >= 1.0 {
                v.downed = false;
                v.downed_by = None;
                v.revive = 0.0;
                v.hp = revive_hp;
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
}
