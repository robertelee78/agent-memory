"""Real installed CLI recovery at retained rollback and resolution boundaries."""
import copy
import json
from pathlib import Path
import unittest
import uuid

from world import World
import layout_recovery_fixtures as f

SCRATCH = '# Disposable scratch\n\nSynthetic temporary work-state; explicitly not durable guidance.\n'


def bank_bytes(bank):
    return {path.relative_to(bank).as_posix(): path.read_bytes() for path in bank.rglob('*')
            if path.is_file() and not path.name.startswith('.agent-memory')}


class RecoveryReconciliationContract(unittest.TestCase):
    def originals(self, world, bank, inspected, wanted):
        directory = f.private_directory(world, bank, inspected['operation_id'])
        for source in inspected['manifest']['sources']:
            self.assertEqual((directory / source['blob']).read_bytes(), wanted[source['path']])

    def test_raw_router_bootstrap_uses_checked_committed_identity_not_normal_router_limits(self):
        with World() as world:
            command, _ = f.installed(world)
            repo, bank, _ = f.memory(world)
            _, original_inspection = f.inspect(world, command, repo)
            committed = (bank / 'MEMORY.md').read_bytes()
            world.git('add', '.', cwd=repo)
            world.git('commit', '-m', 'Controlled existing router evidence', cwd=repo)
            head = world.git('rev-parse', 'HEAD', cwd=repo).stdout.strip()
            identity = original_inspection['manifest']['identity']['scope_context']
            for raw in (committed + b'\n' + b' ' * 17000, b'malformed router with ambiguous UUID-looking prose\n'):
                with self.subTest(size=len(raw)):
                    (bank / 'MEMORY.md').write_bytes(raw)
                    review = {'schema': 'agent-memory/layout-identity-review-v1', 'review_id': str(uuid.uuid4()),
                        'project_id': identity['project_id'], 'repo_root': str(repo),
                        'checkout_key': identity['checkout_key'], 'router_sha256': f.sha(raw),
                        'committed_router': head + ':memory-bank/MEMORY.md', 'committed_router_sha256': f.sha(committed),
                        'reviewer_developer_id': identity['developer_id'], 'reviewer_harness': 'controlled-reviewer',
                        'reviewer_session': 'router-review', 'author_session': 'router-author',
                        'reviewed_at': original_inspection['manifest']['inspected_at'],
                        'rationale': 'Independently checked existing registry/checkout and exact committed router.',
                        'decision': 'accept'}
                    before = bank_bytes(bank)
                    output = world.inside('raw-' + str(uuid.uuid4()) + '.json')
                    inspected = f.call(world, command, 'inspect', '--repo', repo,
                        '--identity-review', f.save(world, review), '--output', output)
                    private = json.loads(output.read_text())
                    self.assertTrue(inspected['observation_complete'])
                    self.assertEqual(private['manifest']['project_id'], identity['project_id'])
                    self.assertEqual(bank_bytes(bank), before)
                    self.originals(world, bank, private, before)
                    self.assertNotEqual(world.run([command, 'bank', 'validate', '--repo', repo], check=False, timeout=30).returncode, 0)
                    if raw.startswith(b'malformed'):
                        wrong = {**review, 'project_id': str(uuid.uuid4())}
                        f.call(world, command, 'inspect', '--repo', repo, '--identity-review', f.save(world, wrong),
                            '--output', world.inside('wrong-' + str(uuid.uuid4()) + '.json'), expected=1)
                        self.assertEqual(bank_bytes(bank), before)

    def test_retained_rollback_repair_handoff_keeps_both_independent_obligations(self):
        for completed in (False, True):
            with self.subTest(completed=completed), World() as world:
                command, source = f.installed(world)
                repo, bank, native = f.memory(world)
                (bank / 'handoff-prompt.md').write_text(SCRATCH)
                original = bank_bytes(bank)
                _, inspected = f.inspect(world, command, repo)
                proposal, _ = f.prepare(world, command, source, repo, inspected, native)
                if completed:
                    f.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
                    restore = world.inside('child.json')
                    f.call(world, command, 'rollback', '--repo', repo, '--operation', inspected['operation_id'], '--output', restore)
                    f.call(world, command, 'rollback', '--repo', repo, '--proposal', restore)
                else:
                    f.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1,
                        env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-before-receipt'})
                    restore = world.inside('rollback.json')
                    f.call(world, command, 'recover', '--repo', repo, '--operation', inspected['operation_id'],
                        '--mode', 'rollback', '--output', restore)
                    f.call(world, command, 'recover', '--repo', repo, '--proposal', restore)
                status = f.call(world, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
                old_ids = set(status['pending_unit_ids'])
                self.assertEqual(len(old_ids), 1)
                self.assertEqual(bank_bytes(bank), original)
                old_marker = (bank / '.agent-memory-transaction.md').read_bytes()
                _, continuation = f.inspect(world, command, repo)
                self.assertEqual(continuation['manifest']['predecessor']['operation_id'], inspected['operation_id'])
                self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), old_marker)
                next_proposal, _ = f.prepare(world, command, source, repo, continuation, native)
                applied = f.call(world, command, 'apply', '--repo', repo, '--proposal', next_proposal)
                self.assertEqual(len(applied['pending_unit_ids']), 2)
                self.assertTrue(old_ids < set(applied['pending_unit_ids']))
                new_ids = set(applied['pending_unit_ids']) - old_ids
                marker = (bank / '.agent-memory-transaction.md').read_bytes()
                self.assertNotEqual(marker, old_marker)
                self.assertEqual(json.loads(marker)['operation_id'], continuation['operation_id'])
                resolution = f.discard_proposal(world, command, repo, bank, continuation, next(iter(new_ids)))
                f.call(world, command, 'resolve', '--repo', repo, '--proposal', resolution)
                after = f.call(world, command, 'status', '--repo', repo, '--operation', continuation['operation_id'])
                self.assertEqual(set(after['pending_unit_ids']), old_ids)
                self.assertFalse(after['semantic_recovery_complete'])
                self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), marker)
                self.assertTrue((bank / '.agent-memory-recovery.md').exists())
                self.originals(world, bank, inspected, original)
                directory = f.private_directory(world, bank, inspected['operation_id'])
                handoff = next(row for row in inspected['manifest']['sources'] if row['path'] == 'handoff-prompt.md')
                original_blob = directory / handoff['blob']
                original_blob.write_bytes(original_blob.read_bytes() + b'changed')
                f.call(world, command, 'status', '--repo', repo, '--operation', continuation['operation_id'], expected=1)
                self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), marker)

    def test_standalone_receipt_retry_finishes_exact_linkage_and_retirement(self):
        cases = [('layout-recovery-resolution-after-receipt', 'exit'),
                 ('layout-recovery-ledger-generation-before-head', 'error'),
                 ('layout-recovery-ledger-generation-after-head', 'exit'),
                 ('layout-recovery-sentinel-retirement', 'error'),
                 ('layout-recovery-retirement-after-marker-removal', 'exit'),
                 ('layout-recovery-retirement-after-sentinel-removal', 'error')]
        for stage, mode in cases:
            with self.subTest(stage=stage, mode=mode), World() as world:
                command, _, repo, bank, _, inspected, applied = f.small_pending(world, SCRATCH)
                canonical = bank_bytes(bank)
                directory = f.private_directory(world, bank, inspected['operation_id'])
                originals = {source['path']: (directory / source['blob']).read_bytes() for source in inspected['manifest']['sources']}
                proposal = f.discard_proposal(world, command, repo, bank, inspected, applied['pending_unit_ids'][0])
                request = json.loads(proposal.read_text())['proposal']
                faulted = world.run([command, 'bank', 'recover-layout', 'resolve', '--repo', repo, '--proposal', proposal],
                    check=False, timeout=30, env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
                self.assertEqual(faulted.returncode, 86 if mode == 'exit' else 1, faulted.stdout + faulted.stderr)
                receipt_file = directory / 'resolutions' / (request['resolution_id'] + '.json')
                receipt = receipt_file.read_bytes()
                if mode == 'exit':
                    f.unlock(world, command, repo)
                changed = copy.deepcopy(request)
                changed['terminal_state'] = 'admitted'
                f.call(world, command, 'resolve', '--repo', repo, '--proposal', f.save(world, changed), expected=1)
                f.call(world, command, 'resolve', '--repo', repo, '--proposal', proposal)
                head = json.loads((directory / 'ledger-head.json').read_text())
                ledger = json.loads((directory / 'ledger/00000001.json').read_text())
                self.assertEqual(head['generation'], 1)
                self.assertEqual(ledger['units'][0]['state'], 'reviewed-non-guidance-discard')
                self.assertEqual(len(ledger['units'][0]['terminal_receipts']), 1)
                self.assertEqual(receipt_file.read_bytes(), receipt)
                self.assertFalse((bank / '.agent-memory-transaction.md').exists())
                self.assertFalse((bank / '.agent-memory-recovery.md').exists())
                status = f.call(world, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
                self.assertTrue(status['semantic_recovery_complete'])
                self.assertEqual(status['pending_units'], 0)
                history = f.call(world, command, 'resolve', '--repo', repo, '--proposal', proposal)
                self.assertTrue(history['already_applied'])
                self.assertFalse(history['writes_performed'])
                self.assertEqual(bank_bytes(bank), canonical)
                self.originals(world, bank, inspected, originals)


if __name__ == '__main__':
    unittest.main()
