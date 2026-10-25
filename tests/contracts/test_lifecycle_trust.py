"""Public lifecycle outcomes under isolated hostile and ordinary Unix layouts."""
import json
import hashlib
import os
from pathlib import Path
import shutil
import sys
import tarfile
import time
import unittest

from world import World
from cli_alias_fixtures import channel, install, alias_tmpdir
from runtime_update_fixtures import snapshot

ROOT = Path(__file__).resolve().parents[2]
HELPER = ROOT / 'skills/agent-memory/scripts/installer.py'


class LifecycleTrustContract(unittest.TestCase):
    def outside(self, w, *args, expected=0):
        result = w.run([sys.executable, '-I', HELPER, *args], check=False, timeout=120)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def setup_install(self, w, *, mode='link'):
        remote, pack = channel(w)
        receipt = install(w, remote, '--mode', mode, '--presets', 'codex')['receipt']
        return remote, pack, receipt, Path(receipt['bin_dir']) / 'am'

    def test_preexisting_nonprivate_state_refuses_before_any_write(self):
        for mode in (0o777, 0o755, 0o770):
            with self.subTest(mode=oct(mode)), World() as w:
                state = w.paths['state'] / 'agent-memory/install'
                state.mkdir(parents=True); state.chmod(mode)
                (state / 'unrelated.txt').write_text('Keep this user file.\n')
                before = snapshot(w.root)
                value = self.outside(w, 'install', '--check', '--presets', 'none', expected=1)
                self.assertFalse(value['verified']); self.assertIn('private', value['error'])
                self.assertEqual(snapshot(w.root), before)

    def test_private_child_cannot_rescue_reachable_nonsticky_writable_parent(self):
        with World() as w:
            w.root.chmod(0o755)
            parent = w.root / 'exposed'; parent.mkdir(mode=0o777); parent.chmod(0o777)
            state = parent / 'private/agent-memory/install'; state.mkdir(parents=True)
            state.chmod(0o700); w.env['XDG_STATE_HOME'] = str(parent / 'private')
            before = snapshot(w.root)
            value = self.outside(w, 'recover', expected=1)
            self.assertIn('group/other write', value['error'])
            self.assertEqual(snapshot(w.root), before)

    def test_private_temp_barrier_native_alias_and_group_write_payloads_work(self):
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as w:
                remote, pack = channel(w); alias_tmpdir(w)
                w.root.chmod(0o755)
                public = w.root / 'public-skills'; public.mkdir(mode=0o755)
                argv = [sys.executable, '-I', HELPER, 'install', '--remote', remote.as_uri(),
                        '--ref', 'main', '--mode', mode, '--presets', 'codex', '--target-dir', public]
                launched = w.run([sys.executable, '-I', '-c',
                    'import os,subprocess,sys;os.umask(0o002);sys.exit(subprocess.call(sys.argv[1:]))',
                    *argv], check=False, timeout=120)
                self.assertEqual(launched.returncode, 0, launched.stdout + launched.stderr)
                receipt = json.loads(launched.stdout)['receipt']; cli = Path(receipt['bin_dir']) / 'am'
                self.assertEqual(Path(receipt['source']['path']).stat().st_mode & 0o777, 0o700)
                payload = Path(receipt['source']['pack']) / 'SKILL.md'
                self.assertTrue(payload.stat().st_mode & 0o020, 'actual legacy umask002 payload needed')
                if mode == 'copy':
                    self.assertEqual((public / 'agent-memory').stat().st_mode & 0o777, 0o700)
                report = json.loads(w.run([cli, '--json', 'doctor']).stdout)
                self.assertFalse([c for c in report['failures'] if c['component'].startswith('runtime')])
                updated = json.loads(w.run([cli, '--json', 'update']).stdout)
                self.assertTrue(updated['verified'])
                recovered = self.outside(w, 'recover'); self.assertTrue(recovered['verified'])
                removed = json.loads(w.run([cli, '--json', 'uninstall']).stdout)
                self.assertTrue(removed['verified'])

    def test_each_multilink_sensitive_record_refuses_without_changes(self):
        for name in ('receipt.json', 'journal.json', 'owner.json', 'lock'):
            with self.subTest(record=name), World() as w:
                _, _, receipt, _ = self.setup_install(w)
                state = w.paths['state'] / 'agent-memory/install'
                record = state / name; os.link(record, w.root / 'record-alias')
                before = snapshot(w.root)
                value = self.outside(w, 'uninstall', expected=1)
                self.assertFalse(value['verified']); self.assertIn('single-link', value['error'])
                self.assertEqual(snapshot(w.root), before)

    def test_nonprivate_state_blocks_update_removal_recovery_and_doctor(self):
        with World() as w:
            self.setup_install(w)
            state = w.paths['state'] / 'agent-memory/install'; state.chmod(0o777)
            before = snapshot(w.root)
            for args in (('update',), ('uninstall',), ('recover',)):
                value = self.outside(w, *args, expected=1)
                self.assertFalse(value['verified']); self.assertEqual(snapshot(w.root), before)
            doctor = self.outside(w, 'doctor', expected=1)
            self.assertFalse(doctor['ok']); self.assertEqual(snapshot(w.root), before)

    def test_symlinked_state_is_not_read_or_adopted(self):
        with World() as w:
            self.setup_install(w)
            state = w.paths['state'] / 'agent-memory/install'; saved = state.with_name('saved')
            state.rename(saved); state.symlink_to(saved, target_is_directory=True)
            before = snapshot(w.root)
            value = self.outside(w, 'recover', expected=1)
            self.assertIn('symlink', value['error']); self.assertEqual(snapshot(w.root), before)

    def test_linked_git_locator_aliases_refuse_before_git_consumes_config(self):
        for locator in ('gitdir', 'commondir'):
            with self.subTest(locator=locator), World() as w:
                remote, _ = channel(w)
                linked = w.root / 'linked-worktree'
                w.git('worktree', 'add', '-b', 'topic', linked, cwd=remote)
                marker = linked / '.git'
                actual = Path(marker.read_text().strip()[8:])
                alias = w.root / 'metadata-alias'
                if locator == 'gitdir':
                    alias.symlink_to(actual, target_is_directory=True)
                    marker.write_text('gitdir: ' + str(alias) + '\n')
                else:
                    common = (actual / (actual / 'commondir').read_text().strip()).resolve()
                    alias.symlink_to(common, target_is_directory=True)
                    (actual / 'commondir').write_text(str(alias) + '\n')
                canary = w.root / 'git-config-executed'
                monitor = w.root / 'monitor'
                monitor.write_text('#!/bin/sh\n: > ' + str(canary) + '\n')
                monitor.chmod(0o755)
                # Configure the real metadata using its explicit trusted path.
                # If Git consumed the redirected config, status could execute it.
                w.git('config', '--file', remote / '.git/config', 'core.fsmonitor', monitor)
                before = snapshot(w.root)
                value = self.outside(w, 'install', '--check', '--development-source', linked,
                                     '--presets', 'none', expected=1)
                self.assertIn('symlink', value['error']); self.assertFalse(canary.exists())
                self.assertEqual(snapshot(w.root), before)

    def test_actual_linked_worktree_relative_common_pointer_and_native_alias_work(self):
        with World() as w:
            remote, _ = channel(w); linked = w.root / 'linked-worktree'
            w.git('worktree', 'add', '-b', 'topic', linked, cwd=remote)
            marker = linked / '.git'; actual = marker.read_text()
            if sys.platform == 'darwin':
                self.assertIn('/private/tmp/', actual)
                marker.write_text(actual.replace('/private/tmp/', '/tmp/', 1))
            before = snapshot(w.root)
            value = self.outside(w, 'install', '--check', '--development-source', linked,
                                 '--presets', 'none')
            self.assertEqual(value['schema'], 'agent-memory/install-plan-v1')
            self.assertEqual(snapshot(w.root), before)

    def test_normal_git_directory_common_pointer_is_also_checked_before_git(self):
        with World() as w:
            remote, _ = channel(w)
            alias = w.root / 'metadata-alias'; alias.symlink_to(remote / '.git', target_is_directory=True)
            (remote / '.git/commondir').write_text(str(alias) + '\n')
            before = snapshot(w.root)
            value = self.outside(w, 'install', '--check', '--development-source', remote,
                                 '--presets', 'none', expected=1)
            self.assertIn('symlink', value['error']); self.assertEqual(snapshot(w.root), before)

    def test_actual_released_v002_updater_accepts_changed_runtime_under_umask002(self):
        with World() as w:
            fixtures = ROOT / 'tests/fixtures/v0.0.2-runtime'
            provenance = json.loads((fixtures / 'provenance.json').read_text())
            archive = fixtures / provenance['archive']
            self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(), provenance['sha256'])
            remote = w.repository('released-v002-channel')
            with tarfile.open(archive) as stream:
                for member in stream:
                    relative = Path(member.name)
                    self.assertFalse(relative.is_absolute()); self.assertNotIn('..', relative.parts)
                    self.assertTrue(relative == Path('skills') or relative.parts[:2] == ('skills', 'agent-memory'))
                    target = remote / relative
                    if member.isdir():
                        target.mkdir(parents=True, exist_ok=True)
                    else:
                        self.assertTrue(member.isfile()); target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes(stream.extractfile(member).read())
                        target.chmod(0o755 if member.mode & 0o111 else 0o644)
            w.git('add', '.', cwd=remote); w.git('commit', '-m', 'Exact released v0.0.2', cwd=remote)
            old = remote / 'skills/agent-memory/scripts/installer.py'
            initial = w.run([sys.executable, '-I', '-c',
                'import os,subprocess,sys;os.umask(0o022);sys.exit(subprocess.call(sys.argv[1:]))',
                sys.executable, '-I', old, 'install', '--remote', remote.as_uri(), '--ref', 'main', '--presets', 'codex'])
            receipt = json.loads(initial.stdout)['receipt']; cli = Path(receipt['bin_dir']) / 'am'
            self.assertEqual(receipt['source']['version'], '0.0.2')
            self.assertEqual(receipt['source']['artifact_digest'], provenance['checkout_artifact_digest'])
            pack = remote / 'skills/agent-memory'; shutil.rmtree(pack)
            shutil.copytree(ROOT / 'skills/agent-memory', pack)
            skill = pack / 'SKILL.md'; skill.write_text(skill.read_text().replace('"version": "0.0.2"', '"version": "0.0.3"'))
            w.git('add', '.', cwd=remote); w.git('commit', '-m', 'Changed v0.0.3 candidate', cwd=remote)
            upgraded = w.run([sys.executable, '-I', '-c',
                'import os,subprocess,sys;os.umask(0o002);sys.exit(subprocess.call(sys.argv[1:]))',
                cli, '--json', 'update'], timeout=120)
            result = json.loads(upgraded.stdout); self.assertTrue(result['verified'])
            self.assertEqual(result['receipt']['source']['version'], '0.0.3')
            payload = Path(result['receipt']['source']['pack']) / 'SKILL.md'
            self.assertTrue(payload.stat().st_mode & 0o020)
            self.assertTrue(w.run([cli, '--version']).stdout.startswith('am 0.0.3\n'))
            repeated = json.loads(w.run([cli, '--json', 'update']).stdout)
            self.assertTrue(repeated['verified']); self.assertFalse(repeated['changed'])
            self.assertTrue(json.loads(w.run([cli, '--json', 'uninstall']).stdout)['verified'])

    def test_common_metadata_directory_with_real_trailing_space_is_not_rewritten(self):
        with World() as w:
            remote, _ = channel(w); linked = w.root / 'linked-worktree'
            w.git('worktree', 'add', '-b', 'topic', linked, cwd=remote)
            metadata = w.root / 'metadata '
            (remote / '.git').rename(metadata)
            (remote / '.git').write_text('gitdir: ' + str(metadata) + '\n')
            gitdir = metadata / 'worktrees/linked-worktree'
            (linked / '.git').write_text('gitdir: ' + str(gitdir) + '\n')
            (gitdir / 'commondir').write_text(str(metadata) + '\n')
            before = snapshot(w.root)
            value = self.outside(w, 'install', '--check', '--development-source', linked,
                                 '--presets', 'none')
            self.assertEqual(value['schema'], 'agent-memory/install-plan-v1')
            self.assertEqual(snapshot(w.root), before)

    def test_parent_substitution_before_activation_preserves_targets_and_recovers(self):
        with World() as w:
            w.root.chmod(0o755)
            remote, _ = channel(w); public = w.root / 'public-bin'; public.mkdir(mode=0o755)
            control = w.root / 'rendezvous'
            w.env.update(AGENT_MEMORY_FAULT_STAGE='install-before-activate-1',
                         AGENT_MEMORY_FAULT_MODE='pause', AGENT_MEMORY_FAULT_DIR=str(control))
            child = w.spawn([sys.executable, '-I', HELPER, 'install', '--remote', remote.as_uri(),
                             '--ref', 'main', '--presets', 'none', '--bin-dir', public])
            for key in ('AGENT_MEMORY_FAULT_STAGE', 'AGENT_MEMORY_FAULT_MODE', 'AGENT_MEMORY_FAULT_DIR'):
                w.env.pop(key)
            ready = control / 'install-before-activate-1.ready'
            deadline = time.monotonic() + 10
            while not ready.exists() and child.poll() is None and time.monotonic() < deadline:
                time.sleep(0.005)
            self.assertTrue(ready.exists(), 'activation rendezvous not reached')
            saved = w.root / 'original-bin'; public.rename(saved)
            replacement = w.root / 'replacement-bin'; replacement.mkdir(); public.symlink_to(replacement)
            (control / 'install-before-activate-1.release').touch()
            stdout, stderr = child.communicate(timeout=30)
            self.assertEqual(child.returncode, 1, stdout + stderr)
            self.assertFalse((replacement / 'am').exists())
            self.assertFalse((replacement / 'agent-memory').exists())
            public.unlink(); saved.rename(public)
            recovered = self.outside(w, 'recover'); self.assertTrue(recovered['verified'])


if __name__ == '__main__':
    unittest.main()
