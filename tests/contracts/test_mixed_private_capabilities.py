"""Disposable public lifecycle boundaries; synthetic sources are not native-client proof."""
import copy
import json
import os
import shutil
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch
import uuid

from world import World
import layout_recovery_fixtures as recovery
import v014_runtime_fixture
from runtime_update_fixtures import PACK, advance, owned, receipt_path, snapshot
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
                         for row in inspected['manifest']['sources']},
                        snapshot(world.paths['state']))
            before = preserved()
            registry_key = 'agent-memory/omp-banks.json'
            self.assertNotIn(registry_key, before[2])
            target_before = snapshot(root)
            variants = []
            wrong = copy.deepcopy(attestation)
            wrong['declared_invocation_routes'].remove(str(root / 'extensions/agent-memory.ts'))
            variants.append(envelope(wrong))
            wrong = copy.deepcopy(attestation)
            wrong['declared_invocation_routes'].remove(str(root / 'skills/agent-memory'))
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
                self.assertEqual(snapshot(root), target_before)
            proof = recovery.save(world, envelope(attestation))
            preview = json.loads(world.run([*args, '--check', '--omp-layout-quiescence', proof], timeout=60).stdout)
            self.assertEqual(preview['private_compatibility']['required_capabilities'], ['layout-frontier-v1'])
            self.assertEqual(preserved(), before)
            self.assertEqual(snapshot(root), target_before)
            result = json.loads(world.run([*args, '--omp-layout-quiescence', proof], timeout=60).stdout)
            self.assertTrue(result['verified'])
            test_omp_boundary.NativeLifecyclePublic().extension(result['receipt'])
            self.assertEqual(snapshot(bank), before[0])
            config_after = snapshot(world.paths['config'])
            self.assertEqual(set(config_after) - set(before[2]), {registry_key})
            self.assertEqual({key: value for key, value in config_after.items()
                              if key != registry_key}, before[2])
            base = directory.parent.parent
            registry = json.loads((world.paths['config'] / registry_key).read_bytes())
            self.assertEqual(registry['schema'], 'agent-memory/omp-bank-registry-v1')
            self.assertIn(str(repo.resolve()), registry['repositories'])
            self.assertEqual(registry['private_bases'][str(repo.resolve())], str(base))
            binding = json.loads((base / 'omp-bank-binding-v1.json').read_bytes())
            self.assertEqual(binding['repository'], str(repo.resolve()))
            self.assertEqual(binding['bank_path'], str(bank.resolve()))
            self.assertEqual(binding['private_base'], str(base))
            self.assertEqual(binding['project_id'], inspected['manifest']['project_id'])
            self.assertEqual((repo / '.git/index').read_bytes(), before[3])
            self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, before[4])
            self.assertEqual({row['source_id']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            entry = {'type': 'message', 'id': 'synthetic-human', 'parentId': None,
                     'message': {'role': 'user', 'attribution': 'user',
                                 'content': 'Disposable synthetic carrier observation, no native client or provider.'}}
            session = str(uuid.uuid4())
            source_view = test_omp_boundary.observer_payload(repo, session, [entry], trigger='human')
            source_view['request_wake'] = False
            payload = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal',
                       'session': session, 'lifecycle': str(uuid.uuid4()), 'payload': source_view}
            def input_command(args, value):
                return subprocess.run([str(command), *map(str, args)], cwd=repo, env=world.env,
                                      input=json.dumps(value), text=True, capture_output=True, timeout=60)
            observed = test_omp_boundary.observe_installed(world, command, repo, result['receipt'], payload)
            self.assertEqual(observed.returncode, 0, observed.stdout + observed.stderr)
            self.assertTrue(json.loads(observed.stdout)['held'])
            def pending():
                response = world.run([command, 'omp-boundary', '--repo', repo, '--list-pending', '--offset', '0'],
                                     timeout=60)
                return json.loads(response.stdout)['pending']
            rows = pending()
            self.assertEqual(len(rows), 2)
            # Native writer quiescence does not authorize removing the sole
            # capable reader while the independent layout hold is active.
            held_before = preserved(), snapshot(world.paths['state']), snapshot(root)
            refused = world.run([command, '--json', 'uninstall', '--omp-writers-quiesced'],
                                check=False, timeout=60)
            self.assertNotEqual(refused.returncode, 0, refused.stdout + refused.stderr)
            self.assertFalse(json.loads(refused.stdout)['verified'])
            self.assertEqual((preserved(), snapshot(world.paths['state']), snapshot(root)), held_before)
            checked = json.loads(world.run([*args, '--check'], timeout=60).stdout)
            self.assertEqual(checked['private_compatibility']['required_capabilities'],
                             ['layout-frontier-v1', 'omp-harness-v1', 'omp-stored-source-v2'])
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
            # Enroll the newly discovered bank through the committed native
            # lifecycle before introducing the invalid recovery source.
            self.assertTrue(lifecycle.public(world, receipt, 'recover')['verified'])
            (bank / 'handoff-prompt.md').write_text(recovery.policy_body(0))
            (repo / 'synthetic-policy.md').write_text(recovery.policy_body(0))
            world.git('add', 'synthetic-policy.md', cwd=repo)
            world.git('commit', '-m', 'Synthetic baseline', cwd=repo)
            (repo / 'synthetic-policy.md').write_text(recovery.policy_body(0) + '\nStaged disposable change.\n')
            world.git('add', 'synthetic-policy.md', cwd=repo)
            entry = {'type': 'message', 'id': 'synthetic-human', 'parentId': None,
                     'message': {'role': 'user', 'attribution': 'user',
                                 'content': 'Disposable synthetic carrier observation, no native client or provider.'}}
            session = str(uuid.uuid4())
            source_view = test_omp_boundary.observer_payload(repo, session, [entry], trigger='human')
            source_view['request_wake'] = False
            request = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal',
                       'session': session, 'lifecycle': str(uuid.uuid4()), 'payload': source_view}
            observed = test_omp_boundary.observe_installed(world, command, repo, receipt, request)
            self.assertEqual(observed.returncode, 0, observed.stdout + observed.stderr)
            pending = json.loads(world.run([command, 'omp-boundary', '--repo', repo, '--list-pending',
                                           '--offset', '0'], timeout=60).stdout)['pending']
            self.assertEqual(len(pending), 2)
            _, inspected = recovery.inspect(world, command, repo)
            layout = recovery.review_request(world, command, Path(receipt['source']['pack']), repo, inspected, native)
            attestation = layout['quiescence_attestation']
            agent_dir = Path(receipt['omp_binding']['agent_dir'])
            attestation['declared_invocation_routes'] += [str(command.parent / 'agent-memory'),
                receipt['omp_binding']['extension_target'], str(agent_dir / 'skills/agent-memory'),
                str(agent_dir / 'AGENTS.md')]
            layout['review']['quiescence_attestation_sha256'] = recovery.sha(recovery.encoded(attestation))
            proposal = world.inside('omp-first-layout.json')
            recovery.call(world, command, 'prepare', '--repo', repo, '--operation', inspected['operation_id'],
                          '--input', recovery.save(world, layout), '--output', proposal)
            applied = recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertFalse(applied['semantic_recovery_complete'])
            check = lifecycle.public(world, receipt, 'update', '--check')
            self.assertEqual(check['private_compatibility']['required_capabilities'],
                             ['layout-frontier-v1', 'omp-harness-v1', 'omp-stored-source-v2'])
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
                lifecycle.public(world, receipt, *args, expected=1)
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
            candidate_source = world.inside('measured update candidate')
            world.git('clone', '--', channel, candidate_source)
            renewed = recovery.review_request(world, command, candidate_source / 'skills/agent-memory',
                                              repo, inspected, native)['quiescence_attestation']
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
                             ['layout-frontier-v1', 'omp-harness-v1', 'omp-stored-source-v2'])
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
            self.assertTrue(lifecycle.public(world, receipt, 'recover')['verified'])
            (bank / 'handoff-prompt.md').write_text(recovery.policy_body(0))
            (repo / 'synthetic-policy.md').write_text(recovery.policy_body(0))
            world.git('add', 'synthetic-policy.md', cwd=repo)
            world.git('commit', '-m', 'Synthetic baseline', cwd=repo)
            entry = {'type': 'message', 'id': 'synthetic-human', 'parentId': None,
                     'message': {'role': 'user', 'attribution': 'user',
                                 'content': 'Disposable synthetic carrier observation, no native client or provider.'}}
            session = str(uuid.uuid4())
            source_view = test_omp_boundary.observer_payload(repo, session, [entry], trigger='human')
            source_view['request_wake'] = False
            request = {'schema': 'agent-memory/omp-observer-request-v2', 'action': 'seal',
                       'session': session, 'lifecycle': str(uuid.uuid4()), 'payload': source_view}
            def send(args, value):
                return subprocess.run([str(command), *map(str, args)], input=json.dumps(value),
                                      cwd=repo, env=world.env, text=True, capture_output=True, timeout=60)
            observed = test_omp_boundary.observe_installed(world, command, repo, receipt, request)
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
            layout = recovery.review_request(world, command, Path(receipt['source']['pack']), repo, inspected, native)
            attestation = layout['quiescence_attestation']
            agent_dir = Path(receipt['omp_binding']['agent_dir'])
            attestation['declared_invocation_routes'] += [str(command.parent / 'agent-memory'),
                receipt['omp_binding']['extension_target'], str(agent_dir / 'skills/agent-memory'),
                str(agent_dir / 'AGENTS.md')]
            layout['review']['quiescence_attestation_sha256'] = recovery.sha(recovery.encoded(attestation))
            proposal = world.inside('completed-omp-layout.json')
            recovery.call(world, command, 'prepare', '--repo', repo, '--operation', inspected['operation_id'],
                          '--input', recovery.save(world, layout), '--output', proposal)
            applied = recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertFalse(applied['semantic_recovery_complete'])
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            base = directory.parent.parent
            marker = base / 'turn-sources-compat-v1.json'
            self.assertTrue(marker.is_file())
            retained_native_sources = snapshot(base / 'omp-source-v1')
            originals = {row['source_id']: (directory / row['blob']).read_bytes()
                         for row in inspected['manifest']['sources']}
            check = lifecycle.public(world, receipt, 'update', '--check')
            self.assertEqual(check['private_compatibility']['required_capabilities'],
                             ['layout-frontier-v1', 'omp-harness-v1', 'omp-stored-source-v2'])
            before = (snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                      (repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout,
                      snapshot(world.paths['state']), snapshot(agent_dir), snapshot(world.paths['backups']))
            denied = lifecycle.public(world, receipt, 'uninstall', '--omp-writers-quiesced', expected=1)
            self.assertIn('Private-state runtime compatibility', denied['error'])
            self.assertTrue(marker.is_file())
            self.assertEqual((snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                              (repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout,
                              snapshot(world.paths['state']), snapshot(agent_dir), snapshot(world.paths['backups'])), before)
            lifecycle.extension(receipt)
            recovered = json.loads(world.run([command, '--json', 'recover'], timeout=60).stdout)
            self.assertTrue(recovered['verified'])
            lifecycle.extension(receipt)
            checked = lifecycle.public(world, receipt, 'update', '--check')
            # Refusal did not disable the native route or drain its marker;
            # recovery leaves the capable reader and unrelated hold intact.
            self.assertTrue(marker.is_file())
            self.assertEqual(pending(), [])
            self.assertEqual(checked['private_compatibility']['required_capabilities'],
                             ['layout-frontier-v1', 'omp-harness-v1', 'omp-stored-source-v2'])
            self.assertEqual(snapshot(directory), before[1])
            self.assertTrue(checked['check_complete'])
            self.assertTrue(recovered['stored_source_activation']['verified'])
            restored_marker = marker.read_bytes()
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
            self.assertEqual(marker.read_bytes(), restored_marker)
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
            self.assertFalse(marker.exists())
            self.assertTrue(list((base / 'omp-drains').glob('*.json')))
            self.assertEqual(snapshot(base / 'omp-source-v1'), retained_native_sources)
            with patch.dict(os.environ, world.env, clear=True):
                self.assertEqual(test_omp_boundary.transport.list_pending(repo)['pending'], [])
                self.assertEqual(test_omp_boundary.boundary.required_source_capabilities(repo), set())
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

    def test_private_lifecycle_observation_refuses_config_bank_hold_and_attestation_races(self):
        with World() as world:
            command, source, repo, bank, _, inspected, _ = recovery.small_pending(world)
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            receipt = json.loads(receipt_path(world).read_text())
            feature = repo / 'feature.txt'
            feature.write_text('Disposable independent feature baseline.\n')
            world.git('add', 'feature.txt', cwd=repo)
            world.git('commit', '-m', 'Independent feature baseline', cwd=repo)
            feature.write_text('Disposable staged feature change.\n')
            world.git('add', 'feature.txt', cwd=repo)
            before = (snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                      owned(receipt), feature.read_bytes(), (repo / '.git/index').read_bytes(),
                      world.git('rev-parse', 'HEAD', cwd=repo).stdout)
            program = r'''
import json, sys
from pathlib import Path
from unittest.mock import patch
sys.path.insert(0, sys.argv.pop(1))
from agent_memory import artifact, private_state, recovery_frontier as frontier, recovery_quiescence as quiescence
from agent_memory.frontmatter import ContractError
repo, bank, directory, source = map(Path, sys.argv[1:])
installation = {'observed_repositories': [str(repo)]}
results = {}
for kind in ('config', 'bank', 'hold', 'attestation'):
    target = artifact.inspect(source)
    expected = private_state.require_compatible(installation, target, 'update')
    original = quiescence._selected_attestation
    original_hold = frontier.verified_hold
    prior_hold_verified = False
    mutation_after_verified_hold = False
    def verified_hold(*args, **kwargs):
        global prior_hold_verified
        held = original_hold(*args, **kwargs)
        prior_hold_verified = held is not None
        return held
    saved = None
    displaced = bank.with_name('displaced-bank')
    if kind == 'config':
        path = repo / '.git/config'
    elif kind == 'hold':
        path = bank / '.agent-memory-transaction.md'
    elif kind == 'attestation':
        path = next((directory / 'proposals').glob('*.json'))
    if kind != 'bank':
        saved = path.read_bytes()
    injected = False
    def change_after_read(*args):
        global injected, mutation_after_verified_hold
        caller = sys._getframe(1)
        selected = original(*args)
        # Synchronize after the lifecycle consumer's own final attestation read,
        # not the earlier independently checked hold's attestation read.
        if not injected and caller.f_globals.get('__name__') == private_state.__name__:
            mutation_after_verified_hold = prior_hold_verified
            injected = True
            if kind == 'bank':
                bank.rename(displaced)
            elif kind == 'config':
                path.write_bytes(saved + b'\n[agent-memory]\n\tbankRoot = foreign-bank\n')
            elif kind == 'hold':
                path.write_bytes(b'Changed hold during the actual read-only observation.\n')
            else:
                value = json.loads(saved)
                value['quiescence_attestation']['declared_invocation_routes'].append('/changed-route')
                path.write_text(json.dumps(value))
        return selected
    refused = False
    try:
        with patch.object(frontier, 'verified_hold', new=verified_hold), \
                patch.object(quiescence, '_selected_attestation', new=change_after_read):
            private_state.require_compatible(installation, target, 'update')
    except ContractError:
        refused = True
    finally:
        if kind == 'bank' and displaced.exists():
            displaced.rename(bank)
        elif kind != 'bank':
            path.write_bytes(saved)
    private_state.require_compatible(installation, target, 'update', expected=expected)
    results[kind] = {'injected': injected, 'refused': refused,
                     'mutation_after_verified_hold': mutation_after_verified_hold}
print(json.dumps(results))
'''
            result = world.run([sys.executable, '-I', '-B', '-c', program,
                                source / 'scripts', repo, bank, directory, source], timeout=60)
            self.assertEqual(json.loads(result.stdout), {
                kind: {'injected': True, 'refused': True, 'mutation_after_verified_hold': True}
                for kind in ('config', 'bank', 'hold', 'attestation')})
            self.assertEqual((snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                              owned(receipt), feature.read_bytes(), (repo / '.git/index').read_bytes(),
                              world.git('rev-parse', 'HEAD', cwd=repo).stdout), before)

    def test_managed_update_accepts_repository_under_symlinked_ancestor(self):
        with World() as world:
            for directory in [world.root, *(p for p in world.root.rglob('*') if p.is_dir())]:
                if directory.stat().st_gid != os.getegid():
                    os.chown(directory, -1, os.getegid())
            lifecycle = test_omp_boundary.NativeLifecyclePublic()
            _, _, receipt = lifecycle.managed(world)
            repo, bank, _ = recovery.memory(world)
            self.assertTrue(lifecycle.public(world, receipt, 'recover')['verified'])
            feature = repo / 'feature.txt'
            feature.write_text('Disposable independent feature baseline.\n')
            world.git('add', 'feature.txt', cwd=repo)
            world.git('commit', '-m', 'Independent feature baseline', cwd=repo)
            feature.write_text('Disposable staged feature change.\n')
            world.git('add', 'feature.txt', cwd=repo)
            ancestor = world.root / 'symlinked-ancestor'
            ancestor.symlink_to(repo.parent, target_is_directory=True)
            selected = ancestor / repo.name
            registry_key = 'agent-memory/omp-banks.json'
            registry_path = world.paths['config'] / registry_key
            registry_before = json.loads(registry_path.read_text()) if registry_path.exists() else None
            before = (snapshot(bank), snapshot(world.paths['config']), owned(receipt),
                      feature.read_bytes(), (repo / '.git/index').read_bytes(),
                      world.git('rev-parse', 'HEAD', cwd=repo).stdout)
            checked = lifecycle.public(world, receipt, 'update', '--repo', str(selected), '--check')
            self.assertTrue(checked['check_complete'])
            self.assertEqual(snapshot(world.paths['config']), before[1])
            self.assertEqual(checked['compatibility']['private_state']['decision'], 'compatible')
            applied = lifecycle.public(world, receipt, 'update', '--repo', str(selected))
            self.assertTrue(applied['verified'])
            config_before = before[1].copy()
            config_after = snapshot(world.paths['config'])
            registry = json.loads(registry_path.read_text())
            canonical = str(repo.resolve())
            previous_repositories = registry_before['repositories'] if registry_before else []
            previous_bases = registry_before['private_bases'] if registry_before else {}
            self.assertEqual(registry['schema'], 'agent-memory/omp-bank-registry-v1')
            self.assertEqual(set(registry['repositories']), set(previous_repositories) | {canonical})
            self.assertEqual({key: value for key, value in registry['private_bases'].items()
                              if key != canonical},
                             {key: value for key, value in previous_bases.items() if key != canonical})
            self.assertIn(canonical, registry['private_bases'])
            if canonical in previous_bases:
                self.assertEqual(registry['private_bases'][canonical], previous_bases[canonical])
            binding = json.loads((Path(registry['private_bases'][canonical]) /
                                  'omp-bank-binding-v1.json').read_text())
            self.assertEqual(binding['repository'], canonical)
            self.assertEqual(binding['bank_path'], str(bank.resolve()))
            self.assertEqual(binding['private_base'], registry['private_bases'][canonical])
            config_before.pop(registry_key, None)
            config_after.pop(registry_key)
            self.assertEqual((snapshot(bank), config_after, owned(receipt),
                              feature.read_bytes(), (repo / '.git/index').read_bytes(),
                              world.git('rev-parse', 'HEAD', cwd=repo).stdout),
                             (before[0], config_before, *before[2:]))
