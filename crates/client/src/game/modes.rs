//! Mode-specific bits: the battle royale lobby, Sabotage overtime, classes, killstreaks.

use super::*;

impl GameClient {
    /// Waiting in the battle royale lobby (playing, the match not started yet).
    pub fn in_br_lobby(&self) -> bool {
        self.mode == Mode::Br && self.team.is_playing() && self.lobby.is_some() && !self.alive
    }

    /// Who stands in the lobby: we first, then our squad (look id, character).
    pub fn lobby_cast(&self, my_skin: u8) -> Vec<(u8, u8)> {
        let Some(l) = &self.lobby else {
            return Vec::new();
        };
        let skin = |id: u8| l.skins.iter().find(|s| s.0 == id).map_or(0, |s| s.1);
        let mut cast = vec![(self.my_id, my_skin)];
        if let Some(g) = l.parties.iter().find(|g| g.contains(&self.my_id)) {
            cast.extend(
                g.iter()
                    .filter(|&&id| id != self.my_id)
                    .map(|&id| (id, skin(id))),
            );
        }
        cast
    }

    /// Sabotage: seconds left before respawns end (shown in the last ten), unless the bomb
    /// is down and the clock stands.
    pub fn overtime_in(&self) -> Option<f32> {
        let planted = self.bomb.as_ref().is_some_and(|b| b.planted_at.is_some());
        (self.mode == Mode::Sab
            && self.state == MatchState::Playing
            && !self.overtime
            && !planted
            && self.time_left > 0.0
            && self.time_left <= 10.0)
            .then_some(self.time_left)
    }

    /// The countdown's ticks and the siren when respawns end.
    pub(super) fn overtime_cues(&mut self, audio: Option<&Audio>) {
        if let Some(left) = self.overtime_in() {
            let n = left.ceil() as i32;
            if n != self.overtime_tick {
                self.overtime_tick = n;
                if let Some(a) = audio {
                    a.play(if n <= 3 { Sfx::TickHigh } else { Sfx::Tick }, 0.8);
                }
            }
        } else {
            self.overtime_tick = 0;
        }
        if self.overtime && !self.was_overtime && self.mode == Mode::Sab {
            self.overtime_at = self.now;
            if let Some(a) = audio {
                a.play(Sfx::Siren, 0.85);
            }
        }
        self.was_overtime = self.overtime;
    }

    pub fn choose_class(&mut self, loadout: Loadout) {
        self.net.send(C2S::SetLoadout(loadout));
        self.has_class = true;
        self.overlay = Overlay::None;
        if self.alive {
            self.center_msg(
                toon_shared::lang::t(
                    "Нове спорядження — після відродження",
                    "Your new loadout comes when you respawn",
                ),
                2.0,
                crate::ui::MUTED,
            );
        }
    }

    /// Starts the ground robot, steered or on its own.
    pub fn call_robot(&mut self, pilot: bool) {
        self.net.send(C2S::UseKillstreak {
            target: None,
            pilot,
        });
        self.overlay = Overlay::None;
    }

    pub fn call_airstrike(&mut self, x: f32, z: f32) {
        self.net.send(C2S::UseKillstreak {
            target: Some([x, z]),
            pilot: false,
        });
        self.overlay = Overlay::None;
    }
}
