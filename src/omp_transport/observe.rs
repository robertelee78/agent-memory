//! The native observer operation: ingest, seal and answer-first wake.
use super::inventory::{PageBytes, pending_inventory};
use super::{
    HUMAN_TRIGGERS, MAX_CAPTURES, MAX_PAYLOAD, MAX_RECORD, ManualRead, NONLEARNING, Names, Seg,
    Site, active_segment, canonical_uuid, capture, control_of, cursor_value, digest,
    drain_enrollments, enroll_admission, err, exact, field, field_text, files, lexists,
    manifest_gaps, manifest_value, max_ordinal, new_segment, read, remove, replace_cursor, retain,
    retained_gap, seal_segment, segment_gap, segments, session_dir, set, sha_text, snapshot_path,
    snapshot_reference, source_bound_origin, strings, text, uuid5_url, validate_root_wakes,
    writable_successor,
};
use crate::codec::json::{self, Value};
use crate::error::Result;
use crate::lifecycle::{get, object, string};
use crate::omp_control;
use crate::omp_installation;
use crate::omp_projection::{self, same, truthy};
use crate::omp_stream::{self, DeliveryWindow, Installed, RecoveryEvidence, SourceCompletion};
use crate::scope_binding::{self, PrivateEnrollmentContext};
use crate::turn_sources::{self, SourceBarrier};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::rc::Rc;

/// Answer-first wake guidance bound (baseline `8192` encoded bytes).
const MAX_GUIDANCE: usize = 8192;
/// Inline wake source rows (baseline `6000` encoded bytes).
const MAX_INLINE: usize = 6000;
/// Payload source references returned to the finite stream (baseline `64`).
const MAX_REFERENCES: usize = 64;
/// A truthy non-string `toolCallId` never names a retained invocation.
const FOREIGN_CALL: &str = "\u{0}";

/// The observer's executable/marker scope (baseline `_verify_observer_receipt`
/// with `repo=`): verify the installed binding, observe the marker under the
/// held source barrier, run `body` with the live private enrollment context,
/// and recheck everything at exit.
pub trait ObserverScope {
    fn observe(
        &self,
        repo: &Path,
        held: Option<&SourceBarrier>,
        body: &mut dyn FnMut(&PrivateEnrollmentContext) -> Result<()>,
    ) -> Result<()>;
}

/// Everything an observer call carries besides the repository and request.
#[derive(Default)]
pub struct Observation<'a, 'c> {
    pub receipt: Option<&'a Value>,
    pub on_started: Option<&'a mut dyn FnMut() -> Result<()>>,
    pub instance_id: Option<&'a str>,
    pub identity: Option<&'a Value>,
    pub delivery_window: Option<DeliveryWindow<'a>>,
    pub source_completion: Option<&'a SourceCompletion<'c>>,
    pub held_source: Option<&'a SourceBarrier>,
}

fn pending(status: &str) -> Value {
    object([
        ("continue", Value::Bool(false)),
        ("status", string(status)),
        ("canonical_writes_performed", Value::Bool(false)),
    ])
}

fn sorted_unique(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        items.iter().all(|item| item.as_str().is_some())
            && items
                .windows(2)
                .all(|pair| matches!(pair, [a, b] if a.as_str() < b.as_str()))
    })
}

/// The observer marker context handed to the finite stream's completion.
pub(crate) fn context_value(context: &PrivateEnrollmentContext) -> Result<Value> {
    let path = |path: &Path| -> Result<Value> {
        Ok(string(
            path.to_str()
                .ok_or_else(|| err("Invalid native bank path"))?,
        ))
    };
    Ok(object([
        ("root", path(context.root())?),
        ("bank", path(context.bank())?),
        ("project_id", context.project_id().clone()),
        ("scope", context.scope()),
        ("held", Value::Bool(context.held())),
    ]))
}

fn root_rows<'r>(rows: &'r [Value], session: &str, root: Option<&Value>) -> Vec<&'r Value> {
    rows.iter()
        .filter(|r| {
            root.is_some_and(|root| {
                get(r, "session").as_str() == Some(session) && get(r, "human_root") == root
            }) && get(r, "status").as_str() != Some("completed")
        })
        .collect()
}

fn wake_payload(rows: &[&Value]) -> Value {
    let mut ids: Vec<&str> = rows
        .iter()
        .filter_map(|r| get(r, "observation_id").as_str())
        .collect();
    ids.sort_unstable();
    let mut refs: Vec<&str> = rows
        .iter()
        .filter(|r| r.get("observation_id").is_none())
        .filter_map(|r| get(r, "source_reference").as_str())
        .filter(|r| r.starts_with("omp:capture:") || r.starts_with("omp:seal:"))
        .collect();
    refs.sort_unstable();
    object([
        ("observation_ids", strings(ids)),
        ("enrollment_refs", strings(refs)),
    ])
}

fn guidance(
    repo: &Path,
    request_id: &str,
    inline: &[&Value],
    complete: bool,
    faults: &[String],
) -> Result<String> {
    let root = crate::fs::canonical_path(repo, false)?;
    let root = string(
        root.to_str()
            .ok_or_else(|| err("Invalid native repository path"))?,
    );
    let inline = Value::Array(inline.iter().map(|row| (*row).clone()).collect());
    Ok(format!(
        "[agent-memory omp request {request_id}] Agent advisory, not human input. \
Answer delivered; never repeat/replace it. Governed maintenance now; never defer. Read skill://agent-memory; approved tools only. \
Inspect exact source: am omp-boundary --repo {} --source ID_OR_REFERENCE --session SESSION --lifecycle LIFECYCLE. Use raw unregistered refs; assess registered IDs/digests only. \
Re-observe/search current memory before governed capture/publication. \
Assess each source: am omp-boundary --repo PATH --stdin --quiet --refresh; exact keys: \
{{session,lifecycle,observation_id,source_sha256,outcome,assessment}}; publication_request only for published. \
Admission: assess enrolled segments only; never waive unresolved gaps. \
Busy/refused/cancelled/unavailable/omitted stays pending: stop; no auto-retry, permission bypass or forced tools. \
Joined backlog: am omp-boundary --repo PATH --list-pending --offset 0; follow next_offset to exhaustion. \
inventory_complete:{complete}. Sources: {} Faults:{} [/agent-memory omp request]",
        json::encode(&root, json::DEFAULT)?,
        json::encode(&inline, json::COMPACT_ASCII)?,
        json::encode(&strings(faults.iter().cloned()), json::DEFAULT)?,
    ))
}

/// Baseline `wake(repo, session, lifecycle, payload, delivery_window=…,
/// enrollment_context=…, source_completion=…)`.
pub fn wake(
    site: &Site<'_>,
    session: &str,
    lifecycle: &str,
    payload: &Value,
    delivery_window: Option<&DeliveryWindow<'_>>,
    source_completion: Option<&SourceCompletion<'_>>,
) -> Result<Value> {
    exact(payload, &["observation_ids", "enrollment_refs"])?;
    for key in ["observation_ids", "enrollment_refs"] {
        if !sorted_unique(get(payload, key)) {
            return Err(err("Use sorted unique native wake references"));
        }
    }
    let namespace = session_dir(site, session)?;
    let directory = namespace.join(canonical_uuid(&string(lifecycle))?);
    let cursor = read(&directory.join("cursor.json"), MAX_PAYLOAD)?;
    cursor_value(&cursor, &string(session), lifecycle)?;
    let (rows, faults) = pending_inventory(site)?;
    let root_value = get(get(&cursor, "origin"), "human_root");
    let current = root_rows(&rows, session, Some(root_value));
    if !same(&wake_payload(&current), payload) {
        return Err(err("Current native root inventory changed before wake"));
    }
    let root = text(root_value)?;
    let target = namespace.join("root-wakes").join(format!("{root}.json"));
    let trigger = get(get(&cursor, "origin"), "native_trigger").as_str();
    if lexists(&target)? || !trigger.is_some_and(|t| HUMAN_TRIGGERS.contains(&t)) {
        return Ok(pending("pending"));
    }
    let root_path = namespace.join(root).join("cursor.json");
    let mut root_cursor = read(&root_path, MAX_PAYLOAD)?;
    let native_user = get(get(&root_cursor, "control_interval"), "human_entry").clone();
    if !source_bound_origin(&namespace.join(root), &root_cursor)? || !truthy(&native_user) {
        return Ok(pending("pending-missing-human-source"));
    }
    let wakes = namespace.join("root-wakes");
    let prior = if lexists(&wakes)? {
        match validate_root_wakes(&wakes, Some(session)) {
            Ok(prior) => prior,
            Err(_) => return Ok(pending("pending-wake-state-unverifiable")),
        }
    } else {
        BTreeSet::new()
    };
    if native_user
        .as_str()
        .is_some_and(|user| prior.contains(user))
    {
        return Ok(pending("pending-already-requested-native-user"));
    }
    let request_id = crate::host::uuid4()?;
    let value = object([
        ("schema", string("agent-memory/omp-root-wake-v1")),
        ("session", string(session)),
        ("human_root", string(root)),
        ("native_user_entry", native_user),
        ("request_id", string(&request_id)),
        ("status", string("requested")),
    ]);
    let earlier = rows.iter().filter(|r| {
        get(r, "status").as_str() != Some("completed")
            && !current.iter().any(|c| std::ptr::eq(*c, *r))
    });
    let mut inline = Vec::new();
    let mut bytes = PageBytes::new();
    let mut omitted = false;
    for row in current.iter().copied().chain(earlier) {
        let length = bytes.with(inline.len(), row)?;
        if length > MAX_INLINE {
            omitted = true;
            continue;
        }
        bytes.push(length);
        inline.push(row);
    }
    let complete = !omitted && faults.is_empty();
    let guidance = guidance(site.repo, &request_id, &inline, complete, &faults)?;
    if guidance.len() > MAX_GUIDANCE {
        return Ok(pending("pending-wake-guidance-bound"));
    }
    if let Some(context) = site.context {
        scope_binding::enrollment_read_context(site.repo, context)?;
        scope_binding::recheck_private_enrollment(context)?;
    }
    if let (Some(completion), Some(context)) = (source_completion, site.context) {
        completion.before_reserve(&context_value(context)?)?;
    } else if source_completion.is_some() {
        return Err(err("Wake requires a committed finite source"));
    }
    #[cfg(feature = "dev-tools")]
    crate::bank_writes::fault("omp.reservation.before-reserve")?;
    if !delivery_window.is_some_and(DeliveryWindow::reserve) {
        return Ok(pending("pending-undeliverable-window"));
    }
    let reserved = (|| -> Result<()> {
        #[cfg(feature = "dev-tools")]
        crate::bank_writes::fault("omp.reservation.after-reserve")?;
        retain(&target, &value, MAX_RECORD)?;
        #[cfg(feature = "dev-tools")]
        crate::bank_writes::fault("omp.reservation.after-root")?;
        let mut control = control_of(&root_cursor);
        set(&mut control, "generated_guidance", string(&guidance));
        set(&mut root_cursor, "control_interval", control);
        replace_cursor(&root_path, &root_cursor)?;
        #[cfg(feature = "dev-tools")]
        crate::bank_writes::fault("omp.reservation.after-cursor")?;
        Ok(())
    })();
    if reserved.is_err() {
        // Reservation may have reached durable storage. Never retry this attempt.
        return Ok(pending("pending-wake-unknown-after-reservation"));
    }
    if let Some(completion) = source_completion {
        completion.offer_root_wake(&target);
    }
    Ok(object([
        ("continue", Value::Bool(true)),
        ("additionalContext", string(guidance)),
        ("request_id", string(request_id)),
        ("inventory_complete", Value::Bool(complete)),
        ("canonical_writes_performed", Value::Bool(false)),
    ]))
}

fn call_lifecycles(segment: &Seg) -> Result<BTreeMap<String, Value>> {
    Ok(get(&*segment.cursor()?, "tool_lifecycles")
        .as_object()
        .map(|pairs| pairs.iter().cloned().collect())
        .unwrap_or_default())
}

fn tool_gap_retained(current: &[Seg], call_id: &str) -> Result<bool> {
    let tool = format!("tool:{call_id}");
    for segment in current {
        for path in files(&segment.directory, "gaps", Names::Dash36)? {
            if field(&read(&path, MAX_RECORD)?, "native_entry_id")?.as_str() == Some(tool.as_str())
            {
                return Ok(true);
            }
        }
        if segment.sealed()? {
            let manifest = manifest_value(&segment.directory, &*segment.cursor()?)?;
            if manifest_gaps(&manifest)?
                .iter()
                .any(|old| get(old, "native_entry_id").as_str() == Some(tool.as_str()))
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}

fn reassociate(segment: &Seg, call_id: &str, recovered: &Value) -> Result<()> {
    let manifest = manifest_value(&segment.directory, &*segment.cursor()?)?;
    let tool = format!("tool:{call_id}");
    for old_gap in manifest_gaps(&manifest)? {
        if get(old_gap, "native_entry_id").as_str() != Some(tool.as_str()) {
            continue;
        }
        let reference = snapshot_reference(field_text(recovered, "native_entry_id")?);
        retain(&segment.directory.join(&reference), recovered, MAX_PAYLOAD)?;
        let gap_id = field_text(old_gap, "gap_id")?;
        let resolution = object([
            ("schema", string("agent-memory/omp-gap-resolution-v1")),
            ("manifest_sha256", string(digest(&manifest)?)),
            ("gap_id", string(gap_id)),
            ("recovered_ref", string(reference)),
            ("recovered_sha256", string(digest(recovered)?)),
            ("reason", string("verified-native-reassociation")),
        ]);
        retain(
            &segment
                .directory
                .join("resolutions")
                .join(format!("{gap_id}.json")),
            &resolution,
            MAX_RECORD,
        )?;
    }
    Ok(())
}

/// Baseline `resolve_tool_result`: recover only an unambiguous native
/// invocation, including crash readback.
pub fn resolve_tool_result(
    current: &[Seg],
    owner: &Seg,
    captured: &Value,
    invocations: &BTreeMap<String, BTreeSet<String>>,
) -> Result<bool> {
    let mut bound = Vec::new();
    for segment in current {
        if segment.bound()? {
            bound.push(Rc::clone(segment));
        }
    }
    if !owner.bound()? {
        return Ok(false);
    }
    let recovered = read(
        &snapshot_path(&owner.directory, field(captured, "snapshot_ref")?)?,
        MAX_PAYLOAD,
    )?;
    let Some(call_id) = get(&recovered, "toolCallId")
        .as_str()
        .filter(|c| !c.is_empty())
    else {
        return Ok(false);
    };
    if get(&recovered, "role").as_str() != Some("toolResult") {
        return Ok(false);
    }
    if recovered.get("instruction_context").is_some() && tool_gap_retained(&bound, call_id)? {
        return Ok(false);
    }
    let mut roots = BTreeSet::new();
    for segment in &bound {
        if call_lifecycles(segment)?.contains_key(call_id) {
            roots.insert(segment.root()?);
        }
    }
    let root = owner.root()?;
    let invoked = invocations.get(call_id).map_or(0, BTreeSet::len);
    if invoked != 1 || roots.len() != 1 || !roots.contains(&root) {
        return Ok(false);
    }
    for segment in &bound {
        if !call_lifecycles(segment)?.contains_key(call_id) {
            continue;
        }
        if segment.sealed()? {
            reassociate(segment, call_id, &recovered)?;
        } else {
            let updated = {
                let mut cursor = segment.cursor_mut()?;
                if let Some(tools) = cursor.get_mut("tool_lifecycles") {
                    remove(tools, call_id);
                }
                cursor.clone()
            };
            replace_cursor(&segment.directory.join("cursor.json"), &updated)?;
        }
    }
    Ok(true)
}

/// Baseline `_checked_payload_coverage`: retained safe native IDs or explicit
/// payload-not-required metadata only.
pub fn checked_payload_coverage(
    current: &[Seg],
    entries: &[Value],
    view: &Value,
    rows: &[Value],
    faults: &[String],
) -> Result<(Vec<String>, Vec<Value>, bool)> {
    let native_ids: BTreeSet<String> = get(view, "native_parents")
        .as_array()
        .unwrap_or_default()
        .iter()
        .filter_map(|item| get(item, "id").as_str().map(str::to_owned))
        .collect();
    let mut qualified = BTreeSet::new();
    for entry in entries {
        let nonlearning = get(entry, "type")
            .as_str()
            .is_some_and(|kind| NONLEARNING.contains(&kind));
        if nonlearning || get(get(entry, "agent"), "kind").as_str() == Some("sub") {
            qualified.insert(field_text(entry, "id")?.to_owned());
        }
    }
    let mut bound_lifecycles = BTreeSet::new();
    for segment in current {
        if !segment.bound()? {
            continue;
        }
        bound_lifecycles.insert(segment.lifecycle()?);
        for path in files(&segment.directory, "captures", Names::Hex64)? {
            let captured = read(&path, MAX_RECORD)?;
            let snapshot = read(
                &snapshot_path(&segment.directory, field(&captured, "snapshot_ref")?)?,
                MAX_PAYLOAD,
            )?;
            if get(&captured, "snapshot_sha256").as_str() != Some(digest(&snapshot)?.as_str()) {
                return Err(err("Checked native payload digest changed"));
            }
            qualified.insert(field_text(&captured, "native_entry_id")?.to_owned());
        }
        let control = control_of(&*segment.cursor()?);
        for (native, captured) in get(&control, "pure_entries")
            .as_object()
            .unwrap_or_default()
        {
            let snapshot = read(
                &snapshot_path(&segment.directory, field(captured, "snapshot_ref")?)?,
                MAX_PAYLOAD,
            )?;
            if get(captured, "snapshot_sha256").as_str() != Some(digest(&snapshot)?.as_str()) {
                return Err(err("Checked native control payload changed"));
            }
            qualified.insert(native.clone());
        }
    }
    let session = get(view, "session");
    let relevant: Vec<&Value> = rows
        .iter()
        .filter(|r| get(r, "session") == session)
        .collect();
    let excluded = [
        "orphan-registration",
        "legacy-unverified-metadata",
        "open-segment",
    ];
    let mut references = Vec::new();
    for row in &relevant {
        let status = get(row, "status").as_str().unwrap_or_default();
        let lifecycle = get(row, "lifecycle").as_str().unwrap_or_default();
        if row.get("observation_id").is_some()
            && row.get("source_sha256").is_some()
            && !excluded.contains(&status)
            && bound_lifecycles.contains(lifecycle)
        {
            references.push(object([
                ("session", get(row, "session").clone()),
                ("lifecycle", get(row, "lifecycle").clone()),
                ("observation_id", get(row, "observation_id").clone()),
                ("source_sha256", get(row, "source_sha256").clone()),
            ]));
        }
    }
    let references_complete = references.len() <= MAX_REFERENCES;
    references.truncate(MAX_REFERENCES);
    let incomplete = [
        "pending-coverage-gap",
        "registration-fault",
        "pending-native-tool",
        "legacy-unverified-metadata",
        "enrollment-pending",
        "orphan-registration",
        "open-segment",
    ];
    let complete = !native_ids.is_empty()
        && native_ids.is_subset(&qualified)
        && faults.is_empty()
        && references_complete
        && !relevant.iter().any(|r| {
            get(r, "status")
                .as_str()
                .is_some_and(|s| incomplete.contains(&s))
        })
        && get(view, "tail_status").as_str() == Some("available");
    let payload_ids = qualified.intersection(&native_ids).cloned().collect();
    Ok((payload_ids, references, complete))
}

/// The pre-guard checked request (baseline top of `observe`).
struct Request {
    action: String,
    session: String,
    lifecycle: String,
    payload: Value,
    sweep: Option<SweepInput>,
}
/// Validated ingest/seal payload fields.
struct SweepInput {
    entries: Vec<Value>,
    branch_ids: Vec<String>,
    inherited_ids: Vec<String>,
    event_ordinal: u64,
    terminal: bool,
    trigger: String,
    request_wake: bool,
    view: Value,
    root: PathBuf,
}

fn ids(value: &Value) -> Result<Vec<String>> {
    let items = value
        .as_array()
        .filter(|items| items.len() <= MAX_CAPTURES)
        .ok_or_else(|| err("Invalid native branch inventory"))?;
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for item in items {
        let native = turn_sources::bounded(item, 512)?;
        if !seen.insert(native) {
            return Err(err("Invalid native branch inventory"));
        }
        result.push(native.to_owned());
    }
    Ok(result)
}

fn sweep_input(repo: &Path, session: &str, action: &str, payload: &Value) -> Result<SweepInput> {
    let mut fields = vec![
        "entries",
        "branch_ids",
        "event_ordinal",
        "terminal",
        "trigger",
        "inherited_ids",
        "view",
    ];
    if action == "seal" {
        fields.push("request_wake");
    }
    exact(payload, &fields)?;
    let request_wake = match payload.get("request_wake") {
        None => false,
        Some(Value::Bool(flag)) => *flag,
        Some(_) => return Err(err("Invalid native wake selection")),
    };
    let event_ordinal = get(payload, "event_ordinal")
        .as_int()
        .and_then(|n| n.as_u64());
    let terminal = get(payload, "terminal").as_bool();
    let trigger = get(payload, "trigger")
        .as_str()
        .filter(|t| super::TRIGGERS.contains(t));
    let (Some(event_ordinal), Some(terminal), Some(trigger)) = (event_ordinal, terminal, trigger)
    else {
        return Err(err("Native stored sweep exceeds bound"));
    };
    let (view, root) = omp_projection::checked_view(repo, session, payload)?;
    #[cfg(feature = "dev-tools")]
    crate::bank_writes::fault("omp.checked-view.after-validation")?;
    let branch_ids = ids(get(payload, "branch_ids"))?;
    let inherited_ids = ids(get(payload, "inherited_ids"))?;
    let parents: BTreeSet<&str> = get(&view, "native_parents")
        .as_array()
        .unwrap_or_default()
        .iter()
        .filter_map(|item| get(item, "id").as_str())
        .collect();
    if !inherited_ids.iter().all(|id| parents.contains(id.as_str())) {
        return Err(err("Inherited native source identity unavailable"));
    }
    Ok(SweepInput {
        entries: get(payload, "entries")
            .as_array()
            .unwrap_or_default()
            .to_vec(),
        branch_ids,
        inherited_ids,
        event_ordinal,
        terminal,
        trigger: trigger.to_owned(),
        request_wake,
        view,
        root,
    })
}

fn request(repo: &Path, value: &Value) -> Result<Request> {
    exact(
        value,
        &["schema", "action", "session", "lifecycle", "payload"],
    )?;
    let action = get(value, "action").as_str().unwrap_or_default();
    if get(value, "schema").as_str() != Some("agent-memory/omp-observer-request-v2")
        || !matches!(action, "ingest" | "seal" | "wake")
    {
        return Err(err("Unsupported native observer operation"));
    }
    let session = turn_sources::bounded(get(value, "session"), 512)?.to_owned();
    let lifecycle = canonical_uuid(get(value, "lifecycle"))?.to_owned();
    let payload = get(value, "payload").clone();
    let sweep = if action == "wake" {
        None
    } else {
        Some(sweep_input(repo, &session, action, &payload)?)
    };
    Ok(Request {
        action: action.to_owned(),
        session,
        lifecycle,
        payload,
        sweep,
    })
}

/// A `toolCallId` as a lookup key (baseline dict semantics).
fn call_key(value: &Value) -> Result<Option<String>> {
    match value {
        Value::String(id) if id.is_empty() => Ok(None),
        Value::String(id) => Ok(Some(id.clone())),
        Value::Array(_) | Value::Object(_) => Err(err("Invalid native tool result identity")),
        other if truthy(other) => Ok(Some(FOREIGN_CALL.to_owned())),
        _ => Ok(None),
    }
}

fn same_segment(left: &Seg, right: &Seg) -> bool {
    left.directory == right.directory
}

/// Retained per-segment inventory of one sweep (baseline locals of `observe`).
#[derive(Default)]
struct Known {
    known: BTreeMap<String, Seg>,
    tools: BTreeMap<String, String>,
    invocations: BTreeMap<String, BTreeSet<String>>,
    results: Vec<(Seg, Value)>,
    snapshots: BTreeMap<String, Vec<Seg>>,
    tool_gaps: BTreeSet<String>,
}

fn referenced_records(segment: &Seg, known: &mut Known) -> Result<BTreeSet<String>> {
    let directory = &segment.directory;
    let control = control_of(&*segment.cursor()?);
    let mut referenced = BTreeSet::new();
    for (_, record) in get(&control, "pure_entries")
        .as_object()
        .unwrap_or_default()
    {
        referenced.insert(text(field(record, "snapshot_ref")?)?.to_owned());
    }
    for path in files(directory, "resolutions", Names::Dash36)? {
        referenced.insert(field_text(&read(&path, MAX_RECORD)?, "recovered_ref")?.to_owned());
    }
    for path in files(directory, "enrollments", Names::Hex64)? {
        referenced.insert(field_text(&read(&path, MAX_RECORD)?, "snapshot_ref")?.to_owned());
    }
    if segment.sealed()? {
        let manifest = manifest_value(directory, &*segment.cursor()?)?;
        for item in get(&manifest, "captures").as_array().unwrap_or_default() {
            referenced.insert(field_text(item, "ref")?.to_owned());
        }
        if let Some(admission) = get(&manifest, "admission_ref").as_str() {
            referenced.insert(admission.to_owned());
        }
        for old in manifest_gaps(&manifest)? {
            if let Some(call) = get(old, "native_entry_id")
                .as_str()
                .and_then(|n| n.strip_prefix("tool:"))
            {
                known.tool_gaps.insert(call.to_owned());
            }
        }
    }
    Ok(referenced)
}

fn retained_captures(
    segment: &Seg,
    known: &mut Known,
    referenced: &mut BTreeSet<String>,
) -> Result<()> {
    let lifecycle = segment.lifecycle()?;
    for path in files(&segment.directory, "captures", Names::Hex64)? {
        let captured = read(&path, MAX_RECORD)?;
        let native = field_text(&captured, "native_entry_id")?.to_owned();
        known.known.insert(native.clone(), Rc::clone(segment));
        let reference = field(&captured, "snapshot_ref")?;
        referenced.insert(text(reference)?.to_owned());
        let snapshot = read(&snapshot_path(&segment.directory, reference)?, MAX_PAYLOAD)?;
        if get(&snapshot, "role").as_str() == Some("assistant") {
            for block in get(&snapshot, "content").as_array().unwrap_or_default() {
                if get(block, "type").as_str() == Some("toolCall") {
                    let call = field_text(block, "id")?.to_owned();
                    known.tools.insert(call.clone(), lifecycle.clone());
                    known
                        .invocations
                        .entry(call)
                        .or_default()
                        .insert(native.clone());
                }
            }
        }
        if get(&snapshot, "role").as_str() == Some("toolResult")
            && truthy(get(&snapshot, "toolCallId"))
        {
            known.results.push((Rc::clone(segment), captured));
        }
    }
    Ok(())
}

fn orphan_snapshots(segment: &Seg, known: &mut Known, referenced: &BTreeSet<String>) -> Result<()> {
    for path in files(&segment.directory, "snapshots", Names::Hex64)? {
        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default()
            .to_owned();
        if referenced.contains(&format!("snapshots/{name}")) {
            continue;
        }
        // A sealed ordinary orphan may be unrecorded pure control; never
        // reopen its parent for later work.
        if segment.sealed()?
            && read(&path, MAX_PAYLOAD)?
                .get("instruction_context")
                .is_none()
        {
            continue;
        }
        known
            .snapshots
            .entry(name)
            .or_default()
            .push(Rc::clone(segment));
    }
    Ok(())
}

fn retained_gaps(segment: &Seg, known: &mut Known) -> Result<()> {
    for path in files(&segment.directory, "gaps", Names::Dash36)? {
        let old = read(&path, MAX_RECORD)?;
        match field(&old, "native_entry_id")? {
            Value::String(native) => {
                if let Some(call) = native.strip_prefix("tool:") {
                    known.tool_gaps.insert(call.to_owned());
                }
                if !native.is_empty() {
                    known
                        .known
                        .entry(native.clone())
                        .or_insert_with(|| Rc::clone(segment));
                }
            }
            Value::Null => {}
            _ => return Err(err("Invalid retained native gap identity")),
        }
    }
    Ok(())
}

fn mark_absent(segment: &Seg, branch_ids: &[String]) -> Result<()> {
    let mut absent = Vec::new();
    for path in files(&segment.directory, "captures", Names::Hex64)? {
        let native = field_text(&read(&path, MAX_RECORD)?, "native_entry_id")?.to_owned();
        if !branch_ids.contains(&native) {
            absent.push(native);
        }
    }
    let updated = {
        let mut cursor = segment.cursor_mut()?;
        let mut control = control_of(&cursor);
        set(&mut control, "absent_native_entries", strings(absent));
        set(&mut cursor, "control_interval", control);
        cursor.clone()
    };
    replace_cursor(&segment.directory.join("cursor.json"), &updated)
}

/// One ingest/seal sweep under the held barrier, transport lock and context.
struct Sweep<'s> {
    site: Site<'s>,
    session: &'s str,
    lifecycle: &'s str,
    receipt: Option<&'s Value>,
    input: &'s SweepInput,
    entries: Vec<Value>,
    trigger: String,
    current: Vec<Seg>,
    retained: Known,
    pure_known: BTreeMap<String, (PathBuf, Value, usize)>,
    raw_calls: BTreeMap<String, Vec<Value>>,
    raw_results: BTreeMap<String, usize>,
    active: Option<Seg>,
}

impl<'s> Sweep<'s> {
    fn new(
        site: Site<'s>,
        session: &'s str,
        lifecycle: &'s str,
        receipt: Option<&'s Value>,
        input: &'s SweepInput,
    ) -> Result<Self> {
        let mut current = Vec::new();
        for segment in segments(&site, session)? {
            if segment.bound()? {
                current.push(segment);
            }
        }
        let mut raw_calls: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let mut raw_results: BTreeMap<String, usize> = BTreeMap::new();
        for candidate in &input.entries {
            let message = get(candidate, "message");
            if candidate.as_object().is_none() || message.as_object().is_none() {
                continue;
            }
            match (get(message, "role").as_str(), get(message, "content")) {
                (Some("assistant"), Value::Array(blocks)) => {
                    for block in blocks {
                        if get(block, "type").as_str() == Some("toolCall") {
                            if let Some(id) = get(block, "id").as_str() {
                                raw_calls
                                    .entry(id.to_owned())
                                    .or_default()
                                    .push(candidate.clone());
                            }
                        }
                    }
                }
                (Some("toolResult"), _) => {
                    if let Some(id) = get(message, "toolCallId").as_str() {
                        *raw_results.entry(id.to_owned()).or_default() += 1;
                    }
                }
                _ => {}
            }
        }
        Ok(Self {
            site,
            session,
            lifecycle,
            receipt,
            input,
            entries: input.entries.clone(),
            trigger: input.trigger.clone(),
            current,
            retained: Known::default(),
            pure_known: BTreeMap::new(),
            raw_calls,
            raw_results,
            active: None,
        })
    }

    fn inventory(&mut self) -> Result<()> {
        let segments = self.current.clone();
        for segment in &segments {
            if !segment.bound()? {
                continue;
            }
            let mut referenced = referenced_records(segment, &mut self.retained)?;
            let lifecycles = call_lifecycles(segment)?;
            for (call, owner) in lifecycles {
                self.retained.tools.insert(call, text(&owner)?.to_owned());
            }
            retained_captures(segment, &mut self.retained, &mut referenced)?;
            orphan_snapshots(segment, &mut self.retained, &referenced)?;
            retained_gaps(segment, &mut self.retained)?;
            if !segment.sealed()? {
                mark_absent(segment, &self.input.branch_ids)?;
            }
        }
        let results = std::mem::take(&mut self.retained.results);
        for (owner, captured) in &results {
            if resolve_tool_result(&self.current, owner, captured, &self.retained.invocations)? {
                let result = read(
                    &snapshot_path(&owner.directory, field(captured, "snapshot_ref")?)?,
                    MAX_PAYLOAD,
                )?;
                self.retained
                    .tools
                    .remove(field_text(&result, "toolCallId")?);
            }
        }
        self.active = active_segment(&self.current, Some(&self.input.branch_ids))?;
        let mut order = 0;
        for segment in &self.current {
            let control = control_of(&*segment.cursor()?);
            for (native, record) in get(&control, "pure_entries")
                .as_object()
                .unwrap_or_default()
            {
                if !self.retained.known.contains_key(native) {
                    // A repeated key keeps its first position (dict assignment).
                    let position = self.pure_known.get(native).map_or(order, |(_, _, p)| *p);
                    self.pure_known.insert(
                        native.clone(),
                        (segment.directory.clone(), record.clone(), position),
                    );
                    order += 1;
                }
            }
        }
        Ok(())
    }

    fn fresh(&self) -> Vec<Value> {
        self.entries
            .iter()
            .filter(|entry| {
                let id = get(entry, "id").as_str().unwrap_or_default();
                let kind = get(entry, "type").as_str();
                entry.as_object().is_some()
                    && !self.retained.known.contains_key(id)
                    && !self.pure_known.contains_key(id)
                    && !kind.is_some_and(|k| NONLEARNING.contains(&k))
            })
            .cloned()
            .collect()
    }

    /// The reserved hidden control interval classification.
    fn classify(&mut self) -> Result<()> {
        let fresh = self.fresh();
        let Some(active) = self.active.clone() else {
            return Ok(());
        };
        if fresh.is_empty() {
            return Ok(());
        }
        let mut control = control_of(&*active.cursor()?);
        let guidance = get(&control, "generated_guidance")
            .as_str()
            .map(str::to_owned);
        let stored_user = fresh.iter().any(|entry| {
            let message = get(entry, "message");
            get(entry, "type").as_str() == Some("message")
                && get(message, "role").as_str() == Some("user")
                && get(message, "attribution").as_str() == Some("user")
        });
        if stored_user {
            if let Some(guidance) = guidance.as_deref().filter(|g| !g.is_empty()) {
                let echoed = fresh.iter().any(|entry| {
                    get(get(&omp_projection::project(entry), "snapshot"), "content").as_str()
                        == Some(guidance)
                });
                if echoed {
                    self.trigger = "human-with-own-guidance".to_owned();
                }
            }
        }
        let continuing = truthy(get(&control, "pure_mode"));
        let tool_ids: Vec<String> = get(&control, "pure_tool_ids")
            .as_array()
            .unwrap_or_default()
            .iter()
            .filter_map(|id| id.as_str().map(str::to_owned))
            .collect();
        let classified =
            omp_control::pure_batch(&fresh, guidance.as_deref(), continuing, &tool_ids);
        if let Some((projected, pure_tools)) = classified {
            let mut records = get(&control, "pure_entries").clone();
            if records.as_object().is_none() {
                records = Value::Object(Vec::new());
            }
            let mut finished_calls = Vec::new();
            for (entry, snapshot) in &projected {
                let native = field_text(entry, "id")?.to_owned();
                let reference = snapshot_reference(&native);
                let hash = retain(&active.directory.join(&reference), snapshot, MAX_PAYLOAD)?;
                let record = object([
                    ("snapshot_ref", string(reference)),
                    ("snapshot_sha256", string(hash)),
                ]);
                set(&mut records, &native, record.clone());
                let order = self.pure_known.len();
                self.pure_known
                    .insert(native, (active.directory.clone(), record, order));
                if get(snapshot, "role").as_str() == Some("toolResult") {
                    if let Some(call) = get(snapshot, "toolCallId").as_str() {
                        finished_calls.push(call.to_owned());
                    }
                }
            }
            set(&mut control, "pure_entries", records);
            set(&mut control, "pure_mode", Value::Bool(true));
            set(&mut control, "pure_tool_ids", strings(pure_tools));
            set(&mut control, "native_trigger", string("own-hidden-pure"));
            let updated = {
                let mut cursor = active.cursor_mut()?;
                if let Some(tools) = cursor.get_mut("tool_lifecycles") {
                    for call in &finished_calls {
                        remove(tools, call);
                    }
                }
                set(&mut cursor, "control_interval", control);
                cursor.clone()
            };
            replace_cursor(&active.directory.join("cursor.json"), &updated)?;
        } else if truthy(get(&control, "pure_mode")) {
            set(&mut control, "pure_mode", Value::Bool(false));
            set(
                &mut control,
                "native_trigger",
                string("own-hidden-ambiguous"),
            );
            let updated = {
                let mut cursor = active.cursor_mut()?;
                set(&mut cursor, "control_interval", control);
                cursor.clone()
            };
            replace_cursor(&active.directory.join("cursor.json"), &updated)?;
            // Reclassification retains the reserved control context in the
            // linked substantive successor, never reopens its parent.
            self.inherit_control()?;
        }
        Ok(())
    }

    fn inherit_control(&mut self) -> Result<()> {
        let mut ordered: Vec<(&String, &(PathBuf, Value, usize))> =
            self.pure_known.iter().collect();
        ordered.sort_by_key(|(_, (_, _, order))| *order);
        let mut inherited = Vec::new();
        for (native, (directory, record, _)) in ordered {
            let snapshot = read(
                &snapshot_path(directory, field(record, "snapshot_ref")?)?,
                MAX_PAYLOAD,
            )?;
            if field(&snapshot, "carrier")?.as_str() == Some("custom_message") {
                inherited.push(object([
                    ("type", string("custom_message")),
                    ("id", string(native)),
                    ("parentId", field(&snapshot, "native_parent_id")?.clone()),
                    ("customType", field(&snapshot, "customType")?.clone()),
                    ("content", field(&snapshot, "content")?.clone()),
                    ("attribution", get(&snapshot, "attribution").clone()),
                ]));
            }
        }
        for item in &inherited {
            self.pure_known
                .remove(get(item, "id").as_str().unwrap_or_default());
        }
        inherited.append(&mut self.entries);
        self.entries = inherited;
        Ok(())
    }

    fn new_segment(&self, lifecycle: &str, root: &str, trigger: &str, ordinal: u64) -> Result<Seg> {
        new_segment(&self.site, self.session, lifecycle, root, trigger, ordinal)
    }
    fn next_ordinal(&self) -> Result<u64> {
        max_ordinal(&self.current, self.input.event_ordinal)
    }
    fn resume(&mut self) -> Result<Seg> {
        let active = match self.active.clone() {
            Some(active) => active,
            None => {
                let resumed = self.new_segment(
                    self.lifecycle,
                    self.lifecycle,
                    "resume",
                    self.input.event_ordinal,
                )?;
                self.current.push(Rc::clone(&resumed));
                resumed
            }
        };
        writable_successor(
            &self.site,
            self.session,
            active,
            &mut self.current,
            self.input.event_ordinal,
        )
    }
    fn writable(&mut self, owner: Seg) -> Result<Seg> {
        writable_successor(
            &self.site,
            self.session,
            owner,
            &mut self.current,
            self.input.event_ordinal,
        )
    }

    fn sweep(&mut self) -> Result<()> {
        let mut deferred: BTreeMap<String, Vec<Value>> = BTreeMap::new();
        let mut position = 0;
        while let Some(entry) = self.entries.get(position).cloned() {
            self.entry(position, &entry, &mut deferred)?;
            position += 1;
        }
        if get(&self.input.view, "tail_status").as_str() == Some("active-unavailable") {
            if self.active.is_none() {
                self.active = Some(self.new_segment(
                    self.lifecycle,
                    self.lifecycle,
                    "resume",
                    self.input.event_ordinal,
                )?);
            }
            let active = self.resume()?;
            segment_gap(&active, None, "ambiguous-lifecycle")?;
            self.active = Some(active);
        }
        Ok(())
    }

    fn following_user(&self, position: usize) -> Option<String> {
        for later in self.entries.iter().skip(position + 1) {
            let message = get(later, "message");
            match get(message, "role").as_str() {
                Some("assistant" | "toolResult") => return None,
                Some("user") if get(message, "attribution").as_str() == Some("user") => {
                    return get(later, "id").as_str().map(str::to_owned);
                }
                _ => {}
            }
        }
        None
    }

    fn tool_owner(&self, call_id: Option<&str>) -> Option<Seg> {
        let call_id = call_id?;
        let lifecycle = self.retained.tools.get(call_id)?;
        self.current
            .iter()
            .find(|item| item.lifecycle().ok().as_deref() == Some(lifecycle.as_str()))
            .cloned()
    }

    fn call_roots(&self, call_id: &str) -> Result<usize> {
        let mut roots = BTreeSet::new();
        for segment in &self.current {
            if call_lifecycles(segment)?.contains_key(call_id) {
                roots.insert(segment.root()?);
            }
        }
        Ok(roots.len())
    }

    fn recover_known(&self, native: &str, entry: &Value) -> Result<()> {
        let Some(segment) = self.retained.known.get(native) else {
            return Ok(());
        };
        // Recover original missing evidence, never mutate an existing safe snapshot.
        let name = sha_text(native);
        if lexists(
            &segment
                .directory
                .join("captures")
                .join(format!("{name}.json")),
        )? {
            return Ok(());
        }
        if segment.sealed()? {
            let mut recoverable = false;
            for path in files(&segment.directory, "gaps", Names::Dash36)? {
                let old = read(&path, MAX_RECORD)?;
                recoverable |= field(&old, "native_entry_id")?.as_str() == Some(native)
                    && field(&old, "reason")?
                        .as_str()
                        .is_some_and(|r| super::RESOLVABLE.contains(&r));
            }
            if !recoverable {
                return Ok(());
            }
        }
        let prior = segment
            .directory
            .join("snapshots")
            .join(format!("{name}.json"));
        if lexists(&prior)?
            && read(&prior, MAX_PAYLOAD)?
                .get("instruction_context")
                .is_some()
        {
            return Err(err(
                "Instruction context conflicts with retained native gap",
            ));
        }
        let mapping = if self.input.branch_ids.iter().any(|id| id == native) {
            "verified"
        } else {
            "absent"
        };
        capture(
            &segment.directory,
            &*segment.cursor()?,
            entry,
            mapping,
            None,
            false,
        )?;
        Ok(())
    }

    #[allow(clippy::too_many_lines)]
    fn entry(
        &mut self,
        position: usize,
        entry: &Value,
        deferred: &mut BTreeMap<String, Vec<Value>>,
    ) -> Result<()> {
        if entry.as_object().is_none() {
            return Err(err("Malformed native stored carrier"));
        }
        let native = turn_sources::bounded(get(entry, "id"), 512)?.to_owned();
        let inherited = self.input.inherited_ids.contains(&native);
        if inherited && !self.retained.known.contains_key(&native) {
            return Ok(());
        }
        if self.pure_known.contains_key(&native) {
            return Ok(());
        }
        if get(get(entry, "agent"), "kind").as_str() == Some("sub")
            && get(entry, "agent").as_object().is_some()
        {
            return Ok(());
        }
        let kind = get(entry, "type").as_str();
        if kind.is_some_and(|k| NONLEARNING.contains(&k)) {
            return Ok(());
        }
        if !matches!(kind, Some("message" | "custom_message")) {
            let active = self.resume()?;
            segment_gap(&active, Some(&native), "missing-stored-source")?;
            self.active = Some(active);
            return Ok(());
        }
        let empty = Value::Object(Vec::new());
        let message = match get(entry, "message") {
            message @ Value::Object(_) => message,
            _ => &empty,
        };
        let stored_user = kind == Some("message")
            && get(message, "role").as_str() == Some("user")
            && get(message, "attribution").as_str() == Some("user");
        let fork_unavailable = get(&self.input.view, "fork_status").as_str() == Some("unavailable");
        let human = stored_user && !fork_unavailable;
        if self.retained.known.contains_key(&native) {
            return self.recover_known(&native, entry);
        }
        let mut mention_owner = None;
        let mut ambiguous_mention = false;
        if get(message, "role").as_str() == Some("fileMention") {
            let following = self.following_user(position);
            match following {
                Some(user) if !self.retained.known.contains_key(&user) => {
                    deferred.entry(user).or_default().push(entry.clone());
                    return Ok(());
                }
                Some(user) => mention_owner = self.retained.known.get(&user).cloned(),
                None => {
                    ambiguous_mention = match &self.active {
                        None => true,
                        Some(active) => active.sealed()?,
                    };
                }
            }
        }
        let call_id = call_key(get(message, "toolCallId"))?;
        let mut tool_owner = mention_owner
            .clone()
            .or_else(|| self.tool_owner(call_id.as_deref()));
        let ambiguous_call = match &call_id {
            None => false,
            Some(call) => {
                tool_owner.is_none()
                    || self.retained.invocations.get(call).map_or(0, BTreeSet::len) > 1
                    || self.call_roots(call)? > 1
            }
        };
        if ambiguous_call {
            tool_owner = None;
        }
        let mut invocation_owners = BTreeSet::new();
        for block in get(message, "content").as_array().unwrap_or_default() {
            if get(block, "type").as_str() == Some("toolCall") {
                if let Some(owner) = get(block, "id")
                    .as_str()
                    .and_then(|id| self.retained.tools.get(id))
                {
                    invocation_owners.insert(owner.clone());
                }
            }
        }
        if tool_owner.is_none() && invocation_owners.len() == 1 {
            tool_owner = self
                .current
                .iter()
                .find(|item| {
                    item.lifecycle()
                        .is_ok_and(|lifecycle| invocation_owners.contains(&lifecycle))
                })
                .cloned();
        }
        if human && !inherited {
            if let Some(active) = &self.active {
                if !active.sealed()? {
                    seal_segment(active)?;
                }
            }
            let root = self
                .site
                .context
                .map(|context| context.root().to_path_buf())
                .ok_or_else(|| err("Native observer requires its live enrollment context"))?;
            let root = root
                .to_str()
                .ok_or_else(|| err("Invalid native repository path"))?;
            let lifecycle = uuid5_url(&format!("{root}:omp:{}:{native}", self.session));
            let trigger = if matches!(
                self.trigger.as_str(),
                "own-hidden-combined" | "human-with-own-guidance"
            ) {
                "human-with-own-guidance"
            } else {
                "human"
            };
            let ordinal = self.next_ordinal()?;
            self.active = Some(self.new_segment(&lifecycle, &lifecycle, trigger, ordinal)?);
        } else if let Some(active) = self.active.clone().filter(|_| tool_owner.is_none()) {
            if active.sealed()? {
                let root = active.root()?;
                let trigger = if self.trigger.starts_with("own-") {
                    "own-hidden-ambiguous"
                } else {
                    "foreign-hidden"
                };
                let ordinal = self.next_ordinal()?;
                self.active =
                    Some(self.new_segment(&crate::host::uuid4()?, &root, trigger, ordinal)?);
            }
        } else if self.active.is_none() && tool_owner.is_none() {
            // Context inherited on a fork is not a new human admission.
            if inherited {
                return Ok(());
            }
            let resumed = self.new_segment(
                self.lifecycle,
                self.lifecycle,
                "resume",
                self.input.event_ordinal,
            )?;
            let active = self.writable(resumed)?;
            segment_gap(&active, Some(&native), "ambiguous-lifecycle")?;
            self.active = Some(active);
        }
        let mut owner = match tool_owner.clone().or_else(|| self.active.clone()) {
            Some(owner) => owner,
            None => return Err(err("Native source has no owning lifecycle")),
        };
        let sealed_tool = match &tool_owner {
            Some(tool) if tool.sealed()? => Some(Rc::clone(tool)),
            _ => None,
        };
        if let Some(tool) = sealed_tool {
            let root = tool.root()?;
            let mut open = None;
            for item in &self.current {
                if item.root()? == root && !item.sealed()? {
                    open = Some(Rc::clone(item));
                    break;
                }
            }
            owner = match open {
                Some(open) => open,
                None => {
                    let ordinal = self.next_ordinal()?;
                    self.new_segment(&crate::host::uuid4()?, &root, "foreign-hidden", ordinal)?
                }
            };
        }
        let snapshot_name = format!("{}.json", sha_text(&native));
        let existing = self
            .retained
            .snapshots
            .get(&snapshot_name)
            .cloned()
            .unwrap_or_default();
        if existing.len() > 1 {
            return Err(err("Ambiguous retained native snapshot"));
        }
        if let Some(first) = existing.first() {
            owner = Rc::clone(first);
        }
        if stored_user && fork_unavailable {
            if owner.sealed()? {
                owner = self.writable(owner)?;
            }
            segment_gap(&owner, Some(&native), "ambiguous-lifecycle")?;
        }
        if (ambiguous_mention || ambiguous_call) && existing.is_empty() {
            segment_gap(&owner, Some(&native), "ambiguous-lifecycle")?;
        }
        if stored_user {
            if let Some(mentions) = deferred.remove(&native) {
                self.capture_mentions(&owner, &mentions)?;
            }
        }
        let manual = self.manual_read(
            entry,
            message,
            &native,
            call_id.as_deref(),
            !existing.is_empty() || ambiguous_call,
        )?;
        let manual_read = manual.as_ref().map(|(invocation, witness)| ManualRead {
            invocation,
            witness,
        });
        let mapping = if self.input.branch_ids.contains(&native) {
            "verified"
        } else {
            "absent"
        };
        let persisted = get(&self.input.view, "source_kind").as_str() == Some("retired-file");
        let captured = capture(
            &owner.directory,
            &*owner.cursor()?,
            entry,
            mapping,
            manual_read.as_ref(),
            persisted,
        )?;
        self.record_capture(&owner, &native, human && !inherited, message)?;
        if let (Some(call), Some(captured)) = (&call_id, &captured) {
            if !ambiguous_call {
                let mut resolution = self.current.clone();
                if !resolution.iter().any(|item| same_segment(item, &owner)) {
                    resolution.push(Rc::clone(&owner));
                }
                if resolve_tool_result(&resolution, &owner, captured, &self.retained.invocations)? {
                    self.retained.tools.remove(call);
                }
            }
        }
        let updated = owner.snapshot()?;
        cursor_value(&updated, &string(self.session), &owner.lifecycle()?)?;
        replace_cursor(&owner.directory.join("cursor.json"), &updated)?;
        if human && !inherited {
            enroll_admission(&owner.directory, &updated, captured.as_ref())?;
        }
        self.retained.known.insert(native, Rc::clone(&owner));
        if !self.current.iter().any(|item| same_segment(item, &owner)) {
            self.current.push(owner);
        }
        Ok(())
    }

    fn capture_mentions(&mut self, owner: &Seg, mentions: &[Value]) -> Result<()> {
        let mut control = control_of(&*owner.cursor()?);
        let mut order: Vec<Value> = get(&control, "capture_order")
            .as_array()
            .map(<[Value]>::to_vec)
            .unwrap_or_default();
        for mention in mentions {
            capture(
                &owner.directory,
                &*owner.cursor()?,
                mention,
                "verified",
                None,
                false,
            )?;
            let mention_id = field(mention, "id")?.clone();
            if !order.contains(&mention_id) {
                order.push(mention_id.clone());
            }
            self.retained
                .known
                .insert(text(&mention_id)?.to_owned(), Rc::clone(owner));
        }
        set(&mut control, "capture_order", Value::Array(order));
        let mut cursor = owner.cursor_mut()?;
        set(&mut cursor, "control_interval", control);
        Ok(())
    }

    fn record_capture(
        &mut self,
        owner: &Seg,
        native: &str,
        human: bool,
        message: &Value,
    ) -> Result<()> {
        let lifecycle = owner.lifecycle()?;
        let mut control = control_of(&*owner.cursor()?);
        let mut order: Vec<Value> = get(&control, "capture_order")
            .as_array()
            .map(<[Value]>::to_vec)
            .unwrap_or_default();
        let native_value = string(native);
        if !order.contains(&native_value) {
            order.push(native_value);
        }
        if order.len() > MAX_CAPTURES {
            segment_gap(owner, Some(native), "oversized-source")?;
            return Err(err("Native capture capacity reached"));
        }
        set(&mut control, "capture_order", Value::Array(order));
        if human {
            set(&mut control, "human_entry", string(native));
        }
        let mut calls = Vec::new();
        if get(message, "role").as_str() == Some("assistant") {
            for block in get(message, "content").as_array().unwrap_or_default() {
                if block.as_object().is_some() && get(block, "type").as_str() == Some("toolCall") {
                    calls.push(turn_sources::bounded(get(block, "id"), 512)?.to_owned());
                }
            }
        }
        {
            let mut cursor = owner.cursor_mut()?;
            set(&mut cursor, "control_interval", control);
            set(&mut cursor, "last_native_entry", string(native));
            let mut tools = get(&cursor, "tool_lifecycles").clone();
            for call in &calls {
                set(&mut tools, call, string(&lifecycle));
            }
            set(&mut cursor, "tool_lifecycles", tools);
        }
        for call in calls {
            self.retained.tools.insert(call.clone(), lifecycle.clone());
            self.retained
                .invocations
                .entry(call)
                .or_default()
                .insert(native.to_owned());
        }
        Ok(())
    }

    /// The exact native full read of an installed manual and its witness.
    fn manual_read(
        &self,
        _entry: &Value,
        message: &Value,
        native: &str,
        call_id: Option<&str>,
        excluded: bool,
    ) -> Result<Option<(Value, omp_installation::OwnedManual)>> {
        let (Some(receipt), Some(call_id)) = (self.receipt, call_id) else {
            return Ok(None);
        };
        let candidates = self.raw_calls.get(call_id).cloned().unwrap_or_default();
        let [candidate] = candidates.as_slice() else {
            return Ok(None);
        };
        let candidate_id = get(candidate, "id").as_str().unwrap_or_default();
        let read_call = |block: &&Value| {
            get(block, "type").as_str() == Some("toolCall")
                && get(block, "id").as_str() == Some(call_id)
                && get(block, "name").as_str() == Some("read")
                && get(block, "arguments").as_object().is_some()
        };
        let blocks = get(get(candidate, "message"), "content")
            .as_array()
            .unwrap_or_default();
        let call = blocks.iter().find(read_call);
        let invoked = self.retained.invocations.get(call_id);
        let eligible = !excluded
            && get(message, "role").as_str() == Some("toolResult")
            && get(message, "toolName").as_str() == Some("read")
            && get(message, "isError") == &Value::Bool(false)
            && self.raw_results.get(call_id) == Some(&1)
            && call.is_some()
            && !self.retained.tool_gaps.contains(call_id)
            && invoked.is_some_and(|set| set.len() == 1 && set.contains(candidate_id))
            && self.retained.known.contains_key(candidate_id)
            && !retained_gap(&self.current, &[native, &format!("tool:{call_id}")])?;
        let (true, Some(call)) = (eligible, call) else {
            return Ok(None);
        };
        let Some(owner) = self.retained.known.get(candidate_id) else {
            return Ok(None);
        };
        let name = format!("{}.json", sha_text(candidate_id));
        let invocation_path = owner.directory.join("snapshots").join(&name);
        let invocation_record = owner.directory.join("captures").join(&name);
        let projected = omp_projection::project(candidate);
        let Some(projected) = projected.get("snapshot") else {
            return Ok(None);
        };
        if !lexists(&invocation_record)? || !lexists(&invocation_path)? {
            return Ok(None);
        }
        let retained = read(&invocation_record, MAX_RECORD)?;
        if get(&retained, "native_entry_id").as_str() != Some(candidate_id)
            || get(&retained, "snapshot_ref").as_str() != Some(format!("snapshots/{name}").as_str())
            || get(&retained, "snapshot_sha256").as_str() != Some(digest(projected)?.as_str())
            || !same(projected, &read(&invocation_path, MAX_PAYLOAD)?)
        {
            return Ok(None);
        }
        let Some(relative) =
            omp_projection::instruction_relative(get(get(call, "arguments"), "path"))
        else {
            return Ok(None);
        };
        let witness = match omp_installation::owned_manual(receipt, &relative) {
            Ok(witness) => witness,
            Err(omp_installation::Error::Contract(error)) => return Err(error),
            Err(omp_installation::Error::MissingActivation) => {
                return Err(crate::error::ContractError::missing_provider(
                    omp_installation::Error::MissingActivation.to_string(),
                ));
            }
        };
        Ok(witness.map(|witness| (candidate.clone(), witness)))
    }

    fn finish(&mut self, action: &str) -> Result<()> {
        if let Some(active) = &self.active {
            let genuine =
                get(get(&*active.cursor()?, "origin"), "kind").as_str() == Some("genuine-user");
            if (action == "seal" && genuine) || self.input.terminal {
                seal_segment(active)?;
            }
        }
        if self.input.terminal {
            for segment in segments(&self.site, self.session)? {
                if segment.bound()? && !segment.sealed()? {
                    seal_segment(&segment)?;
                }
            }
        }
        Ok(())
    }
}

/// Exit checks and post-sweep result of one ingest/seal (baseline tail of `observe`).
struct Finished {
    result: Value,
    latest: Option<String>,
    root: Option<Value>,
}

#[allow(clippy::too_many_arguments)]
fn sweep_result(
    site: &Site<'_>,
    request: &Request,
    input: &SweepInput,
    entries: &[Value],
    fences: &[Value],
    call: &Observation<'_, '_>,
    context: &PrivateEnrollmentContext,
) -> Result<Finished> {
    let current = segments(site, &request.session)?;
    let (rows, faults) = pending_inventory(site)?;
    let chosen = active_segment(&current, Some(&input.branch_ids))?;
    let latest = match &chosen {
        Some(segment) => Some(segment.snapshot()?),
        None => None,
    };
    let root = latest
        .as_ref()
        .map(|c| get(get(c, "origin"), "human_root").clone());
    let rows_of_root = root_rows(&rows, &request.session, root.as_ref());
    let (payload_ids, references, source_complete) =
        checked_payload_coverage(&current, entries, &input.view, &rows, &faults)?;
    let retired = get(&input.view, "source_kind").as_str() == Some("retired-file");
    if let Some(identity) = call
        .identity
        .filter(|i| i.as_object().is_some() && !fences.is_empty())
    {
        let mut native_ids: Vec<String> = get(&input.view, "native_parents")
            .as_array()
            .unwrap_or_default()
            .iter()
            .filter_map(|item| get(item, "id").as_str().map(str::to_owned))
            .collect();
        native_ids.sort();
        let view_id = field_text(identity, "view_id")?;
        let token = if retired
            && source_complete
            && entries.iter().all(|e| {
                get(e, "type")
                    .as_str()
                    .is_some_and(|k| NONLEARNING.contains(&k))
            }) {
            Some(omp_stream::nonlearning_tail_evidence(
                &request.session,
                view_id,
                &native_ids,
            )?)
        } else {
            None
        };
        let tool_ids: Vec<String> = field(identity, "tool_ids")?
            .as_array()
            .unwrap_or_default()
            .iter()
            .map(|id| Ok(text(id)?.to_owned()))
            .collect::<Result<_>>()?;
        let references_value = Value::Array(references.clone());
        let authority = Installed::new(site.runtime_root);
        scope_binding::fresh_enrollment_locations(|| {
            omp_stream::resolve_recovery(
                &authority,
                site.repo,
                fences,
                RecoveryEvidence {
                    view_id,
                    native_ids: &native_ids,
                    tool_ids: &tool_ids,
                    payload_native_ids: &payload_ids,
                    source_references: &references_value,
                    complete: retired && source_complete,
                    leaf_id: get(&input.view, "leaf_id").as_str(),
                    nonlearning_evidence: token.as_ref(),
                },
            )
        })?;
    }
    let status = if rows_of_root.is_empty() && faults.is_empty() {
        "observed"
    } else {
        "pending"
    };
    let latest_lifecycle = latest
        .as_ref()
        .map(|c| get(c, "lifecycle").clone())
        .unwrap_or_else(|| string(&request.lifecycle));
    let result = object([
        ("verified", Value::Bool(true)),
        ("status", string(status)),
        ("session", string(&request.session)),
        ("lifecycle", latest_lifecycle),
        ("wake_payload", wake_payload(&rows_of_root)),
        ("faults", strings(faults.iter().cloned())),
        ("held", Value::Bool(context.held())),
        ("canonical_writes_performed", Value::Bool(false)),
        ("source_references", Value::Array(references)),
        (
            "source_coverage_complete",
            Value::Bool(retired && source_complete),
        ),
    ]);
    Ok(Finished {
        result,
        latest: latest
            .as_ref()
            .and_then(|c| get(c, "lifecycle").as_str().map(str::to_owned)),
        root,
    })
}

fn merge(result: &mut Value, other: Value) {
    for (key, item) in other.as_object().unwrap_or_default() {
        set(result, key, item.clone());
    }
}

struct Locks<'l> {
    source: &'l SourceBarrier,
    transport: &'l crate::fs::PrivateLock,
    base: &'l Path,
}

#[allow(clippy::too_many_arguments)]
fn observed(
    repo: &Path,
    runtime_root: &Path,
    value: &Value,
    request: &Request,
    call: &mut Observation<'_, '_>,
    context: &PrivateEnrollmentContext,
    locks: &Locks<'_>,
) -> Result<Value> {
    let site = Site::new(repo, runtime_root).with(context);
    if let Some(input) = &request.sweep {
        scope_binding::enrollment_read_context(repo, context)?;
        if input.root != context.root() {
            return Err(err("Checked native source belongs to another bank scope"));
        }
        scope_binding::observe_enrollment_locations(context, || {
            scope_binding::observe_enrollment_location(repo, Some(context), runtime_root)
                .map(|_| ())
        })?;
    }
    let fences = if request.action == "ingest" {
        omp_stream::retire_for_recovery(
            &Installed::new(runtime_root),
            repo,
            &request.session,
            call.instance_id,
        )?
    } else {
        Vec::new()
    };
    if let Some(started) = call.on_started.as_mut() {
        started()?;
    }
    if let Some(completion) = call.source_completion {
        let view = request.sweep.as_ref().map(|input| &input.view);
        completion.validate(value, view.unwrap_or(&Value::Null))?;
    }
    scope_binding::observe_enrollment_locations(context, || {
        let Some(input) = &request.sweep else {
            let mut result = object([("verified", Value::Bool(true))]);
            merge(
                &mut result,
                wake(
                    &site,
                    &request.session,
                    &request.lifecycle,
                    &request.payload,
                    call.delivery_window.as_ref(),
                    None,
                )?,
            );
            return Ok(result);
        };
        let mut sweep = Sweep::new(
            site,
            &request.session,
            &request.lifecycle,
            call.receipt,
            input,
        )?;
        sweep.inventory()?;
        sweep.classify()?;
        sweep.sweep()?;
        sweep.finish(&request.action)?;
        let entries = std::mem::take(&mut sweep.entries);
        drop(sweep);
        scope_binding::recheck_private_enrollment(context)?;
        #[cfg(feature = "dev-tools")]
        crate::bank_writes::fault("omp.checkpoint.before-recheck")?;
        if call.source_completion.is_some() {
            // Same checks as the actual successful context exits, against the
            // actual executor-held source barrier and this transport lock.
            turn_sources::check_source_compatibility(locks.base)?;
            locks.source.recheck()?;
            locks.transport.recheck()?;
        } else {
            drain_enrollments(&site)?;
        }
        let finished = sweep_result(&site, request, input, &entries, &fences, call, context)?;
        let mut result = finished.result;
        let payload = match call.source_completion {
            Some(completion) => {
                completion.commit_source(&context_value(context)?, &result)?;
                // The historical result is not current wake authority. Shared
                // sources may change under another harness's shared barrier.
                let (rows, _) = pending_inventory(&site)?;
                wake_payload(&root_rows(&rows, &request.session, finished.root.as_ref()))
            }
            None => get(&result, "wake_payload").clone(),
        };
        let Some(latest) = finished
            .latest
            .filter(|_| request.action == "seal" && input.request_wake)
        else {
            return Ok(result);
        };
        let window = call.delivery_window.as_ref();
        match call.source_completion {
            Some(completion) => {
                let woken = wake(
                    &site,
                    &request.session,
                    &latest,
                    &payload,
                    window,
                    Some(completion),
                )?;
                merge(&mut result, woken);
            }
            None => match wake(&site, &request.session, &latest, &payload, window, None) {
                Ok(woken) => merge(&mut result, woken),
                // The source transaction is verified independently of wake preparation.
                Err(_) => merge(
                    &mut result,
                    object([
                        ("continue", Value::Bool(false)),
                        ("status", string("pending-wake-unavailable")),
                    ]),
                ),
            },
        }
        Ok(result)
    })
}

/// Baseline `observe(repo, value, receipt=…, on_started=…, delivery_window=…,
/// verified_binding=…, source_completion=…, held_source=…)`.
pub fn observe(
    repo: &Path,
    runtime_root: &Path,
    value: &Value,
    mut call: Observation<'_, '_>,
    scope: &dyn ObserverScope,
) -> Result<Value> {
    let request = request(repo, value)?;
    let owned;
    let source = match call.held_source {
        Some(held) => {
            held.recheck()?;
            held
        }
        None => {
            owned = turn_sources::source_barrier(repo, false, false, None, runtime_root)?;
            &owned
        }
    };
    let base = crate::omp_boundary::base(repo)?;
    let transport = turn_sources::private_lock(&base.join("omp-transport.lock"), false, false)?;
    let locks = Locks {
        source,
        transport: &transport,
        base: &base,
    };
    let mut output = None;
    let held = call.held_source;
    scope.observe(repo, held, &mut |context| {
        output = Some(observed(
            repo,
            runtime_root,
            value,
            &request,
            &mut call,
            context,
            &locks,
        )?);
        Ok(())
    })?;
    transport.finish()?;
    source.recheck()?;
    output.ok_or_else(|| err("Native observer scope did not run its operation"))
}

/// Baseline `_drain_after_response`: one oldest-eight batch with its own
/// fresh guard, never a finite retry. Failures stay durable pending work.
pub(crate) fn drain_after_response(repo: &Path, runtime_root: &Path, scope: &dyn ObserverScope) {
    let _ = super::inventory::transport_guard(repo, runtime_root, || {
        scope.observe(repo, None, &mut |context| {
            scope_binding::observe_enrollment_locations(context, || {
                drain_enrollments(&Site::new(repo, runtime_root).with(context))
            })
        })
    });
}
