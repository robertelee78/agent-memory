use super::*;
use crate::layout_resolution::{has_keys, read_optional};

pub fn fields(value: &Value, names: &[&str], schema: Option<&str>) -> Result<()> {
    require(
        has_keys(value, names) && schema.is_none_or(|s| get(value, "schema").as_str() == Some(s)),
        "Unknown or incomplete recovery artifact schema",
    )
}
pub fn participant(value: &Value) -> Result<()> {
    fields(
        value,
        &["developer_id", "harness", "session", "evidence"],
        None,
    )?;
    require(
        uuid(get(value, "developer_id")),
        "Recovery review requires real developer attribution",
    )?;
    for name in ["harness", "session", "evidence"] {
        require(
            get(value, name)
                .as_str()
                .is_some_and(|s| !bank::trim(s).is_empty() && s.len() <= 4096),
            "Recovery review requires retained actual harness/session evidence",
        )?;
    }
    Ok(())
}
pub fn independent(author: &Value, reviewer: &Value) -> Result<()> {
    participant(author)?;
    participant(reviewer)?;
    require(
        !(same(get(author, "harness"), get(reviewer, "harness"))
            && same(get(author, "session"), get(reviewer, "session"))),
        "Recovery requires a distinct independently reviewing agent/session",
    )
}
pub fn unit_map_value(
    value: &Value,
    manifest: &Value,
    intended: &Value,
    retained: Option<&Value>,
) -> Result<()> {
    fields(
        value,
        &["schema", "operation_id", "manifest_sha256", "units"],
        Some("agent-memory/layout-unit-map-v1"),
    )?;
    require(
        same(get(value, "operation_id"), get(manifest, "operation_id"))
            && get(value, "manifest_sha256").as_str() == Some(sha(manifest)?.as_str())
            && get(value, "units").as_array().is_some(),
        "Unit map belongs to another raw source observation",
    )?;
    let mut targets = BTreeMap::new();
    for (path, body) in pairs(intended)? {
        targets.insert(
            path.as_str(),
            digest(
                body.as_str()
                    .ok_or_else(|| err("Source mapping requires exact resulting targets"))?
                    .as_bytes(),
            ),
        );
    }
    if let Some(retained) = retained {
        for (path, hash) in pairs(retained)? {
            targets.insert(
                path.as_str(),
                hash.as_str()
                    .ok_or_else(|| err("Source mapping requires exact resulting targets"))?
                    .to_owned(),
            );
        }
    }
    let sources = items(field(manifest, "sources")?)?;
    let mut coverage: BTreeMap<&str, Vec<(usize, usize)>> = BTreeMap::new();
    let mut pages = BTreeMap::new();
    let mut cursor = 0;
    for source in sources {
        let id = text(source, "source_id")?;
        coverage.insert(id, vec![]);
        let count = super::page_count(size(get(source, "byte_count"))?);
        pages.insert(id, (cursor, cursor + count));
        cursor += count;
    }
    let mut seen = BTreeSet::new();
    for unit in items(field(value, "units")?)? {
        fields(
            unit,
            &[
                "unit_id",
                "source_ranges",
                "semantic_kind",
                "existing_record",
                "provenance",
                "disposition",
                "targets",
                "reason",
                "required_actions",
                "context_pages",
            ],
            None,
        )?;
        let disposition = text(unit, "disposition")?;
        require(
            uuid(get(unit, "unit_id"))
                && seen.insert(text(unit, "unit_id")?)
                && [
                    "preserve",
                    "pending-admission",
                    "governed-terminal",
                    "non-guidance-discard",
                ]
                .contains(&disposition),
            "Every semantic unit needs one stable identity and explicit disposition",
        )?;
        require(
            get(unit, "source_ranges")
                .as_array()
                .is_some_and(|v| !v.is_empty())
                && get(unit, "reason")
                    .as_str()
                    .is_some_and(|s| !bank::trim(s).is_empty())
                && get(unit, "required_actions").as_array().is_some()
                && get(unit, "provenance").as_object().is_some()
                && get(unit, "context_pages")
                    .as_array()
                    .is_some_and(|v| !v.is_empty()),
            "Unit disposition needs complete context, reason and real/uncertain provenance",
        )?;
        let kind = text(unit, "semantic_kind")?;
        require(
            [
                "canonical-guidance",
                "canonical-framing",
                "unadmitted-source",
                "non-guidance",
                "terminal-history",
            ]
            .contains(&kind),
            "Unknown semantic source kind",
        )?;
        let mut required_pages = BTreeSet::new();
        for span in items(field(unit, "source_ranges")?)? {
            fields(span, &["source_id", "start", "end"], None)?;
            let id = text(span, "source_id")?;
            let source = sources
                .iter()
                .find(|s| get(s, "source_id").as_str() == Some(id))
                .ok_or_else(|| err("Unit map range does not name exact complete original bytes"))?;
            let start = size(get(span, "start"))
                .map_err(|_| err("Unit map range does not name exact complete original bytes"))?;
            let end = size(get(span, "end"))
                .map_err(|_| err("Unit map range does not name exact complete original bytes"))?;
            let count = size(get(source, "byte_count"))?;
            require(
                start <= end && end <= count && (start != end || count == 0),
                "Unit map range does not name exact complete original bytes",
            )?;
            coverage
                .get_mut(id)
                .ok_or_else(|| err("Unit map range does not name exact complete original bytes"))?
                .push((start, end));
            let (first, last) = pages.get(id).copied().ok_or_else(|| {
                err("Unit review must bind the complete original source context pages")
            })?;
            required_pages.extend(first..last);
            if disposition == "pending-admission" {
                let path = text(source, "path")?;
                require(
                    path != "MEMORY.md"
                        && !bank::CONTEXTS.contains(&path)
                        && kind == "unadmitted-source"
                        && get(unit, "existing_record") == &Value::Null,
                    "Already available canonical guidance cannot be demoted into private pending material",
                )?;
            }
        }
        let expected_pages = Value::Array(required_pages.into_iter().map(number).collect());
        require(
            items(field(unit, "context_pages")?)?
                .iter()
                .all(|v| int(v).is_some())
                && same(get(unit, "context_pages"), &expected_pages),
            "Unit review must bind the complete original source context pages",
        )?;
        let mapped = items(field(unit, "targets")?)
            .map_err(|_| err("Source mapping requires exact resulting targets"))?;
        for target in mapped {
            fields(target, &["path", "sha256"], None)?;
            require(
                targets
                    .get(text(target, "path")?)
                    .is_some_and(|hash| get(target, "sha256").as_str() == Some(hash)),
                "Unit mapping does not bind the exact intended result",
            )?;
        }
        require(
            !["preserve", "governed-terminal"].contains(&disposition) || !mapped.is_empty(),
            "Preserved knowledge requires an available native result",
        )?;
        require(
            disposition != "pending-admission"
                || (mapped.is_empty() && !items(field(unit, "required_actions")?)?.is_empty()),
            "Pending evidence is neither guidance nor automatically admitted",
        )?;
        require(
            !["non-guidance-discard", "governed-terminal"].contains(&disposition)
                || ["non-guidance", "terminal-history"].contains(&kind),
            "Guidance lifecycle changes require existing governed predicates; preserve/re-layout first",
        )?;
        require(
            disposition != "governed-terminal"
                || (kind == "terminal-history" && get(unit, "existing_record") != &Value::Null),
            "Governed terminal handling must preserve an already real typed terminal outcome",
        )?;
    }
    for source in sources {
        let spans = coverage
            .get_mut(text(source, "source_id")?)
            .ok_or_else(|| err("A retained source was omitted from primary semantic coverage"))?;
        spans.sort();
        require(
            !spans.is_empty(),
            "A retained source was omitted from primary semantic coverage",
        )?;
        let mut position = 0;
        for (start, end) in spans {
            require(
                *start == position,
                "Primary unit coverage has a gap or overlapping double-count",
            )?;
            position = *end;
        }
        require(
            position == size(get(source, "byte_count"))?,
            "Primary unit coverage omitted original source bytes",
        )?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
pub fn review_value(
    review: &Value,
    manifest: &Value,
    unit_map: &Value,
    intended: &Value,
    author: &Value,
    metadata: &Value,
    attestation: &Value,
    inherited: &[Value],
) -> Result<()> {
    fields(
        review,
        &[
            "schema",
            "review_id",
            "operation_id",
            "manifest_sha256",
            "unit_map_sha256",
            "intended_files_sha256",
            "metadata_plan_sha256",
            "authority_sha256",
            "author",
            "reviewer",
            "reviewed_at",
            "unit_decisions",
            "inherited_frontier_sha256",
            "inherited_unit_decisions",
            "quiescence_attestation_sha256",
            "semantic_preservation",
            "security",
            "identity",
            "decision",
            "evidence",
        ],
        Some("agent-memory/layout-review-v1"),
    )?;
    independent(author, field(review, "reviewer")?)?;
    crate::lifecycle::time_value(field(review, "reviewed_at")?)?;
    let attestation_sha = if *attestation == Value::Null {
        Value::Null
    } else {
        string(sha(attestation)?)
    };
    require(
        uuid(get(review, "review_id"))
            && same(get(review, "operation_id"), get(manifest, "operation_id"))
            && get(review, "manifest_sha256").as_str() == Some(sha(manifest)?.as_str())
            && get(review, "unit_map_sha256").as_str() == Some(sha(unit_map)?.as_str())
            && get(review, "intended_files_sha256").as_str() == Some(sha(intended)?.as_str())
            && get(review, "metadata_plan_sha256").as_str() == Some(sha(metadata)?.as_str())
            && get(review, "authority_sha256").as_str()
                == Some(sha(field(field(manifest, "identity")?, "project_evidence")?)?.as_str())
            && same(
                get(review, "quiescence_attestation_sha256"),
                &attestation_sha,
            )
            && (*attestation == Value::Null
                || (attestation.as_object().is_some()
                    && same(get(attestation, "review_id"), get(review, "review_id"))))
            && same(get(review, "author"), author)
            && get(review, "decision").as_str() == Some("approve")
            && ["semantic_preservation", "security", "identity"]
                .iter()
                .all(|k| get(review, k).as_bool() == Some(true))
            && get(review, "evidence")
                .as_str()
                .is_some_and(|s| !bank::trim(s).is_empty()),
        "Independent full-source/result review is missing, stale or not approved",
    )?;
    let units = items(field(unit_map, "units")?)?;
    let decisions = items(field(review, "unit_decisions")?)
        .map_err(|_| err("Independent review must decide every source unit"))?;
    require(
        decisions.len() == units.len(),
        "Independent review must decide every source unit",
    )?;
    let mut seen = BTreeSet::new();
    for decision in decisions {
        let terminal = ["non-guidance-discard", "governed-terminal"]
            .contains(&get(decision, "disposition").as_str().unwrap_or(""));
        let mut names = vec!["unit_id", "disposition", "rationale"];
        if terminal {
            names.push("authority");
        }
        fields(decision, &names, None)?;
        if terminal {
            let authority = field(decision, "authority")?;
            fields(
                authority,
                &[
                    "developer_id",
                    "harness",
                    "session",
                    "evidence",
                    "statement",
                ],
                None,
            )?;
            participant(&select(
                authority,
                &["developer_id", "harness", "session", "evidence"],
            )?)?;
            require(
                get(authority, "statement").as_str()
                    == Some(
                        "Explicitly authorize this source-bound non-guidance archive/removal; byte retention alone is not the reason.",
                    ),
                "Non-guidance disposition requires actual explicit source-bound authority",
            )?;
        }
        require(
            seen.insert(text(decision, "unit_id")?)
                && units.iter().any(|u| {
                    same(get(u, "unit_id"), get(decision, "unit_id"))
                        && same(get(u, "disposition"), get(decision, "disposition"))
                })
                && get(decision, "rationale")
                    .as_str()
                    .is_some_and(|s| !bank::trim(s).is_empty()),
            "Independent review omitted/substituted a source disposition",
        )?;
    }
    require(
        get(review, "inherited_frontier_sha256").as_str()
            == Some(sha(&Value::Array(inherited.to_vec()))?.as_str()),
        "Independent review must bind the complete inherited private source frontier",
    )?;
    let decisions = items(field(review, "inherited_unit_decisions")?)
        .map_err(|_| err("Independent review omitted inherited pending obligations"))?;
    require(
        decisions.len() == inherited.len(),
        "Independent review omitted inherited pending obligations",
    )?;
    let mut seen = BTreeSet::new();
    for decision in decisions {
        fields(decision, &["unit_id", "disposition", "rationale"], None)?;
        require(
            seen.insert(text(decision, "unit_id")?)
                && inherited
                    .iter()
                    .any(|u| same(get(u, "unit_id"), get(decision, "unit_id")))
                && get(decision, "disposition").as_str() == Some("retain-pending")
                && get(decision, "rationale")
                    .as_str()
                    .is_some_and(|s| !bank::trim(s).is_empty()),
            "Raw repair cannot discharge inherited pending guidance",
        )?;
    }
    Ok(())
}
pub fn preserve_records(
    directory: &Path,
    manifest: &Value,
    intended: &Value,
    unit_map: &Value,
) -> Result<()> {
    let mut originals: BTreeMap<(String, Option<i128>, String), Value> = BTreeMap::new();
    for source in items(field(manifest, "sources")?)? {
        let body = source_bytes(directory, source)?;
        let Ok(body) = std::str::from_utf8(&body) else {
            continue;
        };
        let Ok((kind, metadata, _)) = crate::records::parse(body, &[]) else {
            continue;
        };
        if !["standalone-record", "optional-history-event", "tombstone"].contains(&kind.as_str()) {
            continue;
        }
        let key = (
            text(&metadata, "id")?.to_owned(),
            int(get(&metadata, "version")),
            kind.clone(),
        );
        require(
            originals.insert(key, metadata.clone()).is_none(),
            "Ambiguous original record identity requires explicit review",
        )?;
        for unit in items(field(unit_map, "units")?)? {
            if items(field(unit, "source_ranges")?)?
                .iter()
                .any(|r| same(get(r, "source_id"), get(source, "source_id")))
            {
                let terminal = kind != "standalone-record"
                    || ["superseded", "revoked", "expired"]
                        .contains(&get(&metadata, "status").as_str().unwrap_or(""));
                let disposition = get(unit, "disposition").as_str();
                let allowed = disposition == Some("preserve")
                    || (terminal && disposition == Some("governed-terminal"));
                let reference = object([
                    ("id", get(&metadata, "id").clone()),
                    ("version", get(&metadata, "version").clone()),
                ]);
                require(
                    allowed && same(get(unit, "existing_record"), &reference),
                    "Existing typed guidance/history must retain its actual identity and stay canonical",
                )?;
            }
        }
    }
    let mut resulting = BTreeMap::new();
    for (_, body) in pairs(intended)? {
        let Ok((kind, metadata, _)) = crate::records::parse(
            body.as_str().ok_or_else(|| {
                err("Recovery cannot invent record metadata/admission or change stable provenance")
            })?,
            &[],
        ) else {
            continue;
        };
        if !["standalone-record", "optional-history-event", "tombstone"].contains(&kind.as_str()) {
            continue;
        }
        let key = (
            text(&metadata, "id")?.to_owned(),
            int(get(&metadata, "version")),
            kind,
        );
        require(
            originals.get(&key).is_some_and(|m| same(m, &metadata)),
            "Recovery cannot invent record metadata/admission or change stable provenance",
        )?;
        resulting.insert(key, metadata);
    }
    for unit in items(field(unit_map, "units")?)? {
        if get(unit, "disposition").as_str() == Some("governed-terminal") {
            let reference = get(unit, "existing_record");
            require(
                originals.keys().any(|(id, version, _)| {
                    get(reference, "id").as_str() == Some(id)
                        && int(get(reference, "version")) == *version
                }),
                "Recovery cannot manufacture terminal predicates for unadmitted source material",
            )?;
        }
    }
    require(
        originals.len() == resulting.len()
            && originals
                .iter()
                .all(|(k, v)| resulting.get(k).is_some_and(|r| same(v, r))),
        "Recovery result lost existing record identities/versions or provenance",
    )
}
pub(crate) fn required_facts(row: &Value) -> Result<Value> {
    Ok(with(
        select(row, &["uid", "gid", "mode", "xattrs"])?,
        [(
            "mode",
            number(
                int(field(row, "mode")?).ok_or_else(|| err("pending-metadata-preservation"))?
                    & 0o7777,
            ),
        )],
    ))
}
pub fn metadata_plan(
    value: &Value,
    manifest: &Value,
    intended: &Value,
    retained_ids: &[Value],
) -> Result<()> {
    fields(value, &["files", "directories"], None)?;
    require(
        get(value, "files").as_object().is_some()
            && crate::layout_resolution::keys(get(value, "files"))
                == crate::layout_resolution::keys(intended)
            && get(value, "directories").as_object().is_some(),
        "Required metadata plan must cover the full intended map",
    )?;
    let physical = items(field(manifest, "physical_entries")?)?;
    let mut groups = host::groups()?;
    groups.push(host::effective_gid());
    let owner = |facts: &Value| {
        int(get(facts, "uid")) == Some(i128::from(host::effective_uid()))
            && groups
                .iter()
                .any(|gid| int(get(facts, "gid")) == Some(i128::from(*gid)))
    };
    for (path, facts) in pairs(field(value, "files")?)? {
        fields(facts, &["uid", "gid", "mode", "xattrs"], None)?;
        require(
            ["uid", "gid", "mode"]
                .iter()
                .all(|k| int(get(facts, k)).is_some())
                && int(get(facts, "mode")).is_some_and(|m| (0..=0o777).contains(&m))
                && raw::valid_xattrs(get(facts, "xattrs")),
            "Recovery metadata requires ordinary exact ownership/mode and supported xattrs",
        )?;
        let old = row_at(physical, path);
        if let Some(old) = old {
            require(
                get(old, "type").as_str() == Some("file") && same(facts, &required_facts(old)?),
                "Recovery must preserve original required file metadata",
            )?;
        }
        let hash = digest(text(intended, path)?.as_bytes());
        if old.is_none_or(|o| get(o, "sha256").as_str() != Some(hash.as_str())) {
            require(owner(facts), "pending-metadata-preservation")?;
        }
    }
    let mut needed = BTreeSet::new();
    for (path, _) in pairs(intended)? {
        for parent in Path::new(path).ancestors().skip(1) {
            if parent.as_os_str().is_empty() {
                continue;
            }
            let parent = path_text(parent)?;
            if row_at(physical, parent).is_none() {
                needed.insert(parent);
            }
        }
    }
    require(
        crate::layout_resolution::keys(get(value, "directories")) == needed,
        "Recovery must pin every newly created directory",
    )?;
    for (path, facts) in pairs(field(value, "directories")?)? {
        raw::path_value(path)?;
        fields(facts, &["uid", "gid", "mode", "xattrs"], None)?;
        require(
            owner(facts)
                && int(get(facts, "mode")).is_some_and(|m| (0..=0o777).contains(&m))
                && raw::valid_xattrs(get(facts, "xattrs")),
            "pending-metadata-preservation",
        )?;
    }
    for row in physical {
        let path = text(row, "path")?;
        if get(row, "type").as_str() == Some("file")
            && get(row, "runtime").as_bool() != Some(true)
            && intended.get(path).is_none()
            && !items(field(manifest, "sources")?)?.iter().any(|s| {
                get(s, "path").as_str() == Some(path)
                    && retained_ids.iter().any(|id| same(id, get(s, "source_id")))
            })
        {
            require(
                owner(row) && int(get(row, "mode")).is_some_and(|m| m & 0o7000 == 0),
                "pending-metadata-preservation",
            )?;
        }
    }
    Ok(())
}
pub fn pending_rows(directory: &Path, manifest: &Value, units: &[Value]) -> Result<Vec<Value>> {
    let sources = items(field(manifest, "sources")?)?;
    let mut rows = Vec::new();
    for unit in units {
        let mut body = Vec::new();
        for span in items(field(unit, "source_ranges")?)? {
            let source = sources
                .iter()
                .find(|s| same(get(s, "source_id"), get(span, "source_id")))
                .ok_or_else(|| err("Unit map range does not name exact complete original bytes"))?;
            let raw = source_bytes(directory, source)?;
            let part = raw
                .get(size(get(span, "start"))?..size(get(span, "end"))?)
                .ok_or_else(|| err("Unit map range does not name exact complete original bytes"))?;
            require(
                part.len() <= sharing_limits::MAX_FILE.saturating_sub(body.len()),
                "Indivisible pending unit exceeds ordinary comparison/admission capacity",
            )?;
            body.extend_from_slice(part);
        }
        let body = String::from_utf8(body)
            .map_err(|_| crate::error::ContractError::non_utf8("pending source"))?;
        security::ensure_safe(&object([("pending-source.md", string(body.as_str()))]))?;
        rows.push(with(
            select(unit, &["unit_id", "source_ranges", "provenance", "reason"])?,
            [
                ("operation_id", field(manifest, "operation_id")?.clone()),
                ("body", string(body)),
                (
                    "selection_class",
                    string("comparison-only-not-current-guidance"),
                ),
                ("source_assertions_verified", Value::Bool(false)),
            ],
        ));
    }
    Ok(rows)
}
pub fn initial_units(manifest: &Value, unit_map: &Value) -> Result<Vec<Value>> {
    items(field(unit_map, "units")?)?
        .iter()
        .filter(|u| get(u, "disposition").as_str() == Some("pending-admission"))
        .map(|u| {
            Ok(with(
                select(
                    u,
                    &[
                        "unit_id",
                        "source_ranges",
                        "provenance",
                        "reason",
                        "required_actions",
                    ],
                )?,
                [
                    (
                        "detecting_developer_id",
                        field(
                            field(field(manifest, "identity")?, "scope_context")?,
                            "developer_id",
                        )?
                        .clone(),
                    ),
                    ("state", string("pending-admission")),
                    ("terminal_receipts", Value::Array(vec![])),
                ],
            ))
        })
        .collect()
}
pub fn ledger_value(proposal: &Value) -> Result<Value> {
    Ok(with(
        select(
            proposal,
            &[
                "operation_id",
                "project_id",
                "manifest_sha256",
                "unit_map_sha256",
            ],
        )?,
        [
            ("schema", string("agent-memory/layout-pending-ledger-v1")),
            ("proposal_sha256", string(sha(proposal)?)),
            ("generation", number(0)),
            ("previous_generation_sha256", Value::Null),
            ("units", field(proposal, "pending_units")?.clone()),
        ],
    ))
}
pub fn prepare(
    repo: &Path,
    operation_id: &str,
    request: &Value,
    runtime_root: &Path,
) -> Result<Value> {
    let (root, bank, _, _) = operation_paths(repo, operation_id, runtime_root)?;
    locked(&root, &bank, operation_id, |owner| {
        require_completed_resolutions(&root, None, runtime_root)?;
        prepare_locked(&root, operation_id, request, owner, runtime_root)
    })
}
fn retained(manifest: &Value, ids: &[Value]) -> Result<Value> {
    Ok(Value::Object(
        items(field(manifest, "sources")?)?
            .iter()
            .filter(|s| ids.iter().any(|id| same(id, get(s, "source_id"))))
            .map(|s| Ok((text(s, "path")?.to_owned(), field(s, "sha256")?.clone())))
            .collect::<Result<_>>()?,
    ))
}
fn inherited_ids(rows: &[Value]) -> Result<Value> {
    let mut ids = rows
        .iter()
        .map(|r| text(r, "unit_id").map(str::to_owned))
        .collect::<Result<Vec<_>>>()?;
    ids.sort();
    Ok(Value::Array(ids.into_iter().map(string).collect()))
}
fn prepare_locked(
    repo: &Path,
    operation_id: &str,
    request: &Value,
    owner: &[u8],
    runtime_root: &Path,
) -> Result<Value> {
    let op = operation(repo, operation_id, runtime_root)?;
    let manifest = &op.manifest;
    fields(
        request,
        &[
            "schema",
            "operation_id",
            "author",
            "unit_map",
            "intended_files",
            "metadata_plan",
            "review",
            "quiescence_attestation",
            "retained_source_ids",
        ],
        Some("agent-memory/layout-prepare-v1"),
    )?;
    require(
        get(request, "operation_id").as_str() == Some(operation_id),
        "Prepare request belongs to another operation",
    )?;
    let author = field(request, "author")?;
    participant(author)?;
    let (predecessor, pinned) =
        predecessor_value(&op.root, Some(manifest), Some(owner), runtime_root)?;
    let frontier = frontier()?;
    let inherited = frontier.inherited_pending_rows(&op.root, runtime_root)?;
    let scope = field(field(manifest, "identity")?, "scope_context")?;
    require(
        same(get(author, "developer_id"), get(scope, "developer_id")),
        "Prepare author must retain the actual detecting developer",
    )?;
    let intended = field(request, "intended_files")?;
    let ids = items(field(request, "retained_source_ids")?)
        .map_err(|_| err("Retained ordinary occupants require exact source UUIDs"))?;
    let unique: BTreeSet<_> = ids.iter().filter_map(Value::as_str).collect();
    require(
        unique.len() == ids.len() && ids.iter().all(uuid),
        "Retained ordinary occupants require exact source UUIDs",
    )?;
    let retained = retained(manifest, ids)?;
    require(
        pairs(&retained)?.len() == ids.len()
            && pairs(&retained)?.iter().all(|(p, _)| !p.ends_with(".md")),
        "Retained occupants cannot hide an unsupported canonical Markdown source",
    )?;
    let checked = bank::validate_files(intended, None)?;
    require(
        same(get(&checked, "project_id"), get(manifest, "project_id")),
        "Layout recovery cannot change project identity",
    )?;
    for (path, body) in pairs(intended)? {
        security::ensure_safe(&object([(
            path.as_str(),
            string(format!(
                "{path}\n{}",
                body.as_str()
                    .ok_or_else(|| err("Recovery intended files require text"))?
            )),
        )]))?;
    }
    let unit_map = field(request, "unit_map")?;
    unit_map_value(unit_map, manifest, intended, Some(&retained))?;
    let metadata = field(request, "metadata_plan")?;
    metadata_plan(metadata, manifest, intended, ids)?;
    let attestation = field(request, "quiescence_attestation")?;
    let review = field(request, "review")?;
    review_value(
        review,
        manifest,
        unit_map,
        intended,
        author,
        metadata,
        attestation,
        &inherited,
    )?;
    preserve_records(&op.directory, manifest, intended, unit_map)?;
    let mut participants: BTreeSet<String> = items(field(manifest, "sources")?)?
        .iter()
        .map(|s| text(s, "path").map(str::to_owned))
        .collect::<Result<_>>()?;
    participants.extend(pairs(intended)?.iter().map(|(p, _)| p.clone()));
    bank::file_topology(participants.iter().map(String::as_str))?;
    let originals = Value::Object(
        items(field(manifest, "sources")?)?
            .iter()
            .map(|s| Ok((text(s, "path")?.to_owned(), field(s, "sha256")?.clone())))
            .collect::<Result<_>>()?,
    );
    tx::check_layout(&op.bank, &participants, &originals)?;
    let units = initial_units(manifest, unit_map)?;
    let capacity = frontier.prospective_token(
        &op.root,
        manifest,
        &units,
        predecessor.as_ref(),
        Some(owner),
        runtime_root,
    )?;
    let mut pending = pending_rows(&op.directory, manifest, &units)?;
    pending.extend(inherited.clone());
    let reserve = frontier.reservation(intended, &pending, &capacity)?;
    let runtime = recovery_quiescence::measured_runtime(runtime_root)?;
    if !units.is_empty() || !inherited.is_empty() || *attestation != Value::Null {
        recovery_quiescence::attestation_value(attestation, manifest, &runtime)?;
    }
    raw::recheck(
        &op.root,
        manifest,
        Some(owner),
        predecessor.as_ref().map(|p| p.marker()),
    )?;
    let mut final_hashes = pairs(&retained)?.to_vec();
    for (p, b) in pairs(intended)? {
        let hash = string(digest(
            b.as_str()
                .ok_or_else(|| err("Recovery intended files require text"))?
                .as_bytes(),
        ));
        if let Some(row) = final_hashes.iter_mut().find(|(name, _)| name == p) {
            row.1 = hash;
        } else {
            final_hashes.push((p.clone(), hash));
        }
    }
    let final_hashes = Value::Object(final_hashes);
    let mut all = participants;
    all.extend(pairs(&retained)?.iter().map(|(p, _)| p.clone()));
    let mut actions = Vec::new();
    for path in router_last(all) {
        if !same(get(&originals, &path), get(&final_hashes, &path)) {
            actions.push(object([
                ("path", string(path.as_str())),
                ("before", get(&originals, &path).clone()),
                ("after", get(&final_hashes, &path).clone()),
                (
                    "action",
                    string(if final_hashes.get(&path).is_none() {
                        "remove"
                    } else if originals.get(&path).is_none() {
                        "add"
                    } else {
                        "replace"
                    }),
                ),
            ]));
        }
    }
    let unchanged = items(field(manifest, "physical_entries")?)?
        .iter()
        .filter(|r| !actions.iter().any(|a| same(get(a, "path"), get(r, "path"))))
        .cloned()
        .collect();
    let proposal = object([
        ("schema", string(PROPOSAL)),
        ("proposal_id", string(host::uuid4()?)),
        ("operation_id", string(operation_id)),
        ("repo_root", string(path_text(&op.root)?)),
        ("bank_path", string(path_text(&op.bank)?)),
        ("project_id", field(manifest, "project_id")?.clone()),
        ("scope_context", scope.clone()),
        ("manifest_sha256", string(sha(manifest)?)),
        ("unit_map_sha256", string(sha(unit_map)?)),
        ("review_id", field(review, "review_id")?.clone()),
        ("review_sha256", string(sha(review)?)),
        ("review", review.clone()),
        ("author", author.clone()),
        ("intended_files", intended.clone()),
        ("metadata_plan", metadata.clone()),
        ("pending_units", Value::Array(units)),
        ("retained_source_ids", Value::Array(ids.to_vec())),
        ("inherited_unit_ids", inherited_ids(&inherited)?),
        (
            "inherited_frontier_sha256",
            string(sha(&Value::Array(inherited))?),
        ),
        ("capacity_token", capacity.clone()),
        ("predecessor", pinned),
        ("actions", Value::Array(actions.clone())),
        ("unchanged_occupants", Value::Array(unchanged)),
        ("reservation", reserve.clone()),
        (
            "runtime_artifact_digest",
            field(&runtime, "artifact_digest")?.clone(),
        ),
        ("quiescence_attestation", attestation.clone()),
        (
            "quiescence_attestation_sha256",
            if *attestation == Value::Null {
                Value::Null
            } else {
                string(sha(attestation)?)
            },
        ),
        ("hold_id", string(host::uuid4()?)),
        (
            "prepared_at",
            string(crate::local_registry::now(host::Sources::default())?),
        ),
    ]);
    state::write_once(&op.directory.join("unit-map.json"), unit_map)?;
    state::write_once(
        &op.directory
            .join("reviews")
            .join(format!("{}.json", text(review, "review_id")?)),
        review,
    )?;
    if *attestation != Value::Null {
        state::write_once(
            &op.directory
                .join("attestations")
                .join(format!("{}.json", text(attestation, "attestation_id")?)),
            attestation,
        )?;
    }
    state::write_once(
        &op.directory
            .join("proposals")
            .join(format!("{}.json", text(&proposal, "proposal_id")?)),
        &proposal,
    )?;
    Ok(result(
        "prepare",
        Some(operation_id),
        [
            ("state", string("prepared")),
            ("verified", Value::Bool(true)),
            (
                "canonical_eligibility",
                eligibility(
                    &op.root,
                    Some(owner),
                    predecessor.as_ref().map(|p| p.marker()),
                ),
            ),
            ("temporary_lock_writes_performed", Value::Bool(true)),
            ("pending_unit_ids", field(&capacity, "unit_ids")?.clone()),
            (
                "pending_units",
                number(items(field(&capacity, "unit_ids")?)?.len()),
            ),
            ("private_artifact_writes_performed", Value::Bool(true)),
            (
                "preview",
                object([
                    ("actions", Value::Array(actions)),
                    (
                        "unchanged_occupants",
                        field(&proposal, "unchanged_occupants")?.clone(),
                    ),
                    (
                        "original_source_bytes_retained",
                        field(field(manifest, "totals")?, "source_bytes")?.clone(),
                    ),
                    ("all_source_units", field(unit_map, "units")?.clone()),
                    ("reservation", reserve),
                ]),
            ),
            ("proposal", proposal),
        ],
    ))
}
pub fn proposal_value(repo: &Path, supplied: &Value, runtime_root: &Path) -> Result<Value> {
    let p = supplied.get("proposal").unwrap_or(supplied);
    fields(
        p,
        &[
            "schema",
            "proposal_id",
            "operation_id",
            "repo_root",
            "bank_path",
            "project_id",
            "scope_context",
            "manifest_sha256",
            "unit_map_sha256",
            "review_id",
            "review_sha256",
            "review",
            "author",
            "intended_files",
            "metadata_plan",
            "pending_units",
            "retained_source_ids",
            "actions",
            "unchanged_occupants",
            "reservation",
            "runtime_artifact_digest",
            "quiescence_attestation",
            "quiescence_attestation_sha256",
            "hold_id",
            "prepared_at",
            "inherited_unit_ids",
            "inherited_frontier_sha256",
            "capacity_token",
            "predecessor",
        ],
        Some(PROPOSAL),
    )?;
    require(
        [
            "proposal_id",
            "operation_id",
            "project_id",
            "review_id",
            "hold_id",
        ]
        .iter()
        .all(|k| uuid(get(p, k))),
        "Recovery proposal has invalid stable identities",
    )?;
    let (root, bank, _, directory) = operation_paths(repo, text(p, "operation_id")?, runtime_root)?;
    require(
        get(p, "repo_root").as_str() == Some(path_text(&root)?)
            && get(p, "bank_path").as_str() == Some(path_text(&bank)?),
        "Recovery proposal scope changed",
    )?;
    require(
        same(
            &read_value(
                &directory
                    .join("proposals")
                    .join(format!("{}.json", text(p, "proposal_id")?)),
            )?,
            p,
        ),
        "Recovery apply requires the immutable exact reviewed proposal",
    )?;
    Ok(p.clone())
}
pub fn check_review(
    root: &Path,
    directory: &Path,
    manifest: &Value,
    proposal: &Value,
    runtime_root: &Path,
) -> Result<()> {
    let unit_map = read_value(&directory.join("unit-map.json"))?;
    let review = read_value(
        &directory
            .join("reviews")
            .join(format!("{}.json", text(proposal, "review_id")?)),
    )?;
    require(
        get(proposal, "manifest_sha256").as_str() == Some(sha(manifest)?.as_str())
            && get(proposal, "unit_map_sha256").as_str() == Some(sha(&unit_map)?.as_str())
            && get(proposal, "review_sha256").as_str() == Some(sha(&review)?.as_str())
            && same(&review, get(proposal, "review")),
        "Recovery reviewed source/result pins changed",
    )?;
    let scope = field(field(manifest, "identity")?, "scope_context")?;
    require(
        same(get(proposal, "scope_context"), scope)
            && same(get(proposal, "project_id"), get(manifest, "project_id"))
            && same(
                get(get(proposal, "author"), "developer_id"),
                get(scope, "developer_id"),
            )
            && same(
                get(proposal, "pending_units"),
                &Value::Array(initial_units(manifest, &unit_map)?),
            ),
        "Recovery proposal changed its detecting identity or omitted pending source coverage",
    )?;
    let intended = field(proposal, "intended_files")?;
    let ids = items(field(proposal, "retained_source_ids")?)?;
    unit_map_value(
        &unit_map,
        manifest,
        intended,
        Some(&retained(manifest, ids)?),
    )?;
    let inherited = frontier()?.inherited_pending_rows(root, runtime_root)?;
    require(
        same(
            get(proposal, "inherited_unit_ids"),
            &inherited_ids(&inherited)?,
        ) && get(proposal, "inherited_frontier_sha256").as_str()
            == Some(sha(&Value::Array(inherited.clone()))?.as_str()),
        "Recovery inherited private obligations changed",
    )?;
    review_value(
        &review,
        manifest,
        &unit_map,
        intended,
        field(proposal, "author")?,
        field(proposal, "metadata_plan")?,
        field(proposal, "quiescence_attestation")?,
        &inherited,
    )?;
    preserve_records(directory, manifest, intended, &unit_map)?;
    metadata_plan(field(proposal, "metadata_plan")?, manifest, intended, ids)?;
    for (path, body) in pairs(intended)? {
        security::ensure_safe(&object([(
            path.as_str(),
            string(format!(
                "{path}\n{}",
                body.as_str()
                    .ok_or_else(|| err("Recovery intended files require text"))?
            )),
        )]))?;
    }
    require(
        same(
            get(&bank::validate_files(intended, None)?, "project_id"),
            get(manifest, "project_id"),
        ),
        "Recovery intended identity differs",
    )?;
    crate::layout_recovery_context::runtime_guard(root, manifest, proposal, runtime_root)?;
    let mut pending = pending_rows(
        directory,
        manifest,
        items(field(proposal, "pending_units")?)?,
    )?;
    pending.extend(inherited);
    frontier()?.reservation(intended, &pending, field(proposal, "capacity_token")?)?;
    Ok(())
}
pub fn receipt_value(directory: &Path, proposal: &Value) -> Result<Option<Value>> {
    let Some(receipt) = read_optional(&directory.join("receipt.json"))? else {
        return Ok(None);
    };
    require(
        get(&receipt, "schema").as_str() == Some(RECEIPT)
            && ["operation_id", "project_id", "proposal_id"]
                .iter()
                .all(|k| same(get(&receipt, k), get(proposal, k)))
            && get(&receipt, "request_sha256").as_str() == Some(sha(proposal)?.as_str())
            && get(&receipt, "verified").as_bool() == Some(true)
            && ["layout-restored", "rolled-back"]
                .contains(&get(&receipt, "outcome").as_str().unwrap_or("")),
        "Recovery historical receipt is corrupt or operation ID was reused",
    )?;
    Ok(Some(receipt))
}
