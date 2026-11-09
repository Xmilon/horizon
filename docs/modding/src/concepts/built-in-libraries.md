# Design: built-in libraries (curios, player animation, geckolib)

Provisional proposal, not implemented API. It asks how Horizon ships the equivalents of three Java libraries (Curios, a player animation library such as Player Animation Library, GeckoLib) so that a mod, or a ported Java mod, can depend on them and they cost nothing when nothing does.

## Why

Port-pack classifies all three as `library`, tier `none`, and reports them "NotNeeded" (`crates/horizon-port-pack/src/classify.rs`, `report.rs`). A ported mod that used them loses its accessory slots, emotes and animated models, and the ported manifest keeps no trace of the requirement. The roadmap rows in `docs/modpacks.md` already promise "built-in slots API" and "data models".

## Shape: bundled library mods on the existing dependency model

Each library is an ordinary trait mod (`curios`, `player_animation`, `geckolib`) published by the game's own repository and shipped with the game. A mod asks for one the same way it asks for any mod:

```toml
[dependencies]
curios = "^1"
```

The resolver in `crates/horizon-mod-package/src/dependencies.rs` already provides what this needs: semver requirements, `optional = true` with `mod_api::if_installed!`, dependency-ordered dispatch, and typed errors (`Missing`, `Version`, `Cycle`). A library nothing depends on is not compiled in and not dispatched, so the unused cost is zero without a flag. Nothing library-specific lives in the engine: where a library needs engine reach, the engine gains a general hook or host method that every mod can use.

The game calls the resolver once, in `crates/horizon-client/src/entry.rs` (it builds `dependencies::Mod` inputs and calls `dependencies::resolve`); that is where bundled libraries would join the input list.

Open questions for the resolver (not verified in this note): how a "bundled" source satisfies `Missing` without a download; what happens when a user installs a mod with a bundled id (proposal: the bundled copy wins, an incompatible major version is a `Version` error); how the Mods page shows "built-in library, used by N mods".

Engine-independent parsing and maths go into plain workspace crates (a `horizon-geo` crate for Bedrock `geo.json`, `animation.json` and `animation_controllers.json`; the expression compiler in `crates/horizon-optifine/src/cem/expr.rs` promoted to a shared crate for Molang). They are unit-testable without the game and reusable by resource-pack features.

## Library state is attachments

Per-instance state uses the attachment store, the only Part-data store ([part-instance-state](part-instance-state.md)). Each type has a codec and, where clients need it, `sync: true`.

| Library | Attachment | Holder |
| --- | --- | --- |
| curios | `curios:inventory` (stacks keyed by `slot_id` + index, never by ordinal) | entity, player |
| player animation | `player_animation:layers` | player |
| geckolib | `geckolib:controllers` | entity, block entity |

Queries have no joins and no stable order ([attachment-query-joins](attachment-query-joins.md)); libraries must not depend on iteration order. There is no scheduler: ticking uses hooks.

## Data, not code

Behaviour is declared in files, the way Java libraries mostly do it through datapacks and resources:

- curios: `data/<ns>/curios/slots/*.json` (`size`, `operation` SET/ADD/REMOVE, `order`, `icon`, `add_cosmetic`, `render_toggle`, `drop_rule`, `validators`), `data/<ns>/curios/entities/*.json`, item tags `curios:<slot>`. Field names follow the Java library so ports are mechanical.
- player animation: `assets/<ns>/player_animation/*.json` (bones `head`, `torso`, arms, legs, and `body` for the whole player; offset and rotation; easings). Emotes listed in a registry file.
- geckolib: Bedrock `geo.json`, `animation.json`, and `animation_controllers.json` as the data-driven replacement for code-registered controllers. A binding file `{target, id, geo, texture, animations, controller}` ties them to an entity, item, block or armor.

The libraries are written from the published file formats and the libraries' public documentation. No Java source is ported. Licences of the three libraries are not verified in this note.

## Engine gaps (general hooks, each its own issue)

| Gap | State today | Needed by |
| --- | --- | --- |
| Mod entity types | proposal, #792 ([general-entity-types](general-entity-types.md)) | geckolib entities |
| Entity render-state modifiers | proposal, #897 ([entity-render-state](entity-render-state.md)) | geckolib, player animation |
| Writable pose after setup | `HumanoidPart` is read-only in `mob_layer`; first-person arm bypasses `Poses` (`first_person.rs`) | player animation |
| Animation types from JSON | #880, open | geckolib |
| Extra equipment slots on existing entities | none; menu slot counts are static; `SetEquipment` caps at 127 ordinals | curios |
| Player layer hook on every player path | three duplicated chains (own model, remote players, inventory preview) | curios render, player animation |
| Item tags outside own namespace and `minecraft`/`c` | refused by the content fence | curios tags |
| Runtime entity textures | atlas by sprite name with a fixed slot pool | geckolib |

#792, #897 and #880 are unassigned. This note extends them; it does not replace them. Owners must be consulted before anything here is built.

## Order

1. Resolver: bundled source, Mods page row, port-pack `Provided` status and a `[dependencies]` entry in the ported manifest.
2. Curios gaps (dynamic-slot menu, extra slots, player layer hook, tag rule), then the `curios` mod.
3. Shared pure crates (`horizon-geo`, expression compiler).
4. Animation gaps (writable pose, first-person pass-through, #897), then `player_animation`.
5. `geckolib` for items and block entities, then armor, then entities once #792 lands.

## Risks

- Vanilla clients and servers cannot see attachment sync; libraries degrade to local-only. Say so in each library's page.
- Curios bugs seen in the Java library to design out: duplication on death with keep-inventory, desync on rejoin or dimension change, slots added at runtime not synced, transient versus permanent slot modifiers, equip events when the item is unchanged. A slot type that disappears with its mod must move its stacks to an overflow list returned to the player, never delete them.
- Animation bugs to design out: unknown easing or loop names silently becoming linear or play-once (warn at load), re-setting an equal animation being a no-op, head visibility not reset per frame, untrusted Molang in synced emotes stalling the network thread (budget steps and nodes; `cem/model.rs` already has size budgets).
- Armor drawn twice, or at the wrong scale, when a geckolib armor binding and the vanilla layer both render.
- Per-frame CPU geometry with string-keyed pose maps: measure bones times entities before enabling by default.
- Features off by default where they change the vanilla look.

## Not verified

Java library behaviour was read from summarised public pages, not source. GeckoLib 5 manager naming and Player Animation Library bend internals are unconfirmed. Whether any existing non-registry mod source can already feed the resolver was not traced.
