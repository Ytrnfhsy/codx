//! The snapshot each player gets every tick.

use super::*;

impl Game {
    pub fn snapshot_for(&self, id: u8) -> Snapshot {
        let me = self.p(id);
        let my_team = me.map(|p| p.team).unwrap_or(Team::Spectator);
        let my_squad = me.map_or(0, |p| p.squad);
        let watched = me.filter(|p| !p.alive).and_then(|p| p.spectating);
        let uav = my_team.is_playing() && self.uav_until[my_team.index()] > self.time;
        // big maps: only what is near enough to matter (the fog hides the rest anyway)
        let focus = self.interest(id);
        let near = |q: Vec3, r: f32| {
            focus.as_ref().is_none_or(|pts| {
                pts.iter()
                    .any(|f| vec3(q.x - f.x, 0.0, q.z - f.z).length_squared() < r * r)
            })
        };
        // looking through a magnifying scope: further along the line of sight (the fog
        // thins out there for them too)
        let scope = me
            .filter(|p| {
                p.alive
                    && p.flags & flags::ADS != 0
                    && (p.weapon().id.def().scoped || p.inv.sights[p.slot as usize & 1] >= 2)
            })
            .map(|p| (p.eye(), toon_shared::look_dir(p.yaw, p.pitch)));
        let in_scope = |q: Vec3| {
            scope.is_some_and(|(eye, dir)| {
                let to = q - eye;
                let d = to.length();
                d < Self::SCOPE_RANGE && to.dot(dir) > d * 0.93
            })
        };
        let players = self
            .players
            .iter()
            .flatten()
            .filter(|p| p.team.is_playing())
            .filter(|p| {
                in_scope(p.body.pos)
                    || p.id == id
                    || p.team == my_team && my_team.is_playing() && !self.mode.solo()
                    || my_squad != 0 && p.squad == my_squad
                    || watched == Some(p.id)
                    || near(p.body.pos, INTEREST_RANGE)
            })
            // an enemy behind walls is not sent at all: there is nothing for a cheat to show
            .filter(|p| {
                // (bots get no snapshots: theirs are only ever asked for in tests)
                me.is_none_or(|m| {
                    m.is_bot()
                        || !self.hideable(m, p)
                        || self.time - m.seen_at[p.id as usize] < SEEN_LINGER
                })
            })
            .map(|p| {
                let mut f = p.flags;
                if p.reload_end.is_some() {
                    f |= flags::RELOADING;
                }
                if self.mode == Mode::Sab && self.sab.carrier == Some(p.id) {
                    f |= flags::HAS_BOMB;
                }
                if self.time - p.last_shot < 0.15 {
                    f |= flags::FIRING;
                }
                if (p.team != my_team || self.mode.solo()) && p.id != id && p.alive {
                    let uav_vis = uav;
                    // gunfire shows on the radar (not in the battle royale: nobody has one)
                    let pinged = self.time - p.last_shot < RADAR_PING_TIME as f64
                        && !self.mode.solo();
                    if uav_vis || pinged {
                        f |= flags::ON_RADAR;
                    }
                }
                PlayerSnap {
                    id: p.id,
                    team: p.team,
                    alive: p.alive,
                    pos: p.body.pos.to_array(),
                    yaw: p.yaw,
                    pitch: p.pitch,
                    stance: p.body.stance.to_u8()
                        // only teammates see the shield: it must not give away enemies
                        | if p.alive && p.team == my_team && !self.mode.solo() && p.protected_until > self.time { stance_bits::PROTECTED } else { 0 }
                        | if p.downed { stance_bits::DOWNED } else { 0 }
                        | match p.body.chute {
                            toon_shared::physics::Chute::None => 0,
                            toon_shared::physics::Chute::FreeFall => stance_bits::FREEFALL,
                            toon_shared::physics::Chute::Open => stance_bits::CHUTE,
                        },
                    weapon: p.weapon().id,
                    flags: f,
                    // characters are a battle royale thing: elsewhere sides wear uniforms
                    skin: if self.mode.solo() { p.skin } else { 0 },
                    squad: p.squad,
                }
            })
            .collect();
        let me_snap = match me {
            Some(p) => MeSnap {
                alive: p.alive,
                hp: p.hp,
                slot: p.slot,
                ammo: [
                    (p.weapons[0].mag, p.weapons[0].reserve),
                    (p.weapons[1].mag, p.weapons[1].reserve),
                ],
                weapons: [p.weapons[0].id, p.weapons[1].id],
                grenades: p.grenades,
                smokes: p.smokes,
                streak: p.streak as u16,
                rewards: p.rewards.clone(),
                respawn_in: (p.respawn_at - self.time).max(0.0) as f32,
                uav,
                ack_seq: p.last_seq,
                progress: p.progress,
                protected: (p.protected_until - self.time).max(0.0) as f32,
                vehicle: self.vehicle_hud(p.id),
                downed: p.downed,
                seat: self.seat_of(p.id).map(|(i, k)| (self.cars[i].id, k as u8)),
            },
            None => MeSnap::default(),
        };
        let mut ents = Vec::new();
        for g in &self.grenades {
            ents.push(EntSnap::Grenade {
                id: g.id,
                pos: g.pos.to_array(),
                smoke: g.smoke,
                kind: g.kind,
            });
        }
        for f in &self.fires {
            ents.push(EntSnap::Fire {
                id: f.id,
                pos: f.pos.to_array(),
                radius: f.radius,
            });
        }
        for v in &self.vehicles {
            ents.push(EntSnap::Vehicle {
                id: v.id,
                kind: v.kind,
                team: v.team,
                owner: v.owner,
                pos: v.pos.to_array(),
                yaw: v.yaw,
                pitch: v.pitch,
                auto: v.auto,
            });
        }
        for d in &self.drops {
            ents.push(EntSnap::Drop {
                id: d.id,
                pos: d.pos.to_array(),
            });
        }
        for &(id, pos, born) in &self.smokes {
            ents.push(EntSnap::Smoke {
                id,
                pos: pos.to_array(),
                age: (self.time - born) as f32,
            });
        }
        // (guns, supplies and boxes on the ground go out once: `sync_lying`)
        ents.extend(self.br_drop_snaps());
        ents.extend(self.car_snaps());
        for h in &self.helis {
            ents.push(EntSnap::Heli {
                id: h.id,
                pos: h.pos.to_array(),
                yaw: h.yaw,
                team: h.team,
            });
        }
        let bomb = if self.mode == Mode::Sab && self.state != MatchState::Warmup {
            Some(BombSnap {
                pos: self.sab.pos.to_array(),
                carrier: self.sab.carrier,
                planted_at: self.sab.planted_at,
                timer: if self.sab.planted_at.is_some() {
                    (self.sab.explode_at - self.time).max(0.0) as f32
                } else {
                    0.0
                },
                owner: self.sab.owner,
            })
        } else {
            None
        };
        if focus.is_some() {
            ents.retain(|e| match e {
                // aircraft are seen (and heard) from afar
                EntSnap::Heli { .. } | EntSnap::Plane { .. } => true,
                EntSnap::Grenade { pos, .. }
                | EntSnap::Smoke { pos, .. }
                | EntSnap::Vehicle { pos, .. }
                | EntSnap::Drop { pos, .. }
                | EntSnap::Car { pos, .. }
                | EntSnap::AirDrop { pos, .. }
                | EntSnap::Fire { pos, .. } => near(Vec3::from_array(*pos), INTEREST_RANGE),
                EntSnap::DeathBox { pos, .. }
                | EntSnap::Weapon { pos, .. }
                | EntSnap::Loot { pos, .. } => near(Vec3::from_array(*pos), LOOT_RANGE),
            });
        }
        // gunshots carry further than you can see
        let shots = self
            .shots
            .iter()
            .filter(|s| near(Vec3::from_array(s.from), INTEREST_RANGE + 150.0))
            .cloned()
            .collect();
        Snapshot {
            tick: self.tick,
            state: self.state,
            time_left: self.time_left(),
            scores: self.scores,
            overtime: self.overtime,
            players,
            me: me_snap,
            ents,
            bomb,
            shots,
            br: self.br_snap(id),
            ready: (self.cfg.ready_check && self.state == MatchState::Warmup).then(|| {
                let humans = self
                    .players
                    .iter()
                    .flatten()
                    .filter(|p| !p.is_bot() && p.team.is_playing())
                    .count();
                let me = self.p(id).is_some_and(|p| p.ready);
                let left = self
                    .ready_deadline
                    .map_or(0, |t| (t - self.time).max(0.0).ceil() as u16);
                ((humans - self.unready()) as u8, humans as u8, me, left)
            }),
            pings: self.pings_for(id),
            lobby: self.lobby_for(id),
        }
    }
}
