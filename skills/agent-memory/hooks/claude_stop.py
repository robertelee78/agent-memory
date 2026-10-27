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
        if event['stop_hook_active']:
            return 0
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
        scoped = source(event)
        context = scope_binding.require(repo)
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
                donefd = os.open(key + '.done', os.O_RDONLY | os.O_NOFOLLOW, dir_fd=dfd)
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
        prompt += (' Source-bound Claude session ' + scoped['session'] + ', prompt ' + scoped['prompt']
                   + ', user message ' + scoped['source_uuid'] + ' (SHA-256 ' + scoped['source_sha256']
                   + '). Verify that exact source in ' + scoped['transcript']
                   + ' before attributing learning; the final assistant message SHA-256 is '
                   + scoped['assistant_sha256'] + '. If source access or verification fails, report pending.'
                   + ' After a source-bound no-learning assessment or verified publication, acknowledge this boundary via am claude-boundary using an explicit private input. If acknowledgment is unavailable, retain pending evidence.')
        print(prompt, file=sys.stderr)
        return 2
    except (OSError, ValueError, TypeError, UnicodeError, KeyError, ImportError, subprocess.SubprocessError):
        # A failed hook is never an admission or publication receipt.
        print(json.dumps({'systemMessage': 'Agent Memory boundary could not verify its source or private state; capture and publication are pending. Preserve the bank and pending evidence, run am doctor in this repository, and inspect before retrying.'}))
        return 0


if __name__ == '__main__':
    sys.exit(main())
