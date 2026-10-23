"""Real linked-worktree publication and exact sibling/index/tag preservation."""
import shutil
import unittest

from world import World
from publication_fixtures import PublicationFixture


class PublicationWorktreeContract(PublicationFixture):
    def test_linked_worktree_uses_a_private_authority_index_and_does_not_push_follow_tags(self):
        with World() as w:
            repo, bank, base = self.bootstrap(w); self.github(w, repo, bank)
            sibling = w.inside('separate worktree'); w.git('worktree', 'add', '-b', 'session-two', str(sibling), base, cwd=repo)
            other_bank = sibling/'memory-bank'; shutil.copytree(bank, other_bank)
            (sibling/'code.py').write_text('VALUE = 42  # staged sibling work\n'); w.git('add', 'code.py', cwd=sibling)
            w.git('config', '--local', 'push.followTags', 'true', cwd=sibling)
            w.git('tag', '-a', 'unrelated-local-tag', base, '-m', 'Must not publish an unrelated tag', cwd=sibling)
            before = self.state(w, sibling, other_bank); original = self.state(w, repo, bank)
            request = self.prepared(w, sibling)['request']; landed = self.land(w, sibling, request)
            self.assertTrue(landed['publication_verified'])
            self.assertEqual(w.git('ls-remote', '--tags', 'origin', cwd=sibling).stdout, '')
            self.assertEqual(before, self.state(w, sibling, other_bank)); self.assertEqual(original, self.state(w, repo, bank))
            self.assertEqual(w.git('show', request['candidate']+':code.py', cwd=sibling).stdout, 'VALUE = 1\n')
