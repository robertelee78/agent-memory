"""A failed collector must retain the underlying timeout and active suite."""
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
            result = world.run(['sh', checkout/'scripts/test.sh', '--release', '--json'], check=False)
            report = json.loads(result.stdout)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(report['passed'])
            self.assertFalse(report['release_complete'])
            self.assertEqual(report['error_suite'], manifest['suites'][0]['name'])
            self.assertEqual(report['error_stage'], 'suite-execution-and-collection')
            for text in ('TimeoutExpired', 'IsADirectoryError', 'could not be persisted',
                         'gate-producer-stdout-marker', 'gate-producer-stderr-marker', 'Traceback'):
                self.assertIn(text, result.stderr)


if __name__ == '__main__':
    unittest.main()
