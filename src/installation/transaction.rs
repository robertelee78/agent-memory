//! Journaled activation, verification, recovery and the public commands
//! (baseline `verify_hook_target`, `verify_targets`, `rollback_compatibility`,
//! `preserve_preexchange_cache_writer`, `rollback`, `recover`, `install`,
//! `_install`, `finish_stored_source_activation`, `failure_*`, `main`).
use super::filesystem::{
    copy_node, fingerprint, lexists, make_parents, named, native_rename, probe, read_optional,
    replace_record, same, symlink, sync_dir, sync_tree, write_file,
};
use super::plan::{
    Command, InstallRequest, Plan, PrepareOptions, lifecycle_paths, preflight_layout, prepare,
    public_plan, stage_release, unchanged_parent,
};
use super::records::{read_journal, read_receipt};
use super::seams::Services;
use super::{
    ATOMICITY, Environment, Error, HookCode, JOURNAL_SCHEMA, MAX_RECORD, MAX_REPOSITORIES,
    MEMORY_DATA_LOCATION, RECEIPT_SCHEMA, RESTART_NOTICE, Result, absolute, encoded, locations,
    locked, path_text, path_value, require, sha, source_binary,
};
use crate::bank_writes::fault;
use crate::cache_recovery;
use crate::client_hooks;
use crate::codec::json::{self, Value};
use crate::error::{ContractError, Kind};
use crate::fs;
use crate::lifecycle::{CommandResult, get, object, string, text};
use std::path::{Path, PathBuf};

fn set(value: &mut Value, key: &str, field: Value) {
    if let Value::Object(fields) = value {
        match fields.iter_mut().find(|(name, _)| name == key) {
            Some((_, slot)) => *slot = field,
            None => fields.push((key.to_owned(), field)),
        }
    }
}
fn push(value: &mut Value, key: &str, item: Value) {
    if let Some(Value::Array(items)) = value.get_mut(key) {
        items.push(item);
    } else {
        set(value, key, Value::Array(vec![item]));
    }
}
fn without(value: &Value, key: &str) -> Value {
    match value {
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .filter(|(name, _)| name != key)
                .cloned()
                .collect(),
        ),
        other => other.clone(),
    }
}
fn path_of(value: &Value, key: &str) -> PathBuf {
    PathBuf::from(text(value, key).unwrap_or_default())
}
fn truthy(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
        && value.as_str() != Some("")
        && value.as_object().is_none_or(|fields| !fields.is_empty())
        && value.as_array().is_none_or(|items| !items.is_empty())
}

/// A verified owned hook configuration observation.
pub struct HookVerification {
    pub identity: Value,
    pub raw: Vec<u8>,
    pub document: Value,
}

fn hook_error(code: HookCode) -> Error {
    Error::Hook(code)
}

/// Baseline `verify_hook_target`: exact receipt-owned groups in one stable,
/// trusted shared file.
pub fn verify_hook_target(target: &Value) -> Result<HookVerification> {
    let path = PathBuf::from(text(target, "path").ok_or(hook_error(HookCode::UnsafePath))?);
    let valid_path = text(target, "kind") == Some("hook-config")
        && path.is_absolute()
        && !path
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
        && path
            .parent()
            .is_some_and(|parent| fs::canonical_path(parent, false).is_ok_and(|c| c == parent));
    if !valid_path {
        return Err(hook_error(HookCode::UnsafePath));
    }
    let identity = get(target, "identity");
    let mode_ok = get(identity, "mode")
        .as_int()
        .and_then(|value| value.as_u64())
        .is_some_and(|mode| mode <= 0o7777);
    if !(text(identity, "kind") == Some("file") && mode_ok && target.get("hook_entry").is_some()) {
        return Err(hook_error(HookCode::InsufficientReceipt));
    }
    if let Err(error) = fs::trust::inspect(&path, Default::default()) {
        let code = match (error.kind(), named(&path)) {
            (Kind::TrustPath { .. }, Ok(Some(info)))
                if !info.is_regular() && !info.is_symlink() =>
            {
                HookCode::UnsafeType
            }
            _ => HookCode::UnsafePath,
        };
        return Err(hook_error(code));
    }
    let observed = fingerprint(&path).map_err(|_| hook_error(HookCode::ConfigUnavailable))?;
    if observed == Value::Null {
        return Err(hook_error(HookCode::ConfigUnavailable));
    }
    if text(&observed, "kind") != Some("file") {
        return Err(hook_error(HookCode::UnsafeType));
    }
    if get(&observed, "mode") != get(identity, "mode") {
        return Err(hook_error(HookCode::ModeChanged));
    }
    let mut failure = None;
    let mut raw = Vec::new();
    let mut document = Value::Null;
    match read_optional(&path, client_hooks::MAX_CONFIG) {
        Ok(Some(bytes)) => {
            raw = bytes;
            let parsed = client_hooks::parse(&raw).and_then(|value| {
                client_hooks::locate(&value, get(target, "hook_entry"), client_hooks::EVENT)?;
                if let Some(stop) = target.get("stop_entry") {
                    client_hooks::locate(&value, stop, "Stop")?;
                }
                Ok(value)
            });
            match parsed {
                Ok(value) => document = value,
                Err(error) => failure = Some(HookCode::from_code(error.message())),
            }
        }
        Ok(None) => return Err(hook_error(HookCode::ConcurrentChange)),
        Err(Error::Contract(error))
            if error.message() == "Ordinary file exceeds its read bound" =>
        {
            failure = Some(HookCode::ConfigUnavailable)
        }
        Err(_) => return Err(hook_error(HookCode::ConcurrentChange)),
    }
    fault("hook-ownership-before-recheck")?;
    let stable = fs::trust::inspect(&path, Default::default()).is_ok()
        && fingerprint(&path).is_ok_and(|current| same(&current, &observed));
    if !stable {
        return Err(hook_error(HookCode::ConcurrentChange));
    }
    if let Some(code) = failure {
        return Err(hook_error(code));
    }
    Ok(HookVerification {
        identity: observed,
        raw,
        document,
    })
}

/// Baseline `verify_targets`: exact identities, legacy managed qualification
/// and client-visible artifact readback (one artifact digest, pack + binary).
pub fn verify_targets(
    services: &Services,
    targets: &[&Value],
    source: Option<&Value>,
    uninstall_output: bool,
) -> Result<Vec<Value>> {
    let mut qualifications = Vec::new();
    for target in targets {
        let path = path_of(target, "path");
        let kind = text(target, "kind").unwrap_or_default();
        if kind == "hook-config" && !uninstall_output {
            verify_hook_target(target)?;
            continue;
        }
        fs::trust::tree(&path, false)?;
        let observed = fingerprint(&path)?;
        let identity = get(target, "identity");
        if !same(&observed, identity) {
            let legacy = if kind == "legacy-source" {
                target.get("legacy_source")
            } else if kind == "source" {
                source
            } else {
                None
            };
            let legacy = legacy.filter(|value| text(value, "kind") == Some("managed"));
            let qualifiable = legacy.is_some()
                && text(&observed, "kind") == Some("directory")
                && text(identity, "kind") == Some("directory")
                && get(&observed, "mode") == get(identity, "mode");
            require(
                qualifiable,
                format!("Installed target readback failed: {}", path.display()),
            )?;
            let verification =
                crate::managed_source::verify(&path, legacy.unwrap_or(&Value::Null))?;
            qualifications.push(object([
                ("qualification", string(verification.qualification)),
                ("revision", string(verification.revision)),
            ]));
            require(
                same(&fingerprint(&path)?, &observed),
                "Managed source changed during ownership verification",
            )?;
        }
        let rust_era = source.is_some_and(|source| text(source, "target").is_some());
        if *identity != Value::Null && matches!(kind, "copy" | "link") {
            let inspected = if rust_era {
                let binary = source_binary(source.unwrap_or(&Value::Null))?;
                services.artifacts.inspect_runtime(&path, &binary)?
            } else {
                services.artifacts.inspect(&path)?
            };
            require(
                get(&inspected, "artifact_digest") == get(target, "artifact_digest"),
                format!(
                    "Client-visible artifact readback failed: {}",
                    path.display()
                ),
            )?;
        }
    }
    Ok(qualifications)
}

const EXECUTABLE_KINDS: [&str; 6] = [
    "source",
    "copy",
    "link",
    "wrapper",
    "hook-link",
    "command-link",
];

/// Baseline `rollback_compatibility`: no executable restoration until the
/// old runtime can read current banks.
fn rollback_compatibility(
    env: &Environment,
    services: &Services,
    state: &Path,
    journal: &Value,
) -> Result<()> {
    let mut restoring = Vec::new();
    for action in get(journal, "actions").as_array().unwrap_or(&[]) {
        if EXECUTABLE_KINDS.contains(&text(action, "kind").unwrap_or_default())
            && *get(action, "before") != Value::Null
            && !same(
                &fingerprint(&path_of(action, "path"))?,
                get(action, "before"),
            )
        {
            restoring.push(action);
        }
    }
    if restoring.is_empty() {
        return Ok(());
    }
    let compatibility = match get(journal, "previous_compatibility") {
        Value::Null => {
            // Legacy incomplete journals predate this field: their uncommitted
            // receipt must identify these exact targets.
            let previous = read_receipt(env, state)?;
            let identified = previous.as_ref().is_some_and(|previous| {
                restoring.iter().all(|action| {
                    get(previous, "targets")
                        .as_array()
                        .unwrap_or(&[])
                        .iter()
                        .any(|target| {
                            get(target, "path") == get(action, "path")
                                && same(get(target, "identity"), get(action, "before"))
                        })
                })
            });
            require(
                identified,
                "Cannot establish prior runtime compatibility; preserve targets and use a trusted outside helper",
            )?;
            previous
                .map(|previous| get(get(&previous, "source"), "compatibility").clone())
                .unwrap_or(Value::Null)
        }
        value => value.clone(),
    };
    require(
        compatibility.as_object().is_some()
            && crate::artifact::strings(get(&compatibility, "readable_bank_schemas"), false)
            && crate::artifact::strings(get(&compatibility, "migrator_source_schemas"), true),
        "Prior runtime readability metadata is missing; preserve the lifecycle transaction",
    )?;
    let repositories = journal
        .get("rollback_repositories")
        .cloned()
        .unwrap_or(Value::Array(Vec::new()));
    let listed: Option<Vec<String>> = repositories.as_array().and_then(|items| {
        items
            .iter()
            .map(|item| {
                item.as_str()
                    .filter(|path| {
                        absolute(env, Path::new(path)).is_ok_and(|a| a == Path::new(path))
                    })
                    .map(str::to_owned)
            })
            .collect()
    });
    let listed = listed
        .filter(|items| items.len() <= MAX_REPOSITORIES)
        .ok_or_else(|| ContractError::new("Malformed rollback repository inventory"))?;
    services.private_state()?.require_compatible(
        &object([("rollback_repositories", repositories.clone())]),
        &compatibility,
        "rollback",
    )?;
    services.updating()?.banks(&listed, &compatibility, true)?;
    Ok(())
}

/// Baseline `preserve_preexchange_cache_writer`: end only a proved
/// unactivated source-cache race, preserving every byte.
fn preserve_preexchange_cache_writer(
    env: &Environment,
    state: &Path,
    journal: &mut Value,
) -> Result<()> {
    let opaque = get(journal, "opaque_cache_recovery").clone();
    let activated = get(journal, "activated")
        .as_int()
        .and_then(|v| v.as_u64())
        .unwrap_or(0);
    let actions = get(journal, "actions").as_array().unwrap_or(&[]).to_vec();
    let Some(action) = actions.first() else {
        return Ok(());
    };
    if !truthy(&opaque) || activated != 0 || *get(journal, "receipt_identity") != Value::Null {
        return Ok(());
    }
    if text(action, "kind") != Some("source") || *get(action, "before") == Value::Null {
        return Ok(());
    }
    let path = path_of(action, "path");
    let stage = path_of(action, "stage");
    if same(&fingerprint(&path)?, get(action, "before")) {
        return Ok(());
    }
    let others_unchanged = |actions: &[Value]| -> Result<bool> {
        for item in actions.iter().skip(1) {
            if !same(&fingerprint(&path_of(item, "path"))?, get(item, "before")) {
                return Ok(false);
            }
        }
        Ok(true)
    };
    let Some(old) = read_receipt(env, state)? else {
        return Ok(());
    };
    let backup = text(action, "backup").map(PathBuf::from);
    let Some(backup) = backup else {
        return Ok(());
    };
    if text(&opaque, "receipt_sha256") != Some(sha(&encoded(&old)?).as_str())
        || !same(get(&opaque, "old_source_identity"), get(action, "before"))
        || path != path_of(get(&old, "source"), "path")
        || !same(&fingerprint(&stage)?, get(action, "after"))
        || !same(&fingerprint(&backup)?, get(action, "before"))
        || !others_unchanged(&actions)?
    {
        return Ok(());
    }
    // Unknown changes retain the existing strict conflict path.
    let Ok(qualification) = cache_recovery::qualify(&path, &old) else {
        return Ok(());
    };
    if cache_recovery::recheck(&qualification).is_err() {
        return Ok(());
    }
    if !same(&fingerprint(&stage)?, get(action, "after"))
        || !same(&fingerprint(&backup)?, get(action, "before"))
        || !read_receipt(env, state)?.is_some_and(|current| same(&current, &old))
        || !others_unchanged(&actions)?
    {
        return Ok(());
    }
    if same(
        get(get(journal, "preserved_opaque_source"), "identity"),
        &qualification.source_identity,
    ) {
        return Ok(());
    }
    let prior = state.join("journal.json");
    let raw = read_optional(&prior, MAX_RECORD)?.unwrap_or_default();
    let transaction = text(journal, "transaction").unwrap_or_default().to_owned();
    let history = state.join(format!(
        "journal-opaque-preexchange-{transaction}-{}.json",
        sha(&raw)
    ));
    if !lexists(&history)? {
        write_file(&history, &raw, 0o600)?;
    } else {
        fs::trust::record(&history)?;
        require(
            read_optional(&history, MAX_RECORD)?.as_deref() == Some(raw.as_slice()),
            "Preserved preexchange journal differs; retain recovery evidence",
        )?;
    }
    set(
        journal,
        "preserved_opaque_source",
        object([
            ("path", path_value(&path)?),
            ("identity", qualification.source_identity.clone()),
            ("qualification", qualification.summary.clone()),
            (
                "prior_journal",
                object([
                    ("path", path_value(&history)?),
                    ("sha256", string(sha(&raw))),
                ]),
            ),
        ]),
    );
    Ok(())
}

/// Baseline `rollback`: under the private-state activation guard.
pub fn rollback(
    env: &Environment,
    services: &Services,
    state: &Path,
    journal: &mut Value,
) -> Result<Value> {
    let mut installation = read_receipt(env, state)?.unwrap_or(Value::Object(Vec::new()));
    set(
        &mut installation,
        "rollback_repositories",
        journal
            .get("rollback_repositories")
            .cloned()
            .unwrap_or(Value::Array(Vec::new())),
    );
    let compatibility = match get(journal, "previous_compatibility") {
        Value::Null => Value::Object(Vec::new()),
        value => value.clone(),
    };
    let target = object([
        ("compatibility", compatibility),
        (
            "artifact_digest",
            get(journal, "previous_artifact_digest").clone(),
        ),
    ]);
    let guard = services.private_state()?;
    let mut body = |_: &Value| rollback_now(env, services, state, journal);
    guard.activation_guard(
        &installation,
        Some(&target),
        "rollback",
        &mut || Ok(None),
        &mut body,
    )
}

fn rollback_now(
    env: &Environment,
    services: &Services,
    state: &Path,
    journal: &mut Value,
) -> Result<Value> {
    fs::trust::state(state)?;
    rollback_compatibility(env, services, state, journal)?;
    preserve_preexchange_cache_writer(env, state, journal)?;
    let mut conflicts = Vec::new();
    let actions = get(journal, "actions").as_array().unwrap_or(&[]).to_vec();
    for action in actions.iter().rev() {
        let path = path_of(action, "path");
        let stage = path_of(action, "stage");
        unchanged_parent(&path)?;
        fs::trust::tree(&path, false)?;
        fs::trust::tree(&stage, false)?;
        let current = fingerprint(&path)?;
        let staged = fingerprint(&stage)?;
        let preserved = get(journal, "preserved_opaque_source");
        let before = get(action, "before");
        if same(&current, before)
            || (text(action, "kind") == Some("source")
                && get(preserved, "path") == get(action, "path")
                && same(&current, get(preserved, "identity")))
        {
            continue;
        }
        if !same(&current, get(action, "after"))
            || (*before != Value::Null && !same(&staged, before))
        {
            conflicts.push(path_text(&path)?.to_owned());
            continue;
        }
        if matches!(
            text(action, "kind"),
            Some("source" | "copy" | "link" | "wrapper" | "command-link")
        ) && *before != Value::Null
        {
            rollback_compatibility(env, services, state, journal)?;
        }
        if *get(action, "after") == Value::Null {
            native_rename(&stage, &path, false)?;
        } else if *before == Value::Null {
            require(
                staged == Value::Null,
                "Fresh activation rollback stage collision",
            )?;
            native_rename(&path, &stage, false)?;
        } else {
            native_rename(&path, &stage, true)?;
        }
        if let Some(parent) = path.parent() {
            sync_dir(parent)?;
        }
        require(
            same(&fingerprint(&path)?, before),
            format!("Rollback readback failed: {}", path.display()),
        )?;
    }
    set(
        journal,
        "status",
        string(if conflicts.is_empty() {
            "rolled-back"
        } else {
            "rollback-conflict"
        }),
    );
    set(
        journal,
        "conflicts",
        Value::Array(conflicts.iter().map(string).collect()),
    );
    replace_record(&state.join("journal.json"), journal)?;
    require(
        conflicts.is_empty(),
        format!(
            "Recovery preserved externally modified paths; reconcile: {}",
            conflicts.join(", ")
        ),
    )?;
    let opaque = truthy(get(journal, "opaque_cache_recovery"));
    let mut result = vec![
        ("operation".to_owned(), string("recover")),
        ("verified".to_owned(), Value::Bool(!opaque)),
        (
            "status".to_owned(),
            string(if opaque {
                "opaque-cache-recovery-pending"
            } else {
                "rolled-back"
            }),
        ),
    ];
    if opaque {
        result.push(("repair".into(), string("The exact previous tree was restored without executing it. Its caches remain untrusted; retry from the trusted signed installer, preserving this journal and backup.")));
    }
    let mut preserved = Vec::new();
    for action in &actions {
        for key in ["stage", "backup"] {
            if let Some(path) = text(action, key) {
                if fingerprint(Path::new(path))? != Value::Null {
                    preserved.push(string(path));
                }
            }
        }
    }
    result.push(("preserved_backups".into(), Value::Array(preserved)));
    Ok(Value::Object(result))
}

/// Baseline `_verify_rolled_back_targets`.
pub(crate) fn verify_rolled_back_targets(
    services: &Services,
    state: &Path,
    journal: &Value,
) -> Result<()> {
    require(
        text(journal, "status") == Some("rolled-back"),
        "Runtime rollback is not complete",
    )?;
    let preserved = get(journal, "preserved_opaque_source");
    for action in get(journal, "actions").as_array().unwrap_or(&[]) {
        let mut expected = if text(action, "kind") == Some("source")
            && get(preserved, "path") == get(action, "path")
        {
            get(preserved, "identity").clone()
        } else {
            get(action, "before").clone()
        };
        if text(action, "kind") == Some("wrapper")
            && *get(action, "before") == Value::Null
            && lexists(&state.join("omp-transition.json"))?
        {
            if let Some(restored) = services
                .omp_boundary()?
                .native_recovery_expected(state, text(action, "path").unwrap_or_default())?
            {
                expected = restored;
            }
        }
        require(
            same(&fingerprint(&path_of(action, "path"))?, &expected),
            format!(
                "Rolled-back target changed; preserve and reconcile: {}",
                text(action, "path").unwrap_or_default()
            ),
        )?;
    }
    Ok(())
}

pub(crate) fn receipt_committed(state: &Path, journal: &Value) -> Result<bool> {
    let identity = get(journal, "receipt_identity");
    Ok(*identity != Value::Null && same(&fingerprint(&state.join("receipt.json"))?, identity))
}

/// Baseline `_require_replannable_journal`.
pub(crate) fn require_replannable_journal(env: &Environment, state: &Path) -> Result<()> {
    let Some(journal) = read_journal(env, state)? else {
        return Ok(());
    };
    let status = text(&journal, "status").unwrap_or_default();
    let committed = receipt_committed(state, &journal)?;
    require(
        matches!(status, "complete" | "rolled-back") || (status == "activating" && committed),
        "Uncommitted lifecycle transaction; use the trusted recover helper before replanning",
    )?;
    if matches!(status, "complete" | "activating") {
        require(
            committed,
            "Current receipt differs from its committed journal; preserve and reconcile ownership",
        )?;
    }
    if status == "activating" {
        require(
            (text(&journal, "operation") == Some("uninstall"))
                == (get(get(&journal, "receipt"), "uninstalled") == &Value::Bool(true)),
            "Lifecycle journal and receipt operation differ",
        )?;
    }
    Ok(())
}

fn recover_result(status: &str, verified: bool) -> Value {
    object([
        ("operation", string("recover")),
        ("verified", Value::Bool(verified)),
        ("status", string(status)),
    ])
}

/// Baseline `recover`: finish a committed transaction or roll back an
/// uncommitted one using its recorded steps.
pub fn recover(
    env: &Environment,
    services: &Services,
    state: &Path,
    for_install: bool,
    complete_native: bool,
) -> Result<Value> {
    let path = state.join("journal.json");
    let journal = read_journal(env, state)?;
    let restore_native = || -> Result<()> {
        if !for_install && lexists(&state.join("omp-transition.json"))? {
            services.omp_boundary()?.recover_native_binding(state)?;
        }
        Ok(())
    };
    let status = journal
        .as_ref()
        .and_then(|journal| text(journal, "status"))
        .map(str::to_owned);
    if matches!(status.as_deref(), None | Some("complete" | "rolled-back")) {
        restore_native()?;
    }
    let Some(mut journal) = journal else {
        let result = recover_result("no-pending-transaction", true);
        return if complete_native && !for_install {
            finish_stored_source_activation(env, services, state, result)
        } else {
            Ok(result)
        };
    };
    let receipt = get(&journal, "receipt").clone();
    match status.as_deref() {
        Some("complete") => {
            require(
                receipt_committed(state, &journal)?,
                "Completed installation receipt is missing or changed; preserve the journal and reconcile ownership",
            )?;
            if for_install {
                return Ok(recover_result("install-validation-pending", false));
            }
            if get(&receipt, "uninstalled") != &Value::Bool(true) {
                let targets: Vec<&Value> = get(&receipt, "targets")
                    .as_array()
                    .unwrap_or(&[])
                    .iter()
                    .collect();
                verify_targets(services, &targets, Some(get(&receipt, "source")), false)?;
            }
            let result = recover_result("complete", true);
            return if complete_native {
                finish_stored_source_activation(env, services, state, result)
            } else {
                Ok(result)
            };
        }
        Some("rolled-back") => {
            if for_install {
                return Ok(recover_result("install-validation-pending", false));
            }
            verify_rolled_back_targets(services, state, &journal)?;
            let opaque = truthy(get(&journal, "opaque_cache_recovery"));
            let mut result = recover_result(
                if opaque {
                    "opaque-cache-recovery-pending"
                } else {
                    "rolled-back"
                },
                !opaque,
            );
            if opaque {
                set(
                    &mut result,
                    "repair",
                    string(
                        "The previous source is preserved but its opaque caches remain untrusted. Retry using the trusted signed installer; do not execute old runtime caches.",
                    ),
                );
            }
            return if complete_native && !opaque {
                finish_stored_source_activation(env, services, state, result)
            } else {
                Ok(result)
            };
        }
        _ => {}
    }
    // Receipt activation is the commit point; a lost final response must not undo it.
    if receipt_committed(state, &journal)? {
        require(
            status.as_deref() == Some("activating"),
            "Committed receipt has an invalid lifecycle journal state",
        )?;
        let uninstall_output = text(&journal, "operation") == Some("uninstall");
        require(
            uninstall_output == (get(&receipt, "uninstalled") == &Value::Bool(true)),
            "Lifecycle journal and receipt operation differ",
        )?;
        let verified = restore_native().and_then(|()| {
            let targets: Vec<&Value> = get(&receipt, "targets")
                .as_array()
                .unwrap_or(&[])
                .iter()
                .collect();
            let source = (!uninstall_output).then(|| get(&receipt, "source"));
            verify_targets(services, &targets, source, uninstall_output)
        });
        match verified {
            Ok(_) => {}
            Err(Error::Contract(error)) if matches!(error.kind(), Kind::TrustPath { .. }) => {
                return Err(Error::Contract(error));
            }
            Err(Error::Layout(message)) => return Err(Error::Layout(message)),
            // A fresh ownership plan may reconcile allowed surrounding edits.
            Err(_) if for_install => {
                return Ok(recover_result("install-validation-pending", false));
            }
            Err(error) => return Err(error),
        }
        set(&mut journal, "status", string("complete"));
        replace_record(&path, &journal)?;
        let result = recover_result("complete", true);
        return if complete_native {
            finish_stored_source_activation(env, services, state, result)
        } else {
            Ok(result)
        };
    }
    let result = rollback(env, services, state, &mut journal)?;
    restore_native()?;
    Ok(result)
}

/// Baseline `finish_stored_source_activation`: a post-commit qualification
/// failure never rolls ownership back.
pub fn finish_stored_source_activation(
    env: &Environment,
    services: &Services,
    state: &Path,
    result: Value,
) -> Result<Value> {
    let attempt = || -> Result<Option<Value>> {
        let receipt = read_receipt(env, state)?;
        let Some(receipt) = receipt else {
            return Ok(None);
        };
        let selected = get(&receipt, "omp_binding");
        if !truthy(selected) || get(&receipt, "uninstalled") == &Value::Bool(true) {
            return Ok(None);
        }
        let boundary = services.omp_boundary()?;
        if text(selected, "peer_reader_capability") != Some(boundary.capability()) {
            return Ok(None);
        }
        let operation = text(&result, "operation").unwrap_or_default();
        fault(&format!("{operation}-before-stored-source-activation"))?;
        Ok(Some(boundary.complete_stored_source_activation(state)?))
    };
    match attempt() {
        Ok(None) => Ok(result),
        Ok(Some(activation)) => {
            let mut result = result;
            set(&mut result, "stored_source_activation", activation);
            set(
                &mut result,
                "private_state_writes",
                string("stored-source-activation-checked"),
            );
            Ok(result)
        }
        Err(Error::Unregistered(name)) => Err(Error::Unregistered(name)),
        Err(_) => {
            let mut result = result;
            set(&mut result, "verified", Value::Bool(false));
            set(&mut result, "stored_source_activation", string("pending"));
            set(&mut result, "receipt_committed", Value::Bool(true));
            set(
                &mut result,
                "private_state_writes",
                string("stored-source-activation-may-be-partial"),
            );
            set(
                &mut result,
                "repair",
                string(
                    "Preserve the committed runtime and private evidence; quiesce bank writers, restore unavailable banks and run am recover. Stored-source observation remains activation-required.",
                ),
            );
            Ok(result)
        }
    }
}

/// Optional test/updating hooks around activation (baseline keywords).
#[derive(Default)]
pub struct ActivationHooks<'a> {
    pub before_activate: Option<&'a mut dyn FnMut() -> Result<()>>,
    pub after_activate: Option<&'a mut dyn FnMut() -> Result<()>>,
}

/// Baseline `install`: the private-state activation guard around `_install`.
pub fn install(
    env: &Environment,
    services: &Services,
    plan: &mut Plan,
    hooks: ActivationHooks<'_>,
) -> Result<Value> {
    let mut installation = plan.old.clone().unwrap_or(Value::Object(Vec::new()));
    set(
        &mut installation,
        "rollback_repositories",
        Value::Array(plan.rollback_repositories.iter().map(string).collect()),
    );
    let selected = match &plan.uninstall {
        Some(fields) if plan.operation == "uninstall" => {
            fields.preserved_private_state_reader.clone()
        }
        _ => Some(plan.source.clone()),
    };
    let mut plan_value = plan.value()?;
    let omp_transition = plan.omp_transition;
    let binding = plan.omp_binding.is_some();
    let operation = plan.operation.clone();
    // Baseline transition mutates the one plan; body applies the same mutation.
    let transitioned: std::cell::RefCell<Option<Value>> = std::cell::RefCell::new(None);
    let mut transition = || -> Result<Option<Value>> {
        if omp_transition {
            services
                .omp_boundary()?
                .native_transition(&mut plan_value, false)?;
            *transitioned.borrow_mut() = Some(plan_value.clone());
        }
        let prepared = services
            .private_state()?
            .prepare_lifecycle(&plan_value, false)?;
        if binding {
            services
                .omp_boundary()?
                .native_activation(&mut plan_value, false)?;
        }
        Ok(selected.is_some().then_some(prepared))
    };
    let state = plan.state.clone();
    let mut hooks = hooks;
    let mut body = |proof: &Value| -> Result<Value> {
        if let Some(mutated) = transitioned.borrow_mut().take() {
            apply_native_transition(plan, &mutated)?;
        }
        plan.private_state_evidence = Some(proof.clone());
        install_now(env, services, plan, &mut hooks)
    };
    let result = services.private_state()?.activation_guard(
        &installation,
        selected.as_ref(),
        &operation,
        &mut transition,
        &mut body,
    )?;
    finish_stored_source_activation(env, services, &state, result)
}

/// Baseline `omp_boundary.native_transition` plan mutation, applied to the typed
/// plan: the disabled extension action becomes absent-before (or is dropped
/// when it has no after), and the disabled backup is recorded.
fn apply_native_transition(plan: &mut Plan, mutated: &Value) -> Result<()> {
    let backup = get(mutated, "omp_disabled_backup");
    if *backup == Value::Null {
        return Ok(());
    }
    let target = path_of(backup, "original_path");
    plan.actions
        .retain(|action| !(action.path == target && action.after == Value::Null));
    for action in plan
        .actions
        .iter_mut()
        .filter(|action| action.path == target)
    {
        action.before = Value::Null;
        action.changed = true;
    }
    plan.omp_disabled_backup = Some(backup.clone());
    Ok(())
}

fn target_record(plan: &Plan, action: &super::Action) -> Result<Value> {
    let mut target = vec![
        ("path".to_owned(), path_value(&action.path)?),
        ("kind".to_owned(), string(&action.kind)),
        ("identity".to_owned(), action.after.clone()),
        ("pre_change_identity".to_owned(), action.before.clone()),
    ];
    let extension = plan
        .omp_binding
        .as_ref()
        .and_then(|binding| text(binding, "extension_target"))
        == action.path.to_str();
    if matches!(
        action.kind.as_str(),
        "copy" | "link" | "hook-link" | "hook-config"
    ) || extension
    {
        target.push((
            "artifact_digest".into(),
            get(&plan.source, "artifact_digest").clone(),
        ));
    }
    let previous = plan.old.as_ref().and_then(|old| {
        get(old, "targets")
            .as_array()
            .unwrap_or(&[])
            .iter()
            .find(|target| get(target, "path").as_str() == action.path.to_str())
    });
    let originally_absent = || {
        previous
            .and_then(|previous| previous.get("originally_absent").cloned())
            .unwrap_or(Value::Bool(action.before == Value::Null))
    };
    if action.kind == "hook-config" && plan.operation != "uninstall" {
        target.push((
            "hook_entry".into(),
            action.hook_entry.clone().unwrap_or(Value::Null),
        ));
        target.push((
            "hook_original_shape".into(),
            action.hook_original_shape.clone().unwrap_or(Value::Null),
        ));
        if let Some(stop) = &action.stop_entry {
            target.push(("stop_entry".into(), stop.clone()));
            target.push((
                "stop_original_shape".into(),
                action.stop_original_shape.clone().unwrap_or(Value::Null),
            ));
        }
        target.push(("originally_absent".into(), originally_absent()));
    }
    if action.kind == "activation" && plan.operation != "uninstall" {
        target.push((
            "block_hash".into(),
            action.block_hash.clone().map_or(Value::Null, string),
        ));
        target.push(("originally_absent".into(), originally_absent()));
    }
    if let Some(legacy) = &action.legacy_source {
        target.push(("legacy_source".into(), legacy.clone()));
    }
    Ok(Value::Object(target))
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

/// Baseline `_install`: stage, back up, probe, activate by native exchange,
/// verify, then commit the receipt; any failure recovers before returning.
fn install_now(
    env: &Environment,
    services: &Services,
    plan: &mut Plan,
    hooks: &mut ActivationHooks<'_>,
) -> Result<Value> {
    let operation = plan.operation.clone();
    let state = plan.state.clone();
    fs::trust::state(&state)?;
    if let Some(cache) = &plan.cache_recovery {
        require(
            plan.old
                .as_ref()
                .is_some_and(|old| same(old, &cache.receipt))
                && read_receipt(env, &state)?.is_some_and(|current| same(&current, &cache.receipt)),
            "Installation receipt changed after cache qualification",
        )?;
        cache_recovery::recheck(cache)?;
    }
    let checking = operation != "uninstall";
    if checking {
        let checked = services.updating()?.banks(
            &plan.rollback_repositories,
            get(&plan.source, "compatibility"),
            false,
        )?;
        plan.rollback_repositories = observed_repositories(&checked);
    }
    let changed: Vec<usize> = (0..plan.actions.len())
        .filter(|index| {
            plan.actions
                .get(*index)
                .is_some_and(|action| action.changed)
        })
        .collect();
    if let Some(old) = &plan.old {
        let unchanged_targets =
            get(old, "targets")
                .as_array()
                .unwrap_or(&[])
                .iter()
                .all(|target| {
                    plan.actions.iter().any(|action| {
                        get(target, "path").as_str() == action.path.to_str()
                            && same(get(target, "identity"), &action.after)
                    })
                });
        if checking
            && changed.is_empty()
            && same(get(old, "source"), &plan.source)
            && unchanged_targets
        {
            let targets: Vec<&Value> = get(old, "targets")
                .as_array()
                .unwrap_or(&[])
                .iter()
                .collect();
            verify_targets(services, &targets, Some(get(old, "source")), false)?;
            return Ok(object([
                ("operation", string(&operation)),
                ("verified", Value::Bool(true)),
                ("changed", Value::Bool(false)),
                ("receipt", old.clone()),
                ("restart_clients", Value::Array(Vec::new())),
                ("receipt_path", path_value(&state.join("receipt.json"))?),
            ]));
        }
    }
    let transaction = crate::host::uuid4()?.replace('-', "");
    let mut targets = Vec::new();
    for action in &plan.actions {
        targets.push(target_record(plan, action)?);
    }
    let native_binding = plan
        .omp_binding
        .clone()
        .or_else(|| plan.old.as_ref().map(|old| get(old, "omp_binding").clone()))
        .filter(|binding| *binding != Value::Null);
    let installation_id = match plan
        .old
        .as_ref()
        .and_then(|old| text(old, "installation_id"))
    {
        Some(id) => id.to_owned(),
        None => crate::host::uuid4()?.replace('-', ""),
    };
    let installed_at = crate::host::utc_now_isoformat()?;
    let artifact_files = if operation == "uninstall" {
        Value::Object(Vec::new())
    } else {
        get(
            &services
                .artifacts
                .inspect_runtime(&plan.pack_source, &plan.binary_source)?,
            "files",
        )
        .clone()
    };
    let backups = plan
        .old
        .as_ref()
        .map(|old| get(old, "backups").clone())
        .filter(|value| value.as_array().is_some())
        .unwrap_or(Value::Array(Vec::new()));
    let runtime_state = state
        .parent()
        .ok_or_else(|| ContractError::new("Installation state has no parent"))?;
    let mut receipt = object([
        ("schema", string(RECEIPT_SCHEMA)),
        ("installation_id", string(&installation_id)),
        ("installed_at", string(&installed_at)),
        ("verified", Value::Bool(true)),
        ("source", plan.source.clone()),
        ("mode", string(&plan.mode)),
        ("presets", string(&plan.presets)),
        (
            "custom_targets",
            Value::Array(plan.custom_targets.iter().map(string).collect()),
        ),
        ("no_guidance", Value::Bool(plan.no_guidance)),
        ("bin_dir", string(&plan.bin_dir)),
        (
            "enabled_adapters",
            Value::Array(plan.adapters.iter().map(string).collect()),
        ),
        ("data_roots", Value::Array(Vec::new())),
        ("memory_data_location", string(MEMORY_DATA_LOCATION)),
        ("runtime_state", path_value(runtime_state)?),
        ("targets", Value::Array(targets.clone())),
        ("artifact_files", artifact_files),
        ("backups", backups),
        ("created_directories", Value::Array(Vec::new())),
        ("rollback_available", Value::Bool(true)),
        ("atomicity", string(ATOMICITY)),
    ]);
    if let Some(binding) = &native_binding {
        set(&mut receipt, "omp_binding", binding.clone());
    }
    if let Some(backup) = &plan.omp_disabled_backup {
        push(&mut receipt, "backups", backup.clone());
    }
    if let Some(fields) = &plan.uninstall {
        if operation == "uninstall" {
            set(&mut receipt, "uninstalled", Value::Bool(true));
            set(&mut receipt, "uninstalled_at", string(&installed_at));
            set(&mut receipt, "preserved", fields.preserved.clone());
            set(&mut receipt, "reinstall", fields.reinstall.clone());
        }
    }
    let old_source = plan.old.as_ref().map(|old| get(old, "source").clone());
    let mut journal = object([
        ("schema", string(JOURNAL_SCHEMA)),
        ("operation", string(&operation)),
        ("transaction", string(&transaction)),
        ("status", string("staging")),
        ("actions", Value::Array(Vec::new())),
        ("receipt", receipt.clone()),
        ("receipt_identity", Value::Null),
        ("created_directories", Value::Array(Vec::new())),
        (
            "previous_compatibility",
            old_source
                .as_ref()
                .map_or(Value::Null, |source| get(source, "compatibility").clone()),
        ),
        (
            "previous_artifact_digest",
            old_source
                .as_ref()
                .map_or(Value::Null, |source| get(source, "artifact_digest").clone()),
        ),
        (
            "private_state_evidence",
            plan.private_state_evidence.clone().unwrap_or(Value::Null),
        ),
        (
            "rollback_repositories",
            Value::Array(plan.rollback_repositories.iter().map(string).collect()),
        ),
    ]);
    if let Some(cache) = &plan.cache_recovery {
        set(&mut journal, "opaque_cache_recovery", cache.summary.clone());
    }
    if let Some(adoption) = &plan.adoption {
        set(&mut journal, "adoption", adoption.clone());
    }
    let journal_path = state.join("journal.json");
    if lexists(&journal_path)? {
        let prior = read_optional(&journal_path, MAX_RECORD)?.unwrap_or_default();
        write_file(
            &state.join(format!("journal-{transaction}.json")),
            &prior,
            0o600,
        )?;
    }
    replace_record(&journal_path, &journal)?;
    let activated = activate(
        env,
        services,
        plan,
        hooks,
        &changed,
        &transaction,
        &mut journal,
        &mut receipt,
        &targets,
    );
    match activated {
        Ok(result) => Ok(result),
        Err(error) => {
            // If the commit point landed, recovery verifies it; otherwise it restores old targets.
            recover(env, services, &state, false, false)?;
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn activate(
    env: &Environment,
    services: &Services,
    plan: &mut Plan,
    hooks: &mut ActivationHooks<'_>,
    changed: &[usize],
    transaction: &str,
    journal: &mut Value,
    receipt: &mut Value,
    targets: &[Value],
) -> Result<Value> {
    let operation = plan.operation.clone();
    let state = plan.state.clone();
    let journal_path = state.join("journal.json");
    for (index, position) in changed.iter().enumerate() {
        let action = plan
            .actions
            .get(*position)
            .ok_or_else(|| ContractError::new("Planned action is missing"))?
            .clone();
        let path = action.path.clone();
        let parent = path
            .parent()
            .ok_or_else(|| ContractError::new("Target has no parent"))?
            .to_path_buf();
        {
            let mut record = |created: &Path| -> Result<()> {
                push(journal, "created_directories", path_value(created)?);
                replace_record(&journal_path, journal)
            };
            make_parents(&parent, &mut record)?;
        }
        let stage = parent.join(format!(".agent-memory-{transaction}-{index}"));
        require(!lexists(&stage)?, "Staging collision")?;
        let backup = parent.join(format!(".agent-memory-backup-{transaction}-{index}"));
        let entry = object([
            ("path", path_value(&path)?),
            ("kind", string(&action.kind)),
            ("before", action.before.clone()),
            ("after", action.after.clone()),
            ("stage", path_value(&stage)?),
            (
                "backup",
                if action.before == Value::Null {
                    Value::Null
                } else {
                    path_value(&backup)?
                },
            ),
        ]);
        push(journal, "actions", entry);
        replace_record(&journal_path, journal)?;
        if action.after == Value::Null {
            // The old node moves to its rollback stage during removal.
        } else if matches!(action.kind.as_str(), "hook-link" | "command-link") {
            let target = PathBuf::from(text(&action.after, "target").unwrap_or_default());
            super::filesystem::mutate_contract("link-stage", || symlink(&stage, &target))?;
        } else if action.kind == "link" {
            let pack = path_of(&plan.source, "pack");
            super::filesystem::mutate_contract("link-stage", || symlink(&stage, &pack))?;
        } else if matches!(action.kind.as_str(), "copy" | "source") {
            let source = action
                .source_path
                .clone()
                .ok_or_else(|| ContractError::new("Payload source is required"))?;
            copy_node(&source, &stage, true)?;
        } else {
            write_file(
                &stage,
                action.raw.as_deref().unwrap_or_default(),
                action.mode_bits,
            )?;
        }
        if action.after != Value::Null {
            sync_tree(&stage)?;
        }
        sync_dir(&parent)?;
        require(
            same(&fingerprint(&stage)?, &action.after),
            "Staged target differs from preflight source",
        )?;
        if action.before != Value::Null {
            require(
                !lexists(&backup)? && same(&fingerprint(&path)?, &action.before),
                format!(
                    "Backup collision or target changed before backup: {}",
                    path.display()
                ),
            )?;
            copy_node(&path, &backup, false)?;
            require(
                same(&fingerprint(&backup)?, &action.before)
                    && same(&fingerprint(&path)?, &action.before),
                format!("Pre-activation backup readback failed: {}", path.display()),
            )?;
            push(
                receipt,
                "backups",
                object([
                    ("path", path_value(&backup)?),
                    ("original_path", path_value(&path)?),
                    ("identity", action.before.clone()),
                    ("available", Value::Bool(true)),
                    ("kind", string("pre-activation-snapshot")),
                ]),
            );
            push(
                receipt,
                "backups",
                object([
                    ("path", path_value(&stage)?),
                    ("original_path", path_value(&path)?),
                    ("identity", action.before.clone()),
                    ("available", Value::Bool(true)),
                ]),
            );
        }
        replace_record(&journal_path, journal)?;
    }
    // Exercise the actual parent filesystem before changing any target.
    let mut parents: Vec<PathBuf> = get(journal, "actions")
        .as_array()
        .unwrap_or(&[])
        .iter()
        .filter_map(|action| path_of(action, "path").parent().map(Path::to_path_buf))
        .collect();
    parents.sort();
    parents.dedup();
    for parent in &parents {
        probe(parent)?;
    }
    for position in changed {
        if let Some(action) = plan.actions.get(*position) {
            require(
                same(&fingerprint(&action.path)?, &action.before),
                format!("Target changed after preflight: {}", action.path.display()),
            )?;
        }
    }
    if let Some(before) = hooks.before_activate.as_mut() {
        before()?;
    }
    let checking = operation != "uninstall";
    if checking {
        services.updating()?.banks(
            &plan.rollback_repositories,
            get(&plan.source, "compatibility"),
            false,
        )?;
    }
    set(journal, "status", string("activating"));
    replace_record(&journal_path, journal)?;
    let actions = get(journal, "actions").as_array().unwrap_or(&[]).to_vec();
    for (index, action) in actions.iter().enumerate() {
        let path = path_of(action, "path");
        let stage = path_of(action, "stage");
        fault(&format!("{operation}-before-activate-{index}"))?;
        unchanged_parent(&path)?;
        fs::trust::state(&state)?;
        fs::trust::tree(&path, false)?;
        fs::trust::tree(&stage, false)?;
        let before = get(action, "before");
        let after = get(action, "after");
        require(
            same(&fingerprint(&path)?, before) && same(&fingerprint(&stage)?, after),
            format!(
                "Target/stage changed immediately before activation: {}",
                path.display()
            ),
        )?;
        if *after == Value::Null {
            native_rename(&path, &stage, false)?;
        } else {
            native_rename(&stage, &path, *before != Value::Null)?;
        }
        if let Some(parent) = path.parent() {
            sync_dir(parent)?;
        }
        fault(&format!("{operation}-after-activate-{index}"))?;
        require(
            same(&fingerprint(&path)?, after) && same(&fingerprint(&stage)?, before),
            "Activation identity readback failed",
        )?;
        set(journal, "activated", Value::Int((index + 1).into()));
        replace_record(&journal_path, journal)?;
    }
    let owned: Vec<&Value> = targets.iter().collect();
    verify_targets(
        services,
        &owned,
        Some(&plan.source),
        operation == "uninstall",
    )?;
    if let Some(after) = hooks.after_activate.as_mut() {
        after()?;
    }
    if checking {
        services.updating()?.banks(
            &plan.rollback_repositories,
            get(&plan.source, "compatibility"),
            false,
        )?;
    }
    let mut created = plan
        .old
        .as_ref()
        .and_then(|old| {
            get(old, "created_directories")
                .as_array()
                .map(<[Value]>::to_vec)
        })
        .unwrap_or_default();
    created.extend(
        get(journal, "created_directories")
            .as_array()
            .unwrap_or(&[])
            .iter()
            .cloned(),
    );
    set(receipt, "created_directories", Value::Array(created));
    let receipt_path = state.join("receipt.json");
    if lexists(&receipt_path)? {
        let backup = state.join(format!("receipt-{transaction}.json"));
        let raw = read_optional(&receipt_path, MAX_RECORD)?.unwrap_or_default();
        write_file(&backup, &raw, 0o600)?;
        push(
            receipt,
            "backups",
            object([
                ("path", path_value(&backup)?),
                ("original_path", path_value(&receipt_path)?),
                ("identity", fingerprint(&backup)?),
                ("available", Value::Bool(true)),
            ]),
        );
    }
    set(journal, "receipt", receipt.clone());
    set(
        journal,
        "receipt_identity",
        object([
            ("kind", string("file")),
            ("hash", string(sha(&encoded(receipt)?))),
            ("mode", Value::Int(0o600u32.into())),
        ]),
    );
    replace_record(&journal_path, journal)?;
    fault(&format!("{operation}-before-receipt"))?;
    replace_record(&receipt_path, receipt)?;
    require(
        same(
            &fingerprint(&receipt_path)?,
            get(journal, "receipt_identity"),
        ),
        "Receipt readback failed",
    )?;
    fault(&format!("{operation}-after-receipt"))?;
    set(journal, "status", string("complete"));
    replace_record(&journal_path, journal)?;
    let plan_value = plan.value()?;
    if plan.omp_transition || plan.omp_binding.is_some() {
        services
            .omp_boundary()?
            .finish_native_transition(&plan_value)?;
    }
    let mut result = vec![
        ("operation".to_owned(), string(&operation)),
        ("verified".to_owned(), Value::Bool(true)),
        ("changed".to_owned(), Value::Bool(true)),
        ("receipt".to_owned(), receipt.clone()),
    ];
    if let Some(cache) = &plan.cache_recovery {
        let mut summary = cache.summary.clone();
        set(&mut summary, "planned", Value::Bool(false));
        set(&mut summary, "old_runtime_preserved", Value::Bool(true));
        let root = path_value(&cache.root)?;
        let preserved: Vec<Value> = get(receipt, "backups")
            .as_array()
            .unwrap_or(&[])
            .iter()
            .filter(|backup| get(backup, "original_path") == &root)
            .map(|backup| get(backup, "path").clone())
            .collect();
        set(
            &mut summary,
            "preserved_source_backups",
            Value::Array(preserved),
        );
        result.push(("cache_recovery".into(), summary));
    }
    if let Some(adoption) = &plan.adoption {
        result.push(("adoption".into(), adoption.clone()));
        result.push(("restart_notice".into(), string(RESTART_NOTICE)));
    }
    result.push((
        "restart_clients".into(),
        Value::Array(plan.restart.iter().map(string).collect()),
    ));
    result.push(("receipt_path".into(), path_value(&receipt_path)?));
    let _ = env;
    Ok(Value::Object(result))
}

/// Baseline `failure_details`.
pub fn failure_details(error: &Error) -> Option<Value> {
    match error {
        Error::Release(crate::release_distribution::Error::Download(download)) => {
            Some(object([("download", download.details.clone())]))
        }
        _ => None,
    }
}

/// Baseline `failure_repair`.
pub fn failure_repair(env: &Environment, error: &Error) -> String {
    match error {
        Error::Release(crate::release_distribution::Error::Download(download)) => {
            return download.repair.clone();
        }
        Error::Release(_) => {
            return "Preserve the active runtime and receipts. Check the trusted release, curl and OpenSSH signature support from an operator-trusted installer; never fall back to unsigned bytes.".to_owned();
        }
        Error::Contract(error) if matches!(error.kind(), Kind::TrustPath { .. }) => {
            return "Preserve this path and its contents. Use a trusted outside helper if the installed runtime may have been exposed. Review ownership and permissions yourself or choose private per-user XDG/client paths; the installer does not chmod or adopt an untrusted layout.".to_owned();
        }
        Error::Layout(_) => {
            return "Choose separate writable XDG state/data and client targets outside Memory Banks and source trees; rerun install --check. Do not run recovery under the refused layout.".to_owned();
        }
        _ => {}
    }
    let observed = (|| -> Result<(Option<Value>, bool)> {
        let state = locations(env)?.state;
        let journal = read_journal(env, &state)?;
        let committed = match &journal {
            Some(journal) if text(journal, "status") == Some("activating") => {
                receipt_committed(&state, journal)?
            }
            _ => false,
        };
        Ok((journal, committed))
    })();
    let Ok((journal, committed)) = observed else {
        return "Preserve reported paths and installation records; inspect diagnostics and reconcile unreadable recovery metadata before retrying.".to_owned();
    };
    if let Some(journal) = journal
        .as_ref()
        .filter(|journal| !matches!(text(journal, "status"), Some("complete" | "rolled-back")))
    {
        if committed {
            let command = if get(get(journal, "receipt"), "uninstalled") == &Value::Bool(true) {
                "install"
            } else {
                "install or update"
            };
            return format!(
                "Preserve the committed receipt and journal. Use the trusted helper to re-plan {command}; strict recover cannot accept changed targets."
            );
        }
        return "Preserve reported paths; rerun this trusted helper with recover before retrying installation.".to_owned();
    }
    "Preserve reported paths; resolve the reported planning or prerequisite error, then rerun install --check before retrying. No pending activation requires recovery.".to_owned()
}

fn command_result(operation: &str, outcome: Result<Value>, env: &Environment) -> CommandResult {
    let value = match outcome {
        Ok(value) => value,
        Err(error) => {
            let mut fields = vec![
                ("operation".to_owned(), string(operation)),
                ("verified".to_owned(), Value::Bool(false)),
                ("error".to_owned(), string(error.to_string())),
                ("repair".to_owned(), string(failure_repair(env, &error))),
            ];
            if let Some(Value::Object(details)) = failure_details(&error) {
                fields.extend(details);
            }
            Value::Object(fields)
        }
    };
    let success = get(&value, "verified") == &Value::Bool(true)
        || text(&value, "schema") == Some(super::PLAN_SCHEMA);
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

fn install_value(
    env: &Environment,
    services: &Services,
    request: &InstallRequest,
) -> Result<Value> {
    let locations = locations(env)?;
    let state = locations.state.clone();
    let old = read_receipt(env, &state)?;
    require(
        request.remote.is_none() && request.reference.is_none(),
        super::MANAGED_GUIDANCE,
    )?;
    preflight_layout(
        env,
        services,
        Command::Install,
        request,
        &locations,
        old.as_ref(),
    )?;
    // Prove the signed release prerequisite before creating lock/owner records.
    let development = old.as_ref().is_some_and(|receipt| {
        !super::python_era_kind(receipt)
            && text(get(receipt, "source"), "kind") == Some("development")
    });
    let staged = match (
        &request.bootstrap_binary,
        &request.development_source,
        development,
    ) {
        (Some(binary), None, false) => Some(stage_release(services, &absolute(env, binary)?)?),
        _ => None,
    };
    let bootstrap_root = staged.as_ref().map(|(_, root)| root.clone());
    let result = if request.check {
        require_replannable_journal(env, &state)?;
        let current = read_receipt(env, &state)?;
        let plan = prepare(
            env,
            services,
            request,
            &locations,
            current.as_ref(),
            PrepareOptions {
                bootstrap_root: bootstrap_root.as_deref(),
                ..PrepareOptions::default()
            },
        )?;
        let mut result = public_plan(&plan)?;
        let mut plan_value = plan.value()?;
        if plan.omp_binding.is_some() {
            let activation = services
                .omp_boundary()?
                .native_activation(&mut plan_value, true)?;
            set(&mut result, "omp_layout_activation", activation);
        }
        let compatibility =
            services
                .updating()?
                .banks(&[], get(&plan.source, "compatibility"), false)?;
        set(&mut result, "compatibility", compatibility);
        let private = services.private_state()?;
        let target = private.prepare_lifecycle(&plan_value, true)?;
        let installation = plan.old.clone().unwrap_or(Value::Null);
        let compatible = private.require_compatible(&installation, &target, "install")?;
        set(
            &mut result,
            "private_compatibility",
            private.public_summary(&compatible)?,
        );
        result
    } else {
        let paths = lifecycle_paths(env, Command::Install, request, &locations, old.as_ref())?;
        crate::runtime_faults::operation("install", &paths, || {
            locked(env, &state, || {
                recover(env, services, &state, true, true)?;
                let current = read_receipt(env, &state)?;
                let mut plan = prepare(
                    env,
                    services,
                    request,
                    &locations,
                    current.as_ref(),
                    PrepareOptions {
                        bootstrap_root: bootstrap_root.as_deref(),
                        ..PrepareOptions::default()
                    },
                )?;
                install(env, services, &mut plan, ActivationHooks::default())
            })
        })?
    };
    drop(staged);
    crate::runtime_faults::response("install", &result)?;
    Ok(result)
}

/// `am install`: the typed baseline `main` path for the install command.
pub fn install_command(
    env: &Environment,
    services: &Services,
    request: &InstallRequest,
) -> CommandResult {
    command_result("install", install_value(env, services, request), env)
}

fn recover_value(env: &Environment, services: &Services) -> Result<Value> {
    let locations = locations(env)?;
    let state = locations.state.clone();
    let old = read_receipt(env, &state)?;
    let request = InstallRequest::default();
    preflight_layout(
        env,
        services,
        Command::Recover,
        &request,
        &locations,
        old.as_ref(),
    )?;
    let result = if !lexists(&state.join("journal.json"))? {
        let result = object([
            ("operation", string("recover")),
            ("verified", Value::Bool(true)),
            ("status", string("no-pending-transaction")),
            ("writes_performed", Value::Bool(false)),
        ]);
        if old
            .as_ref()
            .is_some_and(|old| truthy(get(old, "omp_binding")))
        {
            locked(env, &state, || {
                finish_stored_source_activation(
                    env,
                    services,
                    &state,
                    without(&result, "writes_performed"),
                )
            })?
        } else {
            result
        }
    } else {
        let paths = lifecycle_paths(env, Command::Recover, &request, &locations, old.as_ref())?;
        crate::runtime_faults::operation("recover", &paths, || {
            locked(env, &state, || recover(env, services, &state, false, true))
        })?
    };
    crate::runtime_faults::response("recover", &result)?;
    Ok(result)
}

/// `am recover`: verify a committed transaction or roll back an interrupted one.
pub fn recover_command(env: &Environment, services: &Services) -> CommandResult {
    command_result("recover", recover_value(env, services), env)
}
