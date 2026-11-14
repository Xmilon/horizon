//! Mods an author develops, linked to their folders: added from a folder of mod projects, packed
//! and installed like any `.hmod`, and packed again from the folder before each build, so an edit
//! is one click (Build and play) from the game. The link is `<id>.folder` beside the mod's
//! `.hmod`, holding the project folder's path.
use crate::browser::java;
use crate::github_mods;
use crate::native_mods::INSTALLED;
use std::fs;
use std::path::{Path, PathBuf};

fn link_path(home: &Path, id: &str) -> PathBuf {
    home.join(INSTALLED).join(format!("{id}.folder"))
}

/// The folder mod `id` in `home` is linked to, if it is an author's folder mod.
pub fn linked(home: &Path, id: &str) -> Option<PathBuf> {
    fs::read_to_string(link_path(home, id))
        .ok()
        .map(|text| PathBuf::from(text.trim()))
}

/// Every folder mod of `home`: its id and folder, by id.
pub fn all(home: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(String, PathBuf)> = fs::read_dir(home.join(INSTALLED))
        .map(|dir| {
            dir.flatten()
                .filter_map(|e| {
                    let name = e.file_name().to_string_lossy().into_owned();
                    let id = name.strip_suffix(".folder")?.to_owned();
                    // A link whose mod was removed some other way is left over, not a mod.
                    if !home.join(INSTALLED).join(format!("{id}.hmod")).exists() {
                        return None;
                    }
                    let folder = linked(home, &id)?;
                    Some((id, folder))
                })
                .collect()
        })
        .unwrap_or_default();
    found.sort();
    found
}

/// Packs `bundle` into `home`'s mods as `<id>.hmod`, checked like a registry file.
fn install(home: &Path, bundle: &crate::native_mods::Bundle) -> Result<(), String> {
    let id = &bundle.manifest.id;
    let bytes = github_mods::pack(bundle)?;
    let dir = home.join(INSTALLED);
    fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let target = dir.join(format!("{id}.hmod"));
    let staged = dir.join(format!("{id}.hmod.new"));
    fs::write(&staged, &bytes).map_err(|e| format!("{}: {e}", staged.display()))?;
    if let Err(e) =
        horizon_mod_package::index::inspect(&horizon_mod_package::index::Kind::NativeMod, &staged)
    {
        let _ = fs::remove_file(&staged);
        return Err(format!("{id}: {e}"));
    }
    fs::rename(&staged, &target).map_err(|e| format!("{}: {e}", target.display()))
}

/// Adds every mod project under `root` to `home`, linked to its folder and turned on. A mod of
/// the same id installed some other way is never replaced. The ids added.
pub fn add(home: &Path, root: &Path) -> Result<Vec<String>, String> {
    let found = github_mods::find_mods(root)?;
    if found.is_empty() {
        return Err(format!(
            "No mod project in {}: a mod's folder holds its manifest.",
            root.display()
        ));
    }
    let lock = crate::browser::lock_home(home).map_err(|e| e.to_string())?;
    for bundle in &found {
        let id = &bundle.manifest.id;
        if home.join(INSTALLED).join(format!("{id}.hmod")).exists() && linked(home, id).is_none() {
            return Err(format!(
                "{id} is already installed and not from a folder: remove it first."
            ));
        }
    }
    let mut ids = Vec::new();
    for bundle in &found {
        install(home, bundle)?;
        let link = link_path(home, &bundle.manifest.id);
        let folder = bundle
            .dir
            .canonicalize()
            .unwrap_or_else(|_| bundle.dir.clone());
        fs::write(&link, folder.to_string_lossy().as_bytes())
            .map_err(|e| format!("{}: {e}", link.display()))?;
        ids.push(bundle.manifest.id.clone());
    }
    drop(lock);
    let refs: Vec<&str> = ids.iter().map(String::as_str).collect();
    java::set_all_enabled(home, &refs, true)?;
    Ok(ids)
}

/// Packs every folder mod of `home` again from its folder: the edits since reach the next build.
pub fn refresh(home: &Path) -> Result<(), String> {
    let linked = all(home);
    if linked.is_empty() {
        return Ok(());
    }
    let _lock = crate::browser::lock_home(home).map_err(|e| e.to_string())?;
    for (id, folder) in linked {
        let bundle = crate::native_mods::bundle_in(&folder)?
            .ok_or_else(|| format!("{id}: no mod project in {} any more.", folder.display()))?;
        if bundle.manifest.id != id {
            return Err(format!(
                "{} now holds {}, not {id}.",
                folder.display(),
                bundle.manifest.id
            ));
        }
        install(home, &bundle)?;
    }
    Ok(())
}

/// Unlinks folder mod `id`, removes its `.hmod` and turns it off; the folder stays.
pub fn remove(home: &Path, id: &str) -> Result<(), String> {
    let lock = crate::browser::lock_home(home).map_err(|e| e.to_string())?;
    for path in [
        link_path(home, id),
        home.join(INSTALLED).join(format!("{id}.hmod")),
    ] {
        if let Err(e) = fs::remove_file(&path) {
            if e.kind() != std::io::ErrorKind::NotFound {
                return Err(format!("{}: {e}", path.display()));
            }
        }
    }
    drop(lock);
    java::set_enabled(home, id, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mod_folder_is_packed_installed_turned_on_refreshed_and_removed() {
        let home = std::env::temp_dir().join(format!("hz-folder-add-{}", std::process::id()));
        let _ = fs::remove_dir_all(&home);
        fs::create_dir_all(&home).unwrap();
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../tools/fixtures/paper-trace-mod");
        let ids = add(&home, &fixture).unwrap();
        assert_eq!(ids, ["paper_trace"]);
        let hmod = home.join(INSTALLED).join("paper_trace.hmod");
        assert!(hmod.is_file());
        assert!(java::is_enabled(&home, "paper_trace"));
        assert_eq!(all(&home).len(), 1);
        assert_eq!(
            linked(&home, "paper_trace"),
            Some(fixture.canonicalize().unwrap())
        );
        // Packing the same folder again gives the same file: a refresh changes nothing.
        let before = fs::read(&hmod).unwrap();
        refresh(&home).unwrap();
        assert_eq!(fs::read(&hmod).unwrap(), before);
        // Added twice is fine (it is linked); a non-linked mod of that id is never replaced.
        assert_eq!(add(&home, &fixture).unwrap(), ["paper_trace"]);
        remove(&home, "paper_trace").unwrap();
        assert!(!hmod.exists());
        assert!(all(&home).is_empty());
        assert!(!java::is_enabled(&home, "paper_trace"));
        fs::write(&hmod, b"x").unwrap();
        assert!(add(&home, &fixture)
            .unwrap_err()
            .contains("not from a folder"));
        let _ = fs::remove_dir_all(&home);
    }

    #[test]
    fn links_are_read_back_by_id() {
        let home = std::env::temp_dir().join(format!("hz-folder-mods-{}", std::process::id()));
        let dir = home.join(INSTALLED);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("greeter.folder"), "/src/greeter\n").unwrap();
        fs::write(dir.join("greeter.hmod"), b"x").unwrap();
        assert_eq!(
            linked(&home, "greeter"),
            Some(PathBuf::from("/src/greeter"))
        );
        assert_eq!(
            all(&home),
            [("greeter".to_string(), PathBuf::from("/src/greeter"))]
        );
        assert_eq!(linked(&home, "other"), None);
        let _ = fs::remove_dir_all(&home);
    }
}
