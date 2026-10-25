"""Deleted registered worktrees stay gaps, not fabricated explicit inputs."""
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from identity_fixtures import bind
from runtime_update_fixtures import ROOT, fixture, advance, call, snapshot, bank, receipt_path, owned


class UpdateDiscoveryGapsContract(unittest.TestCase):
    def test_deleted_registered_worktree_does_not_block_update_repeat_or_recovery(self):
        for mode in ('link', 'copy'):
            with self.subTest(mode=mode), World() as world:
                channel, pack, receipt = fixture(world, mode)
                repo = world.repository('registered project'); memory = bank(world, repo)
                bind(world, repo)
                world.git('add', '.', cwd=repo); world.git('commit', '-m', 'Bound project', cwd=repo)
                missing = world.inside('deleted session')
                world.git('worktree', 'add', '-b', 'deleted-session', missing, cwd=repo)
                shutil.rmtree(missing)  # Leave real stale Git worktree metadata.
                before = snapshot(repo)
                revision = advance(world, channel, pack)
                all_before = snapshot(world.root)
                _, preview = call(world, receipt, '--check')
                self.assertEqual(snapshot(world.root), all_before)
                compatibility = preview['compatibility']
                self.assertTrue(any(g['location'] == str(missing) for g in compatibility['discovery_gaps']))
                self.assertNotIn(str(missing), compatibility['observed_repositories'])
                self.assertIn(str(repo), compatibility['observed_repositories'])
                _, updated = call(world, receipt)
                self.assertTrue(updated['verified']); self.assertEqual(updated['receipt']['source']['revision'], revision)
                self.assertEqual(snapshot(repo), before); self.assertTrue(memory.is_dir())
                self.assertFalse(call(world, updated['receipt'])[1]['changed'])
                recovered = world.run([sys.executable, '-I', ROOT/'scripts/installer.py', 'recover'])
                self.assertTrue(json.loads(recovered.stdout)['verified'])
                self.assertEqual(snapshot(repo), before)

    def test_missing_explicit_repository_remains_an_error_and_preserves_targets(self):
        with World() as world:
            _, _, receipt = fixture(world)
            missing = world.inside('explicit missing repo')
            before = owned(receipt); raw = receipt_path(world).read_bytes()
            for options in (('--check',), ()):
                _, result = call(world, receipt, *options, '--repo', missing, expected=1)
                self.assertIn('Explicit repository is unavailable', result['error'])
                self.assertEqual(owned(receipt), before); self.assertEqual(receipt_path(world).read_bytes(), raw)

    def test_available_repository_without_a_bank_stays_in_compatibility_rechecks(self):
        with World() as world:
            _, _, receipt = fixture(world)
            repo = world.repository('existing empty project')
            before = snapshot(world.root)
            _, result = call(world, receipt, '--check', '--repo', repo)
            self.assertIn(str(repo), result['compatibility']['observed_repositories'])
            self.assertFalse(any(b['repository'] == str(repo) for b in result['compatibility']['banks']))
            self.assertEqual(snapshot(world.root), before)
