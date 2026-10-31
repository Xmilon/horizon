# Rust mods (experimental API 0.1)

A Rust mod is a Cargo library compiled together with Horizon. `inox.toml` identifies the mod and its
entry point; Rust defines registries and behavior. Assets and data keep Minecraft's usual directory
layout. Existing data-only Inox `.imod`, content `.hmod`, and native mixin `.hnmod` mods remain supported.

```text
ruby/
  Cargo.toml
  Cargo.lock                  # when using Cargo directly
  inox.toml
  src/lib.rs
  src/registries.rs            # optional: ordinary Rust modules
  assets/ruby/blockstates/
  assets/ruby/models/
  assets/ruby/textures/
  assets/ruby/lang/
  data/ruby/recipe/
  data/ruby/loot_table/
  data/ruby/tags/
```

This is native code, including dependency build scripts and procedural macros, with the player's
permissions. Inspecting, packaging, installing, and checking source do not execute it. Compiling or
running requires `--trust-native-code`. A Rust `.imod` contains source, not a DLL or a stable Rust ABI.

## Start a mod

Build the developer tool from this checkout (`cargo build -p cargo-horizon-mod`). Then:

```sh
cargo horizon-mod new ../ruby --template rust --api-path /path/to/horizon/crates/horizon-mod-api
cd ../ruby
cargo horizon-mod build
cargo horizon-mod check target/horizon-mod/ruby.imod
cargo horizon-mod native-build --workspace /path/to/horizon --trust-native-code
```

`--api-path` supplies a local SDK dependency for the editor; the native compiler always supplies the
SDK from the chosen Horizon source. The template is also the reference mod: a typed materials registry,
a wand, a two-state machine, persisted machine ticks, an inventory, a custom screen, a command and a
Config setting. The template source is `crates/cargo-horizon-mod/template/rust/lib.rs`, exercised by
the SDK's integration tests.

The output is under `target/horizon-native/target/fast/`. Source is copied to a disposable directory;
the original Horizon checkout is never patched. The target cache and the combined Cargo lockfile are
kept under `target/horizon-native/`. Use `--out <directory>` when the mod itself is inside the Horizon
checkout: staging must be outside the source tree. Add `--mod <folder-or-imod>` for each dependency mod.
Profiles are `dev`, `fast` (default), and `release`.

`cargo horizon-mod run --workspace <checkout> --trust-native-code -- <game arguments>` compiles and
starts the result. Choose a separate game directory for development. Packaging and `check` validate
metadata, ownership, paths, sizes, and native classification; compilation checks Rust code.

## Metadata and dependencies

```toml
[mod]
id = "ruby"
name = "Ruby"
version = "0.1.0"             # must match Cargo.toml
authors = ["Your name"]

[rust]
api = "0.1"
entry = "create"              # public fn() -> Box<dyn horizon_mod_api::Mod>

[rust.dependencies]
another_mod = "1.2.0"         # exact mod version, or "*"
```

Dependencies are checked and sorted before any mod registers. Missing mods, wrong versions, cycles,
duplicate mod IDs and namespace conflicts fail startup. Rust library dependencies belong in
`Cargo.toml`; source packages use registry dependencies. The SDK dependency may have a local path,
which staging replaces. In regular `[dependencies]`, a dependency named for an ID in `[rust.dependencies]` is linked to that selected mod's library. Declare it in Cargo too (for example `ruby = "0.1"`) to import its public Rust types; the compiler supplies its staged path. Other path and Git dependencies and workspace inheritance are refused so
the packaged project cannot quietly depend on files outside its archive. Shared public API crates can also be ordinary Rust library dependencies.

## Registration lifecycle

1. Discover compiled mods and check API versions and mod dependencies.
2. Call every mod's `define_registries` in dependency order.
3. Call `register` in the same order, collecting Rust entries, content, machines, commands and settings.
4. Read typed custom-registry JSON from compiled assets and installed content mods.
5. Resolve declared `EntryRef`s, validate content and freeze registries.
6. Build the resource overlay and load the engine's block, item, rendering and networking tables.
7. Check the world's saved contract before migrations or chunk reads, then start gameplay callbacks.

Registration runs once per process. Restart after changing mods or registry data. Frozen registries
have no mutation API, can be read across threads, and store typed values. Mod callbacks use `&self`;
any transient mutable mod state must be synchronized and reset on `WorldReady`. Durable machine state
belongs in the machine's chunk data.

## Define a registry

```rust
use horizon_mod_api::{RegistryBuilder, RegistryKey, Result};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material { pub power: u32 }

pub const MATERIALS: RegistryKey<Material> = RegistryKey::new("ruby:materials");

fn define(owner: &str, registries: &mut RegistryBuilder) -> Result {
    registries.create(owner, MATERIALS, 1)?;
    registries.insert(owner, MATERIALS, "ruby:ruby", Material { power: 10 })?;
    Ok(())
}
```

`create` takes the owning namespace, a typed key, and a nonzero schema version. `insert` checks type,
namespace and duplicate IDs. Other mods may add entries in their own namespaces to this registry.
`registries.get(MATERIALS, "ruby:ruby")` returns `&Material`; a wrong type or missing entry is an error.
Put references in `Registration.references` to validate them before gameplay. Saved references use
`EntryRef { registry, id }`, never a runtime array index.

A data-only addon can contribute to that registry with
`data/addon/ruby/materials/sapphire.json`, for example `{"power":25}`. Values pass the same typed
Serde decoder as Rust entries. Data never overrides an existing entry. The file path is
`data/<entry namespace>/<registry namespace>/<registry path>/<entry path>.json`.

Defining a registry creates a typed table. An engine system must use that table for it to affect
gameplay; declaring `ruby:entities` alone does not implement entity simulation or rendering.

## Blocks, items and existing Inox declarations

`Registration.content` is an `InoxFile` built in Rust. All existing block/item, ore, armor, tool, tag,
tab, food, equipment, recipe and resource-pack functionality continues through the existing generator
and validation. Rust declarations do not serialize to TOML and parse back.

For a block with its own state space, call `Registration.block(name, appearance, NativeBlock)`.
`NativeBlock` declares properties and their unique values, a complete default state, and collision
boxes. State combinations are sorted deterministically, with at most 4096 per block and the existing
global block-state limit. `Registration.item(name, ItemDecl)` creates a normal item with its own
components. Neither method requires the author to choose a vanilla gameplay template.

The exported engine tables still need neutral solid-block and ordinary-item rows internally; the
adapter expands those rows into the mod's own states, collision, defaults, models and protocol IDs.
Behavior comes from Rust callbacks. Raw `content.block`/`content.item` declarations retain the old
template convenience API. State-specific models live in the mod's blockstate/model assets. Collision
boxes currently apply to all of a native block's states.

## Events, machines and screens

Implement `Mod::event` for world readiness/close, authoritative server ticks, item use, block use,
placement/destruction, commands, keys and Config clicks. Return `Action::Handled` to claim a supported
interaction (or cancel breaking). Commands use the existing command tree and operator levels.
`ServerTick.world_running` distinguishes frozen ticks. Client key/config events cannot mutate worlds.

The borrowed `Host` checks server authority, world height, loaded chunks, valid states, registered
settings and inventory results. It exposes block reads/writes, feedback, item giving and settings.
It does not expose the private `Game`. A callback cannot keep the host past its lifetime.

Register a `Machine` against a declared block. Its NBT compound and item slots use the existing chunk
save, removal, hopper, comparator and synchronized container handling. A native block's engine facts
are marked as a block entity, so no chest/furnace gameplay template is needed. `machine_tick` receives
borrowed `MachineState`; update `data`, `items` and the menu's data slots there. Data slots are transient
and 16-bit on the wire; NBT is persisted with the chunk. Item counts and validity remain the mod's
responsibility when it edits its slots directly.

Menus use the existing slot grids, hopper and furnace layouts; the slot count must match. Override
`screen`, `draw_screen`, `screen_button`, `screen_tooltip` and `machine_button` for custom layouts,
text, fills, GUI sprites and server-side buttons. The existing native screen machinery sends button
clicks and menu data; clients do not modify authoritative machine state. `Setting`s and `KeyBinding`s
appear in the existing in-game Config page. There are no game-side enable switches.

## Launcher builds and compatibility

```sh
horizon-launcher mods install /path/to/ruby.imod
horizon-launcher native-build --source /path/to/horizon --trust-native-code
```

Rust source installs into `horizon-native-mods/`. Enabling/disabling changes the next native build's
selection, as for `.hnmod`. Data-only packages still move between `horizon-mods/` and
`horizon-mods-disabled/`. Import/export already carries both directories. No launcher Mods/browser
page UI is changed for this API.

The compiler stages Cargo projects, adds library dependencies and entrypoint glue, and embeds their
assets. It preserves the existing mixin path when `.hnmod` mods are selected alongside Rust projects.
The native build key includes source inputs, the selected Horizon source, toolchain/profile and the
resolved combined lockfile. Compilation uses that lockfile. An unreleased SDK needs `--source`;
an upstream Horizon checkout without this SDK fails with an explanation.

Every modded world gets `horizon-rust-mods.json`. Before loading or migrating it, Horizon checks saved
mod versions/source fingerprints and registry schemas/values. Missing or changed mods, entries or
schemas refuse the open instead of silently replacing content. This is deliberately conservative:
updating a saved world's Rust mod currently requires restoring the exact source build or writing an
explicit migration outside this API. No automatic schema migration is claimed.

Multiplayer extends the existing registry compatibility handshake with the mod/source contract and
custom registry data. Mismatched builds fail before chunk gameplay. Custom registries use the existing
configuration registry packets, always with their entry data even if the vanilla pack is known.
Both peers must have the same compiled code and registry contract; server-only downloadable Rust code
and replacing a connected client's frozen typed registries are not supported.

This is the first Rust API milestone, not NeoForge parity. Independent entity and fluid engine
adapters, dynamic binary loading, automatic save migrations and Java mod execution are separate work.
