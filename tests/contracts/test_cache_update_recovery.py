"""Public opaque-cache recovery with isolated TLS and disposable signing keys.

Only World-copied fixture releases repin the key/endpoints. Production trust is
unchanged. Opaque files are deliberately not parsed, imported or executed.
"""
import json
import os
from pathlib import Path
import shutil
import sys
import time
import unittest

from world import World
from identity_fixtures import bind
from runtime_update_fixtures import bank, owned, receipt_path, snapshot
from signed_release_fixtures import SignedRelease


class CacheUpdateRecoveryContract(unittest.TestCase):
    def install(self, w, release, *, mode='link'):
        release.prepare()
        result = release.install(mode=mode, presets='codex')
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        installed = json.loads(result.stdout)
        self.assertTrue(installed['verified'])
        repo = w.repository('consumer')
        bank(w, repo)
        bind(w, repo)
        (repo/'code.py').write_text('VALUE = 1\n')
        w.git('add', '.', cwd=repo)
        w.git('commit', '-m', 'Disposable consumer', cwd=repo)
        (repo/'code.py').write_text('VALUE = 2  # staged consumer work\n')
        w.git('add', 'code.py', cwd=repo)
        (repo/'untracked.txt').write_text('Preserve unrelated work.\n')
        return installed['receipt'], repo

    def cli(self, w, receipt, *args, expected=0, env=None):
        result = w.run([Path(receipt['bin_dir'])/'am', '--json', *args],
                       env=env, check=False, timeout=180)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return json.loads(result.stdout)

    def opaque(self, source, *, module='frontmatter', raw=b'opaque untrusted evidence\n'):
        parent = source/'skills/agent-memory/scripts/agent_memory'
        self.assertTrue((parent/(module+'.py')).is_file())
        directory = parent/'__pycache__'
        directory.mkdir(mode=0o700, exist_ok=True)
        path = directory/(module+'.cpython-314.pyc')
        path.write_bytes(raw)
        path.chmod(0o600)
        return path

    def backups(self, receipt, source):
        return [Path(item['path']) for item in receipt['backups']
                if item['original_path']==str(source)
                and item.get('kind')=='pre-activation-snapshot']

    def assert_clean(self, w, receipt):
        source = Path(receipt['source']['path'])
        self.assertEqual(list(source.rglob('*.pyc')), [])
        self.assertEqual(list(source.rglob('__pycache__')), [])
        value = self.cli(w, receipt, 'version')
        self.assertTrue(value['ok'])
        self.assertEqual(value['security']['publisher_authentication'], 'verified')
        self.assertTrue(value['security']['signature_verification_performed'])
        self.assertEqual(value['executing_source']['cleanliness'], 'verified_release')
        self.assertFalse(value['executing_source']['dirty'])

    def test_same_version_preview_fresh_acquisition_exact_backup_and_clean_noop(self):
        with World() as w, SignedRelease(w) as release:
            receipt, repo = self.install(w, release)
            source = Path(receipt['source']['path'])
            self.opaque(source)
            before = (snapshot(source), receipt_path(w).read_bytes(), owned(receipt), snapshot(repo))
            preview = self.cli(w, receipt, 'update', '--check')
            self.assertTrue(preview['check_complete'])
            self.assertFalse(preview['verified'])
            self.assertFalse(preview['update_available'])
            self.assertTrue(preview['cache_recovery']['planned'])
            self.assertFalse(preview['cache_recovery']['cache_validity_verified'])
            self.assertFalse(preview['cache_recovery']['cache_code_executed'])
            self.assertEqual((snapshot(source), receipt_path(w).read_bytes(), owned(receipt), snapshot(repo)), before)
            release.requests.clear()
            updated = self.cli(w, receipt, 'update')
            self.assertTrue(updated['verified'])
            self.assertTrue(updated['changed'])
            self.assertTrue(any(p.endswith('/agent-memory.tar.gz') for p in release.requests))
            self.assertTrue(updated['cache_recovery']['old_runtime_preserved'])
            self.assertTrue(updated['cache_recovery']['preserved_source_backups'])
            current = updated['receipt']
            self.assertEqual(current['source']['revision'], receipt['source']['revision'])
            self.assertEqual(current['source']['artifact_digest'], receipt['source']['artifact_digest'])
            self.assertEqual(snapshot(self.backups(current, source)[-1]), before[0])
            self.assertEqual(snapshot(repo), before[3])
            self.assert_clean(w, current)
            after = (snapshot(source), receipt_path(w).read_bytes(), snapshot(repo))
            release.requests.clear()
            noop = self.cli(w, current, 'update')
            self.assertTrue(noop['verified'])
            self.assertFalse(noop['changed'])
            self.assertFalse(any(p.endswith('/agent-memory.tar.gz') for p in release.requests))
            self.assertEqual((snapshot(source), receipt_path(w).read_bytes(), snapshot(repo)), after)

    def test_older_signed_runtime_upgrades_preserving_all_opaque_bytes(self):
        with World() as w, SignedRelease(w) as release:
            receipt, repo = self.install(w, release)
            source = Path(receipt['source']['path'])
            self.opaque(source, raw=b'\x00\xffarbitrary opaque bytes\n')
            dirty = snapshot(source)
            consumer = snapshot(repo)
            candidate = release.advance()
            updated = self.cli(w, receipt, 'update')
            self.assertTrue(updated['verified'])
            current = updated['receipt']
            self.assertEqual(current['source']['revision'], candidate['source_commit'])
            self.assertEqual(current['source']['version'], '0.0.4')
            self.assertEqual(snapshot(self.backups(current, source)[-1]), dirty)
            self.assertEqual(snapshot(repo), consumer)
            self.assert_clean(w, current)

    def test_copy_client_drift_does_not_receive_source_qualification(self):
        with World() as w, SignedRelease(w) as release:
            receipt, repo = self.install(w, release, mode='copy')
            source = Path(receipt['source']['path'])
            self.opaque(source)
            target = Path(next(v['path'] for v in receipt['targets'] if v['kind']=='copy'))
            directory = target/'scripts/agent_memory/__pycache__'
            directory.mkdir(mode=0o700, exist_ok=True)
            (directory/'frontmatter.cpython-314.pyc').write_bytes(b'opaque copied target')
            before = (snapshot(source), receipt_path(w).read_bytes(), owned(receipt), snapshot(repo))
            result = w.run([Path(receipt['bin_dir'])/'am', '--json', 'update'], check=False, timeout=180)
            self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn(str(target), result.stdout + result.stderr)
            self.assertEqual((snapshot(source), receipt_path(w).read_bytes(), owned(receipt), snapshot(repo)), before)

    def test_exact_count_and_byte_limits_allow_recovery_but_next_cache_refuses(self):
        with World() as w, SignedRelease(w) as release:
            receipt, repo = self.install(w, release)
            source = Path(receipt['source']['path'])
            consumer = snapshot(repo)
            modules = sorted((source/'skills/agent-memory/scripts/agent_memory').glob('*.py'))
            names = [(module, tag, suffix) for module in modules
                     for tag in ('314','315','316') for suffix in ('','.opt-1','.opt-2')]
            self.assertGreaterEqual(len(names), 257)
            def populate(count):
                for module, tag, suffix in names[:count]:
                    directory = module.parent/'__pycache__'
                    directory.mkdir(mode=0o700, exist_ok=True)
                    node = directory/(module.stem+'.cpython-'+tag+suffix+'.pyc')
                    node.write_bytes(b'x')
                    node.chmod(0o600)
            populate(256)
            updated = self.cli(w, receipt, 'update')
            self.assertTrue(updated['verified'])
            self.assertEqual(updated['cache_recovery']['cache_count'], 256)
            self.assertEqual(updated['cache_recovery']['cache_bytes'], 256)
            receipt = updated['receipt']
            self.assert_clean(w, receipt)
            populate(257)
            before = (snapshot(source), receipt_path(w).read_bytes(), owned(receipt), snapshot(repo))
            self.cli(w, receipt, 'update', expected=1)
            self.assertEqual((snapshot(source), receipt_path(w).read_bytes(), owned(receipt), snapshot(repo)), before)
            self.assertEqual(snapshot(repo), consumer)
        with World() as w, SignedRelease(w) as release:
            receipt, repo = self.install(w, release)
            source = Path(receipt['source']['path'])
            consumer = snapshot(repo)
            modules = sorted((source/'skills/agent-memory/scripts/agent_memory').glob('*.py'))[:8]
            self.assertEqual(len(modules), 8)
            for module in modules:
                self.opaque(source, module=module.stem, raw=b'x'*(2*1024*1024))
            dirty = snapshot(source)
            updated = self.cli(w, receipt, 'update')
            self.assertTrue(updated['verified'])
            self.assertEqual(updated['cache_recovery']['cache_bytes'], 16*1024*1024)
            self.assertEqual(snapshot(self.backups(updated['receipt'], source)[-1]), dirty)
            self.assertEqual(snapshot(repo), consumer)
            self.assert_clean(w, updated['receipt'])

    def test_unqualified_payload_cache_types_and_bounds_preserve_every_target(self):
        with World() as w, SignedRelease(w) as release:
            receipt, repo = self.install(w, release)
            source = Path(receipt['source']['path'])
            baseline = w.inside('clean-source-baseline')
            shutil.copytree(source, baseline)
            def changed_payload():
                node = source/'skills/agent-memory/templates/router-proposal.example.json'
                node.write_bytes(node.read_bytes()+b' ')
            def missing_payload():
                (source/'skills/agent-memory/templates/router-proposal.example.json').unlink()
            def wrong_name():
                node = self.opaque(source)
                node.rename(node.parent/'not-a-cache.pyc')
            def wrong_path():
                (source/'skills/agent-memory/unrelated.pyc').write_bytes(b'preserve unrelated bytes')
            def hardlink():
                node = self.opaque(source)
                target = w.inside('hardlink-evidence')
                target.unlink(missing_ok=True)
                os.link(node, target)
            def symlink():
                node = self.opaque(source)
                node.unlink()
                node.symlink_to(w.inside('missing-unrelated-file'))
            def directory_mode():
                self.opaque(source).parent.chmod(0o777)
            def cache_mode():
                self.opaque(source).chmod(0o755)
            def per_file():
                self.opaque(source, raw=b'x'*(2*1024*1024+1))
            def aggregate():
                modules = sorted((source/'skills/agent-memory/scripts/agent_memory').glob('*.py'))[:9]
                self.assertEqual(len(modules), 9)
                for index, module in enumerate(modules):
                    self.opaque(source, module=module.stem,
                                raw=b'x'*(2*1024*1024 if index<8 else 1))
            def manifest():
                node = source/'release-manifest.json'
                node.write_bytes(node.read_bytes()+b' ')
            cases = {'changed-payload':changed_payload, 'missing-payload':missing_payload,
                     'wrong-name':wrong_name, 'wrong-path':wrong_path, 'hardlink':hardlink,
                     'symlink':symlink, 'directory-mode':directory_mode, 'cache-mode':cache_mode,
                     'per-file-bound':per_file, 'aggregate-bound':aggregate, 'signed-metadata':manifest}
            for label, mutate in cases.items():
                with self.subTest(label=label):
                    self.assertTrue(source.is_relative_to(w.root))
                    shutil.rmtree(source)
                    shutil.copytree(baseline, source)
                    mutate()
                    before = (snapshot(source), receipt_path(w).read_bytes(), owned(receipt), snapshot(repo))
                    # Independently fetched bootstrap code diagnoses even missing payload;
                    # no damaged installed source is imported as its own verifier.
                    refused = release.install(presets='codex')
                    self.assertNotEqual(refused.returncode, 0, refused.stdout + refused.stderr)
                    self.assertEqual((snapshot(source), receipt_path(w).read_bytes(), owned(receipt), snapshot(repo)), before)

    def test_interrupted_activation_preserves_dirty_tree_as_pending_then_trusted_retry(self):
        with World() as w, SignedRelease(w) as release:
            receipt, repo = self.install(w, release)
            source = Path(receipt['source']['path'])
            self.opaque(source)
            dirty = snapshot(source)
            consumer = snapshot(repo)
            release.advance()
            interrupted = w.run([Path(receipt['bin_dir'])/'am', '--json', 'update'],
                                check=False, timeout=180,
                                env={'AGENT_MEMORY_FAULT_STAGE':'update-after-activate-0',
                                     'AGENT_MEMORY_FAULT_MODE':'exit'})
            self.assertEqual(interrupted.returncode, 86, interrupted.stdout + interrupted.stderr)
            recovered = w.run([sys.executable, '-I', '-B',
                               release.root/'skills/agent-memory/scripts/installer.py', 'recover'],
                              check=False, timeout=180)
            if recovered.stdout:
                response = json.loads(recovered.stdout)
                if snapshot(source)==dirty:
                    self.assertFalse(response.get('verified', False), response)
            self.assertEqual(snapshot(repo), consumer)
            self.assertTrue(any(snapshot(path)==dirty for path in source.parent.iterdir()
                                if path.is_dir() and not path.is_symlink()))
            retried = release.install(presets='codex')
            self.assertEqual(retried.returncode, 0, retried.stdout + retried.stderr)
            current = json.loads(retried.stdout)['receipt']
            self.assertEqual(current['source']['version'], '0.0.4')
            self.assertEqual(snapshot(repo), consumer)
            self.assert_clean(w, current)

    def test_concurrent_cache_writer_is_preserved_then_source_only_retry_converges(self):
        with World() as w, SignedRelease(w) as release:
            receipt, repo = self.install(w, release)
            source = Path(receipt['source']['path'])
            cache = self.opaque(source)
            consumer = snapshot(repo)
            rendezvous = w.inside('cache-writer-rendezvous')
            rendezvous.mkdir(mode=0o700)
            phase = 'update-before-activate-0'
            w.env.update(AGENT_MEMORY_FAULT_STAGE=phase, AGENT_MEMORY_FAULT_MODE='pause',
                         AGENT_MEMORY_FAULT_DIR=str(rendezvous))
            child = w.spawn([Path(receipt['bin_dir'])/'am', '--json', 'update'])
            ready = rendezvous/(phase+'.ready')
            deadline = time.monotonic()+20
            while not ready.exists() and child.poll() is None and time.monotonic()<deadline:
                time.sleep(.01)
            self.assertTrue(ready.exists(), 'Update did not reach its pre-activation boundary')
            changed = b'opaque concurrent-writer evidence after qualification\n'
            cache.write_bytes(changed)
            (rendezvous/(phase+'.release')).touch()
            stdout, stderr = child.communicate(timeout=120)
            self.assertNotEqual(child.returncode, 0, stdout + stderr)
            for key in ('AGENT_MEMORY_FAULT_STAGE','AGENT_MEMORY_FAULT_MODE','AGENT_MEMORY_FAULT_DIR'):
                w.env.pop(key)
            self.assertTrue(any(path.read_bytes()==changed for path in source.parent.rglob(cache.name)
                                if path.is_file() and not path.is_symlink()))
            self.assertEqual(snapshot(repo), consumer)
            retried = release.install(presets='codex')
            self.assertEqual(retried.returncode, 0, retried.stdout + retried.stderr)
            current = json.loads(retried.stdout)['receipt']
            self.assert_clean(w, current)
            self.assertEqual(snapshot(repo), consumer)

    def test_late_signed_payload_or_other_target_change_never_receives_race_qualification(self):
        for variant in ('signed-payload', 'other-wrapper'):
            with self.subTest(variant=variant), World() as w, SignedRelease(w) as release:
                receipt, repo = self.install(w, release)
                source = Path(receipt['source']['path'])
                self.opaque(source)
                consumer = snapshot(repo)
                rendezvous = w.inside('unknown-writer-rendezvous')
                rendezvous.mkdir(mode=0o700)
                phase = 'update-before-activate-0'
                w.env.update(AGENT_MEMORY_FAULT_STAGE=phase, AGENT_MEMORY_FAULT_MODE='pause',
                             AGENT_MEMORY_FAULT_DIR=str(rendezvous))
                child = w.spawn([Path(receipt['bin_dir'])/'am', '--json', 'update'])
                ready = rendezvous/(phase+'.ready')
                deadline = time.monotonic()+20
                while not ready.exists() and child.poll() is None and time.monotonic()<deadline:
                    time.sleep(.01)
                self.assertTrue(ready.exists(), 'Update did not reach its pre-activation boundary')
                if variant=='signed-payload':
                    changed = source/'skills/agent-memory/templates/router-proposal.example.json'
                    payload = changed.read_bytes()+b'\nUnsigned concurrent payload change.\n'
                else:
                    changed = Path(receipt['bin_dir'])/'agent-memory'
                    payload = changed.read_bytes()+b'\n# Concurrent external wrapper change.\n'
                changed.write_bytes(payload)
                (rendezvous/(phase+'.release')).touch()
                stdout, stderr = child.communicate(timeout=120)
                self.assertNotEqual(child.returncode, 0, stdout + stderr)
                for key in ('AGENT_MEMORY_FAULT_STAGE','AGENT_MEMORY_FAULT_MODE','AGENT_MEMORY_FAULT_DIR'):
                    w.env.pop(key)
                self.assertEqual(changed.read_bytes(), payload)
                self.assertEqual(snapshot(repo), consumer)
                retried = release.install(presets='codex')
                self.assertNotEqual(retried.returncode, 0, retried.stdout + retried.stderr)
                self.assertEqual(changed.read_bytes(), payload)
                self.assertEqual(snapshot(repo), consumer)
