"""Explicit stable developer attribution; independent of installation and banks."""
from __future__ import annotations

import argparse
import copy
from datetime import datetime, timezone
import hashlib
import json
import os
from pathlib import Path
import re
import socket
import subprocess
import sys
import tempfile
import uuid

try:
    import fcntl
except ImportError:  # Ordinary exclusive-create writes still work; recovery refuses.
    fcntl = None

from .frontmatter import ContractError, unique_object
from .lifecycle import time_value

PACK = Path(__file__).resolve().parents[2]
SPEC = json.loads((PACK / "schemas/developers.json").read_text())


def uuid_valid(value):
    try:
        parsed = uuid.UUID(value)
        return str(parsed) == value and parsed.version == 4
    except (ValueError, TypeError, AttributeError):
        return False


def digest(data):
    return hashlib.sha256(data).hexdigest() if data is not None else None


def encoded(data):
    return (json.dumps(data, sort_keys=True, ensure_ascii=False, indent=2, allow_nan=False) + "\n").encode("utf-8")


def load(data):
    return json.loads(data, object_pairs_hook=unique_object)


def text(value, limit=512):
    if not isinstance(value, str) or not value.strip() or len(value) > limit or any(ord(c) < 32 for c in value):
        raise ContractError("Identity metadata must be bounded, nonempty single-line text")
    return value


def alias(value):
    if not isinstance(value, dict) or set(value) != {"kind", "value"} or value["kind"] not in SPEC["alias-kinds"]:
        raise ContractError("Unknown identity alias fields or kind; harness/session are separate context")
    text(value["value"])
    return value


def aliases(values):
    if not isinstance(values, list):
        raise ContractError("Identity aliases must be a list")
    keys = []
    for value in values:
        alias(value)
        keys.append((value["kind"], value["value"]))
    if len(set(keys)) != len(keys):
        raise ContractError("Duplicate identity aliases")
    return values


def labels(values):
    if not isinstance(values, list):
        raise ContractError("Identity labels must be a list")
    for value in values:
        text(value, 128)
    if len(set(values)) != len(values):
        raise ContractError("Duplicate identity labels")
    return values


def paths():
    raw = os.environ.get("AGENT_MEMORY_CONFIG_HOME")
    if raw is None:
        raw = str(Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config"))) / "agent-memory")
    root = Path(raw).expanduser()
    if not root.is_absolute():
        raise ContractError("Identity config root must be absolute")
    for part in (root, *root.parents):
        if part.is_symlink():
            raise ContractError("Identity config roots must not use symlinks; select an explicit canonical root")
    if root.exists() and not root.is_dir():
        raise ContractError("Identity config root is not a directory")
    registry, lock = root / SPEC["registry-file"], root / ".developers.lock"
    if registry.is_symlink() or lock.is_symlink():
        raise ContractError("Identity registry and lock must not be symlinks")
    return root, registry, lock


def empty():
    return {"schema": SPEC["schema"], "selected": None, "developers": {}, "operations": {}}


def validate(data):
    if not isinstance(data, dict) or set(data) != set(SPEC["registry-fields"]) or data["schema"] != SPEC["schema"]:
        raise ContractError("Unsupported attribution registry; preserve it for explicit repair")
    if not isinstance(data["developers"], dict) or not isinstance(data["operations"], dict):
        raise ContractError("Invalid attribution registry maps")
    for identity, developer in data["developers"].items():
        if not uuid_valid(identity) or not isinstance(developer, dict) or set(developer) != set(SPEC["developer-fields"]):
            raise ContractError("Registry stores stable developer IDs and identity metadata only")
        labels(developer["labels"])
        aliases(developer["aliases"])
    if data["selected"] is not None and data["selected"] not in data["developers"]:
        raise ContractError("Selected developer is not registered")
    for op_id, operation in data["operations"].items():
        if not uuid_valid(op_id) or not isinstance(operation, dict) or set(operation) != {"proposal_sha256", "developer_id", "action", "at"}:
            raise ContractError("Invalid identity operation history")
        if not isinstance(operation["proposal_sha256"], str) or not re.fullmatch("[0-9a-f]{64}", operation["proposal_sha256"]):
            raise ContractError("Invalid operation proposal hash")
        if operation["developer_id"] not in data["developers"] or operation["action"] not in SPEC["actions"]:
            raise ContractError("Invalid identity operation binding")
        text(operation["at"])
    return data


def observe():
    _, registry, _ = paths()
    raw = registry.read_bytes() if registry.exists() else None
    return validate(load(raw)) if raw is not None else empty(), digest(raw)


def result(operation, **fields):
    return {"schema": "agent-memory/developer-result-v1", "operation": operation,
            "security_boundary": SPEC["security-boundary"], "changed_memory_paths": [], **fields}


def declared(values):
    output = []
    for item in values:
        kind, separator, value = item.partition("=")
        if not separator:
            raise ContractError("Aliases use kind=value")
        output.append(alias({"kind": kind, "value": value}))
    return aliases(output)


def status(observations, harness=None, session=None):
    data, sha = observe()
    _, registry, lock = paths()
    observed = list(observations)
    for key, kind in (("user.email", "git-author-email"), ("user.name", "git-author-name")):
        try:
            query = subprocess.run(["git", "config", "--get", key], text=True, capture_output=True, timeout=5)
            if query.returncode == 0 and query.stdout.strip():
                item = alias({"kind": kind, "value": query.stdout.strip()})
                if item not in observed:
                    observed.append(item)
        except (FileNotFoundError, subprocess.TimeoutExpired):
            pass
    if os.environ.get("USER"):
        item = alias({"kind": "os-user", "value": os.environ["USER"]})
        if item not in observed:
            observed.append(item)
    evidence = []
    for item in observed:
        matches = [identity for identity, entry in data["developers"].items() if item in entry["aliases"]]
        state = "unknown" if not matches else "conflicting" if matches != [data["selected"]] else "matches-selected"
        evidence.append({**item, "matching_developer_ids": matches, "state": state})
    conflicts = [item for item in evidence if item["state"] == "conflicting"]
    return result("status", developer_id=data["selected"], registered_developers=data["developers"],
                  observed_sha256=sha, registry_path=str(registry), observations=evidence,
                  attribution_conflicts=conflicts, setup_required=data["selected"] is None,
                  identity_authenticated=False, lock_present=lock.exists(),
                  session_context={"harness": harness, "session": session}, writes_performed=False)


def preview(action, developer_id, new_labels, new_aliases):
    data, sha = observe()
    _, registry, lock = paths()
    if lock.exists():
        raise ContractError("Attribution registry is locked; inspect status/recover before retrying")
    if action == "create":
        developer_id = str(uuid.uuid4())
    if not uuid_valid(developer_id):
        raise ContractError("Developer ID must be a canonical random UUIDv4")
    if action == "select" and (developer_id not in data["developers"] or new_labels or new_aliases):
        raise ContractError("Selection requires an existing ID and does not modify aliases or labels")
    proposal = {"schema": "agent-memory/developer-proposal-v1", "operation_id": str(uuid.uuid4()),
                "action": action, "developer_id": developer_id, "labels": labels(new_labels),
                "aliases": aliases(new_aliases), "observed_sha256": sha, "registry_path": str(registry)}
    return result("preview", developer_id=developer_id, proposal=proposal, writes_performed=False, verified=False)


def proposal_value(proposal, registry):
    if isinstance(proposal, dict) and "proposal" in proposal:
        proposal = proposal["proposal"]
    if not isinstance(proposal, dict) or set(proposal) != set(SPEC["proposal-fields"]):
        raise ContractError("Invalid developer proposal fields")
    if proposal["schema"] != "agent-memory/developer-proposal-v1" or proposal["action"] not in SPEC["actions"]:
        raise ContractError("Unknown developer proposal schema/action")
    if not uuid_valid(proposal["operation_id"]) or not uuid_valid(proposal["developer_id"]):
        raise ContractError("Invalid proposal operation/developer ID")
    if proposal["registry_path"] != str(registry):
        raise ContractError("Proposal belongs to a different identity config root")
    if proposal["observed_sha256"] is not None and (not isinstance(proposal["observed_sha256"], str) or not re.fullmatch("[0-9a-f]{64}", proposal["observed_sha256"])):
        raise ContractError("Invalid observed registry hash")
    labels(proposal["labels"])
    aliases(proposal["aliases"])
    return proposal


def apply(proposal):
    root, registry, lock = paths()
    proposal = proposal_value(proposal, registry)
    request_sha = digest(encoded(proposal))
    root.mkdir(parents=True, exist_ok=True, mode=0o700)
    lock_data = {"operation_id": proposal["operation_id"], "host": socket.gethostname(), "pid": os.getpid(),
                 "operation": "developer-" + proposal["action"], "start": datetime.now(timezone.utc).isoformat()}
    try:
        descriptor = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError as exc:
        raise ContractError("Attribution registry is locked; stop and inspect status/recover") from exc
    temporary = None
    owned_inode = os.fstat(descriptor).st_ino
    try:
        with os.fdopen(descriptor, "wb", closefd=False) as stream:
            stream.write(encoded(lock_data))
            stream.flush()
            os.fsync(stream.fileno())
        if fcntl is not None:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        data, before = observe()
        known = data["operations"].get(proposal["operation_id"])
        if known is not None:
            if known["proposal_sha256"] != request_sha:
                raise ContractError("Operation ID was already used for a different proposal")
            return result("apply", operation_id=proposal["operation_id"], developer_id=data["selected"],
                          operation_developer_id=known["developer_id"], observed_sha256=before, resulting_sha256=before,
                          registry_path=str(registry), already_applied=True, writes_performed=False, verified=True)
        if proposal["observed_sha256"] != before:
            raise ContractError("Attribution registry changed after preview; reread and prepare a fresh proposal")
        target = proposal["developer_id"]
        action = proposal["action"]
        updated = copy.deepcopy(data)
        if action == "create" and target in data["developers"]:
            raise ContractError("Create cannot replace a registered developer")
        if action == "select":
            if target not in data["developers"] or proposal["labels"] or proposal["aliases"]:
                raise ContractError("Selection requires an existing ID and cannot change metadata")
        else:
            entry = updated["developers"].setdefault(target, {"labels": [], "aliases": []})
            for key in ("labels", "aliases"):
                for item in proposal[key]:
                    if item not in entry[key]:
                        entry[key].append(item)
        updated["selected"] = target
        updated["operations"][proposal["operation_id"]] = {"proposal_sha256": request_sha, "developer_id": target,
                                                          "action": action, "at": datetime.now(timezone.utc).isoformat()}
        payload = encoded(validate(updated))
        with tempfile.NamedTemporaryFile(prefix=".developers-", dir=root, delete=False) as stream:
            temporary = Path(stream.name)
            os.chmod(temporary, 0o600)
            stream.write(payload)
            stream.flush()
            os.fsync(stream.fileno())
        if observe()[1] != before:
            raise ContractError("External registry edit observed immediately before replacement")
        os.replace(temporary, registry)
        temporary = None
        directory = os.open(root, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
        actual = registry.read_bytes()
        if actual != payload:
            raise ContractError("Attribution write needs inspection: readback differs; replay the same proposal")
        return result("apply", operation_id=proposal["operation_id"], developer_id=target,
                      observed_sha256=before, resulting_sha256=digest(actual), registry_path=str(registry),
                      already_applied=False, writes_performed=True, verified=True)
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)
        # Do not remove a lock somebody replaced manually while this process ran.
        try:
            if (not lock.is_symlink() and lock.exists() and lock.lstat().st_ino == owned_inode
                    and lock.read_bytes() == encoded(lock_data)):
                lock.unlink()
        finally:
            os.close(descriptor)


def recover(prepared=None):
    _, registry, lock = paths()
    if not lock.exists():
        raise ContractError("No developer registry lock to recover")
    if fcntl is None:
        raise ContractError("Safe stale-lock recovery is unavailable on this filesystem/platform; operator review is required")
    descriptor = os.open(lock, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    try:
        try:
            fcntl.flock(descriptor, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            raise ContractError("Lock is held by a writer or another recovery; stop") from exc
        inode = os.fstat(descriptor).st_ino
        if lock.lstat().st_ino != inode:
            raise ContractError("Lock changed before recovery inventory")
        with os.fdopen(descriptor, "rb", closefd=False) as stream:
            raw = stream.read()
        return recover_locked(prepared, registry, lock, inode, raw)
    finally:
        os.close(descriptor)


def recover_locked(prepared, registry, lock, inode, raw):
    data = load(raw)
    expected = {"operation_id", "host", "pid", "operation", "start"}
    if (not isinstance(data, dict) or set(data) != expected or not uuid_valid(data.get("operation_id"))
            or data.get("operation") not in {"developer-" + action for action in SPEC["actions"]}):
        raise ContractError("Unrecognized developer lock requires operator review; preserve it")
    time_value(data["start"])
    if data.get("host") != socket.gethostname() or type(data.get("pid")) is not int or data["pid"] <= 0:
        raise ContractError("Foreign-host or malformed lock needs explicit operator review; no automatic removal")
    try:
        os.kill(data["pid"], 0)
    except ProcessLookupError:
        pass
    except PermissionError as exc:
        raise ContractError("Cannot prove lock owner is gone") from exc
    else:
        raise ContractError("Lock owner is still alive; do not remove its lock")
    if prepared is None:
        return result("recover-preview", proposal={"schema": "agent-memory/developer-lock-recovery-v1",
                      "registry_path": str(registry), "observed_lock_sha256": digest(raw), "operation_id": str(uuid.uuid4())},
                      lock_owner=data, writes_performed=False, verified=False)
    if isinstance(prepared, dict) and "proposal" in prepared:
        prepared = prepared["proposal"]
    if (not isinstance(prepared, dict) or set(prepared) != {"schema", "registry_path", "observed_lock_sha256", "operation_id"}
            or prepared["schema"] != "agent-memory/developer-lock-recovery-v1" or not uuid_valid(prepared["operation_id"])
            or prepared["registry_path"] != str(registry) or prepared["observed_lock_sha256"] != digest(raw)):
        raise ContractError("Lock changed or recovery proposal is invalid; inspect it again")
    if lock.lstat().st_ino != inode or lock.read_bytes() != raw:
        raise ContractError("Lock changed immediately before recovery")
    lock.unlink()
    old_lock_gone = not lock.exists() or lock.lstat().st_ino != inode
    return result("recover", operation_id=prepared["operation_id"], writes_performed=True,
                  replacement_lock_present=lock.exists(), verified=old_lock_gone)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    status_parser = commands.add_parser("status")
    status_parser.add_argument("--observe", action="append", default=[])
    status_parser.add_argument("--harness")
    status_parser.add_argument("--session")
    planner = commands.add_parser("preview")
    action = planner.add_mutually_exclusive_group(required=True)
    action.add_argument("--create", action="store_true")
    action.add_argument("--adopt")
    action.add_argument("--select")
    planner.add_argument("--label", action="append", default=[])
    planner.add_argument("--alias", action="append", default=[])
    writer = commands.add_parser("apply")
    writer.add_argument("--proposal", required=True)
    recovery = commands.add_parser("recover")
    recovery.add_argument("--proposal")
    args = parser.parse_args(argv)
    try:
        if args.command == "status":
            receipt = status(declared(args.observe), args.harness, args.session)
        elif args.command == "preview":
            action_name = "create" if args.create else "adopt" if args.adopt else "select"
            receipt = preview(action_name, args.adopt or args.select, args.label, declared(args.alias))
        elif args.command == "apply":
            receipt = apply(load(Path(args.proposal).read_bytes()))
        else:
            receipt = recover(load(Path(args.proposal).read_bytes()) if args.proposal else None)
        print(json.dumps(receipt, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, ValueError, TypeError, KeyError, AttributeError) as exc:
        print(json.dumps(result(args.command, error=str(exc), verified=False)))
        return 1


if __name__ == "__main__":
    sys.exit(main())
