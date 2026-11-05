"""Collect and derive the #255 SSG runtime license/provenance inventory.

    python3 derive_bom.py collect   # tiny read-only commands -> evidence/*.json
    python3 derive_bom.py derive    # evidence + retained parent outputs -> bom.json
    python3 derive_bom.py scan-cargo <Cargo.lock>   # helper invoked by collect

Stdlib only. `collect` starts throwaway `--network none` containers from the
pinned image (never the owned graph/model services), reads registry/source
metadata, and never pulls, builds, or serves. Evidence records keep the exact
argv, cwd, start time, duration, exit status, stdout and stderr.
This is an inventory for operator/counsel review, not legal clearance.
"""
import datetime
import glob
import hashlib
import json
import os
import re
import subprocess
import sys
import tarfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
EVIDENCE = os.path.join(HERE, "evidence")
RETAINED = "/tmp/am-v15-delivery"
LLAMA = os.path.join(RETAINED, "llama-reference")
SSG = "/opt/ssg"

MAGE_INDEX = "sha256:e8e03640071c57de6a13f2f4c5f9ed1789797dc5e8e771bbe62203b20e1b8c27"
MAGE_REF = "docker.io/memgraph/memgraph-mage@" + MAGE_INDEX
MAGE_CONFIG = "44b5244a3622425584cae52b0cdf1ea1690ebb98e27110b3adfd1af7ab8227a9"
LLAMA_COMMIT = "a578bb6b94529b7b2721d0ee49eaea5dbb2c6d07"
LLAMA_UPSTREAM_PARENT = "7bfe120c210c04e7b53d1e9eca8b92acd4ab252f"
LLAMA_ALT_COMMIT = "6b4e4bd582e457a47ee7cd498058495f47fee228"
LLAMA_ALT_SERVER = "/opt/homebrew/bin/llama-server"
MEMGRAPH_RAW = "https://raw.githubusercontent.com/memgraph/memgraph/v3.10.1/"
HF_8B = ("Qwen/Qwen3-Embedding-8B-GGUF", "69d0e58a13e463cd99a9b83e3f5fee7c10265fab")
HF_06 = ("Qwen/Qwen3-Embedding-0.6B-GGUF", "370f27d7550e0def9b39c1f16d3fbaa13aa67728")
WEIGHTS = {
    "Qwen3-Embedding-8B-Q8_0.gguf": (8047105824, "d20ddc71e8a5c4344f2343481e242233a997dc5eaff442427a945836c97b4deb"),
    "Qwen3-Embedding-0.6B-Q8_0.gguf": (639150592, "06507c7b42688469c4e7298b0a1e16deff06caf291cf0a5b278c308249c3e439"),
}
THROWAWAY = ["docker", "run", "--rm", "--pull=never", "--network", "none", "--platform", "linux/arm64"]
CURL = ["curl", "-sSL", "--max-time", "20"]

# Retained parent outputs (argv/stdout records or copied files) consumed by derive.
RETAINED_INPUTS = [
    "mage-package-bom.json",
    "mage-python-bom.json",
    "mage-python-license-summary.json",
    "mage-files-bom.json",
    "mage-license-files.json",
    "mage-license-paths.json",
    "mage-usr-share-root.json",
    "mage-platform.json",
    "mage-full-dependency-license-tar.json",
    "mage-license-tar-copy.json",
    "mage-license-docs-copy.json",
    "mage-doc-licenses.tar",
    "reference-server-version.json",
    "reference-models.json",
    "alternate-models.json",
    "255-resource-coordination.md",
    "embedding-api-controls.json",
]

COPYLEFT = re.compile(r"\b(A?GPL|LGPL|GNU (Library or )?(Lesser )?General Public|MPL|Mozilla Public|EPL|CDDL|EUPL)", re.I)


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha256_file(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def run(name, argv, cwd=HERE, stdin=None):
    started = datetime.datetime.now(datetime.timezone.utc).isoformat()
    clock = time.monotonic()
    done = subprocess.run(argv, cwd=cwd, input=stdin, capture_output=True, timeout=60)
    record = {
        "argv": argv,
        "cwd": cwd,
        "started_at": started,
        "seconds": round(time.monotonic() - clock, 3),
        "exit": done.returncode,
        "stdout": done.stdout.decode("utf-8", "replace"),
        "stderr": done.stderr.decode("utf-8", "replace"),
    }
    if stdin is not None:
        record["stdin_sha256"] = sha256_bytes(stdin)
    os.makedirs(EVIDENCE, exist_ok=True)
    with open(os.path.join(EVIDENCE, name + ".json"), "w", encoding="utf-8") as handle:
        json.dump(record, handle, indent=1, sort_keys=True)
        handle.write("\n")
    print("%-34s exit=%s %.2fs" % (name, done.returncode, record["seconds"]))


def curl_hash(url):
    return ["sh", "-c", 'curl -sSL --max-time 20 "$0" | shasum -a 256', url]


def collect():
    with open(os.path.join(HERE, "image_probe.py"), "rb") as handle:
        probe = handle.read()
    run("image-probe", THROWAWAY[:2] + ["-i"] + THROWAWAY[2:] + ["--entrypoint", "/usr/bin/python3", MAGE_REF, "-"], stdin=probe)
    run(
        "image-dpkg-verify",
        THROWAWAY + ["--entrypoint", "/usr/bin/dpkg", MAGE_REF, "--verify", "memgraph", "msodbcsql18", "openssl", "libssl3t64", "packages-microsoft-prod"],
    )
    run("image-inspect", ["docker", "image", "inspect", MAGE_CONFIG])
    run("container-bindings", ["docker", "ps", "-a", "--no-trunc", "--filter", "name=am255-graph", "--format", "{{.Names}}\t{{.ID}}\t{{.Image}}\t{{.ImageID}}"])
    run("hub-mage-tag", CURL + ["https://hub.docker.com/v2/repositories/memgraph/memgraph-mage/tags/3.10.1"])
    run("hub-core-tag", CURL + ["https://hub.docker.com/v2/repositories/memgraph/memgraph/tags/3.10.1"])
    for tag in ("v3.10.1", "v3.10.1-rc1"):
        run("gh-memgraph-tag-" + tag, CURL + ["https://api.github.com/repos/memgraph/memgraph/git/ref/tags/" + tag])
    for name in ("BSL.txt", "APL.txt", "MEL.pdf"):
        run("gh-memgraph-v3.10.1-" + name, curl_hash(MEMGRAPH_RAW + "licenses/" + name))
    run("gh-memgraph-v3.10.1-LICENSE", CURL + [MEMGRAPH_RAW + "LICENSE"])
    run("gh-mage-Dockerfile.release", CURL + [MEMGRAPH_RAW + "mage/Dockerfile.release"])
    run("gh-mage-install_runtime_requirements", CURL + [MEMGRAPH_RAW + "mage/install_runtime_requirements.sh"])
    run("gh-mage-python-requirements", CURL + [MEMGRAPH_RAW + "mage/python/requirements.txt"])
    run("gh-mage-archived-repo", CURL + ["https://api.github.com/repos/memgraph/mage"])
    for label, (repo, revision) in (("8b", HF_8B), ("06b", HF_06)):
        run("hf-%s-revision" % label, CURL + ["https://huggingface.co/api/models/%s/revision/%s" % (repo, revision)])
        run("hf-%s-README" % label, curl_hash("https://huggingface.co/%s/raw/%s/README.md" % (repo, revision)))
    weights = [os.path.join(RETAINED, name) for name in sorted(WEIGHTS)]
    run("gguf-general", [sys.executable, os.path.join(HERE, "gguf_general.py")] + weights)
    run("weights-size", ["stat", "-f", "%z %N"] + weights)
    run("llama-head", ["git", "-C", LLAMA, "log", "-1", "--format=%H%n%ci%n%an%n%s", LLAMA_COMMIT])
    run("llama-origin", ["git", "-C", LLAMA, "remote", "get-url", "origin"])
    run("llama-master", ["git", "-C", LLAMA, "log", "-1", "--format=%H %ci", "refs/heads/master"])
    run("llama-commit-on-master", ["git", "-C", LLAMA, "merge-base", "--is-ancestor", LLAMA_COMMIT, "refs/heads/master"])
    run("llama-parent-on-master", ["git", "-C", LLAMA, "merge-base", "--is-ancestor", LLAMA_UPSTREAM_PARENT, "refs/heads/master"])
    run("llama-build-number", ["git", "-C", LLAMA, "rev-list", "--count", LLAMA_COMMIT])
    run("llama-nonmainline-commits", ["git", "-C", LLAMA, "log", "--format=%H\t%an\t%s", LLAMA_UPSTREAM_PARENT + ".." + LLAMA_COMMIT])
    run("llama-license-hashes", ["shasum", "-a", "256", "LICENSE", "licenses/LICENSE-jsonhpp", "../llama-reference-build/license.cpp"], cwd=LLAMA)
    run("llama-alt-version", [LLAMA_ALT_SERVER, "--version"])
    run("llama-alt-on-master", ["git", "-C", LLAMA, "merge-base", "--is-ancestor", LLAMA_ALT_COMMIT, "refs/heads/master"])
    run("llama-alt-tag", ["git", "-C", LLAMA, "tag", "--points-at", LLAMA_ALT_COMMIT])
    run("llama-alt-brew", ["brew", "info", "--json=v2", "llama.cpp"])
    run("ssg-head", ["git", "-C", SSG, "rev-parse", "HEAD"])
    run("ssg-license", ["sh", "-c", "shasum -a 256 LICENSE Cargo.toml Cargo.lock && cat LICENSE"], cwd=SSG)
    run("ssg-cargo-licenses", [sys.executable, os.path.abspath(__file__), "scan-cargo", os.path.join(SSG, "Cargo.lock")])


def scan_cargo(lock_path):
    """Declared `license` of each locked crate from the local cargo registry sources."""
    text = open(lock_path, encoding="utf-8").read()
    roots = glob.glob(os.path.expanduser("~/.cargo/registry/src/*"))
    rows = []
    for block in text.split("[[package]]")[1:]:
        fields = dict(re.findall(r'^(name|version|source|checksum) = "([^"]*)"', block, re.M))
        row = {k: fields.get(k) for k in ("name", "version", "source", "checksum")}
        manifest = next(
            (p for r in roots for p in [os.path.join(r, "%s-%s" % (row["name"], row["version"]), "Cargo.toml")] if os.path.isfile(p)),
            None,
        )
        if row["source"] is None:
            row["origin"] = "workspace"
        elif manifest is None:
            row["origin"] = "registry source not present locally"
        else:
            body = open(manifest, encoding="utf-8").read()
            declared = re.search(r'^license\s*=\s*"([^"]*)"', body, re.M)
            license_file = re.search(r'^license-file\s*=\s*"([^"]*)"', body, re.M)
            row["origin"] = "local registry source"
            row["license"] = declared.group(1) if declared else None
            row["license_file"] = license_file.group(1) if license_file else None
        rows.append(row)
    print(json.dumps(rows, sort_keys=True))


def evidence(name):
    with open(os.path.join(EVIDENCE, name + ".json"), encoding="utf-8") as handle:
        return json.load(handle)


def retained(name):
    with open(os.path.join(RETAINED, name), encoding="utf-8") as handle:
        return json.load(handle)


def stdout_hash(name):
    record = evidence(name)
    return record["stdout"].split()[0] if record["exit"] == 0 and record["stdout"] else None


def dep5_licenses(text):
    if not re.match(r"Format:\s*\S+", text):
        return None
    return sorted({m.strip() for m in re.findall(r"^License:\s*(.+)$", text, re.M)})


def tar_members(path):
    rows = {}
    with tarfile.open(path) as archive:
        for member in archive.getmembers():
            if member.isfile():
                data = archive.extractfile(member).read()
                rows["/" + member.name] = {"bytes": len(data), "sha256": sha256_bytes(data), "data": data}
    return rows


def maintainer_class(name, maintainer):
    """Classify the dpkg Maintainer field; a maintainer string is not proof of origin."""
    lowered = maintainer.lower()
    if name == "memgraph":
        return "memgraph"
    if "memgraph" in lowered:
        return "memgraph-custom-build"
    if "microsoft" in lowered:
        return "microsoft"
    if "ubuntu" in lowered:
        return "ubuntu-developers"
    return "individual-maintainer"


def declaration_status(dist):
    if dist.get("license_expression"):
        return "spdx-expression"
    if dist.get("license"):
        return "free-text"
    if dist.get("license_classifiers"):
        return "classifier-only"
    return "none"


def declared_text(dist):
    parts = [dist.get("license_expression") or ""]
    value = dist.get("license")
    parts.append(value.get("first_line", "") if isinstance(value, dict) else (value or ""))
    parts.extend(dist.get("license_classifiers") or [])
    return " | ".join(p for p in parts if p)


def derive():
    probe_record = evidence("image-probe")
    probe = json.loads(probe_record["stdout"])
    inspect = json.loads(evidence("image-inspect")["stdout"])[0]
    hub_mage = json.loads(evidence("hub-mage-tag")["stdout"])
    hub_core = json.loads(evidence("hub-core-tag")["stdout"])
    members = tar_members(os.path.join(RETAINED, "mage-doc-licenses.tar"))
    copied_root = os.path.join(RETAINED, "memgraph-image-licenses")

    bindings = [line.split("\t") for line in evidence("container-bindings")["stdout"].splitlines() if line]
    platform_host = retained("mage-platform.json")["stdout"].split()[1]
    container_rows = [
        {
            "name": name,
            "container_id": cid,
            "image": image,
            "image_id": image_id,
            "retained_exec_outputs_bound": cid.startswith(platform_host) and name == "am255-graph-b-20261105",
        }
        for name, cid, image, image_id in bindings
    ]

    env = dict(item.split("=", 1) for item in inspect["Config"]["Env"])
    dpkg = {row["Package"]: row for row in probe["dpkg"]}
    non_distro = probe["non_distro_package_files"]
    package_modules = set(non_distro["memgraph"]["listed_query_modules"])

    # Memgraph-shipped licenses: fresh probe hash vs retained tar hash vs retained docker-cp copy hash.
    memgraph_docs = []
    for entry in probe["share_doc_files"]:
        if not entry["path"].startswith("/usr/share/doc/memgraph/"):
            continue
        relative = entry["path"][len("/usr/share/doc/memgraph/"):]
        copied = os.path.join(copied_root, relative)
        memgraph_docs.append(
            {
                "path": entry["path"],
                "bytes": entry["bytes"],
                "sha256": entry["sha256"],
                "retained_tar_sha256_match": members.get(entry["path"], {}).get("sha256") == entry["sha256"],
                "retained_copy_sha256_match": os.path.isfile(copied) and sha256_file(copied) == entry["sha256"],
                "component": relative.split("/")[1] if relative.startswith("third-party/") else "memgraph",
            }
        )
    upstream_license_hashes = {name: stdout_hash("gh-memgraph-v3.10.1-" + name) for name in ("BSL.txt", "APL.txt", "MEL.pdf")}
    for row in memgraph_docs:
        name = os.path.basename(row["path"])
        if row["component"] == "memgraph" and name in upstream_license_hashes:
            row["upstream_v3.10.1_licenses_sha256_match"] = upstream_license_hashes[name] == row["sha256"]
    bsl = members["/usr/share/doc/memgraph/BSL.txt"]["data"].decode("utf-8")

    # Distribution packages.
    packages = []
    for name in sorted(dpkg):
        row = dpkg[name]
        copyright_entry = row.get("copyright")
        text = None
        if copyright_entry:
            resolved = copyright_entry.get("resolved", copyright_entry["path"])
            member = members.get(resolved)
            text = member["data"].decode("utf-8", "replace") if member else None
        maintainer = row.get("Maintainer", "")
        packages.append(
            {
                "name": name,
                "version": row["Version"],
                "architecture": row["Architecture"],
                "status": row.get("Status"),
                "source": row.get("Source"),
                "maintainer": maintainer,
                "homepage": row.get("Homepage"),
                "maintainer_class": maintainer_class(name, maintainer),
                "copyright": (
                    dict(copyright_entry, retained_tar_sha256_match=bool(text) and members[copyright_entry.get("resolved", copyright_entry["path"])]["sha256"] == copyright_entry["sha256"])
                    if copyright_entry
                    else None
                ),
                "dep5_license_short_names": dep5_licenses(text) if text else None,
                "copyright_format": None if not text else ("dep5" if dep5_licenses(text) is not None else "unstructured"),
            }
        )

    # Python distributions.
    python = []
    for dist in probe["python_dists"]:
        status = declaration_status(dist)
        flags = []
        if status == "none":
            flags.append("no-license-declaration-in-metadata")
        if not dist["license_files"]:
            flags.append("no-license-file-in-RECORD")
        if COPYLEFT.search(declared_text(dist)):
            flags.append("copyleft-or-file-copyleft-term-declared")
        if dist["vendored_shared_libraries"]:
            flags.append("vendors-native-shared-libraries")
        python.append(
            {
                "name": dist["name"],
                "version": dist["version"],
                "declaration_status": status,
                "license_expression": dist["license_expression"],
                "license": dist["license"],
                "license_classifiers": dist["license_classifiers"],
                "license_files": dist["license_files"],
                "vendored_shared_libraries": dist["vendored_shared_libraries"],
                "review_flags": flags,
            }
        )

    modules = [e for e in probe["memgraph_lib"] if "/query_modules/" in e["path"]]
    module_rows = [dict(e, owner="memgraph-deb" if e["path"] in package_modules else "mage-layer-unpackaged") for e in modules]
    top_level_modules = sorted(
        {
            e["path"].split("/query_modules/")[1].split("/")[0]
            for e in modules
            if re.search(r"\.(so|py)$", e["path"]) and e["path"].count("/") == 5
        }
    )

    cargo = json.loads(evidence("ssg-cargo-licenses")["stdout"])
    gguf = {os.path.basename(row["path"]): row["general"] for row in json.loads(evidence("gguf-general")["stdout"])}
    sizes = dict(reversed(line.split(" ", 1)) for line in evidence("weights-size")["stdout"].splitlines())
    nonmainline = [line.split("\t") for line in evidence("llama-nonmainline-commits")["stdout"].splitlines()]
    authors = {}
    for _, author, _ in nonmainline:
        authors[author] = authors.get(author, 0) + 1

    def tag_commit(tag):
        return json.loads(evidence("gh-memgraph-tag-" + tag)["stdout"])["object"]["sha"]

    totals = probe["tree_totals"]
    site = probe["python_site_dirs"][0]
    bom = {
        "schema": "agent-memory/ssg-runtime-license-bom-v1",
        "status": "research inventory for operator/counsel disposition; NOT legal clearance, NOT complete provenance",
        "issue": "robertelee78/agent-memory#255",
        "derivation": {
            "script_sha256": sha256_file(os.path.abspath(__file__)),
            "image_probe_sha256": sha256_file(os.path.join(HERE, "image_probe.py")),
            "gguf_reader_sha256": sha256_file(os.path.join(HERE, "gguf_general.py")),
            "evidence": {
                os.path.basename(p)[:-5]: {"sha256": sha256_file(p), "exit": json.load(open(p))["exit"]}
                for p in sorted(glob.glob(os.path.join(EVIDENCE, "*.json")))
            },
            "retained_parent_inputs": {
                name: {"path": os.path.join(RETAINED, name), "sha256": sha256_file(os.path.join(RETAINED, name))}
                for name in RETAINED_INPUTS
            },
        },
        "graph_image": {
            "registry_reference": MAGE_REF,
            "index_digest": MAGE_INDEX,
            "platform_manifests": {i["architecture"]: {"digest": i["digest"], "compressed_bytes": i["size"]} for i in hub_mage["images"]},
            "registry_last_updated": hub_mage.get("last_updated"),
            "executed_platform": "linux/arm64 (Podman machine on macOS); amd64 manifest metadata only",
            "config_image_id": inspect["Id"],
            "repo_digests": inspect.get("RepoDigests"),
            "created": inspect.get("Created"),
            "local_uncompressed_bytes": inspect.get("Size"),
            "rootfs_layers": inspect["RootFS"]["Layers"],
            "env": env,
            "labels": inspect["Config"].get("Labels"),
            "entrypoint": inspect["Config"].get("Entrypoint"),
            "user": inspect["Config"].get("User"),
            "os_release": dict(re.findall(r'^(PRETTY_NAME|VERSION_ID)="?([^"\n]*)', probe["os_release"], re.M)),
            "memgraph_package_version": dpkg["memgraph"]["Version"],
            "build_ref": {
                "MEMGRAPH_REF": env.get("MEMGRAPH_REF"),
                "tag_v3.10.1_commit": tag_commit("v3.10.1"),
                "tag_v3.10.1-rc1_commit": tag_commit("v3.10.1-rc1"),
                "note": "Image env names the rc1 tag; both tags resolved to the same commit when observed. Tags are mutable and the binary was not rebuilt, so this is not reproducible-build provenance.",
            },
            "container_bindings": container_rows,
            "byte_footprint": {
                "python_site_packages": dict(totals[site], path=site),
                "usr_lib_memgraph": totals["/usr/lib/memgraph"],
                "usr_share_doc": totals["/usr/share/doc"],
                "opt": totals["/opt"],
            },
        },
        "core_image_comparison": {
            "registry_reference": "docker.io/memgraph/memgraph@" + hub_core["digest"],
            "platform_manifests": {i["architecture"]: {"digest": i["digest"], "compressed_bytes": i["size"]} for i in hub_core["images"]},
            "status": "registry metadata only; not pulled, inspected, or executed",
        },
        "memgraph_licenses": {
            "files": memgraph_docs,
            "bsl": {
                "licensed_work_line": re.search(r"LICENSED WORK:\s*(.+)", bsl).group(1).strip(),
                "title_line": bsl.splitlines()[0].strip() + " " + bsl.splitlines()[1].strip(),
                "change_date_literal": re.search(r"CHANGE DATE:\s*(\S+)", bsl).group(1),
                "change_license": re.search(r"CHANGE LICENSE:\s*(.+)", bsl).group(1).strip(),
                "note": "Change date literal is not a valid ISO date; recorded verbatim, never normalized.",
            },
            "upstream_repository_license_statement_sha256": sha256_bytes(evidence("gh-memgraph-v3.10.1-LICENSE")["stdout"].encode("utf-8")),
            "upstream_repository_license_statement": evidence("gh-memgraph-v3.10.1-LICENSE")["stdout"].strip(),
            "third_party_versions": "not stated by notice files; linked versions unverified",
        },
        "query_modules": {
            "top_level": top_level_modules,
            "package_owned_count": sum(1 for r in module_rows if r["owner"] == "memgraph-deb"),
            "mage_layer_unpackaged_count": sum(1 for r in module_rows if r["owner"] != "memgraph-deb"),
            "files": module_rows,
            "ssg_reference_uses": ["vector_search.search", "vector_search.cosine_similarity", "vector_search.show_index_info", "CREATE VECTOR INDEX"],
        },
        "memgraph_runtime_files": [e for e in probe["memgraph_lib"] if "/query_modules/" not in e["path"]],
        "non_distro_packages": {
            name: {
                "dpkg": dpkg.get(name),
                "listed_files": info["listed"] if info else None,
                "missing_listed_paths": info["missing"] if info else None,
                "license_like_present": info["license_like"] if info else None,
                "debconf_template_sha256": info["templates"]["sha256"] if info and info["templates"] else None,
            }
            for name, info in non_distro.items()
        },
        "debconf_license_answers": probe["debconf_license_answers"],
        "dpkg_verify": {"exit": evidence("image-dpkg-verify")["exit"], "stdout": evidence("image-dpkg-verify")["stdout"]},
        "dpkg_excludes": probe["dpkg_excludes"],
        "apt_sources": [{"path": s["path"], "sha256": s["sha256"]} for s in probe["apt_sources"]],
        "opt_files": probe["opt_files"],
        "common_licenses": probe["common_licenses"],
        "distro_packages": packages,
        "python_distributions": python,
        "embedding": {
            "weights": {
                name: {
                    "expected_bytes": expected[0],
                    "observed_bytes": int(sizes.get(os.path.join(RETAINED, name), "-1")),
                    "sha256": expected[1],
                    "sha256_status": "verified by parent before use (255-resource-coordination.md); not re-hashed by this derivation",
                    "gguf_general": gguf.get(name),
                }
                for name, expected in WEIGHTS.items()
            },
            "hf_model_repos": {
                label: dict(
                    repo=repo,
                    revision=revision,
                    card_license=json.loads(evidence("hf-%s-revision" % label)["stdout"]).get("cardData", {}).get("license"),
                    base_model=json.loads(evidence("hf-%s-revision" % label)["stdout"]).get("cardData", {}).get("base_model"),
                    files=[s["rfilename"] for s in json.loads(evidence("hf-%s-revision" % label)["stdout"]).get("siblings", [])],
                    readme_sha256=stdout_hash("hf-%s-README" % label),
                )
                for label, (repo, revision) in (("8b", HF_8B), ("06b", HF_06))
            },
            "llama_cpp": {
                "commit": LLAMA_COMMIT,
                "head": evidence("llama-head")["stdout"].splitlines(),
                "fetched_from": evidence("llama-origin")["stdout"].strip(),
                "on_upstream_master": evidence("llama-commit-on-master")["exit"] == 0,
                "upstream_parent": LLAMA_UPSTREAM_PARENT,
                "upstream_parent_on_master": evidence("llama-parent-on-master")["exit"] == 0,
                "build_number_commit_count": int(evidence("llama-build-number")["stdout"].strip()),
                "non_mainline_commit_count": len(nonmainline),
                "non_mainline_authors": dict(sorted(authors.items())),
                "license_hashes": dict(reversed(l.split("  ", 1)) for l in evidence("llama-license-hashes")["stdout"].splitlines()),
            },
            "llama_cpp_alternate": {
                "server": LLAMA_ALT_SERVER,
                "version_line": next(l for l in evidence("llama-alt-version")["stderr"].splitlines() if l.startswith("version:")),
                "commit": LLAMA_ALT_COMMIT,
                "on_upstream_master": evidence("llama-alt-on-master")["exit"] == 0,
                "upstream_tags": evidence("llama-alt-tag")["stdout"].split(),
                "homebrew": {
                    k: json.loads(evidence("llama-alt-brew")["stdout"])["formulae"][0][k] for k in ("name", "license", "installed")
                },
            },
        },
        "ssg": {
            "commit": evidence("ssg-head")["stdout"].strip(),
            "license_hashes": dict(reversed(l.split("  ", 1)) for l in evidence("ssg-license")["stdout"].splitlines() if re.match(r"^[0-9a-f]{64}  ", l)),
            "license_text": evidence("ssg-license")["stdout"].split("\n", 3)[-1].strip(),
            "cargo_lock_packages": len(cargo),
            "cargo_declared_licenses": {
                k: sum(1 for r in cargo if r["origin"] != "workspace" and (r.get("license") or "(none)") == k)
                for k in sorted({r.get("license") or "(none)" for r in cargo if r["origin"] != "workspace"})
            },
            "cargo_undeclared": [
                "%s %s (%s)" % (r["name"], r["version"], r["origin"]) for r in cargo if r["origin"] != "workspace" and not r.get("license")
            ],
        },
    }
    bom["coverage"] = coverage(bom)
    with open(os.path.join(HERE, "bom.json"), "w", encoding="utf-8") as handle:
        json.dump(bom, handle, indent=1, sort_keys=True)
        handle.write("\n")
    print(json.dumps(bom["coverage"], indent=1, sort_keys=True))


def coverage(bom):
    packages, python = bom["distro_packages"], bom["python_distributions"]
    installed = [p for p in packages if p["status"] == "install ok installed"]
    docs = bom["memgraph_licenses"]["files"]
    count = lambda rows, test: sum(1 for r in rows if test(r))
    return {
        "distro_packages_recorded": len(packages),
        "distro_packages_installed": len(installed),
        "distro_installed_with_copyright_file": count(installed, lambda p: p["copyright"]),
        "distro_installed_without_copyright_file": sorted(p["name"] for p in installed if not p["copyright"]),
        "distro_copyright_dep5": count(installed, lambda p: p["copyright_format"] == "dep5"),
        "distro_copyright_unstructured": sorted(p["name"] for p in installed if p["copyright_format"] == "unstructured"),
        "distro_maintainer_not_ubuntu_developers": sorted("%s %s [%s]" % (p["name"], p["version"], p["maintainer"]) for p in installed if p["maintainer_class"] != "ubuntu-developers"),
        "python_distributions": len(python),
        "python_declaration_status": {k: count(python, lambda d, k=k: d["declaration_status"] == k) for k in ("spdx-expression", "free-text", "classifier-only", "none")},
        "python_with_hashed_license_file": count(python, lambda d: d["license_files"]),
        "python_without_license_file": sorted("%s %s" % (d["name"], d["version"]) for d in python if not d["license_files"]),
        "python_without_any_declaration": sorted("%s %s" % (d["name"], d["version"]) for d in python if d["declaration_status"] == "none"),
        "python_copyleft_term_declared": sorted("%s %s" % (d["name"], d["version"]) for d in python if "copyleft-or-file-copyleft-term-declared" in d["review_flags"]),
        "python_vendoring_native_libraries": sorted("%s %s (%d)" % (d["name"], d["version"], len(d["vendored_shared_libraries"])) for d in python if d["vendored_shared_libraries"]),
        "python_vendored_native_library_files": sum(len(d["vendored_shared_libraries"]) for d in python),
        "memgraph_license_files": len(docs),
        "memgraph_license_files_matching_retained_tar": count(docs, lambda d: d["retained_tar_sha256_match"]),
        "memgraph_license_files_matching_retained_copy": count(docs, lambda d: d["retained_copy_sha256_match"]),
        "memgraph_third_party_components": len({d["component"] for d in docs if d["component"] != "memgraph"}),
        "query_module_files": len(bom["query_modules"]["files"]),
        "query_module_files_unpackaged_mage_layer": bom["query_modules"]["mage_layer_unpackaged_count"],
        "unknown_or_unverified": [
            "Versions of the %d Memgraph third-party notice components and whether each is linked into the shipped binaries are not stated or verified."
            % len({d["component"] for d in docs if d["component"] != "memgraph"}),
            "Per-file license headers of unpackaged MAGE query modules were not examined; upstream repository statement is BSL/MEL unless otherwise noted per file.",
            "Licenses of native libraries vendored inside Python wheels are not inventoried.",
            "Files not owned by dpkg, pip RECORD, or /usr/lib/memgraph walk were not enumerated; no whole-rootfs ownership audit.",
            "No binary was rebuilt from source; no reproducible-build or code-origin verification of memgraph, MAGE modules, openssl custom packages, msodbcsql18, or wheels.",
            "Core memgraph/memgraph image contents, query modules and notices are unobserved (registry metadata only).",
            "amd64 MAGE manifest contents are unobserved (metadata only).",
            "Qwen GGUF repos contain no license text file; license is model-card and GGUF metadata declaration only.",
        ],
    }


if __name__ == "__main__":
    command = sys.argv[1] if len(sys.argv) > 1 else ""
    if command == "collect":
        collect()
    elif command == "derive":
        derive()
    elif command == "scan-cargo" and len(sys.argv) == 3:
        scan_cargo(sys.argv[2])
    else:
        sys.exit(__doc__)
