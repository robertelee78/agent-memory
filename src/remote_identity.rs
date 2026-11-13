//! Sanitized repository identifiers, never URLs to fetch or push.
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use std::collections::VecDeque;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

// Generated from tests/vectors/nfkc-netloc-delimiters-v1.json, Unicode 15.1.
pub const NFKC_NETLOC_DELIMITERS: [u32; 19] = [
    0x2047, 0x2048, 0x2049, 0x2100, 0x2101, 0x2105, 0x2106, 0x2a74,
    0xfe13, 0xfe16, 0xfe55, 0xfe56, 0xfe5f, 0xfe6b, 0xff03, 0xff0f, 0xff1a, 0xff1f, 0xff20,
];
fn ambiguous() -> ContractError {
    ContractError::new("Unsupported or ambiguous repository remote; choose an explicit canonical binding")
}

/// Path.resolve(strict=False) identity derivation, not a trusted filesystem read.
/// Resolve each existing symlink before processing following `..`, including after
/// missing components. A finite link budget refuses loops without changing disk.
fn resolve(path: &Path) -> Result<PathBuf> {
    let absolute = if path.is_absolute() { path.to_path_buf() }
        else { std::env::current_dir().map_err(|_| ambiguous())?.join(path) };
    let mut pending: VecDeque<OsString> = absolute.components().filter_map(|c| match c {
        Component::Normal(s) => Some(s.to_owned()), Component::ParentDir => Some(OsString::from("..")),
        _ => None,
    }).collect();
    let mut resolved = PathBuf::from("/"); let mut links = 0usize;
    while let Some(part) = pending.pop_front() {
        if part.is_empty() || part == "." { continue; }
        if part == ".." { resolved.pop(); continue; }
        let candidate = resolved.join(&part);
        match std::fs::symlink_metadata(&candidate) {
            Ok(meta) if meta.file_type().is_symlink() => {
                links += 1; if links > 4096 { return Err(ambiguous()); }
                let target = std::fs::read_link(&candidate).map_err(|_| ambiguous())?;
                if target.is_absolute() { resolved = PathBuf::from("/"); }
                // Keep native OS bytes in symlink targets.
                let mut components: Vec<OsString> = target.components().map(|c| match c {
                    Component::RootDir => OsString::new(), Component::CurDir => OsString::from("."),
                    Component::ParentDir => OsString::from(".."), Component::Normal(s) => s.to_owned(),
                    Component::Prefix(p) => p.as_os_str().to_owned(),
                }).collect();
                while let Some(component) = components.pop() { pending.push_front(component); }
            }
            // CPython's non-strict realpath retains components on all lstat errors.
            _ => { resolved.push(part); }
        }
    }
    Ok(resolved)
}
fn local(path: &Path) -> Result<String> {
    let resolved = resolve(path)?;
    let path = resolved.to_str().ok_or_else(ambiguous)?.trim_end_matches('/');
    if path.chars().any(|c| c < ' ' || c == '\u{7f}') { return Err(ambiguous()); }
    let path = path.strip_suffix(".git").unwrap_or(path);
    if path.is_empty() { return Err(ambiguous()); }
    Ok(format!("file://{path}"))
}
fn helper(value: &str) -> bool {
    let Some((scheme, _)) = value.split_once("::") else { return false; };
    let mut bytes = scheme.bytes();
    bytes.next().is_some_and(|c| c.is_ascii_alphabetic())
        && bytes.all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-'))
}
fn scp(value: &str) -> Option<(&str, &str)> {
    let hostpart = value.split_once('@').filter(|(user, _)| !user.is_empty()
        && !user.contains(['/', ':'])).map_or(value, |(_, rest)| rest);
    if let Some(bracketed) = hostpart.strip_prefix('[') {
        if let Some((host, path)) = bracketed.split_once("]:") {
            if !host.is_empty() && !path.is_empty() && !host.contains(']') {
                return Some((&hostpart[..host.len() + 2], path));
            }
        }
    }
    let (host, path) = hostpart.split_once(':')?;
    if host.is_empty() || path.is_empty() || host.contains('/') { None } else { Some((host, path)) }
}
fn unquote(path: &str) -> String {
    let mut bytes = Vec::with_capacity(path.len()); let input = path.as_bytes(); let mut i = 0;
    while i < input.len() {
        if input[i] == b'%' && i + 2 < input.len() {
            if let (Some(a), Some(b)) = (digit(input[i + 1]), digit(input[i + 2])) {
                bytes.push(a * 16 + b); i += 3; continue;
            }
        }
        bytes.push(input[i]); i += 1;
    }
    String::from_utf8_lossy(&bytes).into_owned()
}
fn digit(c: u8) -> Option<u8> {
    match c { b'0'..=b'9' => Some(c - b'0'), b'a'..=b'f' => Some(c - b'a' + 10), b'A'..=b'F' => Some(c - b'A' + 10), _ => None }
}
fn remote_path(path: &str) -> Result<String> {
    let input = path.trim_matches('/').as_bytes(); let mut result = Vec::with_capacity(input.len()); let mut i = 0;
    while i < input.len() {
        if input[i] != b'%' { result.push(input[i]); i += 1; continue; }
        let a = input.get(i + 1).and_then(|b| digit(*b)).ok_or_else(ambiguous)?;
        let b = input.get(i + 2).and_then(|b| digit(*b)).ok_or_else(ambiguous)?;
        let c = a * 16 + b;
        if c < 32 || matches!(c, b'/' | b'\\') { return Err(ambiguous()); }
        if c.is_ascii_alphanumeric() || matches!(c, b'.' | b'_' | b'~' | b'-') { result.push(c); }
        else { const HEX: &[u8; 16] = b"0123456789ABCDEF"; result.extend_from_slice(&[b'%', HEX[a as usize], HEX[b as usize]]); }
        i += 3;
    }
    String::from_utf8(result).map_err(|_| ambiguous())
}
fn check_netloc(netloc: &str) -> Result<()> {
    // Mirror urllib._checknetloc: remove existing ASCII delimiters first.
    let remaining = netloc.chars().filter(|c| !matches!(c, '@' | ':' | '#' | '?'));
    if remaining.clone().any(|c| NFKC_NETLOC_DELIMITERS.contains(&(c as u32))) { return Err(ambiguous()); }
    Ok(())
}
fn host_port(netloc: &str) -> Result<(String, Option<u16>)> {
    let hostinfo = netloc.rsplit('@').next().unwrap();
    let has_open = netloc.contains('['); let has_close = netloc.contains(']');
    if has_open != has_close { return Err(ambiguous()); }
    let (host, port) = if has_open {
        let (before, rest) = hostinfo.split_once('[').ok_or_else(ambiguous)?;
        if !before.is_empty() { return Err(ambiguous()); }
        let (host, rest) = rest.split_once(']').ok_or_else(ambiguous)?;
        if !rest.is_empty() && !rest.starts_with(':') { return Err(ambiguous()); }
        if let Some(future) = host.strip_prefix('v') {
            let (version, address) = future.split_once('.').ok_or_else(ambiguous)?;
            if version.is_empty() || !version.bytes().all(|b| b.is_ascii_hexdigit()) || address.is_empty() { return Err(ambiguous()); }
        } else {
            let mut scoped = host.split('%'); let ip = scoped.next().unwrap();
            if ip.parse::<std::net::Ipv6Addr>().is_err() { return Err(ambiguous()); }
            if let Some(zone) = scoped.next() { if zone.is_empty() || scoped.next().is_some() { return Err(ambiguous()); } }
        }
        (host, rest.strip_prefix(':').unwrap_or(""))
    } else { hostinfo.split_once(':').unwrap_or((hostinfo, "")) };
    check_netloc(netloc)?;
    if host.is_empty() { return Err(ambiguous()); }
    let port = if port.is_empty() { None } else {
        if !port.bytes().all(|b| b.is_ascii_digit()) { return Err(ambiguous()); }
        // Leading zeros have no bound in Python's int conversion apart from its
        // decimal conversion limit; the whole remote is already <=4096 chars.
        let trimmed = port.trim_start_matches('0');
        Some(if trimmed.is_empty() { 0 } else { trimmed.parse().map_err(|_| ambiguous())? })
    };
    Ok((host.to_lowercase(), port))
}

pub fn normalize(value: &Value, base: Option<&Path>) -> Result<String> {
    let value = value.as_str().filter(|s| !s.is_empty() && s.chars().count() <= 4096
        && !s.chars().any(|c| c < ' ') && !s.contains('\\'))
        .ok_or_else(|| ContractError::new("Invalid repository remote; inspect configuration privately"))?;
    if helper(value) { return Err(ContractError::new("External remote helpers require an explicit supported repository binding")); }
    let converted;
    let value = if !value.contains("://") {
        if let Some((host, path)) = scp(value) {
            converted = format!("ssh://{host}/{}", path.trim_start_matches('/')); &converted
        } else {
            let cwd; let base = match base { Some(base) if !base.as_os_str().is_empty() => base, _ => {
                cwd = std::env::current_dir().map_err(|_| ambiguous())?; &cwd
            }};
            return local(&base.join(value));
        }
    } else { value };
    let value = value.trim_start_matches(' ');
    let (scheme, rest) = value.split_once(':').ok_or_else(ambiguous)?;
    if !scheme.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
        || !scheme.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.')) { return Err(ambiguous()); }
    let scheme = scheme.to_ascii_lowercase();
    let (netloc, rest) = if let Some(rest) = rest.strip_prefix("//") {
        let end = rest.find(['/', '?', '#']).unwrap_or(rest.len()); (&rest[..end], &rest[end..])
    } else { ("", rest) };
    let path = rest.split(['?', '#']).next().unwrap();
    // urlsplit validates brackets/NFKC even for file URLs before scheme handling.
    if netloc.contains(['[', ']']) { host_port(netloc)?; } else { check_netloc(netloc)?; }
    if scheme == "file" {
        if !matches!(netloc, "" | "localhost") || !path.starts_with('/') { return Err(ambiguous()); }
        return local(Path::new(&unquote(path)));
    }
    if !matches!(scheme.as_str(), "https" | "http" | "ssh" | "git" | "repo") { return Err(ambiguous()); }
    let (host, mut port) = host_port(netloc)?;
    let default = match scheme.as_str() { "https" => Some(443), "http" => Some(80), "ssh" => Some(22), "git" => Some(9418), _ => None };
    if scheme != "repo" && port == default { port = None; }
    let authority = if host.contains(':') { format!("[{host}]") } else { host.clone() };
    let authority = match port { Some(port) => format!("{authority}:{port}"), None => authority };
    let mut path = remote_path(path)?;
    if host == "github.com" { path = path.to_lowercase(); }
    let path = path.strip_suffix(".git").unwrap_or(&path);
    if path.is_empty() || path.split('/').any(|p| matches!(p, "" | "." | ".."))
        || path.chars().any(|c| c.is_whitespace() || ('\u{1c}'..='\u{1f}').contains(&c)) { return Err(ambiguous()); }
    Ok(format!("repo://{authority}/{path}"))
}
