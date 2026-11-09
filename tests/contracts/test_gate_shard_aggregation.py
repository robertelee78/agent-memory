"""Negative contracts at the shard-merger CLI boundary."""
import copy
import json
import platform
from pathlib import Path
import runpy
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
GATE = ROOT / 'scripts/test_gate.py'


class ShardAggregationContract(unittest.TestCase):
    def test_incomplete_or_incompatible_receipts_are_refused(self):
        gate = runpy.run_path(str(GATE))
        entries = gate['registry']()
        os_label = {'Linux': 'ubuntu-latest', 'Darwin': 'macos-latest'}[platform.system()]
        python_label = f'{sys.version_info.major}.{sys.version_info.minor}'
        source = gate['source_identity']()
        cell = gate['cell_identity'](os_label, python_label)
        reports = []
        for index in range(3):
            rows = []
            for entry in entries[index::3]:
                row = {'name': entry['name'], 'layer': entry['layer']}
                if entry.get('path'):
                    row.update(status='passed', passed=True, tests=1, failures=0, errors=0,
                               skipped=0, expected_failures=0, unexpected_successes=0)
                else:
                    row.update(status='not-applicable', tests=0, reason=entry['reason'])
                rows.append(row)
            reports.append({'schema': 'agent-memory/test-gate-v1', 'mode': 'release',
                            'passed': True, 'release_complete': False, 'source': source,
                            'cell': cell, 'shard': {'index': index, 'count': 3}, 'suites': rows})
        with tempfile.TemporaryDirectory() as tmp:
            base = Path(tmp)
            def invoke(items):
                paths = []
                for i, item in enumerate(items):
                    path = base / f'{i}.json'
                    path.write_text(json.dumps(item))
                    paths.append(str(path))
                output = base / 'result.json'
                result = subprocess.run([sys.executable, '-I', '-B', str(GATE), '--release',
                    '--shard-count', '3', '--cell-os', os_label, '--cell-python', python_label,
                    '--output', str(output), '--merge-shards', *paths], capture_output=True, text=True)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                receipt = json.loads(output.read_text())
                self.assertFalse(receipt['passed'])
                self.assertFalse(receipt['release_complete'])
            invoke(reports[:2])
            invoke([reports[0], reports[0], reports[2]])
            for field, value in [('source', 'different'), ('cell', {}),
                                 ('passed', False), ('release_complete', True)]:
                changed = copy.deepcopy(reports)
                changed[0][field] = value
                invoke(changed)
            changed = copy.deepcopy(reports)
            changed[0]['suites'][0]['name'] = changed[1]['suites'][0]['name']
            invoke(changed)
            changed = copy.deepcopy(reports)
            row = next(row for row in changed[0]['suites'] if row.get('status') == 'passed')
            row['skipped'] = 1
            invoke(changed)
            # Plausible counts without real partition/batch proofs must not certify a cell.
            invoke(reports)


if __name__ == '__main__':
    unittest.main()
