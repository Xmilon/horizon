//! Mods in the game (`docs/mods.md`): content mods' blocks, items, data and
//! assets (`content_root`), and the Mods page's rows. Code mods are native
//! (`native_hooks.rs`); WebAssembly code mods don't run any more.
use super::Game;
use horizon_client::{
    mod_list::{self, Installed},
    text::Text,
};
use std::path::PathBuf;

#[derive(Default)]
pub(super) struct ModsState {
    /// `--dev-mod PATH`: always on.
    pub(super) dev: Vec<PathBuf>,
    /// The WebAssembly mods found that no longer run, for the one-time notice.
    stopped: Vec<String>,
}

/// `root` with every installed content mod layered over it
/// (docs/modpacks.md): their blocks, items, data and assets. A mod in the
/// folder is loaded; there is no switch. Read once at start, before the
/// resources load; a mod whose content is refused is left out here and its
/// row on the Mods page says why.
pub(super) fn content_root(
    root: &std::path::Path,
    dev: &[std::path::PathBuf],
) -> horizon_client::Result<std::path::PathBuf> {
    use horizon_client::mod_content::{self, ContentMod};
    use horizon_mods::{Capability, Content, Package};
    let here = std::env::current_dir().unwrap_or_default();
    let mut paths = horizon_mods::find(&here.join(mod_list::DIR));
    paths.retain(|p| !dev.contains(p));
    paths.extend(dev.iter().cloned());
    paths.extend(horizon_mods::rust_mod::selected_sources(&here));
    let mut mods: Vec<ContentMod> = Vec::new();
    let mut versions = std::collections::BTreeMap::new();
    let mut source_mods = Vec::new();
    for path in &paths {
        let Ok(package) = Package::read(path) else {
            continue;
        };
        let m = &package.manifest;
        if horizon_mods::rust_mod::descriptor(path)?.is_some() {
            source_mods.push((path, m.clone()));
            continue;
        }
        if !m.capabilities.contains(&Capability::Registries) || mods.iter().any(|c| c.id == m.id) {
            continue;
        }
        let content = Content::read(path, m).and_then(|content| {
            let own = (m.id.as_str(), content.namespaces.as_slice());
            match mods
                .iter()
                .find_map(|c| horizon_mods::content::clash((&c.id, &c.content.namespaces), own))
            {
                Some(error) => Err(error),
                None => Ok(content),
            }
        });
        match content {
            Ok(content) => {
                versions.insert(m.id.clone(), m.version.clone());
                mods.push(ContentMod {
                    id: m.id.clone(),
                    content,
                });
            }
            Err(error) => log::warn!("Horizon: mod {} content not loaded: {error}", m.id),
        }
    }
    let data: Vec<_> = mods
        .iter()
        .map(|m| (m.id.clone(), m.content.clone()))
        .collect();
    let runtime = super::rust_mods::initialize(&data)?;
    for (path, m) in source_mods {
        let compiled = runtime.mods.iter().find(|c| c.declarations.meta.id == m.id);
        if !compiled.is_some_and(|c| {
            c.declarations.meta.version == m.version
                && horizon_mods::rust_mod::read(path).is_ok_and(|files| {
                    c.fingerprint.split('/').next()
                        == Some(horizon_mods::rust_mod::fingerprint(&files).as_str())
                })
        }) {
            return Err(format!(
                "Rust mod {} needs a native rebuild; compile the selected mods with the launcher",
                m.id
            )
            .into());
        }
    }
    for native in &runtime.mods {
        for content in &mods {
            if let Some(error) = horizon_mods::content::clash(
                (&content.id, &content.content.namespaces),
                (&native.declarations.meta.id, &native.content.namespaces),
            ) {
                return Err(error.into());
            }
        }
        mods.push(ContentMod {
            id: native.declarations.meta.id.clone(),
            content: native.content.clone(),
        });
    }
    let resources =
        mod_content::resource_root(root, &mods, &here.join(".horizon-cache")).and_then(|out| {
            if out == root && !runtime.mods.is_empty() {
                let out = here.join(".horizon-cache/rust-content");
                mod_content::build(root, &mods, &out)?;
                Ok(out)
            } else {
                Ok(out)
            }
        });
    match resources {
        Ok(out) => {
            if out != root {
                log::info!(
                    "Horizon: content from {} mod(s) in {}",
                    mods.len(),
                    out.display()
                );
            }
            register_mob_effects(&mods);
            let mut manifest = runtime.manifest();
            for module in &mods {
                if manifest.mods.contains_key(&module.id) || module.content.is_empty() {
                    continue;
                }
                let files = module
                    .content
                    .files
                    .iter()
                    .map(|name| module.content.file(name).map(|bytes| (name.clone(), bytes)))
                    .collect::<Result<std::collections::BTreeMap<_, _>, _>>()?;
                manifest.mods.insert(
                    module.id.clone(),
                    horizon_mod_api::persistence::ModVersion {
                        version: versions.get(&module.id).cloned().unwrap_or_default(),
                        fingerprint: horizon_mods::rust_mod::fingerprint(&files),
                    },
                );
            }
            if !manifest.mods.is_empty() {
                if out == root {
                    return Err("Rust mod content overlay was not created".into());
                }
                std::fs::write(
                    out.join(horizon_mod_api::persistence::FILE),
                    serde_json::to_vec(&manifest)?,
                )?;
                std::fs::write(out.join(horizon_client::content_identity::MARKER), b"1")?;
            }
            Ok(out)
        }
        Err(e) => {
            if !runtime.mods.is_empty() {
                return Err(e);
            }
            log::warn!("Horizon: content mods not loaded: {e}");
            register_mob_effects(&[]);
            Ok(root.to_owned())
        }
    }
}

/// The mods' mob effects join vanilla's (`brewing::effect`), once the root
/// holding their network ids is built. The startup root's effects, none
/// included, stay until restart: a pack reload after a Mods switch builds a
/// root with others.
fn register_mob_effects(mods: &[horizon_client::mod_content::ContentMod]) {
    use horizon_client::brewing::{self, Category};
    use horizon_mods::content::EffectCategory;
    let effects = horizon_client::mod_content::mob_effects(mods);
    let ids: Vec<String> = effects.iter().map(|e| e.id.clone()).collect();
    let set = brewing::register_mod_effects(effects.into_iter().map(|e| {
        let category = match e.category {
            EffectCategory::Beneficial => Category::Beneficial,
            EffectCategory::Harmful => Category::Harmful,
            EffectCategory::Neutral => Category::Neutral,
        };
        (e.id.clone(), e.color, category)
    }));
    if !set
        && brewing::mod_effects()
            .iter()
            .map(|e| e.id)
            .ne(ids.iter().map(String::as_str))
    {
        log::warn!("Horizon: the mods' mob effects changed; restart the game to use them");
    }
}

/// What the launcher writes beside a game it built with native mods: their ids under `mods`.
const BUILD_INFO: &str = "native-mods.json";

/// A row for each native mod the launcher installed in `dir` (`<id>.hnmod`) that this game wasn't
/// built with (`built`: its [`BUILD_INFO`], empty for a release or Nightly) and that has no row
/// yet. The page said "No mods" before, and players copied the files into `horizon-mods` (#4770).
/// Its toggle is the `mod.<id>` line the launcher's next build follows.
fn not_built(dir: &std::path::Path, built: &str, rows: &[Installed]) -> Vec<Installed> {
    let info: serde_json::Value = serde_json::from_str(built).unwrap_or_default();
    let in_build = |id: &str| {
        info["mods"]
            .as_array()
            .is_some_and(|mods| mods.iter().any(|m| m["id"] == id))
    };
    let mut paths: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|d| d.flatten().map(|e| e.path()).collect())
        .unwrap_or_default();
    paths.retain(|p| p.extension().is_some_and(|e| e == "hnmod"));
    paths.sort();
    let mut out: Vec<Installed> = Vec::new();
    for path in paths {
        let Ok(bundle) = horizon_mods::index::native::Bundle::read(&path) else {
            continue;
        };
        let m = bundle.manifest;
        let listed = rows.iter().chain(&out).any(|row| row.id == m.id);
        if !listed && !in_build(&m.id) {
            out.push(Installed {
                title: format!("{} {}", m.id, m.version),
                version: m.version.clone(),
                kind: "Native".into(),
                file: path.display().to_string(),
                id: m.id,
                problem: Some(mod_list::NOT_BUILT.to_owned()),
                ..Default::default()
            });
        }
    }
    out
}

/// A WebAssembly code mod's row on the Mods page.
const STOPPED: &str = "Doesn't run any more: needs a native port";
/// The player was told once about the WebAssembly mods that stopped (`horizon-mods/<this>`).
const NOTICE_SEEN: &str = ".native-notice";

impl Game<'_> {
    /// Lists the installed mods on the Mods page. Content mods load with the resources
    /// (`content_root`); native ones are built into the game (`native_hooks.rs`). WebAssembly code
    /// mods don't run any more: their rows say so, and the player hears it once in the chat
    /// (`tell_native_now`).
    pub(super) fn start_mods(&mut self) {
        let dir = Self::options_path().with_file_name(mod_list::DIR);
        let mut paths = horizon_mods::find(&dir);
        paths.retain(|p| !self.mods.dev.contains(p));
        paths.extend(self.mods.dev.iter().cloned());
        let mut owners: Vec<(String, Vec<String>)> = Vec::new();
        self.settings.mods.installed = paths
            .iter()
            .map(|path| {
                let file = path.file_name().unwrap_or_default().to_string_lossy();
                let package = match horizon_mods::Package::read(path) {
                    Ok(package) => package,
                    Err(e) => {
                        return Installed {
                            id: String::new(),
                            title: file.into_owned(),
                            description: String::new(),
                            file: path.display().to_string(),
                            problem: Some(format!("Can't be read: {e}")),
                            dev: false,
                            native: None,
                            ..Default::default()
                        };
                    }
                };
                let m = &package.manifest;
                let mut problem = package.wasm.is_some().then(|| STOPPED.to_owned());
                let mut additions = Vec::new();
                if m.capabilities
                    .contains(&horizon_mods::Capability::Registries)
                {
                    match horizon_mods::Content::read(path, m) {
                        Ok(c) => {
                            if owners.iter().any(|(id, _)| id == &m.id) {
                                problem = Some(format!("Duplicate mod id: {}", m.id));
                            } else if let Some(e) = owners.iter().find_map(|(id, namespaces)| {
                                horizon_mods::content::clash(
                                    (id, namespaces),
                                    (&m.id, &c.namespaces),
                                )
                            }) {
                                problem = Some(e);
                            } else {
                                owners.push((m.id.clone(), c.namespaces.clone()));
                            }

                            additions.push(format!(
                                "Blocks ({}): {}",
                                c.blocks.len(),
                                c.blocks
                                    .iter()
                                    .map(|b| b.id.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                            additions.push(format!(
                                "Items ({}): {}",
                                c.items.len(),
                                c.items
                                    .iter()
                                    .map(|i| i.id.as_str())
                                    .collect::<Vec<_>>()
                                    .join(", ")
                            ));
                            for (label, marker) in
                                [("Tags", "/tags/"), ("Tabs", "/horizon/creative_tab/")]
                            {
                                let files: Vec<_> = c
                                    .files
                                    .iter()
                                    .filter(|f| f.starts_with("data/") && f.contains(marker))
                                    .cloned()
                                    .collect();
                                additions.push(format!(
                                    "{label} ({}): {}",
                                    files.len(),
                                    files.join(", ")
                                ));
                            }
                        }
                        Err(e) => problem = Some(e),
                    }
                }
                Installed {
                    id: m.id.clone(),
                    title: format!("{} {}", m.name, m.version),
                    description: m.description.clone(),
                    version: m.version.clone(),
                    authors: m.authors.clone(),
                    file: path.display().to_string(),
                    kind: if path.extension().is_some_and(|e| e == "imod")
                        || path.join("inox.toml").is_file()
                    {
                        "Inox"
                    } else {
                        "Content"
                    }
                    .into(),
                    additions,
                    problem,
                    dev: self.mods.dev.contains(path),
                    native: None,
                }
            })
            .collect();
        for (id, title, version, authors, description) in [
            (
                "minecraft",
                "Minecraft",
                horizon_core::version::VERSION_ID,
                "Mojang Studios",
                "Minecraft: Java Edition",
            ),
            (
                "horizon",
                "Horizon",
                env!("CARGO_PKG_VERSION"),
                "Horizon contributors",
                "Minecraft rewritten in Rust",
            ),
        ] {
            self.settings.mods.installed.push(Installed {
                id: id.into(),
                title: title.into(),
                version: version.into(),
                authors: vec![authors.into()],
                description: description.into(),
                kind: "Built-in".into(),
                file: std::env::current_exe()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default(),
                ..Default::default()
            });
        }
        // A mod with a native version built in says so instead, and isn't news.
        self.list_native_mods();
        if let Ok(runtime) = super::rust_mods::catalog() {
            for module in &runtime.mods {
                if let Some(row) = self
                    .settings
                    .mods
                    .installed
                    .iter_mut()
                    .find(|m| m.id == module.declarations.meta.id)
                {
                    row.kind = "Rust (compiled)".into();
                    row.additions = vec![
                        format!(
                            "Blocks: {}",
                            module
                                .content
                                .blocks
                                .iter()
                                .map(|b| b.id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        format!(
                            "Items: {}",
                            module
                                .content
                                .items
                                .iter()
                                .map(|b| b.id.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        format!(
                            "Registries: {}",
                            runtime
                                .registries
                                .data()
                                .keys()
                                .filter(|id| id.starts_with(&format!("{}:", row.id)))
                                .cloned()
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        format!(
                            "Machines: {}",
                            module
                                .machines
                                .iter()
                                .map(|m| m.block.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    ];
                    for (label, marker) in [("Tags", "/tags/"), ("Tabs", "/horizon/creative_tab/")]
                    {
                        let files: Vec<_> = module
                            .content
                            .files
                            .iter()
                            .filter(|file| file.starts_with("data/") && file.contains(marker))
                            .cloned()
                            .collect();
                        row.additions.push(format!(
                            "{label} ({}): {}",
                            files.len(),
                            files.join(", ")
                        ));
                    }
                }
            }
        }
        let built = std::env::current_exe()
            .ok()
            .and_then(|exe| std::fs::read_to_string(exe.with_file_name(BUILD_INFO)).ok())
            .unwrap_or_default();
        let native_dir = Self::options_path().with_file_name(mod_list::NATIVE_DIR);
        let unbuilt = not_built(&native_dir, &built, &self.settings.mods.installed);
        self.settings.mods.installed.extend(unbuilt);
        let installed = &self.settings.mods.installed;
        self.mods.stopped = installed
            .iter()
            .filter(|row| row.problem.as_deref() == Some(STOPPED))
            .map(|row| row.title.clone())
            .collect();
    }
    /// Once, in the first world after the update: which WebAssembly mods stopped.
    pub(super) fn tell_native_now(&mut self) {
        if self.mods.stopped.is_empty() || self.player.is_none() {
            return;
        }
        let names = std::mem::take(&mut self.mods.stopped).join(", ");
        let seen = Self::options_path()
            .with_file_name(mod_list::DIR)
            .join(NOTICE_SEEN);
        if seen.exists() {
            return;
        }
        self.system_message(Text::literal(format!(
            "Horizon mods are native now: {names} stopped running until they have a native version. The launcher's Mods page has the native ones."
        )));
        let _ = std::fs::write(&seen, "");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A minimal `.hnmod` the registry accepts: its manifest and its mixin's.
    fn hnmod(dir: &std::path::Path, id: &str) {
        let horizon = "a".repeat(40);
        let manifest = serde_json::json!({
            "schema": 1, "id": id, "version": "0.3.0", "game": "26.3",
            "horizon": horizon, "mixins": "b".repeat(40),
        });
        let mixin = format!(
            "[mod]\nid = \"{id}\"\nversion = \"0.3.0\"\n[compatibility]\nhorizon_revision = \"{horizon}\"\n"
        );
        let file = std::fs::File::create(dir.join(format!("{id}.hnmod"))).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();
        zip.start_file("horizon-native.json", options).unwrap();
        zip.write_all(manifest.to_string().as_bytes()).unwrap();
        zip.start_file(format!("mixins/{id}/horizon-mixin.toml"), options)
            .unwrap();
        zip.write_all(mixin.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    #[test]
    fn native_mods_installed_but_not_built_get_a_row_saying_so() {
        let dir = std::env::temp_dir().join(format!("horizon-not-built-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for id in ["veinminer", "zoom", "minimap"] {
            hnmod(&dir, id);
        }
        std::fs::write(dir.join("notes.txt"), "not a mod").unwrap();
        std::fs::write(dir.join("broken.hnmod"), "not a zip").unwrap();
        // A release: none is built in; the page lists them, sorted, instead of "No mods".
        let rows = not_built(&dir, "", &[]);
        let ids: Vec<&str> = rows.iter().map(|r| r.id.as_str()).collect();
        assert_eq!(ids, ["minimap", "veinminer", "zoom"]);
        assert_eq!(rows[1].title, "veinminer 0.3.0");
        assert_eq!(rows[1].problem.as_deref(), Some(mod_list::NOT_BUILT));
        assert!(rows.iter().all(|r| r.native.is_none() && !r.dev));
        // A native build with zoom, and minimap's row already listed by its own hook.
        let built =
            r#"{"base":"nightly-2026-10-30-1da354b","mods":[{"id":"zoom","version":"0.3.0"}]}"#;
        let listed = Installed {
            id: "minimap".into(),
            ..rows[0].clone()
        };
        let rows = not_built(&dir, built, &[listed]);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "veinminer");
        // No folder: no rows.
        assert!(not_built(&dir.join("missing"), "", &[]).is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
