"""One real admission can cover eight fully reviewed source units without dropping provenance."""
import json
import unittest

from world import World
import layout_recovery_fixtures as f
from contracts.test_comparison import unpack


class EightUnitAdmissionContract(unittest.TestCase):
    def test_reported_handoff_resolves_every_unit_against_actual_admission(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, native = f.memory(w, oversized=True, handoff=True)
            evidence = repo / 'synthetic-policy.md'
            evidence.write_text('# Independent synthetic source\n\n' + '\n\n'.join(f.POLICIES) + '\n')
            feature = repo / 'feature.txt'
            feature.write_text('committed feature\n')
            w.git('add', 'synthetic-policy.md', 'feature.txt', cwd=repo)
            w.git('commit', '-m', 'Synthetic evidence and feature', cwd=repo)
            feature.write_text('staged feature\n'); w.git('add', 'feature.txt', cwd=repo)
            feature.write_text('unstaged feature\n')
            before = (evidence.read_bytes(), feature.read_bytes(), (repo / '.git/index').read_bytes(),
                w.git('rev-parse', 'HEAD', cwd=repo).stdout)
            originals = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            self.assertEqual(len(originals['handoff-prompt.md']), 28478)
            safe, inspected = f.inspect(w, command, repo)
            proposal, _ = f.prepare(w, command, source, repo, inspected, native)
            applied = f.call(w, command, 'apply', '--repo', repo, '--operation', safe['operation_id'], '--proposal', proposal)
            self.assertTrue(applied['canonical_eligibility'])
            self.assertFalse(applied['semantic_recovery_complete'])
            pending = set(applied['pending_unit_ids'])
            self.assertEqual(len(pending), 8)
            directory = f.private_directory(w, bank, safe['operation_id'])
            initial = json.loads((directory / 'ledger/00000000.json').read_text())
            initial_units = {unit['unit_id']: unit for unit in initial['units']}
            self.assertEqual(set(initial_units), pending)
            for original in inspected['manifest']['sources']:
                self.assertEqual((directory / original['blob']).read_bytes(), originals[original['path']])
            admission, transaction, target = f.admission_proposal(w, command, source, repo, bank,
                inspected, full=True, policy_number=None)
            result = f.public(w, command, 'admission', 'apply', '--proposal', admission)
            self.assertEqual(result['operation_id'], transaction['operation_id'])
            status = f.call(w, command, 'status', '--repo', repo, '--operation', safe['operation_id'])
            self.assertTrue(status['semantic_recovery_complete'])
            self.assertEqual(status['pending_unit_ids'], [])
            head = json.loads((directory / 'ledger-head.json').read_text())
            ledger = json.loads((directory / 'ledger' / (str(head['generation']).zfill(8) + '.json')).read_text())
            self.assertEqual({unit['unit_id'] for unit in ledger['units']}, pending)
            receipt_ids = set()
            for unit in ledger['units']:
                self.assertEqual(unit['state'], 'admitted')
                self.assertEqual(unit['source_ranges'], initial_units[unit['unit_id']]['source_ranges'])
                self.assertEqual(unit['provenance'], initial_units[unit['unit_id']]['provenance'])
                self.assertEqual(len(unit['terminal_receipts']), 1)
                link = unit['terminal_receipts'][0]
                receipt_ids.add(link['resolution_id'])
                receipt = json.loads((directory / 'resolutions' / (link['resolution_id'] + '.json')).read_text())
                self.assertEqual(f.sha(f.encoded(receipt)), link['sha256'])
                self.assertEqual(receipt['canonical_operation_id'], transaction['operation_id'])
                self.assertEqual(receipt['unit_id'], unit['unit_id'])
            self.assertEqual(len(receipt_ids), 8)
            metadata, body = unpack((bank / target).read_text())
            for policy in f.POLICIES:
                self.assertIn(policy, body)
            self.assertIn('synthetic-policy.md', metadata['evidence'])
            self.assertEqual((evidence.read_bytes(), feature.read_bytes(), (repo / '.git/index').read_bytes(),
                w.git('rev-parse', 'HEAD', cwd=repo).stdout), before)
            for original in inspected['manifest']['sources']:
                self.assertEqual((directory / original['blob']).read_bytes(), originals[original['path']])


if __name__ == '__main__':
    unittest.main()
