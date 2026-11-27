//! What a player's client is told about their own battle royale state.

use super::*;

impl Game {
    pub fn br_snap(&self, id: u8) -> Option<BrSnap> {
        if self.mode != Mode::Br {
            return None;
        }
        let time = self.time;
        let z = &self.br.zone;
        let (c, r) = z.at(time);
        let shrinking = time > z.shrink_at && time < z.shrink_end;
        let timer = if shrinking {
            z.shrink_end - time
        } else {
            z.shrink_at - time
        };
        let me = self.p(id);
        Some(BrSnap {
            weather: self.br.weather,
            alive: self.br_alive() as u16,
            zone: if self.br.running {
                [c.x, c.y, r]
            } else {
                [0.0, 0.0, 0.0]
            },
            next: [z.to_c.x, z.to_c.y, z.to_r],
            timer: if self.br.running {
                timer.clamp(0.0, 9999.0) as f32
            } else {
                0.0
            },
            shrinking,
            dps: z.dps,
            plane: self.br.plane_flying(time).then(|| {
                (
                    self.br.plane_pos(time).to_array(),
                    yaw_of(self.br.plane_dir),
                )
            }),
            in_plane: me.is_some_and(|p| p.in_plane),
            aboard: if me.is_some_and(|p| p.in_plane) {
                self.players.iter().flatten().filter(|p| p.in_plane).count() as u8
            } else {
                0
            },
            inv: me.map(|p| p.inv).unwrap_or_default(),
            empty: me.map_or(0, |p| p.empty),
            squad: me
                .filter(|m| m.squad != 0)
                .map(|m| {
                    self.players
                        .iter()
                        .flatten()
                        .filter(|p| p.squad == m.squad)
                        .map(|p| toon_shared::protocol::SquadMate {
                            id: p.id,
                            name: p.name.clone(),
                            hp: p.hp,
                            alive: p.alive || p.in_plane,
                            downed: p.downed,
                            pos: p.body.pos.to_array(),
                            marker: p.marker,
                        })
                        .collect()
                })
                .unwrap_or_default(),
            drops: self.br.drops.iter().map(|d| d.pos.to_array()).collect(),
            red: self
                .br
                .red
                .map(|z| [z.c.x, z.c.y, z.r, (z.start - self.time).max(0.0) as f32]),
            open_box: self.box_view(id),
            watching: me
                .filter(|m| !m.alive && !m.spec_free)
                .and_then(|m| m.spectating),
            summary: me.filter(|m| m.br_place != 0).map(|m| {
                let rec = self.br_stats.get(&m.stats_key());
                toon_shared::protocol::BrSummary {
                    place: m.br_place,
                    of: self.br.entrants as u16,
                    kills: m.kills,
                    damage: m.br_dmg,
                    longest: m.br_longest,
                    survived: (m.br_out_at - self.br.plane_t0).max(0.0) as f32,
                    matches: rec.matches,
                    wins: rec.wins,
                    total_kills: rec.kills,
                    best: rec.best,
                }
            }),
        })
    }
}
