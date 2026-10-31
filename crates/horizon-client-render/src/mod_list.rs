//! The installed Horizon mods (`docs/mods.md`): every mod in the
//! `horizon-mods` folder beside `options.txt` is loaded, and a native mod
//! built into the game runs; there is no switch in the game. A mod stops by
//! taking its file out. `horizon-mods.txt` keeps the keys players chose for
//! mods' key bindings (`key.<id>.<name>:key.keyboard.x`).
use std::{collections::BTreeMap, fs, path::Path};

pub const FILE: &str = "horizon-mods.txt";
/// The folder holding `.hmod` and `.imod` files, beside `options.txt`.
pub const DIR: &str = "horizon-mods";
/// Where the launcher installs native mods (`<id>.hnmod`), beside [`DIR`]. They run only in a
/// game the launcher built with them.
pub const NATIVE_DIR: &str = "horizon-native-mods";
/// Why a native mod installed in [`NATIVE_DIR`] doesn't run in this game.
pub const NOT_BUILT: &str = "Installed, not built into this game: build the game with it in the launcher (Settings > Native mods), then play the instance that build is in.";

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModList {
    /// The keys the player chose for mods' keys, mod id → key name → key
    /// (`key.<id>.<name>:key.keyboard.x` lines).
    pub keys: BTreeMap<String, BTreeMap<String, String>>,
    /// What the Mods page shows, filled by the game from the mods it found.
    pub installed: Vec<Installed>,
}

/// One installed mod as the Mods page shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Installed {
    pub id: String,
    pub version: String,
    pub authors: Vec<String>,
    pub file: String,
    pub kind: String,
    pub additions: Vec<String>,
    /// `Name 1.0` (the file name when the manifest is unreadable).
    pub title: String,
    pub description: String,
    /// Why it is not running, when it failed.
    pub problem: Option<String>,
    /// Loaded from elsewhere with `--dev-mod`.
    pub dev: bool,
    /// A native mod (`game/native_hooks.rs`): built into the game, and its
    /// settings page.
    pub native: Option<NativePage>,
}

/// A native mod's settings page: its keys as Key Binds rows, then its
/// buttons. The keys the player chooses are kept in `ModList::keys`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativePage {
    /// Each key's name, label and default key (`key.keyboard.z`).
    pub keys: Vec<[String; 3]>,
    pub buttons: Vec<String>,
}

impl ModList {
    pub fn load(dir: &Path) -> Self {
        fs::read_to_string(dir.join(FILE))
            .map(|t| Self::parse(&t))
            .unwrap_or_default()
    }
    pub fn parse(text: &str) -> Self {
        let mut keys: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (k, v) in text.lines().filter_map(|l| l.split_once(':')) {
            // Mod ids have no dots; a key's name may.
            if let Some((id, name)) = k
                .trim()
                .strip_prefix("key.")
                .and_then(|r| r.split_once('.'))
            {
                let entry = keys.entry(id.to_owned()).or_default();
                entry.insert(name.to_owned(), v.trim().to_owned());
            }
        }
        Self {
            keys,
            installed: Vec::new(),
        }
    }
    pub fn serialize(&self) -> String {
        self.keys
            .iter()
            .flat_map(|(id, keys)| {
                keys.iter()
                    .map(move |(name, key)| format!("key.{id}.{name}:{key}\n"))
            })
            .collect()
    }
    /// Mod `id`'s chosen keys, for `Mods::set_bindings`.
    pub fn keys_of(&self, id: &str) -> Vec<(&str, &str)> {
        self.keys.get(id).map_or_else(Vec::new, |k| {
            k.iter().map(|(n, k)| (n.as_str(), k.as_str())).collect()
        })
    }
    pub fn save(&self, dir: &Path) {
        let path = dir.join(FILE);
        if self.keys.is_empty() && !path.exists() {
            return;
        }
        // The launcher owns native build selection; saving game keys must preserve it.
        let selection: String = fs::read_to_string(&path)
            .unwrap_or_default()
            .lines()
            .filter(|line| line.trim_start().starts_with("mod."))
            .map(|line| format!("{line}\n"))
            .collect();
        let _ = fs::write(path, selection + &self.serialize());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saving_keys_preserves_launcher_build_selection_without_exposing_switches() {
        let dir =
            std::env::temp_dir().join(format!("horizon-native-selection-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join(FILE),
            "mod.native:true\nmod.other:false\nkey.native.action:key.keyboard.x\n",
        )
        .unwrap();
        let mut list = ModList::load(&dir);
        list.keys
            .entry("native".into())
            .or_default()
            .insert("action".into(), "key.keyboard.z".into());
        list.save(&dir);
        assert_eq!(
            fs::read_to_string(dir.join(FILE)).unwrap(),
            "mod.native:true\nmod.other:false\nkey.native.action:key.keyboard.z\n"
        );
        assert!(!list.serialize().contains("mod."));
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn keeps_only_the_keys_players_chose() {
        // Old files' `mod.<id>:true|false` lines (the switch there was) go.
        let text = "mod.zoom:true\nmod.x:false\nkey.zoom.zoom.in:key.mouse.4\nkey.zoom.out:key.keyboard.x\n";
        let list = ModList::parse(text);
        assert_eq!(
            list.keys_of("zoom"),
            [("out", "key.keyboard.x"), ("zoom.in", "key.mouse.4")]
        );
        assert!(list.keys_of("other").is_empty());
        assert_eq!(
            list.serialize(),
            "key.zoom.out:key.keyboard.x\nkey.zoom.zoom.in:key.mouse.4\n"
        );
        assert_eq!(ModList::parse(&list.serialize()), list);
        assert_eq!(ModList::default().serialize(), "");
    }
}
