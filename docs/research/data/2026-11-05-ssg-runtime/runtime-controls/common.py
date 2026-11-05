"""Private, synthetic-only #255 research helpers; not a production AM module."""
from __future__ import annotations
import contextlib
import fcntl
import hashlib
import ipaddress
import json
import math
import os
from pathlib import Path
import platform
import subprocess
import time
import urllib.error
import urllib.parse
import urllib.request
import uuid

SSG_COMMIT = "9c4fd08c29f37bffe89ecdf75b069a6baeb8a5e0"
IMAGE = "docker.io/memgraph/memgraph-mage@sha256:e8e03640071c57de6a13f2f4c5f9ed1789797dc5e8e771bbe62203b20e1b8c27"
LABEL = "org.agent-memory.g1-runtime-controls"


def sha(path):
    h = hashlib.sha256()
    with Path(path).open("rb") as f:
        for chunk in iter(lambda: f.read(1024 * 1024), b""):
            h.update(chunk)
    return h.hexdigest()


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True, separators=(",", ":"), allow_nan=False).encode()).hexdigest()


def load(path):
    return json.loads(Path(path).read_text())


def fsync_dir(path):
    fd = os.open(path, os.O_RDONLY)
    try:
        os.fsync(fd)
    finally:
        os.close(fd)


def save(path, value):
    path = Path(path)
    temporary = path.with_name(path.name + ".next-" + uuid.uuid4().hex)
    with temporary.open("x", encoding="utf-8") as f:
        os.chmod(temporary, 0o600)
        json.dump(value, f, indent=2, sort_keys=True, allow_nan=False)
        f.write("\n")
        f.flush()
        os.fsync(f.fileno())
    os.replace(temporary, path)
    fsync_dir(path.parent)


@contextlib.contextmanager
def lock(root):
    with (Path(root) / "publication.lock").open("a") as f:
        fcntl.flock(f, fcntl.LOCK_EX)
        yield


def create_run(path, kind):
    path = Path(path).absolute()
    if path.is_symlink() or path.exists():
        raise ValueError("Run root must be new; use cleanup for an existing owned run")
    path.mkdir(parents=True, mode=0o700)
    os.chmod(path, 0o700)
    receipt = {"schema": "am255/runtime-controls-owner-v1", "run_id": uuid.uuid4().hex,
               "kind": kind, "root": str(path.resolve()), "uid": os.getuid(),
               "platform": platform.platform(), "resources": [], "production": False}
    save(path / "owner.json", receipt)
    return path


def owned_run(path):
    root = Path(path).absolute()
    if root.is_symlink() or root.resolve() != root:
        raise ValueError("Symlinked run roots are refused")
    r = load(root / "owner.json")
    if r.get("schema") != "am255/runtime-controls-owner-v1" or r["uid"] != os.getuid() or r["root"] != str(root):
        raise ValueError("Ownership receipt mismatch")
    if root.stat().st_mode & 0o077:
        raise ValueError("Run root is not private")
    return r


def command(argv, timeout=60, env=None, cwd=None, check=True):
    started = time.monotonic()
    p = subprocess.run([str(x) for x in argv], env=env, cwd=cwd, capture_output=True,
                       text=True, timeout=timeout)
    r = {"argv": [str(x) for x in argv], "exit": p.returncode, "stdout": p.stdout,
         "stderr": p.stderr, "seconds": time.monotonic() - started}
    if check and p.returncode:
        raise RuntimeError(json.dumps(r))
    return r


def bind_sources(ssg, destination):
    ssg = Path(ssg).resolve()
    revision = command(["git", "-C", ssg, "rev-parse", "HEAD"])["stdout"].strip()
    if revision != SSG_COMMIT:
        raise ValueError("SSG revision is not the authorized reference")
    paths = ["crates/ssg/src/" + p for p in ("build.rs", "embed.rs", "graph.rs", "workspace.rs",
             "config.rs", "source.rs", "error.rs", "query/mod.rs", "query/run.rs", "query/template.rs",
             "query/render.rs", "embed/cache.rs", "workspace/paths.rs", "workspace/load.rs")]
    paths += ["queries/class.toml", "queries/investigation.toml", "critic/system.md"]
    result = {"ssg_commit": revision, "ssg_sources": {p: sha(ssg / p) for p in paths},
              "controllers": {p.name: sha(p) for p in Path(__file__).parent.glob("*.py")}}
    checkout = Path(__file__).resolve().parents[5]
    result["am_commit"] = command(["git", "-C", checkout, "rev-parse", "HEAD"])["stdout"].strip()
    result["qualification_environment"] = {"python": platform.python_version(), "platform": platform.platform()}
    save(destination, result)
    return result


def api_base(value):
    u = urllib.parse.urlsplit(value)
    if u.scheme not in ("http", "https") or u.username or u.password or u.query or u.fragment:
        raise ValueError("Expected a credential-free API base URL")
    try:
        local = ipaddress.ip_address(u.hostname).is_loopback
    except ValueError:
        local = u.hostname == "localhost"
    if not local:
        raise ValueError("This research recipe sends synthetic data to loopback only")
    path = u.path.rstrip("/") or "/v1"
    return urllib.parse.urlunsplit((u.scheme, u.netloc, path, "", ""))


def key_from(path):
    if path is None:
        return None
    path = Path(path)
    if path.is_symlink() or not path.is_file() or path.stat().st_mode & 0o077 or path.stat().st_uid != os.getuid():
        raise ValueError("Credential file must be an owned, regular private 0600 file")
    key = path.read_text().strip()
    if not key or "\n" in key or "\r" in key:
        raise ValueError("Invalid credential file")
    return key


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def request(base, route, value=None, key=None, timeout=120, max_bytes=32 * 1024 * 1024):
    url = api_base(base) + route
    headers = {"Content-Type": "application/json"}
    if key:
        headers["Authorization"] = "Bearer " + key
    body = None if value is None else json.dumps(value, allow_nan=False).encode()
    req = urllib.request.Request(url, data=body, headers=headers)
    start = time.monotonic()
    try:
        response = urllib.request.build_opener(NoRedirect).open(req, timeout=timeout)
    except urllib.error.HTTPError as e:
        response = e
    with response:
        raw = response.read(max_bytes + 1)
        if len(raw) > max_bytes:
            raise ValueError("Endpoint response exceeds bounded bytes")
        try:
            content = json.loads(raw)
        except ValueError:
            content = {"non_json_bytes": len(raw), "sha256": hashlib.sha256(raw).hexdigest()}
        return {"status": response.code, "body": content, "seconds": time.monotonic() - start,
                "response_sha256": hashlib.sha256(raw).hexdigest(), "response_bytes": len(raw)}


def choose_model(metadata, explicit=None):
    if metadata["status"] != 200:
        if explicit:
            return explicit, "provider model identity not verifiable"
        raise ValueError("model-discovery-unavailable")
    rows = metadata["body"].get("data", [])
    ids = sorted(set(r["id"] for r in rows if isinstance(r, dict) and isinstance(r.get("id"), str) and r["id"]))
    if explicit:
        if explicit not in ids:
            raise ValueError("model-not-in-independent-metadata")
        return explicit, "metadata-alias-only; provider model identity not verifiable"
    if len(ids) != 1:
        raise ValueError("ambiguous-model-discovery" if ids else "empty-model-discovery")
    return ids[0], "metadata-alias-only; provider model identity not verifiable"


def validate(answer, model, count, dimension):
    if answer["status"] != 200:
        raise ValueError("embedding-http-" + str(answer["status"]))
    body = answer["body"]
    if body.get("model") != model:
        raise ValueError("response-model-mismatch")
    rows = body.get("data")
    if not isinstance(rows, list) or len(rows) != count:
        raise ValueError("response-count-mismatch")
    values = [None] * count
    for row in rows:
        i = row.get("index")
        if type(i) is not int or not 0 <= i < count or values[i] is not None:
            raise ValueError("response-index-coverage")
        v = row.get("embedding")
        if not isinstance(v, list) or len(v) != dimension:
            raise ValueError("response-dimension-mismatch")
        if any(type(x) not in (int, float) or not math.isfinite(x) for x in v):
            raise ValueError("response-nonfinite-or-nonnumeric")
        norm = math.sqrt(math.fsum(x * x for x in v))
        if not math.isfinite(norm) or norm == 0:
            raise ValueError("response-zero-or-nonfinite-norm")
        values[i] = [x / norm for x in v]
    return values


def embed(base, model, texts, dimension, key=None):
    a = request(base, "/embeddings", {"model": model, "input": texts, "encoding_format": "float"}, key)
    return validate(a, model, len(texts), dimension), a


def cosine(a, b):
    return math.fsum(x * y for x, y in zip(a, b))


def inventory(root):
    root = Path(root)
    return [{"path": str(p.relative_to(root)), "bytes": p.stat().st_size, "sha256": sha(p)}
            for p in sorted(root.rglob("*")) if p.is_file() and not p.is_symlink()
            and p.name != "artifact-inventory.json" and "credentials" not in p.parts]
