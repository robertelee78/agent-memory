"""Behavioral controls for registered replay routing and proof consumers."""
from contextlib import redirect_stderr
from copy import deepcopy
import io
import json
from pathlib import Path
import shutil
import sys
import unittest

from fault_partitions import GROUPS, OWNERS, PARTITIONS, coverage, emit, extract, registered
from world import World

ROOT = Path(__file__).resolve().parents[2]


def controlled_reports():
    """Gate-control metadata only; never evidence of real product interruptions."""
    reports = []
    for group, configuration in GROUPS.items():
        observed = [{'step': step, 'point': 'controlled-' + str(step), 'kind': 'controlled-write'}
                    for step in range(PARTITIONS * 2 + 3)]
        for index, name in enumerate(configuration['suites']):
            reports.append({'name': name, 'status': 'passed', 'tests': 1,
                'fault_partition': {'group': group, 'index': index, 'count': PARTITIONS,
                    'artifact_digest': '0' * 64, 'observed': deepcopy(observed),
                    'executed': [row['point'] for row in observed if row['step'] % PARTITIONS == index]}})
    return reports


class LifecyclePartitionContract(unittest.TestCase):
    def test_consumer_rejects_missing_duplicate_or_inconsistent_replay_proofs(self):
        reports = controlled_reports()
        summary = coverage(reports)
        self.assertEqual({row['group'] for row in summary}, set(GROUPS))
        for row in summary:
            self.assertEqual(row['partitions'], PARTITIONS)
            self.assertEqual(row['observed_points'], PARTITIONS * 2 + 3)
            self.assertEqual(row['actual_interruptions_and_retries'], row['observed_points'])
        invalid = [reports[1:], [*reports, reports[0]]]
        for key, value in [('status', 'failed'), ('tests', 0), ('tests', 2), ('tests', True),
                           ('fault_partition', None)]:
            changed = deepcopy(reports)
            changed[0][key] = value
            invalid.append(changed)
        for key, value in [('group', 'wrong-group'), ('index', 1), ('index', False),
                           ('count', PARTITIONS - 1), ('count', str(PARTITIONS)),
                           ('observed', []), ('executed', []), ('artifact_digest', 'invalid')]:
            changed = deepcopy(reports)
            changed[0]['fault_partition'][key] = value
            invalid.append(changed)
        changed = deepcopy(reports)
        changed[0].pop('tests')
        invalid.append(changed)
        changed = deepcopy(reports)
        changed[1]['fault_partition']['artifact_digest'] = '1' * 64
        invalid.append(changed)
        # Change a point not executed by this partition: its own subset remains
        # valid, but agreement on the complete observed inventory must still fail.
        changed = deepcopy(reports)
        changed[1]['fault_partition']['observed'][0]['point'] = 'changed-controlled-point'
        invalid.append(changed)
        changed = deepcopy(reports)
        changed[1]['fault_partition']['observed'][0]['kind'] = 'changed-controlled-kind'
        invalid.append(changed)
        for mutation in ('duplicate-point', 'noncontiguous-step', 'missing-kind', 'duplicate-execution'):
            changed = deepcopy(reports)
            proof = changed[0]['fault_partition']
            if mutation == 'duplicate-point':
                proof['observed'][1]['point'] = proof['observed'][0]['point']
            elif mutation == 'noncontiguous-step':
                proof['observed'][1]['step'] = 0
            elif mutation == 'missing-kind':
                proof['observed'][0].pop('kind')
            else:
                proof['executed'].append(proof['executed'][0])
            invalid.append(changed)
        for index, sample in enumerate(invalid):
            with self.subTest(corruption=index), self.assertRaises(ValueError):
                coverage(sample)

    def test_every_required_partition_and_control_must_remain_registered(self):
        entries = json.loads((ROOT / 'tests/contracts.json').read_text())['suites']
        registered(entries)
        for name in set(OWNERS) | {'lifecycle-fault-partitions'}:
            with self.subTest(missing=name), self.assertRaises(ValueError):
                registered([entry for entry in entries if entry['name'] != name])
        for name in OWNERS:
            entry = next(entry for entry in entries if entry['name'] == name)
            with self.subTest(duplicate=name), self.assertRaises(ValueError):
                registered([*entries, entry])
        for alteration in ({'path': None}, {'scope': 'future'}):
            changed = deepcopy(entries)
            next(entry for entry in changed if entry['name'] in OWNERS).update(alteration)
            with self.subTest(alteration=alteration), self.assertRaises(ValueError):
                registered(changed)

    def test_proof_emission_and_extraction_refuse_empty_wrong_or_duplicate_proofs(self):
        sample = controlled_reports()[0]
        proof = sample['fault_partition']
        stream = io.StringIO()
        with redirect_stderr(stream):
            emit(proof['group'], proof['index'], proof['observed'], proof['executed'], proof['artifact_digest'])
        text = stream.getvalue()
        self.assertEqual(extract(text, sample['name']), proof)
        for corrupted in ('', text + text,
                          text.replace(proof['group'], 'wrong-group'),
                          json.dumps({'lifecycle_fault_partition': {**proof, 'executed': []}}),
                          json.dumps({'lifecycle_fault_partition': {**proof, 'count': PARTITIONS - 1}})):
            with self.subTest(stderr=corrupted), self.assertRaises(ValueError):
                extract(corrupted, sample['name'])
        for rows, executed in (([], []), (proof['observed'], [])):
            with self.subTest(observed=rows), redirect_stderr(io.StringIO()), self.assertRaises(ValueError):
                emit(proof['group'], proof['index'], rows, executed, proof['artifact_digest'])

    def test_actual_worker_binds_registered_name_and_path_without_narrowing_direct_runs(self):
        with World() as world:
            checkout = world.inside('routing checkout')
            (checkout / 'scripts').mkdir(parents=True)
            (checkout / 'tests/contracts').mkdir(parents=True)
            shutil.copy2(ROOT / 'scripts/run_contract_suite.py', checkout / 'scripts/run_contract_suite.py')
            for name in ('world.py', 'fault_partitions.py'):
                shutil.copy2(ROOT / 'tests' / name, checkout / 'tests' / name)
            target = checkout / 'tests/contracts/controlled.py'
            log = world.inside('actual-selected-points')
            entries = [{'name': name, 'path': 'tests/contracts/controlled.py'} for name in OWNERS]
            (checkout / 'tests/contracts.json').write_text(json.dumps({'suites': entries}))
            command = [sys.executable, '-I', '-B', checkout / 'scripts/run_contract_suite.py', target]
            for group, configuration in GROUPS.items():
                target.write_text(
                    'import json, unittest\nfrom pathlib import Path\n'
                    'from fault_partitions import PARTITIONS, select, selected_partition\n'
                    'class Controlled(unittest.TestCase):\n'
                    ' def test_replay(self):\n'
                    "  rows = [{'step': i} for i in range(PARTITIONS * 2 + 3)]\n"
                    f'  selected = select(rows, selected_partition({group!r}))\n'
                    f'  Path({str(log)!r}).write_text(json.dumps(selected))\n')
                # Exercise every index, not only partition zero, and all ten
                # ownership groups through the actual isolated worker consumer.
                for index, name in enumerate(configuration['suites']):
                    result = world.run([*command, '--suite', name], check=False)
                    self.assertEqual(result.returncode, 0, result.stderr)
                    self.assertEqual(json.loads(result.stdout)['tests'], 1)
                    self.assertEqual(json.loads(log.read_text()),
                                     [{'step': step} for step in range(PARTITIONS * 2 + 3)
                                      if step % PARTITIONS == index])
                direct = world.run(command, check=False)
                self.assertEqual(direct.returncode, 0, direct.stderr)
                self.assertEqual(json.loads(log.read_text()),
                                 [{'step': step} for step in range(PARTITIONS * 2 + 3)])
                log.unlink()
                other = next(name for name, owner in OWNERS.items() if owner[0] != group)
                wrong_group = world.run([*command, '--suite', other], check=False)
                self.assertNotEqual(wrong_group.returncode, 0)
                self.assertFalse(log.exists())
            elsewhere = checkout / 'tests/contracts/elsewhere.py'
            shutil.copy2(target, elsewhere)
            for name in (next(iter(OWNERS)), 'unregistered-suite'):
                refused = world.run([sys.executable, '-I', '-B', checkout / 'scripts/run_contract_suite.py',
                                     elsewhere, '--suite', name], check=False)
                self.assertNotEqual(refused.returncode, 0)
                self.assertFalse(log.exists())


if __name__ == '__main__':
    unittest.main()
