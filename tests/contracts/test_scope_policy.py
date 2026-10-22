"""Public scope decisions: same bank, explicit applicability, no personal ACL."""
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / 'scripts/scope_policy.py'
RENDER = ROOT / 'scripts/bank_contract.py'


class ScopePolicyContract(unittest.TestCase):
    def command(self, w, request, helper=HELPER):
        source = w.inside('request.json')
        source.write_text(json.dumps(request))
        p = w.run([sys.executable, '-I', helper, 'decide', '--input', source], check=False)
        return p, json.loads(p.stdout)

    def request(self):
        project = str(uuid.uuid4())
        return {'bank_project_id': project, 'project_id': project}

    def test_default_is_one_repository_bank_and_developer_does_not_select_a_namespace(self):
        with World() as w:
            request = self.request()
            results = []
            for developer in (None, str(uuid.uuid4()), str(uuid.uuid4())):
                value = dict(request)
                if developer is not None:
                    value['developer_id'] = developer
                p, result = self.command(w, value)
                self.assertEqual(p.returncode, 0, result)
                self.assertTrue(result['applicable'])
                self.assertEqual(result['scope'], 'repository')
                self.assertFalse(result['identity_authenticated'])
                self.assertFalse(result['writes_performed'])
                results.append(result)
            self.assertEqual(results[0], results[1])
            self.assertEqual(results[1], results[2])
            for developer in (str(uuid.uuid4()), str(uuid.uuid4())):
                p, result = self.command(w, {**request, 'project_id': str(uuid.uuid4()), 'developer_id': developer})
                self.assertEqual(p.returncode, 0, result)
                self.assertFalse(result['same_project'])
                self.assertFalse(result['applicable'])
                self.assertIn('different-project', result['reasons'])
            self.assertFalse(Path(w.env['AGENT_MEMORY_CONFIG_HOME']).exists())
            self.assertFalse(Path(w.env['AGENT_MEMORY_STATE_HOME']).exists())

    def test_all_conditions_must_match_and_missing_context_never_becomes_universal(self):
        with World() as w:
            required = {'branch': 'feature/retries', 'worktree': 'tree-2', 'task': 'issue-8', 'session': 'session-3'}
            context = {**required, 'component': 'src/api/retry.py'}
            request = {**self.request(), 'scope': 'component:src/api', 'conditions': required, 'context': context}
            p, result = self.command(w, request)
            self.assertEqual(p.returncode, 0, result)
            self.assertTrue(result['applicable'])
            for key in context:
                for actual in (None, 'other'):
                    changed = dict(context)
                    if actual is None:
                        changed.pop(key)
                    else:
                        changed[key] = actual
                    p, result = self.command(w, {**request, 'context': changed})
                    self.assertEqual(p.returncode, 0, result)
                    self.assertFalse(result['applicable'], (key, actual))
            for actual, expected in [('src/api', True), ('src/api/nested/file.py', True), ('src/api2/retry.py', False), ('src', False)]:
                p, result = self.command(w, {**request, 'context': {**context, 'component': actual}})
                self.assertEqual(p.returncode, 0, result)
                self.assertEqual(result['applicable'], expected)
            # Additional conditions cannot overwrite the route's own condition.
            p, result = self.command(w, {**request, 'conditions': {**required, 'component': 'src/other'}})
            self.assertEqual(p.returncode, 0, result)
            self.assertFalse(result['applicable'])

    def test_unknown_personal_federation_and_noncanonical_conditions_are_invalid(self):
        with World() as w:
            request = self.request()
            variants = [{'scope': x} for x in ('personal', 'personal:me', 'developer:me', 'project:other', 'component:/etc',
                        'component:../other', 'component:src/../other', 'component:src/./api', 'component:src//api',
                        'component:src/api/', 'component:C:drive', 'component:.', 'branch:', 'branch:x\ncommand')]
            variants += [{'conditions': {'developer': str(uuid.uuid4())}}, {'context': {'personal': 'me'}},
                         {'federation_consent': True}, {'scope': None}, {'conditions': []},
                         {'developer_id': 'display-name'}, {'project_id': 'repo-name'}]
            for changed in variants:
                p, result = self.command(w, {**request, **changed})
                self.assertEqual(p.returncode, 1, changed)
                self.assertFalse(result['valid'])
                self.assertFalse(result['writes_performed'])

    def test_router_and_copied_portable_helper_share_the_same_scope_contract(self):
        with World() as w:
            pack = w.inside('portable scope pack')
            shutil.copytree(ROOT / 'skills/agent-memory', pack)
            request = {**self.request(), 'scope': 'branch:feature', 'context': {'branch': 'feature'}}
            p, result = self.command(w, request, pack / 'scripts/scope_policy.py')
            self.assertEqual(p.returncode, 0, result)
            self.assertTrue(result['applicable'])
            proposal = json.loads((pack / 'templates/router-proposal.example.json').read_text())
            proposal['project-id'] = request['project_id']
            for scope in ('personal:me', 'component:src/./api', 'component:src//api', 'component:C:drive'):
                proposal['contexts']['projectbrief.md']['scope'] = scope
                source = w.inside('router.json')
                source.write_text(json.dumps(proposal))
                p = w.run([sys.executable, '-I', pack / 'scripts/bank_contract.py', 'render', '--input', source], check=False)
                self.assertEqual(p.returncode, 1, (scope, p.stdout))
            p = w.run([sys.executable, '-I', pack / 'scripts/scope_policy.py', 'model'])
            model = json.loads(p.stdout)['model']
            self.assertEqual(model['default-scope'], 'repository')
            self.assertFalse(model['personal-namespaces'])
            self.assertEqual(model['developer-role'], 'provenance-and-clarification-only')


if __name__ == '__main__':
    unittest.main()
