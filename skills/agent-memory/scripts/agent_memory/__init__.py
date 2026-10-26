"""Portable standard-library implementation of agent-memory contracts."""
from pathlib import Path as _Path
import hashlib as _hashlib
import stat as _stat

_ENGINE_SOURCE_ROOT = _Path(__file__).resolve().parents[2]


def _engine_sources():
    """Small trusted-package generation, captured before validator imports.

    Failure disables optional validation reuse; it never disables raw Markdown
    operations. This is not a source fingerprint for any project memory bank.
    """
    files={};total=0
    for path in _ENGINE_SOURCE_ROOT.rglob('*'):
        if path.suffix not in {'.py','.json'}:continue
        info=path.lstat()
        if path.is_symlink() or not _stat.S_ISREG(info.st_mode):
            raise ValueError('Optional catalog requires ordinary executing package sources')
        if len(files)>=1024 or info.st_size>2*1024*1024:
            raise ValueError('Executing package generation exceeds its bounded observation')
        raw=path.read_bytes();total+=len(raw)
        if total>16*1024*1024:raise ValueError('Executing package generation exceeds its byte bound')
        files[path.relative_to(_ENGINE_SOURCE_ROOT).as_posix()]=[_stat.S_IMODE(info.st_mode),_hashlib.sha256(raw).hexdigest()]
    return files


try:
    _INITIAL_ENGINE_SOURCES = _engine_sources()
except (OSError,ValueError,UnicodeError):
    _INITIAL_ENGINE_SOURCES = None
