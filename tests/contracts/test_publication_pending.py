"""Pending exchange preserves local learning and refuses feature-code candidates."""
import json
import shutil
from pathlib import Path
import sys
import unittest
import uuid

from world import World
from publication_fixtures import PublicationFixture


class PublicationPendingContract(PublicationFixture):
    def test_peer_bank_at_another_root_and_its_later_removal_do_not_authorize_a_second_bank(self):
        with World() as w:
            repo, bank, base = self.bootstrap(w); policy = self.github(w, repo, bank)
            proof = self.proof(w, repo); before = self.state(w, repo, bank)
            remote = Path(json.loads(policy.read_text())['remote']); peer = w.clone(remote, 'other-bank-peer')
            other = peer/'docs/shared memory'; other.mkdir(parents=True)
            for path in bank.rglob('*.md'):
                target = other/path.relative_to(bank); target.parent.mkdir(parents=True, exist_ok=True)
                text = path.read_text()
                if path.name == 'MEMORY.md':
                    text = ''.join('project-id: '+json.dumps(str(uuid.uuid4()))+'\n' if line.startswith('project-id: ') else line
                                   for line in text.splitlines(keepends=True))
                target.write_text(text)
            w.git('add', 'docs/shared memory', cwd=peer); w.git('commit', '-m', 'Peer explicitly initialized shared custom bank', cwd=peer)
            w.git('push', 'origin', 'HEAD:main', cwd=peer)
            for removed in (False, True):
                with self.subTest(removed=removed):
                    if removed:
                        w.git('rm', '-r', 'docs/shared memory', cwd=peer)
                        w.git('commit', '-m', 'Retired custom bank remains historical ownership', cwd=peer)
                        w.git('push', 'origin', 'HEAD:main', cwd=peer)
                    p, value = self.call(w, 'publish_memory.py', 'prepare', '--repo', repo, '--input', self.save(w, proof))
                    self.assertEqual(p.returncode, 1); self.assertNotIn('request', value)
                    self.assertFalse(value['publication_verified']); self.assertEqual(before, self.state(w, repo, bank))

    def test_discarded_side_bank_is_not_invented_as_historical_authority_ownership(self):
        with World() as w:
            repo, bank, base = self.bootstrap(w); policy = self.github(w, repo, bank)
            remote = Path(json.loads(policy.read_text())['remote']); peer = w.clone(remote, 'discarded-bank-peer')
            w.git('checkout', '-b', 'unlanded-bank', cwd=peer)
            other = peer/'docs/shared memory'; shutil.copytree(bank, other)
            w.git('add', 'docs/shared memory', cwd=peer); w.git('commit', '-m', 'Proposed bank only on side branch', cwd=peer)
            w.git('checkout', 'main', cwd=peer)
            w.git('merge', '--no-ff', '--strategy=ours', 'unlanded-bank', '-m', 'Discard proposed bank from actual authority', cwd=peer)
            w.git('push', 'origin', 'HEAD:main', cwd=peer)
            before = self.state(w, repo, bank); prepared = self.prepared(w, repo)
            self.assertEqual(prepared['sharing_state'], 'prepared')
            self.assertEqual(before, self.state(w, repo, bank))

    def test_offline_permission_source_and_unsupported_policy_failures_keep_local_memory_pending(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w); policy = self.github(w, repo, bank, peer)
            (bank/'activeContext.md').write_text('# Current focus\n\nRetain this admitted local learning while offline.\n')
            request = self.prepared(w, repo)['request']; before = self.state(w, repo, bank)
            d = json.loads(policy.read_text()); remote = Path(d['remote']); away = remote.with_name('offline.git')
            head = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            remote.rename(away)
            p, value = self.call(w, 'publish_memory.py', 'land', '--repo', repo, '--input', self.save(w, request))
            self.assertEqual(p.returncode, 1); self.assertFalse(value['publication_verified']); self.assertTrue(value['local_memory_available'])
            away.rename(remote); self.assertEqual(before, self.state(w, repo, bank))
            d = json.loads(policy.read_text()); d['push'] = False; policy.write_text(json.dumps(d))
            p, value = self.call(w, 'publish_memory.py', 'land', '--repo', repo, '--input', self.save(w, request))
            self.assertEqual(p.returncode, 1); self.assertFalse(value['publication_verified']); self.assertEqual(before, self.state(w, repo, bank))
            d = json.loads(policy.read_text()); d['push'] = True; policy.write_text(json.dumps(d))
            (bank/'activeContext.md').write_text('# Current focus\n\nNew source evidence requires a fresh semantic review.\n')
            pending = self.land(w, repo, request); self.assertEqual(pending['sharing_state'], 'pending-reconciliation')
            d = json.loads(policy.read_text()); d['rules'] = [{'type':'required_deployments'}]; policy.write_text(json.dumps(d))
            unsupported = self.prepared(w, repo)
            self.assertEqual(unsupported['sharing_state'], 'pending-policy'); self.assertFalse(unsupported['publication_verified'])
            self.assertEqual(head, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
            self.assertIn('New source evidence', (bank/'activeContext.md').read_text())
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])

    def test_removed_authority_bank_is_not_treated_as_first_initialization(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w); self.github(w, repo, bank, peer)
            before = self.state(w, repo, bank)
            w.git('rm', '-r', 'memory-bank', cwd=peer); w.git('commit', '-m', 'Authority bank removal requires inspection', cwd=peer)
            w.git('push', 'origin', 'HEAD:main', cwd=peer)
            p, value = self.call(w, 'reconcile_memory.py', 'inspect', '--repo', repo)
            self.assertEqual(p.returncode, 1); self.assertFalse(value['publication_verified'])
            self.assertEqual(before, self.state(w, repo, bank))

    def test_tampered_candidate_with_identical_memory_but_feature_code_is_refused(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w); self.github(w, repo, bank, peer)
            (bank/'activeContext.md').write_text('# Current focus\n\nPublish this memory but never feature code.\n')
            request = self.prepared(w, repo)['request']; before = self.state(w, repo, bank)
            private_index = w.inside('attacker-fixture-index'); code = w.inside('unrelated-feature-content')
            code.write_text('VALUE = 12345  # unrelated feature code must not land\n')
            w.env['GIT_INDEX_FILE'] = str(private_index)
            try:
                w.git('read-tree', request['candidate'], cwd=repo)
                blob = w.git('hash-object', '-w', str(code), cwd=repo).stdout.strip()
                w.git('update-index', '--add', '--cacheinfo', '100644,'+blob+',code.py', cwd=repo)
                tree = w.git('write-tree', cwd=repo).stdout.strip()
                invalid = w.git('commit-tree', tree, '-p', request['base'], '-m', 'Same memory with unrelated feature code', cwd=repo).stdout.strip()
            finally:
                w.env.pop('GIT_INDEX_FILE')
            request['candidate'] = invalid
            p, value = self.call(w, 'publish_memory.py', 'land', '--repo', repo, '--input', self.save(w, request))
            self.assertEqual(p.returncode, 1); self.assertFalse(value['publication_verified'])
            self.assertEqual(before, self.state(w, repo, bank))
            self.assertEqual(w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout.split()[0], request['base'])
