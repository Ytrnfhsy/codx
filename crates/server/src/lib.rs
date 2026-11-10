//! Toon Warfare dedicated server. Can run standalone (`toon-server`) or embedded in the client
//! ("Створити сервер" in the main menu).

pub mod admin;
pub mod bots;
pub mod boxes;
pub mod br;
pub mod cars;
pub mod combat;
pub mod config;
pub mod game;
pub mod http;
pub mod nav;
pub mod netserver;
pub mod pickups;
pub mod sab;
pub mod update;
pub mod vehicles;

use config::ServerConfig;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;

/// Requests from the console / web panel into the game thread.
pub enum AdminMsg {
    Cmd(String, Sender<String>),
    State(Sender<serde_json::Value>),
    Info(Sender<serde_json::Value>),
    CheckRcon(String, Sender<bool>),
}

pub struct ServerHandle {
    pub stop: Arc<AtomicBool>,
    pub admin: Sender<AdminMsg>,
    pub port: u16,
    pub http_port: Option<u16>,
    thread: Option<JoinHandle<()>>,
}

impl ServerHandle {
    /// Runs an admin command synchronously.
    pub fn command(&self, cmd: &str) -> String {
        let (tx, rx) = channel();
        if self.admin.send(AdminMsg::Cmd(cmd.to_string(), tx)).is_err() {
            return "Сервер не працює".into();
        }
        rx.recv_timeout(std::time::Duration::from_secs(3))
            .unwrap_or_else(|_| "Немає відповіді".into())
    }

    pub fn shutdown(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }

    pub fn is_running(&self) -> bool {
        self.thread
            .as_ref()
            .map(|t| !t.is_finished())
            .unwrap_or(false)
    }

    pub fn wait(mut self) {
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for ServerHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Binds the sockets and starts the game (and optional HTTP) threads.
pub fn start(cfg: ServerConfig, cfg_path: Option<PathBuf>) -> Result<ServerHandle, String> {
    let cfg = cfg.sanitized();
    let addr = format!("{}:{}", cfg.bind, cfg.port);
    let socket =
        UdpSocket::bind(&addr).map_err(|e| format!("Не вдалося відкрити UDP {addr}: {e}"))?;
    let (tx, rx) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    let http_enabled = cfg.http_enabled;
    let bind = cfg.bind.clone();
    let port = cfg.port;
    let mut game = game::Game::new(cfg);
    game.cfg_path = cfg_path;
    let http_port = if http_enabled {
        match http::spawn(&bind, port, tx.clone()) {
            Ok(_) => Some(port),
            Err(e) => {
                eprintln!("[http] веб-панель недоступна ({e})");
                None
            }
        }
    } else {
        None
    };
    let stop2 = stop.clone();
    let thread = std::thread::Builder::new()
        .name("game".into())
        .spawn(move || netserver::run(socket, game, rx, stop2))
        .map_err(|e| e.to_string())?;
    Ok(ServerHandle {
        stop,
        admin: tx,
        port,
        http_port,
        thread: Some(thread),
    })
}
