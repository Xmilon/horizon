//! Experimental Rust source-mod API. Mods are compiled together with Horizon; no Rust ABI crosses a shared-library boundary.
pub mod persistence;
pub mod registry;

pub use horizon_items::items::ItemStack;
pub use horizon_mods::content::NativeBlock;
pub use horizon_mods::imod::{self, BlockDecl, InoxFile, ItemDecl, ModMeta};
pub use horizon_world::nbt::{Compound, Tag};
pub use registry::{EntryRef, Registries, RegistryBuilder, RegistryKey};
use std::collections::BTreeMap;

pub type Result<T = ()> = std::result::Result<T, String>;
pub const API: &str = horizon_mods::rust_mod::API;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlockState {
    pub id: String,
    pub properties: BTreeMap<String, String>,
}

/// Only available for the duration of a callback, on the appropriate game/server thread.
pub trait Host {
    fn dimension(&self) -> &str;
    fn block(&self, position: [i32; 3]) -> Result<BlockState>;
    fn set_block(&mut self, position: [i32; 3], state: &BlockState) -> Result;
    fn feedback(&mut self, text: &str);
    fn give_item(&mut self, id: &str, count: u32) -> Result<u32>;
    fn setting(&self, mod_id: &str, key: &str) -> Option<String>;
    fn set_setting(&mut self, mod_id: &str, key: &str, value: &str) -> Result;
}

#[derive(Clone, Debug)]
pub enum Event<'a> {
    WorldReady,
    WorldClosed,
    ServerTick {
        tick: u64,
        world_running: bool,
    },
    UseItem {
        stack: &'a ItemStack,
    },
    UseItemOn {
        stack: &'a ItemStack,
        position: [i32; 3],
        face: [i32; 3],
    },
    UseBlock {
        position: [i32; 3],
        face: [i32; 3],
        state: BlockState,
    },
    DestroyBlock {
        position: [i32; 3],
        state: BlockState,
    },
    PlaceBlock {
        position: [i32; 3],
        state: BlockState,
    },
    Command {
        player: u128,
        command: &'a str,
    },
    Key {
        name: &'a str,
        pressed: bool,
    },
    Config {
        key: &'a str,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Action {
    #[default]
    Pass,
    Handled,
}

#[derive(Clone, Debug)]
pub struct Command {
    pub usage: String,
    pub permission: u8,
}
#[derive(Clone, Debug)]
pub struct Setting {
    pub key: String,
    pub label: String,
    pub default: String,
}
#[derive(Clone, Debug)]
pub struct KeyBinding {
    pub name: String,
    pub label: String,
    pub default: String,
}

#[derive(Clone, Debug)]
pub struct Screen {
    pub width: f32,
    pub height: f32,
    pub slots: Vec<Option<[f32; 2]>>,
    pub inventory_x: f32,
    pub inventory_y: f32,
    pub own_labels: bool,
}
#[derive(Clone, Debug)]
pub enum Draw {
    Text {
        text: String,
        at: [f32; 2],
        color: u32,
        shadow: bool,
    },
    Fill {
        rect: [f32; 4],
        color: u32,
    },
    Sprite {
        name: String,
        source: [f32; 4],
        rect: [f32; 4],
    },
}

/// A persistent machine using the engine's chunk NBT and inventory handling.
#[derive(Clone, Debug, Default)]
pub struct Machine {
    pub block: String,
    pub initial: Compound,
    pub slots: usize,
    /// Existing synchronized menu: `minecraft:generic_9x1` through `generic_9x6`, hopper or furnace.
    pub menu: Option<String>,
}

/// Borrowed while the machine is out of its chunk; no references survive the callback.
pub struct MachineState<'a> {
    pub position: [i32; 3],
    pub data: &'a mut Compound,
    pub items: &'a mut [Option<ItemStack>],
    pub menu_data: &'a mut Vec<i32>,
}

/// Implemented by a normal Cargo library. Registration is deterministic and runs once per process.
pub trait Mod: Send + Sync {
    fn define_registries(&self, _owner: &str, _registries: &mut RegistryBuilder) -> Result {
        Ok(())
    }
    fn register(&self, _context: &mut Registration<'_>) -> Result {
        Ok(())
    }
    fn event(
        &self,
        _event: &Event<'_>,
        _host: &mut dyn Host,
        _registries: &Registries,
    ) -> Result<Action> {
        Ok(Action::Pass)
    }
    fn machine_tick(
        &self,
        _state: MachineState<'_>,
        _host: &mut dyn Host,
        _registries: &Registries,
    ) -> Result {
        Ok(())
    }
    fn machine_button(
        &self,
        _state: MachineState<'_>,
        _button: i32,
        _host: &mut dyn Host,
        _registries: &Registries,
    ) -> Result {
        Ok(())
    }
    fn screen(&self, _block: &str) -> Option<Screen> {
        None
    }
    fn draw_screen(&self, _block: &str, _data: &[i32]) -> Vec<Draw> {
        Vec::new()
    }
    fn screen_button(
        &self,
        _block: &str,
        _data: &[i32],
        _position: [f32; 2],
        _right: bool,
    ) -> Option<i32> {
        None
    }
    fn screen_tooltip(&self, _block: &str, _data: &[i32], _position: [f32; 2]) -> Vec<String> {
        Vec::new()
    }
}

pub struct Registration<'a> {
    pub content: &'a mut InoxFile,
    pub registries: &'a mut RegistryBuilder,
    pub references: &'a mut Vec<EntryRef>,
    pub machines: &'a mut Vec<Machine>,
    pub commands: &'a mut Vec<Command>,
    pub settings: &'a mut Vec<Setting>,
    pub keys: &'a mut Vec<KeyBinding>,
    native_blocks: &'a mut BTreeMap<String, NativeBlock>,
}
impl Registration<'_> {
    /// A block with its own states and collision; no vanilla gameplay template is required.
    pub fn block(
        &mut self,
        name: &str,
        mut appearance: BlockDecl,
        definition: NativeBlock,
    ) -> Result {
        definition.states()?;
        if self.content.block.contains_key(name) {
            return Err(format!("duplicate block {name}"));
        }
        // Exported engine tables need a neutral solid row; states, collision and callbacks are ours.
        appearance.template = "minecraft:stone".into();
        self.content.block.insert(name.into(), appearance);
        self.native_blocks.insert(name.into(), definition);
        Ok(())
    }
    /// A normal item with its own components and Rust use callbacks.
    pub fn item(&mut self, name: &str, mut definition: ItemDecl) -> Result {
        if self.content.item.contains_key(name) {
            return Err(format!("duplicate item {name}"));
        }
        definition.template = "minecraft:stick".into();
        self.content.item.insert(name.into(), definition);
        Ok(())
    }
}

/// Generated build glue; all assets are embedded so the compiled version is self-contained.
pub struct ModSource {
    pub manifest: &'static str,
    pub fingerprint: &'static str,
    pub files: Vec<(&'static str, &'static [u8])>,
    pub module: Box<dyn Mod>,
}

pub struct LoadedMod {
    pub declarations: InoxFile,
    pub content: horizon_mods::Content,
    pub fingerprint: String,
    pub module: Box<dyn Mod>,
    pub machines: Vec<Machine>,
    pub commands: Vec<Command>,
    pub settings: Vec<Setting>,
    pub keys: Vec<KeyBinding>,
}

pub struct Runtime {
    pub mods: Vec<LoadedMod>,
    pub registries: Registries,
}
impl Runtime {
    pub fn build(sources: Vec<ModSource>) -> Result<Self> {
        Self::build_with_content(sources, &[])
    }
    pub fn build_with_content(
        sources: Vec<ModSource>,
        data: &[(String, horizon_mods::Content)],
    ) -> Result<Self> {
        let mut parsed = Vec::new();
        for source in sources {
            let (_, declarations) = imod::parse(source.manifest)?;
            if declarations.rust.is_none() {
                return Err("compiled source has no [rust] descriptor".into());
            }
            parsed.push((source, declarations));
        }
        let descriptions: Vec<_> = parsed
            .iter()
            .map(|(_, d)| {
                (
                    d.meta.id.clone(),
                    d.meta.version.clone(),
                    d.rust.clone().unwrap(),
                )
            })
            .collect();
        let order = horizon_mods::rust_mod::order(&descriptions)?;
        let mut builder = RegistryBuilder::default();
        for &i in &order {
            parsed[i]
                .0
                .module
                .define_registries(&parsed[i].1.meta.id, &mut builder)?;
        }
        let mut mods = Vec::new();
        let mut references = Vec::new();
        for i in order {
            let (source, original) = &mut parsed[i];
            let mut declarations = original.clone();
            let mut machines = Vec::new();
            let mut commands = Vec::new();
            let mut settings = Vec::new();
            let mut keys = Vec::new();
            let mut native_blocks = BTreeMap::new();
            source.module.register(&mut Registration {
                content: &mut declarations,
                registries: &mut builder,
                references: &mut references,
                machines: &mut machines,
                commands: &mut commands,
                settings: &mut settings,
                keys: &mut keys,
                native_blocks: &mut native_blocks,
            })?;
            if declarations.meta != original.meta || declarations.rust != original.rust {
                return Err(format!(
                    "{}: registration changed its manifest identity",
                    original.meta.id
                ));
            }
            let valid_key = |key: &str| {
                !key.is_empty()
                    && key.len() <= 64
                    && key
                        .bytes()
                        .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
            };
            for list in [
                settings.iter().map(|s| s.key.as_str()).collect::<Vec<_>>(),
                keys.iter().map(|k| k.name.as_str()).collect(),
            ] {
                let mut seen = std::collections::BTreeSet::new();
                if list.iter().any(|key| !valid_key(key) || !seen.insert(*key)) {
                    return Err(format!(
                        "{}: invalid or duplicate setting/key binding",
                        declarations.meta.id
                    ));
                }
            }
            let mut machine_ids = std::collections::BTreeSet::new();
            for machine in &machines {
                registry::identifier(&machine.block)?;
                let prefix = format!("{}:", declarations.meta.id);
                if !machine.block.starts_with(&prefix)
                    || !declarations
                        .block
                        .contains_key(&machine.block[prefix.len()..])
                    || machine.slots > 256
                    || !machine_ids.insert(&machine.block)
                {
                    return Err(format!(
                        "{}: machine must belong to a declared block with at most 256 slots",
                        machine.block
                    ));
                }
                if let Some(menu) = &machine.menu {
                    let count = match menu.as_str() {
                        "minecraft:hopper" => Some(5),
                        "minecraft:generic_3x3" => Some(9),
                        "minecraft:furnace" | "minecraft:blast_furnace" | "minecraft:smoker" => {
                            Some(3)
                        }
                        _ => menu
                            .strip_prefix("minecraft:generic_9x")
                            .and_then(|n| n.parse::<usize>().ok())
                            .filter(|n| (1..=6).contains(n))
                            .map(|n| n * 9),
                    };
                    if count != Some(machine.slots) {
                        return Err(format!(
                            "{}: menu does not match machine slots",
                            machine.block
                        ));
                    }
                }
            }
            let mut roots = std::collections::BTreeSet::new();
            for command in &commands {
                let root = command.usage.split_whitespace().next().unwrap_or_default();
                if !valid_key(root) || command.permission > 4 {
                    return Err(format!(
                        "{}: command needs a plain root and permission 0..4",
                        declarations.meta.id
                    ));
                }
                roots.insert(root);
            }
            for existing in &mods {
                let existing: &LoadedMod = existing;
                if existing
                    .commands
                    .iter()
                    .any(|c| roots.contains(c.usage.split_whitespace().next().unwrap_or_default()))
                {
                    return Err(format!(
                        "{} and {} register the same command",
                        existing.declarations.meta.id, declarations.meta.id
                    ));
                }
            }
            let mut content = horizon_mods::Content::from_declarations(
                &declarations,
                source
                    .files
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_vec()))
                    .collect(),
            )?;
            for block in &mut content.blocks {
                let name = block.id.split_once(':').unwrap().1;
                if let Some(mut definition) = native_blocks.remove(name) {
                    definition.block_entity = machines.iter().any(|m| m.block == block.id);
                    block.native = Some(definition);
                }
            }
            builder.load_data(&declarations.meta.id, &content)?;
            for existing in &mods {
                let existing: &LoadedMod = existing;
                if let Some(problem) = horizon_mods::content::clash(
                    (&existing.declarations.meta.id, &existing.content.namespaces),
                    (&declarations.meta.id, &content.namespaces),
                ) {
                    return Err(problem);
                }
            }
            mods.push(LoadedMod {
                declarations,
                content,
                fingerprint: source.fingerprint.into(),
                module: std::mem::replace(&mut source.module, Box::new(Empty)),
                machines,
                commands,
                settings,
                keys,
            });
        }
        for (id, content) in data {
            builder.load_data(id, content)?;
        }
        Ok(Self {
            mods,
            registries: builder.freeze(&references)?,
        })
    }

    pub fn machine(&self, block: &str) -> Option<(&LoadedMod, &Machine)> {
        self.mods
            .iter()
            .find_map(|m| m.machines.iter().find(|d| d.block == block).map(|d| (m, d)))
    }

    pub fn manifest(&self) -> persistence::Manifest {
        persistence::Manifest {
            api: API.into(),
            mods: self
                .mods
                .iter()
                .map(|m| {
                    (
                        m.declarations.meta.id.clone(),
                        persistence::ModVersion {
                            version: m.declarations.meta.version.clone(),
                            fingerprint: m.fingerprint.clone(),
                        },
                    )
                })
                .collect(),
            registries: self.registries.data(),
        }
    }
}
struct Empty;
impl Mod for Empty {}
