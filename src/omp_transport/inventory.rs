//! Retained native evidence inventory, inspection and completion.
use super::{
    MAX_ENTRIES, MAX_PAYLOAD, MAX_RECORD, Names, Site, admission_metadata, all_segments,
    canonical_uuid, control_of, coverage, cursor_value, digest, directories, err, exact, field,
    field_text, files, lexists, listing, manifest_gaps, manifest_value, matched_metadata, read,
    read_optional, registration, registration_lookup, segments, session_dir, set, sha_text, shared,
    shared_inventory, snapshot_path, source_bound_origin, text,
};
use crate::codec::json::Value;
use crate::error::{ContractError, Result};
use crate::lifecycle::{get, object, string};
use crate::omp_projection::{self, exclusions, same, truthy};
use crate::scope_binding::{self, RequireOptions};
use crate::turn_sources;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

const ALLOWED: [&str; 10] = [
    "cursor.json",
    "admission.json",
    "segment.json",
    "captures",
    "gaps",
    "snapshots",
    "enrollments",
    "resolutions",
    "admissions",
    "registration-attempts",
];
/// One page of the joined pending inventory (baseline `48000` encoded bytes).
const MAX_PAGE: usize = 48000;
/// Highest accepted pending cursor offset (baseline `262144`).
const MAX_OFFSET: i64 = 262144;

fn row(cursor: &Value, status: &str, reference: String) -> Result<Value> {
    Ok(object([
        ("session", field(cursor, "session")?.clone()),
        ("lifecycle", field(cursor, "lifecycle")?.clone()),
        (
            "human_root",
            field(field(cursor, "origin")?, "human_root")?.clone(),
        ),
        ("status", string(status)),
        ("source_reference", string(reference)),
    ]))
}

/// Baseline `historical_completed_family`: preserve completed old assertions
/// without granting new root authority.
pub fn historical_completed_family(site: &Site<'_>, cursor: &Value) -> Result<bool> {
    let root = field(field(cursor, "origin")?, "human_root")?;
    let mut represented = false;
    for segment in segments(site, field_text(cursor, "session")?)? {
        let old = segment.snapshot()?;
        if get(get(&old, "origin"), "human_root") != root {
            continue;
        }
        if !lexists(&segment.directory.join("segment.json"))? {
            return Ok(false);
        }
        let state = shared_inventory(site, &old)?;
        let sources = get(&state, "sources").as_array().unwrap_or_default();
        if truthy(get(&state, "faults")) || sources.is_empty() {
            return Ok(false);
        }
        if sources
            .iter()
            .any(|source| !truthy(get(source, "completed")))
        {
            return Ok(false);
        }
        let mut enrollments = Vec::new();
        for path in files(&segment.directory, "enrollments", Names::Hex64)? {
            enrollments.push(read(&path, MAX_RECORD)?);
        }
        if enrollments.is_empty()
            || !enrollments
                .iter()
                .any(|item| get(item, "kind").as_str() == Some("segment"))
        {
            return Ok(false);
        }
        let pairs = |items: &[Value]| -> Result<BTreeSet<(Vec<u8>, Vec<u8>)>> {
            items
                .iter()
                .map(|item| {
                    Ok((
                        turn_sources::encoded(field(item, "source_reference")?)?,
                        turn_sources::encoded(field(item, "source_sha256")?)?,
                    ))
                })
                .collect()
        };
        if pairs(sources)? != pairs(&enrollments)? {
            return Ok(false);
        }
        represented = true;
    }
    Ok(represented)
}

fn segment_enrollments(directory: &Path, cursor: &Value, admission: &Path) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    for path in files(directory, "enrollments", Names::Hex64)? {
        let enrollment = read(&path, MAX_RECORD)?;
        exact(
            &enrollment,
            &[
                "schema",
                "session",
                "lifecycle",
                "source_reference",
                "source_sha256",
                "snapshot_ref",
                "kind",
                "native_source_id",
            ],
        )?;
        let kind = get(&enrollment, "kind").as_str();
        if get(&enrollment, "schema").as_str() != Some("agent-memory/omp-enrollment-v1")
            || get(&enrollment, "session") != field(cursor, "session")?
            || get(&enrollment, "lifecycle") != field(cursor, "lifecycle")?
            || !matches!(kind, Some("admission" | "segment"))
        {
            return Err(err("Foreign native enrollment"));
        }
        if kind == Some("admission") && !lexists(admission)? {
            return Err(err("Native source admission metadata remains unenrolled"));
        }
        let target = if kind == Some("segment") {
            directory.join("segment.json")
        } else {
            snapshot_path(directory, get(&enrollment, "snapshot_ref"))?
        };
        if get(&enrollment, "source_sha256").as_str()
            != Some(digest(&read(&target, MAX_PAYLOAD)?)?.as_str())
        {
            return Err(err("Native enrollment source digest differs"));
        }
        result.push(enrollment);
    }
    Ok(result)
}

fn tool_calls(snapshot: &Value) -> Result<Value> {
    let mut calls = Vec::new();
    if let Some(content) = get(snapshot, "content").as_array() {
        for block in content {
            if block.as_object().is_some() && get(block, "type").as_str() == Some("toolCall") {
                calls.push(field(block, "id")?.clone());
            }
        }
    }
    Ok(Value::Array(calls))
}

/// Retained captures checked against their snapshots; returns referenced
/// snapshot references and captures by capture ID.
fn checked_captures(
    directory: &Path,
    cursor: &Value,
) -> Result<(BTreeSet<String>, BTreeMap<String, Value>)> {
    let mut referenced = BTreeSet::new();
    let mut by_id = BTreeMap::new();
    for path in files(directory, "captures", Names::Hex64)? {
        let captured = read(&path, MAX_RECORD)?;
        exact(
            &captured,
            &[
                "schema",
                "session",
                "lifecycle",
                "capture_id",
                "native_entry_id",
                "native_parent_id",
                "snapshot_ref",
                "snapshot_sha256",
                "representation",
                "mapping",
                "exclusions",
            ],
        )?;
        if get(&captured, "schema").as_str() != Some("agent-memory/omp-capture-v1")
            || get(&captured, "session") != field(cursor, "session")?
            || get(&captured, "lifecycle") != field(cursor, "lifecycle")?
            || get(&captured, "representation").as_str() != Some("native-stored-learning-v1")
            || !matches!(
                get(&captured, "mapping").as_str(),
                Some("verified" | "unverified" | "absent")
            )
            || get(&captured, "exclusions") != &exclusions()
        {
            return Err(err("Foreign native capture metadata"));
        }
        let capture_id = canonical_uuid(get(&captured, "capture_id"))?.to_owned();
        let native = turn_sources::bounded(get(&captured, "native_entry_id"), 512)?;
        if path.file_name().and_then(|n| n.to_str()) != Some(&format!("{}.json", sha_text(native)))
        {
            return Err(err("Native capture namespace differs"));
        }
        let reference = get(&captured, "snapshot_ref");
        let snapshot = read(&snapshot_path(directory, reference)?, MAX_PAYLOAD)?;
        if get(&captured, "snapshot_sha256").as_str() != Some(digest(&snapshot)?.as_str()) {
            return Err(err("Native capture snapshot digest changed"));
        }
        if get(&snapshot, "native_entry_id").as_str() != Some(native)
            || get(&snapshot, "native_parent_id") != get(&captured, "native_parent_id")
            || !omp_projection::valid_instruction_context(&snapshot)
        {
            return Err(err("Invalid native capture snapshot identity"));
        }
        referenced.insert(text(reference)?.to_owned());
        by_id.insert(capture_id, captured);
    }
    Ok((referenced, by_id))
}

fn check_admission(
    directory: &Path,
    cursor: &Value,
    path: &Path,
    by_id: &BTreeMap<String, Value>,
) -> Result<()> {
    if !lexists(path)? {
        return Ok(());
    }
    let value = read(path, MAX_RECORD)?;
    let admission = admission_metadata(&value, field(cursor, "session")?)?;
    let captured = get(admission, "capture_id")
        .as_str()
        .and_then(|id| by_id.get(id));
    let Some(captured) = captured.filter(|_| {
        get(admission, "lifecycle") == get(cursor, "lifecycle")
            && get(admission, "event_ordinal") == get(cursor, "event_ordinal")
    }) else {
        return Err(err(
            "Native admission differs from its retained source capture",
        ));
    };
    let snapshot = read(
        &snapshot_path(directory, field(captured, "snapshot_ref")?)?,
        MAX_PAYLOAD,
    )?;
    let role = snapshot
        .get("role")
        .cloned()
        .unwrap_or_else(|| string("custom"));
    let attribution = snapshot
        .get("attribution")
        .cloned()
        .unwrap_or_else(|| string("unknown"));
    if !same(get(admission, "role"), &role)
        || !same(get(admission, "attribution"), &attribution)
        || !same(get(admission, "tool_call_ids"), &tool_calls(&snapshot)?)
    {
        return Err(err(
            "Native admission metadata differs from its safe stored projection",
        ));
    }
    Ok(())
}

/// Baseline `segment_inventory(repo, directory, cursor)`.
pub fn segment_inventory(site: &Site<'_>, directory: &Path, cursor: &Value) -> Result<Vec<Value>> {
    if let Some(context) = site.context {
        scope_binding::enrollment_read_context(site.repo, context)?;
    }
    let lifecycle = field_text(cursor, "lifecycle")?;
    if !source_bound_origin(directory, cursor)? && !historical_completed_family(site, cursor)? {
        return Ok(vec![row(
            cursor,
            "legacy-unverified-metadata",
            format!("omp:legacy-metadata:{lifecycle}"),
        )?]);
    }
    #[cfg(feature = "dev-tools")]
    crate::bank_writes::fault("omp.join.after-origin-gate")?;
    let manifest = if lexists(&directory.join("segment.json"))? {
        Some(manifest_value(directory, cursor)?)
    } else {
        None
    };
    let interrupted = "Unknown or interrupted native source record";
    if listing(directory, MAX_ENTRIES, interrupted)?
        .iter()
        .any(|name| !ALLOWED.contains(&name.as_str()))
    {
        return Err(err(interrupted));
    }
    let (mut referenced, by_id) = checked_captures(directory, cursor)?;
    let admission_path = directory.join("admission.json");
    check_admission(directory, cursor, &admission_path, &by_id)?;
    let control = control_of(cursor);
    if let Some(entries) = get(&control, "pure_entries").as_object() {
        for (_, record) in entries {
            exact(record, &["snapshot_ref", "snapshot_sha256"])?;
            let reference = get(record, "snapshot_ref");
            let snapshot = read(&snapshot_path(directory, reference)?, MAX_PAYLOAD)?;
            if get(record, "snapshot_sha256").as_str() != Some(digest(&snapshot)?.as_str()) {
                return Err(err("Own control snapshot digest changed"));
            }
            referenced.insert(text(reference)?.to_owned());
        }
    }
    for path in files(directory, "resolutions", Names::Dash36)? {
        let resolution = read(&path, MAX_RECORD)?;
        exact(
            &resolution,
            &[
                "schema",
                "manifest_sha256",
                "gap_id",
                "recovered_ref",
                "recovered_sha256",
                "reason",
            ],
        )?;
        referenced.insert(text(get(&resolution, "recovered_ref"))?.to_owned());
    }
    for path in files(directory, "gaps", Names::Dash36)? {
        let retained = read(&path, MAX_RECORD)?;
        exact(
            &retained,
            &[
                "schema",
                "session",
                "lifecycle",
                "gap_id",
                "native_entry_id",
                "reason",
            ],
        )?;
        if let Some(manifest) = &manifest {
            if !manifest_gaps(manifest)?
                .iter()
                .any(|item| same(item, &retained))
            {
                return Err(err("Unenrolled late native coverage gap remains pending"));
            }
        }
    }
    for path in files(directory, "snapshots", Names::Hex64)? {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if !referenced.contains(&format!("snapshots/{name}")) {
            return Err(err("Unenrolled native snapshot remains pending"));
        }
    }
    let mut rows = Vec::new();
    if manifest.is_none() {
        rows.push(row(
            cursor,
            "open-segment",
            format!("omp:open:{lifecycle}"),
        )?);
        if let Some(tools) = get(cursor, "tool_lifecycles").as_object() {
            for (call_id, _) in tools {
                rows.push(row(
                    cursor,
                    "pending-native-tool",
                    format!("omp:tool:{call_id}"),
                )?);
            }
        }
    }
    let matched = matched_metadata(cursor);
    for path in files(directory, "admissions", Names::Dash36)? {
        let value = read(&path, MAX_RECORD)?;
        let meta = admission_metadata(&value, field(cursor, "session")?)?;
        let capture_id = text(get(meta, "capture_id"))?;
        if path.file_name().and_then(|n| n.to_str()) != Some(&format!("{capture_id}.json")) {
            return Err(err("Native admission metadata namespace differs"));
        }
        if !matched.contains_key(capture_id) {
            let mut legacy = row(
                cursor,
                "legacy-unverified-notification",
                format!("omp:event:{capture_id}"),
            )?;
            set(&mut legacy, "actionable", Value::Bool(false));
            rows.push(legacy);
        }
    }
    let sampled = sampled_attempts(site, directory, cursor)?;
    let enrollments = segment_enrollments(directory, cursor, &admission_path)?;
    let mut state: Option<Value> = None;
    let mut inventory_failed = false;
    for enrollment in &enrollments {
        let observed = (|| -> Result<(Option<Value>, Vec<Value>)> {
            if site.context.is_none() {
                let value = registration(site, directory, cursor, enrollment, false)?;
                #[cfg(feature = "dev-tools")]
                crate::bank_writes::fault("omp.join.after-unscoped-lookup")?;
                return Ok(value);
            }
            if state.is_none() && !inventory_failed {
                let observation = (|| {
                    #[cfg(feature = "dev-tools")]
                    crate::bank_writes::fault("omp.join.before-shared-inventory")?;
                    let value = shared_inventory(site, cursor)?;
                    #[cfg(feature = "dev-tools")]
                    crate::bank_writes::fault("omp.join.after-shared-inventory")?;
                    Ok::<_, ContractError>(value)
                })();
                match observation {
                    Ok(value) => state = Some(value),
                    Err(_) => inventory_failed = true,
                }
            }
            let Some(state) = state.as_ref().filter(|_| !inventory_failed) else {
                return Ok((None, vec![string("source-unavailable")]));
            };
            let reference = field_text(enrollment, "source_reference")?;
            let sample = sampled.get(reference);
            if let Some((path, attempt)) = sample {
                if !same(&read(path, MAX_RECORD)?, attempt) {
                    return Err(err("Native registration attempt identity changed"));
                }
            }
            let (observation, mut errors) = registration_lookup(state, enrollment)?;
            #[cfg(feature = "dev-tools")]
            crate::bank_writes::fault("omp.join.after-row-lookup")?;
            if observation.is_none() && errors.is_empty() && sample.is_some() {
                errors = vec![string("registration-fault-retained")];
            }
            Ok((observation, errors))
        })();
        let (observation, errors) =
            observed.unwrap_or_else(|_| (None, vec![string("source-unavailable")]));
        let completed = observation
            .as_ref()
            .is_some_and(|o| truthy(get(o, "completed")));
        let mut status = if !errors.is_empty() {
            "registration-fault"
        } else if completed {
            "completed"
        } else if observation.is_some() {
            "pending-assessment"
        } else {
            "enrollment-pending"
        };
        if let Some(manifest) = &manifest {
            if !coverage(directory, manifest, completed)? {
                status = "pending-coverage-gap";
            }
        }
        let mut item = row(
            cursor,
            status,
            field_text(enrollment, "source_reference")?.to_owned(),
        )?;
        // Baseline key order: source_reference before source_sha256 and status.
        super::remove(&mut item, "status");
        set(
            &mut item,
            "source_sha256",
            field(enrollment, "source_sha256")?.clone(),
        );
        set(&mut item, "status", string(status));
        if let Some(observation) = &observation {
            set(
                &mut item,
                "observation_id",
                field(observation, "observation_id")?.clone(),
            );
        }
        if !errors.is_empty() {
            set(&mut item, "faults", Value::Array(errors));
        }
        rows.push(item);
    }
    if let Some(manifest) = &manifest {
        let seal = format!("omp:seal:{}", field_text(manifest, "seal_id")?);
        if !rows
            .iter()
            .any(|r| get(r, "source_reference").as_str() == Some(seal.as_str()))
        {
            rows.push(pending_row(cursor, seal)?);
        }
        let admission_ref = field(manifest, "admission_ref")?;
        if admission_ref != &Value::Null {
            let mut captured = Vec::new();
            for path in files(directory, "captures", Names::Hex64)? {
                let value = read(&path, MAX_RECORD)?;
                if same(field(&value, "snapshot_ref")?, admission_ref) {
                    captured.push(value);
                }
            }
            let [captured] = captured.as_slice() else {
                return Err(err("Native manifest admission capture is unavailable"));
            };
            let reference = format!("omp:capture:{}", field_text(captured, "capture_id")?);
            if !rows
                .iter()
                .any(|r| get(r, "source_reference").as_str() == Some(reference.as_str()))
            {
                rows.push(pending_row(cursor, reference)?);
            }
        }
    }
    Ok(rows)
}

/// A manifest-required enrollment row (baseline key order: reference, status).
fn pending_row(cursor: &Value, reference: String) -> Result<Value> {
    let mut item = row(cursor, "enrollment-pending", reference)?;
    super::remove(&mut item, "status");
    set(&mut item, "status", string("enrollment-pending"));
    Ok(item)
}

fn sampled_attempts(
    site: &Site<'_>,
    directory: &Path,
    cursor: &Value,
) -> Result<BTreeMap<String, (PathBuf, Value)>> {
    let mut sampled = BTreeMap::new();
    for path in files(directory, "registration-attempts", Names::Hex64)? {
        let attempt = read(&path, MAX_RECORD)?;
        exact(
            &attempt,
            &[
                "schema",
                "session",
                "lifecycle",
                "source_reference",
                "source_sha256",
            ],
        )?;
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        if get(&attempt, "schema").as_str() != Some("agent-memory/omp-registration-attempt-v1")
            || get(&attempt, "session") != field(cursor, "session")?
            || get(&attempt, "lifecycle") != field(cursor, "lifecycle")?
            || name != format!("{}.json", digest(get(&attempt, "source_reference"))?)
        {
            return Err(err("Foreign native registration attempt"));
        }
        let enrolled = read(&directory.join("enrollments").join(&name), MAX_RECORD)?;
        for key in ["session", "lifecycle", "source_reference", "source_sha256"] {
            if !same(get(&attempt, key), field(&enrolled, key)?) {
                return Err(err(
                    "Native registration attempt lost its enrollment identity",
                ));
            }
        }
        if site.context.is_some() {
            let reference = text(get(&attempt, "source_reference"))?.to_owned();
            sampled.insert(reference, (path, attempt));
        }
    }
    Ok(sampled)
}

fn sort_key(row: &Value) -> (String, String, String) {
    let part = |key: &str| get(row, key).as_str().unwrap_or_default().to_owned();
    (part("session"), part("lifecycle"), part("source_reference"))
}

fn shared_sources(directory: &Path, faults: &mut Vec<String>) -> Vec<Value> {
    let mut sources = Vec::new();
    for index in 0..turn_sources::MAX_SOURCES {
        match read_optional(&directory.join(format!("source-{index}.json")), MAX_RECORD) {
            Ok(None) => {}
            Ok(Some(value)) if value.as_object().is_some() => {
                if get(&value, "harness").as_str() == Some("omp") {
                    sources.push(value);
                }
            }
            Ok(Some(_)) | Err(_) => faults.push("shared-source-record-unverifiable".to_owned()),
        }
    }
    sources
}

fn orphan_rows(
    site: &Site<'_>,
    directory: &Path,
    first: &Value,
    represented: &BTreeSet<String>,
    rows: &mut Vec<Value>,
    faults: &mut Vec<String>,
) -> Result<()> {
    let ns = shared(site, first)?;
    if ns.directory != directory {
        return Err(err("Foreign shared source namespace"));
    }
    let inventory = turn_sources::locked(directory, || {
        turn_sources::inventory(directory, &ns.identity)
    })?;
    let mut found = Vec::new();
    for value in field(&inventory, "sources")?.as_array().unwrap_or_default() {
        let observation = field_text(value, "observation_id")?;
        if !represented.contains(observation) {
            found.push(object([
                ("session", field(value, "session")?.clone()),
                ("lifecycle", field(value, "lifecycle")?.clone()),
                (
                    "source_reference",
                    field(value, "source_reference")?.clone(),
                ),
                ("observation_id", string(observation)),
                ("source_sha256", field(value, "source_sha256")?.clone()),
                ("status", string("orphan-registration")),
            ]));
        }
    }
    let mut codes = Vec::new();
    for fault in field(&inventory, "faults")?.as_array().unwrap_or_default() {
        codes.push(text(fault)?.to_owned());
    }
    rows.extend(found);
    faults.extend(codes);
    Ok(())
}

/// Baseline `pending_inventory(repo)`: `(rows, faults)` over every retained
/// native segment plus orphan shared registrations.
pub fn pending_inventory(site: &Site<'_>) -> Result<(Vec<Value>, Vec<String>)> {
    if let Some(context) = site.context {
        scope_binding::enrollment_read_context(site.repo, context)?;
    }
    let mut rows = Vec::new();
    let mut faults = Vec::new();
    let mut represented = BTreeSet::new();
    for segment in all_segments(site, Some(&mut faults))? {
        let cursor = segment.snapshot()?;
        match segment_inventory(site, &segment.directory, &cursor) {
            Ok(segment_rows) => {
                for item in &segment_rows {
                    if let Some(id) = get(item, "observation_id").as_str() {
                        represented.insert(id.to_owned());
                    }
                }
                rows.extend(segment_rows);
            }
            Err(_) => {
                faults.push("native-enrollment-unverifiable".to_owned());
                let lifecycle = field_text(&cursor, "lifecycle")?;
                let mut item = row(
                    &cursor,
                    "registration-fault",
                    format!("omp:fault:{lifecycle}"),
                )?;
                super::remove(&mut item, "status");
                set(&mut item, "status", string("registration-fault"));
                rows.push(item);
            }
        }
    }
    // Include orphan shared registrations, even if they never reached the transport receipt.
    let turns = site.base()?.join("turn-sources-v2").join("turns");
    let shared_directories = directories(&turns).unwrap_or_else(|_| {
        faults.push("shared-source-inventory-unverifiable".to_owned());
        Vec::new()
    });
    for directory in shared_directories {
        let sources = shared_sources(&directory, &mut faults);
        let Some(first) = sources.first() else {
            continue;
        };
        if orphan_rows(
            site,
            &directory,
            first,
            &represented,
            &mut rows,
            &mut faults,
        )
        .is_err()
        {
            faults.push("orphan-registration-unverifiable".to_owned());
        }
    }
    rows.sort_by_key(sort_key);
    let faults: BTreeSet<String> = faults.into_iter().collect();
    Ok((rows, faults.into_iter().collect()))
}

/// Run `body` under fresh enrollment locations, the shared source barrier and
/// the exclusive transport lock, each rechecked at a successful exit.
pub(crate) fn transport_guard<T>(
    repo: &Path,
    runtime_root: &Path,
    body: impl FnOnce() -> Result<T>,
) -> Result<T> {
    scope_binding::fresh_enrollment_locations(|| {
        let barrier = turn_sources::source_barrier(repo, false, false, None, runtime_root)?;
        let lock = turn_sources::private_lock(
            &crate::omp_boundary::base(repo)?.join("omp-transport.lock"),
            false,
            false,
        )?;
        let value = body()?;
        lock.finish()?;
        barrier.recheck()?;
        Ok(value)
    })
}

fn stream_rows(repo: &Path, runtime_root: &Path) -> Result<Vec<Value>> {
    let authority = crate::omp_stream::Installed::new(runtime_root);
    let inventory = crate::omp_stream::inventory(&authority, repo, None, false)?;
    let mut rows = Vec::new();
    for (index, item) in get(&inventory, "pending")
        .as_array()
        .unwrap_or_default()
        .iter()
        .enumerate()
    {
        let instance = field_text(item, "instance_id")?;
        rows.push(object([
            ("session", field(item, "session")?.clone()),
            ("lifecycle", string(instance)),
            (
                "source_reference",
                string(format!("omp:stream:{instance}:{index}")),
            ),
            ("status", string("pending-native-coverage")),
            ("qualification", item.clone()),
            ("actionable", Value::Bool(false)),
        ]));
    }
    Ok(rows)
}

/// Encoded length of `[a,b,…]` under the canonical compact encoding.
pub(crate) struct PageBytes(usize);
impl PageBytes {
    pub(crate) fn new() -> Self {
        Self(2)
    }
    /// The page length with `row` appended, without changing the page.
    pub(crate) fn with(&self, count: usize, row: &Value) -> Result<usize> {
        let separator = usize::from(count > 0);
        Ok(self.0 + separator + turn_sources::encoded(row)?.len())
    }
    pub(crate) fn push(&mut self, length: usize) {
        self.0 = length;
    }
}

/// Baseline `list_pending(repo, offset)`.
pub fn list_pending(repo: &Path, runtime_root: &Path, offset: i64) -> Result<Value> {
    if !(0..=MAX_OFFSET).contains(&offset) {
        return Err(err("Invalid native pending cursor"));
    }
    let offset = usize::try_from(offset).map_err(|_| err("Invalid native pending cursor"))?;
    transport_guard(repo, runtime_root, || {
        let (rows, faults) = pending_inventory(&Site::new(repo, runtime_root))?;
        let mut rows: Vec<Value> = rows
            .into_iter()
            .filter(|r| get(r, "status").as_str() != Some("completed"))
            .collect();
        rows.extend(stream_rows(repo, runtime_root)?);
        let mut page = Vec::new();
        let mut bytes = PageBytes::new();
        let mut position = offset;
        for item in rows.iter().skip(offset) {
            let length = bytes.with(page.len(), item)?;
            if length > MAX_PAGE {
                break;
            }
            bytes.push(length);
            page.push(item.clone());
            position += 1;
        }
        let complete = faults.is_empty() && position == rows.len() && offset == 0;
        Ok(object([
            ("operation", string("pending")),
            ("pending", Value::Array(page)),
            ("faults", super::strings(faults)),
            (
                "next_offset",
                if position < rows.len() {
                    Value::Int(position.into())
                } else {
                    Value::Null
                },
            ),
            ("inventory_complete", Value::Bool(complete)),
            ("source_completeness_verified", Value::Bool(false)),
            ("canonical_writes_performed", Value::Bool(false)),
            ("verified", Value::Bool(true)),
        ]))
    })
}

/// Baseline `source(repo, session, lifecycle, identifier)`.
pub fn source(
    repo: &Path,
    runtime_root: &Path,
    session: &str,
    lifecycle: &str,
    identifier: &str,
) -> Result<Value> {
    transport_guard(repo, runtime_root, || {
        source_unlocked(
            &Site::new(repo, runtime_root),
            session,
            lifecycle,
            identifier,
        )
    })
}

fn selected_payload(
    directory: &Path,
    enrollment: &Value,
    manifest: Option<&Value>,
) -> Result<Value> {
    if get(enrollment, "kind").as_str() == Some("segment") {
        return manifest
            .cloned()
            .ok_or_else(|| err("Retained native source digest changed"));
    }
    let reference = field(enrollment, "source_reference")?;
    let mut candidates = Vec::new();
    for path in files(directory, "captures", Names::Hex64)? {
        let captured = read(&path, MAX_RECORD)?;
        if reference.as_str()
            == Some(format!("omp:capture:{}", field_text(&captured, "capture_id")?).as_str())
        {
            candidates.push(captured);
        }
    }
    let [candidate] = candidates.as_slice() else {
        return Err(err("Original native capture unavailable"));
    };
    read(
        &snapshot_path(directory, field(candidate, "snapshot_ref")?)?,
        MAX_PAYLOAD,
    )
}

fn qualifications(directory: &Path, cursor: &Value) -> Result<Vec<Value>> {
    let control = control_of(cursor);
    let absent: Vec<&Value> = get(&control, "absent_native_entries")
        .as_array()
        .map(|items| items.iter().collect())
        .unwrap_or_default();
    let mut result = Vec::new();
    for path in files(directory, "captures", Names::Hex64)? {
        let item = read(&path, MAX_RECORD)?;
        let snapshot_file = snapshot_path(directory, field(&item, "snapshot_ref")?)?;
        let snapshot = read(&snapshot_file, MAX_PAYLOAD)?;
        let native = field(&item, "native_entry_id")?;
        let mapping = if absent.contains(&native) {
            string("absent")
        } else {
            field(&item, "mapping")?.clone()
        };
        let mut qualification = object([
            ("native_entry_id", native.clone()),
            ("mapping", mapping),
            (
                "snapshot_reference",
                string(
                    snapshot_file
                        .to_str()
                        .ok_or_else(|| err("Invalid native snapshot path"))?,
                ),
            ),
            ("snapshot_sha256", field(&item, "snapshot_sha256")?.clone()),
            ("exclusions", field(&item, "exclusions")?.clone()),
        ]);
        if let Some(context) = snapshot.get("instruction_context") {
            set(&mut qualification, "instruction_context", context.clone());
            set(
                &mut qualification,
                "instruction_context_qualification",
                string(
                    "Historical observer assertion; body intentionally nonlearning, not current installation authentication or complete source coverage.",
                ),
            );
        }
        result.push(qualification);
    }
    Ok(result)
}

/// Baseline `_source`: the retained original source of one enrollment.
pub(crate) fn source_unlocked(
    site: &Site<'_>,
    session: &str,
    lifecycle: &str,
    identifier: &str,
) -> Result<Value> {
    canonical_uuid(&string(lifecycle))?;
    let directory = session_dir(site, session)?.join(lifecycle);
    let cursor = read(&directory.join("cursor.json"), MAX_PAYLOAD)?;
    cursor_value(&cursor, &string(session), lifecycle)?;
    let bound = source_bound_origin(&directory, &cursor)?;
    if !bound && !historical_completed_family(site, &cursor)? {
        return Err(err("Native source has no independently stored root origin"));
    }
    segment_inventory(site, &directory, &cursor)?;
    let mut observation = None;
    for path in files(&directory, "enrollments", Names::Hex64)? {
        let enrollment = read(&path, MAX_RECORD)?;
        let (found, errors) = registration(site, &directory, &cursor, &enrollment, false)?;
        let selected = get(&enrollment, "source_reference").as_str() == Some(identifier)
            || found
                .as_ref()
                .is_some_and(|f| get(f, "observation_id").as_str() == Some(identifier));
        if selected {
            if observation.is_some() {
                return Err(err("Ambiguous native source"));
            }
            observation = Some((enrollment, found, errors));
        }
    }
    let (enrollment, shared_observation, faults) =
        observation.ok_or_else(|| err("Original native source unavailable"))?;
    let manifest = if lexists(&directory.join("segment.json"))? {
        Some(manifest_value(&directory, &cursor)?)
    } else {
        None
    };
    let payload = selected_payload(&directory, &enrollment, manifest.as_ref())?;
    if get(&enrollment, "source_sha256").as_str() != Some(digest(&payload)?.as_str()) {
        return Err(err("Retained native source digest changed"));
    }
    let completed = shared_observation
        .as_ref()
        .is_some_and(|o| truthy(get(o, "completed")));
    let coverage_complete = match &manifest {
        Some(manifest) => coverage(&directory, manifest, completed)?,
        None => false,
    };
    let mut resolutions = Vec::new();
    for path in files(&directory, "resolutions", Names::Dash36)? {
        resolutions.push(read(&path, MAX_RECORD)?);
    }
    let gaps = match &manifest {
        Some(manifest) => Value::Array(manifest_gaps(manifest)?.to_vec()),
        None => Value::Array(Vec::new()),
    };
    Ok(object([
        ("verified", Value::Bool(true)),
        ("source", payload),
        ("enrollment", enrollment),
        ("observation", shared_observation.unwrap_or(Value::Null)),
        ("origin", field(&cursor, "origin")?.clone()),
        ("coverage_complete", Value::Bool(coverage_complete)),
        (
            "origin_qualification",
            string(if bound {
                "stored-native-id-bound"
            } else {
                "historical-unverified-native-origin"
            }),
        ),
        ("exclusions", exclusions()),
        ("faults", Value::Array(faults)),
        (
            "retained_directory",
            string(
                directory
                    .to_str()
                    .ok_or_else(|| err("Invalid native source path"))?,
            ),
        ),
        (
            "captures",
            Value::Array(qualifications(&directory, &cursor)?),
        ),
        ("gaps", gaps),
        ("gap_resolutions", Value::Array(resolutions)),
        ("canonical_writes_performed", Value::Bool(false)),
    ]))
}

/// Baseline `complete(repo, value)`: observation-bound assessment of one
/// sealed, registered, fully covered native segment.
pub fn complete(repo: &Path, runtime_root: &Path, value: &Value) -> Result<Value> {
    let fields = [
        "session",
        "lifecycle",
        "observation_id",
        "source_sha256",
        "outcome",
        "assessment",
    ];
    let with_request = [
        "session",
        "lifecycle",
        "observation_id",
        "source_sha256",
        "outcome",
        "assessment",
        "publication_request",
    ];
    if !omp_projection::has_exact(value, &fields)
        && !omp_projection::has_exact(value, &with_request)
    {
        return Err(err("Use exact native observation-bound assessment fields"));
    }
    transport_guard(repo, runtime_root, || {
        // No held-enrollment authority reaches acknowledgement or refresh.
        scope_binding::require(repo, None, RequireOptions::default())?;
        let site = Site::new(repo, runtime_root);
        let session = text(get(value, "session"))?;
        let lifecycle = text(get(value, "lifecycle"))?;
        let observation_id = get(value, "observation_id")
            .as_str()
            .ok_or_else(|| err("Original native source unavailable"))?;
        let checked = source_unlocked(&site, session, lifecycle, observation_id)?;
        if get(&checked, "coverage_complete") != &Value::Bool(true)
            || truthy(get(&checked, "faults"))
            || get(&checked, "observation") == &Value::Null
        {
            return Err(err(
                "Native segment not sealed/enrolled or has unresolved coverage gaps",
            ));
        }
        let directory = session_dir(&site, session)?.join(lifecycle);
        let cursor = read(&directory.join("cursor.json"), MAX_PAYLOAD)?;
        if !source_bound_origin(&directory, &cursor)? {
            let observation = get(&checked, "observation");
            let receipt = if truthy(get(observation, "completed")) {
                get(observation, "completion")
            } else {
                &Value::Null
            };
            if receipt.as_object().is_none()
                || get(receipt, "input_sha256").as_str() != Some(digest(value)?.as_str())
                || get(receipt, "source_sha256") != get(value, "source_sha256")
            {
                return Err(err(
                    "Native assessment requires an independently stored root origin",
                ));
            }
            let mut result = object([
                ("verified", Value::Bool(true)),
                ("already_completed", Value::Bool(true)),
            ]);
            for (key, item) in receipt.as_object().unwrap_or_default() {
                set(&mut result, key, item.clone());
            }
            set(
                &mut result,
                "origin_qualification",
                string("historical-unverified-native-origin"),
            );
            return Ok(result);
        }
        read(&directory.join("segment.json"), MAX_PAYLOAD)?;
        let mut sealed = Vec::new();
        for path in files(&directory, "enrollments", Names::Hex64)? {
            let enrollment = read(&path, MAX_RECORD)?;
            if field(&enrollment, "kind")?.as_str() == Some("segment") {
                sealed.push(enrollment);
            }
        }
        let registered = match sealed.as_slice() {
            [segment] => registration(&site, &directory, &cursor, segment, false)?.0,
            _ => None,
        };
        if registered.is_none() {
            return Err(err(
                "Admission acknowledgement requires registered sealed segment",
            ));
        }
        if get(get(&checked, "enrollment"), "source_sha256") != get(value, "source_sha256") {
            return Err(err("Native assessment source digest differs"));
        }
        let source_sha256 = text(get(value, "source_sha256"))?;
        let receipt = turn_sources::verify_assessment(repo, value, observation_id, runtime_root)?;
        turn_sources::complete(
            repo,
            "omp",
            session,
            lifecycle,
            observation_id,
            source_sha256,
            &receipt,
            runtime_root,
        )
    })
}
