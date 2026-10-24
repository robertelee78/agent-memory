"""One installed entry point delegating to the packaged public operations."""
from pathlib import Path
import runpy


def arguments(argv):
    """Every public operation already emits JSON; accept its explicit selector.

    Preserve inline option values such as '--label=--json' and the literal tail
    after '--'. argparse remains responsible for validation.
    """
    result = []
    for index, value in enumerate(argv):
        if value == '--':
            return result + list(argv[index:])
        if value != '--json':
            result.append(value)
    return result


def execute(command, argv, route):
    helper, prefix = route
    # The caller supplies only its literal built-in route, never a request path.
    entry = Path(__file__).resolve().parent.parent / helper
    delegate = runpy.run_path(str(entry), run_name='agent_memory_cli_delegate')['main']
    intent = {'correct': ('correct',), 'supersede': ('supersede',),
              'revoke': ('revoke',), 'forget': ('expire', 'retention', 'delete')}
    kwargs = {'required_operations': intent[command]} if command in intent else {}
    return delegate([*prefix, *argv], **kwargs)
