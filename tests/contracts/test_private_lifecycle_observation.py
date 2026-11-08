"""Lifecycle absence observations never create or invent source authority."""
from contextlib import contextmanager
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import unittest
import uuid
from unittest.mock import patch

from world import World
from runtime_update_fixtures import snapshot, version
import v014_runtime_fixture

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'skills/agent-memory/scripts'))
from agent_memory import bank_contract, bank_writes, private_state, recovery_frontier, turn_sources
from agent_memory.frontmatter import ContractError


class PrivateLifecycleObservation(unittest.TestCase):
    def fixture(self, world):
        repo = world.repository()
        bank = repo / 'memory-bank'
        bank.mkdir()
        (bank / 'my-note.md').write_text('Preserve these ordinary notes without inventing a router.\n')
        operations, _ = bank_writes.state_paths(repo, bank, '00000000-0000-4000-8000-000000000000')
        return repo, bank, operations.parent

    def files(self, world):
        return {str(path.relative_to(world.root)): (path.read_bytes(), path.stat().st_mode)
                for path in world.root.rglob('*') if path.is_file() and not path.is_symlink()}

    def observe(self):
        return private_state.required_private_state_capabilities(
            None, {'private_state_capabilities': []}, 'install')

    def test_routerless_notes_and_ordinary_receipts_remain_readonly_without_native_obligations(self):
        for prior_receipt in (False, True):
            with self.subTest(prior_receipt=prior_receipt), World() as world:
                with patch.dict(os.environ, world.env, clear=True):
                    repo, bank, base = self.fixture(world)
                    if prior_receipt:
                        operations = base / 'operations'
                        turn_sources.private_mkdir(operations)
                        receipt = operations / 'ordinary-owned-receipt.json'
                        receipt.write_text('{"retained":"ordinary"}\n')
                        receipt.chmod(0o600)
                    before = self.files(world)
                    with patch.object(Path, 'cwd', return_value=repo):
                        first, second = self.observe(), self.observe()
                    self.assertEqual(first['decision'], 'compatible', first)
                    self.assertEqual(second, first)
                    self.assertEqual(first['required_capabilities'], [])
                    self.assertEqual(self.files(world), before)
                    self.assertFalse((base / 'source-barrier.lock').exists())
                    if not prior_receipt:
                        self.assertFalse(base.exists())
                    self.assertFalse((bank / 'MEMORY.md').exists())
                    with self.assertRaises(ContractError):
                        bank_contract.validate(repo)

    def legacy_previews(self, world, seeded, expected):
        before = snapshot(world.root)
        for operation, script, arguments in (
            ('install', 'installer.py', ['install', '--check', '--remote', seeded['channel'],
                                        '--ref', 'main', '--presets', 'codex']),
            ('update', 'update_runtime.py', ['update', '--check']),
            ('uninstall', 'uninstall_runtime.py', ['uninstall', '--check'])):
            with self.subTest(operation=operation):
                result = world.run([sys.executable, '-I', ROOT/'scripts'/script, *arguments],
                                   check=False, timeout=120)
                value = json.loads(result.stdout)
                self.assertEqual(result.returncode, expected, value)
                if not expected:
                    if operation == 'install':
                        self.assertEqual(value['schema'], 'agent-memory/install-plan-v1')
                    else:
                        self.assertTrue(value['check_complete'])
                self.assertEqual(snapshot(world.root), before)
                self.assertFalse(seeded['barrier'].exists())

    def test_released_v014_sources_preview_then_upgrade_without_inventing_native_authority(self):
        with World() as world:
            seeded = v014_runtime_fixture.install_with_pending_source(world)
            before_bank, before_sources = snapshot(seeded['bank']), snapshot(seeded['sources'])
            self.legacy_previews(world, seeded, 0)
            pack = seeded['pack']
            shutil.rmtree(pack)
            shutil.copytree(ROOT/'skills/agent-memory', pack)
            version(pack, '0.0.15')
            world.git('add', '.', cwd=seeded['channel'])
            world.git('commit', '-m', 'Current disposable runtime for released updater', cwd=seeded['channel'])
            cli = Path(seeded['receipt']['bin_dir'])/'am'
            old = json.loads(world.run([cli, '--json', 'update'], timeout=120).stdout)
            self.assertTrue(old['verified'])
            self.assertEqual(old['receipt']['source']['version'], '0.0.15')
            self.assertFalse(seeded['barrier'].exists())
            self.legacy_previews(world, seeded, 0)
            version(pack, '0.0.16')
            world.git('add', '.', cwd=seeded['channel'])
            world.git('commit', '-m', 'Next disposable runtime for current updater', cwd=seeded['channel'])
            current = json.loads(world.run([cli, '--json', 'update'], timeout=120).stdout)
            self.assertTrue(current['verified'])
            self.assertEqual(current['receipt']['source']['version'], '0.0.16')
            self.assertTrue(seeded['barrier'].exists())
            self.assertEqual(snapshot(seeded['bank']), before_bank)
            self.assertEqual(snapshot(seeded['sources']), before_sources)

    def test_lockless_orphan_native_or_malformed_sources_refuse_readonly(self):
        for case in ('indexed-native', 'unindexed-native', 'malformed-source'):
            with self.subTest(case=case), World() as world:
                seeded = v014_runtime_fixture.install_with_pending_source(world)
                if case == 'malformed-source':
                    path, = seeded['sources'].glob('turns/*/source-0.json')
                    path.write_text('{incomplete')
                else:
                    with patch.dict(os.environ, world.env, clear=True):
                        turn_sources.register(seeded['repo'], 'omp', str(uuid.uuid4()), str(uuid.uuid4()),
                                              hashlib.sha256(b'Unresolved native fixture source').hexdigest())
                    if case == 'unindexed-native':
                        references = [path for path in (seeded['sources']/'pending').glob('*.json')
                                      if json.loads(path.read_bytes()).get('harness') == 'omp']
                        self.assertEqual(len(references), 1)
                        references[0].unlink()
                    seeded['barrier'].unlink()
                self.legacy_previews(world, seeded, 1)

    def test_native_barrier_appearing_during_layout_observation_refuses_without_erasing_it(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, bank, base = self.fixture(world)
            original = recovery_frontier._observe_bank
            injected = []
            def observe(*args, **kwargs):
                result = original(*args, **kwargs)
                if not injected:
                    with turn_sources.source_barrier(repo):
                        pass
                    injected.append((base / 'source-barrier.lock').read_bytes())
                return result
            with patch.object(Path, 'cwd', return_value=repo), \
                    patch.object(recovery_frontier, '_observe_bank', side_effect=observe):
                result = self.observe()
            self.assertEqual(len(injected), 1)
            self.assertEqual(result['decision'], 'pending')
            self.assertEqual((base / 'source-barrier.lock').read_bytes(), injected[0])
            self.assertEqual((bank / 'my-note.md').read_text(),
                             'Preserve these ordinary notes without inventing a router.\n')

    def test_existing_barrier_deleted_before_open_is_not_recreated(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, bank, base = self.fixture(world)
            with turn_sources.source_barrier(repo):
                pass
            lock = base / 'source-barrier.lock'
            original = turn_sources.private_lock
            injected = []
            @contextmanager
            def remove_before_open(path, **kwargs):
                if Path(path) == lock and not injected:
                    lock.unlink()
                    injected.append(True)
                with original(path, **kwargs):
                    yield
            with patch.object(Path, 'cwd', return_value=repo), \
                    patch.object(turn_sources, 'private_lock', side_effect=remove_before_open):
                result = self.observe()
            self.assertEqual(injected, [True])
            self.assertEqual(result['decision'], 'pending')
            self.assertFalse(lock.exists())
            self.assertFalse((bank / 'MEMORY.md').exists())


if __name__ == '__main__':
    unittest.main()
