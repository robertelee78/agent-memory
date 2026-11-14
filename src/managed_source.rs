//! Verification of Python-era managed Git checkout ownership.
use crate::clock::Deadline;
use crate::codec::json::Value;
use crate::error::ContractError;
use crate::fs::{Descriptor, DirectoryChain, Identity};
use crate::process::{self, Input, Spec, Status, Target};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::fmt;
use std::os::fd::AsFd;
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::OnceLock;
use std::time::Duration;

pub const MAX_FILES: usize = 40_000;
pub const MAX_BYTES: usize = 512 * 1024 * 1024;
pub const MAX_FILE: usize = 64 * 1024 * 1024;
pub const MAX_OBJECT: usize = 2 * 1024 * 1024;
pub const MAX_INDEX: usize = 64 * 1024 * 1024;
const MAX_DIAGNOSTIC: usize = 64 * 1024;

#[derive(Debug)]
pub enum Error {
    Contract(ContractError),
    PayloadMismatch(String),
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Contract(error) => error.fmt(f),
            Self::PayloadMismatch(message) => write!(f, "Legacy managed source: {message}"),
        }
    }
}
impl std::error::Error for Error {}
impl From<ContractError> for Error {
    fn from(value: ContractError) -> Self {
        Self::Contract(value)
    }
}
pub type Result<T, E = Error> = std::result::Result<T, E>;
fn contract(message: impl AsRef<str>) -> Error {
    Error::Contract(ContractError::new(format!(
        "Legacy managed source: {}",
        message.as_ref()
    )))
}
fn io(error: impl fmt::Display) -> Error {
    contract(error.to_string())
}
fn require(ok: bool, message: &str) -> Result<()> {
    if ok { Ok(()) } else { Err(contract(message)) }
}
fn payload_require(ok: bool, message: impl Into<String>) -> Result<()> {
    if ok {
        Ok(())
    } else {
        Err(Error::PayloadMismatch(message.into()))
    }
}

/// Implemented by installation, whose source_info must verify payload before declaring clean.
pub trait Installation: Send + Sync {
    fn source_info(
        &self,
        root: &Path,
        kind: &str,
        remote: &str,
        reference: &str,
    ) -> crate::error::Result<Value>;
}
static INSTALLATION: OnceLock<&'static dyn Installation> = OnceLock::new();
pub fn register_installation(
    installation: &'static dyn Installation,
) -> std::result::Result<(), &'static str> {
    INSTALLATION
        .set(installation)
        .map_err(|_| "managed source installation already registered")
}

pub fn git(root: &Path, args: &[&str], input: Option<&[u8]>, maximum: usize) -> Result<Vec<u8>> {
    let mut env: Vec<(OsString, OsString)> = std::env::vars_os()
        .filter(|(key, _)| !key.as_bytes().starts_with(b"GIT_"))
        .collect();
    env.extend(
        [
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_NO_REPLACE_OBJECTS", "1"),
            ("GIT_OPTIONAL_LOCKS", "0"),
            ("GIT_TERMINAL_PROMPT", "0"),
            ("GIT_NO_LAZY_FETCH", "1"),
        ]
        .map(|(key, value)| (key.into(), value.into())),
    );
    let mut argv: Vec<OsString> = [
        "git",
        "--no-replace-objects",
        "--literal-pathspecs",
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.untrackedCache=false",
        "-c",
        "protocol.allow=never",
        "-c",
        "core.hooksPath=/dev/null",
        "-C",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    argv.push(root.as_os_str().to_owned());
    argv.extend(args.iter().map(|arg| OsString::from(*arg)));
    // The baseline bounded-reader child sends stderr to DEVNULL, not a pipe.
    let devices = Descriptor::open_directory(Path::new("/dev")).map_err(io)?;
    let discard = devices
        .open_at(OsStr::new("null"), libc::O_WRONLY, 0)
        .map_err(io)?;
    let spec = Spec {
        argv: &argv,
        env: &env,
        cwd: None,
        search_path: true,
        stdin: input.map_or(Input::Null, Input::Bytes),
        stdout: Target::Capture { limit: maximum },
        stderr: Target::Descriptor(discard.as_fd()),
        deadline: Deadline::after(Duration::from_secs(30))?,
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    };
    let output =
        process::run_for(&spec, Duration::from_secs(30)).map_err(|error| match error.reason {
            process::Refusal::CaptureLimit {
                stream: process::Stream::Stdout,
                ..
            } => contract("Git object exceeded its bound while reading"),
            _ => contract("local Git ownership observation failed"),
        })?;
    require(
        output.status == Status::Code(0),
        "local Git ownership observation failed",
    )?;
    Ok(output.stdout)
}
fn valid_oid(oid: &str) -> bool {
    matches!(oid.len(), 40 | 64)
        && oid
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn hash_object(kind: &str, bytes: &[u8], oid_bytes: usize) -> String {
    let header = format!("{kind} {}\0", bytes.len());
    if oid_bytes == 20 {
        let mut digest = sha1::Sha1::new();
        digest.update(header.as_bytes());
        digest.update(bytes);
        crate::validation_memo::hex(&digest.finalize())
    } else {
        let mut digest = Sha256::new();
        digest.update(header.as_bytes());
        digest.update(bytes);
        crate::validation_memo::hex(&digest.finalize())
    }
}
fn object_bytes(root: &Path, kind: &str, oid: &str) -> Result<Vec<u8>> {
    require(valid_oid(oid), "Git object identity is malformed")?;
    let size = git(root, &["cat-file", "-s", oid], None, 128)?;
    let size = size.trim_ascii();
    require(
        !size.is_empty() && size.iter().all(u8::is_ascii_digit),
        "Git object exceeds its bound",
    )?;
    let size = std::str::from_utf8(size)
        .map_err(io)?
        .parse::<usize>()
        .map_err(|_| contract("Git object exceeds its bound"))?;
    require(size <= MAX_OBJECT, "Git object exceeds its bound")?;
    let raw = git(root, &["cat-file", kind, oid], None, MAX_OBJECT)?;
    require(raw.len() == size, "Git object changed while reading")?;
    require(
        hash_object(kind, &raw, oid.len() / 2) == oid,
        "Git object does not match its recorded identity",
    )?;
    Ok(raw)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub oid: String,
    pub executable: bool,
}
#[derive(Debug, Clone)]
pub struct Payload {
    pub files: BTreeMap<String, FileEntry>,
    pub directories: BTreeSet<String>,
    pub oid_bytes: usize,
}
fn walk_tree(
    root: &Path,
    oid: &str,
    prefix: &str,
    depth: usize,
    result: &mut Payload,
) -> Result<()> {
    require(depth <= 64, "tree nesting exceeds its bound")?;
    let raw = object_bytes(root, "tree", oid)?;
    let mut offset = 0;
    while offset < raw.len() {
        let space = raw
            .get(offset..)
            .ok_or_else(|| contract("malformed tree entry"))?
            .iter()
            .position(|&byte| byte == b' ')
            .map(|pos| offset + pos)
            .ok_or_else(|| contract("malformed tree entry"))?;
        let zero = raw
            .get(space + 1..)
            .ok_or_else(|| contract("malformed tree entry"))?
            .iter()
            .position(|&byte| byte == 0)
            .map(|pos| space + 1 + pos)
            .ok_or_else(|| contract("malformed tree entry"))?;
        require(
            space > offset && zero > space && zero + 1 + result.oid_bytes <= raw.len(),
            "malformed tree entry",
        )?;
        let mode = raw
            .get(offset..space)
            .ok_or_else(|| contract("malformed tree entry"))?;
        let name = std::str::from_utf8(
            raw.get(space + 1..zero)
                .ok_or_else(|| contract("malformed tree entry"))?,
        )
        .map_err(io)?;
        require(
            name != "."
                && name != ".."
                && !name.eq_ignore_ascii_case(".git")
                && !name.contains('/')
                && !name.contains('\\'),
            "unsafe tree path",
        )?;
        let path = format!("{prefix}{name}");
        require(
            path.len() <= 4096
                && !result.files.contains_key(&path)
                && !result.directories.contains(&path),
            "duplicate or oversized tree path",
        )?;
        let child = crate::validation_memo::hex(
            raw.get(zero + 1..zero + 1 + result.oid_bytes)
                .ok_or_else(|| contract("malformed tree entry"))?,
        );
        offset = zero + 1 + result.oid_bytes;
        require(
            result.files.len() + result.directories.len() < MAX_FILES,
            "tree inventory exceeds its bound",
        )?;
        if mode == b"40000" {
            result.directories.insert(path.clone());
            walk_tree(root, &child, &format!("{path}/"), depth + 1, result)?;
        } else {
            require(
                mode == b"100644" || mode == b"100755",
                "linked, special or submodule source is unsupported",
            )?;
            result.files.insert(
                path,
                FileEntry {
                    oid: child,
                    executable: mode == b"100755",
                },
            );
        }
    }
    Ok(())
}
pub fn payload(root: &Path, revision: &str) -> Result<Payload> {
    require(valid_oid(revision), "recorded commit identity is malformed")?;
    let commit = object_bytes(root, "commit", revision)?;
    let tree = commit
        .split(|&byte| byte == b'\n')
        .next()
        .unwrap_or_default();
    require(
        tree.starts_with(b"tree ") && tree.len() == 5 + revision.len(),
        "commit has no valid tree",
    )?;
    let oid = std::str::from_utf8(
        tree.get(5..)
            .ok_or_else(|| contract("commit has no valid tree"))?,
    )
    .map_err(io)?;
    let mut result = Payload {
        files: BTreeMap::new(),
        directories: BTreeSet::new(),
        oid_bytes: revision.len() / 2,
    };
    walk_tree(root, oid, "", 0, &mut result)?;
    Ok(result)
}
fn missing(directory: &Descriptor, name: &str) -> Result<bool> {
    match directory.stat_at(OsStr::new(name)) {
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(true),
        Err(error) => Err(io(error)),
    }
}
fn verify_git_directory(root: &Descriptor) -> Result<Descriptor> {
    let git = root
        .open_directory_at(OsStr::new(".git"))
        .map_err(|_| contract("managed checkout requires an ordinary Git directory"))?;
    require(
        missing(&git, "commondir")?,
        "redirected or alternate object stores are unsupported",
    )?;
    let objects = git.open_directory_at(OsStr::new("objects")).map_err(io)?;
    match objects.open_directory_at(OsStr::new("info")) {
        Ok(info) => require(
            missing(&info, "alternates")? && missing(&info, "http-alternates")?,
            "redirected or alternate object stores are unsupported",
        )?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(io(error)),
    }
    Ok(git)
}
fn blob_digest<D: Digest>(file: &mut Descriptor, size: usize) -> Result<String> {
    let mut digest = D::new();
    digest.update(format!("blob {size}\0").as_bytes());
    let mut total = 0;
    let mut block = [0; 65536];
    loop {
        let maximum = block.len().min(MAX_FILE + 1 - total);
        let chunk = block
            .get_mut(..maximum)
            .ok_or_else(|| contract("source file grew beyond its bound"))?;
        let count = file.read_chunk(chunk).map_err(io)?;
        if count == 0 {
            break;
        }
        total += count;
        require(total <= MAX_FILE, "source file grew beyond its bound")?;
        digest.update(
            chunk
                .get(..count)
                .ok_or_else(|| contract("source file changed while reading"))?,
        );
    }
    require(total == size, "source file changed while reading")?;
    Ok(crate::validation_memo::hex(&digest.finalize()))
}
pub fn blob_identity(
    oid_bytes: usize,
    parent: &Descriptor,
    name: &OsStr,
    observed: Identity,
) -> Result<String> {
    let mut file = parent
        .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)
        .map_err(io)?;
    let before = file.stat().map_err(io)?;
    require(
        before.is_regular() && before.nlink == 1 && before == observed,
        "source file changed before reading",
    )?;
    require(
        before.size >= 0 && before.size as u64 <= MAX_FILE as u64,
        "source file exceeds its bound",
    )?;
    let digest = match oid_bytes {
        20 => blob_digest::<sha1::Sha1>(&mut file, before.size as usize)?,
        32 => blob_digest::<Sha256>(&mut file, before.size as usize)?,
        _ => return Err(contract("Git object identity is malformed")),
    };
    require(
        file.stat().map_err(io)? == before && parent.stat_at(name).map_err(io)? == before,
        "source file changed while reading",
    )?;
    Ok(digest)
}
fn walk_source(
    root: &Path,
    directory: &Descriptor,
    prefix: &str,
    payload: &Payload,
    files: &mut BTreeSet<String>,
    directories: &mut BTreeSet<String>,
    total: &mut usize,
) -> Result<()> {
    let before = directory.stat().map_err(io)?;
    let entries = directory
        .entries_bounded(MAX_FILES + 1)
        .map_err(io)?
        .ok_or_else(|| contract("source inventory exceeds its bound"))?;
    for name in entries {
        if prefix.is_empty() && name == OsStr::new(".git") {
            continue;
        }
        let name_text = name
            .to_str()
            .ok_or_else(|| contract("source path is not UTF-8"))?;
        let relative = format!("{prefix}{name_text}");
        let info = directory.stat_at(&name).map_err(io)?;
        require(
            files.len() + directories.len() < MAX_FILES,
            "source inventory exceeds its bound",
        )?;
        if info.is_directory() {
            payload_require(
                payload.directories.contains(&relative),
                format!("extra source directory: {relative}"),
            )?;
            let child = directory.open_directory_at(&name).map_err(io)?;
            require(
                child.stat().map_err(io)? == info,
                "source directory changed before reading",
            )?;
            directories.insert(relative.clone());
            walk_source(
                root,
                &child,
                &format!("{relative}/"),
                payload,
                files,
                directories,
                total,
            )?;
            require(
                directory.stat_at(&name).map_err(io)? == info && child.stat().map_err(io)? == info,
                "source directory changed while reading",
            )?;
        } else {
            let expected = payload.files.get(&relative);
            payload_require(
                expected.is_some() && info.is_regular() && info.nlink == 1,
                format!("extra, linked or special source file: {relative}"),
            )?;
            require(
                info.size >= 0 && info.size as u64 <= MAX_FILE as u64,
                "source file exceeds its bound",
            )?;
            *total = total
                .checked_add(info.size as usize)
                .ok_or_else(|| contract("source bytes exceed their bound"))?;
            require(*total <= MAX_BYTES, "source bytes exceed their bound")?;
            let oid = blob_identity(payload.oid_bytes, directory, &name, info)?;
            let expected = expected.ok_or_else(|| contract("source entries are missing"))?;
            payload_require(
                oid == expected.oid && (info.mode & 0o100 != 0) == expected.executable,
                format!("source bytes or executable intent differ: {relative}"),
            )?;
            files.insert(relative);
        }
    }
    require(
        directory.stat().map_err(io)? == before,
        "source directory changed while reading",
    )
}
pub fn verify_index(root: &Path, files: &BTreeMap<String, FileEntry>) -> Result<()> {
    let raw = git(
        root,
        &["ls-files", "--stage", "--full-name", "-z"],
        None,
        MAX_INDEX,
    )?;
    let mut seen = BTreeSet::new();
    for row in raw.split(|&byte| byte == 0).filter(|row| !row.is_empty()) {
        let tab = row
            .iter()
            .position(|&byte| byte == b'\t')
            .ok_or_else(|| contract("malformed Git index entry"))?;
        let header = std::str::from_utf8(
            row.get(..tab)
                .ok_or_else(|| contract("malformed Git index entry"))?,
        )
        .map_err(io)?;
        let mut fields = header.split(' ');
        let mode = fields.next().unwrap_or("");
        let oid = fields.next().unwrap_or("");
        let stage = fields.next().unwrap_or("");
        require(fields.next().is_none(), "malformed Git index entry")?;
        let path = std::str::from_utf8(
            row.get(tab + 1..)
                .ok_or_else(|| contract("malformed Git index entry"))?,
        )
        .map_err(io)?;
        let expected = files.get(path);
        payload_require(
            expected.is_some() && !seen.contains(path) && seen.len() < MAX_FILES,
            "index contains unexpected staged paths",
        )?;
        let expected =
            expected.ok_or_else(|| contract("index contains unexpected staged paths"))?;
        payload_require(
            stage == "0"
                && oid == expected.oid
                && mode
                    == if expected.executable {
                        "100755"
                    } else {
                        "100644"
                    },
            "index contains staged source changes",
        )?;
        seen.insert(path.to_owned());
    }
    payload_require(
        seen.len() == files.len(),
        "index contains staged source removals",
    )
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PayloadVerification {
    pub qualification: &'static str,
    pub files: usize,
    pub bytes: usize,
    pub revision: String,
}
pub fn verify_payload(root: &Path, revision: &str) -> Result<PayloadVerification> {
    let chain = DirectoryChain::open(root).map_err(io)?;
    let directory = chain.directory();
    let git = verify_git_directory(directory)?;
    let git_identity = git.stat().map_err(io)?;
    let payload = payload(root, revision)?;
    let mut files = BTreeSet::new();
    let mut directories = BTreeSet::new();
    let mut bytes = 0;
    walk_source(
        root,
        directory,
        "",
        &payload,
        &mut files,
        &mut directories,
        &mut bytes,
    )?;
    payload_require(
        files.len() == payload.files.len() && directories == payload.directories,
        "source entries are missing",
    )?;
    verify_index(root, &payload.files)?;
    chain.recheck().map_err(io)?;
    require(
        directory.stat_at(OsStr::new(".git")).map_err(io)? == git_identity
            && git.stat().map_err(io)? == git_identity,
        "Git directory changed while reading",
    )?;
    verify_git_directory(directory)?;
    Ok(PayloadVerification {
        qualification: "legacy-git-payload-equivalent",
        files: files.len(),
        bytes,
        revision: revision.to_owned(),
    })
}
fn field<'a>(value: &'a Value, name: &str) -> Option<&'a str> {
    value.get(name).and_then(Value::as_str)
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verification {
    pub qualification: &'static str,
    pub revision: String,
}
pub fn verify(root: &Path, source: &Value) -> Result<Verification> {
    let path = root
        .to_str()
        .ok_or_else(|| contract("receipt does not own this legacy checkout"))?;
    require(
        field(source, "kind") == Some("managed") && field(source, "path") == Some(path),
        "receipt does not own this legacy checkout",
    )?;
    let remote = field(source, "remote").ok_or_else(|| contract("recorded channel differs"))?;
    let reference = field(source, "ref").ok_or_else(|| contract("recorded channel differs"))?;
    let revision = field(source, "revision").ok_or_else(|| {
        contract("revision, artifact, version or cleanliness differs from receipt")
    })?;
    crate::fs::trust::source(root)?;
    let actual_remote = git(root, &["remote", "get-url", "origin"], None, MAX_DIAGNOSTIC)?;
    let actual_ref = git(
        root,
        &["symbolic-ref", "--short", "HEAD"],
        None,
        MAX_DIAGNOSTIC,
    )?;
    require(
        std::str::from_utf8(&actual_remote).map_err(io)?.trim() == remote
            && std::str::from_utf8(&actual_ref).map_err(io)?.trim() == reference,
        "recorded channel differs",
    )?;
    let installation = INSTALLATION
        .get()
        .ok_or_else(|| contract("installation integration is not registered"))?;
    let observed = installation.source_info(root, "managed", remote, reference)?;
    require(
        crate::client_hooks::membership_equal(&observed, source),
        "revision, artifact, version or cleanliness differs from receipt",
    )?;
    Ok(Verification {
        qualification: "legacy-git-payload-equivalent",
        revision: revision.to_owned(),
    })
}
