#!/usr/bin/env python3
"""Opt-in Claude asyncRewake guidance; not a capture or publication receipt.

Register only as an asyncRewake Stop hook. A second Stop in the resulting
continuation is ignored; the existing agent performs any permitted work.
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
        if not isinstance(event, dict) or event.get('hook_event_name') != 'Stop':
            raise ValueError('unsupported event')
        if type(event.get('stop_hook_active')) is not bool:
            raise ValueError('missing continuation guard')
        if event['stop_hook_active']:
            return 0
        directory = Path(__file__).resolve().parent
        prompt = (directory / 'boundary-prompt.txt').read_text(encoding='utf-8').strip()
        prompt = prompt.replace('{{SKILL_DIR}}', str(directory.parent))
        if not prompt or len(prompt.encode('utf-8')) > 4096:
            raise ValueError('invalid instruction')
        print(prompt, file=sys.stderr)
        return 2
    except (OSError, ValueError, TypeError, UnicodeError):
        # A failed hook is never an admission or publication receipt.
        print(json.dumps({'systemMessage': 'Agent Memory turn hook failed; automatic memory capture and publication are pending.'}))
        return 0


if __name__ == '__main__':
    sys.exit(main())
