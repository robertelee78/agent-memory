"""Old tools preserve actual hold, finish, rollback and terminal markers after lock recovery."""
import json
from pathlib import Path
import sys
import unittest
import uuid

from world import World
from runtime_update_fixtures import snapshot
from contracts import test_layout_recovery as existing
import layout_recovery_fixtures as recovery
import v014_runtime_fixture


class LayoutLegacyMarkers(unittest.TestCase):
    def journey(self, kind, decoy, receipted=False):
        with World() as legacy, World() as world:
            pack = v014_runtime_fixture.extract(legacy.root)
            installed = legacy.run([sys.executable, '-I', pack / 'scripts/installer.py', 'install',
                                    '--development-source', pack, '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old = Path(receipt['bin_dir']) / 'am'
            helper = existing.LayoutRecoveryContract()
            actual_receipt = None
            if receipted:
                command, source, repo, bank, _, first, _ = recovery.small_pending(world)
                admission, _, _ = recovery.admission_proposal(world, command, source, repo, bank, first, full=True)
                admitted = recovery.public(world, command, 'admission', 'apply', '--proposal', admission)
                actual_receipt = Path(admitted['receipt_path'])
                receipt_bytes = actual_receipt.read_bytes()
                native = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')
                          if not p.name.startswith('.agent-memory')}
                (bank / 'projectbrief.md').write_text(native['projectbrief.md'] * 2)
                if kind != 'terminal':
                    (bank / 'handoff-prompt.md').write_text(recovery.policy_body(0))
                originals = {p.relative_to(bank).as_posix(): p.read_bytes()
                             for p in bank.rglob('*') if p.is_file()}
                feature = repo / 'feature.txt'; feature.write_text('Committed feature\n')
                world.git('add', 'feature.txt', cwd=repo)
                world.git('commit', '-m', 'Synthetic feature baseline', cwd=repo)
                feature.write_text('Staged feature\n'); world.git('add', 'feature.txt', cwd=repo)
                feature.write_text('Unstaged feature\n')
                (repo / 'untracked.txt').write_bytes(b'Untracked feature\n')
                git = ((repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout)
            elif kind == 'terminal':
                command, source, repo, bank, native, originals = helper.setup_repair(world)
                feature = repo / 'feature.txt'; feature.write_text('Committed feature\n')
                world.git('add', 'feature.txt', cwd=repo)
                world.git('commit', '-m', 'Synthetic feature baseline', cwd=repo)
                feature.write_text('Staged feature\n'); world.git('add', 'feature.txt', cwd=repo)
                feature.write_text('Unstaged feature\n')
                (repo / 'untracked.txt').write_bytes(b'Untracked feature\n')
                git = ((repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout)
            else:
                command, source, repo, bank, native, originals, _, git = helper.setup_fault_repair(world)
            safe, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            if kind in ('finish', 'rollback'):
                recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1,
                              env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-after-rename:' +
                                   ('projectbrief.md' if receipted else 'activeContext.md')})
                preview = world.inside('legacy-interrupted-mode.json')
                recovery.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                              '--mode', kind, '--output', preview)
                args = [command, 'bank', 'recover-layout', 'recover', '--repo', repo, '--proposal', preview]
                stage = 'layout-recovery-after-marker-swap'
            else:
                args = [command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal]
                stage = 'layout-recovery-after-hold' if kind == 'hold' else 'layout-recovery-after-marker'
            fault = world.run(args, check=False, timeout=60,
                              env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(fault.returncode, 86, fault.stdout + fault.stderr)
            marker = bank / '.agent-memory-transaction.md'
            marker_bytes = marker.read_bytes()
            marker_value = json.loads(marker_bytes)
            if kind == 'hold':
                self.assertEqual(marker_value['schema'], 'agent-memory/layout-frontier-hold-v1')
            else:
                self.assertEqual(marker_value['schema'], 'agent-memory/layout-recovery-transaction-v1')
            directory = recovery.private_directory(world, bank, safe['operation_id'])
            decoy_path = None
            if decoy:
                decoy_id = str(uuid.uuid4())
                decoy_path = directory.parent.parent / 'transactions' / decoy_id
                decoy_path.mkdir(parents=True, mode=0o700)
                journal = decoy_path / 'journal.json'
                journal.write_text(json.dumps({'schema': 'agent-memory/transaction-journal-v1',
                                               'proposal': {'operation_id': decoy_id}, 'base_files': {}}))
                journal.chmod(0o600)
            lock_preview = world.run([old, 'writes', 'recover', '--repo', repo], check=False, timeout=60)
            self.assertEqual(lock_preview.returncode, 0, lock_preview.stdout + lock_preview.stderr)
            unlocked = world.run([old, 'writes', 'recover', '--repo', repo,
                                  '--proposal', recovery.save(world, json.loads(lock_preview.stdout))],
                                 check=False, timeout=60)
            self.assertEqual(unlocked.returncode, 0, unlocked.stdout + unlocked.stderr)
            self.assertFalse((bank / '.agent-memory.lock').exists())
            before = snapshot(bank), snapshot(directory), snapshot(decoy_path) if decoy_path else None
            for args in (['transaction', 'recover', '--repo', repo],
                         ['migrate', 'recover', '--repo', repo, '--mode', 'finish'],
                         ['migrate', 'recover', '--repo', repo, '--mode', 'rollback']):
                refused = world.run([old, *args], check=False, timeout=60)
                self.assertNotEqual(refused.returncode, 0, refused.stdout + refused.stderr)
                self.assertEqual((snapshot(bank), snapshot(directory), snapshot(decoy_path) if decoy_path else None), before)
                self.assertEqual(marker.read_bytes(), marker_bytes)
                if actual_receipt is not None:
                    self.assertEqual(actual_receipt.read_bytes(), receipt_bytes)
            doctor = world.run([old, 'doctor', '--fix', '--repo', repo, '--json'], check=False, timeout=60)
            self.assertFalse(json.loads(doctor.stdout)['repairs_performed'])
            self.assertEqual((snapshot(bank), snapshot(directory), snapshot(decoy_path) if decoy_path else None), before)
            if actual_receipt is not None:
                self.assertEqual(actual_receipt.read_bytes(), receipt_bytes)
            output = world.inside('aware-known-recovery.json')
            desired_mode = 'rollback' if kind == 'rollback' else 'finish'
            recovery.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                          '--mode', desired_mode, '--output', output)
            result = recovery.call(world, command, 'recover', '--repo', repo, '--proposal', output)
            self.assertTrue(result['verified'])
            wanted = originals if kind == 'rollback' else {path: body.encode() for path, body in native.items()}
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*')
                              if p.is_file() and p.name not in ('.agent-memory-transaction.md', '.agent-memory-recovery.md')}, wanted)
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertEqual(result['semantic_recovery_complete'], kind == 'terminal')
            if actual_receipt is not None:
                self.assertEqual(actual_receipt.read_bytes(), receipt_bytes)
            helper.assert_fault_work_unchanged(world, repo, git)


def _case(kind, decoy, receipted):
    def run(self):
        self.journey(kind, decoy, receipted)
    return run


for _kind in ('hold', 'apply', 'finish', 'rollback', 'terminal'):
    for _decoy in (False, True):
        for _receipted in (False, True):
            if _kind == 'apply' and not _decoy and not _receipted:
                continue  # The standalone stale-apply journey owns this case.
            _name = ('test_' + _kind + ('_decoy' if _decoy else '_plain')
                     + ('_receipted' if _receipted else ''))
            setattr(LayoutLegacyMarkers, _name, _case(_kind, _decoy, _receipted))
