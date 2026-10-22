#!/usr/bin/env python3
"""Validate the checked-in v0.0.1 execution queue, not product behavior."""
import json
from pathlib import Path
import subprocess


def check():
    root = Path(__file__).resolve().parents[1]
    plan = json.loads((root / "docs/design/v0.0.1-work.json").read_text())
    design = (root / plan["design"]).read_text()
    stories = plan["stories"]
    by_key = {s["work_key"]: s for s in stories}
    assert len(stories) == len(by_key) == 52, "Expected 52 distinct release stories"
    assert len({s["issue"] for s in stories}) == 52, "Duplicate issue identity"
    release_gates = [s for s in stories if s["delivery"]["kind"] == "release"]
    assert len(release_gates) == 1 and release_gates[0]["issue"] == 96
    final = release_gates[0]
    assert final["delivery"]["release"] == plan["release"] == "v0.0.1"
    covered = set()
    pending = list(final["depends_on"])
    while pending:
        key = pending.pop()
        assert key in by_key, "Unknown release predecessor"
        if key not in covered:
            covered.add(key)
            pending.extend(by_key[key]["depends_on"])
    assert covered == set(by_key) - {final["work_key"]}
    inspected_sources = set()
    for s in stories:
        assert s["milestone"] == plan["release"]
        assert s["outcome"] and s["acceptance"] and s["validation"]
        assert s["fixtures"] and all(f"`{f}`" in design for f in s["fixtures"])
        deps = s["depends_on"]
        assert len(deps) <= 50, "GitHub dependency relationship limit exceeded"
        assert len(deps) == len(set(deps)), (s["issue"], "duplicate predecessor")
        assert all(d in by_key for d in deps), (s["issue"], "outside-release predecessor")
        assert all(by_key[d]["wave"] < s["wave"] for d in deps), "Cycle or wrong wave"
        assert s["wave"] == (1 + max(by_key[d]["wave"] for d in deps) if deps else 0)
        if s is not final:
            assert s["delivery"] == {"kind": "merge"}, "Prerequisite must deliver before release"
        source = s["source"]
        identity = (source["path"], source["commit"], source["blob"])
        if identity not in inspected_sources:
            for ref in (source["commit"], "HEAD"):
                blob = subprocess.check_output(
                    ["git", "rev-parse", f"{ref}:{source['path']}"], cwd=root, text=True
                ).strip()
                assert blob == source["blob"], (source["path"], "source drift", ref)
            inspected_sources.add(identity)
    assert len(inspected_sources) == 12
    print(json.dumps({"ok": True, "stories": len(stories),
                      "waves": 1 + max(s["wave"] for s in stories),
                      "merge_deliveries": 51, "release_gates": 1,
                      "source_bindings": len(inspected_sources),
                      "approval": plan["approval"], "runtime_tested": False}))


if __name__ == "__main__":
    check()
