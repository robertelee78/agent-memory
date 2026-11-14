//! Stage and verify a signed release runtime without migrating banks.
//!
//! Baseline `updating.py`. The Rust runtime updates only from the signed
//! release-v2 channel: the candidate (portable pack, signed manifest and the
//! separately signed per-target `am`) is acquired and verified before any
//! lifecycle write, its declared command surface is read from the signed
//! manifest (the candidate is never executed to discover it), and the owned
//! runtime root is replaced by installation's journaled native exchange, so a
//! running `am` keeps executing its old inode. Python-era receipts and managed
//! Git channels refuse with re-bootstrap guidance.

use crate::cache_recovery::{self, Qualification};
use crate::codec::json::{self, Value};
use crate::error::ContractError;
use crate::fs;
use crate::installation::{
    self as install, ADOPTION_GUIDANCE, ATOMICITY, ActivationHooks, ArtifactInspection, Command,
    Environment, Error, InstallRequest, MANAGED_GUIDANCE, PrepareOptions, RELEASE_BINARY,
    RELEASE_PACK, RegisteredArtifacts, ReleaseAccess, Result, Services, TARGET, require,
};
use crate::lifecycle::{CommandResult, get, object, string, text};
use crate::release_distribution::{self as release, ReleaseObservation, Scratch};
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Baseline `current` refusal for any non-release source.
pub const RECORDED_GUIDANCE: &str =
    "Updates require a recorded managed installation; development sources are never modified";
/// Lifecycle commands every candidate must keep (baseline `compatible_surface`).
pub const LIFECYCLE_COMMANDS: [&str; 3] = ["update", "install", "recover"];
/// Baseline `wrapper_readback` deadline (`timeout=30`).
pub const READBACK_TIMEOUT: Duration = Duration::from_secs(30);
/// Captured readback output bound.
pub const MAX_READBACK_OUTPUT: usize = 65536;
/// Baseline `wrapper_readback` invocations of each owned public command.
const READBACK_ARGUMENTS: [&[&str]; 2] = [&["--help"], &["update", "--help"]];
/// Baseline `migration.read_regular(bank, 'MEMORY.md', 16384)`.
pub const MAX_ROUTER: usize = 16384;
/// Baseline repository inventory bound.
pub const MAX_REPOSITORIES: usize = 1024;
/// Signature asset bound (release_distribution).
const MAX_SIGNATURE: usize = 4096;

fn contract(message: impl Into<String>) -> Error {
    Error::Contract(ContractError::new(message.into()))
}

fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
        && value.as_str() != Some("")
        && value.as_object().is_none_or(|fields| !fields.is_empty())
        && value.as_array().is_none_or(|items| !items.is_empty())
}

/// `{**left, **right}`: right's keys replace left's in place, new keys append.
pub(crate) fn merged(left: Value, right: Value) -> Value {
    let mut fields = match left {
        Value::Object(fields) => fields,
        _ => Vec::new(),
    };
    if let Value::Object(extra) = right {
        for (key, value) in extra {
            match fields.iter_mut().find(|(name, _)| *name == key) {
                Some((_, slot)) => *slot = value,
                None => fields.push((key, value)),
            }
        }
    }
    Value::Object(fields)
}

/// The baseline `execute` output of update, uninstall and purge-data:
/// `json.dumps(result, indent=2)`; success iff verified or check_complete.
pub(crate) fn printed(value: Value) -> CommandResult {
    let success = get(&value, "verified") == &Value::Bool(true)
        || get(&value, "check_complete") == &Value::Bool(true);
    match json::encode(&value, json::INDENT2_ASCII) {
        Ok(text) => CommandResult {
            value: Some(value),
            exit_code: if success { 0 } else { 1 },
            stdout: text + "\n",
            stderr: String::new(),
        },
        Err(error) => CommandResult {
            value: Some(value),
            exit_code: 1,
            stdout: String::new(),
            stderr: format!("{error}\n"),
        },
    }
}

// ----- version ordering -----

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Identifier {
    /// Numeric identifiers compare as integers: (length without leading zeros, digits).
    Numeric(usize, String),
    Text(String),
}

fn numeric(value: &str) -> Identifier {
    let digits = value.trim_start_matches('0');
    Identifier::Numeric(digits.len(), digits.to_owned())
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct VersionKey {
    core: Vec<Identifier>,
    /// Python `int(not pre)`: a release orders after its prereleases.
    release: bool,
    pre: Vec<Identifier>,
}

/// Baseline `version_key`.
pub fn version_key(value: &str) -> Result<VersionKey> {
    require(
        crate::pack_validation::semantic_version(&string(value)),
        "Invalid declared semantic version",
    )?;
    let without_build = value.split('+').next().unwrap_or_default();
    let (core, pre) = without_build
        .split_once('-')
        .map_or((without_build, ""), |(core, pre)| (core, pre));
    let pre = if pre.is_empty() {
        Vec::new()
    } else {
        pre.split('.')
            .map(|part| {
                if !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit()) {
                    numeric(part)
                } else {
                    Identifier::Text(part.to_owned())
                }
            })
            .collect()
    };
    Ok(VersionKey {
        core: core.split('.').map(numeric).collect(),
        release: pre.is_empty(),
        pre,
    })
}

/// Baseline `configuration`: the saved install configuration for update.
pub fn configuration(omp_peers_upgraded: bool) -> InstallRequest {
    InstallRequest {
        omp_peers_upgraded,
        ..InstallRequest::default()
    }
}

/// Python-era (v1/v2) receipts are adoption inputs only: re-run the bootstrap.
fn require_rust_era(receipt: &Value) -> Result<()> {
    require(!install::python_era_kind(receipt), ADOPTION_GUIDANCE)
}

fn targets(receipt: &Value) -> Vec<&Value> {
    get(receipt, "targets")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .collect()
}

fn path_of(value: &Value, key: &str) -> PathBuf {
    PathBuf::from(text(value, key).unwrap_or_default())
}

/// Baseline `current`: the recorded signed runtime and every owned target
/// still exactly match the receipt.
pub fn current(
    env: &Environment,
    services: &Services,
    state: &Path,
    runtime: &Path,
    cache: Option<&Qualification>,
) -> Result<Value> {
    let old = install::read_receipt(env, state)?.ok_or_else(|| contract(RECORDED_GUIDANCE))?;
    require_rust_era(&old)?;
    let source = get(&old, "source");
    require(text(source, "kind") != Some("managed"), MANAGED_GUIDANCE)?;
    require(text(source, "kind") == Some("release"), RECORDED_GUIDANCE)?;
    let root = path_of(source, "path");
    fs::trust::source(&root)?;
    let pack = runtime.join(RELEASE_PACK);
    require(
        root == runtime && text(source, "pack") == pack.to_str(),
        "Managed source/pack location differs from the recorded layout",
    )?;
    let actual = match cache {
        Some(qualification) => {
            require(
                install::same(&old, &qualification.receipt),
                "Installation receipt changed after cache qualification",
            )?;
            cache_recovery::recheck(qualification)?;
            qualification.source.clone()
        }
        None => install::source_info(services, &root, "release")?.value,
    };
    require(
        install::same(&actual, source),
        "Current source revision, version or artifact differs from its receipt",
    )?;
    install::require_replannable_journal(env, state)?;
    for target in targets(&old) {
        let path = path_of(target, "path");
        let kind = text(target, "kind").unwrap_or_default();
        if kind == "activation" {
            require(
                install::is_regular(&path)?,
                "Activation target is missing or linked",
            )?;
            let raw = install::read_optional(&path, install::MAX_GUIDANCE)?.unwrap_or_default();
            install::activation(&raw, text(target, "block_hash"), install::SHIM)?;
        } else if let (Some(qualification), "source", true) = (cache, kind, path == root) {
            require(
                install::same(
                    &install::fingerprint(&root)?,
                    &qualification.source_identity,
                ),
                "Qualified source changed during target verification",
            )?;
        } else if cache.is_some()
            && kind == "link"
            && install::same(
                get(target, "identity"),
                &object([
                    ("kind", string("link")),
                    ("target", get(source, "pack").clone()),
                ]),
            )
        {
            fs::trust::tree(&path, false)?;
            require(
                install::same(&install::fingerprint(&path)?, get(target, "identity"))
                    && get(target, "artifact_digest") == get(source, "artifact_digest"),
                format!("Installed client link changed: {}", path.display()),
            )?;
        } else {
            install::verify_targets(services, &[target], Some(source), false)?;
        }
    }
    if let Some(qualification) = cache {
        cache_recovery::recheck(qualification)?;
    }
    Ok(old)
}

/// Baseline `qualified_current`: a changed signed runtime root may still be
/// qualified as opaque cache additions, never executed.
pub fn qualified_current(
    env: &Environment,
    services: &Services,
    state: &Path,
    runtime: &Path,
) -> Result<(Value, Option<Qualification>)> {
    let error = match current(env, services, state, runtime, None) {
        Ok(old) => return Ok((old, None)),
        Err(Error::Unregistered(name)) => return Err(Error::Unregistered(name)),
        Err(error) => error,
    };
    let Some(old) = install::read_receipt(env, state)? else {
        return Err(error);
    };
    if install::python_era_kind(&old) || text(get(&old, "source"), "kind") != Some("release") {
        return Err(error);
    }
    let runtime_text = runtime.to_str().unwrap_or_default();
    let target = targets(&old).into_iter().find(|item| {
        text(item, "kind") == Some("source") && text(item, "path") == Some(runtime_text)
    });
    // An unchanged source cannot explain a wrapper/client/hook failure.
    // Preserve that exact diagnostic instead of replacing it with a cache error.
    let Some(target) = target else {
        return Err(error);
    };
    if install::same(&install::fingerprint(runtime)?, get(target, "identity")) {
        return Err(error);
    }
    let qualification = cache_recovery::qualify(runtime, &old)?;
    let old = current(env, services, state, runtime, Some(&qualification))?;
    Ok((old, Some(qualification)))
}

// ----- bank compatibility -----

/// Baseline `re.search(br'(?m)^schema:[ \t]*["\']?agent-memory/', raw[:16384])`.
fn has_format_marker(raw: &[u8]) -> bool {
    let prefix = raw.get(..MAX_ROUTER).unwrap_or(raw);
    prefix.split(|byte| *byte == b'\n').any(|line| {
        let Some(mut rest) = line.strip_prefix(b"schema:") else {
            return false;
        };
        while let Some((b' ' | b'\t', tail)) = rest.split_first() {
            rest = tail;
        }
        if let Some((b'"' | b'\'', tail)) = rest.split_first() {
            rest = tail;
        }
        rest.starts_with(b"agent-memory/")
    })
}

fn format_schema(value: &str) -> bool {
    value.strip_prefix("agent-memory/").is_some_and(|name| {
        (1..=128).contains(&name.len())
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'))
    })
}

/// Baseline `migration.router_metadata`: locate a format-family header, not
/// validate or adopt its memory.
pub fn router_metadata(raw: &[u8]) -> Result<Option<Value>> {
    if !has_format_marker(raw) {
        return Ok(None);
    }
    require(
        raw.len() <= MAX_ROUTER && raw.starts_with(b"---\n"),
        "Migration router requires bounded LF frontmatter at byte zero",
    )?;
    let text_value =
        std::str::from_utf8(raw).map_err(|_| contract("Migration router header is not UTF-8"))?;
    let end = text_value
        .get(4..)
        .and_then(|body| body.find("\n---\n"))
        .map(|offset| offset + 4)
        .ok_or_else(|| contract("Migration router header is incomplete"))?;
    let header = text_value.get(4..end).unwrap_or_default();
    let keys: BTreeSet<&str> = header
        .lines()
        .filter(|line| !line.starts_with(' '))
        .map(|line| line.split(':').next().unwrap_or_default())
        .collect();
    let allowed: Vec<&str> = keys.into_iter().collect();
    let (metadata, _) = crate::codec::frontmatter::read(text_value, &allowed)?;
    let metadata = Value::Object(metadata);
    require(
        text(&metadata, "schema").is_some_and(format_schema)
            && crate::developer_identity::uuid_valid(get(&metadata, "project-id")),
        "Migration router needs an explicit format and canonical project UUID",
    )?;
    Ok(Some(metadata))
}

fn git_optional(repo: &Path, args: &[&str]) -> Result<Option<String>> {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    let output = crate::identity_git::git(repo, &args, true, false)
        .map_err(|error| contract(error.to_string()))?;
    match output {
        Some(value) => Ok(Some(
            value
                .into_string()
                .map_err(|_| contract("Git output is not UTF-8"))?,
        )),
        None => Ok(None),
    }
}

fn strings(values: impl IntoIterator<Item = String>) -> Value {
    Value::Array(values.into_iter().map(Value::String).collect())
}

fn listed(compatibility: &Value, key: &str, schema: &str) -> bool {
    get(compatibility, key)
        .as_array()
        .is_some_and(|values| values.iter().any(|value| value.as_str() == Some(schema)))
}

fn gap(location: &str, reason: &str) -> Value {
    object([("location", string(location)), ("reason", string(reason))])
}

fn router_reader(raw: &[u8]) -> crate::error::Result<Option<bool>> {
    router_metadata(raw)
        .map(|metadata| metadata.map(|_| true))
        .map_err(|error| ContractError::new(error.to_string()))
}

fn bank_settings(root: &Path) -> Result<crate::repository_config::Settings> {
    Ok(crate::repository_config::settings_with(
        root,
        Some(&router_reader),
        false,
    )?)
}

fn read_router(bank: &Path) -> Result<Vec<u8>> {
    let (raw, _) = fs::read_regular(bank, Path::new("MEMORY.md"), MAX_ROUTER)
        .map_err(|error| contract(error.to_string()))?;
    Ok(raw)
}

/// One repository of the baseline `banks.inspect`: `(record, gap)`.
fn inspect_bank(
    root: &Path,
    compatibility: &Value,
    require_readable: bool,
) -> Result<(Option<Value>, Option<Value>)> {
    let root_text = install::path_text(root)?;
    if !install::is_directory(root)? {
        return Ok((
            None,
            Some(gap(
                root_text,
                "worktree no longer available; not inspected",
            )),
        ));
    }
    let settings = bank_settings(root)?;
    let bank = root.join(&settings.bank_root);
    let router = bank.join("MEMORY.md");
    if !install::lexists(&router)? {
        return Ok((None, None));
    }
    let bank_link = install::named(&bank)?.is_some_and(|info| info.is_symlink());
    require(
        !bank_link && install::is_regular(&router)?,
        "Discovered bank/router must be an ordinary readable path",
    )?;
    let idle = || -> Result<()> {
        require(
            !install::lexists(&bank.join(".agent-memory.lock"))?,
            "Compatibility requires an idle bank; preserve its live writer",
        )?;
        if install::lexists(&bank.join(crate::bank_transactions::MARKER))? {
            let held =
                crate::bank_writes::recovery_frontier_provider()?.verified_hold(root, None)?;
            let capable = crate::private_state::compatibility(Some(compatibility))?
                .contains("layout-frontier-v1");
            require(
                !held.is_empty() && capable,
                "Target runtime cannot enforce the verified private recovery hold",
            )?;
        }
        Ok(())
    };
    idle()?;
    let raw = read_router(&bank)?;
    let metadata = router_metadata(&raw)?
        .ok_or_else(|| contract("Discovered bank needs an explicit format header"))?;
    let schema = text(&metadata, "schema").unwrap_or_default().to_owned();
    if schema == "agent-memory/v1" {
        let router_text = std::str::from_utf8(&raw)
            .map_err(|_| contract("Discovered bank router is not UTF-8"))?;
        crate::bank_contract::parse_router(router_text)?;
    }
    let readable = listed(compatibility, "readable_bank_schemas", &schema);
    let migratable = listed(compatibility, "migrator_source_schemas", &schema);
    let prefix = if require_readable {
        "Rollback runtime cannot directly read current bank; explicitly restore/migrate memory first: "
    } else {
        "Candidate cannot read or explicitly migrate discovered bank: "
    };
    require(
        readable || (migratable && !require_readable),
        format!("{prefix}{}", bank.display()),
    )?;
    idle()?;
    require(
        read_router(&bank)? == raw && bank_settings(root)? == settings,
        "Discovered bank changed during compatibility observation",
    )?;
    Ok((
        Some(object([
            ("repository", string(root_text)),
            ("bank", string(install::path_text(&bank)?)),
            ("schema", string(schema)),
            ("directly_readable", Value::Bool(readable)),
            ("explicit_migration_required", Value::Bool(!readable)),
            ("router_sha256", string(install::sha(&raw))),
        ])),
        None,
    ))
}

/// Baseline `banks`: use the established one-bank locator; never invent
/// another bank path. Repositories are inspected sequentially in sorted order.
pub fn banks(
    env: &Environment,
    runtime_root: &Path,
    extra: &[String],
    compatibility: &Value,
    require_readable: bool,
) -> Result<Value> {
    let mut roots: BTreeSet<PathBuf> = BTreeSet::new();
    let mut gaps = Vec::new();
    let cwd = fs::canonical_path(
        &std::env::current_dir().map_err(|error| contract(error.to_string()))?,
        false,
    )?;
    let saved = object([("rollback_repositories", strings(extra.iter().cloned()))]);
    let inventory = crate::private_state::repository_inventory(Some(&saved), &cwd, runtime_root)?;
    require(
        inventory.unknown.is_empty(),
        "Private-state bank inventory is unknown; preserve the active capable runtime",
    )?;
    roots.extend(inventory.roots);
    match git_optional(&cwd, &["rev-parse", "--show-toplevel"])? {
        Some(top) => roots.insert(fs::canonical_path(Path::new(top.trim()), false)?),
        None => roots.insert(cwd.clone()),
    };
    for value in extra {
        let path = install::absolute(env, Path::new(value))?;
        require(
            install::is_directory(&fs::canonical_path(&path, false)?)?,
            format!("Explicit repository is unavailable: {}", path.display()),
        )?;
        roots.insert(fs::canonical_path(&path, true)?);
    }
    let (registry, _) = crate::project_identity::observe()?;
    for (_, entry) in get(&registry, "checkouts").as_object().unwrap_or(&[]) {
        for location in get(entry, "locations").as_array().unwrap_or(&[]) {
            let location = location.as_str().unwrap_or_default();
            let path = Path::new(location);
            if !path.is_absolute() || !install::is_directory(&fs::canonical_path(path, false)?)? {
                gaps.push(gap(
                    location,
                    "registered checkout is unavailable; not inspected",
                ));
                continue;
            }
            let worktrees = git_optional(path, &["worktree", "list", "--porcelain"])?;
            let dot_git = path.file_name().is_some_and(|name| name == ".git");
            let Some(worktrees) = worktrees else {
                if !dot_git && !install::lexists(&path.join(".git"))? {
                    roots.insert(fs::canonical_path(path, false)?);
                } else {
                    gaps.push(gap(
                        location,
                        "registered Git location cannot list worktrees",
                    ));
                }
                continue;
            };
            for line in worktrees.lines() {
                if let Some(worktree) = line.strip_prefix("worktree ") {
                    roots.insert(fs::canonical_path(Path::new(worktree), false)?);
                }
            }
        }
    }
    require(
        roots.len() <= MAX_REPOSITORIES,
        "Discovered repository inventory exceeds its bound",
    )?;
    let mut result = Vec::new();
    let mut seen: BTreeSet<PathBuf> = BTreeSet::new();
    let mut observed = Vec::new();
    for root in &roots {
        let (record, missing) = inspect_bank(root, compatibility, require_readable)?;
        if let Some(missing) = missing {
            gaps.push(missing);
            continue;
        }
        observed.push(install::path_text(root)?.to_owned());
        if let Some(record) = record {
            let bank = fs::canonical_path(&path_of(&record, "bank"), false)?;
            if seen.insert(bank) {
                result.push(record);
            }
        }
    }
    let observation = object([
        ("observed_repositories", strings(observed.iter().cloned())),
        ("banks", Value::Array(result.clone())),
    ]);
    let operation = if require_readable {
        "rollback"
    } else {
        "activate"
    };
    let private = crate::private_state::require_compatible(
        Some(&observation),
        Some(compatibility),
        operation,
        None,
        &cwd,
        runtime_root,
    )?;
    Ok(object([
        ("banks", Value::Array(result)),
        ("discovery_gaps", Value::Array(gaps)),
        ("observed_repositories", strings(observed)),
        (
            "private_state",
            crate::private_state::public_summary(&private)?,
        ),
        ("bank_writes_performed", Value::Bool(false)),
        ("implicit_migration_performed", Value::Bool(false)),
    ]))
}

// ----- candidate checks -----

/// Baseline `release_transition`: a signed version never changes identity.
/// The same-version identity is the per-target runtime artifact digest.
pub fn release_transition(source: &Value, candidate: &Value) -> Result<()> {
    require(
        text(candidate, "kind") == Some("release"),
        "Expected verified release acquisition",
    )?;
    let order = version_key(text(candidate, "version").unwrap_or_default())?
        .cmp(&version_key(text(source, "version").unwrap_or_default())?);
    require(order != Ordering::Less, "Runtime downgrade refused")?;
    if order == Ordering::Equal {
        require(
            text(source, "kind") == Some("release")
                && ["artifact_digest", "revision", "release_manifest_sha256"]
                    .iter()
                    .all(|key| get(source, key) == get(candidate, key)),
            "Published release version changed identity",
        )?;
    }
    Ok(())
}

/// Root command names declared by a signed runtime root's command surface.
pub fn declared_commands(root: &Path, signers: &str) -> Result<BTreeSet<String>> {
    let manifest = release::verify_manifest_with(
        &release::regular(&root.join(release::MANIFEST), release::MAX_MANIFEST)?,
        &release::regular(&root.join(release::SIGNATURE), MAX_SIGNATURE)?,
        None,
        signers,
    )?;
    let surface = &manifest.command_surface;
    crate::cli::surface::Declared::from_value(surface)?;
    let mut names = BTreeSet::new();
    for command in get(surface, "commands").as_array().unwrap_or(&[]) {
        let name = match text(command, "kind") {
            Some("route") => text(get(command, "route"), "name"),
            _ => text(command, "name"),
        };
        names.insert(name.unwrap_or_default().to_owned());
    }
    Ok(names)
}

/// Baseline `compatible_surface`: compare the signed declarations of the
/// current runtime and the candidate; nothing is executed.
pub fn compatible_surface(services: &Services, source: &Value, candidate: &Value) -> Result<()> {
    let signers = &services.release.signers;
    let commands = declared_commands(&path_of(candidate, "path"), signers)?;
    let existing = declared_commands(&path_of(source, "path"), signers)?;
    require(
        LIFECYCLE_COMMANDS
            .iter()
            .all(|name| commands.contains(*name))
            && existing.is_subset(&commands),
        "Candidate would remove the owned lifecycle command surface",
    )
}

/// The runtime root of a probed release: signed pack and manifest, then the
/// separately signed per-target `am`, bound to the manifest's size and digest.
pub fn acquire_candidate(
    services: &Services,
    observed: &ReleaseObservation,
) -> Result<(Scratch, PathBuf)> {
    let access = &services.release;
    let scratch = Scratch::new("agent-memory-release-candidate-")?;
    let root = scratch.path.join("source");
    release::acquire_with(&root, observed, access.options)?;
    let identity = observed
        .manifest
        .targets
        .get(TARGET)
        .ok_or_else(|| contract("Unsupported release target"))?;
    let name = format!("am-{TARGET}");
    let raw = release::download_with(
        &format!("{}{name}", observed.base),
        identity.size,
        access.options,
    )
    .map_err(release::Error::from)?;
    let signature = release::download_with(
        &format!("{}{name}.sshsig", observed.base),
        MAX_SIGNATURE,
        access.options,
    )
    .map_err(release::Error::from)?;
    release::verify_signature_with(&raw, &signature, release::BINARY_NAMESPACE, &access.signers)?;
    require(
        raw.len() == identity.size && release::digest(&raw) == identity.sha256,
        "Release binary differs from its signed manifest",
    )?;
    fs::Descriptor::open_directory(&root)?.create_directory(std::ffi::OsStr::new("bin"), 0o755)?;
    release::write_new(&root.join(RELEASE_BINARY), &raw, 0o755)?;
    Ok((scratch, root))
}

/// Baseline `staged`: verify the acquired candidate before any activation.
pub fn staged(services: &Services, old: &Value, root: &Path) -> Result<install::SourceInfo> {
    let candidate = install::source_info(services, root, "release")?;
    release_transition(get(old, "source"), &candidate.value)?;
    compatible_surface(services, get(old, "source"), &candidate.value)?;
    Ok(candidate)
}

/// Baseline `wrapper_readback`: the owned public commands still dispatch
/// after activation (`--help`, `update --help`). Rust owns command links.
pub fn wrapper_readback(old: &Value, targets: Option<&[(String, PathBuf)]>) -> Result<()> {
    let bin_dir = path_of(old, "bin_dir");
    let commands: Vec<PathBuf> = install::COMMAND_NAMES
        .iter()
        .map(|name| bin_dir.join(name))
        .collect();
    let recorded: Vec<(String, PathBuf)> = self::targets(old)
        .into_iter()
        .map(|target| {
            (
                text(target, "kind").unwrap_or_default().to_owned(),
                path_of(target, "path"),
            )
        })
        .collect();
    let owned: Vec<&PathBuf> = recorded
        .iter()
        .filter(|(kind, path)| kind == "command-link" && commands.contains(path))
        .map(|(_, path)| path)
        .collect();
    for (kind, path) in targets.unwrap_or(&recorded) {
        if kind != "command-link" || !owned.contains(&path) {
            continue;
        }
        for args in READBACK_ARGUMENTS {
            let mut argv = vec![path.clone().into_os_string()];
            argv.extend(args.iter().map(OsString::from));
            let output =
                release::run_command(&argv, b"", MAX_READBACK_OUTPUT, None, READBACK_TIMEOUT)?;
            require(
                output.status == crate::process::Status::Code(0),
                format!(
                    "Updated command failed its public dispatch readback: {}",
                    String::from_utf8_lossy(&output.stderr).trim()
                ),
            )?;
        }
    }
    Ok(())
}

// ----- the command -----

/// The baseline `update` argparse namespace.
#[derive(Debug, Clone, Default)]
pub struct UpdateRequest {
    pub check: bool,
    pub repo: Vec<String>,
    pub omp_writers_quiesced: bool,
    pub omp_peers_upgraded: bool,
    pub omp_layout_quiescence: Option<Vec<PathBuf>>,
    pub layout_quiescence: Vec<PathBuf>,
    /// The verified bootstrap binary's own path (re-run install.sh).
    pub bootstrap_binary: Option<PathBuf>,
}

fn summary(old: &Value, candidate: &Value, cache: Option<&Qualification>) -> Value {
    let source = get(old, "source");
    let available: Vec<(String, Value)> = candidate
        .as_object()
        .unwrap_or(&[])
        .iter()
        .filter(|(key, _)| key != "path" && key != "pack")
        .cloned()
        .collect();
    let update_available = [
        "kind",
        "revision",
        "artifact_digest",
        "release_manifest_sha256",
    ]
    .iter()
    .any(|key| get(candidate, key) != get(source, key));
    let mut fields = vec![
        ("operation".to_owned(), string("update")),
        ("current".to_owned(), source.clone()),
        ("available".to_owned(), Value::Object(available)),
        (
            "channel".to_owned(),
            object([
                ("remote", get(source, "remote").clone()),
                ("ref", get(source, "ref").clone()),
            ]),
        ),
        ("update_available".to_owned(), Value::Bool(update_available)),
        (
            "loaded_content_changes".to_owned(),
            Value::Bool(get(candidate, "artifact_digest") != get(source, "artifact_digest")),
        ),
        ("atomicity".to_owned(), string(ATOMICITY)),
    ];
    if let Some(cache) = cache {
        fields.push(("cache_recovery".to_owned(), cache.summary.clone()));
    }
    Value::Object(fields)
}

fn set(value: &mut Value, key: &str, field: Value) {
    *value = merged(
        std::mem::replace(value, Value::Null),
        object([(key, field)]),
    );
}

fn observed_repositories(value: &Value) -> Vec<String> {
    get(value, "observed_repositories")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .filter_map(Value::as_str)
        .map(str::to_owned)
        .collect()
}

fn work(
    env: &Environment,
    services: &Services,
    request: &UpdateRequest,
    root: &Path,
) -> Result<Value> {
    let locations = install::locations(env)?;
    let state = &locations.state;
    let (old, cache) = qualified_current(env, services, state, &locations.runtime)?;
    let candidate = staged(services, &old, root)?;
    let mut summary = summary(&old, &candidate.value, cache.as_ref());
    let same_revision = get(&summary, "update_available") != &Value::Bool(true);
    let kind = text(&candidate.value, "kind")
        .unwrap_or("release")
        .to_owned();
    let candidate_root = (!same_revision || cache.is_some()).then_some(root);
    let mut plan = install::prepare(
        env,
        services,
        &configuration(request.omp_peers_upgraded),
        &locations,
        Some(&old),
        PrepareOptions {
            candidate_root,
            candidate_kind: Some(&kind),
            cache_recovery: cache.clone(),
            ..PrepareOptions::default()
        },
    )?;
    plan.operation = "update".to_owned();
    plan.omp_layout_quiescence = request.omp_layout_quiescence.clone().unwrap_or_default();
    plan.layout_quiescence = request.layout_quiescence.clone();
    plan.omp_transition = request.omp_writers_quiesced;
    let omp_transition = plan.omp_transition;
    let recorded = targets(&old);
    // With no candidate or target changes there are no bank writes to
    // qualify. Still verify ownership and public command dispatch.
    let unchanged = recorded.iter().all(|target| {
        plan.actions.iter().any(|action| {
            action.path.to_str() == text(target, "path")
                && install::same(get(target, "identity"), &action.after)
        })
    });
    if !request.check
        && request.repo.is_empty()
        && !omp_transition
        && same_revision
        && install::same(get(&old, "source"), &plan.source)
        && plan.restart.is_empty()
        && plan.actions.len() == recorded.len()
        && !plan.actions.iter().any(|action| action.changed)
        && unchanged
    {
        install::verify_targets(services, &recorded, Some(get(&old, "source")), false)?;
        wrapper_readback(&old, None)?;
        let bank_scan = if truthy(get(&old, "omp_binding")) {
            "native-private-activation"
        } else {
            "not-needed-no-changes"
        };
        let result = merged(
            summary,
            object([
                (
                    "compatibility",
                    object([
                        ("bank_scan", string(bank_scan)),
                        ("bank_writes_performed", Value::Bool(false)),
                        ("implicit_migration_performed", Value::Bool(false)),
                    ]),
                ),
                ("verified", Value::Bool(true)),
                ("changed", Value::Bool(false)),
                ("receipt", old.clone()),
                ("restart_clients", Value::Array(Vec::new())),
                (
                    "receipt_path",
                    string(install::path_text(&state.join("receipt.json"))?),
                ),
            ]),
        );
        return install::finish_stored_source_activation(env, services, state, result);
    }
    let candidate_compatibility = get(&candidate.value, "compatibility").clone();
    let compatibility =
        services
            .updating()?
            .banks(&request.repo, &candidate_compatibility, false)?;
    set(&mut summary, "compatibility", compatibility.clone());
    plan.rollback_repositories = observed_repositories(&compatibility);
    if request.check {
        let mut plan_value = plan.value()?;
        if omp_transition {
            let transition = services
                .omp_boundary()?
                .native_transition(&mut plan_value, true)?;
            set(&mut summary, "native_transition", transition);
        }
        if plan.omp_binding.is_some() {
            let activation = services
                .omp_boundary()?
                .native_activation(&mut plan_value, true)?;
            set(&mut summary, "native_activation", activation);
        }
        let private = services.private_state()?;
        let target = private.prepare_lifecycle(&plan_value, true)?;
        let compatible = private.require_compatible(&old, &target, "update")?;
        set(
            &mut summary,
            "private_compatibility",
            private.public_summary(&compatible)?,
        );
        return Ok(merged(
            summary,
            object([
                ("check_complete", Value::Bool(true)),
                ("eligible", Value::Bool(true)),
                ("verified", Value::Bool(false)),
                ("installation_writes_performed", Value::Bool(false)),
                (
                    "restart_clients",
                    Value::Array(plan.restart.iter().map(string).collect()),
                ),
            ]),
        ));
    }
    if same_revision && cache.is_none() {
        // The shared planner refreshes a stale outside-text snapshot without
        // reactivating identical code or restarting clients.
        wrapper_readback(&old, None)?;
    }
    let planned: Vec<(String, PathBuf)> = plan
        .actions
        .iter()
        .map(|action| (action.kind.clone(), action.path.clone()))
        .collect();
    let mut compatible_now = || -> Result<()> {
        services
            .updating()?
            .banks(&request.repo, &candidate_compatibility, false)?;
        Ok(())
    };
    let mut readback = || -> Result<()> {
        services
            .updating()?
            .banks(&request.repo, &candidate_compatibility, false)?;
        wrapper_readback(&old, Some(&planned))
    };
    let result = install::install(
        env,
        services,
        &mut plan,
        ActivationHooks {
            before_activate: Some(&mut compatible_now),
            after_activate: Some(&mut readback),
        },
    )?;
    Ok(merged(summary, result))
}

/// Baseline `run`.
pub fn run(env: &Environment, services: &Services, request: &UpdateRequest) -> Result<Value> {
    let locations = install::locations(env)?;
    let state = &locations.state;
    let before = install::read_receipt(env, state)?
        .ok_or_else(|| contract("No installation receipt; install the managed channel first"))?;
    require_rust_era(&before)?;
    // Managed Git channels refuse before any preflight, probe or staging.
    require(
        text(get(&before, "source"), "kind") != Some("managed"),
        MANAGED_GUIDANCE,
    )?;
    install::preflight_layout(
        env,
        services,
        Command::Update,
        &InstallRequest::default(),
        &locations,
        Some(&before),
    )?;
    let source = get(&before, "source");
    let mut scratch = None;
    let mut root = path_of(source, "path");
    if let (Some(binary), Some("release")) = (&request.bootstrap_binary, text(source, "kind")) {
        // The re-run bootstrap: bind and install exactly the verified bytes
        // at PATH (installation `stage_release`), never re-downloading them.
        let (staged_root, candidate) =
            install::stage_release(services, &install::absolute(env, binary)?)?;
        scratch = Some(staged_root);
        root = candidate;
    } else if text(source, "kind") == Some("release") {
        let access = &services.release;
        let observed = release::probe_with(
            &access.first_manifest_url,
            &access.assets,
            access.options,
            &access.signers,
        )?;
        let manifest = &observed.manifest;
        let identity = manifest.targets.get(TARGET);
        // A verified exact manifest needs no archive fetch: use the owned,
        // independently verified installed runtime for planning instead.
        let exact = text(source, "release_manifest_sha256")
            == Some(release::digest(&observed.raw).as_str())
            && text(source, "revision") == Some(manifest.source_commit.as_str())
            && identity.is_some_and(|identity| {
                text(source, "artifact_digest") == Some(identity.artifact_digest.as_str())
            })
            && text(source, "version") == Some(manifest.version.as_str());
        let reuse = exact && current(env, services, state, &locations.runtime, None).is_ok();
        if reuse {
            root = locations.runtime.clone();
        } else {
            let (acquired, candidate) = acquire_candidate(services, &observed)?;
            scratch = Some(acquired);
            root = candidate;
        }
    }
    let result = if request.check {
        work(env, services, request, &root)
    } else {
        let mut paths = vec![
            state.clone(),
            locations.code.clone(),
            locations.runtime.clone(),
        ];
        paths.extend(targets(&before).into_iter().map(|t| path_of(t, "path")));
        let bin_dir = path_of(&before, "bin_dir");
        paths.extend(install::COMMAND_NAMES.iter().map(|name| bin_dir.join(name)));
        if get(&before, "no_guidance") != &Value::Bool(true) {
            let agent = text(get(&before, "omp_binding"), "agent_dir");
            for (name, _, _) in
                install::presets(env, text(&before, "presets").unwrap_or("none"), agent)?
            {
                if let Some(hook) = install::hook_paths(env, &name)? {
                    paths.push(install::absolute(env, &hook)?);
                }
            }
        }
        crate::runtime_faults::operation("update", &paths, || {
            install::locked(env, state, || {
                // Retry a killed activation before selecting another candidate.
                // Never treat a journal's historical completion as current
                // ownership proof.
                install::recover(env, services, state, true, true)?;
                work(env, services, request, &root)
            })
        })
    };
    drop(scratch);
    result
}

/// `am update`: the typed baseline `execute`.
pub fn update_command(
    env: &Environment,
    services: &Services,
    request: &UpdateRequest,
) -> CommandResult {
    let outcome = run(env, services, request).and_then(|result| {
        crate::runtime_faults::response("update", &result)?;
        Ok(result)
    });
    let value = match outcome {
        Ok(value) => value,
        Err(error) => {
            let mut value = object([
                ("operation", string("update")),
                ("verified", Value::Bool(false)),
                ("error", string(error.to_string())),
                ("repair", string(install::failure_repair(env, &error))),
            ]);
            if let Some(details) = install::failure_details(&error) {
                value = merged(value, details);
            }
            value
        }
    };
    printed(value)
}

/// The production `installation::Updating` implementer: the real update
/// checks over the executing environment's signed release access.
pub struct Runtime {
    env: Environment,
    services: Services,
}

impl Runtime {
    pub fn new(
        env: Environment,
        artifacts: Arc<dyn ArtifactInspection>,
        release: ReleaseAccess,
    ) -> Self {
        Self {
            env,
            services: Services {
                updating: None,
                private_state: None,
                omp_boundary: None,
                artifacts,
                release,
                executable: None,
            },
        }
    }
    /// The process environment with the pinned release and registered artifacts.
    pub fn process() -> Self {
        Self::new(
            Environment::process(),
            Arc::new(RegisteredArtifacts),
            ReleaseAccess::default(),
        )
    }
}

impl install::Updating for Runtime {
    fn banks(
        &self,
        repositories: &[String],
        compatibility: &Value,
        require_readable: bool,
    ) -> Result<Value> {
        let runtime_root = install::locations(&self.env)?.runtime_pack();
        banks(
            &self.env,
            &runtime_root,
            repositories,
            compatibility,
            require_readable,
        )
    }
    fn qualified_current(&self, state: &Path, runtime: &Path) -> Result<Option<Qualification>> {
        Ok(qualified_current(&self.env, &self.services, state, runtime)?.1)
    }
    fn current(&self, state: &Path, runtime: &Path, cache: Option<&Qualification>) -> Result<()> {
        current(&self.env, &self.services, state, runtime, cache)?;
        Ok(())
    }
    fn release_transition(&self, saved: &Value, source: &Value, _root: &Path) -> Result<()> {
        release_transition(saved, source)
    }
    fn compatible_surface(
        &self,
        saved: &Value,
        source: &Value,
        _cache: Option<&Qualification>,
    ) -> Result<()> {
        compatible_surface(&self.services, saved, source)
    }
}
