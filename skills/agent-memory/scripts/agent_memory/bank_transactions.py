"""Recoverable per-file bank transactions; mechanical, not admission authority."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import uuid

from . import bank_contract as contract
from . import bank_writes as writes
from . import local_registry as storage
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError
from .lifecycle import time_value

MARKER = ".agent-memory-transaction.md"
SCHEMA = "agent-memory/transaction-proposal-v1"
FIELDS = {"schema", "operation_id", "operation", "repo_root", "project_id", "observed_hashes", "files", "scope_context"}
RECOVERY_SCHEMA = "agent-memory/transaction-recovery-v1"
RECOVERY_FIELDS = {"schema", "operation_id", "repo_root", "mode", "journal_sha256", "marker_sha256", "observed_hashes"}
RECEIPT_FIELDS = {"schema", "operation_id", "request_sha256", "bank_path", "project_id", "observed_hashes",
                  "resulting_hashes", "outcome", "changed_memory_paths", "readback_at", "verified"}


def result(operation, **fields):
    return {"schema": "agent-memory/transaction-result-v1", "operation": operation, **fields}


def file_hashes(files):
    return {path: digest(body.encode("utf-8")) for path, body in files.items()}


def files_at(bank):
    # The shared inventory refuses all symlinks, including runtime and parents.
    observed = writes.hashes(bank)
    files = {path: (bank / path).read_bytes().decode("utf-8") for path in observed}
    if file_hashes(files) != observed or writes.hashes(bank) != observed:
        raise ContractError("Bank changed during transaction inventory")
    return files


def proposal_value(prepared):
    from .scope_binding import context_value
    p = prepared.get("proposal", prepared) if isinstance(prepared, dict) else prepared
    if (not isinstance(p, dict) or set(p) not in (FIELDS, FIELDS | {"initialization_evidence"}, FIELDS | {"admission_evidence"}, FIELDS | {"consolidation_evidence"}) or p["schema"] != SCHEMA
            or not uuid_valid(p["operation_id"]) or not uuid_valid(p["project_id"])
            or p["operation"] not in writes.OPERATIONS or not isinstance(p["repo_root"], str)
            or not Path(p["repo_root"]).is_absolute() or str(Path(p["repo_root"]).resolve()) != p["repo_root"]):
        raise ContractError("Invalid bank transaction proposal")
    if 'initialization_evidence' in p and p['operation'] != 'initialize':
        raise ContractError('Source initialization evidence belongs only to initialization')
    if 'admission_evidence' in p and p['operation'] != 'capture':
        raise ContractError('Admission evidence belongs only to its governed capture transaction')
    if 'consolidation_evidence' in p and p['operation'] != 'compact':
        raise ContractError('Consolidation evidence belongs only to governed maintenance')
    contract.validate_files(p["files"])
    first_bank = p['operation'] == 'initialize' and p['observed_hashes'] == {}
    context_value(p['scope_context'], initialize=first_bank)
    if p["operation"] == "checkpoint":
        raise ContractError("Checkpoint must use the single-file activeContext replacement")
    if not isinstance(p["observed_hashes"], dict) or (not first_bank and "MEMORY.md" not in p["observed_hashes"]):
        raise ContractError("Transaction requires a complete observed existing bank")
    if first_bank and not set(p['files']).issubset({'MEMORY.md', *contract.CONTEXTS}):
        raise ContractError('First initialization creates context orientation only; optional records require governed admission')
    for path, sha in p["observed_hashes"].items():
        if path != "MEMORY.md":
            contract.path_value(path)
        if not isinstance(sha, str) or not re.fullmatch("[0-9a-f]{64}", sha):
            raise ContractError("Observed hashes must be SHA256 values")
    after_id = contract.validate_files(p["files"])["project_id"]
    if after_id != p["project_id"] and p["operation"] != "bind":
        raise ContractError("Project identity changes require an explicit binding operation")
    return p


def preview(repo, files, operation="compact"):
    from .scope_binding import capture
    root, bank = writes.bank_path(repo)
    checked, hashes = writes.snapshot(root)
    p = proposal_value({"schema": SCHEMA, "operation_id": str(uuid.uuid4()), "operation": operation,
                        "repo_root": str(root), "project_id": checked["project_id"], "observed_hashes": hashes,
                        "files": files, "scope_context": capture(root)})
    preflight(bank, p)
    return result("preview", proposal=p, writes_performed=False, verified=False)


def check_layout(bank, participants, observed):
    contract.bank_root(bank)
    contract.portable_aliases([*contract.physical_paths(bank), *participants])
    for path in participants:
        target = bank / path
        if target.exists() and (not target.is_file() or path not in observed):
            raise ContractError("Canonical target has an unobserved physical occupant; preserve it for inspection")
        for parent in target.parents:
            if parent == bank:
                break
            if parent.exists() and not parent.is_dir():
                raise ContractError("Canonical target has a non-directory parent; preserve the existing layout")


def preflight(bank, p, observed=None):
    # File/directory shape changes cannot use the same per-file rollback order.
    # Refuse them before a journal/marker rather than accepting an unrealizable
    # desired bank. Unrelated non-Markdown occupants must also be preserved.
    participants = set(p["observed_hashes"]) | set(p["files"])
    contract.file_topology(participants)
    check_layout(bank, participants, p["observed_hashes"] if observed is None else observed)


def locations(root, bank, operation_id):
    operations, receipt = writes.state_paths(root, bank, operation_id)
    directory = operations.parent / "transactions" / operation_id
    for part in (directory, *directory.parents):
        if part.is_symlink() or (part.exists() and not part.is_dir()):
            raise ContractError("Transaction journal directories must be canonical directories")
    journal = directory / "journal.json"
    if journal.is_symlink():
        raise ContractError("Transaction journal must not be a symlink")
    return journal, receipt


def changed(p):
    final = file_hashes(p["files"])
    return sorted(path for path in set(p["observed_hashes"]) | set(final) if p["observed_hashes"].get(path) != final.get(path))


def marker_bytes(p):
    # Operation identifiers, relative paths and base hashes only; never bodies,
    # source excerpts, developer labels, raw remotes or caller-supplied prose.
    return encoded({"schema": "agent-memory/transaction-marker-v1", "transaction_id": p["operation_id"],
                    "operation": p["operation"], "phase": "applying", "paths": changed(p), "base_hashes": p["observed_hashes"]})


def flush_directory(path):
    fd = os.open(path, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def create_marker(path, payload):
    fd = os.open(path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    try:
        with os.fdopen(fd, "wb", closefd=False) as stream:
            stream.write(payload); stream.flush(); os.fsync(stream.fileno())
        flush_directory(path.parent)
    finally:
        os.close(fd)


def check_marker(bank, expected):
    path = bank / MARKER
    if path.is_symlink() or not path.is_file() or path.read_bytes() != expected:
        raise ContractError("Transaction marker changed or is unknown; preserve for inspection")


def check_observed(bank, expected):
    if writes.hashes(bank) != expected:
        raise ContractError("External edit observed; preserve it and inspect the interrupted transaction")


def replace_file(bank, path, body, expected, marker, participants):
    target = bank / path
    temporary = None
    try:
        check_marker(bank, marker)
        check_observed(bank, expected)
        check_layout(bank, participants, expected)
        if body is not None:
            target.parent.mkdir(parents=True, exist_ok=True)
            with tempfile.NamedTemporaryFile(prefix=".agent-memory-transaction-", suffix=".tmp", dir=target.parent, delete=False) as stream:
                temporary = Path(stream.name)
                os.chmod(temporary, target.stat().st_mode & 0o777 if target.exists() else 0o644)
                stream.write(body.encode("utf-8")); stream.flush(); os.fsync(stream.fileno())
        writes.fault("transaction-before-recheck:" + path)
        # Check the entire read set immediately before every individual change.
        check_marker(bank, marker)
        check_observed(bank, expected)
        check_layout(bank, participants, expected)
        if body is None:
            target.unlink()
        else:
            os.replace(temporary, target)
            temporary = None
        flush_directory(target.parent)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def cleanup(journal, raw):
    if journal.exists():
        if journal.is_symlink() or journal.read_bytes() != raw:
            raise ContractError("Recovery journal changed; do not discard unknown recovery data")
        journal.unlink()
        flush_directory(journal.parent)
        # Keep unrelated files rather than recursively deleting a state directory.
        if not any(journal.parent.iterdir()):
            journal.parent.rmdir()


def receipt_value(path, p):
    if not path.exists():
        return None
    if path.is_symlink():
        raise ContractError("Transaction receipt must not be a symlink")
    r = load(path.read_bytes())
    final = file_hashes(p["files"])
    if (not isinstance(r, dict) or set(r) != RECEIPT_FIELDS
            or r.get("schema") != "agent-memory/transaction-receipt-v1"
            or r.get("operation_id") != p["operation_id"] or r.get("request_sha256") != digest(encoded(p))
            or r.get("bank_path") != str(Path(p["repo_root"]) / contract.SPEC["x-bank-path"])
            or r.get("project_id") != p["project_id"]
            or not isinstance(r.get("outcome"), str) or r["outcome"] not in {"finished", "rolled-back"}
            or r.get("verified") is not True
            or r.get("observed_hashes") != p["observed_hashes"]
            or r.get("resulting_hashes") != (final if r.get("outcome") == "finished" else p["observed_hashes"])
            or r.get("changed_memory_paths") != [contract.SPEC["x-bank-path"] + "/" + path for path in changed(p)]):
        raise ContractError("Unknown receipt or reused operation ID; preserve it")
    # The complete schema/bindings above make every UUID, path and hash equal
    # to an already validated proposal-derived string/map/list. Timestamp
    # format does not prove source freshness or authenticate a past assertion.
    time_value(r["readback_at"])
    return r


def admission_recovery(p, mode, known):
    if (('admission_evidence' in p or 'consolidation_evidence' in p) and mode == 'finish'
            and (known is None or known['outcome'] != 'finished')):
        raise ContractError('Unfinished governed learning requires rollback, then fresh evidence and comparison; preserve the journal')


def journal_value(raw, root, bank, operation_id, *, require_marker=True):
    j = load(raw)
    if not isinstance(j, dict) or set(j) != {"schema", "proposal", "base_files"} or j["schema"] != "agent-memory/transaction-journal-v1":
        raise ContractError("Unknown transaction journal; operator inspection required")
    p = proposal_value(j["proposal"])
    empty_base = p['operation'] == 'initialize' and p['observed_hashes'] == {} and j['base_files'] == {}
    checked = {'project_id': p['project_id']} if empty_base else contract.validate_files(j["base_files"])
    if (p["repo_root"] != str(root) or p["operation_id"] != operation_id
            or file_hashes(j["base_files"]) != p["observed_hashes"] or checked["project_id"] != p["project_id"]):
        raise ContractError("Recovery journal does not match the original bank/operation")
    if require_marker:
        check_marker(bank, marker_bytes(p))
    return j, p


def allowed_partial(p, observed):
    base, final = p["observed_hashes"], file_hashes(p["files"])
    if any(observed.get(path) not in {base.get(path), final.get(path)} for path in set(base) | set(final) | set(observed)):
        raise ContractError("Interrupted bank contains an external edit; automatic recovery would overwrite it")


def converge(root, bank, p, base_files, journal, raw, receipt_path, mode, observed, owner, *, recovery=False):
    marker = marker_bytes(p)
    known = receipt_value(receipt_path, p)
    if recovery:
        admission_recovery(p, mode, known)
    # An exact verified outcome is already committed. Recovery may clean its
    # marker, but cannot replay bytes or demand new semantic/source evidence.
    historical = recovery and known is not None
    outcome = "finished" if mode == "finish" else "rolled-back"
    if known and known["outcome"] != outcome:
        raise ContractError("Transaction already has a different verified outcome; use a new reviewed operation")
    desired = p["files"] if mode == "finish" else base_files
    expected = file_hashes(desired)
    running = dict(observed)
    participants = set(p["observed_hashes"]) | set(p["files"])
    paths = sorted((path for path in set(running) | set(expected) if running.get(path) != expected.get(path)),
                   key=lambda path: (path == "MEMORY.md", path))
    if known and paths:
        raise ContractError("Bank differs from a recorded outcome; do not replay over newer edits")
    for path in paths:
        replace_file(bank, path, desired.get(path), running, marker, participants)
        if path in expected:
            running[path] = expected[path]
        else:
            running.pop(path)
        writes.fault("transaction-after-write:" + path)
    check_marker(bank, marker)
    check_observed(bank, expected)
    check_layout(bank, participants, expected)
    checked = validate_outcome(root, bank, desired, p, owner, marker, admission_sources=not historical)
    writes.fault("transaction-before-receipt")
    check_marker(bank, marker)
    check_observed(bank, expected)
    check_layout(bank, participants, expected)
    validate_outcome(root, bank, desired, p, owner, marker, admission_sources=not historical)
    if not known:
        known = {"schema": "agent-memory/transaction-receipt-v1", "operation_id": p["operation_id"],
                 "request_sha256": digest(encoded(p)), "bank_path": str(bank), "project_id": checked["project_id"],
                 "observed_hashes": p["observed_hashes"], "resulting_hashes": expected, "outcome": outcome,
                 "changed_memory_paths": [contract.SPEC["x-bank-path"] + "/" + path for path in changed(p)],
                 "readback_at": datetime.now(timezone.utc).isoformat(), "verified": True}
        writes.record(receipt_path, encoded(known))
    writes.fault("transaction-after-receipt")
    check_marker(bank, marker)
    check_observed(bank, expected)
    check_layout(bank, participants, expected)
    validate_outcome(root, bank, desired, p, owner, marker, admission_sources=not historical)
    (bank / MARKER).unlink()
    flush_directory(bank)
    writes.fault("transaction-after-marker-removal")
    cleanup(journal, raw)
    return result(mode, **{k: v for k, v in known.items() if k != "schema"}, writes_performed=bool(paths),
                  already_applied=not paths, still_current=True, receipt_path=str(receipt_path), interrupted=False)


def validate_outcome(root, bank, desired, p, owner, marker, *, admission_sources=True):
    if admission_sources and 'admission_evidence' in p and desired == p['files']:
        from .admission import check_sources
        check_sources(p, owner=owner, marker=marker)
    if admission_sources and 'consolidation_evidence' in p and desired == p['files']:
        from .consolidation import check_sources as check_consolidation_sources
        check_consolidation_sources(p, owner=owner, marker=marker)
    if not desired and p['operation'] == 'initialize' and p['observed_hashes'] == {}:
        if contract.physical_paths(bank) or writes.hashes(bank):
            raise ContractError('Uninitialized rollback has an unexpected occupant; preserve it')
        return {'project_id': p['project_id']}
    return contract.validate(root, owned_lock=owner, owned_transaction=marker)


def apply(prepared):
    from .scope_binding import require
    p = proposal_value(prepared)
    if p['operation'] == 'bind':
        raise ContractError('Identity changes require scope_binding preview/apply, not an ordinary bank transaction')
    if p['scope_context'] is None:
        raise ContractError('Proposal has no resolved scope; finish identity setup and preview again')
    if p['operation'] == 'initialize':
        from .initialization import validate_transaction
        validate_transaction(p)
    if 'admission_evidence' in p:
        from .admission import validate_transaction as validate_admission
        # A recorded outcome is read back by _apply; it never reapplies bytes.
        if not locations(*writes.bank_path(p['repo_root']), p['operation_id'])[1].exists():
            validate_admission(p)
    if 'consolidation_evidence' in p:
        from .consolidation import validate_transaction as validate_consolidation
        if not locations(*writes.bank_path(p['repo_root']), p['operation_id'])[1].exists():
            validate_consolidation(p)
    first_bank = p['operation'] == 'initialize' and p['observed_hashes'] == {}
    initializing = first_bank and not (Path(p['repo_root']) / 'memory-bank/MEMORY.md').exists()
    pinned = {**p['scope_context'], 'initialization_only': initializing}
    context = require(p['repo_root'], p['project_id'], expected=pinned, initialize=initializing)
    from .project_identity import status as project_status
    proposed_remote = contract.parse_router(p['files']['MEMORY.md'])[0].get('canonical-remote')
    if proposed_remote != project_status(p['repo_root'])['evidence']['canonical_remote']:
        raise ContractError('Authority identity changes require explicit scope binding, including during initialization')
    from . import security
    classes = ['external']
    if 'admission_evidence' in p:
        classes = [p['admission_evidence']['capture']['request']['source_class']]
    if 'consolidation_evidence' in p and p['consolidation_evidence']['capture'] is not None:
        classes = [p['consolidation_evidence']['capture']['request']['source_class']]
    if p['operation'] == 'initialize':
        evidence = p['initialization_evidence']['inventory']
        classes = ['assistant'] + (['verified-repository'] if evidence['sources'] else []) + (['tool'] if evidence['awa']['sources'] else [])
    def check(owner):
        require(p['repo_root'], p['project_id'], expected=context, owned_lock=owner, initialize=initializing)
        if 'admission_evidence' in p:
            from .admission import validate_transaction as validate_admission
            if not locations(*writes.bank_path(p['repo_root']), p['operation_id'])[1].exists():
                validate_admission(p, owned_lock=owner)
            security.guarded_start(p['repo_root'], p['operation_id'], context,
                {path: p['files'][path] for path in changed(p) if path in p['files']}, classes, action='capture')
        elif 'consolidation_evidence' in p:
            from .consolidation import validate_transaction as validate_consolidation
            if not locations(*writes.bank_path(p['repo_root']), p['operation_id'])[1].exists():
                validate_consolidation(p, owned_lock=owner)
            security.guarded_start(p['repo_root'], p['operation_id'], context,
                {path: p['files'][path] for path in changed(p) if path in p['files']}, classes, action='compact')
        elif p['operation'] == 'initialize':
            validate_transaction(p)
            security.guarded_start(p['repo_root'], p['operation_id'], context,
                {path: p['files'][path] for path in changed(p) if path in p['files']}, classes, action='initialize')
        else:
            security.audit(p['repo_root'], p['operation_id'], 'mutation', 'started', context, classes, action=p['operation'])
    receipt = _apply(p, check_identity=check)
    if receipt['outcome'] == 'finished':
        receipt['security_audit'] = security.audit(p['repo_root'], p['operation_id'], 'mutation', 'completed', context, classes, action=p['operation'])
    return {**receipt, 'resolved_scope': context}


def _apply(prepared, *, check_identity):
    p = proposal_value(prepared)
    root, bank = writes.bank_path(p["repo_root"])
    journal, receipt_path = locations(root, bank, p["operation_id"])
    with storage.exclusive(bank / ".agent-memory.lock", p["operation_id"], "bank-" + p["operation"]) as owner:
        writes.fault('transaction-locked-before-inventory')
        check_identity(owner)
        known = receipt_value(receipt_path, p)
        first_bank = p['operation'] == 'initialize' and p['observed_hashes'] == {}
        if first_bank and not (bank / 'MEMORY.md').exists():
            if (bank / MARKER).exists() or contract.physical_paths(bank):
                raise ContractError('Empty-bank initialization found existing or interrupted content; preserve it')
            checked, observed = {'project_id': p['project_id']}, writes.hashes(bank)
        else:
            checked, observed = writes.snapshot(root, owner)
        if known:
            # A completed or rolled-back operation never reapplies over a correction.
            if journal.exists():
                raw = journal.read_bytes()
                _, recorded = journal_value(raw, root, bank, p["operation_id"], require_marker=False)
                if recorded != p:
                    raise ContractError("Unknown leftover journal; preserve it")
                cleanup(journal, raw)
            return result("apply", **{k: v for k, v in known.items() if k != "schema"}, writes_performed=False,
                          already_applied=True, still_current=observed == known["resulting_hashes"], receipt_path=str(receipt_path))
        if checked["project_id"] != p["project_id"] or observed != p["observed_hashes"]:
            raise ContractError("Bank changed after preview; prepare a fresh transaction")
        base_files = files_at(bank)
        if not first_bank:
            contract.validate_files(base_files)
        if file_hashes(base_files) != observed:
            raise ContractError("Bank changed before journaling")
        preflight(bank, p)
        raw = encoded({"schema": "agent-memory/transaction-journal-v1", "proposal": p, "base_files": base_files})
        journal.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        if journal.exists():
            if journal.read_bytes() != raw:
                raise ContractError("Unknown journal or reused transaction ID; preserve it")
        else:
            storage.replace(journal, raw, None)
        writes.fault("transaction-before-marker")
        check_observed(bank, observed)
        create_marker(bank / MARKER, marker_bytes(p))
        writes.fault("transaction-after-marker")
        return converge(root, bank, p, base_files, journal, raw, receipt_path, "finish", observed, owner)


def recover(repo, mode=None, prepared=None):
    if prepared is None:
        return _recover(repo, mode)
    from .scope_binding import require
    root, bank = writes.bank_path(repo)
    r = prepared.get('proposal', prepared) if isinstance(prepared, dict) else prepared
    operation_id = r.get('operation_id') if isinstance(r, dict) else None
    if not uuid_valid(operation_id):
        raise ContractError('Invalid transaction recovery operation')
    journal, receipt_path = locations(root, bank, operation_id)
    _, p = journal_value(journal.read_bytes(), root, bank, operation_id)
    admission_recovery(p, r['mode'], receipt_value(receipt_path, p))
    if p['operation'] == 'bind':
        raise ContractError('Resume identity binding through its original scope_binding bundle')
    marker = marker_bytes(p)
    initializing = p['operation'] == 'initialize' and not (bank / 'MEMORY.md').exists()
    context = require(root, p['project_id'], owned_transaction=marker, initialize=initializing)
    from . import security
    action = 'recover-' + r['mode']
    def check(owner, marker):
        require(root, p['project_id'], expected=context, owned_lock=owner, owned_transaction=marker, initialize=initializing)
        if r['mode'] == 'finish' and p['operation'] == 'initialize':
            from .initialization import validate_transaction
            validate_transaction(p)
            security.guarded_start(root, p['operation_id'], context,
                {path: p['files'][path] for path in changed(p) if path in p['files']}, ['tool'], action=action)
        else:
            security.audit(root, p['operation_id'], 'mutation', 'started', context, ['tool'], action=action)
    receipt = _recover(root, prepared=prepared, check_identity=check)
    receipt['security_audit'] = security.audit(root, p['operation_id'], 'mutation', 'completed', context, ['tool'], action=action)
    return {**receipt, 'resolved_scope': context}


def _recover(repo, mode=None, prepared=None, *, check_identity=None):
    root, bank = writes.bank_path(repo)
    if prepared is not None:
        r = prepared.get("proposal", prepared) if isinstance(prepared, dict) else prepared
        if (not isinstance(r, dict) or set(r) != RECOVERY_FIELDS or r["schema"] != RECOVERY_SCHEMA
                or not uuid_valid(r["operation_id"]) or r["repo_root"] != str(root) or r["mode"] not in {"finish", "rollback"}):
            raise ContractError("Invalid transaction recovery proposal")
        # Bank lock recovery is deliberately separate; never steal a lock by age.
        with storage.exclusive(bank / ".agent-memory.lock", r["operation_id"], "bank-recover") as owner:
            marker = bank / MARKER
            journal, receipt_path = locations(root, bank, r["operation_id"])
            raw = journal.read_bytes()
            if digest(raw) != r["journal_sha256"] or digest(marker.read_bytes()) != r["marker_sha256"]:
                raise ContractError("Recovery evidence changed after preview; inspect again")
            j, p = journal_value(raw, root, bank, r["operation_id"])
            if check_identity is None:
                raise ContractError('Recovery writer requires resolved scope')
            writes.fault("recovery-locked-before-evidence")
            check_identity(owner, marker_bytes(p))
            observed = writes.hashes(bank)
            if observed != r["observed_hashes"]:
                raise ContractError("Bank changed after recovery preview; preserve the edit")
            allowed_partial(p, observed)
            preflight(bank, p, observed)
            return converge(root, bank, p, j["base_files"], journal, raw, receipt_path, r["mode"], observed, owner, recovery=True)
    if mode not in {"finish", "rollback"}:
        raise ContractError("Recovery preview requires an explicit finish or rollback choice")
    lock = bank / ".agent-memory.lock"
    if lock.exists() or lock.is_symlink():
        raise ContractError("Bank is locked; inspect/recover the bank lock first")
    marker = bank / MARKER
    if marker.is_symlink():
        raise ContractError("Unknown transaction marker; preserve it")
    mraw = marker.read_bytes()
    m = load(mraw)
    if not isinstance(m, dict) or not uuid_valid(m.get("transaction_id")):
        raise ContractError("Unknown transaction marker; operator inspection required")
    journal, receipt_path = locations(root, bank, m["transaction_id"])
    raw = journal.read_bytes()
    j, p = journal_value(raw, root, bank, m["transaction_id"])
    admission_recovery(p, mode, receipt_value(receipt_path, p))
    if mode == 'finish' and p['operation'] == 'initialize':
        from .initialization import validate_transaction
        validate_transaction(p)
    observed = writes.hashes(bank)
    allowed_partial(p, observed)
    preflight(bank, p, observed)
    known = receipt_value(receipt_path, p)
    if known and known["outcome"] != ("finished" if mode == "finish" else "rolled-back"):
        raise ContractError("Transaction already has a different verified outcome")
    # Preview writes no locks, receipts, journals or temporary projections.
    if lock.exists() or lock.is_symlink() or marker.read_bytes() != mraw or journal.read_bytes() != raw or writes.hashes(bank) != observed:
        raise ContractError("Recovery evidence changed during inspection")
    return result("recover-preview", proposal={"schema": RECOVERY_SCHEMA, "operation_id": p["operation_id"],
                  "repo_root": str(root), "mode": mode, "journal_sha256": digest(raw), "marker_sha256": digest(mraw),
                  "observed_hashes": observed}, writes_performed=False, verified=False, interrupted=True)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    previewer = commands.add_parser("preview")
    previewer.add_argument("--repo", default=".")
    previewer.add_argument("--files", required=True, help="JSON complete canonical Markdown map")
    previewer.add_argument("--operation", choices=sorted(writes.OPERATIONS), default="compact")
    writer = commands.add_parser("apply")
    writer.add_argument("--proposal", required=True)
    recovery = commands.add_parser("recover")
    recovery.add_argument("--repo", default=".")
    recovery.add_argument("--mode", choices=["finish", "rollback"])
    recovery.add_argument("--proposal")
    args = parser.parse_args(argv)
    try:
        if args.command == "preview":
            output = preview(args.repo, load(Path(args.files).read_bytes()), args.operation)
        elif args.command == "apply":
            output = apply(load(Path(args.proposal).read_bytes()))
        else:
            output = recover(args.repo, args.mode, load(Path(args.proposal).read_bytes()) if args.proposal else None)
        print(json.dumps(output, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError) as exc:
        print(json.dumps(result(args.command, error=str(exc), verified=False, inspection_required=True)))
        return 1


if __name__ == "__main__":
    sys.exit(main())
