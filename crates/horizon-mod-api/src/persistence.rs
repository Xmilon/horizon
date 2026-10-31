//! Check a world's registry contract before migrations, chunk loading or world writes.
use crate::{Result, registry::RegistryData};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

pub const FILE: &str = "horizon-rust-mods.json";

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModVersion {
    pub version: String,
    pub fingerprint: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub api: String,
    pub mods: BTreeMap<String, ModVersion>,
    pub registries: BTreeMap<String, RegistryData>,
}
impl Manifest {
    pub fn check_world(&self, world: &Path) -> Result {
        let path = world.join(FILE);
        let saved = match std::fs::read(&path) {
            Ok(bytes) => serde_json::from_slice::<Self>(&bytes)
                .map_err(|e| format!("{}: {e}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(e) => return Err(format!("{}: {e}", path.display())),
        };
        if self.api != saved.api {
            return Err("World needs a different Rust mod API; restore the matching build".into());
        }
        for (id, version) in &saved.mods {
            if self.mods.get(id) != Some(version) {
                return Err(format!(
                    "World needs the saved Rust mod {id} {} (same source build); restore it before opening the world",
                    version.version
                ));
            }
        }
        for (id, registry) in &saved.registries {
            let Some(current) = self.registries.get(id) else {
                return Err(format!("World needs missing registry {id}"));
            };
            if current.schema != registry.schema
                || registry
                    .entries
                    .iter()
                    .any(|(id, value)| current.entries.get(id) != Some(value))
            {
                return Err(format!(
                    "World registry {id} changed; an explicit migration is required"
                ));
            }
        }
        Ok(())
    }

    /// Called with the world's session lock held, after compatibility was checked.
    pub fn write_world(&self, world: &Path) -> Result {
        if self.mods.is_empty() && self.registries.is_empty() && !world.join(FILE).exists() {
            return Ok(());
        }
        let path = world.join(FILE);
        let partial = path.with_extension("json.partial");
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(&partial, bytes).map_err(|e| e.to_string())?;
        std::fs::rename(partial, path).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn missing_or_changed_mod_fails_without_writes() {
        let dir =
            std::env::temp_dir().join(format!("horizon-registry-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let empty = Manifest {
            api: crate::API.into(),
            mods: BTreeMap::new(),
            registries: BTreeMap::new(),
        };
        let mut saved = empty.clone();
        saved.mods.insert(
            "ruby".into(),
            ModVersion {
                version: "1".into(),
                fingerprint: "a".into(),
            },
        );
        saved.write_world(&dir).unwrap();
        let before = std::fs::read(dir.join(FILE)).unwrap();
        assert!(empty.check_world(&dir).is_err());
        assert!(saved.check_world(&dir).is_ok());
        saved.mods.get_mut("ruby").unwrap().fingerprint = "b".into();
        assert!(saved.check_world(&dir).is_err());
        assert_eq!(std::fs::read(dir.join(FILE)).unwrap(), before);
        std::fs::remove_file(dir.join(FILE)).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
