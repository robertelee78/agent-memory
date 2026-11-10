"""Consumer-visible recovery dependencies and source-preserving refusals.

Fixture attestations are controlled test inputs, not independent acceptance evidence.
"""
import copy
import json
import unittest
import uuid

from world import World
import layout_recovery_fixtures as f
from contracts import test_layout_recovery_publication as publication


class RecoverySemanticDependenciesContract(unittest.TestCase):
    def test_discard_requires_distinct_reviewer_and_preserves_pending_source(self):
        with World() as w:
            body = '# Unconfirmed scratch\n\nA disposable observation remains unclassified.\n'
            command, _, repo, bank, _, inspected, applied = f.small_pending(w, body)
            operation = inspected['operation_id']
            unit_id, = applied['pending_unit_ids']
            directory = f.private_directory(w, bank, operation)
            source = next(row for row in inspected['manifest']['sources'] if row['path'] == 'handoff-prompt.md')
            blob = directory / source['blob']
            original = blob.read_bytes()
            head_path = directory / 'ledger-head.json'
            head_bytes = head_path.read_bytes()
            head = json.loads(head_bytes)
            ledger = json.loads((directory / 'ledger' / (str(head['generation']).zfill(8) + '.json')).read_text())
            unit = next(row for row in ledger['units'] if row['unit_id'] == unit_id)
            marker = (bank / '.agent-memory-transaction.md').read_bytes()
            canonical = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            authority = {'schema': 'agent-memory/layout-nonguidance-discard-v1', 'review_id': str(uuid.uuid4()),
                'operation_id': operation, 'unit_id': unit_id, 'manifest_sha256': ledger['manifest_sha256'],
                'unit_map_sha256': ledger['unit_map_sha256'], 'ledger_head_sha256': head['sha256'],
                'source_ranges': unit['source_ranges'], 'author_harness': 'controlled-author',
                'author_session': 'same-session',
                'reviewer_developer_id': inspected['manifest']['identity']['scope_context']['developer_id'],
                'reviewer_harness': 'controlled-author', 'reviewer_session': 'same-session',
                'review_time': inspected['manifest']['inspected_at'], 'non_guidance': True,
                'reason': 'Controlled review of the exact disposable scratch source.', 'decision': 'discard'}
            result = f.call(w, command, 'resolve', '--repo', repo, '--operation', operation, '--unit', unit_id,
                            '--input', f.save(w, {'discard_authority': authority}),
                            '--output', w.inside('refused-discard.json'), expected=1)
            self.assertFalse(result['canonical_writes_performed'])
            self.assertEqual(blob.read_bytes(), original)
            self.assertEqual(original, body.encode())
            self.assertEqual(head_path.read_bytes(), head_bytes)
            self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), marker)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}, canonical)
            status = f.call(w, command, 'status', '--repo', repo, '--operation', operation)
            self.assertIn(unit_id, status['pending_unit_ids'])
            proposal = f.discard_proposal(w, command, repo, bank, inspected, unit_id)
            completed = f.call(w, command, 'resolve', '--repo', repo, '--proposal', proposal)
            self.assertTrue(completed['semantic_recovery_complete'])
            self.assertEqual(blob.read_bytes(), original)

    def test_historical_resolution_retry_does_not_revert_later_context(self):
        with World() as w:
            command, _, repo, bank, _, inspected, applied = f.small_pending(w)
            operation = inspected['operation_id']
            unit_id, = applied['pending_unit_ids']
            directory = f.private_directory(w, bank, operation)
            source = next(row for row in inspected['manifest']['sources'] if row['path'] == 'handoff-prompt.md')
            blob = directory / source['blob']
            original = blob.read_bytes()
            proposal = f.discard_proposal(w, command, repo, bank, inspected, unit_id)
            first = f.call(w, command, 'resolve', '--repo', repo, '--proposal', proposal)
            self.assertTrue(first['semantic_recovery_complete'])
            context = bank / 'activeContext.md'
            context.write_bytes(context.read_bytes() + b'\nLater independently retained context.\n')
            later = context.read_bytes()
            head = (directory / 'ledger-head.json').read_bytes()
            retry = f.call(w, command, 'resolve', '--repo', repo, '--proposal', proposal)
            self.assertTrue(retry['already_applied'])
            self.assertTrue(retry['receipt_is_historical'])
            self.assertFalse(retry['writes_performed'])
            self.assertEqual(context.read_bytes(), later)
            self.assertEqual((directory / 'ledger-head.json').read_bytes(), head)
            self.assertEqual(blob.read_bytes(), original)

    def test_publication_refuses_same_session_review_without_touching_sources(self):
        helper = publication.LayoutPublicationContract()
        with World() as w:
            command, _, repo, bank, _, operation, intended = helper.public_repair(w)
            observed_path = w.inside('publication-observation.json')
            f.call(w, command, 'publish', 'inspect', '--repo', repo, '--operation', operation, '--output', observed_path)
            observation_bytes = observed_path.read_bytes()
            observation = json.loads(observation_bytes)['observation']
            review = helper.publication_review(observation, intended)
            valid_review = copy.deepcopy(review)
            review['reviewer'] = copy.deepcopy(review['author'])
            request = {'schema': 'agent-memory/layout-publication-prepare-v1',
                       'observation_id': observation['observation_id'], 'intended_files': intended, 'review': review}
            before = helper.state(w, repo, bank)
            remote = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            refused = f.call(w, command, 'publish', 'prepare', '--repo', repo, '--operation', operation,
                             '--input', f.save(w, request), '--output', w.inside('self-review-refused.json'), expected=1)
            self.assertFalse(refused['publication_verified'])
            self.assertEqual(helper.state(w, repo, bank), before)
            self.assertEqual(w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout, remote)
            self.assertEqual(observed_path.read_bytes(), observation_bytes)
            request['review'] = valid_review
            prepared = f.call(w, command, 'publish', 'prepare', '--repo', repo, '--operation', operation,
                              '--input', f.save(w, request), '--output', w.inside('independent-review.json'))
            self.assertEqual(prepared['state'], 'prepared')
            self.assertFalse(prepared['publication_verified'])
            self.assertEqual(helper.state(w, repo, bank), before)
            self.assertEqual(w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout, remote)

    def test_transcript_source_cannot_be_copied_or_hidden_as_pending(self):
        for placement in ('intended', 'pending'):
            with self.subTest(placement=placement), World() as w:
                command, source = f.installed(w)
                repo, bank, native = f.memory(w)
                transcript = 'User: Private fixture question.\nAssistant: Private fixture response.\n'
                (bank / 'handoff-prompt.md').write_text(transcript)
                before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
                _, inspected = f.inspect(w, command, repo)
                intended = dict(native)
                if placement == 'intended':
                    intended['activeContext.md'] += '\n' + transcript
                request = f.review_request(w, command, source, repo, inspected, intended)
                directory = f.private_directory(w, bank, inspected['operation_id'])
                row = next(row for row in inspected['manifest']['sources'] if row['path'] == 'handoff-prompt.md')
                blob = directory / row['blob']
                self.assertEqual(blob.read_bytes(), transcript.encode())
                result = f.call(w, command, 'prepare', '--repo', repo, '--operation', inspected['operation_id'],
                                '--input', f.save(w, request), '--output', w.inside('unsafe-proposal.json'), expected=1)
                self.assertFalse(result['canonical_writes_performed'])
                self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}, before)
                self.assertEqual(blob.read_bytes(), transcript.encode())
                self.assertFalse((bank / '.agent-memory-transaction.md').exists())
                self.assertFalse((bank / '.agent-memory-recovery.md').exists())
