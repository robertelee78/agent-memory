//! Signed release-v2 acquisition. Unverified candidates are data, never executables.
use crate::codec::json::{self, DecodePolicy, DuplicateKeys, NonFinite, Value};
use crate::error::ContractError;
use crate::fs::{Descriptor, DirectoryChain};
use crate::lifecycle::{object, string, text};
use crate::process::{self, Input, Spec, Status, Target};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Debug)]
pub enum Error {
    Contract(ContractError),
    Io(std::io::Error),
    Process(process::ProcessError),
    Download(Box<DownloadError>),
    MissingProvider(&'static str),
}
impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Contract(e) => e.fmt(f),
            Self::Io(e) => e.fmt(f),
            Self::Process(e) => e.fmt(f),
            Self::Download(e) => e.fmt(f),
            Self::MissingProvider(name) => {
                write!(f, "Release build provider is not registered: {name}")
            }
        }
    }
}
impl std::error::Error for Error {}
impl From<ContractError> for Error {
    fn from(e: ContractError) -> Self {
        Self::Contract(e)
    }
}
impl From<std::io::Error> for Error {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<process::ProcessError> for Error {
    fn from(e: process::ProcessError) -> Self {
        Self::Process(e)
    }
}
impl From<crate::pack_validation::Invalid> for Error {
    fn from(e: crate::pack_validation::Invalid) -> Self {
        Self::Contract(e.into())
    }
}
impl From<DownloadError> for Error {
    fn from(e: DownloadError) -> Self {
        Self::Download(Box::new(e))
    }
}
pub type Result<T, E = Error> = std::result::Result<T, E>;

pub const REPOSITORY: &str = "robertelee78/agent-memory";
pub const ASSETS: &str = "https://github.com/robertelee78/agent-memory/releases/download";
pub const PRINCIPAL: &str = "release@agent-memory";
pub const NAMESPACE: &str = "agent-memory.release";
pub const BINARY_NAMESPACE: &str = "agent-memory.release-binary";
pub const ALLOWED_SIGNERS: &str = include_str!("../skills/agent-memory/keys/allowed-signers");
pub const ARCHIVE: &str = "agent-memory.tar.gz";
pub const MANIFEST: &str = "release-manifest.json";
pub const SIGNATURE: &str = "release-manifest.json.sshsig";
pub const PREFIX: &str = "skills/agent-memory/";
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_FILE: usize = 2 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 4096;
pub const MAX_ARCHIVE: usize = MAX_BYTES + 1024 * 1024;
pub const MAX_TAR: usize = MAX_BYTES + MAX_ENTRIES * 1024;
pub const MAX_METADATA: usize = 65536;
pub const MAX_BINARY: usize = 64 * 1024 * 1024;
pub const TARGETS: [&str; 4] = [
    "aarch64-apple-darwin",
    "x86_64-apple-darwin",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
];
pub const LEGACY_GUIDANCE: &str = "Python-era release-v1 cannot be installed by the Rust runtime; re-run the trusted Rust bootstrap to adopt the existing installation. Preserve the source and receipts.";

pub fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(ContractError::new(message).into())
    }
}
pub fn digest(raw: &[u8]) -> String {
    crate::pack_validation::sha256(raw)
}
pub fn parse(raw: &[u8]) -> Result<Value> {
    require(
        raw.len() <= MAX_METADATA,
        "Release metadata exceeds its bound",
    )?;
    Ok(json::decode_bytes(
        raw,
        DecodePolicy {
            duplicate_keys: DuplicateKeys::Reject,
            non_finite: NonFinite::Reject,
            max_bytes: MAX_METADATA,
            ..DecodePolicy::default()
        },
    )?)
}
pub fn canonical(value: &Value) -> Result<Vec<u8>> {
    Ok(json::encode(
        value,
        json::Style {
            allow_nan: false,
            ..json::SORTED_COMPACT_ASCII
        },
    )?
    .into_bytes())
}
pub fn inventory_digest(files: &Value) -> Result<String> {
    Ok(digest(&canonical(files)?))
}
pub fn regular(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let parent = path
        .parent()
        .ok_or_else(|| ContractError::new("Release input needs a parent"))?;
    let name = path
        .file_name()
        .ok_or_else(|| ContractError::new("Release input needs a name"))?;
    let observation =
        crate::fs::read_source(parent, Path::new(name), maximum).map_err(|error| match error {
            crate::fs::ReadError::Io(error) => Error::Io(error),
            crate::fs::ReadError::SizeLimit { .. } => {
                ContractError::new("Release input exceeds its bound").into()
            }
            _ => ContractError::new("Release input must be an ordinary unlinked file").into(),
        })?;
    require(
        observation.identity.nlink == 1,
        "Release input must be an ordinary unlinked file",
    )?;
    Ok(observation.bytes)
}
pub fn write_new(path: &Path, raw: &[u8], mode: u32) -> Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ContractError::new("Release output needs a parent"))?;
    let chain = DirectoryChain::open(parent)?;
    let mut file = chain.directory().create_exclusive(
        path.file_name()
            .ok_or_else(|| ContractError::new("Release output needs a name"))?,
        libc::O_WRONLY,
        mode,
    )?;
    file.write_all(raw)?;
    file.fsync()?;
    chain.recheck()?;
    Ok(())
}
/// Private scratch paths are never reused; cleanup cannot activate or authenticate bytes.
pub struct Scratch {
    pub path: PathBuf,
    retained: bool,
}
impl Scratch {
    pub fn new(prefix: &str) -> Result<Self> {
        let parent = crate::fs::canonical_path(Path::new("/tmp"), true)?;
        crate::fs::trust::inspect(&parent, Default::default())?;
        let path = parent.join(format!("{prefix}{}", crate::host::uuid4()?));
        let parent_dir = Descriptor::open_directory(&parent)?;
        parent_dir.create_directory(
            path.file_name()
                .ok_or_else(|| ContractError::new("Invalid scratch name"))?,
            0o700,
        )?;
        Ok(Self {
            path,
            retained: false,
        })
    }
    pub fn retain(mut self) -> PathBuf {
        self.retained = true;
        std::mem::take(&mut self.path)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        // Owned scratch only; no trust decision or activation uses this best-effort cleanup.
        if !self.retained {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}
pub fn run_command(
    argv: &[OsString],
    data: &[u8],
    maximum: usize,
    cwd: Option<&Path>,
    timeout: Duration,
) -> Result<process::Output> {
    let env: Vec<_> = std::env::vars_os()
        .filter(|(k, _)| {
            ![
                "GH_HOST",
                "GH_PROMPT_DISABLED",
                "GH_PAGER",
                "GIT_TERMINAL_PROMPT",
            ]
            .iter()
            .any(|v| k == v)
        })
        .chain(
            [
                ("GH_HOST", "github.com"),
                ("GH_PROMPT_DISABLED", "1"),
                ("GH_PAGER", "cat"),
                ("GIT_TERMINAL_PROMPT", "0"),
            ]
            .map(|(k, v)| (k.into(), v.into())),
        )
        .collect();
    Ok(process::run_for(
        &Spec {
            argv,
            search_path: true,
            env: &env,
            cwd,
            stdin: Input::Bytes(data),
            stdout: Target::Capture { limit: maximum },
            stderr: Target::Capture { limit: 8192 },
            deadline: crate::clock::Deadline::after(timeout)?,
            new_session: false,
            file_size_limit: None,
            umask: Some(0o077),
            pass_fds: &[],
        },
        timeout,
    )?)
}
/// Explicit trust input is for independently trusted callers and isolated signing tests.
/// Runtime callers use verify_manifest/verify_binary_signature with the compiled root.
pub fn verify_signature_with(
    raw: &[u8],
    signature: &[u8],
    namespace: &str,
    signers: &str,
) -> Result<()> {
    require(
        !signature.is_empty() && signature.len() <= 4096,
        "Manifest or signature exceeds its bound",
    )?;
    require(
        namespace == NAMESPACE || namespace == BINARY_NAMESPACE,
        "Unknown release signature namespace",
    )?;
    let scratch = Scratch::new("am-signature-")?;
    write_new(
        &scratch.path.join("allowed-signers"),
        signers.as_bytes(),
        0o600,
    )?;
    write_new(&scratch.path.join("signature"), signature, 0o600)?;
    let argv = [
        OsString::from("ssh-keygen"),
        "-Y".into(),
        "verify".into(),
        "-f".into(),
        scratch.path.join("allowed-signers").into_os_string(),
        "-I".into(),
        PRINCIPAL.into(),
        "-n".into(),
        namespace.into(),
        "-s".into(),
        scratch.path.join("signature").into_os_string(),
    ];
    let output = run_command(&argv, raw, 8192, None, Duration::from_secs(20))?;
    require(
        output.status == Status::Code(0),
        "Release publisher signature is invalid",
    )
}
pub fn verify_binary_signature(raw: &[u8], signature: &[u8]) -> Result<()> {
    require(
        !raw.is_empty() && raw.len() <= MAX_BINARY,
        "Release binary exceeds its bound",
    )?;
    verify_signature_with(raw, signature, BINARY_NAMESPACE, ALLOWED_SIGNERS)
}
fn exact_fields(value: &Value, fields: &[&str], message: &str) -> Result<()> {
    let pairs = value
        .as_object()
        .ok_or_else(|| ContractError::new(message))?;
    require(
        pairs.len() == fields.len() && pairs.iter().all(|(k, _)| fields.contains(&k.as_str())),
        message,
    )
}
fn unsigned(value: Option<&Value>) -> Option<usize> {
    value?.as_int()?.to_string().parse().ok()
}
fn hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}
pub fn stable_version(version: &str) -> bool {
    version.len() <= 64
        && version.split('.').count() == 3
        && version.split('.').all(|part| {
            !part.is_empty()
                && part.bytes().all(|b| b.is_ascii_digit())
                && (part.len() == 1 || !part.starts_with('0'))
        })
}
#[derive(Debug, Clone, PartialEq)]
pub struct BinaryIdentity {
    pub size: usize,
    pub sha256: String,
    pub artifact_digest: String,
}
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseManifest {
    pub version: String,
    pub tag: String,
    pub source_commit: String,
    pub archive_size: usize,
    pub archive_sha256: String,
    pub artifact_digest: String,
    pub targets: BTreeMap<String, BinaryIdentity>,
    pub command_surface: Value,
}
impl ReleaseManifest {
    pub fn value(&self) -> Value {
        object([
            ("schema", string("agent-memory/release-v2")),
            ("repository", string(REPOSITORY)),
            ("tag", string(&self.tag)),
            ("version", string(&self.version)),
            ("source_commit", string(&self.source_commit)),
            ("archive", string(ARCHIVE)),
            ("archive_size", Value::Int(self.archive_size.into())),
            ("archive_sha256", string(&self.archive_sha256)),
            ("artifact_digest", string(&self.artifact_digest)),
            (
                "targets",
                Value::Object(
                    self.targets
                        .iter()
                        .map(|(k, v)| {
                            (
                                k.clone(),
                                object([
                                    ("size", Value::Int(v.size.into())),
                                    ("sha256", string(&v.sha256)),
                                    ("artifact_digest", string(&v.artifact_digest)),
                                ]),
                            )
                        })
                        .collect(),
                ),
            ),
            ("command_surface", self.command_surface.clone()),
        ])
    }
    pub fn encoded(&self) -> Result<Vec<u8>> {
        let mut raw = canonical(&self.value())?;
        raw.push(b'\n');
        require(
            raw.len() <= MAX_METADATA,
            "Release metadata exceeds its bound",
        )?;
        Ok(raw)
    }
    pub fn from_value(value: &Value, tag: Option<&str>) -> Result<Self> {
        require(
            text(value, "schema") != Some("agent-memory/release-v1"),
            LEGACY_GUIDANCE,
        )?;
        exact_fields(
            value,
            &[
                "schema",
                "repository",
                "tag",
                "version",
                "source_commit",
                "archive",
                "archive_size",
                "archive_sha256",
                "artifact_digest",
                "targets",
                "command_surface",
            ],
            "Unknown release manifest fields",
        )?;
        require(
            text(value, "schema") == Some("agent-memory/release-v2")
                && text(value, "repository") == Some(REPOSITORY),
            "Wrong release schema or repository",
        )?;
        let version = text(value, "version").unwrap_or("");
        let selected = text(value, "tag").unwrap_or("");
        require(
            stable_version(version)
                && selected == format!("v{version}")
                && tag.is_none_or(|v| v == selected),
            "Release tag/version differs from its selected stable release",
        )?;
        let commit = text(value, "source_commit").unwrap_or("");
        require(hex(commit, 40), "Invalid release source commit")?;
        let archive_size = unsigned(value.get("archive_size")).unwrap_or(0);
        require(
            text(value, "archive") == Some(ARCHIVE)
                && archive_size > 0
                && archive_size <= MAX_ARCHIVE,
            "Invalid release archive identity",
        )?;
        let archive_sha256 = text(value, "archive_sha256").unwrap_or("");
        let artifact_digest = text(value, "artifact_digest").unwrap_or("");
        require(
            hex(archive_sha256, 64) && hex(artifact_digest, 64),
            "Invalid release digest",
        )?;
        let map = value
            .get("targets")
            .ok_or_else(|| ContractError::new("Missing release targets"))?;
        exact_fields(map, &TARGETS, "Unknown release targets")?;
        let mut targets = BTreeMap::new();
        for triple in TARGETS {
            let binary = map
                .get(triple)
                .ok_or_else(|| ContractError::new("Missing release target"))?;
            exact_fields(
                binary,
                &["size", "sha256", "artifact_digest"],
                "Unknown release binary fields",
            )?;
            let size = unsigned(binary.get("size")).unwrap_or(0);
            let sha256 = text(binary, "sha256").unwrap_or("");
            let identity = text(binary, "artifact_digest").unwrap_or("");
            require(
                size > 0 && size <= MAX_BINARY && hex(sha256, 64) && hex(identity, 64),
                "Invalid release binary identity",
            )?;
            targets.insert(
                triple.to_owned(),
                BinaryIdentity {
                    size,
                    sha256: sha256.into(),
                    artifact_digest: identity.into(),
                },
            );
        }
        let surface = value
            .get("command_surface")
            .ok_or_else(|| ContractError::new("Missing command surface"))?;
        require(
            surface.as_object().is_some(),
            "Release command surface must be an object",
        )?;
        canonical(surface)?;
        Ok(Self {
            version: version.into(),
            tag: selected.into(),
            source_commit: commit.into(),
            archive_size,
            archive_sha256: archive_sha256.into(),
            artifact_digest: artifact_digest.into(),
            targets,
            command_surface: surface.clone(),
        })
    }
}
pub fn verify_manifest_with(
    raw: &[u8],
    signature: &[u8],
    tag: Option<&str>,
    signers: &str,
) -> Result<ReleaseManifest> {
    require(
        raw.len() <= MAX_METADATA,
        "Manifest or signature exceeds its bound",
    )?;
    verify_signature_with(raw, signature, NAMESPACE, signers)?;
    ReleaseManifest::from_value(&parse(raw)?, tag)
}
pub fn verify_manifest(raw: &[u8], signature: &[u8], tag: Option<&str>) -> Result<ReleaseManifest> {
    verify_manifest_with(raw, signature, tag, ALLOWED_SIGNERS)
}
pub fn compare_command_surfaces(declared: &Value, expected: &Value) -> Result<()> {
    require(
        declared.as_object().is_some()
            && expected.as_object().is_some()
            && canonical(declared)? == canonical(expected)?,
        "Signed command surface differs from the CLI parser table",
    )
}
pub fn bind_bootstrap_binary(
    manifest: &ReleaseManifest,
    binary: &Path,
    triple: &str,
    compiled_version: &str,
) -> Result<Vec<u8>> {
    require(
        manifest.version == compiled_version,
        "Bootstrap binary version differs from its signed release; re-run the trusted bootstrap",
    )?;
    let target = manifest
        .targets
        .get(triple)
        .ok_or_else(|| ContractError::new("Unsupported release target"))?;
    let raw = regular(binary, MAX_BINARY)?;
    require(
        raw.len() == target.size && digest(&raw) == target.sha256,
        "Bootstrap binary differs from its signed release; re-run the trusted bootstrap",
    )?;
    Ok(raw)
}
pub type Inventory = BTreeMap<String, (Vec<u8>, u32)>;
pub fn inventory_value(files: &Inventory) -> Value {
    Value::Object(
        files
            .iter()
            .map(|(name, (raw, mode))| {
                (
                    name.clone(),
                    object([
                        ("sha256", string(digest(raw))),
                        ("mode", string(format!("0o{mode:o}"))),
                    ]),
                )
            })
            .collect(),
    )
}
fn combined_artifact_digest(files: &Value, binary_sha256: &str) -> Result<String> {
    Ok(crate::artifact::runtime_digest(files, binary_sha256, 0o755)?.1)
}
pub fn portable_files(root: &Path) -> Result<Inventory> {
    let chain = DirectoryChain::open(root)?;
    let root_pin = chain.directory().stat()?;
    let mut files = Inventory::new();
    let mut entries = 0;
    let mut total = 0;
    let mut pins = Vec::new();
    fn walk(
        dir: &Descriptor,
        relative: &Path,
        files: &mut Inventory,
        entries: &mut usize,
        total: &mut usize,
        pins: &mut Vec<(PathBuf, crate::fs::Identity)>,
    ) -> Result<()> {
        require(
            relative.components().count() <= 64,
            "Release inventory exceeds its depth bound",
        )?;
        let names = dir
            .entries_bounded(MAX_ENTRIES.saturating_sub(*entries))?
            .ok_or_else(|| ContractError::new("Release inventory exceeds its entry bound"))?;
        *entries += names.len();
        for name in names {
            let path = relative.join(&name);
            let pin = dir.stat_at(&name)?;
            require(
                pin.is_regular() || pin.is_directory(),
                "Unsafe release file type",
            )?;
            pins.push((path.clone(), pin));
            if pin.is_directory() {
                require(pin.mode & 0o500 == 0o500, "Unreadable release directory")?;
                let child = dir.open_directory_at(&name)?;
                require(
                    child.stat()? == pin,
                    "Release directory changed during inventory",
                )?;
                walk(&child, &path, files, entries, total, pins)?;
                require(
                    child.stat()? == pin && dir.stat_at(&name)? == pin,
                    "Release directory changed during inventory",
                )?;
            } else {
                require(
                    pin.nlink == 1,
                    "Release input must be an ordinary unlinked file",
                )?;
                require(
                    pin.size >= 0 && pin.size as usize <= MAX_FILE,
                    "Release input exceeds its bound",
                )?;
                let mode = pin.mode & 0o7777;
                require(
                    [0o644, 0o755].contains(&mode),
                    "Release bytes or modes violate the portable bound",
                )?;
                let mut file = dir.open_at(&name, libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
                require(
                    file.stat()? == pin,
                    "Release input changed during inventory",
                )?;
                let raw = file
                    .read_bounded(MAX_FILE)
                    .map_err(|_| ContractError::new("Release input changed during inventory"))?;
                *total += raw.len();
                require(
                    *total <= MAX_BYTES,
                    "Release bytes or modes violate the portable bound",
                )?;
                require(
                    file.stat()? == pin && dir.stat_at(&name)? == pin,
                    "Release input changed during inventory",
                )?;
                files.insert(
                    crate::host::text(path.as_os_str(), "portable path")?.into(),
                    (raw, mode),
                );
            }
        }
        Ok(())
    }
    walk(
        chain.directory(),
        Path::new(""),
        &mut files,
        &mut entries,
        &mut total,
        &mut pins,
    )?;
    chain.recheck()?;
    require(
        chain.directory().stat()? == root_pin,
        "Release directory changed during inventory",
    )?;
    for (relative, pin) in pins {
        let path = root.join(relative);
        let parent = DirectoryChain::open(
            path.parent()
                .ok_or_else(|| ContractError::new("Invalid release path"))?,
        )?;
        require(
            parent.directory().stat_at(
                path.file_name()
                    .ok_or_else(|| ContractError::new("Invalid release path"))?,
            )? == pin,
            "Release input changed during inventory",
        )?;
    }
    Ok(files)
}
pub fn package_identity(root: &Path) -> Result<(String, String, Value)> {
    let files = portable_files(root)?;
    require(
        files.contains_key("SKILL.md"),
        "Release lacks its portable root",
    )?;
    require(
        files
            .get("keys/allowed-signers")
            .is_some_and(|(raw, _)| raw == ALLOWED_SIGNERS.as_bytes()),
        "Portable key differs from the pinned publisher",
    )?;
    let skill = files
        .get("SKILL.md")
        .ok_or_else(|| ContractError::new("Release lacks its portable root"))?;
    let text = std::str::from_utf8(&skill.0)
        .map_err(|_| ContractError::new("Portable metadata is not UTF-8"))?;
    let declarations: Vec<_> = text
        .lines()
        .filter_map(|line| line.strip_prefix("metadata: "))
        .collect();
    require(
        declarations.len() == 1,
        "Release needs one version declaration",
    )?;
    let declaration = parse(
        declarations
            .first()
            .ok_or_else(|| ContractError::new("Release needs one version declaration"))?
            .as_bytes(),
    )?;
    let version = crate::lifecycle::text(&declaration, "version").unwrap_or("");
    require(stable_version(version), "Invalid portable release version")?;
    let inventory = inventory_value(&files);
    Ok((version.into(), inventory_digest(&inventory)?, inventory))
}
/// Strict USTAR, rejecting extensions, links, devices, duplicates and noncanonical names.
pub fn tar_entries(expanded: &[u8]) -> Result<Inventory> {
    Ok(parse_tar(expanded)?.0)
}
fn parse_tar(expanded: &[u8]) -> Result<(Inventory, BTreeSet<String>)> {
    require(
        expanded.len() <= MAX_TAR && expanded.len() % 512 == 0,
        "Release expansion exceeds its bound",
    )?;
    let mut result = Inventory::new();
    let mut seen = BTreeSet::new();
    let mut directories = BTreeSet::new();
    let mut cursor = 0;
    let mut total = 0;
    while cursor + 512 <= expanded.len() {
        // The loop proves a complete 512-byte header is present.
        let header = &expanded[cursor..cursor + 512];
        if header.iter().all(|b| *b == 0) {
            require(
                expanded.len() - cursor >= 1024 && expanded[cursor..].iter().all(|b| *b == 0),
                "Invalid signed portable archive",
            )?;
            return Ok((result, directories));
        }
        let field = |range: std::ops::Range<usize>| -> Result<&str> {
            let bytes = header
                .get(range)
                .ok_or_else(|| ContractError::new("Invalid signed portable archive"))?;
            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            // `end` is a position found within this byte slice, or its own length.
            Ok(std::str::from_utf8(&bytes[..end])
                .map_err(|_| ContractError::new("Invalid signed portable archive"))?)
        };
        let number = |range| -> Result<usize> {
            Ok(usize::from_str_radix(field(range)?.trim(), 8)
                .map_err(|_| ContractError::new("Invalid signed portable archive"))?)
        };
        let sum: usize = header
            .iter()
            .enumerate()
            .map(|(i, b)| {
                if (148..156).contains(&i) {
                    32
                } else {
                    *b as usize
                }
            })
            .sum();
        require(
            number(148..156)? == sum && &header[257..263] == b"ustar\0",
            "Invalid signed portable archive",
        )?;
        let leaf = field(0..100)?;
        let prefix = field(345..500)?;
        let name = if prefix.is_empty() {
            leaf.to_owned()
        } else {
            format!("{prefix}/{leaf}")
        };
        let name = if header[156] == b'5' {
            name.strip_suffix('/').unwrap_or(&name).to_owned()
        } else {
            name
        };
        require(
            seen.len() < MAX_ENTRIES
                && seen.insert(name.clone())
                && !name.starts_with('/')
                && name
                    .split('/')
                    .all(|v| !v.is_empty() && v != "." && v != "..")
                && (name.starts_with(PREFIX)
                    || (header[156] == b'5'
                        && ["skills", "skills/agent-memory"].contains(&name.as_str()))),
            "Unsafe, duplicate or excessive release member",
        )?;
        let mode = u32::try_from(number(100..108)?)
            .map_err(|_| ContractError::new("Unsafe release member type or mode"))?;
        let size = number(124..136)?;
        require(
            [0o644, 0o755].contains(&mode) && [0, b'0', b'5'].contains(&header[156]),
            "Unsafe release member type or mode",
        )?;
        cursor += 512;
        require(
            size <= MAX_FILE
                && cursor
                    .checked_add(size)
                    .is_some_and(|n| n <= expanded.len()),
            "Release member exceeds its byte bound",
        )?;
        if header[156] == b'5' {
            require(mode == 0o755 && size == 0, "Invalid release directory")?;
            directories.insert(name);
        } else {
            total += size;
            require(total <= MAX_BYTES, "Release member exceeds its byte bound")?;
            // The size/bounds check above proves this member range is present.
            result.insert(name, (expanded[cursor..cursor + size].to_vec(), mode));
        }
        cursor += size.div_ceil(512) * 512;
    }
    Err(ContractError::new("Invalid signed portable archive").into())
}
pub fn encode_archive(files: &Inventory) -> Result<Vec<u8>> {
    let mut tar = Vec::new();
    for (name, (raw, mode)) in files {
        require(
            name.len() <= 255 && raw.len() <= MAX_FILE && [0o644, 0o755].contains(mode),
            "Unsafe tracked release entry",
        )?;
        let (prefix, leaf) = if name.len() <= 100 {
            ("", name.as_str())
        } else {
            name.rsplit_once('/')
                .ok_or_else(|| ContractError::new("Release member path exceeds USTAR bound"))?
        };
        require(
            prefix.len() <= 155 && leaf.len() <= 100,
            "Release member path exceeds USTAR bound",
        )?;
        let mut header = [0u8; 512];
        // The checked USTAR name bounds and fixed numeric widths fit this 512-byte header.
        header[..leaf.len()].copy_from_slice(leaf.as_bytes());
        header[345..345 + prefix.len()].copy_from_slice(prefix.as_bytes());
        for (start, width, value) in [
            (100, 8, *mode as usize),
            (108, 8, 0),
            (116, 8, 0),
            (124, 12, raw.len()),
            (136, 12, 0),
        ] {
            let value = format!("{value:0width$o}", width = width - 1);
            header[start..start + width - 1].copy_from_slice(value.as_bytes());
        }
        header[148..156].fill(b' ');
        header[156] = b'0';
        header[257..263].copy_from_slice(b"ustar\0");
        header[263..265].copy_from_slice(b"00");
        let sum: usize = header.iter().map(|b| *b as usize).sum();
        header[148..156].copy_from_slice(format!("{sum:06o}\0 ").as_bytes());
        tar.extend_from_slice(&header);
        tar.extend_from_slice(raw);
        tar.resize(tar.len().div_ceil(512) * 512, 0);
    }
    tar.resize((tar.len() + 1024).div_ceil(10240) * 10240, 0);
    require(tar.len() <= MAX_TAR, "Release expansion exceeds its bound")?;
    let mut gzip = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::default());
    gzip.write_all(&tar)?;
    let raw = gzip.finish()?;
    require(raw.len() <= MAX_ARCHIVE, "Release archive exceeds bound")?;
    Ok(raw)
}
pub fn extract(raw: &[u8], destination: &Path, manifest: &ReleaseManifest) -> Result<()> {
    require(
        raw.len() == manifest.archive_size && digest(raw) == manifest.archive_sha256,
        "Archive differs from the signed release identity",
    )?;
    let mut expanded = Vec::new();
    let mut decoder = flate2::bufread::GzDecoder::new(raw);
    decoder
        .by_ref()
        .take((MAX_TAR + 1) as u64)
        .read_to_end(&mut expanded)
        .map_err(|_| ContractError::new("Invalid signed portable archive"))?;
    require(
        decoder.get_ref().is_empty(),
        "Trailing data or multiple gzip members in signed portable archive",
    )?;
    require(
        expanded.len() <= MAX_TAR,
        "Release expansion exceeds its bound",
    )?;
    let (files, directories) = parse_tar(&expanded)?;
    let parent = DirectoryChain::open(
        destination
            .parent()
            .ok_or_else(|| ContractError::new("Invalid release destination"))?,
    )?;
    parent.directory().create_directory(
        destination
            .file_name()
            .ok_or_else(|| ContractError::new("Invalid release destination"))?,
        0o700,
    )?;
    for name in directories {
        portable_directory(destination, Path::new(&name))?;
    }
    for (name, (bytes, mode)) in files {
        let path = destination.join(&name);
        portable_directory(
            destination,
            Path::new(&name)
                .parent()
                .ok_or_else(|| ContractError::new("Invalid release path"))?,
        )?;
        write_new(&path, &bytes, mode)?;
    }
    let (version, identity, _) = package_identity(&destination.join("skills/agent-memory"))?;
    require(
        version == manifest.version && identity == manifest.artifact_digest,
        "Portable identity differs from its signed declaration",
    )
}

pub fn portable_directory(root: &Path, relative: &Path) -> Result<()> {
    let mut parent = Descriptor::open_directory(root)?;
    for part in relative.components() {
        let std::path::Component::Normal(name) = part else {
            return Err(ContractError::new("Invalid portable directory path").into());
        };
        let child = match parent.open_directory_at(name) {
            Ok(child) => child,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                parent.create_directory(name, 0o755)?
            }
            Err(error) => return Err(error.into()),
        };
        require(
            child.stat()?.same_inode(parent.stat_at(name)?),
            "Release directory binding changed",
        )?;
        parent = child;
    }
    Ok(())
}

pub const DOWNLOAD_ATTEMPTS: usize = 3;
pub const DOWNLOAD_BUDGET: f64 = 30.0;
pub const STDERR_LIMIT: usize = 8192;
#[derive(Debug)]
pub struct DownloadError {
    pub details: Value,
    pub repair: String,
    message: String,
}
impl std::fmt::Display for DownloadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}
impl std::error::Error for DownloadError {}
pub fn download_location(url: &str) -> Value {
    let authority = url.split_once("://").map(|(_, v)| v).unwrap_or("");
    let host = authority
        .split('/')
        .next()
        .unwrap_or("")
        .rsplit('@')
        .next()
        .unwrap_or("")
        .split(':')
        .next()
        .unwrap_or("");
    let host = if !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'.' || b == b'-')
    {
        host
    } else {
        "configured HTTPS host"
    };
    let path = authority
        .split_once('/')
        .map(|(_, v)| v)
        .unwrap_or("")
        .split('?')
        .next()
        .unwrap_or("");
    let stage = match path.rsplit('/').next().unwrap_or("") {
        MANIFEST => "release manifest",
        SIGNATURE => "publisher signature",
        ARCHIVE => "release archive",
        "install.sh" => "bootstrap readback",
        "latest" => "release discovery",
        _ => "release download",
    };
    object([("stage", string(stage)), ("host", string(host))])
}
/// Diagnostics are deliberately conservative: potentially secret-bearing lines are wholly redacted.
pub fn safe_download_detail(raw: &[u8]) -> String {
    let clean: String = String::from_utf8_lossy(raw)
        .chars()
        .filter(|c| c.is_ascii_graphic() || c.is_ascii_whitespace())
        .collect();
    let mut lines = Vec::new();
    for line in clean.lines() {
        let lower = line.to_ascii_lowercase();
        if [
            "authorization",
            "cookie",
            "token",
            "password",
            "passwd",
            "secret",
            "api_key",
            "api-key",
        ]
        .iter()
        .any(|k| lower.contains(k))
            || line.contains('=')
        {
            lines.push("[redacted secret]".to_owned());
        } else {
            lines.push(
                line.split_whitespace()
                    .map(|word| {
                        if word.contains("://") {
                            "[redacted URL]"
                        } else if word.contains('@') {
                            "[redacted userinfo]"
                        } else if word.contains('/') {
                            "[redacted local path]"
                        } else {
                            word
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" "),
            );
        }
    }
    lines
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(512)
        .collect()
}
fn rounded_seconds(ns: u64) -> Value {
    Value::Float((ns as f64 / 1_000_000.0).round() / 1000.0)
}
fn download_error(url: &str, attempts: Vec<Value>, elapsed: u64, budget: f64) -> DownloadError {
    let last = attempts.last();
    let code = last
        .and_then(|v| v.get("curl_exit"))
        .cloned()
        .unwrap_or(Value::Null);
    let status = last
        .and_then(|v| v.get("http_status"))
        .cloned()
        .unwrap_or(Value::Null);
    let reason_owned = last
        .and_then(|v| text(v, "reason"))
        .unwrap_or("deadline")
        .to_owned();
    let reason = reason_owned.as_str();
    let numeric = code
        .as_int()
        .and_then(|v| v.to_string().parse::<i32>().ok());
    let category = if reason == "transport" {
        match numeric {
            Some(5) => "proxy DNS failure",
            Some(6) => "DNS failure",
            Some(7) => "connection failure",
            Some(16) => "HTTP/2 failure",
            Some(18) => "incomplete response",
            Some(28) => "transfer timeout",
            Some(52) => "empty response",
            Some(55) => "send failure",
            Some(56) => "receive failure",
            Some(92) => "HTTP/2 stream failure",
            Some(22) => "HTTP rejection",
            Some(35) => "TLS handshake failure",
            Some(60) => "certificate validation failure",
            Some(77) => "certificate configuration failure",
            Some(23) => "local staging write failure",
            _ => "transport failure",
        }
    } else {
        reason
    };
    let http = status
        .as_int()
        .and_then(|v| v.to_string().parse::<i32>().ok());
    let guidance = if reason == "size-limit" {
        "Check expected release size and local temporary-storage/file-size limits; report a persistent mismatch."
    } else if numeric.is_some_and(|n| [35, 51, 58, 59, 60, 64, 66, 77, 82, 83, 90, 91].contains(&n))
    {
        "Check the system clock, trusted certificates and any TLS-inspecting proxy configuration, then retry."
    } else {
        match http {
            Some(429) => {
                "The server rate-limited this request. Wait for its limit to reset, then retry the same command."
            }
            Some(401 | 403) => {
                "Check server access restrictions, proxy policy and GitHub API rate limits, then retry."
            }
            Some(404) => {
                "The release endpoint or asset is unavailable. Check the selected release or report the missing asset."
            }
            _ if numeric == Some(23) || reason == "process-start" => {
                "Check that curl is executable and private temporary storage has space and writable permissions."
            }
            _ => {
                "Retry the same command after checking network, DNS, VPN/proxy access and connection speed to the reported host."
            }
        }
    };
    let location = download_location(url);
    let detail = last.and_then(|v| text(v, "detail")).unwrap_or("");
    let message = format!(
        "Signed release download failed: {} from {} after {} attempt(s) (curl {}; HTTP {}; {}).{}{}",
        text(&location, "stage").unwrap_or("release download"),
        text(&location, "host").unwrap_or("configured HTTPS host"),
        attempts.len(),
        numeric
            .map(|v| v.to_string())
            .unwrap_or("not completed".into()),
        http.map(|v| v.to_string()).unwrap_or("unknown".into()),
        category,
        if detail.is_empty() { "" } else { " " },
        detail
    );
    let mut fields = vec![("schema".into(), string("agent-memory/download-failure-v1"))];
    fields.extend(location.as_object().unwrap_or(&[]).iter().cloned());
    fields.extend([
        ("attempt_count".into(), Value::Int(attempts.len().into())),
        ("attempts".into(), Value::Array(attempts)),
        ("elapsed_seconds".into(), rounded_seconds(elapsed)),
        ("budget_seconds".into(), Value::Float(budget)),
        ("reason".into(), string(category)),
        ("curl_exit".into(), code),
        ("http_status".into(), status),
        ("activation_accepted".into(), Value::Bool(false)),
    ]);
    let mut details = Value::Object(fields);
    let diagnostic = (|| -> Result<PathBuf> {
        let scratch = Scratch::new("am-download-failure-")?;
        let mut raw = json::encode(&details, json::SORTED_INDENT2_ASCII)?.into_bytes();
        raw.push(b'\n');
        require(
            raw.len() <= 8192,
            "Download diagnostic exceeds its private report bound",
        )?;
        let path = scratch.path.join("failure.json");
        write_new(&path, &raw, 0o600)?;
        require(
            regular(&path, 8192)? == raw,
            "Download diagnostic readback differs",
        )?;
        scratch.retain();
        Ok(path)
    })();
    let mut repair =
        format!("Failed downloaded bytes were not accepted for activation. {guidance}");
    if let Value::Object(fields) = &mut details {
        match diagnostic {
            Ok(path) => {
                repair.push_str(&format!(" Private diagnostic: {}", path.display()));
                fields.push(("diagnostic_path".into(), string(path.to_string_lossy())));
            }
            Err(_) => {
                let error = "Could not retain the private diagnostic; save this error text.";
                fields.push(("diagnostic_path".into(), Value::Null));
                fields.push(("diagnostic_error".into(), string(error)));
                repair.push(' ');
                repair.push_str(error);
            }
        }
    }
    DownloadError {
        details,
        repair,
        message,
    }
}
#[derive(Clone, Copy, Default)]
pub struct DownloadOptions {
    pub json_response: bool,
    pub allow_file: bool,
}
pub fn download(url: &str, maximum: usize) -> std::result::Result<Vec<u8>, DownloadError> {
    download_with(url, maximum, DownloadOptions::default())
}
pub fn download_with(
    url: &str,
    maximum: usize,
    options: DownloadOptions,
) -> std::result::Result<Vec<u8>, DownloadError> {
    let environment: Vec<_> = std::env::vars_os().collect();
    download_with_environment(url, maximum, options, &environment, Duration::from_secs(30))
}
/// Explicit process environment/deadline injection avoids global mutation in parallel tests.
pub fn download_with_environment(
    url: &str,
    maximum: usize,
    options: DownloadOptions,
    environment: &[(OsString, OsString)],
    budget: Duration,
) -> std::result::Result<Vec<u8>, DownloadError> {
    let start = crate::clock::monotonic_ns();
    let mut attempts = Vec::new();
    let run = (|| -> Result<Option<Vec<u8>>> {
        require(
            url.starts_with("https://") || (options.allow_file && url.starts_with("file://")),
            "Release acquisition requires HTTPS",
        )?;
        require(maximum <= MAX_BINARY, "Release download exceeds its bound")?;
        require(
            !budget.is_zero() && budget.as_secs_f64() <= DOWNLOAD_BUDGET,
            "Invalid release download deadline",
        )?;
        for number in 1..=DOWNLOAD_ATTEMPTS {
            let now = crate::clock::monotonic_ns();
            let remaining = budget.as_secs_f64() - now.saturating_sub(start) as f64 / 1e9;
            if remaining <= 0.0 {
                break;
            }
            let scratch = Scratch::new("am-download-")?;
            let dir = Descriptor::open_directory(&scratch.path)?;
            let body = dir.create_exclusive(std::ffi::OsStr::new("body"), libc::O_RDWR, 0o600)?;
            use std::os::fd::{AsFd, AsRawFd};
            let fd = body.as_fd();
            let proto = if options.allow_file {
                "=https,file"
            } else {
                "=https"
            };
            let argv: Vec<OsString> = [
                "curl",
                "--disable",
                "--fail",
                "--silent",
                "--show-error",
                "--location",
                "--proto",
                proto,
                "--proto-redir",
                "=https",
                "--max-redirs",
                "5",
                "--max-time",
                &remaining.to_string(),
                "--connect-timeout",
                &remaining.min(5.0).to_string(),
                "--max-filesize",
                &maximum.to_string(),
                "--header",
                if options.json_response {
                    "Accept: application/vnd.github+json"
                } else {
                    "Accept: application/octet-stream"
                },
                "--output",
                &format!("/dev/fd/{}", body.as_raw_fd()),
                "--write-out",
                "%{http_code}",
                "--",
                url,
            ]
            .iter()
            .map(OsString::from)
            .collect();
            let output = process::run(&Spec {
                argv: &argv,
                search_path: true,
                env: environment,
                cwd: None,
                stdin: Input::Null,
                stdout: Target::Capture { limit: 16 },
                stderr: Target::Capture {
                    limit: STDERR_LIMIT,
                },
                deadline: crate::clock::Deadline::from_absolute_ns(
                    start
                        .checked_add(budget.as_nanos() as u64)
                        .ok_or_else(|| ContractError::new("Release download deadline overflows"))?,
                ),
                new_session: false,
                file_size_limit: Some(process::FileSize::SoftCap(
                    (maximum + 1).max(STDERR_LIMIT + 1) as u64,
                )),
                umask: Some(0o077),
                pass_fds: &[fd],
            });
            let (code, metadata, errors, mut reason) = match output {
                Ok(out) => (
                    Some(match out.status {
                        Status::Code(n) => n,
                        Status::Signal(n) => -n,
                    }),
                    out.stdout,
                    out.stderr,
                    "transport",
                ),
                Err(error) => {
                    let reason = match error.reason {
                        process::Refusal::Deadline => "deadline",
                        process::Refusal::Spawn => "process-start",
                        process::Refusal::CaptureLimit { .. } => "size-limit",
                        _ => "process-terminated",
                    };
                    let evidence = error.evidence;
                    (
                        if reason == "deadline" || reason == "process-start" {
                            None
                        } else {
                            evidence.as_ref().and_then(|e| e.status).map(|s| match s {
                                Status::Code(n) => n,
                                Status::Signal(n) => -n,
                            })
                        },
                        evidence
                            .as_ref()
                            .map(|e| e.stdout.clone())
                            .unwrap_or_default(),
                        evidence.map(|e| e.stderr).unwrap_or_default(),
                        reason,
                    )
                }
            };
            let status: Option<u16> =
                if metadata.len() == 3 && metadata.iter().all(u8::is_ascii_digit) {
                    std::str::from_utf8(&metadata)
                        .ok()
                        .and_then(|v| v.parse().ok())
                        .filter(|v| *v != 0)
                } else {
                    None
                };
            let size = body.stat()?.size.max(0) as usize;
            if size > maximum || code == Some(63) || code == Some(-libc::SIGXFSZ) {
                reason = "size-limit";
            } else if code.is_some_and(|v| v < 0) {
                reason = "process-terminated";
            }
            if code == Some(0) && reason == "transport" {
                return regular(&scratch.path.join("body"), maximum).map(Some);
            }
            attempts.push(object([
                ("attempt", Value::Int(number.into())),
                (
                    "curl_exit",
                    code.map(|v| Value::Int(v.into())).unwrap_or(Value::Null),
                ),
                (
                    "http_status",
                    status.map(|v| Value::Int(v.into())).unwrap_or(Value::Null),
                ),
                ("reason", string(reason)),
                ("detail", string(safe_download_detail(&errors))),
                ("received_bytes", Value::Int(size.into())),
                (
                    "seconds",
                    rounded_seconds(crate::clock::monotonic_ns().saturating_sub(now)),
                ),
            ]));
            let retry = reason == "transport"
                && ((status.is_none_or(|s| s < 400)
                    && code.is_some_and(|c| [5, 6, 7, 16, 18, 28, 52, 55, 56, 92].contains(&c)))
                    || (code == Some(22)
                        && status.is_some_and(|s| [408, 500, 502, 503, 504].contains(&s))));
            let delay = Duration::from_millis(250 * number as u64);
            if !retry
                || number == DOWNLOAD_ATTEMPTS
                || crate::clock::monotonic_ns().saturating_sub(start) + delay.as_nanos() as u64
                    >= budget.as_nanos() as u64
            {
                break;
            }
            std::thread::sleep(delay);
        }
        Ok(None)
    })();
    match run {
        Ok(Some(raw)) => Ok(raw),
        result => {
            if let Err(error) = result {
                attempts.push(object([
                    ("attempt", Value::Int((attempts.len() + 1).into())),
                    ("curl_exit", Value::Null),
                    ("http_status", Value::Null),
                    ("reason", string("process-start")),
                    (
                        "detail",
                        string(safe_download_detail(error.to_string().as_bytes())),
                    ),
                    ("received_bytes", Value::Int(0.into())),
                    ("seconds", Value::Float(0.0)),
                ]));
            }
            Err(download_error(
                url,
                attempts,
                crate::clock::monotonic_ns().saturating_sub(start),
                budget.as_secs_f64(),
            ))
        }
    }
}
#[derive(Debug, Clone)]
pub struct ReleaseObservation {
    pub manifest: ReleaseManifest,
    pub raw: Vec<u8>,
    pub signature: Vec<u8>,
    pub base: String,
}
pub fn probe() -> Result<ReleaseObservation> {
    probe_with(
        &format!("https://github.com/{REPOSITORY}/releases/latest/download/{MANIFEST}"),
        ASSETS,
        DownloadOptions::default(),
        ALLOWED_SIGNERS,
    )
}
pub fn probe_with(
    first_manifest_url: &str,
    assets: &str,
    options: DownloadOptions,
    signers: &str,
) -> Result<ReleaseObservation> {
    let raw = download_with(first_manifest_url, MAX_METADATA, options)?;
    // The first response is untrusted; it chooses only a syntactically constrained tag.
    let value = parse(&raw)?;
    let version = text(&value, "version").unwrap_or("");
    require(stable_version(version), "Invalid stable release tag")?;
    let tag = format!("v{version}");
    require(
        text(&value, "tag") == Some(tag.as_str()),
        "Invalid stable release tag",
    )?;
    let base = format!("{}/{tag}/", assets.trim_end_matches('/'));
    let signature = download_with(&format!("{base}{SIGNATURE}"), 4096, options)?;
    let manifest = verify_manifest_with(&raw, &signature, Some(&tag), signers)?;
    Ok(ReleaseObservation {
        manifest,
        raw,
        signature,
        base,
    })
}
pub fn acquire(
    destination: &Path,
    observation: Option<&ReleaseObservation>,
) -> Result<ReleaseManifest> {
    let observed = match observation {
        Some(v) => v.clone(),
        None => probe()?,
    };
    acquire_with(destination, &observed, DownloadOptions::default())
}
pub fn acquire_with(
    destination: &Path,
    observed: &ReleaseObservation,
    options: DownloadOptions,
) -> Result<ReleaseManifest> {
    let raw = download_with(
        &format!("{}{ARCHIVE}", observed.base),
        observed.manifest.archive_size,
        options,
    )?;
    extract(&raw, destination, &observed.manifest)?;
    write_new(&destination.join(MANIFEST), &observed.raw, 0o644)?;
    write_new(&destination.join(SIGNATURE), &observed.signature, 0o644)?;
    Ok(observed.manifest.clone())
}
pub fn installed(root: &Path, triple: &str) -> Result<ReleaseManifest> {
    installed_with(root, triple, ALLOWED_SIGNERS)
}
pub fn installed_with(root: &Path, triple: &str, signers: &str) -> Result<ReleaseManifest> {
    let manifest = verify_manifest_with(
        &regular(&root.join(MANIFEST), MAX_METADATA)?,
        &regular(&root.join(SIGNATURE), 4096)?,
        None,
        signers,
    )?;
    let (version, identity, files) = package_identity(&root.join("skills/agent-memory"))?;
    require(
        version == manifest.version && identity == manifest.artifact_digest,
        "Installed portable identity differs from its signed release",
    )?;
    let binary = crate::fs::read_source(root, Path::new("bin/am"), MAX_BINARY).map_err(|_| {
        ContractError::new("Installed runtime identity differs from its signed release")
    })?;
    let target = manifest
        .targets
        .get(triple)
        .ok_or_else(|| ContractError::new("Unsupported release target"))?;
    require(
        binary.bytes.len() == target.size
            && binary.identity.nlink == 1
            && binary.identity.mode & 0o7777 == 0o755
            && digest(&binary.bytes) == target.sha256
            && combined_artifact_digest(&files, &target.sha256)? == target.artifact_digest,
        "Installed runtime identity differs from its signed release",
    )?;
    let dir = Descriptor::open_directory(root)?;
    require(
        dir.entries_bounded(4)?
            == Some(
                ["bin", MANIFEST, SIGNATURE, "skills"]
                    .map(OsString::from)
                    .into_iter()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect::<Vec<_>>(),
            ),
        "Unexpected content in the signed source layout",
    )?;
    require(
        Descriptor::open_directory(&root.join("skills"))?.entries_bounded(1)?
            == Some(vec![OsString::from("agent-memory")])
            && Descriptor::open_directory(&root.join("bin"))?.entries_bounded(1)?
                == Some(vec![OsString::from("am")]),
        "Unexpected content in the signed source layout",
    )?;
    Ok(manifest)
}
