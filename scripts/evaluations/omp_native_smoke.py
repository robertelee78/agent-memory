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


def run(argv, *, cwd, env, timeout=30):
    result = subprocess.run(argv, cwd=cwd, env=env, capture_output=True, text=True, timeout=timeout)
    if result.returncode:
        raise RuntimeError('Command failed: ' + argv[0] + ' (retained smoke remains unqualified)')
    return result.stdout


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
              'native_version': run([str(binary), '--version'], cwd=repo, env=env).strip(),
              'installed_source': receipt['source'], 'receipt_sha256': hashlib.sha256(receipt_path.read_bytes()).hexdigest(),
              'extension_sha256': owned['identity']['hash'], 'command': argv,
              'model': args.model, 'approval_mode': args.approval_mode or 'native-existing-default',
              'native_requests': [], 'refused_ui_requests': 0, 'task_sentinels': {}, 'failures': []}
    frames = queue.Queue()
    process = subprocess.Popen(argv, cwd=repo, env=env, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                               stderr=subprocess.PIPE, text=True, bufsize=1)
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
    readers = [threading.Thread(target=stdout, daemon=True), threading.Thread(target=stderr, daemon=True)]
    for reader in readers:
        reader.start()
    sentinels = ['OMP_SMOKE_FIRST_' + uuid.uuid4().hex, 'OMP_SMOKE_SECOND_' + uuid.uuid4().hex]
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
                # An unattended driver cannot grant the person's approval. Refusal
                # is preserved, not routed around through another execution path.
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
        until('ready')
        for i, sentinel in enumerate(sentinels):
            identifier = 'human-' + str(i + 1)
            result['native_requests'].append(identifier)
            send({'id': identifier, 'type': 'prompt', 'message':
                  'This is a disposable real-native Agent Memory smoke, not durable project learning. '
                  'Calculate ' + ('19 + 23' if i == 0 else '7 * 8') + ' and return exactly ' + sentinel + '. '
                  'Preserve this substantive answer once. Use the installed Agent Memory skill for ordinary initial recall. '
                  'Do not manually invoke boundary bookkeeping before answering; allow the installed native adapter to request it. '
                  'Under normal tool permissions, review actual retained sources before any no-learning assessment; never fake a receipt. '
                  'If memory is refused/unavailable, finish the task and leave memory pending.'})
            until('session_settled')
        send({'id': 'state', 'type': 'get_state'})
        state = until('response', 'state')
        data = state.get('data', {})
        result['session_id'] = data.get('sessionId')
        result['native_session_file'] = data.get('sessionFile')
        result['native_is_settled'] = data.get('isSettled')
        result['pending_readback'] = json.loads(run([args.am, 'omp-boundary', '--repo', str(repo), '--list-pending'], cwd=repo, env=env))
        result['status_readback'] = json.loads(run([args.am, '--json', 'status', '--repo', str(repo)], cwd=repo, env=env))
        if any(result['task_sentinels'].get(sentinel) != 1 for sentinel in sentinels):
            raise RuntimeError('Substantive answer was missing or duplicated')
        native = result['status_readback'].get('omp', {})
        if native.get('observer_evidence') != 'retained':
            raise RuntimeError('Installed source observer did not retain real evidence')
        result['qualified'] = True
        result['limits'] = ['Smoke proves only the recorded real installed multi-turn/source journey.',
                            'Inspect individual source assessments, refresh and publication separately.',
                            'Pending sources/refused tools remain pending; this is not a full source-drain or release pass.',
                            'Other modes/platforms/clients, durable capture and peer exchange need the full retained matrix.']
    except Exception as error:
        result['failures'].append(type(error).__name__ + ': ' + str(error))
    finally:
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
