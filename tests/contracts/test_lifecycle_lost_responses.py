"""Drop actual successful public lifecycle responses after verified commit."""
import json
from pathlib import Path
import unittest

from world import World
from lifecycle_fault_fixtures import invoke, helper, setup, PurgeWriteCases
from runtime_update_fixtures import advance, snapshot, receipt_path


class LifecycleLostResponses(unittest.TestCase):
    def lose_and_repeat(self, world, argv, name, memory):
        before = snapshot(memory)
        invoke(world, argv, expected=86, env={'AGENT_MEMORY_FAULT_STAGE': name + '-response-lost',
                                            'AGENT_MEMORY_FAULT_MODE': 'exit'})
        committed = receipt_path(world).read_bytes()
        journal = json.loads((receipt_path(world).parent / 'journal.json').read_text())
        self.assertIn(journal['status'], {'complete', 'rolled-back'})
        self.assertEqual(snapshot(memory), before)
        result = invoke(world, argv); self.assertTrue(result['verified'])
        self.assertEqual(receipt_path(world).read_bytes(), committed)
        self.assertEqual(snapshot(memory), before)
        return json.loads(committed), result

    def test_install_response_is_lost_after_real_receipt_and_targets_commit(self):
        with World() as world:
            argv, memory = setup(world, 'copy')
            receipt, result = self.lose_and_repeat(world, argv, 'install', memory)
            self.assertFalse(result['changed']); self.assertFalse(receipt.get('uninstalled', False))

    def test_update_response_is_lost_after_real_new_revision_is_active(self):
        with World() as world:
            argv, memory = setup(world, 'copy'); original = invoke(world, argv)['receipt']
            channel = Path(original['source']['remote'])
            revision = advance(world, channel, channel / 'skills/agent-memory')
            receipt, result = self.lose_and_repeat(world, helper('update'), 'update', memory)
            self.assertFalse(result['changed']); self.assertEqual(receipt['source']['revision'], revision)
            self.assertEqual(receipt['installation_id'], original['installation_id'])

    def test_uninstall_response_is_lost_after_runtime_is_removed_and_data_retained(self):
        with World() as world:
            argv, memory = setup(world, 'copy'); original = invoke(world, argv)['receipt']
            receipt, result = self.lose_and_repeat(world, helper('uninstall'), 'uninstall', memory)
            self.assertTrue(receipt['uninstalled']); self.assertFalse(result['changed'])
            self.assertFalse(Path(original['source']['path']).exists())

    def test_recovery_response_is_lost_after_real_rollback_is_durable(self):
        with World() as world:
            argv, memory = setup(world, 'copy'); original = invoke(world, argv)['receipt']
            invoke(world, helper('uninstall'), expected=86, env={
                'AGENT_MEMORY_FAULT_STAGE': 'uninstall-after-activate-0', 'AGENT_MEMORY_FAULT_MODE': 'exit'})
            receipt, result = self.lose_and_repeat(world, helper('recover'), 'recover', memory)
            self.assertEqual(result['status'], 'rolled-back')
            self.assertEqual(receipt, original)

    def test_purge_response_loss_reuses_the_exact_original_confirmation(self):
        with World() as world:
            memory, preview, argv = PurgeWriteCases().fixture(world)
            invoke(world, argv, expected=86, env={'AGENT_MEMORY_FAULT_STAGE': 'purge-response-lost',
                                                'AGENT_MEMORY_FAULT_MODE': 'exit'})
            path = receipt_path(world).parent / 'purges' / (preview['confirmation'] + '.json')
            raw = path.read_bytes(); value = json.loads(raw)
            self.assertEqual(value['status'], 'complete'); self.assertTrue(value['deletion_readback_verified'])
            self.assertFalse(memory.exists()); self.assertFalse(Path(value['stage']).exists())
            result = invoke(world, argv); self.assertTrue(result['verified']); self.assertFalse(result['changed'])
            self.assertEqual(path.read_bytes(), raw)

    def test_fault_trace_refuses_another_world_target_before_bootstrap_writes(self):
        with World() as world, World() as other:
            argv, _ = setup(world, 'copy')
            before, neighbor = snapshot(world.root), snapshot(other.root)
            refused = invoke(world, [*argv, '--target-dir', other.paths['skills']], expected=1,
                             env={'AGENT_MEMORY_FAULT_TRACE': str(world.inside('trace.jsonl'))})
            self.assertFalse(refused['verified'])
            self.assertEqual(snapshot(world.root), before); self.assertEqual(snapshot(other.root), neighbor)

    def test_fault_trace_itself_cannot_escape_into_another_world(self):
        with World() as world, World() as other:
            argv, _ = setup(world, 'copy')
            before, neighbor = snapshot(world.root), snapshot(other.root)
            refused = invoke(world, argv, expected=1,
                             env={'AGENT_MEMORY_FAULT_TRACE': str(other.inside('trace.jsonl'))})
            self.assertFalse(refused['verified'])
            self.assertEqual(snapshot(world.root), before); self.assertEqual(snapshot(other.root), neighbor)
