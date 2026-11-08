#!/usr/bin/env python3
"""Frozen synthetic cosine diagnostic; see design.json before installing or running."""
import argparse
import hashlib
import importlib.metadata as metadata
import json
import math
import os
import platform
import resource
import subprocess
import sys
import time
import traceback
from pathlib import Path

import numpy as np
from usearch.index import Index

DIM = 64
QUERIES = 8
K = 10
BUDGET = 32
SEED = 20261108
TOL = 2e-5
VERSIONS = {"usearch": "2.26.6", "numpy": "2.2.6", "numkong": "7.8.6", "tqdm": "4.67.1"}
SETTINGS = dict(ndim=DIM, metric="cos", dtype="f32", connectivity=16,
                expansion_add=128, expansion_search=64, multi=False)


def sha(data):
    return hashlib.sha256(data).hexdigest()


def file_sha(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def emit(out, event, **details):
    record = {"event": event, "time_ns": time.time_ns(), **details}
    with (out / "events.jsonl").open("a", encoding="utf-8") as f:
        f.write(json.dumps(record, allow_nan=False, sort_keys=True) + "\n")
        f.flush()
        os.fsync(f.fileno())


def validate(a):
    if a.dtype != np.float32 or a.ndim != 2 or a.shape[1] != DIM or not a.flags.c_contiguous:
        raise ValueError("expected contiguous float32 matrix with 64 columns")
    if not np.isfinite(a).all() or np.any(np.all(a == 0, axis=1)):
        raise ValueError("non-finite or zero-norm vector")


def cosine(a, b):
    # Python float conversion of each original float32 is exact; fsum accumulates in float64.
    dot = math.fsum(float(x) * float(y) for x, y in zip(a, b))
    aa = math.fsum(float(x) * float(x) for x in a)
    bb = math.fsum(float(x) * float(x) for x in b)
    if not math.isfinite(dot) or not math.isfinite(aa) or not math.isfinite(bb) or aa == 0 or bb == 0:
        raise ValueError("invalid cosine input")
    return dot / math.sqrt(aa * bb)


def search(index, query, count, exact):
    t = time.perf_counter_ns()
    matches = index.search(query, count, threads=1, exact=exact)
    elapsed = time.perf_counter_ns() - t
    if len(matches.keys) != len(matches.distances):
        raise ValueError(f"library returned unequal key/distance lengths: {len(matches.keys)}, {len(matches.distances)}")
    return [(int(k), float(d)) for k, d in zip(matches.keys, matches.distances)], elapsed


def check_matches(matches, count, domain, label):
    keys = [key for key, _ in matches]
    if (len(matches) != count or len(set(keys)) != count or
            any(key < 0 or key >= domain or not math.isfinite(distance)
                for key, distance in matches)):
        raise ValueError(f"{label}: invalid count, duplicate/out-of-range key, or non-finite score: {matches!r}")


def tie_recall(scores, returned, k):
    ordered = sorted(scores, key=lambda x: (-x[0], x[1]))
    cutoff = ordered[k - 1][0]
    above = {i for score, i in ordered if score > cutoff}
    tied = {i for score, i in ordered if score == cutoff}
    keys = set(returned)
    return (len(keys & above) + min(k - len(above), len(keys & tied))) / k


def controls(out):
    # Full-domain searches isolate score semantics from candidate truncation.
    vectors = np.zeros((5, DIM), dtype=np.float32)
    vectors[0, 0] = 1
    vectors[1, 0] = -2
    vectors[2, 1] = 3
    vectors[3, 0] = 4
    vectors[4, 0] = 1
    q = vectors[0].copy()
    validate(vectors)
    idx = Index(**SETTINGS)
    idx.add(np.arange(len(vectors), dtype=np.uint64), vectors, threads=1)
    matches, ns = search(idx, q, len(vectors), True)
    approx, approx_ns = search(idx, q, len(vectors), False)
    check_matches(matches, len(vectors), len(vectors), "control exact")
    check_matches(approx, len(vectors), len(vectors), "control approximate")
    got = dict(matches)
    approx_got = dict(approx)
    detail = [{"key": i, "raw_exact_distance": got[i],
               "raw_approx_distance": approx_got[i], "oracle_cosine": cosine(q, v),
               "expected_distance": 1 - cosine(q, v)} for i, v in enumerate(vectors)]
    semantic_ok = all(abs(row[field] - row["expected_distance"]) <= TOL
                      for row in detail for field in ("raw_exact_distance", "raw_approx_distance"))
    invalid = {}
    for name, value in (("zero", 0.0), ("nan", float("nan")), ("infinity", float("inf"))):
        bad = np.zeros((1, DIM), dtype=np.float32)
        bad[0, 0] = value
        try:
            validate(bad)
        except ValueError:
            invalid[name] = "rejected_before_library"
        else:
            invalid[name] = "accepted"
    tie_order = sorted(((-cosine(q, vectors[i]), i) for i in (0, 3, 4)))
    # A separate truncated query has one strictly best vector and three tied
    # at the second-place boundary; accepting any of those ties is intentional.
    boundary = np.zeros((5, DIM), dtype=np.float32)
    boundary[0, 0] = 1
    boundary[1:4, 0:2] = 1
    boundary[4, 0] = -1
    validate(boundary)
    boundary_idx = Index(**SETTINGS)
    boundary_idx.add(np.arange(5, dtype=np.uint64), boundary, threads=1)
    boundary_scores = [(cosine(q, v), i) for i, v in enumerate(boundary)]
    boundary_exact, boundary_exact_ns = search(boundary_idx, q, 2, True)
    boundary_approx, boundary_approx_ns = search(boundary_idx, q, 2, False)
    check_matches(boundary_exact, 2, 5, "boundary exact")
    check_matches(boundary_approx, 2, 5, "boundary approximate")
    boundary_recall = tie_recall(boundary_scores, [i for i, _ in boundary_exact], 2)
    boundary_score_ok = all(abs(d - (1 - boundary_scores[i][0])) <= TOL
                            for i, d in boundary_exact + boundary_approx)
    emit(out, "controls", rows=detail, exact_search_ns=ns, approximate_search_ns=approx_ns,
         exact_raw=matches, approximate_raw=approx, score_semantics_ok=semantic_ok,
         invalid_input=invalid, deterministic_tie_keys=[i for _, i in tie_order],
         boundary_oracle_order=sorted(boundary_scores, key=lambda x: (-x[0], x[1])),
         boundary_exact_raw=boundary_exact, boundary_approximate_raw=boundary_approx,
         boundary_exact_ns=boundary_exact_ns, boundary_approximate_ns=boundary_approx_ns,
         boundary_exact_tie_aware_recall=boundary_recall, boundary_score_ok=boundary_score_ok)
    return (semantic_ok and all(v == "rejected_before_library" for v in invalid.values()) and
            [i for _, i in tie_order] == [0, 3, 4] and boundary_recall == 1 and boundary_score_ok)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--size", type=int, choices=(1000, 10000, 50000), default=1000)
    p.add_argument("--out", type=Path, required=True)
    p.add_argument("--large", action="store_true")
    p.add_argument("--smoke-events", type=Path)
    p.add_argument("--corrupt-child", type=Path, help=argparse.SUPPRESS)
    args = p.parse_args()
    if args.corrupt_child:
        # The native loader itself must refuse; exceptions or None mean refusal.
        try:
            restored = Index.restore(args.corrupt_child)
            if restored is None:
                print("library_returned_none")
                return 0
            print("library_accepted_corrupt_index")
            return 3
        except Exception as e:
            print("library_exception", type(e).__name__, str(e))
            return 0
    if sys.implementation.name != "cpython" or sys.version_info[:3] != (3, 13, 12):
        raise RuntimeError("requires CPython 3.13.12")
    if platform.system() != "Darwin" or platform.machine() != "arm64":
        raise RuntimeError("this frozen wheel cell requires macOS arm64")
    versions = {name: metadata.version(name) for name in VERSIONS}
    if versions != VERSIONS:
        raise RuntimeError(f"wrong installed versions: {versions}")
    if args.size > 1000:
        if not args.large or not args.smoke_events:
            raise ValueError("larger cells require --large and a successful --smoke-events receipt")
        events = [json.loads(line) for line in args.smoke_events.read_text().splitlines()]
        if not any(e.get("event") == "complete" and e.get("pass") is True and e.get("size") == 1000 for e in events):
            raise ValueError("smoke receipt does not show a passing 1000-vector run")
    args.out.mkdir(mode=0o700, parents=False, exist_ok=False)
    out = args.out
    try:
        # Integer-valued float32s avoid generator- or platform-dependent transcendental functions.
        rng = np.random.Generator(np.random.PCG64(SEED))
        data = rng.integers(-127, 128, size=(args.size, DIM), dtype=np.int32).astype(np.float32)
        queries = rng.integers(-127, 128, size=(QUERIES, DIM), dtype=np.int32).astype(np.float32)
        validate(data)
        validate(queries)
        # Register hashes/settings on disk before any search, timing, or result output.
        manifest = {"size": args.size, "dim": DIM, "queries": QUERIES, "seed": SEED,
                    "generator": "numpy 2.2.6 PCG64 integers[-127,128), int32 converted to little-endian f32",
                    "data_sha256": sha(memoryview(data).cast("B")),
                    "queries_sha256": sha(memoryview(queries).cast("B")), "tolerance_absolute": TOL,
                    "k": K, "candidate_budget": BUDGET, "settings": SETTINGS, "threads": 1,
                    "versions": versions, "python": sys.version, "python_executable_sha256": file_sha(sys.executable),
                    "platform": platform.platform(), "source_sha256": file_sha(__file__),
                    "design_sha256": file_sha(Path(__file__).with_name("design.json")),
                    "smoke_events_sha256": file_sha(args.smoke_events) if args.smoke_events else None}
        with (out / "manifest.json").open("x", encoding="utf-8") as f:
            json.dump(manifest, f, indent=2, sort_keys=True, allow_nan=False)
            f.write("\n")
            f.flush()
            os.fsync(f.fileno())
        emit(out, "started", manifest_sha256=file_sha(out / "manifest.json"))
        control_ok = controls(out)
        if not control_ok:
            raise RuntimeError("controls failed; refusing data index build")
        index = Index(**SETTINGS)
        t = time.perf_counter_ns()
        index.add(np.arange(args.size, dtype=np.uint64), data, threads=1)
        build_ns = time.perf_counter_ns() - t
        emit(out, "built", build_ns=build_ns, count=len(index), hardware_acceleration=str(index.hardware_acceleration))
        path = out / "index.usearch"
        t = time.perf_counter_ns()
        index.save(path)
        save_ns = time.perf_counter_ns() - t
        emit(out, "saved", save_ns=save_ns, bytes=path.stat().st_size, sha256=file_sha(path))
        t = time.perf_counter_ns()
        restored = Index.restore(path, expansion_search=64)
        load_ns = time.perf_counter_ns() - t
        if restored is None:
            raise RuntimeError("library refused its own saved index")
        emit(out, "reloaded", load_ns=load_ns, count=len(restored))
        corrupt = out / "truncated.usearch"
        with path.open("rb") as src, corrupt.open("xb") as dst:
            dst.write(src.read(64))
        cmd = [sys.executable, "-B", __file__, "--out", str(out), "--corrupt-child", str(corrupt)]
        try:
            proc = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
            corrupt_ok = proc.returncode == 0
            emit(out, "corrupt_load", returncode=proc.returncode, stdout=proc.stdout, stderr=proc.stderr,
                 refused=corrupt_ok, sha256=file_sha(corrupt), bytes=corrupt.stat().st_size)
        except subprocess.TimeoutExpired as e:
            corrupt_ok = False
            emit(out, "corrupt_load", refused=False, timeout_seconds=30, error=str(e))
        all_ok = control_ok and corrupt_ok and len(index) == args.size and len(restored) == args.size
        for qi, q in enumerate(queries):
            t = time.perf_counter_ns()
            # Linear in N for each of the fixed eight queries; no N-by-N matrix.
            scores = [(cosine(q, data[i]), i) for i in range(args.size)]
            oracle = sorted(scores, key=lambda x: (-x[0], x[1]))[:K]
            oracle_ns = time.perf_counter_ns() - t
            exact, exact_ns = search(index, q, BUDGET, True)
            approx, approx_ns = search(index, q, BUDGET, False)
            reload_matches, reload_ns = search(restored, q, BUDGET, False)
            check_matches(exact, BUDGET, args.size, "exact")
            check_matches(approx, BUDGET, args.size, "approximate")
            check_matches(reload_matches, BUDGET, args.size, "reloaded approximate")
            ids = [key for key, _ in approx]
            # Rerank only returned candidates using original, unmodified fixture bytes.
            reranked = sorted(((cosine(q, data[i]), i) for i in ids), key=lambda x: (-x[0], x[1]))[:K]
            exact_ids = {i for i, _ in exact}
            score_errors = [abs(d - (1 - cosine(q, data[i]))) for i, d in exact + approx]
            valid = all(math.isfinite(e) and e <= TOL for e in score_errors)
            same_reload = ([i for i, _ in approx] == [i for i, _ in reload_matches] and
                           all(abs(a[1] - b[1]) <= TOL for a, b in zip(approx, reload_matches)) and
                           len(approx) == len(reload_matches))
            exact_recall = tie_recall(scores, exact_ids, K)
            if exact_recall != 1 or not valid or not same_reload:
                all_ok = False
            emit(out, "query", query=qi, oracle_top10=oracle, oracle_ns=oracle_ns,
                 exact_raw=exact, exact_ns=exact_ns, exact_candidate_recall_at_10=exact_recall,
                 hnsw_raw=approx, hnsw_ns=approx_ns,
                 hnsw_candidate_recall_at_10=tie_recall(scores, ids, K),
                 hnsw_reranked_recall_at_10=tie_recall(scores, [i for _, i in reranked], K),
                 reranked_top10=reranked, max_abs_distance_error=max(score_errors, default=None),
                 matches_valid=valid, reload_search_ns=reload_ns, reload_identical=same_reload)
        emit(out, "complete", size=args.size, **{"pass": all_ok},
             peak_rss_raw=resource.getrusage(resource.RUSAGE_SELF).ru_maxrss,
             peak_rss_units="bytes on macOS", caveat="busy-host single trial; no production or semantic inference")
        return 0 if all_ok else 1
    except BaseException as exc:
        emit(out, "failure", error_type=type(exc).__name__, error=str(exc), traceback=traceback.format_exc())
        return 1


if __name__ == "__main__":
    sys.exit(main())
