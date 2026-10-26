"""Optional authenticated, immutable metadata derived from checked Markdown.

The catalog never establishes source-currentness: a warm reader must still hash
all actual canonical bytes before output. Same-UID hostile code is outside the
trusted-runtime boundary, just as for the installed interpreter/runtime itself.
"""
from __future__ import annotations
import hashlib
import hmac
import importlib
import json
import math
import os
from pathlib import Path
import re
import secrets
import stat
import sys
import types
import uuid
from contextlib import contextmanager
import fcntl

from . import _ENGINE_SOURCE_ROOT, _INITIAL_ENGINE_SOURCES, _engine_sources
from . import bank_contract, bank_observation, bank_writes as writes, local_registry as storage, trust_paths, records, repository_config, validation_memo
from .frontmatter import read as read_frontmatter
from .developer_identity import digest
from .frontmatter import ContractError

SCHEMA='agent-memory/recall-catalog-v1'
MAX_CACHE=256*1024*1024
MAX_HEADER=16384
MAX_NAMESPACE=256
ENGINE_ROOTS=('frontmatter','developer_identity','scopes','lifecycle','records','bank_contract','bank_layout',
    'bank_observation','bank_writes','repository_config','scope_binding','project_identity','local_registry',
    'security','handoff','discovery','retrieval','recall_catalog','bank_transactions','tombstones',
    'identity_git','remote_identity','trust_paths','checkpoint','sharing_limits','validation_memo')


class CacheMiss(Exception):
    pass


class Generation(dict):
    def __init__(self,value,seen):
        super().__init__(value);self.seen=frozenset(seen)


def compact(value):
    return json.dumps(value,sort_keys=True,separators=(',',':'),ensure_ascii=False,allow_nan=False).encode()


def normalize(value):
    if value is None or type(value) in (str,int,bool):return value
    if type(value) is float:
        if not math.isfinite(value):raise CacheMiss('unsupported engine policy')
        return value
    if isinstance(value,bytes):return {'bytes':value.hex()}
    if isinstance(value,Path):return {'path':str(value)}
    if isinstance(value,re.Pattern):return {'pattern':value.pattern,'flags':value.flags}
    if isinstance(value,(list,tuple)):return [normalize(v) for v in value]
    if isinstance(value,(set,frozenset)):return sorted((normalize(v) for v in value),key=lambda v:compact(v))
    if isinstance(value,dict):
        if all(isinstance(k,str) for k in value):return {k:normalize(v) for k,v in value.items()}
        pairs=[[{'type':type(k).__name__,'value':normalize(k)},normalize(v)] for k,v in value.items()]
        return {'typed_mapping':sorted(pairs,key=lambda row:compact(row[0]))}
    raise CacheMiss('unsupported engine policy')


def function_identity(function,seen=None):
    seen=set() if seen is None else seen
    label=function.__module__+'.'+function.__qualname__
    if id(function) in seen:return {'reference':label}
    seen.add(id(function));closure=[]
    for cell in function.__closure__ or ():
        value=cell.cell_contents
        if isinstance(value,types.FunctionType):closure.append({'function':function_identity(value,seen)})
        elif isinstance(value,type):closure.append({'class':value.__module__+'.'+value.__qualname__})
        else:closure.append(normalize(value))
    return {'code':code_identity(function.__code__),'defaults':normalize(function.__defaults__),
        'kwdefaults':normalize(function.__kwdefaults__),'closure':closure}


def code_identity(code):
    # Marshal encodes incidental object sharing/interning and can change after
    # running the same validator. Bind stable actual CodeType fields instead.
    def constant(value):
        if isinstance(value,types.CodeType):return {'code':code_identity(value)}
        if value is Ellipsis:return {'ellipsis':True}
        if value is NotImplemented:return {'not_implemented':True}
        if isinstance(value,slice):return {'slice':[constant(value.start),constant(value.stop),constant(value.step)]}
        if isinstance(value,complex):return {'complex':[value.real,value.imag]}
        if isinstance(value,tuple):return [constant(item) for item in value]
        if isinstance(value,frozenset):return {'frozenset':sorted((constant(item) for item in value),key=compact)}
        return normalize(value)
    fields=('co_argcount','co_posonlyargcount','co_kwonlyargcount','co_nlocals','co_stacksize','co_flags',
            'co_name','co_qualname','co_filename','co_firstlineno')
    value={name:getattr(code,name) for name in fields}
    value.update(bytecode=code.co_code.hex(),constants=[constant(v) for v in code.co_consts],
        names=list(code.co_names),varnames=list(code.co_varnames),freevars=list(code.co_freevars),
        cellvars=list(code.co_cellvars),linetable=code.co_linetable.hex(),exceptiontable=code.co_exceptiontable.hex())
    return digest(compact(value))


def generation_details():
    """Audited eager native-v1 closure; ambiguity disables reuse/publication."""
    if validation_memo.current() is not None:
        raise CacheMiss('native catalog reuse is separate from active operation validation reuse')
    if _INITIAL_ENGINE_SOURCES is None or _engine_sources()!=_INITIAL_ENGINE_SOURCES:
        raise CacheMiss('executing package generation changed')
    for name in ENGINE_ROOTS:importlib.import_module('agent_memory.'+name)
    modules={name:sys.modules['agent_memory.'+name] for name in ENGINE_ROOTS}
    payload={}
    for name,module in sorted(modules.items()):
        name='agent_memory.'+name
        path=Path(getattr(module,'__file__','')).resolve()
        if not path.is_relative_to(_ENGINE_SOURCE_ROOT) or path.suffix!='.py':
            raise CacheMiss('unbound native dependency')
        relative=path.relative_to(_ENGINE_SOURCE_ROOT).as_posix()
        if relative not in _INITIAL_ENGINE_SOURCES:raise CacheMiss('unbound native dependency')
        functions={};policy={};bindings={}
        for key,value in sorted(vars(module).items()):
            if isinstance(value,types.ModuleType):
                target=value.__name__
                if target.startswith('agent_memory.') and (target.removeprefix('agent_memory.') not in ENGINE_ROOTS
                        or sys.modules.get(target) is not value):raise CacheMiss('unbound validator module binding')
                bindings[key]={'module':target}
            elif isinstance(value,types.FunctionType) and value.__module__.startswith('agent_memory.'):
                if value.__module__.removeprefix('agent_memory.') not in ENGINE_ROOTS:
                    raise CacheMiss('unbound validator function binding')
                functions[key]=function_identity(value)
                bindings[key]={'function':value.__module__+'.'+value.__qualname__}
            elif isinstance(value,type) and value.__module__.startswith('agent_memory.'):
                methods={}
                for method_name,method in vars(value).items():
                    if isinstance(method,(staticmethod,classmethod)):method=method.__func__
                    if isinstance(method,types.FunctionType):
                        methods[method_name]=function_identity(method)
                functions[key]={'methods':methods}
                bindings[key]={'class':value.__module__+'.'+value.__qualname__}
            elif key.isupper() and not isinstance(value,(types.ModuleType,type)):
                policy[key]=normalize(value)
        payload[name]={'file':relative,'functions':functions,'policy':policy,'bindings':bindings}
    value={'python':list(sys.version_info),'implementation':sys.implementation.name,'cache_tag':sys.implementation.cache_tag,
        'package_sources':_INITIAL_ENGINE_SOURCES,'loaded':payload,'codec':SCHEMA}
    return value


def generation():
    value=generation_details()
    seen={name for name in sys.modules if name.startswith('agent_memory.')}
    return Generation({'sha256':digest(compact(value)),'modules':sorted(value['loaded'])},seen)


def coherent(engine):
    current=generation()
    if current!=engine or current.seen-engine.seen:
        raise CacheMiss('validator generation or dependency closure changed')


def physical_inventory(bank):
    """Cheap change hint only; no byte-currentness claim is made here."""
    physical={};stamps={};pending=[(str(bank),'')];visited=0
    while pending:
        directory,prefix=pending.pop()
        with os.scandir(directory) as entries:
            for entry in entries:
                visited+=1
                if visited>bank_observation.MAX_FILES*2:raise CacheMiss('bank physical bound')
                info=entry.stat(follow_symlinks=False);path=prefix+entry.name;mode=info.st_mode
                if stat.S_ISLNK(mode):raise CacheMiss('bank path alias')
                kind='directory' if stat.S_ISDIR(mode) else 'file' if stat.S_ISREG(mode) else 'other'
                if not bank_contract.runtime_path(path):
                    physical[path]=kind
                    if len(physical)>bank_observation.MAX_FILES*2:raise CacheMiss('bank physical bound')
                    if kind=='file' and path.endswith('.md'):
                        stamps[path]=[info.st_dev,info.st_ino,info.st_size,info.st_mtime_ns,info.st_ctime_ns,info.st_mode,info.st_nlink]
                if kind=='directory':pending.append((entry.path,path+'/'))
    bank_observation.physical_contract(physical)
    return physical,stamps


def normalized_physical(physical):
    return {path:kind for path,kind in physical.items() if not bank_contract.runtime_path(path)}


def locations(repo):
    root,bank=writes.bank_path(repo);operations,_=writes.state_paths(root,bank,str(uuid.uuid4()))
    directory=operations.parent/'recall-catalog-v1'
    source=_ENGINE_SOURCE_ROOT.parent.parent if _ENGINE_SOURCE_ROOT.parent.name=='skills' else _ENGINE_SOURCE_ROOT
    if directory.resolve().is_relative_to(source) or directory.resolve().is_relative_to(root):
        raise CacheMiss('cache must stay outside source and checkout')
    trust_paths.inspect(directory)
    return root,bank,directory


def read_private(path,maximum):
    trust_paths.record(path)
    if not path.exists():return None
    fd=os.open(path,os.O_RDONLY|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_NONBLOCK',0))
    with os.fdopen(fd,'rb') as stream:
        info=os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_nlink!=1 or info.st_uid!=os.geteuid() or info.st_mode&0o077:
            raise CacheMiss('cache record permissions')
        raw=stream.read(maximum+1)
    if len(raw)>maximum:raise CacheMiss('cache record bound')
    return raw


def key(directory,*,create=False):
    path=directory/'key'
    try:raw=read_private(path,4096)
    except CacheMiss:
        if not create:raise
        info=path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.geteuid() or info.st_nlink!=1 or info.st_mode&0o077:
            raise
        # Preserve an oversized private corrupt key without reading it in full.
        os.rename(path,directory/('key-recovery-'+uuid.uuid4().hex));raw=None
    if raw is not None and len(raw)==32:return raw
    if not create:raise CacheMiss('cache key unavailable')
    if raw is not None:
        storage.replace(directory/('key-recovery-'+uuid.uuid4().hex),raw,None)
    fresh=secrets.token_bytes(32)
    try:storage.replace(path,fresh,digest(raw))
    except (ContractError,OSError):
        current=read_private(path,4096)
        if current is None or len(current)!=32:raise CacheMiss('cache key publication race')
        return current
    if read_private(path,4096)!=fresh:raise CacheMiss('cache key readback')
    return fresh


def binding(root,bank,project):
    return {'repo_root':str(root),'bank_root':str(bank),'project_id':project,
            'repository_settings_sha256':digest(compact(repository_config.settings(root)))}


def pointer(directory):
    raw=read_private(directory/'current.json',4096)
    if raw is None:raise CacheMiss('catalog missing')
    value=json.loads(raw)
    if (not isinstance(value,dict) or set(value)!={'schema','file'} or value['schema']!=SCHEMA
            or not re.fullmatch(r'g-[0-9a-f]{32}\.catalog',value['file'])):raise CacheMiss('catalog pointer format')
    return value,raw


@contextmanager
def publication_lock(directory):
    """Optional, nonblocking OS lock; no PID marker or stale-lock recovery."""
    path=directory/'publication.lock';trust_paths.record(path)
    if not path.exists() and len(namespace(directory))>=MAX_NAMESPACE:raise CacheMiss('cache namespace capacity')
    fd=os.open(path,os.O_RDWR|os.O_CREAT|getattr(os,'O_NOFOLLOW',0),0o600)
    try:
        info=os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.geteuid() or info.st_nlink!=1 or info.st_mode&0o077:
            raise CacheMiss('cache publication lock permissions')
        try:fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB)
        except BlockingIOError as exc:raise CacheMiss('another optional publisher is active') from exc
        yield
    finally:os.close(fd)


def namespace(directory):
    values=[]
    with os.scandir(directory) as entries:
        for entry in entries:
            if len(values)>=MAX_NAMESPACE:raise CacheMiss('cache namespace needs bounded maintenance')
            values.append(Path(entry.path))
    return values


def authenticate(raw,root,bank,project,secret,*,strict_settings=True):
    first,separator,payload=raw.partition(b'\n')
    if not separator or len(first)>MAX_HEADER:raise CacheMiss('catalog header bound')
    header=json.loads(first);fields={'schema','binding','engine','payload_bytes','payload_sha256','mac'}
    expected=binding(root,bank,project) if strict_settings else {'repo_root':str(root),'bank_root':str(bank),'project_id':project}
    if (not isinstance(header,dict) or set(header)!=fields or header['schema']!=SCHEMA
            or not isinstance(header['binding'],dict)
            or (header['binding']!=expected if strict_settings else any(header['binding'].get(k)!=v for k,v in expected.items()))
            or type(header['payload_bytes']) is not int or not 0<header['payload_bytes']<=MAX_CACHE
            or len(payload)!=header['payload_bytes'] or digest(payload)!=header['payload_sha256']):raise CacheMiss('catalog binding/digest')
    unsigned={k:v for k,v in header.items() if k!='mac'}
    expected=hmac.new(secret,compact(unsigned)+b'\n'+payload,hashlib.sha256).hexdigest()
    if not isinstance(header['mac'],str) or not hmac.compare_digest(header['mac'],expected):raise CacheMiss('catalog authentication')
    return header,payload


def collect_owned(directory,root,bank,project,secret,keep=None):
    names=namespace(directory)
    budget=0
    for path in names:
        if path.name==keep or not re.fullmatch(r'g-[0-9a-f]{32}\.catalog',path.name):continue
        try:
            remaining=2*MAX_CACHE-budget
            if remaining<=0:break
            allowance=min(MAX_CACHE+MAX_HEADER+1,remaining)
            # Charge the attempted bounded read even when its oversized input
            # is rejected. Corrupt records must not reset the aggregate budget.
            budget+=allowance+1
            raw=read_private(path,allowance)
            if raw is None:continue
            budget-=allowance+1-len(raw)
            authenticate(raw,root,bank,project,secret,strict_settings=False)
        except Exception:
            continue  # Preserve unknown, corrupted or foreign recovery evidence.
        path.unlink()


def invalidate(repo,project,expected_pointer):
    try:
        root,bank,directory=locations(repo)
        with publication_lock(directory):
            value,raw=pointer(directory)
            if raw!=expected_pointer:return
            secret=key(directory)
            record=read_private(directory/value['file'],MAX_CACHE+MAX_HEADER+1)
            if record is None:return
            authenticate(record,root,bank,project,secret)
            (directory/'current.json').unlink()
            collect_owned(directory,root,bank,project,secret)
    except Exception:
        pass  # An unusable cache never blocks the full canonical reader.


def invalidate_after_change(repo,current_hashes):
    """Forget/correction/recovery removes stale owned metadata after commit."""
    try:
        root,bank,directory=locations(repo)
        if not directory.exists():return 'not-present'
        with publication_lock(directory):
            value,_=pointer(directory);secret=key(directory)
            raw=read_private(directory/value['file'],MAX_CACHE+MAX_HEADER+1)
            if raw is None:return 'not-present'
            header_end=raw.find(b'\n',0,MAX_HEADER+1)
            if header_end<0:raise CacheMiss('catalog header bound')
            project=json.loads(raw[:header_end])['binding']['project_id']
            header,payload=authenticate(raw,root,bank,project,secret,strict_settings=False)
            data=json.loads(payload)
            if data['hashes']==current_hashes:return 'current'
            (directory/'current.json').unlink()
            collect_owned(directory,root,bank,project,secret)
            return 'invalidated'
    except Exception:
        return 'deferred-to-canonical-fallback'


class Catalog:
    def __init__(self,root,bank,engine,data,header,pointer_raw):
        self.root,self.bank,self.engine,self.data,self.header=root,bank,engine,data,header
        self.hashes=data['hashes'];self.checked=data['checked']
        self.entries={entry['path']:entry for entry in self.checked['entries']}
        self.payload_bytes_read=0;self.payload_file_reads=0;self.parsed_records=0
        self.pointer_raw=pointer_raw

    def selected_entries(self,selected):
        paths=set(selected)|set(self.data['oversized'])
        for path in selected:
            entry=self.entries.get(path,{})
            if 'id' in entry and 'version' in entry:
                paths.update(self.data['references'].get(entry['id']+'/'+str(entry['version']),()))
        return [self.entries[path] for path in sorted(paths) if path in self.entries]

    def source(self,path,maximum=16384):
        target=self.bank/path
        fd=os.open(target,os.O_RDONLY|getattr(os,'O_NOFOLLOW',0)|getattr(os,'O_NONBLOCK',0))
        with os.fdopen(fd,'rb') as stream:
            info=os.fstat(stream.fileno())
            if not stat.S_ISREG(info.st_mode) or info.st_nlink!=1:raise CacheMiss('source no longer ordinary')
            raw=stream.read(maximum+1)
        self.payload_bytes_read+=len(raw);self.payload_file_reads+=1
        if len(raw)<=maximum and digest(raw)!=self.hashes[path]:raise CacheMiss('selected source changed')
        return raw

    def parsed(self,path,text):
        metadata,body=read_frontmatter(text,allowed=records.MODEL['properties'])
        kind,metadata=records.validate(metadata,body)
        self.parsed_records+=1
        if kind!=self.entries[path]['format']:raise CacheMiss('selected format differs')
        return kind,metadata,body

    def recheck(self,owner=None):
        writes.fault('catalog-final-source-recheck')
        root,bank=writes.bank_path(self.root)
        if root!=self.root or bank!=self.bank:raise CacheMiss('bank location changed')
        bank_observation.markers(bank,owner)
        metrics={};physical,hashes,_=bank_observation.scan(bank,read_sources=False,metrics=metrics)
        bank_observation.physical_contract(physical)
        if hashes!=self.hashes or normalized_physical(physical)!=self.data['physical']:
            invalidate(self.root,self.checked['project_id'],self.pointer_raw)
            raise CacheMiss('actual canonical bytes changed')
        for path in self.checked['omitted']:
            if (bank/path).exists() or (bank/path).is_symlink():raise CacheMiss('omitted path occupied')
        current_physical,current_stamps=physical_inventory(bank)
        if current_physical!=self.data['physical'] or current_stamps!=self.data['stamps']:
            raise CacheMiss('late source change')
        bank_observation.markers(bank,owner)
        coherent(self.engine)
        return metrics


def load_catalog(repo,project,*,owner=None):
    if os.environ.get('AGENT_MEMORY_NO_RECALL_CATALOG')=='1':raise CacheMiss('disabled')
    root,bank,directory=locations(repo);bank_observation.markers(bank,owner);engine=generation()
    value,pointer_raw=pointer(directory);secret=key(directory);raw=read_private(directory/value['file'],MAX_CACHE+MAX_HEADER+1)
    if raw is None:raise CacheMiss('catalog generation missing')
    header,payload=authenticate(raw,root,bank,project,secret)
    if header['engine']!=engine:
        invalidate(repo,project,pointer_raw);raise CacheMiss('catalog engine changed')
    data=json.loads(payload)
    if (not isinstance(data,dict) or set(data)!={'schema','hashes','checked','physical','stamps','references','oversized'}
            or data['schema']!=SCHEMA or data['checked']['project_id']!=project
            or len(data['hashes'])>bank_observation.MAX_FILES):raise CacheMiss('catalog payload shape')
    # Stamps only choose a possible witness. Catalog.recheck always hashes all
    # actual bytes before qualified output, even if these hints were forged.
    physical,stamps=physical_inventory(bank)
    if physical!=data['physical'] or stamps!=data['stamps']:
        invalidate(repo,project,pointer_raw);raise CacheMiss('catalog source hint changed')
    bank_observation.markers(bank,owner)
    return Catalog(root,bank,engine,data,header,pointer_raw)


def publish(observation,engine):
    if os.environ.get('AGENT_MEMORY_NO_RECALL_CATALOG')=='1':return False
    coherent(engine)
    root,bank,directory=locations(observation.root)
    for path in (directory.parent,directory):
        trust_paths.inspect(path);path.mkdir(parents=True,exist_ok=True,mode=0o700)
    trust_paths.state(directory)
    with publication_lock(directory):
        # Capacity is established before creating any key/generation/pointer.
        # Unknown evidence is preserved; cache exhaustion never fails recall.
        if len(namespace(directory))>MAX_NAMESPACE-4:raise CacheMiss('cache namespace capacity')
        observation.recheck('catalog-seal-source-recheck')
        physical,stamps=physical_inventory(bank)
        if physical!=normalized_physical(observation.physical) or set(stamps)!=set(observation.hashes):
            raise CacheMiss('source topology changed before cache publication')
        references={};oversized=[]
        for path,(kind,metadata,body) in observation.parsed.items():
            if kind!='standalone-record':continue
            if len(observation.sources[path])>16384 and metadata['status']=='active':oversized.append(path)
            for ref in metadata['corrects']+metadata['supersedes']:
                references.setdefault(ref['id']+'/'+str(ref['version']),[]).append(path)
        checked={'project_id':observation.checked['project_id'],'entries':observation.checked['entries'],
                 'omitted':list(observation.checked['omitted'])}
        data={'schema':SCHEMA,'hashes':observation.hashes,'checked':checked,'physical':physical,'stamps':stamps,
              'references':{k:sorted(set(v)) for k,v in references.items()},'oversized':sorted(oversized)}
        payload=compact(data)
        if len(payload)>MAX_CACHE:raise CacheMiss('catalog encoded bound')
        secret=key(directory,create=True);old=read_private(directory/'current.json',4096)
        header={'schema':SCHEMA,'binding':binding(root,bank,observation.checked['project_id']),'engine':engine,
                'payload_bytes':len(payload),'payload_sha256':digest(payload)}
        header['mac']=hmac.new(secret,compact(header)+b'\n'+payload,hashlib.sha256).hexdigest()
        record=compact(header)+b'\n'+payload
        coherent(engine)
        bank_observation.markers(bank,observation.owner)
        name='g-'+uuid.uuid4().hex+'.catalog'
        storage.replace(directory/name,record,None)
        storage.replace(directory/'current.json',compact({'schema':SCHEMA,'file':name}),digest(old))
        try:collect_owned(directory,root,bank,observation.checked['project_id'],secret,keep=name)
        except Exception:pass
    return True


def optional_failure(exc):
    return isinstance(exc,(CacheMiss,ContractError,OSError,ValueError,UnicodeError,TypeError,KeyError,AttributeError,RecursionError))
