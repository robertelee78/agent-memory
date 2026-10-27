#!/usr/bin/env python3
"""Emit the shared, non-blocking turn instruction for Claude/Codex hooks.

This is invocation guidance, not a memory operation or an admission receipt.
"""
import json
from pathlib import Path
import sys

MAX_INPUT = 64 * 1024


def main():
    try:
        raw = sys.stdin.buffer.read(MAX_INPUT + 1)
        if len(raw) > MAX_INPUT:
            raise ValueError('oversized event')
        event = json.loads(raw)
        if not isinstance(event, dict) or event.get('hook_event_name') != 'UserPromptSubmit':
            raise ValueError('unsupported event')
        directory = Path(__file__).resolve().parent
        prompt = (directory / 'boundary-prompt.txt').read_text(encoding='utf-8').strip()
        prompt = prompt.replace('{{SKILL_DIR}}', str(directory.parent))
        if not prompt or len(prompt.encode('utf-8')) > 4096:
            raise ValueError('invalid instruction')
        print(json.dumps({'hookSpecificOutput': {'hookEventName': 'UserPromptSubmit', 'additionalContext': prompt}}))
    except (OSError, ValueError, TypeError, UnicodeError):
        # No hook error may block the user's turn. Be explicit that memory work
        # remains unverified instead of treating a broken hook as success.
        print(json.dumps({'systemMessage': 'Agent Memory turn hook failed; automatic memory refresh and capture are pending.'}))
    return 0


if __name__ == '__main__':
    sys.exit(main())
