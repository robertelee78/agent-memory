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


def main():
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
        if any(not isinstance(v, str) or not re.fullmatch(r'[A-Za-z0-9_-]{1,128}', v) for v in (session, turn)):
            raise ValueError('turn identity unavailable')
        final = event.get('last_assistant_message')
        if not isinstance(final, str) or not final or len(final.encode()) > 65536:
            raise ValueError('final response unavailable')
        cwd = Path(event['cwd']).resolve(strict=True)
        found = subprocess.run(['git','-C',str(cwd),'rev-parse','--show-toplevel'],capture_output=True,text=True,timeout=5)
        if found.returncode:
            if any(os.path.lexists(parent / '.git') for parent in (cwd, *cwd.parents)):
                raise ValueError('repository cannot be inspected')
            print('{}'); return 0
        repo = Path(found.stdout.strip()).resolve(strict=True)
        directory = Path(__file__).resolve().parent
        sys.path.insert(0, str(directory.parent / 'scripts'))
        from agent_memory import bank_writes, repository_config, scope_binding, trust_paths
        if not os.path.lexists(repository_config.bank_path(repo)):
            print('{}'); return 0
        context = scope_binding.require(repo)
        _, bank = bank_writes.bank_path(repo)
        operations, _ = bank_writes.state_paths(repo, bank, str(uuid.uuid4()))
        ledger = operations.parent / 'codex-boundaries'; trust_paths.state(ledger)
        key = hashlib.sha256((session+':'+turn).encode()).hexdigest()
        dfd = os.open(ledger, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
        try:
            fd = os.open(key+'.pending', os.O_RDONLY | os.O_NOFOLLOW, dir_fd=dfd)
            try:
                import stat
                info=os.fstat(fd)
                if not stat.S_ISREG(info.st_mode) or info.st_uid!=os.geteuid() or info.st_nlink!=1 or info.st_mode&0o077 or info.st_size>4096:raise ValueError('unsafe pending record')
                with os.fdopen(fd,'rb',closefd=False) as f: pending=json.loads(f.read(4097))
            finally: os.close(fd)
        finally: os.close(dfd)
        if pending.get('schema')!='agent-memory/codex-boundary-pending-v1' or pending.get('project_id')!=context['project_id'] or pending.get('source_uuid')!=turn or not re.fullmatch('[0-9a-f]{64}',pending.get('source_sha256','')):raise ValueError('pending source mismatch')
        prompt = (directory / 'boundary-prompt.txt').read_text(encoding='utf-8').strip().replace('{{SKILL_DIR}}', str(directory.parent))
        if not prompt or len(prompt.encode()) > 4096:
            raise ValueError('boundary guidance unavailable')
        reason = (prompt + ' This is the same Codex session ' + session + ', original turn ' + turn
                  + ', with final answer SHA-256 ' + hashlib.sha256(final.encode()).hexdigest()
                  + '. Original user prompt SHA-256 is ' + pending['source_sha256'] + '. Assess the original turn in this same session; do not attribute later turns to it. '
                  + 'Acknowledge the result with am codex-boundary using session, prompt and source_uuid set to the original turn ID and source_sha256 set to the stated prompt hash. '
                  + 'If the original source or permissions are unavailable, report pending. Do not invent capture or publication.')
        print(json.dumps({'decision':'block','reason':reason})); return 0
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, ImportError, subprocess.SubprocessError):
        print(json.dumps({'systemMessage':'Agent Memory could not verify the Codex turn or repository; capture and publication remain pending. Preserve current work and inspect am doctor.'})); return 0


if __name__ == '__main__':
    sys.exit(main())
