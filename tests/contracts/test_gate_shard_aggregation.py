"""Negative contracts at the shard-merger CLI boundary."""
import copy
import json
import platform
from pathlib import Path
import runpy
import subprocess
import sys
import shutil
import tempfile
import unittest

from world import World

ROOT = Path(__file__).resolve().parents[2]
GATE = ROOT / 'scripts/test_gate.py'


class ShardAggregationContract(unittest.TestCase):
    def test_unsplit_worker_reports_only_successfully_executed_cases(self):
        with World() as world:
            checkout = world.inside('unsplit case checkout')
            (checkout / 'scripts').mkdir(parents=True)
            (checkout / 'tests/contracts').mkdir(parents=True)
            shutil.copy2(ROOT / 'scripts/run_contract_suite.py', checkout / 'scripts/run_contract_suite.py')
            shutil.copy2(ROOT / 'tests/world.py', checkout / 'tests/world.py')
            fixture = checkout / 'tests/contracts/fixture.py'
            manifest = {'suites': [{'name': 'fixture', 'path': 'tests/contracts/fixture.py'}]}
            (checkout / 'tests/contracts.json').write_text(json.dumps(manifest))
            fixture.write_text('import unittest\nclass Cases(unittest.TestCase):\n'
                               ' def test_a(self): pass\n'
                               ' @unittest.skip("not executed")\n'
                               ' def test_b(self): pass\n')
            command = [sys.executable, '-I', '-B', checkout / 'scripts/run_contract_suite.py',
                       fixture, '--suite', 'fixture']
            result = world.run(command, check=False)
            counts = json.loads(result.stdout)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(counts['passed'])
            self.assertEqual(counts['skipped'], 1)
            self.assertEqual(counts['case_selection']['executed'], ['contract_suite.Cases.test_a'])
            fixture.write_text(fixture.read_text().replace(' @unittest.skip("not executed")\n', ''))
            result = world.run(command, check=False)
            counts = json.loads(result.stdout)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(counts['passed'])
            self.assertEqual(counts['case_selection']['executed'],
                             ['contract_suite.Cases.test_a', 'contract_suite.Cases.test_b'])

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

    def test_complete_receipt_requires_the_actual_lifecycle_case(self):
        gate = runpy.run_path(str(GATE))
        entries = gate['registry']()
        matrix = json.loads((ROOT / 'tests/lifecycle-matrix.json').read_text())
        required = {}
        for requirement in matrix['requirements']:
            for case in requirement['cases']:
                required.setdefault(case['suite'], set()).add(case['test'])
        groups = {}
        for entry in entries:
            if entry.get('path') and 'tests' in entry:
                groups.setdefault(entry['path'], []).append(entry)
        selected = {}
        for batches in groups.values():
            for entry in batches:
                cases = set()
                for selector in entry['tests']:
                    if '.' in selector:
                        cases.add(selector)
                    else:
                        cases.update(test for test in required.get(entry['name'], ())
                                     if test.split('.')[0] == selector)
                        if not any(test.split('.')[0] == selector for test in cases):
                            cases.add(selector + '.test_fixture')
                selected[entry['name']] = {'contract_suite.' + test for test in cases}
        inventory = {path: sorted(set().union(*(selected[e['name']] for e in batches)))
                     for path, batches in groups.items()}
        fault_groups = gate['partition_coverage'].__globals__['GROUPS']
        rows = []
        for entry in entries:
            row = {'name': entry['name'], 'layer': entry['layer']}
            if not entry.get('path'):
                row.update(status='not-applicable', tests=0, reason=entry['reason'])
            else:
                cases = sorted(selected.get(entry['name'],
                    {'contract_suite.' + test for test in required.get(entry['name'], ())}))
                if not cases:
                    cases = ['contract_suite.Fixture.test_fixture']
                row.update(status='passed', passed=True, tests=len(cases), failures=0, errors=0,
                           skipped=0, expected_failures=0, unexpected_successes=0,
                           case_selection={'batch': entry['name'],
                               'complete': inventory.get(entry['path'], cases),
                               'selected': cases, 'executed': cases[:]})
                if entry['name'] in gate['OWNERS']:
                    group, index = gate['OWNERS'][entry['name']]
                    count = len(fault_groups[group]['suites'])
                    observed = [{'step': n, 'point': f'point-{n}', 'kind': 'write'}
                                for n in range(count)]
                    row['fault_partition'] = {'group': group, 'index': index, 'count': count,
                        'artifact_digest': 'a' * 64, 'observed': observed,
                        'executed': [observed[index]['point']]}
            rows.append(row)
        os_label = {'Linux': 'ubuntu-latest', 'Darwin': 'macos-latest'}[platform.system()]
        python_label = f'{sys.version_info.major}.{sys.version_info.minor}'
        source = gate['source_identity']()
        cell = gate['cell_identity'](os_label, python_label)
        reports = [dict(schema='agent-memory/test-gate-v1', mode='release', passed=True,
                        release_complete=False, source=source, cell=cell,
                        shard={'index': i, 'count': 3}, suites=rows[i::3]) for i in range(3)]
        with tempfile.TemporaryDirectory() as tmp:
            def invoke(items):
                paths = []
                for i, item in enumerate(items):
                    path = Path(tmp) / f'{i}.json'
                    path.write_text(json.dumps(item))
                    paths.append(str(path))
                output = Path(tmp) / 'result.json'
                result = subprocess.run([sys.executable, '-I', '-B', str(GATE), '--release',
                    '--shard-count', '3', '--cell-os', os_label, '--cell-python', python_label,
                    '--output', str(output), '--merge-shards', *paths], capture_output=True, text=True)
                return result, json.loads(output.read_text())
            result, receipt = invoke(reports)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue(receipt['release_complete'])
            self.assertEqual(len(receipt['lifecycle_matrix_coverage']), 15)
            broken = copy.deepcopy(reports)
            row = next(row for shard in broken for row in shard['suites']
                       if row['name'] == 'portable-lifecycle')
            proof = row['case_selection']
            missing = 'contract_suite.PortableLifecycleContract.test_link_custom_only_updates_without_optional_clients_identity_or_a_bank'
            for field in ('selected', 'executed', 'complete'):
                proof[field].remove(missing)
            row['tests'] -= 1
            result, receipt = invoke(broken)
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertFalse(receipt['passed'])
            self.assertFalse(receipt['release_complete'])
            self.assertEqual(receipt['error_suite'], 'lifecycle-matrix')
            self.assertIn('Lifecycle case was not executed', receipt['error'])


if __name__ == '__main__':
    unittest.main()
