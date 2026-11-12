//! Admin commands — shared by the server console, the web panel and in-game `/rcon`.

use crate::game::Game;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use toon_shared::map::MapId;
use toon_shared::protocol::S2C;
use toon_shared::tr;
use toon_shared::tuning::{
    FieldInfo, GrenadeTuning, MoveTuning, StreakTuning, Tuning, WeaponStats,
};
use toon_shared::weapons::WeaponId;
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
        let list = std::fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default();
        Bans {
            list,
            path: Some(path),
        }
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
  map <wreck|site|depot|market|cargo|island> [tdm|sab|br] — змінити карту (і режим)
  mode <tdm|sab|br>             — змінити режим на поточній карті
  nextmap                    — наступна карта з ротації
  rotation                   — показати ротацію
  restart                    — перезапустити матч
  endround                   — завершити матч зараз
  team <id|ім'я> <allies|axis|spec> — перевести гравця
  kill <id|ім'я>             — вбити гравця
  bots <n>                   — заповнювати ботами до n гравців (командні режими)
  brbots <n>                 — те саме для королівської битви
  givestreak <id|ім'я> <radar|airstrike|artillery|heli|fpv|bomber|nrk> — видати кілстрік
  givebomb <id|ім'я> — дати бомбу гравцю (Саботаж)
  tune [movement|weapons|<зброя>]           — показати параметри руху/зброї
  tune <шлях> <значення>     — змінити: tune movement.sprint_speed 8; tune m249.rpm 850
  tune reset [movement|<зброя>|all]         — повернути заводські значення
  set <ключ> <значення>      — scorelimit, timelimit, sabtime, rounds, bombtimer, planttime,
                               defusetime, ff, autobalance, maxplayers, brmaxplayers (королівська
                               битва), password, name, motd,
                               botskill (0..1 або easy/normal/hard/veteran), spawnprotect (секунди)
  save                       — зберегти налаштування у файл
  help                       — ця довідка";

fn find_player(g: &Game, who: &str) -> Option<u8> {
    if let Ok(id) = who.trim_start_matches('#').parse::<u8>() {
        if g.p(id).is_some() {
            return Some(id);
        }
    }
    let low = who.to_lowercase();
    g.players
        .iter()
        .flatten()
        .find(|p| p.name.to_lowercase() == low)
        .map(|p| p.id)
        .or_else(|| {
            g.players
                .iter()
                .flatten()
                .find(|p| p.name.to_lowercase().contains(&low))
                .map(|p| p.id)
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
        g.max_players()
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
            if p.is_bot() {
                "-".to_string()
            } else {
                format!("{}", (p.ping * 1000.0) as u32)
            },
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
            g.say(crate::game::Dest::All, || S2C::Chat {
                from: toon_shared::lang::t("Адмін", "Admin").into(),
                team: Team::Spectator,
                text: rest.into(),
                team_only: false,
            });
            g.logf(format!("Адмін: {rest}"));
            "Надіслано".into()
        }
        "kick" | "ban" => {
            let Some(who) = args.next() else {
                return format!("Використання: {cmd} <id|ім'я> [причина]");
            };
            let reason: String = args.collect::<Vec<_>>().join(" ");
            let reason = if reason.is_empty() {
                "без причини".to_string()
            } else {
                reason
            };
            let Some(id) = find_player(g, who) else {
                return format!("Гравця «{who}» не знайдено");
            };
            let (name, ip, bot) = {
                let p = g.p(id).unwrap();
                (p.name.clone(), p.ip.clone(), p.is_bot())
            };
            if cmd == "ban" {
                if bot {
                    return "Ботів не можна банити — використай kick або bots".into();
                }
                g.bans.list.push(crate::admin::Ban {
                    ip: ip.clone(),
                    name: name.clone(),
                    reason: reason.clone(),
                });
                g.bans.save();
            }
            if bot {
                let (bots, _) = g.limits_mut();
                *bots = bots.saturating_sub(1);
                g.remove_player(id, None);
            } else {
                g.kicked.push((
                    id,
                    format!(
                        "{} адміністратором: {reason}",
                        if cmd == "ban" {
                            "Заблоковано"
                        } else {
                            "Вигнано"
                        }
                    ),
                ));
            }
            let msg = format!(
                "{name} {} ({reason})",
                if cmd == "ban" {
                    "заблоковано"
                } else {
                    "вигнано"
                }
            );
            g.broadcast(S2C::Chat {
                from: String::new(),
                team: Team::Spectator,
                text: msg.clone(),
                team_only: false,
            });
            g.logf(msg.clone());
            msg
        }
        "unban" => {
            let Some(ip) = args.next() else {
                return "Використання: unban <ip>".into();
            };
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
                g.bans
                    .list
                    .iter()
                    .map(|b| format!("{}  {}  ({})", b.ip, b.name, b.reason))
                    .collect::<Vec<_>>()
                    .join("\n")
            }
        }
        "map" => {
            let Some(m) = args.next().and_then(MapId::parse) else {
                return "Використання: map <wreck|site|depot|market|cargo|island> [tdm|sab|br]"
                    .into();
            };
            let mode = args.next().and_then(Mode::parse).unwrap_or(g.mode);
            g.change_map(m, mode);
            format!("Карта змінена: {} ({})", g.map.name, mode.name())
        }
        "mode" => {
            let Some(mode) = args.next().and_then(Mode::parse) else {
                return "Використання: mode <tdm|sab|br>".into();
            };
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
            .map(|(i, r)| {
                format!(
                    "{}{} {} {}",
                    if i == g.rotation_idx { "> " } else { "  " },
                    i + 1,
                    r.map,
                    r.mode
                )
            })
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
            g.end_match(w, ("Завершено адміністратором", "Ended by an admin"));
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
            let Some(id) = find_player(g, who) else {
                return format!("Гравця «{who}» не знайдено");
            };
            g.set_team(id, team, true);
            "Готово".into()
        }
        "kill" => {
            let Some(who) = args.next() else {
                return "Використання: kill <id|ім'я>".into();
            };
            let Some(id) = find_player(g, who) else {
                return format!("Гравця «{who}» не знайдено");
            };
            g.kill(id, None, toon_shared::weapons::Cause::World, false);
            "Готово".into()
        }
        "brbots" => match args.next().and_then(|v| v.parse::<usize>().ok()) {
            Some(n) => {
                g.cfg.br.bots = n.min(g.cfg.br.max_players);
                format!(
                    "Королівська битва заповнюватиметься ботами до {} гравців",
                    g.cfg.br.bots
                )
            }
            None => format!(
                "Ботів у королівській битві: до {} гравців. Використання: brbots <n>",
                g.cfg.br.bots
            ),
        },
        "brsquad" => match args.next().and_then(|v| v.parse::<u8>().ok()) {
            Some(n) if [1, 2, 4].contains(&n) => {
                g.cfg.br.squad = n;
                format!("Загони в королівській битві: по {n} (з наступного раунду)")
            }
            _ => format!(
                "Загони в королівській битві: по {}. Використання: brsquad <1|2|4>",
                g.cfg.br.squad
            ),
        },
        "bots" | "addbot" | "kickbots" => {
            let n = match cmd.as_str() {
                "addbot" => {
                    g.bot_fill()
                        + args
                            .next()
                            .and_then(|v| v.parse::<usize>().ok())
                            .unwrap_or(1)
                }
                "kickbots" => 0,
                _ => match args.next().and_then(|v| v.parse::<usize>().ok()) {
                    Some(n) => n,
                    None => {
                        return format!(
                            "Ботів: заповнення до {} гравців. Використання: bots <n>",
                            g.bot_fill()
                        )
                    }
                },
            };
            // `bots` is the team modes' setting; adding or kicking acts on what is played
            let (bots, max) = if cmd == "bots" {
                (&mut g.cfg.bots, g.cfg.max_players)
            } else {
                let (b, m) = g.limits_mut();
                let m = *m;
                (b, m)
            };
            *bots = n.min(max);
            if n == 0 {
                for id in g.ids() {
                    if g.p(id).map(|p| p.is_bot()).unwrap_or(false) {
                        g.remove_player(id, None);
                    }
                }
            }
            format!(
                "Сервер заповнюватиметься ботами до {} гравців (зараз грається «{}»)",
                g.bot_fill(),
                g.mode.name()
            )
        }
        "givestreak" => {
            let (Some(who), Some(k)) = (args.next(), args.next()) else {
                return "Використання: givestreak <id|ім'я> <radar|airstrike|artillery|heli|fpv|bomber|nrk>".into();
            };
            let streak = match k.to_lowercase().as_str() {
                "uav" | "radar" => toon_shared::weapons::Killstreak::Uav,
                "airstrike" | "air" => toon_shared::weapons::Killstreak::Airstrike,
                "artillery" | "art" => toon_shared::weapons::Killstreak::Artillery,
                "heli" | "helicopter" => toon_shared::weapons::Killstreak::Helicopter,
                "fpv" => toon_shared::weapons::Killstreak::Fpv,
                "bomber" | "drone" => toon_shared::weapons::Killstreak::Bomber,
                "nrk" | "robot" => toon_shared::weapons::Killstreak::Nrk,
                _ => {
                    return "Кілстрік: radar | airstrike | artillery | heli | fpv | bomber | nrk"
                        .into()
                }
            };
            let Some(id) = find_player(g, who) else {
                return format!("Гравця «{who}» не знайдено");
            };
            if let Some(p) = g.pm(id) {
                p.rewards.retain(|r| *r != streak);
                p.rewards.push(streak);
            }
            g.send(id, S2C::StreakReady(streak));
            format!("Видано: {}", streak.name())
        }
        "givebomb" => {
            let Some(who) = args.next() else {
                return "Використання: givebomb <id|ім'я>".into();
            };
            if g.mode != toon_shared::Mode::Sab {
                return "Бомба є лише в режимі Саботаж".into();
            }
            if g.sab.planted_at.is_some() {
                return "Бомбу вже закладено".into();
            }
            let Some(id) = find_player(g, who) else {
                return format!("Гравця «{who}» не знайдено");
            };
            let Some((team, alive, pos)) = g.p(id).map(|p| (p.team, p.alive, p.body.pos)) else {
                return format!("Гравця «{who}» не знайдено");
            };
            if !alive || !team.is_playing() {
                return "Гравець має бути живим і в команді".into();
            }
            g.sab.carrier = Some(id);
            g.sab.owner = Some(team);
            g.sab.pos = pos;
            "Бомбу видано".into()
        }
        "tune" => tune(g, rest),
        "set" => {
            let (Some(key), Some(_)) = (args.next(), rest.split_once(char::is_whitespace)) else {
                return "Використання: set <ключ> <значення>".into();
            };
            let value = rest
                .split_once(char::is_whitespace)
                .map(|x| x.1.trim())
                .unwrap_or("");
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
        "botskill" => match crate::bots::parse_skill(v) {
            Some(n) => g.cfg.bot_skill = n,
            None => return "botskill: число 0..1 або easy | normal | hard | veteran".to_string(),
        },
        "spawnprotect" | "spawnprotection" => match num {
            Ok(n) if (0.0..=30.0).contains(&n) => g.cfg.spawn_protection = n,
            _ => return bad(),
        },
        "maxplayers" => match num {
            Ok(n) if n >= 2.0 => {
                g.cfg.max_players = (n as usize).min(toon_shared::consts::MAX_PLAYERS)
            }
            _ => return bad(),
        },
        "brmaxplayers" => match num {
            Ok(n) if n >= 2.0 => {
                g.cfg.br.max_players = (n as usize).min(toon_shared::consts::MAX_PLAYERS);
                g.cfg.br.bots = g.cfg.br.bots.min(g.cfg.br.max_players);
            }
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
        g.center_all(
            || {
                tr!(
                    "Налаштування змінено: {key} = {v}",
                    "Setting changed: {key} = {v}"
                )
            },
            3.0,
        );
    }
    format!("{key} = {v}")
}

fn fmt_val(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::Number(n) => {
            let f = n.as_f64().unwrap_or(0.0);
            if f.fract() == 0.0 {
                format!("{f:.0}")
            } else {
                format!("{f:.3}").trim_end_matches('0').to_string()
            }
        }
        other => other.to_string(),
    }
}

fn list_fields(obj: &serde_json::Value, fields: &[FieldInfo], prefix: &str) -> String {
    let mut out = String::new();
    for (k, label, lo, hi) in fields {
        let v = obj.get(*k).map(fmt_val).unwrap_or_default();
        out.push_str(&format!(
            "  {prefix}{k:<18} = {v:<8} {label}  [{lo}..{hi}]\n"
        ));
    }
    out
}

/// `tune` admin command: inspect and change movement / weapon tuning live.
pub fn tune(g: &mut Game, rest: &str) -> String {
    let mut args = rest.split_whitespace();
    let Some(first) = args.next() else {
        return "Тонке налаштування:\n  tune movement — параметри руху\n  tune grenade — гранати (дальність кидка тощо)\n  tune streaks — кілстріки (пороги вбивств, урон, тривалість…)\n  tune weapons — список зброї\n  tune <зброя> — параметри зброї (напр. tune m249)\n  tune <шлях> <значення> — tune movement.sprint_speed 8, tune m249.weight 10\n  tune reset [movement|grenade|streaks|<зброя>|all]".into();
    };
    let json = serde_json::to_value(&g.tuning).unwrap_or_default();
    let first_l = first.to_lowercase();
    if first_l == "reset" {
        let what = args.next().unwrap_or("all").to_lowercase();
        let def = Tuning::default();
        let mut t = g.tuning.clone();
        match what.as_str() {
            "all" => t = def,
            "movement" | "move" => t.movement = def.movement,
            "grenade" | "grenades" => t.grenade = def.grenade,
            "streaks" | "streak" | "killstreaks" => t.streaks = def.streaks,
            w => match WeaponId::from_key(w) {
                Some(id) => {
                    t.weapons.insert(id.key().into(), def.weapon(id).clone());
                }
                None => return format!("Невідомо, що скидати: «{w}»"),
            },
        }
        g.set_tuning(t);
        return format!("Скинуто до заводських: {what}");
    }
    let Some(value) = args.next() else {
        // show
        return match first_l.as_str() {
            "movement" | "move" => format!("Рух (tune movement.<ключ> <значення>):\n{}", list_fields(&json["movement"], MoveTuning::FIELDS, "")),
            "grenade" | "grenades" => format!(
                "Гранати (tune grenade.<ключ> <значення>), макс. дальність кидка під 45° ≈ {:.0} м:\n{}",
                g.tuning.grenade.max_range(g.tuning.movement.gravity),
                list_fields(&json["grenade"], GrenadeTuning::FIELDS, "")
            ),
            "streaks" | "streak" | "killstreaks" => format!(
                "Кілстріки (tune streaks.<ключ> <значення>); нагороди йдуть по колу:\n{}",
                list_fields(&json["streaks"], StreakTuning::FIELDS, "")
            ),
            "weapons" => WeaponId::all()
                .map(|w| {
                    let s = g.tuning.weapon(w);
                    format!("  {:<7} {:<13} {:>5.1} кг  {:>4.0} постр./хв  рух ×{:.2}  ADS {:.2} с", w.key(), w.def().name, s.weight, s.rpm, s.move_mult, s.ads_time)
                })
                .collect::<Vec<_>>()
                .join("\n"),
            k => match WeaponId::from_key(k) {
                Some(w) => format!("{} (tune {}.<ключ> <значення>):\n{}", w.def().name, w.key(), list_fields(&json["weapons"][w.key()], WeaponStats::FIELDS, "")),
                None => format!("Невідома група «{k}». Напиши tune"),
            },
        };
    };
    // set: path forms movement.x | <weapon>.x | weapons.<weapon>.x | x (movement shorthand)
    let parts: Vec<&str> = first_l.split('.').collect();
    let (path, fields): (Vec<String>, &[FieldInfo]) = match parts.as_slice() {
        ["movement" | "move", f] => (vec!["movement".into(), f.to_string()], MoveTuning::FIELDS),
        ["grenade" | "grenades", f] => (vec!["grenade".into(), f.to_string()], GrenadeTuning::FIELDS),
        ["streaks" | "streak" | "killstreaks", f] => (vec!["streaks".into(), f.to_string()], StreakTuning::FIELDS),
        ["weapons", w, f] | [w, f] if WeaponId::from_key(w).is_some() => {
            (vec!["weapons".into(), WeaponId::from_key(w).unwrap().key().into(), f.to_string()], WeaponStats::FIELDS)
        }
        [f] if MoveTuning::FIELDS.iter().any(|x| x.0 == *f) => (vec!["movement".into(), f.to_string()], MoveTuning::FIELDS),
        _ => return format!("Невірний шлях «{first}». Приклади: movement.sprint_speed, m249.rpm, grenade.throw_speed, streaks.heli_damage"),
    };
    let field = path.last().unwrap().clone();
    let Some(info) = fields.iter().find(|x| x.0 == field) else {
        return format!(
            "Невідомий параметр «{field}». Список: tune {}",
            if path[0] == "weapons" {
                &path[1]
            } else {
                &path[0]
            }
        );
    };
    let mut json = json;
    let slot = path.iter().fold(&mut json, |v, k| &mut v[k.as_str()]);
    let new = if slot.is_boolean() {
        match parse_bool(value) {
            Some(b) => serde_json::Value::Bool(b),
            None => return format!("Очікується true/false для {field}"),
        }
    } else {
        match value.replace(',', ".").parse::<f64>() {
            Ok(n) if n.is_finite() => {
                if slot.is_u64() {
                    serde_json::json!(n.max(0.0).round() as u64)
                } else {
                    serde_json::json!(n)
                }
            }
            _ => return format!("Очікується число для {field}"),
        }
    };
    *slot = new;
    let t: Tuning = match serde_json::from_value(json) {
        Ok(t) => t,
        Err(e) => return format!("Помилка: {e}"),
    };
    g.set_tuning(t);
    let after = serde_json::to_value(&g.tuning).unwrap_or_default();
    let applied = path.iter().fold(&after, |v, k| &v[k.as_str()]);
    let clamped = match (applied.as_f64(), value.replace(',', ".").parse::<f64>()) {
        (Some(a), Ok(v)) => (a - v).abs() > 1e-3 * v.abs().max(1.0),
        _ => false,
    };
    let note = if clamped {
        format!(" (обмежено діапазоном {}..{})", info.2, info.3)
    } else {
        String::new()
    };
    let msg = format!("{} = {}{note}", path.join("."), fmt_val(applied));
    g.logf(format!("Тюнінг: {msg}"));
    msg
}
