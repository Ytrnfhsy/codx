//! What the use key works on: things on the ground, cars, doors, the bomb.

use super::*;

impl GameClient {
    /// The closest dropped weapon in reach that we don't already carry.
    pub(super) fn find_pickup(&self) -> Option<(u16, Pick)> {
        // what we look at, within reach: the one nearest the crosshair wins (a little
        // nearer is a little better); right at our feet anything goes
        let me = self.body.pos;
        let eye = self.eye();
        let look = look_dir(self.yaw, self.pitch);
        let score = |pos: &[f32; 3], reach: f32, lift: f32| {
            let p = Vec3::from_array(*pos);
            let flat = vec2(p.x - me.x, p.z - me.z).length();
            if flat > reach || (p.y - me.y).abs() > 1.8 {
                return None;
            }
            let to = p + vec3(0.0, lift, 0.0) - eye;
            let angle = to.normalize_or_zero().dot(look).clamp(-1.0, 1.0).acos();
            let cone = if flat < 0.9 { 1.4 } else { 0.55 };
            (angle < cone).then_some(angle * 4.0 + flat * 0.25)
        };
        // (the battle royale's empty slots hold a placeholder pistol)
        let empty = self.br.as_ref().map_or(0, |b| b.empty);
        self.ents
            .iter()
            .filter_map(|e| match e {
                EntSnap::Weapon {
                    id, weapon, pos, ..
                } => {
                    let have = self
                        .weapons
                        .iter()
                        .enumerate()
                        .any(|(k, w)| w.0 == *weapon && empty & (1 << k) == 0);
                    score(pos, PICK_REACH, 0.1)
                        .filter(|_| !have)
                        .map(|d| (d, *id, Pick::Weapon(*weapon)))
                }
                EntSnap::Loot {
                    id,
                    kind,
                    amount,
                    pos,
                } => score(pos, PICK_REACH, 0.2).map(|d| (d, *id, Pick::Loot(*kind, *amount))),
                EntSnap::DeathBox { id, pos } => {
                    score(pos, 2.6, 0.3).map(|d| (d, *id, Pick::DeathBox))
                }
                // a car with a free seat (a little further: it is big)
                EntSnap::Car {
                    id,
                    kind,
                    pos,
                    seats,
                    ..
                } if seats.contains(&255) => {
                    let p = Vec3::from_array(*pos);
                    let d = vec2(p.x - me.x, p.z - me.z).length();
                    (d < 3.6 && (p.y - me.y).abs() < 2.5).then_some((
                        d + 1.5,
                        *id,
                        Pick::Car(*kind),
                    ))
                }
                _ => None,
            })
            .chain(self.door_in_reach())
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, id, w)| (id, w))
    }

    /// The door we look at, close enough to work (a little behind the things on the
    /// ground, so that loot by a doorway is still picked up).
    fn door_in_reach(&self) -> Option<(f32, u16, Pick)> {
        if self.world.doors.is_empty() {
            return None;
        }
        let eye = self.eye();
        let look = look_dir(self.yaw, self.pitch);
        self.world
            .doors
            .iter()
            .enumerate()
            .filter_map(|(k, d)| {
                let to = d.centre() - eye;
                let dist = to.length();
                let angle = to.normalize_or_zero().dot(look).clamp(-1.0, 1.0).acos();
                (dist < 2.6 && angle < 0.9).then(|| {
                    (
                        angle * 4.0 + dist * 0.25 + 0.8,
                        k as u16,
                        Pick::Door(self.world.door_open[k]),
                    )
                })
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
    }

    /// Standing where F plants, defuses or picks up the bomb.
    pub fn near_objective(&self) -> bool {
        if self.mode != Mode::Sab {
            return false;
        }
        let me = self.body.pos;
        let flat = |p: Vec3| vec2(p.x - me.x, p.z - me.z).length();
        let bomb = self
            .bomb
            .as_ref()
            .map(|b| flat(Vec3::from_array(b.pos)) < 2.4);
        bomb.unwrap_or(false)
            || self
                .map
                .sites
                .iter()
                .any(|s| flat(*s) < SAB_SITE_RADIUS + 0.4)
    }
}
