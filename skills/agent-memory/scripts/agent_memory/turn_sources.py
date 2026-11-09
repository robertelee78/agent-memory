"""Private append-only source observations; coverage does not establish truth."""
from __future__ import annotations
from contextlib import contextmanager
from contextvars import ContextVar
from functools import wraps
import base64
import fcntl
import hashlib
import json
import os
from pathlib import Path
import stat
import uuid
from . import bank_writes, scope_binding, trust_paths
from .frontmatter import ContractError

MAX_SOURCES=128
MAX_TURNS=4096
MAX_SOURCE=4096
MAX_COMPLETION=65536
HARNESSES={'codex','claude','opencode','omp'}
MAX_INTENT=8192
INTENT_SCHEMA='agent-memory/hold-source-enrollment-v1'
UNIDENTIFIED_SCHEMA='agent-memory/unidentified-private-input-v1'
MAX_RETAINED_INPUT=131072
_barriers=ContextVar('agent_memory_source_barriers',default={})
_read_only_locks=ContextVar('agent_memory_read_only_source_locks',default=False)


@contextmanager
def read_only_locks():
    """Do not create source synchronization state during observation."""
    token=_read_only_locks.set(True)
    try:yield
    finally:_read_only_locks.reset(token)


class VerifiedReceipt(dict):
    pass


class SourceTransportBusy(ContractError):
    """A nonblocking source lock was unavailable before its protected work."""


def encoded(value):
    return json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()


def bounded(value, maximum=512):
    if not isinstance(value,str) or not value or len(value.encode())>maximum or any(ord(c)<32 for c in value):
        raise ContractError('Invalid bounded source identifier')
    return value


def private_mkdir(path):
    trust_paths.inspect(path)
    missing=[];node=path
    while not node.exists():missing.append(node);node=node.parent
    for node in reversed(missing):
        try:node.mkdir(mode=0o700)
        except FileExistsError:pass
        trust_paths.state(node);sync(node.parent);sync(node)
    trust_paths.state(path)



@contextmanager
def private_lock(path, *, shared=False, existing_only=False):
    existing_only=existing_only or _read_only_locks.get()
    parent_fd=None
    if existing_only:
        trust_paths.state(path.parent)
        parent_fd=os.open(path.parent,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
        try:
            parent_info=os.fstat(parent_fd)
            named_parent=path.parent.lstat()
            if (parent_info.st_dev,parent_info.st_ino)!=(named_parent.st_dev,named_parent.st_ino):
                raise ContractError('Private source lock parent changed')
            try:
                fd=os.open(path.name,os.O_RDWR|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=parent_fd)
            except FileNotFoundError:
                raise ContractError('Private source lock is unavailable; retain pending evidence') from None
        except BaseException:
            os.close(parent_fd)
            raise
    else:
        private_mkdir(path.parent)
        fd=os.open(path,os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW|os.O_NONBLOCK,0o600)
    try:
        info=os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.geteuid() or info.st_nlink!=1 or info.st_mode&0o077:
            raise ContractError('Unsafe private source lock')
        try:fcntl.flock(fd,(fcntl.LOCK_SH if shared else fcntl.LOCK_EX)|fcntl.LOCK_NB)
        except BlockingIOError:raise SourceTransportBusy('Source transport busy; preserve pending work and retry') from None
        named=path.lstat()
        if (info.st_dev,info.st_ino)!=(named.st_dev,named.st_ino):
            raise ContractError('Private source lock changed; retain pending evidence')
        if parent_fd is not None:
            named_parent=path.parent.lstat()
            if (parent_info.st_dev,parent_info.st_ino)!=(named_parent.st_dev,named_parent.st_ino):
                raise ContractError('Private source lock parent changed; retain pending evidence')
        yield
        named=path.lstat()
        if (info.st_dev,info.st_ino)!=(named.st_dev,named.st_ino):
            raise ContractError('Private source lock changed; retain pending evidence')
        if parent_fd is not None:
            named_parent=path.parent.lstat()
            if (parent_info.st_dev,parent_info.st_ino)!=(named_parent.st_dev,named_parent.st_ino):
                raise ContractError('Private source lock parent changed; retain pending evidence')
    finally:
        os.close(fd)
        if parent_fd is not None:os.close(parent_fd)


def check_source_compatibility(base):
    """Understand shared source metadata, never infer a native drain proof."""
    path=base/'turn-sources-compat-v1.json'
    try:value=read(path,MAX_SOURCE)
    except FileNotFoundError:return
    if value not in ({'schema':'agent-memory/turn-sources-compat-v1',
                      'minimum_reader':capability,'harnesses':['omp']}
                     for capability in ('omp-harness-v1','omp-stored-source-v2')):
        raise ContractError('Unknown or malformed shared source compatibility marker; preserve pending evidence')


@contextmanager
def source_barrier(repo, *, exclusive=False, existing_only=False):
    """First source lock family; reentrant reads cannot upgrade to writers."""
    from . import layout_recovery_state
    location=scope_binding._observe_enrollment_location(repo)
    root,bank,recovery_base,_=(layout_recovery_state.paths(repo) if location is None else location)
    base=recovery_base.parent
    check_source_compatibility(base)
    known=_barriers.get()
    if str(base) in known:
        if exclusive and not known[str(base)]:raise ContractError('Cannot upgrade an active shared source barrier')
        yield
        check_source_compatibility(base)
        return
    with private_lock(base/'source-barrier.lock',shared=not exclusive,existing_only=existing_only):
        check_source_compatibility(base)
        token=_barriers.set({**known,str(base):exclusive})
        try:
            yield
            check_source_compatibility(base)
        finally:_barriers.reset(token)


def source_operation(function):
    @wraps(function)
    def guarded(repo,*args,**kwargs):
        with source_barrier(repo):
            return function(repo,*args,**kwargs)
    return guarded

def _namespace(repo,harness,session,lifecycle,context,create=False):
    if harness not in HARNESSES:raise ContractError('Unsupported source harness')
    bounded(session);bounded(lifecycle)
    private=type(context) is scope_binding.PrivateEnrollmentContext
    root,bank=(context.root,context.bank) if private else bank_writes.bank_path(repo)
    project_id=context.project_id if private else context['project_id']
    operations,_=bank_writes.state_paths(root,bank,str(uuid.uuid4()))
    base=operations.parent/'turn-sources-v2'
    key=hashlib.sha256(encoded([str(root),harness,session,lifecycle])).hexdigest()
    directory=base/'turns'/key
    trust_paths.inspect(directory)
    if create:
        if private:scope_binding.recheck_private_enrollment(context)
        private_mkdir(directory)
    return root,context,base,directory,{'harness':harness,'session':session,'lifecycle':lifecycle,'project_id':project_id}


def namespace(repo,harness,session,lifecycle,create=False):
    """Ordinary assessment authority; never accepts a held-enrollment context."""
    return _namespace(repo,harness,session,lifecycle,scope_binding.require(repo),create)


def private_namespace(repo,harness,session,lifecycle,create=False):
    """Private pending inventory/enrollment only, including a verified hold."""
    return _namespace(repo,harness,session,lifecycle,scope_binding.private_enrollment_context(repo),create)


def _enrollment_namespace(repo,harness,session,lifecycle,context):
    """Private locator reuse only while the exact sealed read operation is live."""
    context=scope_binding._enrollment_read_context(repo,context)
    value=_namespace(repo,harness,session,lifecycle,context)
    if str(value[2].parent) not in _barriers.get():
        raise ContractError('Private enrollment locator reuse requires the active source barrier')
    return value


def read_bytes(path,maximum):
    from .layout_recovery_state import signature
    path=Path(path);trust_paths.inspect(path)
    parent=os.open(path.parent,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    try:
        parent_info=os.fstat(parent)
        fd=os.open(path.name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=parent)
        try:
            before=os.fstat(fd)
            if not stat.S_ISREG(before.st_mode) or before.st_uid!=os.geteuid() or before.st_nlink!=1 or before.st_mode&0o077 or before.st_size>maximum:
                raise ContractError('Unsafe source state; preserve it for inspection')
            with os.fdopen(fd,'rb',closefd=False) as stream:raw=stream.read(maximum+1)
            after=os.fstat(fd);named=os.stat(path.name,dir_fd=parent,follow_symlinks=False)
            current_parent=path.parent.lstat()
            if (len(raw)!=before.st_size or signature(before)!=signature(after) or signature(after)!=signature(named)
                    or (parent_info.st_dev,parent_info.st_ino)!=(current_parent.st_dev,current_parent.st_ino)):
                raise ContractError('Source state or its parent changed; retain pending evidence')
        finally:os.close(fd)
    finally:os.close(parent)
    return raw


def read(path,maximum):
    from .developer_identity import load
    return load(read_bytes(path,maximum))


def sync(directory):
    fd=os.open(directory,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW)
    try:os.fsync(fd)
    finally:os.close(fd)


def exclusive(path,value,maximum):
    data=encoded(value)
    if len(data)>maximum:raise ContractError('Encoded source state exceeds bound')
    fd=os.open(path,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600)
    try:
        with os.fdopen(fd,'wb',closefd=False) as f:f.write(data);f.flush();os.fsync(f.fileno())
    finally:os.close(fd)
    sync(path.parent)
    if read(path,maximum)!=value:raise ContractError('Source state durable readback failed')


def ref(directory,identity):
    return {'schema':'agent-memory/turn-reference-v2',**identity,'turn':directory.name}


def validate_reference(value,root,project_id):
    fields={'schema','harness','session','lifecycle','project_id','turn'}
    if not isinstance(value,dict) or set(value)!=fields or not all(isinstance(v,str) for v in value.values()) or value['schema']!='agent-memory/turn-reference-v2' or value['project_id']!=project_id or value['harness'] not in HARNESSES:raise ContractError('Pending index reference is malformed or foreign')
    bounded(value['session']);bounded(value['lifecycle'])
    expected=hashlib.sha256(encoded([str(root),value['harness'],value['session'],value['lifecycle']])).hexdigest()
    if value['turn']!=expected:raise ContractError('Pending index reference names a different namespace')
    return value


def index(base,directory,identity,root,context=None):
    """Fixed slots bound capacity; registration never takes completion lock."""
    target=base/'pending';trust_paths.inspect(target)
    if context is not None:scope_binding.recheck_private_enrollment(context)
    private_mkdir(target)
    value=ref(directory,identity)
    for attempt in range(2):
        free=None;matching=None
        for i in range(MAX_TURNS):
            path=target/(str(i)+'.json')
            try:old=read(path,MAX_SOURCE)
            except FileNotFoundError:
                if free is None:free=path
                continue
            except (ValueError,OSError):
                raise ContractError('Pending turn index has an unverifiable reservation; preserve it and inspect backlog') from None
            validate_reference(old,root,identity['project_id'])
            if old==value:matching=path
        if matching is not None:
            try:
                if read(matching,MAX_SOURCE)==value:return matching
            except FileNotFoundError:continue
            raise ContractError('Pending turn index changed; preserve sources and retry input')
        if free is None:raise ContractError('Pending turn index full; inspect retained backlog before retrying input')
        try:
            if context is not None:scope_binding.recheck_private_enrollment(context)
            exclusive(free,value,MAX_SOURCE)
            if context is not None:scope_binding.recheck_private_enrollment(context)
            return free
        except FileExistsError:continue
    raise ContractError('Pending turn index changed concurrently; preserve sources and retry input')


def fault(directory,code):
    try:exclusive(directory/'fault.json',{'schema':'agent-memory/turn-fault-v2','reason':code},MAX_SOURCE)
    except (OSError,ValueError):pass


def validate_source(v,identity):
    keys={'schema','observation_id','source_sha256','native_source_id','source_reference','control_origin_hint',*identity}
    if not isinstance(v,dict) or set(v)!=keys or v.get('schema')!='agent-memory/turn-source-v2' or any(v.get(k)!=x for k,x in identity.items()):
        raise ContractError('Source observation differs from its namespace')
    if type(v['control_origin_hint']) is not bool:raise ContractError('Invalid control-origin qualification')
    if str(uuid.UUID(v['observation_id']))!=v['observation_id']:raise ContractError('Invalid observation UUID')
    if len(v['source_sha256'])!=64 or any(c not in '0123456789abcdef' for c in v['source_sha256']):raise ContractError('Invalid source digest')
    if v['native_source_id'] is not None:bounded(v['native_source_id'])
    if v['source_reference'] is not None:bounded(v['source_reference'],2048)
    return v


def enrollment_value(value,identity,event):
    fields={'schema','enrollment_id','project_id','harness','session','lifecycle','native_event_id',
            'source_sha256','source_reference','observation_id','context_digest','state'}
    if (not isinstance(value,dict) or set(value)!=fields or value['schema']!=INTENT_SCHEMA
            or any(value.get(key)!=item for key,item in identity.items())
            or value['native_event_id']!=event or value['state'] not in {'intent','source-written','registered','unverified'}):
        raise ContractError('Held enrollment intent is malformed, foreign or conflicting; preserve it')
    for key in ('enrollment_id','observation_id'):
        if str(uuid.UUID(value[key]))!=value[key] or uuid.UUID(value[key]).version!=4:
            raise ContractError('Held enrollment intent has an invalid observation identity')
    for key in ('source_sha256','context_digest'):
        if not isinstance(value[key],str) or len(value[key])!=64 or any(c not in '0123456789abcdef' for c in value[key]):
            raise ContractError('Held enrollment intent has invalid private pins')
    bounded(event)
    if value['source_reference'] is not None:bounded(value['source_reference'],2048)
    return value


def enrollment_state(path,intent,state,context=None):
    from . import local_registry
    from .developer_identity import load
    if context is not None:scope_binding.recheck_private_enrollment(context)
    current=read_bytes(path,MAX_INTENT)
    if load(current)!=intent:raise ContractError('Enrollment intent changed; preserve original observation')
    updated={**intent,'state':state}
    local_registry.replace(path,encoded(updated),hashlib.sha256(current).hexdigest())
    if read(path,MAX_INTENT)!=updated:raise ContractError('Enrollment intent readback failed')
    return updated


def held_register(root,context,base,directory,identity,value):
    """Journal one real native event before publishing any shared source slot."""
    event=value['native_source_id']
    if event is None:raise ContractError('Recovery-pending enrollment requires an unambiguous actual native event identity')
    bounded(event)
    key=hashlib.sha256(encoded([identity,event])).hexdigest()
    intent_path=directory/'enrollments'/(key+'.json')
    scope_binding.recheck_private_enrollment(context)
    with private_lock(directory/'enrollment.lock'):
        scope_binding.recheck_private_enrollment(context)
        private_mkdir(intent_path.parent)
        try:
            intent=enrollment_value(read(intent_path,MAX_INTENT),identity,event)
            if (intent['source_sha256']!=value['source_sha256'] or intent['source_reference']!=value['source_reference']
                    or intent['context_digest']!=context.context_digest):
                raise ContractError('Original native event, source or hold pins differ; preserve the original pending enrollment')
            value={**value,'observation_id':intent['observation_id']}
        except FileNotFoundError:
            intent={'schema':INTENT_SCHEMA,'enrollment_id':str(uuid.uuid4()),**identity,
                    'native_event_id':event,'source_sha256':value['source_sha256'],
                    'source_reference':value['source_reference'],'observation_id':value['observation_id'],
                    'context_digest':context.context_digest,'state':'intent'}
            enrollment_value(intent,identity,event)
            scope_binding.recheck_private_enrollment(context)
            bank_writes.fault('hold-source-before-intent')
            exclusive(intent_path,intent,MAX_INTENT)
        try:
            bank_writes.fault('hold-source-after-intent')
            scope_binding.recheck_private_enrollment(context)
            # Publish pending inventory only after the durable intent exists.
            bank_writes.fault('hold-source-before-index')
            index(base,directory,identity,root,context)
            matching=[];free=None
            for i in range(MAX_SOURCES):
                path=directory/('source-'+str(i)+'.json')
                try:old=validate_source(read(path,MAX_SOURCE),identity)
                except FileNotFoundError:
                    if free is None:free=path
                    continue
                if old['observation_id']==value['observation_id'] or old['native_source_id']==event:
                    expected={**value,'control_origin_hint':old['control_origin_hint']}
                    if old!=expected:raise ContractError('Native event has conflicting retained source evidence')
                    value=expected
                    matching.append(path)
            if len(matching)>1:raise ContractError('Native event has duplicate source slots; retain all evidence')
            if not matching:
                if free is None:raise ContractError('Source capacity reached; retain original enrollment intent')
                scope_binding.recheck_private_enrollment(context)
                bank_writes.fault('hold-source-before-source')
                exclusive(free,value,MAX_SOURCE)
            bank_writes.fault('hold-source-after-source')
            scope_binding.recheck_private_enrollment(context)
            intent=enrollment_state(intent_path,intent,'source-written',context)
            index(base,directory,identity,root,context)
            bank_writes.fault('hold-source-after-index')
            scope_binding.recheck_private_enrollment(context)
            if len([row for row in inventory(directory,identity)['sources'] if row['observation_id']==value['observation_id']])!=1:
                raise ContractError('Held source durable readback failed; preserve its intent')
            intent=enrollment_state(intent_path,intent,'registered',context)
            bank_writes.fault('hold-source-before-final-recheck')
            scope_binding.recheck_private_enrollment(context)
            return value
        except BaseException:
            # Conservative intent bookkeeping never grants enrollment success.
            # Do not erase the source/index or allocate a new observation later.
            try:enrollment_state(intent_path,intent,'unverified')
            except (OSError,ValueError,TypeError,KeyError):pass
            raise


@source_operation
def register(repo,harness,session,lifecycle,source_sha256,*,native_source_id=None,source_reference=None,_enrollment_context=None):
    root,context,base,directory,identity=(private_namespace(repo,harness,session,lifecycle)
        if _enrollment_context is None else _enrollment_namespace(repo,harness,session,lifecycle,_enrollment_context))
    value={'schema':'agent-memory/turn-source-v2',**identity,'observation_id':str(uuid.uuid4()),'source_sha256':source_sha256,'native_source_id':native_source_id,'source_reference':source_reference,'control_origin_hint':False}
    validate_source(value,identity)
    if len(encoded(value))>MAX_SOURCE:raise ContractError('Source observation exceeds encoded bound')
    if context.held and native_source_id is None:
        raise ContractError('Recovery-pending input needs its actual native event identity; retain and retry the same event')
    scope_binding.recheck_private_enrollment(context)
    private_mkdir(directory)
    for i in range(2):
        try:
            wake=read(directory/('wake-'+str(i)+'.json'),MAX_SOURCE)
            if not isinstance(wake,dict) or set(wake)!={'schema','fingerprint','generated_sha256'} or wake['schema']!='agent-memory/turn-wake-v2':raise ContractError('Unsafe retained wake evidence')
            if wake['generated_sha256']==source_sha256:value['control_origin_hint']=True
        except FileNotFoundError:pass
    if context.held:return held_register(root,context,base,directory,identity,value)
    if native_source_id is not None:
        key=hashlib.sha256(encoded([identity,native_source_id])).hexdigest()
        if os.path.lexists(directory/'enrollments'/(key+'.json')):
            raise ContractError('Original held enrollment requires its exact verified hold pins; preserve the pending observation')
    try:
        index(base,directory,identity,root,context)
        for i in range(MAX_SOURCES):
            path=directory/('source-'+str(i)+'.json')
            scope_binding.recheck_private_enrollment(context)
            try:exclusive(path,value,MAX_SOURCE)
            except FileExistsError:continue
            scope_binding.recheck_private_enrollment(context)
            # Cleanup may race append; restore and read back the durable index.
            index(base,directory,identity,root,context)
            scope_binding.recheck_private_enrollment(context)
            return value
        fault(directory,'source-capacity')
        raise ContractError('Source capacity reached; preserve this turn and retry in a new turn after inspecting backlog')
    except (OSError,ValueError):
        fault(directory,'registration-unverified')
        raise


@contextmanager
def locked(directory):
    if _read_only_locks.get():
        path=directory/'completion.lock'
        if os.path.lexists(path):
            with private_lock(path,existing_only=True):yield
            return
        if not _barriers.get().get(str(directory.parents[2])):
            raise ContractError('Source completion lock is absent without a held source barrier')
        trust_paths.state(directory)
        parent=directory.lstat()
        try:yield
        finally:
            named=directory.lstat()
            if os.path.lexists(path) or (parent.st_dev,parent.st_ino)!=(named.st_dev,named.st_ino):
                raise ContractError('Source completion lock appeared during read-only observation')
        return
    trust_paths.state(directory)
    fd=os.open(directory/'completion.lock',os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW|os.O_NONBLOCK,0o600)
    try:
        s=os.fstat(fd)
        if not stat.S_ISREG(s.st_mode) or s.st_uid!=os.geteuid() or s.st_nlink!=1 or s.st_mode&0o077:raise ContractError('Unsafe completion lock')
        try:fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:raise SourceTransportBusy('Source completion is busy; preserve pending work') from None
        yield
    finally:os.close(fd)


def validate_receipt(receipt,observation_id,digest):
    from . import boundary_assessment
    if not isinstance(digest,str) or len(digest)!=64 or any(c not in '0123456789abcdef' for c in digest):raise ContractError('Invalid completed source digest')
    fields={'outcome','assessment','assessment_is_agent_assertion','source_publication_link_verified','publication_verified','witnessed_commit','input_sha256','canonical_writes_performed','observation_id','source_sha256'}
    if (not isinstance(receipt,dict) or set(receipt)!=fields or receipt['observation_id']!=observation_id or receipt['source_sha256']!=digest
            or receipt['assessment_is_agent_assertion'] is not True or receipt['source_publication_link_verified'] is not False
            or receipt['canonical_writes_performed'] is not False or not isinstance(receipt['input_sha256'],str)
            or len(receipt['input_sha256'])!=64 or any(c not in '0123456789abcdef' for c in receipt['input_sha256'])):
        raise ContractError('Completed assessment evidence is malformed')
    if receipt['outcome']=='no-learning':
        if receipt['publication_verified'] is not False or receipt['witnessed_commit'] is not None:raise ContractError('No-learning receipt cannot claim landing')
        boundary_assessment.validate(receipt['assessment'],observation_id)
    elif receipt['outcome']=='published':
        commit=receipt['witnessed_commit']
        if (receipt['publication_verified'] is not True or not isinstance(commit,str) or len(commit) not in {40,64}
                or any(c not in '0123456789abcdef' for c in commit) or not isinstance(receipt['assessment'],str)
                or not receipt['assessment'].strip() or len(receipt['assessment'].encode())>2048):raise ContractError('Published receipt evidence is malformed')
    else:raise ContractError('Unknown completed assessment outcome')
    return receipt


def unidentified_inventory(directory,identity):
    target=directory/'unidentified-inputs';values=[]
    if not os.path.lexists(target):return values
    trust_paths.state(target)
    fields={'schema','retention_id',*identity,'source_reference','source_base64','source_sha256',
            'context_digest','uncertainty'}
    with os.scandir(target) as scan:
        for number,entry in enumerate(scan):
            if number>=MAX_SOURCES:raise ContractError('Unidentified input retention capacity reached; preserve native originals')
            value=read(Path(entry.path),MAX_RETAINED_INPUT)
            if (not isinstance(value,dict) or set(value)!=fields or value['schema']!=UNIDENTIFIED_SCHEMA
                    or any(value.get(key)!=item for key,item in identity.items())
                    or value['uncertainty']!='native-event-identity-unavailable'
                    or str(uuid.UUID(value['retention_id']))!=value['retention_id']
                    or uuid.UUID(value['retention_id']).version!=4 or entry.name!=value['retention_id']+'.json'):
                raise ContractError('Unidentified private input evidence is malformed or foreign')
            raw=base64.b64decode(value['source_base64'],validate=True)
            if not raw or len(raw)>65536 or hashlib.sha256(raw).hexdigest()!=value['source_sha256']:
                raise ContractError('Unidentified private input bytes are missing or changed')
            if value['source_reference'] is not None:bounded(value['source_reference'],2048)
            if (not isinstance(value['context_digest'],str) or len(value['context_digest'])!=64
                    or any(char not in '0123456789abcdef' for char in value['context_digest'])):
                raise ContractError('Unidentified private input has invalid original context pins')
            values.append(value)
    return sorted(values,key=lambda value:value['retention_id'])


@source_operation
def retain_unidentified_input(repo,harness,session,lifecycle,raw_source,source_reference=None):
    """Retain refused input bytes, not an observation or invented native event."""
    root,context,base,directory,identity=private_namespace(repo,harness,session,lifecycle,True)
    if not context.held or not isinstance(raw_source,bytes) or not raw_source or len(raw_source)>65536:
        raise ContractError('Unidentified input retention requires a verified hold and complete bounded source bytes')
    if source_reference is not None:bounded(source_reference,2048)
    with private_lock(directory/'enrollment.lock'):
        scope_binding.recheck_private_enrollment(context)
        if len(unidentified_inventory(directory,identity))>=MAX_SOURCES:
            raise ContractError('Unidentified input capacity reached; preserve native originals for inspection')
        value={'schema':UNIDENTIFIED_SCHEMA,'retention_id':str(uuid.uuid4()),**identity,
               'source_reference':source_reference,'source_base64':base64.b64encode(raw_source).decode('ascii'),
               'source_sha256':hashlib.sha256(raw_source).hexdigest(),'context_digest':context.context_digest,
               'uncertainty':'native-event-identity-unavailable'}
        target=directory/'unidentified-inputs'
        scope_binding.recheck_private_enrollment(context)
        private_mkdir(target)
        scope_binding.recheck_private_enrollment(context)
        exclusive(target/(value['retention_id']+'.json'),value,MAX_RETAINED_INPUT)
        scope_binding.recheck_private_enrollment(context)
        return {'retention_id':value['retention_id'],'verified_observation':False,'outcome':'pending-native-event-identity'}


def enrollment_inventory(directory,identity):
    target=directory/'enrollments';values=[]
    if not os.path.lexists(target):return values
    trust_paths.state(target)
    with os.scandir(target) as scan:
        for number,entry in enumerate(scan):
            if number>=MAX_SOURCES:raise ContractError('Held enrollment inventory exceeds source capacity')
            value=read(Path(entry.path),MAX_INTENT)
            event=value.get('native_event_id') if isinstance(value,dict) else None
            enrollment_value(value,identity,event)
            key=hashlib.sha256(encoded([identity,event])).hexdigest()+'.json'
            if entry.name!=key:raise ContractError('Held enrollment intent is outside its original event namespace')
            values.append(value)
    return sorted(values,key=lambda value:value['enrollment_id'])


def inventory(directory,identity):
    sources=[];faults=[];pins=[]
    if not directory.exists():return {'sources':[],'pending':[],'completed':[],'faults':['source-missing'],'fingerprint':hashlib.sha256(b'missing').hexdigest(),'complete':False}
    trust_paths.state(directory)
    for i in range(MAX_SOURCES):
        path=directory/('source-'+str(i)+'.json')
        try:
            v=validate_source(read(path,MAX_SOURCE),identity)
        except FileNotFoundError:continue
        except (OSError,ValueError,TypeError,KeyError):
            faults.append('source-reservation-unverifiable');pins.append([i,'unverifiable']);continue
        pins.append([i,v])
        completed=None
        try:
            completed=read(directory/(v['observation_id']+'.done'),MAX_COMPLETION)
            if (not isinstance(completed,dict) or set(completed)!={'schema','observation_id','source_sha256','receipt','record_sha256'} or completed['schema']!='agent-memory/source-completed-v2' or completed['observation_id']!=v['observation_id'] or completed['source_sha256']!=v['source_sha256'] or not isinstance(completed['receipt'],dict)):
                raise ContractError('Completion differs from observation')
            if completed['record_sha256']!=hashlib.sha256(encoded({k:x for k,x in completed.items() if k!='record_sha256'})).hexdigest():raise ContractError('Completion checksum differs')
            validate_receipt(completed['receipt'],v['observation_id'],v['source_sha256'])
        except FileNotFoundError:pass
        except (OSError,ValueError,TypeError,KeyError):faults.append('completion-unverifiable');completed=None
        sources.append({**v,'completed':completed is not None,'completion':completed['receipt'] if completed else None})
    if os.path.lexists(directory/'fault.json'):faults.append('registration-fault-retained')
    try:
        enrollments=enrollment_inventory(directory,identity)
        if enrollments:pins.append(['enrollments',enrollments])
        if any(value['state']!='registered' for value in enrollments):faults.append('enrollment-unverified')
        observed={value['observation_id'] for value in sources}
        if any(value['observation_id'] not in observed for value in enrollments):faults.append('enrollment-source-missing')
    except (OSError,ValueError,TypeError,KeyError):
        faults.append('enrollment-unverifiable');pins.append(['enrollments','unverifiable'])
    unidentified=[]
    try:
        retained=unidentified_inventory(directory,identity)
        if retained:
            unidentified=[value['retention_id'] for value in retained]
            faults.append('unidentified-input-retained')
            pins.append(['unidentified-inputs',unidentified])
    except (OSError,ValueError,TypeError,KeyError):
        faults.append('unidentified-input-unverifiable');pins.append(['unidentified-inputs','unverifiable'])
    fingerprint=hashlib.sha256(encoded(pins)).hexdigest()
    return {'sources':sources,'pending':[v for v in sources if not v['completed']],'completed':[v for v in sources if v['completed']],'faults':faults,'fingerprint':fingerprint,'complete':bool(sources) and not faults and all(v['completed'] for v in sources),'unidentified_input_ids':unidentified}


def private_inventory(directory,identity,context):
    value=inventory(directory,identity)
    if context.held:
        value={**value,'complete':False,'recovery_pending':True,'semantic_coverage':'incomplete-recovery',
               'fingerprint':hashlib.sha256(encoded([value['fingerprint'],context.context_digest])).hexdigest()}
    return value


def pending_rows(state,directory,identity):
    rows={source['observation_id']:{'observation_id':source['observation_id'],'source_sha256':source['source_sha256']} for source in state['pending']}
    observed={source['observation_id'] for source in state['sources']}
    for intent in enrollment_inventory(directory,identity):
        if intent['state']!='registered' or intent['observation_id'] not in observed:
            rows[intent['observation_id']]={'observation_id':intent['observation_id'],'source_sha256':intent['source_sha256']}
    return list(rows.values())


def _inspect_enrollment(repo,harness,session,lifecycle,context):
    """Read-only transaction-local inventory; never skips a mutation guard."""
    root,context,base,directory,identity=_enrollment_namespace(repo,harness,session,lifecycle,context)
    if not directory.exists():
        return private_inventory(directory,identity,context)
    with locked(directory):
        value=private_inventory(directory,identity,context)
        again=private_inventory(directory,identity,context)
        if again!=value:raise ContractError('Source inventory changed; inspect again')
        scope_binding._enrollment_read_context(repo,context)
        return value


@source_operation
def inspect(repo,harness,session,lifecycle):
    root,context,base,directory,identity=private_namespace(repo,harness,session,lifecycle)
    if not directory.exists():
        value=private_inventory(directory,identity,context)
        scope_binding.recheck_private_enrollment(context)
        return value
    with locked(directory):
        value=private_inventory(directory,identity,context)
        scope_binding.recheck_private_enrollment(context)
        again=private_inventory(directory,identity,context)
        if again!=value:raise ContractError('Source inventory changed; inspect again')
        scope_binding.recheck_private_enrollment(context)
        return value


def get(repo,harness,session,lifecycle,observation_id):
    str(uuid.UUID(observation_id))
    result=inspect(repo,harness,session,lifecycle)
    matches=[v for v in result['sources'] if v['observation_id']==observation_id]
    if len(matches)!=1:raise ContractError('Original observation unavailable; preserve source state')
    return {k:v for k,v in matches[0].items() if k not in {'completed','completion'}}


def cleanup(base,directory,identity,root):
    pending=base/'pending'
    if not pending.exists():return
    for i in range(MAX_TURNS):
        path=pending/(str(i)+'.json')
        try:v=read(path,MAX_SOURCE)
        except (OSError,ValueError):continue
        if v==ref(directory,identity):
            if not inventory(directory,identity)['complete']:return
            path.unlink();sync(pending)
            if not inventory(directory,identity)['complete']:index(base,directory,identity,root)


@source_operation
def complete(repo,harness,session,lifecycle,observation_id,source_sha256,receipt):
    root,context,base,directory,identity=namespace(repo,harness,session,lifecycle)
    if not isinstance(receipt,VerifiedReceipt):raise ContractError('Use verify_assessment before recording completion')
    if receipt.verification_source_id!=observation_id or receipt.verification_value.get('observation_id')!=observation_id or receipt.verification_value.get('source_sha256')!=source_sha256:
        raise ContractError('Verified assessment belongs to another observation')
    validate_receipt(receipt,observation_id,source_sha256)
    value={'schema':'agent-memory/source-completed-v2','observation_id':observation_id,'source_sha256':source_sha256,'receipt':receipt}
    value['record_sha256']=hashlib.sha256(encoded(value)).hexdigest()
    if len(encoded(value))>MAX_COMPLETION:raise ContractError('Encoded completion exceeds bound')
    with locked(directory):
        checked=verify_assessment(root,receipt.verification_value,receipt.verification_source_id)
        if checked!=receipt:raise ContractError('Assessment evidence changed before completion')
        inv=inventory(directory,identity)
        match=[v for v in inv['sources'] if v['observation_id']==observation_id and v['source_sha256']==source_sha256]
        if len(match)!=1:raise ContractError('Completion must match an original observation')
        path=directory/(observation_id+'.done')
        if os.path.lexists(path):
            if read(path,MAX_COMPLETION)!=value:raise ContractError('Acknowledgment differs from retained completion')
            already=True
        else:exclusive(path,value,MAX_COMPLETION);already=False
        scope_binding.require(root,context['project_id'],expected=context)
        cleanup(base,directory,identity,root)
        return {'verified':True,'already_completed':already,**receipt}


@source_operation
def reserve_wake(repo,harness,session,lifecycle,fingerprint,generated_sha256):
    root,context,base,directory,identity=private_namespace(repo,harness,session,lifecycle)
    for digest in [fingerprint,generated_sha256]:
        if not isinstance(digest,str) or len(digest)!=64 or any(c not in '0123456789abcdef' for c in digest):raise ContractError('Invalid wake digest')
    with locked(directory):
        current=private_inventory(directory,identity,context)
        scope_binding.recheck_private_enrollment(context)
        if current['complete'] or current['fingerprint']!=fingerprint:return False
        for i in range(2):
            path=directory/('wake-'+str(i)+'.json')
            try:
                old=read(path,MAX_SOURCE)
                if not isinstance(old,dict) or set(old)!={'schema','fingerprint','generated_sha256'} or old['schema']!='agent-memory/turn-wake-v2':raise ContractError('Unsafe retained wake evidence')
                if old['fingerprint']==fingerprint:
                    scope_binding.recheck_private_enrollment(context)
                    return False
            except FileNotFoundError:
                scope_binding.recheck_private_enrollment(context)
                exclusive(path,{'schema':'agent-memory/turn-wake-v2','fingerprint':fingerprint,'generated_sha256':generated_sha256},MAX_SOURCE)
                scope_binding.recheck_private_enrollment(context)
                return True
        scope_binding.recheck_private_enrollment(context)
        return False


@source_operation
def backlog(repo,harness=None,offset=0,legacy_offset=0,enrollment_offset=0):
    if (type(offset) is not int or type(legacy_offset) is not int or not 0<=offset<=MAX_TURNS
            or not 0<=legacy_offset<=4096 or type(enrollment_offset) is not int or not 0<=enrollment_offset<=65536):
        raise ContractError('Invalid backlog page offset')
    root,context,base,_,_=private_namespace(repo,harness or 'codex','backlog','backlog')
    result={'turns':[],'legacy':[],'faults':[],'next_offset':None,'next_legacy_offset':None,
            'next_enrollment_offset':None,'source_completeness_verified':False,'recovery_pending':context.held}
    def error(code):
        if code not in result['faults']:result['faults'].append(code)
    def append(field,value):
        trial={**result,field:[*result[field],value]}
        if len(encoded(trial))>60000:return False
        result[field].append(value);return True
    indexed=set()
    pending=base/'pending'
    if pending.exists():
        trust_paths.state(pending)
        for i in range(MAX_TURNS):
            try:v=read(pending/(str(i)+'.json'),MAX_SOURCE)
            except FileNotFoundError:continue
            except (OSError,ValueError):error('index-reservation-unverifiable');continue
            if not isinstance(v,dict) or set(v)!={'schema','harness','session','lifecycle','project_id','turn'} or v['schema']!='agent-memory/turn-reference-v2' or v['project_id']!=context.project_id:
                error('index-reference-unverifiable');continue
            validate_reference(v,root,context.project_id)
            indexed.add(v['turn'])
            if i<offset:continue
            if harness is not None and v['harness']!=harness:continue
            try:
                _,_,_,directory,identity=private_namespace(repo,v['harness'],v['session'],v['lifecycle'])
                if directory.name!=v['turn']:raise ContractError('Index namespace changed')
                state=inspect(repo,v['harness'],v['session'],v['lifecycle'])
                if not state['complete']:
                    row={**identity,'pending':pending_rows(state,directory,identity),
                         'faults':state['faults'],'fingerprint':state['fingerprint'],'complete':False,
                         'unidentified_input_ids':state.get('unidentified_input_ids',[])}
                    if not append('turns',row):result['next_offset']=i;break
            except (OSError,ValueError,TypeError,KeyError):error('pending-turn-unavailable')
    # Intent fsync precedes index publication. Enumerate that crash window in
    # the same namespaces; an intent is never fabricated into a source record.
    turns=base/'turns'
    if turns.exists():
        trust_paths.state(turns)
        with os.scandir(turns) as scan:
            entries=[]
            for number,entry in enumerate(scan):
                if number>=65536:raise ContractError('Source namespace inventory exceeds bound')
                entries.append(entry)
        for position,entry in enumerate(sorted(entries,key=lambda entry:entry.name)):
            if position<enrollment_offset or entry.name in indexed:continue
            directory=Path(entry.path)
            if not any(os.path.lexists(directory/name) for name in ('enrollments','unidentified-inputs')):continue
            try:
                trust_paths.state(directory)
                first=None
                target=directory/('enrollments' if os.path.lexists(directory/'enrollments') else 'unidentified-inputs')
                with os.scandir(target) as scan:
                    for intent_entry in scan:
                        first=read(Path(intent_entry.path),MAX_RETAINED_INPUT)
                        break
                if first is None:continue
                identity={key:first[key] for key in ('harness','session','lifecycle','project_id')}
                validate_reference(ref(directory,identity),root,context.project_id)
                if harness is not None and identity['harness']!=harness:continue
                state=inspect(root,identity['harness'],identity['session'],identity['lifecycle'])
                if state['complete']:continue
                row={**identity,'pending':pending_rows(state,directory,identity),'faults':state['faults'],
                     'fingerprint':state['fingerprint'],'complete':False,
                     'unidentified_input_ids':state.get('unidentified_input_ids',[])}
                if not append('turns',row):
                    result['next_enrollment_offset']=position
                    break
            except (OSError,ValueError,TypeError,KeyError):error('enrollment-orphan-unverifiable')
    operations,_=bank_writes.state_paths(root,bank_writes.bank_path(root)[1],str(uuid.uuid4()))
    seen=0
    for name in ['codex','claude']:
        if harness is not None and name!=harness:continue
        directory=operations.parent/(name+'-boundaries')
        if not directory.exists():continue
        trust_paths.state(directory)
        with os.scandir(directory) as scan:
            entries=[]
            for n,e in enumerate(scan):
                if n>=65536:raise ContractError('Legacy inventory exceeds bound')
                entries.append(e)
            for e in sorted(entries,key=lambda e:e.name):
                if not e.name.endswith('.pending'):continue
                if seen>=4096:raise ContractError('Legacy pending exceeds bound')
                position=seen;seen+=1
                if position<legacy_offset:continue
                try:
                    source=read(Path(e.path),MAX_SOURCE)
                    if not isinstance(source,dict) or source.get('project_id')!=context.project_id:raise ContractError('Legacy project differs')
                    selected={k:source[k] for k in ['schema','project_id','session','turn','source_uuid','source_sha256'] if k in source}
                    if not append('legacy',{'harness':name,'legacy_key':e.name,'source':selected}):
                        result['next_legacy_offset']=position
                        scope_binding.recheck_private_enrollment(context)
                        return result
                except (OSError,ValueError):error('legacy-pending-unverifiable')
    scope_binding.recheck_private_enrollment(context)
    return result


@source_operation
def verify_assessment(repo,value,source_id):
    """Verify existing assessment/publication gates; input flags grant nothing."""
    from . import boundary_assessment, memory_publication, sharing_recovery
    context=scope_binding.require(repo)
    if value.get('observation_id')!=source_id:raise ContractError('Assessment must identify its exact observation')
    digest=value.get('source_sha256')
    if not isinstance(digest,str) or len(digest)!=64 or any(c not in '0123456789abcdef' for c in digest):raise ContractError('Assessment must bind exact source digest')
    outcome=value.get('outcome')
    if outcome=='no-learning':
        if 'publication_request' in value:raise ContractError('No-learning cannot claim publication')
        boundary_assessment.validate(value.get('assessment'),source_id)
        witnessed=None
    elif outcome=='published':
        if not isinstance(value.get('assessment'),str) or not value['assessment'].strip() or len(value['assessment'].encode())>2048:
            raise ContractError('Published completion requires a bounded assessment')
        request=value.get('publication_request')
        memory_publication.request_value(repo,request)
        proof=request['proof']
        if proof.get('blocked_paths') or proof.get('blocked_components'):raise ContractError('Conflicted publication remains pending')
        replay=sharing_recovery.replay(repo,{'base':request['base'],'candidate':request['candidate']},quarantine_records=proof.get('schema')=='agent-memory/semantic-sharing-proof-v2')
        if not replay.get('intended_delta_landed'):raise ContractError('Publication is not witnessed on authority')
        witnessed=replay['witnessed_commit']
    else:raise ContractError('Explicit source assessment required')
    result=VerifiedReceipt({'observation_id':source_id,'source_sha256':digest,'outcome':outcome,'assessment':value['assessment'],'assessment_is_agent_assertion':True,
            'source_publication_link_verified':False,'publication_verified':witnessed is not None,
            'witnessed_commit':witnessed,'input_sha256':hashlib.sha256(encoded(value)).hexdigest(),
            'canonical_writes_performed':False})
    scope_binding.require(repo,context['project_id'],expected=context)
    result.verification_value=value
    result.verification_source_id=source_id
    return result


@source_operation
def pending_notice(repo,harness,session,lifecycle):
    context=scope_binding.private_enrollment_context(repo)
    pending=backlog(repo,harness)
    scope_binding.recheck_private_enrollment(context)
    earlier=[v for v in pending['turns'] if v['session']!=session or v['lifecycle']!=lifecycle]
    summary={'turns':len(earlier),'legacy':len(pending['legacy']),'faults':pending['faults'],'paged':any(pending[key] is not None for key in ('next_offset','next_legacy_offset','next_enrollment_offset')),'recovery_pending':pending['recovery_pending']}
    if not any(summary.values()):return None
    fingerprint=hashlib.sha256(encoded({'turns':earlier,'legacy':pending['legacy'],'faults':pending['faults'],'recovery_pending':pending['recovery_pending']})).hexdigest()
    _,_,_,directory,_=_namespace(repo,harness,session,'pending-notice',context,True)
    with locked(directory):
        path=directory/'notice.json'
        try:
            old=read(path,MAX_SOURCE)
            if old=={'fingerprint':fingerprint}:
                scope_binding.recheck_private_enrollment(context)
                return None
        except FileNotFoundError:pass
        value={'fingerprint':fingerprint}
        temp=directory/(str(uuid.uuid4())+'.tmp')
        try:
            scope_binding.recheck_private_enrollment(context)
            exclusive(temp,value,MAX_SOURCE)
            scope_binding.recheck_private_enrollment(context)
            os.replace(temp,path);sync(directory)
            scope_binding.recheck_private_enrollment(context)
        finally:
            if os.path.lexists(temp):temp.unlink()
    scope_binding.recheck_private_enrollment(context)
    return summary
