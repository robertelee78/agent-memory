"""Copied portable artifact: file initialization, checkpoint and clean resume."""
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World

ROOT = Path(__file__).resolve().parents[2]
CONTEXTS = ('projectbrief.md', 'productContext.md', 'activeContext.md', 'systemPatterns.md', 'techContext.md', 'progress.md')


class OfflineSkillContract(unittest.TestCase):
    def pack(self, w):
        pack = w.inside('portable skill with spaces')
        shutil.copytree(ROOT / 'skills/agent-memory', pack)
        self.assertTrue((pack / 'SKILL.md').is_file())
        self.assertTrue((pack / 'offline/GUIDE.md').is_file())
        # No harness, local registry, cache, credentials or optional service.
        for name in ('config', 'state', 'cache', 'codex', 'claude', 'opencode', 'credentials'):
            shutil.rmtree(w.paths[name])
        return pack

    def helper(self, w, pack, name, *args):
        p = w.run([sys.executable, '-I', pack / 'scripts' / name, *args], check=False)
        return p, json.loads(p.stdout)

    def initialize(self, w, pack):
        repo = w.repository()
        (repo / 'README.md').write_text('# Retry service\n\nProject scope: retry only.\n')
        body = '# Active context\n\nFocus: retry only; [scope](../README.md).\n\nOpen: other components unverified.\n\nNext: inspect the first fixture.\n'
        proposal = {'project-id': str(uuid.uuid4()), 'router-updated': '2026-10-22T20:00:00Z',
                    'contexts': {'activeContext.md': {'body': body, 'purpose': 'Resume verified retry work',
                        'scope': 'component:retry', 'lifecycle': 'active', 'read-when': 'When resuming retry work'}},
                    'omitted': {name: 'No additional verified evidence supplied' for name in CONTEXTS if name != 'activeContext.md'}}
        source = w.inside('reviewed-router.json')
        source.write_text(json.dumps(proposal))
        p, rendered = self.helper(w, pack, 'bank_contract.py', 'render', '--input', source)
        self.assertEqual(p.returncode, 0, rendered)
        bank = repo / 'memory-bank'
        self.assertFalse(bank.exists())
        bank.mkdir()
        for name, content in rendered['files'].items():
            (bank / name).write_text(content)
        self.assertEqual(set(p.name for p in bank.iterdir()), {'MEMORY.md', 'activeContext.md'})
        p, checked = self.helper(w, pack, 'bank_contract.py', 'validate', '--repo', repo)
        self.assertEqual(p.returncode, 0, checked)
        self.assertEqual(checked['project_id'], proposal['project-id'])
        w.git('add', 'README.md', 'memory-bank', cwd=repo)
        w.git('commit', '-m', 'Useful manual bank', cwd=repo)
        return repo, bank, proposal

    def test_offline_manual_checkpoint_and_fresh_file_resume_preserve_project_and_unrelated_work(self):
        with World() as w:
            pack = self.pack(w)
            repo, bank, proposal = self.initialize(w, pack)
            p, inspected = self.helper(w, pack, 'bank_writes.py', 'inspect', '--repo', repo)
            self.assertEqual(p.returncode, 0, inspected)
            router = (bank / 'MEMORY.md').read_bytes()
            (repo / 'feature.py').write_text('unfinished = True\n')
            w.git('add', 'feature.py', cwd=repo)
            (repo / 'scratch.txt').write_text('Preserve unrelated work\n')
            index, head = (repo / '.git/index').read_bytes(), w.git('rev-parse', 'HEAD', cwd=repo).stdout
            target = bank / 'activeContext.md'
            self.assertFalse((bank / '.agent-memory.lock').exists())
            self.assertFalse((bank / '.agent-memory-transaction.md').exists())
            before = target.read_text()
            target.write_text(before.replace('inspect the first fixture', 'inspect the second fixture'))
            self.assertEqual((bank / 'MEMORY.md').read_bytes(), router)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(w.git('rev-parse', 'HEAD', cwd=repo).stdout, head)
            self.assertEqual(w.git('diff', '--name-only', cwd=repo).stdout.strip(), 'memory-bank/activeContext.md')
            self.assertEqual((repo / 'scratch.txt').read_text(), 'Preserve unrelated work\n')
            # Local work is still uncommitted; this is deliberately no sharing receipt.
            self.assertNotIn('second fixture', w.git('show', 'HEAD:memory-bank/activeContext.md', cwd=repo).stdout)
            w.git('add', 'memory-bank/activeContext.md', cwd=repo)
            w.git('commit', '--only', 'memory-bank/activeContext.md', '-m', 'Manual checkpoint', cwd=repo)
            clone = w.clone(repo, 'fresh session')
            resumed = w.run(['cat', clone / 'memory-bank/activeContext.md']).stdout
            self.assertIn('second fixture', resumed)
            self.assertIn('other components unverified', resumed)
            self.assertIn('[scope](../README.md)', resumed)
            p, checked = self.helper(w, pack, 'bank_contract.py', 'validate', '--repo', clone)
            self.assertEqual(p.returncode, 0, checked)
            self.assertEqual(checked['project_id'], proposal['project-id'])
            self.assertEqual((clone / 'memory-bank/MEMORY.md').read_bytes(), router)
            for name in ('config', 'state', 'cache', 'codex', 'claude', 'opencode', 'credentials'):
                self.assertFalse(w.paths[name].exists(), name)

    def test_generated_state_absence_never_bypasses_busy_or_interrupted_bank(self):
        with World() as w:
            pack = self.pack(w)
            repo, bank, _ = self.initialize(w, pack)
            before = {p.name: p.read_bytes() for p in bank.iterdir()}
            for name in ('.agent-memory.lock', '.agent-memory-transaction.md'):
                marker = bank / name
                marker.write_text('Preserve marker; explicit inspection required\n')
                for helper in ('bank_contract.py', 'bank_writes.py'):
                    command = 'validate' if helper == 'bank_contract.py' else 'inspect'
                    p, error = self.helper(w, pack, helper, command, '--repo', repo)
                    self.assertEqual(p.returncode, 1, error)
                    self.assertTrue(marker.exists())
                self.assertEqual({p.name: p.read_bytes() for p in bank.iterdir() if p.name != name}, before)
                marker.unlink()


if __name__ == '__main__':
    unittest.main()
