"""Real linked worktrees, disposable cursor loss and later ordinary code integration."""
import json
from pathlib import Path
import unittest

from world import World
from refresh_fixtures import RefreshFixture


class RefreshIntegrationContract(RefreshFixture):
    def test_linked_worktree_import_does_not_touch_sibling_bank_index_or_feature_heads(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            sibling = w.inside('other session worktree'); w.git('worktree','add','-b','second-session',str(sibling),base,cwd=repo)
            other_bank = sibling/'memory-bank'
            (sibling/'code.py').write_text('VALUE = 77  # staged sibling work\n'); w.git('add','code.py',cwd=sibling)
            (other_bank/'activeContext.md').write_text('# Current focus\n\nIndependent unpublished sibling learning.\n')
            record, path = self.imported(w, peer, 'landed', '# Landed\n\nA shared observation for separate sessions.\n')
            before = self.state(w, sibling, other_bank); original = self.state(w, repo, bank)
            done = self.apply_refresh(w, self.prepared_refresh(w, sibling))
            self.assertTrue(done['remote_freshness_verified']); self.assertTrue((other_bank/path).exists())
            self.assertEqual(before[1:], self.state(w, sibling, other_bank)[1:]); self.assertEqual(original, self.state(w, repo, bank))
            self.assertIn('Independent unpublished sibling', (other_bank/'activeContext.md').read_text())

    def test_cursor_loss_recovers_from_git_and_later_normal_code_merge_does_not_duplicate_memory(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            (peer/'code.py').write_text('VALUE = 900  # authority code awaits normal integration\n'); w.git('add','code.py',cwd=peer)
            record, path = self.imported(w, peer, 'landed', '# Landed\n\nOne stable observation survives normal merges.\n')
            first = self.apply_refresh(w, self.prepared_refresh(w, repo)); self.assertTrue(first['cursor']['stored'])
            cursor = next(Path(w.env['AGENT_MEMORY_STATE_HOME']).rglob('refresh-cursor.json')); cursor.unlink()
            inspection = self.good(w, 'refresh_memory.py', 'inspect', '--repo', repo)
            self.assertEqual(inspection['baseline_source'], 'git-common-base')
            again = self.apply_refresh(w, self.prepared_refresh(w, repo)); self.assertTrue(again['remote_freshness_verified'])
            self.assertEqual(len(list((bank/'topics').glob('*.md'))),1)
            # This is the session's separate normal code integration, after the
            # refresh preservation assertions; the refresh operation never does it.
            w.git('add','code.py','memory-bank',cwd=repo); w.git('commit','-m','Normal session checkpoint before code integration',cwd=repo)
            w.git('fetch','origin',cwd=repo)
            merged = w.git('merge','--no-ff','origin/main','-m','Normal code merge after memory import',cwd=repo,check=False)
            if merged.returncode:
                self.assertEqual(merged.returncode,1)
                self.assertEqual(w.git('diff','--name-only','--diff-filter=U',cwd=repo).stdout.strip(),'code.py')
                (repo/'code.py').write_text('VALUE = 900  # explicit normal code resolution\n')
                w.git('add','code.py',cwd=repo); w.git('commit','-m','Resolve only the normal feature-code conflict',cwd=repo)
            self.good(w,'bank_contract.py','validate','--repo',repo)
            before = self.state(w,repo,bank)
            refreshed = self.apply_refresh(w,self.prepared_refresh(w,repo))
            self.assertTrue(refreshed['remote_freshness_verified']); self.assertFalse(refreshed['feature_code_freshness_verified'])
            self.assertEqual(len(list((bank/'topics').glob('*.md'))),1)
            self.assertEqual(before[1:],self.state(w,repo,bank)[1:]); self.assertIn('VALUE = 900',(repo/'code.py').read_text())
