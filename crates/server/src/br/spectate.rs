//! Watching the round after being knocked out.

use super::*;

impl Game {
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
    pub(super) fn br_spectators(&mut self) {
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
}
