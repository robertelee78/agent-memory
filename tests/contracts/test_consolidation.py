"""Public source-bound maintenance, provenance, replay and safe deferral."""
import copy
import json
from pathlib import Path
import sys
import unittest
import uuid

from world import World
from contracts import test_comparison as fixtures
from contracts import test_admission as admission


class ConsolidationContract(unittest.TestCase):
    call = fixtures.ComparisonContract.call
    good = fixtures.ComparisonContract.good
    save = fixtures.ComparisonContract.save
    fixture = fixtures.ComparisonContract.fixture
    add = fixtures.ComparisonContract.add
    state = fixtures.ComparisonContract.state

    def compared(self, w, repo, metadata, body, paths, captured=None, context=None, overrides=None):
        request = {'project_id': self.pid, 'candidate': {'metadata': metadata, 'body': body},
                   'searches': ['retry policy', 'attempt cap'], 'read_paths': paths}
        observed = self.good(w, 'compare_learning.py', 'inspect', '--repo', repo, '--request', self.save(w, request))
        decisions = []
        for row in observed['matches']:
            kind = 'equivalent' if row['metadata'] and row['lifecycle'] == 'active' else 'related-context'
            if overrides:kind = overrides.get(row['path'], kind)
            decisions.append({'path': row['path'], 'classification': kind,
                              'rationale': 'The actual fixture meaning and conditions were compared; history stays provenance.'})
        judgment = {'observation': observed['observation'], 'decisions': decisions,
                    'rationale': 'Retain the confirmed retry meaning and every distinct source while tightening wording.'}
        reviewed = self.good(w, 'compare_learning.py', 'review', '--repo', repo, '--request', self.save(w, request),
                             '--judgment', self.save(w, judgment, 'judgment.json'))
        return {'comparison_request': request, 'comparison_proof': reviewed['proof'], 'capture': captured, 'context': context or {}}

    def preview(self, w, repo, value):
        return self.call(w, 'consolidate_learning.py', 'preview', '--repo', repo, '--input', self.save(w, value))

    def apply(self, w, proposal):
        return self.call(w, 'consolidate_learning.py', 'apply', '--proposal', self.save(w, proposal, 'maintenance.json'))

    def test_duplicates_keep_stable_identity_all_provenance_and_correction_history_then_converge(self):
        with World() as w:
            repo, bank = self.fixture(w)
            first, p1 = self.add(bank, 'z-primary', body='# Retry policy\n\nStop after three attempts; avoid a fourth retry.\n')
            second, p2 = self.add(bank, 'a-second', body='# Attempt cap\n\nAllow three attempts for the same fixture.\n')
            second['origin'].update({'developer-id': str(uuid.uuid4()), 'harness': 'claude', 'session': 'other-session'})
            (bank / p2).write_text(fixtures.render(second, '# Attempt cap\n\nAllow three attempts for the same fixture.\n'))
            old = (bank / p1).read_text();before = self.state(w, repo, bank)
            body = '# Retry policy\n\nStop after three attempts.\n'
            value = self.compared(w, repo, first, body, [p1, p2])
            r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan);self.assertIsNotNone(plan['proposal'])
            self.assertEqual(before, self.state(w, repo, bank));r, done = self.apply(w, plan['proposal'])
            self.assertEqual(r.returncode, 0, done);self.assertTrue(done['consolidation_verified']);self.assertFalse(done['publication_verified'])
            current, text = fixtures.unpack((bank / p1).read_text())
            self.assertEqual(current['id'], first['id']);self.assertEqual(current['version'], 2);self.assertEqual(current['origin'], first['origin'])
            self.assertEqual(text, body);inactive, _ = fixtures.unpack((bank / p2).read_text())
            self.assertEqual(inactive['status'], 'superseded');self.assertEqual(inactive['origin'], second['origin'])
            self.assertEqual(inactive['evidence'], second['evidence']);self.assertEqual(inactive['conditions'], second['conditions'])
            archived = bank / 'history/versions' / first['id'] / 'v1.md'
            previous, previous_body = fixtures.unpack(archived.read_text())
            self.assertEqual(previous_body, fixtures.unpack(old)[1]);self.assertEqual(previous['origin'], first['origin'])
            self.assertIn(p1, previous['state-reason'])
            self.assertIn({'id': second['id'], 'version': 1}, current['derived-from'])
            self.assertIn('record:' + second['id'] + '/v1', current['evidence'])
            self.assertEqual(len(list((bank / 'history/events').glob('*.md'))), 3)
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])
            before = self.state(w, repo, bank);r, replay = self.apply(w, plan['proposal'])
            self.assertEqual(r.returncode, 0, replay);self.assertTrue(replay['already_applied']);self.assertEqual(before, self.state(w, repo, bank))
            again = self.compared(w, repo, current, body, [p1])
            r, noop = self.preview(w, repo, again);self.assertEqual(r.returncode, 0, noop);self.assertEqual(noop['disposition'], 'no-change')
            self.assertIsNone(noop['proposal']);self.assertEqual(before, self.state(w, repo, bank))

    def test_qualified_duplicate_capture_retains_new_session_origin_without_new_active_identity(self):
        with World() as w:
            repo, bank = self.fixture(w)
            source, path = self.add(bank, 'existing', body='# Retry policy\n\nUse at most three attempts for this fixture.\n')
            helper = admission.AdmissionContract('test_user_correction_admits_small_record_and_history_automatically_without_git_changes')
            helper.pack = self.pack;helper.pid = self.pid;helper.actor = self.actor
            request = helper.request();request['candidate']['metadata']['origin']['session'] = 'new-observing-session'
            captured = helper.capture(w, repo, request)['capture']
            value = self.compared(w, repo, request['candidate']['metadata'], request['candidate']['body'], [path], captured=captured)
            before = self.state(w, repo, bank);r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done);self.assertTrue(done['consolidation_verified'])
            current, body = fixtures.unpack((bank / path).read_text());self.assertEqual(current['id'], source['id'])
            snapshots = list((bank / 'history/sources').glob('*.md'));self.assertEqual(len(snapshots), 1)
            provenance, _ = fixtures.unpack(snapshots[0].read_text())
            self.assertEqual(provenance['origin'], request['candidate']['metadata']['origin']);self.assertEqual(provenance['status'], 'superseded')
            self.assertIn({'id': provenance['id'], 'version': 1}, current['derived-from'])
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])
            value = self.compared(w, repo, request['candidate']['metadata'], body, [path], captured=captured)
            before = self.state(w, repo, bank);r, noop = self.preview(w, repo, value)
            self.assertEqual(r.returncode, 0, noop);self.assertEqual(noop['disposition'], 'no-change');self.assertEqual(before, self.state(w, repo, bank))
