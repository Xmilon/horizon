use horizon_mod_api::{registry::RegistryKey, *};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub power: u32,
}
pub const MATERIALS: RegistryKey<Material> = RegistryKey::new("sample_mod:materials");
pub struct SampleMod;
pub fn create() -> Box<dyn Mod> {
    Box::new(SampleMod)
}

impl Mod for SampleMod {
    fn define_registries(&self, owner: &str, registries: &mut RegistryBuilder) -> Result {
        registries.create(owner, MATERIALS, 1)
    }
    fn register(&self, context: &mut Registration<'_>) -> Result {
        let owner = context.content.meta.id.clone();
        let material = context.registries.insert(
            &owner,
            MATERIALS,
            "sample_mod:ruby",
            Material { power: 10 },
        )?;
        context.references.push(material);
        context.item(
            "wand",
            ItemDecl {
                texture: Some("minecraft:item/blaze_rod".into()),
                ..Default::default()
            },
        )?;
        let mut definition = NativeBlock::default();
        definition
            .properties
            .insert("active".into(), vec!["false".into(), "true".into()]);
        definition
            .default_state
            .insert("active".into(), "false".into());
        context.block(
            "machine",
            BlockDecl {
                texture: Some("minecraft:block/amethyst_block".into()),
                hardness: Some(3.0),
                ..Default::default()
            },
            definition,
        )?;
        context.machines.push(Machine {
            block: "sample_mod:machine".into(),
            slots: 9,
            menu: Some("minecraft:generic_9x1".into()),
            ..Default::default()
        });
        context.commands.push(Command {
            usage: "sample_mod".into(),
            permission: 0,
        });
        context.settings.push(Setting {
            key: "messages".into(),
            label: "Wand messages".into(),
            default: "true".into(),
        });
        Ok(())
    }
    fn event(
        &self,
        event: &Event<'_>,
        host: &mut dyn Host,
        registries: &Registries,
    ) -> Result<Action> {
        match event {
            Event::Command { .. } => {
                host.feedback(&format!(
                    "Ruby power: {}",
                    registries.get(MATERIALS, "sample_mod:ruby")?.power
                ));
                return Ok(Action::Handled);
            }
            Event::UseItem { stack } if &*stack.item == "sample_mod:wand" => {
                if host.setting("sample_mod", "messages").as_deref() != Some("false") {
                    host.feedback("The Rust wand works!");
                }
                return Ok(Action::Handled);
            }
            Event::Config { key } if *key == "messages" => {
                let on = host.setting("sample_mod", key).as_deref() != Some("false");
                host.set_setting("sample_mod", key, if on { "false" } else { "true" })?;
            }
            _ => {}
        }
        Ok(Action::Pass)
    }
    fn machine_tick(
        &self,
        state: MachineState<'_>,
        host: &mut dyn Host,
        _registries: &Registries,
    ) -> Result {
        let ticks = match state.data.get(&"ticks".into()) {
            Some(Tag::Long(ticks)) => *ticks,
            _ => 0,
        } + 1;
        state.data.insert("ticks".into(), Tag::Long(ticks));
        state.menu_data.resize(1, 0);
        state.menu_data[0] = (ticks % 1200) as i32;
        if ticks % 100 == 0 {
            let mut block = host.block(state.position)?;
            let on = block.properties.get("active").is_some_and(|v| v == "true");
            block.properties.insert("active".into(), (!on).to_string());
            host.set_block(state.position, &block)?;
        }
        Ok(())
    }
    fn screen(&self, block: &str) -> Option<Screen> {
        (block == "sample_mod:machine").then(|| Screen {
            width: 176.0,
            height: 148.0,
            slots: (0..9)
                .map(|slot| Some([8.0 + slot as f32 * 18.0, 32.0]))
                .collect(),
            inventory_x: 8.0,
            inventory_y: 66.0,
            own_labels: true,
        })
    }
    fn draw_screen(&self, _block: &str, data: &[i32]) -> Vec<Draw> {
        vec![
            Draw::Text {
                text: format!("Rust machine: {} ticks", data.first().copied().unwrap_or(0)),
                at: [8.0, 8.0],
                color: 0x404040,
                shadow: false,
            },
            Draw::Text {
                text: "Inventory".into(),
                at: [8.0, 54.0],
                color: 0x404040,
                shadow: false,
            },
        ]
    }
}
