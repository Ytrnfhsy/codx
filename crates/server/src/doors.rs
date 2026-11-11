//! Doors on the island: anyone next to one opens or shuts it with the use key, bots open
//! the ones in their way, and every client is told which stand open.

use crate::game::Game;
use toon_shared::physics::body_box;
use toon_shared::protocol::S2C;

/// How close (from the eyes to the middle of the doorway) you must be to work a door.
pub const DOOR_REACH: f32 = 2.8;
/// A door cannot be worked again sooner than this.
const DOOR_COOLDOWN: f64 = 0.4;

impl Game {
    /// Which doors stand open, for the clients.
    pub fn doors_msg(&self) -> S2C {
        S2C::Doors(
            self.world
                .door_open
                .iter()
                .enumerate()
                .filter(|(_, o)| **o)
                .map(|(k, _)| k as u16)
                .collect(),
        )
    }

    /// Every door shut (a new round, a new map).
    pub fn shut_doors(&mut self) {
        if self.world.doors.is_empty() {
            return;
        }
        for k in 0..self.world.doors.len() {
            self.world.set_door(k, false);
        }
        self.door_used.clear();
        let msg = self.doors_msg();
        self.broadcast(msg);
    }

    /// Opens or shuts a door for a player standing next to it. Returns whether it moved.
    pub fn use_door(&mut self, id: u8, k: u16, open: Option<bool>) -> bool {
        let k = k as usize;
        let Some(d) = self.world.doors.get(k).copied() else {
            return false;
        };
        let Some(p) = self.p(id) else { return false };
        if !p.alive || p.downed || p.in_plane || self.seat_of(id).is_some() {
            return false;
        }
        if p.eye().distance(d.centre()) > DOOR_REACH {
            return false;
        }
        let now = self.world.door_open[k];
        let want = open.unwrap_or(!now);
        if want == now {
            return false;
        }
        if self
            .door_used
            .get(&(k as u16))
            .is_some_and(|t| self.time - t < DOOR_COOLDOWN)
        {
            return false;
        }
        // it does not swing through anybody
        let Some(panel) = self.world.door_box(k, want) else {
            return false;
        };
        if self
            .players
            .iter()
            .flatten()
            .any(|q| q.alive && panel.overlaps(&body_box(q.body.pos, q.body.stance.height())))
        {
            return false;
        }
        self.world.set_door(k, want);
        self.door_used.insert(k as u16, self.time);
        let msg = self.doors_msg();
        self.broadcast(msg);
        true
    }

    /// Bots open the shut doors they walk up to.
    pub fn bots_open_doors(&mut self) {
        if self.world.doors.is_empty() || !self.tick.is_multiple_of(5) {
            return;
        }
        let mut want: Vec<(u8, u16)> = Vec::new();
        for p in self.players.iter().flatten() {
            if !p.is_bot() || !p.alive || p.in_plane || p.downed {
                continue;
            }
            let at = p.body.pos;
            for (k, d) in self.world.doors.iter().enumerate() {
                if self.world.door_open[k] {
                    continue;
                }
                let c = d.centre();
                let flat = glam::vec2(c.x - at.x, c.z - at.z).length();
                if flat < 1.6 && (c.y - at.y).abs() < 2.0 {
                    want.push((p.id, k as u16));
                }
            }
        }
        for (id, k) in want {
            self.use_door(id, k, Some(true));
        }
    }
}
