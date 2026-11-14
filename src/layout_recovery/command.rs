use super::*;
use crate::layout_resolution::read_optional;
use crate::lifecycle::CommandResult;

/// Parsed public command; private documents are read only after output preflight.
pub struct Command<'a> {
    pub operation: &'a str,
    pub stage: Option<&'a str>,
    pub repo: &'a Path,
    pub operation_id: Option<&'a str>,
    pub input: Option<&'a Path>,
    pub proposal: Option<&'a Path>,
    pub output: Option<&'a Path>,
    pub page: Option<i128>,
    pub identity_review: Option<&'a Path>,
    pub mode: Option<&'a str>,
    pub nested: Option<&'a str>,
    pub unit: Option<&'a str>,
}
pub fn safe_envelope(value: &Value, exported: Option<&Value>) -> Value {
    const SAFE: &[&str] = &[
        "state",
        "verified",
        "canonical_eligibility",
        "semantic_recovery_complete",
        "pending_unit_ids",
        "pending_units",
        "required_actions",
        "writes_performed",
        "private_artifact_writes_performed",
        "temporary_lock_writes_performed",
        "canonical_writes_performed",
        "original_index_writes_performed",
        "head_writes_performed",
        "feature_writes_performed",
        "history_writes_performed",
        "truth_verified",
        "code_freshness_verified",
        "publication_verified",
        "other_machine_import_verified",
        "observation_complete",
        "source_files",
        "source_bytes",
        "page_count",
        "receipt_is_historical",
        "already_applied",
        "current_receipt_match",
        "parent_operation_id",
        "operations",
        "canonical_mutation_may_have_occurred",
        "failure_code",
        "capacity",
        "original_evidence_available",
        "probe_path",
        "private_artifact_deletions_performed",
        "cleanup_id",
        "original_bytes_available",
        "rollback_available",
        "git_metadata_writes_performed",
        "remote_freshness_verified",
        "source_assertions_verified",
        "feature_code_freshness_verified",
        "cleanup_complete",
        "intended_canonical_eligibility",
    ];
    let mut answer = result(
        get(value, "operation").as_str().unwrap_or("inspect"),
        get(value, "operation_id").as_str(),
        [],
    );
    for name in SAFE {
        if let Some(value) = value.get(name) {
            crate::layout_resolution::set(&mut answer, name, value.clone());
        }
    }
    if let Some(exported) = exported {
        crate::layout_resolution::set(
            &mut answer,
            "private_output",
            object([
                ("path", get(exported, "path").clone()),
                ("bytes", get(exported, "bytes").clone()),
            ]),
        );
        crate::layout_resolution::set(
            &mut answer,
            "private_artifact_writes_performed",
            Value::Bool(true),
        );
    }
    if let Some(receipt) = value.get("receipt").filter(|r| r.as_object().is_some()) {
        let fields = [
            "operation_id",
            "proposal_id",
            "outcome",
            "verified",
            "canonical_eligibility_at_commit",
            "semantic_recovery_complete_at_commit",
            "parent_operation_id",
        ]
        .into_iter()
        .filter_map(|k| receipt.get(k).map(|v| (k, v.clone())));
        crate::layout_resolution::set(&mut answer, "receipt", object(fields));
    }
    if let Some(recall) = value.get("recall").filter(|r| r.as_object().is_some()) {
        crate::layout_resolution::set(
            &mut answer,
            "recall",
            object([("state", get(recall, "state").clone())]),
        );
    }
    answer
}
fn run(
    c: &Command<'_>,
    request: Option<&Value>,
    proposal: Option<&Value>,
    runtime_root: &Path,
    failure_id: &mut Option<String>,
) -> Result<Value> {
    let output = c.output.is_some();
    let id = || {
        c.operation_id
            .ok_or_else(|| err("Recovery operations require canonical UUIDv4 identifiers"))
    };
    match c.operation {
        "inspect" => {
            require(
                output,
                "Private output is required for recovery source inspection",
            )?;
            if let Some(number) = c.page {
                require(
                    c.operation_id.is_some(),
                    "Page inspection needs its exact operation",
                )?;
                page(c.repo, id()?, number, runtime_root)
            } else {
                require(
                    c.operation_id.is_none(),
                    "Existing observations use an explicit page, status or known recovery",
                )?;
                let review = c.identity_review.map(read_value).transpose()?;
                let operation_id = host::uuid4()?;
                *failure_id = Some(operation_id.clone());
                super::inspect_with_id(c.repo, review.as_ref(), runtime_root, &operation_id)
            }
        }
        "prepare" => {
            require(
                request.is_some() && output && c.operation_id.is_some(),
                "Prepare requires complete private input/output and inspected operation",
            )?;
            prepare(
                c.repo,
                id()?,
                request.ok_or_else(|| err(crate::layout_resolution::MALFORMED))?,
                runtime_root,
            )
        }
        "apply" => {
            let proposed =
                proposal.ok_or_else(|| err("Apply requires the exact private proposal"))?;
            require(
                c.operation_id.is_none_or(|id| {
                    get(proposed.get("proposal").unwrap_or(proposed), "operation_id").as_str()
                        == Some(id)
                }),
                "Apply operation differs from the exact proposal",
            )?;
            apply(c.repo, proposed, runtime_root)
        }
        "status" => status(c.repo, c.operation_id, runtime_root),
        "recover" => {
            require(
                proposal.is_some() || output,
                "Known recovery preview requires a private output",
            )?;
            recover(c.repo, c.operation_id, c.mode, proposal, runtime_root)
        }
        "rollback" => {
            require(
                proposal.is_some() || output,
                "Rollback preview requires a private output",
            )?;
            rollback(c.repo, c.operation_id, proposal, runtime_root)
        }
        "resolve" => {
            require(
                proposal.is_some()
                    || (request.is_some()
                        && output
                        && c.unit.is_some()
                        && c.operation_id.is_some()),
                "Resolution preview requires actual terminal evidence and exact unit/operation",
            )?;
            let value = crate::layout_resolution::resolve(
                c.repo,
                c.operation_id,
                c.unit,
                request,
                proposal,
                runtime_root,
            )?;
            let resolved_id = proposal
                .map(|p| get(p.get("proposal").unwrap_or(p), "operation_id").as_str())
                .unwrap_or(c.operation_id);
            let historical = get(&value, "receipt_is_historical").as_bool() == Some(true);
            let mut answer = result("resolve", resolved_id, []);
            for (k, v) in pairs(&value)? {
                crate::layout_resolution::set(&mut answer, k, v.clone());
            }
            Ok(with(
                answer,
                [
                    (
                        "state",
                        string(if proposal.is_none() {
                            "prepared"
                        } else if historical {
                            "historical"
                        } else {
                            "resolved"
                        }),
                    ),
                    ("verified", Value::Bool(true)),
                    (
                        "private_artifact_writes_performed",
                        get(&value, "writes_performed").clone(),
                    ),
                    ("temporary_lock_writes_performed", Value::Bool(!historical)),
                ],
            ))
        }
        "revise-units" => {
            require(
                proposal.is_some() || (request.is_some() && output && c.operation_id.is_some()),
                "Unit refinement requires independently reviewed private input/output",
            )?;
            match proposal {
                Some(proposal) => crate::layout_units::apply(c.repo, proposal, runtime_root),
                None => crate::layout_units::preview(
                    c.repo,
                    id()?,
                    request.ok_or_else(|| err(crate::layout_resolution::MALFORMED))?,
                    runtime_root,
                ),
            }
        }
        "recover-nested" => {
            require(
                proposal.is_some()
                    || (output
                        && c.operation_id.is_some()
                        && c.nested.is_some()
                        && c.mode.is_some()),
                "Nested recovery preview requires the exact operation/nested UUID and mode",
            )?;
            crate::recovery_transactions::recover_nested(
                c.repo,
                c.operation_id,
                c.nested,
                c.mode,
                proposal,
                runtime_root,
            )
        }
        "retention" => {
            if c.stage == Some("inspect") {
                require(
                    output && c.operation_id.is_some(),
                    "Retention inspection requires operation-scoped private output",
                )?;
                crate::layout_recovery_retention::inspect(c.repo, id()?, request, runtime_root)
            } else {
                crate::layout_recovery_retention::apply(
                    c.repo,
                    proposal.ok_or_else(|| {
                        err("Retention apply requires the exact independently reviewed proposal")
                    })?,
                    runtime_root,
                )
            }
        }
        "publish" => {
            require(
                output && c.operation_id.is_some(),
                "Recovery publication requires operation-scoped private output",
            )?;
            match c.stage {
                Some("inspect") => crate::layout_recovery_publication::inspect(
                    c.repo,
                    id()?,
                    request,
                    runtime_root,
                ),
                Some("prepare") => crate::layout_recovery_publication::prepare(
                    c.repo,
                    id()?,
                    request.ok_or_else(|| {
                        err("Publication preparation requires complete independent review")
                    })?,
                    runtime_root,
                ),
                Some("land") => crate::layout_recovery_publication::land(
                    c.repo,
                    id()?,
                    request.ok_or_else(|| {
                        err("Publication landing requires its exact reviewed request")
                    })?,
                    runtime_root,
                ),
                _ => Err(err("Unknown recovery publication stage")),
            }
        }
        _ => Err(err("Unknown layout recovery command")),
    }
}
pub fn command(c: Command<'_>, runtime_root: &Path) -> CommandResult {
    let mut failure_id = c.operation_id.map(str::to_owned);
    let mut started = false;
    let mut retained = None;
    let outcome = (|| -> Result<(Value, i32)> {
        let output = c
            .output
            .map(|p| sharing_limits::preflight_private_output(p, c.repo, runtime_root))
            .transpose()?;
        let request = c.input.map(read_value).transpose()?;
        let proposal = c.proposal.map(read_value).transpose()?;
        if let Some(p) = &proposal {
            failure_id = get(p.get("proposal").unwrap_or(p), "operation_id")
                .as_str()
                .map(str::to_owned)
                .or(failure_id.take());
        }
        started = true;
        let value = run(
            &c,
            request.as_ref(),
            proposal.as_ref(),
            runtime_root,
            &mut failure_id,
        )?;
        retained = Some(value.clone());
        let exported = output
            .as_ref()
            .map(|p| sharing_limits::write_private(p, &value, c.repo, runtime_root))
            .transpose()?;
        let safe = if get(&value, "schema").as_str() == Some("agent-memory/layout-source-page-v1") {
            let exported = exported
                .as_ref()
                .ok_or_else(|| err("Private output is required for recovery source inspection"))?;
            with(
                result(
                    "inspect",
                    get(&value, "operation_id").as_str(),
                    [
                        ("state", string("source-page-exported")),
                        ("verified", Value::Bool(true)),
                    ],
                ),
                [
                    ("private_output", select(exported, &["path", "bytes"])?),
                    ("private_artifact_writes_performed", Value::Bool(true)),
                ],
            )
        } else {
            safe_envelope(&value, exported.as_ref())
        };
        let code = if get(&value, "verified").as_bool() == Some(false) {
            1
        } else {
            0
        };
        Ok((safe, code))
    })();
    let (safe, code) = match outcome {
        Ok(value) => value,
        Err(error) => {
            if let Some(value) = retained {
                (
                    with(
                        safe_envelope(&value, None),
                        [
                            ("private_output_state", string("pending-export")),
                            ("operation_outcome_retained", Value::Bool(true)),
                        ],
                    ),
                    1,
                )
            } else {
                let changed = failure_id.as_deref().is_some_and(|id| {
                    !["publish", "retention"].contains(&c.operation)
                        && (|| -> Result<bool> {
                            let (_, _, _, d) = operation_paths(c.repo, id, runtime_root)?;
                            Ok(read_optional(&d.join("journal.json"))?.is_some())
                        })()
                        .unwrap_or(true)
                });
                let ambiguous = matches!(error.kind(), crate::error::Kind::ProbeOwnership { .. });
                let raw_capacity = raw::capacity_details(&error);
                let code = if raw_capacity.is_some() {
                    "pending-recovery-capacity"
                } else if [
                    "pending-identity-review",
                    "pending-runtime-compatibility",
                    "pending-metadata-preservation",
                    "pending-ambiguous-probe-ownership",
                ]
                .contains(&error.message())
                {
                    error.message()
                } else {
                    "pending-evidence-or-state"
                };
                let unknown = || string("unknown");
                let mut failure = result(
                    c.operation,
                    failure_id.as_deref(),
                    [
                        (
                            "state",
                            string(if ambiguous {
                                "pending"
                            } else if changed {
                                "interrupted"
                            } else {
                                "pending"
                            }),
                        ),
                        ("failure_code", string(code)),
                        (
                            "writes_performed",
                            if changed || ambiguous {
                                unknown()
                            } else {
                                Value::Bool(false)
                            },
                        ),
                        (
                            "temporary_lock_writes_performed",
                            if started {
                                unknown()
                            } else {
                                Value::Bool(false)
                            },
                        ),
                        (
                            "canonical_writes_performed",
                            if changed || ambiguous {
                                unknown()
                            } else {
                                Value::Bool(false)
                            },
                        ),
                        (
                            "canonical_mutation_may_have_occurred",
                            Value::Bool(changed || ambiguous),
                        ),
                        (
                            "private_artifact_writes_performed",
                            if started && c.operation != "status" {
                                unknown()
                            } else {
                                Value::Bool(false)
                            },
                        ),
                        (
                            "required_actions",
                            Value::Array(vec![string(if ambiguous {
                                "Operator: inspect the named probe and its private journal; only after independently establishing ownership, manually remove the ambiguous entry, then repeat known-operation recovery."
                            } else {
                                "Preserve original sources and private evidence; inspect status and use only the exact known-operation review/recovery path"
                            })]),
                        ),
                    ],
                );
                if let crate::error::Kind::ProbeOwnership { path } = error.kind() {
                    crate::layout_resolution::set(&mut failure, "probe_path", string(path));
                }
                if c.operation == "publish" && started {
                    crate::layout_resolution::set(
                        &mut failure,
                        "git_metadata_writes_performed",
                        unknown(),
                    );
                }
                if c.operation == "retention" && started {
                    crate::layout_resolution::set(
                        &mut failure,
                        "private_artifact_deletions_performed",
                        if c.stage == Some("apply") {
                            unknown()
                        } else {
                            Value::Bool(false)
                        },
                    );
                }
                if let Some(capacity) = raw_capacity {
                    crate::layout_resolution::set(&mut failure, "capacity", capacity);
                }
                (safe_envelope(&failure, None), 1)
            }
        }
    };
    match crate::codec::json::encode(&safe, crate::codec::json::INDENT2_UTF8) {
        Ok(body) => CommandResult {
            value: Some(safe),
            exit_code: code,
            stdout: format!("{body}\n"),
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
