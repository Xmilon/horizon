use horizon_mod_api::*;
use std::collections::BTreeMap;

#[path = "../../cargo-horizon-mod/template/rust/lib.rs"]
mod demo;

const MANIFEST: &str = "[mod]\nid = 'sample_mod'\nname = 'Sample'\nversion = '0.1.0'\n[rust]\napi = '0.1'\nentry = 'create'\n";
fn source() -> ModSource {
    ModSource {
        manifest: MANIFEST,
        fingerprint: "fixture",
        files: Vec::new(),
        module: demo::create(),
    }
}

#[test]
fn reference_mod_registers_custom_registry_blocks_items_and_machine() {
    let runtime = Runtime::build(vec![source()]).unwrap();
    assert_eq!(
        runtime
            .registries
            .get(demo::MATERIALS, "sample_mod:ruby")
            .unwrap()
            .power,
        10
    );
    let module = &runtime.mods[0];
    assert_eq!(
        module.content.blocks[0]
            .native
            .as_ref()
            .unwrap()
            .states()
            .unwrap()
            .len(),
        2
    );
    assert!(
        module.content.blocks[0]
            .native
            .as_ref()
            .unwrap()
            .block_entity
    );
    assert_eq!(module.content.items[0].id, "sample_mod:wand");
    assert_eq!(runtime.machine("sample_mod:machine").unwrap().1.slots, 9);
    assert_eq!(runtime.manifest().registries[demo::MATERIALS.id].schema, 1);
}

#[test]
fn content_mod_can_add_typed_entries_without_rust_code() {
    let declarations = InoxFile {
        meta: ModMeta {
            id: "addon".into(),
            name: "Addon".into(),
            namespaces: vec!["extra".into()],
            version: "1".into(),
            ..Default::default()
        },
        ..Default::default()
    };
    let path = "data/addon/sample_mod/materials/sapphire.json";
    let content = horizon_mods::Content::from_declarations(
        &declarations,
        BTreeMap::from([
            (path.into(), br#"{"power":25}"#.to_vec()),
            (
                "data/extra/sample_mod/materials/blue.json".into(),
                br#"{"power":30}"#.to_vec(),
            ),
        ]),
    )
    .unwrap();
    let runtime =
        Runtime::build_with_content(vec![source()], &[("addon".into(), content)]).unwrap();
    assert_eq!(
        runtime
            .registries
            .get(demo::MATERIALS, "addon:sapphire")
            .unwrap()
            .power,
        25
    );
    assert_eq!(
        runtime
            .registries
            .get(demo::MATERIALS, "extra:blue")
            .unwrap()
            .power,
        30
    );
    let malformed = horizon_mods::Content::from_declarations(
        &declarations,
        BTreeMap::from([(path.into(), br#"{"power":"wrong type"}"#.to_vec())]),
    )
    .unwrap();
    assert!(Runtime::build_with_content(vec![source()], &[("addon".into(), malformed)]).is_err());
}

struct FakeHost {
    block: BlockState,
    messages: Vec<String>,
    settings: BTreeMap<String, String>,
}
impl Host for FakeHost {
    fn dimension(&self) -> &str {
        "minecraft:overworld"
    }
    fn block(&self, _position: [i32; 3]) -> Result<BlockState> {
        Ok(self.block.clone())
    }
    fn set_block(&mut self, _position: [i32; 3], state: &BlockState) -> Result {
        self.block = state.clone();
        Ok(())
    }
    fn feedback(&mut self, text: &str) {
        self.messages.push(text.into());
    }
    fn give_item(&mut self, _id: &str, count: u32) -> Result<u32> {
        Ok(count)
    }
    fn setting(&self, _owner: &str, key: &str) -> Option<String> {
        self.settings.get(key).cloned()
    }
    fn set_setting(&mut self, _owner: &str, key: &str, value: &str) -> Result {
        self.settings.insert(key.into(), value.into());
        Ok(())
    }
}
#[test]
fn rust_behavior_and_persistent_machine_data_work() {
    let runtime = Runtime::build(vec![source()]).unwrap();
    let module = &runtime.mods[0].module;
    let mut host = FakeHost {
        block: BlockState {
            id: "sample_mod:machine".into(),
            properties: BTreeMap::from([("active".into(), "false".into())]),
        },
        messages: Vec::new(),
        settings: BTreeMap::new(),
    };
    let wand = ItemStack::new("sample_mod:wand", 1);
    assert_eq!(
        module
            .event(
                &Event::UseItem { stack: &wand },
                &mut host,
                &runtime.registries
            )
            .unwrap(),
        Action::Handled
    );
    assert_eq!(host.messages, ["The Rust wand works!"]);
    module
        .event(
            &Event::Config { key: "messages" },
            &mut host,
            &runtime.registries,
        )
        .unwrap();
    assert_eq!(host.settings["messages"], "false");
    let mut data = Compound::new();
    let mut items = vec![None; 9];
    let mut menu_data = Vec::new();
    for _ in 0..100 {
        module
            .machine_tick(
                MachineState {
                    position: [0, 64, 0],
                    data: &mut data,
                    items: &mut items,
                    menu_data: &mut menu_data,
                },
                &mut host,
                &runtime.registries,
            )
            .unwrap();
    }
    assert_eq!(data.get(&"ticks".into()), Some(&Tag::Long(100)));
    assert_eq!(host.block.properties["active"], "true");
    assert_eq!(menu_data, [100]);
    assert_eq!(items.len(), 9);
}
