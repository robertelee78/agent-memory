"""Aware public comparisons fail closed on substituted private frontier evidence."""
import copy
import json
import unittest
import uuid

from world import World
import layout_recovery_fixtures as f
from contracts.test_layout_recovery_reconciliation import revision_request


class RecoveryFrontierSubstitutionContract(unittest.TestCase):
    def test_foreign_claim_and_old_generation_refuse_then_fresh_review_succeeds(self):
        with World() as world:
            command, source, repo, bank, _, inspected, applied = f.small_pending(world)
            operation = inspected['operation_id']
            proposal, transaction, _ = f.admission_proposal(world, command, source, repo, bank, inspected, full=True)
            evidence = transaction['admission_evidence']
            request, proof = evidence['comparison_request'], evidence['comparison_proof']
            directory = f.private_directory(world, bank, operation)
            accepted = world.run([command, 'compare', 'check', '--repo', repo, '--request', f.save(world, request),
                                  '--proof', f.save(world, proof), '--private-output', world.inside('valid.json')],
                                 check=False, timeout=60)
            self.assertEqual(accepted.returncode, 0, accepted.stdout + accepted.stderr)
            before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            private_before = {p.relative_to(directory).as_posix(): p.read_bytes()
                              for p in directory.rglob('*') if p.is_file()}
            foreign = copy.deepcopy(proof)
            decision = foreign['judgment']['pending_decisions'][0]
            foreign_id = str(uuid.uuid4())
            decision['unit_id'] = foreign_id
            decision['coverage_claim']['unit_id'] = foreign_id
            refused = world.run([command, 'compare', 'check', '--repo', repo, '--request', f.save(world, request),
                                 '--proof', f.save(world, foreign), '--private-output', world.inside('foreign.json')],
                                check=False, timeout=60)
            self.assertNotEqual(refused.returncode, 0)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}, before)
            self.assertEqual({p.relative_to(directory).as_posix(): p.read_bytes()
                              for p in directory.rglob('*') if p.is_file()}, private_before)
            revision = world.inside('revision.json')
            f.call(world, command, 'revise-units', '--repo', repo, '--operation', operation,
                   '--input', f.save(world, revision_request(world, bank, inspected, applied['pending_unit_ids'][0])),
                   '--output', revision)
            f.call(world, command, 'revise-units', '--repo', repo, '--proposal', revision)
            changed_bank = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            refused = world.run([command, 'compare', 'check', '--repo', repo, '--request', f.save(world, request),
                                 '--proof', f.save(world, proof), '--private-output', world.inside('stale.json')],
                                check=False, timeout=60)
            self.assertNotEqual(refused.returncode, 0)
            f.public(world, command, 'admission', 'apply', '--proposal', proposal, expected=1)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}, changed_bank)
            fresh, _, _ = f.admission_proposal(world, command, source, repo, bank, inspected, full=False)
            f.public(world, command, 'admission', 'apply', '--proposal', fresh)
            status = f.call(world, command, 'status', '--repo', repo, '--operation', operation)
            self.assertEqual(len(status['pending_unit_ids']), 2)
            self.assertFalse(status['semantic_recovery_complete'])

    def test_missing_coverage_review_refuses_full_claim_without_changing_source(self):
        with World() as world:
            command, source, repo, bank, _, inspected, applied = f.small_pending(world)
            _, transaction, _ = f.admission_proposal(world, command, source, repo, bank, inspected, full=True)
            evidence = transaction['admission_evidence']
            request, proof = evidence['comparison_request'], evidence['comparison_proof']
            f.public(world, command, 'compare', 'check', '--repo', repo, '--request', f.save(world, request),
                     '--proof', f.save(world, proof), '--private-output', world.inside('reviewed.json'))
            directory = f.private_directory(world, bank, inspected['operation_id'])
            before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            private = {p.relative_to(directory).as_posix(): p.read_bytes()
                       for p in directory.rglob('*') if p.is_file()}
            missing = copy.deepcopy(proof)
            missing['judgment']['observation']['recovery_frontier']['review_digest'] = None
            refused = world.run([command, 'compare', 'check', '--repo', repo, '--request', f.save(world, request),
                                 '--proof', f.save(world, missing), '--private-output', world.inside('unreviewed.json')],
                                check=False, timeout=60)
            self.assertNotEqual(refused.returncode, 0)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}, before)
            self.assertEqual({p.relative_to(directory).as_posix(): p.read_bytes()
                              for p in directory.rglob('*') if p.is_file()}, private)
            self.assertEqual(f.call(world, command, 'status', '--repo', repo,
                                    '--operation', inspected['operation_id'])['pending_unit_ids'], applied['pending_unit_ids'])

    def test_corrupt_private_index_blocks_public_comparison_without_canonical_write(self):
        with World() as world:
            command, source, repo, bank, _, inspected, applied = f.small_pending(world)
            _, transaction, _ = f.admission_proposal(world, command, source, repo, bank, inspected, full=True)
            request = transaction['admission_evidence']['comparison_request']
            index = f.private_directory(world, bank, inspected['operation_id']).parent / 'index.json'
            original = index.read_bytes()
            index.write_bytes(b'{"corrupt": true}\n')
            before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            result = world.run([command, 'compare', 'inspect', '--repo', repo, '--request', f.save(world, request),
                                '--private-output', world.inside('corrupt-index.json')], check=False, timeout=60)
            self.assertNotEqual(result.returncode, 0)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}, before)
            index.write_bytes(original)
            output = world.inside('restored-index.json')
            f.public(world, command, 'compare', 'inspect', '--repo', repo, '--request', f.save(world, request),
                     '--private-output', output)
            self.assertEqual({row['unit_id'] for row in json.loads(output.read_text())['pending_matches']},
                             set(applied['pending_unit_ids']))


if __name__ == '__main__':
    unittest.main()
