"""A failed collector must retain the underlying timeout and active suite."""
import json
from pathlib import Path
import shutil
import unittest

from world import World

ROOT = Path(__file__).resolve().parents[2]


class GateDiagnosticsContract(unittest.TestCase):
    def test_actual_gate_retains_suite_stage_exception_chain_and_partial_timeout_output(self):
        with World() as world:
            checkout = world.inside('diagnostic checkout')
            (checkout/'scripts').mkdir(parents=True)
            (checkout/'tests/contracts').mkdir(parents=True)
            for name in ('test.sh', 'test_gate.py', 'run_contract_suite.py'):
                shutil.copy2(ROOT/'scripts'/name, checkout/'scripts'/name)
            source = (ROOT/'tests/world.py').read_text()
            # Controlled failure only in this disposable gate copy. No real
            # process is signalled and no product outcome is fabricated.
            source += '''
_ActualWorld = World
class World(_ActualWorld):
    def run(self, args, **kwargs):
        if kwargs.get('group'):
            try:
                raise subprocess.TimeoutExpired(args, 300, stderr=b'partial-worker-stderr-marker')
            except subprocess.TimeoutExpired:
                raise PermissionError(1, 'controlled-collector-failure')
        return super().run(args, **kwargs)
'''
            (checkout/'tests/world.py').write_text(source)
            manifest = json.loads((ROOT/'tests/contracts.json').read_text())
            for entry in manifest['suites']:
                if entry['scope'] == 'v0.0.1':
                    entry['path'] = 'tests/contracts/fixture.py'
            (checkout/'tests/contracts/fixture.py').write_text('')
            (checkout/'tests/contracts.json').write_text(json.dumps(manifest))
            result = world.run(['sh', checkout/'scripts/test.sh', '--release', '--json'], check=False)
            report = json.loads(result.stdout)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(report['passed'])
            self.assertFalse(report['release_complete'])
            self.assertEqual(report['error_suite'], manifest['suites'][0]['name'])
            self.assertEqual(report['error_stage'], 'suite-execution-and-collection')
            for text in ('TimeoutExpired', 'PermissionError', 'controlled-collector-failure',
                         'partial-worker-stderr-marker', 'Traceback'):
                self.assertIn(text, result.stderr)


if __name__ == '__main__':
    unittest.main()
