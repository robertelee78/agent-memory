"""Actual public refresh preserves stale feature code, index and unpublished learning."""
import json
from pathlib import Path
import sys
import unittest

from world import World
from publication_fixtures import PublicationFixture
from contracts.test_semantic_sharing import SemanticSharingContract
from contracts.test_retrieval import RetrievalContract


class RefreshFixture(PublicationFixture):
    def fixture(self, w):
        repo, bank, peer, base = super().fixture(w)
        w.git('checkout', '-b', 'feature-session', cwd=repo)
        w.git('commit', '-m', 'Unfinished local feature ancestor', cwd=repo)
        (repo/'code.py').write_text('VALUE = 3  # additional staged feature work\n'); w.git('add', 'code.py', cwd=repo)
        (repo/'unrelated.txt').write_text('Unstaged independent local work\n')
        return repo, bank, peer, base

    def imported(self, w, peer, name, body, **conditions):
        metadata, path = SemanticSharingContract().add(peer/'memory-bank', name, body,
                                                      conditions=conditions or None)
        w.git('add', 'memory-bank', cwd=peer); w.git('commit', '-m', 'Reviewed shared fixture learning', cwd=peer)
        w.git('push', 'origin', 'HEAD:main', cwd=peer)
        return metadata, path

    def prepared_refresh(self, w, repo, *, recall=None, base=None, decide=None, dependencies=None):
        args = ['inspect', '--repo', repo, *(['--base', base] if base else [])]
        view = self.good(w, 'refresh_memory.py', *args)
        judgment = self.judgment(view['inspection'], decide=decide)
        if dependencies is not None:
            judgment['dependencies'] = dependencies
        reviewed = self.good(w, 'reconcile_memory.py', 'review', '--repo', repo,
                             '--input', self.save(w, judgment))
        request = {'proof': reviewed['proof'], 'recall': recall or RetrievalContract().request(
            view['inspection']['binding']['project_id'], query='', include_context=True)}
        self.last_preparation = self.good(w, 'refresh_memory.py', 'prepare', '--repo', repo,
                                         '--input', self.save(w, request))
        return self.last_preparation['proposal']

    def apply_refresh(self, w, proposal):
        return self.good(w, 'refresh_memory.py', 'apply', '--input', self.save(w, proposal, 'refresh-proposal.json'))

