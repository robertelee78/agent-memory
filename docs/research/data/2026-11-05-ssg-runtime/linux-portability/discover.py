#!/usr/bin/env python3
"""Read-only primary-source/registry discovery; does not start containers or pull layers."""
from __future__ import annotations
import argparse
import hashlib
import json
from pathlib import Path
import platform
import subprocess
import time
import urllib.error
import urllib.request

SSG = "9c4fd08c29f37bffe89ecdf75b069a6baeb8a5e0"
UPSTREAM = "6b4e4bd582e457a47ee7cd498058495f47fee228"
BASE = "483adede202bac2dc3a69856a73b16533190ebb7"
IMAGES = {
    "mage": {"repository": "docker.io/memgraph/memgraph-mage", "index": "e8e03640071c57de6a13f2f4c5f9ed1789797dc5e8e771bbe62203b20e1b8c27"},
    "core": {"repository": "docker.io/memgraph/memgraph", "index": "042c8642954a7bde971a9c7683c8b1360e7fcf129a5fae36d63f12449e7154aa"},
}

def sha(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for block in iter(lambda: f.read(1024 * 1024), b""):
            h.update(block)
    return h.hexdigest()

def save(path, value):
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("x") as f:
        json.dump(value, f, indent=2, sort_keys=True)
        f.write("\n")

def command(root, name, argv, timeout=60):
    start = time.time()
    try:
        p = subprocess.run([str(x) for x in argv], capture_output=True, text=True, timeout=timeout)
        result = {"argv": [str(x) for x in argv], "start_unix": start, "duration_seconds": time.time()-start,
                  "exit": p.returncode, "stdout": p.stdout, "stderr": p.stderr}
    except subprocess.TimeoutExpired as exc:
        result = {"argv": [str(x) for x in argv], "start_unix": start, "duration_seconds": time.time()-start,
                  "exit": None, "error": str(exc), "stdout": (exc.stdout or b"").decode(errors="replace"),
                  "stderr": (exc.stderr or b"").decode(errors="replace")}
    save(root / (name + ".json"), result)
    return result

def fetch(root, name, url):
    start = time.time()
    req = urllib.request.Request(url, headers={"User-Agent": "AM255-public-research", "Accept": "application/vnd.github+json"})
    try:
        response = urllib.request.urlopen(req, timeout=60)
    except urllib.error.HTTPError as exc:
        response = exc
    with response:
        data = response.read()
        path = root / name
        path.write_bytes(data)
        save(root / (name + ".receipt.json"), {"url": url, "resolved_url": response.url, "status": response.status,
             "start_unix": start, "duration_seconds": time.time()-start, "bytes": len(data), "sha256": sha(path),
             "headers": dict(response.headers.items())})
    return data

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    root = args.output.resolve()
    root.mkdir(parents=True, exist_ok=False)
    save(root / "binding.json", {"schema": "am255/linux-discovery-v1", "am_baseline": BASE, "ssg": SSG,
         "upstream": UPSTREAM, "platform": platform.platform(), "script_sha256": sha(__file__),
         "scope": "metadata only; no image layers acquired and no containers started"})
    for name, argv in {
        "host": ["uname", "-a"], "vm": ["podman", "machine", "list", "--format", "json"],
        "core-local": ["podman", "image", "inspect", "3792294abf9a035895d8db16cbc56146d48a9fa1d379be6581631fdeae525c4f"],
        "mage-local": ["podman", "image", "inspect", "44b5244a3622425584cae52b0cdf1ea1690ebb98e27110b3adfd1af7ab8227a9"],
        "server-b9360-tag": ["skopeo", "inspect", "--raw", "docker://ghcr.io/ggml-org/llama.cpp:server-b9360"],
    }.items():
        command(root, name, argv)
    release = json.loads(fetch(root, "release.json", "https://api.github.com/repos/ggml-org/llama.cpp/releases/tags/b9360"))
    selected = [a for a in release["assets"] if a["name"] == "llama-b9360-bin-ubuntu-arm64.tar.gz"]
    save(root / "selected-release.json", {"tag": release["tag_name"], "target_commitish": release["target_commitish"],
         "immutable": release["immutable"], "assets": selected,
         "binding_limit": "GitHub publication digest and tag/source relation, not reproducible-build attestation"})
    workflows = json.loads(fetch(root, "workflows.json", "https://api.github.com/repos/ggml-org/llama.cpp/contents/.github/workflows?ref=" + UPSTREAM))
    for item in workflows:
        if "docker" in item["name"] or "ubuntu" in item["name"] or "release" in item["name"]:
            fetch(root, item["name"], item["download_url"])
    for name, path in {"LICENSE": "LICENSE", "cpu.Dockerfile": ".devops/cpu.Dockerfile"}.items():
        fetch(root, name, "https://raw.githubusercontent.com/ggml-org/llama.cpp/" + UPSTREAM + "/" + path)
    image_results = {}
    for name, spec in IMAGES.items():
        result = command(root, name + "-index", ["skopeo", "inspect", "--raw", "docker://" + spec["repository"] + "@sha256:" + spec["index"]])
        if result["exit"] != 0:
            image_results[name] = {**spec, "missing": "registry index unavailable; raw failure retained"}
            continue
        actual = hashlib.sha256(result["stdout"].encode()).hexdigest()
        if actual != spec["index"]:
            raise ValueError("Registry index byte identity mismatch")
        children = []
        for child in json.loads(result["stdout"])["manifests"]:
            plat = child["platform"]
            if plat["os"] == "linux" and plat["architecture"] in ("arm64", "amd64"):
                label = name + "-" + plat["architecture"]
                ref = spec["repository"] + "@" + child["digest"]
                manifest = command(root, label + "-manifest", ["skopeo", "inspect", "--raw", "docker://" + ref])
                config = command(root, label + "-config", ["skopeo", "inspect", "--config", "docker://" + ref])
                entry = {"platform": plat, "reference": ref, "manifest": child["digest"], "metadata_only": True}
                if manifest["exit"] == 0:
                    if hashlib.sha256(manifest["stdout"].encode()).hexdigest() != child["digest"].split(":")[1]:
                        raise ValueError("Child manifest identity mismatch")
                    parsed = json.loads(manifest["stdout"])
                    entry.update({"compressed_bytes": sum(x["size"] for x in parsed["layers"]), "config": parsed["config"]["digest"], "layers": parsed["layers"]})
                children.append(entry)
        image_results[name] = {**spec, "children": children}
    save(root / "image-bindings.json", image_results)
    save(root / "inventory.json", [{"path": str(p.relative_to(root)), "bytes": p.stat().st_size, "sha256": sha(p)} for p in sorted(root.rglob("*")) if p.is_file()])
    print(json.dumps({"output": str(root), "selected_release": selected[0]["name"] if selected else None, "images": image_results}, indent=2))

if __name__ == "__main__":
    main()
