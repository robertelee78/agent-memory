"""Human CLI presentation; packaged helpers remain the JSON authority."""
from contextlib import redirect_stdout, redirect_stderr
from io import StringIO
import json
from pathlib import Path
import sys

NAMES = ('am', 'agent-memory')
HUMAN = {'install', 'update', 'uninstall', 'version', 'status', 'doctor', 'recover'}


def installed_name(original=None):
    """Identify Python's actual initial script, never an arbitrary argument.

    runpy changes argv[0]; orig_argv preserves the existing owned wrapper name.
    Keeping wrapper bytes unchanged makes genuinely old updaters compatible.
    """
    args = list(sys.orig_argv if original is None else original)[1:]
    while args and args[0].startswith('-'):
        flag = args.pop(0)
        if flag in ('-c', '-m', '-'):
            return None
        if flag == '--':
            break
        if flag in ('-W', '-X', '--check-hash-based-pycs'):
            if not args:
                return None
            args.pop(0)
    name = Path(args[0]).name if args else None
    return name if name in NAMES else None


def safe(value, *, lines=False):
    result = []
    for char in str(value):
        if char.isprintable() or (lines and char in '\n\t'):
            result.append(char)
        else:
            result.append(('\\x%02x' if ord(char) <= 255 else '\\u%04x') % ord(char))
    return ''.join(result)


def help_text(prog):
    return f'''Agent Memory — shared repository memory.

Usage:
  {prog} install [--check] [installation options]
  {prog} update [--check]
  {prog} uninstall [--check]
  {prog} --version
  {prog} version
  {prog} -V
  {prog} status [--repo PATH]
  {prog} doctor [--repo PATH]
  {prog} recover
  {prog} help [COMMAND]

Lifecycle and diagnostics print readable results by default.
Use --json for complete machine results; --verbose adds diagnostic detail.
Memory commands retain structured proposals/results: init, search, context,
capture, checkpoint, publish, refresh and the other documented operations.
Use {prog} COMMAND --help for its options. Uninstall preserves project memory.
'''


def options(argv):
    args = []; machine = verbose = False
    for index, arg in enumerate(argv):
        if arg == '--':
            return args + argv[index:], machine, verbose
        if arg == '--json':
            machine = True
        elif arg == '--verbose' or (arg == '-v' and args):
            verbose = True
        else:
            args.append(arg)
    return args, machine, verbose


def bootstrap_command(args):
    """The default repeated one-liner is a guarded update, as in AWA.

    Explicit target reconfiguration retains the mechanical install contract.
    Source options may identify the same channel, never silently switch it.
    """
    if not args or args[0] != 'install':
        return args
    overrides = {}; tail = args[1:]; index = 0
    while index < len(tail):
        arg = tail[index]
        if arg in ('--check', '--dry-run'):
            index += 1
        elif arg in ('--remote', '--ref') and index + 1 < len(tail):
            overrides[arg[2:]] = tail[index + 1]; index += 2
        elif arg.startswith(('--remote=', '--ref=')):
            key, value = arg[2:].split('=', 1); overrides[key] = value; index += 1
        else:
            return args
    from . import installation
    try:
        state, _ = installation.locations()
        old = installation.read_receipt(state)
    except (OSError, ValueError, TypeError, KeyError):
        return args  # The ordinary backend reports the actual refusal.
    if old and old['source']['kind'] == 'managed' and all(
            value == old['source'].get(key) for key, value in overrides.items()):
        return ['update', *(['--check'] if any(v in tail for v in ('--check', '--dry-run')) else [])]
    return args


def render(data, command, prog, code):
    source = (data.get('receipt') or {}).get('source') or data.get('executing_source') or data.get('source') or {}
    version = source.get('version', 'unknown'); revision = source.get('revision') or 'unversioned'
    qualification = source.get('cleanliness') or ('dirty' if source.get('dirty') is True else
                                                 'clean' if source.get('dirty') is False else 'unversioned')
    if code:
        print(safe(f"{prog}: {data.get('error') or command + ' could not be verified'}"), file=sys.stderr)
    if command == 'version' and source:
        print(f'{prog} {safe(version)}')
        print(f'revision {safe(revision)} ({safe(qualification)})')
        print('source ' + safe(source.get('path', 'unavailable')))
    elif command == 'update' and not code:
        current = data.get('current', {}); available = data.get('available', {})
        if data.get('check_complete'):
            if data.get('update_available'):
                print(f"{prog} update available: {safe(current.get('version'))} -> {safe(available.get('version'))} "
                      f"({safe(current.get('revision', '')[:12])} -> {safe(available.get('revision', '')[:12])}).")
            else:
                print(f"{prog} {safe(current.get('version', version))} is current at {safe(current.get('revision', revision))}.")
            print('Preview only; no installation changes.')
        elif data.get('changed'):
            print(f"Updated {prog} {safe(current.get('version'))} -> {safe(version)} "
                  f"({safe(current.get('revision', '')[:12])} -> {safe(revision[:12])}).")
        else:
            print(f'{prog} {safe(version)} is current ({safe(revision[:12])}).')
    elif command in ('install', 'uninstall') and not code:
        preview = data.get('check_complete') or data.get('schema') == 'agent-memory/install-plan-v1'
        if preview:
            print(('Installation' if command == 'install' else 'Uninstall') + ' preview; no changes made:')
            for target in data.get('targets', []):
                action = ('change' if command == 'install' else 'remove') if target.get('changed') else 'keep'
                print(f"  {action} {safe(target['path'])}")
        elif command == 'install':
            print(f"{'Installed' if data.get('changed') else 'Already installed:'} {prog} {safe(version)} ({safe(revision[:12])}).")
            print('source ' + safe(source.get('path', 'unavailable')) + ' (' + safe(qualification) + ')')
            print('artifact ' + safe(source.get('artifact_digest', 'unavailable')))
            receipt = data.get('receipt', {})
            wrappers = [t['path'] for t in receipt.get('targets', []) if t['kind'] == 'wrapper']
            for path in wrappers:
                print('command ' + safe(path))
            print('clients ' + safe(', '.join(receipt.get('enabled_adapters', [])) or 'none'))
            print('memory repository-local; not installer-owned')
        elif data.get('preserved'):
            print('Uninstall finished with preserved targets; removal is not complete.')
        else:
            print('Uninstalled owned runtime and client integration.')
        if command == 'uninstall':
            print('Project memory and user data preserved.')
        for target in data.get('preserved', []):
            print(f"preserved {safe(target['path'])}: {safe(target.get('reason', 'user changes'))}")
    elif command in ('status', 'doctor'):
        label = ('checks failed' if code else 'checks passed with warnings' if data.get('warnings') else 'checks passed')
        print(f'{prog} {command}' + (': ' + label if command == 'doctor' else ''))
        print('runtime ' + safe(data.get('installation', {}).get('state', 'unknown')))
        print('repository ' + safe(data.get('repository', 'unavailable')))
        bank = data.get('bank', {})
        print('memory ' + safe(bank.get('state', 'not inspected')) + (' — ' + safe(bank['path']) if bank.get('path') else ''))
    elif command == 'recover' and not code:
        print({'no-pending-transaction': 'No pending runtime transaction.',
               'complete': 'Verified committed runtime transaction.',
               'rolled-back': 'Rolled back interrupted runtime activation.'}.get(data.get('status'),
                   'Runtime recovery verified.'))
    for level in ('failures', 'warnings'):
        for item in data.get(level, []):
            print(f"{level[:-1]}: {safe(item.get('detail', item))}", file=sys.stderr if level == 'failures' else sys.stdout)
            if item.get('repair'):
                print('  ' + safe(item['repair']), file=sys.stderr if level == 'failures' else sys.stdout)
    if code and data.get('repair'):
        print(safe(data['repair']), file=sys.stderr)
    if not code and data.get('restart_clients'):
        print('Restart running agent sessions to load the changed skill.')


def main(argv=None, *, prog='am', bootstrap=False):
    argv, machine, verbose = options(list(sys.argv[1:] if argv is None else argv))
    if not argv or argv in (['help'], ['--help'], ['-h']):
        print(help_text(prog), end=''); return 0
    if argv[0] == 'help':
        argv = [*argv[1:], '--help']
    if argv[0] in ('--version', '-V'):
        argv = ['version', *argv[1:]]
    if bootstrap:
        argv = bootstrap_command(argv)
    command = argv[0]; output = StringIO(); errors = StringIO(); previous = sys.argv[0]
    from . import installation
    try:
        sys.argv[0] = prog
        try:
            with redirect_stdout(output), redirect_stderr(errors):
                code = installation.main(argv)
        except SystemExit as exc:
            code = exc.code if isinstance(exc.code, int) else (0 if exc.code is None else 1)
            print(safe(output.getvalue(), lines=True), end='')
            print(safe(errors.getvalue(), lines=True), end='', file=sys.stderr)
            return code
    finally:
        sys.argv[0] = previous
    if errors.getvalue():
        print(safe(errors.getvalue(), lines=True), end='', file=sys.stderr)
    if machine or command not in HUMAN:
        print(output.getvalue(), end=''); return code
    try:
        data = json.loads(output.getvalue())
    except (ValueError, TypeError):
        print(f'{prog}: cannot interpret the operation result; inspect --json output.', file=sys.stderr)
        return code or 1
    render(data, command, prog, code)
    if verbose:
        print(json.dumps(data, indent=2), file=sys.stderr if code else sys.stdout)
    return code
