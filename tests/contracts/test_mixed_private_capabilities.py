"""Disposable public lifecycle boundaries; synthetic sources are not native-client proof."""
import copy
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import unittest
import uuid

from world import World
import layout_recovery_fixtures as recovery
import v014_runtime_fixture
from runtime_update_fixtures import PACK, advance, owned, snapshot
from contracts import test_omp_boundary


class MixedPrivateCapabilities(unittest.TestCase):
    def test_layout_first_native_activation_requires_reviewed_new_routes_without_omp_preseed(self):
        with World() as world:
            command, source, repo, bank, native, inspected, applied = recovery.small_pending(world)
            self.assertFalse(applied['semantic_recovery_complete'])
            world.git('add', 'synthetic-policy.md', cwd=repo)
            world.git('commit', '-m', 'Synthetic baseline', cwd=repo)
            (repo / 'synthetic-policy.md').write_text(recovery.policy_body(0) + '\nStaged disposable change.\n')
            world.git('add', 'synthetic-policy.md', cwd=repo)
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            originals = {row['source_id']: (directory / row['blob']).read_bytes()
                         for row in inspected['manifest']['sources']}
            root = world.inside('explicit omp root'); root.mkdir()
            request = recovery.review_request(world, command, source, repo, inspected, native)
            attestation = request['quiescence_attestation']
            attestation['declared_invocation_routes'] += [str(root / path) for path in
                ('skills/agent-memory', 'extensions/agent-memory.ts', 'AGENTS.md')]
            attestation['declared_invocation_routes'].append(str(command.parent / 'agent-memory'))

            def envelope(att):
                return {'attestation': att, 'review': {
                    'schema': 'agent-memory/layout-quiescence-review-v1', 'review_id': att['review_id'],
                    'operation_id': att['operation_id'], 'attestation_sha256': recovery.sha(recovery.encoded(att)),
                    'reviewer_developer_id': att['operator_developer_id'], 'reviewer_harness': 'fixture-independent-reviewer',
                    'reviewer_session': 'mixed-' + str(uuid.uuid4()), 'review_time': att['issued_at'],
                    'decision': 'approve', 'rationale': 'Disposable independent review of the exact bank and new invocation routes.'}}

            args = [command, '--json', 'install', '--development-source', source, '--presets', 'omp',
                    '--omp-agent-dir', root, '--omp-peers-upgraded']
            def preserved():
                return (snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                        (repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout,
                        {row['source_id']: (directory / row['blob']).read_bytes()
                         for row in inspected['manifest']['sources']})
            before = preserved()
            baseline = snapshot(world.paths['state']), snapshot(root)
            variants = []
            wrong = copy.deepcopy(attestation)
            wrong['declared_invocation_routes'].remove(str(root / 'extensions/agent-memory.ts'))
            variants.append(envelope(wrong))
            wrong = copy.deepcopy(attestation)
            wrong['bank_path'] = str(repo / 'foreign-bank')
            variants.append(envelope(wrong))
            wrong = copy.deepcopy(attestation)
            wrong['runtime_artifact_digest'] = 'a' * 64
            variants.append(envelope(wrong))
            unreviewed = envelope(attestation)
            unreviewed['review']['reviewer_harness'] = request['author']['harness']
            unreviewed['review']['reviewer_session'] = request['author']['session']
            variants.append(unreviewed)
            for variant in variants:
                denied = world.run([*args, '--check', '--omp-layout-quiescence', recovery.save(world, variant)],
                                   check=False, timeout=60)
                self.assertNotEqual(denied.returncode, 0, denied.stdout + denied.stderr)
                self.assertEqual(preserved(), before)
                self.assertEqual((snapshot(world.paths['state']), snapshot(root)), baseline)
            proof = recovery.save(world, envelope(attestation))
            preview = json.loads(world.run([*args, '--check', '--omp-layout-quiescence', proof], timeout=60).stdout)
            self.assertEqual(preview['private_compatibility']['required_capabilities'], ['layout-frontier-v1'])
            self.assertEqual(preserved(), before)
            self.assertEqual((snapshot(world.paths['state']), snapshot(root)), baseline)
            result = json.loads(world.run([*args, '--omp-layout-quiescence', proof], timeout=60).stdout)
            self.assertTrue(result['verified'])
            test_omp_boundary.NativeLifecyclePublic().extension(result['receipt'])
            self.assertEqual(snapshot(bank), before[0])
            self.assertEqual(snapshot(world.paths['config']), before[2])
            self.assertEqual((repo / '.git/index').read_bytes(), before[3])
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, before[4])
            self.assertEqual({row['source_id']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            entry = {'type': 'message', 'id': 'synthetic-human', 'parentId': None,
                     'message': {'role': 'user', 'attribution': 'user',
                                 'content': 'Disposable synthetic carrier observation, no native client or provider.'}}
            payload = {'schema': 'agent-memory/omp-observer-request-v1', 'action': 'seal',
                       'session': str(uuid.uuid4()), 'lifecycle': str(uuid.uuid4()),
                       'payload': {'entries': [entry], 'branch_ids': ['synthetic-human'], 'event_ordinal': 1,
                                   'terminal': False, 'trigger': 'human', 'inherited_ids': [], 'metadata': [],
                                   'request_wake': False}}
            def input_command(args, value):
                return subprocess.run([str(command), *map(str, args)], cwd=repo, env=world.env,
                                      input=json.dumps(value), text=True, capture_output=True, timeout=60)
            observed = input_command(['omp-boundary', '--repo', repo, '--observer', '--stdin',
                                      '--artifact', result['receipt']['source']['artifact_digest'],
                                      '--omp-agent-dir', root], payload)
            self.assertEqual(observed.returncode, 0, observed.stdout + observed.stderr)
            self.assertTrue(json.loads(observed.stdout)['held'])
            def pending():
                response = world.run([command, 'omp-boundary', '--repo', repo, '--list-pending', '--offset', '0'],
                                     timeout=60)
                return json.loads(response.stdout)['pending']
            rows = pending()
            self.assertEqual(len(rows), 2)
            checked = json.loads(world.run([*args, '--check'], timeout=60).stdout)
            self.assertEqual(checked['private_compatibility']['required_capabilities'],
                             ['layout-frontier-v1', 'omp-harness-v1'])
            def assessment(row):
                inspected_source = json.loads(world.run([command, 'omp-boundary', '--repo', repo, '--source',
                    row['observation_id'], '--session', row['session'], '--lifecycle', row['lifecycle']],
                    timeout=60).stdout)
                self.assertTrue(inspected_source['verified'])
                self.assertTrue(inspected_source['coverage_complete'])
                self.assertEqual(inspected_source['faults'], [])
                data = inspected_source['source']
                if 'content' in data:
                    self.assertEqual(data['content'], entry['message']['content'])
                    review = 'Reviewed the exact disposable carrier message; it contains no project learning.'
                else:
                    self.assertEqual(data['gaps'], [])
                    self.assertEqual(len(data['captures']), 1)
                    review = 'Reviewed the sealed synthetic manifest and its sole inspected capture; no project learning.'
                value = {key: row[key] for key in ('session', 'lifecycle', 'observation_id', 'source_sha256')}
                value.update(outcome='no-learning', assessment={'source_id': row['observation_id'],
                             'source_review': review, 'candidates': []})
                return value
            assessments = [assessment(row) for row in rows]
            held_before = snapshot(bank), snapshot(directory), snapshot(world.paths['state'])
            refused = input_command(['omp-boundary', '--repo', repo, '--stdin'], assessments[0])
            self.assertNotEqual(refused.returncode, 0, refused.stdout + refused.stderr)
            self.assertEqual((snapshot(bank), snapshot(directory), snapshot(world.paths['state'])), held_before)
            proposal, transaction, _ = recovery.admission_proposal(world, command, source, repo, bank,
                                                                     inspected, full=True)
            admission = recovery.public(world, command, 'admission', 'apply', '--proposal', proposal)
            self.assertEqual(admission['operation_id'], transaction['operation_id'])
            self.assertTrue(admission['admission_verified'])
            status = recovery.call(world, command, 'status', '--repo', repo, '--operation', inspected['operation_id'])
            self.assertTrue(status['semantic_recovery_complete'])
            self.assertEqual([row['observation_id'] for row in pending()],
                             [row['observation_id'] for row in rows])
            self.assertEqual({row['source_id']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            for value in assessments:
                completed = input_command(['omp-boundary', '--repo', repo, '--stdin'], value)
                self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
                self.assertTrue(json.loads(completed.stdout)['verified'])
            self.assertEqual(pending(), [])
            uninstalled = json.loads(world.run([command, '--json', 'uninstall', '--omp-writers-quiesced'],
                                               timeout=60).stdout)
            self.assertTrue(uninstalled['verified'])

    def test_omp_first_mixed_pending_rejects_actual_published_old_managed_candidate(self):
        with World() as world:
            # Controlled transport only, not a real native client or provider.
            for directory in [world.root, *(p for p in world.root.rglob('*') if p.is_dir())]:
                if directory.stat().st_gid != os.getegid():
                    os.chown(directory, -1, os.getegid())
            lifecycle = test_omp_boundary.NativeLifecyclePublic()
            channel, pack, receipt = lifecycle.managed(world)
            command = Path(receipt['bin_dir']) / 'am'
            repo, bank, native = recovery.memory(world)
            (bank / 'handoff-prompt.md').write_text(recovery.policy_body(0))
            (repo / 'synthetic-policy.md').write_text(recovery.policy_body(0))
            world.git('add', 'synthetic-policy.md', cwd=repo)
            world.git('commit', '-m', 'Synthetic baseline', cwd=repo)
            (repo / 'synthetic-policy.md').write_text(recovery.policy_body(0) + '\nStaged disposable change.\n')
            world.git('add', 'synthetic-policy.md', cwd=repo)
            entry = {'type': 'message', 'id': 'synthetic-human', 'parentId': None,
                     'message': {'role': 'user', 'attribution': 'user',
                                 'content': 'Disposable synthetic carrier observation, no native client or provider.'}}
            request = {'schema': 'agent-memory/omp-observer-request-v1', 'action': 'seal',
                       'session': str(uuid.uuid4()), 'lifecycle': str(uuid.uuid4()),
                       'payload': {'entries': [entry], 'branch_ids': ['synthetic-human'], 'event_ordinal': 1,
                                   'terminal': False, 'trigger': 'human', 'inherited_ids': [], 'metadata': [],
                                   'request_wake': False}}
            observed = subprocess.run([str(command), 'omp-boundary', '--repo', str(repo), '--observer', '--stdin',
                '--artifact', receipt['source']['artifact_digest'], '--omp-agent-dir', receipt['omp_binding']['agent_dir']],
                input=json.dumps(request), cwd=repo, env=world.env, text=True, capture_output=True, timeout=60)
            self.assertEqual(observed.returncode, 0, observed.stdout + observed.stderr)
            pending = json.loads(world.run([command, 'omp-boundary', '--repo', repo, '--list-pending',
                                           '--offset', '0'], timeout=60).stdout)['pending']
            self.assertEqual(len(pending), 2)
            _, inspected = recovery.inspect(world, command, repo)
            layout = recovery.review_request(world, command, pack, repo, inspected, native)
            attestation = layout['quiescence_attestation']
            agent_dir = Path(receipt['omp_binding']['agent_dir'])
            attestation['declared_invocation_routes'] += [str(command.parent / 'agent-memory'),
                receipt['omp_binding']['extension_target'], str(agent_dir / 'skills/agent-memory'),
                str(agent_dir / 'AGENTS.md')]
            proposal = world.inside('omp-first-layout.json')
            recovery.call(world, command, 'prepare', '--repo', repo, '--operation', inspected['operation_id'],
                          '--input', recovery.save(world, layout), '--output', proposal)
            applied = recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertFalse(applied['semantic_recovery_complete'])
            check = lifecycle.public(world, receipt, 'update', '--check')
            self.assertEqual(check['private_compatibility']['required_capabilities'],
                             ['layout-frontier-v1', 'omp-harness-v1'])
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            old = v014_runtime_fixture.extract(world.inside('actual-published-v014'))
            shutil.rmtree(pack)
            shutil.copytree(old, pack)
            world.git('add', '-A', cwd=channel)
            world.git('commit', '-m', 'Published legacy candidate', cwd=channel)
            def preserved():
                state = snapshot(world.paths['state'])
                state.pop('agent-memory/install/owner.json', None)
                return (snapshot(bank), snapshot(directory), state, snapshot(world.paths['config']),
                        owned(receipt), snapshot(agent_dir), (repo / '.git/index').read_bytes(),
                        world.git('rev-parse', 'HEAD', cwd=repo).stdout,
                        json.loads(world.run([command, 'omp-boundary', '--repo', repo, '--list-pending',
                                              '--offset', '0'], timeout=60).stdout)['pending'])
            before = preserved()
            for args in (('update', '--check'), ('update',)):
                denied = lifecycle.public(world, receipt, *args, expected=1)
                self.assertIn('Candidate would remove the owned lifecycle command surface', denied['error'])
                self.assertEqual(preserved(), before)
            # A genuinely new capable managed artifact needs a newly reviewed
            # attestation for its own digest and all receipt-owned routes.
            private_before = snapshot(directory)
            bank_before, config_before = snapshot(bank), snapshot(world.paths['config'])
            index_before = (repo / '.git/index').read_bytes()
            head_before = world.git('rev-parse', 'HEAD', cwd=repo).stdout
            shutil.rmtree(pack)
            shutil.copytree(PACK, pack)
            advance(world, channel, pack, '1.0.0')
            renewed = recovery.review_request(world, command, pack, repo, inspected, native)['quiescence_attestation']
            renewed['declared_invocation_routes'] = sorted(set(renewed['declared_invocation_routes']
                + [target['path'] for target in receipt['targets']]))
            envelope = {'attestation': renewed, 'review': {
                'schema': 'agent-memory/layout-quiescence-review-v1', 'review_id': renewed['review_id'],
                'operation_id': renewed['operation_id'], 'attestation_sha256': recovery.sha(recovery.encoded(renewed)),
                'reviewer_developer_id': renewed['operator_developer_id'], 'reviewer_harness': 'fixture-independent-reviewer',
                'reviewer_session': 'managed-' + str(uuid.uuid4()), 'review_time': renewed['issued_at'],
                'decision': 'approve', 'rationale': 'Disposable independent review of exact bank, artifact and invocation routes.'}}
            renewal = recovery.save(world, envelope)
            checked = lifecycle.public(world, receipt, 'update', '--layout-quiescence', str(renewal), '--check')
            self.assertEqual(checked['private_compatibility']['required_capabilities'],
                             ['layout-frontier-v1', 'omp-harness-v1'])
            changed = lifecycle.public(world, receipt, 'update', '--layout-quiescence', str(renewal))
            self.assertTrue(changed['verified'])
            self.assertTrue(changed['changed'])
            receipt = changed['receipt']
            self.assertEqual(snapshot(bank), bank_before)
            self.assertEqual(snapshot(world.paths['config']), config_before)
            self.assertEqual((repo / '.git/index').read_bytes(), index_before)
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, head_before)
            private_after = snapshot(directory)
            self.assertTrue(all(private_after.get(path) == value for path, value in private_before.items()))
            self.assertEqual(json.loads(world.run([command, 'omp-boundary', '--repo', repo, '--list-pending',
                '--offset', '0'], timeout=60).stdout)['pending'], pending)
            shutil.rmtree(pack)
            shutil.copytree(old, pack)
            world.git('add', '-A', cwd=channel)
            world.git('commit', '-m', 'Published legacy downgrade', cwd=channel)
            before = preserved()
            for args in (('update', '--check'), ('update',)):
                denied = lifecycle.public(world, receipt, *args, expected=1)
                self.assertIn('Runtime downgrade refused', denied['error'])
                self.assertEqual(preserved(), before)

    def test_completed_omp_drains_before_layout_then_old_reader_after_both(self):
        with World() as world, World() as legacy:
            for directory in [world.root, *(p for p in world.root.rglob('*') if p.is_dir())]:
                if directory.stat().st_gid != os.getegid():
                    os.chown(directory, -1, os.getegid())
            lifecycle = test_omp_boundary.NativeLifecyclePublic()
            _, pack, receipt = lifecycle.managed(world)
            command = Path(receipt['bin_dir']) / 'am'
            repo, bank, native = recovery.memory(world)
            (bank / 'handoff-prompt.md').write_text(recovery.policy_body(0))
            (repo / 'synthetic-policy.md').write_text(recovery.policy_body(0))
            world.git('add', 'synthetic-policy.md', cwd=repo)
            world.git('commit', '-m', 'Synthetic baseline', cwd=repo)
            entry = {'type': 'message', 'id': 'synthetic-human', 'parentId': None,
                     'message': {'role': 'user', 'attribution': 'user',
                                 'content': 'Disposable synthetic carrier observation, no native client or provider.'}}
            request = {'schema': 'agent-memory/omp-observer-request-v1', 'action': 'seal',
                       'session': str(uuid.uuid4()), 'lifecycle': str(uuid.uuid4()),
                       'payload': {'entries': [entry], 'branch_ids': ['synthetic-human'], 'event_ordinal': 1,
                                   'terminal': False, 'trigger': 'human', 'inherited_ids': [], 'metadata': [],
                                   'request_wake': False}}
            def send(args, value):
                return subprocess.run([str(command), *map(str, args)], input=json.dumps(value),
                                      cwd=repo, env=world.env, text=True, capture_output=True, timeout=60)
            observed = send(['omp-boundary', '--repo', repo, '--observer', '--stdin',
                             '--artifact', receipt['source']['artifact_digest'],
                             '--omp-agent-dir', receipt['omp_binding']['agent_dir']], request)
            self.assertEqual(observed.returncode, 0, observed.stdout + observed.stderr)
            def pending():
                return json.loads(world.run([command, 'omp-boundary', '--repo', repo, '--list-pending',
                                             '--offset', '0'], timeout=60).stdout)['pending']
            rows = pending()
            self.assertEqual(len(rows), 2)
            for row in rows:
                source = json.loads(world.run([command, 'omp-boundary', '--repo', repo, '--source',
                    row['observation_id'], '--session', row['session'], '--lifecycle', row['lifecycle']],
                    timeout=60).stdout)
                self.assertTrue(source['coverage_complete'])
                self.assertEqual(source['faults'], [])
                if 'content' in source['source']:
                    self.assertEqual(source['source']['content'], entry['message']['content'])
                    review = 'Reviewed the exact disposable carrier message; it contains no project learning.'
                else:
                    self.assertEqual(source['source']['gaps'], [])
                    self.assertEqual(len(source['source']['captures']), 1)
                    review = 'Reviewed the sealed synthetic manifest and its sole inspected capture; no project learning.'
                value = {key: row[key] for key in ('session', 'lifecycle', 'observation_id', 'source_sha256')}
                value.update(outcome='no-learning', assessment={'source_id': row['observation_id'],
                             'source_review': review, 'candidates': []})
                done = send(['omp-boundary', '--repo', repo, '--stdin'], value)
                self.assertEqual(done.returncode, 0, done.stdout + done.stderr)
                self.assertTrue(json.loads(done.stdout)['verified'])
            self.assertEqual(pending(), [])
            _, inspected = recovery.inspect(world, command, repo)
            layout = recovery.review_request(world, command, pack, repo, inspected, native)
            attestation = layout['quiescence_attestation']
            agent_dir = Path(receipt['omp_binding']['agent_dir'])
            attestation['declared_invocation_routes'] += [str(command.parent / 'agent-memory'),
                receipt['omp_binding']['extension_target'], str(agent_dir / 'skills/agent-memory'),
                str(agent_dir / 'AGENTS.md')]
            proposal = world.inside('completed-omp-layout.json')
            recovery.call(world, command, 'prepare', '--repo', repo, '--operation', inspected['operation_id'],
                          '--input', recovery.save(world, layout), '--output', proposal)
            applied = recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertFalse(applied['semantic_recovery_complete'])
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            base = directory.parent.parent
            marker = base / 'turn-sources-compat-v1.json'
            self.assertTrue(marker.is_file())
            originals = {row['source_id']: (directory / row['blob']).read_bytes()
                         for row in inspected['manifest']['sources']}
            check = lifecycle.public(world, receipt, 'update', '--check')
            self.assertEqual(check['private_compatibility']['required_capabilities'],
                             ['layout-frontier-v1', 'omp-harness-v1'])
            before = (snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                      (repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout)
            denied = lifecycle.public(world, receipt, 'uninstall', '--omp-writers-quiesced', expected=1)
            self.assertIn('Private-state runtime compatibility', denied['error'])
            self.assertFalse(marker.exists())
            self.assertTrue(list((base / 'omp-drains').glob('*.json')))
            self.assertEqual((snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                              (repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout), before)
            self.assertFalse(Path(receipt['omp_binding']['extension_target']).exists())
            recovered = json.loads(world.run([command, '--json', 'recover'], timeout=60).stdout)
            self.assertTrue(recovered['verified'])
            lifecycle.extension(receipt)
            checked = lifecycle.public(world, receipt, 'update', '--check')
            self.assertEqual(checked['private_compatibility']['required_capabilities'], ['layout-frontier-v1'])
            old = v014_runtime_fixture.extract(legacy.root)
            before_candidate = (snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                                owned(receipt), (repo / '.git/index').read_bytes(),
                                world.git('rev-parse', 'HEAD', cwd=repo).stdout)
            refused_old = lifecycle.public(world, receipt, 'install', '--development-source', str(old),
                                           '--presets', 'none', expected=1)
            self.assertIn('Removing saved omp selection', refused_old['error'])
            self.assertEqual((snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                              owned(receipt), (repo / '.git/index').read_bytes(),
                              world.git('rev-parse', 'HEAD', cwd=repo).stdout), before_candidate)
            self.assertFalse(marker.exists())
            admission, transaction, _ = recovery.admission_proposal(world, command, pack, repo, bank,
                                                                    inspected, full=True)
            result = recovery.public(world, command, 'admission', 'apply', '--proposal', admission)
            self.assertEqual(result['operation_id'], transaction['operation_id'])
            self.assertTrue(result['admission_verified'])
            self.assertTrue(recovery.call(world, command, 'status', '--repo', repo,
                                          '--operation', inspected['operation_id'])['semantic_recovery_complete'])
            self.assertEqual({row['source_id']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            self.assertTrue(lifecycle.public(world, receipt, 'uninstall', '--omp-writers-quiesced')['verified'])
            # Install the byte-verified old archive separately; retained v2
            # ownership is not silently handed to an unaware installer.
            installed = legacy.run([sys.executable, '-I', '-B', old / 'scripts/installer.py', 'install',
                                    '--development-source', old, '--presets', 'none'], timeout=60)
            old_receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(old_receipt['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old_command = Path(old_receipt['bin_dir']) / 'am'
            before_read = (snapshot(bank), snapshot(world.paths['state']), snapshot(world.paths['config']),
                           (repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout)
            read = world.run([old_command, '--json', 'bank', 'validate', '--repo', repo], timeout=60)
            self.assertFalse(json.loads(read.stdout)['writes_performed'])
            self.assertEqual((snapshot(bank), snapshot(world.paths['state']), snapshot(world.paths['config']),
                              (repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout), before_read)
