//! What each player may be told about: interest on big maps and enemies in sight.

use super::*;

impl Game {
    /// How far a player looking through a magnifying scope sees others (the client's fog
    /// recedes up to 2.5 times when scoped).
    pub(super) const SCOPE_RANGE: f32 = 850.0;

    /// Builds the snapshot for one recipient.
    /// Where a player looks from (their body and whatever they steer), on big maps; None
    /// sees everything (small maps, spectators).
    pub(super) fn interest(&self, id: u8) -> Option<Vec<Vec3>> {
        self.map.terrain.as_ref()?;
        let p = self.p(id)?;
        if !p.team.is_playing() {
            return None;
        }
        // out of the battle royale and watching somebody: what is around them
        if !p.alive && self.mode == Mode::Br {
            if let Some(t) = p.spectating.and_then(|t| self.p(t)).filter(|t| t.alive) {
                return Some(vec![t.body.pos, p.body.pos]);
            }
        }
        let mut pts = vec![p.body.pos];
        pts.extend(
            self.vehicles
                .iter()
                .filter(|v| v.owner == id)
                .map(|v| v.pos),
        );
        Some(pts)
    }

    /// Whether `t` may be left out of `viewer`'s snapshots when out of their sight: an
    /// enemy, alive on the ground, not given away anyway (radar, gunfire on the radar, a
    /// car, the sky, footsteps close enough to hear) and not the one being watched.
    pub(crate) fn hideable(&self, viewer: &Player, t: &Player) -> bool {
        if t.id == viewer.id || !viewer.team.is_playing() || !t.team.is_playing() {
            return false;
        }
        if !t.alive || self.allied(viewer, t) || viewer.spectating == Some(t.id) {
            return false;
        }
        if t.in_plane || t.body.chute != toon_shared::physics::Chute::None {
            return false;
        }
        if self.seat_of(t.id).is_some() {
            return false;
        }
        if !self.mode.solo() {
            let team = viewer.team.index();
            let uav = self.uav_until[team] > self.time;
            let pinged = self.time - t.last_shot < RADAR_PING_TIME as f64;
            if uav || pinged {
                return false;
            }
        }
        // footsteps: someone walking or running near enough is heard (and shown where)
        let speed = vec2(t.body.vel.x, t.body.vel.z).length();
        if t.body.stance == Stance::Stand && speed > FOOTSTEPS_WALK_SPEED {
            let hear = if speed > FOOTSTEPS_RUN_SPEED {
                FOOTSTEPS_RUN_HEARD
            } else {
                FOOTSTEPS_WALK_HEARD
            };
            if viewer.body.pos.distance(t.body.pos) < hear {
                return false;
            }
        }
        true
    }

    /// Ten times a second: for every real player, which of the enemies they could hide
    /// from them are in sight. A line from their eyes (and where their eyes will be in a
    /// moment) to the enemy's head, body, feet, sides or where they are heading, none of it
    /// through a wall, a floor or the ground. Bots read the world directly and need none.
    pub(super) fn update_visibility(&mut self) {
        if !self.tick.is_multiple_of(3) {
            return;
        }
        let now = self.time;
        let mut seen: Vec<(u8, u8)> = Vec::new();
        for v in self.players.iter().flatten() {
            if v.is_bot() || !v.team.is_playing() {
                continue;
            }
            // watching somebody after death: through their eyes
            let from = v
                .spectating
                .and_then(|w| self.p(w))
                .filter(|_| !v.alive)
                .unwrap_or(v);
            let eye = from.eye();
            let ahead = if from.body.vel.is_finite() {
                eye + from.body.vel.clamp_length_max(12.0) * 0.15
            } else {
                eye
            };
            for t in self.players.iter().flatten() {
                if !self.hideable(v, t) || t.body.pos.distance(eye) > Self::SCOPE_RANGE + 50.0 {
                    continue;
                }
                if Self::can_see(&self.world, [eye, ahead], t) {
                    seen.push((v.id, t.id));
                }
            }
        }
        for (v, t) in seen {
            if let Some(p) = self.pm(v) {
                p.seen_at[t as usize] = now;
            }
        }
    }

    fn can_see(world: &World, eyes: [Vec3; 2], t: &Player) -> bool {
        let base = t.body.pos;
        let h = t.body.stance.height();
        let flat = vec3(base.x - eyes[0].x, 0.0, base.z - eyes[0].z).normalize_or_zero();
        let side = vec3(-flat.z, 0.0, flat.x) * 0.55;
        let chest = base + vec3(0.0, h * 0.6, 0.0);
        let going = if t.body.vel.is_finite() {
            chest + t.body.vel.clamp_length_max(12.0) * 0.25
        } else {
            chest
        };
        let points = [
            base + vec3(0.0, h + 0.15, 0.0),
            chest,
            base + vec3(0.0, 0.3, 0.0),
            chest + side,
            chest - side,
            going,
        ];
        eyes.iter()
            .any(|e| points.iter().any(|p| world.line_of_sight(*e, *p)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toon_shared::map::{MapBox, Mat};

    /// An empty yard with one wall 3 m high across x = 0 (z from -5 to 5).
    fn yard_with_wall() -> World {
        let mut m = maps::test_yard();
        m.boxes = vec![MapBox {
            min: vec3(-0.2, 0.0, -5.0),
            max: vec3(0.2, 3.0, 5.0),
            color: [200, 200, 200],
            solid: true,
            visible: true,
            mat: Mat::Solid,
            shade: 0,
        }];
        World::from_map(&m)
    }

    fn player_at(pos: Vec3, stance: Stance) -> Player {
        let mut p = Player::new(1, "T".into(), None, 0.0);
        p.body.pos = pos;
        p.body.stance = stance;
        p
    }

    #[test]
    fn a_wall_hides_who_stands_behind_it() {
        let w = yard_with_wall();
        let eye = vec3(-10.0, 1.6, 0.0);
        let behind = player_at(vec3(2.0, 0.0, 0.0), Stance::Stand);
        assert!(!Game::can_see(&w, [eye, eye], &behind));
        // out from behind its end: seen
        let beside = player_at(vec3(2.0, 0.0, 8.0), Stance::Stand);
        assert!(Game::can_see(&w, [eye, eye], &beside));
    }

    #[test]
    fn seen_by_a_side_or_the_head_alone() {
        let w = yard_with_wall();
        let eye = vec3(-10.0, 1.6, 0.0);
        // the head, the chest and the feet are behind the wall's end, one side is not
        let edge = player_at(vec3(2.0, 0.0, 5.8), Stance::Stand);
        let head = edge.body.pos + vec3(0.0, 2.0, 0.0);
        assert!(!w.line_of_sight(eye, head));
        assert!(Game::can_see(&w, [eye, eye], &edge));
        // a viewer high above the wall sees over it
        let high = vec3(-10.0, 9.0, 0.0);
        let behind = player_at(vec3(2.0, 0.0, 0.0), Stance::Stand);
        assert!(Game::can_see(&w, [high, high], &behind));
    }

    #[test]
    fn where_the_eyes_will_be_counts() {
        let w = yard_with_wall();
        // about to step out from behind the wall's end
        let now = vec3(-1.0, 1.6, 4.0);
        let soon = vec3(-1.0, 1.6, 9.0);
        let t = player_at(vec3(2.0, 0.0, 0.0), Stance::Crouch);
        assert!(!Game::can_see(&w, [now, now], &t));
        assert!(Game::can_see(&w, [now, soon], &t));
    }
}
