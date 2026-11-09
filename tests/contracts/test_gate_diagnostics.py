"""A failed collector must retain the underlying timeout and active suite."""
import copy
import importlib.util
import json
from pathlib import Path
import shutil
import subprocess
import sys
import unittest

from world import World

ROOT = Path(__file__).resolve().parents[2]


class GateDiagnosticsContract(unittest.TestCase):
    def test_actual_timeout_and_failed_completion_record_preserve_both_worker_streams(self):
        with World() as world:
            worker = world.inside('timeout and record failure.py')
            worker.write_text('import sys,time\nfrom pathlib import Path\n'
                'print("actual-producer-stdout-marker",flush=True)\n'
                'print("actual-producer-stderr-marker",file=sys.stderr,flush=True)\n'
                'next(Path(sys.argv[1]).glob("owned-command-*" )).joinpath("status.json").mkdir()\n'
                'time.sleep(5)\n')
            with self.assertRaises(OSError) as raised:
                world.run([sys.executable,'-I',worker,world.root],group=True,timeout=0.5)
            error = raised.exception
            self.assertIsInstance(error.__cause__,subprocess.TimeoutExpired)
            self.assertEqual(error.__cause__.timeout,0.5)
            self.assertIn('IsADirectoryError',str(error))
            self.assertIn('actual-producer-stdout-marker',error.output)
            self.assertIn('actual-producer-stderr-marker',error.stderr)
            self.assertEqual(list(world.root.glob('owned-command-*')),[])

    def test_actual_gate_retains_suite_stage_exception_chain_and_partial_timeout_output(self):
        with World() as world:
            checkout = world.inside('diagnostic checkout')
            (checkout/'scripts').mkdir(parents=True)
            (checkout/'tests/contracts').mkdir(parents=True)
            for name in ('test.sh', 'test_gate.py', 'run_contract_suite.py'):
                shutil.copy2(ROOT/'scripts'/name, checkout/'scripts'/name)
            source = (ROOT/'tests/world.py').read_text()
            # The disposable copy uses a short diagnostic deadline. Collection
            # still runs the real supervisor, filesystem failure and gate.
            source += '''
_ActualWorld = World
class World(_ActualWorld):
    def run(self, args, **kwargs):
        if kwargs.get('group'):
            kwargs['timeout'] = 0.5
        return super().run(args, **kwargs)
'''
            (checkout/'tests/world.py').write_text(source)
            shutil.copy2(ROOT/'tests/fault_partitions.py',checkout/'tests/fault_partitions.py')
            manifest = json.loads((ROOT/'tests/contracts.json').read_text())
            for entry in manifest['suites']:
                if entry.get('path'):
                    entry['path'] = 'tests/contracts/fixture.py'
            (checkout/'tests/contracts/fixture.py').write_text(
                'import os,sys,time\nfrom pathlib import Path\n'
                'print("gate-producer-stdout-marker",flush=True)\n'
                'print("gate-producer-stderr-marker",file=sys.stderr,flush=True)\n'
                'next(Path(os.environ["AGENT_MEMORY_TEST_ROOT"]).glob("owned-command-*" )).joinpath("status.json").mkdir()\n'
                'time.sleep(5)\n')
            (checkout/'tests/contracts.json').write_text(json.dumps(manifest))
            for jobs in (1, 2):
                with self.subTest(jobs=jobs):
                    result = world.run(['sh', checkout/'scripts/test.sh', '--release',
                                        '--jobs', str(jobs), '--json'], check=False)
                    report = json.loads(result.stdout)
                    self.assertNotEqual(result.returncode, 0)
                    self.assertFalse(report['passed'])
                    self.assertFalse(report['release_complete'])
                    self.assertEqual(report['error_suite'], manifest['suites'][0]['name'])
                    self.assertEqual(report['error_stage'], 'suite-execution-and-collection')
                    for text in ('TimeoutExpired', 'IsADirectoryError', 'could not be persisted',
                                 'gate-producer-stdout-marker', 'gate-producer-stderr-marker', 'Traceback'):
                        self.assertIn(text, result.stderr)

    def test_merge_rejects_invalid_rows_and_accepts_complete_reordered_shards(self):
        import tempfile
        spec = importlib.util.spec_from_file_location("gate_shard_contract", ROOT/'scripts/test_gate.py')
        gate = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(gate)
        entries = [{'name': name, 'layer': 'bank', 'scope': 'v0.0.15', 'path': name}
                   for name in ('alpha', 'beta', 'gamma', 'delta')]
        source, cell = 'source-a', {'os': 'test'}
        reports = []
        for index in range(2):
            rows = [dict(name=e['name'], layer=e['layer'], status='passed', passed=True,
                         tests=1, failures=0, errors=0, skipped=0, expected_failures=0,
                         unexpected_successes=0) for e in entries[index::2]]
            reports.append(dict(schema='agent-memory/test-gate-v1', mode='release',
                                source=source, cell=cell, shard=dict(index=index, count=2),
                                passed=True, release_complete=False, suites=rows))
        with tempfile.TemporaryDirectory() as tmp:
            def merge(items):
                paths = []
                for index, item in enumerate(items):
                    path = Path(tmp) / f'{index}.json'
                    path.write_text(json.dumps(item))
                    paths.append(path)
                return gate.merge_shards(paths, 2, entries, source, cell)
            self.assertEqual([row['name'] for row in merge(reports[::-1])],
                             [entry['name'] for entry in entries])
            for field, value in [('tests', 0), ('errors', 1), ('failures', 1),
                                 ('expected_failures', 1), ('unexpected_successes', 1),
                                 ('status', 'not-applicable'), ('passed', False)]:
                changed = copy.deepcopy(reports)
                changed[0]['suites'][0][field] = value
                with self.subTest(field=field), self.assertRaises(ValueError):
                    merge(changed)
            for change in (lambda r: r[0]['suites'].pop(),
                           lambda r: r[0]['suites'].__setitem__(1, r[0]['suites'][0]),
                           lambda r: r[0].update(mode='incremental'),
                           lambda r: r[0]['shard'].update(count=3)):
                changed = copy.deepcopy(reports)
                change(changed)
                with self.assertRaises(ValueError):
                    merge(changed)

    def test_optional_row_requires_exact_reason_and_integer_zero(self):
        spec = importlib.util.spec_from_file_location("gate_optional_contract", ROOT/'scripts/test_gate.py')
        gate = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(gate)
        entry = {'name': 'optional', 'layer': 'optional', 'scope': 'future',
                 'reason': 'outside release'}
        row = {'name': 'optional', 'layer': 'optional', 'status': 'not-applicable',
               'tests': 0, 'reason': 'outside release'}
        gate.validate_rows([entry], [row])
        for field, value in [('reason', 'different'), ('tests', False), ('tests', 1),
                             ('status', 'passed')]:
            changed = dict(row, **{field: value})
            with self.subTest(field=field, value=value), self.assertRaises(ValueError):
                gate.validate_rows([entry], [changed])


if __name__ == '__main__':
    unittest.main()
