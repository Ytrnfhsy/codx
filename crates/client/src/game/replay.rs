//! Killcams and the summary between matches.

use super::*;

impl GameClient {
    /// After a match or round ends, everybody watches the last kill again in slow motion.
    pub(super) fn queue_final_killcam(&mut self, now: f64) {
        let since = self.server_now() - 6.0;
        let Some((_, killer, victim, cause, headshot, gear)) = self.recorder.last_kill(since)
        else {
            return;
        };
        let name = |id: u8| -> String {
            self.scoreboard
                .iter()
                .find(|r| r.id == id)
                .map(|r| r.name.clone())
                .unwrap_or_else(|| "?".into())
        };
        let (killer_name, victim_name) = (name(killer), name(victim));
        let killer_team = self
            .scoreboard
            .iter()
            .find(|r| r.id == killer)
            .map(|r| r.team)
            .unwrap_or(Team::Spectator);
        self.killcam = None;
        self.pending_killcam = Some((
            now + FINAL_KILLCAM_DELAY,
            KillInfo {
                killer,
                victim,
                killer_name,
                victim_name,
                killer_team,
                cause,
                headshot,
                gear,
            },
            true,
        ));
    }

    pub(super) fn start_killcam(&mut self, info: KillInfo, final_cam: bool) {
        // the replay ends at the moment the victim went down
        let since = self.server_now() - 8.0;
        let Some(t_kill) = self.recorder.death_time(info.victim, since) else {
            return;
        };
        self.killcam = Killcam::new(&self.recorder, info, t_kill, final_cam);
    }

    /// The results screen is up: the match is over and its final kill has been shown.
    pub fn summary_active(&self) -> bool {
        self.state == MatchState::PostMatch
            && self.summary.is_some()
            && !self.final_killcam_waiting()
            && !self.killcam.as_ref().is_some_and(|k| k.final_cam)
    }

    /// The final killcam (end of a round or match) is about to play.
    pub fn final_killcam_waiting(&self) -> bool {
        self.pending_killcam.as_ref().is_some_and(|p| p.2)
    }

    /// Skips the killcam that is playing or about to play (the final one cannot be
    /// skipped). Returns whether there was one.
    pub fn skip_killcam(&mut self) -> bool {
        let skippable = self.killcam.as_ref().is_some_and(|k| !k.final_cam)
            || self.pending_killcam.as_ref().is_some_and(|p| !p.2);
        if skippable {
            self.killcam = None;
            self.pending_killcam = None;
        }
        skippable
    }
}
