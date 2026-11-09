#!/usr/bin/env python3
"""Offline acceptance gate. Default is incremental; --release requires all v1 contracts."""
from __future__ import annotations

import argparse
from collections import deque
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import sys
import traceback

ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "tests"))
from world import World
from fault_partitions import OWNERS, registered as registered_partitions, extract as partition_proof, coverage as partition_coverage

REQUIRED = {
    "public-cli-user-journey",
    "bank-roundtrip", "identity-isolation", "observed-writes", "learning-admission",
    "retrieval-context", "correction-erasure", "initialize-evidence", "cross-clone-sharing",
    "concurrent-sharing", "offline-replay", "later-code-merge", "portable-lifecycle",
    "security-canaries", "skill-routes",
    "installed-bank-initialization", "installed-bank-qualified-recall", "installed-bank-governance",
}
LAYERS = {"pack", "lifecycle", "bank", "concurrency", "client", "optional", "evaluation"}


def registry():
    data = json.loads((ROOT / "tests" / "contracts.json").read_text())
    if data.get("schema") != "agent-memory/test-contracts-v1":
        raise ValueError("Unsupported contract registry schema")
    entries = data["suites"]
    names = [entry["name"] for entry in entries]
    if len(names) != len(set(names)) or not (REQUIRED | {"harness", "optional-stores"}).issubset(names):
        raise ValueError("Registry must declare every mandatory contract exactly once")
    if set(entry["layer"] for entry in entries) != LAYERS:
        raise ValueError("Registry must route every contract layer")
    for entry in entries:
        if entry["name"] not in REQUIRED | {"harness", "optional-stores"} and not entry.get("path"):
            raise ValueError("Additional foundation suites must be implemented")
        if entry["name"] == "optional-stores":
            if entry.get("scope") != "future" or not entry.get("reason") or entry.get("path"):
                raise ValueError("Optional stores must be explicitly outside v0.0.1")
        elif entry.get("scope") not in {"v0.0.1", "v0.0.2", "v0.0.3", "v0.0.4", "v0.0.6", "v0.0.7", "v0.0.8", "v0.0.9", "v0.0.10", "v0.0.15"}:
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
    registered_partitions(entries)
    return entries


def case_batch_coverage(entries, suites):
    by_name = {suite["name"]: suite for suite in suites}
    paths = {entry["path"] for entry in entries if "tests" in entry}
    summaries = []
    for path in sorted(paths):
        complete, executed = None, set()
        batches = [entry for entry in entries if entry.get("path") == path]
        for entry in batches:
            result = by_name.get(entry["name"], {})
            selection = result.get("case_selection")
            if result.get("status") != "passed" or not isinstance(selection, dict):
                raise ValueError("Missing successful registered case batch: " + entry["name"])
            inventory, selected = selection.get("complete"), selection.get("selected")
            if (selection.get("batch") != entry["name"] or not isinstance(inventory, list)
                    or not inventory or any(not isinstance(case, str) for case in inventory)
                    or len(inventory) != len(set(inventory)) or not isinstance(selected, list)
                    or not selected or any(not isinstance(case, str) for case in selected)
                    or len(selected) != len(set(selected)) or result.get("tests") != len(selected)):
                raise ValueError("Invalid registered case-batch proof")
            if complete is None:
                complete = set(inventory)
            if set(inventory) != complete or executed.intersection(selected):
                raise ValueError("Registered case batches disagree or overlap")
            executed.update(selected)
        if executed != complete:
            raise ValueError("Registered case batches omit actual tests")
        summaries.append({"path": path, "batches": len(batches), "cases": sorted(executed)})
    return summaries


def shard_argument(value):
    try:
        index, count = map(int, value.split("/"))
    except ValueError as exc:
        raise argparse.ArgumentTypeError("Use zero-based INDEX/COUNT") from exc
    if not 2 <= count <= 16 or not 0 <= index < count:
        raise argparse.ArgumentTypeError("Require 2..16 shards and 0 <= INDEX < COUNT")
    return index, count


def shard_entries(entries, index, count):
    return sorted(entries, key=lambda entry: entry["name"])[index::count]


def qualification_binding():
    revisions = subprocess.check_output(
        ["git", "-C", str(ROOT), "rev-parse", "HEAD", "HEAD^{tree}"],
        text=True, timeout=10).splitlines()
    return {
        "commit": revisions[0], "tree": revisions[1],
        "registry_sha256": hashlib.sha256((ROOT / "tests/contracts.json").read_bytes()).hexdigest(),
        "gate_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
        "python": list(sys.version_info[:2]), "platform": sys.platform,
        "architecture": platform.machine(),
    }


def aggregate_shards(entries, reports, binding):
    """Refuse partial, overlapping or foreign qualification as a release."""
    if not reports or any(not isinstance(report, dict)
                          or not isinstance(report.get("shard"), dict) for report in reports):
        raise ValueError("No valid shard receipts")
    count = reports[0].get("shard", {}).get("count")
    if type(count) is not int or not 2 <= count <= 16 or len(reports) != count:
        raise ValueError("Missing or invalid shard count")
    indexes, suites = set(), []
    for report in reports:
        shard = report.get("shard", {})
        index = shard.get("index")
        if (report.get("schema") != "agent-memory/test-gate-v1"
                or report.get("mode") != "release-shard"
                or report.get("binding") != binding or shard.get("count") != count
                or type(index) is not int or not 0 <= index < count or index in indexes
                or report.get("release_complete") is not False
                or report.get("shard_complete") is not True
                or report.get("passed") is not True or "error" in report):
            raise ValueError("Invalid, failed, duplicate or foreign shard receipt")
        selected = shard_entries(entries, index, count)
        expected = {entry["name"]: entry for entry in selected}
        rows = report.get("suites")
        if (not isinstance(rows, list) or any(not isinstance(row, dict) for row in rows)
                or len(rows) != len(expected)
                or {row.get("name") for row in rows} != set(expected)):
            raise ValueError("Shard omits, duplicates or adds a registered suite")
        for row in rows:
            entry = expected[row["name"]]
            status = "not-applicable" if entry["scope"] == "future" else "passed"
            if row.get("layer") != entry["layer"] or row.get("status") != status:
                raise ValueError("Shard contains failed or incorrectly scoped coverage")
        indexes.add(index)
        suites.extend(rows)
    if indexes != set(range(count)):
        raise ValueError("Shard indexes are incomplete")
    return suites


def complete_coverage(entries, report):
    report["fault_partition_coverage"] = partition_coverage(report["suites"])
    report["case_batch_coverage"] = case_batch_coverage(entries, report["suites"])
    report["release_complete"] = all(
        suite["status"] in {"passed", "not-applicable"} for suite in report["suites"])
    report["passed"] = report["release_complete"]


def main(argv=None):
    if sys.version_info < (3, 11):
        print("The offline contract gate requires Python 3.11+", file=sys.stderr)
        return 1
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true", help="fail on missing v0.0.1 coverage")
    parser.add_argument("--json", action="store_true", help="emit a machine-readable coverage receipt")
    parser.add_argument("--jobs", type=int, choices=(1, 2), default=1,
                        help="run one or two isolated contract workers (default: one)")
    distribution = parser.add_mutually_exclusive_group()
    distribution.add_argument("--shard", type=shard_argument, metavar="INDEX/COUNT",
                              help="run one zero-based release shard; never a complete release")
    distribution.add_argument("--aggregate", type=Path, nargs="+", metavar="RECEIPT",
                              help="verify every release shard and aggregate complete coverage")
    args = parser.parse_args(argv)
    if (args.shard or args.aggregate) and not args.release:
        parser.error("--shard and --aggregate require --release")
    report = {"schema": "agent-memory/test-gate-v1", "mode": "release" if args.release else "incremental",
              "suites": [], "release_complete": False, "passed": False}
    active = None; stage = 'registry'
    def diagnostic(exc):
        parts = ["".join(traceback.format_exception(exc))]
        seen = set(); displayed = set()
        while exc is not None and id(exc) not in seen:
            seen.add(id(exc))
            for label, value in (('partial stdout', getattr(exc, 'output', None)),
                                 ('partial stderr', getattr(exc, 'stderr', None))):
                if value and label not in displayed:
                    if isinstance(value, bytes): value = value.decode('utf-8', errors='replace')
                    parts.append('\n' + label + ':\n' + value + '\n'); displayed.add(label)
            exc = exc.__cause__ or exc.__context__
        return ''.join(parts)

    def execute(entry):
        item = {"name": entry["name"], "layer": entry["layer"]}
        output = []
        stage = 'world-setup'
        try:
            if not entry.get("path"):
                item.update(status="not-applicable" if entry["scope"] == "future" else "pending",
                            reason=entry["reason"], tests=0)
            else:
                with World() as world:
                    try:
                        stage = 'suite-execution-and-collection'
                        result = world.run([sys.executable, "-I", "-B", ROOT / "scripts/run_contract_suite.py",
                                            ROOT / entry["path"], "--suite", entry["name"]],
                                           check=False, timeout=300, group=True)
                        output.append(result.stderr)
                        stage = 'result-readback'
                        try:
                            counts = json.loads(result.stdout)
                        except ValueError:
                            output.append(f"\nWorker exit status: {result.returncode}\nWorker stdout:\n{result.stdout}\n")
                            raise
                        item.update(counts)
                        item["status"] = "passed" if result.returncode == 0 and counts["passed"] else "failed"
                        if item['status'] == 'passed' and entry['name'] in OWNERS:
                            item['fault_partition'] = partition_proof(result.stderr, entry['name'])
                    except (subprocess.TimeoutExpired, ValueError, KeyError) as exc:
                        output.append(diagnostic(exc))
                        item.update(status="failed", reason=str(exc), tests=0)
                    stage = 'world-cleanup'
        except (OSError, ValueError, KeyError) as exc:
            return item, stage, ''.join(output), exc
        return item, stage, ''.join(output), None

    try:
        entries = registry()
        selected = entries
        if args.shard or args.aggregate:
            stage = "qualification-binding"
            report["binding"] = qualification_binding()
        if args.aggregate:
            active = "release-shards"; stage = "shard-aggregation"
            receipts = [json.loads(path.read_text()) for path in args.aggregate]
            report["suites"] = aggregate_shards(entries, receipts, report["binding"])
            stage = "coverage-aggregation"
            complete_coverage(entries, report)
        else:
            if args.shard:
                index, count = args.shard
                selected = shard_entries(entries, index, count)
                report.update(mode="release-shard", shard={"index": index, "count": count},
                              shard_complete=False)
            pending = deque()
            remaining = iter(selected)
            with ThreadPoolExecutor(max_workers=args.jobs) as workers:
                def enqueue():
                    entry = next(remaining, None)
                    if entry is not None:
                        pending.append((entry, workers.submit(execute, entry)))
                for _ in range(args.jobs):
                    enqueue()
                try:
                    while pending:
                        entry, future = pending.popleft()
                        active = entry['name']
                        item, stage, output, error = future.result()
                        sys.stderr.write(output)
                        if error is not None:
                            item.update(status='failed', reason=str(error), tests=0)
                            report["suites"].append(item)
                            for _, waiting in pending:
                                waiting.cancel()
                            raise error
                        report["suites"].append(item)
                        enqueue()
                finally:
                    # Every started World owns its supervisor and finishes cleanup.
                    # Preserve its outcome even if an earlier collector failed.
                    for entry, future in pending:
                        if future.cancelled():
                            continue
                        item, worker_stage, output, error = future.result()
                        sys.stderr.write(output)
                        if error is not None:
                            sys.stderr.write(diagnostic(error))
                            item.update(status='failed', reason=str(error), tests=0,
                                        error_stage=worker_stage)
                        report["suites"].append(item)
            if args.shard:
                report["shard_complete"] = (
                    len(report["suites"]) == len(selected) and all(
                        suite["status"] in {"passed", "not-applicable"}
                        for suite in report["suites"]))
                report["passed"] = report["shard_complete"]
            else:
                active = 'lifecycle-fault-partitions'; stage = 'coverage-aggregation'
                report['fault_partition_coverage'] = partition_coverage(report['suites'])
                active = 'registered-case-batches'
                report['case_batch_coverage'] = case_batch_coverage(entries, report['suites'])
                pending = any(s["status"] == "pending" for s in report["suites"])
                failed = any(s["status"] == "failed" for s in report["suites"])
                report["release_complete"] = not pending and not failed
                report["passed"] = not failed and (not args.release or report["release_complete"])
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as exc:
        sys.stderr.write(diagnostic(exc))
        report["error"] = str(exc)
        report['error_suite'] = active
        report['error_stage'] = stage
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
