"""One portable project identity algorithm for repositories, worktrees and clones."""
from __future__ import annotations

import argparse
import copy
import json
from pathlib import Path
import re
import os
import subprocess
import sys
import uuid

from . import repository_config
from .bank_contract import parse_router
from .developer_identity import digest, encoded, load, text, uuid_valid
from .frontmatter import ContractError
from . import local_registry as storage
from .remote_identity import normalize
from .identity_git import git

PACK = Path(__file__).resolve().parents[2]
SPEC = json.loads((PACK / "schemas/projects.json").read_text())


def discover(repo, authority=None, *, owned_lock=None, owned_transaction=None):
    repo = Path(repo).resolve()
    if not repo.is_dir():
        raise ContractError("Repository location must be an existing directory")
    top = git(repo, "rev-parse", "--show-toplevel", optional=True)
    is_git = top is not None
    root = Path(top).resolve() if is_git else repo
    common = (root / git(root, "rev-parse", "--git-common-dir")).resolve() if is_git else root
    stat = common.stat()
    issues, remotes = [], {}
    configured = git(root, "config", "--get-all", "agent-memory.authorityRemote", optional=True) if is_git else None
    configured_values = sorted(set(configured.splitlines())) if configured else []
    if configured:
        values = set(configured_values)
        if len(values) != 1 and authority is None:
            issues.append("Conflicting configured authority remotes; choose one explicitly")
        configured = configured.splitlines()[-1]
    if is_git:
        for name in git(root, "remote").splitlines():
            candidates = []
            try:
                for flags in (("--all",), ("--push", "--all")):
                    values = git(root, "remote", "get-url", *flags, name)
                    for value in values.splitlines():
                        identity = normalize(value, root)
                        if identity not in candidates:
                            candidates.append(identity)
                remotes[name] = {"identities": candidates, "consistent": len(candidates) == 1}
            except ContractError:
                remotes[name] = {"identities": [], "consistent": False}
    selected = authority or configured
    if selected is not None:
        text(selected, 128)
        if selected.startswith("-") or selected not in remotes:
            issues.append("Configured authority remote is missing; choose an existing remote")
    elif remotes:
        plausible = {identity for value in remotes.values() for identity in value["identities"]}
        if "origin" in remotes and len(plausible) == 1 and all(v["consistent"] for v in remotes.values()):
            selected = "origin"
        else:
            issues.append("Multiple plausible remotes or no unambiguous origin; choose an authority remote")
    canonical = None
    if selected in remotes:
        entry = remotes[selected]
        if entry["consistent"]:
            canonical = entry["identities"][0]
        else:
            issues.append("Authority fetch/push URLs do not identify one repository; repair or explicitly bind a consistent remote")
    bank = repository_config.bank_path(root)
    router = bank / "MEMORY.md"
    if bank.is_symlink() or router.is_symlink():
        raise ContractError("Bank/router must not be symlinks")
    for name, owned in ((".agent-memory.lock", owned_lock), (".agent-memory-transaction.md", owned_transaction)):
        path = bank / name
        if path.is_symlink() or (path.exists() and (owned is None or path.read_bytes() != owned)):
            raise ContractError("Bank busy or interrupted; inspect/recover before resolving identity")
    raw = router.read_bytes() if router.exists() else None
    metadata = parse_router(raw.decode("utf-8"))[0] if raw is not None else {}
    return {"repo_root": str(root), "is_git": is_git, "common_git_dir": str(common) if is_git else None,
            "checkout_key": f"{stat.st_dev}:{stat.st_ino}", "location": str(common), "authority_name": selected,
            "canonical_remote": canonical, "configured_authority": configured_values, "remotes": remotes, "bank_project_id": metadata.get("project-id"),
            "bank_canonical_remote": metadata.get("canonical-remote"), "repository_settings": repository_config.settings(root), "router_sha256": digest(raw), "issues": issues}


def canonical(value):
    if value is not None and (not isinstance(value, str) or normalize(value) != value):
        raise ContractError("Registry remote must be a sanitized canonical repository identifier")


def validate(data):
    if not isinstance(data, dict) or set(data) != set(SPEC["registry-fields"]) or data["schema"] != SPEC["schema"]:
        raise ContractError("Unknown project registry schema/fields; preserve for explicit repair")
    if not all(isinstance(data[k], dict) for k in ("projects", "checkouts", "operations")):
        raise ContractError("Invalid project registry maps")
    remotes = []
    for project, entry in data["projects"].items():
        if not uuid_valid(project) or not isinstance(entry, dict) or set(entry) != set(SPEC["project-fields"]):
            raise ContractError("Invalid project registry entry")
        canonical(entry["canonical_remote"])
        if entry["canonical_remote"] is not None:
            remotes.append(entry["canonical_remote"])
        if not isinstance(entry["aliases"], list) or not isinstance(entry["history"], list):
            raise ContractError("Invalid project alias/history lists")
        for value in entry["aliases"]:
            canonical(value)
        if len(set(entry["aliases"])) != len(entry["aliases"]):
            raise ContractError("Duplicate historical remote alias")
        for event in entry["history"]:
            history(event, data)
    if len(remotes) != len(set(remotes)):
        raise ContractError("One canonical remote is bound to multiple project IDs")
    for key, entry in data["checkouts"].items():
        if not re.fullmatch(r"[0-9]+:[0-9]+", key) or not isinstance(entry, dict) or set(entry) != set(SPEC["checkout-fields"]):
            raise ContractError("Invalid checkout binding")
        if entry["project_id"] not in data["projects"] or not isinstance(entry["locations"], list) or not entry["locations"]:
            raise ContractError("Checkout must reference a project and observed locations")
        for location in entry["locations"]:
            if not isinstance(location, str) or not Path(location).is_absolute():
                raise ContractError("Checkout locations must be absolute")
        if entry["authority_name"] is not None:
            text(entry["authority_name"], 128)
        if not isinstance(entry["history"], list):
            raise ContractError("Invalid checkout history")
        for event in entry["history"]:
            history(event, data)
    for operation_id, value in data["operations"].items():
        if (not uuid_valid(operation_id) or not isinstance(value, dict) or set(value) != {"proposal_sha256", "project_id", "at"}
                or value["project_id"] not in data["projects"] or not re.fullmatch("[0-9a-f]{64}", value["proposal_sha256"])):
            raise ContractError("Invalid project operation ledger")
        storage.time_value(value["at"])
    return data


def history(event, data):
    if (not isinstance(event, dict) or set(event) != {"operation_id", "action", "project_id", "canonical_remote", "location", "reason", "at", "previous_project_id"}
            or not uuid_valid(event["operation_id"]) or event["action"] not in SPEC["actions"]
            or event["project_id"] not in data["projects"] or not isinstance(event["location"], str) or not Path(event["location"]).is_absolute()):
        raise ContractError("Invalid project binding history")
    if event["previous_project_id"] is not None and not uuid_valid(event["previous_project_id"]):
        raise ContractError("Invalid previous project identity")
    canonical(event["canonical_remote"])
    text(event["reason"])
    storage.time_value(event["at"])


def observe():
    _, registry, _ = storage.paths("projects")
    raw = registry.read_bytes() if registry.exists() else None
    return (validate(load(raw)) if raw is not None else
            {"schema": SPEC["schema"], "projects": {}, "checkouts": {}, "operations": {}}), digest(raw)


def evidence(repo, data, authority=None, **owned):
    # A saved explicit authority decision remains effective for this checkout.
    initial = discover(repo, authority, **owned)
    known = data["checkouts"].get(initial["checkout_key"])
    configured = git(initial["repo_root"], "config", "--get-all", "agent-memory.authorityRemote", optional=True) if initial["is_git"] else None
    if authority is None and not configured and known and known["authority_name"] is not None:
        return discover(repo, known["authority_name"], **owned)
    return initial


def result(operation, **values):
    return {"schema": "agent-memory/project-result-v1", "operation": operation, "security_boundary": SPEC["security-boundary"],
            "changed_memory_paths": [], **values}


def status(repo, authority=None, **owned):
    data, sha = observe()
    proof = evidence(repo, data, authority, **owned)
    checkout = data["checkouts"].get(proof["checkout_key"])
    project_id = proof["bank_project_id"] or (checkout["project_id"] if checkout else None)
    issues = list(proof["issues"])
    if project_id is None:
        issues.append("No project identity; preview create, then initialize the bank")
    if checkout is None:
        issues.append("Independent checkout is not registered; explicitly bind or link, or fork to a new project")
    elif checkout["project_id"] != project_id:
        issues.append("Bank ID conflicts with this checkout binding; explicit link/fork decision required")
    entry = data["projects"].get(project_id)
    if entry is not None and entry["canonical_remote"] != proof["canonical_remote"]:
        issues.append("Unexpected authority remote change; explicit rebind or fork decision required")
    if proof["bank_project_id"] is None:
        issues.append("Bank initialization or identity update is pending")
    elif proof["bank_canonical_remote"] != proof["canonical_remote"]:
        issues.append("Bank authority evidence differs or is missing; explicitly bind/rebind/fork and update bank identity")
    if entry is None and project_id is not None:
        issues.append("Bank project ID is not locally registered; explicit bind or link required")
    return result("status", project_id=project_id, resolved=not issues, issues=issues, evidence=proof,
                  project_entry=entry, checkout_entry=checkout, observed_sha256=sha, writes_performed=False)


def plan(data, proof, action, target, reason):
    if action not in SPEC["actions"] or proof["issues"]:
        raise ContractError("Repository authority is ambiguous; choose/repair a consistent authority remote before binding")
    text(reason)
    checkout = data["checkouts"].get(proof["checkout_key"])
    bank_id, remote = proof["bank_project_id"], proof["canonical_remote"]
    if action in {"create", "fork"}:
        target = target or str(uuid.uuid4())
        if not uuid_valid(target) or target in data["projects"] or target == bank_id:
            raise ContractError("Create/fork requires a new random project ID")
    elif target is None:
        target = bank_id or (checkout["project_id"] if checkout else None)
    if not uuid_valid(target):
        raise ContractError("Resolve a stable bank project ID before binding/linking")
    if action == "create" and (bank_id is not None or checkout is not None):
        raise ContractError("Existing bank/checkout requires bind, link, rebind or fork, not create")
    if action == "fork" and bank_id is None and checkout is None:
        raise ContractError("Fork requires an existing bank or checkout identity")
    if action == "bind" and (bank_id != target or target in data["projects"] or checkout is not None):
        raise ContractError("Bind adopts a previously unregistered bank; another independent location requires explicit link or fork")
    if action == "link" and (target not in data["projects"] or bank_id != target):
        raise ContractError("Link requires a registered project and matching bank ID")
    if action == "rebind" and (not checkout or checkout["project_id"] != target or bank_id != target):
        raise ContractError("Rebind requires the bank and current checkout to identify the same registered project")
    if action not in {"fork", "rebind"} and proof["bank_canonical_remote"] not in {None, remote}:
        raise ContractError("Copied bank authority conflicts with the observed repository; explicit fork or rebind required")
    if action == "link" and data["projects"][target]["canonical_remote"] != remote:
        raise ContractError("Linked clone authority differs; explicit rebind or fork required")
    for identity, entry in data["projects"].items():
        if remote is not None and entry["canonical_remote"] == remote and identity != target:
            raise ContractError("Authority already identifies another project; link that project rather than silently split its identity")
    return target


def preview(repo, action, target, reason, authority=None):
    data, sha = observe()
    proof = evidence(repo, data, authority)
    target = plan(data, proof, action, target, reason)
    _, registry, lock = storage.paths("projects")
    if lock.exists():
        raise ContractError("Project registry is locked; inspect status/recover")
    proposal = {"schema": "agent-memory/project-proposal-v1", "operation_id": str(uuid.uuid4()), "action": action,
                "project_id": target, "reason": reason, "registry_path": str(registry), "observed_sha256": sha,
                "evidence": proof, "bank_identity": {"project-id": target, "canonical-remote": proof["canonical_remote"]}}
    return result("preview", project_id=target, proposal=proposal, writes_performed=False, verified=False)


def apply(prepared):
    proposal = prepared.get("proposal", prepared) if isinstance(prepared, dict) else prepared
    _, registry, lock = storage.paths("projects")
    if (not isinstance(proposal, dict) or set(proposal) != set(SPEC["proposal-fields"])
            or proposal["schema"] != "agent-memory/project-proposal-v1" or not uuid_valid(proposal["operation_id"])
            or proposal["registry_path"] != str(registry) or proposal["action"] not in SPEC["actions"]
            or not isinstance(proposal["evidence"], dict)):
        raise ContractError("Invalid project proposal or config-root binding")
    request_sha = digest(encoded(proposal))
    with storage.exclusive(lock, proposal["operation_id"], "project-" + proposal["action"]):
        data, sha = observe()
        known = data["operations"].get(proposal["operation_id"])
        if known:
            if known["proposal_sha256"] != request_sha:
                raise ContractError("Operation ID already used for a different proposal")
            # Never reapply old binding decisions after a later rebind/fork.
            try:
                current_status = status(proposal["evidence"]["repo_root"])
            except (ContractError, OSError) as exc:
                current_status = {"resolved": False, "issues": [str(exc)]}
            return result("apply", operation_id=proposal["operation_id"], project_id=known["project_id"],
                          already_applied=True, registry_path=str(registry), writes_performed=False, verified=True,
                          binding_status=current_status)
        if sha != proposal["observed_sha256"]:
            raise ContractError("Project registry changed after preview; reread before retrying")
        proof = evidence(proposal["evidence"]["repo_root"], data, proposal["evidence"]["authority_name"])
        if proof != proposal["evidence"]:
            raise ContractError("Repository/bank evidence changed after preview; prepare a fresh binding decision")
        target = plan(data, proof, proposal["action"], proposal["project_id"], proposal["reason"])
        if proposal["bank_identity"] != {"project-id": target, "canonical-remote": proof["canonical_remote"]}:
            raise ContractError("Proposed bank identity differs from the resolved decision")
        updated = copy.deepcopy(data)
        entry = updated["projects"].setdefault(target, {"canonical_remote": proof["canonical_remote"], "aliases": [], "history": []})
        old_remote = entry["canonical_remote"]
        if proposal["action"] == "rebind" and old_remote != proof["canonical_remote"]:
            if old_remote is not None and old_remote not in entry["aliases"]:
                entry["aliases"].append(old_remote)
            entry["canonical_remote"] = proof["canonical_remote"]
        event = {"operation_id": proposal["operation_id"], "action": proposal["action"], "project_id": target,
                 "canonical_remote": proof["canonical_remote"], "location": proof["location"], "reason": proposal["reason"], "at": storage.now(),
                 "previous_project_id": data["checkouts"].get(proof["checkout_key"], {}).get("project_id") or proof["bank_project_id"]}
        entry["history"].append(event)
        checkout = updated["checkouts"].setdefault(proof["checkout_key"], {"project_id": target, "locations": [], "authority_name": None, "history": []})
        checkout["project_id"] = target
        checkout["authority_name"] = proof["authority_name"]
        if proof["location"] not in checkout["locations"]:
            checkout["locations"].append(proof["location"])
        checkout["history"].append(event)
        updated["operations"][proposal["operation_id"]] = {"proposal_sha256": request_sha, "project_id": target, "at": event["at"]}
        storage.replace(registry, encoded(validate(updated)), sha)
        return result("apply", operation_id=proposal["operation_id"], project_id=target, already_applied=False,
                      registry_path=str(registry), writes_performed=True, verified=True, binding_status=status(proof["repo_root"]))


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    normalizer = commands.add_parser("normalize")
    normalizer.add_argument("remote")
    normalizer.add_argument("--base")
    for name in ("status", "preview"):
        sub = commands.add_parser(name)
        sub.add_argument("--repo", default=".")
        sub.add_argument("--authority")
        if name == "preview":
            sub.add_argument("--action", choices=SPEC["actions"], required=True)
            sub.add_argument("--project-id")
            sub.add_argument("--reason", required=True)
    writer = commands.add_parser("apply")
    writer.add_argument("--proposal", required=True)
    recovery = commands.add_parser("recover")
    recovery.add_argument("--proposal")
    args = parser.parse_args(argv)
    try:
        if args.command == "normalize":
            receipt = result("normalize", canonical_remote=normalize(args.remote, args.base), writes_performed=False)
        elif args.command == "status":
            receipt = status(args.repo, args.authority)
        elif args.command == "preview":
            receipt = preview(args.repo, args.action, args.project_id, args.reason, args.authority)
        elif args.command == "apply":
            receipt = apply(load(Path(args.proposal).read_bytes()))
        else:
            _, registry, lock = storage.paths("projects")
            receipt = result("recover", **storage.recover(registry, lock, {"project-" + a for a in SPEC["actions"]},
                             load(Path(args.proposal).read_bytes()) if args.proposal else None))
        print(json.dumps(receipt, indent=2, ensure_ascii=False))
        return 0
    except (ContractError, OSError, UnicodeError, ValueError, TypeError, KeyError, AttributeError, subprocess.TimeoutExpired) as exc:
        print(json.dumps(result(args.command, error=str(exc), verified=False)))
        return 1


if __name__ == "__main__":
    sys.exit(main())
