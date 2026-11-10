"""Public recovery across journaled metadata-probe interruptions."""
import json
import os
from pathlib import Path, PurePosixPath
import sys
import stat
import unittest

from world import World
from runtime_update_fixtures import snapshot
from contracts import test_layout_recovery as existing
from contracts import test_layout_recovery_darwin_provenance as exact_metadata
import layout_recovery_fixtures as recovery
import legacy_recovery_fixtures as legacy_guard
import v014_runtime_fixture


def attributes(path):
    return {name: bytes.fromhex(value) for name, value in exact_metadata.xattrs(path).items()}


class LayoutMetadataProbeFaults(unittest.TestCase):
    def prepare_fixture(self, world):
        helper = existing.LayoutRecoveryContract()
        command, source, repo, bank, native, originals, modes, git = helper.setup_fault_repair(world)
        original_attrs = {path: attributes(bank / path) for path in originals}
        safe, inspected = recovery.inspect(world, command, repo)
        request = recovery.review_request(world, command, source, repo, inspected, native)
        old = next(path for path in native if path.startswith('topics/'))
        new = 'topics/probe-nested/' + old.rsplit('/', 1)[1]
        intended = dict(native)
        intended[new] = intended.pop(old)
        intended['MEMORY.md'] = intended['MEMORY.md'].replace(old, new)
        for unit in request['unit_map']['units']:
            for target in unit['targets']:
                if target['path'] == old:
                    target['path'] = new
                if target['path'] in (new, 'MEMORY.md'):
                    target['sha256'] = recovery.sha(intended[target['path']].encode())
        plan = request['metadata_plan']
        plan['files'][new] = plan['files'].pop(old)
        parent = bank / 'topics'
        facts = parent.stat()
        plan['directories']['topics/probe-nested'] = {
            'uid': facts.st_uid, 'gid': facts.st_gid, 'mode': stat.S_IMODE(facts.st_mode),
            'xattrs': {name: value.hex() for name, value in attributes(parent).items()}}
        request['intended_files'] = intended
        request['review']['unit_map_sha256'] = recovery.sha(recovery.encoded(request['unit_map']))
        request['review']['intended_files_sha256'] = recovery.sha(recovery.encoded(intended))
        request['review']['metadata_plan_sha256'] = recovery.sha(recovery.encoded(plan))
        output = world.inside('probe-reviewed-proposal.json')
        recovery.call(world, command, 'prepare', '--repo', repo, '--operation', safe['operation_id'],
                      '--input', recovery.save(world, request), '--output', output)
        prepared = json.loads(output.read_text())['proposal']
        directory = recovery.private_directory(world, bank, safe['operation_id'])
        return (helper, command, repo, bank, intended, originals, modes, git, original_attrs,
                safe['operation_id'], inspected, output, prepared, directory, old, new)

    def finish(self, world, fixture, *, probes=(), partial=False):
        (helper, command, repo, bank, intended, originals, modes, git, original_attrs,
         operation, inspected, proposal, prepared, directory, old, new) = fixture
        if not partial:
            self.assertFalse((bank / '.agent-memory-transaction.md').exists())
            self.assertFalse((bank / '.agent-memory-recovery.md').exists())
            for path, body in originals.items():
                self.assertEqual((bank / path).read_bytes(), body)
                self.assertEqual(stat.S_IMODE((bank / path).stat().st_mode), modes[path])
                self.assertEqual(attributes(bank / path), original_attrs[path])
        helper.assert_fault_work_unchanged(world, repo, git)
        preview = world.inside('probe-final-finish.json')
        recovery.call(world, command, 'recover', '--repo', repo, '--operation', operation,
                      '--mode', 'finish', '--output', preview)
        outcome = recovery.call(world, command, 'recover', '--repo', repo, '--proposal', preview)
        self.assertTrue(outcome['verified'])
        self.assertFalse(outcome['semantic_recovery_complete'])
        self.assertEqual({p.relative_to(bank).as_posix(): p.read_bytes() for p in bank.rglob('*')
                          if p.is_file() and p.name not in ('.agent-memory-transaction.md', '.agent-memory-recovery.md')},
                         {path: body.encode() for path, body in intended.items()})
        self.assertTrue(all(not (bank / path).exists() for path in probes))
        new_parent = bank / 'topics/probe-nested'
        expected_parent = prepared['metadata_plan']['directories']['topics/probe-nested']
        parent_info = new_parent.stat()
        self.assertEqual((parent_info.st_uid, parent_info.st_gid, stat.S_IMODE(parent_info.st_mode)),
                         (expected_parent['uid'], expected_parent['gid'], expected_parent['mode']))
        self.assertEqual({name: value.hex() for name, value in attributes(new_parent).items()},
                         expected_parent['xattrs'])
        for path in intended:
            facts = (bank / path).stat()
            expected = prepared['metadata_plan']['files'][path]
            self.assertEqual((facts.st_uid, facts.st_gid, stat.S_IMODE(facts.st_mode)),
                             (expected['uid'], expected['gid'], expected['mode']))
            self.assertEqual({name: value.hex() for name, value in attributes(bank / path).items()},
                             expected['xattrs'])
            source = old if path == new else path
            if source in original_attrs:
                self.assertEqual(attributes(bank / path), original_attrs[source])
        self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                          for row in inspected['manifest']['sources']}, originals)
        self.assertTrue(recovery.call(world, command, 'recover', '--repo', repo,
                                      '--proposal', preview)['receipt_is_historical'])
        helper.assert_fault_work_unchanged(world, repo, git)

    def journey(self, stage, mode):
        with World() as world:
            fixture = self.prepare_fixture(world)
            _, command, repo, bank, _, _, _, _, _, operation, _, proposal, _, directory, _, _ = fixture
            fault = world.run([command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal],
                              check=False, timeout=60,
                              env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            self.assertTrue((directory / 'journal.json').is_file())
            if stage.endswith('probe-directories-removed'):
                self.assertFalse((bank / 'topics/probe-nested').exists())
            self.finish(world, fixture)

    def probe_plan(self, world, fixture):
        _, command, repo, bank, _, _, _, _, _, operation, _, proposal, _, directory, _, _ = fixture
        first = world.run([command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal],
                          check=False, timeout=60,
                          env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-probe-journal',
                               'AGENT_MEMORY_FAULT_MODE': 'error'})
        self.assertEqual(first.returncode, 1, first.stdout + first.stderr)
        plan = json.loads((directory / 'journal.json').read_bytes())['metadata_probes']
        self.assertEqual(plan['operation_id'], operation)
        self.assertIn('topics/probe-nested', plan['directories'])
        nodes, reachable = {}, set()
        for task in plan['tasks']:
            for route in task['routes']:
                self.assertTrue(route)
                route_reachable = (bank / route[0]['path']).parent.is_dir()
                for node in route:
                    path = node['path']
                    self.assertFalse(PurePosixPath(path).is_absolute())
                    self.assertNotIn('..', PurePosixPath(path).parts)
                    if path in nodes:
                        self.assertEqual(nodes[path], node)
                    nodes[path] = node
                    if route_reachable:
                        reachable.add(path)
        self.assertTrue(any(node['type'] == 'directory' for node in nodes.values()))
        self.assertTrue(any(node['type'] == 'file' for node in nodes.values()))
        self.assertTrue(set(nodes) - reachable)
        return nodes, reachable

    def assert_ambiguous_and_remove(self, world, fixture, attempt, path):
        helper, command, repo, bank, _, _, _, git, _, operation, _, _, _, directory, _, _ = fixture
        target = bank / path
        self.assertTrue(target.exists())
        flags = exact_metadata.inode_flags(target) if target.is_file() else None
        marker = bank / '.agent-memory-transaction.md'
        self.assertFalse(marker.exists())
        def metadata():
            return {str(p.relative_to(bank)): (p.lstat().st_uid, p.lstat().st_gid,
                    stat.S_IMODE(p.lstat().st_mode), attributes(p)) for p in (bank, *bank.rglob('*'))}
        before = snapshot(bank), snapshot(attempt), (directory / 'journal.json').read_bytes(), metadata()
        private_before = snapshot(directory)
        inspected_output = world.inside('ambiguous-probe-fresh-inspect.json')
        inspected = recovery.call(world, command, 'inspect', '--repo', repo,
                                  '--output', inspected_output, expected=1)
        self.assertEqual(inspected['state'], 'pending')
        self.assertFalse(inspected['verified'])
        self.assertFalse(inspected['semantic_recovery_complete'])
        self.assertIsNot(inspected['canonical_eligibility'], True)
        self.assertIsNot(inspected.get('observation_complete'), True)
        self.assertFalse(inspected['canonical_writes_performed'])
        self.assertFalse(inspected['canonical_mutation_may_have_occurred'])
        self.assertTrue(inspected['required_actions'])
        self.assertFalse(inspected_output.exists())
        self.assertNotEqual(inspected['operation_id'], operation)
        refused = recovery.private_directory(world, bank, inspected['operation_id'])
        self.assertFalse(json.loads((refused / 'staging.json').read_bytes())['observation_complete'])
        self.assertFalse((refused / 'manifest.json').exists())
        self.assertFalse((refused / 'proposal.json').exists())
        self.assertEqual((snapshot(bank), snapshot(attempt), (directory / 'journal.json').read_bytes(), metadata()), before)
        self.assertEqual(snapshot(directory), private_before)
        self.assertFalse(marker.exists())
        if flags is not None:
            self.assertEqual(exact_metadata.inode_flags(target), flags)
        helper.assert_fault_work_unchanged(world, repo, git)
        output = world.inside('ambiguous-probe-recovery.json')
        pending = world.run([command, 'bank', 'recover-layout', 'recover', '--repo', repo,
                             '--operation', operation, '--mode', 'finish', '--output', output],
                            check=False, timeout=60)
        self.assertEqual(pending.returncode, 1, pending.stdout + pending.stderr)
        result = json.loads(pending.stdout)
        self.assertEqual(result['state'], 'pending')
        self.assertFalse(result['verified'])
        self.assertEqual(result['failure_code'], 'pending-ambiguous-probe-ownership')
        self.assertEqual(result['probe_path'], path)
        self.assertTrue(result['required_actions'])
        self.assertEqual(result['writes_performed'], 'unknown')
        self.assertEqual(result['canonical_writes_performed'], 'unknown')
        self.assertTrue(result['canonical_mutation_may_have_occurred'])
        status = world.run([command, 'bank', 'recover-layout', 'status', '--repo', repo,
                            '--operation', operation], check=False, timeout=60)
        self.assertEqual(status.returncode, 1, status.stdout + status.stderr)
        status_result = json.loads(status.stdout)
        self.assertEqual(status_result['state'], 'pending')
        self.assertFalse(status_result['verified'])
        self.assertEqual(status_result['failure_code'], 'pending-ambiguous-probe-ownership')
        self.assertEqual(status_result['probe_path'], path)
        self.assertTrue(status_result['required_actions'])
        self.assertEqual(status_result['writes_performed'], 'unknown')
        self.assertEqual(status_result['canonical_writes_performed'], 'unknown')
        self.assertTrue(status_result['canonical_mutation_may_have_occurred'])
        for node in json.loads((attempt / 'plan.json').read_bytes())['nodes']:
            if node.get('nonce'):
                self.assertNotIn(node['nonce'], pending.stdout)
                self.assertNotIn(node['nonce'], status.stdout)
        self.assertFalse(output.exists())
        self.assertEqual((snapshot(bank), snapshot(attempt), (directory / 'journal.json').read_bytes(), metadata()), before)
        self.assertFalse(marker.exists())
        if flags is not None:
            self.assertEqual(exact_metadata.inode_flags(target), flags)
        helper.assert_fault_work_unchanged(world, repo, git)
        # Simulated operator inspection/removal in this disposable fixture only.
        if target.is_dir():
            target.rmdir()
        else:
            target.unlink()

    def node_label(self, node, fixture):
        old, new = fixture[-2:]
        target = node['target']
        role = 'original-record' if target == old else 'moved-record' if target == new else target
        shadow = any(part.startswith('.agent-memory-probe-')
                     for part in PurePosixPath(node['path']).parts[:-1])
        return (node['type'], role, shadow)

    def probe_case(self, kind, index, mode, *, old_runtime=None, expected_labels=None):
        with World() as world:
            fixture = self.prepare_fixture(world)
            _, command, repo, bank, _, _, _, _, _, operation, inspected, proposal, _, directory, _, _ = fixture
            nodes, reachable = self.probe_plan(world, fixture)
            template_paths = sorted(nodes, key=lambda path: (len(PurePosixPath(path).parts), path))
            candidates = [(self.node_label(nodes[path], fixture), number)
                          for number, path in enumerate(template_paths) if path in reachable and
                          (old_runtime is None or nodes[path]['type'] == 'file')]
            self.assertEqual(len({label for label, _ in candidates}), len(candidates))
            candidates.sort()
            labels = [label for label, _ in candidates]
            if expected_labels is not None:
                self.assertEqual(labels, expected_labels)
            self.assertLess(index, len(candidates))
            node_index = candidates[index][1]
            attempts = directory / 'metadata-probe-attempts'
            before_attempts = set(attempts.iterdir()) if attempts.exists() else set()
            fault = world.run([command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal],
                              check=False, timeout=60,
                              env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-probe-' +
                                   ('before-pin' if old_runtime is not None else 'after-pin' if kind == 'created' else 'removed') +
                                   ':' + str(node_index),
                                   'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            fresh = set(attempts.iterdir()) - before_attempts
            self.assertEqual(len(fresh), 1)
            attempt = next(iter(fresh))
            plan = json.loads((attempt / 'plan.json').read_bytes())
            node = next(row for row in plan['nodes'] if row['index'] == node_index)
            path = node['path']
            self.assertEqual(node['template'], template_paths[node_index])
            self.assertFalse((bank / '.agent-memory-transaction.md').exists())
            self.assertFalse((bank / '.agent-memory-recovery.md').exists())
            if kind == 'created' and mode == 'exit':
                self.assertTrue((bank / path).exists())
            if kind == 'removed':
                self.assertFalse((bank / path).exists())
            if mode == 'exit':
                if old_runtime is None:
                    recovery.unlock(world, command, repo)
                else:
                    old, pack = old_runtime
                    legacy_guard.guard(self, world, old, pack, repo, bank, directory, inspected, fixture[4])
                    self.assert_ambiguous_and_remove(world, fixture, attempt, path)
            self.finish(world, fixture, probes=[row['path'] for row in plan['nodes']])
            return labels

    def ambiguity_case(self, node_type, *, foreign=False, mode='exit'):
        with World() as world:
            fixture = self.prepare_fixture(world)
            _, command, repo, bank, _, _, _, _, _, _, _, proposal, _, directory, _, _ = fixture
            nodes, reachable = self.probe_plan(world, fixture)
            templates = sorted(nodes, key=lambda path: (len(PurePosixPath(path).parts), path))
            node_index = next(number for number, path in enumerate(templates)
                              if path in reachable and nodes[path]['type'] == node_type)
            attempts = directory / 'metadata-probe-attempts'
            before = set(attempts.iterdir()) if attempts.exists() else set()
            stage = 'after-pin' if foreign else 'before-pin'
            fault = world.run([command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal],
                              check=False, timeout=60,
                              env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-probe-' + stage + ':' + str(node_index),
                                   'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            fresh = set(attempts.iterdir()) - before
            self.assertEqual(len(fresh), 1)
            attempt = next(iter(fresh))
            plan = json.loads((attempt / 'plan.json').read_bytes())
            node = next(row for row in plan['nodes'] if row['index'] == node_index)
            path = node['path']
            target = bank / path
            pin = attempt / 'pins' / (target.name + '.json')
            self.assertEqual(pin.exists(), bool(foreign))
            if mode == 'error':
                self.assertFalse(target.exists())
                before_inspect = snapshot(bank), snapshot(directory)
                safe, observed = recovery.inspect(world, command, repo)
                self.assertTrue(safe['verified'])
                self.assertTrue(safe['observation_complete'])
                self.assertFalse(safe['semantic_recovery_complete'])
                self.assertNotEqual(safe['operation_id'], fixture[9])
                self.assertEqual({row['path'] for row in observed['manifest']['physical_entries']},
                                 set(before_inspect[0]))
                self.assertEqual((snapshot(bank), snapshot(directory)), before_inspect)
                self.finish(world, fixture, probes=[row['path'] for row in plan['nodes']])
                return
            self.assertTrue(target.exists())
            recovery.unlock(world, command, repo)
            if foreign == 'flags':
                required = stat.UF_NODUMP if sys.platform == 'darwin' else 0x40
                flags = exact_metadata.inode_flags(target, exact_metadata.inode_flags(target) | required)
                self.assertEqual(flags & required, required)
            elif foreign:
                original_inode = target.stat().st_ino
                if foreign == 'inode':
                    saved = world.inside('independently-inspected-original-probe')
                    os.replace(target, saved)
                target.write_bytes(b'Foreign fixture occupant; preserve this exact file.\n')
                if foreign == 'inode':
                    self.assertNotEqual(target.stat().st_ino, original_inode)
                else:
                    self.assertEqual(target.stat().st_ino, original_inode)
            self.assert_ambiguous_and_remove(world, fixture, attempt, path)
            self.finish(world, fixture, probes=[row['path'] for row in plan['nodes']])

    def test_unpinned_file_remains_pending_until_operator_removes_it(self):
        self.ambiguity_case('file')

    def test_unpinned_directory_remains_pending_until_operator_removes_it(self):
        self.ambiguity_case('directory')

    def test_before_pin_file_error_cleans_owned_probe(self):
        self.ambiguity_case('file', mode='error')

    def test_before_pin_directory_error_cleans_owned_probe(self):
        self.ambiguity_case('directory', mode='error')

    def test_foreign_replacement_after_pin_remains_pending(self):
        self.ambiguity_case('file', foreign='inode')

    def test_changed_nonce_after_pin_remains_pending(self):
        self.ambiguity_case('file', foreign='nonce')

    def test_changed_inode_flags_after_pin_remain_pending(self):
        self.ambiguity_case('file', foreign='flags')

    def late_probe_case(self, kind, index, mode, *, expected_labels=None):
        with World() as world:
            fixture = self.prepare_fixture(world)
            helper, command, repo, bank, _, originals, _, git, _, operation, inspected, proposal, _, directory, _, new = fixture
            nodes, reachable = self.probe_plan(world, fixture)
            templates = sorted(nodes, key=lambda path: (len(PurePosixPath(path).parts), path))
            deferred = [(self.node_label(nodes[path], fixture), number) for number, path in enumerate(templates)
                        if path not in reachable and (kind == 'created' or nodes[path]['type'] == 'file')]
            self.assertEqual(len({label for label, _ in deferred}), len(deferred))
            deferred.sort()
            labels = [label for label, _ in deferred]
            if expected_labels is not None:
                self.assertEqual(labels, expected_labels)
            self.assertLess(index, len(deferred))
            node_index = deferred[index][1]
            partial = world.run([command, 'bank', 'recover-layout', 'apply', '--repo', repo, '--proposal', proposal],
                                check=False, timeout=60,
                                env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-before-replace:' + new,
                                     'AGENT_MEMORY_FAULT_MODE': 'error'})
            self.assertEqual(partial.returncode, 1, partial.stdout + partial.stderr)
            self.assertTrue((bank / 'topics/probe-nested').is_dir())
            self.assertFalse((bank / new).exists())
            self.assertTrue((bank / '.agent-memory-transaction.md').exists())
            def canonical():
                return {p.relative_to(bank).as_posix(): (p.read_bytes(), stat.S_IMODE(p.stat().st_mode), attributes(p))
                        for p in bank.rglob('*') if p.is_file() and
                        not any(part.startswith('.agent-memory') for part in p.relative_to(bank).parts)}
            before = canonical()
            preview = world.inside('late-probe-preview.json')
            recovery.call(world, command, 'recover', '--repo', repo, '--operation', operation,
                          '--mode', 'finish', '--output', preview)
            attempts = directory / 'metadata-probe-attempts'
            previous = set(attempts.iterdir())
            fault = world.run([command, 'bank', 'recover-layout', 'recover', '--repo', repo, '--proposal', preview],
                              check=False, timeout=60,
                              env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-probe-' +
                                   ('after-pin' if kind == 'created' else 'removed') + ':' + str(node_index),
                                   'AGENT_MEMORY_FAULT_MODE': mode})
            self.assertEqual(fault.returncode, 86 if mode == 'exit' else 1, fault.stdout + fault.stderr)
            fresh = set(attempts.iterdir()) - previous
            self.assertEqual(len(fresh), 1)
            plan = json.loads((next(iter(fresh)) / 'plan.json').read_bytes())
            node = next(row for row in plan['nodes'] if row['index'] == node_index)
            self.assertEqual(node['template'], templates[node_index])
            if mode == 'exit':
                recovery.unlock(world, command, repo)
            self.assertEqual(canonical(), before)
            self.assertTrue((bank / '.agent-memory-transaction.md').exists())
            self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                              for row in inspected['manifest']['sources']}, originals)
            helper.assert_fault_work_unchanged(world, repo, git)
            self.finish(world, fixture, probes=[row['path'] for row in plan['nodes']], partial=True)
            return labels

    def late_probe_partition(self, kind, mode):
        labels = self.late_probe_case(kind, 0, mode)
        for index in range(1, len(labels)):
            with self.subTest(kind=kind, label=labels[index], mode=mode):
                self.late_probe_case(kind, index, mode, expected_labels=labels)

    def test_late_created_error(self):
        self.late_probe_partition('created', 'error')

    def test_late_created_exit(self):
        self.late_probe_partition('created', 'exit')

    def test_late_removed_error(self):
        self.late_probe_partition('removed', 'error')

    def test_late_removed_exit(self):
        self.late_probe_partition('removed', 'exit')

    def probe_partition(self, kind, partition, mode):
        labels = self.probe_case(kind, partition, mode)
        for index in range(partition + 4, len(labels), 4):
            with self.subTest(kind=kind, label=labels[index], mode=mode):
                self.probe_case(kind, index, mode, expected_labels=labels)

    def test_legacy_stale_probe_without_marker(self):
        with World() as legacy:
            pack = v014_runtime_fixture.extract(legacy.root)
            installed = legacy.run([sys.executable, '-I', pack / 'scripts/installer.py', 'install',
                                    '--development-source', pack, '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old = Path(receipt['bin_dir']) / 'am'
            self.probe_case('created', 0, 'exit', old_runtime=(old, pack))

    def test_journal_error(self):
        self.journey('layout-recovery-probe-journal', 'error')

    def test_journal_exit(self):
        self.journey('layout-recovery-probe-journal', 'exit')

    def test_attempt_journal_error(self):
        self.journey('layout-recovery-probe-after-attempt-journal', 'error')

    def test_attempt_journal_exit(self):
        self.journey('layout-recovery-probe-after-attempt-journal', 'exit')

    def test_directories_removed_error(self):
        self.journey('layout-recovery-probe-directories-removed', 'error')

    def test_directories_removed_exit(self):
        self.journey('layout-recovery-probe-directories-removed', 'exit')


def _probe_partition_case(kind, partition, mode):
    def run(self):
        self.probe_partition(kind, partition, mode)
    return run


for _kind in ('created', 'removed'):
    for _partition in range(4):
        for _mode in ('error', 'exit'):
            setattr(LayoutMetadataProbeFaults, 'test_' + _kind + '_' + str(_partition) + '_' + _mode,
                    _probe_partition_case(_kind, _partition, _mode))
