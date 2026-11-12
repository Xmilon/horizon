//! A player's reach and flight as NeoForge's `IPlayerExtension` asks them (#3349): whether an
//! entity is close enough to interact with, and whether the player may fly.
use super::Game;
use horizon_core::geometry::{Aabb, Vec3};
use mod_api::{EntityRef, PlayerRef};

impl Game<'_> {
    /// `IPlayerExtension.isCloseEnough(entity, distance)`: the point where the ray from the
    /// player's eyes to the middle of the entity's box enters the box lies within `distance` of
    /// the eyes; when the ray misses (the eyes inside the box), the player's position is within
    /// `distance` of the entity's. None for a stale player or entity, one without a box, or one
    /// in another level.
    pub(super) fn player_is_close_enough(
        &self,
        player: PlayerRef,
        entity: EntityRef,
        distance: f64,
    ) -> Option<bool> {
        let me = player.entity();
        if self.entity_level(me)? != self.entity_level(entity)? {
            return None;
        }
        let [lo, hi] = self.entity_bounding_box(entity)?;
        let [x, y, z] = self.entity_position(entity)?;
        let v = |[x, y, z]: [f64; 3]| Vec3::new(x, y, z);
        let eye = v(self.entity_eye_position(me)?);
        // `entity.position().add(0, getBbHeight() / 2, 0)`.
        let centre = Vec3::new(x, y + (hi[1] - lo[1]) / 2.0, z);
        let from = match Aabb::new(v(lo), v(hi)).clip(eye, centre) {
            Some(hit) => (eye, hit),
            None => (v(self.entity_position(me)?), Vec3::new(x, y, z)),
        };
        let d2: f64 = from
            .0
            .vector_to(from.1)
            .components()
            .iter()
            .map(|d| d * d)
            .sum();
        Some(d2 < distance * distance)
    }

    /// `IPlayerExtension.mayFly`: whether the player may start flying, by game mode or ability.
    /// Horizon has no creative-flight attribute yet, so this is the `mayfly` ability. None for a
    /// stale or unavailable player.
    pub(super) fn player_may_fly(&self, player: PlayerRef) -> Option<bool> {
        Some(self.player_abilities(player)?.may_fly)
    }
}

#[cfg(test)]
mod tests {
    use crate::game::native_guest_turn_tests::with_guests;
    use horizon_client::mobs::Mob;
    use horizon_core::geometry::Vec3;

    /// The eye-to-box rule: a guest's eyes at (3, 65.62, 5) and a pig level with them on +x,
    /// whose near face is 2.9 blocks away, is close enough at 3; at 3.1 it is not. A player
    /// in survival may not fly until the `mayfly` ability is set.
    #[test]
    #[ignore = "needs prepared resources in HORIZON_TEST_RESOURCES"]
    fn reach_runs_from_the_eyes_to_the_box_and_flight_follows_the_ability() {
        with_guests(|game| {
            let kind = game.res.mob_types().get("minecraft:pig").unwrap().clone();
            let eye_y = 64.0 + 1.62;
            // The pig's box is 0.9 wide and tall: its middle is level with the eyes.
            let feet = |x: f64| Vec3::new(x, eye_y - 0.45, 5.0);
            game.mobs
                .push(Mob::new(900, kind.clone(), feet(3.0 + 2.9 + 0.45), 0.0));
            game.mobs
                .push(Mob::new(901, kind, feet(3.0 + 3.1 + 0.45), 0.0));
            let player = game.player(12).unwrap();
            let (near, far) = (game.entity(900).unwrap(), game.entity(901).unwrap());
            assert_eq!(game.player_is_close_enough(player, near, 3.0), Some(true));
            assert_eq!(game.player_is_close_enough(player, far, 3.0), Some(false));

            assert_eq!(game.player_may_fly(player), Some(false));
            let mut abilities = game.player_abilities(player).unwrap();
            abilities.may_fly = true;
            assert!(game.player_set_abilities(player, abilities));
            assert_eq!(game.player_may_fly(player), Some(true));
        });
    }
}
