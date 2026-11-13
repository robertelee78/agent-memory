use super::descriptor::{Descriptor, DirectoryChain, Identity, contract_io, read_source};
use crate::error::{ContractError, Kind, Result};
use std::io;
use std::path::{Component, Path, PathBuf};

fn trust_result<T>(path: &Path, result: Result<T>) -> Result<T> {
    result.map_err(|error| {
        if matches!(error.kind(), Kind::TrustPath { .. }) { error }
        else { ContractError::trust_path(path, error.message()) }
    })
}

#[derive(Debug, Default, Clone, Copy)]
pub struct InspectOptions {
    pub private_directory: bool,
    pub sensitive_record: bool,
    pub allow_final_link: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Inspection {
    pub path: PathBuf,
    pub present: bool,
    pub unix_path_policy_verified: bool,
    pub acl_audited: bool,
    pub executing_verifier_authenticated: bool,
    pub writes_performed: bool,
}

pub fn refuse<T>(path: &Path, reason: &str) -> Result<T> { Err(ContractError::trust_path(path, reason)) }

pub fn inspect_entry(path: &Path, value: &Identity, mut protected: bool, final_entry: bool, allow_final_link: bool) -> Result<(bool, bool)> {
    let uid = unsafe { libc::geteuid() };
    if value.uid != uid && value.uid != 0 {
        return refuse(path, "owner is neither the executing user nor root");
    }
    if value.is_symlink() {
        if !final_entry || !allow_final_link {
            return refuse(path, "a trusted ordinary path must not be redirected by a symlink");
        }
        return Ok((protected, true));
    }
    if !value.is_directory() && !(final_entry && value.is_regular()) {
        return refuse(path, "an ordinary directory or regular file is required");
    }
    if !protected && value.mode & 0o022 != 0
        && !(value.is_directory() && value.uid == 0 && value.mode & libc::S_ISVTX as u32 != 0) {
        return refuse(path, "group/other write access permits an untrusted principal to change it");
    }
    if value.is_regular() && value.nlink != 1 && value.mode & 0o022 != 0 {
        return refuse(path, "a writable payload has aliases outside its checked path");
    }
    if value.is_directory() && value.uid == uid && value.mode & 0o077 == 0 { protected = true; }
    Ok((protected, false))
}

pub fn inspect(path: &Path, options: InspectOptions) -> Result<Inspection> {
    trust_result(path, (|| {
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return refuse(path, "an absolute canonical path is required");
    }
    let normalized: PathBuf = path.components().collect();
    let mut node = PathBuf::from("/");
    let root = Descriptor::open_directory(Path::new("/")).map_err(contract_io)?;
    let mut protected = false;
    let root_info = root.stat().map_err(contract_io)?;
    protected = inspect_entry(&node, &root_info, protected, node == normalized, options.allow_final_link)?.0;
    let mut leaf = if node == normalized { Some(root_info) } else { None };
    let mut parents = vec![root];
    let mut observations = Vec::new();
    for part in normalized.components() {
        let Component::Normal(name) = part else { continue; };
        node.push(name);
        let parent = parents.last().expect("trust walk includes root");
        let value = match parent.stat_at(name) {
            Ok(info) => info,
            Err(error) if error.raw_os_error() == Some(libc::ENOENT) => break,
            Err(error) => return Err(contract_io(error)),
        };
        let final_entry = node == normalized;
        let (next_protected, link) = inspect_entry(&node, &value, protected, final_entry, options.allow_final_link)?;
        protected = next_protected;
        observations.push((parents.len() - 1, name.to_os_string(), value));
        if final_entry { leaf = Some(value); break; }
        if link { break; }
        let child = parent.open_directory_at(name).map_err(contract_io)?;
        if !child.stat().map_err(contract_io)?.same_binding(value) {
            return refuse(&node, "path changed during trust inspection");
        }
        parents.push(child);
    }
    for (index, name, value) in observations {
        if !parents[index].stat_at(&name).map_err(contract_io)?.same_binding(value) {
            return refuse(&normalized, "path changed during trust inspection");
        }
        if let Some(opened) = parents.get(index + 1) {
            if !opened.stat().map_err(contract_io)?.same_binding(value) {
                return refuse(&normalized, "path changed during trust inspection");
            }
        }
    }
    if let Some(value) = leaf {
        if options.private_directory && (!value.is_directory() || value.uid != unsafe { libc::geteuid() } || value.mode & 0o077 != 0) {
            return refuse(&normalized, "installation state must be a private current-user-owned directory (0700)");
        }
        if options.sensitive_record && (!value.is_regular() || value.uid != unsafe { libc::geteuid() } || value.mode & 0o022 != 0 || value.nlink != 1) {
            return refuse(&normalized, "lifecycle records must be current-user-owned, regular, single-link and not group/other writable");
        }
    }
    Ok(Inspection { path: normalized, present: leaf.is_some(), unix_path_policy_verified: true,
        acl_audited: false, executing_verifier_authenticated: false, writes_performed: false })
    })())
}

pub fn state(path: &Path) -> Result<Inspection> {
    inspect(path, InspectOptions { private_directory: true, ..InspectOptions::default() })
}
pub fn record(path: &Path) -> Result<Inspection> {
    inspect(path, InspectOptions { sensitive_record: true, ..InspectOptions::default() })
}

pub(crate) fn named_stat(path: &Path) -> io::Result<Identity> {
    if path == Path::new("/") { return Descriptor::open_directory(path)?.stat(); }
    let parent = path.parent().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Path has no parent"))?;
    let name = path.file_name().ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "Path has no filename"))?;
    let parents = DirectoryChain::open(parent)?;
    let value = parents.directory().stat_at(name)?;
    parents.recheck()?;
    Ok(value)
}

fn optional_stat(path: &Path) -> Result<Option<Identity>> {
    match named_stat(path) {
        Ok(info) => Ok(Some(info)),
        Err(error) if error.raw_os_error() == Some(libc::ENOENT) => Ok(None),
        Err(error) => Err(contract_io(error)),
    }
}

pub fn tree(path: &Path, git_metadata: bool) -> Result<()> {
    trust_result(path, (|| {
    let inspection = inspect(path, InspectOptions { allow_final_link: true, ..InspectOptions::default() })?;
    if !inspection.present { return Ok(()); }
    let info = named_stat(path).map_err(contract_io)?;
    if !info.is_directory() { return Ok(()); }
    let chain = DirectoryChain::open(path).map_err(contract_io)?;
    fn visit(directory: &Descriptor, path: &Path, git_metadata: bool) -> Result<()> {
        let before = directory.stat().map_err(contract_io)?;
        for name in directory.entries().map_err(contract_io)? {
            let node = path.join(&name);
            let git_metadata = git_metadata || name == ".git";
            inspect(&node, InspectOptions { allow_final_link: !git_metadata, ..InspectOptions::default() })?;
            let info = directory.stat_at(&name).map_err(contract_io)?;
            if info.is_directory() {
                let child = directory.open_directory_at(&name).map_err(contract_io)?;
                if !child.stat().map_err(contract_io)?.same_binding(info) { return refuse(&node, "path changed during trust inspection"); }
                visit(&child, &node, git_metadata)?;
            }
            if !directory.stat_at(&name).map_err(contract_io)?.same_binding(info) { return refuse(&node, "path changed during trust inspection"); }
        }
        if !before.same_binding(directory.stat().map_err(contract_io)?) { return refuse(path, "path changed during trust inspection"); }
        Ok(())
    }
    visit(chain.directory(), path, git_metadata)?;
    chain.recheck().map_err(contract_io)
    })())
}

fn pointer_text(path: &Path, bound_reason: &str) -> Result<String> {
    let info = named_stat(path).map_err(contract_io)?;
    if info.size > 4096 { return refuse(path, bound_reason); }
    let observed = read_source(path.parent().expect("pointer has a parent"), Path::new(path.file_name().expect("pointer has a name")), 4096)?;
    let text = String::from_utf8(observed.bytes).map_err(|_| ContractError::new("Git directory pointer is not UTF-8"))?;
    // Python text mode translates CRLF and bare CR before Git's trailing CR/LF stripping.
    Ok(text.replace("\r\n", "\n").replace('\r', "\n").trim_end_matches(['\r', '\n']).to_owned())
}

pub fn source(path: &Path) -> Result<()> {
    trust_result(path, (|| {
    inspect(path, InspectOptions::default())?;
    tree(path, false)?;
    let marker = path.join(".git");
    let directory = match optional_stat(&marker)? {
        Some(info) if info.is_regular() => {
            inspect(&marker, InspectOptions::default())?;
            let text = pointer_text(&marker, "Git directory pointer exceeds its bound")?;
            let Some(text) = text.strip_prefix("gitdir: ") else { return refuse(&marker, "unrecognized Git directory pointer"); };
            git_pointer(path, text)?
        }
        Some(info) if info.is_directory() => marker,
        _ => {
            if optional_stat(&path.join("HEAD"))?.is_some_and(Identity::is_regular)
                && optional_stat(&path.join("objects"))?.is_some_and(Identity::is_directory) { path.to_path_buf() }
            else { return Ok(()); }
        }
    };
    tree(&directory, true)?;
    let common = directory.join("commondir");
    if optional_stat(&common)?.is_some() {
        inspect(&common, InspectOptions::default())?;
        let text = pointer_text(&common, "Git common-directory pointer exceeds its bound")?;
        tree(&git_pointer(&directory, &text)?, true)?;
    }
    Ok(())
    })())
}

pub fn git_pointer(base: &Path, text: &str) -> Result<PathBuf> {
    trust_result(base, (|| {
    if text.is_empty() || text.chars().any(|value| value < ' ' || value == '\u{7f}') {
        return refuse(base, "unsupported empty/control-containing Git directory pointer");
    }
    let supplied = Path::new(text);
    let mut node = if supplied.is_absolute() { PathBuf::from("/") } else { base.to_path_buf() };
    for part in supplied.components() {
        match part {
            Component::RootDir | Component::CurDir => continue,
            Component::ParentDir => { inspect(&node, InspectOptions::default())?; node.pop(); }
            Component::Normal(name) => node.push(name),
            Component::Prefix(_) => return refuse(base, "unsupported empty/control-containing Git directory pointer"),
        }
        #[cfg(target_os = "macos")]
        {
            let alias = match node.to_str() {
                Some("/tmp") => Some(Path::new("/private/tmp")),
                Some("/var") => Some(Path::new("/private/var")),
                Some("/etc") => Some(Path::new("/private/etc")),
                _ => None,
            };
            if let Some(alias) = alias {
                if let Some(info) = optional_stat(&node)? {
                    if info.is_symlink() {
                        let parent = DirectoryChain::open(node.parent().expect("platform alias has a parent")).map_err(contract_io)?;
                        let target = parent.directory().read_link_at(node.file_name().expect("platform alias has a name")).map_err(contract_io)?;
                        let raw = node.parent().expect("platform alias has a parent").join(target);
                        let mut destination = PathBuf::new();
                        for part in raw.components() {
                            match part { Component::ParentDir => { destination.pop(); }, Component::CurDir => {}, part => destination.push(part.as_os_str()) }
                        }
                        if info.uid != 0 || destination != alias {
                            return refuse(&node, "an unexpected platform alias cannot locate trusted Git metadata");
                        }
                        node = destination;
                    }
                }
            }
        }
        inspect(&node, InspectOptions::default())?;
    }
    if !optional_stat(&node)?.is_some_and(Identity::is_directory) { return refuse(&node, "Git metadata directory is unavailable"); }
    Ok(node)
    })())
}
