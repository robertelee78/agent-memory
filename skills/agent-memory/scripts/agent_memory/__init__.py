"""Portable standard-library implementation of agent-memory contracts."""
from pathlib import Path as _Path
import hashlib as _hashlib
import stat as _stat
import os as _os

_ENGINE_SOURCE_ROOT = _Path(__file__).resolve().parents[2]


def _engine_sources():
    """Small trusted-package generation, captured before validator imports.

    Failure disables optional validation reuse; it never disables raw Markdown
    operations. This is not a source fingerprint for any project memory bank.
    """
    files={};total=0;visited=0
    def identity(value):
        return (value.st_dev,value.st_ino,value.st_size,value.st_mode,value.st_mtime_ns,value.st_ctime_ns)
    def walk(directory,prefix,depth):
        nonlocal total,visited
        if depth>64:raise ValueError('Executing package traversal exceeds its bound')
        with _os.scandir(directory) as entries:
            for entry in entries:
                visited+=1
                if visited>4096:raise ValueError('Executing package traversal exceeds its bound')
                info=entry.stat(follow_symlinks=False);relative=prefix+entry.name
                if _stat.S_ISLNK(info.st_mode):raise ValueError('Linked executing package source')
                if _stat.S_ISDIR(info.st_mode):
                    child=_os.open(entry.name,_os.O_RDONLY|_os.O_DIRECTORY|_os.O_NOFOLLOW,dir_fd=directory)
                    try:walk(child,relative+'/',depth+1)
                    finally:_os.close(child)
                    continue
                if _Path(entry.name).suffix not in {'.py','.json'}:continue
                if not _stat.S_ISREG(info.st_mode) or len(files)>=1024 or info.st_size>2*1024*1024:
                    raise ValueError('Executing package generation exceeds its bounded observation')
                fd=_os.open(entry.name,_os.O_RDONLY|_os.O_NOFOLLOW|_os.O_NONBLOCK,dir_fd=directory)
                try:
                    before=_os.fstat(fd)
                    if not _stat.S_ISREG(before.st_mode):raise ValueError('Executing source is not ordinary')
                    hasher=_hashlib.sha256();size=0
                    while chunk:=_os.read(fd,65536):
                        size+=len(chunk);total+=len(chunk)
                        if size>2*1024*1024 or total>16*1024*1024:raise ValueError('Executing source byte bound')
                        hasher.update(chunk)
                    after=_os.fstat(fd)
                    if size!=before.st_size or identity(info)!=identity(before) or identity(before)!=identity(after):
                        raise ValueError('Executing source changed during observation')
                    files[relative]=[_stat.S_IMODE(before.st_mode),hasher.hexdigest()]
                finally:_os.close(fd)
    root=_os.open(_ENGINE_SOURCE_ROOT,_os.O_RDONLY|_os.O_DIRECTORY|_os.O_NOFOLLOW)
    try:walk(root,'',0)
    finally:_os.close(root)

    return files


try:
    _INITIAL_ENGINE_SOURCES = _engine_sources()
except (OSError,ValueError,UnicodeError,AttributeError):
    _INITIAL_ENGINE_SOURCES = None
