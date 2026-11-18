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
  --map <карта>       wreck | site | depot | market | cargo | island
  --mode <режим>      tdm | sab | br (королівська битва, на карті island)
  --public <yes|no>   показувати сервер у загальному каталозі гри (запам'ятовується)
  --master [порт]     запустити не гру, а загальний каталог серверів (TCP, типово 27900)
  --check-update      показати, чи вийшла новіша версія, і вийти
  --update            завантажити новішу версію сервера з GitHub замість цієї й вийти
                      (потім перезапусти сервер; гравцям гра запропонує оновитися сама)
  --version           версія сервера
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
        if a == "--version" {
            println!("toon-server {}", toon_shared::consts::GAME_VERSION);
            return;
        }
        if a == "--master" {
            // the public server list instead of a game
            let port = args
                .get(i + 1)
                .and_then(|p| p.parse().ok())
                .unwrap_or(toon_shared::consts::MASTER_PORT);
            if let Err(e) = toon_server::master::run_master("::", port) {
                eprintln!("Не вдалося запустити каталог: {e}");
                std::process::exit(1);
            }
            return;
        }
        if a == "--check-update" || a == "--update" {
            std::process::exit(self_update(a == "--update"));
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
    let mut bots: Option<usize> = None;
    for (k, v) in overrides {
        match k.as_str() {
            "port" => cfg.port = v.parse().unwrap_or(cfg.port),
            "name" => cfg.name = v,
            "rcon" => cfg.rcon_password = v,
            "password" => cfg.password = v,
            "bots" => bots = v.parse().ok(),
            "public" => {
                cfg.public = Some(matches!(
                    v.as_str(),
                    "yes" | "y" | "так" | "true" | "1" | "on"
                ));
                let _ = cfg.save(&cfg_path);
            }
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
    // --bots sets the bots of the mode the server starts with (the battle royale keeps
    // its own count)
    if let Some(n) = bots {
        if cfg.rotation[0].resolve().1 == toon_shared::Mode::Br {
            cfg.br.bots = n;
            cfg.br.max_players = cfg.br.max_players.max(n);
        } else {
            cfg.bots = n;
        }
    }
    // the first start: ask whether the server goes on the public list (and remember)
    if cfg.public.is_none() {
        use std::io::{IsTerminal, Write};
        if std::io::stdin().is_terminal() {
            println!();
            println!("Показувати цей сервер у загальному каталозі гри?");
            println!(
                "  Його побачать усі гравці в «Знайти сервери» (каталог {}).",
                cfg.master
            );
            println!(
                "  Для цього сервер має бути доступний з інтернету (відкритий UDP-порт {}).",
                cfg.port
            );
            print!("Додати в каталог? [Т/н]: ");
            let _ = std::io::stdout().flush();
            let mut answer = String::new();
            let _ = std::io::stdin().read_line(&mut answer);
            let a = answer.trim().to_lowercase();
            let yes = a.is_empty() || ["т", "так", "y", "yes", "д", "да"].contains(&a.as_str());
            cfg.public = Some(yes);
            match cfg.save(&cfg_path) {
                Ok(()) => println!(
                    "{} Змінити потім: \"public\" у {} або команда public on|off.",
                    if yes {
                        "Додаю в каталог."
                    } else {
                        "Сервер не буде в каталозі."
                    },
                    cfg_path.display()
                ),
                Err(e) => eprintln!("Не вдалося зберегти відповідь: {e}"),
            }
        } else {
            println!(
                "ℹ  Сервер не в загальному каталозі гри. Щоб додати: \"public\": true у {} (або --public yes, або команда public on).",
                cfg_path.display()
            );
        }
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
    println!(" Версія:     {}", toon_shared::consts::GAME_VERSION);
    println!(" Введи help для списку команд.");
    println!("==============================================");
    if toon_shared::consts::RELEASE_BUILD {
        // a gentle reminder in the console when a newer release is out
        std::thread::spawn(|| {
            if let Ok(Some(r)) = toon_server::update::check("toon-server") {
                println!(
                    "ℹ  Вийшла нова версія v{} (у тебе v{}): зупини сервер і запусти toon-server --update",
                    r.version,
                    toon_shared::consts::GAME_VERSION
                );
            }
        });
    }

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

/// `--check-update` / `--update`: returns the exit code.
fn self_update(install: bool) -> i32 {
    use toon_server::update;
    let me = toon_shared::consts::GAME_VERSION;
    match update::check("toon-server") {
        Ok(None) => {
            println!("Це найновіша версія (v{me}).");
            0
        }
        Ok(Some(r)) if !install => {
            println!(
                "Вийшла нова версія v{} (у тебе v{me}). Онови: toon-server --update",
                r.version
            );
            0
        }
        Ok(Some(r)) => {
            println!(
                "Завантажую v{} ({:.1} МБ)…",
                r.version,
                r.size as f64 / 1_048_576.0
            );
            let mut shown = 0;
            match update::install(&r, |p| {
                let pct = (p * 10.0) as i32 * 10;
                if pct > shown {
                    shown = pct;
                    println!("  {pct}%");
                }
            }) {
                Ok(()) => {
                    println!(
                        "Готово: сервер оновлено до v{}. Перезапусти його.",
                        r.version
                    );
                    0
                }
                Err(e) => {
                    eprintln!("Не вдалося оновити: {e}");
                    1
                }
            }
        }
        Err(e) => {
            eprintln!("Не вдалося перевірити оновлення: {e}");
            1
        }
    }
}
