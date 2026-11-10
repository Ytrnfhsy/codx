#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Toon Warfare — game client.

mod audio;
mod binds;
mod character;
mod effects;
mod game;
mod hud;
mod killcam;
mod level;
mod menu;
mod mesh;
mod models;
mod netclient;
mod pilot;
mod ragdoll;
mod render;
mod settings;
mod soldier;
mod textures;
mod ui;
mod updater;

use game::{GameClient, Overlay, Welcome};
use macroquad::prelude::*;
use menu::{MenuAction, MenuState};
use netclient::NetClient;
use settings::Settings;
use std::time::Instant;
use toon_shared::lang::t;
use toon_shared::map::MapId;
use toon_shared::protocol::{C2S, S2C};
use toon_shared::tr;
use toon_shared::{Mode, Team};

#[derive(Default, Clone)]
struct Args {
    connect: Option<String>,
    name: Option<String>,
    host: bool,
    no_sound: bool,
    shot: Option<String>,
    shot_after: f32,
    /// Take the screenshot this many seconds into the first killcam instead.
    shot_killcam: Option<f32>,
    score_limit: Option<i32>,
    model_gallery: bool,
    auto: bool,
    class: Option<usize>,
    team: Option<String>,
    map: Option<String>,
    mode: Option<String>,
    bots: Option<usize>,
    look: Option<(f32, f32)>,
    spec_pos: Option<Vec3>,
    render_scale: Option<f32>,
    rcon: Option<String>,
    no_vsync: bool,
    /// Do not look for game updates.
    no_update: bool,
    /// Language for this run (also saved): "uk" or "en".
    lang: Option<String>,
    log_fps: bool,
    width: i32,
    height: i32,
}

fn parse_args() -> Args {
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

enum Screen {
    Menu,
    Connecting {
        net: NetClient,
        address: String,
        since: Instant,
    },
    Game(Box<GameClient>),
}

/// Whether the window was created with vsync (changing it needs a restart).
pub static VSYNC_AT_START: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

fn window_conf(a: &Args, vsync: bool, fullscreen: bool) -> macroquad::conf::Conf {
    let mut platform = macroquad::miniquad::conf::Platform::default();
    platform.swap_interval = Some(if vsync { 1 } else { 0 });
    macroquad::conf::Conf {
        miniquad_conf: macroquad::miniquad::conf::Conf {
            platform,
            window_title: "Toon Warfare".to_owned(),
            window_width: a.width,
            window_height: a.height,
            high_dpi: false,
            window_resizable: true,
            fullscreen,
            // the 3D world is drawn into its own target (with its own smoothing and a
            // precise depth buffer); the window itself needs neither
            sample_count: 1,
            ..Default::default()
        },
        draw_call_vertex_capacity: 24000,
        draw_call_index_capacity: 36000,
        ..Default::default()
    }
}

fn main() {
    let args = parse_args();
    let loaded = Settings::load();
    let vsync = loaded.vsync && !args.no_vsync;
    VSYNC_AT_START.store(vsync, std::sync::atomic::Ordering::Relaxed);
    let conf = window_conf(&args, vsync, loaded.fullscreen);
    macroquad::Window::from_config(conf, amain(args));
}

struct Backdrop {
    mesh: mesh::ToonMesh,
    sky: mesh::MeshBuilder,
    map: toon_shared::map::Map,
}

fn draw_backdrop(b: &Backdrop, mats: &mesh::Materials) {
    let t = get_time() as f32 * 0.05;
    let c = b.map.center();
    let r = b.map.size().max_element() * 0.55;
    let pos = vec3(c.x + t.cos() * r, 16.0, c.y + t.sin() * r);
    let cam = Camera3D {
        position: pos,
        target: vec3(c.x, 0.0, c.y),
        up: Vec3::Y,
        fovy: 0.9,
        z_near: 0.1,
        z_far: 600.0,
        ..Default::default()
    };
    let fog = Color::from_rgba(b.map.fog[0], b.map.fog[1], b.map.fog[2], 255);
    clear_background(fog);
    set_camera(&cam);
    {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.push_model_matrix(Mat4::from_translation(vec3(pos.x, 0.0, pos.z)));
    }
    mats.set_view(Vec3::ZERO, fog, 10_000.0, 20_000.0);
    gl_use_material(&mats.world);
    b.sky.draw();
    gl_use_default_material();
    {
        let gl = unsafe { get_internal_gl() }.quad_gl;
        gl.pop_model_matrix();
    }
    mats.set_view(pos, fog, 70.0, 260.0);
    mats.draw_toon(&b.mesh);
    set_default_camera();
    draw_rectangle(
        0.0,
        0.0,
        screen_width(),
        screen_height(),
        Color::new(0.05, 0.06, 0.1, 0.35),
    );
}

async fn amain(args: Args) {
    let fonts = ui::Fonts::load();
    let mats = mesh::Materials::new();
    let mut settings = Settings::load();
    binds::use_binds(&settings.binds);
    match args.lang.as_deref() {
        Some("en") => settings.lang = toon_shared::lang::Lang::En,
        Some("uk") => settings.lang = toon_shared::lang::Lang::Uk,
        _ => {}
    }
    toon_shared::lang::set(settings.lang);
    if let Some(n) = &args.name {
        settings.name = n.clone();
    }
    if let Some(r) = args.render_scale {
        settings.render_scale = r;
    }
    let audio = audio::Audio::load(settings.volume, !args.no_sound).await;
    let backdrop = {
        let map = toon_shared::maps::build(MapId::Market);
        Backdrop {
            mesh: game::build_map_mesh(&map),
            sky: game::build_sky(&map),
            map,
        }
    };
    let mut menu = MenuState::new(!args.no_update);
    let mut screen = Screen::Menu;
    let mut server: Option<toon_server::ServerHandle> = None;
    let mut focus: Option<u32> = None;
    let mut grabbed = false;
    let mut fps = 60.0f32;
    let start = get_time();
    let mut auto_done = false;
    let mut low_res: Option<(RenderTarget, i32)> = None;
    let mut frame_start = std::time::Instant::now();
    let mut fps_window = (std::time::Instant::now(), 0u32);
    let mut worst_frame = 0.0f32;
    let mut prev_mouse = Vec2::ZERO;
    prevent_quit();

    let host = |settings: &Settings, args: &Args| -> Result<toon_server::ServerHandle, String> {
        let hs = &settings.host;
        let mut cfg = toon_server::config::ServerConfig::default();
        cfg.name = hs.name.clone();
        cfg.port = hs.port;
        cfg.bots = hs.bots;
        cfg.br.bots = hs.br_bots;
        cfg.br.max_players = hs.br_max_players;
        cfg.br.squad = hs.br_squad.clamp(1, 4);
        cfg.bot_skill = hs.bot_skill;
        cfg.rcon_password = args.rcon.clone().unwrap_or(hs.rcon.clone());
        cfg.password = hs.password.clone();
        cfg.max_players = hs.max_players;
        cfg.min_players = 1;
        if let Some(n) = args.score_limit {
            cfg.tdm.score_limit = n.max(1);
        }
        let map = args.map.clone().unwrap_or(hs.map.clone());
        let mode = args.mode.clone().unwrap_or(hs.mode.clone());
        // --bots: for the mode we start with
        if let Some(n) = args.bots {
            if toon_shared::Mode::parse(&mode) == Some(toon_shared::Mode::Br) {
                cfg.br.bots = n;
                cfg.br.max_players = cfg.br.max_players.max(n);
            } else {
                cfg.bots = n;
                cfg.max_players = cfg.max_players.max(n);
            }
        }
        if toon_shared::Mode::parse(&mode) == Some(toon_shared::Mode::Br) {
            // a battle royale server plays round after round of it
            cfg.rotation = vec![toon_server::config::RotationEntry { map, mode }];
        } else {
            cfg.rotation.retain(|r| !(r.map == map && r.mode == mode));
            cfg.rotation
                .insert(0, toon_server::config::RotationEntry { map, mode });
        }
        toon_server::start(cfg, None)
    };

    if args.host {
        match host(&settings, &args) {
            Ok(h) => {
                let port = h.port;
                server = Some(h);
                match NetClient::connect(
                    &format!("127.0.0.1:{port}"),
                    &settings.name,
                    &settings.host.password,
                ) {
                    Ok(net) => {
                        screen = Screen::Connecting {
                            net,
                            address: format!("127.0.0.1:{port}"),
                            since: Instant::now(),
                        }
                    }
                    Err(e) => menu.error = Some(e),
                }
            }
            Err(e) => menu.error = Some(e),
        }
    } else if let Some(addr) = &args.connect {
        match NetClient::connect(addr, &settings.name, &settings.password) {
            Ok(net) => {
                screen = Screen::Connecting {
                    net,
                    address: addr.clone(),
                    since: Instant::now(),
                }
            }
            Err(e) => menu.error = Some(e),
        }
    }

    let mut fullscreen_now = settings.fullscreen;
    loop {
        // full screen / window switches at once (F11 too)
        if is_key_pressed(KeyCode::F11) {
            settings.fullscreen = !settings.fullscreen;
            settings.save();
        }
        if settings.fullscreen != fullscreen_now {
            fullscreen_now = settings.fullscreen;
            set_fullscreen(fullscreen_now);
        }
        // the volume slider works at once, in the menu and in a game
        if (audio.volume() - settings.volume).abs() > 1e-4 {
            audio.set_volume(settings.volume);
        }
        // the chosen language for everything this frame draws and says
        toon_shared::lang::set(settings.lang);
        let dt = get_frame_time().min(0.1);
        fps = fps * 0.95 + (1.0 / dt.max(1e-4)) * 0.05;
        let mut chars = Vec::new();
        while let Some(c) = get_char_pressed() {
            chars.push(c);
        }
        let backspace = is_key_pressed(KeyCode::Backspace);
        let enter = is_key_pressed(KeyCode::Enter) || is_key_pressed(KeyCode::KpEnter);
        let (mx, my) = mouse_position();

        if is_quit_requested() {
            if let Screen::Game(g) = &mut screen {
                g.disconnect();
            }
            settings.save();
            break;
        }

        let mut next: Option<Screen> = None;
        match &mut screen {
            Screen::Menu if args.model_gallery => {
                render::draw_model_gallery(&mats);
            }
            Screen::Menu => {
                if grabbed {
                    set_cursor_grab(false);
                    show_mouse(true);
                    grabbed = false;
                }
                draw_backdrop(&backdrop, &mats);
                let mut ui = ui::Ui {
                    fonts: &fonts,
                    mouse: vec2(mx, my),
                    clicked: is_mouse_button_pressed(MouseButton::Left),
                    down: is_mouse_button_down(MouseButton::Left),
                    focus: &mut focus,
                    chars: &chars,
                    backspace,
                    enter,
                    hot: false,
                };
                let running = server.as_ref().filter(|s| s.is_running()).map(|s| s.port);
                match menu::draw_menu(&mut ui, &mut menu, &mut settings, running) {
                    Some(MenuAction::Connect { address }) => {
                        menu.error = None;
                        settings.save();
                        match NetClient::connect(&address, &settings.name, &settings.password) {
                            Ok(net) => {
                                next = Some(Screen::Connecting {
                                    net,
                                    address,
                                    since: Instant::now(),
                                })
                            }
                            Err(e) => menu.error = Some(e),
                        }
                    }
                    Some(MenuAction::Host) => {
                        settings.save();
                        if let Some(mut s) = server.take() {
                            s.shutdown();
                        }
                        match host(&settings, &args) {
                            Ok(h) => {
                                let port = h.port;
                                // back from the game, the main page shows the server's status
                                menu.page = menu::Page::Main;
                                menu.host_info = Some(format!(
                                    "{}",
                                    if h.http_port.is_some() {
                                        tr!(
                                            "адмінка: http://localhost:{port}/admin",
                                            "admin: http://localhost:{port}/admin"
                                        )
                                    } else {
                                        String::new()
                                    }
                                ));
                                server = Some(h);
                                let addr = format!("127.0.0.1:{port}");
                                match NetClient::connect(
                                    &addr,
                                    &settings.name,
                                    &settings.host.password,
                                ) {
                                    Ok(net) => {
                                        next = Some(Screen::Connecting {
                                            net,
                                            address: addr,
                                            since: Instant::now(),
                                        })
                                    }
                                    Err(e) => menu.error = Some(e),
                                }
                            }
                            Err(e) => menu.error = Some(e),
                        }
                    }
                    Some(MenuAction::StopServer) => {
                        if let Some(mut s) = server.take() {
                            s.shutdown();
                        }
                        menu.host_info = None;
                    }
                    Some(MenuAction::Quit) => {
                        settings.save();
                        break;
                    }
                    None => {}
                }
            }
            Screen::Connecting {
                net,
                address,
                since,
            } => {
                draw_backdrop(&backdrop, &mats);
                let ui = ui::Ui {
                    fonts: &fonts,
                    mouse: vec2(mx, my),
                    clicked: false,
                    down: false,
                    focus: &mut focus,
                    chars: &[],
                    backspace: false,
                    enter: false,
                    hot: false,
                };
                let dots = ".".repeat(((get_time() * 3.0) as usize) % 4);
                ui.centered(
                    &tr!(
                        "Під'єднання до {address}{dots}",
                        "Connecting to {address}{dots}"
                    ),
                    screen_width() / 2.0,
                    screen_height() / 2.0,
                    28,
                    ui::ACCENT,
                );
                ui.centered(
                    t("Esc — скасувати", "Esc — cancel"),
                    screen_width() / 2.0,
                    screen_height() / 2.0 + 36.0,
                    16,
                    ui::MUTED,
                );
                net.maybe_flush(0.1);
                let msgs = net.poll();
                let mut welcome = None;
                let mut rest = Vec::new();
                for m in msgs {
                    match m {
                        S2C::Welcome {
                            id,
                            server_name,
                            motd,
                            map,
                            mode,
                            settings: gs,
                        } if welcome.is_none() => {
                            welcome = Some(Welcome {
                                id,
                                server_name,
                                motd,
                                map,
                                mode,
                                settings: gs,
                            });
                        }
                        S2C::Reject(r) => {
                            menu.error = Some(r);
                            next = Some(Screen::Menu);
                        }
                        other => rest.push(other),
                    }
                }
                if let Some(w) = welcome {
                    settings.remember_server(address);
                    settings.save();
                    let net = std::mem::replace(
                        net,
                        NetClient::connect("127.0.0.1:1", "", "").expect("dummy socket"),
                    );
                    let mut g = GameClient::new(net, w, &settings, rest);
                    if let Some((y, p)) = args.look {
                        g.yaw = y;
                        g.pitch = p;
                    }
                    if let Some(p) = args.spec_pos {
                        g.spec_pos = p;
                    }
                    next = Some(Screen::Game(Box::new(g)));
                } else if since.elapsed().as_secs_f32() > 8.0 {
                    menu.error = Some(tr!(
                        "Сервер {address} не відповідає. Перевір адресу і чи відкрито UDP-порт.",
                        "Server {address} does not answer. Check the address and that the UDP port is open."
                    ));
                    next = Some(Screen::Menu);
                } else if is_key_pressed(KeyCode::Escape) {
                    next = Some(Screen::Menu);
                }
            }
            Screen::Game(g) => {
                // ---- keyboard routing
                let mut chars_for_ui: Vec<char> = Vec::new();
                if let Some((text, team)) = g.chat_input.as_mut() {
                    for c in &chars {
                        if !c.is_control() && text.chars().count() < 120 {
                            text.push(*c);
                        }
                    }
                    if backspace {
                        text.pop();
                    }
                    if enter {
                        let (t, team) = (text.clone(), *team);
                        g.chat_input = None;
                        g.send_chat(t, team);
                    } else if is_key_pressed(KeyCode::Escape) {
                        g.chat_input = None;
                    }
                } else if g.overlay == Overlay::None {
                    use binds::Action;
                    if binds::pressed(Action::ChatAll) {
                        g.chat_input = Some((String::new(), false));
                    } else if binds::pressed(Action::ChatTeam) {
                        g.chat_input = Some((String::new(), true));
                    } else if is_key_pressed(KeyCode::Escape) && g.big_map {
                        g.big_map = false;
                    } else if is_key_pressed(KeyCode::Escape) {
                        // Esc first skips a killcam (like a click), then opens the pause menu
                        if !g.skip_killcam() {
                            g.overlay = Overlay::Pause;
                        }
                    } else if binds::pressed(Action::Class) && g.team.is_playing() {
                        if g.mode == toon_shared::Mode::Br {
                            // no classes in the battle royale: the key opens the map
                            g.big_map = !g.big_map;
                        } else {
                            g.overlay = Overlay::Class;
                        }
                    }
                } else {
                    chars_for_ui = chars.clone();
                    // (while the Controls page waits for a key, Esc only cancels that)
                    if is_key_pressed(KeyCode::Escape) && !binds::capturing() {
                        g.overlay = match g.overlay {
                            Overlay::Edit(_) => Overlay::Class,
                            Overlay::Settings => Overlay::Pause,
                            Overlay::Video => {
                                settings.save();
                                Overlay::Settings
                            }
                            Overlay::Controls => {
                                g.binds_editor.close();
                                Overlay::Settings
                            }
                            // before a class is chosen, Esc must still lead somewhere: the
                            // pause menu (settings, disconnect) instead of doing nothing
                            Overlay::Team | Overlay::Class if !g.has_class => Overlay::Pause,
                            Overlay::Pause => g.after_pause(),
                            _ => Overlay::None,
                        };
                    }
                }

                // ---- cursor grab
                // (the big map wants the cursor too: a click puts a marker on it)
                let want_grab = g.overlay == Overlay::None && g.chat_input.is_none() && !g.big_map;
                if want_grab != grabbed {
                    set_cursor_grab(want_grab);
                    show_mouse(!want_grab);
                    grabbed = want_grab;
                }
                let md = mouse_delta_position();
                let pixel = vec2(-md.x * screen_width() * 0.5, -md.y * screen_height() * 0.5);
                let pixel = if grabbed {
                    pixel.clamp(Vec2::splat(-400.0), Vec2::splat(400.0))
                } else {
                    Vec2::ZERO
                };
                // a mouse reporting at 125-1000 Hz and frames drawn at another rate give
                // some frames no movement and others a double step; averaging two frames
                // evens that out without changing how far the view turns
                g.mouse_delta = if settings.mouse_smoothing {
                    (pixel + prev_mouse) * 0.5
                } else {
                    pixel
                };
                prev_mouse = pixel;

                // ---- test automation (screenshots / smoke tests)
                if args.auto && !auto_done && get_time() - start > 1.0 {
                    let team = match args.team.as_deref() {
                        Some("axis") => Some(Team::Axis),
                        Some("allies") => Some(Team::Allies),
                        Some("spec") => Some(Team::Spectator),
                        _ => None,
                    };
                    g.join_team(team);
                    let classes = settings.classes();
                    let c = args.class.unwrap_or(0).min(classes.len() - 1);
                    g.net.send(C2S::SetLoadout(classes[c].1));
                    g.has_class = true;
                    g.overlay = Overlay::None;
                    // (screenshots and smoke tests do not wait for a click on «Ready»)
                    g.net.send(C2S::Ready(true));
                    auto_done = true;
                }

                g.update(dt, &settings, Some(&audio));
                // the 3D world goes into its own target: a 32-bit depth buffer whatever the
                // window got from the system (16 bits on some setups make far walls
                // flicker and stripe), optional smoothing, and a lower resolution for weak
                // GPUs
                let scale = settings.render_scale.clamp(0.4, 1.0);
                let (tw, th) = (
                    (screen_width() * scale).max(64.0) as u32,
                    (screen_height() * scale).max(64.0) as u32,
                );
                let samples = if settings.msaa { 4 } else { 1 };
                if low_res
                    .as_ref()
                    .map(|(t, s): &(RenderTarget, i32)| {
                        (t.texture.width() as u32, t.texture.height() as u32, *s)
                            != (tw, th, samples)
                    })
                    .unwrap_or(true)
                {
                    let rt = render_target_ex(
                        tw,
                        th,
                        RenderTargetParams {
                            sample_count: samples,
                            depth: true,
                        },
                    );
                    rt.texture.set_filter(if scale < 0.99 {
                        FilterMode::Linear
                    } else {
                        FilterMode::Nearest
                    });
                    low_res = Some((rt, samples));
                }
                let target = low_res.as_ref().map(|l| l.0.clone());
                let view = render::make_view(g, &settings, target.clone());
                let tags = render::render_world(g, &mats, &settings, &view);
                render::render_viewmodel(g, &mats, &settings, target.clone());
                set_default_camera();
                if let Some(rt) = &target {
                    clear_background(BLACK);
                    draw_texture_ex(
                        &rt.texture,
                        0.0,
                        0.0,
                        WHITE,
                        DrawTextureParams {
                            dest_size: Some(vec2(screen_width(), screen_height())),
                            flip_y: true,
                            ..Default::default()
                        },
                    );
                }
                let mut ui = ui::Ui {
                    fonts: &fonts,
                    mouse: vec2(mx, my),
                    clicked: is_mouse_button_pressed(MouseButton::Left),
                    down: is_mouse_button_down(MouseButton::Left),
                    focus: &mut focus,
                    chars: &chars_for_ui,
                    backspace,
                    enter,
                    hot: false,
                };
                g.skin_preview = None;
                let action = g.draw_hud(&mut ui, &view, &tags, &mut settings, fps);
                if let Some(rect) = g.skin_preview {
                    render::draw_skin_preview(&mats, settings.skin, rect, get_time() as f32);
                }
                if let Some(hud::HudAction::Leave) = action {
                    g.disconnect();
                    next = Some(Screen::Menu);
                }
                if let Some(reason) = g.leave.take() {
                    menu.error = Some(reason);
                    next = Some(Screen::Menu);
                }
            }
        }

        // screenshot for automated testing
        if let Some(path) = &args.shot {
            let in_killcam = match (&screen, args.shot_killcam) {
                (Screen::Game(g), Some(t)) => g.killcam.as_ref().is_some_and(|k| k.age >= t),
                _ => false,
            };
            if in_killcam || get_time() - start > args.shot_after as f64 {
                get_screen_data().export_png(path);
                println!("screenshot saved: {path}");
                if let Screen::Game(g) = &mut screen {
                    g.disconnect();
                }
                break;
            }
        }

        if let Some(s) = next {
            screen = s;
        }
        let _ = Mode::Tdm;
        // FPS cap when vsync is off: wait until a full frame period has passed since the
        // previous frame began (this includes the time spent presenting it)
        if !VSYNC_AT_START.load(std::sync::atomic::Ordering::Relaxed) && settings.fps_limit > 0 {
            let target = std::time::Duration::from_secs_f64(1.0 / settings.fps_limit as f64);
            let elapsed = frame_start.elapsed();
            if elapsed < target {
                let left = target - elapsed;
                if left > std::time::Duration::from_micros(1500) {
                    std::thread::sleep(left - std::time::Duration::from_micros(1000));
                }
                while frame_start.elapsed() < target {
                    std::hint::spin_loop();
                }
            }
        }
        frame_start = std::time::Instant::now();
        next_frame().await;
        if args.log_fps {
            fps_window.1 += 1;
            worst_frame = worst_frame.max(get_frame_time());
            if fps_window.0.elapsed().as_secs_f32() >= 1.0 {
                // the worst frame shows hitches an average hides
                println!(
                    "fps {:.0}  worst frame {:.1} ms",
                    fps_window.1 as f32 / fps_window.0.elapsed().as_secs_f32(),
                    worst_frame * 1000.0
                );
                fps_window = (std::time::Instant::now(), 0);
                worst_frame = 0.0;
            }
        }
    }
    if let Some(mut s) = server.take() {
        s.shutdown();
    }
}
