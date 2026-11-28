//! Behaviour snapshots of the server: a team deathmatch and a battle royale with bots,
//! played on a fixed seed at the fixed tick rate, boiled down to one number each.
//! A refactor that changes nothing must leave the numbers as they are.
//!
//! One real player plays along: jumps out of the plane, runs and turns with the client's
//! physics. Hashed: every player (by id), what lies on the ground, the zone, the score, the doors,
//! every snapshot and every message the server sends (sets that come out of a hash map in
//! any order are sorted first). The real clock plays no part: one tick is `TICK_DT`.
//!
//! When a change is *meant* to change the game, run
//! `GOLDEN_UPDATE=1 cargo test --release --test golden` and commit the new numbers.
//!
//! The numbers hold only where the maths library gives the same last bits (see
//! `libm_fingerprint`): elsewhere, CI's machines for instance, the comparison is skipped
//! and says so. On a refactor, take a snapshot on your machine before and compare after.

use toon_server::config::ServerConfig;
use toon_server::game::Game;
use toon_shared::consts::{PROTOCOL_VERSION, TICK_DT};
use toon_shared::map::MapId;
use toon_shared::protocol::{C2S, S2C};
use toon_shared::rng::Rng;
use toon_shared::Mode;

/// FNV-1a: the same number on every machine and every Rust version.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Fnv(0xcbf2_9ce4_8422_2325)
    }
    fn bytes(&mut self, b: &[u8]) {
        for &x in b {
            self.0 ^= x as u64;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }
    fn val<T: serde::Serialize>(&mut self, v: &T) {
        self.bytes(&toon_shared::net::encode(v));
    }
}

fn hash_state(g: &Game, h: &mut Fnv) {
    h.val(&(g.time, g.state, g.scores));
    for p in g.players.iter().flatten() {
        h.val(&(
            p.id, &p.name, p.team, p.alive, p.downed, p.in_plane, p.squad,
        ));
        h.val(&(
            p.hp,
            p.body.pos.to_array(),
            p.body.vel.to_array(),
            p.yaw,
            p.pitch,
        ));
        h.val(&(p.kills, p.deaths, p.assists, p.score, p.slot, p.life));
        let guns: Vec<_> = p.weapons.iter().map(|w| (w.id, w.mag, w.reserve)).collect();
        h.val(&(guns, p.inv));
    }
    let mut pk: Vec<_> = g
        .pickups
        .iter()
        .map(|p| (p.id, p.pos.to_array(), p.w.id, p.w.mag, p.w.reserve))
        .collect();
    pk.sort_by_key(|p| p.0);
    h.val(&pk);
    let mut loot: Vec<_> =
        g.br.loot
            .iter()
            .map(|l| (l.id, l.pos.to_array(), l.kind, l.amount))
            .collect();
    loot.sort_by_key(|l| l.0);
    h.val(&loot);
    h.val(&(g.br.running, g.br.entrants, g.br.zone.phase));
    h.val(&g.world.door_open);
    for id in 0..g.players.len() as u8 {
        if g.p(id).is_some() {
            h.val(&g.snapshot_for(id));
        }
    }
}

/// The messages of one tick, with the ones built from a hash map put in order.
fn hash_outbox(g: &mut Game, h: &mut Fnv) {
    for (dest, msg, reliable) in g.outbox.drain(..) {
        let dest = match dest {
            toon_server::game::Dest::All => 1000,
            toon_server::game::Dest::Team(t) => 2000 + t.index() as u32,
            toon_server::game::Dest::One(id) => id as u32,
        };
        h.val(&(dest, reliable));
        match msg {
            S2C::Ents {
                mut add,
                mut remove,
            } => {
                add.sort_by_key(|e| e.id());
                remove.sort();
                h.val(&S2C::Ents { add, remove });
            }
            mut m => {
                // (the rcon nonce is random on purpose)
                if let S2C::Welcome { nonce, .. } = &mut m {
                    *nonce = 0;
                }
                h.val(&m);
            }
        }
    }
}

/// A player who plays: jumps out of the plane, then runs about with the client's own
/// physics, turning slowly, and sends what a client sends.
struct Human {
    id: u8,
    life: u16,
    body: toon_shared::physics::Body,
    seq: u32,
}

impl Human {
    fn step(&mut self, g: &mut Game) {
        use toon_shared::physics::{MoveInput, Stance};
        use toon_shared::protocol::{flags, InputState};
        let Some(p) = g.p(self.id) else { return };
        if p.in_plane && g.tick % 300 == 150 {
            g.handle(self.id, C2S::BrJump);
            return;
        }
        let Some(p) = g.p(self.id) else { return };
        if !p.alive {
            return;
        }
        if p.life != self.life {
            // a new life: where the server put us
            self.life = p.life;
            self.body = p.body;
        }
        let yaw = (g.tick as f32 * 0.01).sin() * 2.5;
        let inp = MoveInput {
            forward: 1.0,
            yaw,
            sprint: g.tick % 200 < 120,
            jump: g.tick.is_multiple_of(97),
            want_stance: Stance::Stand,
            ..Default::default()
        };
        self.body.step(&g.world, &inp, &g.tuning.movement, TICK_DT);
        self.seq += 1;
        let mut fl = if self.body.on_ground {
            flags::ON_GROUND
        } else {
            0
        };
        if inp.sprint {
            fl |= flags::SPRINT;
        }
        g.handle(
            self.id,
            C2S::Input(InputState {
                seq: self.seq,
                life: self.life,
                pos: self.body.pos.to_array(),
                vel: self.body.vel.to_array(),
                yaw,
                pitch: 0.0,
                stance: self.body.stance.to_u8(),
                flags: fl,
                slot: 0,
            }),
        );
    }

    /// The server's answer to a position it would not take: back to where it says.
    fn corrections(&mut self, g: &Game) {
        for (dest, msg, _) in &g.outbox {
            if let (toon_server::game::Dest::One(id), S2C::Correction { pos, life }) = (dest, msg) {
                if *id == self.id && *life == self.life {
                    self.body.pos = glam::Vec3::from_array(*pos);
                    self.body.vel = glam::Vec3::ZERO;
                }
            }
        }
    }
}

fn play(map: MapId, mode: Mode, seed: u64, bots: usize, human: bool, secs: f32) -> u64 {
    let dir = std::env::temp_dir().join(format!("tw-golden-{}-{seed}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let mut cfg = ServerConfig {
        ready_check: false,
        bots,
        data_dir: dir.to_string_lossy().into(),
        ..Default::default()
    };
    cfg.br.bots = bots;
    cfg.br.max_players = cfg.br.max_players.max(bots + 1);
    let mut g = Game::new(cfg);
    g.rng = Rng::new(seed);
    g.change_map(map, mode);
    let mut human = human.then(|| {
        let id = g
            .connect(
                "127.0.0.1:7000".parse().unwrap(),
                "Гравець",
                "",
                PROTOCOL_VERSION,
            )
            .unwrap();
        g.handle(id, C2S::JoinTeam(None));
        g.handle(
            id,
            C2S::SetLoadout(toon_shared::weapons::DEFAULT_CLASSES[0].1),
        );
        Human {
            id,
            life: u16::MAX,
            body: Default::default(),
            seq: 0,
        }
    });
    let mut h = Fnv::new();
    for _ in 0..(secs / TICK_DT) as usize {
        if let Some(m) = human.as_mut() {
            m.step(&mut g);
        }
        g.tick(TICK_DT);
        if let Some(m) = human.as_mut() {
            m.corrections(&g);
        }
        hash_outbox(&mut g, &mut h);
        hash_state(&g, &mut h);
    }
    let _ = std::fs::remove_dir_all(&dir);
    h.0
}

/// What this machine's maths library makes of a few thousand sines, cosines,
/// exponentials and powers. The system's sin, exp and friends may differ in the last bit
/// from one processor to another (they pick their code by what the processor can do), and
/// a match of bots amplifies that into a different game; so a snapshot is only compared on
/// a machine whose maths agree with the one it was taken on.
fn libm_fingerprint() -> String {
    let mut h = Fnv::new();
    for i in 0..4000 {
        let x = (i as f32 - 2000.0) * 0.0173;
        let y = (i as f32) * 0.0031 + 0.01;
        for v in [
            x.sin(),
            x.cos(),
            x.tan(),
            (x * 0.05).exp(),
            y.ln(),
            y.powf(1.7),
            y.atan2(x),
            (x * 0.0005).asin(),
            (x * 0.0005).acos(),
            y.sqrt(),
        ] {
            h.bytes(&v.to_bits().to_le_bytes());
        }
        let d = x as f64;
        for v in [d.sin(), d.cos(), (d * 0.05).exp()] {
            h.bytes(&v.to_bits().to_le_bytes());
        }
    }
    format!("{:016x}", h.0)
}

fn check(name: &str, got: u64) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/golden")
        .join(format!("{name}.txt"));
    let got = format!("{got:016x}");
    let libm = libm_fingerprint();
    if std::env::var_os("GOLDEN_UPDATE").is_some() {
        std::fs::write(&path, format!("{got}\nlibm {libm}\n")).unwrap();
        return;
    }
    let file = std::fs::read_to_string(&path).unwrap_or_default();
    let mut lines = file.lines();
    let want = lines.next().unwrap_or_default().trim();
    let want_libm = lines
        .next()
        .and_then(|l| l.strip_prefix("libm "))
        .unwrap_or_default();
    if want_libm != libm {
        eprintln!(
            "{name}: NOT COMPARED: this machine's maths library differs from the one the \
             snapshot was taken with (libm {libm}, snapshot {want_libm}). Compare on that \
             machine, or take a snapshot here first (GOLDEN_UPDATE=1) and compare after."
        );
        return;
    }
    assert_eq!(
        want, got,
        "{name}: the game played differently. If that is meant, run \
         `GOLDEN_UPDATE=1 cargo test --release --test golden` and commit tests/golden/"
    );
}

#[test]
fn team_deathmatch_plays_the_same() {
    check(
        "tdm_depot",
        play(MapId::Depot, Mode::Tdm, 0x7d3, 9, true, 60.0),
    );
}

#[test]
fn battle_royale_plays_the_same() {
    check(
        "br_island",
        play(MapId::Island, Mode::Br, 0xb4, 23, true, 60.0),
    );
}

/// The two runs of one match agree (nothing depends on the clock or a hash map's order).
#[test]
fn a_match_replays_exactly() {
    let a = play(MapId::Depot, Mode::Tdm, 5, 8, true, 20.0);
    let b = play(MapId::Depot, Mode::Tdm, 5, 8, true, 20.0);
    assert_eq!(a, b);
}
