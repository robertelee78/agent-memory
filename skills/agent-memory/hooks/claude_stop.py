#!/usr/bin/env python3
"""Opt-in Claude asyncRewake guidance; not a capture or publication receipt.

Register only as an asyncRewake Stop hook. A second Stop in the resulting
continuation is ignored; the existing agent performs any permitted work.
"""
import json
import hashlib
import os
import subprocess
import stat
import uuid
from pathlib import Path
import sys

MAX_INPUT = 64 * 1024


def observed_stop(repo, event, pack):
    """A native prompt already enrolled by v2 can finish before final quietly."""
    from agent_memory import turn_sources
    session, prompt = event.get('session_id'), event.get('prompt_id')
    if not isinstance(session, str) or not isinstance(prompt, str):
        return None  # Legacy source handling still reports unsupported evidence.
    state = turn_sources.inspect(repo, 'claude', session, prompt)
    if not state['sources'] and state['faults'] == ['source-missing']:
        return None
    if not state['sources'] and not state['faults']:
        return None
    if state['complete']:
        notice = turn_sources.pending_notice(repo, 'claude', session, prompt)
        if notice:
            print(json.dumps({'systemMessage': 'Agent Memory has earlier retained pending sources; inspect am claude-boundary --list-pending. Current work may continue.'}))
        return 0
    message = ('Agent Memory pass for native Claude session ' + session + ', prompt ' + prompt +
               '. Read ' + str(pack / 'turn/GUIDE.md') +
               '; inspect am claude-boundary --list-pending --session ' + session + ' --prompt ' + prompt +
               '. Assess exact original sources; complete only verified work. Preserve the substantive task answer. '
               'No-learning success is silent. If tools/evidence are unavailable, leave memory pending. The original answer is already retained. Do not repeat it. End silently after success; report only one brief actionable pending qualification if necessary.')
    if len(message.encode()) > 2048:
        raise ValueError('guidance bound')
    if event['stop_hook_active']:
        print(json.dumps({'systemMessage': 'Agent Memory remains pending for retained native sources; preserve the task answer and inspect its original observations.'}))
        return 0
    if not turn_sources.reserve_wake(repo, 'claude', session, prompt, state['fingerprint'], hashlib.sha256(message.encode()).hexdigest()):
        print(json.dumps({'systemMessage': 'Agent Memory remains pending; its bounded continuation was already attempted. Preserve work and inspect the retained source.'}))
        return 0
    print(message, file=sys.stderr)
    return 2


def main():
    try:
        raw = sys.stdin.buffer.read(MAX_INPUT + 1)
        if len(raw) > MAX_INPUT:
            raise ValueError('oversized event')
        event = json.loads(raw)
        if not isinstance(event, dict) or event.get('hook_event_name') != 'Stop':
            raise ValueError('unsupported event')
        if type(event.get('stop_hook_active')) is not bool:
            raise ValueError('missing continuation guard')
        directory = Path(__file__).resolve().parent
        sys.path.insert(0, str(directory))
        sys.path.insert(0, str(directory.parent / 'scripts'))
        from claude_source import source
        from agent_memory import bank_writes, repository_config, scope_binding, trust_paths
        cwd = Path(event['cwd']).resolve(strict=True)
        if not cwd.is_dir():
            raise ValueError('working directory is unavailable')
        found = subprocess.run(['git', '-C', str(cwd), 'rev-parse', '--show-toplevel'],
                               capture_output=True, text=True, timeout=5)
        if found.returncode:
            if any(os.path.lexists(parent / '.git') for parent in (cwd, *cwd.parents)):
                raise ValueError('repository cannot be inspected')
            return 0  # No repository is not a memory failure.
        repo = Path(found.stdout.strip()).resolve(strict=True)
        candidate = repository_config.bank_path(repo)
        if not os.path.lexists(candidate):
            return 0  # A repository without a bank is not automatically initialized.
        repo, bank = bank_writes.bank_path(repo)
        context = scope_binding.require(repo)
        decision = observed_stop(repo, event, directory.parent)
        if decision is not None:
            return decision
        if event['stop_hook_active']:
            return 0
        scoped = source(event, require_final=False)
        operations, _ = bank_writes.state_paths(repo, bank, str(uuid.uuid4()))
        ledger = operations.parent / 'claude-boundaries'
        trust_paths.inspect(ledger)
        ledger.mkdir(parents=True, exist_ok=True, mode=0o700)
        trust_paths.state(ledger)
        key = hashlib.sha256((scoped['session'] + ':' + scoped['prompt']).encode()).hexdigest()
        dfd = os.open(ledger, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            # Preserve unresolved evidence and place a finite upper bound on
            # routine bookkeeping; never delete or silently evict pending work.
            try:
                donefd = os.open(key + '.done', os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=dfd)
            except FileNotFoundError:
                donefd = None
            if donefd is not None:
                try:
                    info = os.fstat(donefd)
                    if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid()
                            or info.st_nlink != 1 or info.st_mode & 0o077 or info.st_size > 4096):
                        os.close(donefd)
                        raise ValueError('unsafe completed boundary')
                    with os.fdopen(donefd, 'rb') as donefile:
                        completed = json.loads(donefile.read(4097))
                    if (completed.get('schema') != 'agent-memory/claude-boundary-completed-v1'
                            or completed.get('project_id') != context['project_id']
                            or completed.get('source_uuid') != scoped['source_uuid']
                            or completed.get('source_sha256') != scoped['source_sha256']):
                        raise ValueError('completed boundary does not match source')
                except (ValueError, AttributeError, TypeError):
                    raise ValueError('invalid completed boundary')
                from agent_memory import turn_sources
                if set(completed) != {'schema','project_id','source_uuid','source_sha256','outcome','assessment','assessment_is_agent_assertion','source_publication_link_verified','witnessed_commit','pending_sha256','input_sha256'}:
                    raise ValueError('malformed completed boundary')
                import re
                if not isinstance(completed['pending_sha256'], str) or not re.fullmatch('[0-9a-f]{64}', completed['pending_sha256']):
                    raise ValueError('invalid pending digest')
                receipt = {k: completed.get(k) for k in ['outcome','assessment','assessment_is_agent_assertion','source_publication_link_verified','witnessed_commit','input_sha256']}
                receipt.update(observation_id=scoped['source_uuid'], source_sha256=scoped['source_sha256'], publication_verified=completed['outcome']=='published', canonical_writes_performed=False)
                turn_sources.validate_receipt(receipt, scoped['source_uuid'], scoped['source_sha256'])
                try:
                    retained = turn_sources.read_bytes(ledger / (key + '.pending'), 4096)
                    old = json.loads(retained)
                    if hashlib.sha256(retained).hexdigest() != completed['pending_sha256']:
                        raise ValueError('pending source bytes differ')
                    if (not isinstance(old, dict) or set(old) != {'schema','project_id','source_uuid','source_sha256','assistant_sha256'} or old.get('schema') != 'agent-memory/claude-boundary-pending-v1' or old.get('project_id') != context['project_id'] or old.get('source_uuid') != scoped['source_uuid'] or old.get('source_sha256') != scoped['source_sha256']):
                        raise ValueError('pending source differs')
                except FileNotFoundError:
                    pass
                return 0
            with os.scandir(dfd) as entries:
                if sum(1 for entry in entries if entry.name.endswith('.pending')) >= 4096:
                    raise ValueError('boundary pending inventory is full')
            try:
                fd = os.open(key + '.pending', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600, dir_fd=dfd)
            except FileExistsError:
                print(json.dumps({'systemMessage':'Agent Memory has a retained pending boundary; inspect it before retrying.'}))
                return 0
            with os.fdopen(fd, 'wb') as stream:
                stream.write(json.dumps({'schema':'agent-memory/claude-boundary-pending-v1',
                    'project_id':context['project_id'], 'source_uuid':scoped['source_uuid'],
                    'source_sha256':scoped['source_sha256'], 'assistant_sha256':scoped['assistant_sha256']}).encode())
                stream.flush(); os.fsync(stream.fileno())
            os.fsync(dfd)
        finally:
            os.close(dfd)
        prompt = (directory / 'boundary-prompt.txt').read_text(encoding='utf-8').strip()
        prompt = prompt.replace('{{SKILL_DIR}}', str(directory.parent))
        if not prompt or len(prompt.encode('utf-8')) > 4096:
            raise ValueError('invalid instruction')
        # A legacy v1 turn still receives one short, source-bound continuation.
        prompt = ('Agent Memory: verify exact Claude source session=' + scoped['session'] +
                  ' prompt=' + scoped['prompt'] + ' source_uuid=' + scoped['source_uuid'] +
                  ' source_sha256=' + scoped['source_sha256'] + '. Verify that exact source in ' +
                  scoped['transcript'] + '; follow ' + str(directory.parent / 'turn/GUIDE.md') +
                  '. Acknowledge truthful no-learning or verified publication via am claude-boundary. '
                  'Preserve the substantive task answer; routine bookkeeping stays quiet. '
                  'Unavailable tools or evidence leave memory pending. The original answer is already retained. Do not repeat it. End silently after success; report only one brief actionable pending qualification if necessary.')
        if len(prompt.encode()) > 2048:
            raise ValueError('guidance bound')
        print(prompt, file=sys.stderr)
        return 2
    except (OSError, ValueError, TypeError, UnicodeError, KeyError, ImportError, subprocess.SubprocessError):
        # A failed hook is never an admission or publication receipt.
        print(json.dumps({'systemMessage': 'Agent Memory boundary could not verify its source or private state; capture and publication are pending. Preserve the bank and pending evidence, run am doctor in this repository, and inspect before retrying.'}))
        return 0


if __name__ == '__main__':
    sys.exit(main())
