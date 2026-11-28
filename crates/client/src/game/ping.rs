//! Pings: pointing things out to the squad or team.

use super::*;

impl GameClient {
    /// The ping key: points out what the crosshair rests on (an enemy, something on the
    /// ground, a place); pressed twice on a place, «enemy here».
    pub(super) fn ping(&mut self) {
        let now = self.now;
        // pinging one of our own pings again takes it down; a squadmate's ping of a thing:
        // «I need that»
        let eye = self.eye();
        let dir = look_dir(self.yaw, self.pitch);
        if let Some(id) = self.ping_aimed(false) {
            self.net.send(C2S::Mark(MarkAt::Claim(id)));
            self.last_ping = (f64::NEG_INFINITY, None);
            return;
        }
        let mine = self
            .pings
            .iter()
            .filter(|p| p.from == self.my_id)
            .map(|p| {
                let to = Vec3::from_array(p.pos) + vec3(0.0, ping_lift(p.what), 0.0) - eye;
                let d = to.length().max(0.1);
                let tol = (1.2 / d).atan().max(0.06);
                (p.id, dir.dot(to / d).clamp(-1.0, 1.0).acos() / tol)
            })
            .filter(|(_, k)| *k < 1.0)
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((id, _)) = mine {
            self.net.send(C2S::Mark(MarkAt::Remove(id)));
            self.pings.retain(|p| p.id != id);
            self.last_ping = (f64::NEG_INFINITY, None);
            return;
        }
        let Some(at) = self.ping_target() else { return };
        if let MarkAt::Spot { pos, .. } = at {
            if let (true, Some(prev)) = (now - self.last_ping.0 < 0.45, self.last_ping.1) {
                self.net.send(C2S::Mark(MarkAt::Spot {
                    pos: prev.to_array(),
                    danger: true,
                }));
                self.last_ping = (f64::NEG_INFINITY, None);
                return;
            }
            self.last_ping = (now, Some(Vec3::from_array(pos)));
        } else {
            self.last_ping = (now, None);
        }
        self.net.send(C2S::Mark(at));
    }

    /// A ping the crosshair rests on: ours (`mine`) or a squadmate's ping of a thing.
    pub fn ping_aimed(&self, mine: bool) -> Option<u16> {
        let eye = self.eye();
        let dir = look_dir(self.yaw, self.pitch);
        self.pings
            .iter()
            .filter(|p| {
                if mine {
                    p.from == self.my_id
                } else {
                    p.from != self.my_id
                        && !matches!(p.what, PingWhat::Spot | PingWhat::Danger | PingWhat::Enemy)
                }
            })
            .map(|p| {
                let to = Vec3::from_array(p.pos) + vec3(0.0, ping_lift(p.what), 0.0) - eye;
                let d = to.length().max(0.1);
                let tol = (1.2 / d).atan().max(0.06);
                (p.id, dir.dot(to / d).clamp(-1.0, 1.0).acos() / tol)
            })
            .filter(|(_, k)| *k < 1.0)
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|p| p.0)
    }

    /// What a ping would point at now.
    pub fn ping_target(&self) -> Option<MarkAt> {
        const FAR: f32 = 1500.0;
        let eye = self.eye();
        let dir = look_dir(self.yaw, self.pitch);
        let wall = self.world.raycast(eye, dir, FAR).map(|h| h.0);
        let limit = wall.unwrap_or(FAR);
        let clear = |to: Vec3| {
            let d = to.length();
            d > 0.01
                && self
                    .world
                    .raycast(eye, to / d, (d - 0.4).max(0.0))
                    .is_none()
        };
        // an enemy in the crosshair, or nearly (a couple of degrees, in sight)
        let mut enemy: Option<(f32, u8)> = None;
        for (&id, r) in self
            .remotes
            .iter()
            .filter(|(_, r)| r.alive && !self.is_friend(r))
        {
            if let Some((t, _)) = ray_player(eye, dir, r.pos, r.yaw, r.stance, limit) {
                if enemy.is_none_or(|e| t < e.0) {
                    enemy = Some((t, id));
                }
            }
        }
        if enemy.is_none() {
            let mut best = 0.035f32;
            for (&id, r) in self
                .remotes
                .iter()
                .filter(|(_, r)| r.alive && !self.is_friend(r))
            {
                let to = r.pos + vec3(0.0, 1.0, 0.0) - eye;
                let d = to.length();
                let ang = dir.dot(to / d).clamp(-1.0, 1.0).acos();
                if d < limit + 2.0 && ang < best && clear(to) {
                    best = ang;
                    enemy = Some((d, id));
                }
            }
        }
        if let Some((_, id)) = enemy {
            return Some(MarkAt::Enemy(id));
        }
        // something lying about, a crate or a car
        let mut thing: Option<(f32, u16)> = None;
        for e in self.ents.iter().chain(self.lying.values()) {
            let (id, pos, lift) = match e {
                EntSnap::Weapon { id, pos, .. }
                | EntSnap::Loot { id, pos, .. }
                | EntSnap::DeathBox { id, pos } => (*id, *pos, 0.25),
                EntSnap::AirDrop { id, pos, .. } | EntSnap::Car { id, pos, .. } => (*id, *pos, 0.8),
                _ => continue,
            };
            let to = Vec3::from_array(pos) + vec3(0.0, lift, 0.0) - eye;
            let d = to.length();
            if !(0.5..=250.0).contains(&d) || d > limit + 3.0 {
                continue;
            }
            let tol = (0.8 / d).atan().max(0.03);
            let k = dir.dot(to / d).clamp(-1.0, 1.0).acos() / tol;
            if k < 1.0 && thing.is_none_or(|t| k < t.0) && clear(to) {
                thing = Some((k, id));
            }
        }
        if let Some((_, id)) = thing {
            return Some(MarkAt::Thing(id));
        }
        // a place
        wall.map(|t| MarkAt::Spot {
            pos: (eye + dir * t).to_array(),
            danger: false,
        })
    }

    /// A ping's colour: red for enemies, otherwise whoever put it.
    pub fn ping_color(&self, p: &PingSnap) -> Color {
        match p.what {
            PingWhat::Enemy | PingWhat::Danger => Color::new(1.0, 0.3, 0.25, 1.0),
            _ if self.br.is_some() => self.mate_color(p.from),
            _ if p.from == self.my_id => Color::new(1.0, 0.85, 0.15, 1.0),
            _ => Color::new(0.45, 0.85, 1.0, 1.0),
        }
    }
}
