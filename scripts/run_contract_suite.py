#!/usr/bin/env python3
"""Worker for test_gate.py; reports counts so empty/skipped suites cannot pass."""
import argparse
import importlib.util
import json
import os
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "tests"))
from world import require_world


def leaf_cases(suite):
    for case in suite:
        if isinstance(case, unittest.TestSuite):
            yield from leaf_cases(case)
        else:
            yield case


def registered_suite(module, path, name):
    loader = unittest.defaultTestLoader
    complete = loader.loadTestsFromModule(module)
    entries = json.loads((ROOT / "tests/contracts.json").read_text())["suites"]
    groups = [entry for entry in entries if entry.get("path") == path.relative_to(ROOT).as_posix()]
    if name is not None and sum(entry["name"] == name for entry in groups) != 1:
        raise ValueError("Named suite does not identify this registered contract file")
    if not any("tests" in entry for entry in groups):
        return complete, None
    cases = list(leaf_cases(complete))
    identities = [case.id() for case in cases]
    if len(identities) != len(set(identities)):
        raise ValueError("Contract discovery contains duplicate case identities")
    selected, covered = {}, set()
    for entry in groups:
        selectors = entry.get("tests")
        if not isinstance(selectors, list) or not selectors or any(not isinstance(s, str) or not s for s in selectors):
            raise ValueError("Every batch of a split contract requires explicit nonempty test selectors")
        batch = []
        for selector in selectors:
            parts = selector.split(".")
            if len(parts) not in (1, 2) or (len(parts) == 2 and not parts[1].startswith("test_")):
                raise ValueError("A batch selector must name a TestCase class or its test method")
            matches = [case for case in cases if type(case).__name__ == parts[0] and
                       (len(parts) == 1 or case._testMethodName == parts[1])]
            if not matches:
                raise ValueError("A registered batch selector matches no discovered test")
            batch.extend(matches)
        actual = [case.id() for case in batch]
        if len(actual) != len(set(actual)) or covered.intersection(actual):
            raise ValueError("Registered contract batches overlap")
        covered.update(actual)
        selected[entry["name"]] = batch
    if covered != set(identities):
        raise ValueError("Registered contract batches omit discovered tests")
    if name is None:
        return complete, None
    chosen = selected[name]
    return unittest.TestSuite(chosen), {"batch": name, "complete": sorted(identities),
                                      "selected": sorted(case.id() for case in chosen)}


def main():
    require_world()
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("path", type=Path)
    parser.add_argument("--suite")
    args = parser.parse_args()
    path = args.path.resolve()
    if not path.is_relative_to(ROOT / "tests") or not path.is_file():
        raise ValueError("Suite must be a test file in this checkout")
    # Test and inherited child output is diagnostic, not part of the single
    # machine-readable worker receipt. Redirect the descriptor as well as
    # Python writes so subprocesses cannot corrupt that receipt.
    sys.stdout.flush()
    receipt_fd = os.dup(sys.stdout.fileno())
    try:
        os.dup2(sys.stderr.fileno(), sys.stdout.fileno())
        spec = importlib.util.spec_from_file_location("contract_suite", path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        suite, selection = registered_suite(module, path, args.suite)
        result = unittest.TextTestRunner(verbosity=2, stream=sys.stderr).run(suite)
        sys.stdout.flush()
    finally:
        os.dup2(receipt_fd, sys.stdout.fileno())
        os.close(receipt_fd)
    counts = {"tests": result.testsRun, "failures": len(result.failures),
              "errors": len(result.errors), "skipped": len(result.skipped),
              "expected_failures": len(result.expectedFailures),
              "unexpected_successes": len(result.unexpectedSuccesses)}
    counts["passed"] = result.wasSuccessful() and counts["tests"] > 0 and not any(
        counts[key] for key in counts if key != "tests")
    if selection is not None:
        counts["passed"] = counts["passed"] and result.testsRun == len(selection["selected"])
        counts["case_selection"] = selection
    print(json.dumps(counts, sort_keys=True))
    return 0 if counts["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
