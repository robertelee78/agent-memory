use super::descriptor::{Descriptor, DirectoryChain, Identity, contract_io};
use crate::codec::json::{Value, SORTED_COMPACT_UTF8, encode};
use crate::error::{ContractError, Result};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io;
use std::os::fd::AsRawFd;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fingerprint {
    Link { target: String },
    File { hash: String, mode: u32 },
    Directory { hash: String, mode: u32 },
}
impl Fingerprint {
    pub fn json_value(&self) -> Value {
        match self {
            Self::Link { target } => object(vec![("kind", Value::String("link".into())), ("target", Value::String(target.clone()))]),
            Self::File { hash, mode } => object(vec![("kind", Value::String("file".into())), ("hash", Value::String(hash.clone())), ("mode", Value::Int((*mode).into()))]),
            Self::Directory { hash, mode } => object(vec![("kind", Value::String("directory".into())), ("hash", Value::String(hash.clone())), ("mode", Value::Int((*mode).into()))]),
        }
    }
}

fn object(fields: Vec<(&str, Value)>) -> Value { Value::Object(fields.into_iter().map(|(name, value)| (name.to_owned(), value)).collect()) }
fn digest_hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut value = String::with_capacity(bytes.len() * 2);
    for byte in bytes { value.push(DIGITS[(byte >> 4) as usize] as char); value.push(DIGITS[(byte & 15) as usize] as char); }
    value
}
fn changed() -> ContractError { ContractError::new("Managed node changed during fingerprint observation") }

fn file_hash(directory: &Descriptor, name: &OsStr, before: Identity, path: &Path) -> Result<String> {
    if before.size < 0 || before.size > 64 * 1024 * 1024 { return Err(ContractError::new(format!("Oversized managed file: {}", path.display()))); }
    let file = directory.open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0).map_err(contract_io)?;
    if file.stat().map_err(contract_io)? != before { return Err(changed()); }
    let mut hasher = Sha256::new();
    let mut block = [0u8; 8192];
    let mut size = 0u64;
    loop {
        let remaining = 64 * 1024 * 1024 + 1 - size;
        let count = block.len().min(remaining as usize);
        let read = unsafe { libc::read(file.as_raw_fd(), block.as_mut_ptr().cast(), count) };
        if read == -1 {
            let error = io::Error::last_os_error();
            if error.kind() == io::ErrorKind::Interrupted { continue; }
            return Err(contract_io(error));
        }
        if read == 0 { break; }
        size += read as u64;
        if size > 64 * 1024 * 1024 { return Err(ContractError::new(format!("Oversized managed file: {}", path.display()))); }
        hasher.update(&block[..read as usize]);
    }
    if size != before.size as u64 || file.stat().map_err(contract_io)? != before || directory.stat_at(name).map_err(contract_io)? != before { return Err(changed()); }
    Ok(digest_hex(&hasher.finalize()))
}

fn leaf(directory: &Descriptor, name: &OsStr, info: Identity, path: &Path) -> Result<Fingerprint> {
    let result = if info.is_symlink() {
        let target = directory.read_link_at(name).map_err(contract_io)?.into_os_string().into_string()
            .map_err(|_| ContractError::new("Managed symlink target is not UTF-8"))?;
        Fingerprint::Link { target }
    } else if info.is_regular() {
        Fingerprint::File { hash: file_hash(directory, name, info, path)?, mode: info.mode & 0o7777 }
    } else { return Err(ContractError::new(format!("Unsupported filesystem node: {}", path.display()))); };
    if directory.stat_at(name).map_err(contract_io)? != info { return Err(changed()); }
    Ok(result)
}

pub fn fingerprint(path: &Path) -> Result<Option<Fingerprint>> {
    let parent = path.parent().ok_or_else(|| ContractError::new("Managed node requires a named path"))?;
    let name = path.file_name().ok_or_else(|| ContractError::new("Managed node requires a named path"))?;
    let parents = match DirectoryChain::open(parent) {
        Ok(parents) => parents,
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => return Ok(None),
        Err(error) => return Err(contract_io(error)),
    };
    let before = match parents.directory().stat_at(name) {
        Ok(before) => before,
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => return Ok(None),
        Err(error) => return Err(contract_io(error)),
    };
    let result = if before.is_directory() {
        let directory = parents.directory().open_directory_at(name).map_err(contract_io)?;
        if directory.stat().map_err(contract_io)? != before { return Err(changed()); }
        let mut entries = BTreeMap::new();
        let mut size = 0u64;
        fn walk(directory: &Descriptor, path: &Path, relative: &str, entries: &mut BTreeMap<String, Value>, size: &mut u64) -> Result<()> {
            let before = directory.stat().map_err(contract_io)?;
            for name in directory.entries().map_err(contract_io)? {
                let name_text = name.to_str().ok_or_else(|| ContractError::new("Managed path is not UTF-8"))?;
                let relative = if relative.is_empty() { name_text.to_owned() } else { format!("{relative}/{name_text}") };
                let path = path.join(&name);
                let info = directory.stat_at(&name).map_err(contract_io)?;
                if entries.len() >= 40000 { return Err(ContractError::new("Managed directory inventory exceeds its bound")); }
                if info.is_directory() {
                    entries.insert(relative.clone(), object(vec![("kind", Value::String("directory".into())), ("mode", Value::Int((info.mode & 0o7777).into()))]));
                    let child = directory.open_directory_at(&name).map_err(contract_io)?;
                    if child.stat().map_err(contract_io)? != info { return Err(changed()); }
                    walk(&child, &path, &relative, entries, size)?;
                } else {
                    if info.is_regular() {
                        *size = size.checked_add(info.size.try_into().map_err(|_| changed())?).ok_or_else(|| ContractError::new("Managed directory inventory exceeds its bound"))?;
                        if *size > 512 * 1024 * 1024 { return Err(ContractError::new("Managed directory inventory exceeds its bound")); }
                    }
                    entries.insert(relative, leaf(directory, &name, info, &path)?.json_value());
                }
                if directory.stat_at(&name).map_err(contract_io)? != info { return Err(changed()); }
            }
            if directory.stat().map_err(contract_io)? != before { return Err(changed()); }
            Ok(())
        }
        walk(&directory, path, "", &mut entries, &mut size)?;
        let encoded = encode(&Value::Object(entries.into_iter().collect()), SORTED_COMPACT_UTF8)?;
        Fingerprint::Directory { hash: digest_hex(&Sha256::digest(encoded.as_bytes())), mode: before.mode & 0o7777 }
    } else { leaf(parents.directory(), name, before, path)? };
    if parents.directory().stat_at(name).map_err(contract_io)? != before { return Err(changed()); }
    parents.recheck().map_err(contract_io)?;
    Ok(Some(result))
}
