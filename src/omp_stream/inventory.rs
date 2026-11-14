//! Complete sibling inventory, drain validation, retirement and exact recovery.
use super::coverage::{StreamAuthority, barrier, source_refs};
use super::{
    ACTIONS, MAX_ENTRIES, MAX_METADATA, NonlearningTailEvidence, Pattern, REFUSAL_CODES, digest,
    directory, entries, err, exact, hint_identity, identity, ids, installation_id, int, integer,
    io, is_hint, lexists, name, object, opt_string, read_bytes_required, read_required, retain,
    same, sequence_of, session_instances, session_sha256, sha, string, uuid, with_lock,
};
use crate::codec::json::Value;
use crate::error::Result;
use crate::fs::{self, DirectoryChain};
use crate::lifecycle::get;
use crate::turn_sources;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

const OPEN_FIELDS: [&str; 11] = [
    "schema",
    "project_id",
    "repo_root",
    "bank_path",
    "checkout_key",
    "session",
    "instance_id",
    "initial_lifecycle",
    "artifact_digest",
    "installation_id",
    "opened_at",
];
const ATTEMPT_FIELDS: [&str; 11] = [
    "schema",
    "open_sha256",
    "instance_id",
    "sequence",
    "action",
    "event_ordinal",
    "disposition",
    "reason",
    "identity",
    "source_kind",
    "frame_kind",
];
const OUTCOME_FIELDS: [&str; 7] = [
    "schema",
    "open_sha256",
    "attempt_sha256",
    "sequence",
    "outcome",
    "code",
    "source_references",
];
const STARTED_FIELDS: [&str; 4] = ["schema", "open_sha256", "attempt_sha256", "sequence"];

/// Validate an OPEN record against its own namespace.
pub fn open_value<'a>(value: &'a Value, directory: &Path) -> Result<&'a Value> {
    exact(value, &OPEN_FIELDS)?;
    if get(value, "schema").as_str() != Some("agent-memory/omp-stream-open-v1") {
        return Err(err("Unknown finite stream OPEN schema"));
    }
    for key in OPEN_FIELDS.iter().filter(|key| **key != "schema") {
        turn_sources::bounded(get(value, key), 4096)?;
    }
    uuid(get(value, "project_id"))?;
    let instance = uuid(get(value, "instance_id"))?;
    installation_id(get(value, "installation_id"))?;
    uuid(get(value, "initial_lifecycle"))?;
    sha(get(value, "artifact_digest"))?;
    super::clock_window::stamp(get(value, "opened_at"))?;
    let session = get(value, "session").as_str().unwrap_or_default();
    let parent_name = directory
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str());
    if name(directory)? != instance || parent_name != Some(session_sha256(session).as_str()) {
        return Err(err("Finite stream OPEN namespace differs"));
    }
    Ok(value)
}

fn id_set(value: &Value, key: &str) -> BTreeSet<String> {
    get(value, key)
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect()
}

/// Apply one recorded identity (baseline, delta, hint or unresolvable).
pub(crate) fn apply_identity(value: &Value, previous: Option<Value>) -> Result<Option<Value>> {
    if same(value, &object([("kind", string("unresolvable"))])) {
        return Ok(None);
    }
    if value.as_object().is_none() {
        return Err(err("Malformed finite identity record"));
    }
    if is_hint(value) {
        hint_identity(value, None, None)?;
        return Ok(previous);
    }
    if get(value, "kind").as_str() == Some("baseline") {
        exact(
            value,
            &["kind", "view_id", "leaf_id", "native_ids", "tool_ids"],
        )?;
        let result = Value::Object(
            value
                .as_object()
                .unwrap_or_default()
                .iter()
                .filter(|(k, _)| k != "kind")
                .cloned()
                .collect(),
        );
        identity(&result)?;
        return Ok(Some(result));
    }
    exact(
        value,
        &[
            "kind",
            "view_id",
            "leaf_id",
            "native_added",
            "native_removed",
            "tool_added",
            "tool_removed",
        ],
    )?;
    let Some(previous) = previous.filter(|_| get(value, "kind").as_str() == Some("delta")) else {
        return Err(err("Finite identity delta has no known baseline"));
    };
    let mut pairs = vec![
        ("view_id".to_owned(), get(value, "view_id").clone()),
        ("leaf_id".to_owned(), get(value, "leaf_id").clone()),
    ];
    for prefix in ["native", "tool"] {
        let added: BTreeSet<String> = ids(get(value, &format!("{prefix}_added")))?
            .into_iter()
            .collect();
        let removed: BTreeSet<String> = ids(get(value, &format!("{prefix}_removed")))?
            .into_iter()
            .collect();
        let old = id_set(&previous, &format!("{prefix}_ids"));
        if old.intersection(&added).next().is_some()
            || !removed.is_subset(&old)
            || added.intersection(&removed).next().is_some()
        {
            return Err(err("Invalid finite identity delta"));
        }
        let result: BTreeSet<String> = old.difference(&removed).cloned().chain(added).collect();
        pairs.push((format!("{prefix}_ids"), super::strings(result)));
    }
    let result = Value::Object(pairs);
    identity(&result)?;
    Ok(Some(result))
}

/// One inventory row; hints and verified native IDs feed the per-session join.
pub(crate) struct Row {
    pub(crate) session: Option<String>,
    pub(crate) session_sha256: Option<String>,
    pub(crate) instance_id: Option<String>,
    pub(crate) open_sha256: Option<String>,
    pub(crate) closed: bool,
    pub(crate) retired: bool,
    pub(crate) pending: Vec<Value>,
    hints: Vec<(u64, Option<String>)>,
    verified: BTreeSet<String>,
}
impl Row {
    fn startup(session_sha256: &str, instance_id: Option<&str>) -> Self {
        Self {
            session: None,
            session_sha256: Some(session_sha256.to_owned()),
            instance_id: instance_id.map(str::to_owned),
            open_sha256: None,
            closed: false,
            retired: false,
            pending: vec![object([("code", string("startup-incomplete"))])],
            hints: Vec::new(),
            verified: BTreeSet::new(),
        }
    }
    fn value(&self) -> Value {
        let mut pairs = vec![("session".to_owned(), opt_string(self.session.as_deref()))];
        if let Some(sha) = self.session_sha256.as_deref() {
            pairs.push(("session_sha256".into(), string(sha)));
        }
        pairs.push((
            "instance_id".into(),
            opt_string(self.instance_id.as_deref()),
        ));
        pairs.push((
            "open_sha256".into(),
            opt_string(self.open_sha256.as_deref()),
        ));
        pairs.push(("closed".into(), Value::Bool(self.closed)));
        pairs.push(("retired".into(), Value::Bool(self.retired)));
        pairs.push(("pending".into(), Value::Array(self.pending.clone())));
        Value::Object(pairs)
    }
}

fn pair(name: impl Into<String>, value: impl Into<String>) -> Value {
    Value::Array(vec![string(name), string(value)])
}
/// Python list ordering of `[name, digest]` string pins.
fn sort_pins(pins: &mut [(String, String)]) {
    pins.sort();
}
fn pin_values(pins: &[(String, String)]) -> Value {
    Value::Array(pins.iter().map(|(a, b)| pair(a, b)).collect())
}
/// Close pins: immutable attempt/outcome/started/transition inventory only.
pub(crate) fn close_pins(pins: &Value) -> Value {
    Value::Array(
        pins.as_array()
            .unwrap_or_default()
            .iter()
            .filter(|item| {
                let name = item
                    .as_array()
                    .and_then(|v| v.first())
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                name != "retirement.json"
                    && !name.starts_with("resolutions/")
                    && !name.starts_with("transition-resolutions/")
            })
            .cloned()
            .collect(),
    )
}
fn relative(path: &Path, base: &Path) -> Result<String> {
    let relative = path
        .strip_prefix(base)
        .map_err(|_| err("Finite stream inventory entry escapes its namespace"))?;
    Ok(super::path_text(relative)?.to_owned())
}
fn sequence_key(path: &Path) -> Result<u64> {
    sequence_of(path)
}

const ALLOWED: [&str; 11] = [
    "open.json",
    "stream.lock",
    "live.lock",
    "attempts",
    "outcomes",
    "started",
    "closed.json",
    "retirement.json",
    "resolutions",
    "transitions",
    "transition-resolutions",
];

/// Inspect one published instance; returns its row and sorted pins.
pub(crate) fn inspect(
    directory: &Path,
    opened: &Value,
    include_close: bool,
) -> Result<(Row, Value)> {
    let opened = open_value(opened, directory)?;
    let open_digest = digest(opened)?;
    fs::trust::state(directory)?;
    {
        let chain = DirectoryChain::open(directory).map_err(io)?;
        let names = chain
            .directory()
            .entries_bounded(MAX_ENTRIES)
            .map_err(io)?
            .ok_or_else(|| err("Unknown finite stream state"))?;
        for entry in names {
            let info = chain.directory().stat_at(&entry).map_err(io)?;
            let text = entry.to_str().unwrap_or_default();
            if !ALLOWED.contains(&text) || info.is_symlink() {
                return Err(err("Unknown finite stream state"));
            }
        }
        chain.recheck().map_err(io)?;
    }
    read_bytes_required(&directory.join("live.lock"), 0)?;
    let mut pins = vec![("open.json".to_owned(), open_digest.clone())];
    let mut pending = Vec::new();
    let mut attempts = BTreeMap::new();
    let mut outcomes = BTreeMap::new();
    let mut starts = BTreeMap::new();
    let mut resolutions = BTreeSet::new();
    let mut transitions = BTreeSet::new();
    let mut transition_resolutions = BTreeSet::new();
    for namespace in [
        "attempts",
        "outcomes",
        "started",
        "resolutions",
        "transitions",
        "transition-resolutions",
    ] {
        let path = directory.join(namespace);
        if !lexists(&path)? {
            continue;
        }
        let pattern = if namespace.contains("transition") {
            Pattern::Transition
        } else {
            Pattern::Sequence
        };
        for child in entries(&path, pattern, false, MAX_ENTRIES)? {
            let value = read_required(&child, MAX_METADATA)?;
            pins.push((relative(&child, directory)?, digest(&value)?));
            match namespace {
                "attempts" => {
                    attempts.insert(sequence_key(&child)?, value);
                }
                "outcomes" => {
                    outcomes.insert(sequence_key(&child)?, value);
                }
                "started" => {
                    starts.insert(sequence_key(&child)?, value);
                }
                "resolutions" => {
                    resolutions.insert(sequence_key(&child)?);
                }
                "transitions" => {
                    transitions.insert(super::stem(&child)?.to_owned());
                }
                _ => {
                    transition_resolutions.insert(super::stem(&child)?.to_owned());
                }
            }
        }
    }
    if !transition_resolutions.is_subset(&transitions) {
        return Err(err("Orphan finite transition recovery"));
    }
    let orphan = outcomes
        .keys()
        .chain(starts.keys())
        .chain(resolutions.iter())
        .any(|sequence| !attempts.contains_key(sequence));
    if orphan {
        return Err(err("Orphan finite stream attempt metadata"));
    }
    let mut known: Option<Value> = None;
    let mut last_checked: Option<Value> = None;
    let mut hints = Vec::new();
    let mut verified_native = BTreeSet::new();
    let mut refusal_count = 0u64;
    let mut ordinal = 0u64;
    for (expected, (sequence, attempt)) in (1u64..).zip(attempts.iter()) {
        let sequence = *sequence;
        if sequence != expected {
            return Err(err("Missing finite source attempt"));
        }
        exact(attempt, &ATTEMPT_FIELDS)?;
        let text = |key| get(attempt, key).as_str();
        if text("schema") != Some("agent-memory/omp-stream-attempt-v1")
            || text("open_sha256") != Some(open_digest.as_str())
            || get(attempt, "instance_id") != get(opened, "instance_id")
            || get(attempt, "sequence").as_int().and_then(|v| v.as_u64()) != Some(sequence)
            || !text("action").is_some_and(|a| ACTIONS.contains(&a))
            || !matches!(text("disposition"), Some("accepted" | "refused"))
        {
            return Err(err("Foreign finite source attempt"));
        }
        let source_kind = get(attempt, "source_kind");
        let refused = text("disposition") == Some("refused");
        if !matches!(source_kind, Value::Null)
            && (!matches!(
                text("source_kind"),
                Some("session-manager" | "retired-file")
            ) || refused)
        {
            return Err(err("Invalid checked source-view kind"));
        }
        if !matches!(text("frame_kind"), Some("REQUEST" | "REFUSED-ATTEMPT"))
            || (text("frame_kind") == Some("REFUSED-ATTEMPT") && !refused)
        {
            return Err(err("Invalid source attempt wire type"));
        }
        let attempt_digest = digest(attempt)?;
        if let Some(start) = starts.get(&sequence) {
            exact(start, &STARTED_FIELDS)?;
            let expected = started_value(&open_digest, &attempt_digest, sequence);
            if !same(start, &expected) {
                return Err(err("Foreign finite stream start record"));
            }
        }
        let event_ordinal = integer(get(attempt, "event_ordinal"))?;
        if event_ordinal < ordinal {
            return Err(err("Regressed finite event ordinal"));
        }
        ordinal = event_ordinal;
        if refused {
            if !text("reason").is_some_and(|r| REFUSAL_CODES.contains(&r)) {
                return Err(err("Unknown finite refusal code"));
            }
        } else if !matches!(get(attempt, "reason"), Value::Null) {
            return Err(err("Accepted finite source has refusal reason"));
        }
        refusal_count += u64::from(text("frame_kind") == Some("REFUSED-ATTEMPT"));
        let attempt_identity = get(attempt, "identity");
        let hint = is_hint(attempt_identity);
        if hint {
            hint_identity(
                attempt_identity,
                get(opened, "session").as_str(),
                get(opened, "repo_root").as_str().map(Path::new),
            )?;
            if text("action") != Some("hint")
                || refused
                || !matches!(source_kind, Value::Null)
                || starts.contains_key(&sequence)
            {
                return Err(err("Neutral hint acquired full source execution authority"));
            }
        }
        known = apply_identity(attempt_identity, known)?;
        if text("source_kind") == Some("session-manager") {
            last_checked = known.clone();
        }
        let outcome = outcomes.get(&sequence);
        let mut code: Option<String> = None;
        match outcome {
            None => code = Some("missing-outcome".into()),
            Some(outcome) => {
                exact(outcome, &OUTCOME_FIELDS)?;
                let result = get(outcome, "outcome").as_str();
                if get(outcome, "schema").as_str() != Some("agent-memory/omp-stream-outcome-v1")
                    || get(outcome, "open_sha256").as_str() != Some(open_digest.as_str())
                    || get(outcome, "attempt_sha256").as_str() != Some(attempt_digest.as_str())
                    || get(outcome, "sequence").as_int().and_then(|v| v.as_u64()) != Some(sequence)
                    || !matches!(result, Some("verified" | "durable-refusal" | "unknown"))
                {
                    return Err(err("Foreign finite stream outcome"));
                }
                let outcome_code = get(outcome, "code").as_str();
                let references = get(outcome, "source_references");
                if result == Some("verified") {
                    if refused || (!hint && !starts.contains_key(&sequence)) {
                        return Err(err(
                            "Verified finite outcome lacks executed accepted attempt",
                        ));
                    }
                    let empty = references.as_array().is_some_and(<[Value]>::is_empty);
                    if hint && (outcome_code != Some("hint-recorded") || !empty) {
                        return Err(err("Neutral hint outcome claims source authority"));
                    }
                    if !hint && outcome_code != Some("verified") {
                        return Err(err("Full source outcome has a non-execution code"));
                    }
                }
                if outcome_code == Some("hint-recorded") && (!hint || result != Some("verified")) {
                    return Err(err(
                        "Hint outcome code differs from neutral metadata execution",
                    ));
                }
                source_refs(references)?;
                let known_code = outcome_code.is_some_and(|c| {
                    REFUSAL_CODES.contains(&c)
                        || matches!(
                            c,
                            "observer-refused"
                                | "verified"
                                | "hint-recorded"
                                | "unknown-after-start"
                                | "unknown-outcome"
                                | "busy-exhausted"
                        )
                });
                if !known_code {
                    return Err(err("Unknown finite outcome code"));
                }
                if result != Some("verified") {
                    code = outcome_code.map(str::to_owned);
                }
            }
        }
        if hint {
            let leaf = get(attempt_identity, "leaf_id").as_str().map(str::to_owned);
            hints.push((sequence, leaf));
        } else if outcome.is_some_and(|o| get(o, "outcome").as_str() == Some("verified"))
            && matches!(
                text("source_kind"),
                Some("session-manager" | "retired-file")
            )
        {
            if let Some(known) = known.as_ref() {
                verified_native.extend(id_set(known, "native_ids"));
            }
        }
        let resolution = directory
            .join("resolutions")
            .join(format!("{sequence}.json"));
        if lexists(&resolution)? {
            if hint {
                return Err(err(
                    "Neutral hint cannot be resolved as a full source attempt",
                ));
            }
            let recovered = read_required(&resolution, MAX_METADATA)?;
            check_resolution(
                &recovered,
                &open_digest,
                &attempt_digest,
                sequence,
                known.as_ref(),
            )?;
            let fence = read_required(&directory.join("retirement.json"), MAX_METADATA)?;
            if get(&recovered, "retirement_sha256").as_str() != Some(digest(&fence)?.as_str()) {
                return Err(err("Stream recovery lacks retirement fence"));
            }
            code = None;
        }
        if let Some(code) = code.filter(|c| !c.is_empty()) {
            pending.push(object([
                ("code", string(code)),
                ("sequence", int(sequence)),
            ]));
        }
        if known.is_none() && !hint {
            pending.push(object([
                ("code", string("unresolvable-identity")),
                ("sequence", int(sequence)),
            ]));
        }
    }
    let retirement_path = directory.join("retirement.json");
    let retired = lexists(&retirement_path)?;
    let mut fence = None;
    if retired {
        let value = read_required(&retirement_path, MAX_METADATA)?;
        exact(
            &value,
            &[
                "schema",
                "open_sha256",
                "instance_id",
                "session",
                "fence_id",
            ],
        )?;
        let expected = object([
            ("schema", string("agent-memory/omp-stream-retirement-v1")),
            ("open_sha256", string(&open_digest)),
            ("instance_id", get(opened, "instance_id").clone()),
            ("session", get(opened, "session").clone()),
            ("fence_id", get(&value, "fence_id").clone()),
        ]);
        if !same(&value, &expected) {
            return Err(err("Foreign finite stream retirement"));
        }
        uuid(get(&value, "fence_id"))?;
        pins.push(("retirement.json".into(), digest(&value)?));
        fence = Some(value);
    }
    let transition_dir = directory.join("transitions");
    if lexists(&transition_dir)? {
        for path in entries(&transition_dir, Pattern::Transition, false, MAX_ENTRIES)? {
            let value = read_required(&path, MAX_METADATA)?;
            check_transition(&value, opened, &open_digest, &path, last_checked.as_ref())?;
            let resolution = directory
                .join("transition-resolutions")
                .join(super::name(&path)?);
            if lexists(&resolution)? {
                let resolved = read_required(&resolution, MAX_METADATA)?;
                check_transition_resolution(&resolved, &value, fence.as_ref())?;
            } else {
                pending.push(object([
                    ("code", get(&value, "reason").clone()),
                    ("transition_id", get(&value, "transition_id").clone()),
                ]));
            }
        }
    }
    sort_pins(&mut pins);
    let mut closed = false;
    if include_close && lexists(&directory.join("closed.json"))? {
        let value = read_required(&directory.join("closed.json"), 4096)?;
        exact(
            &value,
            &[
                "schema",
                "open_sha256",
                "last_attempt_seq",
                "refused_count",
                "last_event_ordinal",
                "inventory_sha256",
            ],
        )?;
        // Close pins contain immutable attempt/outcome/started/transition inventory,
        // not later retirement/recovery records.
        let close = close_pins(&pin_values(&pins));
        let expected = object([
            ("schema", string("agent-memory/omp-stream-close-v1")),
            ("open_sha256", string(&open_digest)),
            ("last_attempt_seq", int(attempts.len() as u64)),
            ("refused_count", int(refusal_count)),
            ("last_event_ordinal", int(ordinal)),
            ("inventory_sha256", string(digest(&close)?)),
        ]);
        if !same(&value, &expected) {
            return Err(err("Finite stream close inventory changed"));
        }
        closed = true;
        pins.push(("closed.json".into(), digest(&value)?));
    }
    if include_close && !closed {
        pending.push(object([("code", string("open-stream"))]));
    }
    sort_pins(&mut pins);
    let row = Row {
        session: get(opened, "session").as_str().map(str::to_owned),
        session_sha256: None,
        instance_id: get(opened, "instance_id").as_str().map(str::to_owned),
        open_sha256: Some(open_digest),
        closed,
        retired,
        pending,
        hints,
        verified: verified_native,
    };
    Ok((row, pin_values(&pins)))
}

fn started_value(open_digest: &str, attempt_digest: &str, sequence: u64) -> Value {
    object([
        ("schema", string("agent-memory/omp-stream-started-v1")),
        ("open_sha256", string(open_digest)),
        ("attempt_sha256", string(attempt_digest)),
        ("sequence", int(sequence)),
    ])
}

fn subset_of(required: &BTreeSet<String>, value: &Value) -> Result<bool> {
    let available: BTreeSet<String> = ids(value)?.into_iter().collect();
    Ok(required.is_subset(&available))
}

fn check_resolution(
    recovered: &Value,
    open_digest: &str,
    attempt_digest: &str,
    sequence: u64,
    known: Option<&Value>,
) -> Result<()> {
    exact(
        recovered,
        &[
            "schema",
            "open_sha256",
            "attempt_sha256",
            "retirement_sha256",
            "sequence",
            "view_id",
            "native_ids",
            "tool_ids",
            "source_references",
        ],
    )?;
    let unbound = || err("Unbound exact native stream recovery");
    let text = |key| get(recovered, key).as_str();
    if text("schema") != Some("agent-memory/omp-stream-recovery-v1")
        || text("open_sha256") != Some(open_digest)
        || text("attempt_sha256") != Some(attempt_digest)
        || get(recovered, "sequence").as_int().and_then(|v| v.as_u64()) != Some(sequence)
    {
        return Err(unbound());
    }
    let Some(known) = known else {
        return Err(unbound());
    };
    let native = id_set(known, "native_ids");
    let tools = id_set(known, "tool_ids");
    if (native.is_empty() && tools.is_empty())
        || !subset_of(&native, get(recovered, "native_ids"))?
        || !subset_of(&tools, get(recovered, "tool_ids"))?
    {
        return Err(unbound());
    }
    uuid(get(recovered, "view_id"))?;
    if source_refs(get(recovered, "source_references"))?.is_empty() {
        return Err(err("Attempt recovery lacks actual source references"));
    }
    Ok(())
}

fn check_transition(
    value: &Value,
    opened: &Value,
    open_digest: &str,
    path: &Path,
    last_checked: Option<&Value>,
) -> Result<()> {
    exact(
        value,
        &[
            "schema",
            "open_sha256",
            "project_id",
            "checkout_key",
            "session",
            "instance_id",
            "transition_id",
            "target_session",
            "target_instance",
            "last_view",
            "reason",
        ],
    )?;
    let text = |key| get(value, key).as_str();
    let bound = ["project_id", "checkout_key", "session", "instance_id"]
        .iter()
        .all(|key| get(value, key) == get(opened, key));
    if text("schema") != Some("agent-memory/omp-transition-tail-v1")
        || text("open_sha256") != Some(open_digest)
        || !bound
        || text("transition_id") != Some(super::stem(path)?)
        || text("reason") != Some("transition-final-view-unavailable")
    {
        return Err(err("Foreign finite transition tail"));
    }
    identity(get(value, "last_view"))?;
    uuid(get(value, "transition_id"))?;
    uuid(get(value, "target_instance"))?;
    turn_sources::bounded(get(value, "target_session"), 512)?;
    let self_target = get(value, "target_session") == get(opened, "session")
        && get(value, "target_instance") == get(opened, "instance_id");
    if !super::same_opt(Some(get(value, "last_view")), last_checked) || self_target {
        return Err(err(
            "Transition does not bind last precommit checked old view",
        ));
    }
    Ok(())
}

fn check_transition_resolution(
    resolved: &Value,
    transition: &Value,
    fence: Option<&Value>,
) -> Result<()> {
    exact(
        resolved,
        &[
            "schema",
            "transition_sha256",
            "retirement_sha256",
            "view_id",
            "native_ids",
            "tool_ids",
            "source_references",
            "nonlearning_native_ids",
        ],
    )?;
    let anchors = get(transition, "last_view");
    let unbound = || err("Unbound transition source recovery");
    let text = |key| get(resolved, key).as_str();
    let Some(fence) = fence else {
        return Err(unbound());
    };
    if text("schema") != Some("agent-memory/omp-transition-recovery-v1")
        || text("transition_sha256") != Some(digest(transition)?.as_str())
        || text("retirement_sha256") != Some(digest(fence)?.as_str())
        || matches!(anchors, Value::Null)
        || !subset_of(&id_set(anchors, "native_ids"), get(resolved, "native_ids"))?
        || !subset_of(&id_set(anchors, "tool_ids"), get(resolved, "tool_ids"))?
    {
        return Err(unbound());
    }
    uuid(get(resolved, "view_id"))?;
    let references = source_refs(get(resolved, "source_references"))?;
    if references.is_empty() {
        let nonlearning = ids(get(resolved, "nonlearning_native_ids"))?;
        let native = get(resolved, "native_ids");
        if id_set(anchors, "native_ids").is_empty()
            || native.as_array().is_none_or(<[Value]>::is_empty)
            || !same(&super::strings(nonlearning), native)
        {
            return Err(err(
                "No-learning recovery lacks nonempty anchored exact graph",
            ));
        }
    } else if !same(
        get(resolved, "nonlearning_native_ids"),
        &Value::Array(Vec::new()),
    ) {
        return Err(err("Mixed source/nonlearning recovery proof"));
    }
    Ok(())
}

/// One fresh linear per-session join; notifications never create source origin.
fn join_hints(
    authority: &dyn StreamAuthority,
    repo: &Path,
    rows: &mut [Row],
    pins: &mut Vec<Value>,
) -> Result<()> {
    let mut full: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut needed: Vec<(String, BTreeSet<String>)> = Vec::new();
    for row in rows.iter_mut() {
        let session = row.session.clone().unwrap_or_default();
        full.entry(session.clone())
            .or_default()
            .extend(std::mem::take(&mut row.verified));
        for (_, leaf) in &row.hints {
            if let Some(leaf) = leaf {
                match needed.iter_mut().find(|(s, _)| *s == session) {
                    Some((_, leaves)) => {
                        leaves.insert(leaf.clone());
                    }
                    None => needed.push((session.clone(), BTreeSet::from([leaf.clone()]))),
                }
            }
        }
    }
    let mut transport = None;
    let mut dispositions = Vec::new();
    for (session, leaves) in &needed {
        let empty = BTreeSet::new();
        let covered = full.get(session).unwrap_or(&empty);
        let missing: BTreeSet<String> = leaves.difference(covered).cloned().collect();
        if missing.is_empty() {
            continue;
        }
        if transport.is_none() {
            transport = Some(authority.transport()?);
        }
        let Some(provider) = transport.as_ref() else {
            continue;
        };
        let actual = provider.native_disposition_ids(repo, session)?;
        for native in &actual {
            turn_sources::bounded(&string(native), 512)?;
        }
        let found: BTreeSet<String> = missing.intersection(&actual).cloned().collect();
        dispositions.push((session.clone(), found));
    }
    for (session, native) in dispositions {
        full.entry(session).or_default().extend(native);
    }
    let mut coverage = Vec::new();
    for row in rows.iter_mut() {
        let session = row.session.clone().unwrap_or_default();
        let empty = BTreeSet::new();
        let covered = full.get(&session).unwrap_or(&empty);
        for (sequence, leaf) in std::mem::take(&mut row.hints) {
            let status = match leaf.as_deref() {
                None => "neutral",
                Some(leaf) if covered.contains(leaf) => "covered",
                Some(_) => "uncovered",
            };
            coverage.push(Value::Array(vec![
                opt_string(row.session.as_deref()),
                opt_string(row.instance_id.as_deref()),
                int(sequence),
                opt_string(leaf.as_deref()),
                string(status),
            ]));
            if status == "uncovered" {
                row.pending.push(object([
                    ("code", string("hint-uncovered")),
                    ("sequence", int(sequence)),
                    ("native_entry_id", opt_string(leaf.as_deref())),
                ]));
            }
        }
    }
    if !coverage.is_empty() {
        pins.push(Value::Array(vec![
            string("hint-coverage"),
            Value::Array(coverage),
        ]));
    }
    Ok(())
}

/// Account only an unpublished startup, never waive missing source records.
fn incomplete_startup(directory: &Path) -> Result<(Row, Value)> {
    let instance = name(directory)?.to_owned();
    super::uuid_text(&instance)?;
    fs::trust::state(directory)?;
    let mut pins = vec![pair("startup-directory", &instance)];
    let chain = DirectoryChain::open(directory).map_err(io)?;
    let names = chain
        .directory()
        .entries_bounded(MAX_ENTRIES)
        .map_err(io)?
        .ok_or_else(|| err("Unpublished stream has unknown or source-bearing state"))?;
    for entry in names {
        let info = chain.directory().stat_at(&entry).map_err(io)?;
        let text = entry.to_str().unwrap_or_default();
        if !matches!(text, "live.lock" | "stream.lock" | "open.json") || info.is_symlink() {
            return Err(err(
                "Unpublished stream has unknown or source-bearing state",
            ));
        }
        let limit = if text == "open.json" { MAX_METADATA } else { 0 };
        // OPEN contains only safe binding metadata, never a source/request body.
        let raw = read_bytes_required(&directory.join(text), limit)?;
        pins.push(pair(text, crate::developer_identity::sha256_hex(&raw)));
    }
    chain.recheck().map_err(io)?;
    let session = directory
        .parent()
        .and_then(|p| p.file_name())
        .and_then(|n| n.to_str())
        .unwrap_or_default()
        .to_owned();
    Ok((Row::startup(&session, Some(&instance)), Value::Array(pins)))
}

fn resolved_repo(repo: &Path) -> Result<String> {
    let resolved = fs::canonical_path(repo, false)?;
    Ok(super::path_text(&resolved)?.to_owned())
}

fn collect(
    authority: &dyn StreamAuthority,
    repo: &Path,
    base: &Path,
    read_only: bool,
) -> Result<Value> {
    if !fs::trust::state(base)?.present {
        if read_only {
            return Ok(object([
                ("schema", string("agent-memory/omp-stream-inventory-v1")),
                ("streams", Value::Array(Vec::new())),
                ("pending", Value::Array(Vec::new())),
                (
                    "inventory_sha256",
                    string(digest(&Value::Array(Vec::new()))?),
                ),
            ]));
        }
        return Err(err("Finite stream private state is offline"));
    }
    let namespace = base.join("omp-stream-v1");
    let mut rows = Vec::new();
    let mut pins = Vec::new();
    let lock_namespace = base.join("omp-stream-session-locks");
    if lexists(&lock_namespace)? {
        for path in entries(&lock_namespace, Pattern::SessionLock, false, MAX_ENTRIES)? {
            read_bytes_required(&path, 0)?;
            let session = name(&path)?.trim_end_matches(".lock").to_owned();
            if !lexists(&namespace.join(&session))? {
                rows.push(Row::startup(&session, None));
                pins.push(Value::Array(vec![
                    string(relative(&path, base)?),
                    Value::Array(vec![pair("orphan-session-startup-lock", &session)]),
                ]));
            }
        }
    }
    let root = resolved_repo(repo)?;
    if lexists(&namespace)? {
        for session_dir in entries(&namespace, Pattern::Session, true, MAX_ENTRIES)? {
            let (directories, marker) = session_instances(&session_dir)?;
            let session = name(&session_dir)?.to_owned();
            if marker.is_some() || directories.is_empty() {
                rows.push(Row::startup(&session, None));
                let state_pin = match marker.as_ref() {
                    Some(marker) => pair("startup-incomplete.json", digest(marker)?),
                    None => pair("empty-startup-directory", &session),
                };
                pins.push(Value::Array(vec![
                    string(relative(&session_dir, base)?),
                    Value::Array(vec![state_pin]),
                ]));
            }
            for directory in directories {
                let opened = turn_sources::read(&directory.join("open.json"), MAX_METADATA)
                    .ok()
                    .flatten();
                let (row, stream_pins) = match opened {
                    None => incomplete_startup(&directory)?,
                    Some(opened) => {
                        open_value(&opened, &directory)?;
                        if get(&opened, "repo_root").as_str() != Some(root.as_str()) {
                            return Err(err("Foreign repository finite stream"));
                        }
                        with_lock(&directory.join("stream.lock"), true, read_only, || {
                            inspect(&directory, &opened, true)
                        })?
                    }
                };
                rows.push(row);
                pins.push(Value::Array(vec![
                    string(relative(&directory, base)?),
                    stream_pins,
                ]));
            }
        }
    }
    join_hints(authority, repo, &mut rows, &mut pins)?;
    let mut pending = Vec::new();
    for row in &rows {
        for item in &row.pending {
            let mut pairs = vec![
                ("session".to_owned(), opt_string(row.session.as_deref())),
                (
                    "instance_id".to_owned(),
                    opt_string(row.instance_id.as_deref()),
                ),
            ];
            if let Some(sha) = row.session_sha256.as_deref() {
                pairs.push(("session_sha256".into(), string(sha)));
            }
            for (key, value) in item.as_object().unwrap_or_default() {
                match pairs.iter_mut().find(|(k, _)| k == key) {
                    Some((_, existing)) => *existing = value.clone(),
                    None => pairs.push((key.clone(), value.clone())),
                }
            }
            pending.push(Value::Object(pairs));
        }
    }
    let streams = rows.iter().map(Row::value).collect();
    Ok(object([
        ("schema", string("agent-memory/omp-stream-inventory-v1")),
        ("streams", Value::Array(streams)),
        ("pending", Value::Array(pending)),
        ("inventory_sha256", string(digest(&Value::Array(pins))?)),
    ]))
}

/// Complete sibling inventory. Lifecycle callers retain an EXCLUSIVE barrier.
pub fn inventory(
    authority: &dyn StreamAuthority,
    repo: &Path,
    base: Option<&Path>,
    read_only: bool,
) -> Result<Value> {
    let run = || -> Result<Value> {
        let base = match base {
            Some(base) => base.to_path_buf(),
            None => authority.base(repo)?,
        };
        collect(authority, repo, &base, read_only)
    };
    if read_only {
        let mut run = Some(run);
        authority.observation_barrier(repo, &mut || {
            run.take()
                .ok_or_else(|| err("Finite stream inventory ran twice"))?()
        })
    } else {
        barrier(authority, repo, |_| run())
    }
}

pub fn drain_state(
    authority: &dyn StreamAuthority,
    repo: &Path,
    base: Option<&Path>,
    read_only: bool,
) -> Result<Value> {
    inventory(authority, repo, base, read_only)
}

pub fn validate_drain(
    authority: &dyn StreamAuthority,
    repo: &Path,
    proof: &Value,
    base: Option<&Path>,
) -> Result<Value> {
    let current = inventory(authority, repo, base, false)?;
    let pending = get(&current, "pending")
        .as_array()
        .is_some_and(|items| !items.is_empty());
    if pending || !same(proof, &current) {
        return Err(err(
            "Finite source stream coverage remains pending or changed",
        ));
    }
    Ok(current)
}

/// Ingest only: source barrier -> transport, before on_started; skip live owners.
pub fn retire_for_recovery(
    authority: &dyn StreamAuthority,
    repo: &Path,
    session: &str,
    exclude_instance: Option<&str>,
) -> Result<Vec<Value>> {
    let session_value = string(session);
    turn_sources::bounded(&session_value, 512)?;
    let base = authority.base(repo)?;
    let namespace = base.join("omp-stream-v1").join(session_sha256(session));
    barrier(authority, repo, |_| {
        let mut handles = Vec::new();
        if !lexists(&namespace)? {
            return Ok(handles);
        }
        let (directories, _) = session_instances(&namespace)?;
        for directory in directories {
            if Some(name(&directory)?) == exclude_instance {
                continue;
            }
            match turn_sources::read(&directory.join("open.json"), MAX_METADATA) {
                Ok(Some(_)) => {}
                Ok(None) | Err(_) => {
                    incomplete_startup(&directory)?;
                    continue;
                }
            }
            if turn_sources::read_bytes(&directory.join("live.lock"), 0)?.is_none() {
                if lexists(&directory.join("open.json"))? {
                    return Err(err("Published finite stream lacks its live-owner lock"));
                }
                // A concurrent startup has not published OPEN. It is not recoverable;
                // a persistent incomplete directory still refuses lifecycle inventory.
                continue;
            }
            let probe = match turn_sources::private_lock(&directory.join("live.lock"), false, false)
            {
                Ok(probe) => probe,
                Err(error) if super::busy(&error) => continue,
                Err(error) => return Err(error),
            };
            let handle = retire_one(&base, &directory, session)?;
            probe.finish()?;
            if let Some(handle) = handle {
                handles.push(handle);
            }
        }
        Ok(handles)
    })
}

fn retire_one(base: &Path, directory: &Path, session: &str) -> Result<Option<Value>> {
    let _ = base;
    with_lock(&directory.join("stream.lock"), false, false, || {
        if !lexists(&directory.join("open.json"))? {
            return Ok(None);
        }
        let opened = read_required(&directory.join("open.json"), MAX_METADATA)?;
        open_value(&opened, directory)?;
        if get(&opened, "session").as_str() != Some(session) {
            return Err(err("Foreign recovery session"));
        }
        let path = directory.join("retirement.json");
        let fence = if lexists(&path)? {
            read_required(&path, MAX_METADATA)?
        } else {
            let fence = object([
                ("schema", string("agent-memory/omp-stream-retirement-v1")),
                ("open_sha256", string(digest(&opened)?)),
                ("instance_id", string(name(directory)?)),
                ("session", string(session)),
                ("fence_id", string(crate::host::uuid4()?)),
            ]);
            retain(&path, &fence, MAX_METADATA)?;
            fence
        };
        inspect(directory, &opened, true)?;
        Ok(Some(object([
            ("directory", string(super::path_text(directory)?)),
            ("fence", fence),
        ])))
    })
}

/// Exact already-ingested native evidence for `resolve_recovery`.
pub struct RecoveryEvidence<'a> {
    pub view_id: &'a str,
    pub native_ids: &'a [String],
    pub tool_ids: &'a [String],
    pub payload_native_ids: &'a [String],
    pub source_references: &'a Value,
    pub complete: bool,
    pub leaf_id: Option<&'a str>,
    pub nonlearning_evidence: Option<&'a NonlearningTailEvidence>,
}

/// Bind exact already-ingested native evidence; never infer unknown identities.
///
/// Only the trusted observer-ingest path calls this after its independently checked
/// source/payload writes. Plain model/observer JSON cannot select this operation.
pub fn resolve_recovery(
    authority: &dyn StreamAuthority,
    repo: &Path,
    fences: &[Value],
    evidence: RecoveryEvidence<'_>,
) -> Result<Vec<Value>> {
    uuid(&string(evidence.view_id))?;
    let native = ids(&super::strings(evidence.native_ids.iter().cloned()))?;
    let tool = ids(&super::strings(evidence.tool_ids.iter().cloned()))?;
    let payload = ids(&super::strings(evidence.payload_native_ids.iter().cloned()))?;
    // NONLEARNING IDs may be included in payload_native_ids only after the
    // observer independently verifies their payload-not-required metadata shape.
    let refs = source_refs(evidence.source_references)?;
    if refs.is_empty() && evidence.nonlearning_evidence.is_none() {
        return Ok(Vec::new());
    }
    let native_set: BTreeSet<String> = native.iter().cloned().collect();
    let tool_set: BTreeSet<String> = tool.iter().cloned().collect();
    let payload_set: BTreeSet<String> = payload.into_iter().collect();
    let base = authority.base(repo)?;
    barrier(authority, repo, |_| {
        let mut recovered = Vec::new();
        for handle in fences {
            exact(handle, &["directory", "fence"])?;
            let fence = get(handle, "fence");
            if refs
                .iter()
                .any(|r| get(r, "session") != get(fence, "session"))
            {
                return Err(err("Recovery references name a different native session"));
            }
            let directory = directory(
                &base,
                get(fence, "session").as_str().unwrap_or_default(),
                get(fence, "instance_id").as_str().unwrap_or_default(),
            )?;
            if get(handle, "directory").as_str() != Some(super::path_text(&directory)?) {
                return Err(err("Foreign recovery fence location"));
            }
            let context = ResolveContext {
                directory: &directory,
                fence,
                refs: &refs,
                native: &native,
                native_set: &native_set,
                tool_set: &tool_set,
                payload_set: &payload_set,
                evidence: &evidence,
            };
            with_lock(&directory.join("stream.lock"), false, false, || {
                resolve_one(&context, &mut recovered)
            })?;
        }
        Ok(recovered)
    })
}

struct ResolveContext<'a> {
    directory: &'a Path,
    fence: &'a Value,
    refs: &'a [Value],
    native: &'a [String],
    native_set: &'a BTreeSet<String>,
    tool_set: &'a BTreeSet<String>,
    payload_set: &'a BTreeSet<String>,
    evidence: &'a RecoveryEvidence<'a>,
}

fn resolve_one(context: &ResolveContext<'_>, recovered: &mut Vec<Value>) -> Result<()> {
    let directory = context.directory;
    let current = read_required(&directory.join("retirement.json"), MAX_METADATA)?;
    if !same(&current, context.fence) {
        return Err(err("Recovery retirement fence changed"));
    }
    let opened = read_required(&directory.join("open.json"), MAX_METADATA)?;
    let (row, _) = inspect(directory, &opened, true)?;
    let mut known: Option<Value> = None;
    let mut all_known = true;
    let attempts = directory.join("attempts");
    let mut paths = if lexists(&attempts)? {
        entries(&attempts, Pattern::Sequence, false, MAX_ENTRIES)?
    } else {
        Vec::new()
    };
    let mut keyed = Vec::with_capacity(paths.len());
    for path in paths.drain(..) {
        keyed.push((sequence_of(&path)?, path));
    }
    keyed.sort();
    let open_digest = digest(&opened)?;
    let fence_digest = digest(context.fence)?;
    for (_, path) in &keyed {
        let attempt = read_required(path, MAX_METADATA)?;
        let attempt_identity = get(&attempt, "identity");
        known = apply_identity(attempt_identity, known)?;
        let hint = is_hint(attempt_identity);
        all_known = all_known && (known.is_some() || hint);
        if hint {
            continue;
        }
        let Some(current) = known.as_ref() else {
            continue;
        };
        let current_native = id_set(current, "native_ids");
        let current_tool = id_set(current, "tool_ids");
        if context.refs.is_empty()
            || (current_native.is_empty() && current_tool.is_empty())
            || !current_native.is_subset(context.native_set)
            || !current_native.is_subset(context.payload_set)
            || !current_tool.is_subset(context.tool_set)
        {
            continue;
        }
        let target = directory.join("resolutions").join(name(path)?);
        if lexists(&target)? {
            continue;
        }
        retain(
            &target,
            &object([
                ("schema", string("agent-memory/omp-stream-recovery-v1")),
                ("open_sha256", string(&open_digest)),
                ("attempt_sha256", string(digest(&attempt)?)),
                ("retirement_sha256", string(&fence_digest)),
                ("sequence", get(&attempt, "sequence").clone()),
                ("view_id", string(context.evidence.view_id)),
                ("native_ids", super::strings(context.native.iter().cloned())),
                (
                    "tool_ids",
                    super::strings(context.evidence.tool_ids.iter().cloned()),
                ),
                ("source_references", Value::Array(context.refs.to_vec())),
            ]),
            MAX_METADATA,
        )?;
        recovered.push(object([
            ("instance_id", get(&opened, "instance_id").clone()),
            ("sequence", get(&attempt, "sequence").clone()),
        ]));
    }
    let evidence = context.evidence;
    let no_learning = context.refs.is_empty()
        && evidence.complete
        && row.closed
        && all_known
        && !keyed.is_empty()
        && !context.native.is_empty()
        && evidence.nonlearning_evidence.is_some_and(|proof| {
            Some(proof.session.as_str()) == get(context.fence, "session").as_str()
                && proof.view_id == evidence.view_id
                && proof.native_ids.as_slice() == context.native
        });
    let transitions = directory.join("transitions");
    if evidence.complete && (!context.refs.is_empty() || no_learning) && lexists(&transitions)? {
        for path in entries(&transitions, Pattern::Transition, false, MAX_ENTRIES)? {
            let transition = read_required(&path, MAX_METADATA)?;
            let anchors = get(&transition, "last_view");
            if matches!(anchors, Value::Null) {
                continue;
            }
            let anchor_native = id_set(anchors, "native_ids");
            let anchor_tool = id_set(anchors, "tool_ids");
            if (no_learning && anchor_native.is_empty())
                || !anchor_native.is_subset(context.native_set)
                || !context.native_set.is_subset(context.payload_set)
                || !anchor_tool.is_subset(context.tool_set)
                || evidence
                    .leaf_id
                    .is_some_and(|leaf| !context.native_set.contains(leaf))
            {
                continue;
            }
            let target = directory.join("transition-resolutions").join(name(&path)?);
            if !lexists(&target)? {
                let nonlearning = if no_learning {
                    super::strings(context.native.iter().cloned())
                } else {
                    Value::Array(Vec::new())
                };
                retain(
                    &target,
                    &object([
                        ("schema", string("agent-memory/omp-transition-recovery-v1")),
                        ("transition_sha256", string(digest(&transition)?)),
                        ("retirement_sha256", string(&fence_digest)),
                        ("view_id", string(evidence.view_id)),
                        ("native_ids", super::strings(context.native.iter().cloned())),
                        (
                            "tool_ids",
                            super::strings(evidence.tool_ids.iter().cloned()),
                        ),
                        ("source_references", Value::Array(context.refs.to_vec())),
                        ("nonlearning_native_ids", nonlearning),
                    ]),
                    MAX_METADATA,
                )?;
            }
        }
    }
    Ok(())
}
