"""Executable transport regressions, not evidence of native omp support."""
import copy
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import sys
import shutil
import subprocess
import time
import unittest
import uuid
from unittest.mock import patch
from contextlib import nullcontext
from world import World
from contracts import test_admission

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'))
from agent_memory import installation, omp_boundary as boundary, omp_installation, omp_projection, omp_transport as transport, turn_sources


def observer_payload(repo, session, entries, *, terminal=False, trigger='resume', inherited_ids=()):
    """Controlled native-format source fixture, not native API qualification."""
    entries = copy.deepcopy(entries)
    for index, entry in enumerate(entries):
        if index and entry.get('parentId') is None:
            entry['parentId'] = entries[index - 1]['id']
    parents = {entry['id']: entry.get('parentId') for entry in entries}
    branch = []
    current = entries[-1]['id'] if entries else None
    while current is not None:
        if current in branch or current not in parents:
            raise ValueError('Malformed controlled native source tree')
        branch.append(current); current = parents[current]
    branch.reverse()
    return {'entries': entries, 'branch_ids': branch, 'event_ordinal': len(entries),
            'terminal': terminal, 'trigger': trigger, 'inherited_ids': list(inherited_ids),
            'view': {'session': session, 'cwd': str(repo), 'file': None,
                     'header': {'id': session, 'cwd': str(repo), 'version': 3, 'parent_session': None, 'timestamp': None},
                     'leaf_id': entries[-1]['id'] if entries else None,
                     'native_parents': [{'id': entry['id'], 'parent_id': entry.get('parentId'), 'timestamp': None} for entry in entries],
                     'source_kind': 'session-manager', 'tail_status': 'available', 'fork_status': 'none'}}


def LiveDeliveryWindow(*, expired=False):
    """Real opaque capability minted only inside this controlled timing fixture."""
    from agent_memory import omp_stream
    now = time.monotonic_ns()
    return omp_stream.VerifiedNativeDeliveryWindow(omp_stream._WINDOW_MINT, L=0, j3=now,
        j_deadline=now - 1 if expired else now + 60_000_000_000, nonce=str(uuid.uuid4()),
        instance=str(uuid.uuid4()), eligible=lambda: True, platform='darwin')


def observe_installed(world, command, repo, receipt, request, *, launcher=None, timeout=60):
    """Exercise the real finite installed pipe; return only its actual verified result."""
    from agent_memory import omp_stream
    instance = str(uuid.uuid4())
    payload = request['payload']; view = payload.get('view')
    if request['action'] == 'hint':
        identity = {'kind': 'hint', **payload}
    else:
        native_ids = sorted(item['id'] for item in view['native_parents'])
        tools = set()
        for entry in payload['entries']:
            message = entry.get('message')
            if not isinstance(message, dict):
                continue
            if message.get('role') == 'toolResult' and isinstance(message.get('toolCallId'), str):
                tools.add(message['toolCallId'])
            if message.get('role') == 'assistant' and isinstance(message.get('content'), list):
                tools.update(block['id'] for block in message['content']
                             if isinstance(block, dict) and block.get('type') == 'toolCall' and isinstance(block.get('id'), str))
        identity = {'view_id': str(uuid.uuid4()), 'leaf_id': view['leaf_id'],
                    'native_ids': native_ids, 'tool_ids': sorted(tools)}
    ordinal = payload.get('event_ordinal', 1)
    wire = omp_stream.encode_frame('REQUEST', 1, {'request': request, 'event_ordinal': ordinal, 'identity': identity})
    wire += omp_stream.encode_frame('FINAL', 2, {'last_attempt_seq': 1, 'refused_count': 0,
                                               'last_event_ordinal': ordinal, 'transition': None})
    argv = list(map(str, launcher)) if launcher is not None else [str(command), 'omp-boundary']
    argv += ['--stream', '--repo', str(repo), '--session', request['session'], '--instance', instance,
             '--lifecycle', request['lifecycle'], '--artifact', receipt['source']['artifact_digest'],
             '--omp-agent-dir', receipt['omp_binding']['agent_dir']]
    completed = subprocess.run(argv, input=wire, capture_output=True, env=world.env, cwd=repo, timeout=timeout)
    stdout = completed.stdout.decode('utf-8', 'strict'); stderr = completed.stderr.decode('utf-8', 'strict')
    if completed.returncode:
        return subprocess.CompletedProcess(argv, completed.returncode, stdout, stderr)
    raw = completed.stdout; result = None; ready = False
    while raw:
        end = raw.find(b'\n')
        if end < 0 or end >= 256:
            raise AssertionError('Incomplete finite observer response header')
        header = raw[:end + 1].decode('ascii', 'strict').split()
        if len(header) != 5 or header[0] != 'AM249' or header[4] != '-':
            raise AssertionError('Malformed finite observer response header')
        kind, sequence, length = header[1], int(header[2]), int(header[3])
        if length > 65536 or len(raw) < end + 1 + length:
            raise AssertionError('Incomplete or oversized finite observer response')
        body = json.loads(raw[end + 1:end + 1 + length].decode('utf-8', 'strict')); raw = raw[end + 1 + length:]
        if body.get('instance_id') != instance:
            raise AssertionError('Foreign finite observer response')
        if kind == 'READY':
            ready = True
        if kind == 'OBSERVED' and sequence == 1 and isinstance(body.get('result'), dict):
            if result is not None:
                raise AssertionError('Duplicate finite observer result')
            result = body['result']
    if not ready or not result or result.get('verified') is not True:
        raise AssertionError('No actual verified finite observer result: ' + stdout + stderr)
    with patch.dict(os.environ, world.env, clear=True):
        streams = omp_stream.inventory(repo)['streams']
        actual = next(row for row in streams if row['instance_id'] == instance)
        if not actual['closed']:
            raise AssertionError('Finite observer exited without actual CLOSED coverage')
    return subprocess.CompletedProcess(argv, completed.returncode, json.dumps(result), stderr)


class NativeSources(unittest.TestCase):
    def fixture(self, world):
        repo, _ = test_admission.AdmissionContract().fixture(world)
        with patch.dict(os.environ, world.env, clear=True):
            boundary._activate_marker(repo)
        return repo, str(uuid.uuid4()), str(uuid.uuid4())

    def entry(self, identifier, role, text, *, attribution='agent', parent=None, **extra):
        return {'type': 'message', 'id': identifier, 'parentId': parent,
                'message': {'role': role, 'attribution': attribution, 'content': text, **extra}}
    def observe_request(self, repo, request, **kwargs):
        with patch.object(transport, '_verify_observer_receipt', side_effect=lambda *args, **kwargs: boundary.verify_marker(repo)):
            return transport.observe(repo, request, **kwargs)


    def observe(self, repo, session, lifecycle, entries, *, seal=True, terminal=False, trigger='human'):
        payload = observer_payload(repo, session, entries, terminal=terminal, trigger=trigger)
        if seal:
            payload['request_wake'] = False
        return self.observe_request(repo, {'schema': 'agent-memory/omp-observer-request-v2',
                                          'action': 'seal' if seal else 'ingest', 'session': session,
                                          'lifecycle': lifecycle, 'payload': payload})

    def acknowledge(self, repo, row):
        value = {key: row[key] for key in ('session', 'lifecycle', 'observation_id', 'source_sha256')}
        value.update(outcome='no-learning', assessment={'source_id': row['observation_id'],
                     'source_review': 'Reviewed the actual ephemeral transport fixture; no durable project learning.', 'candidates': []})
        return transport.complete(repo, value)

    def test_long_segment_uses_two_shared_observations_and_ingest_is_idempotent(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            entries = [self.entry('human', 'user', 'An ephemeral arithmetic question', attribution='user')]
            entries += [self.entry('assistant-' + str(i), 'assistant', 'Ephemeral computation') for i in range(150)]
            with patch.dict(os.environ, world.env, clear=True):
                result = self.observe(repo, session, lifecycle, entries)
                rows, faults = transport.pending_inventory(repo)
                self.assertFalse(faults)
                self.assertEqual(len(rows), 2)
                ids = {r['observation_id'] for r in rows}
                self.observe(repo, session, result['lifecycle'], entries)
                self.assertEqual(ids, {r['observation_id'] for r in transport.pending_inventory(repo)[0]})
                for row in rows:
                    self.assertTrue(self.acknowledge(repo, row)['verified'])
                self.assertEqual(transport.list_pending(repo)['pending'], [])

    def test_admission_requires_sealed_registered_segment(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            entries = [self.entry('human', 'user', 'Actual short source', attribution='user')]
            with patch.dict(os.environ, world.env, clear=True):
                self.observe(repo, session, lifecycle, entries, seal=False)
                rows = transport.pending_inventory(repo)[0]
                admission = next(r for r in rows if 'observation_id' in r)
                with self.assertRaises(ValueError):
                    self.acknowledge(repo, admission)

    def test_gap_refuses_both_assessment_outcomes_and_preserves_marker(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            entries = [self.entry('human', 'user', 'Source with unavailable payload', attribution='user'),
                       self.entry('image', 'assistant', [{'type': 'image', 'data': 'must-not-retain'}])]
            with patch.dict(os.environ, world.env, clear=True):
                marker = boundary._base(repo) / 'turn-sources-compat-v1.json'
                original_marker = marker.read_bytes()
                self.observe(repo, session, lifecycle, entries)
                rows, _ = transport.pending_inventory(repo)
                self.assertTrue(all(row['status'] == 'pending-coverage-gap' for row in rows))
                for row in rows:
                    with self.assertRaises(ValueError):
                        self.acknowledge(repo, row)
                result = boundary.drain_source_capabilities(repo, quiesced=True)
                self.assertFalse(result['drainable'])
                self.assertEqual(marker.read_bytes(), original_marker)
                for path in (boundary._base(repo) / 'omp-source-v1').rglob('*.json'):
                    self.assertNotIn('must-not-retain', path.read_text())

    def test_capacity_retains_arrival_then_reuses_reference_after_capacity_frees(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True), patch.object(turn_sources, 'MAX_TURNS', 1):
                other = turn_sources.register(repo, 'codex', 'other', 'other', 'a' * 64)
                self.observe(repo, session, lifecycle, [self.entry('human', 'user', 'Actual queued source', attribution='user')])
                queued = transport.list_pending(repo)['pending']
                references = {row['source_reference'] for row in queued}
                self.assertTrue(all('observation_id' not in row for row in queued))
                value = {'observation_id': other['observation_id'], 'source_sha256': other['source_sha256'],
                         'outcome': 'no-learning', 'assessment': {'source_id': other['observation_id'], 'source_review': 'Reviewed actual other fixture.', 'candidates': []}}
                receipt = turn_sources.verify_assessment(repo, value, other['observation_id'])
                turn_sources.complete(repo, 'codex', 'other', 'other', other['observation_id'], other['source_sha256'], receipt)
                self.observe(repo, session, lifecycle, [], seal=False)
                enrolled = transport.list_pending(repo)['pending']
                self.assertEqual(references, {row['source_reference'] for row in enrolled})
                self.assertTrue(all('observation_id' in row for row in enrolled))

    def test_root_wake_is_once_and_later_human_carries_unassessed_old_source(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                first = self.entry('h1', 'user', 'First actual short question', attribution='user')
                result = self.observe(repo, session, lifecycle, [first])
                request = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'wake', 'session': session,
                           'lifecycle': result['lifecycle'], 'payload': result['wake_payload']}
                wake = self.observe_request(repo, request, delivery_window=LiveDeliveryWindow())
                self.assertTrue(wake['continue'])
                self.assertFalse(self.observe_request(repo, request, delivery_window=LiveDeliveryWindow())['continue'])
                old_ids = set(result['wake_payload']['observation_ids'])
                second = self.entry('h2', 'user', 'Second actual short question', attribution='user')
                result = self.observe(repo, session, result['lifecycle'], [first, second])
                request.update(lifecycle=result['lifecycle'], payload=result['wake_payload'])
                wake = self.observe_request(repo, request, delivery_window=LiveDeliveryWindow())
                self.assertTrue(wake['continue'])
                self.assertTrue(all(identifier in wake['additionalContext'] for identifier in old_ids))
                self.assertLessEqual(len(wake['additionalContext'].encode()), 8192)

    def test_completed_snapshots_require_bound_drain_proof_after_marker_removal(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                self.observe(repo, session, lifecycle, [self.entry('h1', 'user', 'Actual reviewed short question', attribution='user')])
                for row in transport.pending_inventory(repo)[0]:
                    self.acknowledge(repo, row)
                result = boundary.drain_source_capabilities(repo, quiesced=True)
                self.assertTrue(result['marker_cleared'])
                self.assertEqual(boundary.required_source_capabilities(repo), set())
                self.assertTrue((boundary._base(repo) / 'omp-source-v1').exists())
                for proof in (boundary._base(repo) / 'omp-drains').glob('*.json'):
                    proof.unlink()
                with self.assertRaises(ValueError):
                    boundary.required_source_capabilities(repo)

    def test_manual_marker_delete_cannot_hide_unregistered_source(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                self.observe(repo, session, lifecycle, [self.entry('human', 'user', 'Actual source', attribution='user')], seal=False)
                (boundary._base(repo) / 'turn-sources-compat-v1.json').unlink()
                with self.assertRaises(ValueError):
                    boundary.required_source_capabilities(repo)


    def test_exact_own_hidden_batch_does_not_open_a_source_successor(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                human = self.entry('human', 'user', 'Actual short request', attribution='user')
                result = self.observe(repo, session, lifecycle, [human])
                wake = self.observe_request(repo, {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'wake',
                    'session': session, 'lifecycle': result['lifecycle'], 'payload': result['wake_payload']}, delivery_window=LiveDeliveryWindow())
                control = {'type': 'custom_message', 'id': 'native-control', 'parentId': 'human',
                           'customType': 'session-stop-continuation', 'attribution': 'agent',
                           'content': wake['additionalContext']}
                self.observe(repo, session, result['lifecycle'], [human, control], seal=False)
                self.assertEqual(len(transport.segments(repo, session)), 1)
                substantive = self.entry('late-work', 'assistant', 'New substantive foreign work')
                self.observe(repo, session, result['lifecycle'], [human, control, substantive], seal=False, terminal=True,
                             trigger='own-hidden-ambiguous')
                segments = transport.segments(repo, session)
                self.assertEqual(len(segments), 2)
                self.assertEqual(segments[-1][1]['origin']['human_root'], segments[0][1]['origin']['human_root'])
                self.assertEqual(segments[-1][1]['origin']['native_trigger'], 'own-hidden-ambiguous')
                name = hashlib.sha256(control['id'].encode()).hexdigest() + '.json'
                self.assertFalse((segments[0][0] / 'captures' / name).exists())
                self.assertTrue((segments[-1][0] / 'captures' / name).exists())

    def test_crashed_pure_snapshot_never_reopens_sealed_parent(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, session, lifecycle = self.fixture(world)
            human = self.entry('human', 'user', 'Actual short request', attribution='user')
            result = self.observe(repo, session, lifecycle, [human])
            parent, _ = transport.segments(repo, session)[0]
            manifest = (parent / 'segment.json').read_bytes()
            wake = self.observe_request(repo, {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'wake',
                'session': session, 'lifecycle': result['lifecycle'], 'payload': result['wake_payload']}, delivery_window=LiveDeliveryWindow())
            control = {'type': 'custom_message', 'id': 'native-control', 'parentId': 'human',
                       'customType': 'session-stop-continuation', 'attribution': 'agent',
                       'content': wake['additionalContext']}
            replace = transport.replace_cursor
            def interrupted(path, value):
                if 'native-control' in (value.get('control_interval') or {}).get('pure_entries', {}):
                    raise OSError('Controlled crash after snapshot, before cursor publication')
                return replace(path, value)
            with patch.object(transport, 'replace_cursor', side_effect=interrupted):
                with self.assertRaises(OSError):
                    self.observe(repo, session, result['lifecycle'], [human, control], seal=False)
            substantive = self.entry('late-work', 'assistant', 'New substantive foreign work')
            self.observe(repo, session, result['lifecycle'], [human, control, substantive], seal=False,
                         terminal=True, trigger='own-hidden-ambiguous')
            segments = transport.segments(repo, session)
            name = hashlib.sha256(control['id'].encode()).hexdigest() + '.json'
            self.assertEqual((parent / 'segment.json').read_bytes(), manifest)
            self.assertFalse((parent / 'captures' / name).exists())
            self.assertTrue((segments[-1][0] / 'captures' / name).exists())
            self.assertTrue((parent / 'snapshots' / name).exists())
            self.assertTrue(transport.pending_inventory(repo)[1],
                            'the unqualified historical orphan must remain an explicit inventory fault')

    def test_protected_private_root_cannot_silently_move_or_lose_binding(self):
        with World() as world:
            repo, session, lifecycle = self.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                self.observe(repo, session, lifecycle, [self.entry('human', 'user', 'Actual source', attribution='user')])
                base = boundary._base(repo)
                self.assertIn(Path(repo).resolve(), boundary.source_capability_repositories())
                binding = base / 'omp-bank-binding-v1.json'
                binding.unlink()
                with self.assertRaises(ValueError):
                    boundary.required_source_capabilities(repo)
                with self.assertRaises(ValueError):
                    boundary.source_capability_repositories()



class StoredBatchControls(unittest.TestCase):
    def test_foreign_companions_and_human_input_never_become_own_maintenance(self):
        from agent_memory import omp_control
        own = {'type': 'custom_message', 'id': 'own', 'parentId': None, 'customType': 'session-stop-continuation',
               'attribution': 'agent', 'content': 'exact reserved guidance'}
        self.assertIsNotNone(omp_control.pure_batch([own], 'exact reserved guidance'))
        for custom_type in ('goal-mode-context', 'plan-mode', 'vibe-mode-context', 'todo-error-reminder',
                            'advisor', 'MEMORY_RECALL_CHANGES', 'foreign-before-agent', 'unknown'):
            foreign = {**own, 'id': 'foreign', 'customType': custom_type, 'content': 'Foreign contextual work'}
            self.assertIsNone(omp_control.pure_batch([own, foreign], 'exact reserved guidance'))
        human = {'type': 'message', 'id': 'human', 'parentId': None,
                 'message': {'role': 'user', 'attribution': 'user', 'content': 'Actual new human request'}}
        self.assertIsNone(omp_control.pure_batch([human, own], 'exact reserved guidance'))
        self.assertIsNone(omp_control.pure_batch([{**own, 'content': 'exact reserved guidance\\njoined foreign stop text'}],
                                               'exact reserved guidance'))


class OwnedInstructionBoundary(unittest.TestCase):
    """Structural carrier fixtures exercise real installation ownership, not native support."""

    def installed(self, world, mode='copy'):
        source = Path(__file__).resolve().parents[2]
        root = world.inside('owned native root')
        root.mkdir()
        result = world.run([sys.executable, '-I', '-B',
                            source / 'skills/agent-memory/scripts/installer.py',
                            'install', '--development-source', source, '--presets', 'omp',
                            '--omp-agent-dir', root, '--omp-peers-upgraded', '--mode', mode,
                            '--bin-dir', world.inside('owned bin')], timeout=30)
        receipt = json.loads(result.stdout)['receipt']
        witness = omp_installation._owned_manual(receipt)
        self.assertIsNotNone(witness)
        return receipt, witness

    def pair(self, witness, *, verified=None):
        text, path, _, _, _, _ = witness._verify() if verified is None else verified
        body = text.rstrip('\n')
        lines = len(body.split('\n'))
        invocation = {'type': 'message', 'id': 'manual-invocation', 'parentId': 'human',
                      'message': {'role': 'assistant', 'content': [
                          {'type': 'toolCall', 'id': 'manual-call', 'name': 'read',
                           'arguments': {'i': 'Reading installed guide', 'path': 'skill://agent-memory'}}]}}
        entry = {'type': 'message', 'id': 'manual-result', 'parentId': 'manual-invocation',
                 'message': {'role': 'toolResult', 'toolCallId': 'manual-call', 'toolName': 'read',
                             'isError': False, 'content': [{'type': 'text',
                                 'text': '[Skill file: ' + path + ']\n' + body}],
                             'details': {'contentType': 'text/markdown', 'resolvedPath': path,
                                 'rowPrefixes': False, 'totalLines': lines,
                                 'displayContent': {'lineNumbers': list(range(1, lines + 1)),
                                                    'startLine': 1, 'text': body},
                                 'meta': {'source': {'type': 'internal', 'value': 'skill://agent-memory'}}}}}
        return invocation, entry

    def test_exact_carrier_and_unforgeable_witness(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            invocation, entry = self.pair(witness)
            projected = omp_projection._project_owned_manual(entry, invocation, witness, witness._verify())
            self.assertIsNotNone(projected)
            snapshot = projected['snapshot']
            self.assertEqual(snapshot['content'], [])
            self.assertNotIn('details', snapshot)
            self.assertEqual(snapshot['instruction_context']['observed_receipt_sha256'], transport.digest(receipt))
            self.assertEqual(snapshot['instruction_context']['invocation_native_entry_id'], invocation['id'])
            self.assertTrue(omp_projection._valid_instruction_context(snapshot))
            self.assertIsNone(omp_projection._project_owned_manual(entry, invocation, receipt, witness._verify()))
            self.assertIsNone(omp_projection._project_owned_manual(entry, invocation, {'trusted': True}, witness._verify()))
            forged = self.entry_for_forgery(snapshot['instruction_context'])
            ordinary = omp_projection.project(forged)
            self.assertNotIn('gap', ordinary)
            self.assertEqual(ordinary['snapshot']['content'], forged['message']['content'])
            self.assertNotIn('instruction_context', ordinary['snapshot'])
            with self.assertRaises(TypeError):
                json.dumps(witness)

    def entry_for_forgery(self, context):
        return {'type': 'message', 'id': 'forged-result', 'parentId': None,
                'message': {'role': 'toolResult', 'toolCallId': 'forged-call', 'toolName': 'read',
                            'content': [{'type': 'text', 'text': 'Ordinary benign result.'}],
                            'instruction_context': context}}

    def test_malformed_carriers_and_invocations_decline(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, witness = self.installed(world)
            invocation, entry = self.pair(witness)
            variants = []
            for mutate in (
                lambda m: m.update(isError=True),
                lambda m: m['content'].append({'type': 'text', 'text': 'extra'}),
                lambda m: m['content'][0].update(type='image'),
                lambda m: m['content'][0].update(text=m['content'][0]['text'][:-1]),
                lambda m: m['details'].update(extra='value'),
                lambda m: m['details'].update(resolvedPath='/foreign/SKILL.md'),
                lambda m: m['details'].update(totalLines=True),
                lambda m: m['details']['displayContent'].update(startLine=True),
                lambda m: m['details']['meta']['source'].update(value='skill://other'),
                lambda m: m.update(approved=True),
            ):
                candidate = copy.deepcopy(entry)
                mutate(candidate['message'])
                variants.append((invocation, candidate))
            for mutate in (
                lambda c: c['arguments'].update(path='skill://agent-memory:1-20'),
                lambda c: c['arguments'].update(path='/foreign/SKILL.md'),
                lambda c: c['arguments'].update(extra=True),
                lambda c: c.update(id='another-call'),
                lambda c: c.update(name='bash'),
            ):
                candidate = copy.deepcopy(invocation)
                mutate(candidate['message']['content'][0])
                variants.append((candidate, entry))
            duplicate = copy.deepcopy(invocation)
            duplicate['message']['content'].append(copy.deepcopy(duplicate['message']['content'][0]))
            variants.append((duplicate, entry))
            for call, result in variants:
                with self.subTest(call=call, result_keys=list(result['message'])):
                    self.assertIsNone(omp_projection._project_owned_manual(result, call, witness, witness._verify()))

    def test_witness_declines_changed_receipt_file_and_permissions(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            forged = copy.deepcopy(receipt)
            forged['installation_id'] = str(uuid.uuid4())
            self.assertIsNone(omp_installation._owned_manual(forged))
            manual = Path(receipt['omp_binding']['skill_target']) / 'SKILL.md'
            original = manual.read_bytes()
            mode = manual.stat().st_mode & 0o777
            manual.write_bytes(original + b'\nChanged installed bytes.\n')
            self.assertIsNone(witness._verify())
            self.assertIsNone(omp_installation._owned_manual(receipt))
            manual.write_bytes(original)
            manual.chmod(mode | 0o002)
            self.assertIsNone(witness._verify())
            self.assertIsNone(omp_installation._owned_manual(receipt))

    def test_link_witness_declines_retargeted_owned_link(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world, 'link')
            link = Path(receipt['omp_binding']['skill_target'])
            self.assertTrue(link.is_symlink())
            replacement = world.inside('foreign skill')
            replacement.mkdir()
            (replacement / 'SKILL.md').write_text('Foreign guide.\n')
            link.unlink()
            link.symlink_to(replacement, target_is_directory=True)
            self.assertIsNone(witness._verify())
            self.assertIsNone(omp_installation._owned_manual(receipt))

    def test_historical_annotation_is_closed_and_installation_independent(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            invocation, entry = self.pair(witness)
            snapshot = omp_projection._project_owned_manual(entry, invocation, witness, witness._verify())['snapshot']
            manual = Path(receipt['omp_binding']['skill_target']) / 'SKILL.md'
            manual.unlink()
            self.assertIsNone(witness._verify())
            self.assertTrue(omp_projection._valid_instruction_context(snapshot))
            self.assertTrue(omp_projection._valid_instruction_context({'role': 'user', 'content': []}))
            mutations = (
                lambda s: s.update(content=[{'type': 'text', 'text': 'not metadata'}]),
                lambda s: s.update(details={}),
                lambda s: s['instruction_context'].update(extra='value'),
                lambda s: s['instruction_context'].update(tool_call_id='different'),
                lambda s: s['instruction_context'].update(manual_file_sha256='z' * 64),
                lambda s: s['instruction_context'].update(renderer='another-renderer'),
                lambda s: s['instruction_context'].pop('installation_id'),
            )
            for mutate in mutations:
                changed = copy.deepcopy(snapshot)
                mutate(changed)
                self.assertFalse(omp_projection._valid_instruction_context(changed))

    def observe(self, repo, session, lifecycle, entries, receipt=None, *, seal=False):
        payload = observer_payload(repo, session, entries)
        if seal:
            payload['request_wake'] = False
        with patch.object(transport, '_verify_observer_receipt', side_effect=lambda *args, **kwargs: boundary.verify_marker(repo)) if receipt is None else nullcontext():
            return transport.observe(repo, {'schema': 'agent-memory/omp-observer-request-v2',
                                          'action': 'seal' if seal else 'ingest', 'session': session,
                                          'lifecycle': lifecycle, 'payload': payload}, receipt=receipt)

    def test_fresh_capture_preserves_surrounding_source_and_retries(self):
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world, patch.dict(os.environ, world.env, clear=True):
                receipt, witness = self.installed(world, mode)
                helper = NativeSources()
                repo, session, lifecycle = helper.fixture(world)
                invocation, entry = self.pair(witness)
                human = helper.entry('human', 'user', 'Review this task', attribution='user')
                answer = helper.entry('answer', 'assistant', 'The task is complete.')
                entries = [human, invocation, entry, answer]
                result = self.observe(repo, session, lifecycle, entries, receipt, seal=True)
                directory, _ = transport.segments(repo, session)[0]
                name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
                path = directory / 'snapshots' / name
                original = path.read_bytes()
                snapshot = transport.read(path)
                self.assertEqual(snapshot['content'], [])
                self.assertEqual(snapshot['instruction_context']['tool_call_id'], 'manual-call')
                self.assertTrue(omp_projection._valid_instruction_context(snapshot))
                for native in ('human', 'manual-invocation', 'answer'):
                    self.assertTrue((directory / 'captures' /
                                     (hashlib.sha256(native.encode()).hexdigest() + '.json')).exists())
                manifest = transport.read(directory / 'segment.json')
                self.assertFalse(any(g['native_entry_id'] == 'tool:manual-call' for g in manifest['gaps']))
                self.observe(repo, session, result['lifecycle'], entries, receipt)
                self.assertEqual(path.read_bytes(), original)

    def test_unrelated_ancestor_churn_preserves_witness_and_public_annotation(self):
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world, patch.dict(os.environ, world.env, clear=True):
                receipt, witness = self.installed(world, mode)
                helper = NativeSources()
                repo, session, lifecycle = helper.fixture(world)
                invocation, entry = self.pair(witness, verified=witness._comparison)
                human = helper.entry('human', 'user', 'Review this task', attribution='user')
                sibling = world.inside('unrelated sibling')
                original = omp_installation.observer_binding
                calls, changes = [], []

                def rendezvous(*args, **kwargs):
                    observed = original(*args, **kwargs)
                    calls.append(None)
                    if len(calls) % 2 == 0:
                        if sibling.exists():
                            sibling.rmdir()
                            changes.append('delete')
                        else:
                            sibling.mkdir()
                            changes.append('create')
                    return observed

                # Only schedule real namespace changes; all receipt, artifact,
                # descriptor and trust-policy verification still runs.
                with patch.object(omp_installation, 'observer_binding', side_effect=rendezvous):
                    with self.subTest(boundary='witness'):
                        self.assertIsNotNone(witness._verify())
                    result = self.observe(repo, session, lifecycle, [human, invocation, entry],
                                          receipt, seal=True)
                self.assertIn('create', changes)
                self.assertIn('delete', changes)
                directory, _ = transport.segments(repo, session)[0]
                name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
                snapshot_path = directory / 'snapshots' / name
                self.assertTrue(snapshot_path.exists(), result)
                snapshot = transport.read(snapshot_path)
                self.assertEqual(snapshot['content'], [])
                self.assertEqual(snapshot['instruction_context']['tool_call_id'], 'manual-call')
                self.assertTrue(omp_projection._valid_instruction_context(snapshot))
                manifest = transport.read(directory / 'segment.json')
                self.assertFalse(any(g['native_entry_id'] == 'tool:manual-call'
                                     for g in manifest['gaps']))

    def test_verification_races_refuse_selected_binding_payload_and_policy_changes(self):
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world, patch.dict(os.environ, world.env, clear=True):
                receipt, witness = self.installed(world, mode)
                skill = Path(receipt['omp_binding']['skill_target'])
                manual = skill / 'SKILL.md'
                parent = skill.parent
                state, _ = installation.locations()
                receipt_path = state / 'receipt.json'
                original_manual, original_receipt = manual.read_bytes(), receipt_path.read_bytes()
                manual_mode, parent_mode = manual.stat().st_mode & 0o777, parent.stat().st_mode & 0o777
                receipt_mode = receipt_path.stat().st_mode & 0o777
                original = omp_installation.observer_binding
                races = ['directory-substitution', 'directory-mode', 'receipt', 'receipt-mode']
                if mode == 'copy':
                    races += ['manual-substitution', 'manual-symlink', 'manual-bytes', 'manual-mode']
                if mode == 'link':
                    races += ['link-substitution', 'link-retarget']
                for race in races:
                    with self.subTest(race=race):
                        calls = []
                        mutated = False
                        selected = (parent if race == 'directory-substitution' else
                                    skill if race.startswith('link-') else manual)
                        backup = selected.with_name(selected.name + '.race-backup')

                        def mutate():
                            nonlocal mutated
                            if race == 'directory-substitution':
                                parent.rename(backup)
                                parent.mkdir(mode=parent_mode)
                            elif race == 'directory-mode':
                                parent.chmod(parent_mode | 0o002)
                            elif race in {'manual-substitution', 'manual-symlink'}:
                                manual.rename(backup)
                                if race == 'manual-symlink':
                                    manual.symlink_to(backup)
                                else:
                                    manual.write_bytes(original_manual)
                                    manual.chmod(manual_mode)
                            elif race == 'manual-bytes':
                                manual.write_bytes(bytes([original_manual[0] ^ 1]) + original_manual[1:])
                            elif race == 'manual-mode':
                                manual.chmod(manual_mode | 0o002)
                            elif race == 'receipt':
                                changed = copy.deepcopy(receipt)
                                changed['installation_id'] = uuid.uuid4().hex
                                receipt_path.write_text(json.dumps(changed))
                            elif race == 'receipt-mode':
                                receipt_path.chmod(receipt_mode | 0o002)
                            else:
                                skill.rename(backup)
                                destination = (receipt['source']['pack'] if race == 'link-substitution'
                                               else str(world.paths['repo']))
                                skill.symlink_to(destination, target_is_directory=True)
                            mutated = True

                        def rendezvous(*args, **kwargs):
                            calls.append(None)
                            # Receipt mutations precede its final genuine read.
                            # Other races occur after genuine artifact readback,
                            # so the descriptor/named-entry pins must refuse.
                            if len(calls) == 2 and race.startswith('receipt'):
                                mutate()
                            observed = original(*args, **kwargs)
                            if len(calls) == 2 and not race.startswith('receipt'):
                                mutate()
                            return observed

                        try:
                            with patch.object(omp_installation, 'observer_binding', side_effect=rendezvous):
                                self.assertIsNone(witness._verify())
                            self.assertTrue(mutated)
                        finally:
                            if race == 'directory-substitution' and backup.exists():
                                parent.rmdir()
                                backup.rename(parent)
                            elif race == 'directory-mode':
                                parent.chmod(parent_mode)
                            elif race in {'manual-substitution', 'manual-symlink'} and backup.exists():
                                manual.unlink()
                                backup.rename(manual)
                            elif race == 'manual-bytes':
                                manual.write_bytes(original_manual)
                            elif race == 'manual-mode':
                                manual.chmod(manual_mode)
                            elif race == 'receipt':
                                receipt_path.write_bytes(original_receipt)
                            elif race == 'receipt-mode':
                                receipt_path.chmod(receipt_mode)
                            elif race.startswith('link-') and backup.is_symlink():
                                skill.unlink()
                                backup.rename(skill)

    def test_nonread_result_never_mints_manual_witness(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, _ = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            invocation = helper.entry('call', 'assistant', [
                {'type': 'toolCall', 'id': 'bash-call', 'name': 'bash',
                 'arguments': {'command': 'printf 12'}}])
            result = helper.entry('result', 'toolResult', '12',
                                  toolCallId='bash-call', toolName='bash', isError=False)
            with patch.object(omp_installation, '_owned_manual',
                              side_effect=AssertionError('non-read minted manual witness')):
                self.observe(repo, session, lifecycle, [human, invocation, result], receipt, seal=True)
            directory, _ = transport.segments(repo, session)[0]
            name = hashlib.sha256(result['id'].encode()).hexdigest() + '.json'
            snapshot = transport.read(directory / 'snapshots' / name)
            self.assertEqual(snapshot['toolName'], 'bash')
            self.assertNotIn('instruction_context', snapshot)

    def test_screened_invocation_does_not_hide_following_answer(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            invocation['message']['content'][0]['arguments']['i'] = 'password=secretvalue'
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            answer = helper.entry('answer', 'assistant', 'The task is complete.')
            self.observe(repo, session, lifecycle, [human, invocation, entry, answer], receipt, seal=True)
            directory, _ = transport.segments(repo, session)[0]
            gaps = [transport.read(path) for path in (directory / 'gaps').glob('*.json')]
            self.assertTrue(any(g['native_entry_id'] == invocation['id'] for g in gaps))
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            result_snapshot = directory / 'snapshots' / name
            if result_snapshot.exists():
                self.assertNotIn('instruction_context', transport.read(result_snapshot))
            answer_name = hashlib.sha256(answer['id'].encode()).hexdigest() + '.json'
            self.assertTrue((directory / 'snapshots' / answer_name).exists())
            self.assertTrue(transport.pending_inventory(repo)[0])

    def test_snapshot_publication_recovery_survives_uninstall_and_retry(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            self.observe(repo, session, lifecycle, [human, invocation], receipt)
            directory, _ = transport.segments(repo, session)[0]
            snapshot = omp_projection._project_owned_manual(entry, invocation, witness, witness._verify())['snapshot']
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            path = directory / 'snapshots' / name
            transport.retain(path, snapshot)
            original = path.read_bytes()
            (Path(receipt['omp_binding']['skill_target']) / 'SKILL.md').unlink()
            self.observe(repo, session, lifecycle, [human, invocation, entry])
            capture = transport.read(directory / 'captures' / name)
            self.assertEqual(capture['snapshot_sha256'], transport.digest(snapshot))
            self.assertEqual(path.read_bytes(), original)
            self.observe(repo, session, lifecycle, [human, invocation, entry])
            self.assertEqual(path.read_bytes(), original)
            self.assertFalse(list((directory / 'gaps').glob('*.json')))

    def test_mismatched_orphan_snapshot_is_integrity_failure(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            self.observe(repo, session, lifecycle, [human, invocation], receipt)
            directory, _ = transport.segments(repo, session)[0]
            snapshot = omp_projection._project_owned_manual(entry, invocation, witness, witness._verify())['snapshot']
            snapshot['native_entry_id'] = 'another-entry'
            self.assertTrue(omp_projection._valid_instruction_context(snapshot))
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            path = directory / 'snapshots' / name
            transport.retain(path, snapshot)
            original = path.read_bytes()
            with self.assertRaises(ValueError):
                self.observe(repo, session, lifecycle, [human, invocation, entry], receipt)
            self.assertEqual(path.read_bytes(), original)
            self.assertFalse((directory / 'captures' / name).exists())
            self.assertFalse(list((directory / 'gaps').glob('*.json')))

    def test_preexisting_native_gap_is_not_reclassified(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            unavailable = copy.deepcopy(entry)
            unavailable['message']['content'] = [{'type': 'image', 'data': 'opaque'}]
            self.observe(repo, session, lifecycle, [human, invocation, unavailable], receipt)
            directory, _ = transport.segments(repo, session)[0]
            gaps = {p.name: p.read_bytes() for p in (directory / 'gaps').glob('*.json')}
            self.assertTrue(gaps)
            self.observe(repo, session, lifecycle, [human, invocation, entry], receipt)
            self.assertEqual(gaps, {p.name: p.read_bytes() for p in (directory / 'gaps').glob('*.json')})
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            path = directory / 'snapshots' / name
            if path.exists():
                self.assertNotIn('instruction_context', transport.read(path))
            self.assertTrue(transport.pending_inventory(repo)[0])

    def test_sealed_tool_gap_does_not_authorize_late_classification(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            human = helper.entry('human', 'user', 'Review this task', attribution='user')
            result = self.observe(repo, session, lifecycle, [human, invocation], receipt, seal=True)
            directory, _ = transport.segments(repo, session)[0]
            manifest_path = directory / 'segment.json'
            before = manifest_path.read_bytes()
            self.assertTrue(any(g['native_entry_id'] == 'tool:manual-call'
                                for g in transport.read(manifest_path)['gaps']))
            self.observe(repo, session, result['lifecycle'], [human, invocation, entry], receipt)
            self.assertEqual(manifest_path.read_bytes(), before)
            self.assertFalse((directory / 'resolutions').exists() and
                             list((directory / 'resolutions').glob('*.json')))
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            for segment, _ in transport.segments(repo, session):
                path = segment / 'snapshots' / name
                if path.exists():
                    self.assertNotIn('instruction_context', transport.read(path))

    def test_same_sweep_human_boundary_preserves_sealed_tool_gap(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            receipt, witness = self.installed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            invocation, entry = self.pair(witness)
            first = helper.entry('human', 'user', 'Review the first task', attribution='user')
            second = helper.entry('second-human', 'user', 'Review the next task', attribution='user')
            entries = [first, invocation, second, entry]
            result = self.observe(repo, session, lifecycle, entries, receipt)
            segments = transport.segments(repo, session)
            original = next(directory for directory, _ in segments
                            if (directory / 'segment.json').exists()
                            and any(g['native_entry_id'] == 'tool:manual-call'
                                    for g in transport.read(directory / 'segment.json')['gaps']))
            manifest = (original / 'segment.json').read_bytes()
            gap_value = next(g for g in transport.read(original / 'segment.json')['gaps']
                             if g['native_entry_id'] == 'tool:manual-call')
            gap_path = original / 'gaps' / (gap_value['gap_id'] + '.json')
            gap_bytes = gap_path.read_bytes()
            resolution = original / 'resolutions' / (gap_value['gap_id'] + '.json')
            self.assertFalse(resolution.exists())
            name = hashlib.sha256(entry['id'].encode()).hexdigest() + '.json'
            for directory, _ in segments:
                path = directory / 'snapshots' / name
                if path.exists():
                    self.assertNotIn('instruction_context', transport.read(path))
            self.observe(repo, session, result['lifecycle'], entries, receipt)
            self.assertEqual((original / 'segment.json').read_bytes(), manifest)
            self.assertEqual(gap_path.read_bytes(), gap_bytes)
            self.assertFalse(resolution.exists())
            self.assertTrue(transport.pending_inventory(repo)[0])




class ExplicitNativeOwnership(unittest.TestCase):
    def test_explicit_native_install_applies_preview_for_copy_and_link(self):
        source = Path(__file__).resolve().parents[2]
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world:
                root = world.inside('explicit native root')
                root.mkdir()
                bin_dir = world.inside('owned bin')
                command = [sys.executable, '-I', '-B',
                           source / 'skills/agent-memory/scripts/installer.py',
                           'install', '--development-source', source, '--presets', 'omp',
                           '--omp-agent-dir', root, '--omp-peers-upgraded',
                           '--mode', mode, '--bin-dir', bin_dir]
                preview = json.loads(world.run(command + ['--check'], timeout=30).stdout)
                self.assertFalse(preview['installation_writes_performed'])
                self.assertFalse((root / 'extensions/agent-memory.ts').exists())
                applied = json.loads(world.run(command, timeout=30).stdout)
                self.assertTrue(applied['verified'])
                receipt = applied['receipt']
                self.assertEqual(receipt['omp_binding'], preview['omp']['binding'])
                self.assertEqual(receipt['schema'], 'agent-memory/install-receipt-v2')
                for target in preview['targets']:
                    self.assertTrue(Path(target['path']).exists(), target['path'])
                self.assertEqual(json.loads(world.run(
                    [bin_dir / 'am', '--json', 'version'], timeout=30).stdout)['executing_source']['version'],
                    receipt['source']['version'])

    def test_all_never_enables_omp_and_root_is_required(self):
        self.assertNotIn('omp', {name for name, _, _ in installation.presets('all')})
        with self.assertRaises(ValueError):
            installation.presets('omp')

    def test_explicit_root_and_peer_attestation_never_come_from_environment(self):
        from argparse import Namespace
        args = Namespace(omp_agent_dir=None, omp_peers_upgraded=False)
        with self.assertRaises(ValueError):
            omp_installation.configuration(args, None, 'omp', False)
        with self.assertRaises(ValueError):
            omp_installation.binding('relative/root')

    def test_saved_root_changes_and_implicit_all_removal_refuse(self):
        from argparse import Namespace
        saved = omp_installation.binding('/private/native-owned')
        old = {'omp_binding': saved}
        with self.assertRaises(ValueError):
            omp_installation.configuration(Namespace(omp_agent_dir='/private/different', omp_peers_upgraded=True), old, 'omp', False)
        with self.assertRaises(ValueError):
            omp_installation.configuration(Namespace(omp_agent_dir=None, omp_peers_upgraded=False), old, 'all', False)


class NativeLifecyclePublic(unittest.TestCase):
    def managed(self, world, mode='copy'):
        from runtime_update_fixtures import PACK
        channel = world.repository('native lifecycle channel')
        pack = channel / 'skills/agent-memory'
        shutil.copytree(PACK, pack)
        world.git('add', '.', cwd=channel)
        world.git('commit', '-m', 'Controlled native lifecycle runtime', cwd=channel)
        root = world.inside('explicit native lifecycle root')
        root.mkdir()
        result = world.run([sys.executable, '-I', '-B', PACK / 'scripts/installer.py',
                            'install', '--remote', channel, '--ref', 'main', '--mode', mode,
                            '--presets', 'omp', '--omp-agent-dir', root, '--omp-peers-upgraded'], timeout=30)
        return channel, pack, json.loads(result.stdout)['receipt']

    def public(self, world, receipt, *args, expected=0):
        result = world.run([Path(receipt['bin_dir']) / 'am', '--json', *args], check=False, timeout=30)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def extension(self, receipt):
        path = Path(receipt['omp_binding']['extension_target'])
        owned = next(target for target in receipt['targets'] if target['path'] == str(path))
        self.assertEqual(path.stat().st_mode & 0o777, 0o644)
        self.assertEqual(hashlib.sha256(path.read_bytes()).hexdigest(), owned['identity']['hash'])
        return path

    def enroll_fixture(self, repo, session):
        # Controlled source-verifier/lifecycle setup, NOT native proof. Complete
        # actual retained enrollments without the observer's wall-clock budget;
        # native runs must enroll through their real callbacks independently.
        for directory, cursor in transport.segments(repo, session):
            for path in sorted((directory / 'enrollments').glob('*.json')):
                _, faults = transport.registration(repo, directory, cursor, json.loads(path.read_text()), perform=True)
                self.assertFalse(faults)

    def update_journey(self, world, receipt, advance):
        from runtime_update_fixtures import snapshot, receipt_path
        extension = self.extension(receipt)
        before = snapshot(extension), receipt_path(world).read_bytes()
        self.assertTrue(self.public(world, receipt, 'update', '--check')['check_complete'])
        unchanged = self.public(world, receipt, 'update')
        self.assertTrue(unchanged['verified'])
        self.assertFalse(unchanged['changed'])
        self.assertEqual((snapshot(extension), receipt_path(world).read_bytes()), before)
        advance()
        before = snapshot(extension), receipt_path(world).read_bytes()
        self.assertTrue(self.public(world, receipt, 'update', '--check')['update_available'])
        self.assertEqual((snapshot(extension), receipt_path(world).read_bytes()), before)
        changed = self.public(world, receipt, 'update')
        self.assertTrue(changed['verified'])
        self.assertTrue(changed['changed'])
        self.extension(changed['receipt'])
        self.assertFalse(self.public(world, changed['receipt'], 'update')['changed'])

    def test_managed_native_updates_read_back_only_owned_public_commands(self):
        from runtime_update_fixtures import advance
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world:
                channel, pack, receipt = self.managed(world, mode)
                self.update_journey(world, receipt, lambda: advance(world, channel, pack, '1.0.0'))

    def test_native_update_ignores_missing_historical_worktree_but_preserves_required_enrollment(self):
        from runtime_update_fixtures import advance, owned, receipt_path, snapshot
        from agent_memory.frontmatter import ContractError
        with World() as world:
            channel, pack, receipt = self.managed(world)
            repo, _, _ = NativeSources().fixture(world)
            world.git('add', '.', cwd=repo)
            world.git('commit', '-m', 'Bound native project bank', cwd=repo)
            missing = world.inside('deleted native session')
            world.git('worktree', 'add', '-b', 'deleted-native-session', missing, cwd=repo)
            shutil.rmtree(missing)
            before = snapshot(repo)
            revision = advance(world, channel, pack)
            all_before = snapshot(world.root)
            preview = self.public(world, receipt, 'update', '--check')
            self.assertTrue(preview['check_complete'])
            self.assertEqual(snapshot(world.root), all_before)
            self.assertTrue(any(row['location'] == str(missing)
                                for row in preview['compatibility']['discovery_gaps']))
            updated = self.public(world, receipt, 'update')
            self.assertTrue(updated['verified'])
            receipt = updated['receipt']
            self.assertEqual(receipt['source']['revision'], revision)
            self.assertEqual(snapshot(repo), before)
            self.assertFalse(self.public(world, receipt, 'update')['changed'])
            self.assertTrue(self.public(world, receipt, 'recover')['verified'])
            self.assertEqual(snapshot(repo), before)
            # Losing the actually enrolled repository is not a historical gap.
            retained = world.inside('retained unavailable native project')
            repo.rename(retained)
            protected = owned(receipt), receipt_path(world).read_bytes(), snapshot(world.paths['state'])
            with patch.dict(os.environ, world.env, clear=True):
                with self.assertRaises(ContractError):
                    boundary._enrolled_repositories(receipt)
            refused = world.run([Path(receipt['bin_dir']) / 'am', '--json', 'update', '--check'],
                                cwd=world.paths['home'], check=False, timeout=30)
            self.assertEqual(refused.returncode, 1, refused.stdout + refused.stderr)
            self.assertFalse(json.loads(refused.stdout)['verified'])
            self.assertEqual((owned(receipt), receipt_path(world).read_bytes(),
                              snapshot(world.paths['state'])), protected)
            self.assertEqual(snapshot(retained), before)

    def test_signed_native_updates_preserve_extension_integrity(self):
        from signed_release_fixtures import SignedRelease
        with World() as world, SignedRelease(world) as release:
            release.prepare()
            root = world.inside('signed native root')
            root.mkdir()
            installed = world.run(['bash', '-o', 'pipefail', '-c',
                'curl -fsSL "$1" | bash -s -- --json --mode copy --presets omp '
                '--omp-agent-dir "$2" --omp-peers-upgraded',
                'fixture', release.base + '/download/v0.0.3/install.sh', root],
                timeout=120, check=False)
            self.assertEqual(installed.returncode, 0, installed.stdout + installed.stderr)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['omp_binding']['agent_dir'], str(root))
            self.assertEqual(receipt['source']['kind'], 'release')
            self.update_journey(world, receipt, release.advance)

    def test_public_uninstall_requires_quiescence_and_actually_drains_completed_sources(self):
        from runtime_update_fixtures import snapshot
        with World() as world:
            _, _, receipt = self.managed(world)
            extension = self.extension(receipt)
            initial = snapshot(extension)
            preview = self.public(world, receipt, 'uninstall', '--check', '--omp-writers-quiesced')
            self.assertTrue(preview['check_complete'])
            self.assertEqual(snapshot(extension), initial)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                helper.observe(repo, session, lifecycle, [
                    helper.entry('human', 'user', 'Controlled native lifecycle work', attribution='user')])
                self.enroll_fixture(repo, session)
                for row in transport.pending_inventory(repo)[0]:
                    helper.acknowledge(repo, row)
                base = boundary._base(repo)
            self.extension(receipt)
            retained = snapshot(base / 'omp-source-v1')
            before = snapshot(extension), snapshot(base)
            denied = self.public(world, receipt, 'uninstall', '--check', expected=1)
            self.assertFalse(denied['verified'])
            self.assertEqual((snapshot(extension), snapshot(base)), before)
            # Check mode cannot publish a drain proof or clear the real marker;
            # the common capability union must still refuse reader removal.
            preview = self.public(world, receipt, 'uninstall', '--check', '--omp-writers-quiesced', expected=1)
            self.assertIn('Private-state runtime compatibility', preview['error'])
            self.assertEqual((snapshot(extension), snapshot(base)), before)
            result = self.public(world, receipt, 'uninstall', '--omp-writers-quiesced')
            self.assertTrue(result['verified'])
            self.assertFalse(extension.exists())
            self.assertFalse((base / 'turn-sources-compat-v1.json').exists())
            self.assertTrue(list((base / 'omp-drains').glob('*.json')))
            self.assertEqual(snapshot(base / 'omp-source-v1'), retained)
            with patch.dict(os.environ, world.env, clear=True):
                self.assertEqual(boundary.required_source_capabilities(repo), set())

    def test_public_uninstall_keeps_pending_sources_and_disabled_owned_binding(self):
        with World() as world:
            _, _, receipt = self.managed(world)
            helper = NativeSources()
            repo, session, lifecycle = helper.fixture(world)
            with patch.dict(os.environ, world.env, clear=True):
                helper.observe(repo, session, lifecycle, [
                    helper.entry('human', 'user', 'Still unassessed controlled source', attribution='user')])
                base = boundary._base(repo)
            extension = self.extension(receipt)
            result = self.public(world, receipt, 'uninstall', '--omp-writers-quiesced', expected=1)
            self.assertFalse(result['verified'])
            self.assertFalse(extension.exists())
            self.assertTrue((base / 'turn-sources-compat-v1.json').exists())
            self.assertFalse(list((base / 'omp-drains').glob('*.json')))
            self.assertTrue(list(extension.parent.glob('.agent-memory-disabled-*.ts.disabled')))


class NativeSmokeQualification(unittest.TestCase):
    """Controlled protocol fixtures only: never native/provider qualification."""
    def fixture(self, world):
        from runtime_update_fixtures import ROOT, PACK, receipt_path
        helper = NativeSources()
        repo, session, lifecycle = helper.fixture(world)
        root = world.inside('native smoke root')
        root.mkdir()
        installed = json.loads(world.run([sys.executable, '-I', '-B', PACK / 'scripts/installer.py',
            'install', '--development-source', ROOT, '--presets', 'omp', '--omp-agent-dir', root,
            '--omp-peers-upgraded'], timeout=30).stdout)
        with patch.dict(os.environ, world.env, clear=True):
            helper.observe(repo, session, lifecycle, [
                helper.entry('historical-human', 'user', 'Historical direct source, not current native proof', attribution='user')])
        return repo, installed['receipt'], receipt_path(world)

    def binary(self, world, receipt, *, version='18.16.0', mode='disabled'):
        """Fake only the native protocol; real installed CLI verifies sources."""
        path = world.inside('controlled-native-protocol')
        am = str(Path(receipt['bin_dir']) / 'am')
        path.write_text('#!' + sys.executable + '\n' + 'VERSION = ' + repr(version) + '\nMODE = ' + repr(mode)
            + '\nAM = ' + repr(am) + '\nARTIFACT = ' + repr(receipt['source']['artifact_digest'])
            + '\nRECEIPT = ' + repr(receipt) + '\nTESTS = ' + repr(str(Path(__file__).resolve().parents[1])) + '\n' + r'''
import hashlib, json, os, re, subprocess, sys, uuid
from pathlib import Path
from types import SimpleNamespace
sys.path.insert(0, TESTS)
from contracts.test_omp_boundary import observe_installed, observer_payload
if '--version' in sys.argv:
    print('omp/' + VERSION); raise SystemExit()
Path(__file__).with_suffix('.launched').write_text('Controlled protocol launch, never model proof.')
root = Path(sys.argv[sys.argv.index('--session-dir') + 1]); root.mkdir()
repo = sys.argv[sys.argv.index('--cwd') + 1]
session = str(uuid.uuid4()); lifecycle = str(uuid.uuid4()); entries = []
path = root / 'current.jsonl'
path.write_text(json.dumps({'type': 'session', 'id': session, 'cwd': repo}) + '\n')
print(json.dumps({'type': 'ready'}), flush=True)
for line in sys.stdin:
    value = json.loads(line)
    if value['type'] == 'prompt':
        entry = {'type': 'message', 'id': str(uuid.uuid4()),
                 'parentId': entries[-1]['id'] if entries else None,
                 'message': {'role': 'user', 'attribution': 'user', 'content': value['message']}}
        with path.open('a') as stream:
            stream.write(json.dumps(entry) + '\n')
        entries.append(entry)
        if MODE != 'disabled' and (MODE != 'first-only' or len(entries) == 1):
            retained = json.loads(json.dumps(entries))
            if len(entries) == 2 and MODE == 'wrong-text':
                retained[-1]['message']['content'] = 'Wrong retained human source text.'
            if len(entries) == 2 and MODE == 'wrong-id':
                retained[-1]['id'] = 'foreign-native-human'
            payload = observer_payload(repo, session, retained)
            payload['request_wake'] = False
            request = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal',
                       'session': session, 'lifecycle': lifecycle, 'payload': payload}
            observed = observe_installed(SimpleNamespace(env=dict(os.environ)), AM, repo, RECEIPT, request)
            observed.check_returncode()
            lifecycle = json.loads(observed.stdout)['lifecycle']
        sentinel = re.search(r'OMP_SMOKE_(?:FIRST|SECOND)_[a-f0-9]+', value['message'])[0]
        print(json.dumps({'type': 'message_end', 'message': {'role': 'assistant', 'content': sentinel}}), flush=True)
        print(json.dumps({'type': 'session_settled'}), flush=True)
    elif value['type'] == 'get_state':
        if MODE == 'missing-session':
            path.unlink()
        late = value['id'].startswith(('observer-progress-', 'observer-final-'))
        if MODE == 'late-duplicate' and late:
            print(json.dumps({'type': 'message_end', 'message': {'role': 'assistant', 'content': sentinel}}), flush=True)
        print(json.dumps({'type': 'response', 'id': value['id'], 'data': {
            'sessionId': session, 'sessionFile': str(path),
            'isSettled': not (MODE == 'late-unsettled' and late)}}), flush=True)
''')
        path.chmod(0o700)
        return path

    def smoke(self, world, repo, receipt, receipt_path, binary, *, expected=1, timeout=5):
        from runtime_update_fixtures import ROOT
        output = world.inside('controlled-smoke')
        completed = world.run([sys.executable, '-I', '-B', ROOT / 'scripts/evaluations/omp_native_smoke.py',
            '--binary', binary, '--am', Path(receipt['bin_dir']) / 'am', '--receipt', receipt_path,
            '--repo', repo, '--output', output, '--model', 'controlled/no-provider',
            '--timeout', str(timeout)], check=False, timeout=max(30, timeout * 4 + 15))
        self.assertEqual(completed.returncode, expected, completed.stdout + completed.stderr)
        report = json.loads((output / 'report.json').read_text())
        self.assertEqual(report['qualified'], expected == 0)
        self.assertFalse(report['full_native_qualification'])
        return report

    def test_historical_observers_cannot_qualify_new_current_session(self):
        from runtime_update_fixtures import ROOT
        spec = importlib.util.spec_from_file_location('omp_native_smoke', ROOT / 'scripts/evaluations/omp_native_smoke.py')
        smoke = importlib.util.module_from_spec(spec); spec.loader.exec_module(smoke)
        with World() as world:
            repo, receipt, _ = self.fixture(world)
            root = world.inside('historical-verifier-run')
            (root / 'sessions').mkdir(parents=True)
            session = str(uuid.uuid4())
            prompts = ['First actual current task.', 'Second actual current task.']
            humans = [NativeSources().entry('current-first', 'user', prompts[0], attribution='user'),
                      NativeSources().entry('current-second', 'user', prompts[1],
                                            parent='current-first', attribution='user')]
            # A fork/copied-ID shape must not borrow another session's proof,
            # even when both native IDs and complete prompt bodies are identical.
            with patch.dict(os.environ, world.env, clear=True):
                NativeSources().observe(repo, str(uuid.uuid4()), str(uuid.uuid4()), humans)
            path = root / 'sessions/current.jsonl'
            path.write_text('\n'.join(json.dumps(value) for value in [
                {'type': 'session', 'id': session, 'cwd': str(repo)}, *humans]) + '\n')
            am = str(Path(receipt['bin_dir']) / 'am')
            status = json.loads(world.run([am, '--json', 'status', '--repo', repo], timeout=30).stdout)
            self.assertEqual([entry['id'] for entry in smoke.current_humans(
                repo, root, session, str(path), prompts)], ['current-first', 'current-second'])
            with self.assertRaises(smoke.PendingObservations):
                smoke.current_observations(am, repo, root, session, str(path), prompts, status, world.env)
            with patch.dict(os.environ, world.env, clear=True):
                self.assertEqual(transport.segments(repo, session), [])

    def test_both_exact_current_human_turns_and_persistent_session_are_required(self):
        for mode in ('first-only', 'wrong-text', 'wrong-id', 'missing-session'):
            with self.subTest(mode=mode), World() as world:
                repo, receipt, path = self.fixture(world)
                self.smoke(world, repo, receipt, path, self.binary(world, receipt, mode=mode))

    def test_unsupported_binary_refuses_before_native_launch_or_human_turns(self):
        with World() as world:
            repo, receipt, path = self.fixture(world)
            binary = self.binary(world, receipt, version='18.14.0')
            report = self.smoke(world, repo, receipt, path, binary)
            self.assertEqual(report['native_version'], 'omp/18.14.0')
            self.assertEqual(report['native_requests'], [])
            self.assertFalse(binary.with_suffix('.launched').exists())

    def test_source_verifier_binds_completed_human_ids_text_roots_and_digests(self):
        from runtime_update_fixtures import ROOT
        spec = importlib.util.spec_from_file_location('omp_native_smoke', ROOT / 'scripts/evaluations/omp_native_smoke.py')
        smoke = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(smoke)
        with World() as world:
            repo, receipt, _ = self.fixture(world)
            root = world.inside('source-verifier-run')
            (root / 'sessions').mkdir(parents=True)
            session = str(uuid.uuid4())
            path = root / 'sessions/current.jsonl'
            prompts = ['Controlled first human text.', 'Controlled second human text.']
            helper = NativeSources()
            humans = [helper.entry('current-first', 'user', prompts[0], attribution='user'),
                      helper.entry('current-second', 'user', prompts[1], attribution='user', parent='current-first')]
            path.write_text('\n'.join(json.dumps(value) for value in [
                {'type': 'title', 'v': 1, 'title': 'Controlled native session title',
                 'pad': '', 'updatedAt': '2026-11-06T00:00:00.000Z'},
                {'type': 'session', 'id': session, 'cwd': str(repo)}, *humans]) + '\n')
            with patch.dict(os.environ, world.env, clear=True):
                # A manual shared registration in this exact session still has
                # no observer capture/root, and cannot qualify either human.
                turn_sources.register(repo, 'omp', session, str(uuid.uuid4()), 'a' * 64)
            am = str(Path(receipt['bin_dir']) / 'am')
            status = json.loads(world.run([am, '--json', 'status', '--repo', repo], timeout=30).stdout)
            with self.assertRaises(RuntimeError):
                smoke.current_observations(am, repo, root, session, str(path), prompts, status, world.env)
            with patch.dict(os.environ, world.env, clear=True):
                first = helper.observe(repo, session, str(uuid.uuid4()), humans[:1])
                helper.observe(repo, session, first['lifecycle'], humans)
                NativeLifecyclePublic().enroll_fixture(repo, session)
                for row in transport.pending_inventory(repo)[0]:
                    if row.get('human_root') and row['session'] == session:
                        helper.acknowledge(repo, row)
            evidence = smoke.current_observations(am, repo, root, session, str(path), prompts, status, world.env)
            self.assertEqual([row['native_entry_id'] for row in evidence], ['current-first', 'current-second'])
            self.assertEqual(len({row['human_root'] for row in evidence}), 2)
            with self.assertRaises(RuntimeError):
                smoke.current_observations(am, repo, root, str(uuid.uuid4()), str(path), prompts, status, world.env)
            with self.assertRaises(RuntimeError):
                smoke.current_observations(am, repo, root, session, str(path), prompts[:1], status, world.env)
            with patch.dict(os.environ, world.env, clear=True):
                directory, _ = transport.segments(repo, session)[-1]
            cursor = directory / 'cursor.json'
            original = cursor.read_bytes()
            value = json.loads(original)
            value['origin']['human_root'] = evidence[0]['human_root']
            cursor.write_text(json.dumps(value))
            with self.assertRaises(RuntimeError):
                smoke.current_observations(am, repo, root, session, str(path), prompts, status, world.env)
            cursor.write_bytes(original)
            snapshot = directory / 'snapshots' / (hashlib.sha256(b'current-second').hexdigest() + '.json')
            snapshot.write_text(snapshot.read_text().replace(prompts[1], 'Tampered source digest.'))
            with self.assertRaises(RuntimeError):
                smoke.current_observations(am, repo, root, session, str(path), prompts, status, world.env)

    def test_source_verifier_waits_for_fault_free_shared_registration(self):
        from runtime_update_fixtures import ROOT
        spec = importlib.util.spec_from_file_location('omp_native_smoke', ROOT / 'scripts/evaluations/omp_native_smoke.py')
        smoke = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(smoke)
        with World() as world:
            repo, receipt, _ = self.fixture(world)
            root = world.inside('registration-verifier-run')
            (root / 'sessions').mkdir(parents=True)
            session = str(uuid.uuid4())
            prompt = 'Controlled ephemeral source awaiting registration.'
            human = NativeSources().entry('registration-human', 'user', prompt, attribution='user')
            lifecycle = str(uuid.uuid5(uuid.NAMESPACE_URL, str(repo.resolve()) + ':omp:' + session + ':' + human['id']))
            path = root / 'sessions/current.jsonl'
            path.write_text('\n'.join(json.dumps(value) for value in [
                {'type': 'session', 'id': session, 'cwd': str(repo)}, human]) + '\n')
            am = str(Path(receipt['bin_dir']) / 'am')
            status = json.loads(world.run([am, '--json', 'status', '--repo', repo], timeout=30).stdout)
            with patch.dict(os.environ, world.env, clear=True):
                directory, cursor = transport.new_segment(repo, session, lifecycle, lifecycle, 'human', 1)
                captured = transport.capture(directory, cursor, human)
                # Real stored-user identity and capture, with only shared
                # registration unfinished; never a legacy notification root.
                cursor['control_interval'] = {'human_entry': human['id']}
                cursor['last_native_entry'] = human['id']
                transport.replace_cursor(directory / 'cursor.json', cursor)
                transport.enroll_admission(directory, cursor, captured)
            with self.assertRaises(smoke.PendingObservations):
                smoke.current_observations(am, repo, root, session, str(path), [prompt], status, world.env)
            with patch.dict(os.environ, world.env, clear=True):
                NativeSources().observe(repo, session, lifecycle, [human])
            evidence = smoke.current_observations(am, repo, root, session, str(path), [prompt], status, world.env)
            self.assertEqual([row['native_entry_id'] for row in evidence], ['registration-human'])
            self.assertEqual([row['human_root'] for row in evidence], [lifecycle])

    def test_stream_readback_requires_terminal_receipts_and_current_leaf_not_child_quiescence(self):
        from runtime_update_fixtures import ROOT
        spec = importlib.util.spec_from_file_location('omp_native_smoke', ROOT / 'scripts/evaluations/omp_native_smoke.py')
        smoke = importlib.util.module_from_spec(spec); spec.loader.exec_module(smoke)
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo, receipt, _ = self.fixture(world)
            session, lifecycle = str(uuid.uuid4()), str(uuid.uuid4())
            human = NativeSources().entry('stream-human', 'user', 'Ephemeral checked source.', attribution='user')
            payload = observer_payload(repo, session, [human]); payload['request_wake'] = False
            request = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal',
                       'session': session, 'lifecycle': lifecycle, 'payload': payload}
            am = Path(receipt['bin_dir']) / 'am'
            observed = observe_installed(world, am, repo, receipt, request)
            self.assertEqual(observed.returncode, 0, observed.stdout + observed.stderr)
            native_file = world.inside('current-stream-native.jsonl')
            native_file.write_text('\n'.join(json.dumps(value) for value in [
                {'type': 'session', 'id': session, 'cwd': str(repo)}, human]) + '\n')
            status = json.loads(world.run([am, '--json', 'status', '--repo', repo]).stdout)
            evidence = smoke.current_stream_outcomes(repo, session, native_file, status, world.env, [])
            self.assertEqual(len(evidence), 1)
            self.assertEqual(evidence[0]['received_sequence'], 1)
            self.assertEqual(evidence[0]['terminal_sequence'], 1)
            qualifier = {'qualification': {'session': session, 'code': 'missing-outcome'}}
            with self.assertRaises(smoke.PendingObservations):
                smoke.current_stream_outcomes(repo, session, native_file, status, world.env, [qualifier])
            self.assertEqual(smoke.current_stream_outcomes(repo, session, native_file, status, world.env,
                [{'qualification': {'session': session, 'code': 'open-stream'}}]), evidence)
            # Physically late native usage/background records keep an explicitly
            # selected earlier leaf. They cannot make a healthy seal falsely pending.
            late = [dict(NativeSources().entry('late-background', 'assistant', 'Off-branch context.'),
                         activeLeafId='stream-human'),
                    {'type': 'model_usage', 'id': 'late-usage', 'parentId': 'stream-human',
                     'activeLeafId': 'stream-human'}]
            native_file.write_text(native_file.read_text() + ''.join(json.dumps(entry) + '\n' for entry in late))
            self.assertEqual(smoke.current_stream_outcomes(repo, session, native_file, status, world.env, []), evidence)
            # Exercise the actual /tmp alias on macOS; elsewhere a lexical dot
            # component still requires canonical comparison against OPEN.repo_root.
            alias = Path('/tmp') / repo.relative_to('/private/tmp') if str(repo).startswith('/private/tmp/') else repo / '.'
            self.assertEqual(smoke.current_stream_outcomes(alias, session, native_file, status, world.env, []), evidence)
            records = [human, *late]
            self.assertEqual(smoke.current_native_leaf(records), 'stream-human')
            self.assertIsNone(smoke.current_native_leaf([*records,
                {'type': 'custom', 'id': 'explicit-null', 'parentId': None, 'activeLeafId': None}]))
            self.assertEqual(smoke.current_native_leaf([*records,
                {'type': 'custom', 'id': 'dangling', 'parentId': None, 'activeLeafId': 'missing'}]), 'stream-human')
            native_file.write_text(native_file.read_text() + json.dumps(
                NativeSources().entry('uncaptured-leaf', 'assistant', 'Later source.')) + '\n')
            with self.assertRaises(smoke.PendingObservations):
                smoke.current_stream_outcomes(repo, session, native_file, status, world.env, [])

    def test_late_native_frames_cannot_reuse_initial_answer_or_settled_proof(self):
        for mode in ('complete', 'late-duplicate', 'late-unsettled'):
            with self.subTest(mode=mode), World() as world:
                repo, receipt, path = self.fixture(world)
                report = self.smoke(world, repo, receipt, path,
                    self.binary(world, receipt, mode=mode),
                    expected=0 if mode == 'complete' else 1, timeout=30)
                if mode == 'complete':
                    self.assertEqual(len(report['current_human_observations']), 2)
                    self.assertEqual(len({row['human_root'] for row in report['current_human_observations']}), 2)
                elif mode == 'late-duplicate':
                    self.assertTrue(any(count > 1 for count in report['task_sentinels'].values()))
                else:
                    self.assertFalse(report['native_is_settled'])
