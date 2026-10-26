//! Admin commands — shared by the server console, the web panel and in-game `/rcon`.

use crate::game::Game;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toon_shared::map::MapId;
use toon_shared::protocol::S2C;
use toon_shared::{Mode, Team};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Ban {
    pub ip: String,
    pub name: String,
    pub reason: String,
}

#[derive(Default)]
pub struct Bans {
    pub list: Vec<Ban>,
    path: Option<PathBuf>,
}

impl Bans {
    pub fn load(dir: &str) -> Bans {
        let path = PathBuf::from(dir).join("bans.json");
        let list = std::fs::read_to_string(&path).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
        Bans { list, path: Some(path) }
    }
    pub fn save(&self) {
        if let Some(p) = &self.path {
            if let Some(dir) = p.parent() {
                let _ = std::fs::create_dir_all(dir);
            }
            if let Ok(t) = serde_json::to_string_pretty(&self.list) {
                if let Err(e) = std::fs::write(p, t) {
                    eprintln!("Не вдалося зберегти бани: {e}");
                }
            }
        }
    }
    pub fn find(&self, ip: &str) -> Option<&Ban> {
        self.list.iter().find(|b| b.ip == ip)
    }
}

pub const HELP: &str = "Команди:
  status                     — стан сервера і список гравців
  say <текст>                — повідомлення всім
  kick <id|ім'я> [причина]   — вигнати гравця
  ban <id|ім'я> [причина]    — заблокувати за IP
  unban <ip>                 — розблокувати
  banlist                    — список банів
  map <depot|market|cargo> [tdm|sab] — змінити карту (і режим)
  mode <tdm|sab>             — змінити режим на поточній карті
  nextmap                    — наступна карта з ротації
  rotation                   — показати ротацію
  restart                    — перезапустити матч
  endround                   — завершити матч зараз
  team <id|ім'я> <allies|axis|spec> — перевести гравця
  kill <id|ім'я>             — вбити гравця
  bots <n>                   — заповнювати сервер ботами до n гравців
  givestreak <id|ім'я> <uav|airstrike|heli> — видати кілстрік
  set <ключ> <значення>      — scorelimit, timelimit, sabtime, rounds, bombtimer, planttime,
                               defusetime, ff, autobalance, maxplayers, password, name, motd, botskill
  save                       — зберегти налаштування у файл
  help                       — ця довідка";

fn find_player(g: &Game, who: &str) -> Option<u8> {
    if let Ok(id) = who.trim_start_matches('#').parse::<u8>() {
        if g.p(id).is_some() {
            return Some(id);
        }
    }
    let low = who.to_lowercase();
    g.players.iter().flatten().find(|p| p.name.to_lowercase() == low).map(|p| p.id).or_else(|| {
        g.players.iter().flatten().find(|p| p.name.to_lowercase().contains(&low)).map(|p| p.id)
    })
}

fn parse_bool(v: &str) -> Option<bool> {
    match v.to_lowercase().as_str() {
        "1" | "on" | "true" | "так" | "yes" => Some(true),
        "0" | "off" | "false" | "ні" | "no" => Some(false),
        _ => None,
    }
}

pub fn status(g: &Game) -> String {
    let mut s = format!(
        "{} | карта: {} | режим: {} | стан: {:?} | рахунок {}:{} | гравців: {}/{}\n",
        g.cfg.name,
        g.map.name,
        g.mode.name(),
        g.state,
        g.scores[0],
        g.scores[1],
        g.players.iter().flatten().count(),
        g.cfg.max_players
    );
    s.push_str(" id  ім'я                  команда   рах.  K/D/A     пінг  IP\n");
    for p in g.players.iter().flatten() {
        s.push_str(&format!(
            " {:<3} {:<21} {:<9} {:<5} {}/{}/{:<5} {:<5} {}\n",
            p.id,
            p.name,
            p.team.name(),
            p.score,
            p.kills,
            p.deaths,
            p.assists,
            if p.is_bot() { "-".to_string() } else { format!("{}", (p.ping * 1000.0) as u32) },
            p.ip
        ));
    }
    s
}

/// Executes an admin command and returns the textual reply.
pub fn exec(g: &mut Game, line: &str) -> String {
    let line = line.trim();
    let (cmd, rest) = match line.split_once(char::is_whitespace) {
        Some((c, r)) => (c.to_lowercase(), r.trim()),
        None => (line.to_lowercase(), ""),
    };
    let mut args = rest.split_whitespace();
    match cmd.as_str() {
        "" => String::new(),
        "help" | "?" => HELP.into(),
        "status" | "players" => status(g),
        "say" => {
            if rest.is_empty() {
                return "Використання: say <текст>".into();
            }
            g.broadcast(S2C::Chat { from: "Адмін".into(), team: Team::Spectator, text: rest.into(), team_only: false });
            g.logf(format!("Адмін: {rest}"));
            "Надіслано".into()
        }
        "kick" | "ban" => {
            let Some(who) = args.next() else { return format!("Використання: {cmd} <id|ім'я> [причина]") };
            let reason: String = args.collect::<Vec<_>>().join(" ");
            let reason = if reason.is_empty() { "без причини".to_string() } else { reason };
            let Some(id) = find_player(g, who) else { return format!("Гравця «{who}» не знайдено") };
            let (name, ip, bot) = {
                let p = g.p(id).unwrap();
                (p.name.clone(), p.ip.clone(), p.is_bot())
            };
            if cmd == "ban" {
                if bot {
                    return "Ботів не можна банити — використай kick або bots".into();
                }
                g.bans.list.push(crate::admin::Ban { ip: ip.clone(), name: name.clone(), reason: reason.clone() });
                g.bans.save();
            }
            if bot {
                g.cfg.bots = g.cfg.bots.saturating_sub(1);
                g.remove_player(id, None);
            } else {
                g.kicked.push((id, format!("{} адміністратором: {reason}", if cmd == "ban" { "Заблоковано" } else { "Вигнано" })));
            }
            let msg = format!("{name} {} ({reason})", if cmd == "ban" { "заблоковано" } else { "вигнано" });
            g.broadcast(S2C::Chat { from: String::new(), team: Team::Spectator, text: msg.clone(), team_only: false });
            g.logf(msg.clone());
            msg
        }
        "unban" => {
            let Some(ip) = args.next() else { return "Використання: unban <ip>".into() };
            let before = g.bans.list.len();
            g.bans.list.retain(|b| b.ip != ip);
            g.bans.save();
            if g.bans.list.len() < before {
                format!("{ip} розблоковано")
            } else {
                format!("{ip} немає в списку банів")
            }
        }
        "banlist" => {
            if g.bans.list.is_empty() {
                "Список банів порожній".into()
            } else {
                g.bans.list.iter().map(|b| format!("{}  {}  ({})", b.ip, b.name, b.reason)).collect::<Vec<_>>().join("\n")
            }
        }
        "map" => {
            let Some(m) = args.next().and_then(MapId::parse) else {
                return "Використання: map <depot|market|cargo> [tdm|sab]".into();
            };
            let mode = args.next().and_then(Mode::parse).unwrap_or(g.mode);
            g.change_map(m, mode);
            format!("Карта змінена: {} ({})", g.map.name, mode.name())
        }
        "mode" => {
            let Some(mode) = args.next().and_then(Mode::parse) else { return "Використання: mode <tdm|sab>".into() };
            let m = g.map.id;
            g.change_map(m, mode);
            format!("Режим змінено: {}", mode.name())
        }
        "nextmap" => {
            g.next_map();
            format!("Наступна карта: {} ({})", g.map.name, g.mode.name())
        }
        "rotation" => g
            .cfg
            .rotation
            .iter()
            .enumerate()
            .map(|(i, r)| format!("{}{} {} {}", if i == g.rotation_idx { "> " } else { "  " }, i + 1, r.map, r.mode))
            .collect::<Vec<_>>()
            .join("\n"),
        "restart" => {
            let (m, mode) = (g.map.id, g.mode);
            g.change_map(m, mode);
            "Матч перезапущено".into()
        }
        "endround" | "endmatch" => {
            let w = if g.scores[0] > g.scores[1] {
                Some(Team::Allies)
            } else if g.scores[1] > g.scores[0] {
                Some(Team::Axis)
            } else {
                None
            };
            g.end_match(w, "Завершено адміністратором");
            "Матч завершено".into()
        }
        "team" => {
            let (Some(who), Some(t)) = (args.next(), args.next()) else {
                return "Використання: team <id|ім'я> <allies|axis|spec>".into();
            };
            let team = match t.to_lowercase().as_str() {
                "allies" | "a" | "1" => Team::Allies,
                "axis" | "b" | "2" => Team::Axis,
                "spec" | "spectator" | "0" => Team::Spectator,
                _ => return "Команда: allies | axis | spec".into(),
            };
            let Some(id) = find_player(g, who) else { return format!("Гравця «{who}» не знайдено") };
            g.set_team(id, team, true);
            "Готово".into()
        }
        "kill" => {
            let Some(who) = args.next() else { return "Використання: kill <id|ім'я>".into() };
            let Some(id) = find_player(g, who) else { return format!("Гравця «{who}» не знайдено") };
            g.kill(id, None, toon_shared::weapons::Cause::World, false);
            "Готово".into()
        }
        "bots" | "addbot" | "kickbots" => {
            let n = match cmd.as_str() {
                "addbot" => g.cfg.bots + args.next().and_then(|v| v.parse::<usize>().ok()).unwrap_or(1),
                "kickbots" => 0,
                _ => match args.next().and_then(|v| v.parse::<usize>().ok()) {
                    Some(n) => n,
                    None => return format!("Ботів: заповнення до {} гравців. Використання: bots <n>", g.cfg.bots),
                },
            };
            g.cfg.bots = n.min(g.cfg.max_players);
            if n == 0 {
                for id in g.ids() {
                    if g.p(id).map(|p| p.is_bot()).unwrap_or(false) {
                        g.remove_player(id, None);
                    }
                }
            }
            format!("Сервер заповнюватиметься ботами до {} гравців", g.cfg.bots)
        }
        "givestreak" => {
            let (Some(who), Some(k)) = (args.next(), args.next()) else {
                return "Використання: givestreak <id|ім'я> <uav|airstrike|heli>".into();
            };
            let streak = match k.to_lowercase().as_str() {
                "uav" => toon_shared::weapons::Killstreak::Uav,
                "airstrike" | "air" => toon_shared::weapons::Killstreak::Airstrike,
                "heli" | "helicopter" => toon_shared::weapons::Killstreak::Helicopter,
                _ => return "Кілстрік: uav | airstrike | heli".into(),
            };
            let Some(id) = find_player(g, who) else { return format!("Гравця «{who}» не знайдено") };
            if let Some(p) = g.pm(id) {
                p.rewards.retain(|r| *r != streak);
                p.rewards.push(streak);
            }
            g.send(id, S2C::StreakReady(streak));
            format!("Видано: {}", streak.name())
        }
        "set" => {
            let (Some(key), Some(_)) = (args.next(), rest.split_once(char::is_whitespace)) else {
                return "Використання: set <ключ> <значення>".into();
            };
            let value = rest.split_once(char::is_whitespace).map(|x| x.1.trim()).unwrap_or("");
            set_var(g, &key.to_lowercase(), value)
        }
        "save" => match &g.cfg_path {
            Some(p) => match g.cfg.save(p) {
                Ok(()) => format!("Збережено у {}", p.display()),
                Err(e) => format!("Помилка: {e}"),
            },
            None => "Сервер запущено без файлу конфігурації".into(),
        },
        _ => format!("Невідома команда «{cmd}». Напиши help"),
    }
}

fn set_var(g: &mut Game, key: &str, v: &str) -> String {
    let num = v.parse::<f32>();
    let bad = || format!("Невірне значення «{v}» для {key}");
    match key {
        "scorelimit" => match num {
            Ok(n) if n >= 1.0 => g.cfg.tdm.score_limit = n as i32,
            _ => return bad(),
        },
        "timelimit" => match num {
            Ok(n) if n >= 0.5 => g.cfg.tdm.time_limit = n,
            _ => return bad(),
        },
        "sabtime" => match num {
            Ok(n) if n >= 0.5 => g.cfg.sab.time_limit = n,
            _ => return bad(),
        },
        "rounds" => match num {
            Ok(n) if n >= 1.0 => g.cfg.sab.rounds_to_win = n as u8,
            _ => return bad(),
        },
        "bombtimer" => match num {
            Ok(n) if n >= 5.0 => g.cfg.sab.bomb_timer = n,
            _ => return bad(),
        },
        "planttime" => match num {
            Ok(n) if n >= 0.5 => g.cfg.sab.plant_time = n,
            _ => return bad(),
        },
        "defusetime" => match num {
            Ok(n) if n >= 0.5 => g.cfg.sab.defuse_time = n,
            _ => return bad(),
        },
        "botskill" => match num {
            Ok(n) => g.cfg.bot_skill = n.clamp(0.0, 1.0),
            _ => return bad(),
        },
        "maxplayers" => match num {
            Ok(n) if n >= 2.0 => g.cfg.max_players = (n as usize).min(toon_shared::consts::MAX_PLAYERS),
            _ => return bad(),
        },
        "ff" | "friendlyfire" => match parse_bool(v) {
            Some(b) => g.cfg.friendly_fire = b,
            None => return bad(),
        },
        "autobalance" => match parse_bool(v) {
            Some(b) => g.cfg.auto_balance = b,
            None => return bad(),
        },
        "password" => g.cfg.password = v.to_string(),
        "name" => g.cfg.name = v.to_string(),
        "motd" => g.cfg.motd = v.to_string(),
        _ => return format!("Невідомий ключ «{key}»"),
    }
    if key != "password" {
        g.center_all(format!("Налаштування змінено: {key} = {v}"), 3.0);
    }
    format!("{key} = {v}")
}
