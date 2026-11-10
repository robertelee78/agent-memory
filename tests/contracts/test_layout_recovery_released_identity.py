"""Released identity commands must not mistake a public layout hold for a free bank."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from runtime_update_fixtures import PACK, ROOT, snapshot
import layout_recovery_fixtures as recovery
import v014_runtime_fixture


class ReleasedIdentityHoldContract(unittest.TestCase):
    def test_released_identity_status_preserves_public_hold(self):
        with World() as legacy, World() as world:
            pack = v014_runtime_fixture.extract(legacy.root)
            installed = legacy.run([sys.executable, '-I', pack / 'scripts/installer.py', 'install',
                                    '--development-source', pack, '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['source']['version'], '0.0.14')
            self.assertEqual(receipt['source']['artifact_digest'],
                             '4208c18e2288bdb3f6faf6354fe44b638559ba8ec1fede2cec7ce491bea726f7')
            old = Path(receipt['bin_dir']) / 'am'
            command, source = recovery.installed(world)
            repo, bank, native = recovery.memory(world)
            status = world.run([old, 'identity', 'status', '--repo', repo], check=False, timeout=60)
            self.assertTrue(json.loads(status.stdout)['resolved'], status.stdout + status.stderr)
            (bank / 'handoff-prompt.md').write_text(recovery.policy_body(0))
            (repo / 'synthetic-policy.md').write_text(recovery.policy_body(0))
            _, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            applied = recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertFalse(applied['semantic_recovery_complete'])
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            before = snapshot(bank), snapshot(directory), snapshot(world.paths['config'])
            status = world.run([old, 'identity', 'status', '--repo', repo], check=False, timeout=60)
            value = json.loads(status.stdout)
            self.assertFalse(value['resolved'])
            self.assertFalse(value['writes_performed'])
            self.assertEqual((snapshot(bank), snapshot(directory), snapshot(world.paths['config'])), before)
            current = recovery.call(world, command, 'status', '--repo', repo,
                                    '--operation', inspected['operation_id'])
            self.assertFalse(current['semantic_recovery_complete'])
            self.assertEqual(set(current['pending_unit_ids']), set(applied['pending_unit_ids']))

    def test_incapable_current_runtime_cannot_apply_reviewed_layout(self):
        with World() as world, World() as capable:
            for directory in [world.root, *(p for p in world.root.rglob('*') if p.is_dir())]:
                if directory.stat().st_gid != os.getegid():
                    os.chown(directory, -1, os.getegid())
            channel = world.repository('incapable channel')
            pack = channel / 'skills/agent-memory'
            shutil.copytree(PACK, pack)
            compatibility_path = pack / 'schemas/runtime-compatibility.json'
            compatibility = json.loads(compatibility_path.read_text())
            compatibility['private_state_capabilities'].remove('layout-frontier-v1')
            compatibility_path.write_text(json.dumps(compatibility, indent=2) + '\n')
            world.git('add', '.', cwd=channel)
            world.git('commit', '-m', 'Current runtime without layout frontier capability', cwd=channel)
            installed = world.run([sys.executable, '-I', ROOT / 'scripts/installer.py', 'install',
                                   '--remote', channel, '--ref', 'main', '--mode', 'copy',
                                   '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertNotIn('layout-frontier-v1',
                             receipt['source']['compatibility'].get('private_state_capabilities', []))
            incapable = Path(receipt['bin_dir']) / 'am'
            command, source = recovery.installed(capable)
            repo, bank, native = recovery.memory(world)
            (bank / 'handoff-prompt.md').write_text(recovery.policy_body(0))
            _, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            before = (snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                      snapshot(world.paths['state']))
            refused = world.run([incapable, 'bank', 'recover-layout', 'apply', '--repo', repo,
                                 '--proposal', proposal], check=False, timeout=60)
            self.assertNotEqual(refused.returncode, 0, refused.stdout + refused.stderr)
            result = json.loads(refused.stdout)
            self.assertEqual(result['failure_code'], 'pending-runtime-compatibility')
            self.assertFalse(result['writes_performed'])
            self.assertFalse(result['canonical_writes_performed'])
            self.assertEqual((snapshot(bank), snapshot(directory), snapshot(world.paths['config']),
                              snapshot(world.paths['state'])), before)

    def test_released_managed_update_refuses_real_public_hold_without_activation(self):
        with World() as world, World() as capable:
            for directory in [world.root, *(p for p in world.root.rglob('*') if p.is_dir())]:
                if directory.stat().st_gid != os.getegid():
                    os.chown(directory, -1, os.getegid())
            channel = world.repository('released-channel')
            pack = v014_runtime_fixture.extract(channel)
            world.git('add', '.', cwd=channel)
            world.git('commit', '-m', 'Published v0.0.14 fixture', cwd=channel)
            installed = world.run([sys.executable, '-I', pack / 'scripts/installer.py', 'install',
                                   '--remote', channel, '--ref', 'main', '--mode', 'copy',
                                   '--presets', 'none'], timeout=60)
            receipt = json.loads(installed.stdout)['receipt']
            self.assertEqual(receipt['source']['version'], '0.0.14')
            released_files = json.loads((v014_runtime_fixture.FIXTURE / 'provenance.json').read_bytes())['files']
            installed_pack = Path(receipt['source']['pack'])
            for name, facts in released_files.items():
                installed_file = installed_pack.joinpath(*Path(name).parts[2:])
                self.assertEqual(hashlib.sha256(installed_file.read_bytes()).hexdigest(), facts['sha256'], name)
            old = Path(receipt['bin_dir']) / 'am'
            command, source = recovery.installed(capable)
            repo, bank, native = recovery.memory(world)
            (bank / 'handoff-prompt.md').write_text(recovery.policy_body(0))
            _, inspected = recovery.inspect(world, command, repo)
            proposal, _ = recovery.prepare(world, command, source, repo, inspected, native)
            applied = recovery.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertFalse(applied['semantic_recovery_complete'])
            directory = recovery.private_directory(world, bank, inspected['operation_id'])
            runtime = Path(receipt['source']['path'])
            receipt_file = world.paths['state'] / 'agent-memory/install/receipt.json'
            before = snapshot(bank), snapshot(directory), snapshot(runtime), receipt_file.read_bytes()
            for args in (['--check'], []):
                result = world.run([old, '--json', 'update', '--repo', repo, *args],
                                   cwd=repo, check=False, timeout=60)
                self.assertNotEqual(result.returncode, 0, result.stdout + result.stderr)
                self.assertFalse(json.loads(result.stdout)['verified'])
                self.assertEqual((snapshot(bank), snapshot(directory), snapshot(runtime),
                                  receipt_file.read_bytes()), before)
