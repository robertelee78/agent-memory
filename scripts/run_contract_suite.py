#!/usr/bin/env python3
"""Worker for test_gate.py; reports counts so empty/skipped suites cannot pass."""
import importlib.util
import json
from pathlib import Path
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "tests"))
from world import require_world


def main():
    require_world()
    path = Path(sys.argv[1]).resolve()
    if not path.is_relative_to(ROOT / "tests") or not path.is_file():
        raise ValueError("Suite must be a test file in this checkout")
    spec = importlib.util.spec_from_file_location("contract_suite", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    suite = unittest.defaultTestLoader.loadTestsFromModule(module)
    result = unittest.TextTestRunner(verbosity=2, stream=sys.stderr).run(suite)
    counts = {"tests": result.testsRun, "failures": len(result.failures),
              "errors": len(result.errors), "skipped": len(result.skipped),
              "expected_failures": len(result.expectedFailures),
              "unexpected_successes": len(result.unexpectedSuccesses)}
    counts["passed"] = result.wasSuccessful() and counts["tests"] > 0 and not any(
        counts[key] for key in counts if key != "tests")
    print(json.dumps(counts, sort_keys=True))
    return 0 if counts["passed"] else 1


if __name__ == "__main__":
    sys.exit(main())
