"""One invocation's exact validated bytes; never a persistent/caller-owned cache."""
from __future__ import annotations
from contextlib import contextmanager, ExitStack
import hashlib
import os
from pathlib import Path
import stat

from . import bank_contract as contract, bank_writes as writes, sharing_limits
from .developer_identity import digest, uuid_valid
from .frontmatter import ContractError
from .bank_capacity import BankCapacityError

MAX_FILES=sharing_limits.MAX_FILES
MAX_BYTES=sharing_limits.MAX_BANK


@contextmanager
def _parent_chain(bank, parts):
    descriptors=[]
    try:
        try:
            descriptors.append(os.open(bank, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW))
            for part in parts:
                descriptors.append(os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW,
                                           dir_fd=descriptors[-1]))
        except OSError as exc:
            raise ContractError('Canonical source or parent changed, became unsafe, or cannot be read; preserve it and retry') from exc
        yield descriptors
    finally:
        for fd in reversed(descriptors):
            os.close(fd)


def _check_parents(bank, parts, descriptors):
    try:
        for index,descriptor in enumerate(descriptors):
            current=(os.stat(bank,follow_symlinks=False) if index==0 else
                     os.stat(parts[index-1],dir_fd=descriptors[index-1],follow_symlinks=False))
            opened=os.fstat(descriptor)
            if (not stat.S_ISDIR(current.st_mode) or not stat.S_ISDIR(opened.st_mode)
                    or (current.st_dev,current.st_ino)!=(opened.st_dev,opened.st_ino)):
                raise ContractError('Canonical source parent changed during bounded observation')
    except OSError as exc:
        raise ContractError('Canonical source or parent changed, became unsafe, or cannot be read; preserve it and retry') from exc


def _read_pinned_source(bank, relative, limit, parts, descriptors):
    def signature(info):
        return (info.st_dev,info.st_ino,info.st_size,info.st_mtime_ns,
                info.st_ctime_ns,info.st_mode)
    try:
        fd=os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=descriptors[-1])
        try:
            # Keep ownership even if buffered-stream construction fails.
            with os.fdopen(fd,'rb',closefd=False) as stream:
                before=os.fstat(stream.fileno())
                if not stat.S_ISREG(before.st_mode):
                    raise ContractError('Canonical source is no longer ordinary')
                raw=stream.read(limit+1)
                after=os.fstat(stream.fileno())
                named=os.stat(parts[-1],dir_fd=descriptors[-1],follow_symlinks=False)
                if ((before.st_size <= limit and len(raw) != before.st_size)
                        or signature(before)!=signature(after)
                        or signature(after)!=signature(named)):
                    raise ContractError('Canonical source changed during bounded observation')
                _check_parents(bank,parts,descriptors)
                if before.st_size>sharing_limits.MAX_FILE:
                    raise BankCapacityError('bank-source-byte-capacity',relative,
                                            sharing_limits.MAX_FILE,before.st_size,measurement='exact')
                return raw
        finally:
            os.close(fd)
    except OSError as exc:
        raise ContractError('Canonical source or parent changed, became unsafe, or cannot be read; preserve it and retry') from exc


def read_source(bank, relative, limit):
    """Read bounded bytes and pin the named file and every opened parent."""
    parts=Path(relative).parts
    with _parent_chain(bank,parts[:-1]) as descriptors:
        return _read_pinned_source(bank,relative,limit,parts,descriptors)


def markers(bank,owner,transaction=None):
    for name,expected in (('.agent-memory.lock',owner),('.agent-memory-transaction.md',transaction)):
        target=bank/name
        try:mode=target.lstat().st_mode
        except FileNotFoundError:
            if expected is not None:raise ContractError('Owned bank lock disappeared; preserve work and retry with actual ownership')
            continue
        if expected is None:
            if name == '.agent-memory.lock':
                from . import local_registry
                # Raw recovery identity uses this same final marker guard.
                # No caller gains ownership; ordinary callers still refuse.
                local_registry.check_canonical_writer(target, writes.OPERATIONS)
                if not os.path.lexists(target):
                    continue
            raise ContractError('Bank is busy/interrupted; preserve the operation and retry after recovery')
        if not stat.S_ISREG(mode):raise ContractError('Owned bank marker is not an ordinary record')
        directory=os.open(bank,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
        try:
            fd=os.open(name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=directory)
            with os.fdopen(fd,'rb') as stream:
                before=os.fstat(stream.fileno())
                if not stat.S_ISREG(before.st_mode):raise ContractError('Owned marker type changed')
                raw=stream.read(len(expected)+1)
                after=os.fstat(stream.fileno())
                named=os.stat(name,dir_fd=directory,follow_symlinks=False)
            fields=('st_dev','st_ino','st_size','st_mtime_ns','st_ctime_ns','st_mode')
            if any(getattr(before,key)!=getattr(after,key) or
                   getattr(after,key)!=getattr(named,key) for key in fields):
                raise ContractError('Bank ownership changed during bounded observation')
            current=os.stat(bank,follow_symlinks=False);opened=os.fstat(directory)
            if (not stat.S_ISDIR(current.st_mode) or
                    (current.st_dev,current.st_ino)!=(opened.st_dev,opened.st_ino) or raw!=expected):
                raise ContractError('Bank ownership changed; preserve work and retry with actual ownership')
        finally:
            os.close(directory)


def scan(bank,*,read_sources,metrics=None,require_router=True):
    """Complete physical topology plus streaming canonical hashes, bounded memory."""
    physical={};hashes={};sources={};total=0
    def failure(exc):raise ContractError('Bank physical inventory is incomplete') from exc
    for directory,dirs,files in os.walk(bank,followlinks=False,onerror=failure):
        parent=Path(directory)
        directory_relative=parent.relative_to(bank).as_posix()
        prefix='' if directory_relative=='.' else directory_relative+'/'
        with ExitStack() as stack:
            descriptors=None
            parent_parts=() if directory_relative=='.' else Path(directory_relative).parts
            for name in [*dirs,*files]:
                path=parent/name;relative=prefix+name;mode=path.lstat().st_mode
                if stat.S_ISLNK(mode):raise ContractError('Bank paths must not be symlinks')
                physical[relative]='directory' if stat.S_ISDIR(mode) else 'file' if stat.S_ISREG(mode) else 'other'
                if len(physical)>MAX_FILES*2:raise ContractError('Complete physical bank inventory exceeds its finite budget')
                if path.suffix!='.md' or contract.runtime_path(relative):continue
                if not stat.S_ISREG(mode):
                    raise ContractError('Canonical Markdown source must be an ordinary file')
                # Include the remaining aggregate budget even if the source races.
                remaining=MAX_BYTES-total
                if descriptors is None:
                    descriptors=stack.enter_context(_parent_chain(bank,parent_parts))
                else:
                    _check_parents(bank,parent_parts,descriptors)
                raw=_read_pinned_source(bank,relative,min(sharing_limits.MAX_FILE,remaining),
                                        (*parent_parts,name),descriptors)
                if len(hashes)>=MAX_FILES:
                    raise BankCapacityError('bank-source-count-capacity',relative,MAX_FILES,len(hashes)+1)
                total+=len(raw)
                if total>MAX_BYTES:
                    raise BankCapacityError('bank-aggregate-byte-capacity',relative,MAX_BYTES,total)
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
    def __init__(self,root,bank,owner,physical,hashes,sources,checked,parsed,transaction=None):
        self.root,self.bank,self.owner=root,bank,owner
        self.transaction=transaction
        self.physical,self.hashes,self.sources=physical,hashes,sources
        self.checked,self.parsed=checked,parsed
        self.payload_bytes_read=sum(len(raw) for raw in sources.values())
        self.recovery_frontier = None
        self.payload_file_reads=len(hashes)

    def recheck(self,stage='recall-final-source-recheck'):
        writes.fault(stage)
        root,bank=writes.bank_path(self.root)
        if root!=self.root or bank!=self.bank:raise ContractError('Canonical repository/bank location changed')
        if self.recovery_frontier is not None:
            from . import recovery_frontier
            recovery_frontier.check(self.recovery_frontier, self.root, self.owner, self.transaction)
        markers(bank,self.owner,self.transaction)
        metrics={};physical,hashes,_=scan(bank,read_sources=False,metrics=metrics)
        if physical!=self.physical or hashes!=self.hashes:
            raise ContractError('Canonical sources/topology changed; reread before using recall')
        self.payload_bytes_read+=metrics['payload_bytes']
        self.payload_file_reads+=metrics['file_reads']
        markers(bank,self.owner,self.transaction)


def observe(repo,owner=None,transaction=None):
    root,bank=writes.bank_path(repo)
    from . import recovery_frontier
    frontier = recovery_frontier.observe(root, owner, transaction)
    if transaction is None and frontier['hold_sha256'] is not None:
        held = recovery_frontier.verified_hold(root, owner)
        transaction = held.marker
    markers(bank,owner,transaction)
    physical,hashes,sources=scan(bank,read_sources=True)
    physical_contract(physical)
    writes.fault('recall-after-source-read')
    parsed={}
    checked=contract.validate_files({path:raw.decode('utf-8') for path,raw in sources.items()},parsed_records=parsed)
    for path in checked['omitted']:
        target=bank/path
        if target.exists() or target.is_symlink():
            raise ContractError('An omitted context path is physically occupied; inspect it instead of inferring absence')
    observed=Observation(root,bank,owner,physical,hashes,sources,checked,parsed,transaction)
    observed.recovery_frontier = frontier
    observed.recheck('recall-initial-source-recheck')
    return observed
