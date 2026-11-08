"""Real transport/CLI regressions; not native loading or provider qualification."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import time
import unittest
from unittest.mock import patch

from world import World
from contracts import test_omp_boundary as fixtures

transport = fixtures.transport
turn_sources = fixtures.turn_sources
ROOT = Path(__file__).resolve().parents[2]


class EnrollmentProgress(unittest.TestCase):
    def rows(self, repo, expected_faults=()):
        rows, faults = transport.pending_inventory(repo)
        self.assertEqual(faults, list(expected_faults))
        self.assertTrue(all(row['source_reference'].startswith(('omp:capture:', 'omp:seal:')) for row in rows))
        return {row['source_reference']: row for row in rows}

    def _original_enrollment_projection(self, repo):
        """Original full-mint composition, using the unchanged fresh public APIs."""
        from agent_memory import bank_writes, layout_recovery_state, local_registry, recovery_frontier, scope_binding
        from agent_memory.developer_identity import digest, encoded
        root, bank = bank_writes.bank_path(repo)
        turn_sources.check_source_compatibility(layout_recovery_state.paths(root)[2].parent)
        hold = recovery_frontier.verified_hold(root)
        scope = dict(hold.scope) if hold is not None else scope_binding.require(root)
        token = hold.token if hold is not None else recovery_frontier.observe(root)
        pins = {'scope': scope, 'frontier': token,
                'binding': [[str(path), info.st_dev, info.st_ino, info.st_uid, info.st_gid, info.st_mode]
                            for path in (root, bank.parent, bank) for info in [path.lstat()]],
                'marker': digest(hold.marker) if hold is not None else None}
        if hold is not None:
            self.assertEqual((hold.root, hold.bank), (root, bank))
            directory = layout_recovery_state._paths_for_bank(root, bank, hold.operation_id)[3]
            pins.update(runtime_artifact_digest=hold.runtime_artifact_digest,
                        private_bindings_sha256=digest(layout_recovery_state.read_bytes(
                            local_registry.paths('layout-recovery-bindings')[1])),
                        journal_sha256=digest(layout_recovery_state.read_bytes(directory / 'journal.json')))
        return root, bank, scope, hold, pins, digest(encoded(pins))

    def _assert_original_enrollment_composition(self, repo):
        from agent_memory import recovery_frontier, scope_binding
        root, bank, scope, hold, pins, digest = self._original_enrollment_projection(repo)
        actual = scope_binding.private_enrollment_context(repo)
        self.assertEqual((actual.root, actual.bank, actual.scope, actual.held, actual._pins, actual.context_digest),
                         (root, bank, scope, hold is not None, pins, digest))
        if hold is not None:
            self.assertTrue(recovery_frontier.same_hold(actual.hold, hold))
        return actual

    def test_bounded_later_sweeps_enroll_oldest_and_keep_every_new_arrival(self):
        with World() as world:
            helper = fixtures.NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            entries = [helper.entry('human-' + str(i), 'user', 'Ephemeral request ' + str(i),
                                    attribution='user') for i in range(5)]
            with patch.dict(os.environ, world.env, clear=True):
                first = helper.observe(repo, session, lifecycle, entries)
                initial = self.rows(repo)
                self.assertEqual(len(initial), 10)
                self.assertEqual(sum('observation_id' in row for row in initial.values()), 8)
                waiting = {ref for ref, row in initial.items() if 'observation_id' not in row}
                retained = {ref: row['source_sha256'] for ref, row in initial.items()}
                entries += [helper.entry('human-' + str(i), 'user', 'Ephemeral request ' + str(i),
                                         attribution='user') for i in range(5, 9)]
                second = helper.observe(repo, session, first['lifecycle'], entries)
                arrived = self.rows(repo)
                self.assertEqual(len(arrived), 18)
                self.assertEqual(sum('observation_id' in row for row in arrived.values()), 16)
                self.assertTrue(all('observation_id' in arrived[ref] for ref in waiting))
                self.assertEqual(retained, {ref: arrived[ref]['source_sha256'] for ref in initial})
                self.assertTrue(all(arrived[ref].get('observation_id') == row['observation_id']
                                    for ref, row in initial.items() if 'observation_id' in row))
                helper.observe(repo, session, second['lifecycle'], entries)
                finished = self.rows(repo)
                self.assertEqual(set(finished), set(arrived))
                self.assertTrue(all('observation_id' in row for row in finished.values()))
                self.assertEqual({ref: row['source_sha256'] for ref, row in arrived.items()},
                                 {ref: row['source_sha256'] for ref, row in finished.items()})

    def test_lost_success_reuses_observation_and_unknown_attempt_is_not_retried(self):
        # Construct only the interrupted-write boundary. Recovery itself must go
        # through the ordinary observer; this setup is not native-source proof.
        for successful in (False, True):
            with self.subTest(successful=successful), World() as world:
                helper = fixtures.NativeSources()
                repo, session, lifecycle = helper.fixture(world)
                with patch.dict(os.environ, world.env, clear=True):
                    human = helper.entry('human', 'user', 'Ephemeral interrupted source', attribution='user')
                    lifecycle = str(fixtures.uuid.uuid5(fixtures.uuid.NAMESPACE_URL,
                        str(repo.resolve()) + ':omp:' + session + ':' + human['id']))
                    directory, cursor = transport.new_segment(repo, session, lifecycle, lifecycle, 'human', 1)
                    captured = transport.capture(directory, cursor, human)
                    cursor['last_native_entry'] = human['id']
                    cursor['control_interval'] = {'human_entry': human['id'], 'capture_order': [human['id']]}
                    transport.replace_cursor(directory / 'cursor.json', cursor)
                    transport.enroll_admission(directory, cursor, captured)
                    transport.seal(directory, cursor)
                    enrollment = next(transport.read(path, 4096) for path in
                                      (directory / 'enrollments').glob('*.json')
                                      if transport.read(path, 4096)['kind'] == 'admission')
                    attempt_path = directory / 'registration-attempts' / (transport.digest(enrollment['source_reference']) + '.json')
                    transport.retain(attempt_path, {
                        'schema': 'agent-memory/omp-registration-attempt-v1', 'session': session,
                        'lifecycle': lifecycle, 'source_reference': enrollment['source_reference'],
                        'source_sha256': enrollment['source_sha256']}, 4096)
                    original_attempt = attempt_path.read_bytes()
                    observation = None
                    if successful:
                        observation = turn_sources.register(repo, 'omp', session, lifecycle,
                            enrollment['source_sha256'], native_source_id=enrollment['native_source_id'],
                            source_reference=enrollment['source_reference'])
                    helper.observe(repo, session, lifecycle, [], seal=False)
                    recovered = self.rows(repo)[enrollment['source_reference']]
                    if successful:
                        self.assertEqual(recovered['observation_id'], observation['observation_id'])
                    else:
                        self.assertNotIn('observation_id', recovered)
                        self.assertEqual(recovered['faults'], ['registration-fault-retained'])
                    self.assertEqual(attempt_path.read_bytes(), original_attempt)
                    state = turn_sources.inspect(repo, 'omp', session, lifecycle)
                    matching = [source for source in state['sources']
                                if source['source_reference'] == enrollment['source_reference']]
                    self.assertEqual(len(matching), int(successful))
                    self.assertEqual(recovered['source_sha256'], enrollment['source_sha256'])

    def test_shared_fault_retained_across_later_observer_progress(self):
        with World() as world:
            helper = fixtures.NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            entries = [helper.entry('human', 'user', 'Ephemeral capacity fault', attribution='user')]
            with patch.dict(os.environ, world.env, clear=True):
                # Exercise the real shared source-capacity failure, not a mocked
                # registration result. Restoring capacity must not erase/retry it.
                with patch.object(turn_sources, 'MAX_SOURCES', 1):
                    result = helper.observe(repo, session, lifecycle, entries)
                directory, cursor = transport.segments(repo, session)[0]
                _, _, _, shared, _ = transport._shared(repo, cursor)
                fault_path = shared / 'fault.json'
                original_fault = fault_path.read_bytes()
                attempts = {path.name: path.read_bytes() for path in (directory / 'registration-attempts').glob('*.json')}
                retained = self.rows(repo, ['registration-fault-retained'])
                helper.observe(repo, session, result['lifecycle'], entries)
                retried = self.rows(repo, ['registration-fault-retained'])
                self.assertEqual(retried, retained)
                self.assertEqual(fault_path.read_bytes(), original_fault)
                self.assertEqual({path.name: path.read_bytes() for path in
                                  (directory / 'registration-attempts').glob('*.json')}, attempts)
                self.assertTrue(all(row['status'] == 'registration-fault' for row in retried.values()))
                self.assertEqual(len(turn_sources.inspect(repo, 'omp', session, cursor['lifecycle'])['sources']), 1)

    def test_public_observer_seal_and_no_learning_long_segment(self):
        with World() as world:
            helper = fixtures.NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            agent_dir = world.inside('explicit native root')
            agent_dir.mkdir()
            installed = json.loads(world.run([
                sys.executable, '-I', '-B', ROOT / 'skills/agent-memory/scripts/installer.py',
                'install', '--development-source', ROOT, '--presets', 'omp',
                '--omp-agent-dir', agent_dir, '--omp-peers-upgraded',
                '--bin-dir', world.inside('bin')], timeout=60).stdout)
            receipt = installed['receipt']
            am = Path(receipt['bin_dir']) / 'am'

            def public(*arguments, value=None):
                command = [str(am), '--json', 'omp-boundary']
                result = subprocess.run([*command, '--repo', str(repo), *map(str, arguments)],
                                        input=json.dumps(value) if value is not None else None,
                                        text=True, capture_output=True, env=world.env, cwd=repo, timeout=60)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                return json.loads(result.stdout)

            bank = repo / 'memory-bank'
            before = {path.relative_to(bank).as_posix(): path.read_bytes() for path in bank.rglob('*') if path.is_file()}
            index = (repo / '.git/index').read_bytes()
            entries = [helper.entry('human', 'user', 'Compute only ephemeral arithmetic; no project learning.', attribution='user')]
            entries += [helper.entry('assistant-' + str(i), 'assistant', 'Ephemeral computation ' + str(i)) for i in range(150)]
            payload = fixtures.observer_payload(repo, session, entries, terminal=True)
            payload['request_wake'] = False
            start = time.monotonic()
            pack = Path(receipt['source']['pack'])
            completed = fixtures.observe_installed(world, am, repo, receipt, {
                'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal', 'session': session,
                'lifecycle': lifecycle, 'payload': payload}, launcher=[
                    '/bin/sh', pack / 'hooks/python3.sh', pack / 'scripts/omp_boundary.py'])
            self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
            observed = json.loads(completed.stdout)
            elapsed = time.monotonic() - start
            self.assertTrue(observed['verified'])
            pending = public('--list-pending')
            self.assertFalse(pending['faults'])
            self.assertEqual(len(pending['pending']), 2)
            source_proofs = []
            for row in pending['pending']:
                self.assertIn('observation_id', row)
                source = public('--source', row['observation_id'], '--session', row['session'], '--lifecycle', row['lifecycle'])
                self.assertTrue(source['verified'])
                self.assertEqual(source['enrollment']['source_sha256'], row['source_sha256'])
                self.assertEqual(hashlib.sha256(turn_sources.encoded(source['source'])).hexdigest(), row['source_sha256'])
                source_proofs.append({'reference': row['source_reference'], 'observation_id': row['observation_id'],
                                      'source_sha256': row['source_sha256']})
                if row['source_reference'].startswith('omp:seal:'):
                    self.assertEqual(len(source['source']['captures']), len(entries))
                assessment = {key: row[key] for key in ('session', 'lifecycle', 'observation_id', 'source_sha256')}
                assessment.update(outcome='no-learning', assessment={'source_id': row['observation_id'],
                    'source_review': 'Read the public retained arithmetic-only source and its exact capture manifest; no durable project fact or instruction.',
                    'candidates': []})
                self.assertTrue(public('--stdin', value=assessment)['verified'])
            self.assertEqual(public('--list-pending')['pending'], [])
            self.assertEqual({path.relative_to(bank).as_posix(): path.read_bytes() for path in bank.rglob('*') if path.is_file()}, before)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            print(json.dumps({'proof': 'public-observer-and-no-learning', 'elapsed_seconds': elapsed,
                              'retained_entries': len(entries), 'sources': source_proofs,
                              'native_loading': 'not-exercised', 'model_provider': 'not-exercised'}, sort_keys=True))

    def test_transaction_local_read_reuse_requires_exact_live_context_and_preserves_inventory(self):
        from agent_memory import bank_writes, layout_recovery_state, scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            result = helper.observe(repo, session, lifecycle,
                [helper.entry('human', 'user', 'Actual ephemeral source.', attribution='user')])
            lifecycle = result['lifecycle']
            expected = turn_sources.inspect(repo, 'omp', session, lifecycle)
            context = scope_binding.private_enrollment_context(repo)
            with scope_binding._private_enrollment_reads(context):
                with self.assertRaisesRegex(ValueError, 'active source barrier'):
                    turn_sources._inspect_enrollment(repo, 'omp', session, lifecycle, context)
            with turn_sources.source_barrier(repo), scope_binding._private_enrollment_reads(context):
                with patch.object(scope_binding, 'private_enrollment_context',
                                  wraps=scope_binding.private_enrollment_context) as discovery:
                    for _ in range(3):
                        self.assertEqual(turn_sources._inspect_enrollment(repo, 'omp', session, lifecycle, context), expected)
                    self.assertEqual(discovery.call_count, 0)
                with self.assertRaises(ValueError):
                    fixtures.boundary._base(repo, enrollment_context=context)
                fresh_base = fixtures.boundary._base(repo)
                with scope_binding._observe_enrollment_locations(context):
                    with patch.object(bank_writes, 'bank_path', wraps=bank_writes.bank_path) as locator:
                        self.assertEqual(fixtures.boundary._base(repo, enrollment_context=context), fresh_base)
                        self.assertEqual(transport.session_dir(repo, session, enrollment_context=context),
                            fresh_base / 'omp-source-v1' / hashlib.sha256(session.encode()).hexdigest())
                        self.assertEqual(locator.call_count, 0)
                        self.assertEqual(fixtures.boundary._base(repo), fresh_base)
                        self.assertEqual(locator.call_count, 1)
                        scope_binding.recheck_private_enrollment(context)
                        self.assertGreater(locator.call_count, 1, 'Full scope checks must freshly locate the bank')
                    with patch.object(layout_recovery_state, 'paths', wraps=layout_recovery_state.paths) as locator:
                        with turn_sources.source_barrier(repo):
                            self.assertEqual(scope_binding._observe_enrollment_location(repo)[:2], (context.root, context.bank))
                        self.assertEqual(locator.call_count, 0)
                self.assertIsNone(scope_binding._observe_enrollment_location(repo))
                with self.assertRaises(ValueError):
                    turn_sources._inspect_enrollment(world.inside('foreign'), 'omp', session, lifecycle, context)
                for invalid in ({'project_id': context.project_id}, object()):
                    with self.assertRaises(ValueError):
                        turn_sources._inspect_enrollment(repo, 'omp', session, lifecycle, invalid)
                    with self.assertRaises(ValueError):
                        turn_sources.register(repo, 'omp', session, lifecycle, 'b' * 64,
                                              native_source_id='not-written', _enrollment_context=invalid)
                # Even an actual minted object cannot redirect its reused locator
                # or claim a hold while retaining the original pin dictionary.
                original_bank = context.bank
                try:
                    context.bank = world.inside('foreign-bank')
                    with self.assertRaises(ValueError):
                        scope_binding.recheck_private_enrollment(context)
                finally:
                    context.bank = original_bank
                try:
                    context.hold = object()
                    with self.assertRaises(ValueError):
                        scope_binding.recheck_private_enrollment(context)
                finally:
                    context.hold = None
            with self.assertRaises(ValueError):
                turn_sources._inspect_enrollment(repo, 'omp', session, lifecycle, context)
            self.assertEqual(turn_sources.inspect(repo, 'omp', session, lifecycle), expected)

    def test_changed_configured_bank_before_reused_registration_writes_neither_namespace(self):
        from agent_memory import bank_writes, scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            context = scope_binding.private_enrollment_context(repo)
            original_namespace = turn_sources.private_namespace(repo, 'omp', session, lifecycle)[3]
            alternate_bank = repo / 'alternate-bank'; alternate_bank.mkdir()
            alternate_base = bank_writes.state_paths(repo, alternate_bank, str(fixtures.uuid.uuid4()))[0].parent
            relative = original_namespace.relative_to(original_namespace.parents[2])
            alternate_namespace = alternate_base / relative
            def snapshot(directory):
                return (directory.exists(), {str(path.relative_to(directory)): path.read_bytes()
                        for path in directory.rglob('*') if path.is_file()})
            before = snapshot(original_namespace), snapshot(alternate_namespace)
            config = repo / '.git/config'; original = config.read_bytes()
            with turn_sources.source_barrier(repo), scope_binding._private_enrollment_reads(context):
                try:
                    world.git('config', '--local', 'agent-memory.bankRoot', 'alternate-bank', cwd=repo)
                    with self.assertRaises(ValueError):
                        turn_sources.register(repo, 'omp', session, lifecycle, 'a' * 64,
                            native_source_id='actual-native-source', _enrollment_context=context)
                finally:
                    config.write_bytes(original)
            self.assertEqual((snapshot(original_namespace), snapshot(alternate_namespace)), before)

    def test_transaction_end_fresh_recheck_refuses_late_config_after_read_only_reuse(self):
        from agent_memory import scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            context = scope_binding.private_enrollment_context(repo)
            config = repo / '.git/config'; original = config.read_bytes()
            try:
                with self.assertRaises(ValueError):
                    with turn_sources.source_barrier(repo), scope_binding._private_enrollment_reads(context):
                        world.git('config', '--local', 'agent-memory.authorityRemote', 'missing-native-authority', cwd=repo)
                        # Read-only locator reuse can retain the old private view,
                        # but cannot return a qualified transaction after a changed scope.
                        turn_sources._inspect_enrollment(repo, 'omp', session, lifecycle, context)
            finally:
                config.write_bytes(original)
            self.assertFalse(turn_sources.private_namespace(repo, 'omp', session, lifecycle)[3].exists())
            with self.assertRaises(ValueError):
                turn_sources._inspect_enrollment(repo, 'omp', session, lifecycle, context)

    def test_reused_locator_keeps_post_source_mutation_scope_check_and_original_fault(self):
        from agent_memory import scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            context = scope_binding.private_enrollment_context(repo)
            shared = turn_sources.private_namespace(repo, 'omp', session, lifecycle)[3]
            config = repo / '.git/config'; original = config.read_bytes()
            exclusive = turn_sources.exclusive; written = []
            def changed_after_source(path, value, maximum):
                result = exclusive(path, value, maximum)
                if path.parent == shared and path.name.startswith('source-'):
                    written.append(value)
                    world.git('config', '--local', 'agent-memory.authorityRemote', 'missing-native-authority', cwd=repo)
                return result
            try:
                with self.assertRaises(ValueError), turn_sources.source_barrier(repo), scope_binding._private_enrollment_reads(context):
                    with patch.object(turn_sources, 'exclusive', side_effect=changed_after_source):
                        turn_sources.register(repo, 'omp', session, lifecycle, 'a' * 64,
                            native_source_id='actual-source', source_reference='omp:controlled-source',
                            _enrollment_context=context)
            finally:
                config.write_bytes(original)
            self.assertEqual(len(written), 1)
            self.assertEqual(turn_sources.read(shared / 'source-0.json', turn_sources.MAX_SOURCE), written[0])
            self.assertEqual(turn_sources.read(shared / 'fault.json', turn_sources.MAX_SOURCE)['reason'], 'registration-unverified')
            self.assertFalse((shared / 'source-1.json').exists())

    def test_held_read_reuse_keeps_exact_intent_pins_and_refuses_changed_hold_without_new_source(self):
        from contracts.test_held_source_enrollment import HeldSourceEnrollment
        from agent_memory import scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, bank, _, _, _, _ = HeldSourceEnrollment().fixture(world)
            context = scope_binding.private_enrollment_context(repo)
            session = fixtures.uuid.uuid4().hex; lifecycle = fixtures.uuid.uuid4().hex
            marker = bank / '.agent-memory-transaction.md'; original = marker.read_bytes()
            try:
                with self.assertRaises(ValueError), turn_sources.source_barrier(repo), scope_binding._private_enrollment_reads(context):
                    observed = turn_sources.register(repo, 'claude', session, lifecycle, 'a' * 64,
                        native_source_id='actual-held-native', source_reference='controlled-held-source',
                        _enrollment_context=context)
                    _, _, _, shared, identity = turn_sources._enrollment_namespace(repo, 'claude', session, lifecycle, context)
                    intent = turn_sources.enrollment_inventory(shared, identity)[0]
                    self.assertEqual(intent['context_digest'], context.context_digest)
                    self.assertEqual(intent['observation_id'], observed['observation_id'])
                    self.assertEqual(intent['state'], 'registered')
                    with self.assertRaises(ValueError):
                        turn_sources.namespace(repo, 'claude', session, lifecycle)
                    changed = json.loads(original); changed['hold_id'] = str(fixtures.uuid.uuid4())
                    marker.write_bytes(turn_sources.encoded(changed))
                    turn_sources.register(repo, 'claude', session, lifecycle, 'b' * 64,
                        native_source_id='different-held-native', source_reference='different-controlled-source',
                        _enrollment_context=context)
            finally:
                marker.write_bytes(original)
            state = turn_sources.inspect(repo, 'claude', session, lifecycle)
            self.assertEqual([item['observation_id'] for item in state['sources']], [observed['observation_id']])
            self.assertEqual(len(turn_sources.enrollment_inventory(shared, identity)), 1)

    def test_observe_bank_drift_keeps_original_captures_refuses_final_and_next_frame_freshly_locates(self):
        import shutil
        from agent_memory import bank_writes, scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, session, lifecycle = helper.fixture(world)
            original_bank = repo / 'memory-bank'; alternate_bank = repo / 'alternate-bank'
            original_base = fixtures.boundary._base(repo)
            alternate_base = bank_writes.state_paths(repo, alternate_bank, str(fixtures.uuid.uuid4()))[0].parent
            entries = [helper.entry('first-human', 'user', 'First actual stored source.', attribution='user'),
                       helper.entry('second-human', 'user', 'Second actual stored source.', attribution='user')]
            capture = transport.capture; bank_path = bank_writes.bank_path
            drifted = []; full_locations = []; started_locations = []
            def capture_then_change_config(directory, cursor, entry, **kwargs):
                result = capture(directory, cursor, entry, **kwargs)
                if result is not None and not drifted:
                    shutil.copytree(original_bank, alternate_bank)
                    world.git('config', '--local', 'agent-memory.bankRoot', 'alternate-bank', cwd=repo)
                    drifted.append(True)
                return result
            def fresh_location(path):
                if drifted:
                    full_locations.append(scope_binding._observe_enrollment_location(path))
                return bank_path(path)
            value = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal',
                     'session': session, 'lifecycle': lifecycle,
                     'payload': fixtures.observer_payload(repo, session, entries, terminal=True)}
            value['payload']['request_wake'] = False
            with patch.object(transport, 'capture', side_effect=capture_then_change_config), \
                    patch.object(bank_writes, 'bank_path', side_effect=fresh_location):
                with self.assertRaisesRegex(ValueError, 'Competing memory bank locations'):
                    helper.observe_request(repo, value,
                        on_started=lambda: started_locations.append(scope_binding._observe_enrollment_location(repo)))
            self.assertEqual(started_locations, [None], 'Worker start must precede observer locator reuse')
            self.assertTrue(full_locations)
            self.assertTrue(all(location is None for location in full_locations),
                            'Full scope checks must suspend the transaction locator')
            source_root = original_base / 'omp-source-v1' / hashlib.sha256(session.encode()).hexdigest()
            retained = {transport.read(path, 4096)['native_entry_id'] for path in source_root.glob('*/captures/*.json')}
            self.assertEqual(retained, {'first-human', 'second-human'})
            self.assertFalse((alternate_base / 'omp-source-v1').exists())
            self.assertFalse((alternate_base / 'turn-sources-v2').exists())
            self.assertIsNone(scope_binding._observe_enrollment_location(repo))
            # Remove only the competing locator from this owned fixture, retaining
            # its complete original canonical bank outside the Git checkout.
            original_bank.rename(world.inside('retained-original-bank'))
            self.assertEqual(fixtures.boundary._base(repo), alternate_base)
            self.assertEqual(transport.session_dir(repo, session),
                alternate_base / 'omp-source-v1' / hashlib.sha256(session.encode()).hexdigest())
            started = []
            with self.assertRaises(ValueError):
                helper.observe_request(repo, value, on_started=lambda: started.append(True))
            self.assertEqual(started, [], 'The unactivated new bank must refuse before source execution')
            self.assertFalse((alternate_base / 'omp-source-v1').exists())
            self.assertFalse((alternate_base / 'turn-sources-v2').exists())

    def test_fresh_mint_matches_original_composition_in_regular_and_newline_worktree(self):
        import shutil
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, _, _ = helper.fixture(world)
            ordinary = self._assert_original_enrollment_composition(repo)
            worktree = world.inside('linked worktree with space\n')
            world.git('worktree', 'add', '--detach', worktree, 'HEAD', cwd=repo)
            shutil.copytree(repo / 'memory-bank', worktree / 'memory-bank')
            linked = self._assert_original_enrollment_composition(worktree)
            self.assertEqual(linked.root, worktree.resolve())
            self.assertEqual(linked.project_id, ordinary.project_id)
            self.assertEqual(linked.scope['checkout_key'], ordinary.scope['checkout_key'])

    def test_fresh_mint_matches_original_composition_for_actual_verified_hold(self):
        from contracts.test_held_source_enrollment import HeldSourceEnrollment
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, _, _, _, _, _ = HeldSourceEnrollment().fixture(world)
            context = self._assert_original_enrollment_composition(repo)
            self.assertTrue(context.held)
            self.assertIsNotNone(context._pins['marker'])
            self.assertEqual(context._pins['runtime_artifact_digest'], context.hold.runtime_artifact_digest)

    def test_fresh_mint_and_original_composition_both_refuse_foreign_bank_identity(self):
        import shutil
        from agent_memory import scope_binding
        with World() as world, World() as foreign, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, _, _ = helper.fixture(world)
            with patch.dict(os.environ, foreign.env, clear=True):
                foreign_repo, _, _ = fixtures.NativeSources().fixture(foreign)
            (repo / 'memory-bank').rename(world.inside('retained-original-identity-bank'))
            shutil.copytree(foreign_repo / 'memory-bank', repo / 'foreign-bank')
            world.git('config', '--local', 'agent-memory.bankRoot', 'foreign-bank', cwd=repo)
            before = {str(path.relative_to(world.paths['state'])): path.read_bytes()
                      for path in world.paths['state'].rglob('*') if path.is_file()}
            failures = []
            for mint in (self._original_enrollment_projection, scope_binding.private_enrollment_context):
                with self.assertRaises(ValueError) as refused:
                    mint(repo)
                failures.append(str(refused.exception))
            self.assertEqual(failures[0], failures[1])
            self.assertEqual({str(path.relative_to(world.paths['state'])): path.read_bytes()
                              for path in world.paths['state'].rglob('*') if path.is_file()}, before)

    def test_fresh_mint_and_original_composition_refuse_bank_drift_after_actual_scope_read(self):
        import shutil
        from agent_memory import scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, _, _ = helper.fixture(world)
            config = repo / '.git/config'; original = config.read_bytes()
            require = scope_binding.require
            alternate = repo / 'late-bank'
            for index, mint in enumerate((self._original_enrollment_projection, scope_binding.private_enrollment_context)):
                changed = []
                def scope_then_drift(*args, **kwargs):
                    result = require(*args, **kwargs)
                    if not changed:
                        shutil.copytree(repo / 'memory-bank', alternate)
                        world.git('config', '--local', 'agent-memory.bankRoot', 'late-bank', cwd=repo)
                        changed.append(True)
                    return result
                try:
                    with patch.object(scope_binding, 'require', side_effect=scope_then_drift):
                        with self.assertRaisesRegex(ValueError, 'Competing memory bank locations'):
                            mint(repo)
                    self.assertTrue(changed, 'The actual scope read must precede the real configuration mutation')
                finally:
                    config.write_bytes(original)
                    if alternate.exists():
                        alternate.rename(world.inside('retained-late-bank-' + str(index)))

    def _assert_raw_locator_refusal_preserved(self, *, held, kind):
        from agent_memory import bank_writes, layout_recovery_state, recovery_frontier, scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            if held:
                from contracts.test_held_source_enrollment import HeldSourceEnrollment
                _, repo, bank, _, _, _, _ = HeldSourceEnrollment().fixture(world)
            else:
                repo, _, _ = fixtures.NativeSources().fixture(world)
                bank = repo / 'memory-bank'
            self.assertEqual(recovery_frontier.verified_hold(repo) is not None, held,
                             'The original fixture must have its actual expected hold before adding a locator')
            stray = repo / 'raw-only' / 'MEMORY.md'; stray.parent.mkdir()
            plain = '# Raw locator fixture\n\nDeliberately not a memory router.\n'
            if kind == 'symlink':
                target = repo / 'raw-locator-target.txt'; target.write_text(plain)
                stray.symlink_to(target)
                with self.assertRaises(ValueError):
                    bank_writes.bank_path(repo)
            else:
                stray.write_text(plain)
                if kind == 'committed-missing':
                    relative = stray.relative_to(repo).as_posix()
                    world.git('add', '--', relative, cwd=repo)
                    world.git('commit', '-m', 'Retain synthetic raw-only locator', '--', relative, cwd=repo)
                    stray.unlink()
                self.assertEqual(bank_writes.bank_path(repo), (repo.resolve(), bank))
            with self.assertRaises(ValueError):
                layout_recovery_state.paths(repo)
            before = ({str(path.relative_to(world.paths['state'])): path.read_bytes()
                       for path in world.paths['state'].rglob('*') if path.is_file()},
                      {str(path.relative_to(bank)): path.read_bytes()
                       for path in bank.rglob('*') if path.is_file()},
                      (repo / '.git/index').read_bytes(),
                      world.git('rev-parse', 'HEAD', cwd=repo).stdout)
            failures = []
            for mint in (self._original_enrollment_projection, scope_binding.private_enrollment_context):
                with self.assertRaises(ValueError) as refused:
                    mint(repo)
                failures.append(str(refused.exception))
            self.assertEqual(failures[0], failures[1])
            if kind != 'symlink':
                self.assertIn('Competing memory bank locations', failures[0])
            self.assertEqual(
                ({str(path.relative_to(world.paths['state'])): path.read_bytes()
                  for path in world.paths['state'].rglob('*') if path.is_file()},
                 {str(path.relative_to(bank)): path.read_bytes()
                  for path in bank.rglob('*') if path.is_file()},
                 (repo / '.git/index').read_bytes(),
                 world.git('rev-parse', 'HEAD', cwd=repo).stdout), before)

    def test_fresh_mint_refuses_successful_but_disagreeing_bank_locators(self):
        from agent_memory import bank_writes, layout_recovery_state, scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, _, _ = fixtures.NativeSources().fixture(world)
            bank = repo / 'memory-bank'
            (repo / '.git/info/exclude').write_text('/memory-bank/\n')
            stray = repo / 'raw-only' / 'MEMORY.md'; stray.parent.mkdir()
            stray.write_text('# Unrelated document\n\nNot a memory router.\n')
            self.assertEqual(world.git('ls-files', '--', 'memory-bank', cwd=repo).stdout, '')
            self.assertEqual(bank_writes.bank_path(repo), (repo.resolve(), bank))
            self.assertEqual(layout_recovery_state.paths(repo)[:2], (repo.resolve(), stray.parent))
            # Deliberate tightening: the original composition succeeded while
            # combining the ordinary bank's scope with the other bank's frontier.
            original = self._original_enrollment_projection(repo)
            self.assertEqual(original[:2], (repo.resolve(), bank))
            self.assertIsNone(original[3])
            def preserved():
                return ({str(path.relative_to(world.paths['state'])): path.read_bytes()
                         for path in world.paths['state'].rglob('*') if path.is_file()},
                        {str(path.relative_to(repo)): path.read_bytes()
                         for path in repo.rglob('*') if path.is_file()},
                        world.git('rev-parse', 'HEAD', cwd=repo).stdout)
            before = preserved()
            with self.assertRaises(ValueError):
                scope_binding.private_enrollment_context(repo)
            self.assertEqual(preserved(), before)

    def test_fresh_mint_preserves_raw_nonrouter_refusal_with_and_without_verified_hold(self):
        for held in (False, True):
            with self.subTest(held=held):
                self._assert_raw_locator_refusal_preserved(held=held, kind='untracked')

    def test_fresh_mint_preserves_committed_raw_locator_refusal_with_and_without_verified_hold(self):
        for held in (False, True):
            with self.subTest(held=held):
                self._assert_raw_locator_refusal_preserved(held=held, kind='committed-missing')

    def test_fresh_mint_preserves_symlink_locator_refusal_with_and_without_verified_hold(self):
        for held in (False, True):
            with self.subTest(held=held):
                self._assert_raw_locator_refusal_preserved(held=held, kind='symlink')

    def test_fresh_mint_preserves_late_raw_locator_refusal_with_and_without_verified_hold(self):
        from agent_memory import bank_writes, recovery_frontier, scope_binding
        for held in (False, True):
            with self.subTest(held=held), World() as world, patch.dict(os.environ, world.env, clear=True):
                if held:
                    from contracts.test_held_source_enrollment import HeldSourceEnrollment
                    _, repo, bank, _, _, _, _ = HeldSourceEnrollment().fixture(world)
                else:
                    repo, _, _ = fixtures.NativeSources().fixture(world)
                    bank = repo / 'memory-bank'
                self.assertEqual(recovery_frontier.verified_hold(repo) is not None, held)
                require = scope_binding.require
                stray = repo / 'late-raw-locator' / 'MEMORY.md'
                for index, mint in enumerate((self._original_enrollment_projection, scope_binding.private_enrollment_context)):
                    changed = []
                    def scope_then_add_raw_locator(*args, **kwargs):
                        result = require(*args, **kwargs)
                        if not changed:
                            stray.parent.mkdir()
                            stray.write_text('# Late raw locator\n\nNot a router.\n')
                            changed.append(True)
                        return result
                    try:
                        with patch.object(scope_binding, 'require', side_effect=scope_then_add_raw_locator):
                            with self.assertRaisesRegex(ValueError, 'Competing memory bank locations'):
                                mint(repo)
                        self.assertTrue(changed)
                        self.assertEqual(bank_writes.bank_path(repo), (repo.resolve(), bank),
                                         'The ordinary locator must still accept the original bank')
                    finally:
                        if stray.parent.exists():
                            stray.parent.rename(world.inside('retained-late-raw-locator-' + str(index)))


if __name__ == '__main__':
    unittest.main()
