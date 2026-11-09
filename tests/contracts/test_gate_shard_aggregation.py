"""Shard-consumer controls; fabricated receipts are synthetic, not product evidence."""
import copy
import importlib.util
import json
import platform
from pathlib import Path
import runpy
import shutil
import subprocess
import sys
import tempfile
import unittest

from world import World

ROOT = Path(__file__).resolve().parents[2]
GATE = ROOT / 'scripts/test_gate.py'


def synthetic_rows(gate, entries):
    """Use the entire discovered registry inventory, including inherited methods.

    Discovery does not execute these methods. The resulting receipts deliberately
    fabricate successful execution only to control the merger, never to certify
    this checkout's product behavior.
    """
    inventories = {}
    for entry in entries:
        path = entry.get('path')
        if not path or path in inventories:
            continue
        spec = importlib.util.spec_from_file_location('contract_suite', ROOT / path)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        def leaves(suite):
            for case in suite:
                if isinstance(case, unittest.TestSuite):
                    yield from leaves(case)
                else:
                    yield case.id()
        complete = sorted(leaves(unittest.defaultTestLoader.loadTestsFromModule(module)))
        if not complete or len(complete) != len(set(complete)):
            raise ValueError('Synthetic control requires a complete unique discovered inventory')
        inventories[path] = complete
    groups = gate['partition_coverage'].__globals__['GROUPS']
    rows = []
    for entry in entries:
        row = {'name': entry['name'], 'layer': entry['layer']}
        if not entry.get('path'):
            row.update(status='not-applicable', tests=0, reason=entry['reason'])
        else:
            complete = inventories[entry['path']]
            selectors = entry.get('tests')
            selected = complete[:] if selectors is None else [case for case in complete
                if any(case.removeprefix('contract_suite.') == selector
                       or case.split('.')[-2] == selector for selector in selectors)]
            row.update(status='passed', passed=True, tests=len(selected), failures=0, errors=0,
                       skipped=0, expected_failures=0, unexpected_successes=0,
                       case_selection={'batch': entry['name'], 'complete': complete[:],
                                       'selected': selected[:], 'executed': selected[:]})
            if entry['name'] in gate['OWNERS']:
                group, index = gate['OWNERS'][entry['name']]
                count = groups[group].get('count', len(groups[group]['suites']))
                observed = [{'step': n, 'point': f'synthetic-point-{n}', 'kind': 'synthetic-write'}
                            for n in range(count)]
                row['fault_partition'] = {'group': group, 'index': index, 'count': count,
                    'artifact_digest': 'a' * 64, 'observed': observed,
                    'executed': [observed[index]['point']]}
        rows.append(row)
    return rows


class ShardAggregationContract(unittest.TestCase):
    def test_unsplit_worker_reports_only_actually_started_cases(self):
        with World() as world:
            checkout = world.inside('unsplit case checkout')
            (checkout / 'scripts').mkdir(parents=True)
            (checkout / 'tests/contracts').mkdir(parents=True)
            shutil.copy2(ROOT / 'scripts/run_contract_suite.py', checkout / 'scripts/run_contract_suite.py')
            for name in ('world.py', 'fault_partitions.py'):
                shutil.copy2(ROOT / 'tests' / name, checkout / 'tests' / name)
            fixture = checkout / 'tests/contracts/fixture.py'
            manifest = {'suites': [{'name': 'fixture', 'path': 'tests/contracts/fixture.py'}]}
            (checkout / 'tests/contracts.json').write_text(json.dumps(manifest))
            fixture.write_text('import os,subprocess,sys,unittest\n'
                               'print("python-output-marker",flush=True)\n'
                               'os.write(1,b"descriptor-output-marker")\n'
                               'subprocess.run([sys.executable,"-c","print(12345)"],check=True)\n'
                               'class Cases(unittest.TestCase):\n'
                               ' def test_a(self): pass\n'
                               ' @unittest.skip("not executed")\n'
                               ' def test_b(self): pass\n')
            command = [sys.executable, '-I', '-B', checkout / 'scripts/run_contract_suite.py',
                       fixture, '--suite', 'fixture']
            identities = ['contract_suite.Cases.test_a', 'contract_suite.Cases.test_b']
            result = world.run(command, check=False)
            counts = json.loads(result.stdout)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(counts['passed'])
            self.assertEqual(counts['skipped'], 1)
            # unittest starts skipped cases too; these identities must come from
            # startTest, not successful cases or a copy of the selection.
            self.assertEqual(counts['tests'], 2)
            self.assertEqual(counts['case_selection']['executed'], identities)
            for marker in ('python-output-marker', 'descriptor-output-marker', '12345'):
                self.assertIn(marker, result.stderr)
            fixture.write_text(fixture.read_text().replace(' @unittest.skip("not executed")\n', ''))
            result = world.run(command, check=False)
            counts = json.loads(result.stdout)
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertTrue(counts['passed'])
            self.assertEqual(counts['case_selection']['executed'], identities)
            fixture.write_text('import unittest\nclass Cases(unittest.TestCase):\n'
                               ' @classmethod\n'
                               ' def setUpClass(cls): raise RuntimeError("before any test starts")\n'
                               ' def test_a(self): pass\n'
                               ' def test_b(self): pass\n')
            result = world.run(command, check=False)
            counts = json.loads(result.stdout)
            self.assertNotEqual(result.returncode, 0)
            self.assertFalse(counts['passed'])
            self.assertEqual(counts['tests'], 0)
            self.assertEqual(counts['errors'], 1)
            self.assertEqual(counts['case_selection']['complete'], identities)
            self.assertEqual(counts['case_selection']['selected'], identities)
            self.assertEqual(counts['case_selection']['executed'], [])

    def test_incomplete_or_incompatible_receipts_are_refused(self):
        gate = runpy.run_path(str(GATE))
        entries = gate['registry']()
        os_label = {'Linux': 'ubuntu-latest', 'Darwin': 'macos-latest'}[platform.system()]
        python_label = f'{sys.version_info.major}.{sys.version_info.minor}'
        source = gate['source_identity']()
        cell = gate['cell_identity'](os_label, python_label)
        rows = synthetic_rows(gate, entries)
        reports = [dict(schema='agent-memory/test-gate-v1', mode='release', passed=True,
                        release_complete=False, source=source, cell=cell,
                        control_evidence='synthetic merger control, not product evidence',
                        shard={'index': i, 'count': 3}, suites=rows[i::3]) for i in range(3)]
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
                return result, json.loads(output.read_text())
            result, receipt = invoke(reports)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertTrue(receipt['passed'])
            self.assertTrue(receipt['release_complete'])
            requirements = json.loads((ROOT / 'tests/lifecycle-matrix.json').read_text())['requirements']
            self.assertEqual({row['requirement'] for row in receipt['lifecycle_matrix_coverage']},
                             {row['requirement'] for row in requirements})
            invalid = [reports[:2], [reports[0], reports[0], reports[2]]]
            for field, value in [('source', 'different'), ('cell', {}), ('passed', False),
                                 ('release_complete', True)]:
                changed = copy.deepcopy(reports)
                changed[0][field] = value
                invalid.append(changed)
            changed = copy.deepcopy(reports)
            changed[0]['suites'][0]['name'] = changed[1]['suites'][0]['name']
            invalid.append(changed)
            for field, value in [('skipped', 1), ('tests', True), ('errors', 1)]:
                changed = copy.deepcopy(reports)
                row = next(row for row in changed[0]['suites'] if row.get('status') == 'passed')
                row[field] = value
                invalid.append(changed)
            for proof in ('fault_partition', 'case_selection'):
                changed = copy.deepcopy(reports)
                for shard in changed:
                    for row in shard['suites']:
                        row.pop(proof, None)
                invalid.append(changed)
            for items in invalid:
                with self.subTest(corruption=items):
                    result, receipt = invoke(items)
                    self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                    self.assertFalse(receipt['passed'])
                    self.assertFalse(receipt['release_complete'])

    def test_complete_receipt_requires_the_actual_lifecycle_case(self):
        gate = runpy.run_path(str(GATE))
        consumer = gate['lifecycle_matrix_coverage']
        with World() as world:
            checkout = world.inside('synthetic lifecycle consumer')
            (checkout / 'tests').mkdir(parents=True)
            matrix = copy.deepcopy(json.loads((ROOT / 'tests/lifecycle-matrix.json').read_text()))
            for row in matrix['requirements']:
                row['cases'] = [{'suite': 'controlled', 'test': 'Controlled.test_lifecycle'}]
            matrix_path = checkout / 'tests/lifecycle-matrix.json'
            matrix_path.write_text(json.dumps(matrix))
            consumer.__globals__['ROOT'] = checkout
            entries = [{'name': 'controlled', 'layer': 'lifecycle', 'scope': 'v0.0.1',
                        'path': 'tests/contracts/controlled.py'}]
            identities = ['contract_suite.Controlled.test_lifecycle',
                          'contract_suite.Controlled.test_other']
            report = dict(name='controlled', layer='lifecycle', status='passed', passed=True,
                          tests=2, failures=0, errors=0, skipped=0, expected_failures=0,
                          unexpected_successes=0,
                          control_evidence='synthetic lifecycle consumer, not product evidence',
                          case_selection={'batch': 'controlled', 'complete': identities[:],
                                          'selected': identities[:], 'executed': identities[:]})
            summary = consumer(entries, [report])
            self.assertEqual({row['requirement'] for row in summary},
                             {row['requirement'] for row in matrix['requirements']})
            broken = copy.deepcopy(report)
            for field in ('complete', 'selected', 'executed'):
                broken['case_selection'][field].remove(identities[0])
            broken['tests'] = 1
            # Counts and the remaining inventory agree, but the named lifecycle
            # behavior never started: the matrix consumer itself must refuse it.
            with self.assertRaises(ValueError):
                consumer(entries, [broken])
            for mutate in (
                lambda rows: rows.pop(),
                lambda rows: rows.__setitem__(0, copy.deepcopy(rows[1])),
                lambda rows: rows[0].update(cases=[]),
                lambda rows: rows[0]['cases'][0].update(suite='unregistered'),
                lambda rows: rows[0]['cases'][0].update(test='Controlled.test_missing')):
                changed = copy.deepcopy(matrix)
                mutate(changed['requirements'])
                matrix_path.write_text(json.dumps(changed))
                with self.assertRaises(ValueError):
                    consumer(entries, [report])
            matrix_path.write_text(json.dumps(matrix))
            for field, value in [('scope', 'future'), ('tests', ['Controlled.test_other'])]:
                with self.subTest(registry_field=field), self.assertRaises(ValueError):
                    consumer([{**entries[0], field: value}], [report])
            for field, value in [('status', 'failed'), ('passed', False), ('tests', True),
                                 ('skipped', 1)]:
                with self.subTest(receipt_field=field), self.assertRaises(ValueError):
                    consumer(entries, [{**report, field: value}])
            for field, value in [('batch', 'other'), ('executed', []),
                                 ('complete', identities * 2), ('selected', [identities[1]])]:
                broken = copy.deepcopy(report)
                broken['case_selection'][field] = value
                with self.subTest(selection_field=field), self.assertRaises(ValueError):
                    consumer(entries, [broken])


if __name__ == '__main__':
    unittest.main()
