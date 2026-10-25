"""Installed initialization separates inspected bytes from semantic conditions."""
import hashlib
import json
from pathlib import Path
import unittest

from world import World
import installed_cli_fixtures as installed
from contracts import test_initialization as initialization
from contracts import test_retrieval as retrieval


class ObservationApplicability(unittest.TestCase):
    def helper(self, cli):
        h = initialization.InitializationContract()
        h.command = lambda world, name, *args, **kw: installed.invoke(world, cli, name, *args, **kw)
        return h

    def claim(self, h, value=3):
        claim = h.claim('implementation', value, 'src/app.py', context='techContext.md')
        claim['conditions'] = {'component': 'src'}
        claim['applicability_review'] = 'The inspected retry constant belongs to this component, independently of the memory-bootstrap checkout; qualify the source bytes before use.'
        return claim

    def provenance(self, body):
        line = next(line for line in body.splitlines() if line.startswith('Observation provenance: `'))
        return json.loads(line.removeprefix('Observation provenance: `').removesuffix('`.'))

    def test_unchanged_evidence_survives_memory_only_head_and_capture_branch_changes_without_duplication(self):
        with World() as w:
            cli = installed.install(w)
            h = self.helper(cli)
            repo, project, _ = h.setup(w)
            w.git('add', '.', cwd=repo)
            w.git('commit', '-m', 'Reviewed actual source fixture', cwd=repo)
            w.git('checkout', '-b', 'bootstrap-memory', cwd=repo)
            original_head = w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip()
            claim = self.claim(h)
            preview = h.preview(w, repo, h.review(h.inventory(w, repo), [claim]))
            self.assertTrue(h.apply(w, preview)[1]['verified'])
            bank = repo / 'memory-bank'
            before = h.files(bank)
            body = before['techContext.md'].decode()
            provenance = self.provenance(body)
            self.assertEqual(provenance['sources']['src/app.py'], {
                'sha256': hashlib.sha256((repo / 'src/app.py').read_bytes()).hexdigest(),
                'bytes': (repo / 'src/app.py').stat().st_size})
            self.assertEqual(provenance['observed_checkout'], {'branch': 'bootstrap-memory', 'head': original_head})
            self.assertTrue(provenance['checkout_context_only'])
            self.assertFalse(provenance['publication_verified'])
            applicability = next(line for line in body.splitlines() if line.startswith('Applicability:'))
            self.assertIn('"component": "src"', applicability)
            self.assertNotIn('branch', applicability)
            self.assertNotIn('revision', applicability)
            self.assertIn('Applicability review:', body)
            w.git('add', 'memory-bank', cwd=repo)
            w.git('commit', '-m', 'Memory-only capture', cwd=repo)
            changed = w.git('diff-tree', '--no-commit-id', '--name-only', '-r', 'HEAD', cwd=repo).stdout.splitlines()
            self.assertTrue(changed and all(path.startswith('memory-bank/') for path in changed))
            self.assertNotEqual(original_head, w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip())
            w.git('checkout', '-b', 'another-session', cwd=repo)
            request = retrieval.RetrievalContract().request(project, query='', paths=['techContext.md'],
                kinds=[], include_context=True, context={'component': 'src'})
            r, recalled = installed.command(w, cli, 'get', '--repo', repo, '--request', h.save(w, request))
            self.assertEqual(r.returncode, 0, recalled)
            row = recalled['results'][0]
            self.assertEqual(row['selection_class'], 'orientation-only')
            self.assertIsNone(row['trust'])
            self.assertFalse(row['source_assertions_verified'])
            self.assertEqual(row['body'], body)
            duplicate = dict(claim, context='progress.md')
            again = h.preview(w, repo, h.review(h.inventory(w, repo), [duplicate]))
            self.assertEqual(again['changed_memory_paths'], [])
            self.assertFalse(any('Existing implementation observation' in gap for gap in again['evidence_gaps']))
            self.assertEqual(h.files(bank), before)

    def test_changed_source_refuses_stale_write_and_existing_changed_hash_requires_review_without_duplicate(self):
        with World() as w:
            cli = installed.install(w)
            h = self.helper(cli)
            repo, _, _ = h.setup(w)
            claim = self.claim(h)
            p = h.preview(w, repo, h.review(h.inventory(w, repo), [claim]))
            source = repo / 'src/app.py'
            source.write_text('RETRY_LIMIT = 4\n')
            self.assertNotEqual(h.apply(w, p)[0].returncode, 0)
            self.assertFalse((repo / 'memory-bank').exists())
            claim = self.claim(h, 4)
            self.assertTrue(h.apply(w, h.preview(w, repo, h.review(h.inventory(w, repo), [claim])))[1]['verified'])
            before = h.files(repo / 'memory-bank')
            source.write_text('RETRY_LIMIT = 4\n# Additional implementation evidence to inspect.\n')
            duplicate = dict(claim, context='progress.md')
            p = h.preview(w, repo, h.review(h.inventory(w, repo), [duplicate]))
            self.assertEqual(p['changed_memory_paths'], [])
            self.assertTrue(any('inspected source bytes differ' in gap for gap in p['evidence_gaps']))
            self.assertEqual(h.files(repo / 'memory-bank'), before)
            self.assertNotIn('progress.md', p['proposal']['transaction']['files'])

    def test_legacy_inputs_and_real_branch_conditions_stay_conservative_and_unknown_old_hashes_remain_qualified(self):
        with World() as w:
            cli = installed.install(w)
            h = self.helper(cli)
            repo, _, _ = h.setup(w)
            claim = h.claim('implementation', 3, 'src/app.py', context='techContext.md')
            p = h.preview(w, repo, h.review(h.inventory(w, repo), [claim]))
            self.assertTrue(h.apply(w, p)[1]['verified'])
            bank = repo / 'memory-bank'
            tech = bank / 'techContext.md'
            self.assertIn('"branch": "main"', tech.read_text())
            # A valid pre-provenance prose shape is still readable but cannot
            # acquire invented historical byte binding from a later inspection.
            tech.write_text('\n'.join(line for line in tech.read_text().splitlines()
                if not line.startswith('Observation provenance:')) + '\n')
            before = h.files(bank)
            duplicate = dict(claim, context='progress.md')
            p = h.preview(w, repo, h.review(h.inventory(w, repo), [duplicate]))
            self.assertEqual(p['changed_memory_paths'], [])
            self.assertTrue(any('recorded byte binding is unknown' in gap for gap in p['evidence_gaps']))
            self.assertEqual(h.files(bank), before)
            w.git('checkout', '-b', 'other-branch', cwd=repo)
            claim['applicability_review'] = 'This reason does not override an explicit main-branch condition.'
            r, _ = h.command(w, 'initialize', 'preview', '--repo', repo,
                '--review', h.save(w, h.review(h.inventory(w, repo), [claim])))
            self.assertNotEqual(r.returncode, 0)
            claim.pop('applicability_review')
            claim['conditions'] = {}
            r, _ = h.command(w, 'initialize', 'preview', '--repo', repo,
                '--review', h.save(w, h.review(h.inventory(w, repo), [claim])))
            self.assertNotEqual(r.returncode, 0)
            self.assertEqual(h.files(bank), before)


if __name__ == '__main__':
    unittest.main()
