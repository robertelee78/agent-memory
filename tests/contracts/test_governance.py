"""Governed invalidation through copied portable public interfaces."""
import copy
import hashlib
import json
import sys
import unittest
import uuid
from datetime import datetime, timezone

from world import World
from contracts import test_comparison as fixtures
from contracts import test_admission as admission
from contracts import test_consolidation as consolidation


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
                'impact': 'all-target-conditions', 'review': None, 'reason_class': None},
            'comparison': None, 'capture': None, 'replacement': None, 'delete_ids': [],
            'context_review': {p.name: hashlib.sha256(p.read_bytes()).hexdigest()
                               for p in bank.glob('*.md')},
            'context_edits': {}, 'context': {}}

    def preview(self, w, repo, request):
        return self.call(w, 'govern_memory.py', 'preview', '--repo', repo, '--request', self.save(w, request))

    def apply(self, w, proposal):
        return self.call(w, 'govern_memory.py', 'apply', '--proposal', self.save(w, proposal, 'governance.json'))

    def projection(self, w, repo, metadata):
        request = {'project_id': self.pid, 'records': [{'id': metadata['id'], 'version': metadata['version']}]}
        return self.good(w, 'projection.py', 'build', '--repo', repo, '--input', self.save(w, request))['projection']

    def replacement_capture(self, w, repo, bank, old, path, operation='correct', classification='contradiction', omit_policy=False, **metadata_changes):
        helper = admission.AdmissionContract('test_user_correction_admits_small_record_and_history_automatically_without_git_changes')
        helper.pack = self.pack;helper.pid = self.pid;helper.actor = self.actor
        request = helper.request();request['candidate']['metadata'].update(id=old['id'], **metadata_changes)
        request['candidate']['metadata']['origin']['session'] = 'correcting-session'
        request['candidate']['metadata']['evidence'] = ['user:current-confirmed-correction']
        if omit_policy:
            for key in ('trust', 'visibility', 'confidence', 'admission-reason'):request['candidate']['metadata'].pop(key, None)
        request['candidate']['body'] = '# Retry policy\n\nUse at most three attempts for this fixture.\n'
        captured = helper.capture(w, repo, request)['capture']
        compared_request, proof = helper.compared(w, repo, captured, classification=classification)
        value = self.request(bank, old, path, operation)
        value['capture'] = captured;value['comparison'] = {'request': compared_request, 'proof': proof}
        value['authority']['evidence'] = ['user:current-confirmed-correction']
        return value

    def superseding(self, w, repo, bank, old, p1, replacement, p2):
        request = {'project_id': self.pid, 'candidate': {'metadata': replacement, 'body': fixtures.unpack((bank / p2).read_text())[1]},
            'searches': ['retry policy', 'attempt cap'], 'read_paths': [p1, p2]}
        observed = self.good(w, 'compare_learning.py', 'inspect', '--repo', repo, '--request', self.save(w, request))
        judgment = {'observation': observed['observation'], 'decisions': [
            {'path': row['path'], 'classification': 'contradiction' if row['path'] == p1 else 'equivalent' if row['path'] == p2 else 'related-context',
             'rationale': 'The new confirmed source resolves the wrong claim; the replacement is read unchanged.'} for row in observed['matches']],
            'rationale': 'Use the independently evidenced existing correction without inventing a third claim.'}
        reviewed = self.good(w, 'compare_learning.py', 'review', '--repo', repo, '--request', self.save(w, request),
            '--judgment', self.save(w, judgment, 'judgment.json'))
        value = self.request(bank, old, p1, 'supersede')
        value['replacement'] = {'path': p2, 'id': replacement['id'], 'version': replacement['version'],
                                'sha256': hashlib.sha256((bank / p2).read_bytes()).hexdigest()}
        value['comparison'] = {'request': request, 'proof': reviewed['proof']}
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
            repo, bank = self.fixture(w);old, path = self.add(bank, 'revoked')
            r, plan = self.preview(w, repo, self.request(bank, old, path));self.assertEqual(r.returncode, 0, plan)
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            old, _ = fixtures.unpack((bank / path).read_text());self.assertNotIn('visibility', old)
            value = self.replacement_capture(w, repo, bank, old, path, 'reactivate', omit_policy=True,
                **{'valid-from': datetime.now(timezone.utc).isoformat()})
            r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
            before = self.state(w, repo, bank);r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            current, _ = fixtures.unpack((bank / path).read_text());self.assertEqual(current['status'], 'active');self.assertEqual(current['version'], 2)
            historical, _ = fixtures.unpack((bank / 'history/versions' / old['id'] / 'v1.md').read_text())
            self.assertEqual(historical['status'], 'revoked');self.assertEqual(historical['origin'], old['origin'])
            self.assertIn(path, historical['state-reason'])
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])

    def test_consumed_capture_identity_still_finds_revoked_learning_after_all_inactive_bodies_are_compacted(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'existing')
            capture_helper = admission.AdmissionContract();capture_helper.pack = self.pack;capture_helper.pid = self.pid;capture_helper.actor = self.actor
            capture_request = capture_helper.request();captured = capture_helper.capture(w, repo, capture_request)['capture']
            helper = consolidation.ConsolidationContract();helper.pack = self.pack;helper.pid = self.pid;helper.actor = self.actor
            compared = helper.compared(w, repo, capture_request['candidate']['metadata'], capture_request['candidate']['body'], [path], captured=captured)
            r, plan = helper.preview(w, repo, compared);self.assertEqual(r.returncode, 0, plan)
            r, done = helper.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            current, _ = fixtures.unpack((bank / path).read_text());r, plan = self.preview(w, repo, self.request(bank, current, path))
            self.assertEqual(r.returncode, 0, plan);r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            for source in [*(bank / 'history/sources').rglob('*.md'), *(bank / 'history/versions').rglob('*.md')]:
                compacted = self.good(w, 'inactive_records.py', 'compact', '--file', source)['content']
                content = w.inside('inactive.md');content.write_text(compacted)
                prepared = self.good(w, 'bank_writes.py', 'preview', '--repo', repo, '--path', source.relative_to(bank), '--content', content)
                self.good(w, 'bank_writes.py', 'apply', '--proposal', self.save(w, prepared, 'compact.json'))
            request, proof = capture_helper.compared(w, repo, captured, classification='equivalent')
            self.assertEqual(proof['disposition'], 'lifecycle-review-required')
            before = self.state(w, repo, bank);r, pending = capture_helper.preview(w, repo, captured, request, proof)
            self.assertEqual(r.returncode, 0, pending);self.assertIsNone(pending['proposal'])
            self.assertEqual(pending['disposition'], 'lifecycle-review-required');self.assertEqual(before, self.state(w, repo, bank))

    def test_scope_correction_retains_older_branch_guidance_without_applying_it_on_main(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'older-policy', body='# Retry policy\n\nUse at most three attempts for this fixture.\n')
            value = self.replacement_capture(w, repo, bank, old, path, classification='scope-correction', conditions={'branch': 'older/release'})
            value['authority']['impact'] = 'narrower-contexts-remain'
            r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            recall = {'project_id': self.pid, 'query': '', 'ids': [old['id']], 'paths': [], 'context': {},
                'kinds': ['semantic-fact'], 'minimum_trust': 'reviewed', 'limit': 8, 'include_context': False}
            current = self.good(w, 'retrieve.py', '--repo', repo, '--request', self.save(w, recall))
            self.assertEqual(current['results'], [])
            w.git('checkout', '-b', 'older/release', cwd=repo)
            older = self.good(w, 'retrieve.py', '--repo', repo, '--request', self.save(w, recall))
            self.assertEqual(len(older['results']), 1);self.assertEqual(older['results'][0]['record']['version'], 2)

    def test_narrowing_cannot_replace_the_old_claim_that_still_applies_in_older_contexts(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'older-policy', body='# Retry policy\n\nFive attempts still applies to the older branch.\n')
            value = self.replacement_capture(w, repo, bank, old, path, conditions={'branch': 'older/release'})
            value['authority']['impact'] = 'narrower-contexts-remain'
            before = self.state(w, repo, bank);r, pending = self.preview(w, repo, value)
            self.assertEqual(r.returncode, 0, pending);self.assertIsNone(pending['proposal'])
            self.assertEqual(pending['disposition'], 'narrowing-required');self.assertEqual(before, self.state(w, repo, bank))

    def test_existing_replacement_is_versioned_and_survives_deletion_of_the_wrong_old_claim(self):
        with World() as w:
            repo, bank = self.fixture(w);old, p1 = self.add(bank, 'wrong', body='# Retry policy\n\nFive attempts was the disproved old claim.\n')
            replacement, p2 = self.add(bank, 'correct', body='# Retry policy\n\nThree attempts is the independently evidenced policy.\n')
            value = self.superseding(w, repo, bank, old, p1, replacement, p2)
            compared_request = copy.deepcopy(value['comparison']['request'])
            before = self.state(w, repo, bank);r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            current, body = fixtures.unpack((bank / p2).read_text());self.assertEqual(current['version'], 2)
            self.assertEqual(current['origin'], replacement['origin']);self.assertIn({'id': old['id'], 'version': 1}, current['supersedes'])
            self.assertEqual(fixtures.unpack((bank / p1).read_text())[0]['status'], 'superseded')
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])
            deletion = self.request(bank, fixtures.unpack((bank / p1).read_text())[0], p1, 'delete')
            deletion['authority'].update(basis='explicit-deletion', reason_class='user-request');deletion['delete_ids'] = [old['id']]
            r, plan = self.preview(w, repo, deletion);self.assertEqual(r.returncode, 0, plan)
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            after, text = fixtures.unpack((bank / p2).read_text());self.assertEqual(text, body)
            self.assertEqual(after['origin'], replacement['origin']);self.assertEqual(after['supersedes'], [])
            compared_request['candidate'] = {'metadata': after, 'body': text};compared_request['read_paths'] = [p2]
            inspection = self.good(w, 'compare_learning.py', 'inspect', '--repo', repo, '--request', self.save(w, compared_request))
            self.assertNotIn(p1, [row['path'] for row in inspection['matches']])

    def test_deletion_requires_reviewed_removal_of_copied_ordinary_context(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'erase', body='# Retry policy\n\nERASE_THIS_COPIED_ASSERTION.\n')
            original = (bank / 'projectbrief.md').read_text();(bank / 'projectbrief.md').write_text(original + fixtures.unpack((bank / path).read_text())[1])
            value = self.request(bank, old, path, 'delete');value['authority'].update(basis='explicit-deletion', reason_class='user-request')
            value['delete_ids'] = [old['id']];before = self.state(w, repo, bank)
            r, refused = self.preview(w, repo, value);self.assertEqual(r.returncode, 1, refused);self.assertEqual(before, self.state(w, repo, bank))
            value['context_edits'] = {'projectbrief.md': original}
            r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            self.assertEqual((bank / 'projectbrief.md').read_text(), original)
            self.assertNotIn('ERASE_THIS_COPIED_ASSERTION', '\n'.join(p.read_text() for p in bank.rglob('*.md')))
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])

    def test_deletion_refuses_a_copied_router_assertion_until_its_existing_file_workflow_cleans_it(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'erase', body='# Retry policy\n\nERASE_THIS_ROUTER_COPY.\n')
            router = bank / 'MEMORY.md';original = router.read_text();_, body = fixtures.unpack((bank / path).read_text())
            router.write_text(original.replace('\n## Routes\n', '\n' + body + '\n## Routes\n'))
            value = self.request(bank, old, path, 'delete');value['authority'].update(basis='explicit-deletion', reason_class='user-request')
            value['delete_ids'] = [old['id']];before = self.state(w, repo, bank)
            r, refused = self.preview(w, repo, value);self.assertEqual(r.returncode, 1, refused);self.assertEqual(before, self.state(w, repo, bank))
            router.write_text(original)
            value = self.request(bank, old, path, 'delete');value['authority'].update(basis='explicit-deletion', reason_class='user-request')
            value['delete_ids'] = [old['id']]
            r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            self.assertNotIn('ERASE_THIS_ROUTER_COPY', '\n'.join(p.read_text() for p in bank.rglob('*.md')))

    def test_deleted_independent_duplicates_cannot_return_from_cached_consolidation_output(self):
        with World() as w:
            repo, bank = self.fixture(w);first, p1 = self.add(bank, 'first-copy', body='# Retry policy\n\nUse three attempts; this old duplicate must be erased.\n')
            second, p2 = self.add(bank, 'independent-copy', body='# Attempt cap\n\nUse three attempts for this fixture; independently captured.\n')
            helper = consolidation.ConsolidationContract();helper.pack = self.pack;helper.pid = self.pid;helper.actor = self.actor
            compared = helper.compared(w, repo, first, '# Retry policy\n\nUse three attempts.\n', [p1, p2])
            r, maintenance = helper.preview(w, repo, compared);self.assertEqual(r.returncode, 0, maintenance)
            deletion = self.request(bank, first, p1, 'delete')
            deletion['authority'].update(basis='explicit-deletion', reason_class='user-request')
            deletion['delete_ids'] = [first['id'], second['id']]
            before = self.state(w, repo, bank);r, plan = self.preview(w, repo, deletion);self.assertEqual(r.returncode, 0, plan)
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            self.assertFalse((bank / p1).exists());self.assertFalse((bank / p2).exists())
            after = self.state(w, repo, bank);r, refused = helper.apply(w, maintenance['proposal'])
            self.assertEqual(r.returncode, 1, refused);self.assertEqual(after, self.state(w, repo, bank))
            rebuilt = self.good(w, 'projection.py', 'build', '--repo', repo, '--input', self.save(w, {
                'project_id': self.pid, 'records': [{'id': metadata['id'], 'version': 1,
                    'body': 'cached old learning cannot enter rebuilt guidance'} for metadata in (first, second)]}))
            self.assertEqual(rebuilt['projection']['records'], []);self.assertEqual(before[1:], after[1:])

    def test_erasure_includes_consolidated_capture_identity_so_a_saved_capture_cannot_be_admitted_again(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'existing')
            capture_helper = admission.AdmissionContract();capture_helper.pack = self.pack;capture_helper.pid = self.pid;capture_helper.actor = self.actor
            capture_request = capture_helper.request();capture_request['candidate']['metadata']['origin']['session'] = 'later-observation'
            captured = capture_helper.capture(w, repo, capture_request)['capture'];capture_id = capture_request['candidate']['metadata']['id']
            helper = consolidation.ConsolidationContract();helper.pack = self.pack;helper.pid = self.pid;helper.actor = self.actor
            compared = helper.compared(w, repo, capture_request['candidate']['metadata'], capture_request['candidate']['body'], [path], captured=captured)
            r, plan = helper.preview(w, repo, compared);self.assertEqual(r.returncode, 0, plan)
            r, done = helper.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            source_path = next((bank / 'history/sources').rglob('*.md'));source, _ = fixtures.unpack(source_path.read_text())
            self.assertEqual(source_path.parent.name, capture_id)
            current, _ = fixtures.unpack((bank / path).read_text())
            request = self.request(bank, current, path, 'delete');request['authority'].update(basis='explicit-deletion', reason_class='user-request')
            request['delete_ids'] = [old['id'], source['id']]
            before = self.state(w, repo, bank);r, pending = self.preview(w, repo, request)
            self.assertEqual(r.returncode, 0, pending);self.assertIsNone(pending['proposal'])
            self.assertEqual(set(pending['required_delete_ids']), {old['id'], source['id'], capture_id})
            self.assertEqual(before, self.state(w, repo, bank))
            # An older flat source file has no durable capture-identity role.
            # Preserve it for review rather than guess a reference's meaning.
            structured = source_path.relative_to(bank).as_posix();flat = 'history/sources/' + source['id'] + '.md'
            source_path.rename(bank / flat);router = bank / 'MEMORY.md';router.write_text(router.read_text().replace(structured, flat))
            legacy = self.request(bank, current, path, 'delete');legacy['authority'].update(basis='explicit-deletion', reason_class='user-request')
            legacy['delete_ids'] = [old['id']];before_legacy = self.state(w, repo, bank)
            r, pending = self.preview(w, repo, legacy);self.assertEqual(r.returncode, 0, pending)
            self.assertEqual(pending['disposition'], 'provenance-identity-review-required');self.assertIsNone(pending['proposal'])
            self.assertEqual(before_legacy, self.state(w, repo, bank))
            (bank / flat).rename(source_path);router.write_text(router.read_text().replace(flat, structured))
            request = self.request(bank, current, path, 'delete');request['authority'].update(basis='explicit-deletion', reason_class='user-request')
            request['delete_ids'] = [old['id'], source['id'], capture_id]
            r, plan = self.preview(w, repo, request);self.assertEqual(r.returncode, 0, plan)
            r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            for identity in request['delete_ids']:
                self.assertTrue((bank / 'history/tombstones' / (identity + '.md')).exists())
            replay = capture_helper.capture(w, repo, capture_request)
            self.assertEqual(replay['decision'], 'quarantine');self.assertIsNone(replay['capture'])
            self.assertFalse((bank / path).exists());self.assertFalse(source_path.exists())

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
                ('expire', 'time-boundary', '2099-01-01T00:00:00Z', None), ('expire', 'time-boundary', None, None),
                ('retention', 'policy', None, 'revoked')]:
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

    def test_correction_cannot_reuse_unchanged_evidence_or_promote_a_procedural_capture(self):
        for operation in ('no-new-evidence', 'procedure'):
            with self.subTest(operation=operation), World() as w:
                repo, bank = self.fixture(w)
                old, path = self.add(bank, 'existing', evidence=['user:current-confirmed-correction'] if operation == 'no-new-evidence' else ['evidence.md'],
                    kind='procedural-candidate' if operation == 'procedure' else 'semantic-fact')
                value = self.replacement_capture(w, repo, bank, old, path, kind=old['kind'])
                before = self.state(w, repo, bank);r, refused = self.preview(w, repo, value)
                self.assertEqual(r.returncode, 1, refused);self.assertEqual(before, self.state(w, repo, bank))

    def test_unresolved_other_developer_claim_keeps_correction_pending(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'old')
            peer, peer_path = self.add(bank, 'peer', body='# Retry policy\n\nAnother overlapping claim still needs clarification.\n')
            peer_id = str(uuid.uuid4());peer['origin']['developer-id'] = peer_id
            (bank / peer_path).write_text(fixtures.render(peer, '# Retry policy\n\nAnother overlapping claim still needs clarification.\n'))
            value = self.replacement_capture(w, repo, bank, old, path)
            before = self.state(w, repo, bank);r, pending = self.preview(w, repo, value)
            self.assertEqual(r.returncode, 0, pending);self.assertIsNone(pending['proposal'])
            self.assertIn(peer_id, pending['other_developer_ids']);self.assertTrue(pending['recommend_other_developer_clarification'])
            self.assertFalse(pending['messages_sent']);self.assertEqual(before, self.state(w, repo, bank))

    def test_changed_actual_repository_evidence_refuses_automatic_pruning(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'old');value = self.request(bank, old, path)
            value['authority'].update(source_class='verified-repository', evidence=['evidence.md'],
                repository_evidence={'path': 'evidence.md', 'sha256': hashlib.sha256((repo / 'evidence.md').read_bytes()).hexdigest()})
            r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
            (repo / 'evidence.md').write_text('# Later source that no longer supports the earlier observation\n')
            before = self.state(w, repo, bank);r, refused = self.apply(w, plan['proposal'])
            self.assertEqual(r.returncode, 1, refused);self.assertEqual(before, self.state(w, repo, bank))

    def test_a_hidden_other_type_superseder_cannot_silently_defeat_the_new_replacement_version(self):
        with World() as w:
            repo, bank = self.fixture(w);old, p1 = self.add(bank, 'old');replacement, p2 = self.add(bank, 'replacement')
            self.add(bank, 'unrelated-marker', body='# Withdrawal marker\n\nUnrelated operational orientation.\n',
                kind='procedural-candidate', visibility='hidden', **{'supersedes': [{'id': replacement['id'], 'version': 2}]})
            request = {'project_id': self.pid, 'query': '', 'ids': [replacement['id']], 'paths': [], 'context': {},
                'kinds': ['semantic-fact'], 'minimum_trust': 'reviewed', 'limit': 8, 'include_context': False}
            current = self.good(w, 'retrieve.py', '--repo', repo, '--request', self.save(w, request))
            self.assertEqual([row['record']['version'] for row in current['results']], [1])
            value = self.superseding(w, repo, bank, old, p1, replacement, p2)
            before = self.state(w, repo, bank);r, refused = self.preview(w, repo, value)
            self.assertEqual(r.returncode, 1, refused);self.assertEqual(before, self.state(w, repo, bank))

    def test_current_context_only_deletion_does_not_globally_erase_a_record(self):
        with World() as w:
            repo, bank = self.fixture(w);old, path = self.add(bank, 'conditional')
            value = self.request(bank, old, path, 'delete');value['authority'].update(
                basis='explicit-deletion', reason_class='user-request', impact='narrower-contexts-remain')
            value['delete_ids'] = [old['id']];before = self.state(w, repo, bank)
            r, pending = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, pending)
            self.assertIsNone(pending['proposal']);self.assertEqual(pending['disposition'], 'narrowing-required')
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
