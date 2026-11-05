#!/usr/bin/env python3
"""Actual dedicated Memgraph/llama API research, with SIGKILLed builder processes.

No existing service is adopted. Each graph has separate owned data/log volumes.
Run only in the parent's reserved host slot. No production AM API is implemented.
"""
from __future__ import annotations
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import sys
import time
import uuid
from common import (IMAGE, LABEL, bind_sources, choose_model, command, create_run, digest,
                    embed, fsync_dir, inventory, key_from, load, lock, owned_run, request, save, sha)

PHASES = ("graph-fill", "vector-fill", "index-creation", "similarity", "pointer-before", "pointer-after")


def bolt(uri):
    from neo4j import GraphDatabase
    return GraphDatabase.driver(uri, auth=("", ""), connection_timeout=5, max_connection_pool_size=2)


def query(uri, cypher, **params):
    with bolt(uri) as driver, driver.session(database="memgraph") as s:
        return [r.data() for r in s.run(cypher, **params)]


def available_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]


class Stores:
    def __init__(self, root, engine):
        self.root, self.engine = Path(root), engine
        self.owner = owned_run(root)

    def call(self, *args, check=True, timeout=60):
        return command([self.engine, *args], check=check, timeout=timeout)

    def register(self, resource):
        self.owner = owned_run(self.root)
        self.owner["resources"].append(resource)
        save(self.root / "owner.json", self.owner)

    def inspect(self, resource):
        args = ("volume", "inspect", resource["name"]) if resource["type"] == "volume" else ("inspect", resource.get("id", resource["name"]))
        r = self.call(*args, check=False)
        if r["exit"]:
            # A failed inspect is not sufficient evidence of absence. Check authoritative listing too.
            names = self.call(*(("volume", "ls", "--format", "{{.Name}}") if resource["type"] == "volume" else ("ps", "-a", "--format", "{{.Names}}")))["stdout"].splitlines()
            if resource["name"] in names:
                raise RuntimeError("Resource exists but inspect failed")
            return None
        data = json.loads(r["stdout"])[0]
        labels = data.get("Labels") if resource["type"] == "volume" else data.get("Config", {}).get("Labels")
        if not labels or labels.get(LABEL) != self.owner["run_id"]:
            raise ValueError("Refusing foreign resource ownership mismatch")
        if resource.get("id") and resource["type"] == "container" and data["Id"] != resource["id"]:
            raise ValueError("Refusing recycled container identity")
        if resource.get("created") and str(data.get("CreatedAt", data.get("Created"))) != resource["created"]:
            raise ValueError("Refusing recycled volume identity")
        return data

    def create(self, role):
        name = "am255-rc-" + self.owner["run_id"][:12] + "-" + role + "-" + uuid.uuid4().hex[:6]
        # New globally unique names are not silently adopted, even with a matching prefix.
        for suffix in ("data", "logs"):
            volume = {"type": "volume", "name": name + "-" + suffix}
            if self.call("volume", "inspect", volume["name"], check=False)["exit"] == 0:
                raise ValueError("Volume name already exists")
            self.register(volume)
            self.call("volume", "create", "--label", LABEL + "=" + self.owner["run_id"], volume["name"])
            d = self.inspect(volume)
            volume["created"] = str(d.get("CreatedAt", d.get("Created")))
            self.owner = owned_run(self.root)
            self.owner["resources"][-1] = volume
            save(self.root / "owner.json", self.owner)
        port = available_port()
        container = {"type": "container", "name": name}
        self.register(container)
        r = self.call("run", "-d", "--pull=never", "--name", name, "--label", LABEL + "=" + self.owner["run_id"],
            "--memory=2g", "--cpus=2", "-p", "127.0.0.1:" + str(port) + ":7687",
            "-v", name + "-data:/var/lib/memgraph", "-v", name + "-logs:/var/log/memgraph", IMAGE,
            "--telemetry-enabled=false", "--storage-properties-on-edges=true", "--storage-wal-enabled=true",
            "--storage-snapshot-interval-sec=0", "--storage-snapshot-on-exit=true", "--log-level=WARNING")
        d = self.inspect(container)
        container["id"] = d["Id"]
        self.owner = owned_run(self.root)
        self.owner["resources"][-1] = container
        save(self.root / "owner.json", self.owner)
        uri = "bolt://127.0.0.1:" + str(port)
        limit = time.monotonic() + 45
        while True:
            try:
                query(uri, "RETURN 1 AS ok")
                break
            except Exception:
                if time.monotonic() > limit:
                    raise RuntimeError("Owned graph did not become available in 45 seconds")
                time.sleep(.2)
        instance = {"role": role, "name": name, "container_id": container["id"], "uri": uri,
                    "data_volume": name + "-data", "log_volume": name + "-logs", "image": d["Image"]}
        save(self.root / (name + "-instance.json"), instance)
        return instance

    def stop(self, instance):
        resource = {"type": "container", "name": instance["name"], "id": instance["container_id"]}
        if self.inspect(resource):
            self.call("stop", "-t", "10", resource["id"], timeout=20)

    def volume_inventory(self, instance):
        # Executes only the same pinned locally present image, with no network or writable evidence mount.
        code = ("import os,json,hashlib;from pathlib import Path;"
                "r=Path('/evidence');print(json.dumps([{'path':str(p.relative_to(r)),"
                "'bytes':p.stat().st_size,'sha256':hashlib.sha256(p.read_bytes()).hexdigest()} "
                "for p in sorted(r.rglob('*')) if p.is_file() and not p.is_symlink()]))")
        result = {}
        for field in ("data_volume", "log_volume"):
            volume = next(v for v in owned_run(self.root)["resources"] if v["name"] == instance[field])
            self.inspect(volume)
            name = "am255-inventory-" + uuid.uuid4().hex
            resource = {"type": "container", "name": name}
            self.register(resource)
            r = self.call("run", "--rm", "--pull=never", "--name", name, "--label", LABEL + "=" + self.owner["run_id"],
                          "--network=none", "--read-only", "--entrypoint", "/usr/bin/python3", "-v",
                          instance[field] + ":/evidence:ro", IMAGE, "-c", code, timeout=60)
            result[field] = json.loads(r["stdout"])
        return result

    def erase(self, instance):
        names = {instance["name"], instance["data_volume"], instance["log_volume"]}
        evidence = []
        resources = [r for r in owned_run(self.root)["resources"] if r["name"] in names]
        for r in sorted(resources, key=lambda x: x["type"] != "container"):
            if self.inspect(r):
                self.call(*(("rm", "-f", r.get("id", r["name"])) if r["type"] == "container" else ("volume", "rm", r["name"])))
            evidence.append({"name": r["name"], "type": r["type"], "absent": self.inspect(r) is None})
        return evidence

    def cleanup(self):
        evidence = []
        for r in sorted(owned_run(self.root)["resources"], key=lambda x: x["type"] != "container"):
            try:
                if self.inspect(r):
                    self.call(*(("rm", "-f", r.get("id", r["name"])) if r["type"] == "container" else ("volume", "rm", r["name"])))
                evidence.append({"name": r["name"], "type": r["type"], "absent": self.inspect(r) is None})
            except Exception as e:
                evidence.append({"name": r["name"], "type": r["type"], "absent": False, "reason": str(e)})
        save(self.root / "cleanup.json", {"resources": evidence, "external_backups": "outside authority",
             "status": "qualified" if all(r["absent"] for r in evidence) else "erasure-pending",
             "production_erasure_guarantee": False})
        return evidence


def fixture(project, generation, omit=False):
    bodies = ["Synthetic deleted canary AM255_ERASE_ME.", "Synthetic retry budget is three attempts.",
              "Synthetic pagination follows the next cursor.", "Synthetic compiler parses before lowering."]
    return [{"key": "record:" + str(i), "body": b, "project": project, "generation": generation,
             "source": digest(b), "scope": "component:synthetic", "path": "synthetic/record-" + str(i)}
            for i, b in enumerate(bodies) if not (omit and i == 0)]


def checkpoint(root, phase, pause):
    save(root / "checkpoint.json", {"phase": phase, "pid": os.getpid(), "durable_boundary": True})
    if pause == phase:
        # Files are a control channel between real processes, not simulated graph state.
        while not (root / "continue.json").exists():
            time.sleep(.02)


def worker(jobfile):
    job = load(jobfile)
    root, shared = Path(job["work"]), Path(job["shared"])
    rows = fixture(job["project"], job["generation"], job.get("omit", False))
    uri, phase = job["instance"]["uri"], job.get("pause")
    with bolt(uri) as driver, driver.session(database="memgraph") as s:
        with s.begin_transaction() as tx:
            tx.run("UNWIND $rows AS r CREATE (n:RecordVersion) SET n = r", rows=rows[:1]).consume()
            checkpoint(root, "graph-fill", phase)
            tx.run("UNWIND $rows AS r CREATE (n:RecordVersion) SET n = r", rows=rows[1:]).consume()
            tx.commit()
    config = load(job["endpoint_config"])
    key = key_from(config.get("key_file"))
    model, assurance = choose_model(request(config["url"], "/models", key=key), config.get("model"))
    all_vectors = []
    for i in range(0, len(rows), 2):
        values, evidence = embed(config["url"], model, [r["body"] for r in rows[i:i+2]], config["dimension"], key)
        save(root / ("batch-" + str(i) + ".json"), {"rendered_hashes": [r["source"] for r in rows[i:i+2]],
            "vectors": values, "response_sha256": evidence["response_sha256"], "usage": evidence["body"].get("usage")})
        all_vectors += values
        query(uri, "UNWIND $rows AS r MATCH (n:RecordVersion {key:r.key}) SET n:Embedded,n.embedding=r.vector",
              rows=[{"key": row["key"], "vector": v} for row, v in zip(rows[i:i+2], values)])
        if i == 0:
            checkpoint(root, "vector-fill", phase)
    query(uri, "CREATE VECTOR INDEX am255_generation ON :Embedded(embedding) WITH CONFIG "
          + json.dumps({"dimension": config["dimension"], "capacity": 32, "metric": "cos"}))
    checkpoint(root, "index-creation", phase)
    # Bounded four-record similarity control, not the 50k all-pairs strategy.
    query(uri, "MATCH (a:RecordVersion {key:$a}),(b:RecordVersion {key:$b}) "
          "CREATE (a)-[:SIMILAR_SUGGESTION {generation:$g,model:$m,source:$source}]->(b)",
          a=rows[0]["key"], b=rows[1]["key"], g=job["generation"], m=model, source=rows[0]["source"])
    checkpoint(root, "similarity", phase)
    actual = query(uri, "MATCH (n:RecordVersion) RETURN n.key AS key,n.source AS source,n.project AS project,n.generation AS generation ORDER BY key")
    expected = [{k: r[k] for k in ("key", "source", "project", "generation")} for r in rows]
    if actual != sorted(expected, key=lambda r: r["key"]):
        raise ValueError("Graph/source coverage mismatch")
    covered = query(uri, "MATCH (n:Embedded) RETURN count(n) AS count")[0]["count"]
    if covered != len(rows):
        raise ValueError("Vector coverage mismatch")
    ann = query(uri, "CALL vector_search.search('am255_generation', 8, $vector) YIELD node,similarity "
                "RETURN node.key AS key,node.project AS project,node.generation AS generation,similarity", vector=all_vectors[0])
    if not ann or any(r["project"] != job["project"] or r["generation"] != job["generation"] for r in ann):
        raise ValueError("Index domain/coverage mismatch")
    save(root / "index-query.json", ann)
    save(root / "index-info.json", query(uri, "CALL vector_search.show_index_info() YIELD * RETURN *"))
    manifest = {"schema": "am255/physical-generation-v1", "production": False, "project": job["project"],
                "generation": job["generation"], "epoch": job["epoch"], "bank_state": job["bank_state"],
                "instance": job["instance"], "records": len(rows), "vectors": covered, "complete": True,
                "model": model, "profile": digest(config), "identity_assurance": assurance,
                "source_sha256": digest(expected), "artifacts": {p.name: sha(p) for p in root.glob("batch-*.json")}}
    save(root / "manifest.json", manifest)
    pointer = {"manifest": str(root / "manifest.json"), "manifest_sha256": sha(root / "manifest.json"),
               "generation": job["generation"], "project": job["project"], "epoch": job["epoch"]}
    # Before checkpoint sits outside the lock, allowing a real revoker to win publication.
    save(root / "pointer-candidate.json", pointer)
    checkpoint(root, "pointer-before", phase)
    with lock(shared):
        state = load(shared / "authority.json")
        active = load(shared / "active.json") if (shared / "active.json").exists() else None
        if state != {"epoch": job["epoch"], "bank_state": job["bank_state"], "project": job["project"]}:
            save(root / "refusal.json", {"status": "stale", "reason": "epoch-or-source-fenced", "exit": 1})
            return 1
        if active != job.get("expected_active"):
            save(root / "refusal.json", {"status": "stale", "reason": "active-compare-and-replace-failed", "exit": 1})
            return 1
        # save fsyncs both file and directory around atomic os.replace.
        save(shared / "active.json", pointer)
        checkpoint(root, "pointer-after", phase)
    save(root / "receipt.json", {"status": "qualified", "exit": 0, "generation": job["generation"]})
    return 0


def read_active(root, expected_project=None, expected_bank_state=None):
    # Each read pins one immutable manifest; source/epoch recheck occurs before return.
    with lock(root):
        state = load(root / "authority.json")
        if not (root / "active.json").exists():
            return {"status": "unavailable", "exit": 1}
        ptr = load(root / "active.json")
        manifest_path = Path(ptr["manifest"])
        if not manifest_path.is_relative_to(root) or manifest_path.is_symlink() or sha(manifest_path) != ptr["manifest_sha256"]:
            raise ValueError("Pointer artifact mismatch")
        m = load(manifest_path)
        if not m["complete"] or ptr["generation"] != m["generation"]:
            raise ValueError("Mixed/incomplete generation")
        if m["epoch"] != state["epoch"] or m["bank_state"] != state["bank_state"]:
            return {"status": "stale", "exit": 1, "reason": "privacy-or-bank-state-fenced"}
        if expected_project is not None and m["project"] != expected_project:
            return {"status": "refused", "exit": 1, "reason": "project-mismatch"}
        if expected_bank_state is not None and m["bank_state"] != expected_bank_state:
            return {"status": "stale", "exit": 1, "reason": "checkout-bank-state-mismatch"}
        rows = query(m["instance"]["uri"], "MATCH (n:RecordVersion) RETURN n.key AS key,n.project AS project,n.generation AS generation ORDER BY key")
        if len(rows) != m["records"] or any(r["generation"] != m["generation"] or r["project"] != m["project"] for r in rows):
            raise ValueError("Pinned graph differs from manifest")
        if load(root / "authority.json") != state:
            return {"status": "stale", "exit": 1}
        return {"status": "qualified", "exit": 0, "generation": m["generation"], "records": rows}


def launch(root, instance, config, generation, pause=None, omit=False):
    work = root / ("build-" + generation)
    work.mkdir(mode=0o700)
    state = load(root / "authority.json")
    job = {**state, "shared": str(root), "work": str(work), "generation": generation,
           "endpoint_config": str(config), "instance": instance, "pause": pause, "omit": omit,
           "expected_active": load(root / "active.json") if (root / "active.json").exists() else None}
    save(work / "job.json", job)
    log = (work / "worker.log").open("xb")
    p = subprocess.Popen([sys.executable, "-B", str(Path(__file__).resolve()), "worker", "--job", str(work / "job.json")],
                         stdout=log, stderr=subprocess.STDOUT)
    log.close()
    return p, work


def wait_boundary(p, work, phase, timeout=180):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if p.poll() is not None:
            raise RuntimeError("Builder exited before boundary: " + str(work / "worker.log"))
        if (work / "checkpoint.json").exists() and load(work / "checkpoint.json")["phase"] == phase:
            return load(work / "checkpoint.json")
        time.sleep(.02)
    p.kill()
    p.wait(timeout=10)
    raise TimeoutError("Builder boundary timeout")


def finish(p, work):
    try:
        code = p.wait(timeout=240)
    except subprocess.TimeoutExpired:
        p.kill()
        p.wait(timeout=10)
        raise TimeoutError("Bounded build exceeded 240 seconds")
    if code:
        raise RuntimeError("Builder failed: " + str(work / "worker.log"))


def ssg_controls(root, stores, ssg, binary):
    instance = stores.create("ssg-reference")
    source = root / "ssg-synthetic"
    (source / "expertise").mkdir(parents=True)
    (source / "queries").mkdir()
    unit = source / "expertise/class.md"
    unit.write_text('+++\nkey = "class/synthetic"\ntype = "class"\ntitle = "Synthetic qualification class"\nstatus = "proposed"\ncwe = "CWE-20"\n+++\nSynthetic input must be checked before use.\n')
    template = source / "queries/class.toml"
    for query_file in (Path(ssg) / "queries").glob("*.toml"):
        shutil.copyfile(query_file, source / "queries" / query_file.name)
    env = {k: v for k, v in os.environ.items() if not k.startswith("SSG_")}
    env.update(SSG_HOME=str(source), SSG_BOLT=instance["uri"].removeprefix("bolt://"),
               SSG_CACHE=str(root / "ssg-private-cache"), SSG_EMBED_URL="http://127.0.0.1:1")
    def run(name, argv, expected):
        result = command([binary, *argv], timeout=90, cwd=source, env=env, check=False)
        save(root / ("ssg-" + name + ".json"), result)
        if result["exit"] != expected:
            raise ValueError("Actual SSG exit differs for " + name)
        return result
    results = {}
    results["initial_build"] = run("initial-build", ["build"], 0)
    before = query(instance["uri"], "MATCH (n) RETURN count(n) AS nodes")
    results["named_query"] = run("named-query", ["query", "class", "--param", "key=class/synthetic", "--json"], 0)
    answer = json.loads(results["named_query"]["stdout"])
    if len(answer["items"]) != 1 or answer["items"][0]["key"] != "class/synthetic" or not answer["items"][0]["digest"]:
        raise ValueError("Named SSG query omitted synthetic truth/provenance")
    after = query(instance["uri"], "MATCH (n) RETURN count(n) AS nodes")
    if before != after:
        raise ValueError("Named read changed node count")
    unit.write_text(unit.read_text() + "Synthetic revision two.\n")
    results["stale_sources"] = run("stale-source", ["query", "class", "--param", "key=class/synthetic", "--json"], 11)
    results["rebuild"] = run("rebuild", ["build", "--force"], 0)
    results["rebuilt_query"] = run("rebuilt-query", ["query", "class", "--param", "key=class/synthetic", "--json"], 0)
    template.write_text(template.read_text() + "\n# Synthetic template drift\n")
    results["template_drift"] = run("template-drift", ["query", "class", "--param", "key=class/synthetic", "--json"], 3)
    write_template = source / "queries/tool.toml"
    original_tool = write_template.read_text()
    write_template.write_text('name = "tool"\ndescription = "Synthetic rejected write"\ncypher = "CREATE (n:Forbidden) RETURN 1 AS item ORDER BY item"\n')
    results["write_plan"] = run("write-plan", ["build", "--force"], 3)
    write_template.write_text(original_tool)
    results["recovered_build"] = run("recovered-build", ["build", "--force"], 0)
    no_tools = root / "empty-executable-path"
    no_tools.mkdir()
    # Equality guard runs before docker. Empty PATH prevents incidental startup if that guard regresses.
    results["workspace_same_address_refusal"] = command([binary, "workspace", "up"], timeout=15,
        cwd=source, env={**env, "SSG_WORKSPACE_BOLT": env["SSG_BOLT"],
                        "PATH": str(no_tools), "SSG_WORKSPACE_CONTAINER": "am255-unstarted-" + stores.owner["run_id"]}, check=False)
    save(root / "ssg-workspace-controls.json", results["workspace_same_address_refusal"])
    if results["workspace_same_address_refusal"]["exit"] != 3:
        raise ValueError("SSG workspace did not refuse shared address")
    stores.stop(instance)
    inv = stores.volume_inventory(instance)
    save(root / "ssg-store-inventory.json", inv)
    erased = stores.erase(instance)
    return {"evidence_class": "actual-SSG-binary-and-real-Memgraph", "binary_sha256": sha(binary),
            "synthetic_source_inventory": inventory(source), "named_read_node_counts_equal": before == after,
            "read_only_acl_proven": False, "erasure": erased, "cells": {k: {"exit": v["exit"]} for k,v in results.items()}}


def run(args):
    root = create_run(args.output, "graph-process-faults")
    sources = bind_sources(args.ssg, root / "source-bindings.json")
    stores = Stores(root, args.engine)
    save(root / "engine-image.json", stores.call("image", "inspect", IMAGE))
    # Existing host resources are observed, never modified, and exact IDs are compared after cleanup.
    before = stores.call("ps", "-a", "--format", "{{.ID}} {{.Names}}")
    save(root / "unrelated-before.json", before)
    report = {"schema": "am255/actual-graph-fault-controls-v1", "production_api_guarantee": False,
              "fault_scope": "SIGKILL actual builder at durable real-graph/API/file boundaries, not power-loss or in-engine DDL crash proof",
              "sources": sources, "cells": []}
    live = []
    try:
        report["ssg"] = ssg_controls(root, stores, args.ssg, args.ssg_binary)
        save(root / "authority.json", {"project": "synthetic-project-A", "epoch": 1, "bank_state": "synthetic-checkout-A-state-1"})
        active = stores.create("active")
        p, work = launch(root, active, args.endpoint_config, "initial")
        live.append(p)
        finish(p, work)
        initial_pointer = load(root / "active.json")
        initial_read = read_active(root)
        for phase in PHASES:
            candidate = stores.create("candidate")
            p, work = launch(root, candidate, args.endpoint_config, phase, pause=phase)
            live.append(p)
            boundary = wait_boundary(p, work, phase)
            p.kill()
            code = p.wait(timeout=10)
            observed = read_active(root)
            pointer = load(root / "active.json")
            expected_generation = phase if phase == "pointer-after" else "initial"
            if observed["status"] != "qualified" or observed["generation"] != expected_generation or code != -signal.SIGKILL:
                raise ValueError("Interrupted candidate exposed incomplete/mixed generation")
            if phase != "pointer-after" and pointer != initial_pointer:
                raise ValueError("Incomplete candidate displaced active pointer")
            if phase == "pointer-after":
                # This complete candidate becomes active; keep it for the later fence study.
                previous = active
                active = candidate
                stores.erase(previous)
            else:
                if phase == "graph-fill" and query(candidate["uri"], "MATCH (n) RETURN count(n) AS n")[0]["n"] != 0:
                    raise ValueError("Killed transaction did not roll back")
                if phase == "vector-fill":
                    # A real database-process crash/restart of an INACTIVE partial candidate.
                    # This is not a claim of interruption inside the vector-index engine.
                    stores.inspect({"type": "container", "name": candidate["name"], "id": candidate["container_id"]})
                    stores.call("kill", "--signal", "KILL", candidate["container_id"])
                    stores.call("start", candidate["container_id"])
                    deadline = time.monotonic() + 45
                    while True:
                        try:
                            recovered = query(candidate["uri"], "MATCH (n) RETURN count(n) AS nodes")
                            break
                        except Exception:
                            if time.monotonic() > deadline:
                                raise RuntimeError("Killed candidate did not recover within 45 seconds")
                            time.sleep(.2)
                    still_active = read_active(root)
                    if still_active["status"] != "qualified" or still_active["generation"] != "initial":
                        raise ValueError("Candidate database crash displaced qualified active graph")
                    report["candidate_database_crash"] = {"signal": "SIGKILL", "container": candidate["container_id"],
                        "recovered_partial_graph": recovered, "active_outcome": still_active,
                        "scope": "real candidate-process restart after committed partial vector fill; not power-loss durability"}
                stores.erase(candidate)
            report["cells"].append({"phase": phase, "boundary": boundary, "builder_exit": code,
                                     "public_outcome": observed, "pointer_sha256": digest(pointer)})
        # Actual rebuild into yet another physical store, compare-and-replace against the active pin.
        rebuilt = stores.create("rebuild")
        p, work = launch(root, rebuilt, args.endpoint_config, "rebuilt")
        live.append(p)
        finish(p, work)
        report["rebuild"] = read_active(root)
        stores.erase(active)
        active = rebuilt
        report["wrong_project"] = read_active(root, expected_project="synthetic-project-B")
        report["wrong_checkout"] = read_active(root, expected_bank_state="synthetic-checkout-B-state-1")
        if report["wrong_project"]["status"] != "refused" or report["wrong_checkout"]["status"] != "stale":
            raise ValueError("Project/checkout binding did not refuse")
        foreign = stores.create("foreign-project")
        query(foreign["uri"], "CREATE (:RecordVersion {key:'foreign-canary',project:'synthetic-project-B',generation:'foreign'})")
        if any(r["key"] == "foreign-canary" for r in read_active(root)["records"]):
            raise ValueError("Foreign physical instance leaked into active query")
        report["physical_isolation"] = {"active_container": active["container_id"], "foreign_container": foreign["container_id"],
                                         "active_data": active["data_volume"], "foreign_data": foreign["data_volume"],
                                         "foreign_canary_absent_from_active": True}
        stores.erase(foreign)
        fenced = stores.create("stale-builder")
        p, work = launch(root, fenced, args.endpoint_config, "stale", pause="pointer-before")
        live.append(p)
        wait_boundary(p, work, "pointer-before")
        pointer_before = load(root / "active.json")
        with lock(root):
            save(root / "authority.json", {"project": "synthetic-project-A", "epoch": 2, "bank_state": "synthetic-checkout-A-deleted-revoked"})
        save(work / "continue.json", {"resume": True})
        code = p.wait(timeout=30)
        fenced_read = read_active(root)
        if code != 1 or load(root / "active.json") != pointer_before or fenced_read["status"] != "stale":
            raise ValueError("Stale builder/reader was not fenced")
        report["deleted_revoked_epoch"] = {"builder_exit": code, "builder_refusal": load(work / "refusal.json"),
                                            "old_reader_outcome": fenced_read, "pointer_unchanged": True}
        # Generate real snapshots/WAL before disposal. SQL delete alone is deliberately not erasure.
        erasures = []
        for instance in (active, fenced):
            snap = query(instance["uri"], "CREATE SNAPSHOT")
            save(root / (instance["name"] + "-snapshot-response.json"), snap)
            stores.stop(instance)
            inv = stores.volume_inventory(instance)
            save(root / (instance["name"] + "-pre-erasure.json"), inv)
            data_files = inv["data_volume"]
            if not any("snapshot" in x["path"].lower() for x in data_files):
                raise ValueError("No actual snapshot files observed; erasure cell unqualified")
            if not any("wal" in x["path"].lower() for x in data_files):
                raise ValueError("No actual WAL files observed; erasure cell unqualified")
            erasures.append({"instance": instance["name"], "before": inv, "after": stores.erase(instance)})
        report["owned_store_erasure"] = {"cells": erasures, "whole_volume_disposal": True,
            "forensic_secure_erase": False, "external_backups_and_provider_retention": "outside authority",
            "canonical_deletion": "synthetic source state only; not production canonical deletion API"}
        removed = []
        for build in sorted(root.glob("build-*")):
            if build.is_symlink() or not build.is_dir():
                raise ValueError("Unknown builder artifact directory; erasure pending")
            for artifact in sorted([*build.glob("batch-*.json"), *build.glob("worker.log")]):
                if artifact.is_symlink() or not artifact.is_file() or artifact.stat().st_uid != os.getuid():
                    raise ValueError("Unknown vector/log artifact owner; erasure pending")
                removed.append({"path": str(artifact.relative_to(root)), "sha256_before": sha(artifact)})
                artifact.unlink()
            fsync_dir(build)
        if any(root.glob("build-*/batch-*.json")):
            raise ValueError("Owned old vector artifacts remain; erasure pending")
        report["owned_store_erasure"]["local_vectors_and_builder_logs"] = {"removed": removed, "remaining_old_vector_files": 0}
        save(root / "vector-erasure.json", report["owned_store_erasure"]["local_vectors_and_builder_logs"])
        permitted = stores.create("post-delete")
        p, work = launch(root, permitted, args.endpoint_config, "post-delete", omit=True)
        live.append(p)
        finish(p, work)
        report["post_delete_rebuild"] = read_active(root)
        absent = query(permitted["uri"], "MATCH (n:RecordVersion) WHERE n.body CONTAINS $canary RETURN count(n) AS count", canary="AM255_ERASE_ME")
        if absent[0]["count"] != 0:
            raise ValueError("Deleted synthetic canary resurrected")
        report["post_delete_rebuild"]["deleted_canary_count"] = 0
        # Independent consent-only change: unchanged canonical bank-state, higher privacy epoch.
        revoked = stores.create("revoked-builder")
        p, work = launch(root, revoked, args.endpoint_config, "consent-revoked", pause="pointer-before", omit=True)
        live.append(p)
        wait_boundary(p, work, "pointer-before")
        old_authority = load(root / "authority.json")
        with lock(root):
            save(root / "authority.json", {**old_authority, "epoch": old_authority["epoch"] + 1})
        save(work / "continue.json", {"resume": True})
        revoke_exit = p.wait(timeout=30)
        revoked_read = read_active(root)
        if revoke_exit != 1 or revoked_read["status"] != "stale":
            raise ValueError("Consent-only epoch did not fence unchanged-source readers/builders")
        revoke_erasures = []
        for instance in (permitted, revoked):
            query(instance["uri"], "CREATE SNAPSHOT")
            stores.stop(instance)
            before_erasure = stores.volume_inventory(instance)
            save(root / (instance["name"] + "-revocation-pre-erasure.json"), before_erasure)
            revoke_erasures.append({"instance": instance["name"], "before": before_erasure, "after": stores.erase(instance)})
        revoked_artifacts = []
        for build in (root / "build-post-delete", work):
            for artifact in [*build.glob("batch-*.json"), *build.glob("worker.log")]:
                if artifact.is_symlink() or artifact.stat().st_uid != os.getuid():
                    raise ValueError("Revoked vector artifact ownership mismatch; erasure pending")
                revoked_artifacts.append({"path": str(artifact.relative_to(root)), "sha256_before": sha(artifact)})
                artifact.unlink()
            fsync_dir(build)
        with lock(root):
            (root / "active.json").unlink()
            fsync_dir(root)
        report["consent_only_revocation"] = {"canonical_bank_state_unchanged": True, "builder_exit": revoke_exit,
            "builder_refusal": load(work / "refusal.json"), "old_reader_outcome": revoked_read,
            "post_erasure_public_outcome": read_active(root), "store_erasure": revoke_erasures,
            "vector_and_log_artifacts_removed": revoked_artifacts, "further_embedding_disclosure_after_revoke": False}
        save(root / "report.json", report)
    except Exception as e:
        save(root / "failure.json", {"status": "unavailable", "exit": 1, "type": type(e).__name__, "reason": str(e)})
        raise
    finally:
        for p in live:
            if p.poll() is None:
                p.kill()
                p.wait(timeout=10)
        cleanup_result = stores.cleanup()
        remaining = stores.call("ps", "-a", "--format", "{{.ID}} {{.Names}}")
        save(root / "unrelated-after.json", remaining)
        preserved = set(before["stdout"].splitlines()).issubset(set(remaining["stdout"].splitlines()))
        save(root / "unrelated-preservation.json", {"all_original_container_ids_preserved": preserved,
             "note": "Other owners may legitimately stop containers; this controller never targets their IDs"})
        save(root / "artifact-inventory.json", inventory(root))
        if not all(r["absent"] for r in cleanup_result):
            raise RuntimeError("Owned cleanup incomplete; see cleanup.json erasure-pending resource IDs")


def main():
    p = argparse.ArgumentParser(description=__doc__)
    sub = p.add_subparsers(dest="action", required=True)
    r = sub.add_parser("run")
    r.add_argument("--output", type=Path, required=True)
    r.add_argument("--engine", choices=("podman", "docker"), required=True)
    r.add_argument("--ssg", type=Path, required=True)
    r.add_argument("--ssg-binary", type=Path, required=True)
    r.add_argument("--endpoint-config", type=Path, required=True)
    r.add_argument("--reserved-host-slot", action="store_true", required=True)
    w = sub.add_parser("worker")
    w.add_argument("--job", type=Path, required=True)
    c = sub.add_parser("cleanup")
    c.add_argument("--output", type=Path, required=True)
    c.add_argument("--engine", choices=("podman", "docker"), required=True)
    args = p.parse_args()
    if args.action == "worker":
        sys.exit(worker(args.job))
    elif args.action == "cleanup":
        result = Stores(args.output, args.engine).cleanup()
        sys.exit(0 if all(r["absent"] for r in result) else 1)
    else:
        args.ssg_binary = args.ssg_binary.resolve()
        args.endpoint_config = args.endpoint_config.resolve()
        run(args)


if __name__ == "__main__":
    main()
