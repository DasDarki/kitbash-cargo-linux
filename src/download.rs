use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{Context, Result, bail};
use base64::Engine;
use sha2::{Digest, Sha256, Sha512};

const USER_AGENT: &str = concat!("kitbash-cargo-linux/", env!("CARGO_PKG_VERSION"));

pub enum Checksum {
    Sha256Hex(String),
    Sha512Base64(String),
    None,
}

fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .user_agent(USER_AGENT)
        .timeout_connect(Duration::from_secs(30))
        .timeout_read(Duration::from_secs(120))
        .build()
}

pub fn fetch_text(url: &str) -> Result<String> {
    let response = agent()
        .get(url)
        .call()
        .with_context(|| format!("request to {url} failed"))?;
    response
        .into_string()
        .with_context(|| format!("could not read response from {url}"))
}

pub fn download_file(url: &str, target: &Path, checksum: &Checksum) -> Result<PathBuf> {
    if target.exists() && verify_file(target, checksum).unwrap_or(false) {
        println!("  using cached {}", display_name(target));
        return Ok(target.to_path_buf());
    }
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    println!("  downloading {}", display_name(target));
    let response = agent()
        .get(url)
        .call()
        .with_context(|| format!("request to {url} failed"))?;
    let total = response
        .header("Content-Length")
        .and_then(|value| value.parse::<u64>().ok());
    let partial = target.with_extension("part");
    let mut output = File::create(&partial)?;
    let mut reader = response.into_reader();
    let mut buffer = vec![0u8; 1 << 16];
    let mut received: u64 = 0;
    let mut last_percent = u64::MAX;
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        output.write_all(&buffer[..read])?;
        received += read as u64;
        if let Some(total) = total.filter(|total| *total > 0) {
            let percent = received * 100 / total;
            if percent != last_percent && percent.is_multiple_of(5) {
                print!("\r  {percent:>3}% of {:.1} MiB", total as f64 / 1_048_576.0);
                io::stdout().flush().ok();
                last_percent = percent;
            }
        }
    }
    if total.is_some() {
        println!();
    }
    output.flush()?;
    drop(output);
    if !verify_file(&partial, checksum)? {
        fs::remove_file(&partial).ok();
        bail!("checksum mismatch for {url}");
    }
    fs::rename(&partial, target)?;
    Ok(target.to_path_buf())
}

pub fn verify_file(path: &Path, checksum: &Checksum) -> Result<bool> {
    match checksum {
        Checksum::None => Ok(true),
        Checksum::Sha256Hex(expected) => {
            let digest = hash_file::<Sha256>(path)?;
            Ok(hex::encode(digest).eq_ignore_ascii_case(expected))
        }
        Checksum::Sha512Base64(expected) => {
            let digest = hash_file::<Sha512>(path)?;
            Ok(base64::engine::general_purpose::STANDARD.encode(digest) == *expected)
        }
    }
}

fn hash_file<D: Digest>(path: &Path) -> Result<Vec<u8>> {
    let mut file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;
    let mut hasher = D::new();
    let mut buffer = vec![0u8; 1 << 20];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize().to_vec())
}

fn display_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}

pub fn encode_url_path(segment: &str) -> String {
    let mut encoded = String::with_capacity(segment.len());
    for byte in segment.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    encoded
}
