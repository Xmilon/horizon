//! Bridge the public SDK to the existing native hook sites. No engine internals enter the SDK.
use super::{Game, compiled_mods, execution_role::ExecutionRole};
use horizon_client::{containers::ModBlockEntity, text::Text};
use horizon_core::position::BlockPos;
use horizon_mod_api::{Action, BlockState, Event, Host, Runtime};
use horizon_world::{chunk, level::flags};
use std::sync::OnceLock;

static RUNTIME: OnceLock<Result<Runtime, String>> = OnceLock::new();
pub(super) fn initialize(
    data: &[(String, horizon_mods::Content)],
) -> Result<&'static Runtime, String> {
    RUNTIME
        .get_or_init(|| Runtime::build_with_content(compiled_mods::mods(), data))
        .as_ref()
        .map_err(Clone::clone)
}
pub(super) fn catalog() -> Result<&'static Runtime, String> {
    initialize(&[])
}

struct Context<'a, 'b> {
    game: &'a mut Game<'b>,
    owner: &'a str,
    world_access: bool,
    world_write: bool,
    player_access: bool,
}
impl Context<'_, '_> {
    fn server(&self, position: [i32; 3]) -> Result<chunk::BlockState, String> {
        if !self.world_access
            || self.game.remote.is_some()
            || self.game.options.execution_role == ExecutionRole::Client
        {
            return Err("world operations require the authoritative server".into());
        }
        super::native_world_read::read_block(
            &self.game.world,
            self.game.pack.assets.registry,
            position,
        )
        .map_err(|error| format!("block query: {error:?}"))
    }
}
impl Host for Context<'_, '_> {
    fn dimension(&self) -> &str {
        self.game.dim.id()
    }
    fn block(&self, position: [i32; 3]) -> Result<BlockState, String> {
        let state = self.server(position)?;
        Ok(BlockState {
            id: state.name.clone(),
            properties: state.properties.clone(),
        })
    }
    fn set_block(&mut self, position: [i32; 3], state: &BlockState) -> Result<(), String> {
        self.server(position)?;
        if !self.world_write {
            return Err("world writes are unavailable during this phase".into());
        }
        let state = chunk::BlockState {
            name: state.id.clone(),
            properties: state.properties.clone(),
        };
        let id = self
            .game
            .pack
            .assets
            .registry
            .resolve(&state)
            .map_err(|e| e.to_string())?;
        let pos = BlockPos::new(position[0], position[1], position[2]);
        let old = self.game.world.block(position[0], position[1], position[2]);
        if old == id {
            return Ok(());
        }
        let old_name = self.game.res.facts().get(old).name.clone();
        if !self
            .game
            .with_level(|b, level, ticks| b.set_block(level, ticks, pos, id, flags::UPDATE_ALL))
        {
            return Err("block update was refused".into());
        }
        let landed = self.game.world.block(position[0], position[1], position[2]);
        if old_name.as_ref() != state.name
            && self.game.res.facts().get(landed).name.as_ref() == state.name
            && let Some(entity) = self.game.new_block_entity(&state.name)
        {
            self.game
                .world
                .block_entities
                .insert((position[0], position[1], position[2]), entity);
        }
        Ok(())
    }
    fn feedback(&mut self, text: &str) {
        self.game.native_feedback(Text::literal(text));
    }
    fn give_item(&mut self, id: &str, count: u32) -> Result<u32, String> {
        if !self.player_access
            || self.game.remote.is_some()
            || self.game.options.execution_role == ExecutionRole::Client
        {
            return Err("items require an acting player on the authoritative server".into());
        }
        if self.game.res.items().get(id).is_none() {
            return Err(format!("unknown item {id}"));
        }
        Ok(self.game.native_give_items(id, count))
    }

    fn setting(&self, mod_id: &str, key: &str) -> Option<String> {
        if mod_id != self.owner {
            return None;
        }
        self.game.native_setting(mod_id, key).or_else(|| {
            catalog()
                .ok()?
                .mods
                .iter()
                .find(|m| m.declarations.meta.id == mod_id)?
                .settings
                .iter()
                .find(|s| s.key == key)
                .map(|s| s.default.clone())
        })
    }
    fn set_setting(&mut self, mod_id: &str, key: &str, value: &str) -> Result<(), String> {
        if mod_id != self.owner
            || !catalog()?
                .mods
                .iter()
                .find(|m| m.declarations.meta.id == mod_id)
                .is_some_and(|m| m.settings.iter().any(|s| s.key == key))
        {
            return Err("unknown setting or another mod's settings".into());
        }
        let dir = self.game.native_data_dir(mod_id);
        std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(dir.join(key), value))
            .map_err(|error| error.to_string())
    }
}

impl Game<'_> {
    pub(super) fn rust_screen(
        &self,
        block: &str,
        screen: &mut Option<super::native_hooks::NativeScreen>,
    ) {
        if screen.is_some() {
            return;
        }
        let Some((module, _)) = catalog().ok().and_then(|r| r.machine(block)) else {
            return;
        };
        if let Some(s) = module.module.screen(block) {
            let valid = s.width.is_finite()
                && s.height.is_finite()
                && (1.0..=4096.0).contains(&s.width)
                && (1.0..=4096.0).contains(&s.height)
                && s.inventory_x.is_finite()
                && s.inventory_y.is_finite()
                && s.slots.len() <= 256
                && s.slots.iter().flatten().flatten().all(|v| v.is_finite());
            if valid {
                *screen = Some(super::native_hooks::NativeScreen {
                    width: s.width,
                    height: s.height,
                    slots: s.slots,
                    inventory_x: s.inventory_x,
                    inventory_y: s.inventory_y,
                    own_labels: s.own_labels,
                });
            }
        }
    }
    pub(super) fn rust_screen_draw(
        &self,
        block: &str,
        data: &[i32],
        batch: &mut horizon_client::hud::HudBatch,
        origin: [f32; 2],
    ) {
        let Some((module, _)) = catalog().ok().and_then(|r| r.machine(block)) else {
            return;
        };
        for draw in module.module.draw_screen(block, data) {
            match draw {
                horizon_mod_api::Draw::Text {
                    text,
                    at,
                    color,
                    shadow,
                } => self.native_text(
                    batch,
                    &text,
                    [at[0] + origin[0], at[1] + origin[1]],
                    color,
                    shadow,
                ),
                horizon_mod_api::Draw::Fill { mut rect, color } => {
                    rect[0] += origin[0];
                    rect[1] += origin[1];
                    self.native_fill(batch, rect, color);
                }
                horizon_mod_api::Draw::Sprite {
                    name,
                    source,
                    mut rect,
                } => {
                    rect[0] += origin[0];
                    rect[1] += origin[1];
                    self.native_sprite(batch, &name, source, rect);
                }
            }
        }
    }
    pub(super) fn rust_screen_click(
        &self,
        block: &str,
        data: &[i32],
        at: [f32; 2],
        right: bool,
        button: &mut Option<i32>,
    ) {
        if button.is_none()
            && let Some((module, _)) = catalog().ok().and_then(|r| r.machine(block))
        {
            *button = module.module.screen_button(block, data, at, right);
        }
    }
    pub(super) fn rust_screen_tooltip(
        &self,
        block: &str,
        data: &[i32],
        at: [f32; 2],
        lines: &mut Vec<String>,
    ) {
        if let Some((module, _)) = catalog().ok().and_then(|r| r.machine(block)) {
            lines.extend(module.module.screen_tooltip(block, data, at));
        }
    }
    pub(super) fn rust_machine_button(
        &mut self,
        position: [i32; 3],
        entity: &mut ModBlockEntity,
        button: i32,
    ) {
        let Ok(runtime) = catalog() else { return };
        let Some((module, _)) = runtime.machine(&entity.id) else {
            return;
        };
        let id = &module.declarations.meta.id;
        let state = horizon_mod_api::MachineState {
            position,
            data: &mut entity.nbt,
            items: &mut entity.items,
            menu_data: &mut entity.data,
        };
        let mut context = Context {
            game: self,
            owner: id,
            world_access: true,
            world_write: true,
            player_access: false,
        };
        if let Err(error) =
            module
                .module
                .machine_button(state, button, &mut context, &runtime.registries)
        {
            self.rust_problem(id, &error);
        }
    }
    pub(super) fn rust_use_block(&mut self, position: [i32; 3], face: [i32; 3]) -> bool {
        if self.options.execution_role == ExecutionRole::Client || self.remote.is_some() {
            return false;
        }
        let state = self.world.block(position[0], position[1], position[2]);
        let state = self.rust_block_state(state);
        if self.rust_event(
            Event::UseBlock {
                position,
                face,
                state,
            },
            None,
        ) {
            return true;
        }
        let key = (position[0], position[1], position[2]);
        if self.open_mod_menu(key) {
            self.open_menu(position);
            return true;
        }
        false
    }
    pub(super) fn rust_world_event(&mut self, phase: super::native_lifecycle::WorldPhase) {
        use super::native_lifecycle::WorldPhase;
        match phase {
            WorldPhase::WorldOpenReady => {
                self.rust_event(Event::WorldReady, None);
            }
            WorldPhase::WorldCloseFinal => {
                self.rust_event(Event::WorldClosed, None);
            }
            _ => {}
        }
    }
    pub(super) fn rust_tick_event(
        &mut self,
        context: &super::native_lifecycle::NativeLifecycleContext,
        phase: super::native_lifecycle::ServerTickPhase,
    ) {
        if let super::native_lifecycle::ServerTickPhase::ServerTickEnd { world_running } = phase {
            self.rust_event(
                Event::ServerTick {
                    tick: context.tick.number(),
                    world_running,
                },
                None,
            );
        }
    }
    pub(super) fn rust_commands(&self, commands: &mut Vec<super::native_server::NativeCommand>) {
        let Ok(runtime) = catalog() else { return };
        for module in &runtime.mods {
            commands.extend(
                module
                    .commands
                    .iter()
                    .map(|c| super::native_server::NativeCommand {
                        usage: c.usage.clone(),
                        level: c.permission,
                    }),
            );
        }
    }
    pub(super) fn rust_machine_create(
        &self,
        block: &str,
        entity: &mut Option<(String, horizon_world::nbt::Compound)>,
    ) {
        if entity.is_some() {
            return;
        }
        if let Some((_, machine)) = catalog().ok().and_then(|r| r.machine(block)) {
            *entity = Some((block.into(), machine.initial.clone()));
        }
    }
    pub(super) fn rust_machine_ticks(&self, block: &str, ticks: &mut bool) {
        *ticks |= catalog().ok().is_some_and(|r| r.machine(block).is_some());
    }
    pub(super) fn rust_machine_container(
        &self,
        block: &str,
        container: &mut Option<horizon_client::containers::ModContainer>,
    ) {
        if container.is_some() {
            return;
        }
        if let Some((_, machine)) = catalog().ok().and_then(|r| r.machine(block)) {
            let slots: Vec<_> = (0..machine.slots).collect();
            *container = Some(horizon_client::containers::ModContainer {
                slots: machine.slots,
                faces: std::array::from_fn(|_| slots.clone()),
                inputs: slots.clone(),
                outputs: slots,
                menu: machine.menu.clone(),
            });
        }
    }
    pub(super) fn rust_event(&mut self, event: Event<'_>, owner: Option<&str>) -> bool {
        let Ok(runtime) = catalog() else { return false };
        for module in &runtime.mods {
            let id = module.declarations.meta.id.as_str();
            if owner.is_some_and(|owner| owner != id) {
                continue;
            }
            if let Event::Command { command, .. } = &event {
                let root = command.split_whitespace().next();
                if !module
                    .commands
                    .iter()
                    .any(|c| c.usage.split_whitespace().next() == root)
                {
                    continue;
                }
            }
            let mut context = Context {
                game: self,
                owner: id,
                world_access: !matches!(
                    event,
                    Event::WorldClosed | Event::Key { .. } | Event::Config { .. }
                ),
                world_write: !matches!(
                    event,
                    Event::WorldClosed
                        | Event::Key { .. }
                        | Event::Config { .. }
                        | Event::ServerTick {
                            world_running: false,
                            ..
                        }
                ),
                player_access: matches!(
                    event,
                    Event::UseItem { .. }
                        | Event::UseItemOn { .. }
                        | Event::UseBlock { .. }
                        | Event::Command { .. }
                        | Event::PlaceBlock { .. }
                        | Event::DestroyBlock { .. }
                ),
            };
            match module
                .module
                .event(&event, &mut context, &runtime.registries)
            {
                Ok(Action::Handled) => return true,
                Ok(Action::Pass) => {}
                Err(error) => self.rust_problem(id, &error),
            }
        }
        false
    }
    fn rust_problem(&mut self, id: &str, error: &str) {
        log::error!("Rust mod {id}: {error}");
        if let Some(row) = self.settings.mods.installed.iter_mut().find(|m| m.id == id) {
            row.problem = Some(error.into());
        }
    }
    pub(super) fn rust_machine_tick(&mut self, position: [i32; 3], entity: &mut ModBlockEntity) {
        let Ok(runtime) = catalog() else { return };
        let Some((module, _)) = runtime.machine(&entity.id) else {
            return;
        };
        let id = &module.declarations.meta.id;
        let state = horizon_mod_api::MachineState {
            position,
            data: &mut entity.nbt,
            items: &mut entity.items,
            menu_data: &mut entity.data,
        };
        let mut context = Context {
            game: self,
            owner: id,
            world_access: true,
            world_write: true,
            player_access: false,
        };
        if let Err(error) = module
            .module
            .machine_tick(state, &mut context, &runtime.registries)
        {
            self.rust_problem(id, &error);
        }
    }
    pub(super) fn rust_mod_rows(&self, mods: &mut Vec<super::native_hooks::NativeMod>) {
        let Ok(runtime) = catalog() else { return };
        for module in &runtime.mods {
            let meta = &module.declarations.meta;
            mods.push(super::native_hooks::NativeMod {
                id: meta.id.clone(),
                title: format!("{} {}", meta.name, meta.version),
                version: meta.version.clone(),
                authors: meta.authors.clone(),
                description: meta.description.clone(),
                keys: module
                    .keys
                    .iter()
                    .map(|k| [k.name.clone(), k.label.clone(), k.default.clone()])
                    .collect(),
            });
        }
    }
    pub(super) fn rust_settings(&self, id: &str, buttons: &mut Vec<String>) {
        let Ok(runtime) = catalog() else { return };
        if let Some(module) = runtime.mods.iter().find(|m| m.declarations.meta.id == id) {
            for setting in &module.settings {
                buttons.push(format!(
                    "{}: {}",
                    setting.label,
                    self.native_setting(id, &setting.key)
                        .unwrap_or_else(|| setting.default.clone())
                ));
            }
        }
    }
    pub(super) fn rust_settings_click(&mut self, id: &str, button: usize) {
        let Ok(runtime) = catalog() else { return };
        if let Some(setting) = runtime
            .mods
            .iter()
            .find(|m| m.declarations.meta.id == id)
            .and_then(|m| m.settings.get(button))
        {
            self.rust_event(Event::Config { key: &setting.key }, Some(id));
        }
    }
    pub(super) fn rust_block_state(
        &self,
        state: horizon_world::registry::BlockStateId,
    ) -> BlockState {
        let state = self.pack.assets.registry.state(state);
        BlockState {
            id: state.map(|s| s.name.clone()).unwrap_or_default(),
            properties: state.map(|s| s.properties.clone()).unwrap_or_default(),
        }
    }
}
