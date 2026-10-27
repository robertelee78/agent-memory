"""Common lock and observed single-file replacement, independent of admission."""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import re
import sys
import tempfile
import time
import uuid

from . import repository_config
from . import bank_contract as contract
from . import local_registry as storage
from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError

OPERATIONS = {"replace", "checkpoint", "initialize", "bind", "capture", "compact", "correct", "supersede", "revoke", "forget", "refresh", "recover"}
PROPOSAL_FIELDS = {"schema", "operation_id", "operation", "repo_root", "project_id", "observed_hashes", "path", "content", "scope_context"}


def bank_path(repo):
    root = Path(repo).resolve()
    bank = repository_config.bank_path(root)
    contract.bank_root(bank)
    if bank.is_symlink() or not bank.is_dir():
        raise ContractError("An existing canonical bank directory is required; initialization uses the multi-file protocol")
    return root, bank


def runtime(path):
    return contract.runtime_path(path)


def hashes(bank):
    from . import bank_observation
    return bank_observation.scan(bank, read_sources=False, require_router=False)[1]


def snapshot(repo, owned_lock=None):
    from . import bank_observation
    observed = bank_observation.observe(repo, owner=owned_lock)
    return observed.checked, observed.hashes


def state_paths(repo, bank, operation_id):
    raw = os.environ.get("AGENT_MEMORY_STATE_HOME")
    root = Path(raw or str(Path(os.environ.get("XDG_STATE_HOME", str(Path.home() / ".local/state"))) / "agent-memory")).expanduser()
    if (not root.is_absolute() or any(p.is_symlink() for p in (root, *root.parents))
            or root.resolve().is_relative_to(repo)):
        raise ContractError("Write receipts require a canonical absolute state root outside the repository")
    directory = root / "writes" / digest(str(bank).encode()) / "operations"
    for part in (directory, *directory.parents):
        if part.is_symlink() or (part.exists() and not part.is_dir()):
            raise ContractError("Write receipt directories must be canonical directories without symlinks")
    receipt = directory / (operation_id + ".json")
    if receipt.is_symlink():
        raise ContractError("Write receipt must not be a symlink")
    return directory, receipt


def result(operation, **fields):
    return {"schema": "agent-memory/write-result-v1", "operation": operation, **fields}


def preview(repo, path, content, operation="replace"):
    from .scope_binding import capture
    root, _ = bank_path(repo)
    checked, observed = snapshot(root)
    proposal = {"schema": "agent-memory/write-proposal-v1", "operation_id": str(uuid.uuid4()), "operation": operation,
                "repo_root": str(root), "project_id": checked["project_id"], "observed_hashes": observed,
                "path": path, "content": content, "scope_context": capture(root)}
    proposal_value(proposal)
    return result("preview", proposal=proposal, writes_performed=False, verified=False, changed_memory_paths=[])


def proposal_value(prepared):
    from .scope_binding import context_value
    proposal = prepared.get("proposal", prepared) if isinstance(prepared, dict) else prepared
    if (not isinstance(proposal, dict) or set(proposal) not in (PROPOSAL_FIELDS, PROPOSAL_FIELDS | {'checkpoint_evidence'}) or proposal["schema"] != "agent-memory/write-proposal-v1"
            or not uuid_valid(proposal["operation_id"]) or not uuid_valid(proposal["project_id"])
            or proposal["operation"] not in OPERATIONS or not isinstance(proposal["repo_root"], str)
            or not Path(proposal["repo_root"]).is_absolute() or str(Path(proposal["repo_root"]).resolve()) != proposal["repo_root"]):
        raise ContractError("Invalid observed-write proposal")
    contract.path_value(proposal["path"])
    context_value(proposal['scope_context'])
    if proposal["path"] == "MEMORY.md":
        raise ContractError("Router/identity changes require the governed multi-file or binding operation")
    if not isinstance(proposal["content"], str) or not proposal["content"].strip():
        raise ContractError("Single-file replacement requires nonempty Markdown; deletion uses the governed forget operation")
    proposal["content"].encode("utf-8")
    observed = proposal["observed_hashes"]
    if not isinstance(observed, dict) or proposal["path"] not in observed or "MEMORY.md" not in observed:
        raise ContractError("Proposal must include the complete observed bank and an existing target")
    for path, sha in observed.items():
        if path != "MEMORY.md":
            contract.path_value(path)
        if not isinstance(sha, str) or not re.fullmatch("[0-9a-f]{64}", sha):
            raise ContractError("Observed hashes must be SHA256 values")
    if 'checkpoint_evidence' in proposal:
        from .checkpoint import validate_write
        validate_write(proposal, sources=False)
    return proposal


def fault(stage):
    """Explicit isolated-world failpoints; never enabled by memory or proposals."""
    if os.environ.get("AGENT_MEMORY_FAULT_STAGE") != stage:
        return
    root = Path(os.environ.get("AGENT_MEMORY_TEST_ROOT", "/missing-test-root"))
    token = os.environ.get("AGENT_MEMORY_TEST_TOKEN")
    if (not token or not root.is_absolute() or root.is_symlink() or root.resolve().parent != Path("/tmp").resolve()
            or not root.name.startswith("agent-memory-test-")
            or load((root / ".test-world.json").read_bytes()).get("token") != token):
        raise ContractError("Fault injection requires an explicit disposable test world")
    mode = os.environ.get("AGENT_MEMORY_FAULT_MODE", "error")
    if mode == "exit":
        os._exit(86)
    if mode == "pause":
        directory = Path(os.environ["AGENT_MEMORY_FAULT_DIR"])
        if not directory.is_absolute() or not directory.resolve().is_relative_to(root.resolve()) or directory.is_symlink():
            raise ContractError("Fault rendezvous must stay in the disposable world")
        directory.mkdir(parents=True, exist_ok=True)
        leaf = stage.replace("/", "%2F")
        (directory / (leaf + ".ready")).touch()
        deadline = time.monotonic() + 10
        while not (directory / (leaf + ".release")).exists():
            if time.monotonic() > deadline:
                raise ContractError("Isolated fault rendezvous timed out")
            time.sleep(0.01)
    else:
        raise ContractError("Injected write failure at " + stage)


def record(receipt_path, payload):
    receipt_path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    # Cooperating writers hold the bank lock. Preserve an unexpected receipt
    # with the same immediate observed-byte guard used for metadata writes.
    storage.replace(receipt_path, payload, None)


def apply(prepared):
    from .scope_binding import require
    proposal = proposal_value(prepared)
    if proposal['scope_context'] is None:
        raise ContractError('Proposal has no resolved scope; finish identity setup and preview again')
    context = require(proposal['repo_root'], proposal['project_id'], expected=proposal['scope_context'])
    if 'checkpoint_evidence' in proposal:
        from .checkpoint import validate_write
        root, bank = bank_path(proposal['repo_root'])
        validate_write(proposal, sources=not state_paths(root, bank, proposal['operation_id'])[1].exists())
    from . import security
    def check(owner):
        require(proposal['repo_root'], proposal['project_id'], expected=context, owned_lock=owner)
        if 'checkpoint_evidence' in proposal:
            from .checkpoint import validate_write
            root, bank = bank_path(proposal['repo_root'])
            validate_write(proposal, sources=not state_paths(root, bank, proposal['operation_id'])[1].exists())
        if proposal['operation'] == 'checkpoint':
            security.guarded_start(proposal['repo_root'], proposal['operation_id'], context,
                {proposal['path']: proposal['content']}, ['assistant'], action='checkpoint')
        else:
            security.audit(proposal['repo_root'], proposal['operation_id'], 'mutation', 'started',
                context, ['external'], action=proposal['operation'])
    receipt = _apply(proposal, check_identity=check)
    audit = security.audit(proposal['repo_root'], proposal['operation_id'], 'mutation', 'completed',
        context, ['assistant'] if proposal['operation'] == 'checkpoint' else ['external'], action=proposal['operation'])
    return {**receipt, 'resolved_scope': context, 'security_audit': audit}


def _apply(prepared, *, check_identity):
    proposal = proposal_value(prepared)
    root, bank = bank_path(proposal["repo_root"])
    directory, receipt_path = state_paths(root, bank, proposal["operation_id"])
    request_sha = digest(encoded(proposal))
    target = bank / proposal["path"]
    expected = dict(proposal["observed_hashes"])
    expected[proposal["path"]] = digest(proposal["content"].encode("utf-8"))
    temporary = None
    # No bank inventory or previous receipt is read until this lock is acquired.
    with storage.exclusive(bank / ".agent-memory.lock", proposal["operation_id"], "bank-" + proposal["operation"]) as owner:
        fault("locked-before-inventory")
        check_identity(owner)
        checked, observed = snapshot(root, owner)
        if checked["project_id"] != proposal["project_id"]:
            raise ContractError("Bank project identity changed after preview")
        if receipt_path.exists():
            known = load(receipt_path.read_bytes())
            if (not isinstance(known, dict) or known.get("schema") != "agent-memory/write-receipt-v1"
                    or known.get("operation_id") != proposal["operation_id"] or known.get("request_sha256") != request_sha
                    or known.get("project_id") != proposal["project_id"] or known.get("bank_path") != str(bank)
                    or known.get("resulting_hashes") != expected or known.get("verified") is not True):
                raise ContractError("Unknown receipt or operation ID reused for another proposal; preserve it")
            return result("apply", **{k: v for k, v in known.items() if k != "schema"}, already_applied=True,
                          writes_performed=False, still_current=observed == expected, receipt_path=str(receipt_path))
        already_present = observed == expected
        if observed != proposal["observed_hashes"] and not already_present:
            raise ContractError("Bank changed after preview; validate the external edit and prepare a fresh proposal")
        wrote = False
        try:
            if not already_present:
                with tempfile.NamedTemporaryFile(prefix=".agent-memory-write-", suffix=".tmp", dir=target.parent, delete=False) as stream:
                    temporary = Path(stream.name)
                    os.chmod(temporary, target.stat().st_mode & 0o777)
                    stream.write(proposal["content"].encode("utf-8")); stream.flush(); os.fsync(stream.fileno())
                fault("before-recheck")
                if 'checkpoint_evidence' in proposal:
                    check_identity(owner)
                if snapshot(root, owner)[1] != proposal["observed_hashes"]:
                    raise ContractError("External edit observed immediately before replacement; preserve it")
                os.replace(temporary, target)
                temporary = None
                wrote = True
                directory_fd = os.open(target.parent, os.O_RDONLY)
                try:
                    os.fsync(directory_fd)
                finally:
                    os.close(directory_fd)
                fault("after-replace")
            actual = snapshot(root, owner)[1]
            if actual != expected:
                raise ContractError("Post-write bank readback differs; operation may have changed memory, inspect/replay")
            fault("before-receipt")
            # A manual writer ignoring the lock can still intervene. Detect it before the receipt.
            if snapshot(root, owner)[1] != expected:
                raise ContractError("Bank changed before receipt; do not claim a verified current write")
            receipt = {"schema": "agent-memory/write-receipt-v1", "operation_id": proposal["operation_id"],
                       "project_id": proposal["project_id"], "bank_path": str(bank), "request_sha256": request_sha,
                       "observed_hashes": proposal["observed_hashes"], "resulting_hashes": expected,
                       "changed_memory_paths": [repository_config.relative(root) + "/" + proposal["path"]] if expected != proposal["observed_hashes"] else [],
                       "outcome": "replaced" if wrote else "already-present", "readback_at": datetime.now(timezone.utc).isoformat(), "verified": True}
            record(receipt_path, encoded(receipt))
            fault("after-receipt")
            return result("apply", **{k: v for k, v in receipt.items() if k != "schema"}, already_applied=False,
                          writes_performed=wrote, still_current=True, receipt_path=str(receipt_path))
        finally:
            if temporary is not None:
                temporary.unlink(missing_ok=True)


def recover(repo, prepared=None):
    # Lock removal changes runtime state only. It must stay available before
    # identity resolution, which intentionally refuses an unknown visible lock.
    _, bank = bank_path(repo)
    return result("recover", **storage.recover(bank / "MEMORY.md", bank / ".agent-memory.lock",
                  {"bank-" + op for op in OPERATIONS}, prepared, schema="agent-memory/bank-lock-recovery-v1"))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ("inspect", "preview", "recover"):
        sub = commands.add_parser(name)
        sub.add_argument("--repo", default=".")
        if name == "preview":
            sub.add_argument("--path", required=True)
            sub.add_argument("--content", required=True)
        elif name == "recover":
            sub.add_argument("--proposal")
    writer = commands.add_parser("apply")
    writer.add_argument("--proposal", required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "inspect":
            checked, observed = snapshot(args.repo)
            receipt = result("inspect", project_id=checked["project_id"], observed_hashes=observed,
                             shared_memory_paths=[repository_config.relative(args.repo) + "/" + p for p in observed], writes_performed=False)
        elif args.command == "preview":
            receipt = preview(args.repo, args.path, Path(args.content).read_text(encoding="utf-8"))
        elif args.command == "apply":
            receipt = apply(load(Path(args.proposal).read_bytes()))
        else:
            receipt = recover(args.repo, load(Path(args.proposal).read_bytes()) if args.proposal else None)
        print(json.dumps(receipt, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError) as exc:
        print(json.dumps(result(args.command, error=str(exc), verified=False, inspection_required=args.command == "apply")))
        return 1


if __name__ == "__main__":
    sys.exit(main())
