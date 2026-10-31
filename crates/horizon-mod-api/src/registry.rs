//! Typed, namespaced registries. Registration ends before gameplay starts.
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;
use std::{
    any::{Any, TypeId},
    collections::BTreeMap,
    marker::PhantomData,
};

use crate::Result;

/// A registry's public key. The value type is checked even across mod boundaries.
pub struct RegistryKey<T> {
    pub id: &'static str,
    marker: PhantomData<fn() -> T>,
}
impl<T> Copy for RegistryKey<T> {}
impl<T> Clone for RegistryKey<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> RegistryKey<T> {
    pub const fn new(id: &'static str) -> Self {
        Self {
            id,
            marker: PhantomData,
        }
    }
}

/// A saved reference uses names, never a session's numeric IDs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntryRef {
    pub registry: String,
    pub id: String,
}

/// The canonical wire/save representation, including an explicit schema version.
#[derive(Clone, Debug, PartialEq, Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RegistryData {
    pub schema: u32,
    pub entries: BTreeMap<String, Value>,
}

struct Registry {
    value_type: TypeId,
    decode: fn(Value) -> Result<Box<dyn Any + Send + Sync>>,
    data: RegistryData,
    values: BTreeMap<String, Box<dyn Any + Send + Sync>>,
}

#[derive(Default)]
pub struct RegistryBuilder {
    registries: BTreeMap<String, Registry>,
}
impl RegistryBuilder {
    pub fn create<T: Serialize + DeserializeOwned + Send + Sync + 'static>(
        &mut self,
        owner: &str,
        key: RegistryKey<T>,
        schema: u32,
    ) -> Result<()> {
        owned(owner, key.id)?;
        if schema == 0 {
            return Err(format!("{}: schema must be nonzero", key.id));
        }
        if self.registries.contains_key(key.id) {
            return Err(format!("duplicate registry {}", key.id));
        }
        self.registries.insert(
            key.id.into(),
            Registry {
                value_type: TypeId::of::<T>(),
                decode: |value| {
                    serde_json::from_value::<T>(value)
                        .map(|v| Box::new(v) as Box<dyn Any + Send + Sync>)
                        .map_err(|e| e.to_string())
                },
                data: RegistryData {
                    schema,
                    entries: BTreeMap::new(),
                },
                values: BTreeMap::new(),
            },
        );
        Ok(())
    }

    pub fn insert<T: Serialize + Send + Sync + 'static>(
        &mut self,
        owner: &str,
        key: RegistryKey<T>,
        id: &str,
        value: T,
    ) -> Result<EntryRef> {
        owned(owner, id)?;
        let registry = self
            .registries
            .get_mut(key.id)
            .ok_or_else(|| format!("unknown registry {}", key.id))?;
        if registry.value_type != TypeId::of::<T>() {
            return Err(format!("{}: wrong registry value type", key.id));
        }
        if registry.values.contains_key(id) {
            return Err(format!("{}: duplicate entry {id}", key.id));
        }
        let encoded = serde_json::to_value(&value).map_err(|e| e.to_string())?;
        registry.data.entries.insert(id.into(), encoded);
        registry.values.insert(id.into(), Box::new(value));
        Ok(EntryRef {
            registry: key.id.into(),
            id: id.into(),
        })
    }

    pub fn insert_json(
        &mut self,
        owner: &str,
        registry: &str,
        id: &str,
        value: Value,
    ) -> Result<EntryRef> {
        owned(owner, id)?;
        let r = self
            .registries
            .get_mut(registry)
            .ok_or_else(|| format!("unknown registry {registry}"))?;
        if r.values.contains_key(id) {
            return Err(format!("{registry}: duplicate entry {id}"));
        }
        let decoded = (r.decode)(value.clone()).map_err(|e| format!("{registry}/{id}: {e}"))?;
        r.data.entries.insert(id.into(), value);
        r.values.insert(id.into(), decoded);
        Ok(EntryRef {
            registry: registry.into(),
            id: id.into(),
        })
    }

    /// Read `data/<entry namespace>/<registry namespace>/<registry path>/<entry path>.json`.
    pub fn load_data(&mut self, owner: &str, content: &horizon_mods::Content) -> Result<()> {
        if content.namespaces.first().map(String::as_str) != Some(owner) {
            return Err("registry data owner does not match its content manifest".into());
        }
        for file in &content.files {
            let Some((entry_namespace, rest)) = file
                .strip_prefix("data/")
                .and_then(|rest| rest.split_once('/'))
            else {
                continue;
            };
            if !content.namespaces.iter().any(|ns| ns == entry_namespace) {
                continue;
            }
            let Some((namespace, path)) = rest.split_once('/') else {
                continue;
            };
            let candidates: Vec<_> = self
                .registries
                .keys()
                .filter_map(|registry| {
                    let (ns, name) = registry.split_once(':')?;
                    let entry = (ns == namespace)
                        .then(|| path.strip_prefix(&format!("{name}/")))
                        .flatten()?
                        .strip_suffix(".json")?;
                    Some((registry.clone(), format!("{entry_namespace}:{entry}")))
                })
                .collect();
            if candidates.len() > 1 {
                return Err(format!("{file}: ambiguous registry data path"));
            }
            for (registry, entry) in candidates {
                let value = serde_json::from_slice(&content.file(file)?)
                    .map_err(|e| format!("{file}: {e}"))?;
                self.insert_json(entry_namespace, &registry, &entry, value)?;
            }
        }
        Ok(())
    }

    pub fn freeze(self, references: &[EntryRef]) -> Result<Registries> {
        for reference in references {
            if !self
                .registries
                .get(&reference.registry)
                .is_some_and(|r| r.values.contains_key(&reference.id))
            {
                return Err(format!(
                    "unresolved registry reference {} / {}",
                    reference.registry, reference.id
                ));
            }
        }
        Ok(Registries {
            registries: self.registries,
        })
    }
}

/// Immutable after registration; reads are safe on server and render threads.
pub struct Registries {
    registries: BTreeMap<String, Registry>,
}
impl Registries {
    pub fn get<T: 'static>(&self, key: RegistryKey<T>, id: &str) -> Result<&T> {
        self.registries
            .get(key.id)
            .and_then(|r| r.values.get(id))
            .and_then(|v| v.downcast_ref())
            .ok_or_else(|| format!("{} / {id}: unknown entry or wrong type", key.id))
    }
    /// Every entry of a registry in ID order; empty for an unknown registry or wrong type.
    pub fn entries<T: 'static>(&self, key: RegistryKey<T>) -> impl Iterator<Item = (&str, &T)> {
        self.registries
            .get(key.id)
            .into_iter()
            .flat_map(|r| r.values.iter())
            .filter_map(|(id, v)| Some((id.as_str(), v.downcast_ref()?)))
    }
    pub fn data(&self) -> BTreeMap<String, RegistryData> {
        self.registries
            .iter()
            .map(|(id, r)| (id.clone(), r.data.clone()))
            .collect()
    }
}

pub fn identifier(id: &str) -> Result<()> {
    let Some((ns, path)) = id.split_once(':') else {
        return Err(format!("{id:?}: expected namespace:path"));
    };
    let valid = |s: &str| {
        !s.is_empty()
            && s.bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b"_.-".contains(&b))
    };
    if valid(ns) && path.split('/').all(valid) && !path.split('/').any(|p| p == "." || p == "..") {
        Ok(())
    } else {
        Err(format!("invalid identifier {id:?}"))
    }
}
fn owned(owner: &str, id: &str) -> Result<()> {
    identifier(id)?;
    if id.split_once(':').is_some_and(|(ns, _)| ns == owner)
        && !matches!(owner, "minecraft" | "horizon" | "c")
    {
        Ok(())
    } else {
        Err(format!("{owner} cannot register {id}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const MATERIAL: RegistryKey<u32> = RegistryKey::new("ruby:material");
    #[test]
    fn rust_values_keep_fields_outside_the_data_codec() {
        #[derive(Serialize, serde::Deserialize)]
        struct Entry {
            saved: u32,
            #[serde(skip)]
            runtime: u32,
        }
        let key = RegistryKey::<Entry>::new("ruby:runtime");
        let mut builder = RegistryBuilder::default();
        builder.create("ruby", key, 1).unwrap();
        builder
            .insert(
                "ruby",
                key,
                "ruby:entry",
                Entry {
                    saved: 1,
                    runtime: 42,
                },
            )
            .unwrap();
        assert_eq!(
            builder
                .freeze(&[])
                .unwrap()
                .get(key, "ruby:entry")
                .unwrap()
                .runtime,
            42
        );
    }
    #[test]
    fn typed_owned_frozen_and_named() {
        let mut builder = RegistryBuilder::default();
        builder.create("ruby", MATERIAL, 1).unwrap();
        assert!(builder.create("ruby", MATERIAL, 1).is_err());
        assert!(builder.insert("other", MATERIAL, "ruby:a", 2).is_err());
        assert!(
            builder
                .insert("ruby", RegistryKey::new("ruby:material"), "ruby:a", "bad")
                .is_err()
        );
        let reference = builder.insert("ruby", MATERIAL, "ruby:a", 42).unwrap();
        assert!(builder.insert("ruby", MATERIAL, "ruby:a", 0).is_err());
        let registries = builder.freeze(&[reference]).unwrap();
        assert_eq!(*registries.get(MATERIAL, "ruby:a").unwrap(), 42);
        assert_eq!(
            registries.entries(MATERIAL).collect::<Vec<_>>(),
            [("ruby:a", &42)]
        );
        assert_eq!(registries.data()[MATERIAL.id].schema, 1);
    }
    #[test]
    fn identifiers_and_missing_references_fail() {
        for id in [
            "ruby:",
            ":x",
            "ruby:a/../b",
            "ruby:X",
            "minecraft:x:y",
            "ruby:a//b",
        ] {
            assert!(identifier(id).is_err());
        }
        assert!(
            RegistryBuilder::default()
                .freeze(&[EntryRef {
                    registry: "ruby:missing".into(),
                    id: "ruby:x".into()
                }])
                .is_err()
        );
    }
}
