//! Gameplay tuning shared by client and server.

/// Server simulation rate.
pub const TICK_RATE: u32 = 30;
pub const TICK_DT: f32 = 1.0 / TICK_RATE as f32;
/// Remote players are rendered this far in the past (seconds).
pub const INTERP_DELAY: f32 = 0.1;
/// Max amount of time the server rewinds for lag compensation.
pub const MAX_REWIND: f32 = 0.35;

pub const DEFAULT_PORT: u16 = 27015;
pub const PROTOCOL_VERSION: u16 = 22;
/// Version of the game build: the release tag (`TW_VERSION`, set by CI for `v*` tags,
/// without the `v`); for builds made by hand the nearest `v*` git tag (see build.rs), or
/// the crate version without git.
pub const GAME_VERSION: &str = match option_env!("TW_VERSION") {
    Some(v) => v,
    None => match option_env!("TW_GIT_VERSION") {
        Some(v) => v,
        None => env!("CARGO_PKG_VERSION"),
    },
};
/// Start of the server's reject message for a client of another version (the client
/// then offers to update).
/// (The Ukrainian one is what 0.2.x clients look for.)
pub const OUTDATED: crate::lang::Text = ("Версія гри не збігається.", "Game version mismatch.");
/// Start of the reject message when the server is the older one (nothing to update on
/// the client).
pub const SERVER_OUTDATED: crate::lang::Text = (
    "Сервер старіший за твою гру.",
    "The server is older than your game.",
);
/// True for builds published as a release (they look for and install updates).
pub const RELEASE_BUILD: bool = option_env!("TW_VERSION").is_some();
/// Prefix of a connectionless server info query.
pub const QUERY_MAGIC: &[u8; 4] = b"TWQ?";
pub const QUERY_REPLY_MAGIC: &[u8; 4] = b"TWQ!";
pub const MAX_PLAYERS: usize = 64;
/// On big maps a player is told only about what is this close (m); the fog ends sooner.
pub const INTEREST_RANGE: f32 = 400.0;
/// Guns and supplies on the ground are sent this far (big maps).
pub const LOOT_RANGE: f32 = 150.0;

pub const PLAYER_RADIUS: f32 = 0.35;
pub const STAND_HEIGHT: f32 = 1.9;
pub const CROUCH_HEIGHT: f32 = 1.3;
pub const PRONE_HEIGHT: f32 = 0.6;
pub const STAND_EYE: f32 = 1.62;
pub const CROUCH_EYE: f32 = 1.06;
pub const PRONE_EYE: f32 = 0.38;

// Movement speeds, acceleration, gravity, sprint and weapon handling live in
// `tuning.rs` so server admins can change them.
pub const STEP_HEIGHT: f32 = 0.5;

pub const MAX_HEALTH: f32 = 100.0;
pub const REGEN_DELAY: f32 = 5.0;
pub const REGEN_RATE: f32 = 30.0;

pub const MELEE_RANGE: f32 = 1.8;
pub const MELEE_DAMAGE: f32 = 135.0;
pub const MELEE_COOLDOWN: f32 = 0.9;

pub const HELI_HEIGHT: f32 = 22.0;
/// Firing an unsuppressed weapon shows you on enemy radar this long.
pub const RADAR_PING_TIME: f32 = 1.2;

pub const SAB_PICKUP_RADIUS: f32 = 1.6;
pub const SAB_SITE_RADIUS: f32 = 2.6;
pub const SAB_DEFUSE_RADIUS: f32 = 2.2;
pub const SAB_EXPLOSION_RADIUS: f32 = 12.0;

/// Points for score events.
pub const SCORE_KILL: i32 = 10;
pub const SCORE_HEADSHOT_BONUS: i32 = 5;
pub const SCORE_ASSIST: i32 = 2;
pub const SCORE_PLANT: i32 = 25;
pub const SCORE_DEFUSE: i32 = 25;

/// Smoke grenades: fuse, how long the cloud lasts, its full radius and how fast it swells.
pub const SMOKE_FUSE: f32 = 1.6;
pub const SMOKE_TIME: f32 = 18.0;
pub const SMOKE_RADIUS: f32 = 5.0;
pub const SMOKE_GROW: f32 = 2.5;
/// The cloud's centre sits this high above where the grenade came to rest.
pub const SMOKE_LIFT: f32 = 1.4;

/// Radius of a smoke cloud `age` seconds after it popped: swells, holds, then thins out.
pub fn smoke_radius(age: f32) -> f32 {
    if !(0.0..SMOKE_TIME).contains(&age) {
        return 0.0;
    }
    let grow = (age / SMOKE_GROW).min(1.0);
    let grow = 1.0 - (1.0 - grow) * (1.0 - grow);
    let fade = ((SMOKE_TIME - age) / 3.0).min(1.0);
    SMOKE_RADIUS * grow * fade.sqrt()
}
