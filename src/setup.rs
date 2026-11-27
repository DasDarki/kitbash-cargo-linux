use std::fs::{self, File};
use std::io::{self, Cursor, Read, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, anyhow, bail};
use sevenz_rust2::{ArchiveReader, EntryDecision, Password};

use crate::download::{self, Checksum};

const TODESKTOP_APP_ID: &str = "240724njepa5mms";
const SEVEN_ZIP_SIGNATURE: [u8; 6] = [0x37, 0x7A, 0xBC, 0xAF, 0x27, 0x1C];
const SEVEN_ZIP_START_HEADER_LENGTH: usize = 32;

pub struct ReleaseManifest {
    pub version: String,
    pub file_name: String,
    pub sha512: String,
}

impl ReleaseManifest {
    pub fn fetch() -> Result<Self> {
        let url = format!("https://download.todesktop.com/{TODESKTOP_APP_ID}/latest.yml");
        let text = download::fetch_text(&url)?;
        let value_of = |key: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(&format!("{key}: ")))
                .map(|value| value.trim().trim_matches('\'').trim_matches('"').to_string())
        };
        Ok(Self {
            version: value_of("version").context("manifest has no version")?,
            file_name: value_of("path").context("manifest has no path")?,
            sha512: value_of("sha512").context("manifest has no sha512")?,
        })
    }

    pub fn download(&self, cache_dir: &Path) -> Result<PathBuf> {
        let url = format!(
            "https://download.todesktop.com/{TODESKTOP_APP_ID}/{}",
            download::encode_url_path(&self.file_name)
        );
        download::download_file(
            &url,
            &cache_dir.join(&self.file_name),
            &Checksum::Sha512Base64(self.sha512.clone()),
        )
    }
}

pub struct ExtractedSetup {
    pub electron_version: Option<String>,
}

pub fn extract_setup(setup_path: &Path, target: &Path) -> Result<ExtractedSetup> {
    let setup = fs::read(setup_path)
        .with_context(|| format!("cannot read setup file {}", setup_path.display()))?;
    let payload = find_app_payload(&setup).context(
        "no application payload found, is this the Windows setup of Cargo by KitBash?",
    )?;
    println!(
        "  found application archive ({:.1} MiB)",
        payload.len() as f64 / 1_048_576.0
    );
    let mut archive = ArchiveReader::new(Cursor::new(payload), Password::empty())
        .map_err(|error| anyhow!("cannot open application archive: {error}"))?;
    let mut electron_version = None;
    let mut failure: Option<anyhow::Error> = None;
    archive
        .for_each_entries_with_decision(|entry, reader| {
            let name = entry.name.replace('\\', "/");
            let outcome = handle_entry(&name, entry.is_directory, reader, target, &mut electron_version);
            match outcome {
                Ok(decision) => Ok(decision),
                Err(error) => {
                    failure = Some(error);
                    Ok(EntryDecision::Stop)
                }
            }
        })
        .map_err(|error| anyhow!("cannot extract application archive: {error}"))?;
    if let Some(error) = failure {
        return Err(error);
    }
    if !target.join("resources/app.asar").is_file() {
        bail!("the setup does not contain resources/app.asar");
    }
    Ok(ExtractedSetup { electron_version })
}

fn handle_entry(
    name: &str,
    is_directory: bool,
    reader: &mut dyn Read,
    target: &Path,
    electron_version: &mut Option<String>,
) -> Result<EntryDecision> {
    if is_main_executable(name) {
        if electron_version.is_none() {
            *electron_version = scan_electron_version(reader)?;
        }
        return Ok(EntryDecision::Continue);
    }
    if !should_extract(name) {
        return Ok(EntryDecision::SkipBlock);
    }
    let relative = safe_relative_path(name)?;
    let destination = target.join(relative);
    if is_directory {
        fs::create_dir_all(&destination)?;
        return Ok(EntryDecision::Continue);
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let mut file = File::create(&destination)
        .with_context(|| format!("cannot create {}", destination.display()))?;
    io::copy(reader, &mut file)?;
    file.flush()?;
    Ok(EntryDecision::Continue)
}

fn is_main_executable(name: &str) -> bool {
    !name.contains('/') && name.to_ascii_lowercase().ends_with(".exe")
}

fn should_extract(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    if !lower.starts_with("resources/") {
        return false;
    }
    if lower.starts_with("resources/windows-x64") {
        return false;
    }
    if lower.ends_with(".exe") || lower.ends_with(".dll") || lower.ends_with(".node") {
        return false;
    }
    true
}

fn safe_relative_path(name: &str) -> Result<PathBuf> {
    let mut path = PathBuf::new();
    for component in name.split('/') {
        match component {
            "" | "." => continue,
            ".." => bail!("archive entry escapes target directory: {name}"),
            part => path.push(part),
        }
    }
    Ok(path)
}

fn find_app_payload(setup: &[u8]) -> Option<&[u8]> {
    let mut best: Option<&[u8]> = None;
    let mut search_start = 0;
    while let Some(position) = find_bytes(&setup[search_start..], &SEVEN_ZIP_SIGNATURE) {
        let start = search_start + position;
        search_start = start + 1;
        let Some(length) = seven_zip_length(&setup[start..]) else {
            continue;
        };
        let candidate = &setup[start..start + length];
        if best.is_none_or(|current| candidate.len() > current.len()) {
            best = Some(candidate);
        }
    }
    best
}

fn seven_zip_length(data: &[u8]) -> Option<usize> {
    if data.len() < SEVEN_ZIP_START_HEADER_LENGTH {
        return None;
    }
    let stored_crc = u32::from_le_bytes(data[8..12].try_into().ok()?);
    if crc32fast::hash(&data[12..32]) != stored_crc {
        return None;
    }
    let next_header_offset = u64::from_le_bytes(data[12..20].try_into().ok()?);
    let next_header_size = u64::from_le_bytes(data[20..28].try_into().ok()?);
    let total = (SEVEN_ZIP_START_HEADER_LENGTH as u64)
        .checked_add(next_header_offset)?
        .checked_add(next_header_size)?;
    let total = usize::try_from(total).ok()?;
    if total > data.len() || next_header_size == 0 {
        return None;
    }
    Some(total)
}

fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

fn scan_electron_version(reader: &mut dyn Read) -> Result<Option<String>> {
    const MARKER: &[u8] = b"Electron/";
    const OVERLAP: usize = 32;
    let mut buffer = vec![0u8; 1 << 20];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            return Ok(None);
        }
        let mut window = std::mem::take(&mut carry);
        window.extend_from_slice(&buffer[..read]);
        let mut offset = 0;
        while let Some(position) = find_bytes(&window[offset..], MARKER) {
            let start = offset + position + MARKER.len();
            if let Some(version) = parse_version(&window[start..]) {
                return Ok(Some(version));
            }
            offset = start;
        }
        let keep = window.len().min(OVERLAP);
        carry = window[window.len() - keep..].to_vec();
    }
}

fn parse_version(data: &[u8]) -> Option<String> {
    let text: String = data
        .iter()
        .take(16)
        .take_while(|byte| byte.is_ascii_digit() || **byte == b'.')
        .map(|byte| *byte as char)
        .collect();
    let parts: Vec<&str> = text.split('.').collect();
    if parts.len() == 3 && parts.iter().all(|part| !part.is_empty()) {
        Some(text)
    } else {
        None
    }
}
