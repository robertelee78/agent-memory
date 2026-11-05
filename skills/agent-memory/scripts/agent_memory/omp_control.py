"""Conservative stored-batch classification; this never authorizes tool execution."""
import shlex
from . import omp_projection


def pure_batch(entries, guidance, *, continuing=False, tool_ids=()):
    """Only one exact reserved hidden message, no host/foreign companions.

    Visible assistant text or any unrecognized invocation reclassifies the whole
    interval as substantive. This is metadata classification, not permission or
    a promise that even a valid native tool is actually invoked/completed.
    """
    if not guidance:
        return None
    own = 0
    tools = set(tool_ids)
    projected = []
    for entry in entries:
        if entry.get('type') not in {'message', 'custom_message'}:
            return None
        result = omp_projection.project(entry)
        if 'snapshot' not in result:
            return None
        snapshot = result['snapshot']
        role = snapshot.get('role')
        if snapshot['carrier'] == 'custom_message' or role in {'custom', 'hookMessage'}:
            if (snapshot.get('attribution') != 'agent' or snapshot.get('content') != guidance
                    or own or continuing):
                return None
            own += 1
        elif role == 'assistant':
            content = snapshot['content']
            if content == '':
                content = []
            if not isinstance(content, list):
                return None
            for block in content:
                if block.get('type') == 'text' and not block.get('text', '').strip():
                    continue
                if block.get('type') != 'toolCall' or block.get('name') not in {'bash', 'functions.bash'}:
                    return None
                arguments = block.get('arguments', {})
                if not isinstance(arguments, dict) or not isinstance(arguments.get('command'), str):
                    return None
                try:
                    command = shlex.split(arguments['command'])
                except ValueError:
                    return None
                if (len(command) < 2 or command[0] not in {'am', 'agent-memory'} or command[1] != 'omp-boundary'
                        or any(any(char in token for char in ';&|<>$`') for token in command)):
                    return None
                tools.add(block['id'])
        elif role == 'toolResult' and snapshot.get('toolCallId') in tools:
            tools.remove(snapshot['toolCallId'])
        else:
            return None
        projected.append((entry, snapshot))
    if own != 1 and not continuing:
        return None
    return projected, sorted(tools)
