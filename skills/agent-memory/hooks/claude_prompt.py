#!/usr/bin/env python3
"""Retain native Claude input before transcript publication or interruption."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys


def main(event=None):
    try:
        if event is None:
            raw = sys.stdin.buffer.read(65537)
            if len(raw) > 65536:
                raise ValueError('event bound')
            event = json.loads(raw)
        if event.get('hook_event_name') != 'UserPromptSubmit':
            raise ValueError('event type')
        cwd = Path(event['cwd']).resolve(strict=True)
        found = subprocess.run(['git', '-C', str(cwd), 'rev-parse', '--show-toplevel'], capture_output=True, text=True, timeout=5)
        if found.returncode:
            if any(os.path.lexists(p / '.git') for p in (cwd, *cwd.parents)):
                raise ValueError('repository unavailable')
            print('{}'); return 0
        repo = Path(found.stdout.strip()).resolve(strict=True)
        pack = Path(__file__).resolve().parent.parent
        sys.path.insert(0, str(pack / 'scripts'))
        from agent_memory import repository_config, turn_sources
        if not os.path.lexists(repository_config.bank_path(repo)):
            print('{}'); return 0
        text, session, prompt = event['prompt'], event['session_id'], event['prompt_id']
        reference = event['transcript_path']
        if not isinstance(text, str) or not text or len(text.encode()) > 65536 or not isinstance(reference, str) or not Path(reference).is_absolute():
            raise ValueError('native source unavailable')
        # The transcript may not exist yet. Store its native reference without
        # claiming a transcript UUID or requiring a premature transcript read.
        obs = turn_sources.register(repo, 'claude', session, prompt, hashlib.sha256(text.encode()).hexdigest(),
                                    native_source_id=prompt, source_reference=str(Path(reference).resolve()))
        guide = (pack / 'hooks/boundary-prompt.txt').read_text().strip().replace('{{SKILL_DIR}}', str(pack))
        context = (guide + '\nClaude source: session=' + session + ' prompt=' + prompt +
                   ' observation_id=' + obs['observation_id'] + ' source_sha256=' + obs['source_sha256'] +
                   '. Before final submit top-level session,prompt,observation_id,source_sha256,outcome,assessment '
                   'via am claude-boundary --input - --quiet for no-learning; assessment.source_id=observation_id. '
                   'Read the observation protocol in turn/GUIDE.md. Retain original native source attribution.')
        prior = turn_sources.backlog(repo, harness='claude')
        if any(t.get('session') != session or t.get('lifecycle') != prompt for t in prior.get('turns', [])) or prior.get('legacy'):
            context += ' Earlier memory work remains pending; inspect am claude-boundary --list-pending before assessing an original source.'
        if len(context.encode()) > 2048:
            raise ValueError('guidance bound')
        print(json.dumps({'hookSpecificOutput': {'hookEventName': 'UserPromptSubmit', 'additionalContext': context}}))
        return 0
    except Exception:
        # Failure to enroll input must not masquerade as successful tracking.
        print(json.dumps({'decision': 'block', 'reason': 'Agent Memory could not retain this input safely. Preserve work; run am doctor and inspect pending source capacity/state before retrying.'}))
        return 0


if __name__ == '__main__':
    sys.exit(main())
