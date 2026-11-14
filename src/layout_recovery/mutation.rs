use super::*;
use crate::fs::{Descriptor, DirectoryChain, RenameMode};
use crate::layout_recovery_context::RecoveryReadSet;
use crate::layout_recovery_temps as temps;
use crate::layout_resolution::{io_error, lstat, operation_row, publish_index, unlink_bank_entry};
use std::ffi::OsStr;

pub fn marker(proposal: &Value, kind: &str) -> Result<Vec<u8>> {
    encoded(&object([
        ("schema", string(writes::TRANSACTION)),
        ("project_id", field(proposal, "project_id")?.clone()),
        ("operation_id", field(proposal, "operation_id")?.clone()),
        ("transaction_kind", string(kind)),
    ]))
}
pub fn hold(proposal: &Value) -> Result<Vec<u8>> {
    encoded(&object([
        ("schema", string(writes::HOLD)),
        ("project_id", field(proposal, "project_id")?.clone()),
        ("operation_id", field(proposal, "operation_id")?.clone()),
        ("hold_id", field(proposal, "hold_id")?.clone()),
    ]))
}
pub(crate) fn sentinel(proposal: &Value) -> Result<Vec<u8>> {
    encoded(&object([
        ("schema", string(writes::SENTINEL)),
        ("project_id", field(proposal, "project_id")?.clone()),
        ("operation_id", field(proposal, "operation_id")?.clone()),
    ]))
}
pub fn predecessor_marker(
    repo: &Path,
    proposal: &Value,
    runtime_root: &Path,
) -> Result<Option<Vec<u8>>> {
    let pinned = field(proposal, "predecessor")?;
    if *pinned == Value::Null {
        return Ok(None);
    }
    let index = state::index(repo, runtime_root)?;
    let id = text(pinned, "operation_id")?;
    let indexed = operation_row(&index, id)?;
    require(
        get(indexed, "state").as_str() == Some("rolled-back")
            && same(get(indexed, "project_id"), get(proposal, "project_id")),
        "The journaled predecessor is no longer a retained same-project rollback",
    )?;
    let bank = Path::new(text(&index, "bank_path")?);
    let f = frontier()?;
    let (directory, _, ledger) = f.ledger(repo, id, indexed, bank, runtime_root)?;
    f.evidence(&directory, &ledger, indexed, runtime_root)?;
    let previous = read_value(
        &directory
            .join("proposals")
            .join(format!("{}.json", text(indexed, "proposal_id")?)),
    )?;
    let value = hold(&previous)?;
    require(
        get(pinned, "marker_sha256").as_str() == Some(digest(&value).as_str()),
        "The journaled predecessor marker identity changed",
    )?;
    Ok(Some(value))
}
pub fn marker_payloads(repo: &Path, proposal: &Value, runtime_root: &Path) -> Result<Vec<Vec<u8>>> {
    let mut values = vec![
        marker(proposal, "apply")?,
        marker(proposal, "finish")?,
        marker(proposal, "rollback")?,
        hold(proposal)?,
    ];
    if let Some(previous) = predecessor_marker(repo, proposal, runtime_root)? {
        values.push(previous);
    }
    values.push(sentinel(proposal)?);
    Ok(values)
}
#[allow(clippy::too_many_arguments)]
pub fn atomic_marker(
    target: &Path,
    before: Option<&[u8]>,
    after: &[u8],
    owner: Option<&[u8]>,
    name: &str,
    transaction: Option<&[u8]>,
) -> Result<()> {
    let expected_transaction = if name == MARKER { before } else { transaction };
    bank_observation::markers(target, owner, expected_transaction)?;
    let f = frontier()?;
    require(
        f.marker_bytes(target, name, None)?.as_deref() == before,
        "Owned recovery marker changed before replacement",
    )?;
    if before.is_none() {
        return tx::create_marker(&target.join(name), after);
    }
    let decoded = crate::developer_identity::load(after)?;
    let id = text(&decoded, "operation_id")?;
    require(
        uuid(get(&decoded, "operation_id")),
        "Recovery proposal has invalid stable identities",
    )?;
    let temporary = format!(".agent-memory-layout-marker-{id}.tmp");
    let directory = Descriptor::open_directory(target).map_err(io_error)?;
    let mut file = directory
        .create_exclusive(OsStr::new(&temporary), libc::O_WRONLY, 0o600)
        .map_err(io_error)?;
    let mut installed = false;
    let outcome = (|| {
        file.write_all(after).map_err(io_error)?;
        file.fsync().map_err(io_error)?;
        bank_observation::markers(target, owner, expected_transaction)?;
        require(
            f.marker_bytes(target, name, None)?.as_deref() == before,
            "Owned recovery marker changed immediately before swap",
        )?;
        writes::fault("layout-recovery-before-marker-swap")?;
        crate::fs::rename_at(
            &directory,
            OsStr::new(&temporary),
            &directory,
            OsStr::new(name),
            RenameMode::Replace,
        )
        .map_err(io_error)?;
        installed = true;
        directory.fsync_directory().map_err(io_error)?;
        writes::fault("layout-recovery-after-marker-swap")
    })();
    if !installed {
        match directory.unlink_at(OsStr::new(&temporary)) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io_error(e)),
        }
    }
    outcome
}
fn normalized(rows: &[Value], ignore: &[String]) -> Result<Vec<Value>> {
    let mut out = Vec::new();
    for item in rows {
        let path = text(item, "path")?;
        if [MARKER, SENTINEL, ".agent-memory.lock"].contains(&path)
            || ignore.iter().any(|p| p == path)
        {
            continue;
        }
        out.push(if get(item, "type").as_str() == Some("directory") {
            select(
                item,
                &[
                    "path", "type", "runtime", "dev", "ino", "mode", "uid", "gid", "xattrs",
                ],
            )?
        } else {
            item.clone()
        });
    }
    Ok(out)
}
pub fn view(target: &Path, ignore: &[String]) -> Result<Vec<Value>> {
    let ignored: Vec<&str> = ignore.iter().map(String::as_str).collect();
    let inventory = raw::inventory_with_options(
        target,
        raw::InventoryOptions {
            allow_temporaries: true,
            ignored_paths: &ignored,
            ..Default::default()
        },
    )?;
    normalized(&inventory.physical_entries, ignore)
}
pub fn manifest_view(manifest: &Value) -> Result<Vec<Value>> {
    normalized(items(field(manifest, "physical_entries")?)?, &[])
}
pub fn rebind(
    root: &Path,
    manifest: &Value,
    owner: Option<&[u8]>,
    transaction: Option<&[u8]>,
    runtime_root: &Path,
) -> Result<()> {
    let target = Path::new(text(manifest, "bank_path")?);
    bank_observation::markers(target, owner, transaction)?;
    let (current, _) = raw::read_source(target, "MEMORY.md", raw::MAX_FILE)?;
    let mut identity_review = get(get(manifest, "identity"), "identity_review").clone();
    if identity_review == Value::Null {
        if let Some(transaction) = transaction {
            let current_marker = frontier()?.marker_value(transaction)?;
            if get(&current_marker, "schema").as_str() == Some(writes::TRANSACTION)
                && same(
                    get(&current_marker, "operation_id"),
                    get(manifest, "operation_id"),
                )
            {
                let (_, _, _, directory) =
                    operation_paths(root, text(manifest, "operation_id")?, runtime_root)?;
                let journal = read_value(&directory.join("journal.json"))?;
                if get(&journal, "parent_operation_id") != &Value::Null {
                    let child = crate::layout_restore::value(
                        root,
                        &read_value(
                            &directory
                                .join("proposals")
                                .join(format!("{}.json", text(&journal, "proposal_id")?)),
                        )?,
                        runtime_root,
                    )?;
                    let parent =
                        recovery_source(root, text(&child, "parent_operation_id")?, runtime_root)?;
                    let receipt = receipt_value(&parent.operation.directory, &parent.proposal)?
                        .ok_or_else(|| err("Child restore raw identity parent evidence changed"))?;
                    require(
                        get(&child, "parent_proposal_sha256").as_str()
                            == Some(sha(&parent.proposal)?.as_str())
                            && get(&child, "parent_receipt_sha256").as_str()
                                == Some(sha(&receipt)?.as_str()),
                        "Child restore raw identity parent evidence changed",
                    )?;
                    identity_review = get(
                        get(&parent.operation.manifest, "identity"),
                        "identity_review",
                    )
                    .clone();
                }
            }
        }
    }
    let resolved = raw::identity(
        root,
        &current,
        if identity_review == Value::Null {
            None
        } else {
            Some(&identity_review)
        },
        owner,
        transaction,
    )?;
    let old = field(manifest, "identity")?;
    require(
        same(get(&resolved, "scope_context"), get(old, "scope_context"))
            && same(&raw::git_pins(root)?, get(manifest, "git")),
        "Recovery identity/authority/settings/HEAD/index pins changed",
    )?;
    let info = Descriptor::pin_directory(target)
        .and_then(|d| d.stat())
        .map_err(io_error)?;
    let binding = with(
        select(
            &raw::signature(&info),
            &["dev", "ino", "mode", "uid", "gid"],
        )?,
        [("xattrs", raw::metadata_supported(target)?)],
    );
    require(
        same(&binding, get(manifest, "bank_binding")),
        "Recovery bank binding changed",
    )?;
    let stripped = |value: &Value| -> Result<Value> {
        Ok(Value::Object(
            pairs(value)?
                .iter()
                .filter(|(k, _)| {
                    !["router_sha256", "bank_project_id", "bank_canonical_remote"]
                        .contains(&k.as_str())
                })
                .cloned()
                .collect(),
        ))
    };
    require(
        same(
            &stripped(field(&resolved, "project_evidence")?)?,
            &stripped(field(old, "project_evidence")?)?,
        ),
        "Recovery authority/checkout evidence changed",
    )
}
pub fn ensure_setup(
    root: &Path,
    directory: &Path,
    proposal: &Value,
    owner: &[u8],
    predecessor: Option<&VerifiedEnrollmentHold>,
    runtime_root: &Path,
) -> Result<Vec<u8>> {
    let current = state::index(root, runtime_root)?;
    let id = text(proposal, "operation_id")?;
    let ledger = ledger_value(proposal)?;
    if let Some(existing) = get(&current, "operations").get(id) {
        let head = object([
            ("schema", string("agent-memory/layout-ledger-head-v1")),
            ("operation_id", string(id)),
            ("generation", number(0)),
            ("sha256", string(sha(&ledger)?)),
        ]);
        let expected = object([
            ("state", string("active")),
            ("project_id", field(proposal, "project_id")?.clone()),
            ("ledger_head_sha256", string(sha(&head)?)),
            ("proposal_id", field(proposal, "proposal_id")?.clone()),
            ("hold_id", field(proposal, "hold_id")?.clone()),
        ]);
        require(
            same(existing, &expected),
            "Recovery setup conflicts with the current operation generation",
        )?;
        state::write_once(&directory.join("ledger/00000000.json"), &ledger)?;
        state::write_once(&directory.join("ledger-head.json"), &head)?;
        state::bind(
            root,
            id,
            text(proposal, "project_id")?,
            text(field(proposal, "scope_context")?, "checkout_key")?,
            "active",
            runtime_root,
        )?;
        sentinel(proposal)
    } else {
        encoded(&frontier()?.install(
            root,
            id,
            proposal,
            &ledger,
            text(proposal, "hold_id")?,
            owner,
            predecessor,
            runtime_root,
        )?)
    }
}
pub fn install_sentinel(
    root: &Path,
    target: &Path,
    proposal: &Value,
    owner: &[u8],
    transaction: &[u8],
    runtime_root: &Path,
) -> Result<()> {
    let expected = sentinel(proposal)?;
    let present = frontier()?.marker_bytes(target, SENTINEL, None)?;
    if present.as_deref() == Some(expected.as_slice()) {
        return Ok(());
    }
    let previous = if get(proposal, "predecessor") == &Value::Null {
        None
    } else {
        predecessor_marker(root, proposal, runtime_root)?;
        Some(encoded(&object([
            ("schema", string(writes::SENTINEL)),
            ("project_id", field(proposal, "project_id")?.clone()),
            (
                "operation_id",
                field(field(proposal, "predecessor")?, "operation_id")?.clone(),
            ),
        ]))?)
    };
    require(
        present == previous,
        "Recovery sentinel is unknown or changed",
    )?;
    writes::fault("layout-recovery-before-sentinel")?;
    atomic_marker(
        target,
        present.as_deref(),
        &expected,
        Some(owner),
        SENTINEL,
        Some(transaction),
    )?;
    writes::fault("layout-recovery-after-sentinel")
}
pub fn set_metadata(descriptor: &Descriptor, facts: &Value) -> Result<()> {
    let info = descriptor.stat().map_err(io_error)?;
    require(
        int(get(facts, "uid")) == Some(info.uid.into())
            && int(get(facts, "gid")) == Some(info.gid.into())
            && int(get(facts, "mode")) == Some((info.mode & 0o7777).into()),
        "pending-metadata-preservation",
    )
}
#[allow(clippy::too_many_arguments)]
pub fn replace_source(
    root: &Path,
    target: &Path,
    path: &str,
    body: Option<&[u8]>,
    facts: Option<&Value>,
    running: &[Value],
    context: &RecoveryReadSet,
    runtime_root: &Path,
) -> Result<()> {
    require(
        root == context.root() && Path::new(text(context.manifest(), "bank_path")?) == target,
        "Raw replacement requires its actual internal checked bank observer",
    )?;
    let proposal = context.proposal();
    let action = items(field(proposal, "actions")?)?
        .iter()
        .find(|a| get(a, "path").as_str() == Some(path))
        .ok_or_else(|| {
            err("Raw replacement bytes/path are outside the exact reviewed operation")
        })?;
    let hash = body.map_or(Value::Null, |b| string(digest(b)));
    require(
        same(&hash, get(action, "before")) || same(&hash, get(action, "after")),
        "Raw replacement bytes/path are outside the exact reviewed operation",
    )?;
    if body.is_some() {
        let facts = facts.ok_or_else(|| {
            err("Raw replacement metadata differs from the reviewed forward/rollback plan")
        })?;
        let mut allowed = Vec::new();
        if get(proposal, "schema").as_str() == Some(crate::layout_restore::SCHEMA) {
            for rows in ["base_view", "desired_view"] {
                for row in items(field(proposal, rows)?)? {
                    if get(row, "path").as_str() == Some(path) && same(get(row, "sha256"), &hash) {
                        allowed.push(required_facts(row)?);
                    }
                }
            }
        } else {
            for row in items(field(context.manifest(), "physical_entries")?)? {
                if get(row, "path").as_str() == Some(path) && same(get(row, "sha256"), &hash) {
                    allowed.push(required_facts(row)?);
                }
            }
            if same(get(action, "after"), &hash) {
                if let Some(f) = get(get(proposal, "metadata_plan"), "files").get(path) {
                    allowed.push(f.clone());
                }
            }
        }
        require(
            allowed.iter().any(|f| same(f, facts)),
            "Raw replacement metadata differs from the reviewed forward/rollback plan",
        )?;
    }
    raw::path_value(path)?;
    require(
        same_rows(&view(target, &[])?, running),
        "External source/metadata edit observed; preserve interrupted recovery",
    )?;
    context.check(runtime_root)?;
    let relative = Path::new(path);
    let parent_path = target.join(
        relative
            .parent()
            .ok_or_else(|| err("Recovery replacement parent binding changed"))?,
    );
    let chain = DirectoryChain::open(&parent_path).map_err(io_error)?;
    let parent = chain.directory().reopen_directory().map_err(io_error)?;
    let name = relative
        .file_name()
        .ok_or_else(|| err("Recovery replacement parent binding changed"))?;
    let mut temporary: Option<String> = None;
    let outcome = (|| {
        let mut temporary_facts = Value::Null;
        let mut ignored = None;
        if let Some(body) = body {
            let facts = facts.ok_or_else(|| err("pending-metadata-preservation"))?;
            let marker = crate::developer_identity::load(context.transaction())?;
            let candidate = temps::source_name(text(&marker, "operation_id")?, path);
            let candidate = Path::new(&candidate)
                .file_name()
                .and_then(OsStr::to_str)
                .ok_or_else(|| err("Recovery temporary changed immediately before replacement"))?
                .to_owned();
            let mode = u32::try_from(
                int(field(facts, "mode")?).ok_or_else(|| err("pending-metadata-preservation"))?,
            )
            .map_err(|_| err("pending-metadata-preservation"))?;
            let mut file = parent
                .create_exclusive(OsStr::new(&candidate), libc::O_WRONLY, mode)
                .map_err(io_error)?;
            temporary = Some(candidate.clone());
            set_metadata(&file, facts)?;
            file.write_all(body).map_err(io_error)?;
            file.fsync().map_err(io_error)?;
            require(
                same(
                    &raw::metadata_supported(&parent_path.join(&candidate))?,
                    get(facts, "xattrs"),
                ),
                "pending-metadata-preservation",
            )?;
            writes::fault(&format!("layout-recovery-temporary-write:{path}"))?;
            temporary_facts =
                raw::signature(&parent.stat_at(OsStr::new(&candidate)).map_err(io_error)?);
            ignored = Some(
                path_text(
                    &relative
                        .parent()
                        .ok_or_else(|| err("Recovery replacement parent binding changed"))?
                        .join(candidate),
                )?
                .to_owned(),
            );
        }
        let ignore = ignored.iter().cloned().collect::<Vec<_>>();
        require(
            same_rows(&view(target, &ignore)?, running),
            "Raw read set changed before recovery replacement",
        )?;
        context.check(runtime_root)?;
        chain
            .recheck()
            .map_err(|_| err("Recovery replacement parent binding changed"))?;
        writes::fault(&format!("layout-recovery-before-replace:{path}"))?;
        if let Some(original) = row_at(running, path) {
            let (hash, current) = raw::read_source_hash(target, path, raw::MAX_FILE)?;
            require(
                get(original, "sha256").as_str() == Some(hash.as_str())
                    && pairs(&current)?
                        .iter()
                        .all(|(k, v)| same(v, get(original, k)))
                    && same(
                        &raw::metadata_supported(&target.join(path))?,
                        get(original, "xattrs"),
                    ),
                "Recovery source changed immediately before replacement",
            )?;
        }
        if let Some(ignored) = &ignored {
            let (actual, current) = raw::read_source_hash(target, ignored, raw::MAX_FILE)?;
            require(
                hash.as_str() == Some(actual.as_str())
                    && same(&current, &temporary_facts)
                    && same(
                        &raw::metadata_supported(&target.join(ignored))?,
                        get(
                            facts.ok_or_else(|| err("pending-metadata-preservation"))?,
                            "xattrs",
                        ),
                    ),
                "Recovery temporary changed immediately before replacement",
            )?;
        }
        if let Some(candidate) = &temporary {
            crate::fs::rename_at(
                &parent,
                OsStr::new(candidate),
                &parent,
                name,
                RenameMode::Replace,
            )
            .map_err(io_error)?;
            temporary = None;
            require(
                same(
                    &raw::metadata_supported(&target.join(path))?,
                    get(
                        facts.ok_or_else(|| err("pending-metadata-preservation"))?,
                        "xattrs",
                    ),
                ),
                "pending-metadata-preservation",
            )?;
        } else {
            parent.unlink_at(name).map_err(io_error)?;
        }
        parent.fsync_directory().map_err(io_error)?;
        writes::fault(&format!("layout-recovery-after-rename:{path}"))
    })();
    if let Some(candidate) = temporary {
        parent.unlink_at(OsStr::new(&candidate)).map_err(io_error)?;
    }
    outcome
}
pub fn new_directories(target: &Path, proposal: &Value) -> Result<()> {
    let mut directories = pairs(field(field(proposal, "metadata_plan")?, "directories")?)?
        .iter()
        .collect::<Vec<_>>();
    directories.sort_by_key(|(p, _)| (p.split('/').count(), p.as_str()));
    for (path, facts) in directories {
        raw::path_value(path)?;
        let relative = Path::new(path);
        let parent_path = target.join(
            relative
                .parent()
                .ok_or_else(|| err("pending-metadata-preservation"))?,
        );
        let chain = DirectoryChain::open(&parent_path).map_err(io_error)?;
        let parent = chain.directory().reopen_directory().map_err(io_error)?;
        let name = relative
            .file_name()
            .ok_or_else(|| err("pending-metadata-preservation"))?;
        let mode = u32::try_from(
            int(field(facts, "mode")?).ok_or_else(|| err("pending-metadata-preservation"))?,
        )
        .map_err(|_| err("pending-metadata-preservation"))?;
        let child = match parent.create_directory(name, mode) {
            Ok(child) => child,
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
                parent.open_directory_at(name).map_err(io_error)?
            }
            Err(e) => return Err(io_error(e)),
        };
        set_metadata(&child, facts)?;
        require(
            same(
                &raw::metadata_supported(&target.join(path))?,
                get(facts, "xattrs"),
            ),
            "pending-metadata-preservation",
        )?;
        chain.recheck().map_err(io_error)?;
        child.fsync_directory().map_err(io_error)?;
        parent.fsync_directory().map_err(io_error)?;
    }
    Ok(())
}
pub(crate) fn remove_directories(target: &Path, proposal: &Value) -> Result<()> {
    let mut directories = pairs(field(field(proposal, "metadata_plan")?, "directories")?)?
        .iter()
        .collect::<Vec<_>>();
    directories.sort_by(|(a, _), (b, _)| {
        b.split('/')
            .count()
            .cmp(&a.split('/').count())
            .then(a.cmp(b))
    });
    for (path, facts) in directories {
        let location = target.join(path);
        let Some(info) = lstat(&location)? else {
            continue;
        };
        require(
            info.is_directory(),
            "Created directory gained an unrelated occupant; preserve it",
        )?;
        let child = Descriptor::open_directory(&location).map_err(io_error)?;
        require(
            child.entries_bounded(0).map_err(io_error)?.is_some(),
            "Created directory gained an unrelated occupant; preserve it",
        )?;
        set_metadata(&child, facts)
            .map_err(|_| err("Recovery directory metadata changed before rollback removal"))?;
        require(
            same(&raw::metadata_supported(&location)?, get(facts, "xattrs")),
            "Recovery directory metadata changed before rollback removal",
        )?;
        let parent =
            DirectoryChain::open(location.parent().ok_or_else(|| {
                err("Recovery directory metadata changed before rollback removal")
            })?)
            .map_err(io_error)?;
        parent.recheck().map_err(io_error)?;
        let name = location
            .file_name()
            .ok_or_else(|| err("Recovery directory metadata changed before rollback removal"))?;
        require(
            parent
                .directory()
                .stat_at(name)
                .map_err(io_error)?
                .same_inode(child.stat().map_err(io_error)?),
            "Recovery directory metadata changed before rollback removal",
        )?;
        parent
            .directory()
            .remove_directory_at(name)
            .map_err(io_error)?;
        parent.directory().fsync_directory().map_err(io_error)?;
    }
    Ok(())
}
pub fn expected_result(
    proposal: &Value,
    manifest: &Value,
    current: &[Value],
    rollback: bool,
) -> Result<()> {
    let physical = items(field(manifest, "physical_entries")?)?;
    let mut desired: BTreeMap<&str, (String, Value)> = BTreeMap::new();
    if rollback {
        for source in items(field(manifest, "sources")?)? {
            let path = text(source, "path")?;
            let row = row_at(physical, path).ok_or_else(|| {
                err("Recovery result contains missing or unknown source occupants")
            })?;
            desired.insert(
                path,
                (text(source, "sha256")?.to_owned(), required_facts(row)?),
            );
        }
    } else {
        for (path, body) in pairs(field(proposal, "intended_files")?)? {
            desired.insert(path,(digest(body.as_str().ok_or_else(|| err("Recovery readback bytes/required metadata do not match the reviewed outcome"))?.as_bytes()),field(field(field(proposal,"metadata_plan")?,"files")?,path)?.clone()));
        }
        for source in items(field(manifest, "sources")?)? {
            if items(field(proposal, "retained_source_ids")?)?
                .iter()
                .any(|id| same(id, get(source, "source_id")))
            {
                let path = text(source, "path")?;
                desired.insert(
                    path,
                    (
                        text(source, "sha256")?.to_owned(),
                        required_facts(row_at(physical, path).ok_or_else(|| {
                            err("Recovery result contains missing or unknown source occupants")
                        })?)?,
                    ),
                );
            }
        }
    }
    let actual = current
        .iter()
        .filter(|r| {
            get(r, "type").as_str() == Some("file") && get(r, "runtime").as_bool() != Some(true)
        })
        .map(|r| Ok((text(r, "path")?, r)))
        .collect::<Result<BTreeMap<_, _>>>()?;
    require(
        actual.keys().copied().collect::<BTreeSet<_>>() == desired.keys().copied().collect(),
        "Recovery result contains missing or unknown source occupants",
    )?;
    for (path, (hash, facts)) in desired {
        let row = actual.get(path).ok_or_else(|| {
            err("Recovery readback bytes/required metadata do not match the reviewed outcome")
        })?;
        require(
            get(row, "sha256").as_str() == Some(hash.as_str())
                && same(&required_facts(row)?, &facts),
            "Recovery readback bytes/required metadata do not match the reviewed outcome",
        )?;
    }
    let original = manifest_view(manifest)?;
    for row in &original {
        if get(row, "runtime").as_bool() == Some(true)
            || get(row, "type").as_str() == Some("directory")
        {
            require(
                row_at(current, text(row, "path")?).is_some_and(|r| same(r, row)),
                "Unrelated physical/runtime occupant changed",
            )?;
        }
    }
    let extra = current
        .iter()
        .filter(|r| {
            get(r, "type").as_str() == Some("directory")
                && row_at(&original, get(r, "path").as_str().unwrap_or("")).is_none()
        })
        .map(|r| text(r, "path"))
        .collect::<Result<BTreeSet<_>>>()?;
    let dirs = field(field(proposal, "metadata_plan")?, "directories")?;
    require(
        extra
            == if rollback {
                BTreeSet::new()
            } else {
                crate::layout_resolution::keys(dirs)
            },
        "Unknown directory introduced into recovery result",
    )?;
    for path in extra {
        require(
            same(
                &required_facts(row_at(current, path).ok_or_else(|| {
                    err("Recovery directory metadata readback differs from the reviewed plan")
                })?)?,
                field(dirs, path)?,
            ),
            "Recovery directory metadata readback differs from the reviewed plan",
        )?;
    }
    Ok(())
}
pub fn finish_index(
    root: &Path,
    proposal: &Value,
    outcome: &str,
    owner: &[u8],
    transaction: &[u8],
    runtime_root: &Path,
) -> Result<()> {
    let mut index = state::index(root, runtime_root)?;
    let (_, bank, base, _) = operation_paths(root, text(proposal, "operation_id")?, runtime_root)?;
    let pending = !items(field(proposal, "pending_units")?)?.is_empty()
        || !items(field(proposal, "inherited_unit_ids")?)?.is_empty();
    let row = index
        .get_mut("operations")
        .and_then(|r| r.get_mut(text(proposal, "operation_id").ok()?))
        .ok_or_else(|| err("Recovery setup conflicts with the current operation generation"))?;
    crate::layout_resolution::set(
        row,
        "state",
        string(if outcome == "rolled-back" {
            "rolled-back"
        } else if pending {
            "active"
        } else {
            "terminal"
        }),
    );
    publish_index(root, &base, &index, runtime_root)?;
    state::bind(
        root,
        text(proposal, "operation_id")?,
        text(proposal, "project_id")?,
        text(field(proposal, "scope_context")?, "checkout_key")?,
        if pending { "active" } else { "terminal" },
        runtime_root,
    )?;
    writes::fault("layout-recovery-final-index")?;
    bank_observation::markers(&bank, Some(owner), Some(transaction))?;
    if pending {
        atomic_marker(
            &bank,
            Some(transaction),
            &hold(proposal)?,
            Some(owner),
            MARKER,
            None,
        )?;
    } else {
        unlink_bank_entry(&bank, MARKER)?;
        tx::flush_directory(&bank)?;
        writes::fault("layout-recovery-marker-removal")?;
        require(
            frontier()?.marker_bytes(&bank, SENTINEL, None)?.as_deref()
                == Some(sentinel(proposal)?.as_slice()),
            "Sentinel changed before retirement",
        )?;
        writes::fault("layout-recovery-before-sentinel-retirement")?;
        unlink_bank_entry(&bank, SENTINEL)?;
        tx::flush_directory(&bank)?;
        writes::fault("layout-recovery-sentinel-retirement")?;
    }
    frontier()?.invalidate(root)?;
    Ok(())
}
