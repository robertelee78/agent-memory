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

    def test_release_shards_require_complete_disjoint_same_source_coverage(self):
        spec = importlib.util.spec_from_file_location("gate_shard_contract", ROOT/'scripts/test_gate.py')
        gate = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(gate)
        entries = [{'name': name, 'layer': 'bank', 'scope': 'v0.0.15'}
                   for name in ('alpha', 'beta', 'gamma', 'delta')]
        binding = {'commit': 'candidate-a', 'python': [3, 15], 'platform': 'linux'}
        def receipt(index, names):
            return {'schema': 'agent-memory/test-gate-v1', 'mode': 'release-shard',
                    'binding': binding.copy(), 'shard': {'index': index, 'count': 2},
                    'passed': True, 'release_complete': False, 'shard_complete': True,
                    'suites': [{'name': name, 'layer': 'bank', 'status': 'passed'}
                               for name in names]}
        # Alphabetical distribution is alpha/delta and beta/gamma.
        reports = [receipt(0, ['alpha', 'delta']), receipt(1, ['beta', 'gamma'])]
        result = gate.aggregate_shards(entries, reports[::-1], binding)
        self.assertEqual(sorted(row['name'] for row in result),
                         ['alpha', 'beta', 'delta', 'gamma'])
        invalid = {}
        invalid['missing shard'] = reports[:1]
        invalid['duplicate shard'] = [reports[0], reports[0]]
        for label, change in (
            ('foreign source', lambda r: r[0]['binding'].update(commit='candidate-b')),
            ('foreign runtime', lambda r: r[0]['binding'].update(python=[3, 11])),
            ('partial suite list', lambda r: r[0]['suites'].pop()),
            ('duplicate suite', lambda r: r[0]['suites'].__setitem__(1, r[0]['suites'][0])),
            ('failed worker with passing envelope',
             lambda r: r[0]['suites'][0].update(status='failed')),
            ('future-scope bypass',
             lambda r: r[0]['suites'][0].update(status='not-applicable')),
            ('partial shard', lambda r: r[0].update(shard_complete=False)),
            ('pretended full release', lambda r: r[0].update(release_complete=True)),
        ):
            altered = copy.deepcopy(reports)
            change(altered)
            invalid[label] = altered
        for label, altered in invalid.items():
            with self.subTest(label=label), self.assertRaises(ValueError):
                gate.aggregate_shards(entries, altered, binding)

    def test_joined_worker_success_does_not_bypass_actual_fault_coverage(self):
        spec = importlib.util.spec_from_file_location("gate_coverage_contract", ROOT/'scripts/test_gate.py')
        gate = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(gate)
        report = {'suites': [{'name': 'uninstall-write-copy-before', 'status': 'passed'}],
                  'passed': False, 'release_complete': False}
        with self.assertRaises(ValueError):
            gate.complete_coverage([], report)
        self.assertFalse(report['passed'])
        self.assertFalse(report['release_complete'])


if __name__ == '__main__':
    unittest.main()
