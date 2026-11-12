"""Exercise public diagnostics in isolated homes and actual installations."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import unittest
import time

from world import World
from runtime_update_fixtures import ROOT, PACK, fixture, hook_fixture, bank, snapshot, receipt_path


def call(world, operation, *options, receipt=None, expected=0):
    command = ([Path(receipt['bin_dir']) / 'agent-memory', '--json'] if receipt else
               [sys.executable, '-I', ROOT / 'scripts/diagnose.py'])
    result = world.run([*command, operation, '--json', *options], check=False, timeout=120)
    if result.returncode != expected:
        raise AssertionError((result.returncode, result.stdout, result.stderr))
    value = json.loads(result.stdout)
    assert not value['writes_performed'] and not value['repairs_performed'] and not value['network_performed']
    return value


def component(value, name):
    return [v for v in value['checks'] if v['component'] == name]

def native_fixture(world, mode):
    channel = world.repository('native diagnostic channel')
    shutil.copytree(PACK, channel / 'skills/agent-memory')
    world.git('add', '.', cwd=channel)
    world.git('commit', '-m', 'Disposable native diagnostic runtime', cwd=channel)
    root = world.inside('native root')
    root.mkdir()
    installed = world.run([sys.executable, '-I', '-B', ROOT / 'scripts/installer.py',
        'install', '--remote', channel, '--ref', 'main', '--mode', mode,
        '--presets', 'omp', '--omp-agent-dir', root, '--omp-peers-upgraded',
        '--bin-dir', world.inside('owned bin')], timeout=120)
    bank(world, world.paths['repo'])
    return json.loads(installed.stdout)['receipt']



class RuntimeDiagnosticsContract(unittest.TestCase):
    def test_native_copy_and_link_doctor_distinguishes_extension_and_launchers(self):
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world:
                receipt = native_fixture(world, mode)
                extension = Path(receipt['omp_binding']['extension_target'])
                launcher = Path(receipt['bin_dir']) / 'am'
                self.assertEqual(extension.stat().st_mode & 0o7777, 0o644)

                def inspect(expected=0, command='am'):
                    before = snapshot(world.root)
                    result = world.run([Path(receipt['bin_dir']) / command, '--json',
                        'doctor', '--repo', world.paths['repo']], check=False, timeout=120)
                    self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
                    value = json.loads(result.stdout)
                    self.assertEqual(value['ok'], expected == 0)
                    for key in ('writes_performed', 'repairs_performed', 'network_performed'):
                        self.assertFalse(value[key])
                    self.assertEqual(snapshot(world.root), before)
                    return value

                healthy = inspect()
                wrappers = component(healthy, 'runtime.target.wrapper')
                self.assertEqual({check['path'] for check in wrappers},
                    {str(extension), str(launcher), str(launcher.with_name('agent-memory'))})
                self.assertTrue(all(check['state'] == 'pass' for check in wrappers))
                for path, change in ((extension, 'bytes'), (extension, 'mode'),
                                     (launcher, 'bytes'), (launcher, 'mode')):
                    with self.subTest(target=path.name, change=change):
                        original = path.read_bytes()
                        mode_bits = path.stat().st_mode & 0o7777
                        if change == 'bytes':
                            path.write_bytes(original + b'\nchanged ownership bytes\n')
                        else:
                            path.chmod(0o600)
                        failed = inspect(1, command='agent-memory')
                        check = next(check for check in component(failed, 'runtime.target.wrapper')
                                     if check['path'] == str(path))
                        self.assertEqual(check['state'], 'failure')
                        path.write_bytes(original)
                        path.chmod(mode_bits)
                inspect()

    def test_native_doctor_rejects_rebound_wrapper_bytes_modes_and_roles(self):
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world:
                original = native_fixture(world, mode)
                extension = Path(original['omp_binding']['extension_target'])
                launcher = Path(original['bin_dir']) / 'am'
                receipt_file = receipt_path(world)
                journal_file = receipt_file.parent / 'journal.json'
                receipt_bytes, journal_bytes = receipt_file.read_bytes(), journal_file.read_bytes()
                for case in ('extension-bytes', 'launcher-bytes', 'launcher-mode',
                             'unknown-wrapper', 'extension-role-spoof'):
                    with self.subTest(case=case):
                        receipt = json.loads(receipt_bytes)
                        changed = extension if case == 'extension-bytes' else launcher
                        if case == 'unknown-wrapper':
                            changed = world.inside('unrecognized wrapper')
                            changed.write_bytes(launcher.read_bytes())
                            changed.chmod(0o755)
                            receipt['targets'].append({'path': str(changed), 'kind': 'wrapper',
                                                       'identity': None})
                        original_bytes = changed.read_bytes()
                        original_mode = changed.stat().st_mode & 0o7777
                        if case.endswith('-bytes'):
                            changed.write_bytes(original_bytes + b'\nchanged entrypoint\n')
                        elif case == 'launcher-mode':
                            changed.chmod(0o600)
                        elif case == 'extension-role-spoof':
                            receipt['omp_binding']['extension_target'] = str(launcher)
                        target = next(t for t in receipt['targets'] if t['path'] == str(changed))
                        target['identity'] = {'kind': 'file',
                            'hash': hashlib.sha256(changed.read_bytes()).hexdigest(),
                            'mode': changed.stat().st_mode & 0o7777}
                        raw = (json.dumps(receipt, sort_keys=True, indent=2) + '\n').encode()
                        receipt_file.write_bytes(raw)
                        journal = json.loads(journal_bytes)
                        journal['receipt'] = receipt
                        journal['receipt_identity'] = {'kind': 'file',
                            'hash': hashlib.sha256(raw).hexdigest(),
                            'mode': receipt_file.stat().st_mode & 0o7777}
                        journal_file.write_text(json.dumps(journal))
                        before = snapshot(world.root)
                        value = call(world, 'doctor', '--repo', world.paths['repo'],
                                     receipt=original, expected=1)
                        if case == 'extension-role-spoof':
                            self.assertTrue(component(value, 'runtime.receipt'))
                        else:
                            check = next(c for c in component(value, 'runtime.target.wrapper')
                                         if c['path'] == str(changed))
                            self.assertEqual(check['state'], 'failure')
                            self.assertFalse(component(value, 'runtime.receipt'))
                        self.assertEqual(snapshot(world.root), before)
                        changed.write_bytes(original_bytes)
                        changed.chmod(original_mode)
                        if case == 'unknown-wrapper':
                            changed.unlink()
                        receipt_file.write_bytes(receipt_bytes)
                        journal_file.write_bytes(journal_bytes)

    def test_native_source_writer_defers_read_only_diagnostics_without_repair(self):
        with World() as world:
            receipt = native_fixture(world, 'copy')
            repo = world.paths['repo']
            base = (world.paths['state'] / 'agent-memory/writes'
                    / hashlib.sha256(str(repo / 'memory-bank').encode()).hexdigest())
            base.mkdir(parents=True, mode=0o700)
            (base / 'omp-stream-v1').mkdir(mode=0o700)
            barrier = base / 'source-barrier.lock'
            with barrier.open('xb') as owner:
                os.fchmod(owner.fileno(), 0o600)
                fcntl.flock(owner, fcntl.LOCK_EX | fcntl.LOCK_NB)
                before = snapshot(world.root)
                for operation in ('status', 'doctor'):
                    with self.subTest(operation=operation):
                        value = call(world, operation, '--repo', repo, receipt=receipt)
                        checks = component(value, 'omp.native_boundary')
                        self.assertEqual(len(checks), 1)
                        self.assertEqual(checks[0]['state'], 'warning')
                        self.assertEqual(checks[0]['code'], 'source-transport-busy')
                        self.assertNotIn('omp', value)
                        self.assertEqual(snapshot(world.root), before)
                fcntl.flock(owner, fcntl.LOCK_UN)
            available = call(world, 'status', '--repo', repo, receipt=receipt)
            self.assertEqual(available['omp']['owned_disk_integrity'], 'verified')
            self.assertEqual(available['omp']['transport_qualification']['pending'], [])
            self.assertEqual(snapshot(world.root), before)

    def test_native_doctor_never_repairs_missing_observer_locks(self):
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world:
                receipt = native_fixture(world, mode)
                bank_path = world.paths['repo'] / 'memory-bank'
                base = (world.paths['state'] / 'agent-memory/writes'
                        / hashlib.sha256(str(bank_path).encode()).hexdigest())
                base.mkdir(parents=True, mode=0o700)
                barrier = base / 'source-barrier.lock'
                barrier.write_bytes(b'')
                barrier.chmod(0o600)
                (base / 'omp-source-v1').mkdir(mode=0o700)
                before = snapshot(world.root)
                value = call(world, 'doctor', '--repo', world.paths['repo'],
                             receipt=receipt, expected=1)
                self.assertTrue(component(value, 'omp.native_boundary'))
                self.assertEqual(snapshot(world.root), before)
                self.assertFalse((base / 'omp-transport.lock').exists())

    def test_shared_owned_hook_health_and_negative_controls(self):
        for preset in ('codex', 'claude'):
            with self.subTest(preset=preset), World() as world:
                receipt, path = hook_fixture(world, preset)
                original = json.loads(path.read_bytes())
                foreign = {'hooks': [{'type': 'command', 'command': 'printf foreign'}]}
                value = json.loads(path.read_bytes())
                value['otherToolSetting'] = {'enabled': True}
                value['hooks']['UserPromptSubmit'].append(foreign)
                value['hooks']['Stop'].append(foreign)
                path.write_text(json.dumps(value, sort_keys=True))
                before = snapshot(world.root)
                result = call(world, 'doctor', '--repo', world.paths['repo'])
                check = next(c for c in component(result, 'runtime.target.hook-config') if c['path'] == str(path))
                self.assertEqual((check['state'], check['verification_scope']), ('pass', 'receipt-owned-entries'))
                self.assertEqual(snapshot(world.root), before)
                preview = world.run([sys.executable, '-I', ROOT/'scripts/uninstall_runtime.py',
                                     'uninstall', '--check'])
                self.assertFalse(json.loads(preview.stdout)['preserved'])
                cases = {}
                for event in ('UserPromptSubmit', 'Stop'):
                    missing = json.loads(json.dumps(original)); missing['hooks'][event].pop(0)
                    duplicate = json.loads(json.dumps(original)); duplicate['hooks'][event].append(duplicate['hooks'][event][0])
                    changed = json.loads(json.dumps(original)); changed['hooks'][event][0]['hooks'][0]['command'] += ' changed'
                    cases.update({event+'-missing': json.dumps(missing).encode(),
                                  event+'-duplicate': json.dumps(duplicate).encode(),
                                  event+'-changed': json.dumps(changed).encode()})
                altered = json.loads(json.dumps(original))
                altered['hooks']['Stop'][0]['hooks'][0]['asyncRewake'] = 1
                cases['flag'] = json.dumps(altered).encode()
                altered = json.loads(json.dumps(original))
                altered['hooks']['UserPromptSubmit'][0]['hooks'][0]['type'] = 'other'
                cases['type'] = json.dumps(altered).encode()
                cases['duplicate-key'] = b'{"hooks":{},"hooks":{}}'
                cases['nested-duplicate'] = b'{"hooks":{"Stop":[{"hooks":[{"type":"command","type":"bad"}]}]}}'
                cases['private-duplicate-key'] = b'{"secret-hook-key-784":1,"secret-hook-key-784":2}'
                cases['malformed'] = b'{"hooks":'
                cases['invalid-stop-shape'] = b'{"hooks":{"Stop":{}}}'
                for label, raw in cases.items():
                    with self.subTest(case=label):
                        path.write_bytes(raw)
                        before = snapshot(world.root)
                        result = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
                        check = component(result, 'runtime.target.hook-config')[0]
                        self.assertEqual(check['state'], 'failure')
                        if label in ('duplicate-key', 'nested-duplicate', 'private-duplicate-key'):
                            code = 'duplicate_json_key'
                        elif label.endswith('-duplicate'):
                            code = 'owned_group_duplicate'
                        elif label == 'malformed':
                            code = 'malformed_json'
                        elif label == 'invalid-stop-shape':
                            code = 'unsupported_shape'
                        else:
                            code = 'owned_group_missing_or_changed'
                        self.assertEqual(check['failure_code'], code)
                        self.assertNotIn('secret-hook-key-784', json.dumps(result))
                        self.assertEqual(snapshot(world.root), before)
                path.write_text(json.dumps(original))
                mode = path.stat().st_mode & 0o777
                path.chmod(0o600 if mode != 0o600 else 0o644)
                before = snapshot(world.root)
                check = component(call(world, 'doctor', '--repo', world.paths['repo'], expected=1),
                                  'runtime.target.hook-config')[0]
                self.assertEqual((check['state'], check['failure_code']), ('failure', 'mode_changed'))
                self.assertEqual(snapshot(world.root), before)
                path.chmod(mode)
                path.unlink(); path.symlink_to(world.paths['repo']/'missing-hook')
                before = snapshot(world.root)
                check = component(call(world, 'doctor', '--repo', world.paths['repo'], expected=1),
                                  'runtime.target.hook-config')[0]
                self.assertEqual((check['state'], check['failure_code']), ('failure', 'unsafe_path'))
                self.assertEqual(snapshot(world.root), before)
                path.unlink(); os.mkfifo(path)
                before = snapshot(world.root)
                check = component(call(world, 'doctor', '--repo', world.paths['repo'], expected=1),
                                  'runtime.target.hook-config')[0]
                self.assertEqual((check['state'], check['failure_code']), ('failure', 'unsafe_type'))
                self.assertEqual(snapshot(world.root), before)

    def test_hook_verification_refuses_an_edit_during_readback(self):
        for preset in ('codex', 'claude'):
            for change in ('bytes', 'mode'):
                with self.subTest(preset=preset, change=change), World() as world:
                    _, path = hook_fixture(world, preset)
                    control = world.inside('hook-race'); control.mkdir()
                    world.env.update(AGENT_MEMORY_FAULT_STAGE='hook-ownership-before-recheck',
                                     AGENT_MEMORY_FAULT_MODE='pause', AGENT_MEMORY_FAULT_DIR=str(control))
                    child = world.spawn([sys.executable, '-I', ROOT/'scripts/diagnose.py',
                                         'doctor', '--json', '--repo', world.paths['repo']])
                    for key in ('AGENT_MEMORY_FAULT_STAGE', 'AGENT_MEMORY_FAULT_MODE', 'AGENT_MEMORY_FAULT_DIR'):
                        del world.env[key]
                    ready = control/'hook-ownership-before-recheck.ready'
                    deadline = time.monotonic() + 10
                    while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                        time.sleep(.01)
                    self.assertTrue(ready.exists())
                    receipt_before = receipt_path(world).read_bytes()
                    if change == 'bytes':
                        value = json.loads(path.read_bytes()); value['racingForeignKey'] = True
                        path.write_text(json.dumps(value))
                    else:
                        path.chmod(0o600 if path.stat().st_mode & 0o777 != 0o600 else 0o644)
                    expected = snapshot(path)
                    (control/'hook-ownership-before-recheck.release').touch()
                    stdout, stderr = child.communicate(timeout=30)
                    self.assertEqual(child.returncode, 1, stderr)
                    check = component(json.loads(stdout), 'runtime.target.hook-config')[0]
                    self.assertEqual((check['state'], check['failure_code']), ('failure', 'concurrent_change'))
                    self.assertEqual(snapshot(path), expected)
                    self.assertEqual(receipt_path(world).read_bytes(), receipt_before)

    def test_insufficient_hook_receipt_is_not_adopted_from_matching_bytes(self):
        with World() as world:
            _, path = hook_fixture(world, 'codex')
            receipt_file = receipt_path(world)
            receipt = json.loads(receipt_file.read_bytes())
            target = next(t for t in receipt['targets'] if t['kind'] == 'hook-config')
            del target['hook_entry']
            raw = (json.dumps(receipt, sort_keys=True, indent=2) + '\n').encode()
            receipt_file.write_bytes(raw)
            journal_path = receipt_file.parent/'journal.json'
            journal = json.loads(journal_path.read_bytes())
            journal['receipt'] = receipt
            journal['receipt_identity'] = {'kind': 'file', 'hash': hashlib.sha256(raw).hexdigest(),
                                           'mode': receipt_file.stat().st_mode & 0o7777}
            journal_path.write_text(json.dumps(journal))
            before = snapshot(world.root)
            result = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            check = component(result, 'runtime.target.hook-config')[0]
            self.assertEqual((check['state'], check['failure_code']), ('failure', 'insufficient_receipt'))
            self.assertEqual(snapshot(world.root), before)
            preview = world.run([sys.executable, '-I', ROOT/'scripts/uninstall_runtime.py',
                                 'uninstall', '--check'])
            self.assertTrue(any(item['path'] == str(path) for item in json.loads(preview.stdout)['preserved']))
            self.assertEqual(snapshot(world.root), before)

    def test_prompt_only_legacy_and_invalid_present_stop_receipts(self):
        for preset in ('codex', 'claude'):
            with self.subTest(preset=preset), World() as world:
                _, path = hook_fixture(world, preset)
                receipt_file = receipt_path(world)
                original = json.loads(receipt_file.read_bytes())
                journal_path = receipt_file.parent/'journal.json'
                journal = json.loads(journal_path.read_bytes())
                document = json.loads(path.read_bytes())
                document['unrelated-setting'] = {'preserve': True}
                path.write_text(json.dumps(document))
                for case in ('absent', 'null', 'empty'):
                    with self.subTest(stop_evidence=case):
                        receipt = json.loads(json.dumps(original))
                        target = next(t for t in receipt['targets'] if t['kind'] == 'hook-config')
                        if case == 'absent':
                            del target['stop_entry']
                        else:
                            target['stop_entry'] = None if case == 'null' else {}
                        raw = (json.dumps(receipt, sort_keys=True, indent=2) + '\n').encode()
                        receipt_file.write_bytes(raw)
                        journal['receipt'] = receipt
                        journal['receipt_identity'] = {
                            'kind': 'file', 'hash': hashlib.sha256(raw).hexdigest(),
                            'mode': receipt_file.stat().st_mode & 0o7777}
                        journal_path.write_text(json.dumps(journal))
                        before = snapshot(world.root)
                        result = call(world, 'doctor', '--repo', world.paths['repo'],
                                      expected=0 if case == 'absent' else 1)
                        check = component(result, 'runtime.target.hook-config')[0]
                        if case == 'absent':
                            self.assertEqual(check['state'], 'pass')
                            self.assertEqual(check['verification_scope'], 'receipt-owned-entries')
                        else:
                            self.assertEqual((check['state'], check['failure_code']),
                                             ('failure', 'insufficient_receipt'))
                        self.assertEqual(snapshot(world.root), before)

    def test_opencode_plugin_link_remains_exact(self):
        with World() as world:
            channel = world.repository('opencode channel')
            shutil.copytree(PACK, channel/'skills/agent-memory')
            world.git('add', '.', cwd=channel); world.git('commit', '-m', 'Disposable runtime', cwd=channel)
            installed = json.loads(world.run([sys.executable, '-I', ROOT/'scripts/installer.py', 'install',
                                              '--remote', channel, '--presets', 'opencode']).stdout)
            target = next(t for t in installed['receipt']['targets'] if t['kind'] == 'hook-link')
            path = Path(target['path']); path.unlink(); path.symlink_to(world.paths['repo'])
            before = snapshot(world.root)
            result = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            self.assertEqual(component(result, 'runtime.target.hook-link')[0]['state'], 'failure')
            self.assertEqual(snapshot(world.root), before)

    def test_owner_record_diagnostics_agree_with_actual_recovery_and_preserve_bad_metadata(self):
        for case in ('healthy', 'malformed', 'dangling-symlink'):
            with self.subTest(case=case), World() as world:
                _, _, receipt = fixture(world)
                owner = receipt_path(world).parent / 'owner.json'
                if case == 'malformed':
                    owner.write_text('{"schema":"unrelated-owner-record","operator_note":"preserve"}\n')
                elif case == 'dangling-symlink':
                    owner.unlink(); owner.symlink_to(world.inside('missing-owner-target'))
                before = snapshot(world.root)
                expected = 0 if case == 'healthy' else 1
                value = call(world, 'doctor', '--repo', world.paths['repo'], expected=expected)
                owner_check = component(value, 'runtime.owner')
                self.assertEqual(owner_check[0]['state'], 'pass' if case == 'healthy' else 'failure')
                self.assertEqual(owner_check[0]['path'], str(owner))
                self.assertEqual(snapshot(world.root), before)
                recovered = world.run([sys.executable, '-I', ROOT / 'scripts/installer.py', 'recover'], check=False)
                self.assertEqual(recovered.returncode, expected, recovered.stdout)
                if case != 'healthy':
                    self.assertEqual(snapshot(world.root), before)

    def test_resolved_identity_and_custom_bank_follow_the_selected_worktree(self):
        from identity_fixtures import bind
        with World() as world:
            repo = world.repository()
            directory = bank(world, repo)
            bind(world, repo)
            world.git('add', '.', cwd=repo); world.git('commit', '-m', 'Bound test memory', cwd=repo)
            worktree = world.inside('additional worktree')
            world.git('worktree', 'add', '-b', 'other-session', worktree, cwd=repo)
            before = snapshot(world.root)
            first = call(world, 'doctor', '--repo', repo)
            other = call(world, 'doctor', '--repo', worktree)
            self.assertTrue(first['developer']['resolved'])
            self.assertTrue(first['project']['authority_verified'])
            self.assertTrue(other['project']['authority_verified'])
            self.assertEqual(first['project']['id'], other['project']['id'])
            self.assertEqual(other['bank']['path'], str(worktree / 'memory-bank'))
            self.assertEqual(snapshot(world.root), before)
            directory.rename(repo / 'project-notes')
            world.git('config', 'agent-memory.bankRoot', 'project-notes', cwd=repo)
            world.git('add', '.', cwd=repo); world.git('commit', '-m', 'Move bank with explicit configuration', cwd=repo)
            before = snapshot(world.root)
            status = call(world, 'status', '--repo', repo)
            self.assertEqual(status['bank']['path'], str(repo / 'project-notes'))
            self.assertEqual(status['project']['id'], first['project']['id'])
            self.assertEqual(snapshot(world.root), before)

    def test_doctor_does_not_resume_a_separately_confirmed_purge_or_initialize_replacement(self):
        with World() as world:
            directory = bank(world, world.paths['repo'])
            command = [sys.executable, '-I', ROOT / 'scripts/uninstall_runtime.py', 'purge-data',
                       '--repo', world.paths['repo'], '--path', directory]
            confirmation = json.loads(world.run([*command, '--check']).stdout)['preview']['confirmation']
            interrupted = world.run([*command, '--confirm', confirmation], check=False, env={
                'AGENT_MEMORY_FAULT_STAGE': 'purge-after-detach', 'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(interrupted.returncode, 86)
            before = snapshot(world.root)
            value = call(world, 'doctor', '--repo', world.paths['repo'])
            self.assertEqual(value['bank']['state'], 'purge_pending')
            self.assertTrue(component(value, 'data.purge')); self.assertFalse(component(value, 'bank.setup'))
            self.assertEqual(snapshot(world.root), before)

    def test_no_install_bank_or_identity_is_required_and_nothing_is_bootstrapped(self):
        with World() as world:
            before = snapshot(world.root)
            version = call(world, 'version')
            status = call(world, 'status', '--repo', world.paths['repo'])
            doctor = call(world, 'doctor', '--repo', world.paths['repo'])
            self.assertEqual(version['installation']['state'], 'not_installed')
            self.assertEqual(status['bank']['state'], 'not_located')
            self.assertFalse(status['bank']['absence_verified'])
            self.assertIsNone(status['developer']['id'])
            self.assertEqual(doctor['bank']['state'], 'not_initialized')
            self.assertTrue(doctor['warnings']); self.assertFalse(doctor['failures'])
            self.assertEqual(snapshot(world.root), before)

    def test_installed_version_binds_actual_artifact_revision_mode_and_channel(self):
        with World() as world:
            channel, _, receipt = fixture(world, 'link')
            before = snapshot(world.root)
            value = call(world, 'version', receipt=receipt)
            current = value['executing_source']; install = value['installation']
            self.assertEqual(current['revision'], world.git('rev-parse', 'HEAD', cwd=channel).stdout.strip())
            self.assertEqual(current['version'], '1.0.0'); self.assertEqual(current['cleanliness'], 'clean')
            self.assertEqual(current['artifact_digest'], receipt['source']['artifact_digest'])
            self.assertEqual(install['mode'], 'link'); self.assertEqual(install['receipt_schema'], receipt['schema'])
            self.assertEqual(install['channel']['ref'], 'main')
            self.assertEqual(snapshot(world.root), before)

    def test_status_is_shallow_but_doctor_checks_real_bank_content_and_permissions(self):
        with World() as world:
            directory = bank(world, world.paths['repo'])
            context = directory / 'projectbrief.md'; context.unlink()
            before = snapshot(world.root)
            shallow = call(world, 'status', '--repo', world.paths['repo'])
            self.assertIsNone(shallow['bank']['counts_by_lifecycle'])
            self.assertEqual(shallow['bank']['state'], 'router_observed')
            deep = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            self.assertTrue(component(deep, 'bank.inspection'))
            self.assertEqual(snapshot(world.root), before)
            context.write_text('# Project\n\nReadable context.\n')
            directory.chmod(0o500)
            try:
                denied = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
                self.assertTrue(component(denied, 'bank.permissions'))
            finally:
                directory.chmod(0o700)

    def test_healthy_copy_and_link_installs_and_allowed_outside_activation_edits(self):
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as world:
                _, _, receipt = fixture(world, mode)
                bank(world, world.paths['repo'])
                target = world.paths['codex'] / 'AGENTS.md'
                target.write_text('Extra user text.\n' + target.read_text() + '\nExtra suffix.\n')
                before = snapshot(world.root)
                value = call(world, 'doctor', '--repo', world.paths['repo'], receipt=receipt)
                self.assertEqual(value['bank']['counts_by_lifecycle'], {'active': 1})
                self.assertEqual(value['bank']['index_freshness'], 'canonical_topic_routes_validated')
                self.assertFalse(value['failures'])
                self.assertTrue(component(value, 'runtime.target.activation'))
                self.assertEqual(snapshot(world.root), before)

    def test_changed_wrapper_and_copy_are_reported_from_an_outside_helper(self):
        with World() as world:
            _, _, receipt = fixture(world)
            wrapper = Path(receipt['bin_dir']) / 'agent-memory'
            wrapper.write_text('Do not execute this replacement.\n')
            wrapper.chmod(0o644)
            copy = world.paths['codex'] / 'skills/agent-memory/templates/activation-markdown.example.txt'
            copy.write_text('User replacement.\n')
            before = snapshot(world.root)
            value = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            failed = {v['component'] for v in value['failures']}
            self.assertIn('runtime.target.wrapper', failed); self.assertIn('runtime.target.copy', failed)
            changed_wrapper = next(v for v in component(value, 'runtime.target.wrapper')
                                   if v.get('path') == str(wrapper))
            self.assertIn('0o644', changed_wrapper['detail'])
            self.assertEqual(value['installation']['purge_transactions'], 'none_pending')
            self.assertTrue(all(v.get('repair') for v in value['failures']))
            self.assertEqual(snapshot(world.root), before)

    def test_changed_link_and_activation_block_are_preserved(self):
        with World() as world:
            _, _, receipt = fixture(world, 'link')
            link = world.paths['codex'] / 'skills/agent-memory'
            link.unlink(); link.symlink_to(world.paths['repo'])
            block = world.paths['codex'] / 'AGENTS.md'
            block.write_text(block.read_text().replace('load the agent-memory skill', 'load different instructions'))
            before = snapshot(world.root)
            value = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            failed = {v['component'] for v in value['failures']}
            self.assertIn('runtime.target.link', failed); self.assertIn('runtime.target.activation', failed)
            self.assertEqual(snapshot(world.root), before)

    def test_missing_source_and_committed_receipt_corruption_are_distinct_failures(self):
        with World() as world:
            _, _, receipt = fixture(world)
            shutil.rmtree(receipt['source']['path'])
            missing = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            self.assertTrue(component(missing, 'runtime.source'))
            path = receipt_path(world); value = json.loads(path.read_text()); value['uninstalled'] = True
            path.write_text(json.dumps(value)); before = snapshot(world.root)
            changed = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            self.assertTrue(component(changed, 'runtime.receipt'))
            self.assertEqual(snapshot(world.root), before)

    def test_dirty_managed_source_and_changed_channel_are_not_healthy(self):
        with World() as world:
            _, _, receipt = fixture(world)
            source = Path(receipt['source']['path'])
            (source / 'local-code.txt').write_text('Uncommitted developer code.\n')
            dirty = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            self.assertEqual(dirty['installed_source']['cleanliness'], 'dirty')
            self.assertTrue(component(dirty, 'runtime.source'))
            (source / 'local-code.txt').unlink()
            world.git('remote', 'set-url', 'origin', 'https://example.invalid/different.git', cwd=source)
            changed = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            self.assertTrue(component(changed, 'runtime.source'))

    def test_pending_runtime_and_bank_transactions_are_observed_without_recovery(self):
        with World() as world:
            _, _, receipt = fixture(world)
            directory = bank(world, world.paths['repo'])
            path = receipt_path(world).parent / 'journal.json'
            journal = json.loads(path.read_text()); journal['status'] = 'activating'; path.write_text(json.dumps(journal))
            marker = directory / '.agent-memory-transaction.md'; marker.write_text('Original interrupted bank operation.\n')
            before = snapshot(world.root)
            value = call(world, 'doctor', '--repo', world.paths['repo'])
            self.assertTrue(component(value, 'runtime.transaction')); self.assertTrue(component(value, 'bank.transaction'))
            self.assertEqual(value['bank']['state'], 'busy_or_interrupted')
            self.assertEqual(snapshot(world.root), before)

    def test_live_runtime_lock_is_not_stolen_or_treated_as_an_orphan(self):
        with World() as world:
            _, _, receipt = fixture(world)
            lock = receipt_path(world).parent / 'lock'
            with lock.open('rb') as stream:
                fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
                before = snapshot(world.root)
                value = call(world, 'doctor', '--repo', world.paths['repo'])
                self.assertEqual(component(value, 'runtime.lock')[0]['state'], 'warning')
                self.assertFalse(component(value, 'runtime.target.copy'))
                self.assertEqual(snapshot(world.root), before)

    def test_selected_projection_current_and_stale_do_not_create_audit_or_cache(self):
        with World() as world:
            directory = bank(world, world.paths['repo'])
            path = world.inside('reference-projection.json')
            hashes = {p.relative_to(directory).as_posix(): hashlib.sha256(p.read_bytes()).hexdigest()
                      for p in directory.rglob('*') if p.is_file()}
            path.write_text(json.dumps({'schema': 'agent-memory/reference-projection-v1',
                'project_id': '8e0b66aa-2be5-4fa3-b47f-1245474aa7dd', 'source_hashes': hashes, 'records': []}))
            before = snapshot(world.root)
            value = call(world, 'doctor', '--repo', world.paths['repo'], '--projection', path)
            self.assertTrue(component(value, 'bank.projection'))
            self.assertEqual(snapshot(world.root), before)
            context = directory / 'projectbrief.md'; context.write_text(context.read_text() + '\nNew context.\n')
            before = snapshot(world.root)
            stale = call(world, 'doctor', '--repo', world.paths['repo'], '--projection', path, expected=1)
            self.assertEqual(component(stale, 'bank.projection')[0]['state'], 'failure')
            self.assertEqual(snapshot(world.root), before)

    def test_credential_urls_and_corrupt_records_do_not_escape_diagnostics(self):
        with World() as world:
            _, _, receipt = fixture(world)
            path = receipt_path(world); value = json.loads(path.read_text())
            secret = 'should-never-appear-secret-123'
            value['source']['remote'] = 'https://user:' + secret + '@github.com/example/repo.git'
            path.write_text(json.dumps(value))
            before = snapshot(world.root)
            result = call(world, 'version', expected=1)
            self.assertNotIn(secret, json.dumps(result))
            self.assertEqual(result['installation']['channel']['remote'], 'repo://github.com/example/repo')
            path.write_text('{"invalid":"' + secret + '"')
            result = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            self.assertNotIn(secret, json.dumps(result)); self.assertTrue(result['failures'])

    def test_unknown_bank_schema_and_redirected_router_are_not_accepted(self):
        with World() as world:
            directory = bank(world, world.paths['repo']); router = directory / 'MEMORY.md'
            original = router.read_text(); router.write_text(original.replace('agent-memory/v1', 'agent-memory/v99'))
            before = snapshot(world.root)
            value = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            self.assertTrue(value['failures']); self.assertEqual(snapshot(world.root), before)
            target = world.inside('foreign-router.md'); target.write_text(original)
            router.unlink(); router.symlink_to(target)
            value = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            self.assertTrue(value['failures']); self.assertEqual(target.read_text(), original)
