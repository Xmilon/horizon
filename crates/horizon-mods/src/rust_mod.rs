//! Source-only Rust projects. Inspecting or packaging a project never invokes Cargo.
use serde::Deserialize;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::{Path, PathBuf},
};

pub const API: &str = "0.1";
pub const INSTALLED: &str = "horizon-native-mods";
/// The generated executable package of a build with Rust mods (`compiled-mods/horizon-game`).
/// Only it depends on the mods, so changing a mod never recompiles the game library.
pub const GAME_PACKAGE: &str = "horizon-game";

/// Selected Rust sources must match the compiled client; selection never unloads compiled code.
pub fn selected_sources(home: &Path) -> Vec<PathBuf> {
    let text = std::fs::read_to_string(home.join("horizon-mods.txt")).unwrap_or_default();
    let selected: BTreeSet<_> = text
        .lines()
        .filter_map(|line| {
            let (key, value) = line.split_once(':')?;
            (value.trim() == "true")
                .then(|| key.trim().strip_prefix("mod."))
                .flatten()
        })
        .collect();
    crate::find(&home.join(INSTALLED))
        .into_iter()
        .filter(|path| {
            descriptor(path)
                .ok()
                .flatten()
                .is_some_and(|declarations| selected.contains(declarations.meta.id.as_str()))
        })
        .collect()
}

/// Toolchains such as MSVC and horizon-mixins reject Windows' verbatim drive prefix.
pub fn plain_path(path: PathBuf) -> PathBuf {
    match path.to_str().and_then(|path| path.strip_prefix(r"\\?\")) {
        Some(rest) if rest.as_bytes().get(1) == Some(&b':') => PathBuf::from(rest),
        _ => path,
    }
}

/// Keep stable source paths and unchanged modification times for Cargo's incremental builds.
pub fn sync_workspace(from: &Path, to: &Path) -> Result<(), String> {
    if std::fs::symlink_metadata(to).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err("build staging cannot be a symbolic link".into());
    }
    fn list(root: &Path, dir: &Path, files: &mut BTreeMap<PathBuf, PathBuf>) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let kind = entry.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err(format!(
                    "{}: symbolic link in build staging",
                    path.display()
                ));
            }
            if kind.is_dir() {
                list(root, &path, files)?;
            } else if kind.is_file() {
                files.insert(
                    path.strip_prefix(root)
                        .map_err(|e| e.to_string())?
                        .to_owned(),
                    path,
                );
            }
        }
        Ok(())
    }
    let from = from.canonicalize().map_err(|e| e.to_string())?;
    std::fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let to = to.canonicalize().map_err(|e| e.to_string())?;
    if to.starts_with(&from) || from.starts_with(&to) {
        return Err("build staging directories must not contain each other".into());
    }
    let mut new = BTreeMap::new();
    let mut old = BTreeMap::new();
    list(&from, &from, &mut new)?;
    list(&to, &to, &mut old)?;
    for (name, path) in &new {
        let target = to.join(name);
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        if std::fs::read(&target).ok().as_ref() != Some(&bytes) {
            std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::copy(path, target).map_err(|e| e.to_string())?;
        }
    }
    for (name, path) in old {
        if !new.contains_key(&name) {
            std::fs::remove_file(path).map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

pub fn descriptor(path: &Path) -> Result<Option<crate::imod::InoxFile>, String> {
    if !crate::imod::is_imod(path) {
        return Ok(None);
    }
    let (_, declarations) = crate::imod::parse(&crate::imod::read_toml(path)?)?;
    Ok(declarations.rust.is_some().then_some(declarations))
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RustMod {
    pub api: String,
    /// A public `fn() -> Box<dyn horizon_mod_api::Mod>` in the library crate.
    pub entry: String,
    /// Required mod IDs and exact versions, or `*` for any version.
    #[serde(default)]
    pub dependencies: BTreeMap<String, String>,
}
impl RustMod {
    pub fn check(&self) -> Result<(), String> {
        if self.api != API {
            return Err(format!(
                "Rust mod needs API {}; this build supports {API}",
                self.api
            ));
        }
        if self.entry.is_empty()
            || !self.entry.split("::").all(|p| {
                !p.is_empty()
                    && !p.as_bytes()[0].is_ascii_digit()
                    && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
            })
        {
            return Err("[rust] entry must be a Rust function path within the mod crate".into());
        }
        for (id, version) in &self.dependencies {
            if id.is_empty()
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
                || version.trim().is_empty()
            {
                return Err("[rust.dependencies] needs mod IDs and exact versions (or *)".into());
            }
        }
        Ok(())
    }
}

/// Bounded, symlink-free project contents, suitable for hashing, packaging and staging.
pub fn files(root: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    fn walk(
        root: &Path,
        dir: &Path,
        out: &mut BTreeMap<String, Vec<u8>>,
        total: &mut u64,
    ) -> Result<(), String> {
        for entry in std::fs::read_dir(dir).map_err(|e| e.to_string())? {
            let entry = entry.map_err(|e| e.to_string())?;
            let path = entry.path();
            let ty = entry.file_type().map_err(|e| e.to_string())?;
            let name = path
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            if matches!(entry.file_name().to_str(), Some("target" | ".git")) {
                continue;
            }
            if ty.is_symlink() {
                return Err(format!(
                    "{name}: source projects cannot contain symbolic links"
                ));
            }
            if ty.is_dir() {
                walk(root, &path, out, total)?;
            } else if ty.is_file() {
                check_path(&name)?;
                if entry.metadata().map_err(|e| e.to_string())?.len() > crate::content::MAX_FILE {
                    return Err(format!("{name}: source file exceeds size limit"));
                }
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                *total += bytes.len() as u64;
                if bytes.len() as u64 > crate::content::MAX_FILE
                    || *total > crate::content::MAX_TOTAL
                    || out.len() >= crate::content::MAX_FILES
                {
                    return Err("Rust project exceeds package limits".into());
                }
                out.insert(name, bytes);
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out, &mut 0)?;
    check_files(&out)?;
    Ok(out)
}

fn check_path(name: &str) -> Result<(), String> {
    if name.len() > 512
        || name.split('/').any(|p| {
            p.is_empty()
                || p == "."
                || p == ".."
                || p.ends_with(['.', ' '])
                || !p
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"_.-".contains(&b))
        })
    {
        return Err(format!("invalid Rust project path {name:?}"));
    }
    for part in name.split('/') {
        let stem = part
            .split('.')
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if matches!(stem.as_str(), "con" | "nul" | "prn" | "aux")
            || ["com", "lpt"].iter().any(|prefix| {
                stem.strip_prefix(prefix)
                    .is_some_and(|n| n.len() == 1 && matches!(n.as_bytes()[0], b'1'..=b'9'))
            })
        {
            return Err(format!("{name}: reserved device path"));
        }
    }
    Ok(())
}

pub fn check_files(files: &BTreeMap<String, Vec<u8>>) -> Result<crate::imod::InoxFile, String> {
    let mut folded = BTreeSet::new();
    let mut total = 0u64;
    if files.len() > crate::content::MAX_FILES {
        return Err("too many Rust source files".into());
    }
    for (name, bytes) in files {
        check_path(name)?;
        total = total.saturating_add(bytes.len() as u64);
        if !folded.insert(name.to_ascii_lowercase())
            || bytes.len() as u64 > crate::content::MAX_FILE
            || total > crate::content::MAX_TOTAL
        {
            return Err("duplicate source path or package exceeds size limits".into());
        }
    }
    let text = std::str::from_utf8(files.get("inox.toml").ok_or("Rust mod has no inox.toml")?)
        .map_err(|e| e.to_string())?;
    let (_, declarations) = crate::imod::parse(text)?;
    declarations
        .rust
        .as_ref()
        .ok_or("not a Rust mod: missing [rust]")?;
    let cargo = std::str::from_utf8(
        files
            .get("Cargo.toml")
            .ok_or("Rust mod has no Cargo.toml")?,
    )
    .map_err(|e| e.to_string())?;
    let cargo: toml::Table = cargo.parse().map_err(|e| format!("Cargo.toml: {e}"))?;
    if cargo.contains_key("workspace") {
        return Err("Rust mod must be an independent library package, without [workspace]".into());
    }
    let package = cargo
        .get("package")
        .and_then(|v| v.as_table())
        .ok_or("Cargo.toml: missing [package]")?;
    let name = package
        .get("name")
        .and_then(|v| v.as_str())
        .ok_or("Cargo.toml: missing package name")?;
    if name.is_empty()
        || !name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
    {
        return Err("Cargo.toml: invalid package name".into());
    }
    if package.get("version").and_then(|v| v.as_str()) != Some(&declarations.meta.version) {
        return Err("Cargo.toml version must equal [mod] version in inox.toml".into());
    }
    if !files.contains_key("src/lib.rs") {
        return Err("Rust mod needs src/lib.rs".into());
    }
    if cargo
        .get("lib")
        .and_then(|lib| lib.get("path"))
        .is_some_and(|path| path.as_str() != Some("src/lib.rs"))
    {
        return Err("Rust mod library must use src/lib.rs".into());
    }
    if let Some(build) = package.get("build").and_then(|v| v.as_str()) {
        check_path(build)?;
        if !files.contains_key(build) {
            return Err(format!("Cargo.toml: missing build script {build}"));
        }
    }
    if cargo.contains_key("patch") || cargo.contains_key("replace") {
        return Err("Rust mod cannot patch the combined workspace's Cargo dependencies".into());
    }
    fn dependencies(
        value: &toml::Value,
        mods: &BTreeMap<String, String>,
        source_paths: bool,
    ) -> Result<(), String> {
        let table = value
            .as_table()
            .ok_or("Cargo dependencies must be a table")?;
        for (name, spec) in table {
            if let Some(spec) = spec.as_table() {
                if spec.contains_key("git")
                    || spec.get("workspace").and_then(|v| v.as_bool()) == Some(true)
                {
                    return Err(
                        "Rust mod dependencies cannot use Git or workspace inheritance".into(),
                    );
                }
                let supplied =
                    source_paths && (name == "horizon-mod-api" || mods.contains_key(name));
                if spec.contains_key("path") && !supplied {
                    return Err("Rust mod path dependencies must be the SDK or declared mod IDs in regular [dependencies]".into());
                }
            } else if !spec.is_str() {
                return Err(format!("invalid Cargo dependency {name}"));
            }
        }
        Ok(())
    }
    let groups = ["dependencies", "dev-dependencies", "build-dependencies"];
    let required = &declarations.rust.as_ref().unwrap().dependencies;
    for group in groups {
        if let Some(value) = cargo.get(group) {
            dependencies(value, required, group == "dependencies")?;
        }
    }
    if let Some(targets) = cargo.get("target").and_then(|v| v.as_table()) {
        for target in targets.values() {
            for group in groups {
                if let Some(value) = target.get(group) {
                    dependencies(value, required, false)?;
                }
            }
        }
    }
    if package
        .values()
        .any(|value| value.get("workspace").and_then(|v| v.as_bool()) == Some(true))
        || cargo
            .get("lints")
            .and_then(|v| v.get("workspace"))
            .and_then(|v| v.as_bool())
            == Some(true)
    {
        return Err("Rust mod cannot inherit workspace package fields or lints".into());
    }
    crate::Content::from_declarations(
        &declarations,
        files
            .iter()
            .filter(|(name, _)| name.starts_with("assets/") || name.starts_with("data/"))
            .map(|(name, bytes)| (name.clone(), bytes.clone()))
            .collect(),
    )?;
    Ok(declarations)
}

pub fn write(files: &BTreeMap<String, Vec<u8>>) -> Result<Vec<u8>, String> {
    check_files(files)?;
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, bytes) in files {
        check_path(name)?;
        zip.start_file(name, zip::write::SimpleFileOptions::default())
            .map_err(|e| e.to_string())?;
        zip.write_all(bytes).map_err(|e| e.to_string())?;
    }
    Ok(zip.finish().map_err(|e| e.to_string())?.into_inner())
}

pub fn read(path: &Path) -> Result<BTreeMap<String, Vec<u8>>, String> {
    if path.is_dir() {
        return files(path);
    }
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    if zip.len() > crate::content::MAX_FILES {
        return Err("too many Rust project files".into());
    }
    let mut files = BTreeMap::new();
    let mut total = 0u64;
    let mut folded = BTreeSet::new();
    for i in 0..zip.len() {
        let entry = zip.by_index(i).map_err(|e| e.to_string())?;
        if entry.is_dir() {
            continue;
        }
        let name = entry.name().to_owned();
        check_path(&name)?;
        if entry.unix_mode().is_some_and(|m| m & 0o170000 == 0o120000) {
            return Err("Rust package contains a symbolic link".into());
        }
        if !folded.insert(name.to_ascii_lowercase()) {
            return Err(format!("duplicate Rust project path {name}"));
        }
        let mut bytes = Vec::new();
        entry
            .take(crate::content::MAX_FILE + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        total += bytes.len() as u64;
        if bytes.len() as u64 > crate::content::MAX_FILE || total > crate::content::MAX_TOTAL {
            return Err("Rust package exceeds size limits".into());
        }
        files.insert(name, bytes);
    }
    check_files(&files)?;
    Ok(files)
}

/// Stable dependency order; missing, incompatible and cyclic requirements fail before startup.
pub fn order(mods: &[(String, String, RustMod)]) -> Result<Vec<usize>, String> {
    let mut indices = BTreeMap::new();
    for (i, (id, _, rust)) in mods.iter().enumerate() {
        rust.check()?;
        if indices.insert(id.as_str(), i).is_some() {
            return Err(format!("duplicate Rust mod {id}"));
        }
    }
    let mut done = BTreeSet::new();
    let mut out = Vec::new();
    while out.len() < mods.len() {
        let mut progressed = false;
        for (&id, &i) in &indices {
            if done.contains(id) {
                continue;
            }
            for (dependency, version) in &mods[i].2.dependencies {
                let &other = indices
                    .get(dependency.as_str())
                    .ok_or_else(|| format!("{id} requires missing mod {dependency}"))?;
                if version != "*" && *version != mods[other].1 {
                    return Err(format!(
                        "{id} requires {dependency} {version}, found {}",
                        mods[other].1
                    ));
                }
            }
            if mods[i]
                .2
                .dependencies
                .keys()
                .all(|d| done.contains(d.as_str()))
            {
                done.insert(id);
                out.push(i);
                progressed = true;
            }
        }
        if !progressed {
            return Err("cyclic Rust mod dependencies".into());
        }
    }
    Ok(out)
}

/// Stage validated source projects and generate only the dependency/entrypoint glue.
/// The caller owns a disposable copy of Horizon, never a developer's checkout.
pub fn stage(workspace: &Path, projects: &[PathBuf]) -> Result<(), String> {
    let api = workspace.join("crates/horizon-mod-api");
    if !api.join("Cargo.toml").is_file() {
        return Err("this Horizon source has no Rust mod API; choose an Inox build".into());
    }
    let game = workspace.join("compiled-mods").join(GAME_PACKAGE);
    let path = |crate_name: &str| {
        toml::Value::Table(toml::Table::from_iter([(
            "path".into(),
            toml::Value::String(format!("../../crates/{crate_name}")),
        )]))
    };
    let mut cargo = toml::Table::from_iter([
        (
            "package".into(),
            toml::Value::Table(toml::Table::from_iter([
                ("name".into(), toml::Value::String(GAME_PACKAGE.into())),
                ("version".into(), toml::Value::String("0.1.0".into())),
                ("edition".into(), toml::Value::String("2024".into())),
                ("publish".into(), toml::Value::Boolean(false)),
            ])),
        ),
        (
            "bin".into(),
            toml::Value::Array(vec![toml::Value::Table(toml::Table::from_iter([
                ("name".into(), toml::Value::String("horizon-client".into())),
                ("path".into(), toml::Value::String("src/main.rs".into())),
            ]))]),
        ),
        (
            "dependencies".into(),
            toml::Value::Table(toml::Table::from_iter(
                [
                    "horizon-client",
                    "horizon-threads",
                    "horizon-mods",
                    "horizon-mod-api",
                ]
                .map(|name| (name.to_owned(), path(name))),
            )),
        ),
    ]);
    let mut sources = String::from(
        "// Generated by horizon-mods::rust_mod::stage.\npub fn mods() -> Vec<horizon_mod_api::ModSource> { vec![\n",
    );
    let mut descriptions = Vec::new();
    let mut all = Vec::new();
    for project in projects {
        let files = read(project)?;
        let declarations = check_files(&files)?;
        descriptions.push((
            declarations.meta.id.clone(),
            declarations.meta.version.clone(),
            declarations.rust.clone().unwrap(),
        ));
        all.push((declarations, files));
    }
    let order = order(&descriptions)?;
    let mut packages = BTreeMap::new();
    for (declarations, files) in &all {
        let cargo: toml::Table = std::str::from_utf8(&files["Cargo.toml"])
            .map_err(|e| e.to_string())?
            .parse()
            .map_err(|e| format!("Cargo.toml: {e}"))?;
        packages.insert(
            declarations.meta.id.clone(),
            cargo["package"]["name"].as_str().unwrap().to_owned(),
        );
    }
    let mut root: toml::Table = std::fs::read_to_string(workspace.join("Cargo.toml"))
        .map_err(|e| e.to_string())?
        .parse()
        .map_err(|e| format!("workspace Cargo.toml: {e}"))?;
    for i in order {
        let (declarations, files) = &all[i];
        let id = &declarations.meta.id;
        let relative = format!("compiled-mods/{id}");
        let dir = workspace.join(&relative);
        for (name, bytes) in files {
            let target = dir.join(name);
            std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
            std::fs::write(target, bytes).map_err(|e| e.to_string())?;
        }
        let mut manifest: toml::Table = std::str::from_utf8(&files["Cargo.toml"])
            .map_err(|e| e.to_string())?
            .parse()
            .map_err(|e| format!("{id}: {e}"))?;
        let package = manifest["package"]["name"].as_str().unwrap().to_owned();
        let deps = manifest
            .entry("dependencies")
            .or_insert(toml::Value::Table(toml::Table::new()))
            .as_table_mut()
            .ok_or("invalid dependencies table")?;
        for dependency in declarations.rust.as_ref().unwrap().dependencies.keys() {
            if let Some(declared) = deps.get(dependency).cloned() {
                let mut value = match declared {
                    toml::Value::String(version) => {
                        toml::Table::from_iter([("version".into(), toml::Value::String(version))])
                    }
                    toml::Value::Table(table) => table,
                    _ => return Err(format!("{id}: invalid Cargo dependency {dependency}")),
                };
                value.retain(|key, _| {
                    matches!(
                        key,
                        "version" | "features" | "optional" | "default-features"
                    )
                });
                value.insert(
                    "path".into(),
                    toml::Value::String(format!("../{dependency}")),
                );
                value.insert(
                    "package".into(),
                    toml::Value::String(packages[dependency].clone()),
                );
                deps.insert(dependency.clone(), toml::Value::Table(value));
            }
        }
        deps.insert(
            "horizon-mod-api".into(),
            toml::Value::Table(toml::Table::from_iter([(
                "path".into(),
                toml::Value::String("../../crates/horizon-mod-api".into()),
            )])),
        );
        std::fs::write(
            dir.join("Cargo.toml"),
            toml::to_string(&manifest).map_err(|e| e.to_string())?,
        )
        .map_err(|e| e.to_string())?;
        root["workspace"]["members"]
            .as_array_mut()
            .ok_or("workspace members is not an array")?
            .push(toml::Value::String(relative));
        let alias = format!("inox_{id}");
        cargo["dependencies"]
            .as_table_mut()
            .ok_or("client dependencies is not a table")?
            .insert(
                alias.clone(),
                toml::Value::Table(toml::Table::from_iter([
                    ("path".into(), toml::Value::String(format!("../{id}"))),
                    ("package".into(), toml::Value::String(package)),
                ])),
            );
        let entry = &declarations.rust.as_ref().unwrap().entry;
        sources.push_str(&format!("horizon_mod_api::ModSource {{ manifest: include_str!({:?}), fingerprint: concat!({:?}, \"/\", env!(\"HORIZON_MOD_BUILD_ID\")), files: vec![", format!("../../{id}/inox.toml"), fingerprint(files)));
        for name in files
            .keys()
            .filter(|n| n.starts_with("assets/") || n.starts_with("data/"))
        {
            sources.push_str(&format!(
                "({name:?}, include_bytes!({:?}).as_slice()),",
                format!("../../{id}/{name}")
            ));
        }
        sources.push_str(&format!("], module: {alias}::{entry}() }},\n"));
    }
    sources.push_str("] }\n");
    root["workspace"]["members"]
        .as_array_mut()
        .ok_or("workspace members is not an array")?
        .push(toml::Value::String(format!("compiled-mods/{GAME_PACKAGE}")));
    std::fs::create_dir_all(game.join("src")).map_err(|e| e.to_string())?;
    std::fs::write(
        game.join("Cargo.toml"),
        toml::to_string(&cargo).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(game.join("src/main.rs"), GAME_MAIN).map_err(|e| e.to_string())?;
    std::fs::write(
        workspace.join("Cargo.toml"),
        toml::to_string(&root).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    std::fs::write(game.join("src/compiled_mods.rs"), sources).map_err(|e| e.to_string())?;
    Ok(())
}

/// The modded executable: crates/horizon-client/src/main.rs plus the compiled mods.
const GAME_MAIN: &str = r#"//! Generated by horizon-mods::rust_mod::stage.
#![forbid(unsafe_code)]
mod compiled_mods;

#[global_allocator]
static ALLOCATOR: horizon_threads::Allocator = horizon_threads::Allocator;

static MOD_API_LEVEL: [u8; 27] = horizon_mods::api_level::MARK;

fn main() {
    std::hint::black_box(&MOD_API_LEVEL);
    horizon_client::game::set_compiled_mods(compiled_mods::mods);
    horizon_client::entry::main();
}
"#;

/// Source identity for the native build key and multiplayer manifest.
pub fn fingerprint(files: &BTreeMap<String, Vec<u8>>) -> String {
    use sha2::{Digest, Sha256};
    let mut hash = Sha256::new();
    for (name, bytes) in files {
        hash.update((name.len() as u64).to_le_bytes());
        hash.update(name.as_bytes());
        hash.update((bytes.len() as u64).to_le_bytes());
        hash.update(bytes);
    }
    format!("{:x}", hash.finalize())
}

/// Copy build inputs, excluding caches, prepared Minecraft files and the separate launcher.
/// Refuse a destination inside the source to avoid recursively copying a staging directory.
pub fn copy_workspace(from: &Path, to: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    let from = from.canonicalize().map_err(|e| e.to_string())?;
    let absolute_to = std::path::absolute(to).map_err(|e| e.to_string())?;
    let parent = absolute_to
        .parent()
        .ok_or("staging needs a parent directory")?;
    std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let absolute_to = parent.canonicalize().map_err(|e| e.to_string())?.join(
        absolute_to
            .file_name()
            .ok_or("staging needs a directory name")?,
    );
    if absolute_to.starts_with(&from) {
        return Err("native staging must be outside the source workspace".into());
    }
    if absolute_to.exists() {
        return Err("native source staging must be a fresh directory".into());
    }
    fn copy(from: &Path, root: &Path, to: &Path, hash: &mut Sha256) -> Result<(), String> {
        let mut entries: Vec<_> = std::fs::read_dir(from)
            .map_err(|e| e.to_string())?
            .collect::<Result<_, _>>()
            .map_err(|e| e.to_string())?;
        entries.sort_by_key(|e| e.file_name());
        for entry in entries {
            let name = entry.file_name();
            if matches!(
                name.to_str(),
                Some(
                    "target" | ".git" | ".local" | ".horizon-cache" | "launcher" | "compiled-mods"
                )
            ) {
                continue;
            }
            let path = entry.path();
            let ty = entry.file_type().map_err(|e| e.to_string())?;
            if ty.is_symlink() {
                return Err(format!(
                    "{}: workspace contains a symbolic link",
                    path.display()
                ));
            }
            let target = to.join(path.strip_prefix(root).map_err(|e| e.to_string())?);
            if ty.is_dir() {
                copy(&path, root, to, hash)?;
            } else if ty.is_file() {
                let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
                let relative = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                hash.update((relative.len() as u64).to_le_bytes());
                hash.update(relative.as_bytes());
                hash.update((bytes.len() as u64).to_le_bytes());
                hash.update(&bytes);
                std::fs::create_dir_all(target.parent().unwrap()).map_err(|e| e.to_string())?;
                if std::fs::read(&target).ok().as_ref() != Some(&bytes) {
                    std::fs::copy(&path, target).map_err(|e| e.to_string())?;
                }
            }
        }
        Ok(())
    }
    let mut hash = Sha256::new();
    copy(&from, &from, &absolute_to, &mut hash)?;
    Ok(format!("{:x}", hash.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn canonical_windows_drive_paths_work_with_native_compilers() {
        assert_eq!(
            plain_path(PathBuf::from(r"\\?\C:\build\source")),
            PathBuf::from(r"C:\build\source")
        );
        let unc = PathBuf::from(r"\\?\UNC\server\build");
        assert_eq!(plain_path(unc.clone()), unc);
        assert_eq!(
            plain_path(PathBuf::from("/build/source")),
            PathBuf::from("/build/source")
        );
    }
    #[test]
    fn rust_source_selection_is_read_without_running_code() {
        let root =
            std::env::temp_dir().join(format!("horizon-rust-selection-{}", std::process::id()));
        std::fs::create_dir_all(root.join(INSTALLED)).unwrap();
        let path = root.join(INSTALLED).join("ruby.imod");
        std::fs::write(&path, write(&fixture()).unwrap()).unwrap();
        assert!(selected_sources(&root).is_empty());
        std::fs::write(root.join("horizon-mods.txt"), "mod.ruby:true\n").unwrap();
        assert_eq!(selected_sources(&root), [path]);
        std::fs::write(root.join("horizon-mods.txt"), "mod.ruby:false\n").unwrap();
        assert!(selected_sources(&root).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
    fn rust(deps: &[(&str, &str)]) -> RustMod {
        RustMod {
            api: API.into(),
            entry: "create".into(),
            dependencies: deps
                .iter()
                .map(|(a, b)| (a.to_string(), b.to_string()))
                .collect(),
        }
    }
    #[test]
    fn dependency_errors_and_order_are_deterministic() {
        let a = ("a".into(), "1".into(), rust(&[("b", "1")]));
        let b = ("b".into(), "1".into(), rust(&[]));
        assert_eq!(order(&[a.clone(), b.clone()]).unwrap(), [1, 0]);
        assert!(order(std::slice::from_ref(&a)).is_err());
        assert!(order(&[a.clone(), ("b".into(), "2".into(), rust(&[]))]).is_err());
        assert!(order(&[a, ("b".into(), "1".into(), rust(&[("a", "*")]))]).is_err());
        assert!(order(&[b.clone(), b]).is_err());
    }
    #[test]
    fn rejects_traversal_and_entry_injection() {
        for path in ["../x", "a/../../x", "/absolute", "C:/x", "a\\b", "a/./b"] {
            assert!(check_path(path).is_err());
        }
        let mut r = rust(&[]);
        r.entry = "create();panic!()".into();
        assert!(r.check().is_err());
    }
    pub(crate) fn fixture() -> BTreeMap<String, Vec<u8>> {
        BTreeMap::from([
            ("inox.toml".into(),b"[mod]\nid='ruby'\nname='Ruby'\nversion='1.0.0'\n[rust]\napi='0.1'\nentry='create'\n".to_vec()),
            ("Cargo.toml".into(),b"[package]\nname='ruby'\nversion='1.0.0'\nedition='2024'\n[dependencies]\nhorizon-mod-api='0.1'\n".to_vec()),
            ("src/lib.rs".into(),b"pub fn create() -> Box<dyn horizon_mod_api::Mod> { panic!() }".to_vec()),
        ])
    }
    #[test]
    fn source_packages_are_bounded_checked_and_not_content_packages() {
        let mut files = fixture();
        assert!(write(&files).is_ok());
        files.insert("../escape".into(), Vec::new());
        assert!(write(&files).is_err());
        files.remove("../escape");
        files.insert("src/Lib.rs".into(), Vec::new());
        assert!(write(&files).is_err());
        files.remove("src/Lib.rs");
        files.insert(
            "Cargo.toml".into(),
            b"[package]\nname='ruby'\nversion='1.0.0'\n[dependencies]\nevil={path='../outside'}\n"
                .to_vec(),
        );
        assert!(write(&files).unwrap_err().contains("dependencies"));
    }
    #[test]
    fn staging_generates_cargo_dependencies_and_embedded_assets_only_in_copy() {
        let root = std::env::temp_dir().join(format!(
            "horizon-rust-stage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let source = root.join("source");
        let output = root.join("staged");
        let project = root.join("ruby.imod");
        for (name, text) in [
            (
                "Cargo.toml",
                "[workspace]\nmembers=['crates/horizon-client','crates/horizon-mod-api']",
            ),
            (
                "crates/horizon-client/Cargo.toml",
                "[package]\nname='horizon-client'\nversion='0.1.0'\n[dependencies]",
            ),
            (
                "crates/horizon-mod-api/Cargo.toml",
                "[package]\nname='horizon-mod-api'\nversion='0.1.0'",
            ),
        ] {
            let path = source.join(name);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        }
        let mut files = fixture();
        files.insert("assets/ruby/lang/en_us.json".into(), b"{}".to_vec());
        std::fs::write(&project, write(&files).unwrap()).unwrap();
        let cached = root.join("sha256-cache");
        std::fs::write(&cached, write(&files).unwrap()).unwrap();
        assert!(descriptor(&cached).unwrap().is_some());
        assert_eq!(crate::Package::read(&cached).unwrap().manifest.id, "ruby");
        copy_workspace(&source, &output).unwrap();
        let addon = root.join("addon.imod");
        let mut addon_files = fixture();
        addon_files.insert("inox.toml".into(),b"[mod]\nid='addon'\nname='Addon'\nversion='1.0.0'\n[rust]\napi='0.1'\nentry='create'\n[rust.dependencies]\nruby='1.0.0'".to_vec());
        addon_files.insert("Cargo.toml".into(),b"[package]\nname='addon-library'\nversion='1.0.0'\n[dependencies]\nruby={version='1',path='../ruby'}\n".to_vec());
        std::fs::write(&addon, write(&addon_files).unwrap()).unwrap();
        stage(&output, &[addon, project.clone()]).unwrap();
        let wired: toml::Table =
            std::fs::read_to_string(output.join("compiled-mods/addon/Cargo.toml"))
                .unwrap()
                .parse()
                .unwrap();
        assert_eq!(
            wired["dependencies"]["ruby"]["path"].as_str(),
            Some("../ruby")
        );
        assert_eq!(
            wired["dependencies"]["ruby"]["package"].as_str(),
            Some("ruby")
        );
        let game = output.join("compiled-mods").join(GAME_PACKAGE);
        let generated = std::fs::read_to_string(game.join("src/compiled_mods.rs")).unwrap();
        assert!(generated.contains("inox_ruby::create()"));
        assert!(generated.contains("include_bytes!(\"../../ruby/assets/ruby/lang/en_us.json\")"));
        let package: toml::Table = std::fs::read_to_string(game.join("Cargo.toml"))
            .unwrap()
            .parse()
            .unwrap();
        assert_eq!(
            package["dependencies"]["inox_ruby"]["path"].as_str(),
            Some("../ruby")
        );
        // The game library never depends on the mods: changing one does not recompile it.
        assert_eq!(
            std::fs::read_to_string(output.join("crates/horizon-client/Cargo.toml")).unwrap(),
            std::fs::read_to_string(source.join("crates/horizon-client/Cargo.toml")).unwrap()
        );
        assert!(!source.join("compiled-mods").exists());
        assert!(copy_workspace(&source, &source.join("nested")).is_err());
        #[cfg(feature = "index")]
        {
            assert!(crate::index::inspect(&crate::index::Kind::Mod, &project).is_err());
            assert_eq!(
                crate::index::inspect(&crate::index::Kind::NativeMod, &project)
                    .unwrap()
                    .api,
                "rust-0.1"
            );
        }
        std::fs::remove_dir_all(root).unwrap();
    }
}
