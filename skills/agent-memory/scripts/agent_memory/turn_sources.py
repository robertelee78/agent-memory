"""Private append-only source observations; coverage does not establish truth."""
from __future__ import annotations
from contextlib import contextmanager
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
HARNESSES={'codex','claude','opencode'}


class VerifiedReceipt(dict):
    pass


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


def namespace(repo,harness,session,lifecycle,create=False):
    if harness not in HARNESSES:raise ContractError('Unsupported source harness')
    bounded(session);bounded(lifecycle)
    root,bank=bank_writes.bank_path(repo);context=scope_binding.require(root)
    operations,_=bank_writes.state_paths(root,bank,str(uuid.uuid4()))
    base=operations.parent/'turn-sources-v2'
    key=hashlib.sha256(encoded([str(root),harness,session,lifecycle])).hexdigest()
    directory=base/'turns'/key
    trust_paths.inspect(directory)
    if create:
        private_mkdir(directory)
    return root,context,base,directory,{'harness':harness,'session':session,'lifecycle':lifecycle,'project_id':context['project_id']}


def read_bytes(path,maximum):
    fd=os.open(path,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK)
    try:
        s=os.fstat(fd)
        if not stat.S_ISREG(s.st_mode) or s.st_uid!=os.geteuid() or s.st_nlink!=1 or s.st_mode&0o077 or s.st_size>maximum:
            raise ContractError('Unsafe source state; preserve it for inspection')
        with os.fdopen(fd,'rb',closefd=False) as f:raw=f.read(maximum+1)
        if len(raw)>maximum:raise ContractError('Source state exceeds bound')
    finally:os.close(fd)
    return raw


def read(path,maximum):
    return json.loads(read_bytes(path,maximum))


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


def index(base,directory,identity,root):
    """Fixed slots bound capacity; registration never takes completion lock."""
    target=base/'pending';trust_paths.inspect(target)
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
        try:exclusive(free,value,MAX_SOURCE);return free
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


def register(repo,harness,session,lifecycle,source_sha256,*,native_source_id=None,source_reference=None):
    root,context,base,directory,identity=namespace(repo,harness,session,lifecycle,True)
    value={'schema':'agent-memory/turn-source-v2',**identity,'observation_id':str(uuid.uuid4()),'source_sha256':source_sha256,'native_source_id':native_source_id,'source_reference':source_reference,'control_origin_hint':False}
    for i in range(2):
        try:
            wake=read(directory/('wake-'+str(i)+'.json'),MAX_SOURCE)
            if not isinstance(wake,dict) or set(wake)!={'schema','fingerprint','generated_sha256'} or wake['schema']!='agent-memory/turn-wake-v2':raise ContractError('Unsafe retained wake evidence')
            if wake['generated_sha256']==source_sha256:value['control_origin_hint']=True
        except FileNotFoundError:pass
    validate_source(value,identity)
    if len(encoded(value))>MAX_SOURCE:raise ContractError('Source observation exceeds encoded bound')
    try:
        index(base,directory,identity,root)
        for i in range(MAX_SOURCES):
            path=directory/('source-'+str(i)+'.json')
            try:exclusive(path,value,MAX_SOURCE)
            except FileExistsError:continue
            # Cleanup may race append; restore and read back the durable index.
            index(base,directory,identity,root)
            scope_binding.require(root,context['project_id'],expected=context)
            return value
        fault(directory,'source-capacity')
        raise ContractError('Source capacity reached; preserve this turn and retry in a new turn after inspecting backlog')
    except (OSError,ValueError):
        fault(directory,'registration-unverified')
        raise


@contextmanager
def locked(directory):
    trust_paths.state(directory)
    fd=os.open(directory/'completion.lock',os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW|os.O_NONBLOCK,0o600)
    try:
        s=os.fstat(fd)
        if not stat.S_ISREG(s.st_mode) or s.st_uid!=os.geteuid() or s.st_nlink!=1 or s.st_mode&0o077:raise ContractError('Unsafe completion lock')
        try:fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError:raise ContractError('Source completion is busy; preserve pending work') from None
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
    fingerprint=hashlib.sha256(encoded(pins)).hexdigest()
    return {'sources':sources,'pending':[v for v in sources if not v['completed']],'completed':[v for v in sources if v['completed']],'faults':faults,'fingerprint':fingerprint,'complete':bool(sources) and not faults and all(v['completed'] for v in sources)}


def inspect(repo,harness,session,lifecycle):
    root,context,base,directory,identity=namespace(repo,harness,session,lifecycle)
    if not directory.exists():return inventory(directory,identity)
    with locked(directory):
        value=inventory(directory,identity)
        scope_binding.require(root,context['project_id'],expected=context)
        again=inventory(directory,identity)
        if again!=value:raise ContractError('Source inventory changed; inspect again')
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


def reserve_wake(repo,harness,session,lifecycle,fingerprint,generated_sha256):
    root,context,base,directory,identity=namespace(repo,harness,session,lifecycle)
    for digest in [fingerprint,generated_sha256]:
        if not isinstance(digest,str) or len(digest)!=64 or any(c not in '0123456789abcdef' for c in digest):raise ContractError('Invalid wake digest')
    with locked(directory):
        current=inventory(directory,identity)
        if current['complete'] or current['fingerprint']!=fingerprint:return False
        for i in range(2):
            path=directory/('wake-'+str(i)+'.json')
            try:
                old=read(path,MAX_SOURCE)
                if not isinstance(old,dict) or set(old)!={'schema','fingerprint','generated_sha256'} or old['schema']!='agent-memory/turn-wake-v2':raise ContractError('Unsafe retained wake evidence')
                if old['fingerprint']==fingerprint:return False
            except FileNotFoundError:
                exclusive(path,{'schema':'agent-memory/turn-wake-v2','fingerprint':fingerprint,'generated_sha256':generated_sha256},MAX_SOURCE)
                scope_binding.require(root,context['project_id'],expected=context)
                return True
        return False


def backlog(repo,harness=None,offset=0,legacy_offset=0):
    if type(offset) is not int or type(legacy_offset) is not int or not 0<=offset<=MAX_TURNS or not 0<=legacy_offset<=4096:raise ContractError('Invalid backlog page offset')
    root,context,base,_,_=namespace(repo,harness or 'codex','backlog','backlog')
    result={'turns':[],'legacy':[],'faults':[],'next_offset':None,'next_legacy_offset':None,'source_completeness_verified':False}
    def error(code):
        if code not in result['faults']:result['faults'].append(code)
    def append(field,value):
        trial={**result,field:[*result[field],value]}
        if len(encoded(trial))>60000:return False
        result[field].append(value);return True
    pending=base/'pending'
    if pending.exists():
        trust_paths.state(pending)
        for i in range(offset,MAX_TURNS):
            try:v=read(pending/(str(i)+'.json'),MAX_SOURCE)
            except FileNotFoundError:continue
            except (OSError,ValueError):error('index-reservation-unverifiable');continue
            if not isinstance(v,dict) or set(v)!={'schema','harness','session','lifecycle','project_id','turn'} or v['schema']!='agent-memory/turn-reference-v2' or v['project_id']!=context['project_id']:
                error('index-reference-unverifiable');continue
            if harness is not None and v['harness']!=harness:continue
            try:
                _,_,_,directory,identity=namespace(repo,v['harness'],v['session'],v['lifecycle'])
                if directory.name!=v['turn']:raise ContractError('Index namespace changed')
                state=inspect(repo,v['harness'],v['session'],v['lifecycle'])
                if not state['complete']:
                    row={**identity,'pending':[{'observation_id':x['observation_id'],'source_sha256':x['source_sha256']} for x in state['pending']],
                         'faults':state['faults'],'fingerprint':state['fingerprint'],'complete':False}
                    if not append('turns',row):result['next_offset']=i;break
            except (OSError,ValueError,TypeError,KeyError):error('pending-turn-unavailable')
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
                    if not isinstance(source,dict) or source.get('project_id')!=context['project_id']:raise ContractError('Legacy project differs')
                    selected={k:source[k] for k in ['schema','project_id','session','turn','source_uuid','source_sha256'] if k in source}
                    if not append('legacy',{'harness':name,'legacy_key':e.name,'source':selected}):result['next_legacy_offset']=position;return result
                except (OSError,ValueError):error('legacy-pending-unverifiable')
    return result


def verify_assessment(repo,value,source_id):
    """Verify existing assessment/publication gates; input flags grant nothing."""
    from . import boundary_assessment, memory_publication, sharing_recovery
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
    result.verification_value=value
    result.verification_source_id=source_id
    return result


def pending_notice(repo,harness,session,lifecycle):
    pending=backlog(repo,harness)
    earlier=[v for v in pending['turns'] if v['session']!=session or v['lifecycle']!=lifecycle]
    summary={'turns':len(earlier),'legacy':len(pending['legacy']),'faults':pending['faults'],'paged':pending['next_offset'] is not None or pending['next_legacy_offset'] is not None}
    if not any([summary['turns'],summary['legacy'],summary['faults'],summary['paged']]):return None
    fingerprint=hashlib.sha256(encoded({'turns':earlier,'legacy':pending['legacy'],'faults':pending['faults']})).hexdigest()
    _,_,_,directory,_=namespace(repo,harness,session,'pending-notice',True)
    with locked(directory):
        path=directory/'notice.json'
        try:
            old=read(path,MAX_SOURCE)
            if old=={'fingerprint':fingerprint}:return None
        except FileNotFoundError:pass
        value={'fingerprint':fingerprint}
        temp=directory/(str(uuid.uuid4())+'.tmp')
        try:
            exclusive(temp,value,MAX_SOURCE);os.replace(temp,path);sync(directory)
        finally:
            if os.path.lexists(temp):temp.unlink()
    return summary
