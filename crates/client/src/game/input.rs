//! The frame's update: input, movement prediction, actions.

use super::*;

impl GameClient {
    /// Per-frame update. `input_enabled` is false while menus are open.
    pub fn update(&mut self, dt: f32, settings: &Settings, audio: Option<&Audio>) {
        self.fx.cap = [350, 1500, 3000][settings.effects.min(2) as usize];
        if settings.skin != self.sent_skin {
            self.sent_skin = settings.skin;
            self.net.send(C2S::SetSkin(settings.skin));
        }
        if settings.lang != self.sent_lang {
            self.sent_lang = settings.lang;
            self.net.send(C2S::Lang(settings.lang));
        }
        self.now = crate::clock::now();
        let now = self.now;
        // while a replay plays, the live game is silent
        self.hud_hp = if self.me.hp < self.hud_hp {
            (self.hud_hp - dt * 45.0).max(self.me.hp)
        } else {
            self.me.hp
        };
        // doors swing to where they stand (a new map: straight there)
        if self.door_swing.len() != self.world.doors.len() {
            self.door_swing = self
                .world
                .door_open
                .iter()
                .map(|o| *o as u8 as f32)
                .collect();
        }
        for (s, open) in self.door_swing.iter_mut().zip(&self.world.door_open) {
            let to = *open as u8 as f32;
            *s += (to - *s).clamp(-dt * 4.5, dt * 4.5);
        }
        let mut audio_opt = if self.killcam.is_some() { None } else { audio };
        if let (Some(a), None) = (audio, &self.killcam) {
            a.set_listener_yaw(self.camera().1);
        }
        for m in self.net.poll() {
            self.handle_msg(m, &mut audio_opt);
        }
        if let Some(a) = audio {
            a.tick();
        }
        // killcam replay
        if self.pending_killcam.as_ref().is_some_and(|p| now >= p.0) {
            let (_, info, final_cam) = self.pending_killcam.take().unwrap();
            if final_cam || !self.alive {
                self.start_killcam(info, final_cam);
            }
        }
        if let Some(k) = &mut self.killcam {
            k.update(dt, &self.world, &self.tuning, audio);
            if k.done {
                self.killcam = None;
                if self.summary_active() {
                    self.reel += 1;
                }
            }
        }
        // the moments: cut while the match is on, played one after another on the results
        if self.state == MatchState::Playing {
            let clock = (now - self.match_t0) as f32;
            self.highlights.update(&self.recorder, clock);
        }
        if self.summary_active() && self.killcam.is_none() && !self.highlights.clips.is_empty() {
            if is_key_pressed(KeyCode::Space) {
                self.reel += 1;
            }
            let n = self.highlights.clips.len();
            for _ in 0..n {
                let i = self.reel % n;
                self.killcam = self.highlights.clips[i].replay();
                if self.killcam.is_some() {
                    break;
                }
                self.reel += 1;
            }
        } else if self.summary_active() && is_key_pressed(KeyCode::Space) {
            // next moment now
            self.killcam = None;
            self.reel += 1;
        }
        for (_, r) in &mut self.ragdolls {
            r.update(&self.world, dt);
        }
        self.ragdolls
            .retain(|(_, r)| r.age < crate::ragdoll::LIFETIME);
        self.blows.retain(|_, b| now - b.1 < 3.0);
        self.explosions.retain(|e| now - e.1 < 2.0);
        if self.net_silence() > NET_GIVE_UP {
            self.leave = Some(
                toon_shared::lang::t(
                    "З'єднання з сервером втрачено",
                    "Lost connection to the server",
                )
                .into(),
            );
        }

        let playing_input =
            self.overlay == Overlay::None && self.chat_input.is_none() && !self.big_map;
        // the big map: a click puts a marker there, a right click takes it away
        if self.big_map && self.overlay == Overlay::None {
            if let Some(img) = &self.level.image {
                let (x0, y0, side) = crate::hud::big_map_rect();
                let (mx, my) = mouse_position();
                let inside = mx >= x0 && my >= y0 && mx <= x0 + side && my <= y0 + side;
                if inside && is_mouse_button_pressed(MouseButton::Left) {
                    let k = img.size / side;
                    self.marker = Some(img.origin + vec2(mx - x0, my - y0) * k);
                    // the squad sees it too
                    self.net.send(C2S::Marker(self.marker.map(|m| [m.x, m.y])));
                }
                if is_mouse_button_pressed(MouseButton::Right) {
                    self.marker = None;
                    self.net.send(C2S::Marker(None));
                }
            }
        }
        // reached the marker: it has done its job
        if let Some(mk) = self.marker {
            let at = self
                .seat()
                .and_then(|_| self.seated_at(self.my_id))
                .map_or(self.body.pos, |s| s.0);
            let here = vec2(at.x, at.z);
            if self.alive && here.distance(mk) < MARKER_REACHED {
                self.marker = None;
                self.net.send(C2S::Marker(None));
            }
        }
        // ---- mouse look
        let md = self.mouse_delta;
        if playing_input && self.piloting.is_some() {
            self.pilot_look(md, settings.sensitivity * 0.0011, settings.invert_y);
        } else if playing_input {
            let zoom = (1.0 + (self.ads_zoom() - 1.0) * self.ads_t) * self.draw_zoom();
            let sens = settings.sensitivity * 0.0011 / zoom;
            self.yaw -= md.x * sens;
            let dy = md.y * sens * if settings.invert_y { -1.0 } else { 1.0 };
            self.pitch = (self.pitch - dy).clamp(-1.5, 1.5);
            if dy > 0.0 {
                // pulling down compensates recoil, so less to recover later
                self.recoil_accum = (self.recoil_accum - dy).max(0.0);
            }
            // heavier weapons lag behind the view more
            let inertia = 0.00025 + self.ws().weight * 0.00004;
            self.sway = (self.sway + vec2(md.x, md.y) * inertia)
                .clamp(Vec2::splat(-0.05), Vec2::splat(0.05));
        }
        let settle = 8.0 / (1.0 + self.ws().weight * 0.08);
        self.sway *= 1.0 - (dt * settle).min(1.0);
        // aim returns after a burst
        let ws = self.ws();
        if self.recoil_accum > 0.0 && now - self.last_fire > (ws.fire_interval() * 1.5) as f64 {
            let d = self.recoil_accum * (1.0 - (-ws.recoil_recover * dt).exp());
            self.pitch -= d;
            self.recoil_accum -= d;
        }
        self.yaw = wrap_angle(self.yaw);

        if self.br_out() {
            self.update_br_spectate(playing_input);
        } else if self.spectate.is_some() || self.spec_free {
            self.spectate = None;
            self.spec_free = false;
        }
        if !self.team.is_playing()
            || (!self.alive && self.state != MatchState::PostMatch && self.team == Team::Spectator)
            || self.br_out() && self.spectate.is_none()
        {
            self.update_spectator(dt, playing_input);
        }

        // battle royale: riding the plane, the jump key gets you out
        if !self.alive
            && playing_input
            && self.br.as_ref().is_some_and(|b| b.in_plane)
            && binds::pressed(Action::Jump)
        {
            self.net.send(C2S::BrJump);
        }

        self.update_cars(dt);
        // flames over molotov fires
        if now >= self.next_flame {
            self.next_flame = now + 0.05;
            let fires: Vec<(Vec3, f32)> = self
                .ents
                .iter()
                .filter_map(|e| match e {
                    EntSnap::Fire { pos, radius, .. } => Some((Vec3::from_array(*pos), *radius)),
                    _ => None,
                })
                .collect();
            for (p, r) in fires {
                self.fx.flames(p, r);
            }
        }
        // red smoke over landed supply crates
        if now >= self.next_flare {
            self.next_flare = now + 0.15;
            let flares: Vec<Vec3> = self
                .ents
                .iter()
                .filter_map(|e| match e {
                    EntSnap::AirDrop {
                        pos, landed: true, ..
                    } => Some(Vec3::from_array(*pos) + vec3(0.0, 1.5, 0.0)),
                    _ => None,
                })
                .collect();
            for p in flares {
                self.fx.flare_smoke(p);
            }
        }
        // ---- local player
        let steer = self.alive && playing_input && !self.frozen() && self.killcam.is_none();
        self.update_piloting(dt, steer);
        // while steering a vehicle the soldier just stands there
        let can_act = steer && self.piloting.is_none();
        let key = |a: Action| can_act && binds::down(a);
        let pressed = |a: Action| can_act && binds::pressed(a);
        let def = self.ws().clone();
        let scoped_weapon = self.weapon().def().scoped;
        if self.alive {
            if pressed(Action::Crouch) {
                self.want_stance = if self.want_stance == Stance::Crouch {
                    Stance::Stand
                } else {
                    Stance::Crouch
                };
            }
            if pressed(Action::Prone) {
                self.want_stance = if self.want_stance == Stance::Prone {
                    Stance::Stand
                } else {
                    Stance::Prone
                };
            }
            let mut fwd = 0.0;
            let mut right = 0.0;
            if key(Action::Forward) {
                fwd += 1.0;
            }
            if key(Action::Back) {
                fwd -= 1.0;
            }
            if key(Action::Right) {
                right += 1.0;
            }
            if key(Action::Left) {
                right -= 1.0;
            }
            let downed = self.downed();
            let jump = pressed(Action::Jump) && !downed;
            if jump {
                self.want_stance = Stance::Stand;
            }
            // knocked down: crawling, no aiming, no sprinting
            if downed {
                self.want_stance = Stance::Prone;
            }
            // (a click shorter than a frame still counts)
            let ads_held = can_act
                && (binds::down(Action::Aim) || binds::pressed(Action::Aim) || self.hold_ads)
                && !downed;
            let reloading = self.reload_until.is_some();
            if self.winded && (!key(Action::Sprint) || self.stamina >= self.sprint_max() * 0.35) {
                self.winded = false;
            }
            // Shift with a movement key gets you up off your knees or belly (and running
            // once you are up); Shift alone does not (a sniper lying down holds his
            // breath with it), nor anything while aiming
            if key(Action::Sprint)
                && (fwd != 0.0 || right != 0.0)
                && !downed
                && self.body.stance != Stance::Stand
                && !ads_held
                && self.ads_t <= 0.0
            {
                self.want_stance = Stance::Stand;
            }
            let want_sprint = key(Action::Sprint)
                && !downed
                && fwd > 0.0
                && self.body.stance == Stance::Stand
                && !ads_held
                && self.stamina > 0.0
                && !self.winded
                && now - self.last_fire > 0.2;
            // scoped snipers use Shift to hold breath instead
            let want_sprint = want_sprint && !(scoped_weapon && self.ads_t > 0.5);
            if self.sprinting && !want_sprint {
                self.sprint_end = now;
            }
            self.sprinting = want_sprint;
            let target = if self.sprinting { 1.0 } else { 0.0 };
            self.sprint_pose += (target - self.sprint_pose) * (dt * 12.0).min(1.0);
            if self.sprinting {
                self.stamina = (self.stamina - dt).max(0.0);
                if self.stamina <= 0.0 {
                    self.winded = true;
                }
            } else {
                self.stamina = (self.stamina + dt * self.tuning.movement.sprint_recover)
                    .min(self.sprint_max());
            }
            let holding = scoped_weapon && self.ads_t > 0.5 && binds::down(Action::Sprint);
            if holding {
                self.breath = (self.breath - dt).max(0.0);
            } else {
                self.breath = (self.breath + dt * 0.5).min(4.0);
            }
            // the sway eases off while you hold your breath (the sight drifts back to the
            // crosshair instead of stopping where it was) and comes back gently after
            let (want, rate) = if holding && self.breath > 0.0 {
                (0.05, 3.0)
            } else {
                (1.0, 1.2)
            };
            self.steady += (want - self.steady) * (dt * rate).min(1.0);
            // after sprinting the weapon has to come back up first (longer for heavy guns)
            let raising = now - self.sprint_end < def.sprint_out_time as f64;
            let ads_ok = ads_held
                && !self.hand_empty()
                && !self.sprinting
                && !raising
                && !reloading
                && now >= self.switch_until
                && now >= self.melee_until;
            // up at the gun's pace, down at once (a released button drops the sight
            // straight away, so quick clicks never leave it hanging)
            let speed = 1.0 / def.ads_time.max(0.05);
            self.ads_t = if ads_ok {
                (self.ads_t + dt * speed).min(1.0)
            } else {
                (self.ads_t - dt * speed.max(1.0 / 0.08) * 1.5).max(0.0)
            };

            let inp = MoveInput {
                forward: if self.frozen() { 0.0 } else { fwd },
                right: if self.frozen() { 0.0 } else { right },
                yaw: self.yaw,
                jump: jump && !self.frozen(),
                sprint: self.sprinting,
                ads: self.ads_t > 0.5,
                want_stance: self.want_stance,
                speed_mult: def.move_mult,
                sprint_mult: def.sprint_mult,
                weight: def.weight,
            };
            let eye_before = self.body.stance.eye();
            let seat = self
                .seat()
                .map(|(c, k)| (c.seat_pos(k), c.state.velocity(), k));
            if seat.is_none() && self.was_seated {
                // out of the car: back on our feet
                self.want_stance = Stance::Stand;
            }
            self.was_seated = seat.is_some();
            if let Some((at, vel, k)) = seat {
                // in a car: it carries us (and the driver drives it)
                self.body.pos = at;
                self.body.vel = vel;
                self.body.on_ground = true;
                self.body.stance = Stance::Crouch;
                self.want_stance = Stance::Crouch;
                if k == 0 && now >= self.next_drive {
                    self.next_drive = now + 1.0 / 30.0;
                    let throttle = (key(Action::Forward) as i8 - key(Action::Back) as i8) * 100;
                    let steer = (key(Action::Left) as i8 - key(Action::Right) as i8) * 100;
                    self.net.send(C2S::Car(CarCmd::Drive {
                        throttle,
                        steer,
                        brake: key(Action::Jump),
                    }));
                }
            } else {
                let mut t = dt.min(0.1);
                let kicks = self.body.wall_jumps;
                while t > 0.0 {
                    let h = t.min(1.0 / 60.0);
                    self.body.step(&self.world, &inp, &self.tuning.movement, h);
                    t -= h;
                }
                if self.body.wall_jumps > kicks {
                    self.kicked_until = crate::clock::now() + 0.15;
                }
            }
            if !self.body.on_ground {
                self.fall_speed = self.fall_speed.max(-self.body.vel.y);
            } else if !self.was_on_ground {
                if self.fall_speed > 6.5 {
                    if let Some(a) = audio {
                        a.play(Sfx::Land, ((self.fall_speed - 5.0) / 8.0).clamp(0.2, 0.8));
                    }
                    self.shake = (self.shake + (self.fall_speed - 6.0) * 0.03).min(0.4);
                }
                self.fall_speed = 0.0;
            }
            self.was_on_ground = self.body.on_ground;
            // stairs and stance changes move the view smoothly, not in one jump
            let snap = self.body.take_snap() + (self.body.stance.eye() - eye_before);
            if snap.abs() < 1.5 {
                self.cam_smooth.y -= snap;
            }
            // sunk into something solid (a correction, a stance change in a tight spot):
            // out onto the floor above at once, and the view with it
            if !toon_shared::physics::fits(&self.world, self.body.pos, self.body.stance) {
                let up = self.body.pos + vec3(0.0, 2.0, 0.0);
                if let Some(y) = self.world.floor_below(up, 2.0) {
                    let np = vec3(self.body.pos.x, y + 0.001, self.body.pos.z);
                    if y > self.body.pos.y
                        && y - self.body.pos.y < 1.7
                        && toon_shared::physics::fits(&self.world, np, self.body.stance)
                    {
                        self.body.pos = np;
                        self.cam_smooth = Vec3::ZERO;
                    }
                }
            }
            self.cam_smooth.y = self.cam_smooth.y.clamp(-1.3, 1.3);
            if self.body.stance != self.want_stance && self.want_stance == Stance::Stand {
                // blocked from standing up — keep trying silently
            }
            // footsteps & bob
            let hs = vec2(self.body.vel.x, self.body.vel.z).length();
            // the view leans a little into a strafe
            let (_, right) = yaw_basis(self.yaw);
            let side = self.body.vel.dot(right) / self.tuning.movement.walk_speed.max(0.1);
            let lean = side.clamp(-1.2, 1.2) * 0.02 * (1.0 - self.ads_t);
            self.view_roll += (lean - self.view_roll) * (dt * 8.0).min(1.0);
            self.walk_amount += ((hs / self.tuning.movement.walk_speed).min(1.5)
                - self.walk_amount)
                * (dt * 8.0).min(1.0);
            self.bob_phase += hs * dt * 1.8;
            if self.body.on_ground && hs > 1.5 && self.body.stance == Stance::Stand {
                self.step_acc += hs * dt;
                if self.step_acc > if self.sprinting { 2.6 } else { 2.0 } {
                    self.step_acc = 0.0;
                    if let Some(a) = audio {
                        a.play(step_sfx(&self.world, self.body.pos), 0.25);
                    }
                }
            }

            // ---- weapon actions
            self.spread_bonus = (self.spread_bonus - dt * def.spread_recover).max(0.0);
            self.recoil_vis = (self.recoil_vis - dt * 7.0).max(0.0);
            if let Some((end, _)) = self.reload_until {
                if now >= end {
                    let s = self.slot as usize;
                    let m = self.mag_cap();
                    let take = m.saturating_sub(self.weapons[s].1).min(self.weapons[s].2);
                    self.weapons[s].1 += take;
                    self.weapons[s].2 -= take;
                    self.reload_until = None;
                }
            }
            if let (Some(a), Some((end, dur))) = (audio, self.reload_until) {
                let p = (1.0 - ((end - now) / dur) as f32).clamp(0.0, 1.0);
                let cues = crate::audio::reload_cues(self.weapon().def().class);
                while self.reload_cue < cues.len() && p >= cues[self.reload_cue].0 {
                    a.play(cues[self.reload_cue].1.of(self.weapon()), 0.5);
                    self.reload_cue += 1;
                }
            }
            let slot_before = self.slot;
            if pressed(Action::Primary) {
                self.switch_to(0);
            }
            if pressed(Action::Secondary) {
                self.switch_to(1);
            }
            let wheel = mouse_wheel().1;
            let variable = self.scope_turns();
            // (the range of an enemy under the crosshair is kept a moment for the holdover)
            if self.scoped() {
                if let Some((d, true)) = self.scope_target() {
                    self.range_lock = Some((d, now));
                }
            }
            if can_act && wheel.abs() > 0.1 && variable && self.ads_t > 0.5 {
                // aiming through a variable scope: the wheel turns its magnification
                // (five steps from the weakest to the strongest)
                let k = &mut self.scope_turn[self.slot as usize & 1];
                *k = (*k + wheel.signum() * 0.25).clamp(0.0, 1.0);
                // (the server keeps it for the killcam of whoever we shoot)
                let zoom = (self.ads_zoom() * 10.0).round() as u8;
                self.net.send(C2S::ScopeZoom {
                    slot: self.slot & 1,
                    zoom,
                });
                if let Some(a) = audio {
                    a.play(Sfx::Click, 0.35);
                }
            } else if can_act && wheel.abs() > 0.1 {
                let s = 1 - self.slot;
                self.switch_to(s);
            }
            if self.slot != slot_before {
                if let Some(a) = audio {
                    a.play(Sfx::Raise, 0.45);
                }
            }
            if pressed(Action::Reload) {
                self.start_reload();
            }
            // nobody shoots while skydiving or swimming
            let can_act = can_act
                && self.body.chute == toon_shared::physics::Chute::None
                && !self.body.swimming(&self.world);
            let trig_down = can_act && (binds::down(Action::Fire) || self.hold_fire);
            // also catch very quick clicks (press + release within one frame)
            let trig_pressed =
                can_act && (binds::pressed(Action::Fire) || (trig_down && !self.trigger_was_down));
            self.trigger_was_down = trig_down;
            // spawn protection: you can't be hurt, so you can't attack either.
            // Any attack key gives the protection up instead (no shot is fired).
            if self.is_protected()
                && (trig_pressed
                    || pressed(Action::Grenade)
                    || pressed(Action::Smoke)
                    || pressed(Action::Knife)
                    || pressed(Action::Killstreak))
            {
                self.protection_dropped = true;
                self.net.send(C2S::DropProtection);
                self.next_fire = now + 0.25;
                self.melee_ready = self.melee_ready.max(now + 0.25);
                self.throw_until = self.throw_until.max(now + 0.25);
                self.trigger_was_down = true; // the click must not also fire
            }
            // fall through with attacks blocked below
            // (knocked down: no attacks of any kind)
            let shielded =
                self.is_protected() || self.downed() || self.seat().is_some_and(|(_, k)| k == 0);
            let trigger = if def.auto { trig_down } else { trig_pressed };
            let raising = now - self.sprint_end < def.sprint_out_time as f64;
            let busy = shielded
                || self.reload_until.is_some()
                || now < self.switch_until
                || now < self.melee_until
                || now < self.throw_until
                || self.sprinting
                || raising
                || self.cooking.is_some()
                || self.hand_empty();
            // a machine gun spins up while the trigger is held, down when let go
            self.spin_held = if trig_down && !busy {
                self.spin_held + dt
            } else {
                (self.spin_held - dt * 2.0).max(0.0)
            };
            let bow = self.weapon().draw_time() > 0.0;
            if bow {
                // the bow: hold to draw, let go to loose
                let mag = self.weapons[self.slot as usize].1;
                if !trig_down {
                    self.draw_blocked = false;
                }
                if self.draw_from.is_some() && pressed(Action::Reload) {
                    // let the string down gently: no shot
                    self.draw_from = None;
                    self.draw_blocked = true;
                }
                if busy || mag == 0 || now < self.next_fire || self.draw_blocked {
                    self.draw_from = None;
                } else if trig_down {
                    self.draw_from.get_or_insert(now);
                } else if self.draw_from.is_some() {
                    let d = self.draw_now();
                    self.draw_from = None;
                    self.fire(&audio, d);
                }
            } else {
                self.draw_from = None;
            }
            if trigger && !bow && !busy && now >= self.next_fire {
                let (_, mag, res) = self.weapons[self.slot as usize];
                if mag > 0 {
                    self.fire(&audio, 1.0);
                } else {
                    if trig_pressed {
                        if let Some(a) = audio {
                            a.play(Sfx::Empty, 0.5);
                        }
                    }
                    if res > 0 {
                        self.start_reload();
                    }
                }
            }
            // auto reload on empty after the last shot
            if self.weapons[self.slot as usize].1 == 0
                && self.weapons[self.slot as usize].2 > 0
                && self.reload_until.is_none()
                && now - self.last_fire > 0.25
                && !busy
            {
                self.start_reload();
            }
            // a molotov or a flashbang instead (battle royale): thrown at once
            let special = if self.mode == Mode::Br {
                self.throw_kind
            } else {
                0
            };
            if special != 0 && !shielded && pressed(Action::Grenade) && now >= self.throw_until {
                let inv = self.br.as_ref().map(|b| b.inv).unwrap_or_default();
                let left = if special == toon_shared::inventory::THROW_MOLOTOV {
                    inv.molotovs
                } else {
                    inv.flashes
                };
                if left > 0 {
                    let aim = self.aim_dir();
                    let pitch =
                        aim.y.clamp(-1.0, 1.0).asin() + self.tuning.grenade.throw_arc.to_radians();
                    let flat = vec3(aim.x, 0.0, aim.z).normalize_or_zero();
                    let d = (flat * pitch.cos() + vec3(0.0, pitch.sin(), 0.0)).normalize();
                    self.net.send(C2S::Throw {
                        origin: (self.eye() + d * 0.3).to_array(),
                        dir: d.to_array(),
                        kind: special,
                    });
                    self.last_throw = now;
                    self.throw_until = now + 0.6;
                    if let Some(a) = audio {
                        a.play(Sfx::Knife, 0.4);
                    }
                } else {
                    self.throw_kind = 0;
                }
            }
            // grenade (hold G to cook)
            if !shielded
                && special == 0
                && pressed(Action::Grenade)
                && self.grenades > 0
                && self.cooking.is_none()
                && now >= self.throw_until
            {
                self.cooking = Some(now);
                self.reload_until = None;
            }
            if let Some(start) = self.cooking {
                // (a molotov has no fuse to cook)
                let cookable = self.lethal_now() == toon_shared::weapons::Lethal::Frag;
                let release = !binds::down(Action::Grenade)
                    || !can_act
                    || cookable && now - start > (self.tuning.grenade.fuse - 0.35) as f64;
                if release {
                    self.cooking = None;
                    if self.alive {
                        // tilt the throw upwards by the server's arc setting
                        let aim = self.aim_dir();
                        let pitch = aim.y.clamp(-1.0, 1.0).asin()
                            + self.tuning.grenade.throw_arc.to_radians();
                        let flat = vec3(aim.x, 0.0, aim.z).normalize_or_zero();
                        let d = (flat * pitch.cos() + vec3(0.0, pitch.sin(), 0.0)).normalize();
                        self.net.send(C2S::Grenade {
                            origin: (self.eye() + d * 0.3).to_array(),
                            dir: d.to_array(),
                            cook: (now - start) as f32,
                            smoke: false,
                        });
                        self.grenades = self.grenades.saturating_sub(1);
                        self.last_throw = now;
                        self.throw_until = now + 0.5;
                        if let Some(a) = audio {
                            a.play(Sfx::Knife, 0.4);
                        }
                    }
                }
            }
            // smoke grenade (Q): ready on press, thrown on release (no cooking)
            if !shielded
                && pressed(Action::Smoke)
                && self.smokes > 0
                && self.cooking.is_none()
                && now >= self.throw_until
            {
                self.smoke_held = true;
                self.reload_until = None;
            }
            let release = self.smoke_held && (!binds::down(Action::Smoke) || !can_act);
            if release {
                self.smoke_held = false;
            }
            if release && can_act && self.alive && !shielded && self.smokes > 0 {
                let aim = self.aim_dir();
                let pitch =
                    aim.y.clamp(-1.0, 1.0).asin() + self.tuning.grenade.throw_arc.to_radians();
                let flat = vec3(aim.x, 0.0, aim.z).normalize_or_zero();
                let d = (flat * pitch.cos() + vec3(0.0, pitch.sin(), 0.0)).normalize();
                self.net.send(C2S::Grenade {
                    origin: (self.eye() + d * 0.3).to_array(),
                    dir: d.to_array(),
                    cook: 0.0,
                    smoke: true,
                });
                self.smokes -= 1;
                self.reload_until = None;
                self.last_throw = now;
                self.throw_until = now + 0.5;
                if let Some(a) = audio {
                    a.play(Sfx::Knife, 0.4);
                }
            }
            // (empty-handed in the battle royale the knife is the weapon: the trigger
            // stabs with it too)
            let stab = pressed(Action::Knife) || self.hand_empty() && pressed(Action::Fire);
            if !shielded && stab && now >= self.melee_ready {
                self.melee_ready = now + MELEE_COOLDOWN as f64;
                self.melee_until = now + 0.45;
                self.reload_until = None;
                self.net.send(C2S::Melee);
                if let Some(a) = audio {
                    a.play(Sfx::Knife, 0.7);
                }
            }
            self.using = key(Action::Use);
            // dropped weapons: F swaps (unless F is needed for the bomb right here)
            let seated = self.me.seat.is_some();
            self.pickup_target = if seated { None } else { self.find_pickup() };
            if seated && pressed(Action::Use) && now >= self.next_pickup {
                self.net.send(C2S::Car(CarCmd::Exit));
                self.next_pickup = now + 0.6;
            }
            if let Some((ent, pick)) = self.pickup_target {
                if pressed(Action::Use) && now >= self.next_pickup && !self.near_objective() {
                    match pick {
                        Pick::Car(_) => self.net.send(C2S::Car(CarCmd::Enter(ent))),
                        Pick::Door(_) => self.net.send(C2S::Door(ent)),
                        Pick::DeathBox => {
                            self.net.send(C2S::Box(BoxCmd::Open(ent)));
                            self.overlay = Overlay::DeathBox;
                            self.inventory_opened = now;
                        }
                        _ => self.net.send(C2S::PickUp(ent)),
                    }
                    self.next_pickup = now + 0.6;
                }
            }
            if let Some((_, _, me, _)) = self.ready {
                if pressed(Action::Ready) {
                    self.net.send(C2S::Ready(!me));
                }
            }
            if pressed(Action::Ping) && self.alive {
                self.ping();
            }
            if pressed(Action::Squad) && self.lobby.is_some() {
                self.overlay = Overlay::Squad;
            }
            if self.mode == Mode::Br && pressed(Action::Heal) {
                self.net.send(C2S::UseMedkit);
            }
            if self.mode == Mode::Br && pressed(Action::Bandage) {
                self.net.send(C2S::UseBandage);
            }
            if self.mode == Mode::Br && pressed(Action::Drink) {
                self.net.send(C2S::UseDrink);
            }
            if self.mode == Mode::Br && pressed(Action::NextThrow) {
                // the next kind we have any of (a grenade at least to fall back to)
                let inv = self.br.as_ref().map(|b| b.inv).unwrap_or_default();
                let have = |k: u8| match k {
                    toon_shared::inventory::THROW_MOLOTOV => inv.molotovs > 0,
                    toon_shared::inventory::THROW_FLASH => inv.flashes > 0,
                    _ => true,
                };
                let mut k = self.throw_kind;
                for _ in 0..3 {
                    k = (k + 1) % 3;
                    if have(k) {
                        break;
                    }
                }
                self.throw_kind = k;
            }
            if self.mode == Mode::Br
                && self.br.as_ref().is_some_and(|b| !b.in_plane)
                && pressed(Action::Inventory)
            {
                self.overlay = Overlay::Inventory;
                self.inventory_opened = now;
            }
            if !shielded && pressed(Action::Killstreak) {
                match self.me.rewards.last() {
                    Some(k) if k.needs_target() => self.overlay = Overlay::Airstrike,
                    Some(Killstreak::Nrk) => self.overlay = Overlay::RobotMode,
                    Some(_) => self.net.send(C2S::UseKillstreak {
                        target: None,
                        pilot: true,
                    }),
                    None => self.center_msg(
                        toon_shared::lang::t(
                            "Немає доступних кілстріків",
                            "No killstreaks available",
                        ),
                        1.5,
                        crate::ui::MUTED,
                    ),
                }
            }
        } else {
            self.ads_t = 0.0;
            self.sprinting = false;
            self.using = false;
            // click skips the killcam, then respawns
            if playing_input && binds::pressed(Action::Fire) {
                // a click skips the killcam; once it is over, a click respawns
                if !self.skip_killcam() && self.team.is_playing() && self.me.respawn_in <= 0.0 {
                    self.net.send(C2S::Spawn);
                }
            }
        }

        // ---- send input @30 Hz
        if now - self.last_input_send >= 1.0 / 30.0 {
            self.last_input_send = now;
            self.send_drive();
            self.seq += 1;
            let mut f = 0;
            if self.sprinting {
                f |= flags::SPRINT;
            }
            if self.ads_t > 0.5 {
                f |= flags::ADS;
            }
            if self.body.on_ground {
                f |= flags::ON_GROUND;
            }
            if self.using {
                f |= flags::USE;
            }
            if now < self.kicked_until {
                f |= flags::KICKED;
            }
            self.net.send_unreliable(C2S::Input(InputState {
                seq: self.seq,
                life: self.life,
                pos: self.body.pos.to_array(),
                vel: self.body.vel.to_array(),
                yaw: self.yaw,
                pitch: self.pitch,
                stance: self.body.stance.to_u8(),
                flags: f,
                slot: self.slot,
            }));
            self.net.flush();
        }
        self.net.maybe_flush(1.0 / 60.0);

        // ---- remotes interpolation
        let live_audio = if self.killcam.is_some() { None } else { audio };
        self.update_remotes(dt, live_audio);

        // ---- misc timers
        self.shake = (self.shake - dt * 2.5).max(0.0);
        self.cam_smooth *= (-dt * 14.0).exp();
        if self.cam_smooth.length() < 0.001 {
            self.cam_smooth = Vec3::ZERO;
        }
        self.dmg_dirs.retain(|d| now - d.1 < 1.6);
        self.popups.retain(|p| now - p.1 < 1.6);
        if let Some((_, until, _)) = &self.center {
            if now > *until {
                self.center = None;
            }
        }
        self.airstrikes.retain(|a| now - a.2 < 5.0);
        self.artillery.retain(|a| now - a.2 < 9.0);
        // rain splashes on whatever is under the sky around us
        if self.weather == Weather::Rain && self.killcam.is_none() {
            let eye = self.camera().0;
            let n = (dt * 90.0 * (1.0 + settings.effects as f32)).min(8.0);
            let mut k = n.floor() as u32 + (self.rng.f32() < n.fract()) as u32;
            while k > 0 {
                k -= 1;
                let a = self.rng.range(0.0, std::f32::consts::TAU);
                let d = 1.5 + 12.0 * self.rng.f32().sqrt();
                let p = eye + vec3(a.cos() * d, 0.0, a.sin() * d);
                let top = self
                    .world
                    .floor_below(p + vec3(0.0, 20.0, 0.0), 45.0)
                    .into_iter()
                    .chain(self.world.ground(p.x, p.z))
                    .fold(f32::MIN, f32::max);
                if top > f32::MIN && (top - eye.y).abs() < 12.0 {
                    self.fx.splash(vec3(p.x, top + 0.03, p.z));
                }
            }
        }
        self.overtime_cues(audio);
        self.fx.update(dt);
        for (at, d, flesh) in std::mem::take(&mut self.fx.landed) {
            if flesh {
                self.fx.flesh(at);
            } else {
                self.bullet_hit(at, d);
            }
        }
        self.ruins.update(
            &self.map,
            &mut self.level,
            &mut self.fx,
            crate::clock::now(),
        );
        // grenade smoke trails
        for e in &self.ents {
            if let EntSnap::Grenade { pos, .. } = e {
                self.fx.smoke_trail(Vec3::from_array(*pos));
            }
        }
        // planted bomb beeps faster near the end
        if let Some(b) = &self.bomb {
            if b.planted_at.is_some() {
                let period = (b.timer / 30.0).clamp(0.12, 1.0) as f64;
                if now - self.bomb_beep > period {
                    self.bomb_beep = now;
                    if let Some(a) = audio {
                        a.play_at(Sfx::Beep, Vec3::from_array(b.pos), self.eye(), 0.6, 60.0);
                    }
                }
            }
        }
        if let Some(a) = audio {
            let ear = self.camera().0;
            let nearest = |it: &mut dyn Iterator<Item = Vec3>| {
                it.min_by(|a, b| a.distance(ear).total_cmp(&b.distance(ear)))
            };
            let heli = nearest(&mut self.ents.iter().filter_map(|e| match e {
                EntSnap::Heli { pos, .. } => Some(Vec3::from_array(*pos)),
                _ => None,
            }));
            a.heli(heli.map(|p| (p, ear)));
            // the nearest running engine: ours, or a car or boat that moves or has
            // somebody in it
            let mine = self.me.seat.filter(|_| self.alive).map(|s| s.0);
            let engine = self
                .cars
                .iter()
                .filter(|(id, c)| {
                    Some(**id) == mine
                        || c.state.speed.abs() > 0.5
                        || c.seats.iter().any(|s| *s != 255)
                })
                .map(|(id, c)| {
                    let at = if Some(*id) == mine { ear } else { c.state.pos };
                    (at, c.state.speed.abs(), c.kind.max_speed())
                })
                .min_by(|a, b| a.0.distance(ear).total_cmp(&b.0.distance(ear)));
            match engine {
                Some((at, speed, max)) => a.engine(Some((at, ear)), speed / max.max(1.0)),
                None => a.engine(None, 0.0),
            }
            // rain: loud out in it, a murmur under a roof
            let rain = (self.weather == Weather::Rain && self.killcam.is_none()).then(|| {
                let eye = self.camera().0;
                if self.world.line_of_sight(eye, eye + vec3(0.0, 25.0, 0.0)) {
                    1.0
                } else {
                    0.4
                }
            });
            a.rain(rain);
            // the nearest drone buzzes (our own one right in our ears)
            let own = self
                .piloting
                .as_ref()
                .filter(|p| p.kind != toon_shared::vehicles::VehicleKind::Nrk)
                .map(|_| ear);
            let drone = nearest(
                &mut self
                    .ents
                    .iter()
                    .filter_map(|e| match e {
                        EntSnap::Vehicle { pos, kind, .. }
                            if *kind != toon_shared::vehicles::VehicleKind::Nrk =>
                        {
                            Some(Vec3::from_array(*pos))
                        }
                        _ => None,
                    })
                    .chain(own),
            );
            a.drone(drone.map(|p| (p, ear)));
        }
    }
}
