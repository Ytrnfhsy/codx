//! Gameplay tuning shared by client and server.

/// Server simulation rate.
pub const TICK_RATE: u32 = 30;
pub const TICK_DT: f32 = 1.0 / TICK_RATE as f32;
/// Remote players are rendered this far in the past (seconds).
pub const INTERP_DELAY: f32 = 0.1;
/// Max amount of time the server rewinds for lag compensation.
pub const MAX_REWIND: f32 = 0.35;

pub const DEFAULT_PORT: u16 = 27015;
pub const PROTOCOL_VERSION: u16 = 3;
/// Prefix of a connectionless server info query.
pub const QUERY_MAGIC: &[u8; 4] = b"TWQ?";
pub const QUERY_REPLY_MAGIC: &[u8; 4] = b"TWQ!";
pub const MAX_PLAYERS: usize = 32;

pub const PLAYER_RADIUS: f32 = 0.35;
pub const STAND_HEIGHT: f32 = 1.9;
pub const CROUCH_HEIGHT: f32 = 1.3;
pub const PRONE_HEIGHT: f32 = 0.6;
pub const STAND_EYE: f32 = 1.62;
pub const CROUCH_EYE: f32 = 1.06;
pub const PRONE_EYE: f32 = 0.38;

pub const WALK_SPEED: f32 = 5.2;
pub const SPRINT_SPEED: f32 = 7.9;
pub const CROUCH_SPEED: f32 = 2.7;
pub const PRONE_SPEED: f32 = 1.1;
pub const ADS_SPEED_MULT: f32 = 0.6;
pub const GROUND_ACCEL: f32 = 55.0;
pub const AIR_ACCEL: f32 = 6.0;
pub const GRAVITY: f32 = 20.0;
pub const JUMP_SPEED: f32 = 6.6;
pub const STEP_HEIGHT: f32 = 0.5;
/// Sprint duration in seconds (doubled by Extreme Conditioning).
pub const SPRINT_TIME: f32 = 4.0;

pub const MAX_HEALTH: f32 = 100.0;
pub const REGEN_DELAY: f32 = 5.0;
pub const REGEN_RATE: f32 = 30.0;

pub const MELEE_RANGE: f32 = 1.8;
pub const MELEE_DAMAGE: f32 = 135.0;
pub const MELEE_COOLDOWN: f32 = 0.9;

pub const GRENADE_FUSE: f32 = 3.5;
pub const GRENADE_SPEED: f32 = 17.0;
pub const GRENADE_RADIUS: f32 = 6.5;
pub const GRENADE_DAMAGE: f32 = 150.0;
pub const WEAPON_SWITCH_TIME: f32 = 0.55;

pub const UAV_KILLS: u32 = 3;
pub const AIRSTRIKE_KILLS: u32 = 5;
pub const HELI_KILLS: u32 = 7;
pub const UAV_DURATION: f32 = 30.0;
pub const HELI_DURATION: f32 = 50.0;
pub const HELI_HEALTH: f32 = 1500.0;
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
