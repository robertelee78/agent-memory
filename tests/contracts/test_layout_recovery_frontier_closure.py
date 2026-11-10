"""Public recovery-frontier capacity and disclosure boundaries."""
import copy
import json
from datetime import datetime, timezone
import sys
import unittest
from pathlib import Path
import uuid

from contracts import test_bank_layout, test_comparison, test_governance
from identity_fixtures import bind
from record_fixtures import project_id
from world import World
import layout_recovery_fixtures as f


class RecoveryFrontierClosureContract(unittest.TestCase):
    def test_more_than_64_optional_rows_allow_selective_but_not_broad_comparison(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, _ = f.memory(w)
            helper = test_comparison.ComparisonContract('runTest')
            helper.pid = project_id((bank / 'MEMORY.md').read_text())
            helper.actor = bind(w, repo)['developer_id']
            layout = test_bank_layout.LayoutContract('runTest')
            router = (bank / 'MEMORY.md').read_text()
            paths = []
            for number in range(65):
                _, path = helper.add(bank, 'frontier-%02d' % number,
                                     body='# Capacity fixture\n\ncommonfrontier %s\n' %
                                          ('selectivefrontier' if number == 0 else 'other'))
                paths.append(path)
            files = {'MEMORY.md': router}
            for number, group in enumerate((paths[:33], paths[33:])):
                index = 'topics/frontier-index-%d.md' % number
                routes = [layout.route(path) for path in group]
                (bank / index).write_text(layout.index(w, index, helper.pid, routes))
                layout.root_route(files, layout.route(index))
            (bank / 'MEMORY.md').write_text(files['MEMORY.md'])
            intended = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')}
            (bank / 'handoff-prompt.md').write_text(f.policy_body(0))
            _, inspected = f.inspect(w, command, repo)
            proposal, _ = f.prepare(w, command, source, repo, inspected, intended)
            applied = f.call(w, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertEqual(len(applied['pending_unit_ids']), 1)
            before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            request = helper.candidate()
            request['searches'] = ['selectivefrontier', 'unmatchedsentinel']
            private = w.inside('selective-frontier.json')
            selected = w.run([command, 'compare', 'inspect', '--repo', repo, '--request', f.save(w, request),
                              '--private-output', private], check=False, timeout=60)
            self.assertEqual(selected.returncode, 0, selected.stdout + selected.stderr)
            view = json.loads(private.read_text())
            self.assertEqual([row['path'] for row in view['matches']], [paths[0]])
            self.assertEqual({row['unit_id'] for row in view['pending_matches']}, set(applied['pending_unit_ids']))
            request['searches'] = ['commonfrontier', 'unmatchedsentinel']
            broad = w.run([command, 'compare', 'inspect', '--repo', repo, '--request', f.save(w, request),
                           '--private-output', w.inside('broad-frontier.json')], check=False, timeout=60)
            self.assertNotEqual(broad.returncode, 0)
            self.assertFalse(json.loads(broad.stdout)['comparison_read_complete'])
            self.assertEqual(before, {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()})
            self.assertEqual(f.call(w, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])['pending_unit_ids'], applied['pending_unit_ids'])

    def test_unsafe_or_oversized_indivisible_source_cannot_become_a_pending_frontier(self):
        for body in ('# Unsafe\n\npassword=PRIVATE_FRONTIER_SECRET_987654321\n', 'x' * 16385):
            with self.subTest(case='unsafe' if body.startswith('#') else 'oversized'), World() as w:
                command, source = f.installed(w)
                repo, bank, native = f.memory(w)
                original = bank / 'handoff-prompt.md'
                original.write_text(body)
                _, inspected = f.inspect(w, command, repo)
                request = f.review_request(w, command, source, repo, inspected, native)
                before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
                output = w.inside('refused-frontier-proposal.json')
                refused = w.run([command, 'bank', 'recover-layout', 'prepare', '--repo', repo,
                                 '--operation', inspected['operation_id'], '--input', f.save(w, request),
                                 '--output', output], check=False, timeout=60)
                self.assertNotEqual(refused.returncode, 0)
                self.assertNotIn('PRIVATE_FRONTIER_SECRET_987654321', refused.stdout + refused.stderr)
                self.assertFalse(output.exists())
                self.assertEqual(original.read_text(), body)
                self.assertEqual(before, {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()})

    def test_private_comparison_does_not_put_original_hash_or_ranges_in_envelope_or_audit(self):
        body = '# Confidential fixture\n\nUnique private source evidence 874209 remains pending.\n'
        with World() as w:
            command, _, repo, bank, _, inspected, applied = f.small_pending(w, body)
            helper = test_comparison.ComparisonContract('runTest')
            helper.pid = inspected['manifest']['project_id']
            helper.actor = inspected['manifest']['identity']['scope_context']['developer_id']
            private = w.inside('private-disclosure.json')
            result = w.run([command, 'compare', 'inspect', '--repo', repo,
                            '--request', f.save(w, helper.candidate()), '--private-output', private],
                           check=False, timeout=60)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            view = json.loads(private.read_text())
            row, = view['pending_matches']
            self.assertEqual(row['body'], body)
            self.assertEqual(row['unit_id'], applied['pending_unit_ids'][0])
            audit = Path(view['audit']['path']).read_text()
            original = next(item for item in inspected['manifest']['sources'] if item['path'] == 'handoff-prompt.md')
            for secret in (body.strip(), original['sha256'], row['source_ranges'][0]['source_id'], 'source_ranges'):
                self.assertNotIn(secret, result.stdout + result.stderr)
                self.assertNotIn(secret, audit)

    def test_approval_heading_and_unresolved_version_conflict_remain_private_pending_source(self):
        body = ('# Approved policy\n\n'
                'Version 1 allowed the synthetic action. Version 2 may prohibit it; '
                'the current authority and contradiction are unknown.\n')
        with World() as w:
            command, _, repo, bank, _, inspected, applied = f.small_pending(w, body)
            helper = test_comparison.ComparisonContract('runTest')
            helper.pid = inspected['manifest']['project_id']
            helper.actor = inspected['manifest']['identity']['scope_context']['developer_id']
            private = w.inside('unresolved-policy.json')
            result = w.run([command, 'compare', 'inspect', '--repo', repo,
                            '--request', f.save(w, helper.candidate()), '--private-output', private],
                           check=False, timeout=60)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            view = json.loads(private.read_text())
            row, = view['pending_matches']
            self.assertEqual(row['body'], body)
            self.assertEqual(row['unit_id'], applied['pending_unit_ids'][0])
            self.assertFalse(row['source_assertions_verified'])
            self.assertEqual(row['selection_class'], 'comparison-only-not-current-guidance')
            self.assertNotIn(body.strip(), result.stdout + result.stderr)
            self.assertFalse((bank / 'handoff-prompt.md').exists())
            status = f.call(w, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
            self.assertFalse(status['semantic_recovery_complete'])
            self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])

    def test_reservation_allows_full_credit_proof_and_rejects_unreviewable_frontier(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, native = f.memory(w, oversized=True, handoff=True)
            (repo / 'synthetic-policy.md').write_text(
                '# Independent synthetic source\n\n' + '\n\n'.join(f.POLICIES) + '\n')
            w.git('add', 'synthetic-policy.md', cwd=repo)
            w.git('commit', '-m', 'Independent synthetic recovery evidence', cwd=repo)
            _, inspected = f.inspect(w, command, repo)
            layout, prepared = f.prepare(w, command, source, repo, inspected, native)
            reservation = prepared['proposal']['reservation']
            self.assertLessEqual(reservation['proof_bytes'], 65536)
            self.assertGreaterEqual(reservation['rows'], 8)
            applied = f.call(w, command, 'apply', '--repo', repo, '--proposal', layout)
            self.assertEqual(len(applied['pending_unit_ids']), 8)
            admission, transaction, _ = f.admission_proposal(
                w, command, source, repo, bank, inspected, full=True, policy_number=None)
            proof = transaction['admission_evidence']['comparison_proof']
            claims = [row['coverage_claim'] for row in proof['judgment']['pending_decisions']]
            self.assertEqual(len(claims), 8)
            self.assertTrue(all(claim['coverage'] == 'full' for claim in claims))
            self.assertLessEqual(len(f.encoded(proof)), 65536)
            result = f.public(w, command, 'admission', 'apply', '--proposal', admission)
            self.assertEqual(result['operation_id'], transaction['operation_id'])
            self.assertTrue(f.call(w, command, 'status', '--repo', repo,
                                   '--operation', inspected['operation_id'])['semantic_recovery_complete'])

        with World() as w:
            command, source = f.installed(w)
            repo, bank, native = f.memory(w)
            (bank / 'handoff-prompt.md').write_text('abcdefghijklmnop')
            _, inspected = f.inspect(w, command, repo)
            request = f.review_request(w, command, source, repo, inspected, native)
            pending = next(row for row in request['unit_map']['units']
                           if row['disposition'] == 'pending-admission')
            units = [row for row in request['unit_map']['units'] if row is not pending]
            for number in range(16):
                unit = copy.deepcopy(pending)
                unit['unit_id'] = str(uuid.uuid4())
                unit['source_ranges'][0].update(start=number, end=number + 1)
                units.append(unit)
            request['unit_map']['units'] = units
            request['review']['unit_map_sha256'] = f.sha(f.encoded(request['unit_map']))
            request['review']['unit_decisions'] = [
                {'unit_id': unit['unit_id'], 'disposition': unit['disposition'], 'rationale': unit['reason']}
                for unit in units]
            before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            refused = w.run([command, 'bank', 'recover-layout', 'prepare', '--repo', repo,
                             '--operation', inspected['operation_id'], '--input', f.save(w, request),
                             '--output', w.inside('over-reserved.json')], check=False, timeout=60)
            self.assertNotEqual(refused.returncode, 0)
            self.assertEqual(before, {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()})

    def test_reservation_transition_accepts_last_complete_frontier_and_refuses_next(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, native = f.memory(w)
            raw = f.policy_body(0).encode()
            (repo / 'synthetic-policy.md').write_bytes(raw)
            w.git('add', 'synthetic-policy.md', cwd=repo)
            w.git('commit', '-m', 'Independent reservation evidence', cwd=repo)
            (bank / 'handoff-prompt.md').write_bytes(raw * 16)
            accepted = []
            last = None
            rejected = None
            # Each inspection owns an immutable unit map; the canonical source
            # and World remain unchanged while adjacent partitions are reviewed.
            for count in range(8, 17):
                _, inspected = f.inspect(w, command, repo)
                request = f.review_request(w, command, source, repo, inspected, native)
                pending = next(row for row in request['unit_map']['units']
                               if row['disposition'] == 'pending-admission')
                units = [row for row in request['unit_map']['units'] if row is not pending]
                for number in range(count):
                    unit = copy.deepcopy(pending)
                    unit['unit_id'] = str(uuid.uuid4())
                    unit['source_ranges'][0].update(
                        start=number * len(raw),
                        end=(number + 1 if number < count - 1 else 16) * len(raw))
                    units.append(unit)
                request['unit_map']['units'] = units
                request['review']['unit_map_sha256'] = f.sha(f.encoded(request['unit_map']))
                request['review']['unit_decisions'] = [
                    {'unit_id': unit['unit_id'], 'disposition': unit['disposition'], 'rationale': unit['reason']}
                    for unit in units]
                output = w.inside('reservation-transition-%d.json' % count)
                result = w.run([command, 'bank', 'recover-layout', 'prepare', '--repo', repo,
                                '--operation', inspected['operation_id'], '--input', f.save(w, request),
                                '--output', output], check=False, timeout=60)
                if result.returncode:
                    rejected = count
                    self.assertFalse(output.exists())
                    self.assertEqual((bank / 'handoff-prompt.md').read_bytes(), raw * 16)
                    self.assertFalse(json.loads(result.stdout)['canonical_mutation_may_have_occurred'])
                    break
                prepared = json.loads(output.read_text())['proposal']
                self.assertEqual(len(prepared['pending_units']), count)
                self.assertLessEqual(prepared['reservation']['proof_bytes'], 65536)
                accepted.append(count)
                last = (output, inspected)
            self.assertTrue(accepted)
            self.assertIsNotNone(rejected)
            self.assertEqual(rejected, accepted[-1] + 1)
            output, inspected = last
            applied = f.call(w, command, 'apply', '--repo', repo, '--proposal', output)
            self.assertEqual(len(applied['pending_unit_ids']), accepted[-1])
            admission, transaction, _ = f.admission_proposal(
                w, command, source, repo, bank, inspected, full=True, policy_number=None)
            proof = transaction['admission_evidence']['comparison_proof']
            claims = [decision['coverage_claim'] for decision in proof['judgment']['pending_decisions']]
            self.assertEqual(len(claims), accepted[-1])
            self.assertTrue(all(claim['coverage'] == 'full' for claim in claims))
            self.assertLessEqual(len(f.encoded(proof)), 65536)
            f.public(w, command, 'admission', 'apply', '--proposal', admission)
            self.assertTrue(f.call(w, command, 'status', '--repo', repo,
                                   '--operation', inspected['operation_id'])['semantic_recovery_complete'])

    def test_pending_frontier_is_included_in_update_consolidation_and_governance_routes(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, _ = f.memory(w)
            helper = test_comparison.ComparisonContract('runTest')
            helper.pid = project_id((bank / 'MEMORY.md').read_text())
            helper.actor = bind(w, repo)['developer_id']
            old, first = helper.add(bank, 'route-first')
            _, second = helper.add(bank, 'route-second')
            native = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')}
            (bank / 'handoff-prompt.md').write_text(f.policy_body(0))
            _, inspected = f.inspect(w, command, repo)
            proposal, _ = f.prepare(w, command, source, repo, inspected, native)
            applied = f.call(w, command, 'apply', '--repo', repo, '--proposal', proposal)
            directory = f.private_directory(w, bank, inspected['operation_id'])
            head = json.loads((directory / 'ledger-head.json').read_text())
            ledger = json.loads((directory / 'ledger/00000000.json').read_text())
            before = {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()}
            for disposition, paths in (('update-existing', [first]),
                                       ('consolidate-existing', [first, second]),
                                       ('governance-required', [first])):
                with self.subTest(disposition=disposition):
                    request = helper.candidate()
                    request['searches'] = ['unmatched-lexical-one', 'unmatched-lexical-two']
                    request['read_paths'] = paths
                    if disposition == 'governance-required':
                        metadata = copy.deepcopy(old)
                        metadata['conditions'] = {'branch': 'older/release'}
                        request['candidate'] = {'metadata': metadata,
                                                'body': test_comparison.unpack((bank / first).read_text())[1]}
                    request_file = f.save(w, request)
                    private = w.inside('route-inspect-' + disposition + '.json')
                    f.public(w, command, 'compare', 'inspect', '--repo', repo, '--request', request_file,
                             '--private-output', private)
                    view = json.loads(private.read_text())
                    self.assertEqual({row['path'] for row in view['matches']}, set(paths))
                    self.assertEqual({row['unit_id'] for row in view['pending_matches']}, set(applied['pending_unit_ids']))
                    classification = 'scope-correction' if disposition == 'governance-required' else 'equivalent'
                    decisions = [{'path': row['path'], 'classification': classification,
                                  'rationale': 'Independent review retains the observed conditions.'} for row in view['matches']]
                    pending = [{'unit_id': row['unit_id'], 'classification': 'distinct-knowledge',
                                'rationale': 'Unconfirmed source remains pending without coverage credit.',
                                'coverage_claim': {'unit_id': row['unit_id'], 'coverage': 'none',
                                                   'covered_source_ranges': [], 'target_record_refs': []}}
                               for row in view['pending_matches']]
                    judgment = {'observation': view['observation'], 'decisions': decisions,
                                'pending_decisions': pending, 'rationale': 'Complete source-bound review.'}
                    targets = [{'path': row['path'], 'sha256': row['sha256'], 'id': row['metadata']['id'],
                                'version': row['metadata']['version'], 'required_preserved_metadata': row['metadata'],
                                'scope_chain': row['scope_chain']} for row in view['matches']]
                    proof = {'schema': 'agent-memory/comparison-proof-v2', 'judgment': judgment,
                             'disposition': disposition, 'targets': targets}
                    review = {'schema': 'agent-memory/layout-coverage-review-v1', 'review_id': str(uuid.uuid4()),
                              'operation_id': inspected['operation_id'], 'reviewer_developer_id': helper.actor,
                              'reviewer_harness': 'route-reviewer', 'reviewer_session': str(uuid.uuid4()),
                              'review_time': inspected['manifest']['inspected_at'],
                              'manifest_sha256': ledger['manifest_sha256'], 'unit_map_sha256': ledger['unit_map_sha256'],
                              'ledger_head_sha256': head['sha256'], 'comparison_proof_input_sha256': f.sha(f.encoded(proof)),
                              'intended_output_sha256': f.sha(f.encoded(native)),
                              'claims': [row['coverage_claim'] for row in pending],
                              'rationale': 'Independent complete review; no pending coverage claimed.', 'decision': 'approve'}
                    reviewed = w.inside('route-review-' + disposition + '.json')
                    f.public(w, command, 'compare', 'review', '--repo', repo, '--request', request_file,
                             '--judgment', f.save(w, judgment), '--coverage-review', f.save(w, review),
                             '--private-output', reviewed)
                    value = json.loads(reviewed.read_text())
                    self.assertEqual(value['disposition'], disposition)
                    self.assertEqual({target['path'] for target in value['proof']['targets']}, set(paths))
                    self.assertEqual([row['coverage_claim']['coverage'] for row in value['proof']['judgment']['pending_decisions']], ['none'])
            self.assertEqual(before, {p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*') if p.is_file()})
            self.assertEqual(f.call(w, command, 'status', '--repo', repo,
                                    '--operation', inspected['operation_id'])['pending_unit_ids'], applied['pending_unit_ids'])

    def test_partial_claims_need_two_receipts_and_independent_complete_union(self):
        with World() as w:
            command, source, repo, bank, _, inspected, applied = f.small_pending(w)
            w.git('add', 'synthetic-policy.md', cwd=repo)
            w.git('commit', '-m', 'Independent partial coverage evidence', cwd=repo)
            operation = inspected['operation_id']
            unit_id, = applied['pending_unit_ids']
            directory = f.private_directory(w, bank, operation)
            transactions, receipts, claims = [], [], []
            for half in (0, 1):
                _, transaction, target = f.admission_proposal(
                    w, command, source, repo, bank, inspected, full=False)
                evidence = transaction['admission_evidence']
                request = evidence['comparison_request']
                proof = copy.deepcopy(evidence['comparison_proof'])
                judgment = proof['judgment']
                judgment['observation']['recovery_frontier']['review_digest'] = None
                decision, = judgment['pending_decisions']
                original = next(row for row in json.loads((directory / 'ledger/00000000.json').read_text())['units']
                                if row['unit_id'] == unit_id)['source_ranges'][0]
                midpoint = (original['start'] + original['end']) // 2
                piece = dict(original)
                piece['start'], piece['end'] = ((original['start'], midpoint) if half == 0
                                                else (midpoint, original['end']))
                metadata = request['candidate']['metadata']
                claim = {'unit_id': unit_id, 'coverage': 'partial', 'covered_source_ranges': [piece],
                         'target_record_refs': [{'id': metadata['id'], 'version': 1, 'path': target,
                                                 'resulting_sha256': f.sha(transaction['files'][target].encode())}]}
                decision['coverage_claim'] = claim
                head = json.loads((directory / 'ledger-head.json').read_text())
                ledger = json.loads((directory / 'ledger' / (str(head['generation']).zfill(8) + '.json')).read_text())
                review = {'schema': 'agent-memory/layout-coverage-review-v1', 'review_id': str(uuid.uuid4()),
                          'operation_id': operation, 'reviewer_developer_id': inspected['manifest']['identity']['scope_context']['developer_id'],
                          'reviewer_harness': 'partial-reviewer', 'reviewer_session': str(uuid.uuid4()),
                          'review_time': inspected['manifest']['inspected_at'], 'manifest_sha256': ledger['manifest_sha256'],
                          'unit_map_sha256': ledger['unit_map_sha256'], 'ledger_head_sha256': head['sha256'],
                          'comparison_proof_input_sha256': f.sha(f.encoded(proof)),
                          'intended_output_sha256': f.sha(f.encoded(transaction['files'])), 'claims': [claim],
                          'rationale': 'Independent review of this exact partial source range and target.',
                          'decision': 'approve'}
                request_file = f.save(w, request)
                reviewed = w.inside('partial-proof-%d.json' % half)
                f.public(w, command, 'compare', 'review', '--repo', repo, '--request', request_file,
                         '--judgment', f.save(w, judgment), '--coverage-review', f.save(w, review),
                         '--private-output', reviewed)
                proposal = w.inside('partial-admission-%d.json' % half)
                f.public(w, command, 'admission', 'preview', '--repo', repo,
                         '--capture', f.save(w, evidence['capture']), '--request', request_file,
                         '--proof', reviewed, '--operation-id', transaction['operation_id'],
                         '--recorded-at', evidence['recorded_at'], '--private-output', proposal)
                actual = json.loads(proposal.read_text())['proposal']
                result = f.public(w, command, 'admission', 'apply', '--proposal', proposal)
                receipt = json.loads(Path(result['receipt_path']).read_text())
                transactions.append(actual)
                receipts.append({'operation_id': actual['operation_id'], 'receipt_sha256': f.sha(f.encoded(receipt)),
                                 'proof_sha256': f.sha(f.encoded(actual['admission_evidence']['comparison_proof']))})
                claims.append(claim)
                status = f.call(w, command, 'status', '--repo', repo, '--operation', operation)
                self.assertFalse(status['semantic_recovery_complete'])
                self.assertEqual(status['pending_unit_ids'], [unit_id])
                f.call(w, command, 'resolve', '--repo', repo, '--operation', operation, '--unit', unit_id,
                       '--input', f.save(w, {'transaction_proposal': actual}),
                       '--output', w.inside('partial-single-%d.json' % half), expected=1)
            head = json.loads((directory / 'ledger-head.json').read_text())
            ledger = json.loads((directory / 'ledger' / (str(head['generation']).zfill(8) + '.json')).read_text())
            union = {'schema': 'agent-memory/layout-coverage-review-v1', 'review_id': str(uuid.uuid4()),
                     'operation_id': operation, 'reviewer_developer_id': inspected['manifest']['identity']['scope_context']['developer_id'],
                     'reviewer_harness': 'union-reviewer', 'reviewer_session': str(uuid.uuid4()),
                     'review_time': inspected['manifest']['inspected_at'], 'manifest_sha256': ledger['manifest_sha256'],
                     'unit_map_sha256': ledger['unit_map_sha256'], 'ledger_head_sha256': head['sha256'],
                     'comparison_proof_input_sha256': f.sha(f.encoded(receipts)),
                     'intended_output_sha256': f.sha(f.encoded(transactions[-1]['files'])),
                     'claims': claims, 'rationale': 'Independent review of both complete nonoverlapping committed fragments.',
                     'decision': 'approve'}
            terminal = {'transaction_proposals': transactions, 'coverage_review': union}
            resolution = w.inside('union-resolution.json')
            f.call(w, command, 'resolve', '--repo', repo, '--operation', operation, '--unit', unit_id,
                   '--input', f.save(w, terminal), '--output', resolution)
            finished = f.call(w, command, 'resolve', '--repo', repo, '--proposal', resolution)
            self.assertTrue(finished['semantic_recovery_complete'])
            self.assertEqual(f.call(w, command, 'status', '--repo', repo,
                                    '--operation', operation)['pending_unit_ids'], [])

    def test_pending_repair_preserves_both_canonical_versions_for_comparison(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, _ = f.memory(w)
            helper = test_comparison.ComparisonContract('runTest')
            helper.pid = project_id((bank / 'MEMORY.md').read_text())
            helper.actor = bind(w, repo)['developer_id']
            old, old_path = helper.add(bank, 'historical-version', status='superseded',
                **{'state-reason': 'The independently reviewed correction retains this historical version.'})
            current, current_path = helper.add(bank, 'current-version', id=old['id'], version=2,
                body='# Revised policy\n\nThe corrected fixture allows four attempts.\n',
                corrects=[{'id': old['id'], 'version': old['version']}])
            native = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')}
            (bank / 'handoff-prompt.md').write_text(
                '# Approved policy\n\nThe source does not resolve which version applies.\n')
            _, inspected = f.inspect(w, command, repo)
            proposal, _ = f.prepare(w, command, source, repo, inspected, native)
            applied = f.call(w, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertEqual((bank / old_path).read_text(), native[old_path])
            self.assertEqual((bank / current_path).read_text(), native[current_path])
            request = helper.candidate()
            request['searches'] = ['unrelated-boundary-one', 'unrelated-boundary-two']
            request['read_paths'] = [old_path, current_path]
            private = w.inside('version-history-comparison.json')
            f.public(w, command, 'compare', 'inspect', '--repo', repo,
                     '--request', f.save(w, request), '--private-output', private)
            view = json.loads(private.read_text())
            versions = {(row['metadata']['id'], row['metadata']['version'], row['lifecycle'])
                        for row in view['matches'] if row['metadata']}
            self.assertEqual(versions, {(old['id'], 1, 'superseded'), (current['id'], 2, 'active')})
            self.assertEqual({row['unit_id'] for row in view['pending_matches']}, set(applied['pending_unit_ids']))
            self.assertFalse(f.call(w, command, 'status', '--repo', repo,
                                    '--operation', inspected['operation_id'])['semantic_recovery_complete'])

    def _consumer_flow(self, route, *, pending):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, _ = f.memory(w)
            helper = test_comparison.ComparisonContract('runTest')
            helper.pid = project_id((bank / 'MEMORY.md').read_text())
            helper.actor = bind(w, repo)['developer_id']
            original_body = f.policy_body(0)
            old, first = helper.add(
                bank, 'consumer-first',
                body=('# Synthetic policy 1\n\nDiscard queued items after failure.\n'
                      if route == 'governance' else original_body))
            replacement, second = helper.add(bank, 'consumer-second', body=original_body)
            native = {p.relative_to(bank).as_posix(): p.read_text() for p in bank.rglob('*.md')}
            inspected = applied = directory = head = ledger = None
            operation_id = str(uuid.uuid4())
            stamp = datetime.now(timezone.utc).isoformat()
            if pending:
                (bank / 'handoff-prompt.md').write_text(original_body)
                _, inspected = f.inspect(w, command, repo)
                layout, _ = f.prepare(w, command, source, repo, inspected, native)
                applied = f.call(w, command, 'apply', '--repo', repo, '--proposal', layout)
                directory = f.private_directory(w, bank, inspected['operation_id'])
                head = json.loads((directory / 'ledger-head.json').read_text())
                ledger = json.loads((directory / 'ledger/00000000.json').read_text())

            request = helper.candidate()
            request['candidate'] = {
                'metadata': replacement if route == 'governance' else old,
                'body': original_body if route == 'governance' else original_body + '\n',
            }
            request['searches'] = ['unmatched-consumer-one', 'unmatched-consumer-two']
            request['read_paths'] = [first, second] if route != 'update' else [first]
            request_file = f.save(w, request)
            private = w.inside('consumer-inspection.json')
            inspected_view = f.public(w, command, 'compare', 'inspect', '--repo', repo,
                                      '--request', request_file, '--private-output', private)
            view = json.loads(private.read_text()) if pending else inspected_view
            self.assertEqual({row['path'] for row in view['matches']}, set(request['read_paths']))
            if pending:
                self.assertEqual({row['unit_id'] for row in view['pending_matches']},
                                 set(applied['pending_unit_ids']))
            else:
                self.assertEqual(view['pending_matches'], [])
            decisions = [
                {'path': row['path'],
                 'classification': 'contradiction' if route == 'governance' and row['path'] == first else 'equivalent',
                 'rationale': 'The inspected source meaning and conditions were reviewed independently.'}
                for row in view['matches']
            ]
            pending_decisions = [
                {'unit_id': row['unit_id'], 'classification': 'distinct-knowledge',
                 'rationale': 'Review the complete original against the exact resulting record.',
                 'coverage_claim': {'unit_id': row['unit_id'], 'coverage': 'none',
                                    'covered_source_ranges': [], 'target_record_refs': []}}
                for row in view['pending_matches']
            ]
            judgment = {'observation': view['observation'], 'decisions': decisions,
                        'pending_decisions': pending_decisions, 'rationale': 'Complete independent comparison.'}
            targets = [{'path': row['path'], 'sha256': row['sha256'], 'id': row['metadata']['id'],
                        'version': row['metadata']['version'], 'required_preserved_metadata': row['metadata'],
                        'scope_chain': row['scope_chain']}
                       for row in view['matches']
                       if not (route == 'governance' and row['path'] == first)]
            disposition = ('clarification-required' if route == 'governance' else
                           'consolidate-existing' if route == 'consolidate' else 'update-existing')

            def review(output_hash=None):
                arguments = ['compare', 'review', '--repo', repo, '--request', request_file,
                             '--judgment', f.save(w, judgment)]
                if pending:
                    proof = {'schema': 'agent-memory/comparison-proof-v2', 'judgment': judgment,
                             'disposition': disposition, 'targets': targets}
                    coverage = {
                        'schema': 'agent-memory/layout-coverage-review-v1', 'review_id': str(uuid.uuid4()),
                        'operation_id': inspected['operation_id'], 'reviewer_developer_id': helper.actor,
                        'reviewer_harness': 'consumer-independent-reviewer', 'reviewer_session': str(uuid.uuid4()),
                        'review_time': inspected['manifest']['inspected_at'],
                        'manifest_sha256': ledger['manifest_sha256'], 'unit_map_sha256': ledger['unit_map_sha256'],
                        'ledger_head_sha256': head['sha256'], 'comparison_proof_input_sha256': f.sha(f.encoded(proof)),
                        'intended_output_sha256': output_hash,
                        'claims': [row['coverage_claim'] for row in pending_decisions],
                        'rationale': 'Independent review of complete intended output and original source.',
                        'decision': 'approve',
                    }
                    arguments += ['--coverage-review', f.save(w, coverage)]
                output = w.inside('consumer-reviewed-' + str(uuid.uuid4()) + '.json')
                public_review = f.public(w, command, *arguments, '--private-output', output)
                value = json.loads(output.read_text()) if pending else public_review
                self.assertEqual(value['disposition'], disposition)
                return value['proof']

            reviewed = review(f.sha(f.encoded(native)) if pending else None)
            if route == 'governance':
                governance = test_governance.GovernanceContract('runTest')
                governance.pid, governance.actor = helper.pid, helper.actor
                consumer_input = governance.request(bank, old, first, 'supersede')
                context_names = w.run([sys.executable, '-I', '-B', '-c',
                    "import json,sys;sys.path.insert(0,sys.argv[1]);"
                    "from agent_memory import bank_contract;print(json.dumps(bank_contract.CONTEXTS))",
                    source / 'scripts'], check=False, timeout=30)
                self.assertEqual(context_names.returncode, 0, context_names.stderr)
                consumer_input['context_review'] = {
                    path: f.sha((bank / path).read_bytes())
                    for path in ('MEMORY.md', *json.loads(context_names.stdout))
                    if (bank / path).is_file()}
                consumer_input['replacement'] = {
                    'path': second, 'id': replacement['id'], 'version': replacement['version'],
                    'sha256': f.sha((bank / second).read_bytes()),
                }
                consumer_input['comparison'] = {'request': request, 'proof': reviewed}
            else:
                consumer_input = {'comparison_request': request, 'comparison_proof': reviewed,
                                  'capture': None, 'context': {}}

            if pending:
                stamp = inspected['manifest']['inspected_at']
                if route == 'governance':
                    build = ("import json,sys;sys.path.insert(0,sys.argv[1]);"
                             "from agent_memory import governance;"
                             "r=json.load(open(sys.argv[3]));o=governance.observe(sys.argv[2],r);"
                             "files,deferred=governance.desired(o,r,sys.argv[4],sys.argv[5]);"
                             "print(json.dumps({'files':files,'deferred':deferred}))")
                    args = [source / 'scripts', repo, f.save(w, consumer_input), operation_id, stamp]
                    result_path, result_id = second, replacement['id']
                else:
                    evidence = {'comparison_request': request, 'comparison_proof': reviewed, 'recorded_at': stamp}
                    plan = {'primary': next(row for row in targets if row['path'] == first), 'targets': targets,
                            'qualified': {'scope_context': {'developer_id': helper.actor}}}
                    build = ("import json,sys;from pathlib import Path;sys.path.insert(0,sys.argv[1]);"
                             "from agent_memory import consolidation,bank_transactions;"
                             "v=json.load(open(sys.argv[3]));"
                             "files=consolidation.build(bank_transactions.files_at(Path(sys.argv[2])),"
                             "v['evidence'],sys.argv[4],v['plan'],None);"
                             "print(json.dumps({'files':files,'deferred':None}))")
                    args = [source / 'scripts', bank, f.save(w, {'evidence': evidence, 'plan': plan}), operation_id]
                    result_path, result_id = first, old['id']
                built = w.run([sys.executable, '-I', '-B', '-c', build, *args], check=False, timeout=60)
                self.assertEqual(built.returncode, 0, built.stdout + built.stderr)
                intended = json.loads(built.stdout)
                self.assertIsNone(intended['deferred'])
                files = intended['files']
                self.assertIsNotNone(files)
                resulting_metadata, resulting_body = test_comparison.unpack(files[result_path])
                self.assertEqual(resulting_metadata['id'], result_id)
                self.assertEqual(resulting_metadata['version'], 2)
                self.assertIn(f.POLICIES[0], resulting_body)
                for row, decision in zip(view['pending_matches'], pending_decisions):
                    decision['coverage_claim'] = {
                        'unit_id': row['unit_id'], 'coverage': 'full', 'covered_source_ranges': row['source_ranges'],
                        'target_record_refs': [{'id': result_id, 'version': 2, 'path': result_path,
                                                'resulting_sha256': f.sha(files[result_path].encode())}],
                    }
                reviewed = review(f.sha(f.encoded(files)))
                if route == 'governance':
                    consumer_input['comparison']['proof'] = reviewed
                else:
                    consumer_input['comparison_proof'] = reviewed

            preview = w.inside('consumer-preview.json')
            if route == 'governance':
                arguments = ['govern', 'preview', '--repo', repo, '--request', f.save(w, consumer_input)]
            else:
                arguments = ['compact', 'preview', '--repo', repo, '--input', f.save(w, consumer_input)]
            if pending:
                # A changed valid seed produces different event identities and
                # must not reuse the independent exact-output approval.
                changed = w.run([command, *arguments, '--operation-id', str(uuid.uuid4()),
                                 '--recorded-at', stamp, '--private-output',
                                 w.inside('consumer-changed-seed.json')], check=False, timeout=60)
                self.assertNotEqual(changed.returncode, 0)
                self.assertFalse(json.loads(changed.stdout)['canonical_writes_performed'])
                self.assertEqual({p.relative_to(bank).as_posix(): p.read_text()
                                  for p in bank.rglob('*.md')
                                  if not p.name.startswith('.agent-memory')}, native)
            arguments += ['--operation-id', operation_id, '--recorded-at', stamp]
            result = w.run([command, *arguments, '--private-output', preview], check=False, timeout=60)
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            preview_value = json.loads(preview.read_text()) if pending else json.loads(result.stdout)
            proposal = preview_value['proposal']
            self.assertIsNotNone(proposal)
            if not pending:
                preview = f.save(w, proposal)
            if pending:
                self.assertEqual(proposal['files'], files)
            completed = f.public(w, command, 'govern' if route == 'governance' else 'compact',
                                 'apply', '--proposal', preview)
            self.assertTrue(completed['verified'])
            receipt = json.loads(Path(completed['receipt_path']).read_text())
            self.assertEqual(receipt['operation_id'], proposal['operation_id'])
            self.assertEqual(receipt['request_sha256'], f.sha(f.encoded(proposal)))
            self.assertEqual(receipt['outcome'], 'finished')
            self.assertTrue(receipt['verified'])
            self.assertEqual(receipt['resulting_hashes'],
                             {path: f.sha(body.encode()) for path, body in proposal['files'].items()})
            for path, body in proposal['files'].items():
                self.assertEqual((bank / path).read_text(), body)
            current, body = test_comparison.unpack((bank / (second if route == 'governance' else first)).read_text())
            self.assertEqual(current['version'], 2)
            self.assertIn(f.POLICIES[0], body)
            if route != 'update':
                inactive, _ = test_comparison.unpack((bank / (first if route == 'governance' else second)).read_text())
                self.assertEqual(inactive['status'], 'superseded')
            if pending:
                unit_id, = applied['pending_unit_ids']
                status = f.call(w, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
                self.assertTrue(status['semantic_recovery_complete'])
                self.assertEqual(status['pending_unit_ids'], [])
                resolved_head = json.loads((directory / 'ledger-head.json').read_text())
                resolved = json.loads((directory / 'ledger' /
                    (str(resolved_head['generation']).zfill(8) + '.json')).read_text())
                unit = next(row for row in resolved['units'] if row['unit_id'] == unit_id)
                self.assertEqual(unit['state'], 'governed-terminal')
                link, = unit['terminal_receipts']
                resolution = json.loads((directory / 'resolutions' / (link['resolution_id'] + '.json')).read_text())
                self.assertEqual(link['sha256'], f.sha(f.encoded(resolution)))
                self.assertEqual(resolution['unit_id'], unit_id)
                self.assertEqual(resolution['canonical_operation_id'], proposal['operation_id'])
                self.assertEqual(resolution['canonical_receipt_sha256'], f.sha(f.encoded(receipt)))
                self.assertEqual(resolution['claims'], [pending_decisions[0]['coverage_claim']])

    def test_consumers_review_and_commit_without_pending_frontier(self):
        for route in ('update', 'consolidate', 'governance'):
            with self.subTest(route=route):
                self._consumer_flow(route, pending=False)

    def test_pending_consumers_accept_independently_reviewed_exact_intended_output(self):
        """Require full source credit from real consumer writes and exact receipts."""
        for route in ('update', 'consolidate', 'governance'):
            with self.subTest(route=route):
                self._consumer_flow(route, pending=True)
