//! The falling block's `Entity.move` (`FallingBlockEntity.tick`'s step, #2414's
//! move gate), in its own module so `game.rs` keeps its size.
use super::*;
impl Game<'_> {
    /// One `FallingBlockEntity.tick` move of `f`.
    pub(super) fn fall_step(
        &mut self,
        f: &mut FallingBlock,
        entity_drops: bool,
        entities: &[Aabb],
    ) -> Result<FallStep> {
        if self.res.facts().is_air(f.state) {
            return Ok(FallStep::Gone);
        }
        f.previous = f.position;
        f.age = f.age.wrapping_add(1);
        f.velocity.y -= horizon_client::entity_rules::EntityRules::get_gravity(f);
        let wanted = f.velocity;
        // The falling block's Entity.move goes through the move gate (#2414),
        // armed before the stuck multiplier of the move itself; PR 1a records
        // the movement unchanged.
        let mut gate = horizon_client::move_gate::armed(
            &self.loop_context,
            f.id,
            "minecraft:falling_block",
            MoverType::SelfMovement,
            wanted,
        );
        let mut wanted = gate.delta();
        // Entity.move: a cobweb it was in scales this move, which then
        // starts from rest.
        if f.stuck.length_squared() > 1.0e-7 {
            wanted = wanted.multiply(f.stuck);
            f.stuck = Vec3::ZERO;
            f.velocity = Vec3::ZERO;
        }
        let scene = collisions::SceneCollisions {
            world: &self.world,
            binding: self.collision,
            subject: Some(collisions::PowderSnowSubject {
                fall_distance: f.fall_distance,
                falling_block: true,
                ..Default::default()
            }),
        };
        // Entity.isDescending: only sneaking entities descend through
        // scaffolding. Falling blocks do not sneak, so descending is always
        // false (Entity.isDescending returns isShiftKeyDown, fixes #7553).
        let context = CollisionContext::for_entity(
            CollisionEntity {
                identity: EntityIdentity(0),
                kind: horizon_world::collision_context::EntityContextKind::Other,
                y: f.position.y,
                descending: false,
                main_hand: None,
            },
            false,
        );
        // On an error the caller keeps the block as it was before this tick.
        // Entity.collide: the world border's walls and the entities that
        // `canBeCollidedWith` it (a boat, a shulker) stop it too.
        let mut collisions = collisions::EntityCollisions {
            blocks: scene,
            entities,
            border: Some((self.border.geometry(), [f.position.x, f.position.z])),
        };
        let movement = horizon_client::move_gate::entities_resolve_with_context(
            &mut gate,
            &mut collisions,
            horizon_client::falling_block::bounds(f.position),
            wanted,
            0.0,
            false,
            context,
        )?;
        let next = Vec3::new(
            f.position.x + movement.x,
            f.position.y + movement.y,
            f.position.z + movement.z,
        );
        let on_ground = wanted.y < 0.0 && movement.y != wanted.y;
        f.on_ground = on_ground;
        if movement.y < 0.0 {
            f.fall_distance -= f64::from(movement.y as f32);
        }
        let from = f.position;
        f.position = next;
        // Entity.move stops it on the axes it collided on.
        if movement.x != wanted.x {
            f.velocity.x = 0.0;
        }
        if movement.y != wanted.y {
            f.velocity.y = 0.0;
        }
        if movement.z != wanted.z {
            f.velocity.z = 0.0;
        }
        if on_ground {
            self.falling_block_impact(f);
            if f.fall_distance > 0.0 {
                self.post_landing_event(
                    f.position,
                    horizon_client::falling_block::bounds(f.position),
                    false,
                );
            }
            f.fall_distance = 0.0;
        }
        // applyEffectsFromBlocks: WebBlock.entityInside makes it stuck
        // (Entity.makeStuckInBlock), BubbleColumnBlock.entityInside lifts or
        // drags it. Entity.move records the move unless it was blocked short
        // of a tiny one.
        let moved = movement.length_squared();
        let recorded = moved > 1.0e-7 || wanted.length_squared() - moved < 1.0e-7;
        let world = &self.world;
        let facts = self.res.facts();
        let (registry, collision) = (self.pack.assets.registry, self.collision);
        let mut tops = Vec::new();
        let stuck = horizon_client::falling_block::apply_effects_from_blocks(
            from,
            f.position,
            recorded.then_some(wanted),
            &mut f.velocity,
            &mut f.fall_distance,
            &mut tops,
            |p| {
                use horizon_client::falling_block::Inside;
                if &*facts.get(world.block(p.x, p.y, p.z)).name == "minecraft:cobweb" {
                    Inside::Cobweb
                } else if let Some(column) =
                    horizon_client::motion::bubble_column_at(world, registry, facts, collision, p)
                {
                    Inside::BubbleColumn(column)
                } else {
                    Inside::Nothing
                }
            },
        );
        if let Some(multiplier) = stuck {
            f.stuck = multiplier;
        }
        for pos in tops {
            self.bubble_column_particles(pos);
        }
        self.entity_powder_snow_particles(
            from,
            f.position,
            horizon_client::falling_block::bounds(f.position),
            f.velocity,
            false,
        );
        // FallingBlockEntity.tick handles its portal after the move's block
        // effects, before placing/dropping on the floor under an End portal.
        let teleported = self.falling_through_portal(f);
        // FallingBlockEntity.forceTickAfterTeleportToDuplicate: crossing
        // either way through the End deliberately keeps the source's final
        // placement/drop branch, even though the source entity was removed.
        if let Some(target) = teleported
            && self.dim != Dim::End
            && target != Dim::End
        {
            return Ok(FallStep::Gone);
        }
        let y = next.y.floor() as i32;
        let outside = y <= self.world.bottom_y() || y >= self.world.top_y();
        if !on_ground && (f.age > 100 && outside || f.age > 600) {
            return Ok(if f.drop_item && entity_drops {
                FallStep::Dropped(next)
            } else {
                FallStep::Gone
            });
        }
        if on_ground {
            return Ok(FallStep::Landed([
                f.position.x.floor() as i32,
                f.position.y.floor() as i32,
                f.position.z.floor() as i32,
            ]));
        }
        // Entity.scale(getAirDrag()): 0.98F, widened before it scales motion.
        f.velocity = horizon_client::entity_rules::scale_air_drag(f.velocity);
        Ok(if teleported.is_some() {
            FallStep::Gone
        } else {
            FallStep::Falling
        })
    }
}
