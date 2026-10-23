"""Governed invalidation through copied portable public interfaces."""
import copy
import hashlib
import json
import sys
import unittest
from datetime import datetime, timezone

from world import World
from contracts import test_comparison as fixtures
from contracts import test_admission as admission


class GovernanceContract(unittest.TestCase):
    call = fixtures.ComparisonContract.call
    good = fixtures.ComparisonContract.good
    save = fixtures.ComparisonContract.save
    fixture = fixtures.ComparisonContract.fixture
    add = fixtures.ComparisonContract.add
    state = fixtures.ComparisonContract.state

    def request(self, bank, metadata, path, operation='revoke'):
        return {'project_id': self.pid, 'operation': operation,
            'target': {'path': path, 'id': metadata['id'], 'version': metadata['version'],
                       'sha256': hashlib.sha256((bank / path).read_bytes()).hexdigest()},
            'authority': {'source_class': 'user', 'repository_evidence': None,
                'evidence': ['user:explicit-disposable-fixture-correction'],
                'reason': 'The operator confirmed this claim is incorrect under every retained condition.',
                'confirmed': True, 'strength': 'established', 'basis': 'stronger-evidence',
                'impact': 'all-target-conditions', 'review': None, 'reason_class': 'incorrect'},
            'comparison': None, 'capture': None, 'replacement': None, 'delete_ids': [],
            'context_review': {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                               for p in bank.glob('*.md') if p.name != 'MEMORY.md'},
            'context_edits': {}, 'context': {}}

    def preview(self, w, repo, request):
        return self.call(w, 'govern_memory.py', 'preview', '--repo', repo, '--request', self.save(w, request))

    def apply(self, w, proposal):
        return self.call(w, 'govern_memory.py', 'apply', '--proposal', self.save(w, proposal, 'governance.json'))

    def projection(self, w, repo, metadata):
        request = {'project_id': self.pid, 'records': [{'id': metadata['id'], 'version': metadata['version']}]}
        return self.good(w, 'projection.py', 'build', '--repo', repo, '--input', self.save(w, request))['projection']

    def replacement_capture(self, w, repo, bank, old, path, operation='correct', **metadata_changes):
        helper = admission.AdmissionContract('test_user_correction_admits_small_record_and_history_automatically_without_git_changes')
        helper.pack = self.pack;helper.pid = self.pid;helper.actor = self.actor
        request = helper.request();request['candidate']['metadata'].update(id=old['id'], **metadata_changes)
        request['candidate']['metadata']['origin']['session'] = 'correcting-session'
        request['candidate']['metadata']['evidence'] = ['user:current-confirmed-correction']
        request['candidate']['body'] = '# Retry policy\n\nUse at most three attempts for this fixture.\n'
        captured = helper.capture(w, repo, request)['capture']
        compared_request, proof = helper.compared(w, repo, captured, classification='contradiction')
        value = self.request(bank, old, path, operation)
        value['capture'] = captured;value['comparison'] = {'request': compared_request, 'proof': proof}
        value['authority']['evidence'] = ['user:current-confirmed-correction']
        return value

    def test_stronger_correction_keeps_stable_identity_old_origin_and_useful_history(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'incorrect', body='# Retry policy\n\nThe older fixture incorrectly said five attempts.\n')
            value = self.replacement_capture(w, repo, bank, old, path);before = self.state(w, repo, bank)
            r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
            self.assertEqual(before, self.state(w, repo, bank));r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            current, body = fixtures.unpack((bank / path).read_text());self.assertEqual(current['id'], old['id']);self.assertEqual(current['version'], 2)
            self.assertEqual(current['origin']['session'], 'correcting-session');self.assertIn({'id': old['id'], 'version': 1}, current['corrects'])
            archived, previous = fixtures.unpack((bank / 'history/versions' / old['id'] / 'v1.md').read_text())
            self.assertEqual(archived['origin'], old['origin']);self.assertEqual(archived['status'], 'superseded')
            self.assertIn('incorrectly said five attempts', previous);self.assertEqual(before[1:], self.state(w, repo, bank)[1:])
            before = self.state(w, repo, bank);r, replay = self.apply(w, plan['proposal'])
            self.assertEqual(r.returncode, 0, replay);self.assertTrue(replay['already_applied']);self.assertEqual(before, self.state(w, repo, bank))

    def test_reactivation_needs_new_evidence_and_an_explicit_current_period(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'revoked', status='revoked')
            value = self.replacement_capture(w, repo, bank, old, path, 'reactivate', **{'valid-from': datetime.now(timezone.utc).isoformat()})
            r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
            before = self.state(w, repo, bank);r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            current, _ = fixtures.unpack((bank / path).read_text());self.assertEqual(current['status'], 'active');self.assertEqual(current['version'], 2)
            historical, _ = fixtures.unpack((bank / 'history/versions' / old['id'] / 'v1.md').read_text())
            self.assertEqual(historical['status'], 'revoked');self.assertEqual(historical['origin'], old['origin'])
            self.assertIn(path, historical['state-reason'])
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])

    def test_revocation_removes_current_guidance_and_invalidates_projection_and_cached_payload(self):
        with World() as w:
            repo, bank = self.fixture(w);metadata, path = self.add(bank, 'obsolete', body='# Retry policy\n\nOBSOLETE_BODY_MUST_NOT_BE_CURRENT.\n')
            projection = self.projection(w, repo, metadata);request = self.request(bank, metadata, path)
            before = self.state(w, repo, bank);r, plan = self.preview(w, repo, request)
            self.assertEqual(r.returncode, 0, plan);self.assertEqual(before, self.state(w, repo, bank))
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done);self.assertTrue(done['governance_verified'])
            current, body = fixtures.unpack((bank / path).read_text())
            self.assertEqual(current['status'], 'revoked');self.assertEqual(current['origin'], metadata['origin'])
            self.assertNotIn('OBSOLETE_BODY_MUST_NOT_BE_CURRENT', body)
            r, stale = self.call(w, 'projection.py', 'check', '--repo', repo, '--input', self.save(w, projection))
            self.assertEqual(r.returncode, 1, stale);self.assertNotIn('records', stale)
            request = {'project_id': self.pid, 'records': [{'id': metadata['id'], 'version': 1, 'body': 'OBSOLETE_BODY_MUST_NOT_BE_CURRENT'}]}
            guarded = self.good(w, 'inactive_records.py', 'filter', '--repo', repo, '--input', self.save(w, request))
            self.assertEqual(guarded['remaining_references'], []);self.assertNotIn('OBSOLETE_BODY_MUST_NOT_BE_CURRENT', json.dumps(guarded))
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])
            before = self.state(w, repo, bank);r, replay = self.apply(w, plan['proposal'])
            self.assertEqual(r.returncode, 0, replay);self.assertTrue(replay['already_applied']);self.assertEqual(before, self.state(w, repo, bank))

    def test_expiry_requires_actual_known_boundary_and_policy_retention_is_distinct(self):
        for operation, basis, until, expected in [('expire', 'time-boundary', '2020-01-01T00:00:00Z', 'expired'),
                ('expire', 'time-boundary', '2099-01-01T00:00:00Z', None), ('retention', 'policy', None, 'revoked')]:
            with self.subTest(operation=operation, until=until), World() as w:
                repo, bank = self.fixture(w);metadata, path = self.add(bank, 'bounded', **{'valid-until': until})
                request = self.request(bank, metadata, path, operation);request['authority']['basis'] = basis
                if operation == 'expire':request['authority']['source_class'] = 'tool'
                before = self.state(w, repo, bank);r, plan = self.preview(w, repo, request)
                if expected is None:
                    self.assertEqual(r.returncode, 1, plan);self.assertEqual(before, self.state(w, repo, bank));continue
                self.assertEqual(r.returncode, 0, plan);r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
                self.assertEqual(fixtures.unpack((bank / path).read_text())[0]['status'], expected)
                self.assertEqual(before[1:], self.state(w, repo, bank)[1:])

    def test_ambiguous_evidence_or_older_contexts_still_applying_stays_pending_without_pruning(self):
        for override in ({'strength': 'ambiguous'}, {'confirmed': False}, {'impact': 'narrower-contexts-remain'},
                         {'source_class': 'assistant'}):
            with self.subTest(override=override), World() as w:
                repo, bank = self.fixture(w);metadata, path = self.add(bank, 'conditional')
                request = self.request(bank, metadata, path);request['authority'].update(override)
                before = self.state(w, repo, bank);r, pending = self.preview(w, repo, request)
                self.assertEqual(r.returncode, 0, pending);self.assertIsNone(pending['proposal'])
                self.assertEqual(pending['detecting_developer_id'], self.actor);self.assertFalse(pending['messages_sent'])
                self.assertEqual(before, self.state(w, repo, bank))

    def test_terminal_deletion_requires_explicit_bounded_derived_scope_and_removes_only_safe_markers(self):
        with World() as w:
            repo, bank = self.fixture(w);original, p1 = self.add(bank, 'original', body='# Retry policy\n\nDELETED_CANONICAL_PAYLOAD.\n')
            derived, p2 = self.add(bank, 'derived', **{'derived-from': [{'id': original['id'], 'version': 1}]},
                body='# Derived retry note\n\nDELETED_CANONICAL_PAYLOAD.\n')
            projection = self.projection(w, repo, original);request = self.request(bank, original, p1, 'delete')
            request['authority'].update(basis='explicit-deletion', reason_class='user-request')
            request['delete_ids'] = [original['id']];before = self.state(w, repo, bank)
            r, pending = self.preview(w, repo, request);self.assertEqual(r.returncode, 0, pending)
            self.assertEqual(pending['disposition'], 'deletion-scope-review-required');self.assertIsNone(pending['proposal'])
            self.assertEqual(before, self.state(w, repo, bank))
            request['delete_ids'] = [original['id'], derived['id']]
            r, plan = self.preview(w, repo, request);self.assertEqual(r.returncode, 0, plan)
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            self.assertFalse((bank / p1).exists());self.assertFalse((bank / p2).exists())
            for identity in request['delete_ids']:
                marker = bank / 'history/tombstones' / (identity + '.md');metadata, body = fixtures.unpack(marker.read_text())
                self.assertEqual(set(metadata), {'id', 'scope', 'deleted-at', 'deletion-authority', 'reason-class'});self.assertEqual(body, '')
            self.assertNotIn('DELETED_CANONICAL_PAYLOAD', '\n'.join(p.read_text() for p in bank.rglob('*.md')))
            r, stale = self.call(w, 'projection.py', 'check', '--repo', repo, '--input', self.save(w, projection));self.assertEqual(r.returncode, 1, stale)
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])

    def test_stale_target_or_context_review_and_tampered_delta_refuse_without_writes(self):
        for mutation in ('target', 'context', 'delta'):
            with self.subTest(mutation=mutation), World() as w:
                repo, bank = self.fixture(w);metadata, path = self.add(bank, 'old');request = self.request(bank, metadata, path)
                if mutation == 'context':request['context_review'] = {}
                if mutation == 'target':request['target']['sha256'] = '0' * 64
                before = self.state(w, repo, bank);r, plan = self.preview(w, repo, request)
                if mutation != 'delta':self.assertEqual(r.returncode, 1, plan)
                else:
                    self.assertEqual(r.returncode, 0, plan);plan['proposal']['files']['projectbrief.md'] += '\nUnrelated fabricated edit.\n'
                    r, refused = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 1, refused)
                self.assertEqual(before, self.state(w, repo, bank))
