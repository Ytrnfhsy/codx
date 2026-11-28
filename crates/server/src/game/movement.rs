//! Checks on where clients say they are: running, jumping, falls.

use super::*;

impl Game {
    pub(super) fn on_input(&mut self, id: u8, inp: InputState) {
        let frozen = self.state == MatchState::PreRound || self.state == MatchState::PostMatch;
        // in a car the car moves you: only where you look counts
        let seated = !self.cars.is_empty() && self.seat_of(id).is_some();
        let world = &self.world;
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        if inp.seq <= p.last_seq && p.last_seq - inp.seq < 1_000_000 {
            return;
        }
        p.last_seq = inp.seq;
        // a broken client must not pass NaN or infinite angles on to everybody else
        if inp.yaw.is_finite() {
            p.yaw = toon_shared::wrap_angle(inp.yaw);
        }
        if inp.pitch.is_finite() {
            p.pitch = inp.pitch.clamp(-1.55, 1.55);
        }
        if !p.alive || inp.life != p.life {
            return;
        }
        if p.fall_life != p.life {
            // a new life starts on its feet, wherever it is
            p.fall_life = p.life;
            p.support_y = f32::MAX;
            p.fall_top = None;
        }
        p.flags = inp.flags & (flags::SPRINT | flags::ADS | flags::ON_GROUND | flags::USE);
        if p.flags & flags::SPRINT != 0 {
            p.last_sprint = self.time;
        }
        if seated {
            p.flags &= !flags::SPRINT;
            p.support_y = f32::MAX;
            p.fall_top = None;
            return;
        }
        // (how high this server's tuning lets anybody jump; by a wall, with the kicks off
        // it too)
        let (jump, kicks) = (self.tuning.jump_height(), self.tuning.air_rise());
        let new = Vec3::from_array(inp.pos);
        if !new.is_finite() {
            return;
        }
        let stance = Stance::from_u8(inp.stance);
        let old = p.body.pos;
        let dh = vec3(new.x - old.x, 0.0, new.z - old.z).length();
        let dy = (new.y - old.y).abs();
        let skydiving = p.body.chute != toon_shared::physics::Chute::None;
        let mut ok = dh <= p.move_budget + 0.25 && (dy <= 4.0 || skydiving && dy <= 8.0);
        if frozen && dh > 0.05 {
            ok = false;
        }
        // standing on something (a floor, the ground, a ladder, the water)?
        let supported = toon_shared::physics::in_deep_water(world, new)
            || world.ladder_at(new).is_some()
            || world.ground(new.x, new.z).is_some_and(|g| new.y - g < 0.6)
            || world.floor_below(new + vec3(0.0, 0.1, 0.0), 0.7).is_some();
        // nobody rises higher than a jump above where they last stood (or than a jump
        // and its kicks, once by a wall)
        if !supported && toon_shared::physics::wall_beside(world, new).is_some() {
            p.by_wall = true;
        }
        let rise = if p.by_wall { kicks } else { jump };
        if ok && !skydiving && !supported && new.y > p.support_y + rise + MAX_AIR_RISE {
            ok = false;
        }
        let mut fell = 0.0;
        if ok {
            let mut bb = body_box(new, stance.height());
            bb.min += Vec3::splat(0.03);
            bb.max -= Vec3::splat(0.03);
            if world.overlaps(&bb) {
                ok = false;
            }
        }
        if ok {
            p.move_budget = (p.move_budget - dh).max(0.0);
            p.body.pos = new;
            // (the client's word for its speed: finite and no faster than anything moves
            // here, as grenades inherit it)
            let vel = Vec3::from_array(inp.vel);
            p.body.vel = if vel.is_finite() {
                vel.clamp_length_max(80.0)
            } else {
                Vec3::ZERO
            };
            p.body.stance = stance;
            p.body.on_ground = inp.flags & flags::ON_GROUND != 0;
            // falls: from the highest point since the feet left the ground
            if skydiving {
                p.fall_top = None;
                p.support_y = f32::MAX;
            } else if supported {
                let soft = toon_shared::physics::in_deep_water(world, new)
                    || world.ladder_at(new).is_some();
                if let Some(top) = p.fall_top.take().filter(|_| !soft) {
                    fell = top - new.y;
                }
                p.support_y = new.y;
                p.by_wall = false;
                p.kicks = 0;
            } else {
                let from = if p.support_y == f32::MAX {
                    new.y
                } else {
                    p.support_y
                };
                // a kick off a wall (by a wall, no more often than the kicks the tuning
                // allows) stops the fall: it counts from where it was
                let kicked = inp.flags & flags::KICKED != 0;
                if kicked
                    && !p.kicking
                    && (p.kicks as f32) < self.tuning.movement.wall_jumps.floor()
                    && (p.by_wall || toon_shared::physics::wall_beside(world, new).is_some())
                {
                    p.kicks += 1;
                    p.fall_top = Some(new.y);
                }
                p.kicking = kicked;
                p.fall_top = Some(p.fall_top.unwrap_or(from).max(new.y));
            }
            if p.body.on_ground {
                p.body.chute = toon_shared::physics::Chute::None;
            } else if skydiving && p.body.vel.y > -12.0 {
                // sinking slowly: the canopy is open
                p.body.chute = toon_shared::physics::Chute::Open;
            }
        } else {
            let life = p.life;
            let pos = p.body.pos.to_array();
            self.outbox
                .push((Dest::One(id), S2C::Correction { pos, life }, true));
        }
        // (a jump back down to where one stood never hurts, however high the tuning jumps)
        let safe = SAFE_FALL.max(self.tuning.jump_height() + 1.0);
        if fell > safe {
            let at = Vec3::from_array(inp.pos);
            self.damage(
                id,
                None,
                (fell - safe) * FALL_DAMAGE,
                Cause::Fall,
                false,
                at,
            );
        }
    }
}
