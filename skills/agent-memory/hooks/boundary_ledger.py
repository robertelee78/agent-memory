"""Private hook bookkeeping; queue attempts are not completion receipts."""
import hashlib
import json
import os
import stat
import fcntl
import re
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parent.parent / "scripts"))
from agent_memory import trust_paths


def read_marker(fd,name):
    item=os.open(name,os.O_RDONLY|os.O_NOFOLLOW|os.O_NONBLOCK,dir_fd=fd)
    try:
        info=os.fstat(item)
        if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_nlink!=1 or info.st_mode&0o077 or info.st_size>4096:raise ValueError('unsafe source evidence')
        return json.loads(os.read(item,4097))
    finally:os.close(item)


def valid_intent(value,key,repository,session,source):
    legacy={'schema','turn_key','repository'}
    return (isinstance(value,dict) and value.get('turn_key')==key and value.get('repository')==repository
            and ((set(value)==legacy and value.get('schema')=='agent-memory/experimental-boundary-intent-v1')
                 or (set(value)==legacy|{'session_id','source_id'} and value.get('schema')=='agent-memory/opencode-source-intent-v2'
                     and value['session_id']==session and value['source_id']==source)))


def valid_completion(value,repository):
    return (isinstance(value,dict) and set(value)=={'status','repository','request_hash','witnessed_commit'}
            and value['repository']==repository and value['status'] in ('published','assessed-no-learning')
            and isinstance(value['request_hash'],str) and re.fullmatch('[0-9a-f]{64}',value['request_hash'])
            and ((value['status']=='assessed-no-learning' and value['witnessed_commit'] is None)
                 or (value['status']=='published' and isinstance(value['witnessed_commit'],str)
                     and re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}',value['witnessed_commit']))))


def wake(fd,key,repository,session,source,fingerprint):
    if not isinstance(fingerprint,str) or not re.fullmatch('[0-9a-f]{64}',fingerprint):raise ValueError('invalid wake fingerprint')
    lock=os.open(key+'.wake-lock',os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW|os.O_NONBLOCK,0o600,dir_fd=fd)
    try:
        info=os.fstat(lock)
        if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_nlink!=1 or info.st_mode&0o077:raise ValueError('unsafe wake lock')
        fcntl.flock(lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        try:
            complete=read_marker(fd,key+'.assessed')
        except FileNotFoundError:complete=None
        if complete is not None:
            if not valid_completion(complete,repository):raise ValueError('invalid completion')
            return {'status':'already-published' if complete['status']=='published' else 'assessed-no-learning'}
        if not valid_intent(read_marker(fd,key+'.intent'),key,repository,session,source):raise ValueError('source not enrolled')
        for number in range(2):
            name=key+'.wake-'+str(number)
            try:previous=read_marker(fd,name)
            except FileNotFoundError:break
            if (not isinstance(previous,dict) or set(previous)!={'schema','fingerprint'} or previous['schema']!='agent-memory/opencode-wake-v1'
                    or not isinstance(previous['fingerprint'],str) or not re.fullmatch('[0-9a-f]{64}',previous['fingerprint'])):raise ValueError('unsafe wake evidence')
            if previous['fingerprint']==fingerprint:return {'status':'pending','reason':'wake-already-attempted'}
        else:return {'status':'pending','reason':'wake-cap'}
        item=os.open(name,os.O_WRONLY|os.O_CREAT|os.O_EXCL|os.O_NOFOLLOW,0o600,dir_fd=fd)
        try:
            payload=json.dumps({'schema':'agent-memory/opencode-wake-v1','fingerprint':fingerprint}).encode()
            if os.write(item,payload)!=len(payload):raise OSError('short wake write')
            os.fsync(item)
        finally:os.close(item)
        os.fsync(fd)
        return {'status':'reserved','turn_key':key,'attempt':number+1}
    finally:os.close(lock)


def backlog(fd,repository,offset=0):
    if type(offset) is not int or not 0 <= offset <= 4096:raise ValueError('invalid backlog offset')
    parent=os.open('..',os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=fd)
    sources=[];legacy=0
    try:
        with os.scandir(parent) as directories:
            for scanned,directory in enumerate(directories,1):
                if scanned>4096:return {'status':'pending','reason':'backlog-capacity'}
                child=os.open(directory.name,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=parent)
                try:
                    info=os.fstat(child)
                    if info.st_uid!=os.getuid() or stat.S_IMODE(info.st_mode)!=0o700:raise ValueError('unsafe backlog directory')
                    with os.scandir(child) as entries:
                        for count,entry in enumerate(entries,1):
                            if count>20000:return {'status':'pending','reason':'backlog-capacity'}
                            if not re.fullmatch(r'[0-9a-f]{64}\.intent',entry.name):continue
                            value=read_marker(child,entry.name)
                            if not isinstance(value,dict):raise ValueError('invalid pending source')
                            if value.get('repository')!=repository:continue
                            if value.get('schema')=='agent-memory/experimental-boundary-intent-v1':legacy+=1;continue
                            session,source=value.get('session_id'),value.get('source_id')
                            if not isinstance(session,str) or not isinstance(source,str) or not session or not source or len(session)>512 or len(source)>512:raise ValueError('invalid native identifiers')
                            key=hashlib.sha256(json.dumps([session,source],ensure_ascii=True,separators=(',',':')).encode()).hexdigest()
                            if directory.name!=hashlib.sha256(session.encode()).hexdigest() or entry.name!=key+'.intent' or not valid_intent(value,key,repository,session,source):raise ValueError('invalid native source binding')
                            sources.append({'session_id':session,'source_id':source})
                            if len(sources)>4096:return {'status':'pending','reason':'backlog-capacity'}
                finally:os.close(child)
    finally:os.close(parent)
    sources.sort(key=lambda item:(item['session_id'],item['source_id']))
    next_offset=offset+16 if offset+16<len(sources) else None
    inventory=hashlib.sha256(json.dumps({'sources':sources,'legacy':legacy},sort_keys=True,separators=(',',':')).encode()).hexdigest()
    return {'status':'backlog','sources':sources[offset:offset+16],'pending_count':len(sources),'more_pending':next_offset is not None,'offset':offset,'next_offset':next_offset,'inventory_sha256':inventory,'legacy_pending_count':legacy,'publication_verified':False}


def verify_namespace(parts,identities):
    check=os.open('/',os.O_RDONLY|os.O_DIRECTORY)
    try:
        protected,_=trust_paths.inspect_entry('/',os.fstat(check),False)
        for index,(part,identity) in enumerate(zip(parts,identities)):
            next_check=os.open(part,os.O_RDONLY|os.O_DIRECTORY|os.O_NOFOLLOW,dir_fd=check)
            os.close(check);check=next_check
            info=os.fstat(check)
            if (info.st_dev,info.st_ino)!=identity:raise ValueError('state namespace changed; source remains pending')
            protected,_=trust_paths.inspect_entry('/'+'/'.join(parts[:index+1]),info,protected)
            if index>=len(parts)-2 and (info.st_uid!=os.getuid() or stat.S_IMODE(info.st_mode)!=0o700):raise ValueError('boundary state directory is not private and owned')
    finally:os.close(check)


def reserve(root, session, source, action="reserve", repo=None, publication_request=None, assessment=None, fingerprint=None, offset=0):
    """Persist native enrollment and bounded wakes; preserve legacy reservations."""
    if (not os.path.isabs(root) or not isinstance(session, str) or not isinstance(source, str)
            or not session or not source or len(session) > 512 or len(source) > 512):
        raise ValueError('absolute state root and turn IDs required')
    if repo is not None:
        import subprocess
        import sys
        from pathlib import Path
        sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'scripts'))
        from agent_memory import repository_config
        cwd = Path(repo).resolve(strict=True)
        if not cwd.is_dir():
            raise ValueError('session location unavailable')
        found = subprocess.run(['git', '-C', str(cwd), 'rev-parse', '--show-toplevel'],
                               capture_output=True, text=True, timeout=5)
        if found.returncode:
            if any(os.path.lexists(parent / '.git') for parent in (cwd, *cwd.parents)):
                raise ValueError('repository cannot be inspected')
            if action not in ('reserve','enroll','backlog'):
                raise ValueError('session repository unavailable for acknowledgment')
            return {'status': 'not-applicable'}
        checkout = Path(found.stdout.strip()).resolve(strict=True)
        candidate = repository_config.bank_path(checkout)
        if not os.path.lexists(candidate):
            if action not in ('reserve','enroll','backlog'):
                raise ValueError('session bank unavailable for acknowledgment')
            return {'status': 'not-applicable'}
        common_result = subprocess.run(['git','-C',str(checkout),'rev-parse','--git-common-dir'],capture_output=True,text=True,timeout=5)
        if common_result.returncode:raise ValueError('repository namespace unavailable')
        common=(checkout/common_result.stdout.strip()).resolve(strict=True)
        private=Path(root).resolve()
        if (private.is_relative_to(checkout) or private.is_relative_to(common)
                or (common.name=='.git' and private.is_relative_to(common.parent))):
            raise ValueError('boundary ledger state must stay outside repository and common Git data')
        repo = str(checkout)
    parts = os.path.normpath(root).split(os.sep)[1:] + ['experimental-boundary', hashlib.sha256(session.encode()).hexdigest()]
    fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    protected, _ = trust_paths.inspect_entry('/', os.fstat(fd), False)
    identities = []
    completion_lock = None
    try:
        for index, part in enumerate(parts):
            if index >= len(parts) - 2:
                try:
                    os.mkdir(part, 0o700, dir_fd=fd)
                    os.fsync(fd)
                except FileExistsError:
                    pass
            next_fd = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
            os.close(fd)
            fd = next_fd
            info = os.fstat(fd)
            identities.append((info.st_dev, info.st_ino))
            protected, _ = trust_paths.inspect_entry('/' + '/'.join(parts[:index+1]), info, protected)
            if index == len(parts) - 3 and info.st_uid != os.getuid():
                raise ValueError('state root must be owned by the executing user')
            if index >= len(parts) - 2 and (info.st_uid != os.getuid() or stat.S_IMODE(info.st_mode) != 0o700):
                raise ValueError('boundary state directory is not private and owned')
        repository_context = None
        if repo is not None:
            import sys
            from pathlib import Path
            sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'scripts'))
            from agent_memory import bank_writes, scope_binding
            if not isinstance(repo, str) or not os.path.isabs(repo):
                raise ValueError('canonical session repository required')
            repository, _ = bank_writes.bank_path(repo)
            context = scope_binding.require(repository)
            repository_context = {'repo': str(repository), 'project_id': context['project_id']}
        key = hashlib.sha256(json.dumps([session, source], ensure_ascii=True, separators=(',', ':')).encode()).hexdigest()
        done = key + '.assessed'
        if action == 'backlog':return backlog(fd, repository_context, offset)
        def verify_enrollment():
            if not valid_intent(read_marker(fd,key+'.intent'),key,repository_context,session,source):raise ValueError('native enrollment readback differs')
            verify_namespace(parts,identities)
            if repository_context is not None:
                scope_binding.require(repository,context['project_id'],expected=context)
        if action in ('inspect','assessed-no-learning','published'):
            try: native=read_marker(fd,key+'.intent').get('schema')=='agent-memory/opencode-source-intent-v2'
            except FileNotFoundError:native=False
            try: os.stat(key+'.wake-lock',dir_fd=fd,follow_symlinks=False); native=True
            except FileNotFoundError:pass
            if native:
                completion_lock=os.open(key+'.wake-lock',os.O_RDWR|os.O_CREAT|os.O_NOFOLLOW|os.O_NONBLOCK,0o600,dir_fd=fd)
                info=os.fstat(completion_lock)
                if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.getuid() or info.st_nlink!=1 or info.st_mode&0o077:raise ValueError('unsafe completion lock')
                fcntl.flock(completion_lock,fcntl.LOCK_EX|fcntl.LOCK_NB)
        request_hash = None
        witnessed = None
        if action == 'assessed-no-learning':
            import sys
            from pathlib import Path
            sys.path.insert(0, str(Path(__file__).resolve().parent.parent / 'scripts'))
            from agent_memory import boundary_assessment
            boundary_assessment.validate(assessment, source)
            request_hash = hashlib.sha256(json.dumps(assessment, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
        if action == 'published':
            from agent_memory import memory_publication, sharing_recovery
            if repository_context is None or not isinstance(publication_request, dict):
                raise ValueError('publication requires bound repository and request')
            request_hash = hashlib.sha256(json.dumps(publication_request, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()).hexdigest()
            memory_publication.request_value(repository, publication_request)
            proof = publication_request['proof']
            if proof.get('blocked_paths') or proof.get('blocked_components'):
                raise ValueError('conflicted publication remains pending')
            replay = sharing_recovery.replay(repository, {'base': publication_request['base'], 'candidate': publication_request['candidate']},
                quarantine_records=proof.get('schema') == 'agent-memory/semantic-sharing-proof-v2')
            if not replay.get('intended_delta_landed'):
                raise ValueError('publication not witnessed')
            scope_binding.require(repository, context['project_id'], expected=context)
            witnessed = replay['witnessed_commit']
        try:
            completed = os.open(done, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
        except FileNotFoundError:
            completed = None
        if completed is not None:
            try:
                info = os.fstat(completed)
                if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1 or info.st_mode & 0o077 or info.st_size > 4096:
                    raise ValueError('invalid assessment marker')
                record = json.loads(os.read(completed, 4097))
                if not valid_completion(record,repository_context):
                    raise ValueError('invalid or different completion marker')
                if action in ('reserve','enroll','wake'):
                    return {'status': 'already-published' if record['status'] == 'published' else 'assessed-no-learning'}
                if action == 'inspect':
                    return {'status': 'completed-source'}
                if action != record['status'] or (action in ('published', 'assessed-no-learning') and request_hash != record['request_hash']):
                    raise ValueError('completion differs from original assessment')
                return {'status': action, 'already_completed': True, 'publication_verified': action == 'published',
                        'witnessed_commit': witnessed, 'source_publication_link_verified': False}
            finally:
                os.close(completed)
        if action == 'inspect':
            for suffix in ('.intent', '.attempt'):
                try:
                    existing = os.open(key + suffix, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
                except FileNotFoundError:
                    if suffix == '.attempt':
                        continue
                    raise
                try:
                    info = os.fstat(existing)
                    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1 or info.st_mode & 0o077 or info.st_size > 4096:
                        raise ValueError('invalid pending turn marker')
                    if suffix == '.intent':
                        intent = json.loads(os.read(existing, 4097))
                        if not valid_intent(intent,key,repository_context,session,source):
                            raise ValueError('pending turn binding changed')
                finally:
                    os.close(existing)
            return {'status': 'pending-turn'}
        if action in ('assessed-no-learning', 'published'):
            for suffix in ('.intent', '.attempt'):
                try:
                    existing = os.open(key + suffix, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
                except FileNotFoundError:
                    if suffix == '.attempt':
                        continue
                    raise
                try:
                    info = os.fstat(existing)
                    if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1 or info.st_mode & 0o077 or info.st_size > 4096:
                        raise ValueError('invalid turn marker')
                    if suffix == '.intent':
                        pending_data = json.loads(os.read(existing, 4097))
                        if not valid_intent(pending_data,key,repository_context,session,source):
                            raise ValueError('session repository or project changed')
                finally:
                    os.close(existing)
            complete = os.open(done, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=fd)
            try:
                payload = json.dumps({'status': action, 'repository': repository_context, 'request_hash': request_hash, 'witnessed_commit': witnessed}, sort_keys=True).encode()
                if os.write(complete, payload) != len(payload):
                    raise OSError('short assessment write')
                os.fsync(complete)
            finally:
                os.close(complete)
            os.fsync(fd)
            for suffix in ('.intent', '.attempt'):
                try:
                    os.unlink(key + suffix, dir_fd=fd)
                except FileNotFoundError:
                    if suffix != '.attempt':
                        raise
            os.fsync(fd)
            return {'status': action, 'publication_verified': action == 'published', 'witnessed_commit': witnessed, 'source_publication_link_verified': False}
        if action=='wake':return wake(fd,key,repository_context,session,source,fingerprint)
        if action not in ('reserve','enroll'):
            raise ValueError('unsupported acknowledgment')
        # An operator can raise this bound without deleting unresolved intents.
        try:
            limit = int(os.environ.get('AGENT_MEMORY_BOUNDARY_LEDGER_LIMIT', '20000'))
        except ValueError:
            return {'status': 'pending', 'reason': 'ledger-limit-invalid'}
        if not 2 <= limit <= 1000000:
            return {'status': 'pending', 'reason': 'ledger-limit-invalid'}
        with os.scandir(fd) as entries:
            for count, _ in enumerate(entries, 1):
                if count >= limit:
                    return {'status': 'pending', 'reason': 'ledger-capacity'}
        flags = os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW
        try:
            item = os.open(key + '.intent', flags, 0o600, dir_fd=fd)
        except FileExistsError:
            if action=='enroll':
                verify_enrollment()
                return {'status':'enrolled','already_enrolled':True}
            return {'status': 'pending', 'reason': 'existing-intent'}
        try:
            value={'schema': 'agent-memory/experimental-boundary-intent-v1', 'turn_key': key, 'repository': repository_context}
            if action=='enroll':value.update(schema='agent-memory/opencode-source-intent-v2',session_id=session,source_id=source)
            payload=json.dumps(value).encode()
            if len(payload)>4096:raise ValueError('native source evidence exceeds bound')
            if os.write(item, payload) != len(payload):
                raise OSError('short intent write')
            os.fsync(item)
        finally:
            os.close(item)
        os.fsync(fd)
        if action=='enroll':
            verify_enrollment()
            return {'status':'enrolled','turn_key':key}
        attempt = os.open(key + '.attempt', flags, 0o600, dir_fd=fd)
        try:
            marker = b'queue-attempt-uncertain\n'
            if os.write(attempt, marker) != len(marker):
                raise OSError('short attempt write')
            os.fsync(attempt)
        finally:
            os.close(attempt)
        os.fsync(fd)
        # Detect namespace rebinding observed during the reservation. This is not
        # atomic against an uncooperative same-UID process after the final check.
        check = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
        try:
            for part, identity in zip(parts, identities):
                next_check = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=check)
                os.close(check)
                check = next_check
                info = os.fstat(check)
                if (info.st_dev, info.st_ino) != identity:
                    raise ValueError('state namespace changed; queue attempt pending')
        finally:
            os.close(check)
        return {'status': 'reserved', 'turn_key': key}
    finally:
        if completion_lock is not None:os.close(completion_lock)
        os.close(fd)

if __name__ == '__main__':
    import sys
    try:
        request = json.load(sys.stdin)
        def complete(_repo, _value):
            result = reserve(request['root'], request['session'], request['source'], request.get('action', 'reserve'), request.get('repo'), request.get('publication_request'), request.get('assessment'), request.get('fingerprint'), request.get('offset',0))
            return dict(result, verified=result.get('status') == request.get('action'))
        if request.get('refresh_current') is True:
            if request.get('action') != 'assessed-no-learning' or not request.get('repo'):
                raise ValueError('combined native check requires an actual no-learning assessment and repository')
            from agent_memory import turn_check
            value = {'session':request['session'], 'prompt':request['source'], 'outcome':'no-learning', 'assessment':request.get('assessment')}
            result = turn_check.check(request['repo'], value, complete, 'opencode')
            if result.get('verified') is not True and result.get('notice_required') is not False:
                result['failure_notice'] = turn_check.failure_notice(result)
            # Native routine responses retain stage truth without echoing source reviews.
            result['assessment'] = {key:val for key,val in result['assessment'].items()
                                    if key in ('verified','status','reason','repair')}
        else:
            result = complete(None, None)
            result.pop('verified', None)
        print(json.dumps(result))
    except Exception:
        print(json.dumps({'status': 'pending', 'reason': 'ledger-unavailable'}))
        sys.exit(1)
