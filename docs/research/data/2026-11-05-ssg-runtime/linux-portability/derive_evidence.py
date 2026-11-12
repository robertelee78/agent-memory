#!/usr/bin/env python3
"""Derive byte comparison or ELF dependencies without executing acquired code."""
from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path
import struct
import tarfile

RELEASE_SHA256 = "ff85fef9c91027d8037c182cdb5a6f3d29ea0a12b31557338188810308b112dc"


def sha(data):
    return hashlib.sha256(data).hexdigest()


def binding(path):
    if path.is_absolute() or ".." in path.parts:
        raise ValueError("Use a relative source path below the working directory")
    return {"path": path.as_posix(), "sha256": sha(path.read_bytes())}


def compare(contents):
    sources = {name: contents / name / "contents.json" for name in ("core", "mage")}
    reports = {name: json.loads(path.read_text()) for name, path in sources.items()}
    modules = {name: {row["path"]: row for row in report["query_modules"]}
               for name, report in reports.items()}
    common = sorted(modules["core"].keys() & modules["mage"].keys())
    engines = {name: report["memgraph_binary"] for name, report in reports.items()}
    engines_equal = engines["core"]["sha256"] == engines["mage"]["sha256"]
    return {
        "schema": "am255/amd64-core-mage-byte-comparison-v1",
        "source_reports": {name: binding(path) for name, path in sources.items()},
        "engine_binaries_equal": engines_equal,
        "engine_binaries": engines,
        "common_module_file_count": len(common),
        "common_module_sha256_differences": [path for path in common
            if modules["core"][path]["sha256"] != modules["mage"][path]["sha256"]],
        "vector_search": {name: rows["usr/lib/memgraph/query_modules/vector_search.so"]
                          for name, rows in modules.items()},
        "unknowns": [
            "Equal package versions and file sizes do NOT prove byte identity or runtime/query parity",
            ("Actual amd64 core/MAGE engine hashes match; runtime equivalence unestablished"
             if engines_equal else
             "Actual amd64 core/MAGE engine hashes differ; build/source/linkage explanation unestablished"),
            "No native or emulated amd64 executable was started; arm64 query parity is not inherited",
            "Module/file hash equality is byte evidence only, not correctness or full provenance",
        ],
    }


def needed(data):
    if data[:6] != b"\x7fELF\x02\x01":
        raise ValueError("Expected ELF64 little-endian release member")
    machine = struct.unpack_from("<H", data, 18)[0]
    phoff = struct.unpack_from("<Q", data, 32)[0]
    entsize, count = struct.unpack_from("<HH", data, 54)
    if entsize < 56:
        raise ValueError("Invalid ELF program-header size")
    headers = [struct.unpack_from("<IIQQQQQQ", data, phoff + i * entsize)
               for i in range(count)]
    dynamic = [h for h in headers if h[0] == 2]
    if not dynamic:
        return machine, []
    if len(dynamic) != 1:
        raise ValueError("Ambiguous ELF dynamic segment")
    segment = dynamic[0]
    tags = []
    for offset in range(segment[2], segment[2] + segment[5], 16):
        tag, value = struct.unpack_from("<qQ", data, offset)
        if tag == 0:
            break
        tags.append((tag, value))
    strings = [value for tag, value in tags if tag == 5]
    if len(strings) != 1:
        raise ValueError("Missing or ambiguous ELF string table")
    address = strings[0]
    loads = [h for h in headers if h[0] == 1 and h[3] <= address < h[3] + h[5]]
    if len(loads) != 1:
        raise ValueError("String table is not in exactly one file-backed load segment")
    start = loads[0][2] + address - loads[0][3]
    libraries = []
    for tag, value in tags:
        if tag == 1:
            begin = start + value
            end = data.index(b"\0", begin)
            libraries.append(data[begin:end].decode("utf-8"))
    return machine, libraries


def static_closure(archive):
    if sha(archive.read_bytes()) != RELEASE_SHA256:
        raise ValueError("Pinned release archive SHA-256 mismatch")
    files = []
    with tarfile.open(archive, "r:gz") as source:
        for member in source:
            if not member.isfile():
                continue
            basename = member.name.rsplit("/", 1)[-1]
            if basename != "llama-server" and ".so" not in basename:
                continue
            stream = source.extractfile(member)
            with stream:
                data = stream.read()
            if not data.startswith(b"\x7fELF"):
                continue
            machine, libraries = needed(data)
            files.append({"path": member.name, "bytes": len(data), "sha256": sha(data),
                          "elf_machine": machine, "needed": libraries})
    return {"schema": "am255/release-static-elf-needed-v1", "archive_sha256": RELEASE_SHA256,
            "elf_files": files,
            "scope": "Static DT_NEEDED from pinned archive bytes; no executable loading or full runtime closure"}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_subparsers(dest="mode", required=True)
    comp = modes.add_parser("compare")
    comp.add_argument("--contents", type=Path, required=True)
    elf = modes.add_parser("static")
    elf.add_argument("--archive", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    report = compare(args.contents) if args.mode == "compare" else static_closure(args.archive)
    with args.output.open("x") as target:
        json.dump(report, target, indent=2)
        target.write("\n")
    print(json.dumps({"output": str(args.output), "sha256": sha(args.output.read_bytes()),
                      "acquired_code_executed": False}))


if __name__ == "__main__":
    main()
