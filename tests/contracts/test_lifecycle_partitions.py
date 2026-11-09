"""A release must cover every observed interruption exactly once."""
import copy
import unittest

from fault_partitions import GROUPS, coverage


class LifecyclePartitionContract(unittest.TestCase):
    def receipts(self):
        suites = []
        for group, contract in GROUPS.items():
            observed = [{'step': i, 'point': f'write-{i}', 'kind': 'file-stage'}
                        for i in range(8)]
            for index, name in enumerate(contract['suites']):
                suites.append({'name': name, 'status': 'passed', 'fault_partition': {
                    'group': group, 'index': index, 'count': 4,
                    'artifact_digest': 'a' * 64, 'observed': copy.deepcopy(observed),
                    'executed': [f'write-{index}', f'write-{index + 4}']}})
        return suites

    def test_complete_actual_interruption_inventory_and_artifact_agree(self):
        summaries = {row['group']: row for row in coverage(self.receipts())}
        self.assertEqual(summaries['uninstall-write-copy-before'], {
            'group': 'uninstall-write-copy-before', 'partitions': 4,
            'observed_points': 8, 'actual_interruptions_and_retries': 8,
            'artifact_digest': 'a' * 64})

    def test_missing_duplicate_or_foreign_interruption_cannot_complete_release(self):
        for failure in ('missing-partition', 'omitted-write', 'duplicate-write',
                        'different-artifact', 'different-observed-inventory'):
            suites = self.receipts()
            target = next(row for row in suites if row['name'] == 'uninstall-write-copy-before')
            proof = target['fault_partition']
            if failure == 'missing-partition':
                suites.remove(target)
            elif failure == 'omitted-write':
                proof['executed'].pop()
            elif failure == 'duplicate-write':
                proof['executed'][1] = proof['executed'][0]
            elif failure == 'different-artifact':
                proof['artifact_digest'] = 'b' * 64
            else:
                proof['observed'][0]['point'] = 'unobserved-write'
            with self.subTest(failure=failure), self.assertRaises(ValueError):
                coverage(suites)


if __name__=='__main__':unittest.main()
