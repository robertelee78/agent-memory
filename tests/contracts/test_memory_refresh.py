"""Actual public refresh preserves stale feature code, index and unpublished learning."""
import unittest
import json
import sys

from world import World
from contracts import test_retrieval as retrieval_helpers


from refresh_fixtures import RefreshFixture


class MemoryRefreshContract(RefreshFixture):
    def test_public_cli_rejects_relative_outputs_before_any_refresh_operation(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            before = self.state(w, repo, bank)
            for operation, flags in (
                ('inspect', ['--repo', str(repo)]),
                ('prepare', ['--repo', str(repo), '--input', 'unused.json']),
                ('apply', ['--input', 'unused.json']),
            ):
                with self.subTest(operation=operation):
                    response = w.run([sys.executable, '-I', self.pack/'scripts/installer.py',
                        'refresh', operation, '--v2', *flags, '--output', 'private-secret-output.json'],
                        cwd=repo, check=False)
                    result = json.loads(response.stdout)
                    self.assertEqual(response.returncode, 1)
                    self.assertEqual(result['failure_code'], 'private-output-path-invalid')
                    self.assertFalse(result['canonical_writes_performed'])
                    self.assertFalse(result['git_metadata_writes_performed'])
                    self.assertIn('absolute', result['repair'])
                    self.assertNotIn('private-secret-output.json', response.stdout + response.stderr)
                    self.assertFalse((repo/'private-secret-output.json').exists())
                    self.assertEqual(before, self.state(w, repo, bank))

    def test_second_clone_import_is_recalled_with_conditions_without_code_index_or_local_learning_loss(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            (bank/'activeContext.md').write_text('# Current focus\n\nUnpublished local learning must survive.\n')
            (peer/'code.py').write_text('VALUE = 900  # newer authority code remains outside this branch\n')
            w.git('add', 'code.py', cwd=peer)
            record, path = self.imported(w, peer, 'imported-lesson', '# Imported learning\n\nA relevant shared lesson with provenance.\n', component='code.py')
            before = self.state(w, repo, bank); unrelated = (repo/'unrelated.txt').read_bytes()
            recall = retrieval_helpers.RetrievalContract().request(record['project-id'], ids=[record['id']], query='', context={'component':'code.py'})
            proposal = self.prepared_refresh(w, repo, recall=recall)
            self.assertEqual(before, self.state(w, repo, bank))
            done = self.apply_refresh(w, proposal)
            self.assertEqual(done['sharing_state'], 'refreshed'); self.assertTrue(done['refresh_applied'])
            self.assertTrue(done['remote_freshness_verified']); self.assertTrue(done['relevant_imports_read'])
            self.assertFalse(done['feature_code_freshness_verified']); self.assertIn('Unpublished local', (bank/'activeContext.md').read_text())
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:]); self.assertEqual(unrelated, (repo/'unrelated.txt').read_bytes())
            self.assertEqual(done['recall']['results'][0]['record'], record); self.assertEqual(done['recalled_import_paths'], [path])
            self.assertTrue(done['cursor']['stored']); self.assertTrue(done['cursor']['content_free'])
            repeated = self.apply_refresh(w, proposal)
            self.assertTrue(repeated['receipt']['already_applied']); self.assertFalse(repeated['canonical_writes_performed'])
            self.assertEqual(done['recall']['results'][0]['body'], repeated['recall']['results'][0]['body'])

    def test_cursor_advances_memory_without_feature_ancestry_and_old_retry_cannot_reverse_newer_import(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            first, path = self.imported(w, peer, 'first', '# First shared lesson\n\nPreserve its exact source.\n')
            proposal = self.prepared_refresh(w, repo); one = self.apply_refresh(w, proposal)
            before_code = self.state(w, repo, bank)[1:]
            second, other = self.imported(w, peer, 'second', '# Second shared lesson\n\nA later independent observation.\n')
            view = self.good(w, 'refresh_memory.py', 'inspect', '--repo', repo)
            self.assertEqual(view['baseline_source'], 'cursor')
            self.assertEqual(view['inspection']['binding']['base'], one['observed_authority_head'])
            two = self.apply_refresh(w, self.prepared_refresh(w, repo)); self.assertTrue(two['remote_freshness_verified'])
            before = self.state(w, repo, bank)
            old = self.apply_refresh(w, proposal)
            self.assertEqual(old['sharing_state'], 'historical-or-changed-local'); self.assertFalse(old['canonical_writes_performed'])
            self.assertFalse(old['relevant_imports_read']); self.assertEqual(before, self.state(w, repo, bank))
            self.assertEqual(before_code, before[1:]); self.assertTrue((bank/path).exists()); self.assertTrue((bank/other).exists())
