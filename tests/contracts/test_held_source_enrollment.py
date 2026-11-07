"""Real held identity/enrollment, exact retry and source-preservation regressions.

The fixture constructs retained reviewed recovery state, not a layout-apply or
native-provider acceptance receipt. Hooks and private readers execute unchanged.
"""
import base64
import copy
import hashlib
import json
import os
from pathlib import Path
import pickle
import subprocess
import sys
import time
import unittest
import uuid
from unittest.mock import patch

from world import World
from contracts import test_admission

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / 'skills/agent-memory/scripts'
sys.path.insert(0, str(SCRIPTS))
from agent_memory import artifact, bank_writes, codex_boundary, layout_recovery_observation as raw
from agent_memory import layout_recovery_state as state, local_registry, recovery_frontier, scope_binding
from agent_memory import turn_check, turn_sources as sources
from agent_memory.developer_identity import digest, encoded


def uid():
    return str(uuid.uuid4())


class HeldSourceEnrollment(unittest.TestCase):
    def fixture(self, world):
        helper = test_admission.AdmissionContract()
        repo, bank = helper.fixture(world)
        operation, proposal_id, hold_id, unit_id = uid(), uid(), uid(), uid()
        handoff = b'# Pending fixture\n\nRetain the source conditions until independently confirmed.\n'
        (bank / 'handoff-prompt.md').write_bytes(handoff)
        manifest = raw.observe(repo, operation)
        directory = state.paths(repo, operation)[3]
        scope = manifest['identity']['scope_context']
        unit_map = {'schema': 'agent-memory/layout-unit-map-v1', 'operation_id': operation,
                    'manifest_sha256': digest(encoded(manifest)), 'units': []}
        pending_source = next(source for source in manifest['sources'] if source['path'] == 'handoff-prompt.md')
        for source in manifest['sources']:
            unit_map['units'].append({'unit_id': unit_id if source == pending_source else uid(),
                'source_ranges': [{'source_id': source['source_id'], 'start': 0, 'end': source['byte_count']}],
                'disposition': 'pending-admission' if source == pending_source else 'preserve',
                'provenance': {'origin': 'synthetic fixture; not actual incident evidence'}})
        runtime = artifact.inspect(artifact.PACK)
        self.assertIn('layout-frontier-v1', runtime['compatibility'].get('private_state_capabilities', []),
                      'Held tests require the real integrated capable runtime; no capability mock is permitted')
        review = {'schema': 'agent-memory/layout-review-v1', 'review_id': uid(),
                  'operation_id': operation, 'manifest_sha256': digest(encoded(manifest)),
                  'unit_map_sha256': digest(encoded(unit_map)), 'reviewer_session': 'fixture-independent-review',
                  'author_session': 'fixture-proposal', 'decision': 'accept'}
        attestation = {'schema': 'agent-memory/layout-quiescence-attestation-v1', 'attestation_id': uid(),
            'operation_id': operation, 'project_id': scope['project_id'], 'repo_root': str(repo),
            'bank_path': str(bank), 'checkout_key': scope['checkout_key'],
            'state_root': str(bank_writes.state_paths(repo, bank, operation)[0].parents[2]),
            'manifest_sha256': digest(encoded(manifest)), 'runtime_artifact_digest': runtime['artifact_digest'],
            'operator_developer_id': scope['developer_id'], 'issued_at': local_registry.now(),
            'declared_principals': [scope['developer_id']], 'declared_invocation_routes': ['fixture-current-runtime'],
            'stopped_sessions': ['fixture-legacy-session'],
            'statement': 'All authorized legacy routes are disabled or quiesced.', 'review_id': review['review_id']}
        proposal = {'schema': 'agent-memory/layout-recovery-proposal-v1', 'operation_id': operation,
            'proposal_id': proposal_id, 'scope_context': scope, 'manifest_sha256': digest(encoded(manifest)),
            'unit_map_sha256': digest(encoded(unit_map)), 'review': review, 'review_sha256': digest(encoded(review)),
            'quiescence_attestation': attestation, 'quiescence_attestation_sha256': digest(encoded(attestation))}
        ranges = next(unit['source_ranges'] for unit in unit_map['units'] if unit['unit_id'] == unit_id)
        ledger = {'schema': 'agent-memory/layout-pending-ledger-v1', 'operation_id': operation,
            'project_id': scope['project_id'], 'manifest_sha256': digest(encoded(manifest)),
            'unit_map_sha256': digest(encoded(unit_map)), 'proposal_sha256': digest(encoded(proposal)),
            'generation': 0, 'previous_generation_sha256': None, 'units': [{'unit_id': unit_id,
                'source_ranges': ranges, 'provenance': {'origin': 'unknown; synthetic fixture'},
                'reason': 'Independent confirmation remains required', 'required_actions': ['Review original source'],
                'detecting_developer_id': scope['developer_id'], 'state': 'pending-admission', 'terminal_receipts': []}]}
        head = {'schema': 'agent-memory/layout-ledger-head-v1', 'operation_id': operation,
                'generation': 0, 'sha256': digest(encoded(ledger))}
        for path, value in [('manifest.json', manifest), ('unit-map.json', unit_map),
                ('proposals/' + proposal_id + '.json', proposal), ('ledger/00000000.json', ledger),
                ('ledger-head.json', head), ('journal.json', {'schema': 'agent-memory/layout-recovery-journal-v1',
                 'operation_id': operation, 'proposal_sha256': digest(encoded(proposal)),
                 'manifest_sha256': digest(encoded(manifest))})]:
            state.write_once(directory / path, value)
        index = state.index(repo)
        index['operations'][operation] = {'state': 'active', 'project_id': scope['project_id'],
            'ledger_head_sha256': digest(encoded(head)), 'proposal_id': proposal_id, 'hold_id': hold_id}
        state.set_index(repo, index, None)
        state.bind(repo, operation, scope['project_id'], scope['checkout_key'], 'active')
        (bank / 'handoff-prompt.md').unlink()  # Fixture setup, not a tested recovery apply.
        for name, value in [('.agent-memory-transaction.md', {'schema': recovery_frontier.HOLD,
                'project_id': scope['project_id'], 'operation_id': operation, 'hold_id': hold_id}),
                ('.agent-memory-recovery.md', {'schema': recovery_frontier.SENTINEL,
                 'project_id': scope['project_id'], 'operation_id': operation})]:
            (bank / name).write_bytes(encoded(value)); (bank / name).chmod(0o600)
        return helper, repo, bank, operation, directory, pending_source, handoff

    def snapshot(self, world, repo, bank):
        return ({str(path.relative_to(bank)): path.read_bytes() for path in bank.rglob('*') if path.is_file()},
                (repo / 'code.py').read_bytes(), (repo / '.git/index').read_bytes(),
                world.git('rev-parse', 'HEAD', cwd=repo).stdout)

    def child_register(self, world, repo, session, lifecycle, native, *, fault=None, mode='error'):
        script = ('import sys;sys.path.insert(0,sys.argv[1]);from agent_memory import turn_sources as s;'
                  'import json;print(json.dumps(s.register(sys.argv[2],"claude",sys.argv[3],sys.argv[4],'
                  '"a"*64,native_source_id=sys.argv[5],source_reference="/private/fixture.jsonl")))')
        command = [sys.executable, '-I', '-c', script, str(SCRIPTS), str(repo), session, lifecycle, native]
        environment = None if fault is None else {'AGENT_MEMORY_FAULT_STAGE': fault, 'AGENT_MEMORY_FAULT_MODE': mode}
        return world.run(command, env=environment, check=False, timeout=30)

    def test_typed_context_exact_retry_and_no_ack_authority(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, bank, operation, directory, original, body = self.fixture(world)
            before = self.snapshot(world, repo, bank)
            context = scope_binding.private_enrollment_context(repo)
            self.assertTrue(context.held)
            for serialize in (json.dumps, pickle.dumps, scope_binding.context_value):
                with self.assertRaises((TypeError, ValueError)):
                    serialize(context)
            with self.assertRaises(ValueError):
                scope_binding.require(repo)
            session, lifecycle, event = uid(), uid(), uid()
            first = sources.register(repo, 'claude', session, lifecycle, 'a' * 64,
                                     native_source_id=event, source_reference='/private/fixture.jsonl')
            retry = sources.register(repo, 'claude', session, lifecycle, 'a' * 64,
                                     native_source_id=event, source_reference='/private/fixture.jsonl')
            self.assertEqual(first, retry)
            inspected = sources.inspect(repo, 'claude', session, lifecycle)
            self.assertEqual(len(inspected['sources']), 1)
            self.assertTrue(inspected['recovery_pending']); self.assertFalse(inspected['complete'])
            value = {'session': session, 'prompt': lifecycle, 'observation_id': first['observation_id'],
                     'source_sha256': first['source_sha256'], 'outcome': 'no-learning',
                     'assessment': {'source_id': first['observation_id'], 'source_review': 'Fixture review', 'candidates': []}}
            with self.assertRaises(ValueError):
                sources.verify_assessment(repo, value, first['observation_id'])
            result = turn_check.check(repo, value, codex_boundary.complete, 'claude')
            self.assertFalse(result['verified']); self.assertFalse(result['assessment_verified'])
            self.assertEqual(result['refresh']['sharing_state'], 'recovery-pending')
            self.assertFalse(list(directory.parent.parent.rglob('*.done')))
            self.assertEqual((directory / original['blob']).read_bytes(), body)
            self.assertEqual(self.snapshot(world, repo, bank), before)
            with self.assertRaises(ValueError):
                sources.register(repo, 'claude', session, lifecycle, 'b' * 64, native_source_id=event,
                                 source_reference='/private/fixture.jsonl')
            self.assertEqual(sources.inspect(repo, 'claude', session, lifecycle)['sources'][0]['observation_id'], first['observation_id'])

    def test_error_and_hard_exit_boundaries_reconcile_one_original_observation(self):
        stages = ('before-intent', 'after-intent', 'before-index', 'before-source',
                  'after-source', 'after-index', 'before-final-recheck')
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, bank, operation, directory, original, body = self.fixture(world)
            before = self.snapshot(world, repo, bank)
            for mode in ('error', 'exit'):
                for stage in stages:
                    with self.subTest(mode=mode, stage=stage):
                        session, lifecycle, event = uid(), uid(), uid()
                        failed = self.child_register(world, repo, session, lifecycle, event,
                                                     fault='hold-source-' + stage, mode=mode)
                        self.assertEqual(failed.returncode, 86 if mode == 'exit' else 1, failed.stderr)
                        _, _, _, turn, identity = sources.private_namespace(repo, 'claude', session, lifecycle)
                        intents = sources.enrollment_inventory(turn, identity)
                        observation = intents[0]['observation_id'] if intents else None
                        if observation is not None:
                            backlog = sources.backlog(repo, 'claude')
                            self.assertTrue(any(item['observation_id'] == observation
                                for row in backlog['turns'] for item in row['pending']))
                        completed = self.child_register(world, repo, session, lifecycle, event)
                        self.assertEqual(completed.returncode, 0, completed.stderr)
                        registered = json.loads(completed.stdout)
                        if observation is not None:
                            self.assertEqual(registered['observation_id'], observation)
                        current = sources.inspect(repo, 'claude', session, lifecycle)
                        self.assertEqual(len(current['sources']), 1)
                        self.assertFalse(current['complete'])
                        self.assertEqual(sources.enrollment_inventory(turn, identity)[0]['state'], 'registered')
            self.assertEqual((directory / original['blob']).read_bytes(), body)
            self.assertEqual(self.snapshot(world, repo, bank), before)

    def test_late_hold_registry_and_ledger_changes_retain_written_source_then_retry(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, bank, operation, directory, original, body = self.fixture(world)
            for target in (bank / '.agent-memory-transaction.md', local_registry.paths('projects')[1],
                           directory / 'ledger-head.json'):
                with self.subTest(target=target.name):
                    session, lifecycle, event = uid(), uid(), uid()
                    rendezvous = world.inside('pause-' + event)
                    world.env.update({'AGENT_MEMORY_FAULT_STAGE': 'hold-source-before-final-recheck',
                                      'AGENT_MEMORY_FAULT_MODE': 'pause', 'AGENT_MEMORY_FAULT_DIR': str(rendezvous)})
                    script = ('import sys;sys.path.insert(0,sys.argv[1]);from agent_memory import turn_sources as s;'
                              's.register(sys.argv[2],"claude",sys.argv[3],sys.argv[4],"a"*64,native_source_id=sys.argv[5])')
                    child = world.spawn([sys.executable, '-I', '-c', script, str(SCRIPTS), repo, session, lifecycle, event])
                    for key in ('AGENT_MEMORY_FAULT_STAGE', 'AGENT_MEMORY_FAULT_MODE', 'AGENT_MEMORY_FAULT_DIR'):
                        world.env.pop(key)
                    ready = rendezvous / 'hold-source-before-final-recheck.ready'
                    deadline = time.monotonic() + 10
                    while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                        time.sleep(0.01)
                    self.assertTrue(ready.exists(), 'Child did not reach actual durable registration boundary')
                    retained = target.read_bytes()
                    retained_index = None
                    if target.name == 'projects.json':
                        target.write_bytes(retained + b'\n')  # Same real registry, changed byte pin.
                    elif target.name == 'ledger-head.json':
                        head = json.loads(retained)
                        ledger = state.read(directory / 'ledger/00000000.json')
                        successor = {**ledger, 'generation':1,'previous_generation_sha256':digest(encoded(ledger))}
                        state.write_once(directory / 'ledger/00000001.json', successor)
                        updated_head = {**head,'generation':1,'sha256':digest(encoded(successor))}
                        state.replace(target, updated_head, digest(retained))
                        retained_index = state.index(repo)
                        updated_index = copy.deepcopy(retained_index)
                        updated_index['operations'][operation]['ledger_head_sha256'] = digest(encoded(updated_head))
                        state.set_index(repo, updated_index, digest(encoded(retained_index)))
                    else:
                        changed = json.loads(retained); changed['hold_id'] = uid(); target.write_bytes(encoded(changed))
                    (rendezvous / 'hold-source-before-final-recheck.release').touch()
                    stdout, stderr = child.communicate(timeout=15)
                    self.assertNotEqual(child.returncode, 0, stdout + stderr)
                    target.write_bytes(retained)
                    if retained_index is not None:
                        current_index = state.index(repo)
                        state.set_index(repo, retained_index, digest(encoded(current_index)))
                    _, _, _, turn, identity = sources.private_namespace(repo, 'claude', session, lifecycle)
                    intent = sources.enrollment_inventory(turn, identity)[0]
                    self.assertEqual(intent['state'], 'unverified')
                    self.assertEqual(len(sources.inventory(turn, identity)['sources']), 1)
                    retried = sources.register(repo, 'claude', session, lifecycle, 'a' * 64, native_source_id=event)
                    self.assertEqual(retried['observation_id'], intent['observation_id'])
                    self.assertEqual(len(sources.inspect(repo, 'claude', session, lifecycle)['sources']), 1)
            self.assertEqual((directory / original['blob']).read_bytes(), body)

    def test_unknown_missing_corrupt_hold_and_frontier_only_change_refuse(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, bank, operation, directory, original, body = self.fixture(world)
            context = scope_binding.private_enrollment_context(repo)
            for target in (bank / '.agent-memory-transaction.md', bank / '.agent-memory-recovery.md',
                           directory / 'journal.json', directory / 'ledger-head.json'):
                retained = target.read_bytes()
                for replacement in (None, b'{}\n'):
                    with self.subTest(path=target.name, replacement=replacement):
                        if replacement is None:target.unlink()
                        else:target.write_bytes(replacement); target.chmod(0o600)
                        with self.assertRaises((ValueError, OSError, KeyError)):
                            sources.register(repo, 'claude', uid(), uid(), 'a' * 64, native_source_id=uid())
                        target.write_bytes(retained); target.chmod(0o600)
            index = state.index(repo)
            old_index = copy.deepcopy(index)
            head_path = directory / 'ledger-head.json'
            old_head = state.read(head_path)
            ledger = state.read(directory / 'ledger/00000000.json')
            successor = {**ledger, 'generation': 1, 'previous_generation_sha256': digest(encoded(ledger))}
            state.write_once(directory / 'ledger/00000001.json', successor)
            head = {**old_head, 'generation': 1, 'sha256': digest(encoded(successor))}
            state.replace(head_path, head, digest(encoded(old_head)))
            index['operations'][operation]['ledger_head_sha256'] = digest(encoded(head))
            state.set_index(repo, index, digest(encoded(old_index)))
            with self.assertRaises(ValueError):scope_binding.recheck_private_enrollment(context)
            current = scope_binding.private_enrollment_context(repo)
            self.assertNotEqual(current.context_digest, context.context_digest)
            self.assertTrue(current.held)
            with patch.dict(os.environ, {'AGENT_MEMORY_STATE_HOME': str(world.inside('other-state'))}):
                with self.assertRaises(ValueError):scope_binding.private_enrollment_context(repo)
            self.assertEqual((directory / original['blob']).read_bytes(), body)

    def test_verified_hold_rejects_location_and_hold_changes_during_observation(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, bank, operation, directory, original, body = self.fixture(world)
            config = repo / '.git' / 'config'
            binding = local_registry.paths('layout-recovery-bindings')[1]
            marker = bank / '.agent-memory-transaction.md'
            foreign = json.loads(binding.read_bytes())
            foreign['bindings'][operation]['bank_path'] = str(world.inside('foreign-bank'))
            for target, replacement in (
                    (config, config.read_bytes() + b'\n[agent-memory]\n\tbankroot = foreign-bank\n'),
                    (binding, encoded(foreign)),
                    (marker, b'{}\n')):
                with self.subTest(target=target.name):
                    old = target.read_bytes()
                    observe = recovery_frontier._observe_bank
                    changed = False
                    def change_after_first(*args, **kwargs):
                        nonlocal changed
                        result = observe(*args, **kwargs)
                        if not changed:
                            changed = True
                            target.write_bytes(replacement)
                        return result
                    try:
                        with patch.object(recovery_frontier, '_observe_bank', side_effect=change_after_first):
                            with self.assertRaises((ValueError, OSError, KeyError)):
                                recovery_frontier.verified_hold(repo)
                        self.assertTrue(changed)
                    finally:
                        target.write_bytes(old)
            self.assertTrue(recovery_frontier.verified_hold(repo))
            self.assertEqual((directory / original['blob']).read_bytes(), body)

    def test_shared_source_barrier_reentrancy_and_lifecycle_contention_preserve_sources(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, bank, operation, directory, original, body = self.fixture(world)
            before = self.snapshot(world, repo, bank)
            session, lifecycle, event = uid(), uid(), uid()
            with sources.source_barrier(repo):
                with sources.source_barrier(repo):
                    self.assertTrue(scope_binding.private_enrollment_context(repo).held)
                with self.assertRaises(ValueError):
                    with sources.source_barrier(repo, exclusive=True):
                        self.fail('A shared source barrier must not upgrade')
            with sources.source_barrier(repo, exclusive=True):
                blocked = self.child_register(world, repo, session, lifecycle, event)
                self.assertNotEqual(blocked.returncode, 0)
                with sources.source_barrier(repo):
                    self.assertTrue(scope_binding.private_enrollment_context(repo).held)
            retried = self.child_register(world, repo, session, lifecycle, event)
            self.assertEqual(retried.returncode, 0, retried.stderr)
            self.assertEqual(len(sources.inspect(repo, 'claude', session, lifecycle)['sources']), 1)
            self.assertEqual((directory / original['blob']).read_bytes(), body)
            self.assertEqual(self.snapshot(world, repo, bank), before)

    def test_all_shared_readers_refuse_unknown_compatibility_before_source_index_access(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper = test_admission.AdmissionContract()
            repo, bank = helper.fixture(world)
            session, lifecycle = uid(), uid()
            observed = sources.register(repo, 'codex', session, lifecycle, 'a' * 64)
            _, _, base, turn, identity = sources.namespace(repo, 'codex', session, lifecycle)
            current = sources.inspect(repo, 'codex', session, lifecycle)
            assessment = {'observation_id':observed['observation_id'],'source_sha256':'a' * 64,
                'outcome':'no-learning','assessment':{'source_id':observed['observation_id'],
                'source_review':'Actual synthetic temporary source review','candidates':[]}}
            receipt = sources.verify_assessment(repo, assessment, observed['observation_id'])
            marker = base.parent / 'turn-sources-compat-v1.json'
            valid = {'schema':'agent-memory/turn-sources-compat-v1',
                     'minimum_reader':'omp-harness-v1','harnesses':['omp']}
            source_before = (turn / 'source-0.json').read_bytes()
            index_before = {path.name:path.read_bytes() for path in (base / 'pending').iterdir()}
            operations = (
                lambda:sources.register(repo, 'codex', session, lifecycle, 'b' * 64),
                lambda:sources.inspect(repo, 'codex', session, lifecycle),
                lambda:sources.backlog(repo),
                lambda:sources.pending_notice(repo, 'codex', session, lifecycle),
                lambda:sources.verify_assessment(repo, assessment, observed['observation_id']),
                lambda:sources.reserve_wake(repo, 'codex', session, lifecycle, current['fingerprint'], 'b' * 64),
                lambda:sources.complete(repo, 'codex', session, lifecycle, observed['observation_id'], 'a' * 64, receipt))
            for raw_marker in (b'{partial', encoded({'schema':'unknown'}),
                               encoded({**valid,'minimum_reader':'future-reader'}),
                               encoded({**valid,'harnesses':['omp','codex']})):
                marker.write_bytes(raw_marker); marker.chmod(0o600)
                for operation in operations:
                    with self.assertRaises((ValueError, OSError)):
                        operation()
                self.assertEqual(marker.read_bytes(), raw_marker)
                self.assertEqual((turn / 'source-0.json').read_bytes(), source_before)
                self.assertEqual({path.name:path.read_bytes() for path in (base / 'pending').iterdir()}, index_before)
                self.assertFalse(list(turn.glob('*.done')))
            marker.write_bytes(encoded(valid)); marker.chmod(0o600)
            self.assertEqual(len(sources.inspect(repo, 'codex', session, lifecycle)['sources']), 1)
            with self.assertRaises(ValueError):
                with sources.source_barrier(repo):
                    marker.write_bytes(encoded({'schema':'unknown'}))
            self.assertEqual(marker.read_bytes(), encoded({'schema':'unknown'}))
            marker.unlink()
            # Generic absence is allowed; native retained-state drain proof is
            # independently owned and is not fabricated by this shared reader.
            self.assertEqual(len(sources.inspect(repo, 'codex', session, lifecycle)['sources']), 1)

    def test_real_codex_claude_hooks_keep_task_active_but_boundaries_pending(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            helper, repo, bank, operation, directory, original, body = self.fixture(world)
            before = self.snapshot(world, repo, bank)
            for harness in ('codex', 'claude'):
                session, lifecycle, event_id = uid(), uid(), uid()
                event = {'hook_event_name': 'UserPromptSubmit', 'cwd': str(repo), 'session_id': session,
                         'prompt': 'Synthetic temporary status request; preserve its exact native observation.'}
                if harness == 'codex':event.update(turn_id=lifecycle, event_id=event_id)
                else:event.update(prompt_id=lifecycle, transcript_path=str(world.inside('not-yet-created.jsonl')))
                def hook(name, value):
                    result = subprocess.run([sys.executable, '-I', str(helper.pack / 'hooks' / name)],
                        input=json.dumps(value), text=True, capture_output=True, env=world.env, timeout=30)
                    self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
                    return json.loads(result.stdout)
                output = hook(harness + '_prompt.py', event)
                self.assertIn('recovery-pending', output['hookSpecificOutput']['additionalContext'].lower())
                repeated = hook(harness + '_prompt.py', event)
                self.assertIn('hookSpecificOutput', repeated)
                current = sources.inspect(repo, harness, session, lifecycle)
                self.assertEqual(len(current['sources']), 1)
                self.assertEqual(current['sources'][0]['source_sha256'], hashlib.sha256(event['prompt'].encode()).hexdigest())
                if harness == 'codex':
                    steering = {**event, 'event_id': uid(), 'prompt': 'Distinct native steering in the same lifecycle.'}
                    self.assertIn('hookSpecificOutput', hook('codex_prompt.py', steering))
                    steered = sources.inspect(repo, harness, session, lifecycle)
                    self.assertEqual(len(steered['sources']), 2)
                    self.assertEqual({source['native_source_id'] for source in steered['sources']},
                                     {event_id, steering['event_id']})
                stopped = hook(harness + '_stop.py', {**event, 'hook_event_name': 'Stop', 'stop_hook_active': False})
                self.assertIn('recovery-pending', stopped['systemMessage'])
                self.assertNotIn('decision', stopped)
                self.assertFalse(current['complete'])
            # A lifecycle/turn ID is not a unique steering event ID.
            ambiguous = {'hook_event_name': 'UserPromptSubmit', 'cwd': str(repo), 'session_id': uid(),
                         'turn_id': uid(), 'prompt': 'No unique native event identity is available.',
                         'transcript_path':str(world.inside('actual-native-reference.jsonl'))}
            refused = hook('codex_prompt.py', ambiguous)
            self.assertEqual(refused['decision'], 'block')
            self.assertIn('not an observation', refused['reason'])
            _, _, _, turn, identity = sources.private_namespace(repo, 'codex', ambiguous['session_id'], ambiguous['turn_id'])
            retained = sources.unidentified_inventory(turn, identity)
            self.assertEqual(len(retained), 1)
            self.assertEqual(base64.b64decode(retained[0]['source_base64']), ambiguous['prompt'].encode())
            self.assertEqual(retained[0]['source_reference'], ambiguous['transcript_path'])
            self.assertEqual(sources.inspect(repo, 'codex', ambiguous['session_id'], ambiguous['turn_id'])['sources'], [])
            pending = sources.backlog(repo, 'codex')
            self.assertTrue(any(retained[0]['retention_id'] in row['unidentified_input_ids'] for row in pending['turns']))
            self.assertEqual((directory / original['blob']).read_bytes(), body)
            self.assertEqual(self.snapshot(world, repo, bank), before)
