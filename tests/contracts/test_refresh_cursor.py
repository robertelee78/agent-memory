"""A disposable cursor from another checkout context cannot hide shared learning."""
import unittest

from world import World
from refresh_fixtures import RefreshFixture
from contracts import test_retrieval as retrieval_helpers


class RefreshCursorContract(RefreshFixture):
    def changed_checkout(self, w, *, switch_branch):
        repo, bank, peer, base = self.fixture(w)
        record, path = self.imported(w, peer, 'shared-rule', '# Shared rule\n\nRetain the stable retry identity.\n')
        recall = retrieval_helpers.RetrievalContract().request(record['project-id'], ids=[record['id']], query='')
        first = self.apply_refresh(w, self.prepared_refresh(w, repo, recall=recall))
        self.assertEqual(first['recall']['results'][0]['record']['id'], record['id'])
        # These are normal caller Git actions, separate from refresh itself.
        w.git('stash', 'push', '-u', '-m', 'Caller preserves imported memory', '--', 'memory-bank', cwd=repo)
        self.assertFalse((bank/path).exists())
        stash = w.git('rev-parse', 'refs/stash', cwd=repo).stdout
        if switch_branch:
            w.git('checkout', '-b', 'another-session-branch', cwd=repo)
        else:
            w.git('commit', '-m', 'Caller commits staged feature code after stashing memory', cwd=repo)
        (bank/'activeContext.md').write_text('# Current focus\n\nUnpublished learning in the current checkout must survive.\n')
        before = self.state(w, repo, bank)
        view = self.good(w, 'refresh_memory.py', 'inspect', '--repo', repo)
        self.assertEqual(view['baseline_source'], 'git-common-base')
        self.assertEqual(view['cursor_state'], 'checkout-context-changed')
        self.assertEqual(before, self.state(w, repo, bank))
        imported = self.apply_refresh(w, self.prepared_refresh(w, repo, recall=recall))
        self.assertTrue(imported['remote_freshness_verified'])
        self.assertEqual(imported['recall']['results'][0]['record']['id'], record['id'])
        self.assertTrue((bank/path).exists())
        self.assertIn('Unpublished learning in the current checkout', (bank/'activeContext.md').read_text())
        self.assertEqual(before[1:], self.state(w, repo, bank)[1:])
        self.assertEqual(stash, w.git('rev-parse', 'refs/stash', cwd=repo).stdout)
        current = self.good(w, 'refresh_memory.py', 'inspect', '--repo', repo)
        self.assertEqual(current['baseline_source'], 'cursor')
        self.assertEqual(current['cursor_state'], 'verified-local-hint')

    def test_stashed_import_and_same_head_branch_switch_reimports_the_shared_lesson(self):
        with World() as w:
            self.changed_checkout(w, switch_branch=True)

    def test_stashed_import_and_new_revision_on_same_branch_reimports_the_shared_lesson(self):
        with World() as w:
            self.changed_checkout(w, switch_branch=False)
