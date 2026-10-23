"""Actual local/source/branch/offline changes refuse stale import without losing edits."""
import copy
import json
from pathlib import Path
import unittest
import uuid

from world import World
from refresh_fixtures import RefreshFixture


class RefreshPendingContract(RefreshFixture):
    def test_late_local_edit_and_same_commit_branch_change_refuse_the_original_proposal(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            self.imported(w, peer, 'incoming', '# Incoming\n\nReview actual incoming learning.\n')
            proposal = self.prepared_refresh(w, repo)
            (bank/'activeContext.md').write_text('# Current focus\n\nA late unpublished edit must not be lost.\n')
            before = self.state(w, repo, bank)
            p, value = self.call(w, 'refresh_memory.py', 'apply', '--input', self.save(w, proposal))
            self.assertEqual(p.returncode, 1); self.assertEqual(before, self.state(w, repo, bank))
            fresh = self.prepared_refresh(w, repo)
            w.git('checkout', '-b', 'different-session-branch', cwd=repo); changed = self.state(w, repo, bank)
            p, value = self.call(w, 'refresh_memory.py', 'apply', '--input', self.save(w, fresh))
            self.assertEqual(p.returncode, 1); self.assertEqual(changed, self.state(w, repo, bank))
            self.assertFalse((bank/'.agent-memory-transaction.md').exists())

    def test_new_authority_and_offline_observation_require_fresh_review_before_import(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            first, path = self.imported(w, peer, 'first', '# First\n\nInitial shared evidence.\n')
            proposal = self.prepared_refresh(w, repo); before = self.state(w, repo, bank)
            remote = Path(w.git('remote', 'get-url', 'origin', cwd=repo).stdout.strip()); away = remote.with_name('offline.git')
            remote.rename(away)
            p, value = self.call(w, 'refresh_memory.py', 'apply', '--input', self.save(w, proposal))
            self.assertEqual(p.returncode, 1); self.assertTrue(value['local_memory_available']); self.assertEqual(before, self.state(w, repo, bank))
            away.rename(remote)
            second, other = self.imported(w, peer, 'second', '# Second\n\nNewly landed evidence requires review too.\n')
            p, value = self.call(w, 'refresh_memory.py', 'apply', '--input', self.save(w, proposal))
            self.assertEqual(p.returncode, 1); self.assertEqual(before, self.state(w, repo, bank))
            done = self.apply_refresh(w, self.prepared_refresh(w, repo))
            self.assertTrue(done['remote_freshness_verified']); self.assertTrue((bank/path).exists()); self.assertTrue((bank/other).exists())
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])

    def test_changed_candidate_or_foreign_recall_project_cannot_bypass_exact_source_review(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            record, path = self.imported(w, peer, 'incoming', '# Incoming\n\nPreserve this actual evidence.\n')
            proposal = self.prepared_refresh(w, repo); before = self.state(w, repo, bank)
            changed = copy.deepcopy(proposal); changed['files'][path] += '\nUnreviewed extra claim.\n'
            other = copy.deepcopy(proposal); other['refresh_evidence']['recall']['project_id'] = str(uuid.uuid4())
            wrong_branch = copy.deepcopy(proposal); wrong_branch['refresh_evidence']['recall']['context']['branch'] = 'unobserved-branch'
            for value in (changed, other, wrong_branch):
                p, failed = self.call(w, 'refresh_memory.py', 'apply', '--input', self.save(w, value))
                self.assertEqual(p.returncode, 1); self.assertEqual(before, self.state(w, repo, bank))
            self.assertFalse((bank/'.agent-memory-transaction.md').exists())
