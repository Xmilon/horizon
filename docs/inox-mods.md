# Inox mods (`.imod`)

Inox is Horizon's built-in mod loader for **blocks and items**. A mod is one `inox.toml` that declares
them, plus the mod's textures. The game reads it at start, writes the vanilla files a content mod would
need (declarations, blockstates, models, item definitions, names, loot tables, tool tags), and registers
everything through the same loader as `.hmod` content mods ([mods.md](mods.md), [modpacks.md](modpacks.md)).
No code runs and nothing is compiled: an `.imod` is as safe as a resource pack.

Inox is the first step towards a full mod loader. This version adds blocks and items; behaviour
(code that runs when a block is used, a custom entity, a fluid) comes later. See [What comes next](#what-comes-next).

- [Quick start](#quick-start)
- [The file](#the-file)
- [`inox.toml` reference](#inoxtoml-reference): [`[mod]`](#mod), [`[block.<name>]`](#blockname), [`[item.<name>]`](#itemname)
- [What Inox writes](#what-inox-writes), and how your own files win or merge
- [Textures and models](#textures-and-models)
- [Recipes, tags, loot and anything else](#recipes-tags-loot-and-anything-else)
- [Installing and playing](#installing-and-playing)
- [How it loads](#how-it-loads) (for contributors)
- [Rules and limits](#rules-and-limits)
- [Errors](#errors)
- [A complete example](#a-complete-example)
- [What comes next](#what-comes-next)

## Quick start

```sh
cargo install --git https://github.com/horizon-rust/horizon cargo-horizon-mod
cargo horizon-mod new gems --template inox     # inox.toml with one block, one item, a recipe
cd gems
cargo horizon-mod run                          # build target/horizon-mod/gems.imod and start the game with it on
```

Or by hand: make a folder `gems/` holding `inox.toml`:

```toml
[mod]
id = "gems"
name = "Gems"
version = "0.1.0"

[block.ruby_block]
template = "minecraft:iron_block"
hardness = 5.0
tool = "pickaxe"
tier = "iron"

[item.ruby]
template = "minecraft:diamond"
rarity = "uncommon"
```

and the two textures it names by default:

```text
gems/
  inox.toml
  assets/gems/textures/block/ruby_block.png     16x16 PNG
  assets/gems/textures/item/ruby.png
```

Put the folder (or the zipped `.imod`) in the game folder's `horizon-mods/`, start the game, turn
**Gems** on in Options > Mods, restart. `/give @s gems:ruby_block` and `/give @s gems:ruby` work, both
are in the creative inventory, the block is mined with an iron pickaxe and drops itself.

## The file

An Inox mod is either:

- a **folder** holding `inox.toml` (handy while you make it), or
- a **`.imod` file**: a zip of that folder's contents, with `inox.toml` at the root (not in a sub-folder).
  `cargo horizon-mod build` writes one; any zip tool works too (zip the *contents*, rename to `.imod`).

```text
my_mod.imod
  inox.toml                             the declarations (required)
  assets/<id>/textures/block/*.png      block textures
  assets/<id>/textures/item/*.png       item textures
  assets/...                            any other resource-pack file (models, sounds, lang...)
  data/...                              any other data-pack file (recipes, tags, loot, worldgen...)
```

Everything outside `inox.toml`, `assets/` and `data/` is ignored.

## `inox.toml` reference

Unknown keys are errors everywhere, so a typo fails the load instead of being silently ignored.

### `[mod]`

| Key | Type | Required | Meaning |
|---|---|---|---|
| `id` | string | yes | `[a-z0-9_]`, 1 to 64 characters. The mod's namespace (`<id>:ruby`), its switch in Options > Mods (`mod.<id>` in `horizon-mods.txt`) and its log name. |
| `name` | string | yes | Shown in Options > Mods. |
| `version` | string | yes | Shown after the name. |
| `description` | string | no | Shown under it. |
| `authors` | list of strings | no | |
| `namespaces` | list of strings | no | Extra namespaces your *own* `assets/` and `data/` files use, as a `.hmod`'s (see [mods.md](mods.md)). Not `minecraft`, `c` or `horizon`. Declared blocks and items are always `<id>:<name>`. |

The game treats an Inox mod as a content mod with `api = "0.2"`, `side = "both"` and capability
`registries` ("May add blocks and items to your worlds"); you don't write those.

### `[block.<name>]`

`<name>` is `[a-z0-9_]`, 1 to 64 characters; the block's id is `<id>:<name>`.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `template` | string | required | The vanilla block it **behaves as** (`minecraft:iron_block`). Its shape, collision, states, sounds, map colour, tool needs and behaviour are copied; the keys below change some of them. Must be `minecraft:...`. |
| `name` | string | `<name>` in title case | English name (`ruby_block` → "Ruby Block"). |
| `texture` | string | `<name>` | The texture on every face: `ruby` means `assets/<id>/textures/block/ruby.png`, which must exist. With a `:` it is a full texture id and nothing is checked (`minecraft:block/stone`). |
| `model` | `cube_all` \| `none` | `cube_all` | `cube_all`: Inox writes a blockstate and a one-texture cube model. `none`: you ship `assets/<id>/blockstates/<name>.json` and its models yourself (needed for stairs, logs, slabs: any template with states). |
| `hardness` | number | template's | Seconds-to-mine base; `-1` is unbreakable, otherwise 0 or more. |
| `explosion_resistance` | number | template's | 0 or more. |
| `light` | integer | template's | Light it gives, 0 to 15. |
| `requires_tool` | bool | template's | Whether it drops only when mined with the right tool. |
| `friction` | number | template's | Slipperiness, 0 to 2 (ice is 0.98, most blocks 0.6). |
| `map_color` | integer | template's | A `MapColor` id, 0 to 61. |
| `sounds` | string | template's | The vanilla block whose sounds it plays (`minecraft:amethyst_block`). |
| `item` | bool | `true` | Whether it has an item (placeable, `/give`-able, in the creative inventory). The item's name is the block's. |
| `drops` | `self` \| `none` \| item id | `self` | What breaking it drops (when it survives an explosion). `self` needs `item = true`. |
| `tool` | `pickaxe` \| `axe` \| `shovel` \| `hoe` | none | Adds it to `minecraft:mineable/<tool>`: that tool mines it fast. |
| `tier` | `stone` \| `iron` \| `diamond` | none | Adds it to `minecraft:needs_<tier>_tool`. With `requires_tool`, lower tiers break it without drops. |

### `[item.<name>]`

`<name>` follows the same rule; the item's id is `<id>:<name>`. A name can't be both a block and an item
(a block's item already has it).

| Key | Type | Default | Meaning |
|---|---|---|---|
| `template` | string | required | The vanilla item it **behaves as** (`minecraft:diamond_sword` swings and fights like one). All its data components are copied. Must be `minecraft:...`. |
| `name` | string | `<name>` in title case | English name. |
| `texture` | string | `<name>` | `assets/<id>/textures/item/<texture>.png` (must exist), or a full texture id with `:`. |
| `model` | `generated` \| `handheld` \| `none` | `generated` | `generated`: a flat item. `handheld`: held like a tool. `none`: you ship `assets/<id>/items/<name>.json` and its models. |
| `max_stack_size` | integer | template's | 1 to 99. |
| `rarity` | `common` \| `uncommon` \| `rare` \| `epic` | template's | The name's colour. |
| `components` | table | none | Data components set or removed, in 26.3's data component patch form: `"minecraft:max_damage" = 900` sets one, `"!minecraft:repairable" = {}` removes one. Only component types some vanilla item has; not `max_stack_size` or `rarity` (use the keys above). |

Components are how an item gets its own numbers:

```toml
[item.ruby_sword]
template = "minecraft:iron_sword"
model = "handheld"
components = { "minecraft:max_damage" = 900, "minecraft:attribute_modifiers" = [
  { type = "minecraft:attack_damage", id = "minecraft:base_attack_damage", amount = 7.0,
    operation = "add_value", slot = "mainhand" },
] }
```

Behaviour the game ties to a vanilla item's *id* doesn't come along: a copy of the trident doesn't throw
([#3305](https://github.com/horizon-rust/horizon/issues/3305)).

## What Inox writes

For each declaration, the files a hand-made content mod would hold. `<id>` is the mod id, `<n>` the name.

| Declaration | File | Content |
|---|---|---|
| block | `data/<id>/horizon/block/<n>.json` | `template`, `properties` (the numbers you set), `item` |
| block, `model = "cube_all"` | `assets/<id>/blockstates/<n>.json` | one variant `""` → `<id>:block/<n>` |
| | `assets/<id>/models/block/<n>.json` | parent `minecraft:block/cube_all`, texture `all` |
| block, `item = true` | `assets/<id>/items/<n>.json` | item model = the block model |
| block, `drops != "none"` | `data/<id>/loot_table/blocks/<n>.json` | one roll of the drop, `survives_explosion` |
| block, `tool` | `data/minecraft/tags/block/mineable/<tool>.json` | values += `<id>:<n>` |
| block, `tier` | `data/minecraft/tags/block/needs_<tier>_tool.json` | values += `<id>:<n>` |
| item | `data/<id>/horizon/item/<n>.json` | `template`, `max_stack_size`, `rarity`, `components` |
| item, `model != "none"` | `assets/<id>/models/item/<n>.json` | parent `minecraft:item/generated` or `handheld`, `layer0` |
| | `assets/<id>/items/<n>.json` | → `<id>:item/<n>` |
| every block and item | `assets/<id>/lang/en_us.json` | `block.<id>.<n>` / `item.<id>.<n>` = name |

**Your files win.** If the mod holds a file at one of these paths, Inox writes nothing there: ship
`assets/<id>/models/block/ruby_block.json` and that is the model used. Two files merge instead:

- `assets/<id>/lang/en_us.json`: your names are kept, Inox adds the ones you didn't write.
- tags (`mineable/*`, `needs_*_tool`): your values first, then Inox's.

Generated files are made in memory each start; nothing is written into your mod.

## Textures and models

- Block and item textures are 16x16 PNGs, as in a resource pack.
- `texture = "minecraft:block/amethyst_block"` reuses a vanilla texture: no file needed. The template mod
  from `cargo horizon-mod new --template inox` does this so it runs with no PNG.
- A block that isn't a plain cube, or a template with states (stairs, slabs, logs, doors, crops): set
  `model = "none"` and ship a blockstate covering the template's states, as a resource pack does. The
  states are the template's: a copy of `minecraft:oak_log` has `axis`.
- Translations other than English: ship `assets/<id>/lang/<lang>.json` yourself.

## Recipes, tags, loot and anything else

Everything a `.hmod` may hold, an `.imod` may hold, under the same fence: files in your own namespace
(`assets/<id>/`, `data/<id>/`, and those in `namespaces`), plus additions to vanilla tags
(`data/minecraft/tags/`) and the convention tags (`data/c/tags/`). So a recipe is a vanilla recipe file:

```json
// data/gems/recipe/ruby_block.json
{ "type": "minecraft:crafting_shaped",
  "category": "building",
  "pattern": ["###", "###", "###"],
  "key": { "#": "gems:ruby" },
  "result": { "id": "gems:ruby_block" } }
```

Worldgen (ores, structures), advancements, extra loot and tags work the same way; see
[modpacks.md](modpacks.md) for what the game reads.

## Installing and playing

1. Put the `.imod` (or the mod folder) in `horizon-mods/` in the game folder (the one holding
   `options.txt`; the launcher's instances each have one).
2. Options > **Mods** lists it with "May add blocks and items to your worlds" and a switch. Mods start
   **off**.
3. Turn it on and **restart**: blocks and items load once, before the resources.
4. A mod that can't load shows why in red on its row, and the log says
   `Horizon: mod <id> content not loaded: <reason>`.

While making a mod, `--dev-mod PATH` loads it always on, folder or file:

```sh
horizon-client --dev-mod ./gems --creative --world Test
cargo horizon-mod run -- --creative --world Test       # the same after a build
```

Use a test world while you make a mod: a world saved with a mod's blocks expects that mod on.

## How it loads

For contributors. Inox lives in `crates/horizon-mods/src/imod.rs` and plugs into the content-mod path,
so the game needs no Inox-specific code:

1. `horizon_mods::find` lists `horizon-mods/`: `.hmod` and `.imod` files, folders with
   `horizon-mod.toml` or `inox.toml`.
2. `Package::read` sees an Inox mod (`imod::is_imod`), parses `inox.toml` (`imod::parse`) and builds the
   `Manifest` a content mod would have (`api = "0.2"`, `registries`), checked by `Manifest::validated`, the
   same checks as `horizon-mod.toml`'s. Options > Mods lists it from that.
3. `Content::read` lists the mod's own `assets/` and `data/` files, then `imod::generate` writes the
   generated ones in memory (the mod's files winning, lang and tags merged) and adds them to the list.
   `Content::reader` serves generated files first, then the mod's. Then the usual checks run: the fence,
   the block and item declarations, templates, components.
4. `game/mods.rs` `content_root` and `mod_content::resource_root` build the resource overlay exactly as
   for a `.hmod`: each block gets its template's table rows (states after vanilla's), each item its
   template's components, tags merge, assets and data join the root.

`cargo horizon-mod new --template inox`, `build` (writes `target/horizon-mod/<id>.imod` with
`inox.toml` and your own files; generated files are made again by the game) and `run` handle Inox mods.
Tests: `cargo test -p horizon-mods imod` and `cargo test -p cargo-horizon-mod`.

## Rules and limits

- Mod id and names: `[a-z0-9_]`, 1 to 64 characters.
- Templates must be vanilla (`minecraft:...`); a mod can't copy another mod's block yet.
- Two mods that are on never share a namespace: the first by file name loads, the other is left out with
  the reason in the log.
- At most 65,536 files, 16 MiB each, 512 MiB in all (generated files count). `inox.toml` at most 1 MiB.
- Vanilla's own files can't be replaced, only its tags added to.

## Errors

The load fails with the first problem, in Options > Mods and the log:

| Message (excerpt) | Fix |
|---|---|
| `inox.toml: unknown field "colour"` | A typo or a key that doesn't exist; see the tables above. |
| `inox.toml: id "Gems" is not [a-z0-9_]{1,64}` | Lowercase, digits, `_`. |
| `inox.toml: name "Ruby" is not [a-z0-9_]{1,64}` | A `[block.x]`/`[item.x]` name; the display name goes in `name =`. |
| `block gems:ruby_block: texture assets/gems/textures/block/ruby_block.png is missing` | Add the PNG, or set `texture`. |
| `x is both a block and an item` | Rename one. |
| `drops itself but has no item (item = false): set drops` | `drops = "none"` or an item id. |
| `tool "sword" is pickaxe, axe, shovel or hoe` | |
| `template "x" is not a vanilla block (minecraft:...)` | |
| `component "minecraft:foo" is not a vanilla type` | Only 26.3 component types. |
| `max_stack_size is 1 to 99`, `light is 0 to 15`... | Range checks. |

## A complete example

```text
gems/
  inox.toml
  assets/gems/textures/block/ruby_block.png
  assets/gems/textures/block/ruby_ore.png
  assets/gems/textures/item/ruby.png
  assets/gems/textures/item/ruby_sword.png
  data/gems/recipe/ruby_block.json
  data/gems/recipe/ruby_sword.json
```

```toml
[mod]
id = "gems"
name = "Gems"
version = "1.0.0"
description = "Rubies: an ore, a block and a sword."
authors = ["you"]

[block.ruby_ore]
template = "minecraft:iron_ore"
name = "Ruby Ore"
drops = "gems:ruby"
tool = "pickaxe"
tier = "iron"

[block.ruby_block]
template = "minecraft:iron_block"
hardness = 5.0
light = 2
sounds = "minecraft:amethyst_block"
tool = "pickaxe"
tier = "iron"

[item.ruby]
template = "minecraft:diamond"
rarity = "uncommon"

[item.ruby_sword]
template = "minecraft:iron_sword"
model = "handheld"
components = { "minecraft:max_damage" = 900 }
```

## What comes next

Inox is meant to grow into a full loader, step by step, each step built into the game rather than
patched in:

1. **Now:** blocks and items declared in data (this page).
2. Behaviour hooks for declared blocks and items (use, break, attack, place), called by the game itself.
3. Code in `.imod`s: a sandboxed runtime again (the removed WebAssembly runtime is the starting point),
   with a registry API instead of templates.
4. Block entities, entities and fluids, once the game has registries for them.
