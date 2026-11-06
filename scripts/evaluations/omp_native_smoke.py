#!/usr/bin/env python3
"""Real installed omp multi-turn smoke. This is not the full #249 release gate.

Requires an explicitly prepared disposable, initialized/bound repository, actual
provider access, an installed receipt-v2 binding, and normal native permissions.
Never installs a fake tool, auto-approves a request, or fabricates a source receipt.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import queue
import subprocess
import sys
import threading
import time
import uuid

class PendingObservations(RuntimeError):
    """Read-only qualification must await asynchronous observer publication."""


def run(argv, *, cwd, env, timeout=30):
    result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        try:
            pending = json.loads(result.stdout)
        except ValueError:
            pending = None
        if (isinstance(pending, dict) and pending.get('verified') is False
                and pending.get('status') == 'pending'
                and pending.get('canonical_writes_performed') is False):
            raise PendingObservations('Native readback remains pending')
        if (isinstance(pending, dict)
                and pending.get('schema') == 'agent-memory/diagnostic-result-v1'
                and pending.get('operation') == 'status'):
            failures = pending.get('failures')
            if (isinstance(failures, list) and failures
                    and all(isinstance(item, dict) and item.get('component') == 'omp.native_boundary'
                            for item in failures)):
                raise PendingObservations('Native diagnostic readback is not stable yet')
        raise RuntimeError('Command failed: ' + argv[0] + ' (exit ' + str(result.returncode)
                           + '; retained smoke remains unqualified)')
    return result.stdout


SUPPORTED_VERSION = '18.16.0'


def source_digest(value):
    raw = json.dumps(value, sort_keys=True, separators=(',', ':'), ensure_ascii=False).encode()
    return hashlib.sha256(raw).hexdigest()


def content_text(content):
    if isinstance(content, str):
        return content
    if isinstance(content, list) and all(isinstance(block, dict) and block.get('type') == 'text'
                                         and isinstance(block.get('text'), str) for block in content):
        return ''.join(block['text'] for block in content)
    return None


def current_humans(repo, root, session, session_file, prompts):
    """Bind the submitted text to this run's stored native IDs, not RPC IDs."""
    if not isinstance(session, str) or str(uuid.UUID(session)) != session or not session_file:
        raise RuntimeError('Current persistent native session identity is unavailable')
    path = Path(session_file).resolve(strict=True)
    if not path.is_relative_to(root / 'sessions'):
        raise RuntimeError('Native session file is outside this smoke run')
    entries = [json.loads(line) for line in path.read_text().splitlines() if line.strip()]
    headers = [(index, entry) for index, entry in enumerate(entries) if entry.get('type') == 'session']
    if len(headers) != 1 or headers[0][1].get('id') != session:
        raise RuntimeError('Native session header differs from current RPC state')
    header_index, header = headers[0]
    if any(entry.get('type') != 'title' for entry in entries[:header_index]):
        raise RuntimeError('Unexpected native records precede the session header')
    if Path(header.get('cwd', '')).resolve() != repo or header.get('parentSession'):
        raise RuntimeError('Native session is not a fresh session of the selected repository')
    humans = [entry for entry in entries[header_index + 1:] if entry.get('type') == 'message'
              and entry.get('message', {}).get('role') == 'user'
              and entry['message'].get('attribution') == 'user']
    if [content_text(entry['message'].get('content')) for entry in humans] != prompts:
        raise RuntimeError('Both exact submitted human turns must be retained in the current native session')
    ids = [entry.get('id') for entry in humans]
    if any(not isinstance(identifier, str) or not identifier for identifier in ids) or len(set(ids)) != len(prompts):
        raise RuntimeError('Native human entry IDs are missing or duplicated')
    return humans


def current_observations(am, repo, root, session, session_file, prompts, status, env):
    """Read back source-bound observations for BOTH current stored human turns.

    Private enrollment files locate completed as well as pending observations;
    only the installed public source verifier qualifies them. A historical tree,
    orphan/manual shared registration or status label cannot establish this.
    """
    humans = current_humans(repo, root, session, session_file, prompts)
    bank = Path(status['bank']['path'])
    state = Path(env.get('AGENT_MEMORY_STATE_HOME') or
                 str(Path(env.get('XDG_STATE_HOME', str(Path.home() / '.local/state'))) / 'agent-memory')).expanduser()
    base = state / 'writes' / hashlib.sha256(str(bank).encode()).hexdigest()
    try:
        binding = json.loads((base / 'omp-bank-binding-v1.json').read_text())
    except FileNotFoundError:
        raise PendingObservations('Current native observer binding is not published yet') from None
    if (binding.get('schema') != 'agent-memory/omp-bank-binding-v1'
            or binding.get('repository') != str(repo) or binding.get('bank_path') != str(bank)
            or binding.get('private_base') != str(base)
            or binding.get('project_id') != status['bank'].get('project_id')):
        raise RuntimeError('Retained native sources are not bound to this repository/bank')
    directory = base / 'omp-source-v1' / hashlib.sha256(session.encode()).hexdigest()
    if not directory.is_dir():
        raise PendingObservations('Current native session has no retained observer evidence yet')
    evidence = []
    for human in humans:
        matches = []
        for path in directory.glob('*/enrollments/*.json'):
            enrollment = json.loads(path.read_text())
            if enrollment.get('kind') != 'admission' or enrollment.get('native_source_id') != human['id']:
                continue
            lifecycle = enrollment['lifecycle']
            if str(uuid.UUID(lifecycle)) != lifecycle or path.parent.parent != directory / lifecycle:
                raise RuntimeError('Current human enrollment lifecycle differs from its retained namespace')
            value = json.loads(run([am, 'omp-boundary', '--repo', str(repo), '--source',
                                    enrollment['source_reference'], '--session', session,
                                    '--lifecycle', lifecycle], cwd=repo, env=env))
            source = value.get('source', {})
            observation = value.get('observation')
            origin = value.get('origin', {})
            digest = source_digest(source)
            captures = [capture for capture in value.get('captures', [])
                        if capture.get('native_entry_id') == human['id']]
            if (not value.get('verified') or value.get('faults') or value.get('enrollment') != enrollment
                    or enrollment.get('session') != session or enrollment.get('source_sha256') != digest
                    or source.get('carrier') != 'message' or source.get('native_entry_id') != human['id']
                    or source.get('native_parent_id') != human.get('parentId') or source.get('role') != 'user'
                    or source.get('attribution') != 'user'
                    or content_text(source.get('content')) != content_text(human['message'].get('content'))
                    or origin.get('kind') != 'genuine-user' or origin.get('human_root') != lifecycle
                    or origin.get('native_trigger') not in {'human', 'human-with-own-guidance'}
                    or value.get('retained_directory') != str(directory / lifecycle)
                    or len(captures) != 1 or captures[0].get('mapping') != 'verified'
                    or captures[0].get('snapshot_sha256') != digest):
                raise RuntimeError('Current human retained source failed exact source/root/digest binding')
            if 'observation' in value and observation is None:
                raise PendingObservations('Verified current source is awaiting shared registration')
            if (not isinstance(observation, dict) or observation.get('harness') != 'omp'
                    or observation.get('session') != session
                    or observation.get('lifecycle') != lifecycle or not observation.get('observation_id')
                    or observation.get('source_reference') != enrollment['source_reference']
                    or observation.get('source_sha256') != digest):
                raise RuntimeError('Current human observer evidence failed exact source/root/digest binding')
            matches.append({'native_entry_id': human['id'], 'human_root': lifecycle,
                            'observation_id': observation['observation_id'],
                            'source_reference': enrollment['source_reference'], 'source_sha256': digest,
                            'mapping': captures[0]['mapping']})
        if not matches:
            raise PendingObservations('A current human turn has no retained source-bound observation yet')
        if len(matches) != 1:
            raise RuntimeError('Each current human turn needs exactly one retained source-bound observation')
        evidence.append(matches[0])
    if len({item['human_root'] for item in evidence}) != len(humans):
        raise RuntimeError('Distinct current human turns cannot share an observer root')
    return evidence


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', required=True)
    parser.add_argument('--am', required=True)
    parser.add_argument('--receipt', required=True)
    parser.add_argument('--repo', required=True)
    parser.add_argument('--output', required=True)
    parser.add_argument('--model', required=True, help='Actual operator-selected available provider/model')
    parser.add_argument('--approval-mode', help='Optional explicit normal native mode; never auto-approved by this driver')
    parser.add_argument('--timeout', type=int, default=240)
    args = parser.parse_args()
    root = Path(args.output).resolve()
    root.mkdir(mode=0o700, parents=False, exist_ok=False)
    repo = Path(args.repo).resolve(strict=True)
    binary = Path(args.binary).resolve(strict=True)
    receipt_path = Path(args.receipt).resolve(strict=True)
    receipt = json.loads(receipt_path.read_text())
    if receipt.get('schema') != 'agent-memory/install-receipt-v2' or not receipt.get('omp_binding'):
        raise RuntimeError('Explicit real installed native binding required')
    binding = receipt['omp_binding']
    extension = Path(binding['extension_target'])
    owned = next(t for t in receipt['targets'] if t['path'] == str(extension))
    if hashlib.sha256(extension.read_bytes()).hexdigest() != owned['identity']['hash']:
        raise RuntimeError('Installed native extension differs from receipt')
    if Path(args.am).resolve(strict=True) not in {
            Path(receipt['bin_dir']) / name for name in ('am', 'agent-memory')}:
        raise RuntimeError('Smoke must use an exact receipt-owned public command')
    env = {key: value for key, value in os.environ.items()
           if not key.startswith(('PI_SESSION', 'PI_TOOL_BRIDGE', 'PI_ARTIFACT', 'PI_EVAL', 'VOX_'))}
    env['PI_CODING_AGENT_DIR'] = binding['agent_dir']
    argv = [str(binary), '--mode', 'rpc', '--cwd', str(repo), '--session-dir', str(root / 'sessions'),
            '--model', args.model, '--max-time', str(args.timeout)]
    if args.approval_mode:
        argv += ['--approval-mode', args.approval_mode]
    result = {'schema': 'agent-memory/omp-installed-smoke-v1', 'qualified': False,
              'full_native_qualification': False, 'platform': sys.platform,
              'binary_sha256': hashlib.sha256(binary.read_bytes()).hexdigest(),
              'native_version': None, 'supported_version': SUPPORTED_VERSION,
              'installed_source': receipt['source'], 'receipt_sha256': hashlib.sha256(receipt_path.read_bytes()).hexdigest(),
              'extension_sha256': owned['identity']['hash'], 'command': argv,
              'model': args.model, 'approval_mode': args.approval_mode or 'native-existing-default',
              'native_requests': [], 'refused_ui_requests': 0, 'task_sentinels': {}, 'failures': [],
              'native_adapter_diagnostics': [], 'native_ui_methods': {},
              'readback_attempts': 0}
    frames = queue.Queue()
    process = None
    def stdout():
        for line in process.stdout:
            try:
                frames.put(json.loads(line))
            except ValueError:
                frames.put({'type': 'invalid-frame'})
    # Raw native display output can contain deobfuscated secrets. Retain only
    # fixed smoke sentinel counts/event types, never arbitrary display payloads.
    def stderr():
        for _ in process.stderr:
            pass
    readers = []
    sentinels = ['OMP_SMOKE_FIRST_' + uuid.uuid4().hex, 'OMP_SMOKE_SECOND_' + uuid.uuid4().hex]
    prompts = []
    def send(value):
        process.stdin.write(json.dumps(value) + '\n'); process.stdin.flush()
    def until(kind, identifier=None):
        deadline = time.monotonic() + args.timeout
        while time.monotonic() < deadline:
            try:
                frame = frames.get(timeout=1)
            except queue.Empty:
                if process.poll() is not None:
                    raise RuntimeError('Real native process exited before ' + kind)
                continue
            if frame.get('type') == 'extension_ui_request':
                method = frame.get('method')
                known_methods = {'select', 'confirm', 'input', 'editor', 'ask', 'cancel',
                                 'notify', 'setStatus', 'setWidget', 'setWorkingMessage',
                                 'setTitle', 'set_editor_text', 'open_url'}
                label = method if method in known_methods else 'unknown'
                result['native_ui_methods'][label] = result['native_ui_methods'].get(label, 0) + 1
                # Retain only fixed owned-adapter codes, never arbitrary UI text.
                for code in ('inactive: unsupported version or missing explicit installed binding',
                             'duplicate-binding', 'observer pending', 'metadata-capacity coverage gap',
                             'observer wait ended; retained work pending'):
                    values = (frame.get('message'), frame.get('text'), frame.get('statusText'))
                    if ('am: ' + code in values or
                            'Agent Memory: ' + code + '; sources remain pending. Use am status/doctor and am omp-boundary --list-pending.' in values):
                        if code not in result['native_adapter_diagnostics']:
                            result['native_adapter_diagnostics'].append(code)
                # Notifications/status are not approval dialogs and need no response.
                # Never grant interactive or unknown requests on the person's behalf.
                if method not in known_methods or method in {'select', 'confirm', 'input', 'editor', 'ask'}:
                    result['refused_ui_requests'] += 1
                    send({'type': 'extension_ui_response', 'id': frame['id'], 'cancelled': True})
            if frame.get('type') == 'message_end':
                message = frame.get('message', {})
                if message.get('role') == 'assistant':
                    content = message.get('content', [])
                    text = content if isinstance(content, str) else ''.join(b.get('text', '') for b in content if b.get('type') == 'text')
                    for sentinel in sentinels:
                        result['task_sentinels'][sentinel] = result['task_sentinels'].get(sentinel, 0) + text.count(sentinel)
            if frame.get('type') == kind and (identifier is None or frame.get('id') == identifier):
                return frame
        raise TimeoutError('Real native ' + kind + ' was not observed')
    try:
        # This gate precedes process launch, provider/model use and human turns.
        result['native_version'] = run([str(binary), '--version'], cwd=repo, env=env).strip()
        expected_version = 'omp/' + SUPPORTED_VERSION
        if result['native_version'] != expected_version:
            raise RuntimeError('Unsupported omp binary: require exactly ' + expected_version
                               + ', observed ' + result['native_version'])
        process = subprocess.Popen(argv, cwd=repo, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                                   stderr=subprocess.PIPE, text=True, bufsize=1)
        readers = [threading.Thread(target=stdout, daemon=True), threading.Thread(target=stderr, daemon=True)]
        for reader in readers:
            reader.start()
        until('ready')
        for i, sentinel in enumerate(sentinels):
            identifier = 'human-' + str(i + 1)
            result['native_requests'].append(identifier)
            prompt = ('This is a disposable real-native Agent Memory smoke, not durable project learning. '
                  'Calculate ' + ('19 + 23' if i == 0 else '7 * 8') + ' and return exactly ' + sentinel + '. '
                  'Preserve this substantive answer once. Use the installed Agent Memory skill for ordinary initial recall. '
                  'Do not manually invoke boundary bookkeeping before answering; allow the installed native adapter to request it. '
                  'Under normal tool permissions, review actual retained sources before any no-learning assessment; never fake a receipt. '
                  'If memory is refused/unavailable, finish the task and leave memory pending.')
            prompts.append(prompt)
            send({'id': identifier, 'type': 'prompt', 'message': prompt})
            until('session_settled')
        send({'id': 'state', 'type': 'get_state'})
        state = until('response', 'state')
        data = state.get('data', {})
        result['session_id'] = data.get('sessionId')
        result['native_session_file'] = data.get('sessionFile')
        result['native_is_settled'] = data.get('isSettled')
        if any(result['task_sentinels'].get(sentinel) != 1 for sentinel in sentinels):
            raise RuntimeError('Substantive answer was missing or duplicated')
        if result['native_is_settled'] is not True:
            raise RuntimeError('Current native session is not settled')
        if hashlib.sha256(receipt_path.read_bytes()).hexdigest() != result['receipt_sha256']:
            raise RuntimeError('Installed ownership receipt changed during this smoke')
        if hashlib.sha256(extension.read_bytes()).hexdigest() != owned['identity']['hash']:
            raise RuntimeError('Installed native extension changed during this smoke')
        # Native session_settled does not promise observer queue completion:
        # each callback can stop awaiting after 950 ms while durable work continues.
        # Wait read-only, with a deadline; never manufacture or acknowledge sources.
        waiting = time.monotonic()
        deadline = waiting + args.timeout
        quiet_samples = 0
        while True:
            result['readback_attempts'] += 1
            try:
                # Readback locks are nonblocking. Do not contend with the native
                # helper and make its one-shot durable observation fail ourselves.
                identifier = 'observer-progress-' + str(result['readback_attempts'])
                send({'id': identifier, 'type': 'get_state'})
                progress = until('response', identifier)
                if progress.get('data', {}).get('sessionId') != result['session_id']:
                    raise RuntimeError('Native session changed while awaiting its observer')
                processes = subprocess.run(['ps', '-axo', 'pid=,ppid='], env=env,
                                           capture_output=True, text=True, check=True, timeout=5)
                children = [line for line in processes.stdout.splitlines()
                            if len(line.split()) == 2 and int(line.split()[1]) == process.pid]
                quiet_samples = 0 if children else quiet_samples + 1
                if quiet_samples < 2:
                    if time.monotonic() >= deadline:
                        raise PendingObservations('Native observer helpers did not become idle before the deadline')
                    time.sleep(0.1)
                    continue
                result['pending_readback'] = json.loads(run(
                    [args.am, 'omp-boundary', '--repo', str(repo), '--list-pending'], cwd=repo, env=env))
                result['status_readback'] = json.loads(run(
                    [args.am, '--json', 'status', '--repo', str(repo)], cwd=repo, env=env))
                native = result['status_readback'].get('omp', {})
                if native.get('binding') != binding or native.get('owned_disk_integrity') != 'verified':
                    raise RuntimeError('Current installed native ownership is not verified')
                observations = current_observations(
                    args.am, repo, root, result['session_id'], result['native_session_file'],
                    prompts, result['status_readback'], env)
                if time.monotonic() >= deadline:
                    raise PendingObservations('Native observations completed after the readback deadline')
                break
            except PendingObservations:
                quiet_samples = 0
                if time.monotonic() >= deadline or process.poll() is not None:
                    raise PendingObservations('Current human observer evidence did not complete before the readback deadline')
                time.sleep(min(0.5, max(0, deadline - time.monotonic())))
        result['observer_wait_seconds'] = time.monotonic() - waiting
        result['current_human_observations'] = observations
        result['qualified'] = True
        result['limits'] = ['Smoke proves only the recorded real installed multi-turn/source journey.',
                            'Inspect individual source assessments, refresh and publication separately.',
                            'Pending sources/refused tools remain pending; this is not a full source-drain or release pass.',
                            'Other modes/platforms/clients, durable capture and peer exchange need the full retained matrix.']
    except Exception as error:
        result['failures'].append(type(error).__name__ + ': ' + str(error))
    finally:
        if process is not None:
            process.stdin.close()
            try:
                process.wait(timeout=15)
            except subprocess.TimeoutExpired:
                process.terminate(); process.wait(timeout=15)
            result['native_exit'] = process.returncode
        for reader in readers:
            reader.join(timeout=3)
        path = root / 'report.json'
        with path.open('x') as stream:
            os.chmod(path, 0o600)
            json.dump(result, stream, indent=2); stream.write('\n')
        print(json.dumps({'report': str(path), 'qualified': result['qualified'], 'full_native_qualification': False}))
    return 0 if result['qualified'] else 1


if __name__ == '__main__':
    raise SystemExit(main())
