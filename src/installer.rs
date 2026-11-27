use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Value, json};

use crate::asar::Asar;
use crate::paths::Paths;
use crate::setup::{self, ReleaseManifest};
use crate::{desktop, patcher, runtime};

const BETTER_SQLITE_BINARY: &str = "node_modules/better-sqlite3/build/Release/better_sqlite3.node";

pub struct InstallOptions {
    pub setup: Option<PathBuf>,
    pub electron_version: Option<String>,
    pub extra_blender_versions: Vec<String>,
}

pub struct InstallInfo {
    pub cargo_version: String,
    pub electron_version: String,
    pub extra_blender_versions: Vec<String>,
}

pub fn install(paths: &Paths, options: InstallOptions) -> Result<()> {
    fs::create_dir_all(paths.install_root())?;
    fs::create_dir_all(paths.cache_dir())?;

    let setup_path = match options.setup {
        Some(path) => path,
        None => {
            println!("[1/7] Fetching latest Cargo release");
            let manifest = ReleaseManifest::fetch()?;
            println!("  latest version is {}", manifest.version);
            manifest.download(&paths.cache_dir())?
        }
    };

    let staging = paths.staging_dir();
    if staging.exists() {
        fs::remove_dir_all(&staging)?;
    }
    fs::create_dir_all(&staging)?;

    println!("[2/7] Extracting {}", setup_path.display());
    let extracted = setup::extract_setup(&setup_path, &staging)?;
    let electron_version = options
        .electron_version
        .or(extracted.electron_version)
        .context("could not detect the Electron version, pass --electron-version")?;
    println!("  application is built on Electron {electron_version}");

    println!("[3/7] Installing Electron {electron_version} for Linux");
    runtime::install_electron(&electron_version, &paths.cache_dir(), &staging)?;

    println!("[4/7] Patching application for Linux");
    let asar_path = staging.join("resources/app.asar");
    let mut asar = Asar::open(&asar_path)?;
    let package: Value = serde_json::from_str(&asar.read_text("package.json")?)?;
    let cargo_version = package
        .get("version")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_string();
    let main_script = package
        .get("main")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("package.json has no main entry"))?
        .trim_start_matches("./")
        .to_string();
    let (patched_main, report) = patcher::patch_main_script(
        &asar.read_text(&main_script)?,
        &options.extra_blender_versions,
    )?;
    for name in &report.applied {
        println!("  applied: {name}");
    }
    for name in &report.skipped {
        println!("  skipped (not found in this version): {name}");
    }
    asar.replace_file(&main_script, patched_main.into_bytes())?;
    let patched_package = patcher::patch_package_json(&asar.read_text("package.json")?)?;
    asar.replace_file("package.json", patched_package.into_bytes())?;

    println!("[5/7] Installing native modules");
    let sqlite_package: Value =
        serde_json::from_str(&asar.read_text("node_modules/better-sqlite3/package.json")?)?;
    let sqlite_version = sqlite_package
        .get("version")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("better-sqlite3 has no version"))?;
    let module_version = runtime::node_module_version(&staging.join(crate::paths::APP_BINARY_NAME))?;
    println!("  better-sqlite3 {sqlite_version} for module version {module_version}");
    let sqlite_binary = runtime::download_better_sqlite(sqlite_version, &module_version, &paths.cache_dir())?;
    let unpacked_target = staging.join("resources/app.asar.unpacked").join(BETTER_SQLITE_BINARY);
    fs::create_dir_all(unpacked_target.parent().unwrap())?;
    fs::write(&unpacked_target, &sqlite_binary)?;
    asar.update_unpacked_file(BETTER_SQLITE_BINARY, &sqlite_binary)?;

    let rebuilt = staging.join("resources/app.asar.rebuilt");
    asar.write(&rebuilt)?;
    drop(asar);
    fs::rename(&rebuilt, &asar_path)?;

    println!("[6/7] Activating installation");
    activate_staging(paths)?;
    install_loader_binary(paths)?;
    write_install_info(
        paths,
        &InstallInfo {
            cargo_version: cargo_version.clone(),
            electron_version: electron_version.clone(),
            extra_blender_versions: options.extra_blender_versions.clone(),
        },
    )?;

    println!("[7/7] Registering desktop integration");
    desktop::install_icons(paths, &paths.app_dir().join("resources/Cargo_Icon.png"))?;
    desktop::install_desktop_entry(paths, &cargo_version)?;

    println!();
    println!("Cargo by KitBash {cargo_version} is installed.");
    println!("Start it from your application menu or run: {} launch", paths.loader_binary().display());
    Ok(())
}

pub fn update(paths: &Paths, force: bool, blender_versions: Option<Vec<String>>) -> Result<()> {
    let manifest = ReleaseManifest::fetch()?;
    let installed = read_install_info(paths);
    let extra_blender_versions = blender_versions
        .or_else(|| installed.as_ref().map(|info| info.extra_blender_versions.clone()))
        .unwrap_or_default();
    if let Some(info) = &installed {
        if info.cargo_version == manifest.version && !force && info.extra_blender_versions == extra_blender_versions {
            println!("Cargo by KitBash {} is already up to date.", info.cargo_version);
            return Ok(());
        }
        println!("Updating from {} to {}", info.cargo_version, manifest.version);
    }
    let setup_path = manifest.download(&paths.cache_dir())?;
    install(
        paths,
        InstallOptions {
            setup: Some(setup_path),
            electron_version: None,
            extra_blender_versions,
        },
    )
}

pub fn uninstall(paths: &Paths) -> Result<()> {
    desktop::remove_desktop_entry(paths);
    desktop::remove_icons(paths);
    if paths.install_root().exists() {
        fs::remove_dir_all(paths.install_root())?;
    }
    if paths.cache_dir().exists() {
        fs::remove_dir_all(paths.cache_dir())?;
    }
    println!("Cargo by KitBash has been removed.");
    if let Some(home) = env::var_os("HOME").map(PathBuf::from) {
        println!("Your data was kept in:");
        println!("  {}", home.join(".kitbash3d").display());
        println!("  {}", home.join(".config/Cargo by KitBash").display());
    }
    Ok(())
}

pub fn status(paths: &Paths) -> Result<()> {
    match read_install_info(paths) {
        Some(info) => {
            println!("Installed:  Cargo by KitBash {}", info.cargo_version);
            println!("Electron:   {}", info.electron_version);
            if !info.extra_blender_versions.is_empty() {
                println!("Blender:    also allowing {}", info.extra_blender_versions.join(", "));
            }
            println!("Location:   {}", paths.app_dir().display());
        }
        None => println!("Installed:  no"),
    }
    match ReleaseManifest::fetch() {
        Ok(manifest) => println!("Latest:     {}", manifest.version),
        Err(error) => println!("Latest:     unknown ({error})"),
    }
    Ok(())
}

fn activate_staging(paths: &Paths) -> Result<()> {
    let previous = paths.previous_dir();
    if previous.exists() {
        fs::remove_dir_all(&previous)?;
    }
    if paths.app_dir().exists() {
        fs::rename(paths.app_dir(), &previous)?;
    }
    fs::rename(paths.staging_dir(), paths.app_dir())?;
    if previous.exists() {
        fs::remove_dir_all(&previous)?;
    }
    Ok(())
}

fn install_loader_binary(paths: &Paths) -> Result<()> {
    let current = env::current_exe().context("cannot locate the running loader binary")?;
    let target = paths.loader_binary();
    if same_file(&current, &target) {
        return Ok(());
    }
    let temporary = target.with_extension("new");
    fs::copy(&current, &temporary)?;
    fs::rename(&temporary, &target)?;
    Ok(())
}

fn same_file(first: &Path, second: &Path) -> bool {
    match (fs::canonicalize(first), fs::canonicalize(second)) {
        (Ok(first), Ok(second)) => first == second,
        _ => false,
    }
}

fn write_install_info(paths: &Paths, info: &InstallInfo) -> Result<()> {
    let installed_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let value = json!({
        "cargoVersion": info.cargo_version,
        "electronVersion": info.electron_version,
        "extraBlenderVersions": info.extra_blender_versions,
        "installedAt": installed_at,
    });
    fs::write(paths.install_info(), serde_json::to_string_pretty(&value)?)?;
    Ok(())
}

pub fn read_install_info(paths: &Paths) -> Option<InstallInfo> {
    let text = fs::read_to_string(paths.install_info()).ok()?;
    let value: Value = serde_json::from_str(&text).ok()?;
    Some(InstallInfo {
        cargo_version: value.get("cargoVersion")?.as_str()?.to_string(),
        electron_version: value.get("electronVersion")?.as_str()?.to_string(),
        extra_blender_versions: value
            .get("extraBlenderVersions")
            .and_then(Value::as_array)
            .map(|versions| {
                versions
                    .iter()
                    .filter_map(Value::as_str)
                    .map(String::from)
                    .collect()
            })
            .unwrap_or_default(),
    })
}

pub fn ensure_installed(paths: &Paths) -> Result<()> {
    if !paths.app_binary().is_file() {
        bail!(
            "Cargo by KitBash is not installed yet, run \"{} install\" first",
            crate::paths::LOADER_BINARY_NAME
        );
    }
    Ok(())
}
