"""Read-only rendering and validation of the first-release router/file contract."""
from __future__ import annotations

import argparse
import json
from pathlib import Path, PurePosixPath
import re
import sys
import uuid

from .frontmatter import ContractError, read, unique_object, write
from .lifecycle import MODEL, time_value
from .remote_identity import normalize

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
    if not isinstance(value, str) or not value or any(c in value for c in "\\\n\r|[]()<>#%"):
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
    if not isinstance(value, str) or not value.strip() or any(c in value for c in "\n\r|\\"):
        raise ContractError("Route cells require nonempty single-line text without pipes or backslashes")
    return value.strip()


def route_value(route):
    if not isinstance(route, dict) or set(route) != set(FIELDS):
        raise ContractError("Routes require path, purpose, scope, lifecycle and read-when")
    route = {key: text_cell(route[key]) for key in FIELDS}
    path_value(route["path"])
    scope = route["scope"]
    if scope != "repository" and not re.fullmatch(r"(?:component|branch|worktree|task|session):\S(?:.*\S)?", scope):
        raise ContractError("Scope must be repository or an explicit component/branch/worktree/task/session condition")
    if scope.startswith("component:"):
        component = PurePosixPath(scope.removeprefix("component:"))
        if component.is_absolute() or any(part in {".", ".."} for part in component.parts):
            raise ContractError("Component scope must stay within the repository")
    if route["lifecycle"] not in MODEL["states"]:
        raise ContractError("Unknown route lifecycle")
    return route


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
    body += "| Path | Purpose | Scope | Lifecycle | Read when |\n| --- | --- | --- | --- | --- |\n"
    for route in routes:
        cells = [f"[{route['path']}](<{route['path']}>)", *[route[key] for key in FIELDS[1:]]]
        body += "| " + " | ".join(cells) + " |\n"
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
    rows = [line.strip() for line in routes_body.splitlines() if line.strip()]
    if rows[:2] != ["| Path | Purpose | Scope | Lifecycle | Read when |", "| --- | --- | --- | --- | --- |"]:
        raise ContractError("Invalid route table header")
    routes = []
    for row in rows[2:]:
        if not row.startswith("|") or not row.endswith("|"):
            raise ContractError("Invalid route table row")
        cells = [cell.strip() for cell in row[1:-1].split("|")]
        if len(cells) != len(FIELDS):
            raise ContractError("Route row must have five columns")
        match = re.fullmatch(r"\[([^\]]+)\]\((?:<([^<>]+)>|([^<>\s]+))\)", cells[0])
        destination = (match[2] or match[3]) if match else None
        if not match or match[1] != destination:
            raise ContractError("Route label and relative target must agree")
        cells[0] = destination
        routes.append(route_value(dict(zip(FIELDS, cells))))
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
    metadata, routes, omitted = parse_router(files["MEMORY.md"])
    for route in routes:
        if route["path"] not in files:
            raise ContractError(f"Missing or empty routed file: {route['path']}")
    for path in omitted:
        if path in files:
            raise ContractError(f"Context exists but is marked omitted: {path}")
    if set(files) != {"MEMORY.md", *[r["path"] for r in routes]}:
        raise ContractError("Project Markdown must be reachable from the router")
    return {"schema": "agent-memory/bank-contract-result-v1", "operation": "validate",
            "valid": True, "validation_scope": "router-and-file-contract", "project_id": metadata["project-id"],
            "router_updated": metadata["router-updated"], "routes": routes, "omitted": omitted,
            "writes_performed": False}


def validate(repo, *, owned_lock=None):
    bank = Path(repo).resolve() / SPEC["x-bank-path"]
    if bank.is_symlink():
        raise ContractError("Bank directory must not be a symlink")
    if (bank / ".agent-memory-transaction.md").exists():
        raise ContractError("Interrupted bank: recover the transaction before resuming")
    lock = bank / ".agent-memory.lock"
    if lock.is_symlink() or (lock.exists() and (owned_lock is None or lock.read_bytes() != owned_lock)):
        raise ContractError("Busy bank: retry validation after the writer finishes")
    router = bank / "MEMORY.md"
    if router.is_symlink():
        raise ContractError("Router must not be a symlink")
    metadata, routes, omitted = parse_router(router.read_text(encoding="utf-8"))
    for route in routes:
        target = bank / route["path"]
        relative = target.relative_to(bank)
        parts = [bank.joinpath(*relative.parts[:i]) for i in range(1, len(relative.parts) + 1)]
        if any(part.is_symlink() for part in parts) or not target.resolve().is_relative_to(bank.resolve()):
            raise ContractError("Routed target must remain inside the bank without symlinks")
        if not target.is_file() or not target.read_text(encoding="utf-8").strip():
            raise ContractError(f"Missing or empty routed file: {route['path']}")
    for path in omitted:
        if (bank / path).exists() or (bank / path).is_symlink():
            raise ContractError(f"Context exists but is marked omitted: {path}")
    # Optional files cannot become an unindexed second bank. Bounded topic-index
    # traversal is added by the disclosure/optional-record stories; this first
    # contract accepts directly routed optional Markdown files.
    files = {"MEMORY.md": router.read_text(encoding="utf-8")}
    for target in bank.rglob("*"):
        if target.is_symlink():
            raise ContractError("Bank paths must not be symlinks")
        relative = target.relative_to(bank).as_posix()
        if target.is_file() and target.suffix == ".md" and not any(part.startswith(".agent-memory") for part in Path(relative).parts):
            files[relative] = target.read_text(encoding="utf-8")
    return validate_files(files)


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
