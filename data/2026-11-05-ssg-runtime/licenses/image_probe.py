"""Read-only license/provenance probe executed inside the pinned MAGE image.

Run with no network, as a throwaway container, by derive_bom.py's recorded argv.
Stdlib only. Emits one JSON document on stdout; never writes inside the image.
"""
import hashlib
import importlib.metadata
import json
import os
import re

LICENSE_NAME = re.compile(r"(licen[cs]e|copying|notice|authors|copyright|patents)", re.I)


def sha256(path):
    digest = hashlib.sha256()
    with open(path, "rb") as handle:
        for block in iter(lambda: handle.read(1 << 20), b""):
            digest.update(block)
    return digest.hexdigest()


def file_entry(path):
    real = os.path.realpath(path)
    entry = {"path": path, "bytes": os.path.getsize(real), "sha256": sha256(real)}
    if real != path:
        entry["resolved"] = real
    return entry


def license_field(value):
    """Keep short declarations verbatim; bind embedded full texts by digest."""
    if value is None or len(value) <= 200:
        return value
    return {
        "first_line": value.strip().splitlines()[0][:200],
        "chars": len(value),
        "sha256": hashlib.sha256(value.encode("utf-8")).hexdigest(),
    }


def python_dists():
    rows = []
    for dist in importlib.metadata.distributions():
        files = dist.files or []
        located = [str(dist.locate_file(item)) for item in files]
        license_files = sorted(
            {path for path in located if LICENSE_NAME.search(os.path.basename(path)) and os.path.isfile(path)}
        )
        vendored = sorted(
            path for path in located if re.search(r"\.libs/|/\.dylibs/", path) and ".so" in os.path.basename(path)
        )
        rows.append(
            {
                "name": dist.metadata["Name"],
                "version": dist.version,
                "license": license_field(dist.metadata.get("License")),
                "license_expression": dist.metadata.get("License-Expression"),
                "license_file_metadata": dist.metadata.get_all("License-File", []),
                "license_classifiers": [c for c in dist.metadata.get_all("Classifier", []) if c.startswith("License")],
                "home_page": dist.metadata.get("Home-page"),
                "record_present": dist.files is not None,
                "record_file_count": len(files),
                "license_files": [file_entry(path) for path in license_files],
                "vendored_shared_libraries": [os.path.basename(path) for path in vendored],
            }
        )
    return sorted(rows, key=lambda row: row["name"].lower())


def walk_files(root):
    rows = []
    for current, dirs, names in os.walk(root):
        dirs.sort()
        for name in sorted(names):
            path = os.path.join(current, name)
            if os.path.isfile(path):
                rows.append(file_entry(path))
    return rows


def dpkg_status():
    rows, record = [], {}
    with open("/var/lib/dpkg/status", encoding="utf-8") as handle:
        for line in handle.read().split("\n") + [""]:
            if not line:
                if record:
                    rows.append(record)
                record = {}
                continue
            if line[0] in " \t":
                continue
            key, _, value = line.partition(":")
            if key in {"Package", "Version", "Architecture", "Source", "Maintainer", "Homepage", "Installed-Size", "Status"}:
                record[key] = value.strip()
    for row in rows:
        copyright_path = "/usr/share/doc/%s/copyright" % row["Package"]
        row["copyright"] = file_entry(copyright_path) if os.path.isfile(copyright_path) else None
    return sorted(rows, key=lambda row: row["Package"])


def apt_sources():
    rows = []
    for root in ("/etc/apt/sources.list", "/etc/apt/sources.list.d"):
        paths = [root] if os.path.isfile(root) else [os.path.join(root, n) for n in sorted(os.listdir(root))] if os.path.isdir(root) else []
        for path in paths:
            if os.path.isfile(path):
                with open(path, encoding="utf-8", errors="replace") as handle:
                    rows.append({"path": path, "sha256": sha256(path), "text": handle.read()})
    return rows


def tree_totals(roots):
    totals = {}
    for root in roots:
        count = size = 0
        for current, _, names in os.walk(root):
            for name in names:
                path = os.path.join(current, name)
                if os.path.isfile(path) and not os.path.islink(path):
                    count += 1
                    size += os.path.getsize(path)
        totals[root] = {"files": count, "bytes": size}
    return totals


NON_DISTRO = ("memgraph", "msodbcsql18", "openssl", "libssl3t64", "packages-microsoft-prod")


def package_files(package):
    listing = "/var/lib/dpkg/info/%s.list" % package
    if not os.path.isfile(listing):
        return None
    with open(listing, encoding="utf-8") as handle:
        paths = [line.rstrip("\n") for line in handle if line.strip()]
    files = [p for p in paths if os.path.isfile(p)]
    missing = [p for p in paths if not os.path.lexists(p)]
    return {
        "listed": len(paths),
        "present_files": len(files),
        "missing": missing,
        "listed_query_modules": sorted(p for p in files if "/query_modules/" in p),
        "templates": package_templates(package),
        "license_like": [file_entry(p) for p in files if LICENSE_NAME.search(os.path.basename(p))],
    }


def dpkg_excludes():
    root = "/etc/dpkg/dpkg.cfg.d"
    rows = []
    for name in sorted(os.listdir(root)) if os.path.isdir(root) else []:
        path = os.path.join(root, name)
        with open(path, encoding="utf-8", errors="replace") as handle:
            rows.append({"path": path, "sha256": sha256(path), "text": handle.read()})
    return rows


def dpkg_info_copyrights():
    root = "/var/lib/dpkg/info"
    return [file_entry(os.path.join(root, n)) for n in sorted(os.listdir(root)) if n.endswith(".copyright")]


def debconf_license_answers():
    """Debconf questions/answers mentioning EULA or license (image-build acceptance record)."""
    path = "/var/cache/debconf/config.dat"
    if not os.path.isfile(path):
        return None
    with open(path, encoding="utf-8", errors="replace") as handle:
        stanzas = handle.read().split("\n\n")
    return [s for s in stanzas if re.search(r"eula|licen", s, re.I)]


def package_templates(package):
    path = "/var/lib/dpkg/info/%s.templates" % package
    if not os.path.isfile(path):
        return None
    with open(path, encoding="utf-8", errors="replace") as handle:
        return {"path": path, "sha256": sha256(path), "text": handle.read()}


site = sorted({os.path.dirname(str(d._path)) for d in importlib.metadata.distributions()})
print(
    json.dumps(
        {
            "os_release": open("/etc/os-release", encoding="utf-8").read(),
            "memgraph_lib": walk_files("/usr/lib/memgraph"),
            "opt_files": walk_files("/opt") if os.path.isdir("/opt") else [],
            "share_doc_files": walk_files("/usr/share/doc"),
            "common_licenses": walk_files("/usr/share/common-licenses"),
            "debconf_license_answers": debconf_license_answers(),
            "dpkg": dpkg_status(),
            "dpkg_excludes": dpkg_excludes(),
            "dpkg_info_copyrights": dpkg_info_copyrights(),
            "non_distro_package_files": {p: package_files(p) for p in NON_DISTRO},
            "apt_sources": apt_sources(),
            "python_site_dirs": site,
            "python_dists": python_dists(),
            "tree_totals": tree_totals(["/usr/lib/memgraph", "/usr/share/doc", "/opt"] + site),
        },
        sort_keys=True,
    )
)
