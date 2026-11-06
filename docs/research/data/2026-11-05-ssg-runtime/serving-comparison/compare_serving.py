#!/usr/bin/env python3
"""Private synthetic, serial, same-weight serving comparison; never a G1 gate.

Freeze first, commit the frozen inputs, then run against existing artifacts only.
No downloads/builds, remote providers, held-out tuning, or service adoption.
"""
from __future__ import annotations

import argparse
from datetime import datetime, timezone
import math
import os
from pathlib import Path
import platform
import re
import socket
import subprocess
import sys
import time

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent / "runtime-controls"))
from common import choose_model, command, create_run, digest, inventory, load, request, save, sha, validate

BASE = "c267b0071d6869bb2e5fc1ea368cf6d38374b3cc"
WEIGHT = Path("/tmp/am-v15-delivery/Qwen3-Embedding-8B-Q8_0.gguf")
WEIGHT_SHA = "d20ddc71e8a5c4344f2343481e242233a997dc5eaff442427a945836c97b4deb"
SERVERS = {
    "fork": {"binary": "/tmp/am-v15-delivery/llama-reference-build/bin/llama-server",
             "source": "/tmp/am-v15-delivery/llama-reference",
             "commit": "a578bb6b94529b7b2721d0ee49eaea5dbb2c6d07", "build": "9341",
             "binary_sha256": "b57ea6bb97b43a4f74b5a5ab71f1cc2ef292899d83c0e45ddfa9290b169598b5"},
    "upstream": {"binary": "/opt/homebrew/opt/llama.cpp/bin/llama-server",
                 "commit": "6b4e4bd582e457a47ee7cd498058495f47fee228", "build": "9360",
                 "binary_sha256": "619eff2bb5fa47bbcaec8804c5659ef426fa4a9aceb4e30ed33bc9d63a82d203"},
}


def timestamp():
    return datetime.now(timezone.utc).isoformat()


def freeze(path):
    spec = load(HERE / "cache-probe-spec.json")
    by_id = {d["id"]: d["own"] for d in spec["documents"]}
    rendered = {"probes": spec["probes"],
                "queries": [spec["query_prefix"] + q["text"] for q in spec["queries"]],
                "own": [d["own"] for d in spec["documents"]],
                "relationship": [d["own"] + spec["relationship_separator"] + by_id[d["neighbor"]]
                                 for d in spec["documents"]],
                "short": spec["short"], "batch_rows": spec["batch_rows"]}
    for key in ("over_context", "aggregate_budget"):
        item = spec[key]
        rendered[key] = item["repeat"] * item["count"] + item["suffix"]
    if path.exists():
        raise ValueError("Refusing to overwrite frozen inputs")
    pinned = artifacts()
    save(path, {"schema": "am255/serving-input-freeze-v2", "frozen_at": timestamp(),
                "spec": spec, "spec_sha256": sha(HERE / "cache-probe-spec.json"),
                "pre_output_artifacts": pinned,
                "pre_output_artifact_identity": artifact_identity(pinned),
                "script_sha256": sha(Path(__file__)), "common_sha256": sha(HERE.parent / "runtime-controls/common.py"),
                "rendered": rendered, "canonical_rendered_sha256": digest(rendered),
                "rendered_group_sha256": {k: digest(v) for k, v in rendered.items()},
                "canonicalization": "UTF-8 json.dumps(sort_keys=True,separators=(',',':'),allow_nan=False); ensure_ascii default true",
                "quality_answers_inspected": False, "outputs_existed_at_freeze": False})
    print("Frozen canonical rendered inputs:", digest(rendered))


def cosine(a, b):
    dot = math.fsum(x * y for x, y in zip(a, b))
    return dot / math.sqrt(math.fsum(x * x for x in a) * math.fsum(y * y for y in b))


def delta(a, b):
    if len(a) != len(b) or any(len(x) != len(y) for x, y in zip(a, b)):
        raise ValueError("Cannot compare vectors with different shapes")
    return {"rows": len(a), "dimension": len(a[0]),
            "max_abs_coordinate": max(abs(x - y) for av, bv in zip(a, b) for x, y in zip(av, bv)),
            "max_cosine_distance": max(abs(1 - cosine(av, bv)) for av, bv in zip(a, b)),
            "exact_coordinates_equal": a == b}


def brief(answer):
    body = answer["body"]
    return {k: v for k, v in answer.items() if k != "body"} | {
        "model": body.get("model"), "usage": body.get("usage"), "error": body.get("error"),
        "indices": [v.get("index") for v in body.get("data", []) if isinstance(v, dict)]}


def no_servers():
    # Fail closed; never terminate, adopt, or inspect any unrelated server endpoint.
    result = command(["pgrep", "-x", "llama-server"], check=False)
    if result["exit"] not in (0, 1) or result["stdout"].strip():
        raise RuntimeError("Embedding-server exclusivity prerequisite not met: " + result["stdout"])
    return result


def artifacts(root=None):
    weight = WEIGHT.resolve(strict=True)
    if weight.stat().st_size != 8047105824 or sha(weight) != WEIGHT_SHA:
        raise ValueError("Exact approved 8B weight is required")
    report = {"weight": {"path": str(weight), "bytes": weight.stat().st_size, "sha256": WEIGHT_SHA},
              "os_identity": {"sw_vers": command(["sw_vers"])["stdout"], "kernel": command(["uname", "-a"])["stdout"]},
              "dynamic_library_scope": "Pre-output candidate file pins, checked against actual dyld-loaded images per launch; system shared-cache images are UUID/OS-bound, not standalone-file hashes. No reproduced-build or legal attestation.",
              "servers": {}}
    for key, configured in SERVERS.items():
        binary = Path(configured["binary"]).resolve(strict=True)
        if sha(binary) != configured["binary_sha256"]:
            raise ValueError("Serving binary changed: " + key)
        version = command([binary, "--version"])
        text = version["stdout"] + version["stderr"]
        if "version: " + configured["build"] not in text or configured["commit"][:9] not in text:
            raise ValueError("Serving build/source version mismatch: " + key)
        item = dict(configured, resolved_binary=str(binary), version=version,
                    linkage=command(["otool", "-L", binary]))
        roots = [binary.parent] if key == "fork" else [
            binary.parent.parent / "lib", Path("/opt/homebrew/opt/ggml/lib"),
            Path("/opt/homebrew/opt/ggml/libexec")]
        roots += [Path("/opt/homebrew/opt/openssl@4/lib" if key == "fork" else "/opt/homebrew/opt/openssl@3/lib"),
                  Path("/opt/homebrew/opt/libomp/lib")]
        files = {binary}
        for libroot in roots:
            files.update(p.resolve() for p in libroot.rglob("*") if p.is_file()
                         and p.suffix in (".dylib", ".so", ".metallib", ".metal"))
        # Follow concrete transitive dependencies as well as backend candidates.
        pending = list(files)
        while pending:
            path = pending.pop()
            if path.suffix in (".metal", ".metallib"):
                continue
            linkage = command(["otool", "-L", path])["stdout"]
            for line in linkage.splitlines()[1:]:
                dependency = line.strip().split(" (", 1)[0]
                if dependency.startswith("/") and Path(dependency).is_file():
                    resolved = Path(dependency).resolve(strict=True)
                    if resolved not in files:
                        files.add(resolved)
                        pending.append(resolved)
        item["local_library_inventory"] = [{"path": str(p), "sha256": sha(p), "bytes": p.stat().st_size} for p in sorted(files)]
        if key == "fork":
            item["observed_source_revision"] = command(["git", "-C", configured["source"], "rev-parse", "HEAD"])
            if item["observed_source_revision"]["stdout"].strip() != configured["commit"]:
                raise ValueError("Reference source checkout changed")
            item["source_worktree_diff"] = command(["git", "-C", configured["source"], "diff", "HEAD", "--stat"])
            item["source_identity_limit"] = "Existing source checkout and built artifact binding; build was not reproduced"
        else:
            prefix = binary.parent.parent
            receipt = prefix / "INSTALL_RECEIPT.json"
            formula = prefix / ".brew/llama.cpp.rb"
            if load(receipt)["source"]["scm_revision"] != configured["commit"]:
                raise ValueError("Upstream package receipt source changed")
            item["package_provenance"] = {str(p): {"sha256": sha(p), "content": p.read_text()}
                                          for p in (receipt, formula, prefix / "sbom.spdx.json")}
            ggml_prefix = Path("/opt/homebrew/opt/ggml").resolve(strict=True)
            item["ggml_runtime_package"] = {
                "prefix": str(ggml_prefix),
                "receipt_sha256": sha(ggml_prefix / "INSTALL_RECEIPT.json"),
                "receipt": load(ggml_prefix / "INSTALL_RECEIPT.json"),
                "build_time_ggml_version": "0.13.0",
                "installed_ggml_version": load(ggml_prefix / "INSTALL_RECEIPT.json")["source"]["versions"]["stable"]}
            item["source_identity_limit"] = "b9360 bottle built against ggml 0.13.0; this run pins installed ggml and verifies actual loaded backend paths. The old run did not hash its backend modules and is not retroactively attested."
        report["servers"][key] = item
    if root is not None:
        save(root / "artifacts.json", report)
    return report


def artifact_identity(bound):
    return digest({"weight": bound["weight"], "os_identity": bound["os_identity"],
                   "servers": {key: item["local_library_inventory"] for key, item in bound["servers"].items()}})


def loaded_closure(work, kind, bound):
    """Bind observed loads, including dlopen backends, rather than guessing lib roots."""
    text = (work / "server-stderr.log").read_text()
    pins = {item["path"]: item for item in bound["servers"][kind]["local_library_inventory"]}
    images = {}
    for uuid, name in re.findall(r"dyld\[\d+\]:\s+<([0-9A-Fa-f-]+)>\s+(/[^\n]+)", text):
        path = Path(name)
        if path.is_file():
            resolved = str(path.resolve(strict=True))
            observed = {"path": resolved, "sha256": sha(path), "bytes": path.stat().st_size}
            system_image = name.startswith(("/System/", "/usr/lib/"))
            if not system_image and (resolved not in pins or observed != pins[resolved]):
                raise ValueError("Unfrozen or changed dynamically loaded image: " + resolved)
            images[resolved] = observed | {"dyld_uuid": uuid, "observed_path": name,
                                          "pre_output_binding": "OS build" if system_image else "exact file hash"}
        else:
            if not name.startswith(("/System/", "/usr/lib/")):
                raise ValueError("Unresolved non-system dynamically loaded image: " + name)
            images[name] = {"path": name, "dyld_uuid": uuid, "binding": "OS dyld shared-cache image; not an independently hashable file"}
    local = [row for row in images.values() if "sha256" in row]
    for backend in ("ggml-metal", "ggml-cpu", "ggml-blas"):
        if not any(backend in row["path"] for row in local):
            raise ValueError("Actual loaded backend not observed: " + backend)
    closure = {"schema": "am255/observed-dyld-closure-v1", "images": sorted(images.values(), key=lambda row: row["path"]),
               "local_file_closure_sha256": digest(sorted(local, key=lambda row: row["path"])),
               "complete_observed_image_identity_sha256": digest(sorted(images.values(), key=lambda row: row["path"])),
               "os": command(["sw_vers"]), "kernel": command(["uname", "-a"]),
               "scope": "Every DYLD_PRINT_LIBRARIES image observed through all requests. Local executable/dependency/backend bytes pinned before outputs; OS shared-cache images UUID/build-bound. Metal embedded-library status is in retained logs; no cross-host/driver/reproducible-build attestation."}
    save(work / "loaded-closure.json", closure)
    return closure


def cache_evidence(work, frozen, traces):
    text = (work / "server-stderr.log").read_text()
    # HTTP completion can race with unrelated trailing log writes. Match actual
    # task IDs over the retained stream, then group the strictly serial requests.
    prefixes = {int(task): {"task_id": int(task), "cached_prefix_tokens": int(a), "memory_start": int(b)}
                for task, a, b in re.findall(r"task\s+(\d+)\s+\| n_tokens = (\d+), memory_seq_rm \[(\d+), end\)", text)}
    completed = [{"task_id": int(task), "total_tokens": int(a), "evaluated_batch_tokens": int(b)}
                 for task, a, b in re.findall(r"task\s+(\d+)\s+\| prompt processing done, n_tokens = (\d+), batch.n_tokens = (\d+)", text)]
    rows, offset = [], 0
    for trace in traces:
        done = completed[offset:offset + trace["expected_rows"]]
        splits = [prefixes[row["task_id"]] for row in done]
        rows.append(trace | {"prefix_splits": splits, "completed_prompts": done,
                             "scope": "Actual task-ID-bound token logs, grouped in serial request order; not inferred from HTTP-field acceptance"})
        offset += trace["expected_rows"]
    expected, observed = offset, len(completed)
    report = {"frozen_profile": frozen["spec"]["cache_profile"],
              "ram_cache_disabled_log_confirmed": "prompt cache is disabled" in text and "prompt cache is enabled" not in text,
              "successful_embedding_rows": expected, "logged_completed_prompts": observed, "requests": rows,
              "slot_prefix_reuse_observed": any(s["cached_prefix_tokens"] > 0 for row in rows for s in row["prefix_splits"]),
              "per_request_cache_field_support_claimed": False,
              "scope": "Reference embedding source does not parse cache_prompt; field deliberately absent on both servers. RAM cache disabled does not disable slot-local reuse. Actual prefix/evaluated-token logs define upstream behavior independently."}
    save(work / "cache-evidence.json", report)
    if not report["ram_cache_disabled_log_confirmed"]:
        raise ValueError("Chosen --cache-ram 0 was not confirmed by actual server logs")
    if observed != expected or len(prefixes) != expected or any(
            split["cached_prefix_tokens"] + done["evaluated_batch_tokens"] != done["total_tokens"]
            for row in rows for split, done in zip(row["prefix_splits"], row["completed_prompts"])):
        raise ValueError("Incomplete or inconsistent actual token/cache log coverage")
    return report


def policy(counts, spec):
    if len(counts) > spec["maintenance_policy"]["max_inputs"]:
        return "maintenance-batch-input-limit"
    if any(n > spec["settings"]["context"] for n in counts):
        return "maintenance-single-input-context-limit"
    if sum(counts) > spec["maintenance_policy"]["aggregate_tokens"]:
        return "maintenance-aggregate-token-limit"
    return "accepted"


def ranking(query, docs, frozen):
    ids = [d["id"] for d in frozen["spec"]["documents"]]
    rows = []
    for q, vector in zip(frozen["spec"]["queries"], query):
        order = sorted(((cosine(vector, d), key) for key, d in zip(ids, docs)), key=lambda x: (-x[0], x[1]))
        found = [key for _, key in order].index(q["expected"]) + 1
        rows.append({"query_sha256": digest(q["text"]), "expected_synthetic": q["expected"],
                     "order": [key for _, key in order], "scores": dict((key, score) for score, key in order),
                     "expected_rank": found, "reciprocal_rank": 1 / found})
    return rows


def rank_delta(left, right):
    return {"changed_query_orders": sum(a["order"] != b["order"] for a, b in zip(left, right)),
            "changed_top1": sum(a["order"][0] != b["order"][0] for a, b in zip(left, right)),
            "max_rank_displacement": max(abs(a["order"].index(key) - b["order"].index(key))
                                         for a, b in zip(left, right) for key in a["order"]),
            "max_score_drift": max(abs(a["scores"][key] - b["scores"][key])
                                   for a, b in zip(left, right) for key in a["order"])}


def launch(root, kind, number, bound, frozen):
    no_servers()
    work = root / (kind + "-cold-" + str(number))
    work.mkdir(mode=0o700)
    settings = frozen["spec"]["settings"]
    with socket.socket() as available:
        available.bind(("127.0.0.1", 0))
        port = available.getsockname()[1]
    if port in (18192, 18193):
        raise ValueError("Refusing previously used service ports")
    base = "http://127.0.0.1:" + str(port)
    argv = [bound["servers"][kind]["resolved_binary"], "--model", bound["weight"]["path"],
            "--host", "127.0.0.1", "--port", str(port), "--alias", settings["alias"],
            "--embedding", "--pooling", settings["pooling"], "--ctx-size", str(settings["context"]),
            "--parallel", "1", "--threads", "4", "--threads-batch", "4", "--threads-http", "4",
            "--batch-size", "8192", "--ubatch-size", "8192", "--device", settings["device"], "--gpu-layers", "99",
            "--flash-attn", "off", "--fit", "off", "--offline", "--no-webui",
            "--cache-ram", str(settings["cache_ram_mib"]), "--log-verbosity", str(settings["log_verbosity"])]
    env = {k: v for k, v in os.environ.items() if not (k.startswith("LLAMA_") or k.startswith("GGML_") or k.startswith("TURBO_") or k.startswith("DYLD_") or k in ("OPENAI_API_KEY", "HF_TOKEN"))}
    env["OMP_NUM_THREADS"] = "4"
    env["DYLD_PRINT_LIBRARIES"] = "1"
    save(work / "launch.json", {"argv": argv, "at": timestamp(), "canonical_inputs_sha256": frozen["canonical_rendered_sha256"],
                               "weight_sha256": bound["weight"]["sha256"], "binary_sha256": bound["servers"][kind]["binary_sha256"],
                               "removed_environment_prefixes": ["LLAMA_", "GGML_", "TURBO_", "DYLD_"], "OMP_NUM_THREADS": "4",
                               "observability_environment": {"DYLD_PRINT_LIBRARIES": "1"},
                               "cache_profile": frozen["spec"]["cache_profile"],
                               "cold_definition": "New process/model load; OS file cache not purged; normal shared host load"})
    start = time.monotonic()
    with (work / "server-stdout.log").open("xb") as out, (work / "server-stderr.log").open("xb") as err:
        os.chmod(work / "server-stdout.log", 0o600)
        os.chmod(work / "server-stderr.log", 0o600)
        process = subprocess.Popen(argv, env=env, stdout=out, stderr=err)
    save(work / "owner.json", {"pid": process.pid, "argv": argv, "uid": os.getuid(), "root": str(work),
                               "ps": command(["ps", "-p", str(process.pid), "-o", "lstart=", "-o", "command="], check=False)})
    responses, vectors, traces = {}, {}, []

    def call(name, route, value=None):
        sent = {"name": name, "route": route, "body": value, "at": timestamp()}
        save(work / (name + "-request.json"), sent)
        answer = request(base + route, "", value, timeout=180)
        save(work / (name + "-response.json"), answer)
        responses[name] = brief(answer)
        return answer

    def embedding(name, texts, model=None):
        request_inputs_sha256 = digest(texts)
        answer = call(name, "/v1/embeddings", {"input": texts, "model": model or settings["alias"]})
        traces.append({"name": name, "expected_rows": len(texts), "input_sha256": request_inputs_sha256})
        validate(answer, model or settings["alias"], len(texts), settings["dimension"])
        # Preserve provider coordinates verbatim; common.validate returns normalized copies.
        values = [row["embedding"] for row in sorted(answer["body"]["data"], key=lambda row: row["index"])]
        vectors[name] = values
        return values

    try:
        deadline = start + 240
        attempts = []
        while True:
            if process.poll() is not None:
                raise RuntimeError("Owned " + kind + " launch exited; retained stderr is authoritative")
            try:
                health = request(base + "/health", "", timeout=2)
                attempts.append(brief(health))
                if health["status"] == 200:
                    break
            except (OSError, ValueError) as error:
                attempts.append({"error": type(error).__name__})
            if time.monotonic() > deadline:
                raise TimeoutError("Owned readiness bound exceeded")
            time.sleep(.2)
        startup = time.monotonic() - start
        save(work / "readiness.json", {"seconds": startup, "attempts": attempts, "successful_health": health})
        models = call("models", "/v1/models")
        call("props", "/props")
        catalog = [row.get("id") for row in models["body"].get("data", [])]
        if models["status"] != 200 or catalog != [settings["alias"]]:
            raise ValueError("Single independently discovered metadata identity prerequisite failed")
        try:
            choose_model(models, frozen["spec"]["unknown_alias"])
        except ValueError as error:
            metadata_unknown_refusal = str(error)
        else:
            raise ValueError("Independent metadata accepted unknown model")
        for name in ("probes", "queries", "own", "relationship"):
            for repeat in range(3 if name == "probes" else 2):
                embedding(name + "-" + str(repeat), frozen["rendered"][name])
        repeated = {name: [delta(vectors[name + "-0"], vectors[name + "-" + str(i)])
                           for i in range(1, 3 if name == "probes" else 2)]
                    for name in ("probes", "queries", "own", "relationship")}
        token_counts = {}
        for name in ("short", "over_context", "aggregate_budget"):
            answer = call("tokenize-" + name, "/tokenize", {"content": frozen["rendered"][name], "add_special": True})
            tokens = answer["body"].get("tokens")
            if answer["status"] != 200 or not isinstance(tokens, list):
                raise ValueError("Real tokenizer prerequisite failed")
            token_counts[name] = len(tokens)
        embedding("short", [frozen["rendered"]["short"]])
        over = call("over-context", "/v1/embeddings", {"model": settings["alias"], "input": [frozen["rendered"]["over_context"]]})
        unknown_vectors = embedding("unknown-model", frozen["rendered"]["probes"][:1], frozen["spec"]["unknown_alias"])
        unknown = responses["unknown-model"]
        known = embedding("known-single", frozen["rendered"]["probes"][:1])
        unknown_comparison = delta(known, unknown_vectors)
        for name, value in (("empty-input", []), ("invalid-input", {"synthetic": "invalid type"})):
            call(name, "/v1/embeddings", {"model": settings["alias"], "input": value})
        for count in (1, 4, 8, 9):
            embedding("batch-" + str(count), frozen["rendered"]["batch_rows"][:count])
        ranks = {name: ranking(vectors["queries-0"], vectors[name + "-0"], frozen) for name in ("own", "relationship")}
        probe_mean = sum(responses["probes-" + str(i)]["seconds"] for i in range(3)) / 3
        costs = {"startup_seconds": startup, "probe_mean_seconds": probe_mean,
                 "own_six_document_fill_seconds": responses["own-0"]["seconds"],
                 "relationship_six_document_fill_seconds": responses["relationship-0"]["seconds"],
                 "probe_fraction_of_tiny_own_fill": probe_mean / responses["own-0"]["seconds"],
                 "probe_tokens": responses["probes-0"]["usage"],
                 "own_fill_tokens": responses["own-0"]["usage"], "relationship_fill_tokens": responses["relationship-0"]["usage"],
                 "performance_acceptance": False}
        limits = {"token_counts": token_counts, "over_context_refused": token_counts["over_context"] > 8192 and over["status"] in (400, 413, 422),
                  "maintenance_policy": {"evidence_class": "Offline draft policy using actual tokenizer; would refuse, not an implemented AM client or blocked request",
                                         "nine_inputs_would_refuse": policy([token_counts["short"]] * 9, frozen["spec"]),
                                         "two_input_aggregate_tokens": token_counts["aggregate_budget"] * 2,
                                         "aggregate_would_refuse": policy([token_counts["aggregate_budget"]] * 2, frozen["spec"]),
                                         "nine_input_embedding_request_sent": True,
                                         "large_aggregate_embedding_request_sent": False},
                  "empty_input_status": responses["empty-input"]["status"], "invalid_input_status": responses["invalid-input"]["status"],
                  "batch_statuses": {str(n): responses["batch-" + str(n)]["status"] for n in (1, 4, 8, 9)},
                  "scope": "Observed short batches and one over-context input; no maximum-batch or universal no-truncation proof"}
        result = {"kind": kind, "launch": number, "path": work.name, "responses": responses, "repeated": repeated,
                  "rankings": ranks, "own_to_relationship": {"vectors": delta(vectors["own-0"], vectors["relationship-0"]),
                                                            "repeated_ranks": {name: rank_delta(ranks[name], ranking(vectors["queries-1"], vectors[name + "-1"], frozen)) for name in ("own", "relationship")},
                                                            "ranks": rank_delta(ranks["own"], ranks["relationship"])},
                  "identity": {"catalog": catalog, "dimension": 4096, "same_document_query_model": True,
                               "unknown_alias_in_catalog": frozen["spec"]["unknown_alias"] in catalog,
                               "metadata_unknown_refusal": metadata_unknown_refusal,
                               "unknown_alias_http": unknown["status"], "unknown_alias_vector_comparison": unknown_comparison,
                               "unknown_alias_is_identity_proof": False, "identity_authority": "Owned launch + exact weight/binary/observed dependency closure; not response model echo"},
                  "limits": limits, "costs": costs}
    finally:
        if process.poll() is None:
            process.terminate()
            try:
                process.wait(timeout=20)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait(timeout=10)
        save(work / "reaped.json", {"pid": process.pid, "exit": process.returncode, "reaped": True,
                                   "only_owned_popen_signaled": True, "at": timestamp(),
                                   "stdout_sha256": sha(work / "server-stdout.log"), "stderr_sha256": sha(work / "server-stderr.log")})
    # The logger may still be draining after the final HTTP response. Parse only
    # the finalized stream after reaping, never a live log snapshot.
    closure = loaded_closure(work, kind, bound)
    result["identity"]["observed_closure_sha256"] = closure["complete_observed_image_identity_sha256"]
    result["cache_evidence"] = cache_evidence(work, frozen, traces)
    save(work / "result.json", result)
    return result, vectors


def run(output, frozen_path):
    spec_path = HERE / "cache-probe-spec.json"
    frozen = load(frozen_path)
    if frozen["spec_sha256"] != sha(spec_path) or frozen["canonical_rendered_sha256"] != digest(frozen["rendered"]):
        raise ValueError("Frozen inputs changed")
    if frozen["script_sha256"] != sha(Path(__file__)) or frozen["common_sha256"] != sha(HERE.parent / "runtime-controls/common.py"):
        raise ValueError("Freeze the executable and helper before producing new outputs")
    source_paths = [Path(__file__).resolve(), HERE.parent / "runtime-controls/common.py", frozen_path, spec_path]
    command(["git", "ls-files", "--error-unmatch", "--", *source_paths])
    command(["git", "diff", "--exit-code", "HEAD", "--", *source_paths])
    freeze_commit = command(["git", "log", "-1", "--format=%H", "--", frozen_path])["stdout"].strip()
    if not freeze_commit:
        raise ValueError("Commit the input/cache/artifact freeze before executing comparisons")
    root = create_run(output, "same-weight-explicit-cache-serving-comparison")
    save(root / "source-bindings.json", {"base_commit": BASE, "freeze_commit": freeze_commit,
         "execution_commit": command(["git", "rev-parse", "HEAD"])["stdout"].strip(),
         "clone": str(Path.cwd()), "script_sha256": sha(Path(__file__)), "common_sha256": sha(HERE.parent / "runtime-controls/common.py"),
         "frozen_inputs_file_sha256": sha(frozen_path), "spec_sha256": sha(spec_path),
         "canonical_inputs_sha256": frozen["canonical_rendered_sha256"],
         "pre_output_artifact_identity": frozen["pre_output_artifact_identity"],
         "committed_source_paths_clean_before_outputs": True,
         "started_at": timestamp(), "platform": platform.platform(), "python": platform.python_version(),
         "contention": "Normal shared host load; graph comparison/other ordinary jobs may overlap. No clean timing reservation or OS cache purge.",
         "host": command(["sysctl", "hw.model", "hw.memsize", "hw.ncpu"]), "load_before": os.getloadavg()})
    bound = artifacts(root)
    reports, values, cross = [], {}, []
    try:
        if artifact_identity(bound) != frozen["pre_output_artifact_identity"]:
            raise ValueError("Candidate dependencies changed after pre-output freeze")
        for kind, number in (("fork", 1), ("fork", 2), ("upstream", 1), ("upstream", 2)):
            result, vectors = launch(root, kind, number, bound, frozen)
            reports.append(result)
            values[(kind, number)] = vectors
        for left, right in ((("fork", 1), ("fork", 2)), (("upstream", 1), ("upstream", 2)),
                            (("fork", 1), ("upstream", 1)), (("fork", 2), ("upstream", 2))):
            a, b = values[left], values[right]
            ar = next(r for r in reports if (r["kind"], r["launch"]) == left)
            br = next(r for r in reports if (r["kind"], r["launch"]) == right)
            cross.append({"left": list(left), "right": list(right),
                          "vectors": {name: delta(a[name + "-0"], b[name + "-0"]) for name in ("probes", "queries", "own", "relationship")},
                          "ranks": {name: rank_delta(ar["rankings"][name], br["rankings"][name]) for name in ("own", "relationship")}})
        tolerances = frozen["spec"]["diagnostics"]
        deltas = [d for r in reports for group in r["repeated"].values() for d in group]
        deltas += [d for item in cross for d in item["vectors"].values()]
        stable = all(d["max_abs_coordinate"] <= tolerances["coordinate_tolerance"] and d["max_cosine_distance"] <= tolerances["cosine_distance_tolerance"] for d in deltas)
        ranks_stable = all(d["changed_query_orders"] == 0 for item in cross for d in item["ranks"].values())
        coordinate_ok = all(d["max_abs_coordinate"] <= tolerances["coordinate_tolerance"] for d in deltas)
        cosine_ok = all(d["max_cosine_distance"] <= tolerances["cosine_distance_tolerance"] for d in deltas)
        result = {"schema": "am255/same-weight-serving-comparison-v2", "status": "actual-comparison-complete", "launches": reports,
                  "cross_comparisons": cross, "canonical_inputs_sha256": frozen["canonical_rendered_sha256"],
                  "diagnostic_tolerances_frozen_before_execution": tolerances,
                  "diagnostic_outcome": {"coordinate_within_1e_5": coordinate_ok, "cosine_within_1e_5": cosine_ok,
                                         "coordinate_and_cosine_within_diagnostic": stable, "cross_rank_orders_unchanged": ranks_stable,
                                         "eligibility_or_upstream_usability_verdict": False},
                  "api_observations": {"all_over_context_refused": all(r["limits"]["over_context_refused"] for r in reports),
                                       "empty_input_statuses": [r["limits"]["empty_input_status"] for r in reports],
                                       "malformed_input_statuses": [r["limits"]["invalid_input_status"] for r in reports],
                                       "server_cutover_blocker_inferred_from_empty_malformed": False},
                  "candidate_identity": {"status": "source-and-observed-kernel-closure-bound-synthetic-candidate-only",
                                         "upstream_build": "9360", "installed_ggml": bound["servers"]["upstream"]["ggml_runtime_package"]["installed_ggml_version"],
                                         "observed_launch_closures": {r["path"]: r["identity"]["observed_closure_sha256"] for r in reports}},
                  "client_contract_draft": {"implemented_in_actual_am_production_client": False,
                    "input_preflight": "Would refuse empty/non-string input, >8 rows, >8192 individual/aggregate tokens before sending; treat every non-2xx or invalid vector response as failure. This comparator deliberately sends invalid/batch9 diagnostic requests.",
                    "generic_byo_api": "URL plus optional key; explicit model only if discovery is ambiguous. No universal provider-specific cache fields.",
                    "identity_guard": "Local candidate uses owned launch/weight/binary/dependency closure, never the alias echo.",
                    "scope": "Applies to all serving profiles; HTTP500 without vectors fails closed and is not comparative evidence against upstream usability."},
                  "decision_authority": "Diagnostics only. Parent owns profile/fingerprint, persisted-vector rebuild/reuse, ADR and G1. No acceptance criterion is created or relaxed.",
                  "excluded_claims": ["G1 accepted", "final ADR/profile freeze", "Linux pass", "clean timing", "performance pass", "full quality pass", "legal pass", "production implementation", "0.6B comparison"],
                  "finished_at": timestamp(), "load_after": os.getloadavg()}
        save(root / "results.json", result)
        print("Actual comparison: diagnostic-only; no eligibility or usability verdict")
        print("Coordinate/cosine diagnostic:", stable, "rank unchanged:", ranks_stable)
    except BaseException as error:
        save(root / "failure.json", {"type": type(error).__name__, "error": str(error), "at": timestamp(), "completed_launches": len(reports)})
        raise
    finally:
        save(root / "exclusivity-after.json", no_servers())
        save(root / "artifact-inventory.json", inventory(root))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=("freeze", "run"))
    parser.add_argument("--output", type=Path)
    parser.add_argument("--freeze-file", type=Path, default=HERE / "cache-frozen-inputs-finalized.json")
    args = parser.parse_args()
    if args.action == "freeze":
        freeze(args.freeze_file)
    elif args.output is None:
        parser.error("run requires --output (new private evidence directory)")
    else:
        run(args.output, args.freeze_file)


if __name__ == "__main__":
    main()
