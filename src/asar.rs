use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

use anyhow::{Context, Result, anyhow, bail};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};

const INTEGRITY_BLOCK_SIZE: usize = 4 * 1024 * 1024;

pub struct Asar {
    header: Value,
    data: Vec<u8>,
    data_offset: usize,
    replacements: HashMap<String, Vec<u8>>,
}

impl Asar {
    pub fn open(path: &Path) -> Result<Self> {
        let data = fs::read(path).with_context(|| format!("cannot read {}", path.display()))?;
        if data.len() < 16 {
            bail!("{} is too small to be an asar archive", path.display());
        }
        let read_u32 = |offset: usize| u32::from_le_bytes(data[offset..offset + 4].try_into().unwrap()) as usize;
        let header_size = read_u32(4);
        let json_length = read_u32(12);
        let json_end = 16 + json_length;
        if json_end > data.len() || 8 + header_size > data.len() {
            bail!("{} has a corrupt asar header", path.display());
        }
        let header: Value = serde_json::from_slice(&data[16..json_end])
            .context("asar header is not valid JSON")?;
        Ok(Self {
            header,
            data_offset: 8 + header_size,
            data,
            replacements: HashMap::new(),
        })
    }

    pub fn read_file(&self, path: &str) -> Result<Vec<u8>> {
        if let Some(content) = self.replacements.get(path) {
            return Ok(content.clone());
        }
        let entry = self.entry(path)?;
        if entry.get("unpacked").and_then(Value::as_bool).unwrap_or(false) {
            bail!("{path} is stored outside of the archive");
        }
        let (offset, size) = file_range(entry)?;
        let start = self.data_offset + offset;
        self.data
            .get(start..start + size)
            .map(<[u8]>::to_vec)
            .ok_or_else(|| anyhow!("{path} points outside of the archive"))
    }

    pub fn read_text(&self, path: &str) -> Result<String> {
        String::from_utf8(self.read_file(path)?).with_context(|| format!("{path} is not UTF-8"))
    }

    pub fn replace_file(&mut self, path: &str, content: Vec<u8>) -> Result<()> {
        self.entry(path)?;
        self.replacements.insert(path.to_string(), content);
        Ok(())
    }

    pub fn update_unpacked_file(&mut self, path: &str, content: &[u8]) -> Result<()> {
        let entry = self.entry_mut(path)?;
        let object = entry
            .as_object_mut()
            .ok_or_else(|| anyhow!("{path} is not a file entry"))?;
        object.insert("size".into(), json!(content.len()));
        object.insert("integrity".into(), integrity(content));
        Ok(())
    }

    pub fn write(&self, path: &Path) -> Result<()> {
        let mut header = self.header.clone();
        let mut chunks: Vec<&[u8]> = Vec::new();
        let mut next_offset: usize = 0;
        let root = header
            .get_mut("files")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| anyhow!("asar header has no files"))?;
        self.layout_directory(root, "", &mut chunks, &mut next_offset)?;

        let json_text = serde_json::to_vec(&header)?;
        let padded_length = json_text.len().div_ceil(4) * 4;
        let header_pickle_size = 8 + padded_length;

        let file = File::create(path).with_context(|| format!("cannot create {}", path.display()))?;
        let mut writer = BufWriter::new(file);
        writer.write_all(&4u32.to_le_bytes())?;
        writer.write_all(&(header_pickle_size as u32).to_le_bytes())?;
        writer.write_all(&((4 + padded_length) as u32).to_le_bytes())?;
        writer.write_all(&(json_text.len() as u32).to_le_bytes())?;
        writer.write_all(&json_text)?;
        writer.write_all(&vec![0u8; padded_length - json_text.len()])?;
        for chunk in chunks {
            writer.write_all(chunk)?;
        }
        writer.flush()?;
        Ok(())
    }

    fn layout_directory<'a>(
        &'a self,
        directory: &mut Map<String, Value>,
        prefix: &str,
        chunks: &mut Vec<&'a [u8]>,
        next_offset: &mut usize,
    ) -> Result<()> {
        for (name, entry) in directory.iter_mut() {
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            if let Some(children) = entry.get_mut("files").and_then(Value::as_object_mut) {
                self.layout_directory(children, &path, chunks, next_offset)?;
                continue;
            }
            if entry.get("link").is_some() {
                continue;
            }
            if entry.get("unpacked").and_then(Value::as_bool).unwrap_or(false) {
                continue;
            }
            let content: &[u8] = match self.replacements.get(&path) {
                Some(replacement) => replacement,
                None => {
                    let (offset, size) = file_range(entry)?;
                    let start = self.data_offset + offset;
                    self.data
                        .get(start..start + size)
                        .ok_or_else(|| anyhow!("{path} points outside of the archive"))?
                }
            };
            let object = entry
                .as_object_mut()
                .ok_or_else(|| anyhow!("{path} is not a file entry"))?;
            if self.replacements.contains_key(&path) {
                object.insert("size".into(), json!(content.len()));
                object.insert("integrity".into(), integrity(content));
            }
            object.insert("offset".into(), json!(next_offset.to_string()));
            *next_offset += content.len();
            chunks.push(content);
        }
        Ok(())
    }

    fn entry(&self, path: &str) -> Result<&Value> {
        let mut current = &self.header;
        for part in path.split('/') {
            current = current
                .get("files")
                .and_then(|files| files.get(part))
                .ok_or_else(|| anyhow!("{path} not found in asar archive"))?;
        }
        Ok(current)
    }

    fn entry_mut(&mut self, path: &str) -> Result<&mut Value> {
        let mut current = &mut self.header;
        for part in path.split('/') {
            current = current
                .get_mut("files")
                .and_then(|files| files.get_mut(part))
                .ok_or_else(|| anyhow!("{path} not found in asar archive"))?;
        }
        Ok(current)
    }
}

fn file_range(entry: &Value) -> Result<(usize, usize)> {
    let size = entry
        .get("size")
        .and_then(Value::as_u64)
        .ok_or_else(|| anyhow!("asar entry without size"))? as usize;
    let offset = entry
        .get("offset")
        .and_then(Value::as_str)
        .and_then(|offset| offset.parse::<usize>().ok())
        .ok_or_else(|| anyhow!("asar entry without offset"))?;
    Ok((offset, size))
}

fn integrity(content: &[u8]) -> Value {
    let blocks: Vec<String> = content
        .chunks(INTEGRITY_BLOCK_SIZE)
        .map(|block| hex::encode(Sha256::digest(block)))
        .collect();
    let blocks = if blocks.is_empty() {
        vec![hex::encode(Sha256::digest([]))]
    } else {
        blocks
    };
    json!({
        "algorithm": "SHA256",
        "hash": hex::encode(Sha256::digest(content)),
        "blockSize": INTEGRITY_BLOCK_SIZE,
        "blocks": blocks,
    })
}
