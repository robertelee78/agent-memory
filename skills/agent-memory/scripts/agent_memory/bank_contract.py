"""Read-only rendering and validation of the first-release router/file contract."""
from __future__ import annotations

import argparse
import json
import posixpath
from pathlib import Path, PurePosixPath
import re
import sys
import unicodedata
import uuid

from . import repository_config
from .frontmatter import ContractError, read, unique_object, write
from .lifecycle import MODEL, time_value
from .remote_identity import normalize
from .scopes import parse_scope

PACK = Path(__file__).resolve().parents[2]
SPEC = json.loads((PACK / "schemas/bank-router.json").read_text())
CONTEXTS = SPEC["x-contexts"]
FIELDS = SPEC["x-route-fields"]


def timestamp(value):
    try:
        time_value(value)
    except ContractError as exc:
        raise ContractError("Invalid router-updated time") from exc


def header(metadata):
    if not set(SPEC["required"]).issubset(metadata) or not set(metadata).issubset(SPEC["properties"]):
        raise ContractError("Router requires schema, project-id and router-updated; only known fields are allowed")
    if "canonical-remote" in metadata:
        remote = metadata["canonical-remote"]
        if not isinstance(remote, str) or normalize(remote) != remote:
            raise ContractError("canonical-remote must be a sanitized canonical repository identifier")
    if metadata["schema"] != SPEC["properties"]["schema"]["const"]:
        raise ContractError("Unsupported bank schema; no mutation or implicit migration is allowed")
    try:
        identity = uuid.UUID(metadata["project-id"])
        if str(identity) != metadata["project-id"] or identity.version != 4:
            raise ValueError("not a canonical random UUID")
    except (ValueError, TypeError, AttributeError) as exc:
        raise ContractError("project-id must be a canonical random UUIDv4") from exc
    timestamp(metadata["router-updated"])


def path_value(value):
    if (not isinstance(value, str) or not value or any(c in value for c in "\\|[]()<>#%")
            or any(ord(c) < 32 or ord(c) == 127 or c in "\u0085\u2028\u2029" for c in value)):
        raise ContractError("Route path must be an unambiguous relative Markdown path")
    path = PurePosixPath(value)
    if path.is_absolute() or str(path) != value or any(p in {".", ".."} or p.startswith(".") for p in path.parts):
        raise ContractError("Route escapes the bank or names runtime/hidden state")
    if path.suffix != ".md" or value == "MEMORY.md":
        raise ContractError("Routes must name Markdown targets other than the router")
    if value not in CONTEXTS and (len(path.parts) < 2 or path.parts[0] not in SPEC["x-optional-directories"]):
        raise ContractError("Routes must name context files or optional project-record directories")
    return value


def text_cell(value):
    if (not isinstance(value, str) or not value.strip() or any(c in value for c in "|\\")
            or any(ord(c) < 32 or ord(c) == 127 or c in "\u0085\u2028\u2029" for c in value)):
        raise ContractError("Route cells require nonempty single-line text without pipes or backslashes")
    return value.strip()


def route_value(route):
    if not isinstance(route, dict) or set(route) != set(FIELDS):
        raise ContractError("Routes require path, purpose, scope, lifecycle and read-when")
    route = {key: text_cell(route[key]) for key in FIELDS}
    path_value(route["path"])
    parse_scope(route["scope"])
    if route["lifecycle"] not in MODEL["states"]:
        raise ContractError("Unknown route lifecycle")
    return route


def route_table(routes, source="MEMORY.md"):
    if len(routes) > SPEC["x-max-routes"]:
        raise ContractError("Route limit exceeded; use a bounded topic index")
    routes = [route_value(route) for route in routes]
    text = "| Path | Purpose | Scope | Lifecycle | Read when |\n| --- | --- | --- | --- | --- |\n"
    for route in routes:
        destination = posixpath.relpath(route['path'], str(PurePosixPath(source).parent))
        cells = [f"[{route['path']}](<{destination}>)", *[route[key] for key in FIELDS[1:]]]
        text += "| " + " | ".join(cells) + " |\n"
    return text


def parse_table(text, source="MEMORY.md"):
    rows = [line.strip() for line in text.splitlines() if line.strip()]
    if rows[:2] != ["| Path | Purpose | Scope | Lifecycle | Read when |", "| --- | --- | --- | --- | --- |"]:
        raise ContractError("Invalid route table header")
    if len(rows) - 2 > SPEC["x-max-routes"]:
        raise ContractError("Route limit exceeded; use a bounded topic index")
    routes = []
    for row in rows[2:]:
        if not row.startswith("|") or not row.endswith("|"):
            raise ContractError("Invalid route table row")
        cells = [cell.strip() for cell in row[1:-1].split("|")]
        if len(cells) != len(FIELDS):
            raise ContractError("Route row must have five columns")
        match = re.fullmatch(r"\[([^\]]+)\]\((?:<([^<>]+)>|([^<>\s]+))\)", cells[0])
        destination = (match[2] or match[3]) if match else None
        if not match:
            raise ContractError("Route requires an explicit relative Markdown link")
        path_value(match[1])
        expected = posixpath.relpath(match[1], str(PurePosixPath(source).parent))
        if destination != expected:
            raise ContractError("Route label and relative target must agree")
        cells[0] = match[1]
        routes.append(route_value(dict(zip(FIELDS, cells))))
    if len({route['path'] for route in routes}) != len(routes):
        raise ContractError("Duplicate route path")
    return routes


def router_text(metadata, routes, omitted):
    header(metadata)
    if len(routes) > SPEC["x-max-routes"]:
        raise ContractError("Router route limit exceeded; use a bounded topic index")
    routes = [route_value(r) for r in routes]
    paths = [r["path"] for r in routes]
    if len(paths) != len(set(paths)):
        raise ContractError("Duplicate route path")
    expected = set(CONTEXTS) - set(paths)
    if set(omitted) != expected:
        raise ContractError("Every absent context needs exactly one omission reason")
    body = "# Project memory\n\nRouter time describes this map, not every file's revision.\n\n## Routes\n\n"
    body += route_table(routes)
    body += "\n## Omitted contexts\n\n"
    if omitted:
        body += "".join(f"- `{path}`: {text_cell(omitted[path])}\n" for path in CONTEXTS if path in omitted)
    else:
        body += "None.\n"
    text = write(metadata) + body
    if len(text.encode()) > SPEC["x-max-router-bytes"]:
        raise ContractError("Router byte limit exceeded; move detail into project context")
    return text


def parse_router(text):
    if len(text.encode()) > SPEC["x-max-router-bytes"]:
        raise ContractError("Router byte limit exceeded")
    metadata, body = read(text, allowed=SPEC["properties"])
    header(metadata)
    if body.count("\n## Routes\n") != 1 or body.count("\n## Omitted contexts\n") != 1:
        raise ContractError("Router needs exactly one Routes and Omitted contexts section")
    _, rest = body.split("\n## Routes\n")
    routes_body, omissions_body = rest.split("\n## Omitted contexts\n")
    routes = parse_table(routes_body)
    omitted = {}
    if omissions_body.strip() != "None.":
        for line in omissions_body.strip().splitlines():
            match = re.fullmatch(r"- `([^`]+)`: (.+)", line)
            if not match or match[1] in omitted:
                raise ContractError("Invalid or duplicated context omission")
            omitted[match[1]] = text_cell(match[2])
    # Use the same bounds/completeness checks as the canonical renderer.
    router_text(metadata, routes, omitted)
    return metadata, routes, omitted


def context_route(path):
    """Navigation defaults for a newly created ordinary context, not authority."""
    profiles = {
        'projectbrief.md': ('Project purpose and authority boundaries',
                            'At session start to identify the project'),
        'productContext.md': ('Users, workflow and scope exclusions',
                              'When deciding product scope or comparing requirements'),
        'activeContext.md': ('Scoped handoff, unresolved questions and next step',
                             'When resuming the matching task or investigating its questions'),
        'systemPatterns.md': ('Architecture and established project patterns',
                              'When changing design, integration or project conventions'),
        'techContext.md': ('Qualified implementation and technical source evidence',
                           'When working on the relevant implementation or tooling'),
        'progress.md': ('Delivery evidence and work-source references',
                        'When checking delivery claims or release scope'),
    }
    if path not in profiles:
        raise ContractError('Navigation defaults require a known ordinary context')
    purpose, trigger = profiles[path]
    return {'path': path, 'purpose': purpose, 'scope': 'repository',
            'lifecycle': 'active', 'read-when': trigger}


def add_context_routes(text, paths, updated):
    """Add missing contexts without re-rendering existing map prose or rows."""
    _, routes, omitted = parse_router(text)
    timestamp(updated)
    paths = list(paths)
    if len(set(paths)) != len(paths) or any(path not in omitted for path in paths):
        raise ContractError('Only missing ordinary context routes may be added')
    if not paths:
        return text
    new_routes = [context_route(path) for path in paths]
    end = text.index('\n---\n', 4)
    prefix = re.sub(r'(?m)^router-updated:.*$',
                    lambda _: 'router-updated: ' + json.dumps(updated), text[:end])
    intro, rest = text[end:].split('\n## Routes\n')
    table, omissions = rest.split('\n## Omitted contexts\n')
    lines = table.splitlines(keepends=True)
    insertion = len(lines)
    while insertion and not lines[insertion - 1].strip():
        insertion -= 1
    table = (''.join(lines[:insertion]) +
             ''.join(route_table(new_routes).splitlines(keepends=True)[2:]) +
             ''.join(lines[insertion:]))
    remaining = [line for line in omissions.splitlines(keepends=True)
                 if not any(line.strip().startswith('- `' + path + '`: ') for path in paths)]
    omissions = ''.join(remaining) if set(omitted) - set(paths) else '\n\nNone.\n'
    result = prefix + intro + '\n## Routes\n' + table + '\n## Omitted contexts\n' + omissions
    # Recheck route count, byte bounds, identity and complete omission accounting.
    _, final_routes, _ = parse_router(result)
    if final_routes != routes + new_routes:
        raise ContractError('Existing route metadata must be preserved')
    return result


def render(proposal):
    required = {"project-id", "router-updated", "contexts", "omitted"}
    if not isinstance(proposal, dict) or not required.issubset(proposal) or not set(proposal).issubset(required | {"canonical-remote"}):
        raise ContractError("Render input needs project-id, router-updated, contexts and omitted")
    contexts = proposal["contexts"]
    if not isinstance(contexts, dict) or not contexts or not set(contexts).issubset(CONTEXTS):
        raise ContractError("Provide at least one useful recognizable context file")
    if not isinstance(proposal["omitted"], dict):
        raise ContractError("omitted must map absent contexts to explicit reasons")
    metadata = {"schema": "agent-memory/v1", "project-id": proposal["project-id"], "router-updated": proposal["router-updated"]}
    if "canonical-remote" in proposal:
        metadata["canonical-remote"] = proposal["canonical-remote"]
    files, routes = {}, []
    for path in CONTEXTS:
        if path not in contexts:
            continue
        context = contexts[path]
        if not isinstance(context, dict) or set(context) != {"body", *FIELDS[1:]}:
            raise ContractError("Each context needs body, purpose, scope, lifecycle and read-when")
        body = context["body"]
        if not isinstance(body, str) or not body.strip():
            raise ContractError("Empty context placeholders are forbidden")
        files[path] = body.rstrip() + "\n"
        routes.append({"path": path, **{k: context[k] for k in FIELDS[1:]}})
    files = {"MEMORY.md": router_text(metadata, routes, proposal["omitted"]), **files}
    return {"schema": "agent-memory/bank-contract-result-v1", "operation": "render",
            "bank_path": SPEC["x-bank-path"], "project_id": metadata["project-id"],
            "files": files, "writes_performed": False}


def portable_key(value):
    return unicodedata.normalize("NFC", unicodedata.normalize("NFC", value).casefold())


def portable_aliases(paths):
    seen = {}
    for path in paths:
        parts = PurePosixPath(path).parts
        for size in range(1, len(parts) + 1):
            spelling = PurePosixPath(*parts[:size]).as_posix()
            key = portable_key(spelling)
            if key in seen and seen[key] != spelling:
                raise ContractError("Bank paths have case or Unicode component aliases; preserve the physical entries")
            seen[key] = spelling


def runtime_path(path):
    return any(part.startswith(".agent-memory") for part in PurePosixPath(path).parts)


def bank_root(bank):
    # Do not adopt a differently spelled directory just because this host treats
    # it as the same path. Such a bank would fail on a case-sensitive clone.
    if bank.parent.is_dir():
        for entry in bank.parent.iterdir():
            if portable_key(entry.name) == portable_key(bank.name) and entry.name != bank.name:
                raise ContractError("Bank root has a case or Unicode spelling alias; explicit repair is required")


def physical_paths(bank):
    paths = []
    for target in bank.rglob("*"):
        if target.is_symlink():
            raise ContractError("Bank paths must not be symlinks")
        path = target.relative_to(bank).as_posix()
        if not runtime_path(path):
            paths.append(path)
    return paths


def file_topology(paths):
    paths = set(paths)
    portable_aliases(paths)
    for path in paths:
        if any(parent.as_posix() in paths for parent in PurePosixPath(path).parents):
            raise ContractError("A canonical file cannot also be a parent directory")


def validate_files(files):
    """Validate a proposed complete canonical map without temporary file writes."""
    if not isinstance(files, dict) or "MEMORY.md" not in files:
        raise ContractError("A complete bank requires MEMORY.md")
    for path, body in files.items():
        if path != "MEMORY.md":
            path_value(path)
        if not isinstance(body, str) or not body.strip():
            raise ContractError(f"Missing or empty routed file: {path}")
        body.encode("utf-8")
    file_topology(files)
    metadata, routes, omitted = parse_router(files["MEMORY.md"])
    for route in routes:
        if route["path"] not in files:
            raise ContractError(f"Missing or empty routed file: {route['path']}")
    for path in omitted:
        if path in files:
            raise ContractError(f"Context exists but is marked omitted: {path}")
    from .bank_layout import inventory
    entries = inventory(files, metadata, routes)
    return {"schema": "agent-memory/bank-contract-result-v1", "operation": "validate",
            "valid": True, "validation_scope": "router-layout-and-record-format", "project_id": metadata["project-id"],
            "router_updated": metadata["router-updated"], "routes": routes, "omitted": omitted, "entries": entries,
            "writes_performed": False}


def validate(repo, *, owned_lock=None, owned_transaction=None):
    bank = repository_config.bank_path(repo)
    bank_root(bank)
    if bank.is_symlink():
        raise ContractError("Bank directory must not be a symlink")
    marker = bank / ".agent-memory-transaction.md"
    if marker.is_symlink() or (marker.exists() and (owned_transaction is None or marker.read_bytes() != owned_transaction)):
        raise ContractError("Interrupted bank: recover the transaction before resuming")
    lock = bank / ".agent-memory.lock"
    if lock.is_symlink() or (lock.exists() and (owned_lock is None or lock.read_bytes() != owned_lock)):
        raise ContractError("Busy bank: retry validation after the writer finishes")
    router = bank / "MEMORY.md"
    if router.is_symlink():
        raise ContractError("Router must not be a symlink")
    metadata, routes, omitted = parse_router(router.read_bytes().decode("utf-8"))
    portable_aliases([*physical_paths(bank), "MEMORY.md", *[route["path"] for route in routes]])
    for route in routes:
        target = bank / route["path"]
        relative = target.relative_to(bank)
        parts = [bank.joinpath(*relative.parts[:i]) for i in range(1, len(relative.parts) + 1)]
        if any(part.is_symlink() for part in parts) or not target.resolve().is_relative_to(bank.resolve()):
            raise ContractError("Routed target must remain inside the bank without symlinks")
        if not target.is_file() or not target.read_bytes().decode("utf-8").strip():
            raise ContractError(f"Missing or empty routed file: {route['path']}")
    for path in omitted:
        if (bank / path).exists() or (bank / path).is_symlink():
            raise ContractError(f"Context exists but is marked omitted: {path}")
    from .tombstones import canonical_paths
    deletion_paths = canonical_paths(bank)
    # Inventory every physical Markdown source before the shared pure layout
    # validator resolves bounded topic indices and standalone record metadata.
    files = {"MEMORY.md": router.read_bytes().decode("utf-8")}
    for target in bank.rglob("*"):
        if target.is_symlink():
            raise ContractError("Bank paths must not be symlinks")
        relative = target.relative_to(bank).as_posix()
        if target.is_file() and target.suffix == ".md" and not runtime_path(relative):
            files[relative] = target.read_bytes().decode("utf-8")
    result = validate_files(files)
    if canonical_paths(bank) != deletion_paths:
        raise ContractError("Deletion history changed during validation")
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    renderer = commands.add_parser("render", help="emit a file proposal; write nothing")
    renderer.add_argument("--input", required=True, help="JSON file, or - for stdin")
    checker = commands.add_parser("validate", help="inspect the repository-root memory-bank")
    checker.add_argument("--repo", default=".")
    args = parser.parse_args(argv)
    try:
        if args.command == "render":
            text = sys.stdin.read() if args.input == "-" else Path(args.input).read_text()
            result = render(json.loads(text, object_pairs_hook=unique_object))
        else:
            result = validate(args.repo)
        print(json.dumps(result, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError) as exc:
        print(json.dumps({"schema": "agent-memory/bank-contract-result-v1", "operation": args.command,
                          "valid": False, "error": str(exc), "writes_performed": False}))
        return 1
