#!/usr/bin/env python3
"""Explicit, serial, owned REAL llama.cpp endpoint qualification; reserved slot only.

No downloads, global config, existing service adoption, or background daemon.
Native backend/platform qualification is limited to the machine actually running it.
"""
from __future__ import annotations
import argparse
import json
import os
from pathlib import Path
import platform
import secrets
import signal
import socket
import subprocess
import sys
import time
from common import (bind_sources, command, create_run, inventory, load, owned_run, request, save, sha)

EXPECTED = {
    "reference": {"sha256": "d20ddc71e8a5c4344f2343481e242233a997dc5eaff442427a945836c97b4deb", "bytes": 8047105824,
                  "alias": "qwen3-embedding-8b-q8_0-d20ddc71e8a5", "dimension": 4096, "context": 8192},
    "alternate": {"sha256": "06507c7b42688469c4e7298b0a1e16deff06caf291cf0a5b278c308249c3e439", "bytes": 639150592,
                  "alias": "qwen3-embedding-06b-q8_0-06507c7b4268", "dimension": 1024, "context": 4096},
}


def port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


def run(config, root, with_graph):
    bind_sources(config["ssg"], root / "source-bindings.json")
    reference_commit = command(["git", "-C", config["reference_source"], "rev-parse", "HEAD"])["stdout"].strip()
    if reference_commit != "a578bb6b94529b7b2721d0ee49eaea5dbb2c6d07":
        raise ValueError("Reference llama.cpp source revision mismatch")
    artifacts = {}
    for profile, expected in EXPECTED.items():
        weight = Path(config[profile + "_weight"]).resolve()
        binary = Path(config[profile + "_binary"]).resolve()
        if weight.stat().st_size != expected["bytes"] or sha(weight) != expected["sha256"]:
            raise ValueError(profile + " weight identity mismatch")
        artifacts[profile] = {"weight": str(weight), **expected, "binary": str(binary), "binary_sha256": sha(binary),
                              "version": command([binary, "--version"], timeout=60)}
        text = artifacts[profile]["version"]["stdout"] + artifacts[profile]["version"]["stderr"]
        required_build = "9341" if profile == "reference" else "9360"
        if "version: " + required_build not in text:
            raise ValueError(profile + " server build differs from approved research profile")
        if profile == "reference":
            artifacts[profile]["source_commit"] = reference_commit
    save(root / "actual-artifacts.json", artifacts)
    reports = []
    here = Path(__file__).resolve().parent
    backend = config.get("backend", "metal")
    if backend not in ("metal", "cpu") or (backend == "metal" and platform.system() != "Darwin"):
        raise ValueError("Managed profile/backend does not match actual host")
    for cell in ("reference", "alternate", "ambiguous", "nonembedding", "key-protected"):
        profile = "reference" if cell == "reference" else "alternate"
        artifact = artifacts[profile]
        work = root / cell
        work.mkdir(mode=0o700)
        listen = port()
        base = "http://127.0.0.1:" + str(listen)
        argv = [artifact["binary"], "--host", "127.0.0.1", "--port", str(listen), "--no-webui"]
        if cell == "ambiguous":
            models = work / "synthetic-router-catalog"
            models.mkdir(mode=0o700)
            for name, item in artifacts.items():
                (models / (name + ".gguf")).symlink_to(item["weight"])
            argv += ["--models-dir", str(models), "--no-models-autoload", "--models-max", "1"]
        else:
            argv += ["--model", artifact["weight"], "--alias", artifact["alias"], "--ctx-size", str(artifact["context"]),
                     "--parallel", "1", "-b", "8192", "-ub", "8192", "-t", "4", "-ngl", "99" if backend == "metal" else "0",
                     "--pooling", "none" if cell == "nonembedding" else "last"]
            if cell != "nonembedding":
                argv += ["--embedding"]
        key_path = None
        if cell == "key-protected":
            credentials = work / "credentials"
            credentials.mkdir(mode=0o700)
            key_path = credentials / "endpoint.key"
            with key_path.open("x") as f:
                os.chmod(key_path, 0o600)
                f.write(secrets.token_hex(32) + "\n")
                f.flush()
                os.fsync(f.fileno())
            argv += ["--api-key-file", str(key_path)]
        argv += ["--log-file", str(work / "server-native.log"), "--offline"]
        env = {k: v for k, v in os.environ.items() if not (k.startswith("LLAMA_") or k == "OPENAI_API_KEY")}
        log = (work / "server.log").open("xb")
        p = subprocess.Popen(argv, stdout=log, stderr=subprocess.STDOUT, env=env)
        log.close()
        process_identity = command(["ps", "-p", str(p.pid), "-o", "lstart=", "-o", "command="], check=False)
        save(work / "owned-process.json", {"pid": p.pid, "argv": argv, "binary_sha256": artifact["binary_sha256"],
             "run_id": load(root / "owner.json")["run_id"], "ps_identity": process_identity["stdout"].strip()})
        try:
            deadline = time.monotonic() + 180
            while True:
                if p.poll() is not None:
                    raise RuntimeError("Owned server exited; inspect private synthetic server.log")
                try:
                    # /health is a documented actual llama.cpp native route; it is not a BYO fallback guess.
                    health = request(base + "/health", "", timeout=2)
                    if health["status"] == 200 and p.poll() is None:
                        break
                except (OSError, ValueError):
                    pass
                if time.monotonic() > deadline:
                    raise TimeoutError("Owned server readiness exceeded 180 seconds")
                time.sleep(.2)
            endpoint_config = {"url": base, "dimension": artifact["dimension"], "context_tokens": artifact["context"],
                               "tokenizer_url": base + "/tokenize"}
            # No explicit model: real single-model discovery is independently exercised.
            if key_path:
                endpoint_config["key_file"] = str(key_path)
            config_path = work / "endpoint-config.json"
            save(config_path, endpoint_config)
            kind = "ambiguous" if cell == "ambiguous" else "nonembedding" if cell == "nonembedding" else "embedding"
            result = command([sys.executable, "-B", here / "endpoint_controls.py", "--config", config_path,
                              "--output", work / "qualification", "--ssg", config["ssg"], "--kind", kind], timeout=900, check=False)
            save(work / "qualification-command.json", result)
            if result["exit"]:
                raise RuntimeError("Endpoint controller failed in " + cell)
            reports.append({"cell": cell, "report": load(work / "qualification/report.json"), "host": platform.platform(),
                            "backend": backend, "profile_identity_evidence": {"weight_sha256": artifact["sha256"],
                            "server_binary_sha256": artifact["binary_sha256"], "source": "owned-launch-artifacts-not-response-model-echo"}})
            if with_graph and cell == "reference":
                graph = command([sys.executable, "-B", here / "graph_controls.py", "run", "--output", root / "graph-controls",
                                 "--engine", config["engine"], "--ssg", config["ssg"], "--ssg-binary", config["ssg_binary"],
                                 "--endpoint-config", config_path, "--reserved-host-slot"], timeout=1800, check=False)
                save(work / "graph-command.json", graph)
                if graph["exit"]:
                    raise RuntimeError("Actual graph controller failed; its owned cleanup receipt is authoritative")
        finally:
            if p.poll() is None:
                p.terminate()
                try:
                    p.wait(timeout=15)
                except subprocess.TimeoutExpired:
                    p.kill()
                    p.wait(timeout=10)
            save(work / "process-stop.json", {"pid": p.pid, "exit": p.returncode, "reaped": True,
                                              "log_sha256": sha(work / "server.log")})
            logs = []
            key_value = key_path.read_text().strip().encode() if key_path and key_path.exists() else None
            for log_path in (work / "server.log", work / "server-native.log"):
                if log_path.exists():
                    raw = log_path.read_bytes()
                    logs.append({"path": log_path.name, "sha256": sha(log_path), "bytes": len(raw),
                                 "credential_detected": key_value in raw if key_value else False})
                    log_path.unlink()
            save(work / "owned-log-disposal.json", {"logs": logs, "all_owned_log_files_unlinked": True,
                                                   "forensic_secure_erase": False})
            if key_path and key_path.exists():
                key_path.unlink()
                leaked = []
                for retained in work.rglob("*"):
                    if retained.is_file() and not retained.is_symlink() and key_value in retained.read_bytes():
                        leaked.append({"path": str(retained.relative_to(work)), "sha256_before": sha(retained)})
                        retained.unlink()
                save(work / "credential-disposal.json", {"private_file_unlinked": not key_path.exists(),
                     "secure_forensic_erase": False, "retained_files_scanned_for_key": True,
                     "credential_echo_artifacts_removed": leaked, "credential_value_in_retained_artifacts": False})
                if leaked:
                    raise ValueError("Actual provider echoed credential into owned evidence; removed and marked incompatible")
    save(root / "report.json", {"schema": "am255/managed-serial-endpoints-v1", "cells": reports,
                                "production": False, "performance_acceptance": False,
                                "platform_scope": "only this observed host/backend; Linux/macOS results cannot be inferred from each other"})


def cleanup(root):
    receipt = owned_run(root)
    results = []
    for record in sorted(root.glob("*/owned-process.json")):
        if record.is_symlink():
            raise ValueError("Symlinked process receipt refused")
        r = load(record)
        if r["run_id"] != receipt["run_id"]:
            raise ValueError("Foreign process receipt refused")
        stopped = record.parent / "process-stop.json"
        if not stopped.exists():
            now = command(["ps", "-p", str(r["pid"]), "-o", "lstart=", "-o", "command="], check=False)
            if now["exit"] == 0:
                if not r["ps_identity"] or now["stdout"].strip() != r["ps_identity"] or sha(r["argv"][0]) != r["binary_sha256"]:
                    raise ValueError("PID reuse/identity mismatch; refusing to signal")
                os.kill(r["pid"], signal.SIGTERM)
                deadline = time.monotonic() + 15
                while time.monotonic() < deadline:
                    state = command(["ps", "-p", str(r["pid"]), "-o", "state="], check=False)
                    if state["exit"] or state["stdout"].strip().startswith("Z"):
                        break
                    time.sleep(.1)
                else:
                    current = command(["ps", "-p", str(r["pid"]), "-o", "lstart=", "-o", "command="], check=False)
                    if current["stdout"].strip() != r["ps_identity"]:
                        raise ValueError("PID changed during cleanup; refusing kill")
                    os.kill(r["pid"], signal.SIGKILL)
        removed = []
        for item in (record.parent / "server.log", record.parent / "server-native.log", record.parent / "credentials/endpoint.key"):
            if item.exists():
                if item.is_symlink() or item.stat().st_uid != os.getuid():
                    raise ValueError("Unknown owned log/credential identity; erasure pending")
                removed.append(str(item.relative_to(root)))
                item.unlink()
        results.append({"process_receipt": str(record.relative_to(root)), "removed_private_files": removed})
    graph = root / "graph-controls"
    if graph.exists():
        results.append({"graph_cleanup": "Run graph_controls.py cleanup with the original engine", "root": str(graph)})
    save(root / "managed-cleanup.json", {"owned_processes": results, "forensic_secure_erase": False})


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument("--config", type=Path)
    p.add_argument("--output", type=Path, required=True)
    p.add_argument("--with-graph", action="store_true")
    p.add_argument("--cleanup", action="store_true")
    p.add_argument("--reserved-host-slot", action="store_true", required=True)
    args = p.parse_args()
    if args.cleanup:
        cleanup(args.output.resolve())
        return
    if args.config is None:
        p.error("--config is required unless --cleanup")
    root = create_run(args.output, "managed-serial-endpoints")
    # SIGTERM exits through per-process finally; SIGKILL requires operator inspection, not unsafe PID reuse.
    def terminated(signum, frame):
        raise KeyboardInterrupt("Owned controller termination")
    signal.signal(signal.SIGTERM, terminated)
    try:
        run(load(args.config), root, args.with_graph)
    except BaseException as e:
        save(root / "failure.json", {"status": "unavailable", "error_type": type(e).__name__,
                                    "reason": str(e), "remaining_prerequisite": "Inspect this real run; do not call missing cells passing"})
        raise
    finally:
        save(root / "artifact-inventory.json", inventory(root))


if __name__ == "__main__":
    main()
