//! Developer entry to the same source staging used by the launcher's native compiler.
use horizon_mods::rust_mod;
use std::{
    path::{Path, PathBuf},
    process::Command,
};

pub(super) fn new(root: &Path, id: &str, name: &str, api: Option<&str>) -> Result<(), String> {
    let manifest = format!(
        "[mod]\nid = {id:?}\nname = {name:?}\nversion = \"0.1.0\"\n\n[rust]\napi = \"0.1\"\nentry = \"create\"\n"
    );
    let mut cargo = format!(
        "[package]\nname = {id:?}\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nhorizon-mod-api = \"0.1\"\nserde = {{ version = \"1\", features = [\"derive\"] }}\n"
    );
    if let Some(path) = api {
        let path = rust_mod::plain_path(Path::new(path).canonicalize().map_err(|e| e.to_string())?)
            .to_string_lossy()
            .replace('\\', "/");
        cargo = cargo.replace(
            "horizon-mod-api = \"0.1\"",
            &format!("horizon-mod-api = {{ path = {path:?} }}"),
        );
    }
    let source = include_str!("../template/rust/lib.rs").replace("sample_mod", id);
    for (file, text) in [
        ("inox.toml", manifest),
        ("Cargo.toml", cargo),
        ("src/lib.rs", source),
        (".gitignore", "/target/\n".into()),
    ] {
        let path = root.join(file);
        std::fs::create_dir_all(path.parent().unwrap()).map_err(|e| e.to_string())?;
        std::fs::write(path, text).map_err(|e| e.to_string())?;
    }
    rust_mod::files(root)?;
    println!(
        "Created Rust mod {}. Package: cargo horizon-mod build. Compile: cargo horizon-mod native-build --workspace <Horizon checkout> --trust-native-code",
        root.display()
    );
    Ok(())
}

pub(super) fn package(root: &Path) -> Result<PathBuf, String> {
    let files = rust_mod::files(root)?;
    let declarations = rust_mod::check_files(&files)?;
    let out = root.join("target/horizon-mod");
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let path = out.join(format!("{}.imod", declarations.meta.id));
    let partial = path.with_extension("imod.partial");
    std::fs::write(&partial, rust_mod::write(&files)?).map_err(|e| e.to_string())?;
    std::fs::rename(partial, &path).map_err(|e| e.to_string())?;
    Ok(path)
}

pub(super) fn build(args: &[String]) -> Result<PathBuf, String> {
    if !args.iter().any(|s| s == "--trust-native-code") {
        return Err("Rust mods and Cargo build scripts run with your computer's permissions. Read the source, then pass --trust-native-code.".into());
    }
    let workspace = rust_mod::plain_path(
        Path::new(
            super::flag(args, "--workspace")
                .ok_or("native-build needs --workspace <Horizon source checkout>")?,
        )
        .canonicalize()
        .map_err(|e| e.to_string())?,
    );
    let root = std::env::current_dir().map_err(|e| e.to_string())?;
    let out = super::flag(args, "--out")
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("target/horizon-native"));
    std::fs::create_dir_all(&out).map_err(|e| e.to_string())?;
    let out = rust_mod::plain_path(out.canonicalize().map_err(|e| e.to_string())?);
    let build_lock = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(out.join("build.lock"))
        .map_err(|e| e.to_string())?;
    build_lock
        .try_lock()
        .map_err(|_| "another native build is already running in this output folder".to_string())?;
    let stage = out.join(format!(
        "source-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let mut projects = vec![root];
    for pair in args.windows(2) {
        if pair[0] == "--mod" {
            projects.push(PathBuf::from(&pair[1]));
        }
    }
    for project in &projects {
        rust_mod::read(project)?;
    }
    let source_id = rust_mod::copy_workspace(&workspace, &stage)?;
    rust_mod::stage(&stage, &projects)?;
    let stable = out.join("source");
    rust_mod::sync_workspace(&stage, &stable)?;
    let checked = stage.canonicalize().map_err(|e| e.to_string())?;
    if !checked.starts_with(out.canonicalize().map_err(|e| e.to_string())?) {
        return Err("source staging escaped its build folder".into());
    }
    std::fs::remove_dir_all(checked).map_err(|e| e.to_string())?;
    let stage = stable;
    let profile = super::flag(args, "--profile").unwrap_or("fast");
    if !matches!(profile, "dev" | "fast" | "release") {
        return Err("profile must be dev, fast or release".into());
    }
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let target = out.join("target");
    if out.join("Cargo.lock").is_file() {
        std::fs::copy(out.join("Cargo.lock"), stage.join("Cargo.lock"))
            .map_err(|e| e.to_string())?;
    }
    let status = Command::new(&cargo)
        .current_dir(&stage)
        .args(["metadata", "--format-version", "1"])
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("could not resolve the combined Cargo dependencies".into());
    }
    let lock = std::fs::read(stage.join("Cargo.lock")).map_err(|e| e.to_string())?;
    let build_id = rust_mod::fingerprint(&std::collections::BTreeMap::from([
        ("source".into(), source_id.into_bytes()),
        ("Cargo.lock".into(), lock),
    ]));
    let mut command = Command::new(&cargo);
    command
        .current_dir(&stage)
        .env("CARGO_TARGET_DIR", &target)
        .env("RUST_MIN_STACK", "33554432")
        .env("HORIZON_MOD_BUILD_ID", &build_id)
        .args([
            "build",
            "--locked",
            "--profile",
            profile,
            "-j",
            "1",
            "-p",
            "horizon-client",
            "--bin",
            "horizon-client",
        ]);
    if !command.status().map_err(|e| e.to_string())?.success() {
        return Err(format!(
            "native build failed; source and compiler diagnostics are in {}",
            stage.display()
        ));
    }
    let binary = target
        .join(if profile == "dev" { "debug" } else { profile })
        .join(format!("horizon-client{}", std::env::consts::EXE_SUFFIX));
    // Keep the exact merged lockfile that produced this binary, beside it for reproducibility.
    std::fs::copy(stage.join("Cargo.lock"), out.join("Cargo.lock")).map_err(|e| e.to_string())?;
    Ok(binary)
}

pub(super) fn run(args: &[String]) -> Result<(), String> {
    let binary = build(args)?;
    let extra = args.iter().skip_while(|a| *a != "--").skip(1);
    let status = Command::new(binary)
        .args(extra)
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("game exited with {status}"))
    }
}
