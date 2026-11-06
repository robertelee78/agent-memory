#!/usr/bin/env python3
"""Acquire exact graph-image layers, inspect merged contents without executing foreign code.

This is not a complete license/linkage/provenance audit. No container is created.
All downloaded bytes remain below the explicitly named new acquisition directory.
"""
from __future__ import annotations
import argparse
import email
from email import policy
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import tarfile
import time
from discover import command, save, sha


def selected(path):
    return (path.startswith("usr/share/doc/") or path.startswith("usr/share/common-licenses/") or
            path == "var/lib/dpkg/status" or path.startswith("var/lib/dpkg/info/") or
            path in ("etc/os-release", "usr/lib/os-release", "etc/dpkg/dpkg.cfg.d/excludes") or
            (".dist-info/" in path and path.rsplit("/", 1)[-1] in ("METADATA", "RECORD", "WHEEL")) or
            ("site-packages/" in path and any(x in path.lower().rsplit("/", 1)[-1] for x in ("license", "copying", "notice"))))


def binary(path):
    return path in ("usr/lib/memgraph/memgraph", "usr/bin/mgconsole") or ".so" in path.rsplit("/", 1)[-1] or path.startswith("usr/lib/memgraph/query_modules/")


def scan(directory, output, binding):
    manifest_path = directory / "manifest.json"
    manifest = json.loads(manifest_path.read_text())
    if sha(manifest_path) != binding["manifest"].split(":")[1]:
        raise ValueError("Acquired child manifest differs from frozen identity")
    config_path = directory / manifest["config"]["digest"].split(":")[1]
    if sha(config_path) != manifest["config"]["digest"].split(":")[1]:
        raise ValueError("Image config digest mismatch")
    config = json.loads(config_path.read_text())
    if config["architecture"] != binding["platform"]["architecture"] or config["os"] != "linux":
        raise ValueError("Acquired image architecture mismatch")
    files = {}
    texts = {}
    layers = []
    for layer in manifest["layers"]:
        path = directory / layer["digest"].split(":")[1]
        if sha(path) != layer["digest"].split(":")[1] or path.stat().st_size != layer["size"]:
            raise ValueError("Layer digest/size mismatch")
        changes = {}
        changes_text = {}
        whiteouts = []
        with tarfile.open(path, "r|*") as archive:
            for member in archive:
                name = str(PurePosixPath(member.name.removeprefix("./")))
                if name.startswith("/") or ".." in PurePosixPath(name).parts:
                    raise ValueError("Unsafe image-layer path")
                part = PurePosixPath(name)
                if part.name.startswith(".wh."):
                    whiteouts.append(name)
                    continue
                if member.isdir():
                    continue
                entry = {"path": name, "bytes": member.size, "mode": member.mode, "uid": member.uid, "gid": member.gid,
                         "type": "file" if member.isfile() else "symlink" if member.issym() else "hardlink" if member.islnk() else "other",
                         "link": member.linkname or None, "layer": layer["digest"]}
                if member.isfile() and (selected(name) or binary(name)):
                    stream = archive.extractfile(member)
                    h = hashlib.sha256()
                    chunks = [] if selected(name) else None
                    prefix = b""
                    for block in iter(lambda: stream.read(1024 * 1024), b""):
                        if not prefix:
                            prefix = block[:20]
                        h.update(block)
                        if chunks is not None:
                            chunks.append(block)
                    entry["sha256"] = h.hexdigest()
                    if prefix[:4] == b"\x7fELF" and len(prefix) >= 20:
                        entry["elf_machine"] = int.from_bytes(prefix[18:20], "little" if prefix[5] == 1 else "big")
                    if chunks is not None:
                        changes_text[name] = b"".join(chunks)
                changes[name] = entry
        for item in whiteouts:
            part = PurePosixPath(item)
            opaque = part.name == ".wh..wh..opq"
            target = str(part.parent) if opaque else str(part.parent / part.name.removeprefix(".wh."))
            prefix = target.rstrip("/") + "/"
            for old in list(files):
                if (opaque and old.startswith(prefix)) or (not opaque and (old == target or old.startswith(prefix))):
                    files.pop(old)
                    texts.pop(old, None)
        files.update(changes)
        for name in changes:
            texts.pop(name, None)
        texts.update(changes_text)
        layers.append({"digest": layer["digest"], "bytes": path.stat().st_size, "verified": True, "entries": len(changes), "whiteouts": whiteouts})
    output.mkdir(parents=True, exist_ok=False)
    with tarfile.open(output / "selected-texts.tar.gz", "w:gz") as tar:
        for name, data in sorted(texts.items()):
            info = tarfile.TarInfo(name)
            info.size = len(data)
            info.mode = 0o644
            info.mtime = 0
            tar.addfile(info, io.BytesIO(data))
    status = texts.get("var/lib/dpkg/status", b"").decode(errors="replace")
    packages = []
    for stanza in status.split("\n\n"):
        fields = email.message_from_string(stanza, policy=policy.default)
        if fields.get("Package"):
            package = fields["Package"]
            copyright_path = "usr/share/doc/" + package + "/copyright"
            packages.append({"name": package, "version": fields.get("Version"), "status": fields.get("Status"),
                 "architecture": fields.get("Architecture"), "maintainer": fields.get("Maintainer"),
                 "copyright": files.get(copyright_path)})
    python = []
    for name, data in sorted(texts.items()):
        if name.endswith(".dist-info/METADATA"):
            meta = email.message_from_bytes(data, policy=policy.default)
            base = name.rsplit("/", 1)[0]
            licenses = [v for k, v in files.items() if k.startswith(base + "/") and any(x in k.lower() for x in ("license", "copying", "notice"))]
            python.append({"name": meta.get("Name"), "version": meta.get("Version"), "license": meta.get("License"),
                 "license_expression": meta.get("License-Expression"), "classifiers": meta.get_all("Classifier", []),
                 "metadata": files[name], "license_files": licenses})
    modules = [entry for name, entry in sorted(files.items()) if name.startswith("usr/lib/memgraph/query_modules/")]
    vendored = [entry for name, entry in sorted(files.items()) if "site-packages/" in name and ".so" in name.rsplit("/", 1)[-1]]
    save(output / "contents.json", {"schema": "am255/nonexecuting-image-contents-v1", "binding": binding, "config": config,
         "acquired_manifest_sha256": sha(manifest_path), "layers": layers, "file_count": len(files),
         "regular_file_bytes": sum(x["bytes"] for x in files.values() if x["type"] == "file"),
         "packages": packages, "python_distributions": python, "query_modules": modules, "python_native_files": vendored,
         "memgraph_binary": files.get("usr/lib/memgraph/memgraph"), "mgconsole": files.get("usr/bin/mgconsole"),
         "legal_files": [v for k,v in sorted(files.items()) if k.startswith("usr/share/doc/memgraph/")],
         "selected_text_archive": {"sha256": sha(output / "selected-texts.tar.gz"), "files": len(texts)},
         "unknowns": ["No foreign binary executed: package/METADATA declarations are not runtime validation",
           "No reproducible build or binary-source attestation", "Third-party versions and static/dynamic linkage not completely resolved",
           "Wheel-vendored native-library license coverage remains incomplete", "All local/distribution/hosted operator/counsel dispositions remain pending",
           "Hash coverage is selected notices/metadata/native/module files, not every rootfs byte"]})
    save(output / "file-inventory.json", list(sorted(files.values(), key=lambda x: x["path"])))
    return {"output": str(output), "architecture": config["architecture"], "packages": len(packages), "python": len(python),
            "module_files": len(modules), "selected_texts_sha256": sha(output / "selected-texts.tar.gz"), "contents_sha256": sha(output / "contents.json")}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bindings", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--acquisition", type=Path, required=True)
    parser.add_argument("--reuse-acquisition", action="store_true",
                        help="Reuse only existing byte-verified pinned layers from the retained failed attempt")
    parser.add_argument("--reserved-host-slot", action="store_true", required=True)
    args = parser.parse_args()
    args.output.mkdir(parents=True, exist_ok=False)
    if args.reuse_acquisition:
        if args.acquisition.is_symlink() or not args.acquisition.is_dir():
            raise ValueError("Existing owned acquisition directory required")
    else:
        args.acquisition.mkdir(parents=True, exist_ok=False)
    source = command(args.output, "execution-source", ["git", "rev-parse", "HEAD"])
    save(args.output / "freeze.json", {"bindings_sha256": sha(args.bindings), "script_sha256": sha(__file__),
         "helper_sha256": sha(Path(__file__).with_name("discover.py")), "start_unix": time.time(),
         "executed_source_commit": source["stdout"].strip(),
         "scope": "Exact amd64 core/MAGE image contents only; zero native/emulated graph execution"})
    results = []
    for name, spec in json.loads(args.bindings.read_text()).items():
        binding = next(x for x in spec["children"] if x["platform"]["architecture"] == "amd64")
        acquired = args.acquisition / name
        if args.reuse_acquisition and acquired.is_dir() and not acquired.is_symlink():
            save(args.output / (name + "-reuse.json"), {"path": str(acquired.resolve()),
                 "source": "retained prior acquired layers; every manifest/config/layer hash revalidated by scan"})
        else:
            copied = command(args.output, name + "-acquire", ["skopeo", "copy", "--preserve-digests", "docker://" + binding["reference"], "dir:" + str(acquired.resolve())], timeout=1200)
            if copied["exit"] != 0:
                results.append({"name": name, "missing_prerequisite": "Exact pinned layers unavailable; inspect retained acquisition failure"})
                continue
        results.append({"name": name, **scan(acquired, args.output / name, binding)})
    save(args.output / "report.json", {"results": results, "native_amd64_execution": False, "any_foreign_code_execution": False,
         "production": False, "license_acceptance": False})
    print(json.dumps(results, indent=2))

if __name__ == "__main__":
    main()
