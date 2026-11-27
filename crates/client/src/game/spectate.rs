//! Watching: after being knocked out of a battle royale, or as a spectator.

use super::*;

impl GameClient {
    /// Out of the battle royale: watch the one who got us, a squadmate, or anybody still
    /// in (LMB / RMB: next / previous, Space: a free camera).
    pub(super) fn update_br_spectate(&mut self, input: bool) {
        // the server picks whom we watch (squadmates only while the squad is in)
        self.spectate = if self.spec_free {
            None
        } else {
            self.br
                .as_ref()
                .and_then(|b| b.watching)
                .filter(|t| self.remotes.get(t).is_some_and(|r| r.alive))
        };
        if !input {
            return;
        }
        if binds::pressed(Action::Jump) {
            self.spec_free = !self.spec_free;
            if self.spec_free {
                // the free camera starts where we were watching from
                let (p, y, pi) = self.camera();
                self.spec_pos = p;
                self.yaw = y;
                self.pitch = pi;
                self.spectate = None;
            }
            self.net
                .send(C2S::Spectate(if self.spec_free { 2 } else { 3 }));
        } else if !self.spec_free && binds::pressed(Action::Fire) {
            self.net.send(C2S::Spectate(0));
        } else if !self.spec_free && binds::pressed(Action::Aim) {
            self.net.send(C2S::Spectate(1));
        }
    }

    pub(super) fn update_spectator(&mut self, dt: f32, input: bool) {
        if !input {
            return;
        }
        let (f, r) = yaw_basis(self.yaw);
        let d = look_dir(self.yaw, self.pitch);
        let mut v = Vec3::ZERO;
        if binds::down(Action::Forward) {
            v += d;
        }
        if binds::down(Action::Back) {
            v -= d;
        }
        if binds::down(Action::Right) {
            v += r;
        }
        if binds::down(Action::Left) {
            v -= r;
        }
        if binds::down(Action::Jump) {
            v.y += 1.0;
        }
        if binds::down(Action::Crouch) {
            v.y -= 1.0;
        }
        let _ = f;
        let speed = if binds::down(Action::Sprint) {
            25.0
        } else {
            10.0
        };
        self.spec_pos += v.normalize_or_zero() * speed * dt;
    }
}
