//! Inox mods (`.imod`, docs/inox-mods.md): blocks and items declared in one
//! `inox.toml`, built into the game by the content-mod loader.
//!
//! An `.imod` is a zip (or a folder) holding `inox.toml`, the mod's textures
//! under `assets/<id>/textures/`, and optionally any `assets/` and `data/`
//! file a content mod may hold. From the declarations this module writes, in
//! memory, the files a content mod would carry by hand: the block and item
//! declarations in `data/<id>/horizon/`, blockstates, block and item models,
//! item definitions, the English names, loot tables and tool tags. The mod's
//! own files win over generated ones at the same path, except for the
//! language file and tags, which are merged. Everything then goes through the
//! same checks and loader as a `.hmod` (`content.rs`, `mod_content.rs`).
use crate::manifest::{Capability, MANIFEST, Manifest, Side};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
};

/// The declarations' file name in an `.imod` or an Inox mod folder.
pub const IMOD_MANIFEST: &str = "inox.toml";
/// An Inox mod file's extension.
pub const EXTENSION: &str = "imod";

/// Whether `path` is an Inox mod: an `.imod` file or a folder holding
/// `inox.toml`.
pub fn is_imod(path: &Path) -> bool {
    if path.is_dir() {
        path.join(IMOD_MANIFEST).is_file()
    } else {
        path.extension().is_some_and(|e| e == EXTENSION)
    }
}

/// `inox.toml`'s text, from an `.imod` or a mod folder.
pub fn read_toml(path: &Path) -> Result<String, String> {
    use std::io::Read;
    let bytes = if path.is_dir() {
        std::fs::read(path.join(IMOD_MANIFEST)).map_err(|e| format!("{IMOD_MANIFEST}: {e}"))?
    } else {
        let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
        let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
        let entry = zip
            .by_name(IMOD_MANIFEST)
            .map_err(|e| format!("{IMOD_MANIFEST}: {e}"))?;
        let mut out = Vec::new();
        // Declarations are text: 1 MiB is far more than any needs.
        entry
            .take(1 << 20)
            .read_to_end(&mut out)
            .map_err(|e| e.to_string())?;
        out
    };
    String::from_utf8(bytes).map_err(|_| format!("{IMOD_MANIFEST}: not UTF-8"))
}

/// The `.imod` bytes for `inox.toml`'s text and the mod's own `files`
/// (`assets/...`, `data/...`) by path. Checks the declarations parse.
pub fn write(toml_text: &str, files: &[(&str, &[u8])]) -> Result<Vec<u8>, String> {
    use std::io::Write;
    parse(toml_text)?;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let options = zip::write::SimpleFileOptions::default();
    let mut put = |name: &str, bytes: &[u8]| -> Result<(), String> {
        zip.start_file(name, options).map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())
    };
    put(IMOD_MANIFEST, toml_text.as_bytes())?;
    for (name, bytes) in files {
        crate::content::check_path(name)?;
        put(name, bytes)?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

/// `inox.toml`.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct InoxFile {
    #[serde(rename = "mod")]
    pub meta: ModMeta,
    /// Blocks by name: `[block.ruby_block]` is `<id>:ruby_block`.
    #[serde(default)]
    pub block: BTreeMap<String, BlockDecl>,
    /// Items by name: `[item.ruby]` is `<id>:ruby`.
    #[serde(default)]
    pub item: BTreeMap<String, ItemDecl>,
}

/// `[mod]`: who the mod is, as Options > Mods lists it.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ModMeta {
    /// `[a-z0-9_]`, 1 to 64 characters: its namespace, settings key and log name.
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub authors: Vec<String>,
    /// More namespaces its raw `assets/` and `data/` files use.
    #[serde(default)]
    pub namespaces: Vec<String>,
}

/// A block: a vanilla template's behaviour with its own look, name and numbers.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct BlockDecl {
    /// The vanilla block it behaves as (`minecraft:iron_block`).
    pub template: String,
    /// Its English name; the name in title case when left out.
    pub name: Option<String>,
    /// `assets/<id>/textures/block/<texture>.png`, or a full texture id with
    /// `:` (`minecraft:block/stone`). The block's name when left out.
    pub texture: Option<String>,
    /// How the block model is made: `cube_all` (one texture on every face,
    /// the default) or `none` (the mod ships its own blockstate and model).
    pub model: Option<String>,
    pub hardness: Option<f32>,
    pub explosion_resistance: Option<f32>,
    pub light: Option<u8>,
    pub requires_tool: Option<bool>,
    pub friction: Option<f32>,
    pub map_color: Option<u8>,
    /// The vanilla block whose sounds it plays.
    pub sounds: Option<String>,
    /// Whether it has an item (placeable, in the creative inventory). Default true.
    #[serde(default = "yes")]
    pub item: bool,
    /// What breaking it drops: `self` (default), `none`, or an item id.
    pub drops: Option<String>,
    /// The tool that mines it fast: `pickaxe`, `axe`, `shovel` or `hoe`.
    pub tool: Option<String>,
    /// The tool tier it needs: `stone`, `iron` or `diamond`.
    pub tier: Option<String>,
}

/// An item: a vanilla template's behaviour with its own look, name and numbers.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct ItemDecl {
    /// The vanilla item it behaves as (`minecraft:diamond`).
    pub template: String,
    pub name: Option<String>,
    /// `assets/<id>/textures/item/<texture>.png`, or a full texture id with `:`.
    pub texture: Option<String>,
    /// `generated` (flat, the default), `handheld` (held like a tool), or
    /// `none` (the mod ships its own item model and definition).
    pub model: Option<String>,
    pub max_stack_size: Option<u8>,
    /// `common`, `uncommon`, `rare` or `epic`.
    pub rarity: Option<String>,
    /// Data components set (`minecraft:max_damage`) or removed (`!minecraft:repairable`).
    #[serde(default)]
    pub components: BTreeMap<String, Value>,
}

fn yes() -> bool {
    true
}

/// Parses `inox.toml`: the manifest the content loader checks a mod with
/// (a content mod, `api = "0.2"`, capability `registries`), and the
/// declarations.
pub fn parse(text: &str) -> Result<(Manifest, InoxFile), String> {
    let file: InoxFile = toml::from_str(text).map_err(|e| format!("{IMOD_MANIFEST}: {e}"))?;
    let m = &file.meta;
    let manifest = Manifest {
        id: m.id.clone(),
        name: m.name.clone(),
        version: m.version.clone(),
        description: m.description.clone(),
        authors: m.authors.clone(),
        api: "0.2".into(),
        side: Side::Both,
        capabilities: vec![Capability::Registries],
        namespaces: m.namespaces.clone(),
        experimental: Vec::new(),
    };
    let manifest = manifest
        .validated()
        .map_err(|e| e.replace(MANIFEST, IMOD_MANIFEST))?;
    for name in file.block.keys().chain(file.item.keys()) {
        check_name(name)?;
    }
    if let Some(n) = file.block.keys().find(|n| file.item.contains_key(*n)) {
        return Err(format!(
            "{IMOD_MANIFEST}: {n} is both a block and an item (a block's item has its name)"
        ));
    }
    Ok((manifest, file))
}

/// A block or item name: `[a-z0-9_]`, 1 to 64 characters.
fn check_name(name: &str) -> Result<(), String> {
    let ok = (1..=64).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    if ok {
        Ok(())
    } else {
        Err(format!(
            "{IMOD_MANIFEST}: name {name:?} is not [a-z0-9_]{{1,64}}"
        ))
    }
}

/// `ruby_block` -> `Ruby Block`.
fn title(name: &str) -> String {
    name.split('_')
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// A texture id, and the file it needs in the mod when it is the mod's own.
fn texture(id: &str, kind: &str, name: &str, given: Option<&str>) -> (String, Option<String>) {
    let t = given.unwrap_or(name);
    if t.contains(':') {
        (t.to_owned(), None)
    } else {
        (
            format!("{id}:{kind}/{t}"),
            Some(format!("assets/{id}/textures/{kind}/{t}.png")),
        )
    }
}

fn pretty(v: &Value) -> Vec<u8> {
    let mut out = serde_json::to_vec_pretty(v).unwrap_or_default();
    out.push(b'\n');
    out
}

/// What `generate` writes, before merging with the mod's own files.
#[derive(Default)]
struct Out {
    files: BTreeMap<String, Value>,
    /// English names, `block.<id>.<name>` / `item.<id>.<name>`.
    lang: BTreeMap<String, String>,
    /// Tag file -> values.
    tags: BTreeMap<String, Vec<String>>,
}

/// The files `file`'s declarations make, merged with the mod's own `raw`
/// files (`read` gives one's bytes): the mod's files win at a path, except
/// that `assets/<id>/lang/en_us.json` and tags are merged (the mod's names
/// and values first). Refuses a missing texture or an unknown option.
pub fn generate(
    file: &InoxFile,
    raw: &BTreeSet<String>,
    mut read: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    let id = file.meta.id.as_str();
    let mut out = Out::default();
    let need = |path: Option<String>, what: &str| -> Result<(), String> {
        match path {
            Some(p) if !raw.contains(&p) => Err(format!("{what}: texture {p} is missing")),
            _ => Ok(()),
        }
    };
    for (name, b) in &file.block {
        let what = format!("block {id}:{name}");
        let mut properties = serde_json::Map::new();
        let mut put = |k: &str, v: Option<Value>| {
            if let Some(v) = v {
                properties.insert(k.into(), v);
            }
        };
        put("hardness", b.hardness.map(Value::from));
        put(
            "explosion_resistance",
            b.explosion_resistance.map(Value::from),
        );
        put("light", b.light.map(Value::from));
        put("requires_tool", b.requires_tool.map(Value::from));
        put("friction", b.friction.map(Value::from));
        put("map_color", b.map_color.map(Value::from));
        put("sounds", b.sounds.clone().map(Value::from));
        out.files.insert(
            format!("data/{id}/horizon/block/{name}.json"),
            json!({ "template": b.template, "properties": properties, "item": b.item }),
        );
        match b.model.as_deref().unwrap_or("cube_all") {
            "cube_all" => {
                let (tex, file) = texture(id, "block", name, b.texture.as_deref());
                need(file, &what)?;
                out.files.insert(
                    format!("assets/{id}/blockstates/{name}.json"),
                    json!({ "variants": { "": { "model": format!("{id}:block/{name}") } } }),
                );
                out.files.insert(
                    format!("assets/{id}/models/block/{name}.json"),
                    json!({ "parent": "minecraft:block/cube_all", "textures": { "all": tex } }),
                );
            }
            "none" => {}
            m => return Err(format!("{what}: model {m:?} is cube_all or none")),
        }
        if b.item {
            out.files.insert(
                format!("assets/{id}/items/{name}.json"),
                json!({ "model": { "type": "minecraft:model", "model": format!("{id}:block/{name}") } }),
            );
        }
        out.lang.insert(
            format!("block.{id}.{name}"),
            b.name.clone().unwrap_or_else(|| title(name)),
        );
        let drop = match b.drops.as_deref().unwrap_or("self") {
            "none" => None,
            "self" if !b.item => {
                return Err(format!(
                    "{what}: drops itself but has no item (item = false): set drops"
                ));
            }
            "self" => Some(format!("{id}:{name}")),
            other if other.contains(':') => Some(other.to_owned()),
            other => {
                return Err(format!(
                    "{what}: drops {other:?} is self, none or an item id"
                ));
            }
        };
        if let Some(drop) = drop {
            out.files.insert(
                format!("data/{id}/loot_table/blocks/{name}.json"),
                json!({
                    "type": "minecraft:block",
                    "pools": [{
                        "rolls": 1,
                        "entries": [{ "type": "minecraft:item", "name": drop }],
                        "conditions": [{ "condition": "minecraft:survives_explosion" }]
                    }],
                    "random_sequence": format!("{id}:blocks/{name}")
                }),
            );
        }
        if let Some(tool) = &b.tool {
            if !matches!(tool.as_str(), "pickaxe" | "axe" | "shovel" | "hoe") {
                return Err(format!(
                    "{what}: tool {tool:?} is pickaxe, axe, shovel or hoe"
                ));
            }
            let tag = format!("data/minecraft/tags/block/mineable/{tool}.json");
            out.tags
                .entry(tag)
                .or_default()
                .push(format!("{id}:{name}"));
        }
        if let Some(tier) = &b.tier {
            if !matches!(tier.as_str(), "stone" | "iron" | "diamond") {
                return Err(format!("{what}: tier {tier:?} is stone, iron or diamond"));
            }
            let tag = format!("data/minecraft/tags/block/needs_{tier}_tool.json");
            out.tags
                .entry(tag)
                .or_default()
                .push(format!("{id}:{name}"));
        }
    }
    for (name, i) in &file.item {
        let what = format!("item {id}:{name}");
        let mut decl = serde_json::Map::new();
        decl.insert("template".into(), i.template.clone().into());
        if let Some(n) = i.max_stack_size {
            decl.insert("max_stack_size".into(), n.into());
        }
        if let Some(r) = &i.rarity {
            decl.insert("rarity".into(), r.clone().into());
        }
        if !i.components.is_empty() {
            let c = i.components.clone().into_iter().collect();
            decl.insert("components".into(), Value::Object(c));
        }
        out.files.insert(
            format!("data/{id}/horizon/item/{name}.json"),
            Value::Object(decl),
        );
        let parent = match i.model.as_deref().unwrap_or("generated") {
            "generated" => Some("minecraft:item/generated"),
            "handheld" => Some("minecraft:item/handheld"),
            "none" => None,
            m => {
                return Err(format!(
                    "{what}: model {m:?} is generated, handheld or none"
                ));
            }
        };
        if let Some(parent) = parent {
            let (tex, file) = texture(id, "item", name, i.texture.as_deref());
            need(file, &what)?;
            out.files.insert(
                format!("assets/{id}/models/item/{name}.json"),
                json!({ "parent": parent, "textures": { "layer0": tex } }),
            );
            out.files.insert(
                format!("assets/{id}/items/{name}.json"),
                json!({ "model": { "type": "minecraft:model", "model": format!("{id}:item/{name}") } }),
            );
        }
        out.lang.insert(
            format!("item.{id}.{name}"),
            i.name.clone().unwrap_or_else(|| title(name)),
        );
    }
    let mut files = BTreeMap::new();
    for (path, value) in out.files {
        if !raw.contains(&path) {
            files.insert(path, pretty(&value));
        }
    }
    if !out.lang.is_empty() {
        let path = format!("assets/{id}/lang/en_us.json");
        let mut names = serde_json::Map::new();
        if raw.contains(&path) {
            let own: Value =
                serde_json::from_slice(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
            let Value::Object(own) = own else {
                return Err(format!("{path}: not a JSON object"));
            };
            names = own;
        }
        for (k, v) in out.lang {
            names.entry(k).or_insert(v.into());
        }
        files.insert(path, pretty(&Value::Object(names)));
    }
    for (path, values) in out.tags {
        let mut tag = json!({ "values": [] });
        if raw.contains(&path) {
            tag = serde_json::from_slice(&read(&path)?).map_err(|e| format!("{path}: {e}"))?;
        }
        let Some(list) = tag.get_mut("values").and_then(Value::as_array_mut) else {
            return Err(format!("{path}: a tag holds a values list"));
        };
        list.extend(values.into_iter().map(Value::from));
        files.insert(path, pretty(&tag));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TOML: &str = r#"
[mod]
id = "ruby"
name = "Ruby"
version = "0.1.0"

[block.ruby_block]
template = "minecraft:iron_block"
hardness = 4.0
light = 3
tool = "pickaxe"
tier = "iron"

[block.ruby_ore]
template = "minecraft:iron_ore"
name = "Ruby Ore"
texture = "minecraft:block/stone"
drops = "ruby:ruby"
tool = "pickaxe"

[item.ruby]
template = "minecraft:diamond"
rarity = "uncommon"

[item.ruby_sword]
template = "minecraft:iron_sword"
model = "handheld"
components = { "minecraft:max_damage" = 900 }
"#;

    fn raw(files: &[&str]) -> BTreeSet<String> {
        files.iter().map(|s| (*s).to_owned()).collect()
    }
    fn json_of(files: &BTreeMap<String, Vec<u8>>, path: &str) -> Value {
        serde_json::from_slice(&files[path]).unwrap()
    }
    const TEXTURES: [&str; 3] = [
        "assets/ruby/textures/block/ruby_block.png",
        "assets/ruby/textures/item/ruby.png",
        "assets/ruby/textures/item/ruby_sword.png",
    ];

    #[test]
    fn declarations_make_a_content_mod() {
        let (manifest, file) = parse(TOML).unwrap();
        assert_eq!(manifest.api, "0.2");
        assert_eq!(manifest.capabilities, [Capability::Registries]);
        let files = generate(&file, &raw(&TEXTURES), |_| unreachable!()).unwrap();
        assert_eq!(
            json_of(&files, "data/ruby/horizon/block/ruby_block.json"),
            json!({ "template": "minecraft:iron_block", "item": true,
                    "properties": { "hardness": 4.0, "light": 3 } })
        );
        assert_eq!(
            json_of(&files, "assets/ruby/models/block/ruby_ore.json")["textures"]["all"],
            "minecraft:block/stone"
        );
        assert_eq!(
            json_of(&files, "data/ruby/loot_table/blocks/ruby_ore.json")["pools"][0]["entries"][0]
                ["name"],
            "ruby:ruby"
        );
        assert_eq!(
            json_of(&files, "data/minecraft/tags/block/mineable/pickaxe.json"),
            json!({ "values": ["ruby:ruby_block", "ruby:ruby_ore"] })
        );
        assert_eq!(
            json_of(&files, "data/minecraft/tags/block/needs_iron_tool.json"),
            json!({ "values": ["ruby:ruby_block"] })
        );
        assert_eq!(
            json_of(&files, "data/ruby/horizon/item/ruby_sword.json"),
            json!({ "template": "minecraft:iron_sword",
                    "components": { "minecraft:max_damage": 900 } })
        );
        assert_eq!(
            json_of(&files, "assets/ruby/models/item/ruby_sword.json")["parent"],
            "minecraft:item/handheld"
        );
        let lang = json_of(&files, "assets/ruby/lang/en_us.json");
        assert_eq!(lang["block.ruby.ruby_block"], "Ruby Block");
        assert_eq!(lang["block.ruby.ruby_ore"], "Ruby Ore");
        assert_eq!(lang["item.ruby.ruby_sword"], "Ruby Sword");
    }

    #[test]
    fn the_mods_own_files_win_and_lang_and_tags_merge() {
        let (_, file) = parse(TOML).unwrap();
        let mut own = raw(&TEXTURES);
        own.extend(raw(&[
            "assets/ruby/models/block/ruby_block.json",
            "assets/ruby/lang/en_us.json",
            "data/minecraft/tags/block/mineable/pickaxe.json",
        ]));
        let files = generate(&file, &own, |p| {
            Ok(match p {
                "assets/ruby/lang/en_us.json" => br#"{"item.ruby.ruby": "Red Gem"}"#.to_vec(),
                _ => br#"{"values": ["ruby:other"]}"#.to_vec(),
            })
        })
        .unwrap();
        assert!(!files.contains_key("assets/ruby/models/block/ruby_block.json"));
        assert_eq!(
            json_of(&files, "assets/ruby/lang/en_us.json")["item.ruby.ruby"],
            "Red Gem"
        );
        assert_eq!(
            json_of(&files, "data/minecraft/tags/block/mineable/pickaxe.json")["values"][0],
            "ruby:other"
        );
    }

    #[test]
    fn mistakes_are_refused() {
        let (_, file) = parse(TOML).unwrap();
        let e = generate(&file, &raw(&TEXTURES[1..]), |_| unreachable!()).unwrap_err();
        assert!(
            e.contains("assets/ruby/textures/block/ruby_block.png is missing"),
            "{e}"
        );
        let bad = |extra: &str| {
            let text = format!("[mod]\nid = \"m\"\nname = \"M\"\nversion = \"1\"\n{extra}");
            parse(&text).and_then(|(_, f)| generate(&f, &raw(&[]), |_| unreachable!()))
        };
        assert!(
            bad("[block.a]\ntemplate = \"minecraft:stone\"\nmodel = \"none\"\ntool = \"sword\"")
                .unwrap_err()
                .contains("tool")
        );
        assert!(
            bad("[block.a]\ntemplate = \"minecraft:stone\"\ncolour = 1")
                .unwrap_err()
                .contains("unknown field")
        );
        assert!(
            bad("[block.A]\ntemplate = \"minecraft:stone\"")
                .unwrap_err()
                .contains("name")
        );
        assert!(
            bad("[block.a]\ntemplate = \"x\"\n[item.a]\ntemplate = \"y\"")
                .unwrap_err()
                .contains("both a block and an item")
        );
        assert!(
            bad("[block.a]\ntemplate = \"minecraft:stone\"\nmodel = \"none\"\nitem = false")
                .unwrap_err()
                .contains("no item")
        );
        assert!(
            parse("[mod]\nid = \"Bad\"\nname = \"x\"\nversion = \"1\"")
                .unwrap_err()
                .contains("inox.toml: id")
        );
    }

    #[test]
    fn folders_and_imod_files_load_as_content_mods() {
        use crate::{Content, Package, find};
        let dir = std::env::temp_dir().join(format!("horizon-imod-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let folder = dir.join("ruby");
        for t in TEXTURES {
            let path = folder.join(t);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, b"png").unwrap();
        }
        std::fs::write(folder.join(IMOD_MANIFEST), TOML).unwrap();
        let png: &[u8] = b"png";
        let own: Vec<(&str, &[u8])> = TEXTURES.iter().map(|t| (*t, png)).collect();
        std::fs::write(dir.join("ruby.imod"), write(TOML, &own).unwrap()).unwrap();
        assert_eq!(find(&dir), [dir.join("ruby"), dir.join("ruby.imod")]);
        for path in find(&dir) {
            let package = Package::read(&path).unwrap();
            assert!(package.wasm.is_none());
            assert_eq!(package.manifest.name, "Ruby");
            let content = Content::read(&path, &package.manifest).unwrap();
            let blocks: Vec<_> = content.blocks.iter().map(|b| b.id.as_str()).collect();
            let items: Vec<_> = content.items.iter().map(|i| i.id.as_str()).collect();
            assert_eq!(blocks, ["ruby:ruby_block", "ruby:ruby_ore"]);
            assert_eq!(items, ["ruby:ruby", "ruby:ruby_sword"]);
            assert_eq!(content.blocks[0].properties.light, Some(3));
            assert!(
                content
                    .files
                    .contains(&"assets/ruby/lang/en_us.json".to_owned())
            );
            assert!(content.files.contains(&TEXTURES[0].to_owned()));
            let lang = content.file("assets/ruby/lang/en_us.json").unwrap();
            assert!(String::from_utf8(lang).unwrap().contains("Ruby Sword"));
            assert_eq!(content.file(TEXTURES[0]).unwrap(), b"png");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn names_become_titles() {
        assert_eq!(title("ruby_block"), "Ruby Block");
        assert_eq!(title("x"), "X");
    }
}
