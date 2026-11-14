//! Fresh bounded Git observations, deletion-aware merge plans and publication readback.
//! Plans are not semantic approval and never update canonical files or feature state.
use crate::bank_capacity::{BankCapacityError, CapacityCode, Measurement};
use crate::bank_contract as bank;
use crate::bank_transactions as transactions;
use crate::bank_writes as writes;
use crate::codec::{frontmatter, json::Value};
use crate::developer_identity::encoded;
use crate::error::{ContractError, Kind, Result};
use crate::lifecycle::{CommandResult, get, object, string};
use crate::process::{self, FileSize, Input, Spec, Status, Target};
use crate::scope_binding::{self, RequireOptions};
use crate::{host, repository_config, sharing_limits as limits};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::{OsStr, OsString};
use std::path::Path;
use std::time::Duration;

pub const MAX_FILES: usize = limits::MAX_FILES;
pub const MAX_BYTES: usize = limits::MAX_BANK;
pub const MAX_FILE: usize = limits::MAX_FILE;
pub const MAX_LOCATOR_TREE_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_REQUEST: usize = 65536;
pub const AUTHORITY_REPAIR: &str = "Check connectivity and access for the configured authority remote and local Git metadata. Preserve the bank, feature work and pending evidence; repeat the original refresh inspection after the issue clears. Do not reset/reinitialize memory or remove unknown locks.";
const CLEANUP_MESSAGE: &str = "Private observation ref cleanup failed; preserve Git metadata and inspect repository permissions before retrying.";

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn text(value: &Value) -> Result<&str> {
    value
        .as_str()
        .ok_or_else(|| err("Sharing state requires text"))
}
fn pairs(value: &Value) -> Result<&[(String, Value)]> {
    value
        .as_object()
        .ok_or_else(|| err("Sharing state requires a mapping"))
}
fn rows(value: &Value) -> Result<&[Value]> {
    value
        .as_array()
        .ok_or_else(|| err("Sharing state requires an array"))
}
fn same(a: &Value, b: &Value) -> Result<bool> {
    Ok(encoded(a)? == encoded(b)?)
}
fn authority(code: &str, component: &str, message: &str, repair: &str) -> ContractError {
    ContractError::sharing_authority(code, component, message, repair)
}
pub fn fetch_failure() -> ContractError {
    authority(
        "authority-fetch-unavailable",
        "authority-fetch",
        "Fetching the configured authority branch failed; no fresh remote observation was established.",
        AUTHORITY_REPAIR,
    )
}
/// Cleanup failure never replaces the primary typed refusal or reveals Git stderr.
pub fn finish_observation(primary: Result<Value>, cleanup: Result<()>) -> Result<Value> {
    if cleanup.is_ok() {
        return primary;
    }
    match primary {
        Ok(_) => Err(authority(
            "observation-cleanup-failed",
            "observation-cleanup",
            CLEANUP_MESSAGE,
            AUTHORITY_REPAIR,
        )),
        Err(error) => {
            let message = format!("{} {}", error.message(), CLEANUP_MESSAGE);
            match error.kind() {
                Kind::SharingAuthority {
                    code,
                    component,
                    repair,
                } => Err(authority(
                    code,
                    component,
                    &message,
                    &format!("{repair} {CLEANUP_MESSAGE}"),
                )),
                _ => Err(error.with_message(message)),
            }
        }
    }
}

/// Run with ambient Git redirection removed, no hooks, no lazy fetch, bounded pipes
/// and an owned direct child with the baseline's fixed 10/30 second deadline.
pub fn raw_command(
    repo: &Path,
    args: &[&str],
    network: bool,
    limit: usize,
    glob_pathspecs: bool,
    payload: Option<&[u8]>,
) -> Result<Vec<u8>> {
    let allowed = [
        "GIT_CONFIG_NOSYSTEM",
        "GIT_CONFIG_SYSTEM",
        "GIT_CONFIG_GLOBAL",
        "GIT_TERMINAL_PROMPT",
        "GIT_ASKPASS",
        "GIT_ALLOW_PROTOCOL",
    ];
    let mut env: BTreeMap<OsString, OsString> = std::env::vars_os()
        .filter(|(key, _)| {
            !key.as_encoded_bytes().starts_with(b"GIT_")
                || allowed.iter().any(|name| key == OsStr::new(name))
        })
        .collect();
    for (key, value) in [
        ("GIT_NO_LAZY_FETCH", "1"),
        ("GIT_OPTIONAL_LOCKS", "0"),
        ("GIT_TRACE", "0"),
        ("GIT_TRACE2", "0"),
        ("GIT_TRACE2_EVENT", "0"),
        ("GIT_TRACE2_PERF", "0"),
        ("GIT_TERMINAL_PROMPT", "0"),
    ] {
        env.insert(key.into(), value.into());
    }
    if !network {
        env.insert("GIT_ALLOW_PROTOCOL".into(), "".into());
    }
    let env: Vec<_> = env.into_iter().collect();
    let mut argv: Vec<OsString> = [
        "git",
        "--no-pager",
        "--no-replace-objects",
        if glob_pathspecs {
            "--glob-pathspecs"
        } else {
            "--literal-pathspecs"
        },
        "-c",
        "core.fsmonitor=false",
        "-c",
        "core.untrackedCache=false",
        "-c",
        "core.hooksPath=/dev/null",
        "-C",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    argv.push(repo.as_os_str().to_owned());
    argv.extend(args.iter().map(OsString::from));
    let reader = args
        .first()
        .is_some_and(|a| ["ls-tree", "cat-file", "ls-files"].contains(a));
    let timeout = Duration::from_secs(if network { 30 } else { 10 });
    let deadline = crate::clock::Deadline::after(timeout)
        .map_err(|_| err("Git sharing observation failed; retain local memory and retry"))?;
    let dev = crate::fs::Descriptor::open_directory(Path::new("/dev"))
        .map_err(|_| err("Git sharing observation failed; retain local memory and retry"))?;
    let null = dev
        .open_at(OsStr::new("null"), libc::O_WRONLY, 0)
        .map_err(|_| err("Git sharing observation failed; retain local memory and retry"))?;
    use std::os::fd::AsFd;
    let output = process::run(&Spec {
        argv: &argv,
        search_path: true,
        env: &env,
        cwd: None,
        stdin: payload.map(Input::Bytes).unwrap_or(Input::Null),
        stdout: Target::Capture { limit },
        stderr: Target::Descriptor(null.as_fd()),
        deadline,
        new_session: false,
        file_size_limit: reader.then_some(FileSize::Pinned(limit.saturating_add(1) as u64)),
        umask: None,
        pass_fds: &[],
    })
    .map_err(|failure| {
        if matches!(failure.reason, process::Refusal::CaptureLimit { .. }) {
            err("Git sharing observation failed or exceeded its bound; retain local memory")
        } else {
            err("Git sharing observation failed; retain local memory and retry")
        }
    })?;
    if output.status != Status::Code(0) {
        return Err(err(
            "Git sharing observation failed or exceeded its bound; retain local memory",
        ));
    }
    Ok(output.stdout)
}
pub fn command(
    repo: &Path,
    args: &[&str],
    network: bool,
    limit: usize,
    glob_pathspecs: bool,
) -> Result<String> {
    String::from_utf8(raw_command(
        repo,
        args,
        network,
        limit,
        glob_pathspecs,
        None,
    )?)
    .map_err(|_| err("Git sharing observation is not UTF-8"))
}
fn query(repo: &Path, args: &[&str]) -> Result<String> {
    command(repo, args, false, MAX_BYTES, false)
}
pub fn oid(value: &str) -> Result<&str> {
    if ![40, 64].contains(&value.len())
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(err("An immutable full Git object ID is required"));
    }
    Ok(value)
}
pub fn commit<'a>(repo: &Path, value: &'a str) -> Result<&'a str> {
    oid(value)?;
    if query(repo, &["cat-file", "-t", value])?.trim() != "commit" {
        return Err(err("Sharing snapshots must name commits"));
    }
    Ok(value)
}
pub fn bounded(files: &Value, allow_absent: bool, quarantine_records: bool) -> Result<Value> {
    limits::bank_size(files)?;
    if pairs(files)?.is_empty() && allow_absent {
        return Ok(object([
            ("project_id", Value::Null),
            ("entries", Value::Array(vec![])),
            ("routes", Value::Array(vec![])),
        ]));
    }
    let checked = bank::validate_files(files, None)?;
    for (path, body) in pairs(files)? {
        if !rows(get(
            &crate::security::scan(
                &string(format!("{path}\n{}", text(body)?)),
                crate::security::LIMIT,
            )?,
            "reasons",
        ))?
        .is_empty()
        {
            if quarantine_records
                && rows(get(&checked, "entries"))?.iter().any(|e| {
                    get(e, "path").as_str() == Some(path)
                        && get(e, "format").as_str() == Some("standalone-record")
                })
            {
                let (_, metadata, _) = crate::records::parse(text(body)?, &[])?;
                let raw = encoded(&metadata)?;
                let metadata =
                    std::str::from_utf8(&raw).map_err(|_| err("Unsafe record metadata"))?;
                if rows(get(
                    &crate::security::scan(
                        &string(format!("{path}\n{metadata}")),
                        crate::security::LIMIT,
                    )?,
                    "reasons",
                ))?
                .is_empty()
                {
                    continue;
                }
            }
            return Err(err(
                "Unsafe or unreviewable shared memory; inspect without echoing content",
            ));
        }
    }
    Ok(checked)
}
fn capacity(
    code: CapacityCode,
    path: &str,
    limit: usize,
    observed: usize,
    exact: bool,
) -> ContractError {
    BankCapacityError {
        code,
        path: path.into(),
        limit,
        observed,
        measurement: if exact {
            Measurement::Exact
        } else {
            Measurement::AtLeast
        },
        unit: code.unit(),
    }
    .into()
}
#[derive(Clone)]
struct TreeEntry {
    mode: String,
    kind: String,
    blob: String,
    path: String,
}
fn tree_entries(raw: &str) -> Result<Vec<TreeEntry>> {
    raw.split('\0')
        .filter(|s| !s.is_empty())
        .map(|item| {
            let (header, path) = item
                .split_once('\t')
                .ok_or_else(|| err("Unreviewable authority path inventory"))?;
            let mut fields = header.split_whitespace();
            let mode = fields.next().ok_or_else(|| err("Incomplete tree header"))?;
            let kind = fields.next().ok_or_else(|| err("Incomplete tree header"))?;
            let blob = fields.next().ok_or_else(|| err("Incomplete tree header"))?;
            if fields.next().is_some() {
                return Err(err("Incomplete tree header"));
            }
            Ok(TreeEntry {
                mode: mode.into(),
                kind: kind.into(),
                blob: oid(blob)?.into(),
                path: path.into(),
            })
        })
        .collect()
}
fn batch_header(line: &[u8], expected: &str) -> Result<usize> {
    let line = std::str::from_utf8(line)
        .map_err(|_| err("Batch object identity/type is not the requested blob"))?;
    let fields: Vec<_> = line.split_whitespace().collect();
    if fields.len() != 3 || fields.first() != Some(&expected) || fields.get(1) != Some(&"blob") {
        return Err(err("Batch object identity/type is not the requested blob"));
    }
    let size = fields
        .get(2)
        .ok_or_else(|| err("Incomplete batch object header"))?;
    if size.is_empty() || !size.bytes().all(|b| b.is_ascii_digit()) {
        return Err(err("Batch size inventory must name exact requested blobs"));
    }
    size.parse()
        .map_err(|_| err("Batch object size exceeds bounds"))
}
/// SHA-1 is only Git object framing verification, never an authority signature.
fn git_sha1(prefix: &[u8], body: &[u8]) -> String {
    let mut hash = sha1::Sha1::new();
    hash.update(prefix);
    hash.update(body);
    hex(hash.finalize())
}
fn hex(bytes: impl IntoIterator<Item = u8>) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let bytes = bytes.into_iter();
    let mut output = String::with_capacity(bytes.size_hint().0 * 2);
    for byte in bytes {
        // Invariant: splitting a byte into nibbles always indexes the 16 hex digits.
        output.push(DIGITS[usize::from(byte >> 4)] as char);
        output.push(DIGITS[usize::from(byte & 15)] as char);
    }
    output
}
pub fn tree(repo: &Path, revision: &str, prefix: &str, quarantine_records: bool) -> Result<Value> {
    commit(repo, revision)?;
    let listing = command(
        repo,
        &["ls-tree", "-rz", "--full-tree", revision, "--", prefix],
        false,
        MAX_LOCATOR_TREE_BYTES,
        false,
    )?;
    let mut objects = Vec::new();
    let mut paths = BTreeSet::new();
    for entry in tree_entries(&listing)? {
        let path = entry
            .path
            .strip_prefix(&format!("{prefix}/"))
            .ok_or_else(|| err("Nonordinary shared bank tree requires review"))?;
        if entry.mode != "100644" || entry.kind != "blob" {
            return Err(err("Nonordinary shared bank tree requires review"));
        }
        if path != "MEMORY.md" {
            bank::path_value(&string(path))?;
        }
        if bank::runtime_path(path) || !paths.insert(path.to_owned()) {
            return Err(err(
                "Excluded runtime artifacts or duplicate bank tree paths require review",
            ));
        }
        if objects.len() >= MAX_FILES {
            return Err(capacity(
                CapacityCode::SourceCount,
                path,
                MAX_FILES,
                objects.len() + 1,
                false,
            ));
        }
        objects.push((path.to_owned(), entry.blob));
    }
    let mut blob_paths = BTreeMap::new();
    for (path, blob) in &objects {
        blob_paths.entry(blob.clone()).or_insert(path.clone());
    }
    let unique: Vec<_> = blob_paths.keys().cloned().collect();
    let mut blobs = BTreeMap::new();
    if !unique.is_empty() {
        let payload = format!("{}\n", unique.join("\n"));
        let metadata = raw_command(
            repo,
            &["cat-file", "--batch-check"],
            false,
            unique.len() * 128,
            false,
            Some(payload.as_bytes()),
        )?;
        let lines: Vec<_> = metadata
            .split(|b| *b == b'\n')
            .filter(|s| !s.is_empty())
            .collect();
        if lines.len() != unique.len() {
            return Err(err("Incomplete batch object size inventory"));
        }
        let mut total = 0usize;
        let mut sizes = BTreeMap::new();
        for (expected, line) in unique.iter().zip(lines) {
            let size = batch_header(line, expected)?;
            total = total
                .checked_add(size)
                .ok_or_else(|| err("Shared object exceeds bounds"))?;
            let path = blob_paths
                .get(expected)
                .ok_or_else(|| err("Incomplete batch inventory"))?;
            if size > MAX_FILE {
                return Err(capacity(
                    CapacityCode::SourceBytes,
                    path,
                    MAX_FILE,
                    size,
                    true,
                ));
            }
            if total > MAX_BYTES {
                return Err(capacity(
                    CapacityCode::AggregateBytes,
                    ".",
                    MAX_BYTES,
                    total,
                    false,
                ));
            }
            sizes.insert(expected.clone(), size);
        }
        let raw = raw_command(
            repo,
            &["cat-file", "--batch"],
            false,
            total + unique.len() * 128,
            false,
            Some(payload.as_bytes()),
        )?;
        let mut remaining = raw.as_slice();
        for expected in &unique {
            let end = remaining
                .iter()
                .take(128)
                .position(|b| *b == b'\n')
                .ok_or_else(|| err("Incomplete batch object header"))?;
            let header = remaining
                .get(..end)
                .ok_or_else(|| err("Incomplete batch object header"))?;
            let size = batch_header(header, expected)?;
            if sizes.get(expected) != Some(&size) {
                return Err(err(
                    "Shared object exceeds bounds or has incomplete framing",
                ));
            }
            remaining = remaining
                .get(end + 1..)
                .ok_or_else(|| err("Incomplete batch object header"))?;
            let body = remaining
                .get(..size)
                .ok_or_else(|| err("Shared object exceeds bounds or has incomplete framing"))?;
            if remaining.get(size) != Some(&b'\n') {
                return Err(err(
                    "Shared object exceeds bounds or has incomplete framing",
                ));
            }
            let header = format!("blob {size}\0");
            let actual = if expected.len() == 40 {
                git_sha1(header.as_bytes(), body)
            } else {
                let mut hash = Sha256::new();
                hash.update(header.as_bytes());
                hash.update(body);
                hex(hash.finalize())
            };
            if actual != *expected {
                return Err(err(
                    "Shared object bytes do not match their actual Git identity",
                ));
            }
            blobs.insert(
                expected.clone(),
                string(std::str::from_utf8(body).map_err(|_| err("Shared object is not UTF-8"))?),
            );
            remaining = remaining
                .get(size + 1..)
                .ok_or_else(|| err("Incomplete batch framing"))?;
        }
        if !remaining.is_empty() {
            return Err(err("Unexpected trailing batch object data"));
        }
    }
    let files = Value::Object(
        objects
            .into_iter()
            .map(|(path, blob)| {
                Ok((
                    path,
                    blobs
                        .get(&blob)
                        .cloned()
                        .ok_or_else(|| err("Incomplete batch inventory"))?,
                ))
            })
            .collect::<Result<_>>()?,
    );
    if pairs(&files)?.is_empty() {
        if !query(repo, &["rev-list", "--max-count=1", revision, "--", prefix])?
            .trim()
            .is_empty()
        {
            return Err(err(
                "Previously occupied shared bank is absent; inspect its history before initialization",
            ));
        }
    } else {
        bounded(&files, false, quarantine_records)?;
    }
    Ok(files)
}
pub fn server(repo: &Path, remote: &str, branch: &str) -> Result<String> {
    let branch_ref = format!("refs/heads/{branch}");
    let response = command(repo, &["ls-remote", "--symref", "--", remote, "HEAD", &branch_ref], true, 65536, false).map_err(|_| authority("authority-advertisement-unavailable", "authority-advertisement", "Reading the configured authority branch advertisement failed; no fresh remote observation was established.", AUTHORITY_REPAIR))?;
    let (mut default, mut observed) = (None, None);
    for line in response.lines() {
        let (left, reference) = line.split_once('\t').ok_or_else(|| {
            err("Configured authority differs from the server default or is unresolved")
        })?;
        if reference == "HEAD" {
            default = left.strip_prefix("ref: refs/heads/");
        } else if reference == branch_ref {
            observed = Some(oid(left)?.to_owned());
        }
    }
    if default != Some(branch) {
        return Err(err(
            "Configured authority differs from the server default or is unresolved",
        ));
    }
    observed
        .ok_or_else(|| err("Configured authority differs from the server default or is unresolved"))
}
pub fn authority_bank_location(repo: &Path, revision: &str, selected: Option<&str>) -> Result<()> {
    let output = command(
        repo,
        &["ls-tree", "-rz", "--full-tree", revision],
        false,
        MAX_LOCATOR_TREE_BYTES,
        false,
    )?;
    let entries = tree_entries(&output)?;
    if entries.len() > repository_config::MAX_DISCOVERY_ENTRIES {
        return Err(err(
            "Authority locator inventory exceeds its complete metadata bound",
        ));
    }
    let mut locators = Vec::new();
    let mut contexts: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for entry in entries {
        let path = entry.path.as_str();
        if path.starts_with('/') || path.split('/').any(|s| s == "..") {
            return Err(err("Unreviewable authority path inventory"));
        }
        if path.split('/').any(|s| s.starts_with('.')) {
            continue;
        }
        let (parent, name) = path.rsplit_once('/').unwrap_or((".", path));
        if bank::CONTEXTS.contains(&name) {
            contexts
                .entry(parent.into())
                .or_default()
                .insert(name.into());
        }
        if crate::unicode::casefold(name) == "memory.md" {
            if entry.mode != "100644" || entry.kind != "blob" {
                return Err(err(
                    "Nonordinary authority memory locator requires inspection",
                ));
            }
            locators.push((parent.to_owned(), name.to_owned(), path.to_owned()));
            if locators.len() > repository_config::MAX_ROUTERS {
                return Err(err(
                    "Authority has too many possible memory locators for complete discovery",
                ));
            }
        }
    }
    let mut cache = BTreeMap::new();
    for (parent, name, path) in &locators {
        let raw = repository_config::object_router_bytes(
            repo,
            OsStr::new(&format!("{revision}:{path}")),
            &mut cache,
        )?;
        if repository_config::router_kind(&raw)?.is_some() {
            if name != "MEMORY.md" {
                return Err(err(
                    "Authority memory locator spelling requires explicit reconciliation",
                ));
            }
            if Some(parent.as_str()) != selected {
                return Err(err(
                    "Authority already contains a bank at another location; explicitly reconcile identity and location",
                ));
            }
        }
    }
    if contexts.iter().any(|(parent, names)| {
        Some(parent.as_str()) != selected
            && (names.len() >= 2 || locators.iter().any(|(p, _, _)| p == parent))
    }) {
        return Err(err(
            "Possible damaged authority bank requires inspection before declaring absence",
        ));
    }
    Ok(())
}
pub fn first_bank_history(repo: &Path, head: &str) -> Result<()> {
    let revisions = command(
        repo,
        &[
            "rev-list",
            "--first-parent",
            "--max-count=129",
            head,
            "--",
            ":(glob,icase)**/MEMORY.md",
        ],
        false,
        MAX_BYTES,
        true,
    )?;
    if revisions.lines().count() > 128 {
        return Err(err(
            "Historical authority locator changes exceed the complete review bound",
        ));
    }
    let mut seen = BTreeSet::new();
    for revision in revisions.lines() {
        let parents = query(repo, &["rev-list", "--parents", "-n", "1", revision])?;
        for snapshot in std::iter::once(revision).chain(parents.split_whitespace().skip(1).take(1))
        {
            if seen.insert(snapshot.to_owned()) {
                authority_bank_location(repo, snapshot, None)?;
            }
        }
    }
    Ok(())
}
pub fn observe(repo: &Path, owned_lock: Option<&[u8]>, quarantine_records: bool) -> Result<Value> {
    let (root, _) = writes::bank_path(repo)?;
    let context = scope_binding::require(
        &root,
        None,
        RequireOptions {
            owned_lock,
            ..RequireOptions::default()
        },
    )?;
    let boundary = crate::sharing_boundary::inspect(&root, owned_lock)?;
    let mut allowed = BTreeSet::from(["canonical-memory-is-not-tracked"]);
    if quarantine_records
        && !rows(get(&boundary, "safety_reasons"))?
            .iter()
            .any(|v| v.as_str() == Some("unadmitted-content"))
    {
        bounded(
            &transactions::files_at(&root.join(text(get(&boundary, "bank_root"))?))?,
            false,
            true,
        )?;
        allowed.insert("unsafe-or-unreviewable-memory-content");
    }
    if rows(get(&boundary, "blockers"))?
        .iter()
        .any(|v| !v.as_str().is_some_and(|s| allowed.contains(s)))
    {
        return Err(err(
            "Resolve the listed local sharing boundary blockers before exchange",
        ));
    }
    let local_head = query(&root, &["rev-parse", "--verify", "HEAD"])?
        .trim()
        .to_owned();
    let remote = text(get(&boundary, "authority_remote"))?;
    let branch = text(get(&boundary, "authority_branch"))?;
    let prefix = text(get(&boundary, "bank_root"))?;
    let head = server(&root, remote, branch)?;
    let reference = format!("refs/agent-memory/observations/{}", host::uuid4()?);
    let primary = (|| {
        let refspec = format!("refs/heads/{branch}:{reference}");
        command(
            &root,
            &[
                "fetch",
                "--no-tags",
                "--no-write-fetch-head",
                "--refmap=",
                "--no-recurse-submodules",
                "--",
                remote,
                &refspec,
            ],
            true,
            65536,
            false,
        )
        .map_err(|_| fetch_failure())?;
        writes::fault("sharing-after-fetch")?;
        let fetched = query(&root, &["rev-parse", "--verify", &reference])?;
        if fetched.trim() != head || server(&root, remote, branch)? != head {
            return Err(err(
                "Authority advanced during observation; reconcile against a new observation",
            ));
        }
        authority_bank_location(&root, &head, Some(prefix))?;
        let files = tree(&root, &head, prefix, quarantine_records)?;
        let present = !pairs(&files)?.is_empty();
        if !present {
            first_bank_history(&root, &head)?;
        }
        let router_raw = limits::read_bytes(&root.join(prefix).join("MEMORY.md"), MAX_FILE)?;
        let router_text =
            std::str::from_utf8(&router_raw).map_err(|_| err("Local router is not UTF-8"))?;
        let local_router = bank::parse_router(router_text)?.0;
        if present {
            let shared_router = bank::parse_router(text(get(&files, "MEMORY.md"))?)?.0;
            if get(&bounded(&files, false, quarantine_records)?, "project_id")
                != get(&context, "project_id")
                || !same(
                    get(&shared_router, "canonical-remote"),
                    get(&local_router, "canonical-remote"),
                )?
            {
                return Err(err("Shared authority belongs to a different project"));
            }
        }
        scope_binding::require(
            &root,
            Some(text(get(&context, "project_id"))?),
            RequireOptions {
                expected: Some(&context),
                owned_lock,
                ..RequireOptions::default()
            },
        )?;
        if !same(
            &crate::sharing_boundary::inspect(&root, owned_lock)?,
            &boundary,
        )? || query(&root, &["rev-parse", "--verify", "HEAD"])?.trim() != local_head
        {
            return Err(err(
                "Local memory or boundary changed during remote observation",
            ));
        }
        Ok(object([
            (
                "repo_root",
                string(host::text(root.as_os_str(), "repository root")?),
            ),
            ("project_id", get(&context, "project_id").clone()),
            ("bank_root", string(prefix)),
            ("authority_remote", string(remote)),
            ("authority_branch", string(branch)),
            ("authority_head", string(&head)),
            ("local_head", string(local_head)),
            ("scope_context", context),
            (
                "local_bank_sha256",
                get(&boundary, "bank_observation_sha256").clone(),
            ),
            ("landed_hashes", transactions::file_hashes(&files)?),
            ("shared_bank_present", Value::Bool(present)),
            ("observed_at", string(host::utc_now_isoformat()?)),
            ("remote_freshness_verified", Value::Bool(true)),
        ]))
    })();
    let cleanup = query(&root, &["update-ref", "-d", &reference]).map(|_| ());
    finish_observation(primary, cleanup)
}
pub fn result(operation: &str, values: Value) -> Result<Value> {
    let mut value = object([
        ("schema", string("agent-memory/sharing-recovery-v1")),
        ("operation", string(operation)),
        ("writes_performed", Value::Bool(false)),
        ("git_metadata_writes_performed", Value::Bool(false)),
        ("canonical_writes_performed", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
        ("feature_code_freshness_verified", Value::Bool(false)),
    ]);
    bank::merge(&mut value, &values)?;
    Ok(value)
}
pub fn inspect(repo: &Path, fetch: bool) -> Result<Value> {
    let boundary = crate::sharing_boundary::inspect(repo, None)?;
    if !fetch {
        return result(
            "inspect",
            object([
                ("sharing_state", string("pending-stale")),
                ("remote_freshness_verified", Value::Bool(false)),
                ("local_memory_available", Value::Bool(true)),
                ("boundary", boundary),
            ]),
        );
    }
    match observe(repo, None, false) {
        Ok(observation) => result(
            "inspect",
            object([
                ("writes_performed", Value::Bool(true)),
                ("git_metadata_writes_performed", Value::Bool(true)),
                ("sharing_state", string("observed")),
                ("observation", observation),
                ("remote_freshness_verified", Value::Bool(true)),
                ("local_memory_available", Value::Bool(true)),
            ]),
        ),
        Err(error)
            if matches!(
                error.kind(),
                Kind::BankCapacity { .. } | Kind::MissingProvider
            ) =>
        {
            Err(error)
        }
        Err(_) => result(
            "inspect",
            object([
                ("writes_performed", Value::Null),
                ("git_metadata_writes_performed", Value::Null),
                ("sharing_state", string("pending-stale")),
                ("remote_freshness_verified", Value::Bool(false)),
                ("local_memory_available", Value::Bool(true)),
                ("boundary", boundary),
                (
                    "reason",
                    string(
                        "Remote unavailable, changed or unverified; retain local capture and retry",
                    ),
                ),
            ]),
        ),
    }
}

/// Missing is distinct from JSON null and from an empty file/map.
pub fn choose<T: PartialEq + Clone>(base: &T, local: &T, landed: &T) -> Result<T> {
    if local == landed {
        Ok(local.clone())
    } else if local == base {
        Ok(landed.clone())
    } else if landed == base {
        Ok(local.clone())
    } else {
        Err(err(
            "Concurrent changes require semantic reconciliation by the detecting session",
        ))
    }
}
fn choose_value(
    base: Option<&Value>,
    local: Option<&Value>,
    landed: Option<&Value>,
) -> Result<Option<Value>> {
    let equal = |a: Option<&Value>, b: Option<&Value>| -> Result<bool> {
        match (a, b) {
            (Some(a), Some(b)) => same(a, b),
            (None, None) => Ok(true),
            _ => Ok(false),
        }
    };
    if equal(local, landed)? || equal(landed, base)? {
        Ok(local.cloned())
    } else if equal(local, base)? {
        Ok(landed.cloned())
    } else {
        Err(err(
            "Concurrent changes require semantic reconciliation by the detecting session",
        ))
    }
}
pub fn mapping(base: &Value, local: &Value, landed: &Value) -> Result<Value> {
    let mut keys = BTreeSet::new();
    for value in [base, local, landed] {
        keys.extend(pairs(value)?.iter().map(|(key, _)| key.clone()));
    }
    let mut merged = Vec::new();
    for key in keys {
        if let Some(value) = choose_value(base.get(&key), local.get(&key), landed.get(&key))? {
            merged.push((key, value));
        }
    }
    Ok(Value::Object(merged))
}
#[derive(Clone)]
struct Parts {
    meta: Value,
    routes: Value,
    omitted: Value,
    prefix: Option<String>,
}
fn route_mapping(routes: Vec<Value>) -> Result<Value> {
    Ok(Value::Object(
        routes
            .into_iter()
            .map(|r| Ok((text(get(&r, "path"))?.to_owned(), r)))
            .collect::<Result<_>>()?,
    ))
}
fn router_parts(body: &str) -> Result<Parts> {
    let (meta, routes, omitted) = bank::parse_router(body)?;
    let (_, body) = frontmatter::read(body, repository_config::ROUTER_FIELDS)?;
    Ok(Parts {
        meta,
        routes: route_mapping(routes)?,
        omitted,
        prefix: Some(
            body.split_once("\n## Routes\n")
                .ok_or_else(|| err("Router lacks routes"))?
                .0
                .into(),
        ),
    })
}
fn index_parts(path: &str, body: &str) -> Result<Parts> {
    let (meta, body) = frontmatter::read(body, &["schema", "project-id", "updated"])?;
    let meta_value = Value::Object(meta);
    let routes = crate::bank_layout::parse_index(path, &meta_value, body)?;
    Ok(Parts {
        meta: meta_value,
        routes: route_mapping(routes)?,
        omitted: object([] as [(&str, Value); 0]),
        prefix: Some(
            body.split_once("\n## Routes\n")
                .ok_or_else(|| err("Index lacks routes"))?
                .0
                .into(),
        ),
    })
}
fn absent_parts() -> Parts {
    Parts {
        meta: Value::Object(vec![]),
        routes: Value::Object(vec![]),
        omitted: Value::Object(vec![]),
        prefix: None,
    }
}
fn filtered(value: &Value, keep: impl Fn(&str) -> bool) -> Result<Value> {
    Ok(Value::Object(
        pairs(value)?
            .iter()
            .filter(|(key, _)| keep(key))
            .cloned()
            .collect(),
    ))
}
pub fn header_order(metadata: &Value, originals: &[Value]) -> Result<Value> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for original in originals.iter().chain(std::iter::once(metadata)) {
        for (key, _) in pairs(original)? {
            if seen.insert(key.clone()) {
                if let Some(value) = metadata.get(key) {
                    result.push((key.clone(), value.clone()));
                }
            }
        }
    }
    Ok(Value::Object(result))
}
fn merge_parts(
    parts: &[Parts; 3],
    timestamp: &str,
    drops: &BTreeSet<String>,
) -> Result<(Value, Value, String)> {
    let [base, local, landed] = parts;
    let mut meta = mapping(
        &filtered(&base.meta, |k| k != timestamp)?,
        &filtered(&local.meta, |k| k != timestamp)?,
        &filtered(&landed.meta, |k| k != timestamp)?,
    )?;
    let updated = parts
        .iter()
        .filter_map(|p| p.meta.get(timestamp))
        .map(text)
        .collect::<Result<Vec<_>>>()?
        .into_iter()
        .max()
        .ok_or_else(|| err("Structural metadata lacks update timestamp"))?;
    bank::merge(&mut meta, &object([(timestamp, string(updated))]))?;
    meta = header_order(
        &meta,
        &parts.iter().map(|p| p.meta.clone()).collect::<Vec<_>>(),
    )?;
    let routes = mapping(
        &filtered(&base.routes, |k| !drops.contains(k))?,
        &filtered(&local.routes, |k| !drops.contains(k))?,
        &filtered(&landed.routes, |k| !drops.contains(k))?,
    )?;
    let prefix = choose(&base.prefix, &local.prefix, &landed.prefix)?
        .ok_or_else(|| err("Structural introduction is missing"))?;
    Ok((meta, routes, prefix))
}
fn order_index(
    path: &str,
    maps: &[Value; 3],
    structural: &BTreeSet<String>,
    visiting: &mut BTreeSet<String>,
    visited: &mut BTreeSet<String>,
    ordered: &mut Vec<String>,
) -> Result<()> {
    if visiting.contains(path) {
        return Err(err(
            "Combined index topology requires explicit reconciliation",
        ));
    }
    if visited.contains(path) {
        return Ok(());
    }
    visiting.insert(path.into());
    for files in maps {
        if let Some(value) = files.get(path) {
            for (child, _) in pairs(&index_parts(path, text(value)?)?.routes)? {
                if structural.contains(child) && child != "MEMORY.md" {
                    order_index(child, maps, structural, visiting, visited, ordered)?;
                }
            }
        }
    }
    visiting.remove(path);
    visited.insert(path.into());
    ordered.push(path.into());
    Ok(())
}
pub fn merge_files(
    base: &Value,
    local: &Value,
    landed: &Value,
    quarantine_records: bool,
) -> Result<Value> {
    let checks = [
        bounded(base, true, quarantine_records)?,
        bounded(local, true, quarantine_records)?,
        bounded(landed, true, quarantine_records)?,
    ];
    let projects: BTreeSet<_> = checks
        .iter()
        .filter_map(|c| get(c, "project_id").as_str())
        .collect();
    if projects.len() != 1 {
        return Err(err(
            "A reconciliation cannot transfer memory between projects",
        ));
    }
    let sources = [base, local, landed];
    let mut tombs = Value::Object(vec![]);
    let mut removed = BTreeSet::new();
    for (files, checked) in sources.iter().zip(&checks) {
        for entry in rows(get(checked, "entries"))? {
            if get(entry, "format").as_str() == Some("tombstone") {
                let path = text(get(entry, "path"))?;
                let body = files.get(path).ok_or_else(|| err("Missing tombstone"))?;
                if tombs.get(path).is_some_and(|v| v != body) {
                    return Err(err(
                        "Different deletion authorities require explicit reconciliation",
                    ));
                }
                bank::merge(&mut tombs, &object([(path, body.clone())]))?;
                removed.insert(text(get(entry, "id"))?.to_owned());
            }
        }
    }
    let mut drops = BTreeSet::new();
    let mut maps = [base.clone(), local.clone(), landed.clone()];
    for (current, checked) in maps.iter_mut().zip(&checks) {
        for entry in rows(get(checked, "entries"))? {
            if get(entry, "format").as_str() != Some("standalone-record") {
                continue;
            }
            let path = text(get(entry, "path"))?;
            if removed.contains(text(get(entry, "id"))?) {
                drops.insert(path.to_owned());
                *current = filtered(current, |k| k != path)?;
            } else {
                let (_, metadata, _) = crate::records::parse(text(get(current, path))?, &[])?;
                if get(&metadata, "derived-from")
                    .as_array()
                    .unwrap_or(&[])
                    .iter()
                    .any(|reference| {
                        get(reference, "id")
                            .as_str()
                            .is_some_and(|id| removed.contains(id))
                    })
                {
                    return Err(err(
                        "A derived observation of deleted learning requires explicit erasure review",
                    ));
                }
            }
        }
    }
    let mut structural = BTreeSet::from(["MEMORY.md".to_owned()]);
    for checked in &checks {
        for entry in rows(get(checked, "entries"))? {
            if get(entry, "format").as_str() == Some("topic-index") {
                structural.insert(text(get(entry, "path"))?.to_owned());
            }
        }
    }
    let payloads: Vec<_> = maps
        .iter()
        .map(|files| filtered(files, |p| !structural.contains(p) && tombs.get(p).is_none()))
        .collect::<Result<_>>()?;
    let mut payloads = payloads.iter();
    let base_payload = payloads.next().ok_or_else(|| err("Missing base payload"))?;
    let local_payload = payloads
        .next()
        .ok_or_else(|| err("Missing local payload"))?;
    let landed_payload = payloads
        .next()
        .ok_or_else(|| err("Missing authority payload"))?;
    let mut merged = mapping(base_payload, local_payload, landed_payload)?;
    bank::merge(&mut merged, &tombs)?;
    let (mut visiting, mut visited, mut ordered) = (BTreeSet::new(), BTreeSet::new(), Vec::new());
    for path in structural.iter().filter(|p| p.as_str() != "MEMORY.md") {
        order_index(
            path,
            &maps,
            &structural,
            &mut visiting,
            &mut visited,
            &mut ordered,
        )?;
    }
    for path in ordered {
        let [base_map, local_map, landed_map] = &maps;
        let values = [
            base_map.get(&path),
            local_map.get(&path),
            landed_map.get(&path),
        ];
        let (meta, routes, prefix) = match values {
            [None, Some(local), Some(landed)] => merge_parts(
                &[
                    absent_parts(),
                    index_parts(&path, text(local)?)?,
                    index_parts(&path, text(landed)?)?,
                ],
                "updated",
                &drops,
            )?,
            [Some(base), Some(local), Some(landed)] => merge_parts(
                &[
                    index_parts(&path, text(base)?)?,
                    index_parts(&path, text(local)?)?,
                    index_parts(&path, text(landed)?)?,
                ],
                "updated",
                &drops,
            )?,
            [base, local, landed] => {
                let selected = choose_value(base, local, landed)?;
                let Some(selected) = selected else {
                    continue;
                };
                let parts = index_parts(&path, text(&selected)?)?;
                (
                    parts.meta,
                    parts.routes,
                    parts
                        .prefix
                        .ok_or_else(|| err("Index lacks introduction"))?,
                )
            }
        };
        let routes = filtered(&routes, |p| merged.get(p).is_some())?;
        if pairs(&routes)?.is_empty() {
            if prefix
                != "# Topic index\n\nLabels name bank-relative files; links are relative to this index.\n"
            {
                return Err(err(
                    "An empty custom index needs explicit preservation before removal",
                ));
            }
            drops.insert(path);
            continue;
        }
        let mut body = frontmatter::write(pairs(&meta)?)?
            + &prefix
            + "\n## Routes\n\n"
            + &bank::route_table(
                &pairs(&routes)?
                    .iter()
                    .map(|(_, v)| v.clone())
                    .collect::<Vec<_>>(),
                &path,
            )?;
        for source in &maps {
            if let Some(original) = source.get(&path) {
                let part = index_parts(&path, text(original)?)?;
                if same(&part.meta, &meta)?
                    && same(&part.routes, &routes)?
                    && part.prefix.as_deref() == Some(&prefix)
                {
                    body = text(original)?.into();
                    break;
                }
            }
        }
        bank::merge(&mut merged, &object([(path, string(body))]))?;
    }
    let parse = |files: &Value| -> Result<Parts> {
        if pairs(files)?.is_empty() {
            Ok(absent_parts())
        } else {
            router_parts(text(get(files, "MEMORY.md"))?)
        }
    };
    let [base_map, local_map, landed_map] = &maps;
    let parts = [parse(base_map)?, parse(local_map)?, parse(landed_map)?];
    let (meta, routes, prefix) = merge_parts(&parts, "router-updated", &drops)?;
    let mut routes = filtered(&routes, |p| merged.get(p).is_some())?;
    for (path, _) in pairs(&tombs)? {
        for part in &parts {
            if let Some(route) = part.routes.get(path) {
                bank::merge(&mut routes, &object([(path, route.clone())]))?;
                break;
            }
        }
    }
    let mut omissions = Value::Object(vec![]);
    let [base_part, local_part, landed_part] = &parts;
    for path in bank::CONTEXTS.iter() {
        if merged.get(path).is_none() {
            let reason = choose_value(
                base_part.omitted.get(path),
                local_part.omitted.get(path),
                landed_part.omitted.get(path),
            )?
            .ok_or_else(|| err("A removed ordinary context requires its omission explanation"))?;
            bank::merge(&mut omissions, &object([(*path, reason)]))?;
        }
    }
    let mut body = frontmatter::write(pairs(&meta)?)?
        + &prefix
        + "\n## Routes\n\n"
        + &bank::route_table(
            &pairs(&routes)?
                .iter()
                .map(|(_, v)| v.clone())
                .collect::<Vec<_>>(),
            "MEMORY.md",
        )?
        + "\n## Omitted contexts\n\n";
    if pairs(&omissions)?.is_empty() {
        body.push_str("None.\n");
    } else {
        for path in bank::CONTEXTS.iter() {
            if let Some(reason) = omissions.get(path) {
                body.push_str(&format!("- `{path}`: {}\n", text(reason)?));
            }
        }
    }
    for (source, part) in maps.iter().zip(&parts) {
        if !pairs(source)?.is_empty()
            && same(&part.meta, &meta)?
            && same(&part.routes, &routes)?
            && same(&part.omitted, &omissions)?
            && part.prefix.as_deref() == Some(&prefix)
        {
            body = text(get(source, "MEMORY.md"))?.into();
            break;
        }
    }
    bank::merge(&mut merged, &object([("MEMORY.md", string(body))]))?;
    let final_check = bounded(&merged, false, quarantine_records)?;
    let mut inactive: BTreeMap<String, u64> = BTreeMap::new();
    for checked in &checks {
        for entry in rows(get(checked, "entries"))? {
            if get(entry, "format").as_str() == Some("standalone-record")
                && get(entry, "lifecycle").as_str() != Some("active")
            {
                let version = get(entry, "version")
                    .as_int()
                    .and_then(|n| n.as_u64())
                    .ok_or_else(|| err("Record version is invalid"))?;
                let previous = inactive.entry(text(get(entry, "id"))?.into()).or_default();
                *previous = (*previous).max(version);
            }
        }
    }
    for entry in rows(get(&final_check, "entries"))? {
        if get(entry, "format").as_str() == Some("standalone-record")
            && get(entry, "lifecycle").as_str() == Some("active")
            && get(entry, "version")
                .as_int()
                .and_then(|n| n.as_u64())
                .ok_or_else(|| err("Record version is invalid"))?
                <= inactive.get(text(get(entry, "id"))?).copied().unwrap_or(0)
        {
            return Err(err(
                "Inactive guidance cannot reactivate through an old version; review fresh lifecycle evidence",
            ));
        }
    }
    Ok(merged)
}
pub fn ancestor(repo: &Path, older: &str, newer: &str) -> Result<bool> {
    Ok(query(
        repo,
        &["rev-list", "--max-count=1", older, &format!("^{newer}")],
    )?
    .trim()
    .is_empty())
}
pub fn reconcile(repo: &Path, base: Option<&str>) -> Result<Value> {
    let observation = observe(repo, None, false)?;
    let root = Path::new(text(get(&observation, "repo_root"))?);
    let prefix = text(get(&observation, "bank_root"))?;
    let head = text(get(&observation, "authority_head"))?;
    let base = if let Some(base) = base {
        base.to_owned()
    } else {
        let ancestors = query(
            root,
            &[
                "merge-base",
                "--all",
                text(get(&observation, "local_head"))?,
                head,
            ],
        )?;
        let mut lines = ancestors.lines();
        let base = lines.next().ok_or_else(|| {
            err("Lost cursor has no unique common base; inspect instead of guessing")
        })?;
        if lines.next().is_some() {
            return Err(err(
                "Lost cursor has no unique common base; inspect instead of guessing",
            ));
        }
        base.to_owned()
    };
    commit(root, &base)?;
    if !ancestor(root, &base, head)? {
        return Err(err(
            "Observed base is outside current authority history; explicit reconciliation required",
        ));
    }
    let (_, hashes) = writes::snapshot(root, None)?;
    let local = transactions::files_at(&root.join(prefix))?;
    if !same(&transactions::file_hashes(&local)?, &hashes)? {
        return Err(err("Local memory changed while preparing reconciliation"));
    }
    let merged = merge_files(
        &tree(root, &base, prefix, false)?,
        &local,
        &tree(root, head, prefix, false)?,
        false,
    )?;
    if server(
        root,
        text(get(&observation, "authority_remote"))?,
        text(get(&observation, "authority_branch"))?,
    )? != head
        || !same(&writes::snapshot(root, None)?.1, &hashes)?
        || query(root, &["rev-parse", "--verify", "HEAD"])?.trim()
            != text(get(&observation, "local_head"))?
    {
        return Err(err("Shared head or local memory advanced; reconcile again"));
    }
    let paths: BTreeSet<_> = pairs(&local)?
        .iter()
        .chain(pairs(&merged)?)
        .map(|(p, _)| p.clone())
        .collect();
    let changed = paths
        .into_iter()
        .filter(|p| local.get(p) != merged.get(p))
        .map(|p| string(format!("{prefix}/{p}")))
        .collect();
    result(
        "reconcile",
        object([
            ("writes_performed", Value::Bool(true)),
            ("git_metadata_writes_performed", Value::Bool(true)),
            ("observation", observation),
            ("base", string(base)),
            ("local_hashes", hashes),
            ("candidate_hashes", transactions::file_hashes(&merged)?),
            ("candidate_files", merged),
            ("changed_memory_paths", Value::Array(changed)),
            ("semantic_review_required", Value::Bool(true)),
            ("semantic_approval_verified", Value::Bool(false)),
            ("remote_freshness_verified", Value::Bool(true)),
            ("local_memory_available", Value::Bool(true)),
        ]),
    )
}
/// Authority-mainline replay; turn_sources and the boundary ledger call it for published completions.
pub fn replay(repo: &Path, request: &Value, quarantine_records: bool) -> Result<Value> {
    if !request.as_object().is_some_and(|v| {
        v.len() == 2 && request.get("base").is_some() && request.get("candidate").is_some()
    }) {
        return Err(err(
            "Replay requires exact base and memory-only candidate commit IDs",
        ));
    }
    let observation = observe(repo, None, quarantine_records)?;
    let root = Path::new(text(get(&observation, "repo_root"))?);
    let prefix = text(get(&observation, "bank_root"))?;
    let base = commit(root, text(get(request, "base"))?)?;
    let candidate = commit(root, text(get(request, "candidate"))?)?;
    let parents = query(root, &["rev-list", "--parents", "-n", "1", candidate])?;
    if parents.split_whitespace().skip(1).collect::<Vec<_>>() != [base] {
        return Err(err(
            "Publication candidate must have exactly the observed authority base as parent",
        ));
    }
    let changed = query(
        root,
        &[
            "diff-tree",
            "--no-commit-id",
            "--no-renames",
            "--name-only",
            "-r",
            "-z",
            base,
            candidate,
        ],
    )?;
    let changed: Vec<_> = changed.split('\0').filter(|p| !p.is_empty()).collect();
    if changed.is_empty()
        || changed
            .iter()
            .any(|p| !p.starts_with(&format!("{prefix}/")))
    {
        return Err(err(
            "Replay candidate contains no memory delta or imports unrelated code",
        ));
    }
    let before = tree(root, base, prefix, quarantine_records)?;
    let after = tree(root, candidate, prefix, quarantine_records)?;
    for files in [&before, &after] {
        if !pairs(files)?.is_empty()
            && get(&bounded(files, false, quarantine_records)?, "project_id")
                != get(&observation, "project_id")
        {
            return Err(err("Replay cannot change project identity"));
        }
    }
    let paths: BTreeSet<_> = pairs(&before)?
        .iter()
        .chain(pairs(&after)?)
        .map(|(p, _)| p.clone())
        .collect();
    let delta: Vec<_> = paths
        .into_iter()
        .filter(|p| before.get(p) != after.get(p))
        .map(|p| {
            let value = after.get(&p).cloned();
            (p, value)
        })
        .collect();
    if quarantine_records {
        for (path, value) in &delta {
            if let Some(body) = value {
                if !rows(get(
                    &crate::security::scan(
                        &string(format!("{path}\n{}", text(body)?)),
                        crate::security::LIMIT,
                    )?,
                    "reasons",
                ))?
                .is_empty()
                {
                    return Err(err(
                        "Publication readback cannot authorize a new unsafe payload",
                    ));
                }
            }
        }
    }
    let head = text(get(&observation, "authority_head"))?;
    if !ancestor(root, base, head)? {
        return Err(err(
            "Authority history changed; preserve candidate and reconcile instead of retrying",
        ));
    }
    let lineage = query(
        root,
        &["rev-list", "--first-parent", "--max-count=130", head],
    )?;
    let history: Vec<_> = lineage
        .lines()
        .take_while(|revision| *revision != base)
        .collect();
    if !lineage.lines().any(|revision| revision == base) {
        return Err(err(
            "Authority mainline base is unproved or exceeds replay bounds; inspect before retry",
        ));
    }
    if history.len() > 128 {
        return Err(err(
            "Publication history exceeds bounded replay inventory; explicit readback required",
        ));
    }
    let matches = |files: &Value| {
        delta
            .iter()
            .all(|(path, body)| files.get(path) == body.as_ref())
    };
    let mut witnessed = None;
    for revision in history {
        if matches(&tree(root, revision, prefix, quarantine_records)?) {
            witnessed = Some(revision.to_owned());
            break;
        }
    }
    let current = tree(root, head, prefix, quarantine_records)?;
    if server(
        root,
        text(get(&observation, "authority_remote"))?,
        text(get(&observation, "authority_branch"))?,
    )? != head
    {
        return Err(err(
            "Authority advanced during replay readback; inventory again",
        ));
    }
    let landed = witnessed.is_some();
    result(
        "replay",
        object([
            ("writes_performed", Value::Bool(true)),
            ("git_metadata_writes_performed", Value::Bool(true)),
            ("publication_verified", Value::Bool(landed)),
            ("intended_delta_landed", Value::Bool(landed)),
            (
                "witnessed_commit",
                witnessed.map(string).unwrap_or(Value::Null),
            ),
            ("intended_delta_current", Value::Bool(matches(&current))),
            (
                "retry_state",
                string(if landed {
                    "already-landed"
                } else {
                    "reconciliation-required"
                }),
            ),
            ("remote_freshness_verified", Value::Bool(true)),
            ("safe_to_retry_without_reconciliation", Value::Bool(false)),
            ("observation", observation),
        ]),
    )
}
pub fn command_result(operation: &str, fetch: bool, outcome: Result<Value>) -> CommandResult {
    let (value, exit_code, pretty) = match outcome {
        Ok(value) => (value, 0, true),
        Err(error) => {
            let writes = if operation == "inspect" && !fetch {
                Value::Bool(false)
            } else {
                Value::Null
            };
            let mut fields = object([
                ("writes_performed", writes.clone()),
                ("git_metadata_writes_performed", writes),
                ("remote_freshness_verified", Value::Bool(false)),
                ("sharing_state", string("pending-stale")),
                (
                    "error",
                    string(if matches!(error.kind(), Kind::BankCapacity { .. }) {
                        error.message()
                    } else {
                        "Unresolved, changed or unsafe sharing state; retain local work and inspect before retry"
                    }),
                ),
            ]);
            let _ = bank::merge(&mut fields, &crate::bank_capacity::details(&error));
            (result(operation, fields).unwrap_or(Value::Null), 1, false)
        }
    };
    let style = if pretty {
        crate::codec::json::INDENT2_ASCII
    } else {
        crate::codec::json::DEFAULT
    };
    CommandResult::json(value, exit_code, style)
}

#[cfg(test)]
mod tests {
    use super::git_sha1;
    #[test]
    fn git_sha1_matches_published_git_blob_identities() {
        assert_eq!(
            git_sha1(b"blob 0\0", b""),
            "e69de29bb2d1d6434b8b29ae775ad8c2e48c5391"
        );
        assert_eq!(
            git_sha1(b"blob 5\0", b"hello"),
            "b6fc4c620b67d95f953a5c1c1230aaab5db5a1b0"
        );
    }
}
