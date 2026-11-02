//! Each player gets the server's messages in their own language.

use toon_server::config::ServerConfig;
use toon_server::game::{Dest, Game};
use toon_shared::consts::{GAME_VERSION, PROTOCOL_VERSION};
use toon_shared::lang::Lang;
use toon_shared::protocol::S2C;
use toon_shared::Team;

#[test]
fn messages_come_in_each_players_language() {
    let mut g = Game::new(ServerConfig {
        bots: 0,
        data_dir: std::env::temp_dir()
            .join("tw-test-lang")
            .to_string_lossy()
            .into(),
        ..Default::default()
    });
    let mut join = |port: u16, name: &str, lang: Lang| {
        g.connect_as(
            format!("127.0.0.1:{port}").parse().unwrap(),
            name,
            "",
            PROTOCOL_VERSION,
            GAME_VERSION,
            lang,
        )
        .unwrap()
    };
    let uk = join(7001, "Тарас", Lang::Uk);
    let en = join(7002, "John", Lang::En);
    g.outbox.clear();
    g.end_match(Some(Team::Allies), ("Час вийшов", "Time is up"));
    let text_for = |id: u8| {
        g.outbox.iter().find_map(|(d, m, _)| match (d, m) {
            (Dest::One(to), S2C::MatchEnd { text, .. }) if *to == id => Some(text.clone()),
            _ => None,
        })
    };
    assert_eq!(text_for(uk).unwrap(), "Морпіхи перемагають! Час вийшов");
    assert_eq!(text_for(en).unwrap(), "Marines win! Time is up");

    // a refusal is in the newcomer's language too
    let err = g
        .connect_as(
            "127.0.0.1:7003".parse().unwrap(),
            "Old",
            "",
            PROTOCOL_VERSION - 1,
            "0.0.1",
            Lang::En,
        )
        .unwrap_err();
    assert!(err.starts_with("Game version mismatch."), "{err}");
}
