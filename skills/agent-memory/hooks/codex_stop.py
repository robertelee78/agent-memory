#!/usr/bin/env python3
"""Request one Codex turn-local governed memory pass; no receipt is implied."""
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
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
        from agent_memory import repository_config, scope_binding
        if not os.path.lexists(repository_config.bank_path(repo)):
            print('{}'); return 0
        scope_binding.require(repo)
        prompt = (directory / 'boundary-prompt.txt').read_text(encoding='utf-8').strip().replace('{{SKILL_DIR}}', str(directory.parent))
        if not prompt or len(prompt.encode()) > 4096:
            raise ValueError('boundary guidance unavailable')
        reason = (prompt + ' This is the same Codex session ' + session + ', original turn ' + turn
                  + ', with final answer SHA-256 ' + hashlib.sha256(final.encode()).hexdigest()
                  + '. Assess the actual original turn in this same session; do not attribute later or unrelated turns to it. '
                  + 'If the original source or permissions are unavailable, report pending. Do not invent capture or publication.')
        print(json.dumps({'decision':'block','reason':reason})); return 0
    except (OSError, ValueError, TypeError, KeyError, UnicodeError, ImportError, subprocess.SubprocessError):
        print(json.dumps({'systemMessage':'Agent Memory could not verify the Codex turn or repository; capture and publication remain pending. Preserve current work and inspect am doctor.'})); return 0


if __name__ == '__main__':
    sys.exit(main())
