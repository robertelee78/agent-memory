"""Local metadata storage: exclusive writes, observed bytes, bounded lock recovery."""
from contextlib import contextmanager
from datetime import datetime, timezone
import os
from pathlib import Path
import socket
import stat
import tempfile
import uuid

from .developer_identity import digest, encoded, load, uuid_valid
from .frontmatter import ContractError
from .lifecycle import time_value

try:
    import fcntl
except ImportError:
    fcntl = None


class CanonicalWriterBusy(ContractError):
    """A pinned, known local canonical writer still holds its kernel lock."""


class CanonicalWriterInitializing(ContractError):
    """A sampled empty publication slot; never owner or source authority."""


def check_canonical_writer(lock, operations):
    """Refuse a marker, distinguishing bounded waits from interrupted ownership.

    Complete owners require actual kernel contention. An exact empty private
    slot permits only bounded initialization retry, not a known-owner claim.
    Observers must rediscover all authority after either condition clears.
    """
    fields = ('st_dev', 'st_ino', 'st_size', 'st_mtime_ns', 'st_ctime_ns', 'st_mode', 'st_uid', 'st_nlink')
    identity = ('st_dev', 'st_ino', 'st_mode', 'st_uid', 'st_nlink')
    def same(infos, keys=fields):
        return all(all(getattr(infos[0], key) == getattr(info, key) for key in keys) for info in infos[1:])
    def safe(info):
        return (stat.S_ISREG(info.st_mode) and info.st_uid == os.geteuid()
                and info.st_nlink == 1 and not info.st_mode & 0o077 and info.st_size <= 4096)
    try:
        named = lock.lstat()
    except FileNotFoundError:
        return
    except OSError as exc:
        raise ContractError('Canonical writer marker cannot be safely inspected') from exc
    if fcntl is None or not safe(named):
        raise ContractError('Unknown or unsafe canonical writer marker; preserve for inspection')
    try:
        directory = os.open(lock.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    except OSError as exc:
        raise ContractError('Canonical writer bank cannot be safely pinned') from exc
    try:
        opened = os.fstat(directory)
        def current_name():
            # Absence means fresh rediscovery only at this exact pinned bank.
            # A replaced/missing bank is not a normal writer release.
            parent = lock.parent.lstat()
            if (not stat.S_ISDIR(parent.st_mode)
                    or (parent.st_dev, parent.st_ino) != (opened.st_dev, opened.st_ino)):
                raise ContractError('Canonical writer bank changed; preserve for inspection')
            try:
                return os.stat(lock.name, dir_fd=directory, follow_symlinks=False)
            except FileNotFoundError:
                return None
        try:
            fd = os.open(lock.name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=directory)
        except FileNotFoundError:
            if current_name() is None:
                return
            raise ContractError('Canonical writer marker changed before opening')
        try:
            before = os.fstat(fd)
            current = current_name()
            if current is None:
                return
            publishing = (named.st_size == 0 and safe(before) and safe(current)
                          and same((named, before, current), identity)
                          and (before.st_size == 0 or same((before, current))))
            if not same((named, before, current)) and not publishing:
                if current_name() is None:
                    return
                raise ContractError('Canonical writer marker changed before observation')
            if before.st_size == 0:
                # Never flock an unpublished slot: the writer may still be
                # between O_EXCL and its first EX flock. Probing SH here could
                # abort that writer. Empty is NOT proved live ownership, and
                # source callers still refuse when their existing budget ends.
                after = os.fstat(fd)
                current = current_name()
                if current is None:
                    return
                if (not safe(after) or not safe(current)
                        or not same((before, after, current), identity)
                        or (after.st_size != 0 and not same((after, current)))):
                    if current_name() is None:
                        return
                    raise ContractError('Canonical writer initialization slot changed unsafely')
                raise CanonicalWriterInitializing('Canonical writer owner is unpublished; bounded retry without authority')
            with os.fdopen(fd, 'rb', closefd=False) as stream:
                raw = stream.read(4097)
            after = os.fstat(fd)
            current = current_name()
            if current is None:
                return
            if len(raw) > 4096 or not same((before, after, current)):
                if current_name() is None:
                    return
                raise ContractError('Canonical writer marker changed; preserve for inspection')
            try:
                owner = load(raw)
            except (ValueError, UnicodeError) as exc:
                raise ContractError('Malformed canonical writer marker; preserve for inspection') from exc
            if (not isinstance(owner, dict)
                    or set(owner) != {'operation_id', 'operation', 'host', 'pid', 'start'}
                    or not uuid_valid(owner['operation_id']) or not isinstance(owner['operation'], str)
                    or not owner['operation'].startswith('bank-') or owner['operation'][5:] not in operations
                    or owner['host'] != socket.gethostname() or type(owner['pid']) is not int or owner['pid'] <= 0
                    or encoded(owner) != raw):
                raise ContractError('Unknown canonical writer owner; preserve for inspection')
            time_value(owner['start'])
            busy = False
            try:
                fcntl.flock(fd, fcntl.LOCK_SH | fcntl.LOCK_NB)
            except BlockingIOError:
                busy = True
            # Release unlinks before closing the writer descriptor. Recheck
            # BOTH flock results, including successful SH on an unlinked inode.
            inspected = os.fstat(fd)
            current = current_name()
            if current is None:
                return
            if not same((after, inspected, current)):
                if current_name() is None:
                    return
                raise ContractError('Canonical writer marker changed during contention observation')
            if busy:
                raise CanonicalWriterBusy('Canonical writer busy; unwind and rediscover before private source work') from None
            raise ContractError('Canonical writer marker is not actively held; inspect/recover before private source work')
        finally:
            os.close(fd)
    except OSError as exc:
        raise ContractError('Canonical writer observation became unsafe; preserve for inspection') from exc
    finally:
        os.close(directory)


def private_mkdir(path):
    """Create each missing directory owner-only, without changing existing modes."""
    path = Path(path)
    if not path.is_absolute() or '..' in path.parts:
        raise ContractError('Private state directory must be an absolute canonical path')
    flags = os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW
    descriptors = []
    try:
        descriptors.append(os.open(path.anchor, flags))
        for part in path.parts[1:]:
            parent = descriptors[-1]
            try:
                child = os.open(part, flags, dir_fd=parent)
            except FileNotFoundError:
                try:
                    os.mkdir(part, 0o700, dir_fd=parent)
                except FileExistsError:
                    pass
                child = os.open(part, flags, dir_fd=parent)
            descriptors.append(child)
        # Check every opened directory is still the entry reached from its
        # pinned parent. Never chmod or replace an existing directory.
        for i, part in enumerate(path.parts[1:]):
            opened = os.fstat(descriptors[i + 1])
            named = os.stat(part, dir_fd=descriptors[i], follow_symlinks=False)
            if (not stat.S_ISDIR(named.st_mode)
                    or (opened.st_dev, opened.st_ino) != (named.st_dev, named.st_ino)):
                raise ContractError('Private state path changed while creating directories')
    except OSError as exc:
        raise ContractError('Cannot safely create private state directories; preserve existing paths') from exc
    finally:
        for fd in reversed(descriptors):
            os.close(fd)


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


def _discard_created_empty_slot(lock, fd, created):
    """Only a live O_EXCL creator may discard its own failed first-flock slot."""
    directory = os.open(lock.parent, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
    try:
        opened_parent, parent = os.fstat(directory), lock.parent.lstat()
        if (not stat.S_ISDIR(parent.st_mode)
                or (opened_parent.st_dev, opened_parent.st_ino) != (parent.st_dev, parent.st_ino)):
            raise ContractError('Created metadata slot parent changed; preserve the marker')
        opened = os.fstat(fd)
        try:
            named = os.stat(lock.name, dir_fd=directory, follow_symlinks=False)
        except FileNotFoundError:
            return
        fields = ('st_dev', 'st_ino', 'st_uid', 'st_gid', 'st_mode', 'st_nlink', 'st_size', 'st_mtime_ns', 'st_ctime_ns')
        if (created.st_size != 0 or created.st_nlink != 1 or created.st_uid != os.geteuid()
                or not stat.S_ISREG(created.st_mode) or created.st_mode & 0o077
                or any(getattr(created, key) != getattr(info, key)
                       for info in (opened, named) for key in fields)):
            return
        os.unlink(lock.name, dir_fd=directory)
    finally:
        os.close(directory)


@contextmanager
def exclusive(lock, operation_id, operation):
    lock.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
    owner = {"operation_id": operation_id, "operation": operation, "host": socket.gethostname(), "pid": os.getpid(), "start": now()}
    payload = encoded(owner)
    try:
        fd = os.open(lock, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
    except FileExistsError as exc:
        raise ContractError("Memory metadata is locked; inspect status/recover") from exc
    created = os.fstat(fd)
    inode = created.st_ino
    try:
        # Complete owner bytes appear only under EX ownership. Recovery can
        # still probe the initial empty slot before this first flock.
        if fcntl is not None:
            try:
                fcntl.flock(fd, fcntl.LOCK_EX | fcntl.LOCK_NB)
            except OSError as exc:
                try:
                    _discard_created_empty_slot(lock, fd, created)
                except OSError as cleanup_error:
                    raise ContractError('Metadata ownership acquisition failed; empty-slot cleanup became unsafe') from cleanup_error
                if isinstance(exc, BlockingIOError):
                    raise ContractError('Metadata ownership acquisition was contended before publication; retry without adopting the slot') from exc
                raise ContractError('Metadata ownership acquisition failed before publication; retry without adopting the slot') from exc
        try:
            written = os.write(fd, payload)
        except OSError as exc:
            raise ContractError('Metadata owner publication failed; preserve the marker for inspection') from exc
        if written != len(payload):
            raise ContractError('Incomplete metadata owner publication; preserve the marker for inspection')
        os.fsync(fd)
        yield payload
    finally:
        try:
            if not lock.is_symlink() and lock.exists() and lock.lstat().st_ino == inode and lock.read_bytes() == payload:
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


def recover(registry, lock, operations, prepared=None, *, schema="agent-memory/project-lock-recovery-v1"):
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
            return {"proposal": {"schema": schema, "operation_id": str(uuid.uuid4()),
                                 "registry_path": str(registry), "observed_lock_sha256": digest(raw)},
                    "lock_owner": owner, "writes_performed": False, "verified": False}
        prepared = prepared.get("proposal", prepared) if isinstance(prepared, dict) else prepared
        if (not isinstance(prepared, dict) or set(prepared) != {"schema", "operation_id", "registry_path", "observed_lock_sha256"}
                or prepared["schema"] != schema or not uuid_valid(prepared["operation_id"])
                or prepared["registry_path"] != str(registry) or prepared["observed_lock_sha256"] != digest(raw)
                or lock.lstat().st_ino != inode or lock.read_bytes() != raw):
            raise ContractError("Stale or invalid lock-recovery proposal")
        lock.unlink()
        return {"operation_id": prepared["operation_id"], "writes_performed": True,
                "verified": not lock.exists() or lock.lstat().st_ino != inode}
    finally:
        os.close(fd)
