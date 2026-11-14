//! Bounded raw evidence for layout recovery; never a normal bank snapshot.
use crate::codec::json::Value;
use crate::developer_identity::{digest, encoded, uuid_valid};
use crate::error::{ContractError, Kind, Result};
use crate::fs::{self, Descriptor, DirectoryChain, Identity, ReadError, ReadOptions};
use crate::{artifact, bank_contract as contract, bank_observation, bank_writes as writes};
use crate::{host, identity_git, layout_recovery_state as state, local_registry};
use crate::{project_identity, repository_config, scope_binding, sharing_limits};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};

pub const MAX_FILE: usize = 1_048_576;
pub const MAX_BYTES: usize = 67_108_864;
pub const MAX_FILES: usize = 65_536;
pub const MAX_ENTRIES: usize = 131_072;
pub const MAX_DEPTH: usize = 64;
pub const CHUNK: usize = 8192;
/// Explicit output budget for local Git identity/configuration evidence.
pub const MAX_GIT_OUTPUT: usize = 64 * 1024 * 1024;
pub const METADATA_POLICY: &str =
    "ordinary-modes-ownership-darwin-provenance-exact-no-acl-other-xattr-flags";
pub const SIGNATURE: &[&str] = &[
    "dev", "ino", "mode", "uid", "gid", "nlink", "size", "mtime_ns", "ctime_ns",
];

fn s(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn n(value: impl Into<crate::codec::json::Integer>) -> Value {
    Value::Int(value.into())
}
fn object<const N: usize>(pairs: [(&str, Value); N]) -> Value {
    Value::Object(pairs.into_iter().map(|(k, v)| (k.to_owned(), v)).collect())
}
fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn io_err(error: std::io::Error) -> ContractError {
    err(&error.to_string())
}
fn field<'a>(value: &'a Value, name: &str) -> Result<&'a Value> {
    value
        .get(name)
        .ok_or_else(|| err("Recovery manifest location/completeness changed"))
}
fn text(path: &Path) -> Result<&str> {
    path.to_str()
        .ok_or_else(|| err("Unsafe raw-source path; preserve the physical bank"))
}
fn merge(mut value: Value, fields: impl IntoIterator<Item = (String, Value)>) -> Value {
    if let Value::Object(pairs) = &mut value {
        pairs.extend(fields);
    }
    value
}
fn equal(a: &Value, b: &Value) -> Result<bool> {
    Ok(match (a, b) {
        (Value::Object(left), Value::Object(right)) => {
            left.len() == right.len()
                && left.iter().all(|(key, value)| {
                    b.get(key)
                        .is_some_and(|other| equal(value, other).is_ok_and(|same| same))
                })
        }
        (Value::Array(left), Value::Array(right)) => {
            left.len() == right.len()
                && left
                    .iter()
                    .zip(right)
                    .all(|(a, b)| equal(a, b).is_ok_and(|same| same))
        }
        _ => a == b,
    })
}
fn capacity(dimension: &str, limit: usize, measured: u64, exact: bool) -> ContractError {
    ContractError::raw_recovery_capacity(dimension, limit, measured, exact)
}
/// The baseline RawCapacityError.details, without losing the typed error at seams.
pub fn capacity_details(error: &ContractError) -> Option<Value> {
    let Kind::RawRecoveryCapacity {
        dimension,
        limit,
        measured,
        exact,
    } = error.kind()
    else {
        return None;
    };
    Some(object([
        ("dimension", s(dimension)),
        ("limit", n(*limit)),
        ("measured", n(*measured)),
        (
            "measurement",
            s(if *exact { "exact" } else { "lower-bound" }),
        ),
        ("observation_complete", Value::Bool(false)),
    ]))
}
pub fn signature(info: &Identity) -> Value {
    object([
        ("dev", n(info.dev)),
        ("ino", n(info.ino)),
        ("mode", n(info.mode)),
        ("uid", n(info.uid)),
        ("gid", n(info.gid)),
        ("nlink", n(info.nlink)),
        ("size", n(info.size)),
        ("mtime_ns", n(info.mtime_ns)),
        ("ctime_ns", n(info.ctime_ns)),
    ])
}
pub fn path_value(relative: &str) -> Result<&str> {
    if relative.is_empty()
        || relative.len() > 4096
        || relative.starts_with('/')
        || relative
            .split('/')
            .any(|p| p.is_empty() || p == "." || p == "..")
        || relative
            .chars()
            .any(|c| c < ' ' || matches!(c, '\\' | '\u{7f}' | '\u{85}' | '\u{2028}' | '\u{2029}'))
    {
        return Err(err("Unsafe raw-source path; preserve the physical bank"));
    }
    Ok(relative)
}
pub fn valid_xattrs(value: &Value) -> bool {
    value.as_object().is_some_and(|pairs| {
        pairs.iter().all(|(name, value)| {
            name == "com.apple.provenance"
                && value.as_str().is_some_and(|body| {
                    body.len() <= 32768
                        && body.len() % 2 == 0
                        && body
                            .bytes()
                            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                })
        })
    })
}
pub fn inode_flags(descriptor: &Descriptor) -> Result<u32> {
    fs::inode_flags(descriptor).map_err(io_err)
}
pub fn metadata_supported(path: &Path) -> Result<Value> {
    Ok(Value::Object(
        fs::metadata_supported(path)?
            .into_iter()
            .map(|(k, v)| (k, s(v)))
            .collect(),
    ))
}
fn read_error(error: ReadError) -> ContractError {
    match error {
        ReadError::NotRegular | ReadError::MultipleLinks => {
            err("Recovery requires ordinary single-link sources")
        }
        ReadError::SizeLimit {
            limit,
            observed,
            exact,
        } => capacity("source-bytes", limit, observed, exact),
        ReadError::SourceChanged => err("Raw source changed during observation"),
        ReadError::ParentChanged => err("Raw source parent changed during observation"),
        _ => err("Raw source binding/type/permissions changed; preserve evidence"),
    }
}
pub fn read_source(bank: &Path, relative: &str, limit: usize) -> Result<(Vec<u8>, Value)> {
    path_value(relative)?;
    let observed = fs::read_source_with_options(
        bank,
        Path::new(relative),
        limit,
        ReadOptions::LAYOUT_RECOVERY,
    )
    .map_err(read_error)?;
    Ok((observed.bytes, signature(&observed.identity)))
}
/// Hash the original Git index at its exact observed size, never allocating its contents.
pub fn read_source_hash(bank: &Path, relative: &str, limit: usize) -> Result<(String, Value)> {
    path_value(relative)?;
    let observe = || -> std::result::Result<(String, Identity), ReadError> {
        let relative = Path::new(relative);
        let parent = relative.parent().unwrap_or_else(|| Path::new(""));
        let name = relative.file_name().ok_or(ReadError::InvalidPath)?;
        let parents = DirectoryChain::open(&bank.join(parent))?;
        let mut file = parents
            .directory()
            .open_at(name, libc::O_RDONLY | libc::O_NONBLOCK, 0)?;
        let before = file.stat()?;
        if !before.is_regular() {
            return Err(ReadError::NotRegular);
        }
        if before.nlink != 1 {
            return Err(ReadError::MultipleLinks);
        }
        if before.size < 0 {
            return Err(ReadError::InvalidSize);
        }
        if before.size as u128 > limit as u128 {
            return Err(ReadError::SizeLimit {
                limit,
                observed: before.size as u64,
                exact: true,
            });
        }
        let maximum = limit.checked_add(1).ok_or(ReadError::LimitOverflow)?;
        let mut measured = 0;
        let mut block = [0u8; CHUNK];
        let mut hasher = Sha256::new();
        loop {
            let count = CHUNK.min(maximum - measured);
            let buffer = block.get_mut(..count).ok_or(ReadError::LimitOverflow)?;
            let amount = file.read_chunk(buffer)?;
            if amount == 0 {
                break;
            }
            measured += amount;
            if measured > limit {
                return Err(ReadError::SizeLimit {
                    limit,
                    observed: measured as u64,
                    exact: false,
                });
            }
            hasher.update(block.get(..amount).ok_or(ReadError::LimitOverflow)?);
        }
        let after = file.stat()?;
        if before != after
            || after != parents.directory().stat_at(name)?
            || measured as u128 != before.size as u128
        {
            return Err(ReadError::SourceChanged);
        }
        parents
            .recheck_full_from(bank)
            .map_err(|_| ReadError::ParentChanged)?;
        Ok((crate::validation_memo::hex(&hasher.finalize()), before))
    };
    let (hash, facts) = observe().map_err(read_error)?;
    Ok((hash, signature(&facts)))
}
pub struct LayoutObservation;
impl project_identity::LayoutObservation for LayoutObservation {
    fn read_source(&self, bank: &Path, relative: &Path, limit: usize) -> Result<Vec<u8>> {
        read_source(bank, text(relative)?, limit).map(|(body, _)| body)
    }
}
fn named(path: &Path) -> Result<Option<Identity>> {
    let Some(parent) = path.parent() else {
        return Err(err("Raw path has no parent"));
    };
    let Some(name) = path.file_name() else {
        return Err(err("Raw path has no name"));
    };
    let chain = DirectoryChain::open(parent).map_err(io_err)?;
    let result = match chain.directory().stat_at(name) {
        Ok(info) => Ok(Some(info)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(io_err(error)),
    };
    chain.recheck().map_err(io_err)?;
    result
}
pub fn locate(repo: &Path) -> Result<(PathBuf, PathBuf)> {
    let root = fs::canonical_path(repo, false)?;
    let bank = root.join(repository_config::recovery_settings(&root)?.bank_root);
    contract::bank_root(&bank)?;
    if !named(&bank)?.is_some_and(|info| info.is_directory()) {
        return Err(err("Recovery requires the existing configured bank"));
    }
    Ok((root, bank))
}
fn git(root: &Path, args: &[&str], optional: bool) -> Result<Option<String>> {
    let invocation = identity_git::_invocation(
        root,
        &args.iter().map(OsString::from).collect::<Vec<_>>(),
        false,
    );
    let spec = crate::process::Spec {
        argv: &invocation.argv,
        env: &invocation.env,
        cwd: None,
        search_path: true,
        stdin: crate::process::Input::Inherit,
        stdout: crate::process::Target::Capture {
            limit: MAX_GIT_OUTPUT,
        },
        stderr: crate::process::Target::Capture {
            limit: MAX_GIT_OUTPUT,
        },
        deadline: crate::clock::Deadline::from_absolute_ns(u64::MAX),
        new_session: false,
        file_size_limit: None,
        umask: None,
        pass_fds: &[],
    };
    identity_git::_git_output(
        crate::process::run_for(
            &spec,
            std::time::Duration::from_secs_f64(identity_git::_QUERY_TIMEOUT),
        ),
        optional,
    )
    .map_err(|error| err(&error.to_string()))?
    .map(|output| {
        output
            .into_string()
            .map_err(|_| err("Original Git pins must be UTF-8"))
    })
    .transpose()
}
fn required_git(root: &Path, args: &[&str]) -> Result<String> {
    git(root, args, false)?.ok_or_else(|| err("Original Git pins are unavailable"))
}
pub fn git_pins(root: &Path) -> Result<Value> {
    let Some(common) = git(root, &["rev-parse", "--git-common-dir"], true)? else {
        return Ok(object([
            ("head", Value::Null),
            ("branch", Value::Null),
            ("index", Value::Null),
            ("configuration", Value::Null),
            ("common_git_dir", Value::Null),
            ("common_identity", Value::Null),
        ]));
    };
    let common_path = fs::canonical_path(&root.join(common), false)?;
    let index_name = required_git(root, &["rev-parse", "--git-path", "index"])?;
    let mut index_path = PathBuf::new();
    for part in root.join(index_name).components() {
        match part {
            std::path::Component::ParentDir => {
                index_path.pop();
            }
            std::path::Component::CurDir => {}
            _ => index_path.push(part.as_os_str()),
        }
    }
    let ancestors: Vec<_> = index_path.ancestors().collect();
    for path in ancestors.into_iter().rev() {
        if path.parent().is_some()
            && named(path)?
                .is_some_and(|info| info.mode & libc::S_IFMT as u32 == libc::S_IFLNK as u32)
        {
            return Err(err(
                "Original Git index must not follow redirected bindings",
            ));
        }
    }
    let (index_sha, facts) = if let Some(info) = named(&index_path)? {
        let size = usize::try_from(info.size)
            .map_err(|_| err("Original Git index has an invalid size"))?;
        let parent = index_path
            .parent()
            .ok_or_else(|| err("Original Git index has no parent"))?;
        let name = index_path
            .file_name()
            .ok_or_else(|| err("Original Git index has no name"))?;
        let (hash, facts) = read_source_hash(parent, text(Path::new(name))?, size)?;
        (s(hash), facts)
    } else {
        (Value::Null, Value::Null)
    };
    let common_info = Descriptor::open_directory(&common_path)
        .map_err(io_err)?
        .stat()
        .map_err(io_err)?;
    let configuration = required_git(root, &["config", "--null", "--list"])?;
    Ok(object([
        (
            "head",
            git(root, &["rev-parse", "--verify", "HEAD"], true)?.map_or(Value::Null, s),
        ),
        (
            "branch",
            git(root, &["symbolic-ref", "-q", "HEAD"], true)?.map_or(Value::Null, s),
        ),
        (
            "index",
            object([
                ("path", s(text(&index_path)?)),
                ("sha256", index_sha),
                ("facts", facts),
            ]),
        ),
        (
            "configuration",
            digest(Some(configuration.as_bytes())).map_or(Value::Null, s),
        ),
        ("common_git_dir", s(text(&common_path)?)),
        (
            "common_identity",
            object([("dev", n(common_info.dev)), ("ino", n(common_info.ino))]),
        ),
    ]))
}
pub fn identity(
    repo: &Path,
    raw: &[u8],
    review: Option<&Value>,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<Value> {
    let token = project_identity::RawIdentityEvidence::observe(repo, raw, review)?;
    let context = scope_binding::raw_recovery_context(repo, &token, owner, transaction)?;
    let project = project_identity::status_with_options(
        repo,
        None,
        project_identity::DiscoveryOptions {
            owned_lock: owner,
            owned_transaction: transaction,
            raw_identity: Some(&token),
            migration_reader: None,
        },
    )?;
    if named(&local_registry::paths("projects")?.2)?.is_some()
        || project.get("resolved").and_then(Value::as_bool) != Some(true)
    {
        return Err(err("pending-identity-review"));
    }
    Ok(object([
        ("scope_context", context),
        ("project_evidence", field(&project, "evidence")?.clone()),
        (
            "identity_review",
            token.review().cloned().unwrap_or(Value::Null),
        ),
    ]))
}
#[derive(Debug, Default, Clone, Copy)]
pub struct Totals {
    pub source_bytes: usize,
    pub source_files: usize,
    pub physical_entries: usize,
    pub depth: usize,
}
impl Totals {
    pub fn value(&self) -> Value {
        object([
            ("source_bytes", n(self.source_bytes)),
            ("source_files", n(self.source_files)),
            ("physical_entries", n(self.physical_entries)),
            ("depth", n(self.depth)),
        ])
    }
}
/// Exact baseline indent-2 nested accounting, checked before collecting each row.
pub struct ManifestBudget {
    fixed: usize,
    rows: usize,
    physical: usize,
    sources: usize,
}
impl ManifestBudget {
    pub fn new(envelope: &Value, totals: &Value) -> Result<Self> {
        let mut pairs = envelope
            .as_object()
            .ok_or_else(|| err("Recovery envelope must be an object"))?
            .to_vec();
        pairs.retain(|(k, _)| !["physical_entries", "sources", "totals"].contains(&k.as_str()));
        pairs.extend([
            ("physical_entries".into(), Value::Array(vec![])),
            ("sources".into(), Value::Array(vec![])),
            ("totals".into(), object([])),
        ]);
        let budget = Self {
            fixed: encoded(&Value::Object(pairs))?.len(),
            rows: 0,
            physical: 0,
            sources: 0,
        };
        budget.check(totals)?;
        Ok(budget)
    }
    pub fn measured(&self, totals: &Value) -> Result<usize> {
        let totals = encoded(totals)?;
        let lines = totals.iter().filter(|b| **b == b'\n').count();
        Ok(self.fixed + self.rows + totals.len() - 1 + 2 * (lines - 1) - 2)
    }
    pub fn check(&self, totals: &Value) -> Result<()> {
        let measured = self.measured(totals)?;
        if measured > sharing_limits::MAX_PRIVATE {
            return Err(capacity(
                "encoded-manifest-bytes",
                sharing_limits::MAX_PRIVATE,
                measured as u64,
                false,
            ));
        }
        Ok(())
    }
    pub fn add(&mut self, kind: &str, row: &Value, totals: &Value) -> Result<()> {
        let body = encoded(row)?;
        let count = match kind {
            "physical_entries" => &mut self.physical,
            "sources" => &mut self.sources,
            _ => return Err(err("Invalid manifest row kind")),
        };
        self.rows += body.len() - 1
            + 4 * body.iter().filter(|b| **b == b'\n').count()
            + if *count == 0 { 4 } else { 2 };
        *count += 1;
        self.check(totals)
    }
}
#[derive(Default)]
pub struct InventoryOptions<'a> {
    pub retain_directory: Option<&'a Path>,
    pub prior_sources: Option<&'a [Value]>,
    pub allow_temporaries: bool,
    pub envelope: Option<&'a Value>,
    pub ignored_paths: &'a [&'a str],
}
#[derive(Debug)]
pub struct Inventory {
    pub physical_entries: Vec<Value>,
    pub sources: Vec<Value>,
    pub totals: Value,
}
struct Walker<'a> {
    bank: &'a Path,
    options: InventoryOptions<'a>,
    old: BTreeMap<&'a str, &'a Value>,
    physical: Vec<Value>,
    sources: Vec<Value>,
    totals: Totals,
    budget: ManifestBudget,
}
impl Walker<'_> {
    fn walk(&mut self, descriptor: &Descriptor, prefix: &str, depth: usize) -> Result<()> {
        let before = descriptor.stat().map_err(io_err)?;
        // At most one root lock is omitted from the physical-entry allowance.
        let allowance = MAX_ENTRIES - self.totals.physical_entries + usize::from(prefix.is_empty());
        let names = descriptor
            .entries_bounded(allowance)
            .map_err(io_err)?
            .ok_or_else(|| {
                capacity(
                    "physical-entries",
                    MAX_ENTRIES,
                    (MAX_ENTRIES + 1) as u64,
                    false,
                )
            })?;
        for name in &names {
            if prefix.is_empty() && name == OsStr::new(".agent-memory.lock") {
                continue;
            }
            self.totals.physical_entries += 1;
            if self.totals.physical_entries > MAX_ENTRIES {
                return Err(capacity(
                    "physical-entries",
                    MAX_ENTRIES,
                    self.totals.physical_entries as u64,
                    false,
                ));
            }
            self.budget.check(&self.totals.value())?;
        }
        for name in &names {
            if prefix.is_empty() && name == OsStr::new(".agent-memory.lock") {
                continue;
            }
            let relative = format!("{prefix}{}", text(Path::new(name))?);
            path_value(&relative)?;
            if self.options.ignored_paths.contains(&relative.as_str()) {
                continue;
            }
            let level = depth + 1;
            if level > MAX_DEPTH {
                return Err(capacity("depth", MAX_DEPTH, level as u64, false));
            }
            self.totals.depth = self.totals.depth.max(level);
            self.budget.check(&self.totals.value())?;
            let info = descriptor.stat_at(name).map_err(io_err)?;
            let kind = if info.is_directory() {
                "directory"
            } else if info.is_regular() {
                "file"
            } else {
                "unsupported"
            };
            if kind == "unsupported" || kind == "file" && info.nlink != 1 {
                return Err(err("Unsupported raw source type/alias; preserve the bank"));
            }
            let xattrs = metadata_supported(&self.bank.join(&relative))?;
            let runtime = contract::runtime_path(&relative);
            if runtime
                && relative.split('/').any(|part| {
                    part.starts_with(".agent-memory-layout-")
                        || part.starts_with(".agent-memory-probe-")
                })
                && !self.options.allow_temporaries
            {
                return Err(err(
                    "Unknown layout staging artifact requires its known operation recovery",
                ));
            }
            if runtime
                && (relative.contains("transaction") || relative.contains("recovery"))
                && ![".agent-memory-transaction.md", ".agent-memory-recovery.md"]
                    .contains(&relative.as_str())
            {
                return Err(err("Unknown runtime recovery artifact"));
            }
            let mut row = merge(
                object([
                    ("path", s(&relative)),
                    ("type", s(kind)),
                    ("runtime", Value::Bool(runtime)),
                    ("xattrs", xattrs),
                ]),
                signature(&info).as_object().unwrap_or(&[]).iter().cloned(),
            );
            if kind == "directory" {
                let child = descriptor.open_directory_at(name).map_err(io_err)?;
                if child.stat().map_err(io_err)? != info {
                    return Err(err("Raw directory binding changed"));
                }
                self.budget
                    .add("physical_entries", &row, &self.totals.value())?;
                self.physical.push(row);
                self.walk(&child, &format!("{relative}/"), level)?;
            } else {
                let (body, facts) = read_source(self.bank, &relative, MAX_FILE)?;
                if facts != signature(&info) {
                    return Err(err("Raw named source changed"));
                }
                let hash =
                    digest(Some(&body)).ok_or_else(|| err("Raw source digest unavailable"))?;
                row = merge(row, [("sha256".into(), s(&hash))]);
                self.budget
                    .add("physical_entries", &row, &self.totals.value())?;
                self.physical.push(row);
                if !runtime {
                    self.totals.source_files += 1;
                    self.totals.source_bytes += body.len();
                    if self.totals.source_files > MAX_FILES {
                        return Err(capacity(
                            "source-files",
                            MAX_FILES,
                            self.totals.source_files as u64,
                            false,
                        ));
                    }
                    if self.totals.source_bytes > MAX_BYTES {
                        return Err(capacity(
                            "aggregate-bytes",
                            MAX_BYTES,
                            self.totals.source_bytes as u64,
                            false,
                        ));
                    }
                    let source_id = match self.old.get(relative.as_str()) {
                        Some(row) => field(row, "source_id")?.clone(),
                        None => s(host::uuid4()?),
                    };
                    if !uuid_valid(&source_id) {
                        return Err(err("Invalid prior raw source identifier"));
                    }
                    let identifier = source_id
                        .as_str()
                        .ok_or_else(|| err("Invalid prior raw source identifier"))?;
                    let blob = format!("originals/{identifier}.bin");
                    let source = object([
                        ("source_id", source_id),
                        ("path", s(relative)),
                        ("byte_count", n(body.len())),
                        ("sha256", s(hash)),
                        ("blob", s(&blob)),
                    ]);
                    self.budget.add("sources", &source, &self.totals.value())?;
                    if let Some(directory) = self.options.retain_directory {
                        state::put_bytes(&directory.join(blob), &body)?;
                    }
                    self.sources.push(source);
                    if self.options.retain_directory.is_some() {
                        writes::fault("layout-recovery-partial-original-staging")?;
                    }
                }
            }
        }
        if descriptor.stat().map_err(io_err)? != before
            || descriptor
                .entries_bounded(names.len())
                .map_err(io_err)?
                .as_ref()
                != Some(&names)
        {
            return Err(err("Raw directory changed during inventory"));
        }
        Ok(())
    }
}
pub fn inventory(bank: &Path) -> Result<Inventory> {
    inventory_with_options(bank, InventoryOptions::default())
}
pub fn inventory_with_options<'a>(
    bank: &'a Path,
    options: InventoryOptions<'a>,
) -> Result<Inventory> {
    let totals = Totals::default();
    let budget = ManifestBudget::new(options.envelope.unwrap_or(&object([])), &totals.value())?;
    metadata_supported(bank)?;
    let mut old = BTreeMap::new();
    for row in options.prior_sources.unwrap_or(&[]) {
        let path = field(row, "path")?
            .as_str()
            .ok_or_else(|| err("Invalid prior raw source path"))?;
        old.insert(path, row);
    }
    let parents = DirectoryChain::open(bank).map_err(io_err)?;
    let descriptor = parents.directory().reopen_directory().map_err(io_err)?;
    let before = descriptor.stat().map_err(io_err)?;
    let mut walker = Walker {
        bank,
        options,
        old,
        physical: vec![],
        sources: vec![],
        totals,
        budget,
    };
    walker.walk(&descriptor, "", 0)?;
    parents
        .recheck()
        .map_err(|_| err("Bank binding changed during inventory"))?;
    if !descriptor.stat().map_err(io_err)?.same_inode(before) {
        return Err(err("Bank binding changed during inventory"));
    }
    contract::portable_aliases(
        walker
            .physical
            .iter()
            .filter_map(|row| row.get("path").and_then(Value::as_str)),
    )?;
    walker.physical.sort_by(|a, b| {
        a.get("path")
            .and_then(Value::as_str)
            .cmp(&b.get("path").and_then(Value::as_str))
    });
    walker.sources.sort_by(|a, b| {
        a.get("path")
            .and_then(Value::as_str)
            .cmp(&b.get("path").and_then(Value::as_str))
    });
    Ok(Inventory {
        physical_entries: walker.physical,
        sources: walker.sources,
        totals: walker.totals.value(),
    })
}
fn bank_binding(bank: &Path) -> Result<Value> {
    let info = Descriptor::open_directory(bank)
        .map_err(io_err)?
        .stat()
        .map_err(io_err)?;
    Ok(object([
        ("dev", n(info.dev)),
        ("ino", n(info.ino)),
        ("mode", n(info.mode)),
        ("uid", n(info.uid)),
        ("gid", n(info.gid)),
        ("xattrs", metadata_supported(bank)?),
    ]))
}
#[derive(Clone, Copy)]
pub struct ObserveOptions<'a> {
    pub owner: Option<&'a [u8]>,
    pub transaction: Option<&'a [u8]>,
    pub retain: bool,
    pub identity_review: Option<&'a Value>,
    pub predecessor: Option<&'a Value>,
}
impl Default for ObserveOptions<'_> {
    fn default() -> Self {
        Self {
            owner: None,
            transaction: None,
            retain: true,
            identity_review: None,
            predecessor: None,
        }
    }
}
/// Both executing runtime root and binary are explicit; never infer authority from __file__ or ambient globals.
pub fn observe(
    repo: &Path,
    operation_id: &str,
    runtime_root: &Path,
    runtime_binary: &Path,
    options: ObserveOptions<'_>,
) -> Result<Value> {
    if !uuid_valid(&s(operation_id)) {
        return Err(err("Recovery operation requires a canonical UUIDv4"));
    }
    let (root, bank) = locate(repo)?;
    bank_observation::markers(&bank, options.owner, options.transaction)?;
    if options.transaction.is_none() && named(&bank.join(".agent-memory-recovery.md"))?.is_some() {
        return Err(err(
            "Known recovery must use its operation-specific interface",
        ));
    }
    let (router, _) = read_source(&bank, "MEMORY.md", MAX_FILE)?;
    let resolved = identity(
        &root,
        &router,
        options.identity_review,
        options.owner,
        options.transaction,
    )?;
    let directory = if options.retain {
        state::paths(&root, Some(operation_id), runtime_root)?.3
    } else {
        None
    };
    if let Some(directory) = &directory {
        state::write_once(
            &directory.join("staging.json"),
            &object([
                ("schema", s("agent-memory/layout-recovery-staging-v1")),
                ("operation_id", s(operation_id)),
                ("repo_root", s(text(&root)?)),
                ("bank_path", s(text(&bank)?)),
                (
                    "project_id",
                    field(field(&resolved, "scope_context")?, "project_id")?.clone(),
                ),
                ("identity", resolved.clone()),
                ("observation_complete", Value::Bool(false)),
            ]),
        )?;
    }
    let before_git = git_pins(&root)?;
    let measured =
        artifact::inspect_runtime(&runtime_root.join("skills/agent-memory"), runtime_binary)
            .map_err(|error| err(&error.to_string()))?;
    let runtime = object([
        (
            "artifact_digest",
            field(&measured, "artifact_digest")?.clone(),
        ),
        ("version", field(&measured, "version")?.clone()),
        (
            "private_state_capabilities",
            field(&measured, "compatibility")?
                .get("private_state_capabilities")
                .cloned()
                .unwrap_or(Value::Array(vec![])),
        ),
    ]);
    let envelope = object([
        ("schema", s("agent-memory/layout-source-manifest-v1")),
        ("operation_id", s(operation_id)),
        (
            "inspected_at",
            s(local_registry::now(host::Sources::default())?),
        ),
        ("repo_root", s(text(&root)?)),
        ("bank_path", s(text(&bank)?)),
        (
            "project_id",
            field(field(&resolved, "scope_context")?, "project_id")?.clone(),
        ),
        ("identity", resolved.clone()),
        ("observation_complete", Value::Bool(true)),
        ("git", before_git.clone()),
        ("runtime_artifact", runtime),
        (
            "predecessor",
            options.predecessor.cloned().unwrap_or(Value::Null),
        ),
        ("metadata_policy", s(METADATA_POLICY)),
        ("bank_binding", bank_binding(&bank)?),
    ]);
    let first = inventory_with_options(
        &bank,
        InventoryOptions {
            retain_directory: directory.as_deref(),
            envelope: Some(&envelope),
            ..Default::default()
        },
    )?;
    let after = inventory_with_options(
        &bank,
        InventoryOptions {
            prior_sources: Some(&first.sources),
            envelope: Some(&envelope),
            ..Default::default()
        },
    )?;
    if first.physical_entries != after.physical_entries
        || first.sources != after.sources
        || !equal(&first.totals, &after.totals)?
        || !equal(&git_pins(&root)?, &before_git)?
    {
        return Err(err(
            "Recovery observation changed; preserve staged evidence",
        ));
    }
    if !equal(
        &identity(
            &root,
            &router,
            options.identity_review,
            options.owner,
            options.transaction,
        )?,
        &resolved,
    )? {
        return Err(err("Recovery identity changed"));
    }
    bank_observation::markers(&bank, options.owner, options.transaction)?;
    Ok(merge(
        envelope,
        [
            (
                "physical_entries".into(),
                Value::Array(first.physical_entries),
            ),
            ("sources".into(), Value::Array(first.sources)),
            ("totals".into(), first.totals),
        ],
    ))
}
pub fn recheck(
    repo: &Path,
    manifest: &Value,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<bool> {
    let (root, bank) = locate(repo)?;
    if field(manifest, "repo_root")?.as_str() != Some(text(&root)?)
        || field(manifest, "bank_path")?.as_str() != Some(text(&bank)?)
        || field(manifest, "observation_complete")?.as_bool() != Some(true)
    {
        return Err(err("Recovery manifest location/completeness changed"));
    }
    bank_observation::markers(&bank, owner, transaction)?;
    let prior = field(manifest, "sources")?
        .as_array()
        .ok_or_else(|| err("Recovery manifest location/completeness changed"))?;
    let current = inventory_with_options(
        &bank,
        InventoryOptions {
            prior_sources: Some(prior),
            ..Default::default()
        },
    )?;
    let binding = bank_binding(&bank)?;
    let (router, _) = read_source(&bank, "MEMORY.md", MAX_FILE)?;
    let review = field(manifest, "identity")?
        .get("identity_review")
        .filter(|v| **v != Value::Null);
    let resolved = identity(&root, &router, review, owner, transaction)?;
    if !equal(
        &Value::Array(current.physical_entries),
        field(manifest, "physical_entries")?,
    )? || !equal(&Value::Array(current.sources), field(manifest, "sources")?)?
        || !equal(&current.totals, field(manifest, "totals")?)?
        || !equal(&git_pins(&root)?, field(manifest, "git")?)?
        || !equal(&binding, field(manifest, "bank_binding")?)?
        || !equal(&resolved, field(manifest, "identity")?)?
    {
        return Err(err("Recovery source/metadata/identity/Git pins changed"));
    }
    Ok(true)
}
