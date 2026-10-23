"""Memory-only publication through isolated public commands and real Git remotes."""
from datetime import datetime, timezone
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World
from identity_fixtures import bind
from contracts import test_semantic_sharing as semantic
from contracts import test_sharing_recovery as recovery
import publication_github

ROOT = Path(__file__).resolve().parents[2]


from publication_fixtures import PublicationFixture


class MemoryPublicationContract(PublicationFixture):
    def test_first_publication_excludes_feature_ancestry_and_preserves_original_files_index(self):
        with World() as w:
            repo, bank, base = self.bootstrap(w)
            self.github(w, repo, bank)
            before = self.state(w, repo, bank); unrelated = (repo/'unrelated.txt').read_bytes()
            prepared = self.prepared(w, repo); self.assertEqual(prepared['sharing_state'], 'prepared')
            request = prepared['request']
            parents = w.git('rev-list', '--parents', '-n', '1', request['candidate'], cwd=repo).stdout.split()
            self.assertEqual(parents, [request['candidate'], base])
            self.assertEqual(w.git('show', request['candidate']+':code.py', cwd=repo).stdout, 'VALUE = 1\n')
            landed = self.land(w, repo, request)
            self.assertTrue(landed['publication_verified']); self.assertEqual(landed['sharing_state'], 'landed')
            self.assertEqual(w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout.split()[0], request['candidate'])
            self.assertEqual(before, self.state(w, repo, bank)); self.assertEqual(unrelated, (repo/'unrelated.txt').read_bytes())
            again = self.land(w, repo, request)
            self.assertEqual(again['sharing_state'], 'already-landed'); self.assertEqual(before, self.state(w, repo, bank))

    def test_required_human_review_and_checks_remain_pending_even_for_an_admin(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            policy = self.github(w, repo, bank, peer, protected=True)
            (bank/'activeContext.md').write_text('# Current focus\n\nKeep independently useful learning conditional on unfinished work.\n')
            before = self.state(w, repo, bank); head = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            prepared = self.prepared(w, repo); request = prepared['request']
            pending = self.land(w, repo, request)
            self.assertFalse(pending['publication_verified']); self.assertEqual(pending['sharing_state'], 'pending-repository-rules')
            self.assertIn('review', pending['reason'].lower()); self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
            d = json.loads(policy.read_text()); self.assertEqual(len(d['pulls']), 1)
            d.update(review='APPROVED', checks='PENDING'); policy.write_text(json.dumps(d))
            checks = self.land(w, repo, request); self.assertFalse(checks['publication_verified'])
            self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
            d = json.loads(policy.read_text()); d['checks'] = 'SUCCESS'; policy.write_text(json.dumps(d))
            landed = self.land(w, repo, request); self.assertTrue(landed['publication_verified'])
            actual = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout.split()[0]
            self.assertEqual(landed['witnessed_commit'], actual)
            self.assertEqual(w.git('rev-list', '--parents', '-n', '1', actual, cwd=repo).stdout.split()[1:], [request['base'], request['candidate']])
            self.assertEqual(before, self.state(w, repo, bank))

    def test_first_bank_compares_real_absence_without_a_second_starter(self):
        with World() as w:
            repo, bank, base = self.bootstrap(w); before = self.state(w, repo, bank)
            proof = self.proof(w, repo)
            self.assertEqual(proof['judgment']['binding']['base'], base)
            self.assertFalse(proof['judgment']['binding']['shared_bank_present'])
            self.assertEqual(proof['judgment']['binding']['source_hashes']['base'], {})
            self.assertEqual(proof['judgment']['binding']['source_hashes']['landed'], {})
            checked = self.good(w, 'reconcile_memory.py', 'check', '--repo', repo,
                                '--input', self.save(w, proof))
            self.assertEqual(checked['publish_files'], checked['refresh_files'])
            self.assertEqual(set(checked['publish_files']), {p.relative_to(bank).as_posix() for p in bank.rglob('*.md')})
            self.assertEqual(before, self.state(w, repo, bank))

    def test_unstaged_canonical_deletion_needs_real_prior_ownership_not_an_index_edit(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            (bank/'activeContext.md').unlink()
            router = bank/'MEMORY.md'
            text = ''.join(line for line in router.read_text().splitlines(keepends=True)
                           if not line.startswith('| [activeContext.md]'))
            text = text.replace('## Omitted contexts\n\n', '## Omitted contexts\n\n- `activeContext.md`: Finished session; no active focus remains.\n')
            router.write_text(text)
            before = self.state(w, repo, bank)
            boundary = self.good(w, 'sharing_boundary.py', '--repo', repo)
            self.assertTrue(boundary['local_boundary_ready'])
            self.assertEqual(boundary['proven_deleted_memory_paths'], ['memory-bank/activeContext.md'])
            self.proof(w, repo)
            self.assertEqual(before, self.state(w, repo, bank))
            # A missing tracked artifact with no canonical ownership is still excluded.
            (bank/'cache.json').write_text('{}\n'); w.git('add', 'memory-bank/cache.json', cwd=repo)
            (bank/'cache.json').unlink(); index = (repo/'.git/index').read_bytes()
            blocked = self.good(w, 'sharing_boundary.py', '--repo', repo)
            self.assertIn('excluded-bank-artifacts-are-tracked', blocked['blockers'])
            self.assertEqual(index, (repo/'.git/index').read_bytes())
