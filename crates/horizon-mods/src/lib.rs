//! Horizon's mod files (`docs/mods.md`): content mods (`.hmod`: blocks, items, data and
//! assets, no code), Inox mods (`.imod`: blocks and items declared in `inox.toml`,
//! docs/inox-mods.md), modpacks and, with `index`, the mod registry's index. Code mods are
//! native (docs/native-mods.md).
pub mod api_level;
pub mod content;
pub mod imod;
#[cfg(feature = "index")]
pub mod index;
pub mod manifest;
pub mod modpack;
pub mod rust_mod;

pub use content::Content;
pub use manifest::{Capability, Manifest, Package, Side, find};
