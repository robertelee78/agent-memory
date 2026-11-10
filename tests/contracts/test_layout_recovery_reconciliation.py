"""Real installed CLI recovery at retained rollback and resolution boundaries."""
import copy
import json
from pathlib import Path
import unittest
import uuid

from world import World
import layout_recovery_fixtures as f

SCRATCH = '# Disposable scratch\n\nSynthetic temporary work-state; explicitly not durable guidance.\n'


def bank_bytes(bank):
    return {path.relative_to(bank).as_posix(): path.read_bytes() for path in bank.rglob('*')
            if path.is_file() and not path.name.startswith('.agent-memory')}


def retained(directory):
    return {path.relative_to(directory).as_posix(): path.read_bytes() for path in directory.rglob('*') if path.is_file()}


def revision_request(world, bank, inspected, unit_id):
    directory = f.private_directory(world, bank, inspected['operation_id'])
    head = json.loads((directory / 'ledger-head.json').read_text())
    ledger = json.loads((directory / 'ledger' / (str(head['generation']).zfill(8) + '.json')).read_text())
    unit_map = json.loads((directory / 'unit-map.json').read_text())
    if f.sha(f.encoded(unit_map)) != ledger['unit_map_sha256']:
        unit_map = json.loads((directory / 'unit-maps' / (ledger['unit_map_sha256'] + '.json')).read_text())
    parent = next(unit for unit in unit_map['units'] if unit['unit_id'] == unit_id)
    span = parent['source_ranges'][0]
    middle = (span['start'] + span['end']) // 2
    children = [{'unit_id': str(uuid.uuid4()),
        'source_ranges': [{**span, 'start': start, 'end': end}],
        'semantic_rationale': 'Controlled reviewed partition retains all original source conditions.',
        'provenance': parent['provenance'], 'reason': parent['reason'],
        'required_actions': parent['required_actions']} for start, end in ((span['start'], middle), (middle, span['end']))]
    replacements = [{'parent_unit_id': unit_id, 'source_ranges': parent['source_ranges'], 'new_units': children,
        'semantic_rationale': 'Controlled independent refinement of the original complete synthetic source.'}]
    revised_map = {**unit_map, 'units': [unit for unit in unit_map['units'] if unit['unit_id'] != unit_id]
        + [{**parent, **{key: child[key] for key in ('unit_id', 'source_ranges', 'provenance', 'reason', 'required_actions')}}
           for child in children]}
    actor = {'developer_id': inspected['manifest']['identity']['scope_context']['developer_id'],
        'harness': 'controlled-revision-author', 'session': 'revision-author', 'evidence': 'Controlled exact source review.'}
    review = {'schema': 'agent-memory/layout-unit-revision-review-v1', 'review_id': str(uuid.uuid4()),
        'operation_id': inspected['operation_id'], 'author': actor,
        'reviewer': {**actor, 'harness': 'controlled-independent-reviewer', 'session': 'revision-reviewer'},
        'parent_unit_map_sha256': ledger['unit_map_sha256'], 'parent_ledger_head_sha256': f.sha(f.encoded(head)),
        'manifest_sha256': f.sha(f.encoded(inspected['manifest'])), 'replacements_sha256': f.sha(f.encoded(replacements)),
        'intended_unit_map_sha256': f.sha(f.encoded(revised_map)), 'reviewed_at': inspected['manifest']['inspected_at'],
        'decision': 'approve', 'rationale': 'Controlled full-source partition and native reservation review.',
        'evidence': 'Every original source byte is retained in exactly one child.'}
    return {'schema': 'agent-memory/layout-unit-revision-request-v1', 'operation_id': inspected['operation_id'],
        'replacements': replacements, 'review': review}


class RecoveryReconciliationContract(unittest.TestCase):
    def originals(self, world, bank, inspected, wanted):
        directory = f.private_directory(world, bank, inspected['operation_id'])
        for source in inspected['manifest']['sources']:
            self.assertEqual((directory / source['blob']).read_bytes(), wanted[source['path']])

    def test_revision_rejects_overlap_and_loss_then_exact_retry_is_historical(self):
        with World() as world:
            command, _, repo, bank, _, inspected, applied = f.small_pending(world, SCRATCH)
            operation = inspected['operation_id']
            parent = applied['pending_unit_ids'][0]
            request = revision_request(world, bank, inspected, parent)
            canonical = bank_bytes(bank)
            directory = f.private_directory(world, bank, operation)
            head_before = (directory / 'ledger-head.json').read_bytes()
            for delta in (-1, 1):
                invalid = copy.deepcopy(request)
                invalid['replacements'][0]['new_units'][1]['source_ranges'][0]['start'] += delta
                # The altered request is independently pinned; rejection must
                # concern its source partition, not an old replacements hash.
                invalid['review']['replacements_sha256'] = f.sha(f.encoded(invalid['replacements']))
                f.call(world, command, 'revise-units', '--repo', repo, '--operation', operation,
                       '--input', f.save(world, invalid), '--output',
                       world.inside('invalid-revision-' + str(delta) + '.json'), expected=1)
                self.assertEqual(bank_bytes(bank), canonical)
                self.assertEqual((directory / 'ledger-head.json').read_bytes(), head_before)
                self.assertEqual(f.call(world, command, 'status', '--repo', repo,
                                        '--operation', operation)['pending_unit_ids'], [parent])
            proposal = world.inside('valid-revision.json')
            f.call(world, command, 'revise-units', '--repo', repo, '--operation', operation,
                   '--input', f.save(world, request), '--output', proposal)
            f.call(world, command, 'revise-units', '--repo', repo, '--proposal', proposal)
            head = json.loads((directory / 'ledger-head.json').read_text())
            ledger = json.loads((directory / 'ledger' / (str(head['generation']).zfill(8) + '.json')).read_text())
            row = next(unit for unit in ledger['units'] if unit['unit_id'] == parent)
            children = {unit['unit_id'] for unit in request['replacements'][0]['new_units']}
            self.assertEqual(row['state'], 'superseded-by-revision')
            self.assertEqual(set(row['child_unit_ids']), children)
            self.assertEqual(set(f.call(world, command, 'status', '--repo', repo,
                                        '--operation', operation)['pending_unit_ids']), children)
            before_retry = retained(directory), bank_bytes(bank)
            retry = f.call(world, command, 'revise-units', '--repo', repo, '--proposal', proposal)
            self.assertTrue(retry['already_applied'])
            self.assertTrue(retry['receipt_is_historical'])
            self.assertEqual((retained(directory), bank_bytes(bank)), before_retry)

    def test_raw_router_bootstrap_uses_checked_committed_identity_not_normal_router_limits(self):
        with World() as world:
            command, _ = f.installed(world)
            repo, bank, _ = f.memory(world)
            _, original_inspection = f.inspect(world, command, repo)
            committed = (bank / 'MEMORY.md').read_bytes()
            world.git('add', '.', cwd=repo)
            world.git('commit', '-m', 'Controlled existing router evidence', cwd=repo)
            head = world.git('rev-parse', 'HEAD', cwd=repo).stdout.strip()
            identity = original_inspection['manifest']['identity']['scope_context']
            for raw in (committed + b'\n' + b' ' * 17000, b'malformed router with ambiguous UUID-looking prose\n'):
                with self.subTest(size=len(raw)):
                    (bank / 'MEMORY.md').write_bytes(raw)
                    review = {'schema': 'agent-memory/layout-identity-review-v1', 'review_id': str(uuid.uuid4()),
                        'project_id': identity['project_id'], 'repo_root': str(repo),
                        'checkout_key': identity['checkout_key'], 'router_sha256': f.sha(raw),
                        'committed_router': head + ':memory-bank/MEMORY.md', 'committed_router_sha256': f.sha(committed),
                        'reviewer_developer_id': identity['developer_id'], 'reviewer_harness': 'controlled-reviewer',
                        'reviewer_session': 'router-review', 'author_session': 'router-author',
                        'reviewed_at': original_inspection['manifest']['inspected_at'],
                        'rationale': 'Independently checked existing registry/checkout and exact committed router.',
                        'decision': 'accept'}
                    before = bank_bytes(bank)
                    output = world.inside('raw-' + str(uuid.uuid4()) + '.json')
                    inspected = f.call(world, command, 'inspect', '--repo', repo,
                        '--identity-review', f.save(world, review), '--output', output)
                    private = json.loads(output.read_text())
                    self.assertTrue(inspected['observation_complete'])
                    self.assertEqual(private['manifest']['project_id'], identity['project_id'])
                    self.assertEqual(bank_bytes(bank), before)
                    self.originals(world, bank, private, before)
                    self.assertNotEqual(world.run([command, 'bank', 'validate', '--repo', repo], check=False, timeout=30).returncode, 0)
                    if raw.startswith(b'malformed'):
                        wrong = {**review, 'project_id': str(uuid.uuid4())}
                        f.call(world, command, 'inspect', '--repo', repo, '--identity-review', f.save(world, wrong),
                            '--output', world.inside('wrong-' + str(uuid.uuid4()) + '.json'), expected=1)
                        self.assertEqual(bank_bytes(bank), before)
            conflicting = committed.replace(identity['project_id'].encode(), str(uuid.uuid4()).encode())
            (bank / 'MEMORY.md').write_bytes(conflicting)
            before = bank_bytes(bank)
            index = (repo / '.git/index').read_bytes()
            checked = {**review, 'router_sha256': f.sha(conflicting)}
            for supplied in (None, checked):
                args = ['--identity-review', f.save(world, supplied)] if supplied else []
                f.call(world, command, 'inspect', '--repo', repo, *args,
                    '--output', world.inside('conflicting-' + str(uuid.uuid4()) + '.json'), expected=1)
                self.assertEqual(bank_bytes(bank), before)
                self.assertEqual((repo / '.git/index').read_bytes(), index)
                self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout.strip(), head)

    def test_retained_rollback_repair_handoff_keeps_both_independent_obligations(self):
        for completed in (False, True):
            with self.subTest(completed=completed), World() as world:
                command, source = f.installed(world)
                repo, bank, native = f.memory(world)
                (bank / 'handoff-prompt.md').write_text(SCRATCH)
                original = bank_bytes(bank)
                _, inspected = f.inspect(world, command, repo)
                proposal, _ = f.prepare(world, command, source, repo, inspected, native)
                if completed:
                    f.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
                    restore = world.inside('child.json')
                    f.call(world, command, 'rollback', '--repo', repo, '--operation', inspected['operation_id'], '--output', restore)
                    f.call(world, command, 'rollback', '--repo', repo, '--proposal', restore)
                else:
                    f.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1,
                        env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-before-receipt'})
                    restore = world.inside('rollback.json')
                    f.call(world, command, 'recover', '--repo', repo, '--operation', inspected['operation_id'],
                        '--mode', 'rollback', '--output', restore)
                    f.call(world, command, 'recover', '--repo', repo, '--proposal', restore)
                status = f.call(world, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
                old_ids = set(status['pending_unit_ids'])
                self.assertEqual(len(old_ids), 1)
                self.assertEqual(bank_bytes(bank), original)
                old_marker = (bank / '.agent-memory-transaction.md').read_bytes()
                _, continuation = f.inspect(world, command, repo)
                self.assertEqual(continuation['manifest']['predecessor']['operation_id'], inspected['operation_id'])
                self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), old_marker)
                next_proposal, _ = f.prepare(world, command, source, repo, continuation, native)
                applied = f.call(world, command, 'apply', '--repo', repo, '--proposal', next_proposal)
                self.assertEqual(len(applied['pending_unit_ids']), 2)
                self.assertTrue(old_ids < set(applied['pending_unit_ids']))
                new_ids = set(applied['pending_unit_ids']) - old_ids
                marker = (bank / '.agent-memory-transaction.md').read_bytes()
                self.assertNotEqual(marker, old_marker)
                self.assertEqual(json.loads(marker)['operation_id'], continuation['operation_id'])
                resolution = f.discard_proposal(world, command, repo, bank, continuation, next(iter(new_ids)))
                f.call(world, command, 'resolve', '--repo', repo, '--proposal', resolution)
                after = f.call(world, command, 'status', '--repo', repo, '--operation', continuation['operation_id'])
                self.assertEqual(set(after['pending_unit_ids']), old_ids)
                self.assertFalse(after['semantic_recovery_complete'])
                self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), marker)
                self.assertTrue((bank / '.agent-memory-recovery.md').exists())
                self.originals(world, bank, inspected, original)
                directory = f.private_directory(world, bank, inspected['operation_id'])
                handoff = next(row for row in inspected['manifest']['sources'] if row['path'] == 'handoff-prompt.md')
                original_blob = directory / handoff['blob']
                original_blob.write_bytes(original_blob.read_bytes() + b'changed')
                f.call(world, command, 'status', '--repo', repo, '--operation', continuation['operation_id'], expected=1)
                self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), marker)

    def test_unfinished_rolled_back_resolution_excludes_outer_repair_handoff(self):
        with World() as world:
            command, source = f.installed(world)
            repo, bank, native = f.memory(world)
            (bank / 'handoff-notes.txt').write_text(SCRATCH)
            original = bank_bytes(bank)
            _, inspected = f.inspect(world, command, repo)
            initial_request = f.review_request(world, command, source, repo, inspected, native,
                pending_paths=('handoff-notes.txt',))
            proposal = world.inside('initial-pending-layout.json')
            f.call(world, command, 'prepare', '--repo', repo, '--operation', inspected['operation_id'],
                '--input', f.save(world, initial_request), '--output', proposal)
            f.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1,
                env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-before-receipt'})
            rollback = world.inside('rolled-back-parent.json')
            f.call(world, command, 'recover', '--repo', repo, '--operation', inspected['operation_id'],
                '--mode', 'rollback', '--output', rollback)
            f.call(world, command, 'recover', '--repo', repo, '--proposal', rollback)
            _, continuation = f.inspect(world, command, repo)
            continuation_request = f.review_request(world, command, source, repo, continuation, native,
                pending_paths=('handoff-notes.txt',))
            next_proposal = world.inside('continuation-proposal.json')
            f.call(world, command, 'prepare', '--repo', repo, '--operation', continuation['operation_id'],
                '--input', f.save(world, continuation_request), '--output', next_proposal)
            status = f.call(world, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
            resolution = f.discard_proposal(world, command, repo, bank, inspected, status['pending_unit_ids'][0])
            f.call(world, command, 'resolve', '--repo', repo, '--proposal', resolution, expected=1,
                env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-resolution-after-receipt'})
            directory = f.private_directory(world, bank, inspected['operation_id'])
            before = retained(directory.parent)
            marker = (bank / '.agent-memory-transaction.md').read_bytes()
            f.call(world, command, 'apply', '--repo', repo, '--proposal', next_proposal, expected=1)
            f.call(world, command, 'inspect', '--repo', repo,
                '--output', world.inside('reserved-inspection.json'), expected=1)
            f.call(world, command, 'prepare', '--repo', repo, '--operation', continuation['operation_id'],
                '--input', f.save(world, continuation_request),
                '--output', world.inside('reserved-preparation.json'), expected=1)
            self.assertEqual(retained(directory.parent), before)
            self.assertEqual((bank / '.agent-memory-transaction.md').read_bytes(), marker)
            self.assertEqual(bank_bytes(bank), original)
            completed = f.call(world, command, 'resolve', '--repo', repo, '--proposal', resolution)
            self.assertTrue(completed['semantic_recovery_complete'])
            f.call(world, command, 'apply', '--repo', repo, '--proposal', next_proposal, expected=1)
            f.inspect(world, command, repo)
            self.assertEqual(bank_bytes(bank), original)

    def test_standalone_receipt_retry_finishes_exact_linkage_and_retirement(self):
        cases = [('layout-recovery-resolution-after-receipt', 'exit'),
                 ('layout-recovery-ledger-generation-before-head', 'error'),
                 ('layout-recovery-ledger-generation-after-head', 'exit'),
                 ('layout-recovery-sentinel-retirement', 'error'),
                 ('layout-recovery-retirement-after-marker-removal', 'exit'),
                 ('layout-recovery-retirement-after-sentinel-removal', 'error')]
        for stage, mode in cases:
            with self.subTest(stage=stage, mode=mode), World() as world:
                command, _, repo, bank, _, inspected, applied = f.small_pending(world, SCRATCH)
                canonical = bank_bytes(bank)
                directory = f.private_directory(world, bank, inspected['operation_id'])
                originals = {source['path']: (directory / source['blob']).read_bytes() for source in inspected['manifest']['sources']}
                proposal = f.discard_proposal(world, command, repo, bank, inspected, applied['pending_unit_ids'][0])
                request = json.loads(proposal.read_text())['proposal']
                faulted = world.run([command, 'bank', 'recover-layout', 'resolve', '--repo', repo, '--proposal', proposal],
                    check=False, timeout=30, env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
                self.assertEqual(faulted.returncode, 86 if mode == 'exit' else 1, faulted.stdout + faulted.stderr)
                receipt_file = directory / 'resolutions' / (request['resolution_id'] + '.json')
                receipt = receipt_file.read_bytes()
                if mode == 'exit':
                    f.unlock(world, command, repo)
                changed = copy.deepcopy(request)
                changed['terminal_state'] = 'admitted'
                f.call(world, command, 'resolve', '--repo', repo, '--proposal', f.save(world, changed), expected=1)
                f.call(world, command, 'resolve', '--repo', repo, '--proposal', proposal)
                head = json.loads((directory / 'ledger-head.json').read_text())
                ledger = json.loads((directory / 'ledger/00000001.json').read_text())
                self.assertEqual(head['generation'], 1)
                self.assertEqual(ledger['units'][0]['state'], 'reviewed-non-guidance-discard')
                self.assertEqual(len(ledger['units'][0]['terminal_receipts']), 1)
                self.assertEqual(receipt_file.read_bytes(), receipt)
                self.assertFalse((bank / '.agent-memory-transaction.md').exists())
                self.assertFalse((bank / '.agent-memory-recovery.md').exists())
                status = f.call(world, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
                self.assertTrue(status['semantic_recovery_complete'])
                self.assertEqual(status['pending_units'], 0)
                history = f.call(world, command, 'resolve', '--repo', repo, '--proposal', proposal)
                self.assertTrue(history['already_applied'])
                self.assertFalse(history['writes_performed'])
                self.assertEqual(bank_bytes(bank), canonical)
                self.originals(world, bank, inspected, originals)

    def test_unfinished_resolution_excludes_new_resolutions_nested_admission_and_revision(self):
        for fault_mode in ('error', 'exit'):
            with self.subTest(mode=fault_mode), World() as world:
                command, source = f.installed(world)
                repo, bank, native = f.memory(world)
                (bank / 'handoff-prompt.md').write_text(SCRATCH + f.policy_body(0))
                (repo / 'synthetic-policy.md').write_text(f.policy_body(0))
                original = bank_bytes(bank)
                _, inspected = f.inspect(world, command, repo)
                operation = inspected['operation_id']
                request = f.review_request(world, command, source, repo, inspected, native)
                pending = next(unit for unit in request['unit_map']['units'] if unit['disposition'] == 'pending-admission')
                span = pending['source_ranges'][0]
                units = [{**pending, 'unit_id': str(uuid.uuid4()), 'source_ranges': [{**span, 'start': start, 'end': end}]}
                    for start, end in ((0, len(SCRATCH.encode())), (len(SCRATCH.encode()), span['end']))]
                request['unit_map']['units'] = [unit for unit in request['unit_map']['units'] if unit != pending] + units
                request['review']['unit_map_sha256'] = f.sha(f.encoded(request['unit_map']))
                request['review']['unit_decisions'] = [{'unit_id': unit['unit_id'], 'disposition': unit['disposition'],
                    'rationale': unit['reason']} for unit in request['unit_map']['units']]
                layout = world.inside('paired-layout.json')
                f.call(world, command, 'prepare', '--repo', repo, '--operation', operation,
                    '--input', f.save(world, request), '--output', layout)
                f.call(world, command, 'apply', '--repo', repo, '--proposal', layout)
                directory = f.private_directory(world, bank, operation)
                first = f.discard_proposal(world, command, repo, bank, inspected, units[0]['unit_id'])
                first_p = json.loads(first.read_text())['proposal']
                second = f.discard_proposal(world, command, repo, bank, inspected, units[1]['unit_id'])
                second_p = json.loads(second.read_text())['proposal']
                admission, _, _ = f.admission_proposal(world, command, source, repo, bank, inspected, full=True)
                revision_input = f.save(world, revision_request(world, bank, inspected, units[1]['unit_id']))
                revision = world.inside('blocked-revision.json')
                f.call(world, command, 'revise-units', '--repo', repo, '--operation', operation,
                    '--input', revision_input, '--output', revision)
                fault = world.run([command, 'bank', 'recover-layout', 'resolve', '--repo', repo, '--proposal', first],
                    check=False, timeout=30, env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-resolution-after-receipt',
                        'AGENT_MEMORY_FAULT_MODE': fault_mode})
                self.assertEqual(fault.returncode, 86 if fault_mode == 'exit' else 1, fault.stdout + fault.stderr)
                if fault_mode == 'exit':
                    f.unlock(world, command, repo)
                canonical = bank_bytes(bank)
                safeguards = {name: (bank / name).read_bytes() for name in
                    ('.agent-memory-transaction.md', '.agent-memory-recovery.md')}
                private = retained(directory)
                index = (directory.parent / 'index.json').read_bytes()
                for p in (first_p, second_p):
                    f.call(world, command, 'resolve', '--repo', repo, '--operation', operation, '--unit', p['unit_id'],
                        '--input', f.save(world, p['terminal']), '--output', world.inside(str(uuid.uuid4()) + '.json'), expected=1)
                f.call(world, command, 'resolve', '--repo', repo, '--proposal', second, expected=1)
                f.public(world, command, 'admission', 'apply', '--proposal', admission, expected=1)
                f.call(world, command, 'revise-units', '--repo', repo, '--proposal', revision, expected=1)
                f.call(world, command, 'revise-units', '--repo', repo, '--operation', operation,
                    '--input', revision_input, '--output', world.inside('new-blocked-revision.json'), expected=1)
                self.assertEqual(retained(directory), private)
                self.assertEqual((directory.parent / 'index.json').read_bytes(), index)
                self.assertEqual(bank_bytes(bank), canonical)
                self.assertEqual({name: (bank / name).read_bytes() for name in safeguards}, safeguards)
                completion = directory / 'resolution-completions' / (first_p['resolution_id'] + '.json')
                completion.parent.mkdir(mode=0o700, exist_ok=True)
                completion.write_bytes(f.encoded({'verified': True})); completion.chmod(0o600)
                corrupt = retained(directory)
                f.call(world, command, 'resolve', '--repo', repo, '--proposal', first, expected=1)
                f.call(world, command, 'resolve', '--repo', repo, '--proposal', second, expected=1)
                f.public(world, command, 'admission', 'apply', '--proposal', admission, expected=1)
                f.call(world, command, 'revise-units', '--repo', repo, '--proposal', revision, expected=1)
                self.assertEqual(retained(directory), corrupt)
                self.assertEqual((directory.parent / 'index.json').read_bytes(), index)
                self.assertEqual(bank_bytes(bank), canonical)
                completion.unlink()
                f.call(world, command, 'resolve', '--repo', repo, '--proposal', first)
                self.assertEqual(json.loads((directory / 'ledger-head.json').read_text())['generation'], 1)
                self.assertEqual((directory / 'resolutions' / (first_p['resolution_id'] + '.json')).read_bytes(),
                    private['resolutions/' + first_p['resolution_id'] + '.json'])
                refreshed = f.save(world, revision_request(world, bank, inspected, units[1]['unit_id']))
                refined = world.inside('refined.json')
                f.call(world, command, 'revise-units', '--repo', repo, '--operation', operation,
                    '--input', refreshed, '--output', refined)
                f.call(world, command, 'revise-units', '--repo', repo, '--proposal', refined)
                fresh_admission, _, _ = f.admission_proposal(world, command, source, repo, bank, inspected, full=True)
                f.public(world, command, 'admission', 'apply', '--proposal', fresh_admission)
                status = f.call(world, command, 'status', '--repo', repo, '--operation', operation)
                self.assertEqual(len(status['pending_unit_ids']), 1)
                final = f.discard_proposal(world, command, repo, bank, inspected, status['pending_unit_ids'][0])
                f.call(world, command, 'resolve', '--repo', repo, '--proposal', final)
                self.assertEqual(json.loads((directory / 'ledger-head.json').read_text())['generation'], 4)
                self.assertTrue(f.call(world, command, 'status', '--repo', repo, '--operation', operation)['semantic_recovery_complete'])
                f.call(world, command, 'retention', 'inspect', '--repo', repo, '--operation', operation,
                    '--output', world.inside('retention.json'))
                self.assertFalse((bank / '.agent-memory-transaction.md').exists())
                self.assertFalse((bank / '.agent-memory-recovery.md').exists())
                self.originals(world, bank, inspected, original)



if __name__ == '__main__':
    unittest.main()
