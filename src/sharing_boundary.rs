//! Inspect the configured memory-only Git boundary without publishing anything.
use crate::codec::json::{self, Value};
use crate::developer_identity::{encoded, sha256_hex};
use crate::error::{ContractError, Kind, Result};
use crate::fs::{self, DirectoryChain};
use crate::lifecycle::{CommandResult, get, object, string};
use crate::{
    bank_capacity, bank_writes as writes, identity_git, project_identity, repository_config,
    scope_binding, security, sharing_limits, sharing_recovery as recovery,
};
use std::collections::{BTreeMap, BTreeSet};
use std::ffi::OsString;
use std::path::Path;

pub const SCHEMA: &str = "agent-memory/sharing-boundary-v1";
use crate::sharing_recovery::MAX_LOCATOR_TREE_BYTES;
pub const MAX_REVIEW_BYTES: usize = 16_384;
pub type IndexEntries = BTreeMap<String, (String, String, String)>;

fn err(message: &str) -> ContractError {
    ContractError::new(message)
}
fn git(root: &Path, args: &[&str], optional: bool) -> Result<Option<String>> {
    identity_git::git(
        root,
        &args.iter().map(OsString::from).collect::<Vec<_>>(),
        optional,
        false,
    )
    .map_err(|e| err(&e.to_string()))?
    .map(|s| {
        s.into_string()
            .map_err(|_| ContractError::non_utf8("Git boundary evidence"))
    })
    .transpose()
}
fn strings(values: impl IntoIterator<Item = impl Into<String>>) -> Value {
    Value::Array(values.into_iter().map(string).collect())
}
fn exists(root: &Path, path: &str) -> Result<bool> {
    let target = root.join(path);
    let parent = target
        .parent()
        .ok_or_else(|| err("Invalid indexed memory path"))?;
    let chain = match DirectoryChain::open(parent) {
        Ok(chain) => chain,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(e) => return Err(err(&e.to_string())),
    };
    let name = target
        .file_name()
        .ok_or_else(|| err("Invalid indexed memory path"))?;
    let present = match chain.directory().stat_at(name) {
        Ok(_) => true,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => false,
        Err(e) => return Err(err(&e.to_string())),
    };
    chain.recheck().map_err(|e| err(&e.to_string()))?;
    Ok(present)
}
/// Prove absent indexed files belonged to the last complete canonical bank.
pub fn prior_deletions(
    root: &Path,
    prefix: &str,
    indexed: &IndexEntries,
    current: &BTreeSet<String>,
) -> Result<BTreeSet<String>> {
    let missing: Vec<_> = indexed
        .iter()
        .filter(|(p, _)| !current.contains(*p))
        .collect();
    if missing.is_empty() {
        return Ok(BTreeSet::new());
    }
    let head = git(root, &["rev-parse", "--verify", "HEAD"], false)?
        .ok_or_else(|| err("Missing Git HEAD"))?;
    let files = match recovery::tree(root, &head, prefix, false) {
        Ok(files) => files,
        Err(e) if matches!(e.kind(), Kind::BankCapacity { .. } | Kind::MissingProvider) => {
            return Err(e);
        }
        Err(_) => return Ok(BTreeSet::new()),
    };
    let mut proven = BTreeSet::new();
    let bank_prefix = format!("{prefix}/");
    for (path, (mode, blob, stage)) in missing {
        let Some(relative) = path.strip_prefix(&bank_prefix) else {
            continue;
        };
        if files.get(relative).is_none() || mode != "100644" || stage != "0" || exists(root, path)?
        {
            continue;
        }
        let revision = format!("{head}:{path}");
        let expected = recovery::command(
            root,
            &["rev-parse", &revision],
            false,
            MAX_LOCATOR_TREE_BYTES,
            false,
        )?;
        if blob == expected.trim() {
            proven.insert(path.clone());
        }
    }
    Ok(proven)
}
pub fn inspect(repo: &Path, owned_lock: Option<&[u8]>) -> Result<Value> {
    let (root, bank) = writes::bank_path(repo)?;
    let options = scope_binding::RequireOptions {
        owned_lock,
        ..Default::default()
    };
    let context = scope_binding::require(&root, None, options)?;
    let configuration = repository_config::settings(&root)?;
    let (checked, observed) = writes::snapshot(&root, owned_lock)?;
    let mut source_paths: Vec<_> = observed
        .as_object()
        .ok_or_else(|| err("Invalid canonical observation"))?
        .iter()
        .map(|(p, _)| p.as_str())
        .collect();
    source_paths.sort_unstable();
    let prefix = format!("{}/", configuration.bank_root);
    let canonical: BTreeSet<String> = source_paths
        .iter()
        .map(|p| format!("{prefix}{p}"))
        .collect();
    let canonical_value = strings(canonical.iter().cloned());
    if canonical.len() > sharing_limits::MAX_FILES
        || encoded(&canonical_value)?.len() > MAX_LOCATOR_TREE_BYTES
    {
        return Err(err(
            "Sharing path inventory exceeds its complete metadata budget; preserve it without truncation",
        ));
    }
    let project = project_identity::status_with_options(
        &root,
        None,
        project_identity::DiscoveryOptions {
            owned_lock,
            ..Default::default()
        },
    )?;
    let remote_value = get(get(&project, "evidence"), "authority_name");
    let remote = remote_value.as_str().filter(|s| !s.is_empty());
    if let Some(remote) = remote {
        repository_config::safe_label(remote)?;
    }
    let remote_head = if let Some(remote) = remote {
        git(
            &root,
            &[
                "symbolic-ref",
                "--quiet",
                &format!("refs/remotes/{remote}/HEAD"),
            ],
            true,
        )?
    } else {
        None
    };
    let observed_branch = remote.and_then(|r| {
        remote_head
            .as_deref()?
            .strip_prefix(&format!("refs/remotes/{r}/"))
    });
    let (branch, branch_source) = if let Some(branch) = configuration.authority_branch.as_deref() {
        (branch, "explicit-local-configuration")
    } else if let Some(branch) = observed_branch.filter(|b| !b.is_empty()) {
        (branch, "local-remote-default-branch-observation")
    } else {
        ("main", "initial-main-default-unverified")
    };
    repository_config::validate_branch(branch)?;
    let index_args = [
        "ls-files",
        "--stage",
        "-z",
        "--",
        configuration.bank_root.as_str(),
    ];
    let indexed = recovery::command(&root, &index_args, false, MAX_LOCATOR_TREE_BYTES, false)?;
    let mut entries = IndexEntries::new();
    for item in indexed.split('\0').filter(|s| !s.is_empty()) {
        let (header, path) = item
            .split_once('\t')
            .ok_or_else(|| err("Invalid memory index entry"))?;
        let mut fields = header.split_whitespace();
        let mode = fields
            .next()
            .ok_or_else(|| err("Invalid memory index mode"))?;
        let blob = fields
            .next()
            .ok_or_else(|| err("Invalid memory index blob"))?;
        let stage = fields
            .next()
            .ok_or_else(|| err("Invalid memory index stage"))?;
        if fields.next().is_some() {
            return Err(err("Invalid memory index entry"));
        }
        let stage = if entries.contains_key(path) {
            "unmerged"
        } else {
            stage
        };
        entries.insert(path.into(), (mode.into(), blob.into(), stage.into()));
    }
    let deleted = prior_deletions(&root, &configuration.bank_root, &entries, &canonical)?;
    let mut tracked = BTreeSet::new();
    let mut unmerged = 0usize;
    let mut excluded_tracked = 0usize;
    for (path, (mode, _, stage)) in &entries {
        if !canonical.contains(path) && !deleted.contains(path) {
            excluded_tracked += 1;
        } else if stage != "0" || mode != "100644" {
            unmerged += 1;
        } else {
            tracked.insert(path.clone());
        }
    }
    let mut ignored = BTreeSet::new();
    for selection in ["--cached", "--others"] {
        let output = recovery::command(
            &root,
            &[
                "ls-files",
                selection,
                "--ignored",
                "--exclude-standard",
                "-z",
                "--",
                &configuration.bank_root,
            ],
            false,
            MAX_LOCATOR_TREE_BYTES,
            false,
        )?;
        ignored.extend(
            output
                .split('\0')
                .filter(|p| canonical.contains(*p))
                .map(str::to_owned),
        );
    }
    let pending = canonical.difference(&tracked).count();
    let mut blockers = Vec::new();
    if !ignored.is_empty() {
        blockers.push("canonical-memory-is-ignored");
    }
    if excluded_tracked != 0 {
        blockers.push("excluded-bank-artifacts-are-tracked");
    }
    if unmerged != 0 {
        blockers.push("unmerged-or-nonordinary-memory-index-entry");
    }
    if pending != 0 {
        blockers.push("canonical-memory-is-not-tracked");
    }
    if remote.is_none() {
        blockers.push("no-resolved-sharing-remote");
    }
    if branch_source == "initial-main-default-unverified" {
        blockers.push("default-branch-needs-explicit-configuration-or-fresh-remote-observation");
    }
    let mut safety = BTreeSet::new();
    for entry in get(&checked, "entries").as_array().into_iter().flatten() {
        if matches!(
            get(entry, "lifecycle").as_str(),
            Some("candidate" | "quarantined")
        ) {
            safety.insert("unadmitted-content".to_owned());
        }
    }
    for path in source_paths {
        let raw = match fs::read_regular(&bank, Path::new(path), MAX_REVIEW_BYTES + 1) {
            Ok((bytes, _)) if bytes.len() <= MAX_REVIEW_BYTES => bytes,
            Ok(_) | Err(fs::ReadError::SizeLimit { .. }) => {
                safety.insert("bounded-content-review-required".into());
                continue;
            }
            Err(e) => return Err(err(&e.to_string())),
        };
        let body =
            std::str::from_utf8(&raw).map_err(|_| ContractError::non_utf8("canonical memory"))?;
        let scan = security::scan(
            &string(format!("{body}\n{prefix}{path}")),
            security::REVIEW_LIMIT,
        )?;
        for reason in get(&scan, "reasons").as_array().into_iter().flatten() {
            safety.insert(
                reason
                    .as_str()
                    .ok_or_else(|| err("Invalid security reason"))?
                    .to_owned(),
            );
        }
    }
    if !safety.is_empty() {
        blockers.push("unsafe-or-unreviewable-memory-content");
    }
    let project_id = get(&checked, "project_id")
        .as_str()
        .ok_or_else(|| err("Missing canonical project identity"))?;
    scope_binding::require(
        &root,
        Some(project_id),
        scope_binding::RequireOptions {
            expected: Some(&context),
            ..options
        },
    )?;
    if encoded(&writes::snapshot(&root, owned_lock)?.1)? != encoded(&observed)?
        || repository_config::settings(&root)? != configuration
        || recovery::command(&root, &index_args, false, MAX_LOCATOR_TREE_BYTES, false)? != indexed
    {
        return Err(err(
            "Canonical bank or boundary configuration changed; inspect again",
        ));
    }
    let digest = sha256_hex(&encoded(&object([
        ("project_id", string(project_id)),
        ("sources", observed),
    ]))?);
    Ok(object([
        ("schema", string(SCHEMA)),
        ("project_id", string(project_id)),
        ("bank_root", string(&configuration.bank_root)),
        (
            "authority_remote",
            remote.map(string).unwrap_or(Value::Null),
        ),
        ("authority_branch", string(branch)),
        ("authority_branch_source", string(branch_source)),
        ("remote_freshness_verified", Value::Bool(false)),
        ("publication_verified", Value::Bool(false)),
        ("admission_verified", Value::Bool(false)),
        ("memory_path_count", Value::Int(canonical.len().into())),
        (
            "eligible_memory_paths",
            if safety.is_empty() {
                canonical_value
            } else {
                strings(Vec::<String>::new())
            },
        ),
        ("untracked_memory_count", Value::Int(pending.into())),
        ("ignored_memory_count", Value::Int(ignored.len().into())),
        (
            "proven_deleted_memory_paths",
            if safety.is_empty() {
                strings(deleted)
            } else {
                strings(Vec::<String>::new())
            },
        ),
        (
            "excluded_tracked_artifact_count",
            Value::Int(excluded_tracked.into()),
        ),
        ("unmerged_or_nonordinary_count", Value::Int(unmerged.into())),
        ("safety_reasons", strings(safety)),
        ("blockers", strings(blockers.iter().copied())),
        ("local_boundary_ready", Value::Bool(blockers.is_empty())),
        ("bank_observation_sha256", string(digest)),
        (
            "roles",
            object([
                ("memory", string("durable-project-learning")),
                ("awa", string("work-state-and-delivery")),
                ("vox", string("live-coordination")),
            ]),
        ),
        ("requires_awa", Value::Bool(false)),
        ("requires_vox", Value::Bool(false)),
        ("personal_memory", Value::Bool(false)),
        ("writes_performed", Value::Bool(false)),
        ("validation_scope", string("local-memory-git-boundary-only")),
    ]))
}
pub fn command_result(outcome: Result<Value>) -> CommandResult {
    let (value, code, style) = match outcome {
        Ok(value) => (value, 0, json::INDENT2_ASCII),
        Err(error) => {
            let message = if matches!(error.kind(), Kind::BankCapacity { .. }) {
                error.message()
            } else {
                "Unresolved, unsafe, interrupted or changed repository boundary; inspect and preserve local work"
            };
            let mut value = object([
                ("schema", string(SCHEMA)),
                ("verified", Value::Bool(false)),
                ("writes_performed", Value::Bool(false)),
                ("error", string(message)),
            ]);
            if let (Value::Object(fields), Value::Object(details)) =
                (&mut value, bank_capacity::details(&error))
            {
                fields.extend(details);
            }
            (value, 1, json::DEFAULT)
        }
    };
    match json::encode(&value, style) {
        Ok(output) => CommandResult {
            value: Some(value),
            exit_code: code,
            stdout: output + "\n",
            stderr: String::new(),
        },
        Err(error) => CommandResult {
            value: None,
            exit_code: 1,
            stdout: String::new(),
            stderr: format!("{error}\n"),
        },
    }
}
