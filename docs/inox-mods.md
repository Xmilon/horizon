# Inox mods (`.imod`)

Inox is Horizon's built-in mod loader for **content**: blocks, items, tools and weapons, food, armour sets
and ore generation. A mod is one `inox.toml` that declares them, plus the mod's textures and any vanilla
data files it wants (recipes, advancements...). The game reads it at start, writes the vanilla files a
content mod would need (declarations, blockstates, models, item definitions, names, loot tables, tags,
worldgen features, equipment assets), and registers everything through the same loader as `.hmod`
content mods ([mods.md](mods.md), [modpacks.md](modpacks.md)). No code runs and nothing is compiled: an
`.imod` is as safe as a resource pack.

Inox is the first step towards a full mod loader. Behaviour that needs code (a block that does something
new when used, a new entity, a fluid) comes later; see [What comes next](#what-comes-next).

- [Quick start](#quick-start)
- [The file](#the-file)
- [`inox.toml` reference](#inoxtoml-reference): [`[mod]`](#mod), [`[block.<name>]`](#blockname),
  [`[item.<name>]`](#itemname), [`[ore.<name>]`](#orename), [`[armor.<name>]`](#armorname)
- [What Inox writes](#what-inox-writes), and how your own files win or merge
- [Textures and models](#textures-and-models), [armour textures](#armour-textures)
- [Recipes, tags, loot and anything else](#recipes-tags-loot-and-anything-else)
- [Biome modifiers](#biome-modifiers) (worldgen by hand)
- [Installing and playing](#installing-and-playing)
- [How it loads](#how-it-loads) (for contributors)
- [Rules and limits](#rules-and-limits)
- [Errors](#errors)
- [A complete example: Ruby](#a-complete-example-ruby)
- [What comes next](#what-comes-next)

## Quick start

```sh
cargo install --git https://github.com/Xmilon/horizon --branch inox-loader cargo-horizon-mod
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
  inox.toml                                        the declarations (required)
  assets/<id>/textures/block/*.png                 block textures
  assets/<id>/textures/item/*.png                  item textures
  assets/<id>/textures/entity/equipment/...        armour textures
  assets/...                                       any other resource-pack file (models, sounds, lang...)
  data/...                                         any other data-pack file (recipes, tags, loot, worldgen...)
```

Everything outside `inox.toml`, `assets/` and `data/` is ignored (a `tools/` folder with your own
scripts is fine).

## `inox.toml` reference

Unknown keys are errors everywhere, so a typo fails the load instead of being silently ignored. Names
(`<name>` in `[block.<name>]`, `[item.<name>]`, `[ore.<name>]`, `[armor.<name>]`) are `[a-z0-9_]`, 1 to 64
characters.

### `[mod]`

| Key | Type | Required | Meaning |
|---|---|---|---|
| `id` | string | yes | `[a-z0-9_]`, 1 to 64 characters. The mod's namespace (`<id>:ruby`), its switch in Options > Mods (`mod.<id>` in `horizon-mods.txt`) and its log name. |
| `name` | string | yes | Shown in Options > Mods. |
| `version` | string | yes | Shown after the name. |
| `description` | string | no | Shown under it. |
| `authors` | list of strings | no | |
| `namespaces` | list of strings | no | Extra namespaces your *own* `assets/` and `data/` files use, as a `.hmod`'s (see [mods.md](mods.md)). Not `minecraft`, `c` or `horizon`. Declared things are always `<id>:<name>`. |

The game treats an Inox mod as a content mod with `api = "0.2"`, `side = "both"` and capability
`registries` ("May add blocks and items to your worlds"); you don't write those.

### `[block.<name>]`

The block's id is `<id>:<name>`.

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
| `tier` | `stone` \| `iron` \| `diamond` | none | Adds it to `minecraft:needs_<tier>_tool`. With `requires_tool` (an ore template has it), lower tiers break it without drops. |
| `tags` | list of tag ids | none | Block tags it joins: `minecraft:beacon_base_blocks`, `c:ores`, `<id>:...`. Vanilla's, the convention `c:` ones and your own. |

### `[item.<name>]`

The item's id is `<id>:<name>`. A name can't be both a block and an item (a block's item already has
it).

| Key | Type | Default | Meaning |
|---|---|---|---|
| `template` | string | required | The vanilla item it **behaves as** (`minecraft:diamond_sword` swings and fights like one, `minecraft:golden_apple` is eaten). All its data components are copied. Must be `minecraft:...`. |
| `name` | string | `<name>` in title case | English name. |
| `texture` | string | `<name>` | `assets/<id>/textures/item/<texture>.png` (must exist), or a full texture id with `:`. |
| `model` | `generated` \| `handheld` \| `none` | `generated` | `generated`: a flat item. `handheld`: held like a tool. `none`: you ship `assets/<id>/items/<name>.json` and its models. |
| `max_stack_size` | integer | template's | 1 to 99. |
| `rarity` | `common` \| `uncommon` \| `rare` \| `epic` | template's | The name's colour. |
| `durability` | integer | template's | Uses before it breaks (`minecraft:max_damage`). Iron sword 250, diamond 1561, netherite 2031. |
| `attack_damage`, `attack_speed` | numbers | template's | As the tooltip shows them: an iron sword is 6 and 1.6, a diamond sword 7 and 1.6, a netherite axe 10 and 1. **Both or neither**: they replace the template's main-hand `minecraft:attribute_modifiers`. |
| `repair` | item id or `#`tag | template's | What repairs it in an anvil (`minecraft:repairable`). |
| `enchantability` | integer | template's | How well it enchants (`minecraft:enchantable`): iron tools 14, diamond 10, gold 22. |
| `food` | table | none | Makes it edible: see below. |
| `tags` | list of tag ids | none | Item tags it joins. A sword should join `minecraft:swords` (Sharpness, Looting and the like look for it); an ingot `c:ingots`. |
| `components` | table | none | Any other data component set or removed, in 26.3's data component patch form: `"minecraft:max_damage" = 900` sets one, `"!minecraft:repairable" = {}` removes one. Only component types some vanilla item has; not `max_stack_size` or `rarity` (use the keys above), nor one a key above already sets. |

`food`:

| Key | Type | Default | Meaning |
|---|---|---|---|
| `nutrition` | integer | required | Hunger points (half drumsticks): bread 5, steak 8, golden apple 4. |
| `saturation` | number | required | Saturation points: bread 6.0, steak 12.8, golden apple 9.6. |
| `always_eat` | bool | `false` | Edible with a full hunger bar, as a golden apple. |
| `eat_seconds` | number | 1.6 | Time to eat it. |
| `effects` | list | none | `{ id = "minecraft:speed", seconds = 30, level = 2, chance = 1.0 }`: a vanilla effect, how long, its level (1 is I) and the chance it is given (1 by default). |

A tool or weapon is a copy with its own numbers:

```toml
[item.ruby_sword]
template = "minecraft:diamond_sword"
model = "handheld"
durability = 1200
attack_damage = 7.5
attack_speed = 1.6
repair = "ruby:ruby_ingot"
tags = ["minecraft:swords"]
```

A pickaxe copies `minecraft:iron_pickaxe` (and joins `minecraft:pickaxes`); what it can mine stays the
template's (`minecraft:tool`), which `components` can replace.

Behaviour the game ties to a vanilla item's *id* doesn't come along: a copy of the trident doesn't throw
([#3305](https://github.com/horizon-rust/horizon/issues/3305)). Behaviour that comes from data
components does: food, effects, durability, attributes, equipping, repairing, enchanting.

### `[ore.<name>]`

An ore vein that generates in the world: a configured feature and a placed feature `<id>:<name>`, and
a [biome modifier](#biome-modifiers) that adds it to the biomes' ores. New chunks get it; chunks already
generated don't.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `block` | block | required | The block it puts in stone, granite, diorite and andesite (`minecraft:stone_ore_replaceables`): a block of this mod by name (`ruby_ore`), or an id. |
| `deepslate_block` | block | none | The block it puts in deepslate and tuff (`minecraft:deepslate_ore_replaceables`). Without it the vein skips them, as emerald ore's does. |
| `size` | integer | required | Blocks in a vein, 1 to 64 (coal 17, iron 9, gold 9, diamond 4 to 12). |
| `count` | integer | required | Veins tried per chunk (iron's middle band 10, gold 4, diamond 7). |
| `min_y`, `max_y` | integers | required | The lowest and highest Y a vein starts at (the overworld is -64 to 319). |
| `shape` | `uniform` \| `triangle` | `uniform` | `uniform`: every height as likely. `triangle`: most veins in the middle of the range, fewer at its ends, as iron's and diamond's. |
| `discard_on_air` | number | 0 | 0 to 1: the chance a block of the vein that touches air is skipped. 0.5 hides most of it from caves (diamond's). |
| `biomes` | id, `#`tag or list | `#minecraft:is_overworld` | Where it generates: `#minecraft:is_overworld`, `#minecraft:is_nether`, `minecraft:desert`, `["minecraft:badlands", "#minecraft:is_mountain"]`. |

```toml
[ore.ruby_ore]
block = "ruby_ore"
deepslate_block = "deepslate_ruby_ore"
size = 6
count = 5
min_y = -56
max_y = 40
shape = "triangle"
```

A nether ore: `block = "nether_ruby_ore"` with a block copying `minecraft:nether_gold_ore`, and
`biomes = "#minecraft:is_nether"`. The vein replaces only stone-like and deepslate-like blocks, so for
netherrack write the features yourself (vanilla's `minecraft:ore` with a `minecraft:block_match` on
`minecraft:netherrack`) and a [biome modifier](#biome-modifiers).

### `[armor.<name>]`

An armour set: `<id>:<name>_helmet`, `_chestplate`, `_leggings` and `_boots`, each copying the iron
piece (equipping, the slot, trims), with its own numbers, and the equipment asset `<id>:<name>` that
draws it on players and armour stands.

| Key | Type | Default | Meaning |
|---|---|---|---|
| `durability` | integer | required | The durability multiplier of vanilla's armour materials: leather 5, iron 15, diamond 33, netherite 37. A helmet lasts 11 times it, a chestplate 16, leggings 15, boots 13. |
| `defense` | 4 integers | required | Armour points of the helmet, chestplate, leggings and boots. Iron `[2, 6, 5, 2]`, diamond `[3, 8, 6, 3]`. |
| `toughness` | number | 0 | Armour toughness of each piece: diamond 2, netherite 3. |
| `knockback_resistance` | number | 0 | 0 to 1 per piece: netherite 0.1. |
| `name` | string | `<name>` in title case | The set's name before each piece's: `Ruby` makes "Ruby Helmet". |
| `repair` | item id or `#`tag | iron armour's | What repairs it. |
| `enchantability` | integer | iron's (9) | Diamond 10, gold 25, netherite 15. |
| `equip_sound` | sound id | `minecraft:item.armor.equip_iron` | Played when a piece is put on. |
| `rarity` | string | template's | |
| `pieces` | list | all four | Some of `helmet`, `chestplate`, `leggings`, `boots`. |

Each piece joins `minecraft:<slot>_armor` (`head_armor`, `chest_armor`, `leg_armor`, `foot_armor`: what
Protection, Thorns and the like look for) and `minecraft:trimmable_armor`. Its textures:
`assets/<id>/textures/item/<name>_<piece>.png` for the item, and the [armour textures](#armour-textures)
for the body.

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
| block or item, `tags` | `data/<ns>/tags/block/<path>.json`, `.../item/...` | values += `<id>:<n>` |
| item | `data/<id>/horizon/item/<n>.json` | `template`, `max_stack_size`, `rarity`, `components` (from the keys and `components`) |
| item, `model != "none"` | `assets/<id>/models/item/<n>.json` | parent `minecraft:item/generated` or `handheld`, `layer0` |
| | `assets/<id>/items/<n>.json` | → `<id>:item/<n>` |
| ore | `data/<id>/worldgen/configured_feature/<n>.json` | `minecraft:ore`: `size`, `discard_chance_on_air_exposure`, the targets |
| | `data/<id>/worldgen/placed_feature/<n>.json` | `count`, `in_square`, `height_range` (`uniform` or `trapezoid`), `biome` |
| | `data/<id>/neoforge/biome_modifier/<n>.json` | `neoforge:add_features`, step `underground_ores` |
| armour set | the item files above for each piece | template `minecraft:iron_<piece>`; `max_damage`, `attribute_modifiers` (`armor`, `armor_toughness`, `knockback_resistance`), `equippable` (slot, `asset_id` `<id>:<n>`, sound), `repairable`, `enchantable` |
| | `assets/<id>/equipment/<n>.json` | layers `humanoid` and `humanoid_leggings` → texture `<id>:<n>` |
| every block and item | `assets/<id>/lang/en_us.json` | `block.<id>.<n>` / `item.<id>.<n>` = name |

**Your files win.** If the mod holds a file at one of these paths, Inox writes nothing there: ship
`assets/<id>/models/block/ruby_block.json` and that is the model used. Two kinds of file merge instead:

- `assets/<id>/lang/en_us.json`: your names are kept, Inox adds the ones you didn't write.
- tags: your values first, then Inox's.

Generated files are made in memory each start; nothing is written into your mod.

## Textures and models

- Block and item textures are 16x16 PNGs, as in a resource pack.
- `texture = "minecraft:block/amethyst_block"` reuses a vanilla texture: no file needed. The template mod
  from `cargo horizon-mod new --template inox` does this so it runs with no PNG.
- A block that isn't a plain cube, or a template with states (stairs, slabs, logs, doors, crops): set
  `model = "none"` and ship a blockstate covering the template's states, as a resource pack does. The
  states are the template's: a copy of `minecraft:oak_log` has `axis`.
- Translations other than English: ship `assets/<id>/lang/<lang>.json` yourself.

### Armour textures

An armour set needs, besides its four item textures, the textures drawn on the body, laid out as
vanilla's 64x32 armour layers:

| File | Drawn for | Used regions (x, y, w, h) |
|---|---|---|
| `assets/<id>/textures/entity/equipment/humanoid/<name>.png` | helmet, chestplate, boots | head 0,0,32,16; body 16,16,24,16; arms 40,16,16,16; legs (boots) 0,16,16,16 |
| `assets/<id>/textures/entity/equipment/humanoid_leggings/<name>.png` | leggings | body (waist) 16,16,24,16; legs 0,16,16,16 |

Each box's faces sit in its region the way a player skin's do. Leave the helmet's outer layer (32,0,32,16)
clear unless you want a second shell. A set without leggings needs no `humanoid_leggings` file, one with
only leggings no `humanoid` file. Vanilla's own layers are the best reference for where each face goes.

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

```json
// data/gems/recipe/ruby_from_smelting.json
{ "type": "minecraft:smelting", "category": "misc",
  "ingredient": "gems:ruby_ore", "result": { "id": "gems:ruby" },
  "experience": 1.0, "cookingtime": 200 }
```

Blasting (`minecraft:blasting`, usually half the time), shapeless (`minecraft:crafting_shapeless` with
`ingredients`), stonecutting and smithing recipes are vanilla's formats too. Advancements, extra loot,
structures and features work the same way; see [modpacks.md](modpacks.md) for what the game reads.

## Biome modifiers

Vanilla biomes list their features in their own files, which a mod may not change. A mod adds to them
with NeoForge's biome modifier format, which Horizon reads from every content mod (`.imod` and `.hmod`),
so NeoForge mods' modifiers carry over as they are. `[ore]` writes one; for any other placed feature
(a custom tree, a flower patch, a nether ore) write it yourself:

```json
// data/<id>/neoforge/biome_modifier/<anything>.json
{ "type": "neoforge:add_features",
  "biomes": "#minecraft:is_overworld",
  "features": "<id>:my_feature",
  "step": "underground_ores" }
```

| Key | Meaning |
|---|---|
| `type` | `neoforge:add_features`. Other types (`remove_features`, `add_spawns`...) are left out with a warning in the log, for now. |
| `biomes` | A biome id, a `#` biome tag (tags in tags included) or a list of them. Biomes nobody has are skipped. |
| `features` | A placed feature id or a list; each must exist (vanilla's or a mod's, from `data/<ns>/worldgen/placed_feature/`). |
| `step` | The decoration step: `raw_generation`, `lakes`, `local_modifications`, `underground_structures`, `surface_structures`, `strongholds`, `underground_ores`, `underground_decoration`, `fluid_springs`, `vegetal_decoration`, `top_layer_modification`. |

The features are appended to the step's list in each biome, after vanilla's and the mods' before, in
load order; one already there isn't added twice.

## Installing and playing

1. Put the `.imod` (or the mod folder) in `horizon-mods/` in the game folder (the one holding
   `options.txt`; the launcher's instances each have one).
2. Options > **Mods** lists it with "May add blocks and items to your worlds" and a switch. Mods start
   **off**.
3. Turn it on and **restart**: content loads once, before the resources.
4. A mod that can't load shows why in red on its row, and the log says
   `Horizon: mod <id> content not loaded: <reason>`, or `Horizon: content mods not loaded: <reason>` for a
   problem found while building the content (an unknown feature in a biome modifier).

While making a mod, `--dev-mod PATH` loads it always on, folder or file:

```sh
horizon-client --dev-mod ./gems --creative --world Test
cargo horizon-mod run -- --creative --world Test       # the same after a build
```

Use a test world while you make a mod: a world saved with a mod's blocks expects that mod on, and ores
only appear in chunks generated after the mod is on.

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
   Armour sets become four item declarations (`armor_items`), item keys become components
   (`item_components`), ores become two features and a biome modifier (`ore_files`).
   `Content::reader` serves generated files first, then the mod's. Then the usual checks run: the fence,
   the block and item declarations, templates, components.
4. `game/mods.rs` `content_root` and `mod_content::build` make the resource overlay exactly as for a
   `.hmod`: each block gets its template's table rows (states after vanilla's), each item its template's
   components with the mod's patch, tags merge, assets and data join the root, features join
   `worldgen/features.json` and `worldgen/placed_features.json`. Then `mod_content::add_features`
   applies every mod's biome modifiers to `worldgen/biomes.json`, once the features and biome tags are in.
5. Armour: `ArmorStandAssets::load` (`game/armor_stands.rs`) reads the equipment assets of `minecraft`
   and of every content mod's namespace, and the GUI atlas (`hud.rs`, `mod_equipment_sprites`) packs the
   mods' `textures/entity/equipment/` as `<ns>:entity/equipment/...` sprites, so the layers draw.

`cargo horizon-mod new --template inox`, `build` (writes `target/horizon-mod/<id>.imod` with
`inox.toml` and your own files; generated files are made again by the game) and `run` handle Inox mods.
Tests: `cargo test -p horizon-mods imod`, `cargo test -p cargo-horizon-mod`, and
`cargo test -p horizon-client --lib mod_content::tests::biome_modifiers`.

## Rules and limits

- Mod id and names: `[a-z0-9_]`, 1 to 64 characters. An armour set's pieces (`<name>_helmet`...) may not
  also be declared.
- Templates must be vanilla (`minecraft:...`); a mod can't copy another mod's block yet.
- Two mods that are on never share a namespace: the first by file name loads, the other is left out with
  the reason in the log.
- At most 65,536 files, 16 MiB each, 512 MiB in all (generated files count). `inox.toml` at most 1 MiB.
- Vanilla's own files can't be replaced, only its tags added to, and its biomes' features through
  biome modifiers.
- Zombies and skeletons wearing a mod's armour (from `/item replace` or spawn gear) draw nothing yet;
  players and armour stands do.

## Errors

The load fails with the first problem, in Options > Mods and the log:

| Message (excerpt) | Fix |
|---|---|
| `inox.toml: unknown field "colour"` | A typo or a key that doesn't exist; see the tables above. |
| `inox.toml: id "Gems" is not [a-z0-9_]{1,64}` | Lowercase, digits, `_`. |
| `inox.toml: name "Ruby" is not [a-z0-9_]{1,64}` | A `[block.x]`/`[item.x]` name; the display name goes in `name =`. |
| `block gems:ruby_block: texture assets/gems/textures/block/ruby_block.png is missing` | Add the PNG, or set `texture`. |
| `armor gems:ruby: texture assets/gems/textures/entity/equipment/humanoid/ruby.png is missing` | Add the armour textures. |
| `x is both a block and an item` | Rename one. |
| `armor ruby makes ruby_boots, which is also declared` | Rename the set or the item. |
| `drops itself but has no item (item = false): set drops` | `drops = "none"` or an item id. |
| `tool "sword" is pickaxe, axe, shovel or hoe` | |
| `set attack_damage and attack_speed together` | Write both. |
| `component minecraft:max_damage is also set by one of its keys` | Use the key (`durability`) or the component, not both. |
| `ore gems:ruby_ore: ruby_ore is not a block of this mod` | Declare the block, or give a full id. |
| `ore ...: size is 1 to 64`, `count is 1 or more and min_y at most max_y` | Range checks. |
| `<file>: placed feature x:y is not defined` | A biome modifier names a feature nobody has. |
| `<file>: step "ores" is one of raw_generation, ...` | Use a step name from the list. |
| `template "x" is not a vanilla block (minecraft:...)` | |
| `component "minecraft:foo" is not a vanilla type` | Only 26.3 component types. |
| `max_stack_size is 1 to 99`, `light is 0 to 15`... | Range checks. |

## A complete example: Ruby

The Ruby mod (in `Horizon Minecraft Mods/ruby/`, beside the fork) has an ore with a deepslate variant
that generates in the overworld, ingots smelted from it, a storage block, a sword, a full armour set and
a custom food, the Ruby Apple:

```text
ruby/
  inox.toml
  tools/make_textures.py                       draws the PNGs below (original art)
  assets/ruby/textures/block/{ruby_ore,deepslate_ruby_ore,ruby_block}.png
  assets/ruby/textures/item/{ruby_ingot,ruby_sword,ruby_apple}.png
  assets/ruby/textures/item/ruby_{helmet,chestplate,leggings,boots}.png
  assets/ruby/textures/entity/equipment/humanoid/ruby.png
  assets/ruby/textures/entity/equipment/humanoid_leggings/ruby.png
  data/ruby/recipe/*.json                      12 recipes: block <-> ingots, sword, armour, apple, smelting and blasting
```

```toml
[mod]
id = "ruby"
name = "Ruby"
version = "1.0.0"
description = "Ruby ore, ingots, a ruby block, sword and armour, and the Ruby Apple."

[block.ruby_ore]
template = "minecraft:iron_ore"
name = "Ruby Ore"
hardness = 3.5
tool = "pickaxe"
tier = "iron"
tags = ["c:ores"]

[block.deepslate_ruby_ore]
template = "minecraft:deepslate_iron_ore"
name = "Deepslate Ruby Ore"
hardness = 5.0
sounds = "minecraft:deepslate"
tool = "pickaxe"
tier = "iron"
tags = ["c:ores"]

[block.ruby_block]
template = "minecraft:iron_block"
name = "Block of Ruby"
hardness = 5.0
light = 2
sounds = "minecraft:amethyst_block"
tool = "pickaxe"
tier = "iron"
tags = ["minecraft:beacon_base_blocks"]

[ore.ruby_ore]
block = "ruby_ore"
deepslate_block = "deepslate_ruby_ore"
size = 6
count = 5
min_y = -56
max_y = 40
shape = "triangle"
discard_on_air = 0.3

[item.ruby_ingot]
template = "minecraft:iron_ingot"
name = "Ruby Ingot"
rarity = "uncommon"
tags = ["c:ingots", "minecraft:beacon_payment_items"]

[item.ruby_sword]
template = "minecraft:diamond_sword"
name = "Ruby Sword"
model = "handheld"
durability = 1200
attack_damage = 7.5
attack_speed = 1.6
repair = "ruby:ruby_ingot"
enchantability = 12
tags = ["minecraft:swords"]

[item.ruby_apple]
template = "minecraft:golden_apple"
name = "Ruby Apple"
rarity = "rare"
food = { nutrition = 6, saturation = 12.0, always_eat = true, effects = [
  { id = "minecraft:regeneration", seconds = 8, level = 2 },
  { id = "minecraft:strength", seconds = 30 },
  { id = "minecraft:fire_resistance", seconds = 60 },
] }

[armor.ruby]
name = "Ruby"
durability = 25
defense = [3, 7, 6, 2]
toughness = 1.0
repair = "ruby:ruby_ingot"
enchantability = 12
rarity = "uncommon"
```

Try it: `/give @s ruby:ruby_sword`, `/give @s ruby:ruby_chestplate`, `/give @s ruby:ruby_apple`, or dig
around Y -8 in a new world for ruby ore. Smelt the ore for ingots.

## What comes next

Inox is meant to grow into a full loader, step by step, each step built into the game rather than
patched in:

1. **Now:** blocks, items, tools, food, armour and ore generation declared in data (this page).
2. More worldgen shorthands (trees, flowers, nether and end ores), recipe shorthands, mob armour.
3. Behaviour hooks for declared blocks and items (use, break, attack, place), called by the game itself.
4. Code in `.imod`s: a sandboxed runtime again (the removed WebAssembly runtime is the starting point),
   with a registry API instead of templates.
5. Block entities, entities and fluids, once the game has registries for them.
