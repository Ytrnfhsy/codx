//! `toon-server` — dedicated server with an interactive admin console.

use std::io::BufRead;
use std::path::PathBuf;
use toon_server::config::ServerConfig;

fn usage() {
    println!(
        "Toon Warfare — виділений сервер

Використання: toon-server [опції]
  --config <файл>     файл налаштувань (типово server.json; створюється автоматично)
  --port <порт>       порт (UDP для гри + TCP для веб-панелі)
  --name <назва>      назва сервера
  --rcon <пароль>     пароль адміністратора
  --password <пароль> пароль для входу на сервер
  --bots <n>          заповнювати ботами до n гравців
  --map <карта>       depot | market | cargo
  --mode <режим>      tdm | sab
  -h, --help          ця довідка

Після запуску в консолі доступні адмін-команди (help)."
    );
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut cfg_path = PathBuf::from("server.json");
    let mut overrides: Vec<(String, String)> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let a = args[i].as_str();
        if a == "-h" || a == "--help" {
            usage();
            return;
        }
        let Some(v) = args.get(i + 1).cloned() else {
            eprintln!("Немає значення для {a}");
            usage();
            std::process::exit(2);
        };
        if a == "--config" {
            cfg_path = PathBuf::from(v);
        } else {
            overrides.push((a.trim_start_matches('-').to_string(), v));
        }
        i += 2;
    }

    let mut cfg = match ServerConfig::load_or_create(&cfg_path) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Помилка конфігурації: {e}");
            std::process::exit(1);
        }
    };
    let mut first: Option<(String, String)> = None;
    for (k, v) in overrides {
        match k.as_str() {
            "port" => cfg.port = v.parse().unwrap_or(cfg.port),
            "name" => cfg.name = v,
            "rcon" => cfg.rcon_password = v,
            "password" => cfg.password = v,
            "bots" => cfg.bots = v.parse().unwrap_or(cfg.bots),
            "map" => first.get_or_insert((String::new(), String::new())).0 = v,
            "mode" => first.get_or_insert((String::new(), String::new())).1 = v,
            _ => {
                eprintln!("Невідома опція --{k}");
                usage();
                std::process::exit(2);
            }
        }
    }
    if let Some((map, mode)) = first {
        let cur = cfg.rotation[0].clone();
        cfg.rotation.insert(
            0,
            toon_server::config::RotationEntry {
                map: if map.is_empty() { cur.map } else { map },
                mode: if mode.is_empty() { cur.mode } else { mode },
            },
        );
    }
    if cfg.rcon_password.is_empty() {
        println!("⚠  rcon_password порожній — веб-адмінка і /rcon вимкнені (консоль працює).");
    }

    let handle = match toon_server::start(cfg.clone(), Some(cfg_path.clone())) {
        Ok(h) => h,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    println!("==============================================");
    println!(" Toon Warfare сервер «{}»", cfg.name);
    println!(" Гра:        UDP порт {}", handle.port);
    if let Some(p) = handle.http_port {
        println!(" Веб-панель: http://localhost:{p}/admin  (TCP порт {p})");
    }
    println!(" Конфіг:     {}", cfg_path.display());
    println!(" Введи help для списку команд.");
    println!("==============================================");

    let admin = handle.admin.clone();
    std::thread::spawn(move || {
        let stdin = std::io::stdin();
        for line in stdin.lock().lines() {
            let Ok(line) = line else { break };
            let line = line.trim().to_string();
            if line.is_empty() {
                continue;
            }
            if line == "quit" || line == "exit" {
                std::process::exit(0);
            }
            let (tx, rx) = std::sync::mpsc::channel();
            if admin.send(toon_server::AdminMsg::Cmd(line, tx)).is_err() {
                break;
            }
            if let Ok(out) = rx.recv_timeout(std::time::Duration::from_secs(3)) {
                println!("{out}");
            }
        }
    });
    handle.wait();
}
