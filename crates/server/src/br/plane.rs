//! The plane: when bots jump and where they land, and leaving it.

use super::*;

impl Game {
    /// Gives every bot its own landing spot, as far from the others as the flight path
    /// allows (by preference next to a gun), and the moment to jump: when the plane is
    /// closest to it.
    pub(super) fn br_assign_drops(&mut self, bots: &[u8]) {
        if bots.is_empty() {
            return;
        }
        let from = self.br.plane_from;
        let dir = self.br.plane_dir;
        let run = PLANE_SPEED * self.br.plane_time;
        // what a bot can reach after leaving the plane (with some room to spare)
        let reach = glide_reach(PLANE_HEIGHT - 20.0, true) * 0.8;
        let (lo, hi) = (self.map.min, self.map.max);
        let margin = 25.0;
        let along_of = |q: Vec3| {
            let rel = vec3(q.x - from.x, 0.0, q.z - from.z);
            let along = rel.dot(dir);
            let side = (rel - dir * along).length();
            (along, side)
        };
        let fits = |q: Vec3| {
            let (along, side) = along_of(q);
            side < reach
                && along > run * 0.06
                && along < run * 0.94
                && q.x > lo.x + margin
                && q.x < hi.x - margin
                && q.z > lo.y + margin
                && q.z < hi.y - margin
        };
        // candidates: guns on the ground, then any walkable ground
        let mut cands: Vec<(Vec3, f32)> = self
            .pickups
            .iter()
            .map(|pk| pk.pos)
            .filter(|q| fits(*q))
            .map(|q| (q, 12.0))
            .collect();
        cands.retain(|c| self.reachable(c.0));
        for _ in 0..(bots.len() * 6).max(60) {
            let q = self.nav.random_point(&mut self.rng);
            if fits(q) {
                cands.push((q, 0.0));
            }
        }
        let mut chosen: Vec<Vec3> = Vec::new();
        // squadmates come down together: the first of a squad picks, the others follow
        let mut squad_spot: Vec<(u8, Vec3, f64)> = Vec::new();
        for &id in bots {
            let squad = self.p(id).map_or(0, |p| p.squad);
            if let Some(&(_, spot, at)) = squad_spot.iter().find(|s| s.0 == squad && squad != 0) {
                let n = squad_spot.len() as f32;
                let off = vec3((n * 2.4).cos() * 7.0, 0.0, (n * 2.4).sin() * 7.0);
                let land = self.nav.snap(spot + off).unwrap_or(spot);
                if let Some(b) = self.pm(id).and_then(|p| p.bot.as_mut()) {
                    b.br_jump_at = at + 0.3;
                    b.br_land = Some(land);
                }
                squad_spot.push((squad, spot, at));
                continue;
            }
            let pick = cands
                .iter()
                .enumerate()
                .map(|(i, (q, bonus))| {
                    let gap = chosen
                        .iter()
                        .map(|c| vec2(c.x - q.x, c.z - q.z).length())
                        .fold(400.0f32, f32::min);
                    (i, gap + bonus + self.rng.range(0.0, 6.0))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(i, _)| i);
            let time = self.time;
            let t0 = self.br.plane_t0;
            let window = self.br.plane_time as f64;
            let (land, at) = match pick {
                Some(i) => {
                    let (q, _) = cands.swap_remove(i);
                    let (along, _) = along_of(q);
                    // jump when the plane passes it
                    let at = t0 + (along / PLANE_SPEED) as f64;
                    (Some(q), at.clamp(time + 0.1 * window, time + 0.95 * window))
                }
                // nowhere left: anywhere along the way
                None => (None, time + self.rng.range(0.15, 0.8) as f64 * window),
            };
            if let Some(q) = land {
                chosen.push(q);
                if squad != 0 {
                    squad_spot.push((squad, q, at));
                }
            }
            if let Some(b) = self.pm(id).and_then(|p| p.bot.as_mut()) {
                b.br_jump_at = at;
                b.br_land = land;
            }
        }
    }

    /// Leaves the plane.
    pub fn br_jump(&mut self, id: u8) {
        let time = self.time;
        if self.mode != Mode::Br || !self.br.running {
            return;
        }
        // the plane starts and ends its run beyond the map's edge: whoever jumps there (or
        // is pushed out at the end) comes down just inside it
        let margin = 25.0;
        let p = self.br.plane_pos(time);
        let pos = vec3(
            p.x.clamp(self.map.min.x + margin, self.map.max.x - margin),
            p.y - 4.0,
            p.z.clamp(self.map.min.y + margin, self.map.max.y - margin),
        );
        let yaw = yaw_of(self.br.plane_dir);
        let Some(p) = self.players[id as usize].as_mut() else {
            return;
        };
        if !p.in_plane {
            return;
        }
        p.in_plane = false;
        p.alive = true;
        p.hp = MAX_HEALTH;
        p.inv = Default::default();
        // empty hands: whatever you fight with, you find (one medkit to start with)
        p.inv.medkits = 1;
        p.empty = 0b11;
        p.body = Body {
            pos,
            vel: p.body.vel,
            on_ground: false,
            stance: Stance::Stand,
            chute: Chute::FreeFall,
            ..Default::default()
        };
        p.yaw = yaw;
        p.pitch = 0.0;
        p.flags = 0;
        p.slot = 0;
        p.weapons = [
            WState {
                id: WeaponId::Fort17,
                mag: 0,
                reserve: 0,
            },
            WState {
                id: WeaponId::Glock18,
                mag: 0,
                reserve: 0,
            },
        ];
        p.grenades = 0;
        p.smokes = 0;
        p.reload_end = None;
        p.switch_end = time;
        p.shot_budget = 1.0;
        p.attackers.clear();
        p.life = p.life.wrapping_add(1);
        p.protected_until = 0.0;
        p.history.clear();
        p.move_budget = 3.0;
        p.last_damage = -100.0;
        p.progress = 0.0;
        let life = p.life;
        let loadout = p.loadout;
        if let Some(b) = p.bot.as_mut() {
            b.on_spawn();
        }
        self.send(
            id,
            S2C::Spawned {
                pos: pos.to_array(),
                yaw,
                life,
                loadout,
            },
        );
    }
}
