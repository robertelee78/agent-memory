"""Private source-bound Claude Stop identity; no memory admission is implied."""
import hashlib
import json
import os
from pathlib import Path
import re
import stat
import sys

LIMIT = 16 * 1024 * 1024
UUID = re.compile(r'[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\Z')


def source(event):
    session, prompt = event.get('session_id'), event.get('prompt_id')
    if not isinstance(session, str) or not UUID.fullmatch(session) or not isinstance(prompt, str) or not UUID.fullmatch(prompt):
        raise ValueError('missing prompt identity')
    transcript = event.get('transcript_path')
    if not isinstance(transcript, str) or not Path(transcript).is_absolute():
        raise ValueError('missing transcript')
    fd = os.open(transcript, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or info.st_size > LIMIT:
            raise ValueError('unsafe transcript')
        with os.fdopen(fd, 'rb', closefd=False) as stream:
            raw = stream.read(LIMIT + 1)
        if len(raw) > LIMIT:
            raise ValueError('oversized transcript')
        after = os.fstat(fd)
        named = os.stat(transcript, follow_symlinks=False)
        if (after.st_dev, after.st_ino, after.st_size, after.st_mtime_ns) != (info.st_dev, info.st_ino, info.st_size, info.st_mtime_ns) or (named.st_dev, named.st_ino) != (info.st_dev, info.st_ino):
            raise ValueError('transcript changed during observation')
    finally:
        os.close(fd)
    found = []
    for line in raw.splitlines():
        row = json.loads(line)
        if (isinstance(row, dict) and row.get('type') == 'user' and row.get('sessionId') == session
                and row.get('promptId') == prompt and row.get('isMeta') is not True):
            message = row.get('message', {})
            if isinstance(message, dict) and isinstance(message.get('content'), str):
                found.append(row)
    if len(found) != 1 or not isinstance(found[0].get('uuid'), str) or not UUID.fullmatch(found[0]['uuid']):
        raise ValueError('source user message is missing or ambiguous')
    last = event.get('last_assistant_message')
    if not isinstance(last, str) or not last or len(last.encode()) > 65536:
        raise ValueError('final assistant message is unavailable')
    source_hash = hashlib.sha256(found[0]['message']['content'].encode()).hexdigest()
    last_hash = hashlib.sha256(last.encode()).hexdigest()
    return {'session': session, 'prompt': prompt, 'source_uuid': found[0]['uuid'],
            'source_sha256': source_hash, 'assistant_sha256': last_hash, 'transcript': transcript}
