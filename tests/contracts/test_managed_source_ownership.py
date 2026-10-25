"""Real legacy-index drift and ownership refusals, never live install targets."""
import hashlib
import json
from pathlib import Path
import shutil
import sys
import tarfile
import unittest
import zlib

from world import World
from runtime_update_fixtures import ROOT, PACK, fixture, advance, call, snapshot, receipt_path, bank


class ManagedSourceOwnershipContract(unittest.TestCase):
    def drift(self, world, receipt):
        root = Path(receipt['source']['path'])
        before = (root/'.git/index').read_bytes()
        result = world.git('status', '--porcelain', '--untracked-files=all', cwd=root)
        self.assertEqual(result.stdout, '')
        self.assertNotEqual((root/'.git/index').read_bytes(), before)
        return root

    def test_clean_index_refresh_previews_updates_and_repeats_preserving_old_source_and_bank(self):
        with World() as world:
            channel, pack, receipt = fixture(world)
            memory = bank(world, world.paths['repo']); memory_before = snapshot(memory)
            code = self.drift(world, receipt)
            old_source = snapshot(code)
            revision = advance(world, channel, pack)
            before = snapshot(world.root)
            _, preview = call(world, receipt, '--check')
            self.assertTrue(preview['check_complete']); self.assertEqual(snapshot(world.root), before)
            _, updated = call(world, receipt)
            self.assertEqual(updated['receipt']['source']['revision'], revision)
            backups = [Path(b['path']) for b in updated['receipt']['backups']
                       if b['original_path'] == str(code) and b.get('kind') == 'pre-activation-snapshot']
            self.assertTrue(backups); self.assertEqual(snapshot(backups[-1]), old_source)
            self.assertFalse(call(world, updated['receipt'])[1]['changed'])
            self.assertEqual(snapshot(memory), memory_before)

    def test_actual_v002_runtime_failure_is_repaired_only_by_outside_helper(self):
        with World() as world:
            channel = world.repository('frozen v002 channel')
            provenance = json.loads((ROOT/'tests/fixtures/v0.0.2-runtime/provenance.json').read_text())
            archive = ROOT/'tests/fixtures/v0.0.2-runtime'/provenance['archive']
            self.assertEqual(hashlib.sha256(archive.read_bytes()).hexdigest(), provenance['sha256'])
            with tarfile.open(archive) as stream:
                for member in stream:
                    relative = Path(member.name)
                    self.assertFalse(relative.is_absolute() or '..' in relative.parts)
                    self.assertTrue(relative == Path('skills') or relative.parts[:2] == ('skills', 'agent-memory'))
                    target = channel/relative
                    if member.isdir(): target.mkdir(parents=True, exist_ok=True)
                    else:
                        self.assertTrue(member.isfile())
                        target.parent.mkdir(parents=True, exist_ok=True)
                        target.write_bytes(stream.extractfile(member).read())
                        target.chmod(0o755 if member.mode & 0o111 else 0o644)
            world.git('add', '.', cwd=channel); world.git('commit', '-m', 'Frozen v0.0.2', cwd=channel)
            old_helper = channel/'skills/agent-memory/scripts/installer.py'
            # Reproduce the published checkout's exact modes on either platform,
            # independently of the Linux user's usual collaborative umask.
            result = world.run([sys.executable, '-I', '-c',
                                'import os,subprocess,sys;os.umask(0o022);sys.exit(subprocess.call(sys.argv[1:]))',
                                sys.executable, '-I', old_helper, 'install', '--remote', channel.as_uri(),
                                '--presets', 'none'])
            receipt = json.loads(result.stdout)['receipt']
            self.assertEqual(receipt['source']['artifact_digest'], provenance['checkout_artifact_digest'])
            self.drift(world, receipt)
            failed = world.run([Path(receipt['bin_dir'])/'am', 'update', '--check'], check=False)
            self.assertEqual(failed.returncode, 1); self.assertIn('Installed target readback failed', failed.stderr)
            shutil.rmtree(channel/'skills/agent-memory')
            shutil.copytree(PACK, channel/'skills/agent-memory')
            world.git('add', '.', cwd=channel); world.git('commit', '-m', 'Repair runtime', cwd=channel)
            _, preview = call(world, receipt, '--check', source=True)
            self.assertTrue(preview['check_complete'])
            _, repaired = call(world, receipt, source=True)
            self.assertTrue(repaired['verified'])
            self.assertFalse(call(world, repaired['receipt'])[1]['changed'])

    def test_hidden_tracked_changes_extra_ignored_files_modes_and_channel_are_preserved_and_refused(self):
        for case in ('hidden-by-index', 'skip-worktree', 'staged-only', 'staged-deletion', 'ignored-extra', 'empty-directory',
                     'executable', 'package-mode', 'root-mode', 'origin', 'head', 'wrapper',
                     'forged-commit', 'forged-tree', 'alternate-objects'):
            with self.subTest(case=case), World() as world:
                _, _, receipt = fixture(world)
                code = self.drift(world, receipt)
                target = code/'skills/agent-memory/SKILL.md'
                if case in ('hidden-by-index', 'skip-worktree'):
                    flag = '--assume-unchanged' if case == 'hidden-by-index' else '--skip-worktree'
                    world.git('update-index', flag, str(target.relative_to(code)), cwd=code)
                    target.write_text(target.read_text() + '\nChanged local instructions.\n')
                elif case == 'staged-only':
                    raw = target.read_bytes()
                    target.write_bytes(raw + b'\nStaged local instructions.\n')
                    world.git('add', str(target.relative_to(code)), cwd=code)
                    target.write_bytes(raw)
                elif case == 'staged-deletion':
                    world.git('rm', '--cached', str(target.relative_to(code)), cwd=code)
                elif case == 'ignored-extra':
                    (code/'.git/info/exclude').write_text('local-kept-note\n')
                    (code/'local-kept-note').write_text('User data must survive.\n')
                elif case == 'empty-directory': (code/'user-empty-directory').mkdir()
                elif case == 'executable': target.chmod(0o755)
                elif case == 'package-mode': target.chmod(0o600)
                elif case == 'root-mode': code.chmod(0o750)
                elif case == 'origin': world.git('remote', 'set-url', 'origin', '/different-channel', cwd=code)
                elif case == 'head': world.git('checkout', '--detach', cwd=code)
                elif case.startswith('forged-'):
                    kind = case.split('-')[1]
                    oid = receipt['source']['revision'] if kind == 'commit' else world.git(
                        'rev-parse', 'HEAD^{tree}', cwd=code).stdout.strip()
                    raw = world.git('cat-file', kind, oid, cwd=code).stdout.encode() if kind == 'commit' else None
                    object_path = code/'.git/objects'/oid[:2]/oid[2:]
                    # Fixture objects are loose. Modify valid raw content while
                    # retaining its false filename; Git cat-file alone accepts it.
                    framed = zlib.decompress(object_path.read_bytes())
                    header, body = framed.split(b'\0', 1)
                    body = body[:-1] + bytes([body[-1] ^ 1])
                    object_path.chmod(0o600)
                    object_path.write_bytes(zlib.compress(header + b'\0' + body))
                elif case == 'alternate-objects':
                    (code/'.git/objects/info/alternates').write_text(str(world.paths['repo']/'.git/objects')+'\n')
                else:
                    wrapper = Path(receipt['bin_dir'])/'am'
                    wrapper.write_text(wrapper.read_text() + '\n# User modification\n')
                before = snapshot(world.root)
                call(world, receipt, '--check', expected=1, source=True)
                self.assertEqual(snapshot(world.root), before)

    def test_local_ownership_does_not_execute_fsmonitor_or_filters_or_follow_ambient_git_directory(self):
        with World() as world:
            _, _, receipt = fixture(world)
            code = self.drift(world, receipt)
            marker = world.inside('must-not-exist')
            trap = world.inside('git-trap.sh')
            trap.write_text('#!/bin/sh\nprintf unsafe > "'+str(marker)+'"\n'); trap.chmod(0o700)
            attributes = world.inside('external-attributes')
            attributes.write_text('* filter=unexpected\n')
            world.git('config', 'core.fsmonitor', str(trap), cwd=code)
            world.git('config', 'core.attributesFile', str(attributes), cwd=code)
            world.git('config', 'filter.unexpected.clean', str(trap), cwd=code)
            world.env.update(GIT_DIR=str(world.paths['repo']/'.git'),
                             GIT_INDEX_FILE=str(world.inside('wrong-index')))
            before = snapshot(world.root)
            _, preview = call(world, receipt, '--check', source=True)
            self.assertTrue(preview['check_complete']); self.assertFalse(marker.exists())
            cli = Path(receipt['bin_dir'])/'am'
            version = json.loads(world.run([cli, '--json', 'version']).stdout)
            self.assertEqual(version['executing_source']['cleanliness'], 'clean')
            doctor = json.loads(world.run([cli, '--json', 'doctor', '--repo', world.paths['repo']]).stdout)
            self.assertTrue(doctor['ok']); self.assertFalse(marker.exists())
            self.assertEqual(snapshot(world.root), before)

    def test_interrupted_switch_restores_exact_drifted_source_and_retries(self):
        with World() as world:
            channel, pack, receipt = fixture(world)
            memory = bank(world, world.paths['repo']); memory_before = snapshot(memory)
            code = self.drift(world, receipt); before = snapshot(code)
            advance(world, channel, pack)
            call(world, receipt, expected=86, env={'AGENT_MEMORY_FAULT_STAGE': 'update-after-activate-0',
                                                  'AGENT_MEMORY_FAULT_MODE': 'exit'})
            result = world.run([sys.executable, '-I', PACK/'scripts/installer.py', 'recover'])
            self.assertTrue(json.loads(result.stdout)['verified'])
            self.assertEqual(snapshot(code), before); self.assertEqual(snapshot(memory), memory_before)
            self.assertTrue(call(world, receipt)[1]['verified'])

    def test_doctor_uses_the_same_legacy_ownership_check(self):
        with World() as world:
            _, _, receipt = fixture(world)
            code = self.drift(world, receipt)
            command = [sys.executable, '-I', PACK/'scripts/diagnose.py', 'doctor', '--json']
            before = snapshot(world.root)
            result = json.loads(world.run(command).stdout)
            source = next(c for c in result['checks'] if c['component'] == 'runtime.source')
            self.assertEqual(source['state'], 'pass'); self.assertIn('raw directory identity', source['detail'])
            self.assertEqual(snapshot(world.root), before)
            (code/'.git/info/exclude').write_text('kept\n'); (code/'kept').write_text('Keep this.\n')
            failed = world.run(command, check=False)
            self.assertEqual(failed.returncode, 1)
            self.assertTrue(any(c['component'] == 'runtime.source' and c['state'] == 'failure'
                                for c in json.loads(failed.stdout)['checks']))
