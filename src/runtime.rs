use std::fs::{self, File};
use std::io::{self, Read};
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, anyhow, bail};
use flate2::read::GzDecoder;

use crate::download::{self, Checksum};
use crate::paths::APP_BINARY_NAME;

const ELECTRON_RELEASES: &str = "https://github.com/electron/electron/releases/download";
const BETTER_SQLITE_RELEASES: &str = "https://github.com/WiseLibs/better-sqlite3/releases/download";

pub fn install_electron(version: &str, cache_dir: &Path, target: &Path) -> Result<()> {
    let archive_name = format!("electron-v{version}-linux-x64.zip");
    let checksums = download::fetch_text(&format!("{ELECTRON_RELEASES}/v{version}/SHASUMS256.txt"))
        .context("cannot fetch Electron checksums")?;
    let expected = checksums
        .lines()
        .find_map(|line| {
            let (hash, name) = line.split_once(' ')?;
            (name.trim_start_matches(['*', ' ']) == archive_name).then(|| hash.to_string())
        })
        .ok_or_else(|| anyhow!("no checksum for {archive_name}"))?;
    let archive_path = download::download_file(
        &format!("{ELECTRON_RELEASES}/v{version}/{archive_name}"),
        &cache_dir.join(&archive_name),
        &Checksum::Sha256Hex(expected),
    )?;
    extract_zip(&archive_path, target)?;
    let default_app = target.join("resources/default_app.asar");
    if default_app.exists() {
        fs::remove_file(default_app)?;
    }
    fs::rename(target.join("electron"), target.join(APP_BINARY_NAME))
        .context("Electron archive did not contain the electron binary")?;
    Ok(())
}

fn extract_zip(archive_path: &Path, target: &Path) -> Result<()> {
    let file = File::open(archive_path)?;
    let mut archive = zip::ZipArchive::new(file)?;
    for index in 0..archive.len() {
        let mut entry = archive.by_index(index)?;
        let Some(relative) = entry.enclosed_name() else {
            bail!("unsafe path in {}", archive_path.display());
        };
        let destination = target.join(relative);
        if entry.is_dir() {
            fs::create_dir_all(&destination)?;
            continue;
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        if entry.is_symlink() {
            let mut link_target = String::new();
            entry.read_to_string(&mut link_target)?;
            if destination.exists() {
                fs::remove_file(&destination)?;
            }
            symlink(link_target, &destination)?;
            continue;
        }
        let mut output = File::create(&destination)?;
        io::copy(&mut entry, &mut output)?;
        if let Some(mode) = entry.unix_mode() {
            fs::set_permissions(&destination, fs::Permissions::from_mode(mode & 0o7777))?;
        }
    }
    Ok(())
}

pub fn node_module_version(electron_binary: &Path) -> Result<String> {
    let output = Command::new(electron_binary)
        .env("ELECTRON_RUN_AS_NODE", "1")
        .arg("-p")
        .arg("process.versions.modules")
        .output()
        .with_context(|| format!("cannot run {}", electron_binary.display()))?;
    if !output.status.success() {
        bail!(
            "Electron failed to report its module version: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let version = String::from_utf8(output.stdout)?.trim().to_string();
    if version.is_empty() || !version.chars().all(|character| character.is_ascii_digit()) {
        bail!("unexpected module version \"{version}\"");
    }
    Ok(version)
}

pub fn download_better_sqlite(version: &str, module_version: &str, cache_dir: &Path) -> Result<Vec<u8>> {
    let archive_name = format!("better-sqlite3-v{version}-electron-v{module_version}-linux-x64.tar.gz");
    let archive_path: PathBuf = download::download_file(
        &format!("{BETTER_SQLITE_RELEASES}/v{version}/{archive_name}"),
        &cache_dir.join(&archive_name),
        &Checksum::None,
    )?;
    let mut archive = tar::Archive::new(GzDecoder::new(File::open(&archive_path)?));
    for entry in archive.entries()? {
        let mut entry = entry?;
        if entry.path()?.ends_with("build/Release/better_sqlite3.node") {
            let mut content = Vec::new();
            entry.read_to_end(&mut content)?;
            if !content.starts_with(b"\x7fELF") {
                bail!("{archive_name} does not contain a Linux library");
            }
            return Ok(content);
        }
    }
    fs::remove_file(&archive_path).ok();
    bail!("{archive_name} does not contain better_sqlite3.node")
}
