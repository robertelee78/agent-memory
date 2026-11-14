//! Invocation-local, bounded canonical bytes and stable physical topology.
use crate::bank_capacity::{BankCapacityError, CapacityCode, Measurement};
use crate::bank_contract::{self as contract, ParsedRecords};
use crate::bank_writes as writes;
use crate::codec::json::Value;
use crate::developer_identity::uuid_valid;
use crate::error::{ContractError, Result};
use crate::fs::{self, DirectoryChain, ReadError};
use crate::lifecycle::{get, object, string};
use crate::sharing_limits;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};

pub const MAX_FILES: usize = sharing_limits::MAX_FILES;
pub const MAX_BYTES: usize = sharing_limits::MAX_BANK;
pub const MAX_DEPTH: usize = 256;
pub type Physical = BTreeMap<String, String>;

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn unsafe_source() -> ContractError {
    err(
        "Canonical source or parent changed, became unsafe, or cannot be read; preserve it and retry",
    )
}
fn read_error(error: ReadError) -> ContractError {
    match error {
        ReadError::SourceChanged => err("Canonical source changed during bounded observation"),
        ReadError::ParentChanged => {
            err("Canonical source parent changed during bounded observation")
        }
        ReadError::NotRegular => err("Canonical source is no longer ordinary"),
        _ => unsafe_source(),
    }
}
fn capacity(
    code: CapacityCode,
    path: &str,
    limit: usize,
    observed: usize,
    measurement: Measurement,
) -> ContractError {
    match BankCapacityError::new(code, path, limit, observed, measurement) {
        Ok(error) => error.into(),
        Err(error) => error,
    }
}

/// The T0 default explicitly refuses growth rather than classifying it as capacity.
pub fn read_source(bank: &Path, relative: &str, limit: usize) -> Result<Vec<u8>> {
    match fs::read_source(
        bank,
        Path::new(relative),
        limit.min(sharing_limits::MAX_FILE),
    ) {
        Ok(read) => Ok(read.bytes),
        Err(ReadError::SizeLimit { observed, .. })
            if observed > sharing_limits::MAX_FILE as u64 =>
        {
            let size = usize::try_from(observed).map_err(|_| unsafe_source())?;
            Err(capacity(
                CapacityCode::SourceBytes,
                relative,
                sharing_limits::MAX_FILE,
                size,
                Measurement::Exact,
            ))
        }
        Err(error) => Err(read_error(error)),
    }
}

pub fn markers(bank: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>) -> Result<()> {
    let chain = DirectoryChain::open(bank).map_err(|_| unsafe_source())?;
    let directory = chain.directory();
    for (name, expected) in [
        (".agent-memory.lock", owner),
        (".agent-memory-transaction.md", transaction),
    ] {
        let current = match directory.stat_at(OsStr::new(name)) {
            Ok(info) => info,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                if expected.is_some() {
                    return Err(err(
                        "Owned bank lock disappeared; preserve work and retry with actual ownership",
                    ));
                }
                continue;
            }
            Err(_) => return Err(unsafe_source()),
        };
        let Some(expected) = expected else {
            if name == ".agent-memory.lock" {
                crate::local_registry::check_canonical_writer(&bank.join(name), writes::OPERATIONS)
                    .map_err(|error| err(&error.to_string()))?;
                match directory.stat_at(OsStr::new(name)) {
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
                    Err(_) => return Err(unsafe_source()),
                    Ok(_) => {}
                }
            }
            return Err(err(
                "Bank is busy/interrupted; preserve the operation and retry after recovery",
            ));
        };
        if !current.is_regular() {
            return Err(err("Owned bank marker is not an ordinary record"));
        }
        let observed =
            fs::read_source(bank, Path::new(name), expected.len()).map_err(
                |error| match error {
                    ReadError::SourceChanged | ReadError::ParentChanged => {
                        err("Bank ownership changed during bounded observation")
                    }
                    ReadError::NotRegular => err("Owned marker type changed"),
                    ReadError::SizeLimit { .. } => {
                        err("Bank ownership changed; preserve work and retry with actual ownership")
                    }
                    _ => unsafe_source(),
                },
            )?;
        chain.recheck().map_err(|_| {
            err("Bank ownership changed; preserve work and retry with actual ownership")
        })?;
        if observed.bytes != expected {
            return Err(err(
                "Bank ownership changed; preserve work and retry with actual ownership",
            ));
        }
    }
    chain.recheck().map_err(|_| unsafe_source())
}

#[derive(Debug, Clone, Copy)]
pub struct ScanLimits {
    pub max_files: usize,
    pub max_bytes: usize,
}
impl Default for ScanLimits {
    fn default() -> Self {
        Self {
            max_files: MAX_FILES,
            max_bytes: MAX_BYTES,
        }
    }
}
#[derive(Debug)]
pub struct Scan {
    pub physical: Physical,
    pub hashes: Value,
    pub sources: BTreeMap<String, Vec<u8>>,
    pub payload_bytes: usize,
    pub file_reads: usize,
}
struct Scanner<'a> {
    bank: &'a Path,
    read_sources: bool,
    hash_sources: bool,
    limits: ScanLimits,
    physical_limit: usize,
    result: Scan,
    hashes: BTreeMap<String, Value>,
}
impl Scanner<'_> {
    fn walk(&mut self, relative: &Path, depth: usize) -> Result<()> {
        if depth > MAX_DEPTH {
            return Err(err(
                "Bank physical inventory exceeds its finite depth budget",
            ));
        }
        let chain = DirectoryChain::open(&self.bank.join(relative))
            .map_err(|_| err("Bank physical inventory is incomplete"))?;
        let directory = chain
            .directory()
            .reopen_directory()
            .map_err(|_| err("Bank physical inventory is incomplete"))?;
        let names = directory
            .entries_bounded(self.physical_limit.saturating_add(1))
            .map_err(|_| err("Bank physical inventory is incomplete"))?
            .ok_or_else(|| err("Complete physical bank inventory exceeds its finite budget"))?;
        for name in names {
            chain
                .recheck()
                .map_err(|_| err("Canonical source parent changed during bounded observation"))?;
            let path = relative.join(&name);
            let text = crate::host::text(path.as_os_str(), "canonical bank path")?.to_owned();
            let info = directory
                .stat_at(&name)
                .map_err(|_| err("Bank physical inventory is incomplete"))?;
            if info.is_symlink() {
                return Err(err("Bank paths must not be symlinks"));
            }
            let kind = if info.is_directory() {
                "directory"
            } else if info.is_regular() {
                "file"
            } else {
                "other"
            };
            self.result.physical.insert(text.clone(), kind.to_owned());
            if self.result.physical.len() > self.physical_limit {
                return Err(err(
                    "Complete physical bank inventory exceeds its finite budget",
                ));
            }
            if self.hash_sources
                && path.extension() == Some(OsStr::new("md"))
                && !contract::runtime_path(&text)
            {
                if !info.is_regular() {
                    return Err(err("Canonical Markdown source must be an ordinary file"));
                }
                let remaining = self
                    .limits
                    .max_bytes
                    .saturating_sub(self.result.payload_bytes);
                let limit = remaining.min(sharing_limits::MAX_FILE);
                let raw = match fs::read_source(self.bank, &path, limit) {
                    Ok(read) => read.bytes,
                    Err(ReadError::SizeLimit { observed, .. }) => {
                        let size = usize::try_from(observed).map_err(|_| unsafe_source())?;
                        if size > sharing_limits::MAX_FILE {
                            return Err(capacity(
                                CapacityCode::SourceBytes,
                                &text,
                                sharing_limits::MAX_FILE,
                                size,
                                Measurement::Exact,
                            ));
                        }
                        chain.recheck().map_err(|_| {
                            err("Canonical source parent changed during bounded observation")
                        })?;
                        if self.hashes.len() >= self.limits.max_files {
                            return Err(capacity(
                                CapacityCode::SourceCount,
                                &text,
                                self.limits.max_files,
                                self.hashes.len() + 1,
                                Measurement::AtLeast,
                            ));
                        }
                        return Err(capacity(
                            CapacityCode::AggregateBytes,
                            &text,
                            self.limits.max_bytes,
                            self.result
                                .payload_bytes
                                .saturating_add(remaining.saturating_add(1)),
                            Measurement::AtLeast,
                        ));
                    }
                    Err(error) => return Err(read_error(error)),
                };
                chain.recheck().map_err(|_| {
                    err("Canonical source parent changed during bounded observation")
                })?;
                if self.hashes.len() >= self.limits.max_files {
                    return Err(capacity(
                        CapacityCode::SourceCount,
                        &text,
                        self.limits.max_files,
                        self.hashes.len() + 1,
                        Measurement::AtLeast,
                    ));
                }
                self.result.payload_bytes += raw.len();
                self.hashes.insert(
                    text.clone(),
                    string(crate::validation_memo::hex(&Sha256::digest(&raw))),
                );
                if self.read_sources {
                    self.result.sources.insert(text.clone(), raw);
                }
            }
            if info.is_directory() {
                self.walk(&path, depth + 1)?;
                chain.recheck().map_err(|_| {
                    err("Canonical source parent changed during bounded observation")
                })?;
                let named = directory.stat_at(&name).map_err(|_| unsafe_source())?;
                if !info.same_binding(named) {
                    return Err(err(
                        "Canonical source parent changed during bounded observation",
                    ));
                }
            }
        }
        chain
            .recheck()
            .map_err(|_| err("Canonical source parent changed during bounded observation"))
    }
}

pub fn scan(bank: &Path, read_sources: bool, require_router: bool) -> Result<Scan> {
    scan_with_limits(bank, read_sources, require_router, ScanLimits::default())
}
pub fn scan_with_limits(
    bank: &Path,
    read_sources: bool,
    require_router: bool,
    limits: ScanLimits,
) -> Result<Scan> {
    scan_internal(bank, read_sources, require_router, limits, true)
}
pub fn physical_inventory(bank: &Path) -> Result<Physical> {
    Ok(scan_internal(bank, false, false, ScanLimits::default(), false)?.physical)
}
fn scan_internal(
    bank: &Path,
    read_sources: bool,
    require_router: bool,
    limits: ScanLimits,
    hash_sources: bool,
) -> Result<Scan> {
    if limits.max_files > MAX_FILES || limits.max_bytes > MAX_BYTES {
        return Err(err(
            "Bank scan limits must not exceed canonical observation bounds",
        ));
    }
    let mut scanner = Scanner {
        bank,
        read_sources,
        hash_sources,
        limits,
        physical_limit: limits.max_files.saturating_mul(2),
        hashes: BTreeMap::new(),
        result: Scan {
            physical: BTreeMap::new(),
            hashes: Value::Null,
            sources: BTreeMap::new(),
            payload_bytes: 0,
            file_reads: 0,
        },
    };
    scanner.walk(Path::new(""), 0)?;
    if require_router && !scanner.hashes.contains_key("MEMORY.md") {
        return Err(err("A complete canonical router is required"));
    }
    scanner.result.file_reads = scanner.hashes.len();
    scanner.result.hashes = object(scanner.hashes);
    Ok(scanner.result)
}

pub fn physical_contract(physical: &Physical) -> Result<()> {
    contract::portable_aliases(physical.keys().map(String::as_str).chain([
        "MEMORY.md",
        "history",
        "history/tombstones",
    ]))?;
    for name in ["history", "history/tombstones"] {
        if physical.get(name).is_some_and(|kind| kind != "directory") {
            return Err(err("Deletion history requires canonical directories"));
        }
    }
    for (path, kind) in physical {
        if let Some(name) = path.strip_prefix(crate::tombstones::PREFIX) {
            if kind != "file"
                || name.contains('/')
                || !name
                    .strip_suffix(".md")
                    .is_some_and(|id| uuid_valid(&string(id)))
            {
                return Err(err(
                    "Deletion history permits only canonical UUID marker files",
                ));
            }
        }
    }
    Ok(())
}

#[derive(Debug)]
pub struct Observation {
    pub root: PathBuf,
    pub bank: PathBuf,
    pub owner: Option<Vec<u8>>,
    pub transaction: Option<Vec<u8>>,
    pub physical: Physical,
    pub hashes: Value,
    pub sources: BTreeMap<String, Vec<u8>>,
    pub checked: Value,
    pub parsed: ParsedRecords,
    pub payload_bytes_read: usize,
    pub payload_file_reads: usize,
    pub recovery_frontier: Value,
}
impl Observation {
    pub fn recheck(&mut self, stage: &str) -> Result<()> {
        writes::fault(stage)?;
        let (root, bank) = writes::bank_path(&self.root)?;
        if root != self.root || bank != self.bank {
            return Err(err("Canonical repository/bank location changed"));
        }
        writes::recovery_frontier_provider()?.check(
            &self.recovery_frontier,
            &self.root,
            self.owner.as_deref(),
            self.transaction.as_deref(),
        )?;
        markers(&bank, self.owner.as_deref(), self.transaction.as_deref())?;
        let latest = scan(&bank, false, true)?;
        if latest.physical != self.physical || latest.hashes != self.hashes {
            return Err(err(
                "Canonical sources/topology changed; reread before using recall",
            ));
        }
        self.payload_bytes_read += latest.payload_bytes;
        self.payload_file_reads += latest.file_reads;
        markers(&bank, self.owner.as_deref(), self.transaction.as_deref())
    }
    pub fn into_contract(self) -> contract::Observation {
        contract::Observation {
            checked: self.checked,
            hashes: self.hashes,
            sources: self.sources,
            recovery_frontier: self.recovery_frontier,
        }
    }
}
pub fn observe(
    repo: &Path,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
) -> Result<Observation> {
    let (root, bank) = writes::bank_path(repo)?;
    let provider = writes::recovery_frontier_provider()?;
    let frontier = provider.observe_owned(&root, owner, transaction)?;
    let held = if transaction.is_none() {
        match frontier
            .get("hold_sha256")
            .ok_or_else(|| err("'hold_sha256'"))?
        {
            Value::Null => None,
            _ => Some(provider.verified_hold(&root, owner)?),
        }
    } else {
        None
    };
    let transaction = transaction.or(held.as_deref());
    markers(&bank, owner, transaction)?;
    let scanned = scan(&bank, true, true)?;
    physical_contract(&scanned.physical)?;
    writes::fault("recall-after-source-read")?;
    let files = object(
        scanned
            .sources
            .iter()
            .map(|(path, raw)| {
                std::str::from_utf8(raw)
                    .map(|text| (path.clone(), string(text)))
                    .map_err(|_| err("Canonical Markdown source must be UTF-8"))
            })
            .collect::<Result<Vec<_>>>()?,
    );
    let mut parsed = ParsedRecords::new();
    let checked = contract::validate_files(&files, Some(&mut parsed))?;
    let omitted = get(&checked, "omitted")
        .as_object()
        .ok_or_else(|| err("Invalid canonical omission inventory"))?;
    for (path, _) in omitted {
        if scanned.physical.contains_key(path) {
            return Err(err(
                "An omitted context path is physically occupied; inspect it instead of inferring absence",
            ));
        }
    }
    let mut observed = Observation {
        root,
        bank,
        owner: owner.map(<[u8]>::to_vec),
        transaction: transaction.map(<[u8]>::to_vec),
        physical: scanned.physical,
        hashes: scanned.hashes,
        sources: scanned.sources,
        checked,
        parsed,
        payload_bytes_read: scanned.payload_bytes,
        payload_file_reads: scanned.file_reads,
        recovery_frontier: frontier,
    };
    observed.recheck("recall-initial-source-recheck")?;
    Ok(observed)
}

/// Observed canonical hashes, without source bytes and with no router required.
pub fn scan_hashes(bank: &Path) -> Result<Value> {
    Ok(scan(bank, false, false)?.hashes)
}
/// Baseline scan(read_sources=True, require_router=False): physical map
/// (including runtime entries, path -> directory/file/other), observed hashes,
/// and exact canonical source bytes, with no router required.
pub fn scan_sources(bank: &Path) -> Result<(Value, Value, BTreeMap<String, Vec<u8>>)> {
    let scanned = scan(bank, true, false)?;
    Ok((
        object(
            scanned
                .physical
                .into_iter()
                .map(|(path, kind)| (path, string(kind))),
        ),
        scanned.hashes,
        scanned.sources,
    ))
}
