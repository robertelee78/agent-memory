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

    def assert_pending_source_retained(self, repo, session, lifecycle, original):
        sources = turn_sources.inspect(repo, 'omp', session, lifecycle)['sources']
        self.assertEqual(len(sources), 1)
        self.assertEqual(sources[0]['observation_id'], original['observation_id'])
        self.assertEqual(sources[0]['source_sha256'], original['source_sha256'])
        self.assertEqual(sources[0]['native_source_id'], original['native_source_id'])
        self.assertFalse(sources[0]['completed'])


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
                              'native_loading': 'not-exercised', 'model_provider': 'not-exercised'}, sort_keys=True),
                  file=sys.stderr)

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
            capture = transport.capture
            drifted = []; started = []
            def capture_then_change_config(directory, cursor, entry, **kwargs):
                result = capture(directory, cursor, entry, **kwargs)
                if result is not None and not drifted:
                    shutil.copytree(original_bank, alternate_bank)
                    world.git('config', '--local', 'agent-memory.bankRoot', 'alternate-bank', cwd=repo)
                    drifted.append(True)
                return result
            value = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal',
                     'session': session, 'lifecycle': lifecycle,
                     'payload': fixtures.observer_payload(repo, session, entries, terminal=True)}
            value['payload']['request_wake'] = False
            with patch.object(transport, 'capture', side_effect=capture_then_change_config):
                with self.assertRaises(ValueError):
                    helper.observe_request(repo, value, on_started=lambda: started.append(True))
            self.assertEqual(started, [True])
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

    def test_fresh_mint_resolves_regular_and_newline_worktree_identity(self):
        import shutil
        from agent_memory import scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, _, _ = helper.fixture(world)
            ordinary = scope_binding.private_enrollment_context(repo)
            worktree = world.inside('linked worktree with space\n')
            world.git('worktree', 'add', '--detach', worktree, 'HEAD', cwd=repo)
            shutil.copytree(repo / 'memory-bank', worktree / 'memory-bank')
            linked = scope_binding.private_enrollment_context(worktree)
            self.assertEqual(linked.root, worktree.resolve())
            self.assertEqual(linked.project_id, ordinary.project_id)
            self.assertEqual(linked.scope['checkout_key'], ordinary.scope['checkout_key'])


    def test_fresh_mint_refuses_foreign_bank_identity_without_private_writes(self):
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
            with self.assertRaises(ValueError):
                scope_binding.private_enrollment_context(repo)
            self.assertEqual({str(path.relative_to(world.paths['state'])): path.read_bytes()
                              for path in world.paths['state'].rglob('*') if path.is_file()}, before)

    def test_fresh_mint_refuses_bank_drift_after_actual_scope_read(self):
        import shutil
        from agent_memory import scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = fixtures.NativeSources(); repo, _, _ = helper.fixture(world)
            config = repo / '.git/config'; original = config.read_bytes()
            require = scope_binding.require
            alternate = repo / 'late-bank'
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
                        scope_binding.private_enrollment_context(repo)
                self.assertTrue(changed, 'The actual scope read must precede the real configuration mutation')
            finally:
                config.write_bytes(original)
                if alternate.exists():
                    alternate.rename(world.inside('retained-late-bank'))

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
            with self.assertRaises(ValueError):
                scope_binding.private_enrollment_context(repo)
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
            # Successful locators must still agree on the same namespace.
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
                        with self.assertRaises(ValueError):
                            scope_binding.private_enrollment_context(repo)
                    self.assertTrue(changed)
                    self.assertEqual(bank_writes.bank_path(repo), (repo.resolve(), bank))
                finally:
                    if stray.parent.exists():
                        stray.parent.rename(world.inside('retained-late-raw-locator'))


    def test_next_mint_refuses_a_new_raw_only_bank_locator(self):
        from agent_memory import scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, _, _ = fixtures.NativeSources().fixture(world)
            scope_binding.private_enrollment_context(repo)
            stray = repo / 'next-observation' / 'MEMORY.md'
            stray.parent.mkdir()
            stray.write_text('# A raw-only locator created between observations.\n')
            with self.assertRaisesRegex(ValueError, 'Competing memory bank locations'):
                scope_binding.private_enrollment_context(repo)
            self.assertEqual(stray.read_text(), '# A raw-only locator created between observations.\n')

    def test_live_marker_context_cannot_escape_its_read_lifetime(self):
        from agent_memory import scope_binding
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, session, lifecycle = fixtures.NativeSources().fixture(world)
            observation = fixtures.boundary._observe_marker(repo)
            with turn_sources.source_barrier(repo), observation as context:
                state = turn_sources._inspect_enrollment(repo, 'omp', session, lifecycle, context)
                self.assertEqual(state['sources'], [])
            with self.assertRaises(ValueError):
                turn_sources._inspect_enrollment(repo, 'omp', session, lifecycle, context)

    def test_live_marker_observation_refuses_late_authority_change_without_private_writes(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, session, lifecycle = fixtures.NativeSources().fixture(world)
            config = repo / '.git/config'; original = config.read_bytes()
            before = {str(path.relative_to(world.paths['state'])): path.read_bytes()
                      for path in world.paths['state'].rglob('*') if path.is_file()}
            try:
                with self.assertRaises(ValueError):
                    with turn_sources.source_barrier(repo), fixtures.boundary._observe_marker(repo) as context:
                        world.git('config', '--local', 'agent-memory.authorityRemote',
                                  'missing-late-authority', cwd=repo)
                        turn_sources._inspect_enrollment(repo, 'omp', session, lifecycle, context)
            finally:
                config.write_bytes(original)
            self.assertEqual({str(path.relative_to(world.paths['state'])): path.read_bytes()
                              for path in world.paths['state'].rglob('*') if path.is_file()}, before)

    def test_sparse_pending_index_reads_last_slot_and_allocates_lowest_hole(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, session, lifecycle = fixtures.NativeSources().fixture(world)
            first = turn_sources.register(repo, 'omp', session, lifecycle, 'a' * 64,
                                          native_source_id='first-index-source')
            root, _, base, directory, identity = turn_sources.private_namespace(repo, 'omp', session, lifecycle)
            pending = base / 'pending'; last = pending / (str(turn_sources.MAX_TURNS - 1) + '.json')
            (pending / '0.json').rename(last)
            retained = last.read_bytes()
            # These were never fixed numeric slots and must not reserve holes.
            for name in ('01.json', '-1.json', '4096.json', 'unrelated'):
                (pending / name).write_text('unrelated existing name\n')
            self.assertEqual(turn_sources.index(base, directory, identity, root), last)
            second = turn_sources.register(repo, 'omp', session + '-next', lifecycle, 'b' * 64,
                                           native_source_id='second-index-source')
            self.assertTrue((pending / '0.json').is_file())
            self.assertEqual(last.read_bytes(), retained)
            self.assert_pending_source_retained(repo, session, lifecycle, first)
            self.assert_pending_source_retained(repo, session + '-next', lifecycle, second)

    def test_pending_index_refuses_malformed_or_symlink_last_slot_before_new_source(self):
        for kind in ('malformed', 'symlink'):
            with self.subTest(kind=kind), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo, session, lifecycle = fixtures.NativeSources().fixture(world)
                first = turn_sources.register(repo, 'omp', session, lifecycle, 'a' * 64,
                                              native_source_id='retained-index-source')
                _, _, base, _, _ = turn_sources.private_namespace(repo, 'omp', session, lifecycle)
                pending = base / 'pending'; retained = (pending / '0.json').read_bytes()
                last = pending / (str(turn_sources.MAX_TURNS - 1) + '.json')
                if kind == 'symlink':
                    last.symlink_to(pending / '0.json')
                else:
                    last.write_bytes(b'not-json\n'); last.chmod(0o600)
                with self.assertRaises(ValueError):
                    turn_sources.register(repo, 'omp', session + '-new', lifecycle, 'b' * 64,
                                          native_source_id='must-not-be-written')
                _, _, _, new_directory, _ = turn_sources.private_namespace(repo, 'omp', session + '-new', lifecycle)
                self.assertFalse((new_directory / 'source-0.json').exists())
                self.assertEqual(turn_sources.read(new_directory / 'fault.json', 4096)['reason'],
                                 'registration-unverified')
                self.assertEqual((pending / '0.json').read_bytes(), retained)
                self.assert_pending_source_retained(repo, session, lifecycle, first)

    def test_full_pending_index_preserves_actual_4096_slot_capacity(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, session, lifecycle = fixtures.NativeSources().fixture(world)
            turn_sources.register(repo, 'omp', session, lifecycle, 'a' * 64,
                                  native_source_id='retained-capacity-source')
            _, _, base, _, _ = turn_sources.private_namespace(repo, 'omp', session, lifecycle)
            pending = base / 'pending'; reference = (pending / '0.json').read_bytes()
            self.assertEqual(turn_sources.MAX_TURNS, 4096)
            for number in range(1, turn_sources.MAX_TURNS):
                path = pending / (str(number) + '.json')
                path.write_bytes(reference); path.chmod(0o600)
            with self.assertRaisesRegex(ValueError, 'Pending turn index full'):
                turn_sources.register(repo, 'omp', session + '-new', lifecycle, 'b' * 64,
                                      native_source_id='must-not-exceed-capacity')
            _, _, _, new_directory, _ = turn_sources.private_namespace(repo, 'omp', session + '-new', lifecycle)
            self.assertFalse((new_directory / 'source-0.json').exists())
            self.assertEqual(len(list(pending.iterdir())), 4096)
            self.assertEqual((pending / '4095.json').read_bytes(), reference)

    def test_pending_index_concurrent_slot_creation_keeps_both_sources_and_next_hole(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, session, lifecycle = fixtures.NativeSources().fixture(world)
            first = turn_sources.register(repo, 'omp', session, lifecycle, 'a' * 64,
                                          native_source_id='original-racing-source')
            _, _, base, _, _ = turn_sources.private_namespace(repo, 'omp', session, lifecycle)
            pending = base / 'pending'; last = pending / (str(turn_sources.MAX_TURNS - 1) + '.json')
            (pending / '0.json').rename(last)
            old_reference = turn_sources.read(last, 4096)
            exclusive = turn_sources.exclusive; attempted = []
            def create_competing_slot(path, value, maximum):
                if path.parent == pending:
                    attempted.append(path.name)
                    if len(attempted) == 1:
                        exclusive(path, old_reference, maximum)
                return exclusive(path, value, maximum)
            with patch.object(turn_sources, 'exclusive', side_effect=create_competing_slot):
                second = turn_sources.register(repo, 'omp', session + '-next', lifecycle, 'b' * 64,
                                               native_source_id='new-racing-source')
            self.assertEqual(attempted, ['0.json', '1.json'])
            self.assertEqual(turn_sources.read(pending / '0.json', 4096), old_reference)
            self.assert_pending_source_retained(repo, session, lifecycle, first)
            self.assert_pending_source_retained(repo, session + '-next', lifecycle, second)

    def test_pending_index_retries_transient_scan_change_and_refuses_persistent_churn(self):
        from contextlib import contextmanager
        for persistent in (False, True):
            with self.subTest(persistent=persistent), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo, session, lifecycle = fixtures.NativeSources().fixture(world)
                first = turn_sources.register(repo, 'omp', session, lifecycle, 'a' * 64,
                                              native_source_id='original-scanned-source')
                _, _, base, _, _ = turn_sources.private_namespace(repo, 'omp', session, lifecycle)
                pending = base / 'pending'; scandir = os.scandir; changed = []
                @contextmanager
                def changing_scan(path):
                    with scandir(path) as entries:
                        if persistent or not changed:
                            before = pending.stat()
                            (pending / ('late-unrelated-' + str(len(changed)))).write_text('retained concurrent entry\n')
                            # Force an observed change regardless of directory iteration
                            # order or filesystem timestamp granularity.
                            os.utime(pending, ns=(before.st_atime_ns, before.st_mtime_ns + 2_000_000_000))
                            changed.append(True)
                        yield entries
                def intercept(path):
                    return changing_scan(path) if Path(path) == pending else scandir(path)
                with patch.object(os, 'scandir', side_effect=intercept):
                    if persistent:
                        with self.assertRaises(ValueError):
                            turn_sources.register(repo, 'omp', session + '-new', lifecycle, 'b' * 64,
                                                  native_source_id='must-not-follow-incomplete-scan')
                    else:
                        second = turn_sources.register(repo, 'omp', session + '-new', lifecycle, 'b' * 64,
                                                       native_source_id='retained-after-transient-scan')
                _, _, _, new_directory, _ = turn_sources.private_namespace(repo, 'omp', session + '-new', lifecycle)
                if persistent:
                    self.assertFalse((new_directory / 'source-0.json').exists())
                    self.assertFalse((pending / '1.json').exists())
                else:
                    self.assertTrue((pending / '1.json').is_file())
                    self.assertFalse((new_directory / 'fault.json').exists())
                    self.assert_pending_source_retained(repo, session + '-new', lifecycle, second)
                self.assert_pending_source_retained(repo, session, lifecycle, first)

    def test_pending_directory_overflow_retains_existing_source_without_allocating(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, session, lifecycle = fixtures.NativeSources().fixture(world)
            first = turn_sources.register(repo, 'omp', session, lifecycle, 'a' * 64,
                                          native_source_id='retained-directory-overflow-source')
            root, _, base, directory, identity = turn_sources.private_namespace(repo, 'omp', session, lifecycle)
            pending = base / 'pending'
            retained = (pending / '0.json').read_bytes()
            for number in range(6):
                (pending / ('unrelated-' + str(number))).write_text('retained unrelated entry\n')
            with patch.object(turn_sources, 'MAX_TURNS', 3):
                with self.assertRaisesRegex(ValueError, 'inventory bound'):
                    turn_sources.index(base, directory, identity, root)
            self.assertFalse((pending / '1.json').exists())
            self.assertEqual((pending / '0.json').read_bytes(), retained)
            self.assert_pending_source_retained(repo, session, lifecycle, first)


if __name__ == '__main__':
    unittest.main()
