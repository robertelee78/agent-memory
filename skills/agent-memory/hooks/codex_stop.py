#!/usr/bin/env python3
"""Request one Codex turn-local governed memory pass; no receipt is implied."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import uuid
import sys



def diagnostic(repo, stage=None, reason=None):
    if repo is None: return
    try:
        sys.path.insert(0,str(Path(__file__).resolve().parent.parent/'scripts'))
        from agent_memory import hook_failure
        if stage is None: hook_failure.clear(repo,'codex','stop')
        else: hook_failure.record(repo,'codex','stop',stage,reason)
    except Exception:
        pass  # Reporting must preserve the original fixed failure, never its content.


def main():
    diagnostic_repo = None
    stage, reason = "event", "event.invalid"
    try:
        raw = sys.stdin.buffer.read(65537)
        if len(raw) > 65536:
            raise ValueError('event exceeds bound')
        event = json.loads(raw)
        if not isinstance(event, dict) or event.get('hook_event_name') != 'Stop' or type(event.get('stop_hook_active')) is not bool:
            raise ValueError('unsupported Stop event')
        if event['stop_hook_active']:
            print('{}'); return 0
        session, turn = event.get('session_id'), event.get('turn_id')
        if any(not isinstance(v, str) or not re.fullmatch(r'[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}', v) for v in (session, turn)):
            raise ValueError('turn identity unavailable')
        final = event.get('last_assistant_message')
        if not isinstance(final, str) or not final or len(final.encode()) > 65536:
            raise ValueError('final response unavailable')
        stage, reason = 'repository', 'repository.unavailable'
        cwd = Path(event['cwd']).resolve(strict=True)
        found = subprocess.run(['git','-C',str(cwd),'rev-parse','--show-toplevel'],capture_output=True,text=True,timeout=5)
        if found.returncode:
            if any(os.path.lexists(parent / '.git') for parent in (cwd, *cwd.parents)):
                raise ValueError('repository cannot be inspected')
            print('{}'); return 0
        repo = Path(found.stdout.strip()).resolve(strict=True)
        diagnostic_repo = repo
        directory = Path(__file__).resolve().parent
        sys.path.insert(0, str(directory.parent / 'scripts'))
        from agent_memory import bank_writes, repository_config, scope_binding, trust_paths
        if not os.path.lexists(repository_config.bank_path(repo)):
            diagnostic(repo);print('{}'); return 0
        stage, reason = 'identity', 'identity.unresolved'
        context = scope_binding.require(repo)
        stage, reason = 'boundary', 'state.unavailable'
        _, bank = bank_writes.bank_path(repo)
        operations, _ = bank_writes.state_paths(repo, bank, str(uuid.uuid4()))
        ledger = operations.parent / 'codex-boundaries'
        if not os.path.lexists(ledger):
            reason = 'boundary.missing'
            raise FileNotFoundError()
        reason = 'boundary.unsafe'
        trust_paths.state(ledger)
        key = hashlib.sha256((session+':'+turn).encode()).hexdigest()
        reason = 'boundary.missing'
        dfd = os.open(ledger, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            reason = 'boundary.unsafe'
            try:
                fd = os.open(key+'.pending', os.O_RDONLY | os.O_NOFOLLOW, dir_fd=dfd)
            except FileNotFoundError:
                reason = 'boundary.missing'
                raise
            try:
                reason = 'boundary.unsafe'
                import stat
                info=os.fstat(fd)
                if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.geteuid() or info.st_nlink!=1 or info.st_mode&0o077 or info.st_size>4096:raise ValueError('unsafe pending record')
                with os.fdopen(fd,'rb',closefd=False) as f: pending=json.loads(f.read(4097))
            finally: os.close(fd)
        finally: os.close(dfd)
        reason = 'boundary.source_mismatch'
        if not isinstance(pending,dict) or pending.get('schema')!='agent-memory/codex-boundary-pending-v1' or pending.get('project_id')!=context['project_id'] or pending.get('session')!=session or pending.get('turn')!=turn or pending.get('source_uuid')!=turn or not re.fullmatch('[0-9a-f]{64}',pending.get('source_sha256','')):raise ValueError('pending source mismatch')
        stage, reason = 'guidance', 'guidance.unavailable'
        prompt = (directory / 'boundary-prompt.txt').read_text(encoding='utf-8').strip().replace('{{SKILL_DIR}}', str(directory.parent))
        if not prompt or len(prompt.encode()) > 4096:
            raise ValueError('boundary guidance unavailable')
        continuation_reason = (prompt + ' This is the same Codex session ' + session + ', original turn ' + turn
                  + ', with final answer SHA-256 ' + hashlib.sha256(final.encode()).hexdigest()
                  + '. Original user prompt SHA-256 is ' + pending['source_sha256'] + '. Assess the original turn in this same session; do not attribute later turns to it. '
                  + 'Acknowledge the result with am codex-boundary using session, prompt and source_uuid set to the original turn ID and source_sha256 set to the stated prompt hash. '
                  + 'If the original source or permissions are unavailable, report pending. Do not invent capture or publication.')
        diagnostic(repo)
        print(json.dumps({'decision':'block','reason':continuation_reason})); return 0
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, ImportError, subprocess.SubprocessError):
        diagnostic(diagnostic_repo,stage,reason)
        print(json.dumps({'systemMessage':f'Agent Memory Codex stop pending [{stage}/{reason}]. Run am doctor for this repository; capture and publication remain pending. Preserve work and assess the original source before retrying.'})); return 0

    except Exception:
        diagnostic(diagnostic_repo,stage,'internal.unexpected')
        print(json.dumps({'systemMessage':f'Agent Memory Codex stop pending [{stage}/internal.unexpected]. Preserve work and run am doctor; no capture or publication is established.'})); return 0


if __name__ == '__main__':
    sys.exit(main())
