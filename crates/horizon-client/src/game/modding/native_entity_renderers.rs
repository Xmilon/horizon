//! Entity types drawn by mods' renderers on the client: `native_register_entity_renderers`
//! (NeoForge's `EntityRenderersEvent.RegisterRenderers`) is asked once at start, before resources
//! load, and every resource load resolves the registrations into the slots the draw routes read, so
//! a frame hands an entity to its type's renderer without a lookup by name or a scan over the mods.
//! The renderer draws through the shared drawing boundary (`BlockEntityDrawContext`) for native and
//! wasm mods alike.
//!
//! The routes that read the table so far: the content mods' thrown types
//! (`ProjectileKind::Thrown`, by [`ThrownType::slot`]). Each other entity family becomes a route of
//! its own; until then a registration for one of its types is kept and the type keeps its drawing.
//! Each load also asks for the layers of the living renderers (`native_entity_layers`), which draw
//! through the same guard ([`Game::guarded_entity_draw`]).
use super::{
    Game, host::draw::GameBlockEntityDrawContext, native_block_entity_draw::BlockEntityDraw,
};
use glam::{DVec3, Mat4};
use horizon_client::{
    block_entity_render::diffuse_lights, mesher::Vertex, projectiles::ThrownType,
};
use mod_api::{BlockEntityDrawContext, EntityRenderState, EntityRenderer};
use std::{
    collections::BTreeMap,
    sync::{
        Arc, OnceLock, PoisonError, RwLock,
        atomic::{AtomicBool, Ordering},
    },
};

/// The registrations, by entity type, the last one for a type winning: kept once at start.
static REGISTERED: OnceLock<BTreeMap<String, Arc<dyn EntityRenderer>>> = OnceLock::new();

/// The registrations when no mod registered any (and before the start registered them).
static NO_RENDERERS: BTreeMap<String, Arc<dyn EntityRenderer>> = BTreeMap::new();

/// The table the last resource load resolved; empty before the first.
static TABLE: RwLock<EntityRendererTable> = RwLock::new(EntityRendererTable { thrown: Vec::new() });

/// Keeps the mods' registrations, once per process (false when they already were).
pub(super) fn register(renderers: BTreeMap<String, Arc<dyn EntityRenderer>>) -> bool {
    REGISTERED.set(renderers).is_ok()
}

/// A mod's renderer or layer as a load resolved it, and whether it has failed (then it is asked no
/// more until the next resource load resolves it again).
pub(in crate::game) struct Resolved<T: ?Sized> {
    pub(super) inner: Arc<T>,
    pub(super) failed: AtomicBool,
}

impl<T: ?Sized> Resolved<T> {
    pub(super) fn new(inner: Arc<T>) -> Self {
        Self {
            inner,
            failed: AtomicBool::new(false),
        }
    }
}

/// The registered renderers, by the slot each draw route reads: rebuilt on every resource load.
pub(super) struct EntityRendererTable {
    /// By [`ThrownType::slot`].
    thrown: Vec<Option<Arc<Resolved<dyn EntityRenderer>>>>,
}

impl EntityRendererTable {
    /// The table for `registered`, with `thrown` the content mods' thrown types and `known`
    /// whether an id is any entity type; and the ids of the registrations for no entity type,
    /// which it leaves out. A registration for a known type no route reads yet is kept silently:
    /// the type keeps its own drawing.
    pub(super) fn resolve(
        registered: &BTreeMap<String, Arc<dyn EntityRenderer>>,
        thrown: &[ThrownType],
        known: impl Fn(&str) -> bool,
    ) -> (Self, Vec<String>) {
        let mut table = Self {
            thrown: thrown.iter().map(|_| None).collect(),
        };
        let mut unknown = Vec::new();
        for (id, renderer) in registered {
            if let Some(t) = thrown.iter().find(|t| t.id == id) {
                table.thrown[t.slot] = Some(Arc::new(Resolved::new(renderer.clone())));
            } else if !known(id) {
                unknown.push(id.clone());
            }
        }
        (table, unknown)
    }

    /// The renderer of the thrown type `t`, when a mod registered one.
    pub(super) fn thrown(&self, t: &ThrownType) -> Option<&Arc<Resolved<dyn EntityRenderer>>> {
        self.thrown.get(t.slot)?.as_ref()
    }
}

/// The renderer the last resource load resolved for the thrown type `t`, when a mod registered one.
pub(in crate::game) fn thrown_renderer(
    t: &ThrownType,
) -> Option<Arc<Resolved<dyn EntityRenderer>>> {
    let table = TABLE.read().unwrap_or_else(PoisonError::into_inner);
    table.thrown(t).cloned()
}

impl Game<'_> {
    /// Resolves the registered renderers against the entity types of the resources now loaded, then
    /// asks for the living renderers' layers: at the first load and on every reload after it. A
    /// registration for no entity type is logged with its id.
    pub(in crate::game) fn resolve_entity_renderers(&self) {
        let registered = REGISTERED.get().unwrap_or(&NO_RENDERERS);
        let rows = super::entity_type_rows::entity_type_rows(&self.options.resources);
        let (table, unknown) = EntityRendererTable::resolve(
            registered,
            horizon_client::projectiles::thrown_types(),
            |id| rows.contains(id),
        );
        for id in unknown {
            log::warn!(
                "Horizon: an entity renderer is registered for {id}, which is no entity type; it is left out"
            );
        }
        *TABLE.write().unwrap_or_else(PoisonError::into_inner) = table;
        self.resolve_entity_layers(&rows, registered);
    }

    /// Draws `entity` with `resolved`'s renderer, at `at` relative to the camera (`block` the
    /// block it stands in), lit by `light` as the entity pass packs it: true when it drew, and the
    /// type's own drawing is left out. False, with nothing it drew kept, when it fails this frame
    /// and from then until the next resource load resolves it again.
    pub(in crate::game) fn mod_entity_renderer_geometry(
        &self,
        resolved: &Resolved<dyn EntityRenderer>,
        entity: &EntityRenderState,
        at: DVec3,
        block: [i32; 3],
        light: u32,
        out: &mut Vec<Vertex>,
    ) -> bool {
        let pose = Mat4::from_translation(at.as_vec3());
        self.guarded_entity_draw(
            &resolved.failed,
            entity.partial,
            block,
            pose,
            light,
            out,
            |draw| resolved.inner.submit(entity, draw),
        )
    }

    /// Runs one mod renderer's or layer's `submit` on a draw posed at `pose` (`block` the block the
    /// entity stands in, lit by `light`), adding to `out`: true when it drew. An `Err`, a panic and
    /// a refused or trapped wasm call (an `Err` from the proxy) all drop what it drew this frame,
    /// log once and set `failed`, after which it is asked no more until the next resource load
    /// resolves it again.
    #[allow(clippy::too_many_arguments)]
    pub(in crate::game) fn guarded_entity_draw(
        &self,
        failed: &AtomicBool,
        partial: f32,
        block: [i32; 3],
        pose: Mat4,
        light: u32,
        out: &mut Vec<Vertex>,
        submit: impl FnOnce(&mut dyn BlockEntityDrawContext) -> Result<(), String>,
    ) -> bool {
        if failed.load(Ordering::Relaxed) {
            return false;
        }
        let start = out.len();
        let mut surface = BlockEntityDraw::posed(
            partial,
            self.world.game_time,
            light,
            block,
            pose,
            diffuse_lights(self.nether_lighting()),
            std::mem::take(out),
        )
        .animated(self.animation_time());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            submit(&mut GameBlockEntityDrawContext {
                game: self,
                surface: &mut surface,
            })
        }))
        .unwrap_or_else(|_| Err("it panicked".into()));
        *out = surface.into_vertices();
        let Err(error) = result else {
            return true;
        };
        out.truncate(start);
        if !failed.swap(true, Ordering::Relaxed) {
            log::warn!(
                "Horizon: an entity renderer or layer failed ({error}); it is asked no more until resources reload"
            );
        }
        false
    }
}
