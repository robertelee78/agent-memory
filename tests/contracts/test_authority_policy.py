"""Current-source conflicts, domain boundaries and nonmutating candidates."""
import copy
import json
from pathlib import Path
import shutil
import sys
import unittest
import uuid

from world import World

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / 'scripts/authority_policy.py'


class AuthorityPolicyContract(unittest.TestCase):
    def request(self):
        project, detector, author = (str(uuid.uuid4()) for _ in range(3))
        return {'fact': 'implementation', 'fact_key': 'retry-limit', 'project_id': project,
                'detecting_developer_id': detector,
                'memory': {'id': str(uuid.uuid4()), 'version': 2, 'project_id': project, 'value': 5,
                           'scope': 'component:retry.py', 'conditions': {'branch': 'main'},
                           'developer_id': author, 'evidence': ['historical:retry-observation']}, 'sources': []}

    def source(self, request, kind, value, ref=None, **flags):
        return {'kind': kind, 'ref': ref or kind + ':current-reference', 'project_id': request['project_id'],
                'fact_key': request['fact_key'], 'current': True, 'applicable': True, 'qualified': True,
                'value': value, **flags}

    def command(self, w, request, helper=HELPER):
        file = w.inside('reviewed-assertions.json')
        file.write_text(json.dumps(request))
        before = file.read_bytes()
        p = w.run([sys.executable, '-I', helper, 'assess', '--input', file], check=False)
        self.assertEqual(file.read_bytes(), before)
        return p, json.loads(p.stdout)

    def test_every_adr_authority_domain_preserves_the_full_matrix_and_no_memory_proof(self):
        with World() as w:
            p = w.run([sys.executable, '-I', HELPER, 'model'])
            model = json.loads(p.stdout)['model']
            expected = {'implementation': {'code', 'test'}, 'intent': {'approved-planning'},
                        'delivery': {'issue', 'ci', 'integration', 'release'}, 'coordination': {'coordination'},
                        'requirement': {'user-statement', 'approved-decision'},
                        'historical-observation': {'memory-event'}, 'procedure': {'reviewed-procedure'}}
            self.assertEqual({r['fact']: set(r['source-kinds']) for r in model['domains']}, expected)
            self.assertFalse(model['memory-is-proof'])
            self.assertFalse(model['landing-time-is-authority'])
            for fact, kinds in expected.items():
                with self.subTest(fact=fact):
                    request = self.request()
                    request['fact'] = fact
                    kind = sorted(kinds)[0]
                    request['sources'] = [self.source(request, kind, 3), self.source(request, 'memory', 5)]
                    p, result = self.command(w, request)
                    self.assertEqual(p.returncode, 0, result)
                    self.assertEqual(result['action'], 'propose-correct')
                    self.assertEqual(result['source_refs'], [kind + ':current-reference'])
                    self.assertFalse(result['memory_is_proof'])
                    self.assertFalse(result['source_assertions_verified'])
                    self.assertFalse(result['identity_authenticated'])
                    self.assertFalse(result['writes_performed'])
                    candidate = result['correction_candidate']
                    self.assertEqual(candidate['target'], request['memory'])
                    self.assertEqual(candidate['proposed_value'], 3)
                    self.assertEqual(candidate['detected_by'], request['detecting_developer_id'])

    def test_conflicting_code_intent_and_direct_project_decision_keep_their_own_authorities(self):
        with World() as w:
            repo = w.repository()
            (repo / 'retry.py').write_text('ATTEMPTS = 3\n')
            (repo / 'approved-adr.md').write_text('Approved desired retry limit: 5\n')
            (repo / 'project-decision.md').write_text('Operator decision for this project: yellow walls.\n')
            w.git('add', '.', cwd=repo)
            w.git('commit', '-m', 'Fixture evidence', cwd=repo)
            revision = w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip()
            before = {p.name: p.read_bytes() for p in repo.iterdir() if p.is_file()}
            request = self.request()
            actual = int((repo / 'retry.py').read_text().split('=')[1])
            request['sources'] = [self.source(request, 'code', actual, revision + ':retry.py'),
                                  self.source(request, 'approved-planning', 5, revision + ':approved-adr.md'),
                                  self.source(request, 'memory', 5, 'memory:landed-later')]
            p, result = self.command(w, request)
            self.assertEqual(p.returncode, 0, result)
            self.assertEqual(result['correction_candidate']['proposed_value'], 3)
            self.assertEqual(result['source_refs'], [revision + ':retry.py'])
            self.assertIn(revision + ':approved-adr.md', result['observed_refs'])
            request['fact'] = 'intent'
            p, result = self.command(w, request)
            self.assertEqual(p.returncode, 0, result)
            self.assertEqual(result['action'], 'use-current-authority')
            self.assertEqual(result['source_refs'], [revision + ':approved-adr.md'])
            request['fact'], request['fact_key'], request['memory']['value'] = 'requirement', 'project-wall-color', 'blue'
            request['sources'] = [self.source(request, 'user-statement', 'yellow', revision + ':project-decision.md'),
                                  self.source(request, 'memory', 'blue', 'memory:newest-note')]
            p, result = self.command(w, request)
            self.assertEqual(p.returncode, 0, result)
            self.assertEqual(result['correction_candidate']['proposed_value'], 'yellow')
            self.assertEqual({p.name: p.read_bytes() for p in repo.iterdir() if p.is_file()}, before)
            self.assertEqual(w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip(), revision)
            self.assertEqual(w.git('status', '--porcelain', cwd=repo).stdout, '')

    def test_peer_conflict_routes_to_detecting_developer_and_known_other_author_without_last_landing_wins(self):
        with World() as w:
            request = self.request()
            request['fact'], request['fact_key'] = 'requirement', 'project-wall-color'
            request['sources'] = [self.source(request, 'approved-decision', 'yellow', 'decision:first'),
                                  self.source(request, 'approved-decision', 'blue', 'decision:second')]
            for sources in (request['sources'], list(reversed(request['sources']))):
                p, result = self.command(w, {**request, 'sources': sources})
                self.assertEqual(p.returncode, 0, result)
                self.assertEqual(result['action'], 'clarify')
                self.assertIsNone(result['correction_candidate'])
                self.assertEqual(result['clarification']['detecting_developer_id'], request['detecting_developer_id'])
                self.assertEqual(result['clarification']['known_other_developer_ids'], [request['memory']['developer_id']])
            # Python's True == 1 must not silently settle a typed fact conflict.
            request['sources'] = [self.source(request, 'approved-decision', True, 'decision:boolean'),
                                  self.source(request, 'approved-decision', 1, 'decision:number')]
            p, result = self.command(w, request)
            self.assertEqual(p.returncode, 0, result)
            self.assertEqual(result['action'], 'clarify')
            request['sources'] = [self.source(request, 'approved-decision', 'yellow')]
            request['remedy'] = 'revoke'
            p, result = self.command(w, request)
            self.assertEqual(p.returncode, 0, result)
            self.assertEqual(result['action'], 'propose-revoke')
            self.assertEqual(result['correction_candidate']['target'], request['memory'])
            self.assertNotIn('proposed_value', result['correction_candidate'])

    def test_unqualified_inapplicable_or_stale_sources_cannot_promote_memory_to_proof(self):
        with World() as w:
            request = self.request()
            for flags in ({'current': False}, {'applicable': False}, {'qualified': False}):
                request['sources'] = [self.source(request, 'code', 3, **flags), self.source(request, 'memory', 5)]
                p, result = self.command(w, request)
                self.assertEqual(p.returncode, 0, result)
                self.assertEqual(result['action'], 'inspect-current-authority')
                self.assertIsNone(result['correction_candidate'])
            request['fact'] = 'procedure'
            request['sources'] = [self.source(request, 'reviewed-procedure', 'claimed-approved', qualified=False),
                                  self.source(request, 'memory', 'claimed-approved')]
            p, result = self.command(w, request)
            self.assertEqual(p.returncode, 0, result)
            self.assertEqual(result['action'], 'inspect-current-authority')

    def test_invalid_identity_fact_references_and_scope_refuse_and_copied_artifact_is_read_only(self):
        with World() as w:
            original = self.request()
            original['sources'] = [self.source(original, 'code', 3)]
            variants = []
            for key, value in [('project_id', str(uuid.uuid4())), ('fact_key', 'different-question'), ('current', 'yes')]:
                bad = copy.deepcopy(original)
                bad['sources'][0][key] = value
                variants.append(bad)
            for key, value in [('version', True), ('scope', 'personal:me'), ('project_id', str(uuid.uuid4()))]:
                bad = copy.deepcopy(original)
                bad['memory'][key] = value
                variants.append(bad)
            bad = copy.deepcopy(original)
            bad['sources'].append(dict(bad['sources'][0]))
            variants.append(bad)
            for bad in variants:
                p, result = self.command(w, bad)
                self.assertEqual(p.returncode, 1, bad)
                self.assertFalse(result['writes_performed'])
            pack = w.inside('portable authority pack')
            shutil.copytree(ROOT / 'skills/agent-memory', pack)
            p, result = self.command(w, original, pack / 'scripts/authority_policy.py')
            self.assertEqual(p.returncode, 0, result)
            self.assertEqual(result['action'], 'propose-correct')
            self.assertFalse(Path(w.env['AGENT_MEMORY_CONFIG_HOME']).exists())
            self.assertFalse(Path(w.env['AGENT_MEMORY_STATE_HOME']).exists())


if __name__ == '__main__':
    unittest.main()
