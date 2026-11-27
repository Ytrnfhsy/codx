//! The gun in hand: sights and scopes, spread, the bow's draw, firing, reloading, switching.

use super::*;

impl GameClient {
    /// Sleeve and hand colours for a first-person view.
    pub fn vm_colors(&self, id: u8, team: Team) -> ([u8; 3], [u8; 3]) {
        crate::soldier::arm_colors(self.seen_team(team), id)
    }

    /// First-person weapon animation state.
    pub fn vm_state(&self) -> VmState {
        let now = self.now;
        VmState {
            draw: self.draw_now(),
            loaded: self.weapons[self.slot as usize & 1].1 > 0,
            sight: self.sight(),
            mods: self.mods(),
            weapon: self.weapon(),
            sleeve: self.vm_colors(self.my_id, self.team).0,
            hand: self.vm_colors(self.my_id, self.team).1,
            ads: self.ads_t,
            bob_phase: self.bob_phase,
            bob: self.walk_amount * (1.0 - self.ads_t * 0.85) * self.tuning.movement.view_bob,
            sway: self.sway,
            recoil: self.recoil_vis,
            sprint: self.sprint_pose,
            reload: self
                .reload_until
                .map(|(end, dur)| (1.0 - ((end - now) / dur) as f32).clamp(0.0, 1.0)),
            raise: if now < self.switch_until {
                ((self.switch_until - now) as f32 / self.switch_dur.max(0.05)).clamp(0.0, 1.0)
            } else {
                0.0
            },
            knife: (now < self.melee_until)
                .then(|| (1.0 - ((self.melee_until - now) / 0.45) as f32).clamp(0.0, 1.0)),
            throwing: self.cooking.is_some() || now < self.throw_until,
            flash: now - self.muzzle_flash < 0.05,
            bare: self.hand_empty(),
        }
    }

    // ------------------------------------------------------------------ simulation

    fn current_spread(&self) -> f32 {
        let def = self.ws();
        let mut hip = def.hip_spread;
        hip *= match self.body.stance {
            Stance::Stand => 1.0,
            Stance::Crouch => 0.8,
            Stance::Prone => 0.65,
        };
        let mut base = hip + (def.ads_spread - hip) * self.ads_t;
        // a bow is as steady as it is drawn: a full draw puts the arrow where you look
        if self.weapon().draw_time() > 0.0 {
            base *= 1.0 - 0.9 * smooth(self.draw_now());
        }
        let speed = vec2(self.body.vel.x, self.body.vel.z).length();
        let mv = (speed / self.tuning.movement.walk_speed).min(1.3)
            * def.move_spread
            * (1.0 - self.ads_t * 0.8);
        let air = if self.body.on_ground { 0.0 } else { 3.0 };
        // a vertical grip steadies the gun
        let grip = if self.has_mod(toon_shared::inventory::MOD_GRIP) {
            0.8
        } else {
            1.0
        };
        (base + mv) * grip + air + self.spread_bonus
    }

    pub fn spread_px(&self, fovy: f32) -> f32 {
        let s = self.current_spread().to_radians();
        s.tan() / (fovy * 0.5).tan() * screen_height() * 0.5
    }

    /// Battle royale: no gun in hand (everybody lands with just a knife).
    pub fn hand_empty(&self) -> bool {
        self.br
            .as_ref()
            .is_some_and(|b| b.empty & (1 << (self.slot & 1)) != 0)
    }

    pub fn scoped(&self) -> bool {
        (self.weapon().def().scoped || self.sight() >= 2) && self.ads_t > 0.85
    }

    /// Battle royale sight on the gun in hand (0 = iron sights).
    pub fn sight(&self) -> u8 {
        let Some(br) = &self.br else { return 0 };
        let l = br.inv.sights[self.slot as usize & 1];
        if toon_shared::inventory::sight_fits(l, self.weapon()) {
            l
        } else {
            0
        }
    }

    /// Attachments on the gun in hand, by place (muzzle, magazine, grip).
    pub fn mods(&self) -> [u8; 3] {
        self.br
            .as_ref()
            .map(|b| b.inv.mods[self.slot as usize & 1])
            .unwrap_or_default()
    }

    /// The gun in hand has this attachment on.
    pub fn has_mod(&self, kind: u8) -> bool {
        self.mods()[toon_shared::inventory::mod_place(kind)] == kind
    }

    /// Rounds the magazine of the gun in hand holds.
    pub fn mag_cap(&self) -> u16 {
        toon_shared::inventory::mag_size(
            self.ws().mag,
            self.has_mod(toon_shared::inventory::MOD_EXTMAG),
        )
    }

    /// Magnification when aiming: a scope's (a variable one at what it is turned to),
    /// otherwise the gun's own.
    pub fn ads_zoom(&self) -> f32 {
        // a rifle's own scope that turns (no scope from the loot on it)
        if self.gun_scope_turns() {
            let (lo, hi) = self.weapon().zoom_range();
            let k = self.scope_turn[self.slot as usize & 1].clamp(0.0, 1.0);
            return lo + (hi - lo) * k;
        }
        toon_shared::inventory::aimed_zoom(self.ws().ads_zoom, self.sight(), self.sight_zoom_now())
    }

    /// The gun in hand has its own variable scope and nothing put over it.
    pub fn gun_scope_turns(&self) -> bool {
        let (lo, hi) = self.weapon().zoom_range();
        hi > lo && self.sight() < 2
    }

    /// The scope can be turned (its own, or one from the loot).
    pub fn scope_turns(&self) -> bool {
        self.gun_scope_turns() || toon_shared::inventory::sight_variable(self.sight())
    }

    /// The distance the scope's holdover is set for: what the crosshair rests on, but an
    /// enemy it rested on in the last two seconds is kept (lifting the barrel over him
    /// to hold over must not lose his range).
    pub fn hold_range(&self) -> Option<f32> {
        match self.range_lock {
            Some((d, t)) if self.now - t < 2.0 => Some(d),
            _ => self.scope_target().map(|r| r.0),
        }
    }

    /// What the sight on the gun in hand magnifies right now.
    pub fn sight_zoom_now(&self) -> f32 {
        let l = self.sight();
        let (lo, hi) = toon_shared::inventory::sight_zoom_range(l);
        let k = self.scope_turn[self.slot as usize & 1].clamp(0.0, 1.0);
        lo + (hi - lo) * k
    }

    /// Stats of the weapon in hand.
    pub fn ws(&self) -> &WeaponStats {
        self.tuning.weapon(self.weapon())
    }

    /// Aim sway while aiming down sights (radians). Heavier/longer guns sway more;
    /// crouching, lying down and (for scopes) holding breath steady it.
    pub fn sway_offset(&self) -> Vec2 {
        if !self.alive {
            return Vec2::ZERO;
        }
        // a bow held fully drawn: steady for two seconds, then the arms start shaking
        if self.draw_from.is_some() {
            let tired = (self.draw_held() - 2.0).max(0.0);
            let k = 0.0006 + (tired * 0.004).min(0.012);
            let t = self.now as f32;
            return vec2(
                (t * 1.3).sin() + (t * 7.1).sin() * 0.5 * tired.min(1.0),
                (t * 1.9).sin() * 0.8 + (t * 8.3).sin() * 0.5 * tired.min(1.0),
            ) * k;
        }
        if self.ads_t < 0.5 {
            return Vec2::ZERO;
        }
        let scoped = self.weapon().def().scoped;
        // a stronger sight shows the sway more
        let magnified = scoped || self.sight() >= 2;
        let stance = match self.body.stance {
            Stance::Stand => 1.0,
            Stance::Crouch => 0.7,
            Stance::Prone => 0.4,
        };
        let base = if scoped {
            0.007
        } else if magnified {
            0.0018 + 0.0012 * self.ads_zoom()
        } else {
            0.0018
        };
        let k = base * self.ws().sway * stance * if scoped { self.steady } else { 1.0 };
        let t = self.now as f32;
        vec2(
            (t * 0.9).sin() * 1.5 + (t * 2.3).sin() * 0.4,
            (t * 1.7).sin() * 1.1,
        ) * k
    }

    /// The scope's rangefinder: how far off is what the crosshair rests on (a player or
    /// the world), up to 2 km.
    pub fn scope_range(&self) -> Option<f32> {
        self.scope_target().map(|r| r.0)
    }

    /// What the crosshair rests on: how far, and whether it is a player.
    pub fn scope_target(&self) -> Option<(f32, bool)> {
        const MAX: f32 = 2000.0;
        let eye = self.eye();
        let dir = self.aim_dir();
        let mut best = self.world.raycast(eye, dir, MAX).map(|h| (h.0, false));
        for r in self.remotes.values().filter(|r| r.alive) {
            let len = best.map_or(MAX, |b| b.0);
            if let Some((t, _)) = ray_player(eye, dir, r.pos, r.yaw, r.stance, len) {
                best = Some((t, true));
            }
        }
        best
    }

    pub fn aim_dir(&self) -> Vec3 {
        let s = self.sway_offset();
        look_dir(self.yaw + s.x, self.pitch + s.y)
    }

    /// What `G` throws in this life (the battle royale has only frags on it).
    pub fn lethal_now(&self) -> toon_shared::weapons::Lethal {
        if self.mode == Mode::Br {
            toon_shared::weapons::Lethal::Frag
        } else {
            self.kit.lethal
        }
    }

    /// What `Q` throws in this life.
    pub fn tactical_now(&self) -> toon_shared::weapons::Tactical {
        if self.mode == Mode::Br {
            toon_shared::weapons::Tactical::Smoke
        } else {
            self.kit.tactical
        }
    }

    /// How much a drawn bow narrows the view (1 = not at all).
    pub fn draw_zoom(&self) -> f32 {
        1.0 + 0.3 * smooth(self.draw_now())
    }

    /// Seconds a full draw has been held (the arms tire after a while).
    pub fn draw_held(&self) -> f32 {
        let t = self.weapon().draw_time();
        match self.draw_from {
            Some(s) if t > 0.0 => ((self.now - s) as f32 - t).max(0.0),
            _ => 0.0,
        }
    }

    /// Where an arrow loosed now would come down (and how far that is), if it hits
    /// anything within its range.
    pub fn arrow_landing(&self) -> Option<(Vec3, f32)> {
        let w = self.weapon();
        if w.draw_time() <= 0.0 || self.draw_from.is_none() {
            return None;
        }
        let mut b = toon_shared::ballistics::Ballistics::of(w);
        let (speed, _) = WeaponId::draw_power(self.draw_now());
        b.velocity *= speed;
        b.max_range *= speed;
        let eye = self.eye();
        let path = toon_shared::ballistics::trace_path(&self.world, eye, self.aim_dir(), &b);
        let end = path.end();
        (path.hit_world || end.y < eye.y - 30.0).then(|| (end, path.length()))
    }

    /// How far the bow is drawn now (0..1).
    pub fn draw_now(&self) -> f32 {
        let t = self.weapon().draw_time();
        match self.draw_from {
            Some(s) if t > 0.0 => ((self.now - s) as f32 / t).clamp(0.0, 1.0),
            _ => 0.0,
        }
    }

    pub(super) fn fire(&mut self, audio: &Option<&Audio>, draw: f32) {
        let now = self.now;
        let w = self.weapon();
        let def = self.ws().clone();
        if now - self.last_fire > (def.fire_interval() * 2.5 + 0.05) as f64 {
            self.shot_streak = 0;
        }
        self.next_fire = now + (def.fire_interval() / w.spin(self.spin_held)) as f64;
        self.weapons[self.slot as usize].1 -= 1;
        self.last_fire = now;
        let spread = self.current_spread().to_radians();
        let eye = self.eye();
        let base = self.aim_dir();
        let dirs: Vec<Vec3> = (0..def.pellets.max(1))
            .map(|_| cone(&mut self.rng, base, spread))
            .collect();
        self.net.send(C2S::Fire {
            weapon: w,
            origin: eye.to_array(),
            dirs: dirs.iter().map(|d| d.to_array()).collect(),
            draw,
        });

        // local visuals: the bullet's arc through the world, and remote players on it
        // (an arrow leaves from where it is seen on the bow)
        let muzzle = if w == WeaponId::Bow {
            self.view_to_world(crate::render::bow_pose(draw, self.ads_t).tip)
        } else {
            self.viewmodel_muzzle_world()
        };
        let mut b = toon_shared::ballistics::Ballistics::of(w);
        if w.draw_time() > 0.0 {
            let (speed, _) = WeaponId::draw_power(draw);
            b.velocity *= speed;
            b.max_range *= speed;
        }
        let slow = w.shot() != toon_shared::weapons::Shot::Bullet;
        for d in dirs.iter().take(3) {
            // bullets go through planks and cloth (as on the server)
            let path = toon_shared::ballistics::trace_path(&self.world, eye, *d, &b);
            let mut hit = path.end();
            let mut flesh = false;
            let mut stop = path.length();
            'path: for i in 0..path.points.len().saturating_sub(1) {
                let (a, c) = (path.points[i], path.points[i + 1]);
                let len = a.distance(c);
                if len < 1e-4 {
                    continue;
                }
                let dir = (c - a) / len;
                // where the target will be when the bullet gets there
                let t_arrive = i as f32 * toon_shared::ballistics::STEP;
                for r in self.remotes.values() {
                    if !r.alive || self.is_friend(r) {
                        continue;
                    }
                    let at = r.pos + r.vel * t_arrive;
                    if let Some((rt, _)) = ray_player(a, dir, at, r.yaw, r.stance, len) {
                        hit = a + dir * rt;
                        stop = path.dist[i] + rt;
                        flesh = true;
                        break 'path;
                    }
                }
            }
            for p in path.pierced.iter().filter(|p| p.0 < stop) {
                // (wood lets less through than cloth)
                self.fx.pierce(path.point_at(p.0), p.1 < 0.9);
            }
            if slow {
                let rocket = w.shot() == toon_shared::weapons::Shot::Rocket;
                self.fx.fly(muzzle, hit, b.velocity, rocket, flesh);
                continue;
            }
            self.fx.tracer(muzzle, hit);
            if flesh {
                self.fx.flesh(hit);
            } else if path.hit_world {
                let d = (hit - muzzle).normalize_or_zero();
                self.bullet_hit(hit, d);
            }
        }
        self.muzzle_flash = now;
        // recoil
        // recoil: builds up over a burst (LMGs!), steadier crouched / prone (bipod)
        let ads_k = 1.0 - self.ads_t * 0.45;
        let stance_k = match self.body.stance {
            Stance::Stand => 1.0,
            Stance::Crouch => def.crouch_recoil,
            Stance::Prone => def.prone_recoil,
        };
        let ramp = def.recoil_scale(self.shot_streak);
        self.shot_streak += 1;
        // a compensator and a grip tame the kick
        let stance_k = stance_k
            * if self.has_mod(toon_shared::inventory::MOD_COMPENSATOR) {
                0.7
            } else {
                1.0
            }
            * if self.has_mod(toon_shared::inventory::MOD_GRIP) {
                0.85
            } else {
                1.0
            };
        let kick =
            (def.recoil_v * 0.55 * ads_k * stance_k * ramp * self.rng.range(0.8, 1.2)).to_radians();
        self.pitch = (self.pitch + kick).min(1.5);
        self.recoil_accum += kick;
        self.yaw +=
            (def.recoil_h * 0.5 * ads_k * stance_k * ramp.sqrt() * self.rng.range(-1.0, 1.0))
                .to_radians();
        self.spread_bonus = (self.spread_bonus + def.shot_spread).min(6.0);
        let heft = w.def().heft;
        self.recoil_vis = (self.recoil_vis + 0.5 + heft * 0.5).min(1.4);
        self.shake = (self.shake + heft * 0.05).min(0.3);
        if let Some(a) = audio {
            let quiet = self.has_mod(toon_shared::inventory::MOD_SUPPRESSOR);
            a.play(Sfx::for_weapon(w), if quiet { 0.3 } else { 0.85 });
            if !slow {
                a.play_later(Sfx::Shell, 0.35 + self.rng.range(0.0, 0.2), 0.25);
            }
        }
    }

    /// A point in front of the eye (x right, y up, −z ahead) in the world.
    pub fn view_to_world(&self, v: Vec3) -> Vec3 {
        let f = look_dir(self.yaw, self.pitch);
        let r = f.cross(Vec3::Y).normalize_or(Vec3::X);
        let u = r.cross(f);
        self.eye() + r * v.x + u * v.y - f * v.z
    }

    pub fn viewmodel_muzzle_world(&self) -> Vec3 {
        let (f, r) = yaw_basis(self.yaw);
        let d = look_dir(self.yaw, self.pitch);
        let up = r.cross(d);
        let _ = f;
        let k = 1.0 - self.ads_t;
        self.eye() + d * 0.9 + r * (0.16 * k) - up * (0.12 * k + 0.04)
    }

    pub(super) fn start_reload(&mut self) {
        let (w, mag, res) = self.weapons[self.slot as usize];
        let def = self.tuning.weapon(w);
        if self.reload_until.is_some() || mag >= self.mag_cap() || res == 0 {
            return;
        }
        let dur = def.reload_time as f64;
        self.reload_until = Some((self.now + dur, dur));
        self.reload_cue = 0;
        self.net.send(C2S::Reload);
    }

    pub(crate) fn switch_to(&mut self, slot: u8) {
        if slot == self.slot || slot > 1 {
            return;
        }
        self.slot = slot;
        self.reload_until = None;
        let swap = self.tuning.weapon(self.weapons[slot as usize].0).swap_time;
        self.switch_until = self.now + swap as f64;
        self.switch_dur = swap;
        self.shot_streak = 0;
        self.ads_t = 0.0;
        self.net.send(C2S::Switch(slot));
    }
}
