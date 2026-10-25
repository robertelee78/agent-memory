"""Installed human/JSON setup evidence without implicit initialization or repair."""
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World
from runtime_update_fixtures import ROOT, fixture, bank, snapshot, receipt_path
from identity_fixtures import bind
from contracts import test_initialization as initialization
import installed_cli_fixtures as installed


class SetupDiagnostics(unittest.TestCase):
    def check(self, w, cli, repo, operation, state, expected=0):
        before = snapshot(w.root)
        r = w.run([cli, operation, '--repo', repo, '--json'], check=False, timeout=120)
        self.assertEqual(r.returncode, expected, r.stdout + r.stderr)
        value = json.loads(r.stdout)
        self.assertEqual(value['setup']['state'], state, value)
        self.assertTrue(value['setup']['next_step'])
        self.assertFalse(value['writes_performed'])
        human = w.run([cli, operation, '--repo', repo], check=False, timeout=120)
        self.assertEqual(human.returncode, expected, human.stdout + human.stderr)
        self.assertIn('setup ' + state, human.stdout)
        self.assertIn('next ' + value['setup']['next_step'], human.stdout)
        self.assertEqual(snapshot(w.root), before)
        return value

    def test_new_repository_and_precreated_identity_need_setup_without_invented_mismatch(self):
        with World() as w:
            _, _, receipt = fixture(w)
            cli = Path(receipt['bin_dir']) / 'am'
            repo = w.repository()
            for reserved in (False, True):
                if reserved:
                    h = initialization.InitializationContract()
                    h.command = lambda world, name, *args, **kw: installed.invoke(world, cli, name, *args, **kw)
                    p = h.good(w, 'developer_identity', 'preview', '--create', '--label', 'Fixture developer')
                    h.good(w, 'developer_identity', 'apply', '--proposal', h.save(w, p))
                    p = h.good(w, 'scope_binding', 'preview', '--repo', repo, '--action', 'create', '--reason', 'Reserve explicit fixture identity')
                    h.good(w, 'scope_binding', 'apply', '--proposal', h.save(w, p))
                status = self.check(w, cli, repo, 'status', 'setup_needed')
                self.assertFalse(status['setup']['absence_verified'])
                self.assertFalse(any(c['component'] == 'identity.project' for c in status['warnings']))
                doctor = self.check(w, cli, repo, 'doctor', 'not_initialized')
                self.assertTrue(doctor['setup']['absence_verified'])
                self.assertFalse((repo / 'memory-bank').exists())

    def test_bank_binding_required_matching_and_actual_mismatch_are_distinct(self):
        with World() as w:
            _, _, receipt = fixture(w)
            cli = Path(receipt['bin_dir']) / 'am'
            repo = w.repository()
            directory = bank(w, repo)
            self.check(w, cli, repo, 'status', 'binding_required')
            self.check(w, cli, repo, 'doctor', 'binding_required')
            bind(w, repo)
            self.check(w, cli, repo, 'status', 'bound_not_audited')
            self.check(w, cli, repo, 'doctor', 'ready')
            router = directory / 'MEMORY.md'
            router.write_text(router.read_text().replace('8e0b66aa-2be5-4fa3-b47f-1245474aa7dd', str(uuid.uuid4())))
            self.check(w, cli, repo, 'status', 'binding_mismatch')
            self.check(w, cli, repo, 'doctor', 'binding_mismatch')

    def test_index_head_custom_location_and_worktree_prove_known_missing_without_replacement(self):
        for case in ('indexed', 'staged-deletion', 'custom-worktree'):
            with self.subTest(case=case), World() as w:
                _, _, receipt = fixture(w)
                cli = Path(receipt['bin_dir']) / 'am'
                repo = w.repository()
                directory = bank(w, repo)
                bind(w, repo)
                if case == 'custom-worktree':
                    directory.rename(repo / 'project-notes')
                    w.git('config', 'agent-memory.bankRoot', 'project-notes', cwd=repo)
                w.git('add', '.', cwd=repo)
                if case != 'indexed':
                    w.git('commit', '-m', 'Known fixture bank', cwd=repo)
                if case == 'staged-deletion':
                    w.git('rm', '-r', 'memory-bank', cwd=repo)
                elif case == 'custom-worktree':
                    other = w.inside('another session')
                    w.git('worktree', 'add', '-b', 'other-session', other, cwd=repo)
                    repo = other
                    (repo / 'project-notes/MEMORY.md').unlink()
                else:
                    shutil.rmtree(directory)
                for op in ('status', 'doctor'):
                    value = self.check(w, cli, repo, op, 'missing_known_bank')
                    self.assertIn('recognized-router-in-index-or-HEAD', value['setup']['evidence'])
                    self.assertIn('do not initialize a replacement', value['setup']['next_step'])

    def test_damaged_runtime_and_malformed_or_ignored_bank_evidence_remain_independent(self):
        with World() as w:
            _, _, receipt = fixture(w)
            cli = Path(receipt['bin_dir']) / 'am'
            repo = w.repository()
            directory = bank(w, repo)
            bind(w, repo)
            saved = receipt_path(w).read_bytes()
            receipt_path(w).write_text('{invalid receipt')
            value = self.check(w, cli, repo, 'doctor', 'ready', 1)
            self.assertTrue(any(c['component'] == 'runtime.receipt' for c in value['failures']))
            receipt_path(w).write_bytes(saved)
            router = directory / 'MEMORY.md'
            saved = router.read_text()
            router.write_text('malformed router\n')
            for op in ('status', 'doctor'):
                self.check(w, cli, repo, op, 'inspection_required', 1)
            router.write_text(saved)
            directory.rename(repo / 'hidden-notes')
            (repo / '.gitignore').write_text('hidden-notes/\n')
            self.check(w, cli, repo, 'status', 'setup_needed')
            self.check(w, cli, repo, 'doctor', 'inspection_required', 1)

    def test_busy_bank_and_pending_purge_never_advise_first_use(self):
        with World() as w:
            _, _, receipt = fixture(w)
            cli = Path(receipt['bin_dir']) / 'am'
            repo = w.repository()
            directory = bank(w, repo)
            bind(w, repo)
            marker = directory / '.agent-memory.lock'
            marker.write_text('unknown owner, preserve evidence\n')
            for op in ('status', 'doctor'):
                self.check(w, cli, repo, op, 'busy_or_interrupted')
            marker.unlink()  # Explicit test-fixture marker, not a recovered live lock.
            args = [sys.executable, '-I', ROOT / 'scripts/uninstall_runtime.py', 'purge-data',
                    '--repo', repo, '--path', directory]
            confirmation = json.loads(w.run([*args, '--check']).stdout)['preview']['confirmation']
            r = w.run([*args, '--confirm', confirmation], check=False,
                env={'AGENT_MEMORY_FAULT_STAGE': 'purge-after-detach', 'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(r.returncode, 86)
            for op in ('status', 'doctor'):
                self.check(w, cli, repo, op, 'purge_pending')


if __name__ == '__main__':
    unittest.main()
