"""A real old reader and writer cannot cross an atomic marker handoff."""
import json
from pathlib import Path
import sys
import time
import unittest

from world import World
from contracts import test_admission
from contracts import test_layout_recovery as existing
import layout_recovery_fixtures as recovery
import v014_runtime_fixture


class LayoutLegacySwap(unittest.TestCase):
    def journey(self, boundary, *, to_hold=False):
        with World() as legacy, World() as world:
            pack = v014_runtime_fixture.extract(legacy.root)
            installed = legacy.run([sys.executable, '-I', pack / 'scripts/installer.py', 'install',
                                    '--development-source', pack, '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old = Path(receipt['bin_dir']) / 'am'
            helper = existing.LayoutRecoveryContract()
            command, source, repo, bank, native, originals, modes, git = helper.setup_fault_repair(world)
            safe, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            admission = test_admission.AdmissionContract()
            admission.pid = inspected['manifest']['project_id']
            admission.actor = inspected['manifest']['identity']['scope_context']['developer_id']
            if to_hold:
                recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1,
                              env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-after-receipt'})
                finish = world.inside('paused-finish.json')
                recovery.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                              '--mode', 'finish', '--output', finish)
                child_command = [command, 'bank', 'recover-layout', 'recover', '--repo', repo, '--proposal', finish]
            else:
                child_command = [command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal]
            capture = recovery.save(world, admission.request())
            rendezvous = world.inside('marker-rendezvous')
            stage = 'layout-recovery-' + boundary + '-marker-swap'
            world.env.update(AGENT_MEMORY_FAULT_STAGE=stage, AGENT_MEMORY_FAULT_MODE='pause',
                             AGENT_MEMORY_FAULT_DIR=str(rendezvous))
            child = world.spawn(child_command)
            for key in ('AGENT_MEMORY_FAULT_STAGE', 'AGENT_MEMORY_FAULT_MODE', 'AGENT_MEMORY_FAULT_DIR'):
                world.env.pop(key)
            ready = rendezvous / (stage + '.ready')
            try:
                deadline = time.monotonic() + 10
                while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                    time.sleep(.01)
                self.assertTrue(ready.exists())
                marker = bank / '.agent-memory-transaction.md'
                saved = marker.read_bytes()
                self.assertIn(json.loads(saved)['schema'], {'agent-memory/layout-frontier-hold-v1',
                                                           'agent-memory/layout-recovery-transaction-v1'})
                before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
                for args in (['bank', 'validate', '--repo', repo],
                             ['capture', '--repo', repo, '--request', capture],
                             ['transaction', 'recover', '--repo', repo]):
                    old_result = world.run([old, *args], check=False, timeout=30)
                    self.assertNotEqual(old_result.returncode, 0, old_result.stdout + old_result.stderr)
                    self.assertEqual(marker.read_bytes(), saved)
                    self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}, before)
            finally:
                rendezvous.mkdir(exist_ok=True)
                (rendezvous / (stage + '.release')).touch()
                stdout, stderr = child.communicate(timeout=60)
            self.assertEqual(child.returncode, 0, stdout + stderr)
            directory = recovery.private_directory(world, bank, safe['operation_id'])
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            helper.assert_fault_work_unchanged(world, repo, git)
            status = recovery.call(world, command, 'status', '--repo', repo, '--operation', safe['operation_id'])
            self.assertFalse(status['semantic_recovery_complete'])
            self.assertGreater(status['pending_units'], 0)

    def test_old_consumers_before_swap(self):
        self.journey('before')

    def test_old_consumers_after_swap(self):
        self.journey('after')

    def test_old_consumers_before_hold_swap(self):
        self.journey('before', to_hold=True)

    def test_old_consumers_after_hold_swap(self):
        self.journey('after', to_hold=True)
