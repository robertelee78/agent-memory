"""Private hook bookkeeping; queue attempts are not completion receipts."""
import hashlib
import json
import os
import stat


def reserve(root, session, source, action="reserve", repo=None, publication_request=None, assessment=None):
    """Persist intent and attempt before queueing; never retry an existing intent."""
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
            if action != 'reserve':
                raise ValueError('session repository unavailable for acknowledgment')
            return {'status': 'not-applicable'}
        checkout = Path(found.stdout.strip()).resolve(strict=True)
        candidate = repository_config.bank_path(checkout)
        if not os.path.lexists(candidate):
            if action != 'reserve':
                raise ValueError('session bank unavailable for acknowledgment')
            return {'status': 'not-applicable'}
        repo = str(checkout)
    parts = os.path.normpath(root).split(os.sep)[1:] + ['experimental-boundary', hashlib.sha256(session.encode()).hexdigest()]
    fd = os.open('/', os.O_RDONLY | os.O_DIRECTORY)
    identities = []
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
            if index < len(parts) - 3 and (info.st_uid not in (0, os.getuid()) or (info.st_mode & 0o022 and not (info.st_uid == 0 and info.st_mode & stat.S_ISVTX))):
                raise ValueError('state ancestor is not trusted')
            if index == len(parts) - 3 and (info.st_uid != os.getuid() or info.st_mode & 0o022):
                raise ValueError('state root must be owned and not group/world writable')
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
            completed = os.open(done, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
        except FileNotFoundError:
            completed = None
        if completed is not None:
            try:
                info = os.fstat(completed)
                if not stat.S_ISREG(info.st_mode) or info.st_uid != os.getuid() or info.st_nlink != 1 or info.st_mode & 0o077 or info.st_size > 4096:
                    raise ValueError('invalid assessment marker')
                record = json.loads(os.read(completed, 4097))
                if (not isinstance(record, dict) or set(record) != {'status', 'repository', 'request_hash', 'witnessed_commit'}
                        or record['status'] not in ('published', 'assessed-no-learning')
                        or record['repository'] != repository_context):
                    raise ValueError('invalid or different completion marker')
                if action == 'reserve':
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
                    existing = os.open(key + suffix, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
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
                        if (not isinstance(intent, dict) or set(intent) != {'schema', 'turn_key', 'repository'} or intent['schema'] != 'agent-memory/experimental-boundary-intent-v1' or intent['turn_key'] != key or intent['repository'] != repository_context):
                            raise ValueError('pending turn binding changed')
                finally:
                    os.close(existing)
            return {'status': 'pending-turn'}
        if action in ('assessed-no-learning', 'published'):
            for suffix in ('.intent', '.attempt'):
                try:
                    existing = os.open(key + suffix, os.O_RDONLY | os.O_NOFOLLOW, dir_fd=fd)
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
                        if (not isinstance(pending_data, dict) or set(pending_data) != {'schema', 'turn_key', 'repository'} or pending_data['schema'] != 'agent-memory/experimental-boundary-intent-v1' or pending_data['turn_key'] != key or pending_data['repository'] != repository_context):
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
        if action != 'reserve':
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
            return {'status': 'pending', 'reason': 'existing-intent'}
        try:
            payload = json.dumps({'schema': 'agent-memory/experimental-boundary-intent-v1', 'turn_key': key, 'repository': repository_context}).encode()
            if os.write(item, payload) != len(payload):
                raise OSError('short intent write')
            os.fsync(item)
        finally:
            os.close(item)
        os.fsync(fd)
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
        os.close(fd)

if __name__ == '__main__':
    import sys
    try:
        request = json.load(sys.stdin)
        print(json.dumps(reserve(request['root'], request['session'], request['source'], request.get('action', 'reserve'), request.get('repo'), request.get('publication_request'), request.get('assessment'))))
    except Exception:
        print(json.dumps({'status': 'pending', 'reason': 'ledger-unavailable'}))
        sys.exit(1)
