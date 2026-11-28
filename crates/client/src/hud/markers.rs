//! Things marked in the world: the map marker, pings, squadmates, Sabotage's sites and bomb.

use super::*;

impl GameClient {
    /// The map marker over the world, with how far it is (on the screen's edge when it
    /// is behind or to the side).
    pub(super) fn draw_world_marker(&self, ui: &Ui, view: &View) {
        let Some(mk) = self.marker else { return };
        let (w, h) = (screen_width(), screen_height());
        let ground = self.world.ground(mk.x, mk.y).unwrap_or(0.0);
        let at = vec3(mk.x, ground + 2.0, mk.y);
        let (eye, yaw, _) = self.camera();
        let dist = vec2(mk.x - eye.x, mk.y - eye.z).length();
        let label = if dist >= 1000.0 {
            format!("{:.1} {}", dist / 1000.0, t("км", "km"))
        } else {
            format!("{:.0} {}", dist, t("м", "m"))
        };
        let on_screen = view
            .project(at)
            .filter(|p| p.x > 20.0 && p.x < w - 20.0 && p.y > 30.0 && p.y < h - 20.0);
        let p = match on_screen {
            Some(p) => p,
            None => {
                // off screen: along the top edge, left or right by where it lies
                let to = vec3(mk.x - eye.x, 0.0, mk.y - eye.z);
                let side = wrap_angle(yaw_of(to) - yaw);
                let x = w / 2.0 - (side / std::f32::consts::PI) * (w / 2.0 - 30.0);
                vec2(x.clamp(30.0, w - 30.0), 70.0)
            }
        };
        draw_marker_pin(p, 0.9);
        let tw = ui.width(&label, 15, true);
        ui.shadowed(
            &label,
            p.x - tw / 2.0,
            p.y + 16.0,
            15,
            Color::new(1.0, 0.85, 0.15, 1.0),
        );
    }

    /// Pings over the world, seen through walls: a diamond with what it points at, its
    /// name and how far it is (on the screen's edge when it is behind or to the side).
    pub(super) fn draw_pings(&self, ui: &Ui, view: &View) {
        use toon_shared::protocol::PingWhat;
        let (w, h) = (screen_width(), screen_height());
        let (eye, yaw, _) = self.camera();
        let (cx, cy) = (w / 2.0, h / 2.0);
        for pg in &self.pings {
            let lift = crate::game::ping_lift(pg.what);
            let at = Vec3::from_array(pg.pos) + vec3(0.0, lift, 0.0);
            let c = self.ping_color(pg);
            let on_screen = view
                .project(at)
                .filter(|p| p.x > 24.0 && p.x < w - 24.0 && p.y > 40.0 && p.y < h - 24.0);
            let p = match on_screen {
                Some(p) => p,
                None => {
                    let to = vec3(at.x - eye.x, 0.0, at.z - eye.z);
                    let side = wrap_angle(yaw_of(to) - yaw);
                    let x = w / 2.0 - (side / std::f32::consts::PI) * (w / 2.0 - 30.0);
                    // (below the banners at the top)
                    vec2(x.clamp(30.0, w - 30.0), h * 0.36)
                }
            };
            // pops in, and fades out at the very end for enemies
            let pop = (pg.age / 0.18).clamp(0.0, 1.0);
            let k = 1.0 + (1.0 - pop) * 0.6;
            // a stem down to the very place (places only, where it is on screen)
            if on_screen.is_some() && matches!(pg.what, PingWhat::Spot | PingWhat::Danger) {
                draw_line(p.x, p.y, p.x, p.y - 22.0, 2.0, c);
                draw_circle_lines(p.x, p.y, 4.0, 1.5, c);
            }
            let top = if matches!(pg.what, PingWhat::Spot | PingWhat::Danger) && on_screen.is_some()
            {
                vec2(p.x, p.y - 36.0)
            } else {
                p
            };
            if ping_is_thing(pg.what) {
                // a thing: its own picture, as in the inventory, only smaller
                ping_thing(pg.what, top, 15.0 * k, c);
            } else {
                ping_diamond(top, 15.0 * k, c);
                ping_glyph(pg.what, top, 15.0 * k, c);
            }
            // the name close to the crosshair (or for a moment after it went up), the
            // distance always
            let dist = at.distance(eye);
            let d = if dist >= 1000.0 {
                format!("{:.1} {}", dist / 1000.0, t("км", "km"))
            } else {
                format!("{:.0} {}", dist, t("м", "m"))
            };
            let near_aim = vec2(top.x - cx, top.y - cy).length() < 90.0;
            let label = if near_aim || pg.age < 3.0 {
                format!("{} · {d}", crate::game::ping_label(pg.what))
            } else {
                d
            };
            let tw = ui.width(&label, 14, true);
            ui.shadowed(&label, top.x - tw / 2.0, top.y + 30.0, 14, c);
            // «I need that»: who, in their colour, with a ring round the badge
            if let Some(cl) = pg.claimed {
                let cc = if self.br.is_some() {
                    self.mate_color(cl)
                } else {
                    GOOD
                };
                draw_circle_lines(top.x, top.y, 15.0 * k + 6.0, 2.5, cc);
                let who = if cl == self.my_id {
                    t("Ти береш", "You take it").to_string()
                } else {
                    tr!("{} бере", "{} takes it", self.name_of(cl).unwrap_or("?"))
                };
                let ww = ui.width(&who, 13, true);
                ui.shadowed(&who, top.x - ww / 2.0, top.y + 46.0, 13, cc);
            }
        }
        // on a squadmate's ping of a thing: how to say «I need that»
        if self.alive && self.overlay == Overlay::None {
            if let Some(id) = self.ping_aimed(false) {
                let claimed = self
                    .pings
                    .iter()
                    .find(|p| p.id == id)
                    .is_some_and(|p| p.claimed == Some(self.my_id));
                let text = if claimed {
                    tr!(
                        "{} — уже не потрібно",
                        "{} — no longer needed",
                        key(Action::Ping)
                    )
                } else {
                    tr!("{} — мені потрібно", "{} — I need that", key(Action::Ping))
                };
                ui.centered(&text, cx, cy + 46.0, 15, TEXT);
            }
        }
    }

    /// Squadmates' markers and the ones who are down, over the world; knocked down
    /// ourselves: what is left of us; next to a squadmate who is down: how to pick them up.
    pub(super) fn draw_squad_world(&self, ui: &Ui, view: &View) {
        let Some(br) = &self.br else { return };
        let (w, h) = (screen_width(), screen_height());
        let (eye, _, _) = self.camera();
        for m in br.squad.iter().filter(|m| m.id != self.my_id) {
            let c = self.mate_color(m.id);
            if let Some(mk) = m.marker {
                let ground = self.world.ground(mk[0], mk[1]).unwrap_or(0.0);
                if let Some(p) = view.project(vec3(mk[0], ground + 2.0, mk[1])) {
                    if p.x > 0.0 && p.x < w && p.y > 0.0 && p.y < h {
                        draw_pin(p, 0.8, c);
                        let d = vec2(mk[0] - eye.x, mk[1] - eye.z).length();
                        let label = format!("{:.0} {}", d, t("м", "m"));
                        let tw = ui.width(&label, 13, true);
                        ui.shadowed(&label, p.x - tw / 2.0, p.y + 6.0, 13, c);
                    }
                }
            }
            if m.alive && m.downed {
                let pos = Vec3::from_array(m.pos) + vec3(0.0, 1.0, 0.0);
                if let Some(p) = view.project(pos) {
                    if p.x > 0.0 && p.x < w && p.y > 0.0 && p.y < h {
                        draw_circle(p.x, p.y, 11.0, BAD);
                        draw_rectangle(p.x - 2.0, p.y - 7.0, 4.0, 14.0, WHITE);
                        draw_rectangle(p.x - 7.0, p.y - 2.0, 14.0, 4.0, WHITE);
                        let d = pos.distance(eye);
                        let label = format!("{} · {:.0} {}", m.name, d, t("м", "m"));
                        let tw = ui.width(&label, 13, true);
                        ui.shadowed(&label, p.x - tw / 2.0, p.y + 26.0, 13, BAD);
                    }
                }
            }
        }
        // knocked down ourselves
        if self.downed() {
            draw_rectangle(0.0, 0.0, w, h, Color::new(0.5, 0.0, 0.0, 0.18));
            ui.centered(
                t("ТЕБЕ ПОРАНЕНО", "YOU ARE DOWN"),
                w / 2.0,
                h * 0.62,
                30,
                BAD,
            );
            ui.centered(
                t(
                    "Повзи в укриття, щит тримай до ворога — напарник може підняти (тримаючи дію)",
                    "Crawl to cover, shield towards the enemy — a squadmate can pick you up (holding use)",
                ),
                w / 2.0,
                h * 0.62 + 26.0,
                16,
                TEXT,
            );
            let bw = 260.0;
            let (bx, by) = ((w - bw) / 2.0, h * 0.62 + 40.0);
            soft_rect(bx, by, bw, 8.0, Color::new(0.0, 0.0, 0.0, 0.6));
            draw_rectangle(bx, by, bw * (self.me.hp / 100.0).clamp(0.0, 1.0), 8.0, BAD);
            if self.me.progress > 0.0 {
                draw_rectangle(
                    bx,
                    by + 12.0,
                    bw * self.me.progress.clamp(0.0, 1.0),
                    8.0,
                    GOOD,
                );
            }
            return;
        }
        // a squadmate down right next to us
        if self.alive {
            if let Some(m) = br.squad.iter().find(|m| {
                m.id != self.my_id
                    && m.alive
                    && m.downed
                    && Vec3::from_array(m.pos).distance(self.body.pos) < 2.0
            }) {
                ui.centered(
                    &tr!(
                        "Тримай {} — підняти {}",
                        "Hold {} — pick up {}",
                        key(Action::Use),
                        m.name
                    ),
                    w / 2.0,
                    h * 0.62,
                    20,
                    GOOD,
                );
                if self.me.progress > 0.0 {
                    let bw = 220.0;
                    let (bx, by) = ((w - bw) / 2.0, h * 0.62 + 12.0);
                    soft_rect(bx, by, bw, 8.0, Color::new(0.0, 0.0, 0.0, 0.6));
                    draw_rectangle(bx, by, bw * self.me.progress.clamp(0.0, 1.0), 8.0, GOOD);
                }
            }
        }
    }

    pub(super) fn draw_sab_markers(&self, ui: &Ui, view: &View) {
        let Some(b) = &self.bomb else { return };
        if self.state == MatchState::Warmup {
            return;
        }
        let me = self.team;
        for t in [Team::Allies, Team::Axis] {
            let site = self.map.sites[t.index()];
            let Some(sp) = view.project(site + vec3(0.0, 2.2, 0.0)) else {
                continue;
            };
            let ours = t == me;
            let label = if ours {
                toon_shared::lang::t("ЗАХИСТИТИ", "DEFEND")
            } else {
                toon_shared::lang::t("ПІДІРВАТИ", "DESTROY")
            };
            let c = if ours { self.tcol(me) } else { BAD };
            let planted_here = b.planted_at == Some(t);
            let r = if planted_here && (self.now * 4.0).fract() < 0.5 {
                20.0
            } else {
                16.0
            };
            draw_circle(sp.x, sp.y, r + 3.0, Color::new(0.0, 0.0, 0.0, 0.6));
            draw_circle(sp.x, sp.y, r, c);
            let letter = if t == Team::Allies { "A" } else { "B" };
            let lw = ui.width(letter, 18, true);
            ui.bold(letter, sp.x - lw / 2.0, sp.y + 7.0, 18, WHITE);
            let d = site.distance(self.body.pos);
            let txt = tr!("{label} {d:.0}м", "{label} {d:.0}m");
            let tw = ui.width(&txt, 13, true);
            ui.shadowed(&txt, sp.x - tw / 2.0, sp.y + r + 16.0, 13, c);
        }
        // a teammate carrying the bomb: an icon over their head, seen through walls, so
        // everyone knows whom to cover
        if let (Some(c), None) = (b.carrier, b.planted_at) {
            let ours = self.remotes.get(&c).filter(|r| r.alive && r.team == me);
            if let Some(r) = ours {
                if let Some(sp) = view.project(r.pos + vec3(0.0, 3.0, 0.0)) {
                    let pulse = 0.5 + 0.5 * (self.now * 4.0).sin() as f32;
                    draw_bomb_icon(sp.x, sp.y, 1.2, pulse);
                    let d = r.pos.distance(self.body.pos);
                    ui.centered(
                        &tr!("ПРИКРИЙ {d:.0}м", "COVER {d:.0}m"),
                        sp.x,
                        sp.y - 24.0,
                        13,
                        self.tcol(me),
                    );
                }
            }
        }
        // the bomb itself when lying around
        if b.carrier.is_none() && b.planted_at.is_none() {
            if let Some(sp) = view.project(Vec3::from_array(b.pos) + vec3(0.0, 1.2, 0.0)) {
                draw_circle(sp.x, sp.y - 8.0, 11.0, Color::new(0.0, 0.0, 0.0, 0.7));
                draw_circle(sp.x, sp.y - 8.0, 8.0, BAD);
                let d = Vec3::from_array(b.pos).distance(self.body.pos);
                ui.centered(
                    &tr!("бомба {d:.0}м", "bomb {d:.0}m"),
                    sp.x,
                    sp.y + 18.0,
                    13,
                    ACCENT,
                );
            }
        }
        // planted timer
        if b.planted_at.is_some() {
            ui.centered(
                &tr!("БОМБА {:.1}", "BOMB {:.1}", b.timer),
                screen_width() / 2.0,
                70.0,
                34,
                BAD,
            );
        }
        // plant / defuse hints & progress
        let (w, h) = (screen_width(), screen_height());
        if self.me.progress > 0.0 {
            let bw = 300.0;
            soft_rect(
                w / 2.0 - bw / 2.0 - 3.0,
                h * 0.7 - 3.0,
                bw + 6.0,
                22.0,
                Color::new(0.0, 0.0, 0.0, 0.7),
            );
            draw_rectangle(
                w / 2.0 - bw / 2.0,
                h * 0.7,
                bw * self.me.progress,
                16.0,
                ACCENT,
            );
            let label = if b.planted_at.is_some() {
                t("Знешкодження…", "Defusing…")
            } else {
                t("Закладання…", "Planting…")
            };
            ui.centered(label, w / 2.0, h * 0.7 - 10.0, 18, TEXT);
        } else if self.alive {
            let carrying = b.carrier == Some(self.my_id);
            let enemy_site = self.map.sites[me.enemy().index()];
            if carrying && self.body.pos.distance(enemy_site) < SAB_SITE_RADIUS {
                ui.centered(
                    &tr!(
                        "Тримай {}, щоб закласти бомбу",
                        "Hold {} to plant the bomb",
                        key(Action::Use)
                    ),
                    w / 2.0,
                    h * 0.7,
                    22,
                    ACCENT,
                );
            } else if b.planted_at == Some(me)
                && self.body.pos.distance(Vec3::from_array(b.pos)) < SAB_DEFUSE_RADIUS
            {
                ui.centered(
                    &tr!(
                        "Тримай {}, щоб знешкодити бомбу",
                        "Hold {} to defuse the bomb",
                        key(Action::Use)
                    ),
                    w / 2.0,
                    h * 0.7,
                    22,
                    ACCENT,
                );
            } else if carrying {
                ui.centered(
                    t(
                        "У тебе бомба — неси її на ворожу точку!",
                        "You have the bomb — take it to the enemy site!",
                    ),
                    w / 2.0,
                    h - 150.0,
                    18,
                    ACCENT,
                );
            }
        }
    }
}

/// The map marker: a yellow pin whose point is at `p`.
pub(super) fn draw_marker_pin(p: Vec2, k: f32) {
    draw_pin(p, k, Color::new(1.0, 0.85, 0.15, 1.0));
}

/// A map marker's pin in a colour (a squadmate's).
pub(super) fn draw_pin(p: Vec2, k: f32, c: Color) {
    let top = p - vec2(0.0, 20.0 * k);
    draw_triangle(p, top + vec2(-7.0 * k, 0.0), top + vec2(7.0 * k, 0.0), c);
    draw_circle(top.x, top.y, 8.0 * k, c);
    draw_circle_lines(top.x, top.y, 8.0 * k, 2.0, BLACK);
    draw_circle(top.x, top.y, 3.0 * k, BLACK);
}

/// A supply crate on a map: a blue box with an orange band.
pub(super) fn draw_crate_icon(p: Vec2, k: f32) {
    let s = 7.0 * k;
    draw_rectangle(
        p.x - s - 1.5,
        p.y - s - 1.5,
        2.0 * s + 3.0,
        2.0 * s + 3.0,
        BLACK,
    );
    draw_rectangle(
        p.x - s,
        p.y - s,
        2.0 * s,
        2.0 * s,
        Color::new(0.24, 0.43, 0.67, 1.0),
    );
    draw_rectangle(
        p.x - s,
        p.y - 1.5 * k,
        2.0 * s,
        3.0 * k,
        Color::new(0.94, 0.59, 0.16, 1.0),
    );
}

/// Real players' names in the tables (the bots stay white).
pub(super) const HUMAN: Color = Color::new(0.45, 0.9, 1.0, 1.0);

/// A ping's badge: a dark diamond with a coloured rim, `r` from the centre to a corner.
pub(super) fn ping_diamond(p: Vec2, r: f32, c: Color) {
    let (n, e, s, w) = (
        p + vec2(0.0, -r),
        p + vec2(r, 0.0),
        p + vec2(0.0, r),
        p + vec2(-r, 0.0),
    );
    let o = r + 2.5;
    let dark = Color::new(0.06, 0.07, 0.1, 0.85);
    draw_triangle(p + vec2(0.0, -o), p + vec2(o, 0.0), p + vec2(0.0, o), BLACK);
    draw_triangle(
        p + vec2(0.0, -o),
        p + vec2(-o, 0.0),
        p + vec2(0.0, o),
        BLACK,
    );
    draw_triangle(n, e, s, c);
    draw_triangle(n, w, s, c);
    let i = r * 0.72;
    draw_triangle(p + vec2(0.0, -i), p + vec2(i, 0.0), p + vec2(0.0, i), dark);
    draw_triangle(p + vec2(0.0, -i), p + vec2(-i, 0.0), p + vec2(0.0, i), dark);
}

/// A ping on a thing (not a place or an enemy).
pub(super) fn ping_is_thing(what: toon_shared::protocol::PingWhat) -> bool {
    use toon_shared::protocol::PingWhat;
    !matches!(what, PingWhat::Spot | PingWhat::Danger | PingWhat::Enemy)
}

/// A ping on a thing: a dark round tile with a rim in the pinger's colour and the thing's
/// own picture in its own colours (a medkit red, a vest blue...), `r` the tile's radius.
pub(super) fn ping_thing(what: toon_shared::protocol::PingWhat, p: Vec2, r: f32, ring: Color) {
    use toon_shared::protocol::{LootKind, PingWhat};
    draw_circle(p.x, p.y, r + 3.0, BLACK);
    draw_circle(p.x, p.y, r + 1.5, ring);
    draw_circle(p.x, p.y, r - 1.0, Color::new(0.07, 0.08, 0.11, 0.92));
    let sz = r * 1.35;
    let red = Color::new(1.0, 0.4, 0.4, 1.0);
    let blue = Color::new(0.45, 0.65, 1.0, 1.0);
    let sight = Color::new(1.0, 0.45, 0.35, 1.0);
    match what {
        PingWhat::Loot(k, _) => match k {
            LootKind::Medkit => draw_icon(Icon::Medkit, p.x, p.y, sz, red),
            LootKind::Bandages => draw_icon(Icon::Medkit, p.x, p.y, sz * 0.8, WHITE),
            LootKind::Drink => {
                // a can
                round_rect(p.x - r * 0.3, p.y - r * 0.5, r * 0.6, r, r * 0.12, ACCENT);
                draw_rectangle(p.x - r * 0.3, p.y - r * 0.1, r * 0.6, r * 0.2, BLACK);
            }
            LootKind::Armor(_) => draw_icon(Icon::Vest, p.x, p.y, sz, blue),
            LootKind::Helmet(_) => draw_icon(Icon::Helmet, p.x, p.y, sz, blue),
            LootKind::Backpack(_) => draw_icon(Icon::Backpack, p.x, p.y, sz, ACCENT),
            LootKind::Sight(1) => draw_icon(Icon::RedDot, p.x, p.y, sz, sight),
            LootKind::Sight(_) => draw_icon(Icon::Scope, p.x, p.y, sz, sight),
            LootKind::Ammo(_) => {
                let brass = Color::new(0.95, 0.75, 0.3, 1.0);
                for i in -1..=1 {
                    let x = p.x + i as f32 * r * 0.36;
                    draw_rectangle(x - r * 0.12, p.y - r * 0.2, r * 0.24, r * 0.65, brass);
                    draw_circle(x, p.y - r * 0.22, r * 0.12, Color::new(0.8, 0.5, 0.25, 1.0));
                }
            }
            LootKind::Mod(_) => {
                draw_rectangle(
                    p.x - r * 0.55,
                    p.y - r * 0.15,
                    r * 1.1,
                    r * 0.3,
                    Color::new(0.7, 0.7, 0.75, 1.0),
                );
                draw_rectangle(
                    p.x - r * 0.15,
                    p.y - r * 0.45,
                    r * 0.3,
                    r * 0.9,
                    Color::new(0.55, 0.55, 0.6, 1.0),
                );
            }
            LootKind::Molotov => {
                draw_circle(
                    p.x,
                    p.y + r * 0.15,
                    r * 0.4,
                    Color::new(0.4, 0.7, 0.35, 1.0),
                );
                draw_rectangle(
                    p.x - r * 0.1,
                    p.y - r * 0.6,
                    r * 0.2,
                    r * 0.4,
                    Color::new(1.0, 0.55, 0.15, 1.0),
                );
            }
            _ => {
                // grenades and flashbangs
                let c = if k == LootKind::Flash {
                    WHITE
                } else {
                    Color::new(0.4, 0.6, 0.3, 1.0)
                };
                draw_circle(p.x, p.y + r * 0.1, r * 0.42, c);
                draw_rectangle(
                    p.x - r * 0.1,
                    p.y - r * 0.55,
                    r * 0.2,
                    r * 0.25,
                    Color::new(0.6, 0.6, 0.6, 1.0),
                );
            }
        },
        PingWhat::Weapon(w) => {
            // as big as fits the tile
            let (_, w0) = gun_rects(w, r * 0.7);
            let gh = r * 0.7 * (r * 1.6 / w0).min(1.0);
            let (_, gw) = gun_rects(w, gh);
            draw_cause_icon(Cause::Weapon(w), p.x - gw / 2.0, p.y, gh, 1.0);
        }
        PingWhat::DeathBox | PingWhat::AirDrop => draw_crate_icon(p, r / 11.0),
        PingWhat::Car(_) => {
            let c = Color::new(0.55, 0.65, 0.4, 1.0);
            draw_rectangle(p.x - r * 0.6, p.y - r * 0.1, r * 1.2, r * 0.35, c);
            draw_rectangle(p.x - r * 0.3, p.y - r * 0.4, r * 0.6, r * 0.32, c);
            draw_circle(p.x - r * 0.32, p.y + r * 0.3, r * 0.16, BLACK);
            draw_circle(p.x + r * 0.32, p.y + r * 0.3, r * 0.16, BLACK);
        }
        _ => {}
    }
}

/// What is inside a ping's badge.
pub(super) fn ping_glyph(what: toon_shared::protocol::PingWhat, p: Vec2, r: f32, c: Color) {
    use toon_shared::protocol::{LootKind, PingWhat};
    let sz = r * 1.05;
    let bang = |col: Color| {
        draw_rectangle(p.x - r * 0.1, p.y - r * 0.45, r * 0.2, r * 0.55, col);
        draw_circle(p.x, p.y + r * 0.3, r * 0.11, col);
    };
    match what {
        PingWhat::Spot => draw_circle(p.x, p.y, r * 0.22, c),
        PingWhat::Danger | PingWhat::Enemy => bang(c),
        PingWhat::Loot(k, _) => match k {
            LootKind::Medkit | LootKind::Bandages | LootKind::Drink => {
                draw_icon(Icon::Medkit, p.x, p.y, sz, c)
            }
            LootKind::Armor(_) => draw_icon(Icon::Vest, p.x, p.y, sz, c),
            LootKind::Helmet(_) => draw_icon(Icon::Helmet, p.x, p.y, sz, c),
            LootKind::Backpack(_) => draw_icon(Icon::Backpack, p.x, p.y, sz, c),
            LootKind::Sight(1) => draw_icon(Icon::RedDot, p.x, p.y, sz, c),
            LootKind::Sight(_) => draw_icon(Icon::Scope, p.x, p.y, sz, c),
            LootKind::Ammo(_) => {
                // three rounds side by side
                for i in -1..=1 {
                    let x = p.x + i as f32 * r * 0.28;
                    draw_rectangle(x - r * 0.09, p.y - r * 0.2, r * 0.18, r * 0.5, c);
                    draw_circle(x, p.y - r * 0.22, r * 0.09, c);
                }
            }
            _ => {
                // a grenade-like round thing
                draw_circle(p.x, p.y + r * 0.05, r * 0.3, c);
                draw_rectangle(p.x - r * 0.08, p.y - r * 0.42, r * 0.16, r * 0.18, c);
            }
        },
        PingWhat::Weapon(w) => {
            let gh = r * 0.55;
            let (_, gw) = gun_rects(w, gh);
            draw_cause_icon(Cause::Weapon(w), p.x - gw / 2.0, p.y, gh, 1.0);
        }
        PingWhat::DeathBox | PingWhat::AirDrop => draw_crate_icon(p, r / 14.0),
        PingWhat::Car(_) => {
            draw_rectangle(p.x - r * 0.45, p.y - r * 0.1, r * 0.9, r * 0.3, c);
            draw_rectangle(p.x - r * 0.25, p.y - r * 0.32, r * 0.5, r * 0.24, c);
            draw_circle(p.x - r * 0.25, p.y + r * 0.25, r * 0.12, c);
            draw_circle(p.x + r * 0.25, p.y + r * 0.25, r * 0.12, c);
        }
    }
}
