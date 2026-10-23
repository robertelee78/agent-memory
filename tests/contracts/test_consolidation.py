"""Public source-bound maintenance, provenance, replay and safe deferral."""
import copy
import hashlib
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
            self.assertIn('Allow three attempts for the same fixture.', fixtures.unpack((bank / p2).read_text())[1])
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
            self.assertEqual(fixtures.unpack(snapshots[0].read_text())[1], request['candidate']['body'])
            self.assertIn({'id': provenance['id'], 'version': 1}, current['derived-from'])
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])
            value = self.compared(w, repo, request['candidate']['metadata'], body, [path], captured=captured)
            before = self.state(w, repo, bank);r, noop = self.preview(w, repo, value)
            self.assertEqual(r.returncode, 0, noop);self.assertEqual(noop['disposition'], 'no-change');self.assertEqual(before, self.state(w, repo, bank))

    def test_contradiction_and_uncertainty_defer_to_detecting_developer_without_writes_or_messages(self):
        for classification in ('contradiction', 'uncertain'):
            with self.subTest(classification=classification), World() as w:
                repo, bank = self.fixture(w)
                peer = str(uuid.uuid4())
                metadata, path = self.add(bank, 'peer')
                metadata['origin']['developer-id'] = peer
                (bank / path).write_text(fixtures.render(metadata, '# Retry policy\n\nThe observed retry cap needs clarification.\n'))
                candidate = fixtures.ComparisonContract.candidate(self)['candidate']
                value = self.compared(w, repo, candidate['metadata'], candidate['body'], [path], overrides={path: classification})
                before = self.state(w, repo, bank);r, pending = self.preview(w, repo, value)
                self.assertEqual(r.returncode, 0, pending);self.assertIsNone(pending['proposal'])
                self.assertEqual(pending['detecting_developer_id'], self.actor);self.assertFalse(pending['messages_sent'])
                if classification == 'contradiction':
                    self.assertEqual(pending['disposition'], 'clarification-required')
                    self.assertEqual(pending['other_developer_ids'], [peer])
                    self.assertTrue(pending['recommend_other_developer_clarification'])
                self.assertEqual(before, self.state(w, repo, bank))

    def test_higher_order_pattern_remains_a_proposal_instead_of_an_invented_decision(self):
        with World() as w:
            repo, bank = self.fixture(w);metadata, path = self.add(bank, 'source')
            candidate = fixtures.ComparisonContract.candidate(self)['candidate']
            value = self.compared(w, repo, candidate['metadata'], '# Retry policy\n\nProposal: generalize the cap to unrelated services.\n',
                                  [path], overrides={path: 'distinct-knowledge'})
            before = self.state(w, repo, bank);r, pending = self.preview(w, repo, value)
            self.assertEqual(r.returncode, 0, pending);self.assertEqual(pending['disposition'], 'proposal-only')
            self.assertIsNone(pending['proposal']);self.assertEqual(before, self.state(w, repo, bank))

    def test_distinct_validity_trust_and_inapplicable_conditions_are_not_silently_folded(self):
        for changes in ({'valid-until': '2099-01-01T00:00:00Z'}, {'trust': 'verified'},
                        {'applicability': 'unknown'}, {'conditions': {'branch': 'another-branch'}}):
            with self.subTest(changes=changes), World() as w:
                repo, bank = self.fixture(w);first, p1 = self.add(bank, 'first')
                if 'conditions' in changes or 'applicability' in changes:
                    first.update(changes);(bank / p1).write_text(fixtures.render(first, '# Retry policy\n\nUse three attempts.\n'))
                    targets = [p1]
                else:
                    _, p2 = self.add(bank, 'second', **changes);targets = [p1, p2]
                value = self.compared(w, repo, first, '# Retry policy\n\nUse at most three attempts.\n', targets)
                before = self.state(w, repo, bank);r, refused = self.preview(w, repo, value)
                self.assertEqual(r.returncode, 1, refused);self.assertEqual(before, self.state(w, repo, bank))

    def test_revoked_or_deleted_lineage_cannot_be_revived_by_maintenance(self):
        for terminal in ('revoked', 'deleted'):
            with self.subTest(terminal=terminal), World() as w:
                repo, bank = self.fixture(w);old, prior = self.add(bank, 'prior', status='revoked', **{'state-reason': 'Explicitly revoked fixture'})
                current, path = self.add(bank, 'current', **{'derived-from': [{'id': old['id'], 'version': 1}]})
                if terminal == 'deleted':
                    tomb = 'history/tombstones/' + old['id'] + '.md';(bank / tomb).parent.mkdir(parents=True)
                    (bank / tomb).write_text(fixtures.render({'id': old['id'], 'scope': self.pid,
                        'deleted-at': '2026-10-22T20:00:00Z', 'deletion-authority': self.actor, 'reason-class': 'user-request'}, ''))
                    (bank / prior).unlink();router = bank / 'MEMORY.md'
                    router.write_text(router.read_text().replace('| [' + prior + '](<' + prior + '>) | Reviewed fixture learning | repository | revoked | When comparing related concepts |',
                        '| [' + tomb + '](<' + tomb + '>) | Terminal marker | repository | deleted | Before any reuse |'))
                    before = self.state(w, repo, bank)
                    request = {'project_id': self.pid, 'candidate': {'metadata': current, 'body': '# Retry policy\n\nUse three attempts.\n'},
                               'searches': ['retry policy', 'attempt cap'], 'read_paths': [path]}
                    r, refused = self.call(w, 'compare_learning.py', 'inspect', '--repo', repo, '--request', self.save(w, request))
                else:
                    value = self.compared(w, repo, current, '# Retry policy\n\nUse three attempts.\n', [path])
                    before = self.state(w, repo, bank);r, refused = self.preview(w, repo, value)
                self.assertEqual(r.returncode, 1, refused);self.assertEqual(before, self.state(w, repo, bank))

    def test_arriving_memory_tampered_delta_and_checkout_changes_require_a_fresh_plan(self):
        for mutation in ('arrival', 'delta', 'branch'):
            with self.subTest(mutation=mutation), World() as w:
                repo, bank = self.fixture(w);metadata, path = self.add(bank, 'existing')
                value = self.compared(w, repo, metadata, '# Retry policy\n\nUse at most three attempts.\n', [path])
                r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
                proposal = plan['proposal']
                if mutation == 'arrival':self.add(bank, 'arrived')
                elif mutation == 'delta':proposal['files']['projectbrief.md'] += '\nUnrelated fabricated change.\n'
                else:w.git('checkout', '-b', 'later-branch', cwd=repo)
                before = self.state(w, repo, bank);r, refused = self.apply(w, proposal)
                self.assertEqual(r.returncode, 1, refused);self.assertEqual(before, self.state(w, repo, bank))

    def test_procedure_capture_remains_pending_separate_explicit_promotion(self):
        with World() as w:
            repo, bank = self.fixture(w);metadata, path = self.add(bank, 'existing', kind='procedural-candidate')
            helper = admission.AdmissionContract('test_user_correction_admits_small_record_and_history_automatically_without_git_changes')
            helper.pack = self.pack;helper.pid = self.pid;helper.actor = self.actor
            request = helper.request();request['candidate']['metadata']['kind'] = 'procedural-candidate'
            captured = helper.capture(w, repo, request)
            self.assertEqual(captured['decision'], 'review-required')
            value = self.compared(w, repo, request['candidate']['metadata'], request['candidate']['body'], [path], captured=captured['capture'])
            before = self.state(w, repo, bank);r, refused = self.preview(w, repo, value)
            self.assertEqual(r.returncode, 1, refused);self.assertFalse(refused['procedure_promotion_performed'])
            self.assertEqual(before, self.state(w, repo, bank))

    def test_interrupted_uncommitted_consolidation_rolls_back_before_fresh_comparison(self):
        with World() as w:
            repo, bank = self.fixture(w);metadata, path = self.add(bank, 'existing')
            value = self.compared(w, repo, metadata, '# Retry policy\n\nUse at most three attempts.\n', [path])
            r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan);p = plan['proposal']
            before = self.state(w, repo, bank)
            stage = 'transaction-after-write:history/events/' + p['operation_id'] + '.md'
            w.env.update(AGENT_MEMORY_FAULT_STAGE=stage, AGENT_MEMORY_FAULT_MODE='exit')
            r = w.run([sys.executable, '-I', self.pack / 'scripts/consolidate_learning.py', 'apply', '--proposal', self.save(w, p)], check=False)
            w.env.pop('AGENT_MEMORY_FAULT_STAGE');w.env.pop('AGENT_MEMORY_FAULT_MODE');self.assertEqual(r.returncode, 86, r.stdout + r.stderr)
            unlock = self.good(w, 'bank_writes.py', 'recover', '--repo', repo)
            self.good(w, 'bank_writes.py', 'recover', '--repo', repo, '--proposal', self.save(w, unlock, 'unlock.json'))
            self.assertEqual(self.call(w, 'bank_transactions.py', 'recover', '--repo', repo, '--mode', 'finish')[0].returncode, 1)
            recovery = self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--mode', 'rollback')
            done = self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--proposal', self.save(w, recovery, 'rollback.json'))
            self.assertTrue(done['verified']);self.assertEqual(done['outcome'], 'rolled-back');self.assertEqual(before, self.state(w, repo, bank))

    def test_recorded_completion_cleans_up_without_reapplying_over_later_memory(self):
        for stage in ('transaction-after-receipt', 'transaction-after-marker-removal'):
            with self.subTest(stage=stage), World() as w:
                repo, bank = self.fixture(w);metadata, path = self.add(bank, 'existing')
                value = self.compared(w, repo, metadata, '# Retry policy\n\nUse at most three attempts.\n', [path])
                r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan);p = plan['proposal']
                w.env.update(AGENT_MEMORY_FAULT_STAGE=stage, AGENT_MEMORY_FAULT_MODE='exit')
                r = w.run([sys.executable, '-I', self.pack / 'scripts/consolidate_learning.py', 'apply', '--proposal', self.save(w, p)], check=False)
                w.env.pop('AGENT_MEMORY_FAULT_STAGE');w.env.pop('AGENT_MEMORY_FAULT_MODE');self.assertEqual(r.returncode, 86, r.stdout + r.stderr)
                unlock = self.good(w, 'bank_writes.py', 'recover', '--repo', repo)
                self.good(w, 'bank_writes.py', 'recover', '--repo', repo, '--proposal', self.save(w, unlock, 'unlock.json'))
                if (bank / '.agent-memory-transaction.md').exists():
                    self.assertEqual(self.call(w, 'bank_transactions.py', 'recover', '--repo', repo, '--mode', 'rollback')[0].returncode, 1)
                    recovery = self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--mode', 'finish')
                    done = self.good(w, 'bank_transactions.py', 'recover', '--repo', repo, '--proposal', self.save(w, recovery, 'finish.json'))
                    self.assertFalse(done['writes_performed'])
                else:
                    (bank / path).write_text((bank / path).read_text() + '\nA later manual correction; preserve it.\n')
                before = self.state(w, repo, bank);r, replay = self.apply(w, p)
                self.assertEqual(r.returncode, 0, replay);self.assertTrue(replay['already_applied'])
                self.assertEqual(before, self.state(w, repo, bank))

    def test_new_capture_evidence_must_still_match_actual_source_before_apply(self):
        with World() as w:
            repo, bank = self.fixture(w);metadata, path = self.add(bank, 'existing')
            helper = admission.AdmissionContract('test_user_correction_admits_small_record_and_history_automatically_without_git_changes')
            helper.pack = self.pack;helper.pid = self.pid;helper.actor = self.actor
            request = helper.request('verified-repository')
            request['repository_evidence'] = {'path': 'evidence.md', 'sha256': hashlib.sha256((repo / 'evidence.md').read_bytes()).hexdigest()}
            captured = helper.capture(w, repo, request)['capture']
            value = self.compared(w, repo, request['candidate']['metadata'], request['candidate']['body'], [path], captured=captured)
            r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
            (repo / 'evidence.md').write_text('# Later changed source evidence\n')
            before = self.state(w, repo, bank);r, refused = self.apply(w, plan['proposal'])
            self.assertEqual(r.returncode, 1, refused);self.assertEqual(before, self.state(w, repo, bank))

    def test_nested_topic_index_and_router_preserve_introductions_and_source_lifecycle(self):
        with World() as w:
            repo, bank = self.fixture(w);first, p1 = self.add(bank, 'primary');second, p2 = self.add(bank, 'duplicate')
            index_path = 'topics/group/index.md'
            routes = [{'path': path, 'purpose': 'Retry source', 'scope': 'repository', 'lifecycle': 'active',
                       'read-when': 'When the retry cap applies'} for path in (p1, p2)]
            request = {'path': index_path, 'project-id': self.pid, 'updated': '2026-10-22T22:00:00Z', 'routes': routes}
            indexed = self.good(w, 'bank_layout.py', 'render-index', '--input', self.save(w, request))['content']
            indexed = indexed.replace('\n## Routes\n', '\nFixture index introduction must survive.\n\n## Routes\n')
            (bank / index_path).parent.mkdir(parents=True);(bank / index_path).write_text(indexed)
            router = bank / 'MEMORY.md';lines = router.read_text().splitlines(keepends=True)
            text = ''.join(line for line in lines if not any(line.startswith('| [' + path + ']') for path in (p1, p2)))
            row = '| [' + index_path + '](<' + index_path + '>) | Retry index | repository | active | When retries matter |\n'
            text = text.replace('\n## Omitted contexts', row + '\n## Omitted contexts')
            router.write_text(text.replace('\n## Routes\n', '\nFixture router introduction must survive.\n\n## Routes\n'))
            value = self.compared(w, repo, first, '# Retry policy\n\nUse at most three attempts.\n', [p1, p2])
            r, plan = self.preview(w, repo, value);self.assertEqual(r.returncode, 0, plan)
            before = self.state(w, repo, bank);r, done = self.apply(w, plan['proposal']);self.assertEqual(r.returncode, 0, done)
            self.assertIn('Fixture index introduction must survive.', (bank / index_path).read_text())
            self.assertIn('Fixture router introduction must survive.', router.read_text())
            self.assertIn('repository | superseded |', (bank / index_path).read_text())
            self.good(w, 'bank_contract.py', 'validate', '--repo', repo)
            self.assertEqual(before[1:], self.state(w, repo, bank)[1:])
