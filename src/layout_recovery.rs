//! Source-preserving layout recovery with immutable reviews and typed journals.
//! Private originals are evidence, never admission. Historical receipts and
//! current semantic coverage are intentionally separate observations.
mod command;
mod journal;
mod mutation;
pub mod publication;
mod validation;
use crate::codec::json::Value;
use crate::developer_identity::{encoded, sha256_hex};
use crate::error::Result;
use crate::layout_resolution::{
    err, field, frontier, int, items, locked, number, operation_paths, path_text, read_raw,
    read_value, require_completed_resolutions, same, sha, text, uuid, with,
};
use crate::layout_restore::{Operation, RecoverySource};
use crate::lifecycle::{get, object, string};
use crate::scope_binding::VerifiedEnrollmentHold;
use crate::{
    bank_contract as bank, bank_observation, bank_transactions as tx, bank_writes as writes, host,
    layout_recovery_observation as raw, layout_recovery_state as state, recovery_quiescence,
    security, sharing_limits,
};
pub use command::{Command, command, safe_envelope};
pub use journal::*;
pub use mutation::*;
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
pub use validation::*;
pub const RESULT: &str = "agent-memory/layout-recovery-result-v1";
pub const PROPOSAL: &str = "agent-memory/layout-recovery-proposal-v1";
pub const JOURNAL: &str = crate::layout_restore::RECOVERY_JOURNAL;
pub const RECEIPT: &str = crate::layout_restore::RECOVERY_RECEIPT;
pub const MARKER: &str = tx::MARKER;
pub const SENTINEL: &str = crate::layout_resolution::SENTINEL_FILE;
pub const PAGE_BYTES: usize = 24_576;
pub fn digest(bytes: &[u8]) -> String {
    sha256_hex(bytes)
}
pub(crate) fn pairs(value: &Value) -> Result<&[(String, Value)]> {
    value
        .as_object()
        .ok_or_else(|| err(crate::layout_resolution::MALFORMED))
}
pub(crate) fn require(condition: bool, message: &str) -> Result<()> {
    if condition { Ok(()) } else { Err(err(message)) }
}
pub(crate) fn same_rows(left: &[Value], right: &[Value]) -> bool {
    left.len() == right.len() && left.iter().zip(right).all(|(a, b)| same(a, b))
}
pub(crate) fn select(value: &Value, names: &[&str]) -> Result<Value> {
    Ok(Value::Object(
        names
            .iter()
            .map(|name| Ok(((*name).to_owned(), field(value, name)?.clone())))
            .collect::<Result<_>>()?,
    ))
}
pub(crate) fn size(value: &Value) -> Result<usize> {
    int(value)
        .and_then(|n| usize::try_from(n).ok())
        .ok_or_else(|| err(crate::layout_resolution::MALFORMED))
}
pub(crate) fn row_at<'a>(rows: &'a [Value], path: &str) -> Option<&'a Value> {
    rows.iter()
        .find(|row| get(row, "path").as_str() == Some(path))
}
pub(crate) fn files_facts(proposal: &Value) -> Result<BTreeMap<String, Value>> {
    Ok(pairs(field(field(proposal, "metadata_plan")?, "files")?)?
        .iter()
        .cloned()
        .collect())
}
pub(crate) fn source_bytes(directory: &Path, source: &Value) -> Result<Vec<u8>> {
    read_raw(&directory.join(text(source, "blob")?), raw::MAX_FILE)
}
pub(crate) fn router_last(paths: BTreeSet<String>) -> Vec<String> {
    let mut paths: Vec<_> = paths.into_iter().collect();
    paths.sort_by(|a, b| (a == "MEMORY.md", a).cmp(&(b == "MEMORY.md", b)));
    paths
}
pub fn result(
    operation: &str,
    operation_id: Option<&str>,
    values: impl IntoIterator<Item = (&'static str, Value)>,
) -> Value {
    with(
        object([
            ("schema", string(RESULT)),
            ("operation", string(operation)),
            ("operation_id", operation_id.map_or(Value::Null, string)),
            ("state", string("pending")),
            ("verified", Value::Bool(false)),
            ("canonical_eligibility", string("unavailable")),
            ("semantic_recovery_complete", Value::Bool(false)),
            ("pending_unit_ids", Value::Array(vec![])),
            ("pending_units", number(0)),
            ("required_actions", Value::Array(vec![])),
            ("writes_performed", Value::Bool(false)),
            ("private_artifact_writes_performed", Value::Bool(false)),
            ("temporary_lock_writes_performed", Value::Bool(false)),
            ("canonical_writes_performed", Value::Bool(false)),
            ("original_index_writes_performed", Value::Bool(false)),
            ("head_writes_performed", Value::Bool(false)),
            ("feature_writes_performed", Value::Bool(false)),
            ("history_writes_performed", Value::Bool(false)),
            ("truth_verified", Value::Bool(false)),
            ("code_freshness_verified", Value::Bool(false)),
            ("publication_verified", Value::Bool(false)),
            ("other_machine_import_verified", Value::Bool(false)),
        ]),
        values,
    )
}
pub fn eligibility(repo: &Path, owner: Option<&[u8]>, transaction: Option<&[u8]>) -> Value {
    match bank::validate(repo, owner, transaction) {
        Ok(_) => Value::Bool(true),
        Err(error) if matches!(error.kind(), crate::error::Kind::BankCapacity { .. }) => {
            Value::Bool(false)
        }
        Err(_) => string("unavailable"),
    }
}
pub fn operation(repo: &Path, operation_id: &str, runtime_root: &Path) -> Result<Operation> {
    let (root, bank, base, directory) = operation_paths(repo, operation_id, runtime_root)?;
    let manifest = read_value(&directory.join("manifest.json"))?;
    fields(
        &manifest,
        &[
            "schema",
            "operation_id",
            "inspected_at",
            "repo_root",
            "bank_path",
            "project_id",
            "identity",
            "observation_complete",
            "physical_entries",
            "sources",
            "totals",
            "git",
            "runtime_artifact",
            "metadata_policy",
            "bank_binding",
            "predecessor",
        ],
        Some("agent-memory/layout-source-manifest-v1"),
    )?;
    require(
        uuid(get(&manifest, "project_id"))
            && get(&manifest, "sources")
                .as_array()
                .is_some_and(|a| a.len() <= raw::MAX_FILES)
            && get(&manifest, "physical_entries")
                .as_array()
                .is_some_and(|a| a.len() <= raw::MAX_ENTRIES)
            && get(&manifest, "metadata_policy").as_str()
                == Some(
                    "ordinary-modes-ownership-darwin-provenance-exact-no-acl-other-xattr-flags",
                ),
        "Recovery manifest identity/type/capacity is invalid",
    )?;
    crate::lifecycle::time_value(field(&manifest, "inspected_at")?)?;
    let sources = items(field(&manifest, "sources")?)?;
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut total = 0usize;
    for source in sources {
        fields(
            source,
            &["source_id", "path", "byte_count", "sha256", "blob"],
            None,
        )?;
        raw::path_value(text(source, "path")?)?;
        let bytes = size(get(source, "byte_count"))?;
        require(
            ids.insert(text(source, "source_id")?)
                && paths.insert(text(source, "path")?)
                && bytes <= raw::MAX_FILE
                && state::sha(get(source, "sha256")),
            "Recovery manifest duplicates or misrepresents an original source",
        )?;
        total = total
            .checked_add(bytes)
            .ok_or_else(|| err("Recovery complete-source totals do not match the manifest"))?;
    }
    require(
        total <= raw::MAX_BYTES
            && int(get(get(&manifest, "totals"), "source_bytes")) == Some(total as i128)
            && int(get(get(&manifest, "totals"), "source_files")) == Some(sources.len() as i128),
        "Recovery complete-source totals do not match the manifest",
    )?;
    require(
        get(&manifest, "operation_id").as_str() == Some(operation_id)
            && get(&manifest, "repo_root").as_str() == Some(path_text(&root)?)
            && get(&manifest, "bank_path").as_str() == Some(path_text(&bank)?)
            && get(&manifest, "observation_complete").as_bool() == Some(true),
        "Recovery requires the exact complete inspected operation",
    )?;
    require(
        read_raw(
            &directory.join("manifest.sha256"),
            sharing_limits::MAX_PRIVATE,
        )? == format!("{}\n", sha(&manifest)?).as_bytes(),
        "Recovery manifest digest is missing or corrupt",
    )?;
    for source in sources {
        require(
            uuid(get(source, "source_id"))
                && text(source, "blob")? == format!("originals/{}.bin", text(source, "source_id")?),
            "Recovery source locator is not owned by this operation",
        )?;
        let body = source_bytes(&directory, source)?;
        require(
            body.len() == size(get(source, "byte_count"))?
                && digest(&body) == text(source, "sha256")?,
            "Original recovery evidence is missing or damaged",
        )?;
    }
    Ok(Operation {
        root,
        bank,
        base,
        directory,
        manifest,
    })
}
pub fn predecessor_value(
    repo: &Path,
    manifest: Option<&Value>,
    owner: Option<&[u8]>,
    runtime_root: &Path,
) -> Result<(Option<VerifiedEnrollmentHold>, Value)> {
    let predecessor = frontier()?.rolled_back_predecessor(repo, owner, runtime_root)?;
    let pinned = predecessor.as_ref().map_or(Value::Null, |p| {
        object([
            ("operation_id", string(p.operation_id())),
            ("marker_sha256", string(digest(p.marker()))),
            ("frontier", p.token().clone()),
        ])
    });
    require(
        manifest.is_none_or(|m| same(get(m, "predecessor"), &pinned)),
        "Raw continuation lost its exact internally checked rollback predecessor",
    )?;
    Ok((predecessor, pinned))
}
pub fn inspect(repo: &Path, identity_review: Option<&Value>, runtime_root: &Path) -> Result<Value> {
    inspect_with_id(repo, identity_review, runtime_root, &host::uuid4()?)
}
fn inspect_with_id(
    repo: &Path,
    identity_review: Option<&Value>,
    runtime_root: &Path,
    operation_id: &str,
) -> Result<Value> {
    let (root, target) = raw::locate(repo)?;
    let (predecessor, _) = predecessor_value(&root, None, None, runtime_root)?;
    bank_observation::markers(&target, None, predecessor.as_ref().map(|p| p.marker()))?;
    let (manifest, eligible) = locked(&root, &target, &operation_id, |owner| {
        require_completed_resolutions(&root, None, runtime_root)?;
        let (predecessor, pinned) = predecessor_value(&root, None, Some(owner), runtime_root)?;
        let transaction = predecessor.as_ref().map(|p| p.marker());
        let manifest = raw::observe(
            &root,
            &operation_id,
            runtime_root,
            &recovery_quiescence::runtime_binary(runtime_root),
            raw::ObserveOptions {
                owner: Some(owner),
                transaction,
                identity_review,
                predecessor: Some(&pinned),
                ..Default::default()
            },
        )?;
        let (_, _, _, directory) = operation_paths(&root, &operation_id, runtime_root)?;
        state::write_once(&directory.join("manifest.json"), &manifest)?;
        state::put_bytes(
            &directory.join("manifest.sha256"),
            format!("{}\n", sha(&manifest)?).as_bytes(),
        )?;
        writes::fault("layout-recovery-originals-completed")?;
        raw::recheck(&root, &manifest, Some(owner), transaction)?;
        let eligible = eligibility(&root, Some(owner), transaction);
        Ok((manifest, eligible))
    })?;
    let pages = items(field(&manifest, "sources")?)?
        .iter()
        .map(|s| size(get(s, "byte_count")).map(page_count))
        .collect::<Result<Vec<_>>>()?
        .iter()
        .sum::<usize>();
    Ok(result(
        "inspect",
        Some(&operation_id),
        [
            ("state", string("observed")),
            ("verified", Value::Bool(true)),
            ("canonical_eligibility", eligible),
            ("private_artifact_writes_performed", Value::Bool(true)),
            ("temporary_lock_writes_performed", Value::Bool(true)),
            ("observation_complete", Value::Bool(true)),
            (
                "source_files",
                field(field(&manifest, "totals")?, "source_files")?.clone(),
            ),
            (
                "source_bytes",
                field(field(&manifest, "totals")?, "source_bytes")?.clone(),
            ),
            ("page_count", number(pages)),
            ("manifest", manifest),
            (
                "required_actions",
                Value::Array(vec![string(
                    "Review every source page and the full intended native result independently before prepare",
                )]),
            ),
        ],
    ))
}
fn page_count(bytes: usize) -> usize {
    bytes.div_ceil(PAGE_BYTES).max(1)
}
pub fn page(repo: &Path, operation_id: &str, number: i128, runtime_root: &Path) -> Result<Value> {
    let op = operation(repo, operation_id, runtime_root)?;
    let number = usize::try_from(number)
        .map_err(|_| err("Recovery page requires a nonnegative ordered number"))?;
    let sources = items(field(&op.manifest, "sources")?)?;
    let mut cursor = 0;
    let pages = sources
        .iter()
        .map(|s| size(get(s, "byte_count")).map(page_count))
        .collect::<Result<Vec<_>>>()?
        .iter()
        .sum::<usize>();
    for source in sources {
        let count = page_count(size(get(source, "byte_count"))?);
        if number >= cursor && number - cursor < count {
            let offset = (number - cursor) * PAGE_BYTES;
            let body = source_bytes(&op.directory, source)?;
            let part = body
                .get(offset..body.len().min(offset + PAGE_BYTES))
                .ok_or_else(|| err("Recovery page is outside the complete source inventory"))?;
            let mut value = object([
                ("schema", string("agent-memory/layout-source-page-v1")),
                ("operation_id", string(operation_id)),
                ("manifest_sha256", string(sha(&op.manifest)?)),
                ("page", crate::layout_resolution::number(number)),
                ("source_id", field(source, "source_id")?.clone()),
                ("path", field(source, "path")?.clone()),
                ("offset", crate::layout_resolution::number(offset)),
                ("byte_count", crate::layout_resolution::number(part.len())),
                (
                    "source_byte_count",
                    crate::layout_resolution::number(body.len()),
                ),
                ("encoding", string("base64")),
                ("body", string(base64(part))),
                (
                    "source_complete",
                    Value::Bool(offset + part.len() == body.len()),
                ),
                (
                    "next_page",
                    if number + 1 < pages {
                        crate::layout_resolution::number(number + 1)
                    } else {
                        Value::Null
                    },
                ),
            ]);
            let token = sha(&value)?;
            value = with(value, [("page_token", string(token))]);
            require(
                encoded(&value)?.len() <= sharing_limits::MAX_PAGE,
                "Complete recovery page exceeds its encoded bound",
            )?;
            return Ok(value);
        }
        cursor += count;
    }
    Err(err(
        "Recovery page is outside the complete source inventory",
    ))
}
/// Production implementation of the layout_restore/context/units registration.
pub struct LayoutRecovery;
impl crate::layout_restore::LayoutRecovery for LayoutRecovery {
    fn fields(&self, v: &Value, n: &[&str], s: Option<&str>) -> Result<()> {
        fields(v, n, s)
    }
    fn independent(&self, a: &Value, r: &Value) -> Result<()> {
        independent(a, r)
    }
    fn result(&self, o: &str, id: Option<&str>, v: Vec<(&'static str, Value)>) -> Result<Value> {
        Ok(result(o, id, v))
    }
    fn operation(&self, r: &Path, id: &str, rt: &Path) -> Result<Operation> {
        operation(r, id, rt)
    }
    fn recovery_source(&self, r: &Path, id: &str, rt: &Path) -> Result<RecoverySource> {
        recovery_source(r, id, rt)
    }
    fn receipt_value(&self, d: &Path, p: &Value) -> Result<Option<Value>> {
        receipt_value(d, p)
    }
    fn recover(&self, r: &Path, id: &str, m: &str, rt: &Path) -> Result<Value> {
        recover(r, Some(id), Some(m), None, rt)
    }
    fn hold(&self, p: &Value) -> Result<Vec<u8>> {
        hold(p)
    }
    fn view(&self, b: &Path, i: &[String]) -> Result<Vec<Value>> {
        view(b, i)
    }
    fn expected_result(&self, p: &Value, m: &Value, c: &[Value]) -> Result<()> {
        expected_result(p, m, c, false)
    }
    fn ledger_value(&self, p: &Value) -> Result<Value> {
        ledger_value(p)
    }
    fn manifest_view(&self, m: &Value) -> Result<Vec<Value>> {
        manifest_view(m)
    }
    fn rebind(
        &self,
        r: &Path,
        m: &Value,
        o: Option<&[u8]>,
        t: Option<&[u8]>,
        rt: &Path,
    ) -> Result<()> {
        rebind(r, m, o, t, rt)
    }
    fn atomic_marker(
        &self,
        b: &Path,
        before: Option<&[u8]>,
        after: &[u8],
        o: Option<&[u8]>,
    ) -> Result<()> {
        atomic_marker(b, before, after, o, MARKER, None)
    }
    fn new_directories(&self, b: &Path, p: &Value) -> Result<()> {
        new_directories(b, p)
    }
    fn replace_source(
        &self,
        r: &Path,
        b: &Path,
        p: &str,
        body: Option<&[u8]>,
        f: Option<&Value>,
        running: &[Value],
        c: &crate::layout_recovery_context::RecoveryReadSet,
        rt: &Path,
    ) -> Result<()> {
        replace_source(r, b, p, body, f, running, c, rt)
    }
    fn eligibility(&self, r: &Path, o: Option<&[u8]>, t: Option<&[u8]>) -> Result<Value> {
        Ok(eligibility(r, o, t))
    }
    fn proposal_value(&self, r: &Path, p: &Value, rt: &Path) -> Result<()> {
        proposal_value(r, p, rt).map(|_| ())
    }
    fn partial_boundary(&self, p: &Value, m: &Value, c: &[Value]) -> Result<usize> {
        partial_boundary(p, m, c)
    }
    fn unit_map_value(&self, v: &Value, m: &Value, i: &Value) -> Result<()> {
        unit_map_value(v, m, i, None)
    }
    fn pending_rows(&self, d: &Path, m: &Value, u: &[Value]) -> Result<Vec<Value>> {
        pending_rows(d, m, u)
    }
}

fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut encoded = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let a = chunk.first().copied().unwrap_or(0);
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        // Invariant: each alphabet index below contains exactly six bits.
        encoded.push(ALPHABET[usize::from(a >> 2)] as char);
        encoded.push(ALPHABET[usize::from(((a & 3) << 4) | (b >> 4))] as char);
        encoded.push(if chunk.len() > 1 {
            ALPHABET[usize::from(((b & 15) << 2) | (c >> 6))] as char
        } else {
            '='
        });
        encoded.push(if chunk.len() > 2 {
            ALPHABET[usize::from(c & 63)] as char
        } else {
            '='
        });
    }
    encoded
}
