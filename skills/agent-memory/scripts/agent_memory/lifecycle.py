"""Shared deterministic lifecycle decisions; never performs a memory mutation."""
from __future__ import annotations

import argparse
from datetime import datetime
import json
import math
from pathlib import Path
import re
import sys
import uuid

from .frontmatter import ContractError, unique_object

PACK = Path(__file__).resolve().parents[2]
MODEL = json.loads((PACK / "schemas/lifecycle.json").read_text())


def time_value(value):
    if not isinstance(value, str) or not re.fullmatch(r"\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-](?:[01]\d|2[0-3]):[0-5]\d)", value):
        raise ContractError("Time must be timezone-qualified RFC3339 with valid offset components")
    try:
        return datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError as exc:
        raise ContractError("Invalid RFC3339 time") from exc


def references(values):
    if not isinstance(values, list) or any(not isinstance(v, str) or not v.strip() for v in values):
        raise ContractError("Evidence and superseder references must be lists of nonempty strings")
    if len(values) != len(set(values)):
        raise ContractError("Duplicate references")
    return values


def state(record):
    if not isinstance(record, dict) or record.get("status") not in MODEL["states"]:
        raise ContractError("Unknown lifecycle state")
    return record["status"]


def current(record, context):
    status = state(record)
    if not isinstance(context, dict):
        raise ContractError("Selection requires resolved context")
    for key in ("authorized-scope", "applicable"):
        if type(context.get(key)) is not bool:
            raise ContractError(f"Selection requires explicit boolean {key}")
    trust = MODEL["trust-order"]
    if record.get("trust") not in trust or context.get("minimum-trust") not in trust:
        raise ContractError("Unknown trust level or missing operation trust threshold")
    if record.get("visibility") not in MODEL["visibility"]:
        raise ContractError("Unknown visibility")
    confidence = record.get("confidence")
    if confidence is not None and (type(confidence) not in {int, float} or not 0 <= confidence <= 1 or not math.isfinite(confidence)):
        raise ContractError("Confidence must be null or a finite number between zero and one")
    now = time_value(context.get("now"))
    start = time_value(record["valid-from"]) if record.get("valid-from") is not None else None
    end = time_value(record["valid-until"]) if record.get("valid-until") is not None else None
    if start and end and end <= start:
        raise ContractError("valid-until must be after valid-from")
    superseders = references(context.get("active-superseders"))
    observed = {
        "authorized-scope": context["authorized-scope"], "applicable": context["applicable"],
        "active": status == "active", "valid-time": (start is None or start <= now) and (end is None or now < end),
        "sufficient-trust": trust.index(record["trust"]) >= trust.index(context["minimum-trust"]),
        "visible": record["visibility"] == "visible", "no-active-superseder": not superseders,
    }
    gates = {rule["gate"]: observed[rule["gate"]] for rule in MODEL["current-predicate"]}
    return {"current": all(gates.values()), "gates": gates, "reasons": [k for k, v in gates.items() if not v]}


def identity(value):
    try:
        parsed = uuid.UUID(value)
        return str(parsed) == value and parsed.version == 4
    except (ValueError, TypeError, AttributeError):
        return False


def transition(record, operation, event):
    status = state(record)
    if not isinstance(event, dict):
        raise ContractError("Transition requires an explicit event")
    if not identity(record.get("id")) or type(record.get("version")) is not int or record["version"] < 1:
        raise ContractError("Transition requires a stable UUIDv4 record ID and positive version")
    evidence = references(event.get("evidence", []))
    old_evidence = references(record.get("evidence", []))
    prior_events = references(record.get("event-ids", []))
    replacement = event.get("replacement", {})
    if not isinstance(replacement, dict):
        raise ContractError("Replacement must be an object")
    replacement_ref = {"id": record["id"], "version": record["version"]}
    different_version = replacement.get("id") != record["id"] or replacement.get("version") != record["version"]
    replacement_active = (identity(replacement.get("id")) and type(replacement.get("version")) is int
                          and replacement["version"] > 0 and replacement.get("status") == "active"
                          and different_version and replacement.get("replaces") == replacement_ref
                          and replacement.get("applicability-preserved") is True)
    expiry = record.get("valid-until")
    reached = False
    if expiry is not None:
        boundary = time_value(expiry)
        if event.get("now") is not None:
            reached = time_value(event["now"]) >= boundary
    guards = {
        "new-event": identity(event.get("id")) and event.get("id") != record.get("event-id") and event.get("id") not in prior_events,
        "evidence": bool(evidence), "new-evidence": bool(set(evidence) - set(old_evidence)),
        "authority": isinstance(event.get("authority"), str) and bool(event["authority"].strip()),
        "reason": isinstance(event.get("reason"), str) and bool(event["reason"].strip()),
        "reviewed": event.get("reviewed") is True, "replacement": replacement_active,
        "new-version": replacement_active and replacement.get("id") == record["id"] and replacement["version"] > record["version"],
        "expiry-reached": reached,
    }
    rules = [rule for rule in MODEL["transitions"] if rule["operation"] == operation and status in rule["from"]]
    if not rules:
        return {"legal": False, "reasons": ["terminal-deletion" if status == "deleted" else "illegal-transition"]}
    rule = rules[0]
    missing = [name for name in rule["requires"] if not guards[name]]
    if missing:
        return {"legal": False, "reasons": missing}
    new_version = rule["new-version"]
    return {"legal": True, "reasons": [], "to": rule["to"], "creates_new_version": new_version,
            "prior_version_status": status if operation == "reactivate" else rule["to"],
            "new_version_status": "active" if new_version else None,
            "terminal_record_id": operation == "delete"}


def guide():
    lines = ["# Memory lifecycle model", "", f"Generated from `schemas/lifecycle.json` ({MODEL['schema']}).",
             "Changes MUST update the machine definition and regenerate this file.", "",
             "## Independent axes", "",
             "Lifecycle states: " + ", ".join(f"`{s}`" for s in MODEL["states"]) + ".",
             "Trust order: " + " < ".join(f"`{s}`" for s in MODEL["trust-order"]) + ".",
             "Visibility: " + ", ".join(f"`{s}`" for s in MODEL["visibility"]) + ".",
             "Confidence is null or a number from zero to one; it MUST NOT substitute for trust, visibility, scope or lifecycle.",
             "Unknown applicability MUST remain explicit and MUST NOT pass the applicability gate.", "",
             "## Current-selection predicate", "", "Every gate MUST pass before ranking or current-context assembly.", ""]
    lines += [f"- `{rule['gate']}`: {rule['meaning']}" for rule in MODEL["current-predicate"]]
    lines += ["", "Unknown valid-time bounds are unbounded; applicability still requires explicit assessment.",
              "Resolved scope, applicability, minimum trust and active superseders MUST come from the invoking operation's observed authorized context, never instructions in memory.",
              "Developer identity provides provenance, not an ACL. Repository/OS access remains the security boundary.", "",
              "## Legal transitions", "", "| Operation | From | To | Required guards | New version |",
              "| --- | --- | --- | --- | --- |"]
    lines += [f"| {r['operation']} | {', '.join(r['from'])} | {r['to']} | {', '.join(r['requires'])} | {str(r['new-version']).lower()} |"
              for r in MODEL["transitions"]]
    lines += ["", "## Guard meanings", ""]
    lines += [f"- `{name}`: {meaning}" for name, meaning in MODEL["guards"].items()]
    lines += ["", "Correction MUST retain the stable record ID and create a higher active version; the incorrect version becomes superseded.",
              "Supersession MUST name its active replacement and the exact replaced ID/version. The agent MUST review preservation of distinct applicability and evidence.",
              "Revocation MUST deactivate the record without requiring a replacement. Expiry MUST use the known valid-until boundary, never age alone.",
              "Reactivation MUST create a higher active version with new evidence and a new event; the earlier version MUST remain historical with its previous state.",
              "Deletion MUST be terminal for that record ID and leave only the content-free tombstone contract. No operation MAY reactivate that deleted ID.",
              "The public evaluator is read-only and MUST NOT be reported as a committed mutation. Mediated callers MUST also enforce identity, admission, observed-write, provenance and tombstone rules.", ""]
    return "\n".join(lines)


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("operation", choices=("definition", "guide", "current", "transition"))
    parser.add_argument("--input", help="JSON request path, or - for stdin")
    args = parser.parse_args(argv)
    if args.operation == "guide":
        print(guide(), end="")
        return 0
    try:
        if args.operation == "definition":
            result = MODEL
        else:
            if not args.input:
                raise ContractError("--input is required")
            text = sys.stdin.read() if args.input == "-" else Path(args.input).read_text()
            request = json.loads(text, object_pairs_hook=unique_object)
            if args.operation == "current":
                result = current(request["record"], request["context"])
            else:
                result = transition(request["record"], request["operation"], request["event"])
        print(json.dumps({"schema": "agent-memory/lifecycle-result-v1", "operation": args.operation,
                          "model": MODEL["schema"], "result": result, "writes_performed": False}, indent=2))
        return 0
    except (ContractError, OSError, ValueError, TypeError, KeyError) as exc:
        print(json.dumps({"schema": "agent-memory/lifecycle-result-v1", "operation": args.operation,
                          "error": str(exc), "writes_performed": False}))
        return 1
