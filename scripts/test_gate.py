#!/usr/bin/env python3
"""Offline acceptance gate. Default is incremental; --release requires all v1 contracts."""
from __future__ import annotations

import argparse
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "tests"))
from world import World

REQUIRED = {
    "bank-roundtrip", "identity-isolation", "observed-writes", "learning-admission",
    "retrieval-context", "correction-erasure", "initialize-evidence", "cross-clone-sharing",
    "concurrent-sharing", "offline-replay", "later-code-merge", "portable-lifecycle",
    "security-canaries", "skill-routes",
}
LAYERS = {"pack", "lifecycle", "bank", "concurrency", "client", "optional", "evaluation"}


def registry():
    data = json.loads((ROOT / "tests" / "contracts.json").read_text())
    if data.get("schema") != "agent-memory/test-contracts-v1":
        raise ValueError("Unsupported contract registry schema")
    entries = data["suites"]
    names = [entry["name"] for entry in entries]
    if len(names) != len(set(names)) or not (REQUIRED | {"harness", "optional-stores"}).issubset(names):
        raise ValueError("Registry must declare every v0.0.1 contract exactly once")
    if set(entry["layer"] for entry in entries) != LAYERS:
        raise ValueError("Registry must route every contract layer")
    for entry in entries:
        if entry["name"] not in REQUIRED | {"harness", "optional-stores"} and not entry.get("path"):
            raise ValueError("Additional foundation suites must be implemented")
        if entry["name"] == "optional-stores":
            if entry.get("scope") != "future" or not entry.get("reason") or entry.get("path"):
                raise ValueError("Optional stores must be explicitly outside v0.0.1")
        elif entry.get("scope") != "v0.0.1":
            raise ValueError("Required contracts cannot be marked optional")
        if entry.get("path"):
            raw = ROOT / entry["path"]
            path = raw.resolve()
            if raw.is_symlink() or not path.is_relative_to(ROOT / "tests") or not path.is_file():
                raise ValueError(f"Invalid suite path: {entry['name']}")
        elif not entry.get("reason"):
            raise ValueError("Pending suites require an explicit reason")
    if not next(e for e in entries if e["name"] == "harness").get("path"):
        raise ValueError("Harness suite must be implemented")
    return entries


def main(argv=None):
    if sys.version_info < (3, 11):
        print("The offline contract gate requires Python 3.11+", file=sys.stderr)
        return 1
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true", help="fail on missing v0.0.1 coverage")
    parser.add_argument("--json", action="store_true", help="emit a machine-readable coverage receipt")
    args = parser.parse_args(argv)
    report = {"schema": "agent-memory/test-gate-v1", "mode": "release" if args.release else "incremental",
              "suites": [], "release_complete": False, "passed": False}
    try:
        entries = registry()
        for entry in entries:
            item = {"name": entry["name"], "layer": entry["layer"]}
            if not entry.get("path"):
                item.update(status="not-applicable" if entry["scope"] == "future" else "pending",
                            reason=entry["reason"], tests=0)
            else:
                with World() as world:
                    try:
                        result = world.run([sys.executable, "-I", ROOT / "scripts/run_contract_suite.py",
                                            ROOT / entry["path"]], check=False, timeout=300, group=True)
                        sys.stderr.write(result.stderr)
                        counts = json.loads(result.stdout)
                        item.update(counts)
                        item["status"] = "passed" if result.returncode == 0 and counts["passed"] else "failed"
                    except (subprocess.TimeoutExpired, ValueError, KeyError) as exc:
                        item.update(status="failed", reason=str(exc), tests=0)
            report["suites"].append(item)
        pending = any(s["status"] == "pending" for s in report["suites"])
        failed = any(s["status"] == "failed" for s in report["suites"])
        report["release_complete"] = not pending and not failed
        report["passed"] = not failed and (not args.release or report["release_complete"])
    except (OSError, ValueError, KeyError) as exc:
        report["error"] = str(exc)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        for suite in report["suites"]:
            print(f"{suite['status']}: {suite['name']} ({suite.get('tests', 0)} tests)")
        print("Release coverage: " + ("complete" if report["release_complete"] else "INCOMPLETE"))
        print("PASS" if report["passed"] else "FAIL")
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
