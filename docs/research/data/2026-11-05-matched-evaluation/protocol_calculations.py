#!/usr/bin/env python3
"""#248 research-only calculation sheet and worked-manifest source check.

This is NOT the #36 evaluation harness: it launches no agent, model, harness or
test suite. It (1) recomputes the protocol's run/pair/token/cost/time arithmetic
from explicit placeholder assumptions, (2) computes exact few-cluster sign-flip
limits and an illustrative seeded power simulation, and (3) checks the worked
manifest's pins and solver-tree registry transformation against the actual Git
source. Run from the repository root: python3 -I <this file>.
"""
from __future__ import annotations

import io
import json
import random
import subprocess
import sys
import tarfile
import tempfile
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parents[3]
HARNESSES = ("omp", "codex", "claude-code", "opencode")

# ---- Placeholder assumptions copied from the approved plan; NOT measurements ----
IN_TOKENS, OUT_TOKENS = 120_000, 20_000          # cumulative per multi-turn run
IN_RATE, OUT_RATE = 3.0, 15.0                     # USD per million tokens (placeholder)
CACHE_READ_FRACTION_OF_INPUT_RATE = 0.1           # placeholder for warm-cache sensitivity only
MINUTES_PER_RUN = 10
SCENARIOS = (0.5, 1, 2, 5, 10)


def run_cost(multiplier=1.0, cached_share=0.0):
    tin, tout = IN_TOKENS * multiplier, OUT_TOKENS * multiplier
    fresh, cached = tin * (1 - cached_share), tin * cached_share
    usd = (fresh * IN_RATE + cached * IN_RATE * CACHE_READ_FRACTION_OF_INPUT_RATE + tout * OUT_RATE) / 1e6
    return tin + tout, usd


def designs():
    pilot_base = 12 * len(HARNESSES)
    pilot_repeat = 4 * 2 * len(HARNESSES)
    pilot_pairs = pilot_base + pilot_repeat
    confirm_pairs = 60 * 3 * len(HARNESSES)
    # Mechanism pilot (omp only, all memory-on variants; off arm never padded):
    # 24 items (12 coding, 6 investigation, 6 orientation) x 3 exposure variants,
    # plus 12 unpadded off runs for the investigation/orientation total-effect reference.
    mech_on_variant_runs = 24 * 3
    mech_off_reference_runs = 6 + 6
    # Off-arm discovery canary: per harness, one clean and one seeded-sentinel session.
    canary_runs = 2 * len(HARNESSES)
    return {
        "pilot": {"pairs": pilot_pairs, "runs": 2 * pilot_pairs, "per_arm_runs": pilot_pairs,
                  "base_pairs": pilot_base, "repeat_pairs": pilot_repeat},
        "confirmatory": {"pairs": confirm_pairs, "runs": 2 * confirm_pairs, "per_arm_runs": confirm_pairs},
        "mechanism_pilot_omp": {"runs": mech_on_variant_runs + mech_off_reference_runs,
                                "on_variant_runs": mech_on_variant_runs,
                                "off_reference_runs": mech_off_reference_runs},
        "discovery_canaries": {"runs": canary_runs},
    }


def budget_table(d):
    rows = []
    for name in ("pilot", "confirmatory", "mechanism_pilot_omp", "discovery_canaries"):
        runs = d[name]["runs"]
        for m in SCENARIOS:
            tokens, usd = run_cost(m)
            _, usd_warm = run_cost(m, cached_share=0.8)
            rows.append({"design": name, "runs": runs, "multiplier": m,
                         "tokens_total": round(tokens * runs),
                         "usd_cold": round(usd * runs, 2), "usd_warm80": round(usd_warm * runs, 2),
                         "serial_minutes": round(MINUTES_PER_RUN * m * runs, 1),
                         "per_arm_usd_cold": round(usd * runs / 2, 2) if name in ("pilot", "confirmatory") else None})
    return rows


def min_two_sided_p(clusters):
    return 2 / 2 ** clusters


def sign_flip_p(values):
    """Exact two-sided sign-flip p for H0: cluster totals symmetric about zero."""
    observed = sum(values)
    sums = [0.0]
    for v in values:
        sums = sums + [s + v for s in sums]
    # flipping subset A changes the total to observed - 2*sum(A)
    hits = sum(1 for s in sums if abs(observed - 2 * s) >= abs(observed) - 1e-12)
    return hits / len(sums)


def simulate_power(repos, tasks, reps, delta, sd_repo, sd_task, base_mean, sims, alpha, seed):
    rng = random.Random(seed)
    per_repo = [tasks // repos + (1 if i < tasks % repos else 0) for i in range(repos)]
    rejections = 0
    for _ in range(sims):
        totals = []
        for r in range(repos):
            u = rng.gauss(0, sd_repo)
            total = 0.0
            for _t in range(per_repo[r]):
                p0 = min(max(rng.betavariate(base_mean * 4, (1 - base_mean) * 4), 0.0), 1.0)
                p1 = min(max(p0 + delta + u + rng.gauss(0, sd_task), 0.0), 1.0)
                diff = sum((rng.random() < p1) - (rng.random() < p0) for _ in range(reps)) / reps
                total += diff
            totals.append(total)  # task-weighted: repository total of task-average differences
        if sign_flip_p(totals) <= alpha:
            rejections += 1
    return rejections / sims


def git(*args):
    return subprocess.run(["git", "-C", str(ROOT), *args], check=True, capture_output=True, text=True).stdout.strip()


def check_manifest(path):
    manifest = json.loads(path.read_text())
    base = manifest["base"]["commit"]
    findings = {"blob_mismatches": [], "checked_blobs": 0}
    pinned = [*manifest["target_source"], manifest["hidden_oracle"], *manifest["grader"]["runner_sources"],
              *[r for r in manifest["solver_tree_transform"]["remove"] if r.get("blob")],
              *manifest["solver_tree_transform"]["visible_overlap_review"]]
    for item in pinned:
        path_key = item.get("path") or item.get("module")
        actual = git("rev-parse", f"{base}:{path_key}")
        findings["checked_blobs"] += 1
        if actual != item["blob"]:
            findings["blob_mismatches"].append({"path": path_key, "manifest": item["blob"], "actual": actual})
    rewrite = manifest["solver_tree_transform"]["rewrite"][0]
    if git("rev-parse", f"{base}:{rewrite['path']}") != rewrite["base_blob"]:
        findings["blob_mismatches"].append({"path": rewrite["path"], "manifest": rewrite["base_blob"]})
    findings["tree_matches"] = git("rev-parse", f"{base}^{{tree}}") == manifest["base"]["tree"]
    findings["integrated_main_same_tree"] = git("rev-parse", f"{manifest['base']['integrated_main']}^{{tree}}") == manifest["base"]["tree"]
    tracked_bank = sorted(git("ls-tree", "-r", "--name-only", base, "memory-bank").splitlines())
    listed_bank = sorted(r["path"] for r in manifest["solver_tree_transform"]["remove"] if r["path"].startswith("memory-bank/"))
    findings["every_committed_bank_file_listed"] = tracked_bank == listed_bank
    findings["committed_bank_files"] = len(tracked_bank)

    # Materialize the base tree privately, apply the manifest's solver transform,
    # and evaluate the ACTUAL registry() rules of the pinned scripts/test_gate.py.
    archive = subprocess.run(["git", "-C", str(ROOT), "archive", base], check=True, capture_output=True).stdout
    with tempfile.TemporaryDirectory(prefix="am248-solver-", dir="/tmp") as scratch:
        tree = Path(scratch)
        with tarfile.open(fileobj=io.BytesIO(archive)) as tar:
            tar.extractall(tree, filter="data")
        sys.path[:0] = [str(tree / "scripts"), str(tree / "tests")]
        import test_gate  # noqa: E402  (pinned bytes from the extracted base)
        import fault_partitions  # noqa: E402
        oracle = manifest["hidden_oracle"]["module"]
        (tree / oracle).unlink()
        for item in manifest["solver_tree_transform"]["remove"]:
            target = tree / item["path"]
            if target.is_file():
                target.unlink()
        try:
            test_gate.registry()
            findings["registry_without_rewrite"] = "valid"
        except ValueError as exc:
            findings["registry_without_rewrite"] = "invalid: " + str(exc)
        registry_path = tree / rewrite["path"]
        data = json.loads(registry_path.read_text())
        names = {e["name"] for e in data["suites"]}
        drop = set(rewrite["drop_entries"])
        findings["dropped_entries_exist"] = drop <= names
        findings["dropped_entries_not_required_or_partition_owned"] = not (
            drop & (test_gate.REQUIRED | {"harness", "optional-stores"} | set(fault_partitions.OWNERS) | {"lifecycle-fault-partitions"}))
        data["suites"] = [e for e in data["suites"] if e["name"] not in drop]
        registry_path.write_text(json.dumps(data, indent=2) + "\n")
        try:
            findings["registry_after_rewrite"] = f"valid ({len(test_gate.registry())} suites)"
        except ValueError as exc:
            findings["registry_after_rewrite"] = "invalid: " + str(exc)
        sys.path[:2] = []
    return findings


def main():
    d = designs()
    tokens1, usd1 = run_cost(1)
    out = {
        "schema": "agent-memory/248-protocol-calculations-v0",
        "assumptions": {"input_tokens_per_run": IN_TOKENS, "output_tokens_per_run": OUT_TOKENS,
                        "usd_per_million_input": IN_RATE, "usd_per_million_output": OUT_RATE,
                        "cache_read_fraction_of_input_rate": CACHE_READ_FRACTION_OF_INPUT_RATE,
                        "minutes_per_run": MINUTES_PER_RUN, "status": "placeholders, not measurements or prices"},
        "per_run": {"tokens": tokens1, "usd": round(usd1, 4)},
        "designs": d,
        "budget": budget_table(d),
        "few_cluster_sign_flip": [{"clusters": r, "patterns": 2 ** r, "min_two_sided_p": min_two_sided_p(r),
                                   "can_reject_at_0.05": min_two_sided_p(r) <= 0.05,
                                   "can_reject_at_holm_first_0.05/3": min_two_sided_p(r) <= 0.05 / 3}
                                  for r in range(3, 13)],
        "illustrative_power": [],
    }
    # Illustrative only: invented variance components; real values must come from the pilot.
    for repos in (5, 6, 8, 10, 12):
        for delta in (0.10, 0.20):
            power = simulate_power(repos, 60, 3, delta, sd_repo=0.05, sd_task=0.10, base_mean=0.4,
                                   sims=300, alpha=0.05, seed=248_000 + repos * 100 + int(delta * 100))
            out["illustrative_power"].append({"repositories": repos, "tasks": 60, "reps": 3, "delta": delta,
                                              "sd_repo": 0.05, "sd_task": 0.10, "base_pass_mean": 0.4,
                                              "sims": 300, "alpha_two_sided": 0.05, "power": round(power, 3)})
    out["worked_manifest_check"] = check_manifest(HERE / "worked-manifest-scope.json")
    print(json.dumps(out, indent=2, sort_keys=True))


if __name__ == "__main__":
    main()
