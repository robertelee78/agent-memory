"""Public comparison-v2 and private-frontier behavior, not wording/source tests."""
import copy
import json
from pathlib import Path
import stat
import unittest

from world import World
from contracts.test_comparison import ComparisonContract
import layout_recovery_fixtures as recovery


class RecoveryFrontierContract(unittest.TestCase):
    def test_checked_empty_frontier_is_explicit_and_v1_or_omitted_fields_cannot_write(self):
        helper = ComparisonContract(methodName='runTest')
        with World() as world:
            repo, bank = helper.fixture(world)
            helper.add(bank, 'existing')
            request = helper.candidate()
            before = helper.state(world, repo, bank)
            inspection = helper.inspect(world, repo, request)
            self.assertEqual(inspection['schema'], 'agent-memory/comparison-result-v2')
            self.assertEqual(inspection['observation']['schema'], 'agent-memory/comparison-observation-v2')
            token = inspection['observation']['recovery_frontier']
            self.assertTrue(token['coverage_complete'])
            self.assertEqual(token['unit_ids'], [])
            self.assertEqual(inspection['pending_matches'], [])
            judgment = helper.judgment(inspection)
            accepted, reviewed = helper.review(world, repo, request, judgment)
            self.assertEqual(accepted.returncode, 0, reviewed)
            self.assertEqual(reviewed['proof']['schema'], 'agent-memory/comparison-proof-v2')
            for field in ('pending_decisions',):
                missing = copy.deepcopy(judgment)
                missing.pop(field)
                refused, _ = helper.review(world, repo, request, missing)
                self.assertNotEqual(refused.returncode, 0)
            missing_frontier = copy.deepcopy(judgment)
            missing_frontier['observation'].pop('recovery_frontier')
            self.assertNotEqual(helper.review(world, repo, request, missing_frontier)[0].returncode, 0)
            legacy = copy.deepcopy(reviewed['proof'])
            legacy['schema'] = 'agent-memory/comparison-proof-v1'
            self.assertNotEqual(helper.check(world, repo, request, legacy)[0].returncode, 0)
            self.assertEqual(helper.state(world, repo, bank), before)

    def pending(self, world):
        command, source = recovery.installed(world)
        repo, bank, native = recovery.memory(world, oversized=True, handoff=True)
        originals = {path.relative_to(bank).as_posix(): path.read_bytes()
            for path in bank.rglob('*') if path.is_file()}
        safe, inspected = recovery.inspect(world, command, repo)
        proposal_file, prepared = recovery.prepare(world, command, source, repo, inspected, native)
        applied = recovery.call(world, command, 'apply', '--repo', repo,
            '--operation', safe['operation_id'], '--proposal', proposal_file)
        self.assertTrue(applied['canonical_eligibility'])
        self.assertFalse(applied['semantic_recovery_complete'])
        return command, repo, bank, inspected, originals

    def recall(self, world, command, repo, project):
        request = {'project_id': project, 'query': '', 'ids': [], 'paths': [], 'context': {},
            'kinds': [], 'minimum_trust': 'reviewed', 'limit': 8, 'include_context': True}
        result = world.run([command, 'search', '--repo', repo, '--request', recovery.save(world, request)],
            check=False, env={'AGENT_MEMORY_NO_RECALL_CATALOG': '1'}, timeout=60)
        return result, json.loads(result.stdout)

    def test_reported_size_pending_repair_retains_exact_private_originals_and_qualified_recall(self):
        with World() as world:
            command, repo, bank, inspected, originals = self.pending(world)
            manifest = inspected['manifest']
            operation = manifest['operation_id']
            directory = (Path(world.env['AGENT_MEMORY_STATE_HOME']) / 'writes' /
                recovery.sha(str(bank).encode()) / 'layout-recovery' / operation)
            self.assertEqual(stat.S_IMODE(directory.stat().st_mode), 0o700)
            for source in manifest['sources']:
                original = directory / source['blob']
                self.assertEqual(stat.S_IMODE(original.stat().st_mode), 0o600)
                self.assertEqual(original.read_bytes(), originals[source['path']])
            result, recalled = self.recall(world, command, repo, manifest['project_id'])
            self.assertEqual(result.returncode, 0, recalled)
            self.assertEqual(recalled['semantic_coverage'], 'incomplete-recovery')
            self.assertEqual(recalled['pending_recovery_units'], 8)
            self.assertTrue(recalled['results'])
            helper = ComparisonContract(methodName='runTest')
            helper.pid = manifest['project_id']
            helper.actor = manifest['identity']['scope_context']['developer_id']
            request = helper.candidate()
            refused = world.run([command, 'compare', 'inspect', '--repo', repo,
                '--request', recovery.save(world, request)], check=False, timeout=60)
            self.assertNotEqual(refused.returncode, 0)
            self.assertNotIn('pending_matches', json.loads(refused.stdout))
            output = world.inside('private-frontier-inspection.json')
            private = world.run([command, 'compare', 'inspect', '--repo', repo,
                '--request', recovery.save(world, request), '--private-output', output], check=False, timeout=60)
            self.assertEqual(private.returncode, 0, private.stdout + private.stderr)
            envelope = json.loads(private.stdout)
            self.assertNotIn('pending_matches', envelope)
            view = json.loads(output.read_text())
            self.assertEqual(len(view['pending_matches']), 8)
            self.assertEqual({row['unit_id'] for row in view['pending_matches']},
                set(recalled['pending_recovery_unit_ids']))
            for row in view['pending_matches']:
                self.assertNotIn(row['body'], private.stdout + private.stderr)

    def test_removed_sentinel_is_pending_not_successful_empty_coverage(self):
        with World() as world:
            command, repo, bank, inspected, _ = self.pending(world)
            (bank / '.agent-memory-recovery.md').unlink()
            result, value = self.recall(world, command, repo, inspected['manifest']['project_id'])
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('results', value)
            self.assertTrue((bank / '.agent-memory-transaction.md').exists())

    def test_rolled_back_pending_units_keep_private_ledger_and_legacy_hold(self):
        with World() as world:
            command, source = recovery.installed(world)
            repo, bank, native = recovery.memory(world, oversized=True, handoff=True)
            first, observed = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, observed, native)
            applied = recovery.call(world, command, 'apply', '--repo', repo,
                '--operation', first['operation_id'], '--proposal', proposal)
            old_ids = set(applied['pending_unit_ids'])
            self.assertEqual(len(old_ids), 8)
            restore = world.inside('child-restore.json')
            recovery.call(world, command, 'rollback', '--repo', repo,
                '--operation', first['operation_id'], '--output', restore)
            rolled_back = recovery.call(world, command, 'rollback', '--repo', repo, '--proposal', restore)
            self.assertFalse(rolled_back['semantic_recovery_complete'])
            self.assertTrue((bank / '.agent-memory-transaction.md').exists())
            self.assertTrue((bank / '.agent-memory-recovery.md').exists())
            status = recovery.call(world, command, 'status', '--repo', repo,
                '--operation', first['operation_id'])
            self.assertFalse(status['semantic_recovery_complete'])
            self.assertEqual(set(status['pending_unit_ids']), old_ids)
            blocked = world.run([command, 'bank', 'validate', '--repo', repo], check=False, timeout=60)
            self.assertNotEqual(blocked.returncode, 0)


if __name__ == '__main__':
    unittest.main()
