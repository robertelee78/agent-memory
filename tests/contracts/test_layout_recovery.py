"""Public recovery and real-count observation against disposable synthetic sources."""
import base64
import copy
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
import unittest
import uuid

from world import World
import layout_recovery_fixtures as fixture

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'))
from agent_memory import layout_recovery_observation as raw_observation


class LayoutRecoveryContract(unittest.TestCase):
    def setup_repair(self, world, *, pending=False):
        command, source = fixture.installed(world)
        repo, bank, native = fixture.memory(world, oversized=True, handoff=pending)
        originals = {path.relative_to(bank).as_posix(): path.read_bytes()
                     for path in bank.rglob('*') if path.is_file()}
        return command, source, repo, bank, native, originals

    def private_directory(self, world, bank, operation):
        return (Path(world.env['AGENT_MEMORY_STATE_HOME']) / 'writes'
                / fixture.sha(str(bank).encode()) / 'layout-recovery' / operation)

    def setup_fault_repair(self, world):
        command, source = fixture.installed(world)
        repo, bank, native = fixture.memory(world, oversized=True)
        active = bank / 'activeContext.md'
        active.write_text(native['activeContext.md'] + '\n' + native['activeContext.md'])
        (bank / 'handoff-prompt.md').write_text(fixture.policy_body(0))
        (repo / 'synthetic-policy.md').write_text(fixture.policy_body(0))
        originals = {path.relative_to(bank).as_posix(): path.read_bytes()
                     for path in bank.rglob('*') if path.is_file()}
        modes = {path: stat.S_IMODE((bank / path).stat().st_mode) for path in originals}
        feature = repo / 'feature.txt'; feature.write_text('Committed feature\n')
        world.git('add', 'feature.txt', cwd=repo)
        world.git('commit', '-m', 'Synthetic feature baseline', cwd=repo)
        feature.write_text('Staged feature\n'); world.git('add', 'feature.txt', cwd=repo)
        feature.write_text('Unstaged feature\n')
        untracked = repo / 'untracked.txt'; untracked.write_bytes(b'Untracked feature\n')
        git = ((repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout)
        return command, source, repo, bank, native, originals, modes, git

    def assert_fault_work_unchanged(self, world, repo, git):
        self.assertEqual((repo / '.git/index').read_bytes(), git[0])
        self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, git[1])
        self.assertEqual((repo / 'feature.txt').read_text(), 'Unstaged feature\n')
        self.assertEqual((repo / 'untracked.txt').read_bytes(), b'Untracked feature\n')

    def test_initial_quiescence_requires_exact_independent_review(self):
        with World() as world:
            command, source, repo, bank, native, originals = self.setup_repair(world, pending=True)
            (repo / 'feature.txt').write_text('Fixture baseline\n')
            world.git('add', 'feature.txt', cwd=repo)
            world.git('commit', '-m', 'Fixture baseline', cwd=repo)
            index = (repo / '.git/index').read_bytes()
            head = world.git('rev-parse', 'HEAD', cwd=repo).stdout
            safe, inspected = fixture.inspect(world, command, repo)
            good = fixture.review_request(world, command, source, repo, inspected, native)
            output = world.inside('initial-reviewed.json')
            fixture.call(world, command, 'prepare', '--repo', repo, '--operation', safe['operation_id'],
                         '--input', fixture.save(world, good), '--output', output)
            directory = self.private_directory(world, bank, safe['operation_id'])
            review_path = directory / 'reviews' / (good['review']['review_id'] + '.json')
            original_review = review_path.read_bytes()
            altered = copy.deepcopy(good)
            altered['quiescence_attestation']['attestation_id'] = str(uuid.uuid4())
            altered['quiescence_attestation']['declared_invocation_routes'].append('/unreviewed-route')
            missing = copy.deepcopy(good)
            del missing['review']['quiescence_attestation_sha256']
            wrong_pin = copy.deepcopy(good)
            wrong_pin['review']['quiescence_attestation_sha256'] = '0' * 64
            missing_metadata = copy.deepcopy(good)
            del missing_metadata['review']['metadata_plan_sha256']
            wrong_metadata = copy.deepcopy(good)
            wrong_metadata['review']['metadata_plan_sha256'] = '0' * 64
            same_session = copy.deepcopy(good)
            same_session['review']['reviewer']['session'] = same_session['author']['session']
            same_session['review']['reviewer']['harness'] = same_session['author']['harness']
            for request in (altered, missing, wrong_pin, missing_metadata, wrong_metadata, same_session):
                refused = fixture.call(world, command, 'prepare', '--repo', repo, '--operation', safe['operation_id'],
                    '--input', fixture.save(world, request), '--output', world.inside(str(uuid.uuid4()) + '.json'),
                    expected=1)
                self.assertFalse(refused['canonical_writes_performed'])
                self.assertEqual(review_path.read_bytes(), original_review)
            fresh = copy.deepcopy(altered)
            fresh['review']['review_id'] = str(uuid.uuid4())
            fresh['quiescence_attestation']['review_id'] = fresh['review']['review_id']
            fresh['review']['quiescence_attestation_sha256'] = fixture.sha(fixture.encoded(fresh['quiescence_attestation']))
            fixture.call(world, command, 'prepare', '--repo', repo, '--operation', safe['operation_id'],
                         '--input', fixture.save(world, fresh), '--output', world.inside('fresh-reviewed.json'))
            self.assertEqual(review_path.read_bytes(), original_review)
            self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes()
                              for p in bank.rglob('*') if p.is_file()}, originals)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, head)

    def test_partial_original_staging_remains_incomplete_and_fresh_inspection_preserves_it(self):
        with World() as world:
            command, source, repo, bank, native, originals, modes, git = self.setup_fault_repair(world)
            failed = fixture.call(world, command, 'inspect', '--repo', repo,
                '--output', world.inside('partial-inspect.json'), expected=1,
                env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-partial-original-staging'})
            operation = failed['operation_id']
            self.assertFalse(failed['canonical_mutation_may_have_occurred'])
            directory = self.private_directory(world, bank, operation)
            staging = (directory / 'staging.json').read_bytes()
            self.assertFalse(json.loads(staging)['observation_complete'])
            self.assertFalse((directory / 'manifest.json').exists())
            blobs = {path.name: path.read_bytes() for path in (directory / 'originals').glob('*.bin')}
            self.assertEqual(len(blobs), 1)
            self.assertIn(next(iter(blobs.values())), originals.values())
            status = fixture.call(world, command, 'status', '--repo', repo, '--operation', operation)
            self.assertEqual(status['state'], 'observation-incomplete')
            self.assertFalse(status['observation_complete'])
            self.assertFalse(status['semantic_recovery_complete'])
            refused = fixture.call(world, command, 'prepare', '--repo', repo, '--operation', operation,
                '--input', fixture.save(world, {}), '--output', world.inside('incomplete-proposal.json'), expected=1)
            self.assertFalse(refused['canonical_writes_performed'])
            safe, inspected = fixture.inspect(world, command, repo)
            self.assertNotEqual(safe['operation_id'], operation)
            self.assertTrue(safe['observation_complete'])
            complete = self.private_directory(world, bank, safe['operation_id'])
            self.assertEqual({row['path']: (complete / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertEqual((directory / 'staging.json').read_bytes(), staging)
            self.assertEqual({path.name: path.read_bytes() for path in (directory / 'originals').glob('*.bin')}, blobs)
            self.assertFalse((directory / 'manifest.json').exists())
            self.assertEqual({path.relative_to(bank).as_posix(): path.read_bytes()
                              for path in bank.rglob('*') if path.is_file()}, originals)
            self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in originals}, modes)
            self.assert_fault_work_unchanged(world, repo, git)

    def test_first_replacement_before_progress_supports_known_finish_and_rollback(self):
        for mode in ('finish', 'rollback'):
            with self.subTest(mode=mode), World() as world:
                command, source, repo, bank, native, originals, modes, git = self.setup_fault_repair(world)
                safe, inspected = fixture.inspect(world, command, repo)
                proposal, prepared = fixture.prepare(world, command, source, repo, inspected, native)
                actions = prepared['proposal']['actions']
                first = actions[0]['path']
                self.assertEqual(first, 'activeContext.md')
                self.assertEqual(actions[0]['action'], 'replace')
                failed = fixture.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1,
                    env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-after-rename:' + first})
                self.assertTrue(failed['canonical_mutation_may_have_occurred'])
                directory = self.private_directory(world, bank, safe['operation_id'])
                self.assertEqual((bank / first).read_bytes(), native[first].encode())
                self.assertNotEqual((bank / first).read_bytes(), originals[first])
                self.assertFalse((directory / 'progress' / 'finish-00000000.json').exists())
                self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                                  for row in inspected['manifest']['sources']}, originals)
                status = fixture.call(world, command, 'status', '--repo', repo, '--operation', safe['operation_id'])
                self.assertEqual(status['state'], 'interrupted')
                self.assertIsNot(status['canonical_eligibility'], True)
                self.assertFalse(status['semantic_recovery_complete'])
                self.assertTrue((bank / '.agent-memory-transaction.md').exists())
                self.assertTrue((bank / '.agent-memory-recovery.md').exists())
                output = world.inside('first-replacement-recovery.json')
                fixture.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                             '--mode', mode, '--output', output)
                outcome = fixture.call(world, command, 'recover', '--repo', repo, '--proposal', output)
                self.assertTrue(outcome['verified'])
                self.assertFalse(outcome['semantic_recovery_complete'])
                self.assertGreater(outcome['pending_units'], 0)
                wanted = {path: body.encode() for path, body in native.items()} if mode == 'finish' else originals
                self.assertEqual({path.relative_to(bank).as_posix(): path.read_bytes()
                                  for path in bank.rglob('*') if path.is_file()
                                  and path.name not in ('.agent-memory-transaction.md', '.agent-memory-recovery.md')}, wanted)
                self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in wanted},
                                 {path: modes[path] for path in wanted})
                self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                                  for row in inspected['manifest']['sources']}, originals)
                self.assertTrue((bank / '.agent-memory-transaction.md').exists())
                self.assertTrue((bank / '.agent-memory-recovery.md').exists())
                self.assert_fault_work_unchanged(world, repo, git)

    def test_reported_sizes_fail_normal_gate_and_public_pages_reconstruct_every_original(self):
        with World() as world:
            command, _, repo, bank, _, originals = self.setup_repair(world, pending=True)
            self.assertEqual(len(originals['activeContext.md']), 79054)
            self.assertEqual(len(originals['handoff-prompt.md']), 28478)
            normal = world.run([command, 'bank', 'validate', '--repo', repo], check=False)
            self.assertNotEqual(normal.returncode, 0)
            safe, inspected = fixture.inspect(world, command, repo)
            self.assertIsNone(inspected['manifest']['predecessor'])
            self.assertFalse(safe['canonical_eligibility'])
            self.assertNotIn('manifest', safe)
            self.assertNotIn('sha256', json.dumps(safe))
            reconstructed = {row['path']: bytearray() for row in inspected['manifest']['sources']}
            for number in range(safe['page_count']):
                output = world.inside('page-' + str(number) + '.json')
                envelope = fixture.call(world, command, 'inspect', '--repo', repo,
                    '--operation', safe['operation_id'], '--page', number, '--output', output)
                self.assertNotIn('body', envelope)
                raw = output.read_bytes()
                self.assertLessEqual(len(raw), 65536)
                page = json.loads(raw)
                self.assertEqual(page['offset'], len(reconstructed[page['path']]))
                reconstructed[page['path']].extend(base64.b64decode(page['body'], validate=True))
                self.assertEqual(page['operation_id'], safe['operation_id'])
            self.assertEqual({path: bytes(raw) for path, raw in reconstructed.items()}, originals)
            self.assertEqual({path.relative_to(bank).as_posix(): path.read_bytes()
                              for path in bank.rglob('*') if path.is_file()}, originals)

    def test_all_preserve_apply_keeps_metadata_feature_head_and_exact_original_index(self):
        with World() as world:
            command, source, repo, bank, native, originals = self.setup_repair(world)
            feature = repo / 'feature.txt'; feature.write_text('Committed baseline\n')
            world.git('add', 'feature.txt', cwd=repo); world.git('commit', '-m', 'Synthetic feature baseline', cwd=repo)
            feature.write_text('Staged unfinished feature\n'); world.git('add', 'feature.txt', cwd=repo)
            feature.write_text('Unstaged unfinished feature\n')
            untracked = repo / 'untracked.txt'; untracked.write_bytes(b'Untracked feature\n')
            before_index = (repo / '.git/index').read_bytes()
            before_head = world.git('rev-parse', 'HEAD', cwd=repo).stdout
            before_modes = {path.relative_to(bank).as_posix(): stat.S_IMODE(path.stat().st_mode)
                            for path in bank.rglob('*') if path.is_file()}
            safe, inspected = fixture.inspect(world, command, repo)
            proposal, prepared = fixture.prepare(world, command, source, repo, inspected, native)
            self.assertFalse(prepared['canonical_writes_performed'])
            applied = fixture.call(world, command, 'apply', '--repo', repo,
                '--operation', safe['operation_id'], '--proposal', proposal)
            self.assertTrue(applied['canonical_eligibility'])
            self.assertTrue(applied['semantic_recovery_complete'])
            self.assertFalse(applied['original_index_writes_performed'])
            self.assertEqual((repo / '.git/index').read_bytes(), before_index)
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, before_head)
            self.assertEqual(feature.read_text(), 'Unstaged unfinished feature\n')
            self.assertEqual(untracked.read_bytes(), b'Untracked feature\n')
            self.assertEqual({path: (bank / path).read_text() for path in native}, native)
            self.assertEqual({path: stat.S_IMODE((bank / path).stat().st_mode) for path in native}, before_modes)
            directory = self.private_directory(world, bank, safe['operation_id'])
            for row in inspected['manifest']['sources']:
                self.assertEqual((directory / row['blob']).read_bytes(), originals[row['path']])
            self.assertFalse((bank / '.agent-memory-transaction.md').exists())
            self.assertFalse((bank / '.agent-memory-recovery.md').exists())
            self.assertEqual(world.run([command, 'bank', 'validate', '--repo', repo], check=False).returncode, 0)

    def test_exact_retry_is_historical_and_never_overwrites_later_context(self):
        with World() as world:
            command, source, repo, bank, native, _ = self.setup_repair(world)
            safe, inspected = fixture.inspect(world, command, repo)
            proposal, _ = fixture.prepare(world, command, source, repo, inspected, native)
            fixture.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            changed = native['activeContext.md'] + '\nLater independently retained context.\n'
            (bank / 'activeContext.md').write_text(changed)
            retry = fixture.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertTrue(retry['already_applied'])
            self.assertTrue(retry['receipt_is_historical'])
            self.assertFalse(retry['current_receipt_match'])
            self.assertFalse(retry['semantic_recovery_complete'])
            self.assertEqual((bank / 'activeContext.md').read_text(), changed)
            fixture.call(world, command, 'rollback', '--repo', repo, '--operation', safe['operation_id'],
                         '--output', world.inside('stale-child.json'), expected=1)
            self.assertEqual((bank / 'activeContext.md').read_text(), changed)

    def test_prepare_requires_full_coverage_independent_review_real_identity_and_safe_output(self):
        with World() as world:
            command, source, repo, bank, native, originals = self.setup_repair(world, pending=True)
            safe, inspected = fixture.inspect(world, command, repo)
            good = fixture.review_request(world, command, source, repo, inspected, native)
            variants = []
            same_reviewer = copy.deepcopy(good); same_reviewer['review']['reviewer'] = same_reviewer['author']; variants.append(same_reviewer)
            missing = copy.deepcopy(good); missing['unit_map']['units'].pop(); variants.append(missing)
            gap = copy.deepcopy(good); gap['unit_map']['units'][0]['source_ranges'][0]['end'] -= 1; variants.append(gap)
            wrong_target = copy.deepcopy(good); wrong_target['unit_map']['units'][0]['targets'][0]['sha256'] = '0' * 64; variants.append(wrong_target)
            fake_identity = copy.deepcopy(good); fake_identity['author']['developer_id'] = str(uuid.uuid4()); variants.append(fake_identity)
            no_attestation = copy.deepcopy(good); no_attestation['quiescence_attestation'] = None; variants.append(no_attestation)
            unknown = copy.deepcopy(good); unknown['skip_security'] = True; variants.append(unknown)
            for field in ('inherited_frontier_sha256', 'inherited_unit_decisions'):
                omitted = copy.deepcopy(good); omitted['review'].pop(field); variants.append(omitted)
            for request in variants:
                with self.subTest(request=request.keys()):
                    fixture.call(world, command, 'prepare', '--repo', repo, '--operation', safe['operation_id'],
                        '--input', fixture.save(world, request), '--output', world.inside(str(uuid.uuid4()) + '.json'), expected=1)
            self.assertEqual({path.relative_to(bank).as_posix(): path.read_bytes()
                              for path in bank.rglob('*') if path.is_file()}, originals)
            self.assertFalse((bank / '.agent-memory-transaction.md').exists())
            self.assertFalse((bank / '.agent-memory-recovery.md').exists())

    def test_prepare_refuses_demoting_canonical_context_or_typed_prior_guidance(self):
        for kind in ('context', 'typed-record'):
            with self.subTest(kind=kind), World() as world:
                command, source, repo, bank, native, originals = self.setup_repair(world)
                feature = repo / 'feature.txt'
                feature.write_text('Staged unrelated work\n')
                world.git('add', 'feature.txt', cwd=repo)
                index = (repo / '.git/index').read_bytes()
                head = (repo / '.git/HEAD').read_bytes()
                safe, inspected = fixture.inspect(world, command, repo)
                path = ('projectbrief.md' if kind == 'context' else
                        next(path for path in native if path.startswith('topics/')))
                request = fixture.review_request(world, command, source, repo, inspected, native,
                                                 pending_paths=(path,))
                source_id = next(row['source_id'] for row in inspected['manifest']['sources']
                                 if row['path'] == path)
                unit = next(row for row in request['unit_map']['units']
                            if row['source_ranges'][0]['source_id'] == source_id)
                self.assertEqual(unit['semantic_kind'], 'unadmitted-source')
                if kind == 'context':
                    self.assertIsNone(unit['existing_record'])
                else:
                    metadata, _ = fixture.unpack(originals[path].decode('utf-8'))
                    self.assertEqual(unit['existing_record'], {'id': metadata['id'], 'version': metadata['version']})
                    self.assertEqual(metadata['status'], 'active')
                unit['reason'] = 'Move existing guidance to private pending material to fit the recovered layout'
                review = request['review']
                review['unit_map_sha256'] = fixture.sha(fixture.encoded(request['unit_map']))
                review['intended_files_sha256'] = fixture.sha(fixture.encoded(request['intended_files']))
                decision = next(row for row in review['unit_decisions'] if row['unit_id'] == unit['unit_id'])
                decision['disposition'] = unit['disposition']
                decision['rationale'] = unit['reason']
                result = fixture.call(world, command, 'prepare', '--repo', repo,
                    '--operation', safe['operation_id'], '--input', fixture.save(world, request),
                    '--output', world.inside('demotion-proposal.json'), expected=1)
                self.assertEqual(result['failure_code'], 'pending-evidence-or-state')
                self.assertEqual(result['state'], 'pending')
                self.assertFalse(result['canonical_writes_performed'])
                self.assertFalse(result['canonical_mutation_may_have_occurred'])
                self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes()
                                  for p in bank.rglob('*') if p.is_file()}, originals)
                self.assertEqual((repo / '.git/index').read_bytes(), index)
                self.assertEqual((repo / '.git/HEAD').read_bytes(), head)
                self.assertEqual(feature.read_text(), 'Staged unrelated work\n')
                self.assertFalse((bank / '.agent-memory-transaction.md').exists())
                self.assertFalse((bank / '.agent-memory-recovery.md').exists())
                _, prepared = fixture.prepare(world, command, source, repo, inspected, native)
                self.assertFalse(prepared['canonical_writes_performed'])

    def test_changed_source_or_metadata_invalidates_apply_before_any_canonical_replacement(self):
        for change in ('bytes', 'mode', 'index'):
            with self.subTest(change=change), World() as world:
                command, source, repo, bank, native, _ = self.setup_repair(world)
                _, inspected = fixture.inspect(world, command, repo)
                proposal, _ = fixture.prepare(world, command, source, repo, inspected, native)
                target = bank / 'activeContext.md'
                if change == 'bytes':
                    target.write_bytes(target.read_bytes() + b'Later source\n')
                elif change == 'mode':
                    target.chmod(0o600)
                else:
                    (repo / 'staged.txt').write_text('Different original index\n'); world.git('add', 'staged.txt', cwd=repo)
                before = {path.relative_to(bank).as_posix(): path.read_bytes() for path in bank.rglob('*') if path.is_file()}
                fixture.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1)
                self.assertEqual({path.relative_to(bank).as_posix(): path.read_bytes() for path in bank.rglob('*') if path.is_file()}, before)

    def test_raw_file_limit_is_inclusive_and_unsupported_aliases_never_hang_or_mutate(self):
        with World() as world:
            command, _, repo, bank, _, _ = self.setup_repair(world)
            opaque = bank / 'opaque.bin'; opaque.write_bytes(b'x' * 1048576)
            _, observed = fixture.inspect(world, command, repo)
            self.assertEqual(next(row['byte_count'] for row in observed['manifest']['sources'] if row['path'] == 'opaque.bin'), 1048576)
            opaque.write_bytes(b'x' * 1048577)
            overflow = fixture.call(world, command, 'inspect', '--repo', repo,
                '--output', world.inside('overflow.json'), expected=1)
            self.assertEqual(overflow['failure_code'], 'pending-recovery-capacity')
            self.assertEqual(overflow['capacity']['limit'], 1048576)
            opaque.unlink()
            for kind in ('symlink', 'hardlink', 'fifo'):
                with self.subTest(kind=kind):
                    if kind == 'symlink':
                        opaque.symlink_to(bank / 'MEMORY.md')
                    elif kind == 'hardlink':
                        os.link(bank / 'MEMORY.md', opaque)
                    else:
                        os.mkfifo(opaque)
                    refused = fixture.call(world, command, 'inspect', '--repo', repo,
                        '--output', world.inside(kind + '.json'), expected=1)
                    self.assertFalse(refused['canonical_writes_performed'])
                    opaque.unlink()

    def test_raw_depth_boundary_and_unknown_marker_precedence(self):
        with World() as world:
            command, _, repo, bank, _, _ = self.setup_repair(world)
            deepest = bank
            for _ in range(63):
                deepest = deepest / 'd'; deepest.mkdir()
            (deepest / 'leaf.bin').write_bytes(b'opaque private fixture')
            _, observed = fixture.inspect(world, command, repo)
            self.assertEqual(observed['manifest']['totals']['depth'], 64)
            (deepest / 'too-deep').mkdir(); (deepest / 'too-deep' / 'leaf.bin').write_bytes(b'x')
            refused = fixture.call(world, command, 'inspect', '--repo', repo,
                '--output', world.inside('depth-refused.json'), expected=1)
            self.assertEqual(refused['failure_code'], 'pending-recovery-capacity')
            self.assertEqual(refused['capacity']['dimension'], 'depth')
            marker = bank / '.agent-memory-transaction.md'; marker.write_bytes(b'unknown-owner')
            marker.chmod(0o600)
            busy = fixture.call(world, command, 'inspect', '--repo', repo,
                '--output', world.inside('busy.json'), expected=1)
            self.assertNotIn('capacity', busy)
            self.assertEqual(marker.read_bytes(), b'unknown-owner')

    def test_known_finish_and_rollback_follow_exact_journal_boundaries(self):
        for mode in ('finish', 'rollback'):
            for stage in ('layout-recovery-after-journal', 'layout-recovery-after-marker',
                          'layout-recovery-after-router', 'layout-recovery-after-receipt'):
                if mode == 'rollback' and stage == 'layout-recovery-after-receipt':
                    continue  # A durable layout receipt requires a distinct child restore instead.
                with self.subTest(mode=mode, stage=stage), World() as world:
                    command, source, repo, bank, native, originals = self.setup_repair(world)
                    safe, inspected = fixture.inspect(world, command, repo)
                    proposal, _ = fixture.prepare(world, command, source, repo, inspected, native)
                    fixture.call(world, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1,
                        env={'AGENT_MEMORY_FAULT_STAGE': stage})
                    output = world.inside('known-preview.json')
                    fixture.call(world, command, 'recover', '--repo', repo, '--operation', safe['operation_id'],
                        '--mode', mode, '--output', output)
                    outcome = fixture.call(world, command, 'recover', '--repo', repo, '--proposal', output)
                    self.assertTrue(outcome['verified'])
                    wanted = {path: body.encode() for path, body in native.items()} if mode == 'finish' else originals
                    self.assertEqual({path.relative_to(bank).as_posix(): path.read_bytes()
                                      for path in bank.rglob('*') if path.is_file()}, wanted)
                    again = fixture.call(world, command, 'recover', '--repo', repo, '--proposal', output)
                    self.assertTrue(again['receipt_is_historical'])

    def test_completed_restore_has_distinct_child_and_keeps_parent_historical_receipt(self):
        with World() as world:
            command, source, repo, bank, native, originals = self.setup_repair(world)
            safe, inspected = fixture.inspect(world, command, repo)
            proposal, _ = fixture.prepare(world, command, source, repo, inspected, native)
            fixture.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            directory = self.private_directory(world, bank, safe['operation_id'])
            before_receipt = (directory / 'receipt.json').read_bytes()
            output = world.inside('restore.json')
            preview = fixture.call(world, command, 'rollback', '--repo', repo,
                '--operation', safe['operation_id'], '--output', output)
            self.assertNotEqual(preview['operation_id'], safe['operation_id'])
            restored = fixture.call(world, command, 'rollback', '--repo', repo, '--proposal', output)
            self.assertFalse(restored['canonical_eligibility'])
            self.assertFalse(restored['semantic_recovery_complete'])
            self.assertEqual((directory / 'receipt.json').read_bytes(), before_receipt)
            self.assertEqual({path.relative_to(bank).as_posix(): path.read_bytes()
                              for path in bank.rglob('*') if path.is_file()}, originals)
            historical = fixture.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertTrue(historical['receipt_is_historical'])
            self.assertFalse(historical['current_receipt_match'])


class LayoutRecoveryInventoryBounds(unittest.TestCase):
    def test_actual_aggregate_boundary_preserves_all_sources_on_overflow(self):
        with World() as world:
            bank = world.inside('raw-aggregate'); bank.mkdir()
            body = b'raw-source-canary\n' + b'x' * (1048576 - len(b'raw-source-canary\n'))
            expected = {f'{number:02d}.bin': hashlib.sha256(body).hexdigest() for number in range(64)}
            for name in expected:
                (bank / name).write_bytes(body)
            _, sources, totals = raw_observation.inventory(bank)
            self.assertEqual(totals['source_bytes'], 67108864)
            self.assertEqual({row['path']: row['sha256'] for row in sources}, expected)
            overflow = bank / 'overflow.bin'; overflow.write_bytes(b'!')
            with self.assertRaises(raw_observation.RawCapacityError) as caught:
                raw_observation.inventory(bank)
            self.assertEqual(caught.exception.details, {
                'dimension': 'aggregate-bytes', 'limit': 67108864, 'measured': 67108865,
                'measurement': 'lower-bound', 'observation_complete': False})
            self.assertEqual({path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                              for path in bank.iterdir()},
                             {**expected, 'overflow.bin': hashlib.sha256(b'!').hexdigest()})

    def test_actual_source_count_boundary_does_not_drop_or_change_excess_source(self):
        with World() as world:
            bank = world.inside('raw-source-count'); bank.mkdir()
            expected = {}
            for number in range(65536):
                name = f'{number:05d}.bin'; body = f'raw-source-{number}'.encode()
                (bank / name).write_bytes(body)
                expected[name] = hashlib.sha256(body).hexdigest()
            _, sources, totals = raw_observation.inventory(bank)
            self.assertEqual(totals['source_files'], 65536)
            self.assertEqual({row['path']: row['sha256'] for row in sources}, expected)
            overflow = bank / 'overflow.bin'; overflow.write_bytes(b'!')
            with self.assertRaises(raw_observation.RawCapacityError) as caught:
                raw_observation.inventory(bank)
            self.assertEqual(caught.exception.details, {
                'dimension': 'source-files', 'limit': 65536, 'measured': 65537,
                'measurement': 'lower-bound', 'observation_complete': False})
            self.assertEqual({path.name: hashlib.sha256(path.read_bytes()).hexdigest()
                              for path in bank.iterdir()},
                             {**expected, 'overflow.bin': hashlib.sha256(b'!').hexdigest()})

    def test_actual_physical_entry_boundary_preserves_every_directory_on_overflow(self):
        with World() as world:
            bank = world.inside('raw-physical-count'); bank.mkdir()
            expected = {f'd{number:06d}' for number in range(131072)}
            for name in expected:
                (bank / name).mkdir()
            physical, sources, totals = raw_observation.inventory(bank)
            self.assertEqual(totals['physical_entries'], 131072)
            self.assertEqual(totals['source_files'], 0)
            self.assertEqual(sources, [])
            self.assertEqual({row['path'] for row in physical if row['type'] == 'directory'}, expected)
            (bank / 'overflow').mkdir()
            with self.assertRaises(raw_observation.RawCapacityError) as caught:
                raw_observation.inventory(bank)
            self.assertEqual(caught.exception.details, {
                'dimension': 'physical-entries', 'limit': 131072, 'measured': 131073,
                'measurement': 'lower-bound', 'observation_complete': False})
            self.assertEqual({path.name for path in bank.iterdir()}, expected | {'overflow'})
            self.assertTrue(all(path.is_dir() and not any(path.iterdir()) for path in bank.iterdir()))


if __name__ == '__main__':
    unittest.main()
