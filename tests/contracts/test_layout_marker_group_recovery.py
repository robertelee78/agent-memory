"""Installed marker-temp recovery with real, reproducible parent-group ownership.

Requires an eligible supplementary non-primary group; a missing prerequisite is
an explicit failure, never a skipped ownership regression or synthetic stat.
"""
import json
import os
import stat
import sys
import unittest
import uuid

from world import World
from identity_fixtures import bind
import layout_recovery_fixtures as f
from runtime_update_fixtures import ROOT
from contracts.test_layout_recovery_darwin_provenance import set_xattr, xattrs


class LayoutMarkerGroupRecoveryContract(unittest.TestCase):
    def metadata(self, path):
        facts = path.stat(follow_symlinks=False)
        return facts.st_uid, facts.st_gid, stat.S_IMODE(facts.st_mode), xattrs(path)

    def snapshot(self, root):
        return {path.relative_to(root).as_posix():
                (path.read_bytes() if path.is_file() else None, self.metadata(path))
                for path in root.rglob('*')}

    def setup_repair(self, world, *, setgid=False):
        command, source = f.installed(world)
        repo = world.repository()
        bank = repo / 'memory-bank'
        bank.mkdir()
        # Select ownership on the empty bank, after installed() has normalized
        # World directories and before any original bank/source is created.
        groups = sorted(set(os.getgroups()) - {os.getegid()})
        errors = []
        for group in groups:
            try:
                os.chown(bank, -1, group)
                break
            except OSError as exc:
                errors.append((group, str(exc)))
        else:
            raise RuntimeError('Marker-group recovery prerequisite: an eligible supplementary '
                               'non-primary group is required; groups=' + repr(groups)
                               + ', chown failures=' + repr(errors))
        bank.chmod(0o2755 if setgid else 0o755)
        self.assertEqual(bank.stat().st_gid, group)
        self.assertNotEqual(group, os.getegid())
        self.assertEqual(bool(bank.stat().st_mode & stat.S_ISGID), setgid)
        request = {'project-id': '8e0b66aa-2be5-4fa3-b47f-1245474aa7dd',
                   'router-updated': '2026-10-22T17:00:00Z',
                   'contexts': {'activeContext.md': {
                       'body': '# Active context\n\nPreserve synthetic source evidence before recovery.\n',
                       'purpose': 'Orient the agent', 'scope': 'repository',
                       'lifecycle': 'active', 'read-when': 'When resuming work'}},
                   'omitted': {name: 'No applicable evidence yet' for name in
                               ('projectbrief.md', 'productContext.md', 'systemPatterns.md',
                                'techContext.md', 'progress.md')}}
        rendered = world.run([sys.executable, '-I', '-B', ROOT / 'scripts/bank_contract.py',
                              'render', '--input', f.save(world, request)])
        native = json.loads(rendered.stdout)['files']
        for path, body in native.items():
            (bank / path).write_text(body)
        bind(world, repo)
        active = bank / 'activeContext.md'
        active.write_text(native['activeContext.md'] + '\n' + native['activeContext.md'])
        (bank / 'handoff-prompt.md').write_text(f.policy_body(0))
        (repo / 'synthetic-policy.md').write_text(f.policy_body(0))
        expected_gid = group if sys.platform != 'linux' or setgid else os.getegid()
        originals = self.snapshot(bank)
        self.assertTrue(originals)
        for _, metadata in originals.values():
            self.assertEqual(metadata[1], expected_gid)
        feature = repo / 'feature.txt'
        feature.write_text('Committed feature\n')
        world.git('add', 'feature.txt', cwd=repo)
        world.git('commit', '-m', 'Synthetic feature baseline', cwd=repo)
        feature.write_text('Staged feature\n')
        world.git('add', 'feature.txt', cwd=repo)
        feature.write_text('Unstaged feature\n')
        (repo / 'untracked.txt').write_bytes(b'Untracked feature\n')
        git = (repo / '.git/index').read_bytes(), world.git('rev-parse', 'HEAD', cwd=repo).stdout
        return command, source, repo, bank, native, originals, git, expected_gid

    def assert_work_preserved(self, world, repo, git):
        self.assertEqual((repo / '.git/index').read_bytes(), git[0])
        self.assertEqual(world.git('rev-parse', 'HEAD', cwd=repo).stdout, git[1])
        self.assertEqual((repo / 'feature.txt').read_text(), 'Unstaged feature\n')
        self.assertEqual((repo / 'untracked.txt').read_bytes(), b'Untracked feature\n')

    def assert_private_preserved(self, directory, saved):
        for path, (body, metadata) in saved.items():
            target = directory / path
            self.assertEqual(target.read_bytes() if target.is_file() else None, body)
            self.assertEqual(self.metadata(target), metadata)

    def assert_sources(self, bank, wanted, originals, root_metadata):
        self.assertEqual({path.relative_to(bank).as_posix(): path.read_bytes()
                          for path in bank.rglob('*') if path.is_file()
                          and not path.name.startswith('.agent-memory')}, wanted)
        for path in wanted:
            self.assertEqual(self.metadata(bank / path), originals[path][1])
        self.assertEqual(self.metadata(bank), root_metadata)
        self.assertFalse(list(bank.glob('.agent-memory-layout-marker-*.tmp')))

    def interrupt(self, world, command, repo, action, proposal, bank, operation, gid):
        failed = world.run([command, 'bank', 'recover-layout', action, '--repo', repo,
                            '--proposal', proposal], check=False, timeout=60,
                           env={'AGENT_MEMORY_FAULT_STAGE': 'layout-recovery-before-marker-swap',
                                'AGENT_MEMORY_FAULT_MODE': 'exit'})
        self.assertEqual(failed.returncode, 86, failed.stdout + failed.stderr)
        temporary = bank / ('.agent-memory-layout-marker-' + operation + '.tmp')
        self.assertTrue(temporary.is_file())
        facts = temporary.stat()
        self.assertEqual(facts.st_uid, os.geteuid())
        self.assertEqual(facts.st_gid, gid)
        self.assertEqual(stat.S_IMODE(facts.st_mode), 0o600)
        self.assertEqual(json.loads(temporary.read_bytes())['operation_id'], operation)
        before = temporary.read_bytes(), self.metadata(temporary)
        f.unlock(world, command, repo)
        self.assertEqual((temporary.read_bytes(), self.metadata(temporary)), before)
        return temporary

    def known_recovery(self, world, command, repo, operation, mode, temporary):
        preview = world.inside('known-' + str(uuid.uuid4()) + '.json')
        before = temporary.read_bytes(), self.metadata(temporary)
        f.call(world, command, 'recover', '--repo', repo, '--operation', operation,
               '--mode', mode, '--output', preview)
        self.assertEqual((temporary.read_bytes(), self.metadata(temporary)), before)
        pins = json.loads(preview.read_text())['proposal']['temporary_entries']
        self.assertEqual(len(pins), 1)
        self.assertEqual(pins[0]['path'], temporary.name)
        self.assertEqual(pins[0]['facts']['gid'], temporary.stat().st_gid)
        self.assertEqual(pins[0]['sha256'], f.sha(temporary.read_bytes()))
        applied = world.inside('known-applied-' + str(uuid.uuid4()) + '.json')
        outcome = f.call(world, command, 'recover', '--repo', repo, '--proposal', preview,
                         '--output', applied)
        self.assertTrue(outcome['verified'])
        self.assertFalse(outcome['semantic_recovery_complete'])
        self.assertGreater(len(json.loads(applied.read_text())['receipt']['pending_unit_ids_at_commit']), 0)
        self.assertFalse(temporary.exists())
        return preview

    def apply_recovery(self, *, setgid):
        for mode in ('finish', 'rollback'):
            with self.subTest(setgid=setgid, mode=mode), World() as world:
                command, source, repo, bank, native, originals, git, gid = self.setup_repair(world, setgid=setgid)
                root_metadata = self.metadata(bank)
                safe, inspected = f.inspect(world, command, repo)
                proposal, _ = f.prepare(world, command, source, repo, inspected, native)
                operation = safe['operation_id']
                directory = f.private_directory(world, bank, operation)
                private = self.snapshot(directory)
                self.assertEqual({row['path']: (directory / row['blob']).read_bytes()
                                  for row in inspected['manifest']['sources']},
                                 {path: body for path, (body, _) in originals.items()})
                temporary = self.interrupt(world, command, repo, 'apply', proposal, bank, operation, gid)
                self.assert_work_preserved(world, repo, git)
                preview = self.known_recovery(world, command, repo, operation, mode, temporary)
                wanted = ({path: body.encode() for path, body in native.items()} if mode == 'finish'
                          else {path: body for path, (body, _) in originals.items()})
                self.assert_sources(bank, wanted, originals, root_metadata)
                self.assert_private_preserved(directory, private)
                self.assertTrue((bank / '.agent-memory-transaction.md').is_file())
                self.assertTrue((bank / '.agent-memory-recovery.md').is_file())
                before = self.snapshot(bank), self.snapshot(directory.parent)
                historical = f.call(world, command, 'recover', '--repo', repo, '--proposal', preview)
                self.assertTrue(historical['receipt_is_historical'])
                self.assertEqual((self.snapshot(bank), self.snapshot(directory.parent)), before)
                self.assert_work_preserved(world, repo, git)

    def test_ordinary_parent_marker_temp_known_finish_and_rollback(self):
        self.apply_recovery(setgid=False)

    def test_setgid_parent_marker_temp_known_finish_and_rollback(self):
        self.apply_recovery(setgid=True)

    def test_child_restore_marker_temp_known_finish(self):
        for setgid in (False, True):
            with self.subTest(setgid=setgid), World() as world:
                command, source, repo, bank, native, originals, git, gid = self.setup_repair(world, setgid=setgid)
                root_metadata = self.metadata(bank)
                safe, inspected = f.inspect(world, command, repo)
                proposal, _ = f.prepare(world, command, source, repo, inspected, native)
                parent = safe['operation_id']
                directory = f.private_directory(world, bank, parent)
                private = self.snapshot(directory)
                applied = f.call(world, command, 'apply', '--repo', repo, '--proposal', proposal)
                child_file = world.inside('child-restore.json')
                child = f.call(world, command, 'rollback', '--repo', repo, '--operation', parent,
                               '--output', child_file)
                child_id = child['operation_id']
                temporary = self.interrupt(world, command, repo, 'rollback', child_file, bank, child_id, gid)
                preview = self.known_recovery(world, command, repo, child_id, 'finish', temporary)
                self.assert_sources(bank, {path: body for path, (body, _) in originals.items()},
                                    originals, root_metadata)
                self.assert_private_preserved(directory, private)
                status = f.call(world, command, 'status', '--repo', repo, '--operation', parent)
                self.assertEqual(status['pending_unit_ids'], applied['pending_unit_ids'])
                self.assertFalse(status['semantic_recovery_complete'])
                before = self.snapshot(bank), self.snapshot(directory.parent)
                self.assertTrue(f.call(world, command, 'recover', '--repo', repo,
                                       '--proposal', preview)['receipt_is_historical'])
                self.assertEqual((self.snapshot(bank), self.snapshot(directory.parent)), before)
                self.assert_work_preserved(world, repo, git)

    def test_foreign_and_mismatched_marker_temps_refuse_without_mutation(self):
        for variant in ('foreign-name', 'payload', 'gid', 'mode', 'xattr', 'preview-pin'):
            with self.subTest(variant=variant), World() as world:
                command, source, repo, bank, native, originals, git, gid = self.setup_repair(world)
                safe, inspected = f.inspect(world, command, repo)
                proposal, _ = f.prepare(world, command, source, repo, inspected, native)
                operation = safe['operation_id']
                directory = f.private_directory(world, bank, operation)
                private = self.snapshot(directory)
                temporary = self.interrupt(world, command, repo, 'apply', proposal, bank, operation, gid)
                preview = world.inside('refused-recovery.json')
                if variant == 'preview-pin':
                    f.call(world, command, 'recover', '--repo', repo, '--operation', operation,
                           '--mode', 'finish', '--output', preview)
                    replacement = bank / '.contract-marker-replacement'
                    replacement.write_bytes(temporary.read_bytes())
                    replacement.chmod(0o600)
                    self.assertNotEqual(replacement.stat().st_ino, temporary.stat().st_ino)
                    os.replace(replacement, temporary)
                elif variant == 'foreign-name':
                    temporary.rename(bank / ('.agent-memory-layout-marker-' + str(uuid.uuid4()) + '.tmp'))
                elif variant == 'payload':
                    temporary.write_bytes(temporary.read_bytes() + b'\n')
                elif variant == 'gid':
                    os.chown(temporary, -1, os.getegid() if gid != os.getegid() else bank.stat().st_gid)
                elif variant == 'xattr':
                    name = ('org.agent-memory.marker-drift' if sys.platform == 'darwin'
                            else 'user.org.agent-memory.marker-drift')
                    set_xattr(temporary, name, b'Unreviewed marker metadata')
                else:
                    temporary.chmod(0o640)
                before = self.snapshot(bank), self.snapshot(directory.parent)
                if variant == 'preview-pin':
                    f.call(world, command, 'recover', '--repo', repo, '--proposal', preview, expected=1)
                else:
                    f.call(world, command, 'recover', '--repo', repo, '--operation', operation,
                           '--mode', 'finish', '--output', preview, expected=1)
                self.assertEqual((self.snapshot(bank), self.snapshot(directory.parent)), before)
                self.assert_private_preserved(directory, private)
                self.assertEqual({path: (bank / path).read_bytes() for path in originals},
                                 {path: body for path, (body, _) in originals.items()})
                self.assert_work_preserved(world, repo, git)
