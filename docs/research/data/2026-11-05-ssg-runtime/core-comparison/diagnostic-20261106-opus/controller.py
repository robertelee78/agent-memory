#!/usr/bin/env python3
"""Synthetic-only #255 core/MAGE comparison. No pulls, builds or profile adoption.

Use the existing qualification-venv Python (neo4j 5.x), for example:
  /tmp/am-v15-delivery/qualification-venv/bin/python compare.py \
      --authorized-shared-host --output actual-20261106 --ssg /opt/ssg
A new output root is required. One new graph at a time; exact owned cleanup runs
on failure too. Historical evidence is never overwritten. Timings include shared
host/Podman VM/client costs and are NOT clean timing or G1 acceptance.
"""
from __future__ import annotations
import argparse
import datetime
import hashlib
import importlib.metadata
import json
import math
from pathlib import Path
import platform
import re
import socket
import subprocess
import sys
import time
import tomllib

HERE = Path(__file__).resolve().parent
REPO = HERE.parents[4]
sys.path.insert(0, str(HERE.parent / "runtime-controls"))
from common import create_run, inventory, load, owned_run, save, sha
from neo4j import GraphDatabase, Query

BASE = "ba0087a129fed6dcda03ff4aa25060289531014a"
SSG = "9c4fd08c29f37bffe89ecdf75b069a6baeb8a5e0"
LABEL = "org.agent-memory.core-comparison"
IMAGES = {
    "core": {
        "ref": "docker.io/memgraph/memgraph@sha256:042c8642954a7bde971a9c7683c8b1360e7fcf129a5fae36d63f12449e7154aa",
        "child": "sha256:334cd2a524cdb05bb68310c8338c91de8d44d2998a7aa0349a6de9674824846d",
        "config": "3792294abf9a035895d8db16cbc56146d48a9fa1d379be6581631fdeae525c4f",
    },
    "mage": {
        "ref": "docker.io/memgraph/memgraph-mage@sha256:e8e03640071c57de6a13f2f4c5f9ed1789797dc5e8e771bbe62203b20e1b8c27",
        "child": "sha256:fe2f37e45e263225129bcb6955612ac9956dea0e8db614f9c39eb74b105c6c39",
        "config": "44b5244a3622425584cae52b0cdf1ea1690ebb98e27110b3adfd1af7ab8227a9",
    },
}
FLAGS = ["--telemetry-enabled=false", "--storage-properties-on-edges=true",
         "--storage-wal-enabled=true", "--storage-snapshot-interval-sec=300",
         "--storage-snapshot-on-exit=true", "--log-level=WARNING", "--also-log-to-stderr=true"]
VECTOR_CONTROLS = {
    "index_capacity": 2048,  # SSG: next_power_of_two(max(needed, 1000) * 2).
    "ann_k": 12,
    "nonunit_query_vector": [2.0, 0.0, 0.0, 0.0],
    "scope": "Exhaustive seven-vector diagnostic, not eligible ANN recall or production AM exact cosine.",
}


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()


class Run:
    def __init__(self, root):
        self.root = root
        self.owner = owned_run(root)
        self.sequence = 0
        (root / "commands").mkdir()

    def command(self, name, argv, *, stdin=None, binary=False, check=True, timeout=120):
        self.sequence += 1
        stem = f"{self.sequence:03d}-{name}"
        start = time.monotonic()
        record = {"argv": [str(a) for a in argv], "cwd": str(REPO),
                  "started_at": datetime.datetime.now(datetime.timezone.utc).isoformat()}
        try:
            p = subprocess.run(record["argv"], cwd=REPO, input=stdin, capture_output=True, timeout=timeout)
            record.update(exit=p.returncode, stderr=p.stderr.decode("utf-8", "replace"))
            if binary:
                artifact = self.root / (stem + ".tar")
                artifact.write_bytes(p.stdout)
                record.update(stdout_artifact=artifact.name, stdout_bytes=len(p.stdout), stdout_sha256=sha(artifact))
            else:
                record["stdout"] = p.stdout.decode("utf-8", "replace")
        except subprocess.TimeoutExpired as e:
            record.update(exit=None, timeout_seconds=timeout,
                          stdout=(e.stdout or b"").decode("utf-8", "replace"),
                          stderr=(e.stderr or b"").decode("utf-8", "replace"))
        record["seconds"] = time.monotonic() - start
        if stdin is not None:
            record["stdin_sha256"] = hashlib.sha256(stdin).hexdigest()
        save(self.root / "commands" / (stem + ".json"), record)
        if check and record["exit"] != 0:
            raise RuntimeError("Command failed; retained " + stem)
        return record

    def podman(self, name, *args, **kwargs):
        return self.command(name, ["podman", *args], **kwargs)

    def register(self, resource):
        self.owner["resources"].append(resource)
        save(self.root / "owner.json", self.owner)

    def inspect_owned(self, resource):
        args = ("volume", "inspect", resource["name"]) if resource["type"] == "volume" else ("inspect", resource.get("id", resource["name"]))
        record = self.podman("owned-inspect", *args, check=False)
        if record["exit"]:
            args = ("volume", "ls", "--format", "{{.Name}}") if resource["type"] == "volume" else ("ps", "-a", "--format", "{{.Names}}")
            names = self.podman("absence-list", *args)["stdout"].splitlines()
            if resource["name"] in names:
                raise RuntimeError("Inspect failed for existing resource; refusing cleanup")
            return None
        data = json.loads(record["stdout"])[0]
        labels = data.get("Labels") if resource["type"] == "volume" else data["Config"].get("Labels")
        if not labels or labels.get(LABEL) != self.owner["run_id"]:
            raise RuntimeError("Refusing foreign resource")
        if resource.get("id") and resource["id"] != data["Id"]:
            raise RuntimeError("Container ID changed")
        created = str(data.get("CreatedAt", data.get("Created")))
        if resource.get("created") and resource["created"] != created:
            raise RuntimeError("Volume creation identity changed")
        return data

    def cleanup(self):
        rows = []
        for resource in sorted(self.owner["resources"], key=lambda r: r["type"] != "container"):
            try:
                if self.inspect_owned(resource):
                    if resource["type"] == "container":
                        cid = resource.get("id", resource["name"])
                        self.podman("final-logs", "logs", cid, check=False)
                        self.podman("stop", "stop", "-t", "10", cid, check=False, timeout=30)
                        self.podman("remove-container", "rm", cid)
                    else:
                        self.podman("remove-volume", "volume", "rm", resource["name"])
                rows.append({**resource, "absent": self.inspect_owned(resource) is None})
            except Exception as e:
                rows.append({**resource, "absent": False, "error": str(e)})
        save(self.root / "cleanup.json", rows)
        return rows

    def graph(self, role):
        pin = IMAGES[role]
        image = json.loads(self.podman(role + "-image", "image", "inspect", pin["ref"])["stdout"])[0]
        if image["Id"].removeprefix("sha256:") != pin["config"] or not any(r.endswith("@" + pin["child"]) for r in image["RepoDigests"]):
            raise ValueError("Pinned child/config mismatch")
        if image["Architecture"] != "arm64" or image["Config"]["User"] != "memgraph":
            raise ValueError("Unexpected platform or service user")
        name = "am255-compare-" + self.owner["run_id"][:12] + "-" + role
        for suffix in ("data", "logs"):
            resource = {"type": "volume", "name": name + "-" + suffix}
            names = self.podman("volume-preexistence", "volume", "ls", "--format", "{{.Name}} ")["stdout"].split()
            if resource["name"] in names:
                raise ValueError("Volume already exists")
            self.register(resource)
            self.podman("create-volume", "volume", "create", "--label", LABEL + "=" + self.owner["run_id"], resource["name"])
            data = self.inspect_owned(resource)
            resource["created"] = str(data.get("CreatedAt", data.get("Created")))
            save(self.root / "owner.json", self.owner)
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        resource = {"type": "container", "name": name}
        self.register(resource)
        start = time.monotonic()
        self.podman(role + "-start", "run", "-d", "--pull=never", "--image-volume=ignore",
                    "--name", name, "--label", LABEL + "=" + self.owner["run_id"],
                    "--memory=2g", "--cpus=2", "-p", f"127.0.0.1:{port}:7687",
                    "-v", name + "-data:/var/lib/memgraph", "-v", name + "-logs:/var/log/memgraph", pin["ref"], *FLAGS)
        data = self.inspect_owned(resource)
        resource["id"] = data["Id"]
        save(self.root / "owner.json", self.owner)
        mounts = data["Mounts"]
        if {m["Destination"] for m in mounts} != {"/var/lib/memgraph", "/var/log/memgraph"}:
            raise ValueError("Unexpected/anonymous mount; refusing to proceed")
        if {m.get("Name") for m in mounts} != {name + "-data", name + "-logs"}:
            raise ValueError("Mount identity mismatch")
        if data["Config"]["User"] != "memgraph" or data["Image"].removeprefix("sha256:") != pin["config"]:
            raise ValueError("Container/image/user binding mismatch")
        driver = GraphDatabase.driver(f"bolt://127.0.0.1:{port}", auth=("", ""), connection_timeout=2,
                                     max_connection_pool_size=1, max_transaction_retry_time=0)
        attempts = []
        try:
            while True:
                try:
                    with driver.session(database="memgraph") as session:
                        session.run("RETURN 1 AS ok").consume()
                    break
                except Exception as e:
                    attempts.append({"after_seconds": time.monotonic() - start, "error": str(e)})
                    state = self.inspect_owned(resource)
                    if not state["State"]["Running"] or time.monotonic() - start > 60:
                        self.podman(role + "-startup-logs", "logs", resource["id"], check=False)
                        raise RuntimeError("Graph unavailable; startup evidence retained")
                    time.sleep(.2)
        except BaseException:
            driver.close()
            raise
        return resource, driver, {"image": image, "container": data, "startup_seconds": time.monotonic() - start,
                                  "startup_attempts": attempts, "uri": f"bolt://127.0.0.1:{port}"}


def exercise(run, role, driver, fixture, ssg):
    records = []
    def query(name, text, params=None, read_bound=False):
        start = time.monotonic()
        record = {"name": name, "cypher": text, "params": params or {}, "timeout_seconds": .25 if read_bound else 10}
        try:
            with driver.session(database="memgraph") as session:
                result = session.run(Query(text, timeout=record["timeout_seconds"]), params or {})
                record["rows"] = [r.data() for r in result]
                result.consume()
            record["success"] = True
        except Exception as e:
            record.update(success=False, error=str(e))
            raise
        finally:
            record["seconds"] = time.monotonic() - start
            records.append(record)
            save(run.root / (role + "-queries.json"), records)
        return record["rows"]

    query("procedures", "CALL mg.procedures() YIELD * RETURN *")
    query("functions", "CALL mg.functions() YIELD * RETURN *")
    # Labels/types come only from frozen local fixture, never user/model text.
    for node in fixture["nodes"]:
        if any(not re.fullmatch("[A-Za-z][A-Za-z0-9_]*", x) for x in node["labels"]):
            raise ValueError("Invalid fixture label")
        props = {**node["properties"], "project_id": fixture["project_id"], "generation_id": fixture["generation_id"],
                 "source": "synthetic/fixture.json", "digest": digest(node)}
        query("create-node", "CREATE (n:" + ":".join(node["labels"]) + ") SET n = $props", {"props": props})
    for a, relation, b in fixture["edges"]:
        if not re.fullmatch("[A-Z_]+", relation):
            raise ValueError("Invalid fixture relation")
        props = {"asserting_source": "synthetic/fixture.json", "asserting_hash": digest([a, relation, b]),
                 "endpoint_a": a, "endpoint_b": b, "generation_id": fixture["generation_id"],
                 "project_id": fixture["project_id"], "condition": "condition:synthetic", "origin": "origin:synthetic",
                 "channel": "inferred" if relation == "SIMILAR_SUGGESTION" else "recorded", "when": "synthetic only", "params": "{}"}
        query("create-edge", "MATCH (a {key:$a}),(b {key:$b}) CREATE (a)-[r:" + relation + "]->(b) SET r=$props", {"a": a, "b": b, "props": props})
    query("graph-coverage", "MATCH (n) RETURN count(n) AS nodes")
    query("edge-coverage", "MATCH ()-[r]->() RETURN count(r) AS edges")
    query("create-vector-index", "CREATE VECTOR INDEX ssg_embedding ON :Embedded(embedding) WITH CONFIG "
          + json.dumps({"dimension": 4, "capacity": VECTOR_CONTROLS["index_capacity"], "metric": "cos"}))
    query("indexinfo", "CALL vector_search.show_index_info() YIELD * RETURN *")
    controls = {}
    for name, vector in (("unit", fixture["query_vector"]), ("nonunit", VECTOR_CONTROLS["nonunit_query_vector"])):
        suffix = "" if name == "unit" else "-nonunit"
        # No ORDER BY: retain engine arrival order and both returned score columns.
        ann = query("vector-search" + suffix, "CALL vector_search.search('ssg_embedding', $k, $v) YIELD node,similarity,distance RETURN node.key AS key,similarity,distance",
                    {"v": vector, "k": VECTOR_CONTROLS["ann_k"]})
        exact = query("ssg-exact-cosine" + suffix, "MATCH (n:Embedded) RETURN n.key AS key,vector_search.cosine_similarity(n.embedding,$v) AS similarity ORDER BY similarity DESC,key", {"v": vector})
        query_norm = math.sqrt(math.fsum(x*x for x in vector))
        host = sorted([{"key": n["properties"]["key"], "similarity":
                        math.fsum(a*b for a,b in zip(n["properties"]["embedding"], vector)) /
                        (math.sqrt(math.fsum(x*x for x in n["properties"]["embedding"])) * query_norm)}
                       for n in fixture["nodes"] if "embedding" in n["properties"]], key=lambda x: (-x["similarity"], x["key"]))
        if (len(ann) != len(host) or {r["key"] for r in ann} != {r["key"] for r in host}
                or len(exact) != len(host) or [r["key"] for r in exact] != [r["key"] for r in host]):
            raise ValueError("Vector coverage or exact rank differs from frozen control")
        if any(abs(a["similarity"] - b["similarity"]) > 1e-5 for a,b in zip(exact,host)):
            raise ValueError("Controller cosine control mismatch")
        controls[name] = {
            "query_vector": vector, "query_norm": query_norm,
            "controller_exact_cosine": host,
            "raw_engine_order": [row["key"] for row in ann],
            "ann_exact_score_deviations": [
                {"key": row["key"], "ann_similarity": row["similarity"], "ann_distance": row["distance"],
                 "controller_exact_similarity": next(r["similarity"] for r in host if r["key"] == row["key"])}
                for row in ann if abs(row["similarity"] - next(r["similarity"] for r in host if r["key"] == row["key"])) > 1e-5],
        }
    typed = {
        "replacement-evidence": ("MATCH p=(n:RecordVersion {key:$seed})-[:CORRECTS|SUPERSEDES|SUPPORTED_BY*1..4]-(x) RETURN x.key AS key,[r IN relationships(p)|{type:type(r),source:r.asserting_source,hash:r.asserting_hash,channel:r.channel}] AS provenance,[r IN relationships(p)|type(r)] AS types ORDER BY key,types LIMIT 64", {"seed": "record:old:1"}),
        "component-attempts": ("MATCH (s:Scope {key:$scope})<-[:IN_SCOPE]-(n:RecordVersion) OPTIONAL MATCH (n)-[:SUPPORTED_BY|ATTRIBUTED_TO|DERIVED_FROM]->(x) RETURN n.key AS key,collect(x.key) AS context ORDER BY key LIMIT 64", {"scope": "scope:parser"}),
        "recorded-related": ("MATCH p=(n:RecordVersion {key:$seed})-[r*1..2]-(x) WHERE all(e IN r WHERE type(e) IN $types AND e.channel='recorded') RETURN DISTINCT x.key AS key,[e IN r|type(e)] AS types ORDER BY key,types LIMIT 32", {"seed": "record:new:2", "types": ["CORRECTS", "SUPERSEDES", "SUPPORTED_BY", "DERIVED_FROM", "IN_SCOPE"]}),
    }
    for name,(text,params) in typed.items():
        query("explain-" + name, "EXPLAIN " + text, params, read_bound=True)
        rows = query(name, text, params, read_bound=True)
        if not rows:
            raise ValueError("Empty typed traversal")
    for name in ("search", "nearby", "class", "investigation"):
        source = ssg / "queries" / (name + ".toml")
        text = tomllib.loads(source.read_text())["cypher"]
        params = {"text": fixture["query_vector"]} if name == "search" else {"text": fixture["query_vector"], "packages": [{"key":"cargo/synthetic", "vkey":1}], "package_keys":["cargo/synthetic"], "framework_keys": []} if name == "nearby" else {"key": name + "/synthetic"}
        query("explain-ssg-" + name, "EXPLAIN " + text, params, read_bound=True)
        rows = query("ssg-template-" + name, text, params, read_bound=True)
        if not rows:
            raise ValueError("Empty SSG template control")
    search = tomllib.loads((ssg / "queries/search.toml").read_text())["cypher"]
    query("ssg-template-search-nonunit", search, {"text": VECTOR_CONTROLS["nonunit_query_vector"]}, read_bound=True)
    return {"records": records, "controller_reference": "Controller Python signed cosine, normalizing both query and stored vector; not AM's normalized-float32 vector-store implementation.",
            "vector_controls": controls, "ann_exact_score_deviations": controls["unit"]["ann_exact_score_deviations"],
            "fixture_nodes": len(fixture["nodes"]), "fixture_edges": len(fixture["edges"])}


def summarize(probe):
    modules = [r for r in probe["memgraph_lib"] if "/query_modules/" in r["path"]]
    owned = set(probe["non_distro_package_files"]["memgraph"]["listed_query_modules"])
    installed = [r for r in probe["dpkg"] if r["Status"] == "install ok installed"]
    return {"modules": len(modules), "deb_owned_modules": sum(r["path"] in owned for r in modules),
            "outside_deb_modules": sum(r["path"] not in owned for r in modules),
            "python_distributions": len(probe["python_dists"]), "installed_packages": len(installed),
            "missing_package_copyrights": [{"package": r["Package"], "version": r["Version"]} for r in installed if not r["copyright"]],
            "memgraph_binary": next(r for r in probe["memgraph_lib"] if r["path"] == "/usr/lib/memgraph/memgraph"),
            "vector_search_binary": next(r for r in modules if r["path"].endswith("/vector_search.so")),
            "memgraph_license_files": [r for r in probe["share_doc_files"] if r["path"].startswith("/usr/share/doc/memgraph/")],
            "tree_totals": probe["tree_totals"]}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--authorized-shared-host", action="store_true", required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--ssg", type=Path, required=True)
    args = parser.parse_args()
    root = create_run(args.output, "core-mage-comparison")
    run = Run(root)
    report = {"schema": "am255/core-mage-comparison-v1", "production": False, "g1_accepted": False,
              "profile_cutover": False, "limits": {"new_graphs_at_once":1,"cpus":2,"memory_bytes":2147483648,
              "network":"only Bolt host publication is loopback; default container networking, not an egress-isolation proof; telemetry disabled", "host":"shared ordinary load, Podman Linux VM; not a clean timing slot",
              "timings":"observations including host/VM/client and instrumentation overhead, no performance pass"}, "roles": {}}
    before = None
    try:
        source = run.command("am-base", ["git", "rev-parse", "HEAD"])["stdout"].strip()
        run.command("am-baseline-ancestry", ["git", "merge-base", "--is-ancestor", BASE, source])
        if run.command("ssg-base", ["git", "-C", str(args.ssg), "rev-parse", "HEAD"])["stdout"].strip() != SSG:
            raise ValueError("SSG revision mismatch")
        probe_path = REPO / "data/2026-11-05-ssg-runtime/licenses/image_probe.py"
        fixture = load(HERE / "fixture.json")
        (root / "controller.py").write_bytes(Path(__file__).read_bytes())
        (root / "image-probe.py").write_bytes(probe_path.read_bytes())
        save(root / "frozen-inputs.json", {"fixture":fixture,"fixture_sha256":sha(HERE / "fixture.json"),
             "images":IMAGES,"flags":FLAGS,"am_baseline":BASE,"am_source_commit":source,"ssg_commit":SSG,
             "controller_sha256":sha(__file__),"probe_sha256":sha(probe_path),
             "common_sha256":sha(HERE.parent / "runtime-controls/common.py"),
             "am_template_contract_sha256":sha(REPO / "docs/design/v0.0.16-graph-embedding-execution.md"),
             "ssg_templates":{n:sha(args.ssg / "queries" / (n + ".toml")) for n in ("search","nearby","class","investigation")},
             "vector_controls":VECTOR_CONTROLS,"ssg_index_source_sha256":sha(args.ssg / "crates/ssg/src/embed.rs"),
             "python":sys.executable,"python_version":platform.python_version(),"neo4j_version":importlib.metadata.version("neo4j"),
             "clone":str(REPO),"platform":platform.platform()})
        run.podman("version", "version")
        run.podman("volume-policy-help", "run", "--help")
        run.command("host-load-before", ["uptime"])
        before = json.loads(run.podman("containers-before", "ps", "-a", "--no-trunc", "--format", "json")["stdout"])
        volumes_before = set(run.podman("volumes-before", "volume", "ls", "--format", "{{.Name}}")["stdout"].splitlines())
        for role in ("core", "mage"):
            resource, driver, result = run.graph(role)
            report["roles"][role] = result
            try:
                result["rss_before"] = run.podman(role + "-rss-before", "exec", resource["id"], "cat", "/proc/1/status")
                result["uid"] = run.podman(role + "-uid", "exec", resource["id"], "id")
                if "uid=0(" in result["uid"]["stdout"]:
                    raise ValueError("Root service refused")
                result["queries"] = exercise(run, role, driver, fixture, args.ssg)
                result["rss_after_queries"] = run.podman(role + "-rss-after", "exec", resource["id"], "cat", "/proc/1/status")
                result["stats"] = run.podman(role + "-stats", "stats", "--no-stream", "--format", "json", resource["id"])
                record = run.podman(role + "-inventory", "exec", "-i", resource["id"], "/usr/bin/python3", "-", stdin=probe_path.read_bytes())
                probe = json.loads(record["stdout"])
                save(root / (role + "-bom.json"), probe)
                result["bom_summary"] = summarize(probe)
                result["legal_text_archive"] = run.podman(role + "-legal-texts", "exec", resource["id"], "tar", "-C", "/usr/share/doc/memgraph", "-cf", "-", ".", binary=True)
                result["dpkg_verify"] = run.podman(role + "-dpkg-verify", "exec", resource["id"], "dpkg", "--verify", "memgraph", "openssl", "libssl3t64", check=False)
                result["vector_module_source"] = run.podman(role + "-vector-source", "exec", resource["id"], "cat", "/usr/lib/memgraph/query_modules/src/vector_search_module.cpp")
                result["success"] = True
            finally:
                driver.close()
                cleanup = run.cleanup()
                if not all(r["absent"] for r in cleanup):
                    raise RuntimeError("Owned cleanup incomplete; no next graph permitted")
            save(root / "report.json", report)
        a,b = (report["roles"][r] for r in ("core","mage"))
        selected_names = ("replacement-evidence","component-attempts","recorded-related","ssg-exact-cosine","vector-search","ssg-template-search","ssg-template-nearby","ssg-template-class","ssg-template-investigation")
        # Canonicalize only the parity comparison's ANN rowset, never raw evidence.
        # Tied engine arrival order is reported separately and is not a pass gate.
        select = lambda r: {x["name"]: sorted(x["rows"], key=lambda row: row["key"]) if x["name"] == "vector-search" else x["rows"]
                            for x in r["queries"]["records"] if x["name"] in selected_names}
        if select(a) != select(b):
            raise ValueError("Core/MAGE query results differ; inspect raw evidence")
        report["comparison"] = {"identical_selected_query_outputs":True,"selected_query_names":sorted(select(a)),
            "image_bytes":{r:report["roles"][r]["image"]["Size"] for r in IMAGES},
            "binary_equal":a["bom_summary"]["memgraph_binary"]["sha256"] == b["bom_summary"]["memgraph_binary"]["sha256"],
            "vector_search_equal":a["bom_summary"]["vector_search_binary"]["sha256"] == b["bom_summary"]["vector_search_binary"]["sha256"],
            "memgraph_notice_sets_equal":a["bom_summary"]["memgraph_license_files"] == b["bom_summary"]["memgraph_license_files"]}
        report["comparison"]["ann_exact_score_deviations"] = {r:report["roles"][r]["queries"]["ann_exact_score_deviations"] for r in IMAGES}
        report["comparison"]["ann_score_warning"] = "The unmodified SSG search template inherits the antiparallel ANN error (most dissimilar vector first). Nine matching selected outputs prove image parity, not ANN score/ranking correctness."
        report["comparison"]["ann_parity_order"] = "Canonical key-sorted rowset only; raw engine arrival order and distance are retained in query records and vector_controls."
        report["comparison"]["vector_controls"] = {r:report["roles"][r]["queries"]["vector_controls"] for r in IMAGES}
        report["recommendation"] = {"candidate":"arm64 core is sufficient only for the bounded observed graph/query-call capabilities, excluding ANN score/ranking correctness; smaller observed dependency/image surface",
            "not_adopted":"Parent retains profile/ADR decisions; G1 not accepted, no ADR/profile cutover; ADR-0024 amendment and independent review required",
            "controller_exact_cosine":"Controller reference normalizes query and stored vector; not AM's exact-cosine implementation or an eligible vector-store baseline",
            "legal":"Core retains BSL/MEL/APL and AUG restrictions; smaller image does not confer legal clearance",
            "remaining_closure":["eligible exact-cosine baseline in AM's persisted vector store",
                "ANN recall versus eligible exact cosine with k<N and resource comparison",
                "raw engine ordering/score semantics, antiparallel and nonunit controls, and exact rescoring",
                "physical project/generation domain isolation and eligibility filtering/completeness; global top-K then filtering is not a completeness proof",
                "amd64 contents", "complete rootfs ownership audit", "third-party versions/linkage and reproducible origins", "per-file MAGE license headers", "Python wheel-vendored native licenses", "custom OpenSSL origin/missing notices (both images)", "distribution/hosted/operator/counsel disposition", "production template/ACL/native-reader qualification", "clean performance and independent G1 quality gates"]}
        report["success"] = True
    except BaseException as e:
        report.update(success=False, error=type(e).__name__ + ": " + str(e))
        raise
    finally:
        report["cleanup"] = run.cleanup()
        if before is not None:
            after = json.loads(run.podman("containers-after", "ps", "-a", "--no-trunc", "--format", "json")["stdout"])
            after_ids = {r["Id"] for r in after}
            report["preexisting_containers_preserved"] = all(r["Id"] in after_ids for r in before)
            volumes_after = set(run.podman("volumes-after", "volume", "ls", "--format", "{{.Name}}")["stdout"].splitlines())
            report["preexisting_volumes_preserved"] = volumes_before <= volumes_after
            report["new_remaining_volumes"] = sorted(volumes_after - volumes_before)
        if (not all(r["absent"] for r in report["cleanup"])
                or report.get("preexisting_containers_preserved") is False
                or report.get("preexisting_volumes_preserved") is False):
            report.update(success=False, cleanup_or_preservation_failed=True)
        run.command("host-load-after", ["uptime"])
        save(root / "report.json", report)
        save(root / "artifact-inventory.json", inventory(root))
    print(json.dumps({"success":report["success"],"root":str(root),"report_sha256":sha(root / "report.json"),"g1_accepted":False}))
    if not report["success"]:
        raise SystemExit(1)


if __name__ == "__main__":
    main()
