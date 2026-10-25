"""One invocation's exact validated bytes; never a persistent/caller-owned cache."""
from __future__ import annotations
import hashlib
import os
from pathlib import Path
import stat

from . import bank_contract as contract, bank_writes as writes
from .developer_identity import digest, uuid_valid
from .frontmatter import ContractError

MAX_FILES=65536
MAX_BYTES=64*1024*1024
CHUNK=65536


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


def scan(bank,*,read_sources):
    """Complete physical topology plus streaming canonical hashes, bounded memory."""
    physical={};hashes={};sources={};total=0
    def failure(exc):raise ContractError('Bank physical inventory is incomplete') from exc
    for directory,dirs,files in os.walk(bank,followlinks=False,onerror=failure):
        parent=Path(directory)
        for name in [*dirs,*files]:
            path=parent/name;relative=path.relative_to(bank).as_posix();mode=path.lstat().st_mode
            if stat.S_ISLNK(mode):raise ContractError('Bank paths must not be symlinks')
            physical[relative]='directory' if stat.S_ISDIR(mode) else 'file' if stat.S_ISREG(mode) else 'other'
            if len(physical)>MAX_FILES*2:raise ContractError('Complete physical bank inventory exceeds its finite budget')
            if not stat.S_ISREG(mode) or path.suffix!='.md' or contract.runtime_path(relative):continue
            if len(hashes)>=MAX_FILES:raise ContractError('Complete canonical bank exceeds its finite file budget')
            hasher=hashlib.sha256();parts=[]
            # Refuse a replaced final symlink rather than follow it during read.
            fd=os.open(path,os.O_RDONLY|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_NONBLOCK',0))
            with os.fdopen(fd,'rb') as stream:
                if not stat.S_ISREG(os.fstat(stream.fileno()).st_mode):raise ContractError('Canonical source is no longer ordinary')
                while chunk:=stream.read(CHUNK):
                    total+=len(chunk)
                    if total>MAX_BYTES:raise ContractError('Complete canonical bank exceeds 64MiB; preserve it without truncation')
                    hasher.update(chunk)
                    if read_sources:parts.append(chunk)
            hashes[relative]=hasher.hexdigest()
            if read_sources:sources[relative]=b''.join(parts)
    if 'MEMORY.md' not in hashes:
        raise ContractError('A complete canonical router is required')
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
        physical,hashes,_=scan(bank,read_sources=False)
        if physical!=self.physical or hashes!=self.hashes:
            raise ContractError('Canonical sources/topology changed; reread before using recall')
        self.payload_bytes_read+=sum(len(raw) for raw in self.sources.values())
        self.payload_file_reads+=len(hashes)
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
