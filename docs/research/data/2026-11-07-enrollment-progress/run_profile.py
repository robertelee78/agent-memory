import argparse
import cProfile
import datetime
import faulthandler
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import signal
import subprocess
import sys
import time
import unittest

parser = argparse.ArgumentParser()
parser.add_argument('source', type=Path)
parser.add_argument('output', type=Path)
parser.add_argument('method')
parser.add_argument('--worker', action='store_true')
parser.add_argument('--profile', action='store_true')
args = parser.parse_args()
source = args.source.resolve()
output = args.output.resolve()
output.mkdir(parents=True, exist_ok=True)
sys.path.insert(0, str(source / 'tests'))
from world import World, require_world

if args.worker:
    require_world()
    faulthandler.dump_traceback_later(60, repeat=True)
    profiler = cProfile.Profile() if args.profile else None
    def finish(signum, frame):
        if profiler:
            profiler.disable()
            profiler.dump_stats(output / (args.method + '.profile'))
        faulthandler.dump_traceback()
        raise SystemExit(128 + signum)
    signal.signal(signal.SIGTERM, finish)
    spec = importlib.util.spec_from_file_location('contract_suite', source / 'tests/contracts/test_omp_enrollment_progress.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    suite = unittest.TestSuite([module.EnrollmentProgress(args.method)])
    started = time.monotonic()
    if profiler:
        profiler.enable()
    try:
        result = unittest.TextTestRunner(verbosity=2).run(suite)
    finally:
        if profiler:
            profiler.disable()
            profiler.dump_stats(output / (args.method + '.profile'))
        faulthandler.cancel_dump_traceback_later()
    counts = {'tests': result.testsRun, 'failures': len(result.failures), 'errors': len(result.errors),
              'skipped': len(result.skipped), 'expected_failures': len(result.expectedFailures),
              'unexpected_successes': len(result.unexpectedSuccesses), 'elapsed_seconds': time.monotonic() - started}
    counts['passed'] = result.wasSuccessful() and result.testsRun == 1 and not any(counts[key] for key in ('failures', 'errors', 'skipped', 'expected_failures', 'unexpected_successes'))
    print(json.dumps(counts, sort_keys=True))
    raise SystemExit(0 if counts['passed'] else 1)

paths = ['tests/contracts/test_omp_enrollment_progress.py', 'skills/agent-memory/scripts/agent_memory/omp_transport.py', 'tests/world.py']
def identity():
    return {name: hashlib.sha256((source / name).read_bytes()).hexdigest() for name in paths}
receipt = {'source_root': str(source), 'source_commit': subprocess.check_output(['git', 'rev-parse', 'HEAD'], cwd=source, text=True).strip(),
           'source_files_before': identity(), 'timeout_seconds': 300, 'profiled': args.profile,
           'method': args.method, 'started_at': datetime.datetime.now(datetime.timezone.utc).isoformat()}
with World() as world:
    command = [sys.executable, '-I', '-B', str(Path(__file__).resolve()), str(source), str(output), args.method, '--worker']
    if args.profile:
        command.append('--profile')
    receipt['command'] = command
    started = time.monotonic()
    process = subprocess.Popen(command, cwd=world.paths['repo'], env=world.env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, start_new_session=True)
    try:
        stdout, stderr = process.communicate(timeout=300)
        receipt['timed_out'] = False
    except subprocess.TimeoutExpired:
        receipt['timed_out'] = True
        os.killpg(process.pid, signal.SIGTERM)
        try:
            stdout, stderr = process.communicate(timeout=10)
        except subprocess.TimeoutExpired:
            os.killpg(process.pid, signal.SIGKILL)
            stdout, stderr = process.communicate(timeout=10)
    receipt.update(exit=process.returncode, elapsed_seconds=time.monotonic() - started, stdout=stdout, stderr=stderr)
receipt['source_files_after'] = identity()
receipt['passed'] = not receipt['timed_out'] and receipt['exit'] == 0
receipt_path = output / (args.method + '.json')
receipt_path.write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps({'receipt': str(receipt_path), **receipt}, sort_keys=True))
raise SystemExit(0 if receipt['passed'] else 1)
