use super::*;

impl Game<'_> {
    /// Player.drop: thrown from the eyes along the view direction. True when
    /// the selected stack's `on_dropped_by_player` Part kept it in the slot
    /// (`ServerPlayer.drop(boolean)`, as NeoForge asks it).
    pub(in crate::game) fn drop_held(&mut self, all: bool) -> bool {
        if self.eye_position().is_none() {
            return false;
        }
        let spectator = self.spectator();
        if !spectator && self.native_dropped_by_player_event(all) {
            return true;
        }
        let Some(thrown) = horizon_client::items::take_held_drop(
            &mut self.inventory[self.selected],
            all,
            spectator,
        ) else {
            return false;
        };
        self.player_drop(thrown);
        false
    }

    /// A guest's drop key. A drop the held item's Part refused sends the
    /// inventory back, undoing the removal the client predicted.
    pub(in crate::game) fn guest_drop_held(&mut self, gi: usize, conn: u32, all: bool) {
        let kept = self.as_guest(conn, |g| g.drop_held(all)) == Some(true);
        if let Some(host) = self.lan.as_mut() {
            if kept {
                host.hub.send_inventory(gi);
            } else {
                host.hub.guests.update(gi, |g| g.told_selected());
            }
        }
    }
}
