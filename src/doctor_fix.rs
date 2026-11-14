//! Reconcile only independently verified, receipt-owned runtime faults.
//!
//! Baseline `doctor_fix.py` (`am doctor --fix`). Diagnostics is not ported
//! yet: its `run` and `Report` are reached through the `Diagnostics` seam,
//! whose absence is a typed refusal. Repairs reuse installation's journaled
//! recovery and transaction; nothing changed by a user is replaced.

use crate::codec::json::Value;
use crate::error::ContractError;
use crate::fs;
use crate::installation::{
    self as install, ADOPTION_GUIDANCE, Command, Environment, Error, Locations, MANAGED_GUIDANCE,
    PrepareOptions, Result, Services, require,
};
use crate::lifecycle::{get, object, string, text};
use crate::updating::merged;
use std::cell::Cell;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

/// Owned target kinds a doctor finding may report as failed.
pub const TARGET_COMPONENTS: [&str; 7] = [
    "runtime.target.hook-link",
    "runtime.target.hook-config",
    "runtime.target.link",
    "runtime.target.copy",
    "runtime.target.wrapper",
    "runtime.target.activation",
    "runtime.target.command-link",
];
/// Missing owned targets the installer may recreate from the receipt.
pub const RECREATABLE: [&str; 7] = [
    "hook-config",
    "hook-link",
    "link",
    "copy",
    "wrapper",
    "activation",
    "command-link",
];
/// Baseline `migration.MAX_FILE` bound for the bank transaction marker.
pub const MAX_MARKER: usize = 65536;

/// Baseline `diagnostics` callees (`run`, `Report`). Implemented by `diagnostics`.
pub trait Diagnostics: Send + Sync {
    /// `diagnostics.run(operation, repo, projection)`: the finished report
    /// (`checks`, `warnings`, `ok`, …) or a refusal (`diagnostics.ERRORS`).
    fn run(&self, operation: &str, repo: &Path, projection: Option<&Path>) -> Result<Value>;
    /// `Report(operation)` with one added finding, then `finish()`.
    fn report(
        &self,
        operation: &str,
        component: &str,
        state: &str,
        detail: &str,
        repair: &str,
    ) -> Result<Value>;
}

static DIAGNOSTICS: RwLock<Option<Arc<dyn Diagnostics>>> = RwLock::new(None);

pub fn register_diagnostics(provider: Option<Arc<dyn Diagnostics>>) {
    *DIAGNOSTICS
        .write()
        .unwrap_or_else(|error| error.into_inner()) = provider;
}

pub(crate) fn diagnostics_provider() -> Result<Arc<dyn Diagnostics>> {
    DIAGNOSTICS
        .read()
        .unwrap_or_else(|error| error.into_inner())
        .clone()
        .ok_or(Error::Unregistered("Diagnostics (doctor report)"))
}

fn contract(message: impl Into<String>) -> Error {
    Error::Contract(ContractError::new(message.into()))
}

fn path_of(value: &Value, key: &str) -> PathBuf {
    PathBuf::from(text(value, key).unwrap_or_default())
}

/// Baseline `trusted_receipt`: the running runtime is exactly the recorded
/// source before any repair.
pub fn trusted_receipt(
    env: &Environment,
    services: &Services,
    locations: &Locations,
    receipt: Option<Value>,
) -> Result<Value> {
    let old = match receipt {
        Some(receipt) => receipt,
        None => install::read_receipt(env, &locations.state)?
            .ok_or_else(|| contract("No active installation receipt"))?,
    };
    require(!install::python_era_kind(&old), ADOPTION_GUIDANCE)?;
    let source = get(&old, "source");
    let kind = text(source, "kind").unwrap_or_default().to_owned();
    require(kind != "managed", MANAGED_GUIDANCE)?;
    if kind == "release" {
        require(
            path_of(source, "path") == locations.runtime,
            "Recorded managed source differs from its owned location",
        )?;
    }
    let binary = install::source_binary(source)?;
    let executing = services
        .executable
        .as_deref()
        .map(|path| fs::canonical_path(path, true))
        .transpose()?;
    require(
        executing.is_some_and(|executing| {
            fs::canonical_path(&binary, true).is_ok_and(|binary| binary == executing)
        }),
        "Executing package differs from the installed receipt",
    )?;
    let observed = install::source_info(services, &path_of(source, "path"), &kind)?;
    require(
        install::same(&observed.value, source),
        "Installed source differs from the receipt",
    )?;
    Ok(old)
}

/// Baseline `reconcile_hooks`: use the existing transactional installer;
/// never replace a changed target.
pub fn reconcile_hooks(
    env: &Environment,
    services: &Services,
    locations: &Locations,
    old: &Value,
) -> Result<Value> {
    let source = get(old, "source");
    let mut missing = false;
    for target in get(old, "targets").as_array().unwrap_or(&[]) {
        let path = path_of(target, "path");
        let kind = text(target, "kind").unwrap_or_default();
        if !install::lexists(&path)? && RECREATABLE.contains(&kind) {
            missing = true;
        } else if kind == "activation" {
            require(
                install::is_regular(&path)?,
                "Activation target is missing or linked",
            )?;
            let raw = install::read_optional(&path, install::MAX_GUIDANCE)?.unwrap_or_default();
            install::activation(&raw, text(target, "block_hash"), install::SHIM)?;
        } else {
            install::verify_targets(services, &[target], Some(source), false)?;
        }
    }
    let mut request = crate::updating::configuration(false);
    install::preflight_layout(
        env,
        services,
        Command::Install,
        &request,
        locations,
        Some(old),
    )?;
    let kind = text(source, "kind").unwrap_or_default().to_owned();
    let root = path_of(source, "path");
    let mut plan = if kind == "release" {
        install::prepare(
            env,
            services,
            &request,
            locations,
            Some(old),
            PrepareOptions {
                candidate_root: Some(&root),
                candidate_kind: Some(&kind),
                repair_missing_hooks: missing,
                ..PrepareOptions::default()
            },
        )?
    } else {
        request.development_source = Some(root.clone());
        install::prepare(
            env,
            services,
            &request,
            locations,
            Some(old),
            PrepareOptions {
                repair_missing_hooks: missing,
                ..PrepareOptions::default()
            },
        )?
    };
    // A concurrent edit must remain a refusal in the install transaction.
    install::install(env, services, &mut plan, Default::default())
}

/// Baseline `recover_completed_bank`: finish only a previously receipted
/// outcome using the public bank recovery protocol.
pub fn recover_completed_bank(repo: &Path) -> Result<Option<Value>> {
    let (root, bank) = crate::bank_writes::bank_path(repo)?;
    let marker = bank.join(crate::bank_transactions::MARKER);
    if !install::lexists(&marker)? || install::lexists(&bank.join(".agent-memory.lock"))? {
        return Ok(None);
    }
    let (raw, _) = fs::read_regular(
        &bank,
        Path::new(crate::bank_transactions::MARKER),
        MAX_MARKER,
    )
    .map_err(|error| contract(error.to_string()))?;
    let value = crate::developer_identity::load(&raw)?;
    let transaction = get(&value, "transaction_id");
    if value.as_object().is_none() || !crate::developer_identity::uuid_valid(transaction) {
        return Ok(None);
    }
    let transaction = transaction.as_str().unwrap_or_default();
    let (journal, receipt_path) = crate::bank_transactions::locations(&root, &bank, transaction)?;
    let journal_raw =
        crate::sharing_limits::read_bytes(&journal, crate::sharing_limits::MAX_PRIVATE)?;
    let (_, proposal) =
        crate::bank_transactions::journal_value(&journal_raw, &root, &bank, transaction, true)?;
    let Some(receipt) = crate::bank_transactions::receipt_value(&receipt_path, &proposal)? else {
        return Ok(None);
    };
    let mode = if text(&receipt, "outcome") == Some("finished") {
        "finish"
    } else {
        "rollback"
    };
    let preview = crate::bank_transactions::recover(repo, Some(mode), None)?;
    let recovered = crate::bank_transactions::recover(repo, None, Some(get(&preview, "proposal")))?;
    Ok(Some(recovered))
}

fn components<'a>(report: &'a Value, key: &str) -> &'a [Value] {
    get(report, key).as_array().unwrap_or(&[])
}

fn action(component: &str, state: &str, detail: &str) -> Vec<(String, Value)> {
    vec![
        ("component".to_owned(), string(component)),
        ("state".to_owned(), string(state)),
        ("detail".to_owned(), string(detail)),
    ]
}

fn with_repair(mut fields: Vec<(String, Value)>, repair: &str) -> Value {
    fields.push(("repair".to_owned(), string(repair)));
    Value::Object(fields)
}

fn verified(value: &Value) -> bool {
    get(value, "verified") == &Value::Bool(true)
}

/// The locked runtime repair (baseline `run`'s lifecycle section).
fn repair_runtime(
    env: &Environment,
    services: &Services,
    locations: &Locations,
    pending: bool,
    hooks: bool,
    acquired: &Cell<bool>,
    actions: &mut Vec<Value>,
) -> Result<()> {
    let state = &locations.state;
    let observed = install::read_receipt(env, state)?;
    let mut paths = vec![
        state.clone(),
        locations.code.clone(),
        locations.runtime.clone(),
    ];
    if let Some(observed) = &observed {
        paths.extend(
            get(observed, "targets")
                .as_array()
                .unwrap_or(&[])
                .iter()
                .map(|target| path_of(target, "path")),
        );
    }
    crate::runtime_faults::operation("doctor-fix", &paths, || {
        install::locked(env, state, || {
            acquired.set(true);
            if pending {
                let journal = install::read_journal(env, state)?;
                if trusted_receipt(env, services, locations, None).is_err() {
                    // During activation the source can already be the
                    // journal's candidate while the receipt records the old one.
                    let recorded = journal
                        .as_ref()
                        .map(|journal| get(journal, "receipt").clone())
                        .ok_or_else(|| contract("Missing recovery journal"))?;
                    trusted_receipt(env, services, locations, Some(recorded))?;
                }
                let was_pending = journal.as_ref().is_some_and(|journal| {
                    !matches!(text(journal, "status"), Some("complete" | "rolled-back"))
                });
                let recovered = install::recover(env, services, state, false, true)?;
                require(verified(&recovered), "Lifecycle recovery was not verified")?;
                actions.push(Value::Object(action(
                    "runtime.transaction",
                    if was_pending { "repaired" } else { "unchanged" },
                    "Verified the recorded runtime transaction.",
                )));
            }
            let old = trusted_receipt(env, services, locations, None)?;
            if hooks {
                let outcome = reconcile_hooks(env, services, locations, &old)?;
                require(verified(&outcome), "Hook reconciliation was not verified")?;
                let mut fields = action(
                    "runtime.hooks",
                    if get(&outcome, "changed") == &Value::Bool(true) {
                        "repaired"
                    } else {
                        "unchanged"
                    },
                    "Reconciled receipt-owned client integration.",
                );
                fields.push((
                    "restart_clients".to_owned(),
                    get(&outcome, "restart_clients").clone(),
                ));
                actions.push(Value::Object(fields));
            }
            Ok(())
        })
    })
}

/// Baseline `run`: inspect, repair safely identified owned faults, recheck.
pub fn run(
    env: &Environment,
    services: &Services,
    repo: &Path,
    projection: Option<&Path>,
) -> Result<Value> {
    let diagnostics = diagnostics_provider()?;
    let before = diagnostics.run("doctor", repo, projection)?;
    let mut actions: Vec<Value> = Vec::new();
    let locations = install::locations(env)?;
    let acquired = Cell::new(false);
    let warned = |component: &str| {
        components(&before, "warnings")
            .iter()
            .any(|check| text(check, "component") == Some(component))
    };
    let pending = warned("runtime.transaction");
    let gaps = components(&before, "warnings").iter().any(|check| {
        text(check, "component").is_some_and(|c| c.starts_with("runtime.hook_migration."))
    });
    let hook_failures: Vec<&Value> = components(&before, "checks")
        .iter()
        .filter(|check| {
            TARGET_COMPONENTS.contains(&text(check, "component").unwrap_or_default())
                && text(check, "state") == Some("failure")
        })
        .collect();
    let mut missing = false;
    for check in &hook_failures {
        if let Some(path) = text(check, "path").filter(|path| !path.is_empty()) {
            missing |= !install::lexists(Path::new(path))?;
        }
    }
    let runtime_work = pending || gaps || !hook_failures.is_empty();
    if runtime_work {
        let repaired = repair_runtime(
            env,
            services,
            &locations,
            pending,
            gaps || !hook_failures.is_empty(),
            &acquired,
            &mut actions,
        );
        if let Err(error) = repaired {
            if let Error::Unregistered(name) = error {
                return Err(Error::Unregistered(name));
            }
            actions.push(with_repair(
                action(
                    "runtime.repair",
                    "refused",
                    "Cannot safely verify and reconcile the owned runtime; existing state was preserved.",
                ),
                "Inspect am doctor and the installation receipt; preserve modified targets and use the trusted recover helper when directed.",
            ));
        }
    }
    let identity_attempted = Cell::new(false);
    let identity = (|| -> Result<()> {
        let identity = crate::project_identity::status(repo)?;
        let historical =
            text(get(&identity, "evidence"), "binding_source") == Some("historical-device-change");
        if !pending && get(&identity, "resolved") == &Value::Bool(true) && historical {
            identity_attempted.set(true);
            let preview = crate::scope_binding::preview(
                repo,
                "link",
                text(&identity, "project_id"),
                "Persist verified checkout continuity after filesystem device-number change",
                None,
            )?;
            let result = crate::scope_binding::apply(&preview)?;
            require(
                verified(&result) && get(&result, "resolved") == &Value::Bool(true),
                "Checkout repair readback failed",
            )?;
            actions.push(with_repair(
                action(
                    "identity.checkout_continuity",
                    "repaired",
                    "Persisted the current checkout binding while preserving the project identity and bank.",
                ),
                "Prepare previously pending proposals again; their pinned registry evidence changed. Retry prompts that were not recorded.",
            ));
        }
        Ok(())
    })();
    if identity.is_err() && identity_attempted.get() {
        actions.push(with_repair(
            action(
                "identity.checkout_continuity",
                "refused",
                "Checkout evidence changed or repair could not be verified; preserve current state and inspect identity status.",
            ),
            "Run am identity status; resolve the reported conflict before retrying.",
        ));
    }
    let mut bank_attempted = false;
    if warned("bank.transaction") {
        // No decision is made without an already verified outcome receipt.
        bank_attempted = true;
        let recovered = recover_completed_bank(repo).and_then(|recovered| {
            if let Some(recovered) = &recovered {
                require(verified(recovered), "Bank recovery was not verified")?;
            }
            Ok(recovered)
        });
        match recovered {
            Ok(Some(_)) => actions.push(Value::Object(action(
                "bank.transaction",
                "repaired",
                "Completed cleanup for the previously verified bank transaction outcome.",
            ))),
            Ok(None) => {}
            Err(_) => actions.push(with_repair(
                action(
                    "bank.transaction",
                    "refused",
                    "The recorded bank outcome could not be safely recovered.",
                ),
                "Preserve the bank and inspect am doctor and the original transaction evidence.",
            )),
        }
    }
    let mut after = match diagnostics.run("doctor", repo, projection) {
        Ok(after) => after,
        Err(Error::Unregistered(name)) => return Err(Error::Unregistered(name)),
        Err(_) => diagnostics.report(
            "doctor-fix",
            "diagnostic.post_repair",
            "failure",
            "Post-repair inspection could not be completed.",
            "Preserve the current state and rerun am doctor; inspect the receipt and journal if it still fails.",
        )?,
    };
    // Report every remaining actionable finding explicitly; neither a live
    // bank transaction's direction nor arbitrary projections are guessed.
    for check in components(&after, "checks") {
        if !matches!(text(check, "state"), Some("warning" | "failure")) {
            continue;
        }
        let repair = text(check, "repair")
            .unwrap_or("Inspect this finding with am doctor and resolve its reported cause.");
        actions.push(with_repair(
            vec![
                ("component".to_owned(), get(check, "component").clone()),
                ("state".to_owned(), string("skipped")),
                (
                    "detail".to_owned(),
                    string("This finding remains; automatic repair cannot safely determine the required action."),
                ),
            ],
            repair,
        ));
    }
    let repaired = |component: Option<&str>| {
        actions.iter().any(|item| {
            text(item, "state") == Some("repaired")
                && component.is_none_or(|component| text(item, "component") == Some(component))
        })
    };
    let writes = if repaired(Some("identity.checkout_continuity"))
        || acquired.get()
        || repaired(Some("bank.transaction"))
    {
        Value::Bool(true)
    } else if runtime_work || bank_attempted || identity_attempted.get() {
        Value::Null
    } else {
        Value::Bool(false)
    };
    let restart: BTreeSet<String> = actions
        .iter()
        .flat_map(|item| {
            get(item, "restart_clients")
                .as_array()
                .unwrap_or(&[])
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect::<Vec<_>>()
        })
        .collect();
    let hooks_repaired = repaired(Some("runtime.hooks"));
    let refused = actions
        .iter()
        .any(|item| text(item, "state") == Some("refused"));
    let repairs_performed = repaired(None);
    after = merged(
        after,
        object([
            ("operation", string("doctor-fix")),
            ("repairs", Value::Array(actions)),
            ("repairs_performed", Value::Bool(repairs_performed)),
            ("writes_performed", writes),
            (
                "restart_clients",
                Value::Array(restart.into_iter().map(Value::String).collect()),
            ),
        ]),
    );
    if missing && hooks_repaired {
        let warning = object([
            ("component", string("runtime.hook_recreated")),
            ("state", string("warning")),
            (
                "detail",
                string(
                    "Missing owned integration targets were recreated from the ownership receipt; other content previously removed from missing files was not restored.",
                ),
            ),
            (
                "repair",
                string(
                    "Review the affected client configuration and restore any separately maintained settings if needed.",
                ),
            ),
        ]);
        for key in ["checks", "warnings"] {
            let mut items = components(&after, key).to_vec();
            items.push(warning.clone());
            after = merged(after, object([(key, Value::Array(items))]));
        }
    }
    if refused {
        after = merged(after, object([("ok", Value::Bool(false))]));
    }
    Ok(after)
}
