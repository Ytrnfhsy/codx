//! The minimap and the big map.

use super::*;

impl GameClient {
    pub(super) fn draw_minimap(&self, ui: &Ui) {
        let size = (screen_height() * 0.26).clamp(150.0, 240.0);
        let (x0, y0) = (16.0, 16.0);
        // pixels per metre: big maps show a wider area
        let scale = size / if self.level.terrain { 200.0 } else { 55.0 };
        let c = vec2(x0 + size / 2.0, y0 + size / 2.0);
        soft_rect(
            x0 - 4.0,
            y0 - 4.0,
            size + 8.0,
            size + 8.0,
            Color::new(0.0, 0.0, 0.0, 0.7),
        );
        draw_rectangle(x0, y0, size, size, Color::new(0.16, 0.22, 0.18, 0.85));
        let plane = self
            .br
            .as_ref()
            .filter(|b| b.in_plane)
            .and_then(|b| b.plane);
        let (pos, yaw) = if let Some((p, _)) = plane {
            // riding the plane: where it is now
            (Vec3::from_array(p), self.yaw)
        } else if self.alive || !self.team.is_playing() {
            let (p, y, _) = self.camera();
            (p, y)
        } else {
            (self.death_pos, self.yaw)
        };
        let to_map = |p: Vec3| -> Vec2 {
            let rx = p.x - pos.x;
            let rz = p.z - pos.z;
            let sx = rx * yaw.cos() - rz * yaw.sin();
            let sy = rx * yaw.sin() + rz * yaw.cos();
            c + vec2(sx, sy) * scale
        };
        let gl = crate::mesh::quad_gl();
        let dpi = screen_dpi_scale();
        gl.scissor(Some((
            (x0 * dpi) as i32,
            (y0 * dpi) as i32,
            (size * dpi) as i32,
            (size * dpi) as i32,
        )));
        if let Some(img) = &self.level.image {
            // big maps: the painted map, turned with the view
            let corner = |u: f32, v: f32| {
                let w = img.origin + vec2(u, v) * img.size;
                Vertex::new2(to_map(vec3(w.x, 0.0, w.y)).extend(0.0), vec2(u, v), WHITE)
            };
            let mesh = Mesh {
                vertices: vec![
                    corner(0.0, 0.0),
                    corner(1.0, 0.0),
                    corner(1.0, 1.0),
                    corner(0.0, 1.0),
                ],
                indices: vec![0, 1, 2, 0, 2, 3],
                texture: Some(img.tex.clone()),
            };
            draw_mesh(&mesh);
        }
        if let Some(br) = self.br.as_ref().filter(|b| b.zone[2] > 0.0) {
            let c = to_map(vec3(br.zone[0], 0.0, br.zone[1]));
            draw_circle_lines(
                c.x,
                c.y,
                br.zone[2] * scale,
                3.0,
                Color::new(0.3, 0.55, 1.0, 1.0),
            );
            let n = to_map(vec3(br.next[0], 0.0, br.next[1]));
            draw_circle_lines(n.x, n.y, br.next[2] * scale, 2.0, WHITE);
        }
        // the red zone
        if let Some(rz) = self.br.as_ref().and_then(|b| b.red) {
            let p = to_map(vec3(rz[0], 0.0, rz[1]));
            draw_circle(p.x, p.y, rz[2] * scale, Color::new(0.9, 0.1, 0.1, 0.22));
            draw_circle_lines(
                p.x,
                p.y,
                rz[2] * scale,
                2.0,
                Color::new(0.9, 0.15, 0.1, 0.9),
            );
        }
        // supply crates (on the edge when further away)
        for d in self.br.iter().flat_map(|b| b.drops.iter()) {
            let p = to_map(vec3(d[0], 0.0, d[2]));
            let r = size / 2.0 - 8.0;
            let dd = p - c;
            let p = if dd.length() > r {
                c + dd.normalize() * r
            } else {
                p
            };
            draw_crate_icon(p, 0.7);
        }
        // squadmates' markers
        for m in self
            .br
            .iter()
            .flat_map(|b| b.squad.iter())
            .filter(|m| m.id != self.my_id)
        {
            if let Some(mk) = m.marker {
                let p = to_map(vec3(mk[0], 0.0, mk[1]));
                let r = size / 2.0 - 8.0;
                let d = p - c;
                let p = if d.length() > r {
                    c + d.normalize() * r
                } else {
                    p
                };
                draw_pin(p, 0.6, self.mate_color(m.id));
            }
        }
        // pings: little diamonds
        for pg in &self.pings {
            let p = to_map(Vec3::from_array(pg.pos));
            let r = size / 2.0 - 8.0;
            let d = p - c;
            let p = if d.length() > r {
                c + d.normalize() * r
            } else {
                p
            };
            ping_diamond(p, 6.0, self.ping_color(pg));
        }
        // the marker (kept on the edge when it is further than the minimap shows)
        if let Some(mk) = self.marker {
            let p = to_map(vec3(mk.x, 0.0, mk.y));
            let r = size / 2.0 - 8.0;
            let d = p - c;
            let p = if d.length() > r {
                c + d.normalize() * r
            } else {
                p
            };
            draw_marker_pin(p, 0.7);
        }
        for b in &self.map.boxes {
            if self.level.terrain {
                break;
            }
            if !b.visible || b.min.y < -0.5 || b.max.y < 0.3 && !b.solid {
                if b.visible && !b.solid && b.max.y < 0.3 {
                    // floor paint (roads, pads)
                } else {
                    continue;
                }
            }
            let ht = (b.max.y / 7.0).clamp(0.0, 1.0);
            let col = if b.solid {
                Color::new(0.55 + ht * 0.35, 0.58 + ht * 0.32, 0.6 + ht * 0.3, 0.95)
            } else {
                Color::from_rgba(b.color[0], b.color[1], b.color[2], 90)
            };
            let p = [
                to_map(vec3(b.min.x, 0.0, b.min.z)),
                to_map(vec3(b.max.x, 0.0, b.min.z)),
                to_map(vec3(b.max.x, 0.0, b.max.z)),
                to_map(vec3(b.min.x, 0.0, b.max.z)),
            ];
            draw_triangle(p[0], p[1], p[2], col);
            draw_triangle(p[0], p[2], p[3], col);
        }
        // sites & bomb
        if self.mode == Mode::Sab {
            for t in [Team::Allies, Team::Axis] {
                let p = to_map(self.map.sites[t.index()]);
                draw_circle(
                    p.x,
                    p.y,
                    8.0,
                    if t == self.team { self.tcol(t) } else { BAD },
                );
                let l = if t == Team::Allies { "A" } else { "B" };
                ui.bold(l, p.x - 5.0, p.y + 5.0, 13, WHITE);
            }
            if let Some(b) = &self.bomb {
                let visible = b.carrier.is_none()
                    || b.carrier
                        .map(|c| {
                            self.remotes
                                .get(&c)
                                .map(|r| r.team == self.team)
                                .unwrap_or(c == self.my_id)
                        })
                        .unwrap_or(false);
                if visible {
                    let p = to_map(Vec3::from_array(b.pos));
                    draw_circle(p.x, p.y, 6.0, Color::new(0.1, 0.1, 0.1, 1.0));
                    draw_circle(p.x, p.y, 4.0, ACCENT);
                }
            }
        }
        // players
        let edge = size / 2.0 - 8.0;
        for (&rid, r) in &self.remotes {
            if !r.alive {
                continue;
            }
            let p = to_map(r.pos);
            // ours beyond the minimap: a dot on its edge, in their direction
            let off = p - c;
            if self.is_friend(r) && off.length() > edge {
                let p = c + off.normalize() * edge;
                let col = match (r.downed, self.br.is_some()) {
                    (true, _) => BAD,
                    (false, true) => self.mate_color(rid),
                    (false, false) => self.tcol(self.team),
                };
                draw_circle(p.x, p.y, 6.0, BLACK);
                draw_circle(p.x, p.y, 4.5, col);
                continue;
            }
            // battle royale: only the squad is on our side
            if self.is_friend(r) && r.downed {
                draw_circle(p.x, p.y, 6.0, BLACK);
                draw_circle(p.x, p.y, 5.0, BAD);
                draw_rectangle(p.x - 1.0, p.y - 3.5, 2.0, 7.0, WHITE);
                draw_rectangle(p.x - 3.5, p.y - 1.0, 7.0, 2.0, WHITE);
            } else if self.is_friend(r) {
                let a = r.yaw - yaw;
                let d = vec2(-a.sin(), -a.cos());
                let perp = vec2(-d.y, d.x);
                draw_triangle(
                    p + d * 7.0,
                    p - d * 4.0 + perp * 5.0,
                    p - d * 4.0 - perp * 5.0,
                    self.tcol(self.team),
                );
            } else if r.flags & flags::ON_RADAR != 0 {
                let pulse = 5.0 + ((self.now * 6.0).sin() as f32) * 1.0;
                draw_circle(p.x, p.y, pulse + 1.5, BLACK);
                draw_circle(p.x, p.y, pulse, BAD);
            }
        }
        // squadmates too far to be sent as players: where the squad list says they are,
        // on the edge when beyond the minimap
        for m in self
            .br
            .iter()
            .flat_map(|b| b.squad.iter())
            .filter(|m| m.id != self.my_id && m.alive && !self.remotes.contains_key(&m.id))
        {
            let p = to_map(Vec3::from_array(m.pos));
            let r = size / 2.0 - 8.0;
            let d = p - c;
            let p = if d.length() > r {
                c + d.normalize() * r
            } else {
                p
            };
            draw_circle(p.x, p.y, 6.0, BLACK);
            draw_circle(
                p.x,
                p.y,
                4.5,
                if m.downed { BAD } else { self.mate_color(m.id) },
            );
        }
        // artillery about to land: a pulsing red ring
        for (t, r, at) in &self.artillery {
            let p = to_map(*t);
            let pulse = 0.5 + 0.5 * ((self.now - at) * 6.0).sin() as f32;
            draw_circle_lines(p.x, p.y, r * scale, 2.0 + pulse * 2.0, BAD);
        }
        // jeeps and boats
        for c in self.cars.values() {
            let p = to_map(c.state.pos);
            let col = if c.seats.contains(&self.my_id) {
                ACCENT
            } else {
                Color::new(0.85, 0.85, 0.8, 0.9)
            };
            draw_rectangle(p.x - 4.0, p.y - 3.0, 8.0, 6.0, BLACK);
            draw_rectangle(p.x - 3.0, p.y - 2.0, 6.0, 4.0, col);
        }
        // vehicles: ours always, enemy ones like enemies (on radar)
        for e in &self.ents {
            if let EntSnap::Vehicle { pos, team, .. } = e {
                let friendly = *team == self.team;
                if !friendly && !self.me.uav {
                    continue;
                }
                let p = to_map(Vec3::from_array(*pos));
                let c = if friendly { self.tcol(*team) } else { BAD };
                draw_rectangle(p.x - 4.0, p.y - 4.0, 8.0, 8.0, BLACK);
                draw_rectangle(p.x - 3.0, p.y - 3.0, 6.0, 6.0, c);
            }
        }
        for e in &self.ents {
            if let EntSnap::Smoke { pos, age, .. } = e {
                let p = to_map(Vec3::from_array(*pos));
                let r = smoke_radius(*age) * scale;
                draw_circle(p.x, p.y, r, Color::new(0.85, 0.86, 0.88, 0.75));
            }
        }
        for e in &self.ents {
            if let EntSnap::Heli { pos, team, .. } = e {
                let p = to_map(Vec3::from_array(*pos));
                draw_rectangle(
                    p.x - 6.0,
                    p.y - 6.0,
                    12.0,
                    12.0,
                    if *team == self.team {
                        self.tcol(*team)
                    } else {
                        BAD
                    },
                );
            }
        }
        gl.scissor(None);
        // me
        draw_triangle(
            c + vec2(0.0, -9.0),
            c + vec2(6.0, 6.0),
            c + vec2(-6.0, 6.0),
            ACCENT,
        );
        // compass letters around the edge
        for (label, a) in [
            (t("Пн", "N"), 0.0f32),
            (t("Сх", "E"), -std::f32::consts::FRAC_PI_2),
            (t("Пд", "S"), std::f32::consts::PI),
            (t("Зх", "W"), std::f32::consts::FRAC_PI_2),
        ] {
            let rel = a - yaw;
            let d = vec2(-rel.sin(), -rel.cos());
            let p = c + d * (size / 2.0 - 10.0);
            let tw = ui.width(label, 12, true);
            ui.shadowed(label, p.x - tw / 2.0, p.y + 5.0, 12, TEXT);
        }
        if self.me.uav {
            ui.shadowed(t("БПЛА", "UAV"), x0 + 6.0, y0 + size - 8.0, 14, GOOD);
        }
    }

    /// Battle royale: the whole island, north up, with the zone and the plane's course.
    pub fn draw_big_map(&self, ui: &Ui) {
        let (Some(br), Some(img)) = (&self.br, &self.level.image) else {
            return;
        };
        let (w, h) = (screen_width(), screen_height());
        let (x0, y0, side) = big_map_rect();
        soft_rect(0.0, 0.0, w, h, Color::new(0.0, 0.0, 0.0, 0.5));
        draw_texture_ex(
            &img.tex,
            x0,
            y0,
            WHITE,
            DrawTextureParams {
                dest_size: Some(vec2(side, side)),
                ..Default::default()
            },
        );
        let k = side / img.size;
        // circles and the plane's course stay inside the map
        let gl = crate::mesh::quad_gl();
        let dpi = screen_dpi_scale();
        gl.scissor(Some((
            (x0 * dpi) as i32,
            (y0 * dpi) as i32,
            (side * dpi) as i32,
            (side * dpi) as i32,
        )));
        let to_map =
            |x: f32, z: f32| vec2(x0 + (x - img.origin.x) * k, y0 + (z - img.origin.y) * k);
        if br.zone[2] > 0.0 {
            let c = to_map(br.zone[0], br.zone[1]);
            draw_circle_lines(
                c.x,
                c.y,
                br.zone[2] * k,
                3.0,
                Color::new(0.3, 0.55, 1.0, 1.0),
            );
            let n = to_map(br.next[0], br.next[1]);
            draw_circle_lines(n.x, n.y, br.next[2] * k, 2.0, WHITE);
        }
        if let Some((p, yaw)) = br.plane {
            let d = look_dir(yaw, 0.0);
            let a = to_map(p[0] - d.x * 1500.0, p[2] - d.z * 1500.0);
            let b = to_map(p[0] + d.x * 1500.0, p[2] + d.z * 1500.0);
            draw_line(a.x, a.y, b.x, b.y, 2.0, Color::new(1.0, 1.0, 1.0, 0.5));
            let pp = to_map(p[0], p[2]);
            draw_circle(pp.x, pp.y, 6.0, ACCENT);
        }
        // me
        let pos = if br.in_plane {
            br.plane
                .map(|p| Vec3::from_array(p.0))
                .unwrap_or(self.body.pos)
        } else {
            self.body.pos
        };
        let m = to_map(pos.x, pos.z);
        let d = look_dir(self.yaw, 0.0);
        let f = vec2(d.x, d.z);
        let perp = vec2(-f.y, f.x);
        draw_triangle(
            m + f * 12.0,
            m - f * 7.0 + perp * 7.0,
            m - f * 7.0 - perp * 7.0,
            ACCENT,
        );
        draw_triangle_lines(
            m + f * 12.0,
            m - f * 7.0 + perp * 7.0,
            m - f * 7.0 - perp * 7.0,
            2.0,
            BLACK,
        );
        if let Some(rz) = br.red {
            let c = to_map(rz[0], rz[1]);
            draw_circle(c.x, c.y, rz[2] * k, Color::new(0.9, 0.1, 0.1, 0.25));
            draw_circle_lines(c.x, c.y, rz[2] * k, 2.0, Color::new(0.9, 0.15, 0.1, 0.9));
        }
        for d in &br.drops {
            let p = to_map(d[0], d[2]);
            draw_crate_icon(p, 1.0);
        }
        if let Some(mk) = self.marker {
            let p = to_map(mk.x, mk.y);
            draw_marker_pin(p, 1.0);
        }
        for pg in &self.pings {
            ping_diamond(to_map(pg.pos[0], pg.pos[2]), 7.0, self.ping_color(pg));
        }
        for m in br.squad.iter().filter(|m| m.id != self.my_id) {
            if let Some(mk) = m.marker {
                draw_pin(to_map(mk[0], mk[1]), 0.9, self.mate_color(m.id));
            }
            if m.alive {
                let p = to_map(m.pos[0], m.pos[2]);
                draw_circle(p.x, p.y, 6.0, BLACK);
                draw_circle(
                    p.x,
                    p.y,
                    4.5,
                    if m.downed { BAD } else { self.mate_color(m.id) },
                );
            }
        }
        let gl = crate::mesh::quad_gl();
        gl.scissor(None);
        draw_rectangle_lines(x0, y0, side, side, 3.0, BLACK);
        ui.centered(
            &tr!(
                "{} — закрити карту · ЛКМ — поставити мітку, ПКМ — прибрати",
                "{} — close the map · LMB — put a marker, RMB — remove it",
                key(Action::Class)
            ),
            w / 2.0,
            y0 + side + 24.0,
            16,
            TEXT,
        );
    }
}

/// Where the big map is on the screen: (x, y, side).
pub fn big_map_rect() -> (f32, f32, f32) {
    let (w, h) = (screen_width(), screen_height());
    let side = (h * 0.86).min(w * 0.9);
    ((w - side) / 2.0, (h - side) / 2.0, side)
}
