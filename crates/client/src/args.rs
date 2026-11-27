//! The command line: what to connect to or host, and the switches for screenshots and
//! smoke tests.

use macroquad::prelude::{vec3, Vec3};

#[derive(Default, Clone)]
pub(crate) struct Args {
    /// Battle royale weather for a hosted game ("rain", "fog", …).
    pub(crate) weather: Option<String>,
    pub(crate) connect: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) host: bool,
    pub(crate) no_sound: bool,
    pub(crate) shot: Option<String>,
    pub(crate) shot_after: f32,
    /// Take the screenshot this many seconds into the first killcam instead.
    pub(crate) shot_killcam: Option<f32>,
    pub(crate) score_limit: Option<i32>,
    pub(crate) model_gallery: bool,
    pub(crate) auto: bool,
    pub(crate) class: Option<usize>,
    /// (smoke tests) this gun instead of the class's (a sidearm also goes into the hand)
    pub(crate) gun: Option<String>,
    /// (smoke tests) leave the loadout screen open
    pub(crate) open_loadout: bool,
    /// (smoke tests) the Controls page open
    pub(crate) open_controls: bool,
    /// (screenshots) the battle royale inventory open
    pub(crate) open_inventory: bool,
    /// (screenshots) the Esc menu open
    pub(crate) open_menu: bool,
    /// (screenshots) every frame lasts exactly 1/30 s, so that runs come out the same
    pub(crate) fixed_clock: bool,
    /// (screenshots, tests) the hosted server's dice seed
    pub(crate) seed: Option<u64>,
    /// (smoke tests) hold the trigger down once alive
    pub(crate) hold_fire: bool,
    /// (smoke tests) hold the trigger for a second, then let go (a bow's shot)
    pub(crate) tap_fire: bool,
    /// (smoke tests) aim down the sights once alive
    pub(crate) ads: bool,
    /// (smoke tests) keep looking this far up (radians; negative: down)
    pub(crate) pitch: Option<f32>,
    /// (screenshots) no HUD over the world
    pub(crate) no_hud: bool,
    /// (smoke tests) an admin command sent once alive (needs --rcon)
    pub(crate) exec: Option<String>,
    /// (smoke tests) keep looking at this point
    pub(crate) look_at: Option<Vec3>,
    /// (smoke tests) a spectator's camera here
    pub(crate) spec_at: Option<Vec3>,
    pub(crate) team: Option<String>,
    pub(crate) map: Option<String>,
    pub(crate) mode: Option<String>,
    pub(crate) bots: Option<usize>,
    pub(crate) look: Option<(f32, f32)>,
    pub(crate) spec_pos: Option<Vec3>,
    pub(crate) render_scale: Option<f32>,
    pub(crate) rcon: Option<String>,
    pub(crate) no_vsync: bool,
    /// Do not look for game updates.
    pub(crate) no_update: bool,
    /// Language for this run (also saved): "uk" or "en".
    pub(crate) lang: Option<String>,
    pub(crate) log_fps: bool,
    pub(crate) width: i32,
    pub(crate) height: i32,
}

pub(crate) fn parse_args() -> Args {
    let mut a = Args {
        shot_after: 6.0,
        width: 1280,
        height: 720,
        ..Default::default()
    };
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut i = 0;
    while i < v.len() {
        let next = v.get(i + 1).cloned();
        match v[i].as_str() {
            "--connect" => {
                a.connect = next;
                i += 1;
            }
            "--name" => {
                a.name = next;
                i += 1;
            }
            "--host" => a.host = true,
            "--no-vsync" => a.no_vsync = true,
            "--no-update" => a.no_update = true,
            "--lang" => {
                a.lang = next;
                i += 1;
            }
            "--log-fps" => a.log_fps = true,
            "--no-sound" | "--mute" => a.no_sound = true,
            "--screenshot" => {
                a.shot = next;
                i += 1;
            }
            "--screenshot-after" => {
                a.shot_after = next.and_then(|s| s.parse().ok()).unwrap_or(6.0);
                i += 1;
            }
            "--screenshot-killcam" => {
                a.shot_killcam = next.and_then(|s| s.parse().ok());
                i += 1;
            }
            "--score-limit" => {
                a.score_limit = next.and_then(|s| s.parse().ok());
                i += 1;
            }
            "--model-gallery" => a.model_gallery = true,
            "--auto" => a.auto = true,
            "--gun" => {
                a.gun = next;
                i += 1;
            }
            "--open-loadout" => a.open_loadout = true,
            "--open-controls" => a.open_controls = true,
            "--open-inventory" => a.open_inventory = true,
            "--open-menu" => a.open_menu = true,
            "--fixed-clock" => a.fixed_clock = true,
            "--seed" => {
                a.seed = next.and_then(|s| s.parse().ok());
                i += 1;
            }
            "--hold-fire" => a.hold_fire = true,
            "--tap-fire" => a.tap_fire = true,
            "--ads" => a.ads = true,
            "--pitch" => {
                a.pitch = next.and_then(|s| s.parse().ok());
                i += 1;
            }
            "--no-hud" => a.no_hud = true,
            "--exec" => {
                a.exec = next;
                i += 1;
            }
            "--spec-at" => {
                a.spec_at = next.and_then(|s| {
                    let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    (v.len() == 3).then(|| vec3(v[0], v[1], v[2]))
                });
                i += 1;
            }
            "--look-at" => {
                a.look_at = next.and_then(|s| {
                    let v: Vec<f32> = s.split(',').filter_map(|x| x.trim().parse().ok()).collect();
                    (v.len() == 3).then(|| vec3(v[0], v[1], v[2]))
                });
                i += 1;
            }
            "--class" => {
                a.class = next.and_then(|s| s.parse().ok());
                i += 1;
            }
            "--team" => {
                a.team = next;
                i += 1;
            }
            "--map" => {
                a.map = next;
                i += 1;
            }
            "--mode" => {
                a.mode = next;
                i += 1;
            }
            "--weather" => {
                a.weather = next;
                i += 1;
            }
            "--bots" => {
                a.bots = next.and_then(|s| s.parse().ok());
                i += 1;
            }
            "--look" => {
                a.look = next.and_then(|s| {
                    let (y, p) = s.split_once(',')?;
                    Some((y.parse().ok()?, p.parse().ok()?))
                });
                i += 1;
            }
            "--spec-pos" => {
                a.spec_pos = next.and_then(|s| {
                    let v: Vec<f32> = s.split(',').filter_map(|x| x.parse().ok()).collect();
                    (v.len() == 3).then(|| vec3(v[0], v[1], v[2]))
                });
                i += 1;
            }
            "--rcon" => {
                a.rcon = next;
                i += 1;
            }
            "--render-scale" => {
                a.render_scale = next.and_then(|s| s.parse().ok());
                i += 1;
            }
            "--size" => {
                if let Some((w, h)) = next.as_deref().and_then(|s| s.split_once('x')) {
                    a.width = w.parse().unwrap_or(1280);
                    a.height = h.parse().unwrap_or(720);
                }
                i += 1;
            }
            "-h" | "--help" => {
                println!(
                    "toon-warfare [--connect IP:порт] [--name ім'я] [--host] [--map wreck|site|depot|market|cargo] [--mode tdm|sab] [--bots N] [--score-limit N] [--no-sound] [--no-update] [--lang uk|en] [--size 1280x720]"
                );
                std::process::exit(0);
            }
            _ => {}
        }
        i += 1;
    }
    a
}
