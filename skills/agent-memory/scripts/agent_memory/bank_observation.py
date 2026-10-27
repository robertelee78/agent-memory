"""One invocation's exact validated bytes; never a persistent/caller-owned cache."""
from __future__ import annotations
import hashlib
import os
from pathlib import Path
import stat

from . import bank_contract as contract, bank_writes as writes, sharing_limits
from .developer_identity import digest, uuid_valid
from .frontmatter import ContractError

MAX_FILES=65536
MAX_BYTES=64*1024*1024


def read_source(bank, relative, limit):
    """Pin each directory and refuse replacement links while reading bounded bytes."""
    descriptors=[]
    try:
        descriptors.append(os.open(bank, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW))
        parts=Path(relative).parts
        for part in parts[:-1]:
            descriptors.append(os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                                       dir_fd=descriptors[-1]))
        fd=os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=descriptors[-1])
        with os.fdopen(fd,'rb') as stream:
            before=os.fstat(stream.fileno())
            if not stat.S_ISREG(before.st_mode):
                raise ContractError('Canonical source is no longer ordinary')
            raw=stream.read(limit+1)
            after=os.fstat(stream.fileno())
            if (before.st_size <= limit and len(raw) != before.st_size) or (
                    before.st_dev,before.st_ino,before.st_size,before.st_mtime_ns,before.st_ctime_ns) != (
                    after.st_dev,after.st_ino,after.st_size,after.st_mtime_ns,after.st_ctime_ns):
                raise ContractError('Canonical source changed during bounded observation')
            return raw
    except OSError as exc:
        raise ContractError('Canonical source or parent changed, became unsafe, or cannot be read; preserve it and retry') from exc
    finally:
        for fd in reversed(descriptors):
            os.close(fd)


def markers(bank,owner):
    for name,expected in (('.agent-memory.lock',owner),('.agent-memory-transaction.md',None)):
        target=bank/name
        try:mode=target.lstat().st_mode
        except FileNotFoundError:
            if expected is not None:raise ContractError('Owned bank lock disappeared; preserve work and retry with actual ownership')
            continue
        if expected is None:
            raise ContractError('Bank is busy/interrupted; preserve the operation and retry after recovery')
        if not stat.S_ISREG(mode):raise ContractError('Owned bank marker is not an ordinary record')
        fd=os.open(target,os.O_RDONLY|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_NONBLOCK',0))
        with os.fdopen(fd,'rb') as stream:
            if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):raise ContractError('Owned marker type changed')
            raw=stream.read(4097)
        if len(raw)>4096 or raw!=expected:
            raise ContractError('Bank ownership changed; preserve work and retry with actual ownership')


def scan(bank,*,read_sources,metrics=None,require_router=True):
    """Complete physical topology plus streaming canonical hashes, bounded memory."""
    physical={};hashes={};sources={};total=0
    def failure(exc):raise ContractError('Bank physical inventory is incomplete') from exc
    for directory,dirs,files in os.walk(bank,followlinks=False,onerror=failure):
        parent=Path(directory)
        directory_relative=parent.relative_to(bank).as_posix()
        prefix='' if directory_relative=='.' else directory_relative+'/'
        for name in [*dirs,*files]:
            path=parent/name;relative=prefix+name;mode=path.lstat().st_mode
            if stat.S_ISLNK(mode):raise ContractError('Bank paths must not be symlinks')
            physical[relative]='directory' if stat.S_ISDIR(mode) else 'file' if stat.S_ISREG(mode) else 'other'
            if len(physical)>MAX_FILES*2:raise ContractError('Complete physical bank inventory exceeds its finite budget')
            if path.suffix!='.md' or contract.runtime_path(relative):continue
            if not stat.S_ISREG(mode):
                raise ContractError('Canonical Markdown source must be an ordinary file')
            if len(hashes)>=MAX_FILES:raise ContractError('Complete canonical bank exceeds its finite file budget')
            # The read limit includes the remaining aggregate budget: no large
            # source is loaded even when it races after the initial lstat.
            remaining=MAX_BYTES-total
            raw=read_source(bank,relative,min(sharing_limits.MAX_FILE,remaining))
            if len(raw)>sharing_limits.MAX_FILE:
                raise ContractError('Canonical source exceeds 16KiB; preserve it without truncation')
            total+=len(raw)
            if total>MAX_BYTES:raise ContractError('Complete canonical bank exceeds 64MiB; preserve it without truncation')
            hashes[relative]=hashlib.sha256(raw).hexdigest()
            if read_sources:sources[relative]=raw
    if require_router and 'MEMORY.md' not in hashes:
        raise ContractError('A complete canonical router is required')
    if metrics is not None:metrics.update(payload_bytes=total,file_reads=len(hashes))
    return physical,hashes,sources


def physical_contract(physical):
    contract.portable_aliases([*physical,'MEMORY.md','history','history/tombstones'])
    for directory in ('history','history/tombstones'):
        if directory in physical and physical[directory]!='directory':raise ContractError('Deletion history requires canonical directories')
    prefix='history/tombstones/'
    for path,kind in physical.items():
        if path.startswith(prefix) and (kind!='file' or '/' in path[len(prefix):] or not path.endswith('.md') or not uuid_valid(Path(path).stem)):
            raise ContractError('Deletion history permits only canonical UUID marker files')


class Observation:
    """Constructed internally from actual reads; retrieve accepts no such input."""
    def __init__(self,root,bank,owner,physical,hashes,sources,checked,parsed):
        self.root,self.bank,self.owner=root,bank,owner
        self.physical,self.hashes,self.sources=physical,hashes,sources
        self.checked,self.parsed=checked,parsed
        self.payload_bytes_read=sum(len(raw) for raw in sources.values())
        self.payload_file_reads=len(hashes)

    def recheck(self,stage='recall-final-source-recheck'):
        writes.fault(stage)
        root,bank=writes.bank_path(self.root)
        if root!=self.root or bank!=self.bank:raise ContractError('Canonical repository/bank location changed')
        markers(bank,self.owner)
        metrics={};physical,hashes,_=scan(bank,read_sources=False,metrics=metrics)
        if physical!=self.physical or hashes!=self.hashes:
            raise ContractError('Canonical sources/topology changed; reread before using recall')
        self.payload_bytes_read+=metrics['payload_bytes']
        self.payload_file_reads+=metrics['file_reads']
        markers(bank,self.owner)


def observe(repo,owner=None):
    root,bank=writes.bank_path(repo)
    markers(bank,owner)
    physical,hashes,sources=scan(bank,read_sources=True)
    physical_contract(physical)
    writes.fault('recall-after-source-read')
    parsed={}
    checked=contract.validate_files({path:raw.decode('utf-8') for path,raw in sources.items()},parsed_records=parsed)
    for path in checked['omitted']:
        target=bank/path
        if target.exists() or target.is_symlink():
            raise ContractError('An omitted context path is physically occupied; inspect it instead of inferring absence')
    observed=Observation(root,bank,owner,physical,hashes,sources,checked,parsed)
    observed.recheck('recall-initial-source-recheck')
    return observed
