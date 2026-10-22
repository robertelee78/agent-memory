"""Local metadata storage: exclusive writes, observed bytes, bounded lock recovery."""
from contextlib import contextmanager
from datetime import datetime, timezone
import os
from pathlib import Path
import socket
import tempfile
import uuid

from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError
from .lifecycle import time_value

try:
    import fcntl
except ImportError:
    fcntl = None


def paths(name):
    raw = os.environ.get("AGENT_MEMORY_CONFIG_HOME")
    root = Path(raw or str(Path(os.environ.get("XDG_CONFIG_HOME", str(Path.home() / ".config"))) / "agent-memory")).expanduser()
    if not root.is_absolute() or any(p.is_symlink() for p in (root, *root.parents)):
        raise ContractError("Registry config root must be absolute and use canonical directories")
    if root.exists() and not root.is_dir():
        raise ContractError("Registry config root is not a directory")
    registry, lock = root / (name + ".json"), root / ("." + name + ".lock")
    if registry.is_symlink() or lock.is_symlink():
        raise ContractError("Registry and lock must not be symlinks")
    return root, registry, lock


def now():
    return datetime.now(timezone.utc).isoformat()


@contextmanager
def exclusive(lock, operation_id, operation):
    lock.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    owner = {"operation_id": operation_id, "operation": operation, "host": socket.gethostname(), "pid": os.getpid(), "start": now()}
    try:
        fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError as exc:
        raise ContractError("Project registry is locked; inspect status/recover") from exc
    inode = os.fstat(fd).st_ino
    try:
        with os.fdopen(fd, "wb", closefd=False) as stream:
            stream.write(encoded(owner)); stream.flush(); os.fsync(fd)
        if fcntl is not None:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        yield
    finally:
        try:
            if not lock.is_symlink() and lock.exists() and lock.lstat().st_ino == inode and lock.read_bytes() == encoded(owner):
                lock.unlink()
        finally:
            os.close(fd)


def replace(registry, payload, observed):
    temporary = None
    try:
        with tempfile.NamedTemporaryFile(prefix="." + registry.stem + "-", dir=registry.parent, delete=False) as stream:
            temporary = Path(stream.name)
            os.chmod(temporary, 0o600)
            stream.write(payload); stream.flush(); os.fsync(stream.fileno())
        raw = registry.read_bytes() if registry.exists() else None
        if registry.is_symlink() or digest(raw) != observed:
            raise ContractError("Registry changed immediately before replacement; preserve the external edit")
        os.replace(temporary, registry)
        temporary = None
        directory = os.open(registry.parent, os.O_RDONLY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
        if registry.read_bytes() != payload:
            raise ContractError("Registry readback differs; inspect and replay the same operation")
    finally:
        if temporary is not None:
            temporary.unlink(missing_ok=True)


def recover(registry, lock, operations, prepared=None):
    if not lock.exists() or fcntl is None:
        raise ContractError("Safe stale-lock recovery requires an existing local lock and supported file locking; operator review otherwise")
    fd = os.open(lock, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    try:
        try:
            fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError as exc:
            raise ContractError("Lock held by another writer/recovery") from exc
        inode = os.fstat(fd).st_ino
        if lock.lstat().st_ino != inode:
            raise ContractError("Lock changed before inspection")
        with os.fdopen(fd, "rb", closefd=False) as stream:
            raw = stream.read()
        owner = load(raw)
        if (not isinstance(owner, dict) or set(owner) != {"operation_id", "operation", "host", "pid", "start"}
                or not uuid_valid(owner["operation_id"]) or owner["operation"] not in operations):
            raise ContractError("Unknown lock; preserve it for operator review")
        time_value(owner["start"])
        if owner["host"] != socket.gethostname() or type(owner["pid"]) is not int or owner["pid"] <= 0:
            raise ContractError("Foreign or malformed lock requires operator review")
        try:
            os.kill(owner["pid"], 0)
        except ProcessLookupError:
            pass
        except PermissionError as exc:
            raise ContractError("Cannot prove owner is gone") from exc
        else:
            raise ContractError("Lock owner is still alive")
        if prepared is None:
            return {"proposal": {"schema": "agent-memory/project-lock-recovery-v1", "operation_id": str(uuid.uuid4()),
                                 "registry_path": str(registry), "observed_lock_sha256": digest(raw)},
                    "lock_owner": owner, "writes_performed": False, "verified": False}
        prepared = prepared.get("proposal", prepared) if isinstance(prepared, dict) else prepared
        if (not isinstance(prepared, dict) or set(prepared) != {"schema", "operation_id", "registry_path", "observed_lock_sha256"}
                or prepared["schema"] != "agent-memory/project-lock-recovery-v1" or not uuid_valid(prepared["operation_id"])
                or prepared["registry_path"] != str(registry) or prepared["observed_lock_sha256"] != digest(raw)
                or lock.lstat().st_ino != inode or lock.read_bytes() != raw):
            raise ContractError("Stale or invalid lock-recovery proposal")
        lock.unlink()
        return {"operation_id": prepared["operation_id"], "writes_performed": True,
                "verified": not lock.exists() or lock.lstat().st_ino != inode}
    finally:
        os.close(fd)
