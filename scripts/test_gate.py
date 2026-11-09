#!/usr/bin/env python3
"""Offline acceptance gate. Default is incremental; --release requires all v1 contracts."""
from __future__ import annotations

import argparse
import hashlib
from collections import deque
from concurrent.futures import ThreadPoolExecutor
import json
import os
import platform
from pathlib import Path
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
                    or len(selected) != len(set(selected)) or type(result.get("tests")) is not int
                    or result["tests"] != len(selected)
                    or selection.get("executed") != sorted(selected)):
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

def lifecycle_matrix_coverage(entries, suites):
    matrix = json.loads((ROOT / "tests/lifecycle-matrix.json").read_text())
    required = {"fresh-repeat", "modes", "spaces-optional-clients", "ownership-backups",
                "sentinel-edits", "update-guards", "activation-interrupts", "lost-responses",
                "rollback-locks", "removal-purge", "bank-preservation", "generated-state-loss",
                "client-identity", "pack-validation", "isolation"}
    rows = matrix.get("requirements") if isinstance(matrix, dict) else None
    if (not isinstance(matrix, dict) or matrix.get("schema") != "agent-memory/lifecycle-test-matrix-v1"
            or not isinstance(rows, list) or any(not isinstance(row, dict) for row in rows)
            or len(rows) != len(required) or {row.get("requirement") for row in rows} != required):
        raise ValueError("Invalid approved lifecycle requirements")
    registry = {entry["name"]: entry for entry in entries}
    results = {suite["name"]: suite for suite in suites}
    summaries = []
    for row in rows:
        cases = row.get("cases")
        if not isinstance(cases, list) or not cases:
            raise ValueError("Lifecycle requirement has no executable cases")
        for case in cases:
            if not isinstance(case, dict):
                raise ValueError("Invalid lifecycle case")
            name, test = case.get("suite"), case.get("test")
            if not isinstance(name, str) or not isinstance(test, str):
                raise ValueError("Invalid lifecycle case identity")
            entry, result = registry.get(name, {}), results.get(name, {})
            selection = result.get("case_selection")
            if (entry.get("scope") != "v0.0.1" or not entry.get("path")
                    or result.get("status") != "passed" or not isinstance(selection, dict)):
                raise ValueError("Missing successful lifecycle case suite: " + name)
            complete, selected = selection.get("complete"), selection.get("selected")
            if (selection.get("batch") != name or not isinstance(complete, list)
                    or not complete or any(not isinstance(item, str) for item in complete)
                    or len(complete) != len(set(complete)) or not isinstance(selected, list)
                    or not selected or any(not isinstance(item, str) for item in selected)
                    or len(selected) != len(set(selected)) or type(result.get("tests")) is not int
                    or result["tests"] != len(selected)
                    or selection.get("executed") != sorted(selected)
                    or not set(selected) <= set(complete)):
                raise ValueError("Invalid lifecycle case execution proof: " + name)
            selectors = entry.get("tests")
            if selectors is None:
                if set(selected) != set(complete):
                    raise ValueError("Incomplete lifecycle suite execution: " + name)
            elif not isinstance(selectors, list) or not any(
                    selector == test or selector == test.split(".")[0] for selector in selectors):
                raise ValueError("Lifecycle case is not selected by registered suite: " + name)
            if "contract_suite." + test not in selected:
                raise ValueError("Lifecycle case was not executed: " + name + " / " + test)
        summaries.append({"requirement": row["requirement"], "cases": len(cases)})
    return summaries


def source_identity():
    """Hash checkout inputs, independent of checkout location and generated bytecode."""
    files = []
    for name in ("skills", "scripts", "tests", "memory-bank"):
        base = ROOT / name
        if base.is_symlink() or not base.is_dir():
            raise ValueError("Invalid source directory: " + str(base))
        for directory, dirs, names in os.walk(base, followlinks=False):
            dirs[:] = sorted(d for d in dirs if d != "__pycache__")
            for entry in dirs:
                if (Path(directory) / entry).is_symlink():
                    raise ValueError("Symlink in source inputs")
            for entry in sorted(names):
                path = Path(directory) / entry
                if path.suffix in {".pyc", ".pyo"}:
                    continue
                if path.is_symlink() or not path.is_file():
                    raise ValueError("Non-regular source input: " + str(path))
                files.append(path)
    files.extend([ROOT / "install.sh", ROOT / ".github/workflows/contracts.yml",
                  ROOT / "docs/research/data/2026-10-26-recall-native/literal-quality/1000-labels.json",
                  ROOT / "docs/research/data/2026-10-25-recall-final/semantic-1000/wall-disagreement.json",
                  ROOT / "docs/research/data/2026-10-25-recall-final/semantic-controller-v5.py"])
    digest = hashlib.sha256()
    for path in sorted(files, key=lambda p: p.relative_to(ROOT).as_posix()):
        if (any(part.is_symlink() for part in (path, *path.parents) if part != ROOT and part.is_relative_to(ROOT))
                or not path.is_file()):
            raise ValueError("Invalid source input: " + str(path))
        name = path.relative_to(ROOT).as_posix().encode()
        data = path.read_bytes()
        digest.update(len(name).to_bytes(8, "big") + name)
        digest.update(len(data).to_bytes(8, "big") + data)
    return digest.hexdigest()


def cell_identity(os_label, python_label):
    actual_os = platform.system()
    expected_os = {"ubuntu-latest": "Linux", "macos-latest": "Darwin"}.get(os_label)
    actual_python = f"{sys.version_info.major}.{sys.version_info.minor}"
    if expected_os != actual_os or python_label != actual_python:
        raise ValueError("Declared cell does not match actual OS/Python")
    return {"os": os_label, "python": python_label, "system": actual_os,
            "machine": platform.machine(), "python_version": platform.python_version(),
            "implementation": platform.python_implementation()}


def validate_rows(entries, rows):
    if not isinstance(rows, list) or len(rows) != len(entries):
        raise ValueError("Shard suite count differs from registry assignment")
    for entry, row in zip(entries, rows):
        if not isinstance(row, dict) or row.get("name") != entry["name"] or row.get("layer") != entry["layer"]:
            raise ValueError("Shard suite assignment differs from registry")
        if not entry.get("path"):
            if (entry.get("scope") != "future" or row.get("status") != "not-applicable"
                    or type(row.get("tests")) is not int or row["tests"] != 0
                    or row.get("reason") != entry.get("reason")):
                raise ValueError("Missing or invalid required suite")
            continue
        if (row.get("status") != "passed" or row.get("passed") is not True
                or type(row.get("tests")) is not int or row["tests"] < 1
                or any(type(row.get(key)) is not int or row[key] != 0 for key in
                       ("failures", "errors", "skipped", "expected_failures", "unexpected_successes"))):
            raise ValueError("Suite is failed, empty, skipped, or has invalid counts")


def merge_shards(paths, count, entries, source, cell):
    if len(paths) != count:
        raise ValueError("Exactly one report per shard is required")
    by_index = {}
    for path in paths:
        shard = json.loads(Path(path).read_text())
        if not isinstance(shard, dict):
            raise ValueError("Shard report must be an object")
        scope = shard.get("shard")
        if (shard.get("schema") != "agent-memory/test-gate-v1"
                or shard.get("mode") != "release" or shard.get("passed") is not True
                or "error" in shard or shard.get("release_complete") is not False
                or shard.get("source") != source
                or shard.get("cell") != cell or not isinstance(scope, dict)
                or type(scope.get("count")) is not int or scope["count"] != count
                or type(scope.get("index")) is not int
                or not 0 <= scope["index"] < count or scope["index"] in by_index):
            raise ValueError("Invalid, duplicate, incompatible, or failed shard report")
        index = scope["index"]
        assigned = entries[index::count]
        validate_rows(assigned, shard.get("suites"))
        by_index[index] = shard["suites"]
    if set(by_index) != set(range(count)):
        raise ValueError("Missing shard report")
    rows = [None] * len(entries)
    for index, shard_rows in by_index.items():
        rows[index::count] = shard_rows
    return rows

def main(argv=None):
    if sys.version_info < (3, 11):
        print("The offline contract gate requires Python 3.11+", file=sys.stderr)
        return 1
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--release", action="store_true", help="fail on missing v0.0.1 coverage")
    parser.add_argument("--json", action="store_true", help="emit a machine-readable coverage receipt")
    parser.add_argument("--jobs", type=int, choices=(1, 2), default=1,
                        help="run one or two isolated contract workers (default: one)")
    parser.add_argument("--shard-index", type=int)
    parser.add_argument("--shard-count", type=int)
    parser.add_argument("--cell-os")
    parser.add_argument("--cell-python")
    parser.add_argument("--merge-shards", nargs="+", metavar="REPORT")
    parser.add_argument("--output", type=Path, help="write the JSON receipt to this path")
    args = parser.parse_args(argv)
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
                            if not isinstance(counts, dict):
                                raise ValueError("Worker result must be a JSON object")
                        except ValueError:
                            output.append(f"\nWorker exit status: {result.returncode}\nWorker stdout:\n{result.stdout}\n")
                            raise
                        item.update(counts)
                        item["status"] = "passed" if result.returncode == 0 and counts["passed"] else "failed"
                        if item['status'] == 'passed' and entry['name'] in OWNERS:
                            item['fault_partition'] = partition_proof(result.stderr, entry['name'])
                    except (subprocess.TimeoutExpired, ValueError, KeyError) as exc:
                        output.append(diagnostic(exc))
                        item.update(status="failed", passed=False, reason=str(exc), tests=0)
                    stage = 'world-cleanup'
        except (OSError, ValueError, KeyError) as exc:
            return item, stage, ''.join(output), exc
        return item, stage, ''.join(output), None

    try:
        scoped = args.shard_index is not None or args.merge_shards is not None
        if scoped:
            if (not args.release or args.shard_count is None or args.shard_count < 1
                    or not args.cell_os or not args.cell_python
                    or (args.merge_shards is not None and args.shard_index is not None)
                    or (args.merge_shards is None and
                        (args.shard_index is None or not 0 <= args.shard_index < args.shard_count))):
                raise ValueError("Invalid release shard or merge arguments")
            source = source_identity()
            cell = cell_identity(args.cell_os, args.cell_python)
            report.update(source=source, cell=cell)
        elif args.shard_count is not None or args.cell_os or args.cell_python:
            raise ValueError("Shard count and cell require a shard or merge")
        entries = registry()
        if args.merge_shards is not None:
            report["merged_shards"] = args.shard_count
            report["suites"] = merge_shards(args.merge_shards, args.shard_count, entries, source, cell)
        else:
            if scoped:
                report["shard"] = {"index": args.shard_index, "count": args.shard_count}
                entries_to_run = entries[args.shard_index::args.shard_count]
            else:
                entries_to_run = entries
        pending = deque()
        remaining = iter(entries_to_run if args.merge_shards is None else [])
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
        if args.shard_index is not None:
            validate_rows(entries_to_run, report["suites"])
            report["passed"] = True
        else:
            if args.release:
                validate_rows(entries, report["suites"])
            active = 'lifecycle-fault-partitions'; stage = 'coverage-aggregation'
            report['fault_partition_coverage'] = partition_coverage(report['suites'])
            active = 'registered-case-batches'
            report['case_batch_coverage'] = case_batch_coverage(entries, report['suites'])
            active = 'lifecycle-matrix'
            report['lifecycle_matrix_coverage'] = lifecycle_matrix_coverage(entries, report['suites'])
            pending = any(s["status"] == "pending" for s in report["suites"])
            failed = any(s["status"] == "failed" for s in report["suites"])
            report["release_complete"] = not pending and not failed
            report["passed"] = not failed and (not args.release or report["release_complete"])
        if scoped and source_identity() != source:
            raise ValueError("Source inputs changed during gate execution")
    except (OSError, ValueError, KeyError, TypeError, subprocess.SubprocessError) as exc:
        report["passed"] = False
        report["release_complete"] = False
        sys.stderr.write(diagnostic(exc))
        report["error"] = str(exc)
        report['error_suite'] = active
        report['error_stage'] = stage
    if args.output:
        args.output.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n")
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
