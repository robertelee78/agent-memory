"""Fresh repository identity consumers against real Git state and bounded faults."""
import os
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

from world import World
from identity_fixtures import bind
from contracts import test_scope_binding as scope_fixtures

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'))
from agent_memory import identity_git, project_identity, repository_config, scope_binding
from agent_memory.frontmatter import ContractError


class RepositoryObservationBatch(unittest.TestCase):
    def bank(self, world, repo, *, custom=False):
        bank, identity = scope_fixtures.ScopeBindingContract().bank(world, repo)
        if custom:
            bank.rename(repo / 'project-notes')
            bank = repo / 'project-notes'
            world.git('config', '--local', 'agent-memory.bankRoot', bank.name, cwd=repo)
        bind(world, repo)
        return bank, identity

    def commit(self, world, repo):
        world.git('add', '.', cwd=repo)
        world.git('commit', '-m', 'Owned repository observation fixture', cwd=repo)

    def preserved(self, world, repo):
        return {str(path.relative_to(world.root)): path.read_bytes()
                for root in (repo, world.paths['config'], world.paths['state'])
                for path in root.rglob('*') if path.is_file()}

    def assert_refused(self, repo):
        with self.assertRaises(ContractError):
            repository_config.settings(repo)
        status = scope_binding.status(repo)
        self.assertFalse(status['resolved'], status)
        self.assertIsNone(status['project_id'], status)
        self.assertTrue(status['project'].get('error'), status)
        self.assertFalse(status['writes_performed'])

    def fault(self, result):
        run = identity_git._run
        def inspect(repo, *args, **kwargs):
            if args == ('rev-parse', '--show-toplevel', '--verify', '--quiet', 'HEAD'):
                if isinstance(result, BaseException):
                    raise result
                return subprocess.CompletedProcess(['git'], result[0], result[1], b'')
            return run(repo, *args, **kwargs)
        return patch.object(identity_git, '_run', side_effect=inspect)

    def test_real_root_spellings_and_linked_worktrees_preserve_public_identity(self):
        for name in ('normal root', 'embedded\nnewline root', 'trailing LF root\n\n'):
            with self.subTest(name=name), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo = world.repository(name)
                bank, identity = self.bank(world, repo, custom=True)
                self.commit(world, repo)
                linked = world.inside(name + ' linked\n')
                world.git('worktree', 'add', '--detach', linked, 'HEAD', cwd=repo)
                for root in (repo, linked):
                    before = self.preserved(world, root)
                    self.assertEqual(repository_config.settings(root)['bank_root'], bank.name)
                    self.assertEqual(repository_config.recovery_settings(root)['bank_root'], bank.name)
                    status = project_identity.status(root)
                    self.assertTrue(status['resolved'], status)
                    self.assertEqual(status['project_id'], identity)
                    combined = scope_binding.status(root)
                    self.assertTrue(combined['resolved'], combined)
                    context = scope_binding.private_enrollment_context(root)
                    self.assertEqual(context.root, root.resolve())
                    self.assertEqual(context.project_id, identity)
                    self.assertEqual(self.preserved(world, root), before)

    def test_sha256_head_retains_committed_locator_after_staged_deletion(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.inside('sha256 repository\n')
            world.git('init', '--initial-branch=main', '--object-format=sha256', repo)
            bank, identity = self.bank(world, repo, custom=True)
            self.commit(world, repo)
            world.git('rm', '-r', '--', bank.name, cwd=repo)
            before = self.preserved(world, repo)
            self.assertEqual(repository_config.settings(repo)['bank_root'], bank.name)
            self.assertEqual(repository_config.git_roots(repo), {bank.name})
            status = project_identity.status(repo)
            self.assertFalse(status['resolved'], status)
            self.assertEqual(status['project_id'], identity)
            self.assertIn('Bank initialization or identity update is pending', status['issues'])
            self.assertEqual(self.preserved(world, repo), before)

    def test_unborn_root_preserves_local_settings_and_indexed_bank_identity(self):
        for name in ('unborn normal', 'unborn embedded\nroot', 'unborn trailing\n\n'):
            with self.subTest(name=name), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo = world.repository(name)
                bank, identity = self.bank(world, repo, custom=True)
                world.git('add', '--', bank.name, cwd=repo)
                before = self.preserved(world, repo)
                self.assertEqual(repository_config.settings(repo)['bank_root'], bank.name)
                self.assertEqual(repository_config.git_roots(repo), {bank.name})
                status = project_identity.status(repo)
                self.assertTrue(status['resolved'], status)
                self.assertEqual(status['project_id'], identity)
                self.assertEqual(self.preserved(world, repo), before)

    def test_non_git_directory_remains_readable_without_inventing_identity(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.inside('plain directory\n\n'); repo.mkdir()
            before = self.preserved(world, repo)
            self.assertEqual(repository_config.settings(repo),
                             {'bank_root': 'memory-bank', 'authority_branch': None})
            self.assertEqual(repository_config.recovery_settings(repo), repository_config.settings(repo))
            status = scope_binding.status(repo)
            self.assertFalse(status['resolved'], status)
            self.assertIsNone(status['project_id'])
            self.assertEqual(self.preserved(world, repo), before)
            self.assertFalse((repo / 'memory-bank').exists())

    def test_corrupt_missing_and_dangling_head_never_establish_empty_repository(self):
        for damage in ('corrupt', 'missing', 'dangling'):
            with self.subTest(damage=damage), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo = world.repository(); self.bank(world, repo); self.commit(world, repo)
                head = repo / '.git/HEAD'
                if damage == 'missing':
                    head.unlink()
                else:
                    head.write_text('not a valid reference\n' if damage == 'corrupt' else 'f' * 40 + '\n')
                before = self.preserved(world, repo)
                self.assert_refused(repo)
                self.assertEqual(self.preserved(world, repo), before)

    def test_unreadable_or_malformed_metadata_preserves_refusal_and_original_bank(self):
        for damage in ('config-directory', 'malformed-config', 'invalid-gitfile'):
            with self.subTest(damage=damage), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo = world.repository(); self.bank(world, repo); self.commit(world, repo)
                config = repo / '.git/config'
                if damage == 'config-directory':
                    config.rename(world.inside('retained-owned-config')); config.mkdir()
                elif damage == 'malformed-config':
                    config.write_text('[unterminated fixture section\n')
                else:
                    (repo / '.git').rename(world.inside('retained-owned-metadata'))
                    (repo / '.git').write_text('unreadable metadata locator\n')
                before = self.preserved(world, repo)
                self.assert_refused(repo)
                self.assertEqual(self.preserved(world, repo), before)

    def test_subdirectory_still_refuses_repository_root_authority(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository('parent\n'); self.bank(world, repo); self.commit(world, repo)
            child = repo / 'ordinary source directory'; child.mkdir()
            before = self.preserved(world, repo)
            with self.assertRaises(ContractError):
                repository_config.settings(child)
            with self.assertRaises(ContractError):
                scope_binding.private_enrollment_context(child)
            self.assertEqual(self.preserved(world, repo), before)

    def test_changed_head_with_deleted_competing_router_refuses_enrollment_exit(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository(); bank, _ = self.bank(world, repo); self.commit(world, repo)
            scope_binding.private_enrollment_context(repo)
            require = scope_binding.require
            changed = []
            def read_scope_then_commit(*args, **kwargs):
                result = require(*args, **kwargs)
                if not changed:
                    other = repo / 'competing/MEMORY.md'; other.parent.mkdir()
                    other.write_bytes((bank / 'MEMORY.md').read_bytes())
                    world.git('add', '--', 'competing/MEMORY.md', cwd=repo)
                    world.git('commit', '-m', 'Concurrent committed locator fixture', cwd=repo)
                    world.git('rm', '--', 'competing/MEMORY.md', cwd=repo)
                    changed.append(self.preserved(world, repo))
                return result
            with patch.object(scope_binding, 'require', side_effect=read_scope_then_commit):
                with self.assertRaises(ContractError):
                    scope_binding.private_enrollment_context(repo)
            self.assertTrue(changed)
            self.assertEqual(self.preserved(world, repo), changed[0])
            with self.assertRaises(ContractError):
                scope_binding.private_enrollment_context(repo)
            self.assertEqual(self.preserved(world, repo), changed[0])

    def test_independent_git_roots_resamples_head_and_validates_its_own_observation(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository(); bank, _ = self.bank(world, repo); self.commit(world, repo)
            self.assertEqual(repository_config.git_roots(repo), {bank.name})
            other = repo / 'later/MEMORY.md'; other.parent.mkdir()
            other.write_bytes((bank / 'MEMORY.md').read_bytes())
            self.commit(world, repo)
            world.git('rm', '--', 'later/MEMORY.md', cwd=repo)
            before = self.preserved(world, repo)
            self.assertEqual(repository_config.git_roots(repo), {bank.name, 'later'})
            run = identity_git._run
            def malformed_head(root, *args, **kwargs):
                if args == ('rev-parse', '--verify', 'HEAD'):
                    return subprocess.CompletedProcess(['git'], 0, b'HEAD\n', b'')
                return run(root, *args, **kwargs)
            with patch.object(identity_git, '_run', side_effect=malformed_head):
                with self.assertRaises(ContractError):
                    repository_config.git_roots(repo)
            self.assertEqual(self.preserved(world, repo), before)

    def test_index_untracked_and_raw_locators_added_after_scope_read_refuse_exit(self):
        for kind in ('indexed', 'untracked', 'raw-only'):
            with self.subTest(kind=kind), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo = world.repository(); bank, _ = self.bank(world, repo); self.commit(world, repo)
                require = scope_binding.require
                changed = []
                def read_scope_then_add(*args, **kwargs):
                    result = require(*args, **kwargs)
                    if not changed:
                        other = repo / 'late-locator/MEMORY.md'; other.parent.mkdir()
                        other.write_bytes(b'# Raw source, not a memory router.\n' if kind == 'raw-only'
                                          else (bank / 'MEMORY.md').read_bytes())
                        if kind == 'indexed':
                            world.git('add', '--', 'late-locator/MEMORY.md', cwd=repo)
                        changed.append(self.preserved(world, repo))
                    return result
                with patch.object(scope_binding, 'require', side_effect=read_scope_then_add):
                    with self.assertRaises(ContractError):
                        scope_binding.private_enrollment_context(repo)
                self.assertTrue(changed)
                self.assertEqual(self.preserved(world, repo), changed[0])
                with self.assertRaises(ContractError):
                    scope_binding.private_enrollment_context(repo)
                self.assertEqual(self.preserved(world, repo), changed[0])

    def test_sampled_head_is_immutable_and_next_settings_read_observes_later_commit(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository(); bank, _ = self.bank(world, repo); self.commit(world, repo)
            run = identity_git._run
            changed = []
            def commit_before_names(root, *args, **kwargs):
                if args and args[0] == 'ls-files' and not changed:
                    other = repo / 'late-commit/MEMORY.md'; other.parent.mkdir()
                    other.write_bytes((bank / 'MEMORY.md').read_bytes())
                    self.commit(world, repo)
                    world.git('rm', '--', 'late-commit/MEMORY.md', cwd=repo)
                    changed.append(self.preserved(world, repo))
                return run(root, *args, **kwargs)
            with patch.object(identity_git, '_run', side_effect=commit_before_names):
                self.assertEqual(repository_config.settings(repo)['bank_root'], bank.name)
            self.assertTrue(changed)
            with self.assertRaises(ContractError):
                repository_config.settings(repo)
            self.assertEqual(self.preserved(world, repo), changed[0])

    def test_malformed_successful_observation_never_resolves_existing_identity(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository(); self.bank(world, repo); self.commit(world, repo)
            root = os.fsencode(repo)
            oid = world.git('rev-parse', 'HEAD', cwd=repo).stdout.rstrip('\n').encode()
            outputs = (b'', root + b'\n', root + b'\n' + oid,
                       root + b'\n' + oid[:-1] + b'\n', root + b'\nHEAD\n',
                       b'\n' + oid + b'\n', root + b'\n' + oid + b'\n\n')
            before = self.preserved(world, repo)
            for output in outputs:
                with self.subTest(output=output), self.fault((0, output)):
                    self.assert_refused(repo)
                self.assertEqual(self.preserved(world, repo), before)

    def test_signaled_or_incomplete_failed_observation_never_adopts_partial_root(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository(); self.bank(world, repo); self.commit(world, repo)
            root = os.fsencode(repo)
            before = self.preserved(world, repo)
            failures = ((1, b''), (1, root), (1, b'\n'), (-15, root + b'\n'),
                        (-9, root + b'\n' + b'a' * 40 + b'\n'))
            for failure in failures:
                with self.subTest(failure=failure), self.fault(failure):
                    self.assert_refused(repo)
                self.assertEqual(self.preserved(world, repo), before)

    def test_timeout_and_launch_failure_never_mint_identity_from_partial_output(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository(); self.bank(world, repo); self.commit(world, repo)
            partial = os.fsencode(repo) + b'\n' + b'a' * 40 + b'\n'
            failures = (subprocess.TimeoutExpired(['git'], 10, output=partial),
                        FileNotFoundError('Owned Git launch fault'), PermissionError('Owned Git access fault'))
            before = self.preserved(world, repo)
            for failure in failures:
                with self.subTest(failure=type(failure).__name__), self.fault(failure):
                    self.assert_refused(repo)
                    with self.assertRaises(ContractError):
                        scope_binding.private_enrollment_context(repo)
                self.assertEqual(self.preserved(world, repo), before)
            plain = world.inside('genuine non-Git launch failure'); plain.mkdir()
            with self.fault(failures[0]):
                self.assertEqual(repository_config.settings(plain)['bank_root'], 'memory-bank')
            self.assertFalse((plain / 'memory-bank').exists())

    def test_unavailable_commit_or_locator_blob_never_becomes_absent_memory(self):
        for missing in ('commit', 'locator-blob'):
            with self.subTest(missing=missing), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo = world.repository(); self.bank(world, repo); self.commit(world, repo)
                expression = 'HEAD' if missing == 'commit' else 'HEAD:memory-bank/MEMORY.md'
                oid = world.git('rev-parse', expression, cwd=repo).stdout.rstrip('\n')
                (repo / '.git/objects' / oid[:2] / oid[2:]).unlink()
                before = self.preserved(world, repo)
                self.assert_refused(repo)
                self.assertEqual(self.preserved(world, repo), before)

    def test_coalesced_observation_ignores_ambient_authority_and_fsmonitor(self):
        with World() as world:
            repo = world.repository(); bank, identity = self.bank(world, repo); self.commit(world, repo)
            foreign = world.repository('foreign repository')
            canary = world.inside('fsmonitor-executed')
            trap = world.inside('owned-fsmonitor-trap')
            trap.write_text('#!/bin/sh\nprintf executed > ' + "'" + str(canary) + "'" + '\n')
            trap.chmod(0o700)
            world.git('config', 'core.fsmonitor', trap, cwd=repo)
            trace = world.inside('unwanted-git-trace')
            env = {**world.env, 'GIT_DIR': str(foreign / '.git'), 'GIT_WORK_TREE': str(foreign),
                   'GIT_INDEX_FILE': str(world.inside('unwanted-index')), 'GIT_CONFIG_COUNT': '1',
                   'GIT_CONFIG_KEY_0': 'agent-memory.bankRoot', 'GIT_CONFIG_VALUE_0': 'foreign-bank',
                   'GIT_TRACE2_EVENT': str(trace)}
            before = self.preserved(world, repo)
            with patch.dict(os.environ, env, clear=True):
                self.assertEqual(repository_config.settings(repo)['bank_root'], bank.name)
                status = project_identity.status(repo)
                self.assertTrue(status['resolved'], status)
                self.assertEqual(status['project_id'], identity)
                self.assertEqual(scope_binding.private_enrollment_context(repo).project_id, identity)
            self.assertFalse(canary.exists())
            self.assertFalse(trace.exists())
            self.assertFalse(world.inside('unwanted-index').exists())
            self.assertEqual(self.preserved(world, repo), before)


if __name__ == '__main__':
    unittest.main()
