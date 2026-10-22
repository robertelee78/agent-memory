"""Content-free tombstones and the shared projection anti-resurrection guard."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from . import bank_contract as contract
from . import bank_writes as writes
from .developer_identity import digest, load, uuid_valid
from .frontmatter import ContractError, read, write
from .lifecycle import time_value

SPEC = json.loads((Path(__file__).resolve().parents[2] / "schemas/tombstone.json").read_bytes())
FIELDS = SPEC["required"]
PREFIX = "history/tombstones/"


def value(metadata):
    if not isinstance(metadata, dict) or set(metadata) != set(FIELDS):
        raise ContractError("Tombstones retain exactly the five safe deletion fields")
    if any(not uuid_valid(metadata[key]) for key in ("id", "scope", "deletion-authority")):
        raise ContractError("Tombstone record ID, project scope and deletion authority must be canonical random UUIDv4 values")
    time_value(metadata["deleted-at"])
    if metadata["reason-class"] not in SPEC["properties"]["reason-class"]["enum"]:
        raise ContractError("Tombstone reason must be a known non-sensitive class")
    return {key: metadata[key] for key in FIELDS}


def relative_path(identity):
    return PREFIX + identity + ".md"


def parse(text):
    metadata, body = read(text, allowed=FIELDS)
    if body.strip():
        raise ContractError("Tombstone bodies must be empty; never retain deleted content or evidence")
    return value(metadata)


def render(metadata):
    metadata = value(metadata)
    return {"schema": "agent-memory/tombstone-result-v1", "operation": "render", "metadata": metadata,
            "path": relative_path(metadata["id"]), "content": write(metadata), "writes_performed": False,
            "identity_authenticated": False}


def canonical_paths(bank):
    # Markdown inventory deliberately ignores other file types and runtime
    # names. None of those omissions may mean "no deletion" in this namespace.
    contract.portable_aliases([p.relative_to(bank).as_posix() for p in bank.rglob("*")]
                              + ["history", "history/tombstones"])
    for directory in (bank / "history", bank / "history/tombstones"):
        if directory.is_symlink() or (directory.exists() and not directory.is_dir()):
            raise ContractError("Deletion history requires canonical directories")
    directory = bank / "history/tombstones"
    physical = set()
    if directory.exists():
        for entry in directory.iterdir():
            if (entry.is_symlink() or not entry.is_file() or entry.suffix != ".md"
                    or not uuid_valid(entry.stem)):
                raise ContractError("Deletion history permits only canonical UUID marker files")
            physical.add(entry.relative_to(bank).as_posix())
    return physical


def history(repo):
    checked, observed = writes.snapshot(repo)
    _, bank = writes.bank_path(repo)
    tombstones = {}
    routes = {route["path"]: route for route in checked["routes"]}
    if canonical_paths(bank) != {path for path in observed if path.startswith(PREFIX)}:
        raise ContractError("Deletion history changed during inventory")
    # Use every canonical history file, including targets that a later bounded
    # index resolver makes reachable. Never restrict deletion to the read set
    # chosen for the current coding task.
    for path in observed:
        if not path.startswith(PREFIX):
            continue
        raw = (bank / path).read_bytes()
        if digest(raw) != observed[path]:
            raise ContractError("Deletion history changed during inventory")
        metadata = parse(raw.decode("utf-8"))
        route = routes.get(path)
        if (path != relative_path(metadata["id"]) or metadata["scope"] != checked["project_id"] or metadata["id"] in tombstones
                or (route is not None and (route["lifecycle"] != "deleted" or route["scope"] != "repository"))):
            raise ContractError("Canonical tombstone route must match its record ID, deleted state and scope")
        tombstones[metadata["id"]] = metadata
    return checked["project_id"], observed, tombstones


def filter_projection(repo, projection):
    project_id, observed, deleted = history(repo)
    if (not isinstance(projection, dict) or set(projection) != {"project_id", "records"}
            or projection["project_id"] != project_id or not isinstance(projection["records"], list)):
        raise ContractError("Projection must name this canonical project and its source records")
    seen, kept, suppressed = set(), [], 0
    for record in projection["records"]:
        if not isinstance(record, dict) or not uuid_valid(record.get("id")) or record["id"] in seen:
            raise ContractError("Projection records require unique stable random IDs; do not infer identity from content")
        seen.add(record["id"])
        if record["id"] in deleted:
            suppressed += 1
        else:
            kept.append(record)
    latest_project, latest_hashes, latest_deleted = history(repo)
    if latest_project != project_id or latest_hashes != observed or latest_deleted != deleted:
        raise ContractError("Canonical history changed during projection filtering; reread before rebuild")
    return {"schema": "agent-memory/projection-guard-result-v1", "operation": "filter", "project_id": project_id,
            "records": kept, "suppressed": suppressed, "source_hashes": observed, "writes_performed": False,
            "validation_scope": "terminal-deletion-guard", "identity_authenticated": False}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    renderer = commands.add_parser("render")
    renderer.add_argument("--input", required=True)
    checker = commands.add_parser("validate")
    checker.add_argument("--file", required=True)
    guard = commands.add_parser("filter")
    guard.add_argument("--repo", default=".")
    guard.add_argument("--input", required=True)
    args = parser.parse_args(argv)
    try:
        if args.command == "render":
            output = render(load(Path(args.input).read_bytes()))
        elif args.command == "validate":
            metadata = parse(Path(args.file).read_bytes().decode("utf-8"))
            output = {"schema": "agent-memory/tombstone-result-v1", "operation": "validate", "metadata": metadata,
                      "path": relative_path(metadata["id"]), "valid": True, "writes_performed": False,
                      "validation_scope": "tombstone-format", "identity_authenticated": False}
        else:
            output = filter_projection(args.repo, load(Path(args.input).read_bytes()))
        print(json.dumps(output, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError):
        # Rejected memory and metadata may themselves contain the deleted secret.
        # Keep error output content-free instead of echoing an offending value.
        print(json.dumps({"schema": "agent-memory/tombstone-result-v1", "operation": args.command,
                          "valid": False, "writes_performed": False, "error": "Invalid tombstone, source identity, projection or canonical bank; preserve input for explicit inspection"}))
        return 1


if __name__ == "__main__":
    sys.exit(main())
