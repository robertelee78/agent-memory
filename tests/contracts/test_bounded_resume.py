"""Representative file-only resumes disclose the selected task's current files."""
import json
from pathlib import Path
import shutil
import unittest

from world import World
from contracts import test_retrieval as fixtures


class BoundedResumeContract(unittest.TestCase):
    command = fixtures.RetrievalContract.command
    good = fixtures.RetrievalContract.good
    save = fixtures.RetrievalContract.save
    fixture = fixtures.RetrievalContract.fixture
    add = fixtures.RetrievalContract.add

    def request(self, project, paths, context=None):
        return {'project_id': project, 'query': '', 'ids': [], 'paths': paths,
            'context': context or {}, 'kinds': ['semantic-fact', 'project-decision'],
            'minimum_trust': 'reviewed', 'limit': 8, 'include_context': True}

    def test_router_then_exact_task_context_omits_unrelated_records_without_optional_stores(self):
        with World() as w:
            repo, bank, project = self.fixture(w)
            chosen, path = self.add(bank, project, 'api', scope='component:src/api',
                conditions={'task': 'issue-42'}, body='# Retry limit\n\nKeep three attempts for issue-42 in the API.\n')
            self.add(bank, project, 'worker', scope='component:src/worker',
                body='# Worker\n\nUNRELATED_WORKER_CONTEXT_MUST_NOT_LOAD.\n')
            self.add(bank, project, 'episodic-detail', kind='episodic-attempt',
                body='# Old probe\n\nUNREQUESTED_PROBE_MUST_NOT_LOAD.\n')
            before = ((repo / 'app.py').read_bytes(), (repo / '.git/index').read_bytes(), w.git('rev-parse', 'HEAD', cwd=repo).stdout)
            router = (bank / 'MEMORY.md').read_bytes();self.assertLessEqual(len(router), 16384)
            self.assertIn(b'Before qualified recall', router)
            for name in ('cache', 'data'):
                shutil.rmtree(w.paths[name])
            request = self.request(project, ['activeContext.md', path], {'component': 'src/api', 'task': 'issue-42'})
            value = self.good(w, 'retrieve.py', '--repo', repo, '--request', self.save(w, request))
            self.assertEqual({row['path'] for row in value['results']}, {'activeContext.md', path})
            text = json.dumps(value);self.assertNotIn('UNRELATED_WORKER_CONTEXT_MUST_NOT_LOAD', text)
            self.assertNotIn('UNREQUESTED_PROBE_MUST_NOT_LOAD', text)
            self.assertEqual(before, ((repo / 'app.py').read_bytes(), (repo / '.git/index').read_bytes(), w.git('rev-parse', 'HEAD', cwd=repo).stdout))

    def test_exact_resume_preserves_branch_unknown_validity_trust_and_kind_gates(self):
        with World() as w:
            repo, bank, project = self.fixture(w);paths = []
            for name, values in [('current', {}), ('other-branch', {'conditions': {'branch': 'feature/not-here'}}),
                    ('unknown-task', {'conditions': {'task': 'not-observed'}}), ('unknown-applicability', {'applicability': 'unknown'}),
                    ('expired', {'valid-until': '2020-01-01T00:00:00Z'}), ('untrusted', {'trust': 'untrusted'}),
                    ('other-kind', {'kind': 'episodic-attempt'})]:
                _, path = self.add(bank, project, name, body='# Scoped lesson\n\nResume only under its actual conditions.\n', **values);paths.append(path)
            request = self.request(project, paths)
            value = self.good(w, 'retrieve.py', '--repo', repo, '--request', self.save(w, request))
            self.assertEqual([row['path'] for row in value['results']], [paths[0]])
            self.assertNotIn('main_freshness_verified', value);self.assertFalse(value['publication_verified'])

    def test_ambiguous_task_keeps_only_router_and_prose_never_gains_typed_trust(self):
        with World() as w:
            repo, bank, project = self.fixture(w);self.add(bank, project, 'task-specific', conditions={'task': 'other-session-task'})
            router = (bank / 'MEMORY.md').read_bytes();self.assertLessEqual(len(router), 16384)
            # A selected empty typed set is an explicit no-details request;
            # it is not an empty-query request for the whole qualified bank.
            request = self.request(project, []);request.update(kinds=[], include_context=False)
            empty = self.good(w, 'retrieve.py', '--repo', repo, '--request', self.save(w, request))
            self.assertEqual(empty['results'], [])
            request = self.request(project, ['activeContext.md'])
            result = self.good(w, 'retrieve.py', '--repo', repo, '--request', self.save(w, request))
            self.assertEqual(len(result['results']), 1);row = result['results'][0]
            self.assertEqual(row['path'], 'activeContext.md');self.assertEqual(row['trust'], None)
            self.assertEqual(row['selection_class'], 'orientation-only')
