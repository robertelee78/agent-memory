//! Remove proven runtime ownership while preserving project memory and edits.
//!
//! Baseline `uninstallation.py`. Only targets recorded in the Rust-era (v3)
//! receipt are removed or restored: command links, client copies/links,
//! activation blocks, owned hook groups, the owned runtime directory and an
//! adopted Python-era `legacy-source` tree. Banks, foreign configuration and
//! recorded backups are never touched. Removal reuses installation's
//! journaled transaction, so interruption recovers exactly like install.

use crate::client_hooks;
use crate::codec::json::Value;
use crate::error::ContractError;
use crate::fs;
use crate::installation::{
    self as install, ADOPTION_GUIDANCE, Action, BEGIN, Command, END, Environment, Error,
    InstallRequest, Locations, Plan, Result, Services, UninstallFields, require,
};
use crate::lifecycle::{CommandResult, get, object, string, text};
use crate::updating::{merged, printed};
use std::path::{Path, PathBuf};

/// Baseline `execute` repair text for every uninstall refusal.
pub const REPAIR: &str = "Preserve the external receipt, backups and reported paths; use a trusted complete package to recover or reconcile ownership.";
/// Native private-state capabilities an OMP transition may still be missing.
const NATIVE_CAPABILITIES: [&str; 2] = ["omp-harness-v1", "omp-stored-source-v2"];
/// Invocation routes a preserved capable runtime must still own.
const INVOCATION_KINDS: [&str; 4] = ["wrapper", "hook-link", "hook-config", "command-link"];

fn contract(message: impl Into<String>) -> Error {
    Error::Contract(ContractError::new(message.into()))
}

fn path_of(value: &Value, key: &str) -> PathBuf {
    PathBuf::from(text(value, key).unwrap_or_default())
}

fn targets(receipt: &Value) -> &[Value] {
    get(receipt, "targets").as_array().unwrap_or(&[])
}

fn flag(value: &Value, key: &str) -> bool {
    get(value, key) == &Value::Bool(true)
}

fn file_identity(raw: &[u8], before: &Value) -> Value {
    object([
        ("kind", string("file")),
        ("hash", string(install::sha(raw))),
        ("mode", get(before, "mode").clone()),
    ])
}

/// The baseline `reinstall` guidance, retargeted to the `am` installer.
/// Signed releases are reinstalled through the trusted bootstrap.
pub fn reinstall(old: &Value) -> Value {
    let source = get(old, "source");
    let mut argv = vec![string("am"), string("install")];
    let requires = if text(source, "kind") == Some("release") {
        "Run the trusted bootstrap (install.sh); it verifies the signed release am and runs this install"
    } else {
        argv.push(string("--development-source"));
        argv.push(get(source, "path").clone());
        "Use a trusted am built from the development source (target/release/am)"
    };
    for (option, key) in [
        ("--mode", "mode"),
        ("--presets", "presets"),
        ("--bin-dir", "bin_dir"),
    ] {
        argv.push(string(option));
        argv.push(get(old, key).clone());
    }
    for target in get(old, "custom_targets").as_array().unwrap_or(&[]) {
        argv.push(string("--target-dir"));
        argv.push(target.clone());
    }
    if flag(old, "no_guidance") {
        argv.push(string("--no-guidance"));
    }
    object([
        ("requires", string(requires)),
        ("argv", Value::Array(argv)),
        (
            "project_memory",
            string("preserved; do not initialize a replacement bank"),
        ),
    ])
}

/// The recorded pre-activation snapshot of `path`, when it still verifies.
fn original_snapshot(old: &Value, path: &Path, limit: usize) -> Result<Option<Vec<u8>>> {
    let path_text = install::path_text(path)?;
    for backup in get(old, "backups").as_array().unwrap_or(&[]) {
        if text(backup, "original_path") != Some(path_text)
            || text(backup, "kind") != Some("pre-activation-snapshot")
        {
            continue;
        }
        let saved = path_of(backup, "path");
        let identity = get(backup, "identity");
        if text(identity, "kind") == Some("file")
            && install::same(&install::fingerprint(&saved)?, identity)
        {
            return install::read_optional(&saved, limit);
        }
        // A later update's snapshot is not the original user file.
        return Ok(None);
    }
    Ok(None)
}

/// Baseline `without_block`: remove exactly the owned activation block.
pub fn without_block(path: &Path, target: &Value, old: &Value) -> Result<Option<Vec<u8>>> {
    let raw = install::read_optional(path, install::MAX_GUIDANCE)?.unwrap_or_default();
    let text_value = std::str::from_utf8(&raw)
        .map_err(|_| contract("Activation file is not UTF-8; preserve and reconcile manually"))?;
    // Prove exactly one owned block.
    install::activation(&raw, text(target, "block_hash"), install::SHIM)?;
    let begin = text_value
        .find(BEGIN)
        .ok_or_else(|| contract("Owned activation block is missing"))?;
    let end = text_value
        .find(END)
        .map(|end| end + END.len())
        .ok_or_else(|| contract("Owned activation block is missing"))?;
    let prefix = text_value.get(..begin).unwrap_or_default().as_bytes();
    let suffix = text_value.get(end..).unwrap_or_default().as_bytes();
    let mut outside = prefix.to_vec();
    outside.extend_from_slice(suffix);
    if flag(target, "originally_absent") && prefix.is_empty() && matches!(suffix, b"" | b"\n") {
        return Ok(None);
    }
    // Restore exact original bytes only when current surrounding content
    // proves that no later outside edit would be lost. Backups are never deleted.
    if let Some(original) = original_snapshot(old, path, install::MAX_GUIDANCE)? {
        let mut expected_prefix = original.clone();
        if !original.is_empty() && !original.ends_with(b"\n") {
            expected_prefix.push(b'\n');
        }
        let contains_begin = original
            .windows(BEGIN.len())
            .any(|window| window == BEGIN.as_bytes());
        if !contains_begin && prefix == expected_prefix.as_slice() && suffix == b"\n" {
            return Ok(Some(original));
        }
    }
    Ok(Some(outside))
}

fn shape_flag(shape: &Value, key: &str) -> bool {
    get(shape, key) == &Value::Bool(true)
}

/// Remove the owned prompt and Stop groups of a verified hook configuration.
fn without_hooks(
    target: &Value,
    old: &Value,
    path: &Path,
    before: &Value,
) -> Result<Option<Vec<u8>>> {
    let checked = install::verify_hook_target(target)?;
    require(
        install::same(&checked.identity, before),
        "Client hook configuration changed during planning",
    )?;
    let current = checked.raw;
    let absent = flag(target, "originally_absent");
    let shape = get(target, "hook_original_shape");
    let mut raw = client_hooks::remove(
        &current,
        get(target, "hook_entry"),
        absent,
        shape_flag(shape, "event_present"),
        shape_flag(shape, "hooks_present"),
        client_hooks::EVENT,
    )?;
    if let Some(stop) = target
        .get("stop_entry")
        .filter(|stop| **stop != Value::Null)
    {
        let shape = get(target, "stop_original_shape");
        // Baseline: removing Stop from an already removed document refuses.
        let prompt_removed = raw
            .as_deref()
            .ok_or_else(|| contract("unsupported_shape"))?;
        raw = client_hooks::remove(
            prompt_removed,
            stop,
            absent,
            shape_flag(shape, "event_present"),
            shape_flag(shape, "hooks_present"),
            "Stop",
        )?;
    }
    // Restore exact original bytes only if the entire current JSON still
    // matches the original plus our owned entries.
    if let Some(original) = original_snapshot(old, path, client_hooks::MAX_CONFIG)? {
        let mut rebuilt = client_hooks::merge(
            &original,
            None,
            get(target, "hook_entry"),
            client_hooks::EVENT,
        )?;
        if let Some(stop) = target
            .get("stop_entry")
            .filter(|stop| **stop != Value::Null)
        {
            rebuilt = client_hooks::merge(&rebuilt, None, stop, "Stop")?;
        }
        if client_hooks::equal(
            &client_hooks::parse(&rebuilt)?,
            &client_hooks::parse(&current)?,
        )? {
            raw = Some(original);
        }
    }
    Ok(raw)
}

/// Baseline managed-source requalification: Git metadata of an adopted
/// Python-era managed checkout may change without changing its payload.
fn requalified_legacy_source(target: &Value, path: &Path, before: &Value) -> bool {
    let legacy = get(target, "legacy_source");
    let identity = get(target, "identity");
    if text(legacy, "kind") != Some("managed")
        || text(before, "kind") != Some("directory")
        || text(identity, "kind") != Some("directory")
        || get(before, "mode") != get(identity, "mode")
    {
        return false;
    }
    crate::managed_source::verify(path, legacy).is_ok()
        && install::fingerprint(path).is_ok_and(|current| install::same(&current, before))
}

struct Planner<'a> {
    env: &'a Environment,
    old: &'a Value,
    actions: Vec<Action>,
    preserved: Vec<Value>,
}

impl Planner<'_> {
    fn preserve(&mut self, target: &Value, reason: &str) {
        self.preserved.push(object([
            ("path", get(target, "path").clone()),
            ("kind", get(target, "kind").clone()),
            ("reason", string(reason)),
        ]));
    }

    /// Baseline `prepare.inspect`.
    fn inspect(&mut self, target: &Value) -> Result<()> {
        let path = path_of(target, "path");
        let kind = text(target, "kind").unwrap_or_default().to_owned();
        install::preflight_path(self.env, &path)?;
        fs::trust::tree(&path, false)?;
        let before = install::fingerprint(&path)?;
        let mut raw = None;
        let mut after = Value::Null;
        if kind == "hook-config" && before == Value::Null {
            self.preserve(target, "Client hook configuration is unavailable");
            return Ok(());
        }
        if before != Value::Null && kind == "hook-config" {
            if text(&before, "kind") != Some("file") {
                self.preserve(
                    target,
                    "Client hook configuration is no longer an ordinary file",
                );
                return Ok(());
            }
            match without_hooks(target, self.old, &path, &before) {
                Ok(value) => raw = value,
                Err(Error::Contract(_) | Error::Hook(_)) => {
                    self.preserve(
                        target,
                        "Owned client hook was changed; unrelated configuration preserved",
                    );
                    return Ok(());
                }
                Err(error) => return Err(error),
            }
            if let Some(raw) = &raw {
                after = file_identity(raw, &before);
            }
        } else if before != Value::Null && kind == "activation" {
            if text(&before, "kind") != Some("file") {
                self.preserve(target, "Activation path is no longer an ordinary file");
                return Ok(());
            }
            match without_block(&path, target, self.old) {
                Ok(value) => raw = value,
                Err(Error::Contract(_)) => {
                    self.preserve(
                        target,
                        "Owned activation block was changed; surrounding content preserved",
                    );
                    return Ok(());
                }
                Err(error) => return Err(error),
            }
            if let Some(raw) = &raw {
                after = file_identity(raw, &before);
            }
        } else if before != Value::Null && !install::same(&before, get(target, "identity")) {
            // An adopted managed checkout's .git metadata can change without
            // changing its receipt-owned payload. Re-establish that ownership,
            // retaining the exact current fingerprint as the transaction guard.
            if !(kind == "legacy-source" && requalified_legacy_source(target, &path, &before)) {
                self.preserve(
                    target,
                    "Current identity differs from installation ownership",
                );
                return Ok(());
            }
        }
        let mode_bits = if text(&before, "kind") == Some("file") {
            get(&before, "mode")
                .as_int()
                .and_then(|value| value.as_u64())
                .and_then(|value| u32::try_from(value).ok())
                .unwrap_or(0o644)
        } else {
            0o644
        };
        let changed = !install::same(&before, &after);
        self.actions.push(Action {
            path,
            kind,
            before,
            after,
            raw,
            source_path: None,
            mode_bits,
            changed,
            block_hash: None,
            hook_entry: None,
            hook_original_shape: None,
            stop_entry: None,
            stop_original_shape: None,
            legacy_source: None,
        });
        Ok(())
    }
}

fn source_kind(target: &Value) -> bool {
    matches!(text(target, "kind"), Some("source" | "legacy-source"))
}

/// Baseline `prepare`: plan removal of every unchanged owned target.
pub fn prepare(env: &Environment, old: &Value, locations: &Locations) -> Result<Plan> {
    let mut planner = Planner {
        env,
        old,
        actions: Vec::new(),
        preserved: Vec::new(),
    };
    for target in targets(old).iter().filter(|target| !source_kind(target)) {
        planner.inspect(target)?;
    }
    for target in targets(old).iter().filter(|target| source_kind(target)) {
        if planner.preserved.is_empty() {
            planner.inspect(target)?;
        } else {
            planner.preserve(
                target,
                "Retained conservatively because a preserved client target may still reference it",
            );
        }
    }
    let source = get(old, "source").clone();
    let pack = path_of(&source, "pack");
    let binary = install::source_binary(&source).unwrap_or_else(|_| pack.clone());
    let strings = |key: &str| -> Vec<String> {
        get(old, key)
            .as_array()
            .unwrap_or(&[])
            .iter()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect()
    };
    Ok(Plan {
        operation: "uninstall".to_owned(),
        state: locations.state.clone(),
        source,
        mode: text(old, "mode").unwrap_or_default().to_owned(),
        presets: text(old, "presets").unwrap_or_default().to_owned(),
        custom_targets: strings("custom_targets"),
        no_guidance: flag(old, "no_guidance"),
        bin_dir: text(old, "bin_dir").unwrap_or_default().to_owned(),
        adapters: Vec::new(),
        actions: planner.actions,
        restart: Vec::new(),
        omp_binding: None,
        omp_peers_upgraded: false,
        omp_layout_quiescence: Vec::new(),
        layout_quiescence: Vec::new(),
        old: Some(old.clone()),
        pack_source: pack,
        binary_source: binary,
        cache_recovery: None,
        rollback_repositories: Vec::new(),
        private_state_evidence: None,
        adoption: None,
        uninstall: Some(UninstallFields {
            preserved: Value::Array(planner.preserved),
            reinstall: reinstall(old),
            preserved_private_state_reader: None,
            omp_transition: false,
            omp_disabled_backup: None,
        }),
    })
}

fn relative(left: &Path, right: &Path) -> bool {
    left.starts_with(right) || right.starts_with(left)
}

/// Baseline `preserved_reader`: select a real separate already-active
/// installation, not a capability flag.
pub fn preserved_reader(
    env: &Environment,
    services: &Services,
    selection: &Path,
    plan: &Plan,
) -> Result<Value> {
    let supplied = install::absolute(env, selection)?;
    let select = "Select the existing receipt.json of the capable installation to preserve";
    require(
        supplied
            .file_name()
            .is_some_and(|name| name == "receipt.json")
            && install::is_regular(&supplied)?,
        select,
    )?;
    let parent = supplied.parent().ok_or_else(|| contract(select))?;
    let other = install::read_receipt(env, parent)?;
    let old_id = plan
        .old
        .as_ref()
        .map(|old| get(old, "installation_id").clone())
        .unwrap_or(Value::Null);
    let other = other
        .filter(|other| get(other, "installation_id") != &old_id)
        .ok_or_else(|| {
            contract("The preserved capable reader must be a separate active installation")
        })?;
    let other_source = get(&other, "source");
    let recorded: Vec<&Value> = targets(&other).iter().collect();
    install::verify_targets(services, &recorded, Some(other_source), false)?;
    let pack = path_of(other_source, "pack");
    let runtime = if install::receipt_is_rust_era(&other) {
        services
            .artifacts
            .inspect_runtime(&pack, &install::source_binary(other_source)?)?
    } else {
        services.artifacts.inspect(&pack)?
    };
    require(
        get(&runtime, "artifact_digest") == get(other_source, "artifact_digest"),
        "Preserved runtime changed from its actual owned artifact",
    )?;
    let mut protected = vec![path_of(other_source, "path"), parent.to_path_buf()];
    protected.extend(recorded.iter().map(|target| path_of(target, "path")));
    let overlapping = plan
        .actions
        .iter()
        .filter(|action| action.changed)
        .any(|action| protected.iter().any(|path| relative(&action.path, path)));
    require(
        !overlapping,
        "Uninstall would remove part of the selected preserved capable runtime",
    )?;
    let routes: Vec<Value> = recorded
        .iter()
        .filter(|target| INVOCATION_KINDS.contains(&text(target, "kind").unwrap_or_default()))
        .map(|target| get(target, "path").clone())
        .collect();
    require(
        !routes.is_empty(),
        "Preserved runtime must retain an actually active executable invocation route",
    )?;
    let raw = crate::layout_recovery_state::read_bytes(&supplied, install::MAX_RECORD, false)?
        .unwrap_or_default();
    Ok(merged(
        other_source.clone(),
        object([
            ("compatibility", get(&runtime, "compatibility").clone()),
            ("already_active_verified", Value::Bool(true)),
            (
                "reader_receipt_path",
                string(install::path_text(&supplied)?),
            ),
            ("reader_receipt_sha256", string(install::sha(&raw))),
            ("reader_invocation_routes", Value::Array(routes)),
        ]),
    ))
}

/// The baseline `uninstall` argparse namespace.
#[derive(Debug, Clone, Default)]
pub struct UninstallRequest {
    pub check: bool,
    pub omp_writers_quiesced: bool,
    pub preserve_capable_runtime: Option<PathBuf>,
    pub layout_quiescence: Vec<PathBuf>,
}

fn work(
    env: &Environment,
    services: &Services,
    request: &UninstallRequest,
    locations: &Locations,
    native_preflight: bool,
) -> Result<Value> {
    let state = &locations.state;
    let Some(current) = install::read_receipt(env, state)? else {
        let historical = install::load(&state.join("receipt.json"))?;
        require(
            flag(&historical, "uninstalled"),
            "No active ownership receipt; do not guess removal targets",
        )?;
        let journal = install::read_journal(env, state)?;
        require(
            journal.as_ref().is_none_or(|journal| {
                matches!(text(journal, "status"), Some("complete" | "rolled-back"))
            }),
            "Removal receipt is committed but targets remain unverified; use trusted install to re-plan",
        )?;
        return Ok(object([
            ("operation", string("uninstall")),
            ("verified", Value::Bool(!request.check)),
            ("check_complete", Value::Bool(request.check)),
            ("changed", Value::Bool(false)),
            ("uninstalled", Value::Bool(true)),
            ("receipt", historical.clone()),
            ("preserved", get(&historical, "preserved").clone()),
            ("reinstall", get(&historical, "reinstall").clone()),
        ]));
    };
    require(!install::python_era_kind(&current), ADOPTION_GUIDANCE)?;
    let mut plan = prepare(env, &current, locations)?;
    let selected = match &request.preserve_capable_runtime {
        Some(selection) => Some(preserved_reader(env, services, selection, &plan)?),
        None => None,
    };
    if let Some(fields) = plan.uninstall.as_mut() {
        fields.omp_transition = request.omp_writers_quiesced;
        fields.preserved_private_state_reader = selected;
    }
    plan.layout_quiescence = request.layout_quiescence.clone();
    let omp_transition = request.omp_writers_quiesced;
    let plan_value = uninstall_value(&plan)?;
    if omp_transition && request.check && !native_preflight {
        services
            .omp_boundary()?
            .native_transition(&plan_value, true)?;
    }
    let private = services.private_state()?;
    let target = private.prepare_lifecycle(&plan_value, true)?;
    if omp_transition && (native_preflight || !request.check) {
        // Only the real transition can disable the owned route and prove a
        // drain. Never project that future state into a public preview.
        let cwd = fs::canonical_path(
            &std::env::current_dir().map_err(|error| contract(error.to_string()))?,
            false,
        )?;
        let proof = crate::private_state::required_private_state_capabilities(
            Some(&current),
            Some(&target).filter(|target| **target != Value::Null),
            "uninstall",
            &cwd,
            &locations.runtime_pack(),
        )?;
        let missing_native = get(&proof, "missing_capabilities")
            .as_array()
            .unwrap_or(&[])
            .iter()
            .all(|value| NATIVE_CAPABILITIES.contains(&value.as_str().unwrap_or_default()));
        let unknown = get(&proof, "unknown_state")
            .as_array()
            .is_some_and(|items| !items.is_empty());
        require(
            !unknown && missing_native,
            "Private-state runtime compatibility is pending; preserve the capable active runtime and evidence",
        )?;
    } else {
        private.require_compatible(&current, &target, "uninstall")?;
    }
    let fields = plan
        .uninstall
        .as_ref()
        .ok_or_else(|| contract("Uninstall plan is incomplete"))?;
    let preserved = fields.preserved.clone();
    let reinstall = fields.reinstall.clone();
    if request.check {
        return Ok(object([
            ("operation", string("uninstall")),
            ("verified", Value::Bool(false)),
            ("check_complete", Value::Bool(true)),
            ("installation_writes_performed", Value::Bool(false)),
            (
                "targets",
                Value::Array(
                    plan.actions
                        .iter()
                        .map(Action::value)
                        .collect::<Result<_>>()?,
                ),
            ),
            ("preserved", preserved),
            ("reinstall", reinstall),
            ("project_memory_writes_performed", Value::Bool(false)),
            ("purge_performed", Value::Bool(false)),
        ]));
    }
    let result = install::install(env, services, &mut plan, Default::default())?;
    Ok(merged(
        result,
        object([
            ("uninstalled", Value::Bool(true)),
            ("preserved", preserved),
            ("reinstall", reinstall),
            ("project_memory_writes_performed", Value::Bool(false)),
            ("purge_performed", Value::Bool(false)),
        ]),
    ))
}

/// The plan dictionary the baseline passed to its lazy callees.
fn uninstall_value(plan: &Plan) -> Result<Value> {
    let mut value = plan.value()?;
    if let Some(fields) = &plan.uninstall {
        let mut extra = vec![
            ("preserved".to_owned(), fields.preserved.clone()),
            ("reinstall".to_owned(), fields.reinstall.clone()),
        ];
        if let Some(reader) = &fields.preserved_private_state_reader {
            extra.push(("preserved_private_state_reader".to_owned(), reader.clone()));
        }
        if fields.omp_transition {
            extra.push((
                "omp_transition".to_owned(),
                object([("writers_quiesced", Value::Bool(true))]),
            ));
        }
        value = merged(value, Value::Object(extra));
    }
    Ok(value)
}

/// Baseline `run`.
pub fn run(
    env: &Environment,
    services: &Services,
    request: &UninstallRequest,
    native_preflight: bool,
) -> Result<Value> {
    let locations = install::locations(env)?;
    let state = &locations.state;
    let old = install::read_receipt(env, state)?;
    if let Some(old) = &old {
        require(!install::python_era_kind(old), ADOPTION_GUIDANCE)?;
    }
    install::preflight_layout(
        env,
        services,
        Command::Recover,
        &InstallRequest::default(),
        &locations,
        old.as_ref(),
    )?;
    if request.check {
        install::require_replannable_journal(env, state)?;
        return work(env, services, request, &locations, native_preflight);
    }
    // Refuse a known private-state hold before publishing even diagnostic
    // lock ownership. The locked path repeats this check and guards activation.
    let journal = install::read_journal(env, state)?;
    let settled = match &journal {
        None => true,
        Some(journal) => match text(journal, "status") {
            Some("complete" | "rolled-back") => true,
            Some("activating") => install::receipt_committed(state, journal)?,
            _ => false,
        },
    };
    if old.is_some() && settled {
        let preview = UninstallRequest {
            check: true,
            ..request.clone()
        };
        run(env, services, &preview, true)?;
    }
    let mut paths = vec![
        state.clone(),
        locations.code.clone(),
        locations.runtime.clone(),
    ];
    if let Some(old) = &old {
        paths.extend(targets(old).iter().map(|target| path_of(target, "path")));
    }
    crate::runtime_faults::operation("uninstall", &paths, || {
        install::locked(env, state, || {
            install::recover(env, services, state, true, true)?;
            work(env, services, request, &locations, false)
        })
    })
}

/// `am uninstall`: the typed baseline `execute`.
pub fn uninstall_command(
    env: &Environment,
    services: &Services,
    request: &UninstallRequest,
) -> CommandResult {
    let outcome = run(env, services, request, false).and_then(|result| {
        crate::runtime_faults::response("uninstall", &result)?;
        Ok(result)
    });
    printed(match outcome {
        Ok(value) => value,
        Err(error) => object([
            ("operation", string("uninstall")),
            ("verified", Value::Bool(false)),
            ("error", string(error.to_string())),
            ("repair", string(REPAIR)),
        ]),
    })
}
