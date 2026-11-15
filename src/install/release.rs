//! Signed release-v2 acquisition and verification. Downloaded bytes are
//! data until the pinned publisher key has verified them.
use crate::bail;
use crate::json::{self, Value, obj, s};
use crate::util::{Error, Result, io_at, run, sha256_hex, uuid4};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const REPOSITORY: &str = "robertelee78/agent-memory";
pub const ASSETS: &str = "https://github.com/robertelee78/agent-memory/releases/download";
pub const LATEST: &str =
    "https://github.com/robertelee78/agent-memory/releases/latest/download/release-manifest.json";
pub const PRINCIPAL: &str = "release@agent-memory";
pub const NAMESPACE: &str = "agent-memory.release";
pub const BINARY_NAMESPACE: &str = "agent-memory.release-binary";
pub const ALLOWED_SIGNERS: &str = include_str!("../../skills/agent-memory/keys/allowed-signers");
pub const ARCHIVE: &str = "agent-memory.tar.gz";
pub const MANIFEST: &str = "release-manifest.json";
pub const SIGNATURE: &str = "release-manifest.json.sshsig";
pub const PACK: &str = "skills/agent-memory";
pub const BINARY: &str = "bin/am";
pub const MAX_FILE: usize = 2 * 1024 * 1024;
pub const MAX_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_ENTRIES: usize = 4096;
pub const MAX_ARCHIVE: usize = MAX_BYTES + 1024 * 1024;
pub const MAX_TAR: usize = MAX_BYTES + MAX_ENTRIES * 1024;
pub const MAX_MANIFEST: usize = 1024 * 1024;
pub const MAX_BINARY: usize = 64 * 1024 * 1024;
pub const MAX_SIGNATURE: usize = 4096;
pub const TARGETS: [&str; 3] = [
    "aarch64-apple-darwin",
    "x86_64-unknown-linux-musl",
    "aarch64-unknown-linux-musl",
];
/// The compiled target triple.
pub const TARGET: &str = if cfg!(all(target_arch = "aarch64", target_os = "macos")) {
    "aarch64-apple-darwin"
} else if cfg!(all(target_arch = "x86_64", target_os = "linux")) {
    "x86_64-unknown-linux-musl"
} else if cfg!(all(target_arch = "aarch64", target_os = "linux")) {
    "aarch64-unknown-linux-musl"
} else {
    "unsupported"
};
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(Error::new(message))
    }
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
/// Numeric `x.y.z` ordering key.
pub fn version_key(version: &str) -> Vec<u64> {
    version.split('.').map(|p| p.parse().unwrap_or(0)).collect()
}

/// Sorted, compact, ASCII JSON: the signed inventory encoding.
pub fn canonical(value: &Value) -> Vec<u8> {
    json::encode(value, json::CANONICAL).into_bytes()
}

#[derive(Debug, Clone, PartialEq)]
pub struct BinaryIdentity {
    pub size: usize,
    pub sha256: String,
    pub artifact_digest: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Manifest {
    pub version: String,
    pub tag: String,
    pub source_commit: String,
    pub archive_size: usize,
    pub archive_sha256: String,
    pub artifact_digest: String,
    pub targets: BTreeMap<String, BinaryIdentity>,
    pub command_surface: Value,
}

impl Manifest {
    pub fn value(&self) -> Value {
        let targets = self
            .targets
            .iter()
            .map(|(triple, b)| {
                (
                    triple.clone(),
                    obj([
                        ("size", Value::Int(b.size as i64)),
                        ("sha256", s(&b.sha256)),
                        ("artifact_digest", s(&b.artifact_digest)),
                    ]),
                )
            })
            .collect();
        obj([
            ("schema", s("agent-memory/release-v2")),
            ("repository", s(REPOSITORY)),
            ("tag", s(&self.tag)),
            ("version", s(&self.version)),
            ("source_commit", s(&self.source_commit)),
            ("archive", s(ARCHIVE)),
            ("archive_size", Value::Int(self.archive_size as i64)),
            ("archive_sha256", s(&self.archive_sha256)),
            ("artifact_digest", s(&self.artifact_digest)),
            ("targets", Value::Object(targets)),
            ("command_surface", self.command_surface.clone()),
        ])
    }
    /// Canonical bytes plus a final newline (what is signed).
    pub fn encoded(&self) -> Vec<u8> {
        let mut raw = canonical(&self.value());
        raw.push(b'\n');
        raw
    }
    pub fn from_value(value: &Value, tag: Option<&str>) -> Result<Manifest> {
        require(
            value.str("schema") != Some("agent-memory/release-v1"),
            "This is a Python-era release-v1 manifest; run the current install.sh instead.",
        )?;
        const FIELDS: [&str; 11] = [
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
        ];
        let exact = |v: &Value, fields: &[&str]| {
            v.as_object().is_some_and(|p| {
                p.len() == fields.len() && p.iter().all(|(k, _)| fields.contains(&k.as_str()))
            })
        };
        require(
            exact(value, &FIELDS),
            "Release manifest has unknown or missing fields",
        )?;
        require(
            value.str("schema") == Some("agent-memory/release-v2")
                && value.str("repository") == Some(REPOSITORY),
            "Wrong release schema or repository",
        )?;
        let version = value.str("version").unwrap_or("");
        let selected = value.str("tag").unwrap_or("");
        require(
            stable_version(version)
                && selected == format!("v{version}")
                && tag.is_none_or(|t| t == selected),
            "Release tag/version differs from the selected release",
        )?;
        let commit = value.str("source_commit").unwrap_or("");
        require(hex(commit, 40), "Invalid release source commit")?;
        let size =
            |v: Option<&Value>| v.and_then(Value::as_i64).filter(|n| *n > 0).unwrap_or(0) as usize;
        let archive_size = size(value.get("archive_size"));
        require(
            value.str("archive") == Some(ARCHIVE)
                && archive_size > 0
                && archive_size <= MAX_ARCHIVE,
            "Invalid release archive identity",
        )?;
        let archive_sha256 = value.str("archive_sha256").unwrap_or("");
        let artifact_digest = value.str("artifact_digest").unwrap_or("");
        require(
            hex(archive_sha256, 64) && hex(artifact_digest, 64),
            "Invalid release digest",
        )?;
        let map = value.get("targets").unwrap_or(&Value::Null);
        require(
            exact(map, &TARGETS),
            "Release targets differ from the supported hosts",
        )?;
        let mut targets = BTreeMap::new();
        for triple in TARGETS {
            let binary = map.get(triple).unwrap_or(&Value::Null);
            require(
                exact(binary, &["size", "sha256", "artifact_digest"]),
                "Unknown release binary fields",
            )?;
            let identity = BinaryIdentity {
                size: size(binary.get("size")),
                sha256: binary.str("sha256").unwrap_or("").into(),
                artifact_digest: binary.str("artifact_digest").unwrap_or("").into(),
            };
            require(
                identity.size > 0
                    && identity.size <= MAX_BINARY
                    && hex(&identity.sha256, 64)
                    && hex(&identity.artifact_digest, 64),
                "Invalid release binary identity",
            )?;
            targets.insert(triple.to_owned(), identity);
        }
        let surface = value.get("command_surface").cloned().unwrap_or(Value::Null);
        require(
            surface.as_object().is_some(),
            "Release command surface must be an object",
        )?;
        Ok(Manifest {
            version: version.into(),
            tag: selected.into(),
            source_commit: commit.into(),
            archive_size,
            archive_sha256: archive_sha256.into(),
            artifact_digest: artifact_digest.into(),
            targets,
            command_surface: surface,
        })
    }
}

/// A private temporary directory removed on drop.
pub struct Scratch(pub PathBuf);
impl Scratch {
    pub fn new(prefix: &str) -> Result<Scratch> {
        Scratch::under(&std::env::temp_dir(), prefix)
    }
    pub fn under(parent: &Path, prefix: &str) -> Result<Scratch> {
        use std::os::unix::fs::DirBuilderExt;
        let path = parent.join(format!("{prefix}{}", uuid4()));
        io_at(std::fs::DirBuilder::new().mode(0o700).create(&path), &path)?;
        Ok(Scratch(path))
    }
    /// Keep the directory (it was moved into place).
    pub fn keep(mut self) -> PathBuf {
        std::mem::take(&mut self.0)
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        if !self.0.as_os_str().is_empty() {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
}

/// Verify an SSH signature against the pinned release key.
pub fn verify_signature(
    raw: &[u8],
    signature: &[u8],
    namespace: &str,
    signers: &str,
) -> Result<()> {
    require(
        !signature.is_empty() && signature.len() <= MAX_SIGNATURE,
        "Release signature is empty or too large",
    )?;
    let scratch = Scratch::new("am-signature-")?;
    let signers_path = scratch.0.join("allowed-signers");
    let signature_path = scratch.0.join("signature");
    io_at(std::fs::write(&signers_path, signers), &signers_path)?;
    io_at(std::fs::write(&signature_path, signature), &signature_path)?;
    let output = run(
        "ssh-keygen",
        &[
            OsStr::new("-Y"),
            OsStr::new("verify"),
            OsStr::new("-f"),
            signers_path.as_os_str(),
            OsStr::new("-I"),
            OsStr::new(PRINCIPAL),
            OsStr::new("-n"),
            OsStr::new(namespace),
            OsStr::new("-s"),
            signature_path.as_os_str(),
        ],
        None,
        &[],
        Some(raw),
    )?;
    require(
        output.ok(),
        "Release signature is not from the pinned agent-memory release key",
    )
}

pub fn verify_manifest(
    raw: &[u8],
    signature: &[u8],
    tag: Option<&str>,
    signers: &str,
) -> Result<Manifest> {
    require(raw.len() <= MAX_MANIFEST, "Release manifest is too large")?;
    verify_signature(raw, signature, NAMESPACE, signers)?;
    Manifest::from_value(&json::parse(raw)?, tag)
}

/// Download `url` with curl: HTTPS only, bounded, three attempts for
/// transient failures.
pub fn download(url: &str, maximum: usize) -> Result<Vec<u8>> {
    require(
        url.starts_with("https://") || url.starts_with("file://"),
        "Release downloads require HTTPS",
    )?;
    let scratch = Scratch::new("am-download-")?;
    let body = scratch.0.join("body");
    let protocols = if url.starts_with("file://") {
        "=https,file"
    } else {
        "=https"
    };
    let limit = maximum.to_string();
    let mut last = String::new();
    for attempt in 1..=3 {
        let _ = std::fs::remove_file(&body);
        let args = [
            "--disable",
            "--fail",
            "--silent",
            "--show-error",
            "--location",
            "--proto",
            protocols,
            "--proto-redir",
            "=https",
            "--max-redirs",
            "5",
            "--max-time",
            "60",
            "--connect-timeout",
            "5",
            "--max-filesize",
            &limit,
            "--header",
            "Accept: application/octet-stream",
            "--output",
        ];
        let mut argv: Vec<&OsStr> = args.iter().map(OsStr::new).collect();
        argv.extend([body.as_os_str(), OsStr::new("--"), OsStr::new(url)]);
        let output = run("curl", &argv, None, &[], None)?;
        if output.ok() {
            let raw = io_at(std::fs::read(&body), &body)?;
            require(
                raw.len() <= maximum,
                "Release download exceeds its size bound",
            )?;
            return Ok(raw);
        }
        last = format!("curl exit {}: {}", output.code, output.error_text());
        // Retry DNS, connect, timeout, partial and server (5xx) failures.
        let transient = [5, 6, 7, 16, 18, 28, 52, 55, 56, 92].contains(&output.code)
            || (output.code == 22
                && ["500", "502", "503", "504", "408"]
                    .iter()
                    .any(|c| last.contains(c)));
        if !transient || attempt == 3 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(500 * attempt));
    }
    let name = url.rsplit('/').next().unwrap_or(url);
    bail!(
        "Could not download {name} ({last}). Nothing was installed; check the network or proxy and retry."
    )
}

/// The latest signed release: manifest, its signature and asset base URL.
pub struct Observation {
    pub manifest: Manifest,
    pub raw: Vec<u8>,
    pub signature: Vec<u8>,
    pub base: String,
}

pub fn probe() -> Result<Observation> {
    let raw = download(LATEST, MAX_MANIFEST)?;
    // Unverified, the first response only selects a well-formed tag.
    let value = json::parse(&raw)?;
    let version = value.str("version").unwrap_or("");
    require(
        stable_version(version),
        "Latest release has an invalid version",
    )?;
    let tag = format!("v{version}");
    let base = format!("{ASSETS}/{tag}/");
    let signature = download(&format!("{base}{SIGNATURE}"), MAX_SIGNATURE)?;
    let manifest = verify_manifest(&raw, &signature, Some(&tag), ALLOWED_SIGNERS)?;
    Ok(Observation {
        manifest,
        raw,
        signature,
        base,
    })
}

/// Portable files: relative path → (bytes, mode).
pub type Inventory = BTreeMap<String, (Vec<u8>, u32)>;

pub fn inventory_value(files: &Inventory) -> Value {
    Value::Object(
        files
            .iter()
            .map(|(name, (raw, mode))| {
                (
                    name.clone(),
                    obj([
                        ("sha256", s(sha256_hex(raw))),
                        ("mode", s(format!("0o{mode:o}"))),
                    ]),
                )
            })
            .collect(),
    )
}
pub fn inventory_digest(files: &Value) -> String {
    sha256_hex(&canonical(files))
}
/// Digest of the portable inventory plus `bin/am` (mode 0o755).
pub fn runtime_digest(files: &Value, binary_sha256: &str) -> String {
    let mut combined = files.clone();
    combined.set(
        "bin/am",
        obj([("sha256", s(binary_sha256)), ("mode", s("0o755"))]),
    );
    inventory_digest(&combined)
}

/// Read a pack directory as a portable inventory.
pub fn portable_files(root: &Path) -> Result<Inventory> {
    use std::os::unix::fs::PermissionsExt;
    let mut files = Inventory::new();
    let mut total = 0;
    let mut stack = vec![PathBuf::new()];
    while let Some(relative) = stack.pop() {
        let dir = root.join(&relative);
        for entry in io_at(std::fs::read_dir(&dir), &dir)? {
            let entry = entry?;
            let path = relative.join(entry.file_name());
            let meta = io_at(entry.path().symlink_metadata(), &entry.path())?;
            if meta.is_dir() {
                stack.push(path);
            } else if meta.is_file() {
                let mode = meta.permissions().mode() & 0o7777;
                require(
                    [0o644, 0o755].contains(&mode),
                    "Release files must have mode 0644 or 0755",
                )?;
                let raw = io_at(std::fs::read(entry.path()), &entry.path())?;
                total += raw.len();
                require(
                    raw.len() <= MAX_FILE && total <= MAX_BYTES && files.len() < MAX_ENTRIES,
                    "Release pack exceeds its size bound",
                )?;
                files.insert(path.to_string_lossy().into_owned(), (raw, mode));
            } else {
                bail!(
                    "Release pack holds a link or special file: {}",
                    path.display()
                );
            }
        }
    }
    Ok(files)
}

/// `(version, artifact digest, inventory)` of a pack: `SKILL.md` declares
/// the version and the pack carries the pinned key.
pub fn package_identity(files: &Inventory) -> Result<(String, String, Value)> {
    let skill = files
        .get("SKILL.md")
        .ok_or_else(|| Error::new("Release pack lacks SKILL.md"))?;
    require(
        files
            .get("keys/allowed-signers")
            .is_some_and(|(raw, _)| raw == ALLOWED_SIGNERS.as_bytes()),
        "Release pack key differs from the pinned publisher",
    )?;
    let text = String::from_utf8_lossy(&skill.0);
    let declared: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("metadata: "))
        .collect();
    let [declaration] = declared.as_slice() else {
        bail!("SKILL.md needs exactly one `metadata:` version line");
    };
    let version = json::parse(declaration.as_bytes())?
        .str("version")
        .unwrap_or("")
        .to_owned();
    require(
        stable_version(&version),
        "SKILL.md declares an invalid version",
    )?;
    let inventory = inventory_value(files);
    Ok((version, inventory_digest(&inventory), inventory))
}

/// Parse a strict USTAR archive of `skills/agent-memory/` regular files.
pub fn tar_entries(expanded: &[u8]) -> Result<Inventory> {
    require(
        expanded.len() <= MAX_TAR && expanded.len() % 512 == 0,
        "Invalid release archive",
    )?;
    let mut files = Inventory::new();
    let mut cursor = 0;
    let mut total = 0;
    while cursor + 512 <= expanded.len() {
        let header = &expanded[cursor..cursor + 512];
        if header.iter().all(|b| *b == 0) {
            return Ok(files);
        }
        let field = |range: std::ops::Range<usize>| {
            let bytes = &header[range];
            let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
            String::from_utf8_lossy(&bytes[..end]).into_owned()
        };
        let number = |range| usize::from_str_radix(field(range).trim(), 8).unwrap_or(usize::MAX);
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
            number(148..156) == sum && &header[257..263] == b"ustar\0",
            "Invalid release archive",
        )?;
        let prefix = field(345..500);
        let leaf = field(0..100);
        let name = if prefix.is_empty() {
            leaf
        } else {
            format!("{prefix}/{leaf}")
        };
        let kind = header[156];
        let mode = number(100..108) as u32;
        let size = number(124..136);
        cursor += 512;
        require(
            size <= MAX_FILE
                && cursor
                    .checked_add(size)
                    .is_some_and(|end| end <= expanded.len()),
            "Release archive member is too large",
        )?;
        if kind == b'5' {
            cursor += size.div_ceil(512) * 512;
            continue;
        }
        let relative = name.strip_prefix("skills/agent-memory/").unwrap_or("");
        require(
            [0, b'0'].contains(&kind)
                && [0o644, 0o755].contains(&mode)
                && !relative.is_empty()
                && relative
                    .split('/')
                    .all(|p| !p.is_empty() && p != "." && p != "..")
                && !files.contains_key(relative),
            "Unsafe or duplicate release archive member",
        )?;
        total += size;
        require(
            total <= MAX_BYTES && files.len() < MAX_ENTRIES,
            "Release archive is too large",
        )?;
        files.insert(
            relative.to_owned(),
            (expanded[cursor..cursor + size].to_vec(), mode),
        );
        cursor += size.div_ceil(512) * 512;
    }
    Err(Error::new("Invalid release archive"))
}

/// A deterministic gzip USTAR archive of the pack under `skills/agent-memory/`.
pub fn encode_archive(files: &Inventory) -> Result<Vec<u8>> {
    let mut tar = Vec::new();
    for (relative, (raw, mode)) in files {
        let name = format!("{PACK}/{relative}");
        let (prefix, leaf) = if name.len() <= 100 {
            ("", name.as_str())
        } else {
            name.rsplit_once('/')
                .ok_or_else(|| Error::new("Release path is too long"))?
        };
        require(
            prefix.len() <= 155 && leaf.len() <= 100,
            "Release path is too long",
        )?;
        let mut header = [0u8; 512];
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
    let mut gzip = flate2::GzBuilder::new()
        .mtime(0)
        .write(Vec::new(), flate2::Compression::default());
    gzip.write_all(&tar)?;
    Ok(gzip.finish()?)
}

/// Write an inventory under `root` (directories 0755).
pub fn write_files(root: &Path, files: &Inventory) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    for (relative, (raw, mode)) in files {
        let path = root.join(relative);
        if let Some(parent) = path.parent() {
            io_at(std::fs::create_dir_all(parent), parent)?;
        }
        io_at(std::fs::write(&path, raw), &path)?;
        io_at(
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(*mode)),
            &path,
        )?;
    }
    Ok(())
}

/// Check the archive against the manifest and unpack its pack into `root/skills/agent-memory`.
pub fn extract(raw: &[u8], root: &Path, manifest: &Manifest) -> Result<()> {
    require(
        raw.len() == manifest.archive_size && sha256_hex(raw) == manifest.archive_sha256,
        "Release archive differs from its signed manifest",
    )?;
    let mut expanded = Vec::new();
    flate2::read::GzDecoder::new(raw)
        .take((MAX_TAR + 1) as u64)
        .read_to_end(&mut expanded)
        .map_err(|_| Error::new("Release archive is not valid gzip"))?;
    let files = tar_entries(&expanded)?;
    let (version, digest, _) = package_identity(&files)?;
    require(
        version == manifest.version && digest == manifest.artifact_digest,
        "Release pack differs from its signed identity",
    )?;
    write_files(&root.join(PACK), &files)
}

/// Stage a complete runtime root: signed manifest, pack and the verified `bin/am`.
pub fn stage(root: &Path, observed: &Observation, binary: &[u8]) -> Result<()> {
    let archive = download(
        &format!("{}{ARCHIVE}", observed.base),
        observed.manifest.archive_size,
    )?;
    extract(&archive, root, &observed.manifest)?;
    for (name, raw) in [(MANIFEST, &observed.raw), (SIGNATURE, &observed.signature)] {
        let path = root.join(name);
        io_at(std::fs::write(&path, raw), &path)?;
    }
    write_files(
        root,
        &BTreeMap::from([(BINARY.to_owned(), (binary.to_vec(), 0o755))]),
    )
}

/// The verified bootstrap binary's bytes, bound to the signed manifest.
pub fn bind_bootstrap(manifest: &Manifest, binary: &Path) -> Result<Vec<u8>> {
    require(
        manifest.version == VERSION,
        "The bootstrap binary's version differs from the latest signed release; re-run install.sh",
    )?;
    let target = manifest
        .targets
        .get(TARGET)
        .ok_or_else(|| Error::new("No signed release exists for this host"))?;
    let raw = io_at(std::fs::read(binary), binary)?;
    require(
        raw.len() == target.size && sha256_hex(&raw) == target.sha256,
        "The bootstrap binary differs from its signed release; re-run install.sh",
    )?;
    Ok(raw)
}

/// Download and verify this host's signed binary for an update.
pub fn fetch_binary(observed: &Observation) -> Result<Vec<u8>> {
    let target = observed
        .manifest
        .targets
        .get(TARGET)
        .ok_or_else(|| Error::new("No signed release exists for this host"))?;
    let name = format!("am-{TARGET}");
    let raw = download(&format!("{}{name}", observed.base), target.size)?;
    let signature = download(&format!("{}{name}.sshsig", observed.base), MAX_SIGNATURE)?;
    verify_signature(&raw, &signature, BINARY_NAMESPACE, ALLOWED_SIGNERS)?;
    require(
        raw.len() == target.size && sha256_hex(&raw) == target.sha256,
        "Downloaded binary differs from its signed manifest",
    )?;
    Ok(raw)
}

/// Verify an installed runtime root against its stored signed manifest.
pub fn installed(root: &Path) -> Result<Manifest> {
    let read = |name: &str| io_at(std::fs::read(root.join(name)), &root.join(name));
    let manifest = verify_manifest(&read(MANIFEST)?, &read(SIGNATURE)?, None, ALLOWED_SIGNERS)?;
    let (version, digest, files) = package_identity(&portable_files(&root.join(PACK))?)?;
    require(
        version == manifest.version && digest == manifest.artifact_digest,
        "The installed skill pack differs from its signed release",
    )?;
    let binary = read(BINARY)?;
    let target = manifest
        .targets
        .get(TARGET)
        .ok_or_else(|| Error::new("No signed release exists for this host"))?;
    let sha = sha256_hex(&binary);
    require(
        binary.len() == target.size
            && sha == target.sha256
            && runtime_digest(&files, &sha) == target.artifact_digest,
        "The installed am binary differs from its signed release",
    )?;
    Ok(manifest)
}
