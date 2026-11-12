"""Retention keeps real probe evidence and refuses unfinished or foreign leftovers."""
import json
import os
import unittest
import uuid

from world import World
from contracts import test_layout_recovery_retention as retention
from contracts import test_layout_recovery_darwin_provenance as exact_metadata
from runtime_update_fixtures import snapshot
import layout_recovery_fixtures as recovery


def metadata(root):
    return {str(path.relative_to(root)): (path.lstat().st_uid, path.lstat().st_gid, path.lstat().st_mode,
                                        exact_metadata.xattrs(path))
            for path in (root, *root.rglob('*'))}


class LayoutMetadataProbeRetention(unittest.TestCase):
    def setup(self, world):
        helper = retention.LayoutRetentionContract()
        command, _, repo, bank, observed, layout_path, layout_proposal, applied = helper.public_repair(world)
        self.assertTrue(applied['semantic_recovery_complete'])
        directory = recovery.private_directory(world, bank, observed['operation_id'])
        attempts = sorted((directory / 'metadata-probe-attempts').glob('*/plan.json'))
        self.assertTrue(attempts)
        plan_path = attempts[0]
        plan = json.loads(plan_path.read_bytes())
        completion = plan_path.parent / 'completion.json'
        self.assertTrue(completion.is_file())
        return helper, command, repo, bank, observed, layout_path, layout_proposal, directory, plan, completion

    def test_reviewed_cleanup_retains_completed_attempts_and_historical_receipt(self):
        with World() as world:
            helper, command, repo, bank, observed, layout_path, layout_proposal, directory, plan, _ = self.setup(world)
            attempts = snapshot(directory / 'metadata-probe-attempts')
            journal = (directory / 'journal.json').read_bytes()
            before = snapshot(bank), metadata(bank)
            reviewed, proposal = helper.reviewed_cleanup(world, command, repo, observed, layout_proposal)
            self.assertTrue(any(name.startswith('metadata-probe-attempts/') for name in proposal['retain']))
            self.assertIn('journal.json', proposal['retain'])
            for node in plan['nodes']:
                if node.get('nonce'):
                    self.assertNotIn(node['nonce'], json.dumps(proposal))
            recovery.call(world, command, 'retention', 'apply', '--repo', repo,
                          '--operation', observed['operation_id'], '--proposal', reviewed)
            self.assertEqual(snapshot(directory / 'metadata-probe-attempts'), attempts)
            self.assertEqual((directory / 'journal.json').read_bytes(), journal)
            self.assertEqual((snapshot(bank), metadata(bank)), before)
            self.assertTrue(recovery.call(world, command, 'apply', '--repo', repo,
                                          '--proposal', layout_path)['receipt_is_historical'])

    def test_missing_attempt_completion_refuses_without_deletion(self):
        with World() as world:
            _, command, repo, bank, observed, _, _, directory, _, completion = self.setup(world)
            completion.unlink()
            before = snapshot(bank), metadata(bank), snapshot(directory)
            recovery.call(world, command, 'retention', 'inspect', '--repo', repo,
                          '--operation', observed['operation_id'], '--output', world.inside('retention-refused.json'), expected=1)
            self.assertEqual((snapshot(bank), metadata(bank), snapshot(directory)), before)
            self.assertFalse(completion.exists())

    def test_foreign_probe_at_completed_path_refuses_without_deletion(self):
        with World() as world:
            _, command, repo, bank, observed, _, _, directory, plan, _ = self.setup(world)
            node = next(item for item in plan['nodes'] if item['type'] == 'file'
                        and (bank / item['path']).parent.is_dir())
            foreign = bank / node['path']
            self.assertFalse(foreign.exists())
            foreign.write_bytes(b'External fixture occupant must remain.\n')
            before = snapshot(bank), metadata(bank), snapshot(directory)
            recovery.call(world, command, 'retention', 'inspect', '--repo', repo,
                          '--operation', observed['operation_id'], '--output', world.inside('foreign-retention.json'), expected=1)
            self.assertEqual((snapshot(bank), metadata(bank), snapshot(directory)), before)
            self.assertEqual(foreign.read_bytes(), b'External fixture occupant must remain.\n')

    def test_changed_attempt_plan_refuses_without_deletion(self):
        with World() as world:
            _, command, repo, bank, observed, _, _, directory, plan, completion = self.setup(world)
            path = completion.parent / 'plan.json'
            plan['plan_sha256'] = '0' * 64
            path.write_bytes(recovery.encoded(plan))
            before = snapshot(bank), metadata(bank), snapshot(directory)
            recovery.call(world, command, 'retention', 'inspect', '--repo', repo,
                          '--operation', observed['operation_id'], '--output', world.inside('changed-plan.json'), expected=1)
            self.assertEqual((snapshot(bank), metadata(bank), snapshot(directory)), before)

    def test_unknown_pin_refuses_without_deletion(self):
        with World() as world:
            _, command, repo, bank, observed, _, _, directory, _, completion = self.setup(world)
            pins = completion.parent / 'pins'
            pins.mkdir(mode=0o700, exist_ok=True)
            foreign = pins / ('.agent-memory-probe-' + str(uuid.uuid4()) + '.json')
            foreign.write_bytes(recovery.encoded({'unowned': True})); foreign.chmod(0o600)
            before = snapshot(bank), metadata(bank), snapshot(directory)
            recovery.call(world, command, 'retention', 'inspect', '--repo', repo,
                          '--operation', observed['operation_id'], '--output', world.inside('unknown-pin.json'), expected=1)
            self.assertEqual((snapshot(bank), metadata(bank), snapshot(directory)), before)
            self.assertTrue(foreign.is_file())
