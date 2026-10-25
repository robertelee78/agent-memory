"""Separate complete-bank/private-operation budgets from agent disclosure."""
from pathlib import Path

from .developer_identity import encoded, load
from .frontmatter import ContractError

MAX_FILES = 65536
MAX_BANK = 64 * 1024 * 1024
MAX_FILE = 16384
MAX_PRIVATE = 512 * 1024 * 1024
MAX_PAGE = 65536


def bank_size(files):
    if (not isinstance(files, dict) or len(files) > MAX_FILES
            or any(not isinstance(v, str) or len(v.encode()) > MAX_FILE for v in files.values())
            or sum(len(v.encode()) for v in files.values()) > MAX_BANK):
        raise ContractError('Complete bank exceeds 65,536 files/64MiB or 16KiB per source; preserve sources without truncation')


def private_bytes(value):
    raw = encoded(value)
    if len(raw) > MAX_PRIVATE:
        raise ContractError('Complete private operation exceeds 512MiB; preserve sources and recovery evidence')
    return raw


def read_bytes(path, limit=MAX_PRIVATE):
    with Path(path).open('rb') as stream:
        raw = stream.read(limit + 1)
    if len(raw) > limit:
        raise ContractError('Complete private input exceeds its explicit encoded budget; no truncation')
    return raw


def read_json(path, limit=MAX_PRIVATE):
    return load(read_bytes(path, limit))


def write_private(path, value, repo):
    from . import local_registry as storage, trust_paths
    from .developer_identity import digest
    supplied = Path(path).expanduser()
    if not supplied.is_absolute() or '..' in supplied.parts:
        raise ContractError('Select an absolute owner-only output outside the repository')
    # Reuse the existing checked locator, including only fixed Darwin aliases.
    parent = trust_paths.git_pointer(Path('/'), str(supplied.parent))
    trust_paths.state(parent)
    target = parent / supplied.name
    trust_paths.record(target)
    if target.is_relative_to(Path(repo).resolve()):
        raise ContractError('Private review output must stay outside the repository')
    raw = private_bytes(value)
    if target.exists():
        if target.stat().st_mode & 0o077 or read_bytes(target) != raw:
            raise ContractError('Preserve existing private output; choose a new path')
    else:
        storage.replace(target, raw, None)
    return {'schema':'agent-memory/private-review-artifact-v2','path':str(target),
            'sha256':digest(raw),'bytes':len(raw),'canonical_writes_performed':False}
