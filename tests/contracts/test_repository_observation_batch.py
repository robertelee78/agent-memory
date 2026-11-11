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
from contracts.test_identity_git_group import Children, ClosingPipe

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
            spawn = identity_git._spawn
            changed = []
            def commit_before_names(root, *args, **kwargs):
                if args and args[0] == 'ls-files' and not changed:
                    other = repo / 'late-commit/MEMORY.md'; other.parent.mkdir()
                    other.write_bytes((bank / 'MEMORY.md').read_bytes())
                    self.commit(world, repo)
                    world.git('rm', '--', 'late-commit/MEMORY.md', cwd=repo)
                    changed.append(self.preserved(world, repo))
                return spawn(root, *args, **kwargs)
            with patch.object(identity_git, '_spawn', side_effect=commit_before_names):
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

    def test_invalid_config_refuses_before_speculative_real_git_inventory_acceptance(self):
        for invalid in ('duplicate', 'empty', 'unsafe-root', 'unsafe-label'):
            with self.subTest(invalid=invalid), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo = world.repository(); bank, _ = self.bank(world, repo); self.commit(world, repo)
                if invalid == 'duplicate':
                    world.git('config', '--add', 'agent-memory.bankRoot', bank.name, cwd=repo)
                    world.git('config', '--add', 'agent-memory.bankRoot', bank.name, cwd=repo)
                    message = 'one explicit value per key'
                else:
                    value = {'empty': '', 'unsafe-root': '../outside',
                             'unsafe-label': 'docs/password=BOUNDARY_ROOT_SECRET'}[invalid]
                    world.git('config', 'agent-memory.bankRoot', value, cwd=repo)
                    message = {'empty': 'one explicit value per key',
                               'unsafe-root': 'canonical repository-relative',
                               'unsafe-label': 'requires safety review'}[invalid]
                before = self.preserved(world, repo)
                spawn = identity_git._spawn
                speculative = []
                def launch(root, *args, **kwargs):
                    process = spawn(root, *args, **kwargs)
                    if args[0] in ('ls-files', 'ls-tree'):
                        speculative.append(process)
                    return process
                def reject_inventory(*args):
                    self.fail('Invalid local settings reached router inventory interpretation')
                stat = Path.stat
                forbidden = (bank, world.inside('outside'), repo / 'docs/password=BOUNDARY_ROOT_SECRET')
                def gated_stat(target, *args, **kwargs):
                    if any(target == path or path in target.parents for path in forbidden):
                        self.fail('Invalid config accessed a configured/source location before validation')
                    return stat(target, *args, **kwargs)
                with patch.object(identity_git, '_spawn', side_effect=launch), \
                        patch.object(Path, 'stat', gated_stat):
                    with self.assertRaisesRegex(ContractError, message) as caught:
                        repository_config.settings(repo, router_reader=reject_inventory)
                self.assertNotIn('BOUNDARY_ROOT_SECRET', str(caught.exception))
                self.assertTrue(speculative, 'Refusal must clean actual speculative Git children')
                for process in speculative:
                    self.assertIsNotNone(process.returncode)
                    self.assertTrue(process.stdout.closed)
                    self.assertTrue(process.stderr.closed)
                    with self.assertRaises(ChildProcessError):
                        os.waitpid(process.pid, os.WNOHANG)
                self.assertFalse(world.inside('outside').exists())
                self.assertEqual(self.preserved(world, repo), before)

    def test_earlier_real_config_refusal_beats_every_later_child_failure(self):
        failures = (
            FileNotFoundError('owned missing later Git'),
            PermissionError('owned denied later Git'),
            'sys.exit(17)',
            'os.kill(os.getpid(), signal.SIGTERM)',
            'receive()',
            'os.write(1, b"malformed inventory without a NUL")',
        )
        for position in ('ls-files', 'ls-tree'):
            for failure in failures:
                with self.subTest(position=position, failure=failure), World() as world, \
                        patch.dict(os.environ, world.env, clear=True):
                    repo = world.repository(); self.bank(world, repo); self.commit(world, repo)
                    world.git('config', '--add', 'agent-memory.bankRoot', 'memory-bank', cwd=repo)
                    world.git('config', '--add', 'agent-memory.bankRoot', 'another-bank', cwd=repo)
                    before = self.preserved(world, repo)
                    children = Children(self, (failure,))
                    spawn = identity_git._spawn
                    def launch(root, *args, **kwargs):
                        if args[0] == position:
                            return children.spawn(root, '0')
                        return spawn(root, *args, **kwargs)
                    try:
                        with patch.object(identity_git, '_spawn', side_effect=launch):
                            with self.assertRaisesRegex(ContractError, 'one explicit value per key'):
                                repository_config.settings(repo)
                        children.assert_closed()
                        self.assertEqual(self.preserved(world, repo), before)
                    finally:
                        children.close()

    def test_malformed_config_child_output_never_accepts_speculative_names(self):
        for raw in (b'agent-memory.bankroot\nmemory-bank',
                    b'agent-memory.bankroot\0',
                    b'agent-memory.unknown\nmemory-bank\0',
                    b'agent-memory.bankroot\nmemory-bank\0agent-memory.bankroot\nmemory-bank\0'):
            with self.subTest(raw=raw), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo = world.repository(); self.bank(world, repo); self.commit(world, repo)
                before = self.preserved(world, repo)
                children = Children(self, ('os.write(1, ' + repr(raw) + ')',))
                spawn = identity_git._spawn
                speculative = []
                def launch(root, *args, **kwargs):
                    if args[0] == 'config' and '--get-regexp' in args:
                        return children.spawn(root, '0')
                    process = spawn(root, *args, **kwargs)
                    if args[0] in ('ls-files', 'ls-tree'):
                        speculative.append(process)
                    return process
                try:
                    with patch.object(identity_git, '_spawn', side_effect=launch):
                        with self.assertRaisesRegex(ContractError, 'Repository memory configuration'):
                            repository_config.settings(repo)
                    children.assert_closed()
                    self.assertTrue(speculative)
                    for process in speculative:
                        self.assertIsNotNone(process.returncode)
                        self.assertTrue(process.stdout.closed and process.stderr.closed)
                        with self.assertRaises(ChildProcessError):
                            os.waitpid(process.pid, os.WNOHANG)
                    self.assertEqual(self.preserved(world, repo), before)
                finally:
                    children.close()

    def test_branch_validation_remains_after_competing_router_refusal(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository(); bank, _ = self.bank(world, repo); self.commit(world, repo)
            world.git('config', 'agent-memory.authorityBranch', '../unsafe', cwd=repo)
            other = repo / 'competing/MEMORY.md'; other.parent.mkdir()
            other.write_bytes((bank / 'MEMORY.md').read_bytes())
            before = self.preserved(world, repo)
            with self.assertRaisesRegex(ContractError, 'Competing memory bank locations'):
                repository_config.settings(repo)
            self.assertEqual(self.preserved(world, repo), before)

    def test_grouped_discovery_keeps_effective_includes_rewrites_and_remote_conflicts(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository(); self.bank(world, repo); self.commit(world, repo)
            included = world.inside('included-authority.gitconfig')
            included.write_text('[agent-memory]\n authorityRemote = Upstream\n')
            world.git('config', 'include.path', included, cwd=repo)
            world.git('config', 'url.https://github.com/.insteadOf', 'fixture:', cwd=repo)
            world.git('remote', 'add', 'Upstream', 'fixture:example/project.git', cwd=repo)
            world.git('remote', 'set-url', '--push', 'Upstream', 'https://github.com/example/project.git', cwd=repo)
            canonical = project_identity.normalize('https://github.com/example/project.git', repo)
            before = self.preserved(world, repo)
            found = project_identity.discover(repo)
            self.assertEqual(found['configured_authority'], ['Upstream'])
            self.assertEqual(found['authority_name'], 'Upstream')
            self.assertEqual(found['canonical_remote'], canonical)
            self.assertEqual(found['remotes']['Upstream'], {'identities': [canonical], 'consistent': True})
            self.assertEqual(self.preserved(world, repo), before)
            world.git('remote', 'add', 'Other', 'https://github.com/example/other.git', cwd=repo)
            world.git('config', '--add', 'agent-memory.authorityRemote', 'Other', cwd=repo)
            before = self.preserved(world, repo)
            found = project_identity.discover(repo)
            self.assertIn('Conflicting configured authority remotes; choose one explicitly', found['issues'])
            self.assertEqual(set(found['configured_authority']), {'Upstream', 'Other'})
            self.assertEqual(project_identity.discover(repo, authority='Upstream')['canonical_remote'], canonical)
            self.assertEqual(self.preserved(world, repo), before)
            world.git('remote', 'set-url', '--push', 'Upstream', 'https://github.com/example/different.git', cwd=repo)
            before = self.preserved(world, repo)
            found = project_identity.discover(repo, authority='Upstream')
            self.assertIsNone(found['canonical_remote'])
            self.assertFalse(found['remotes']['Upstream']['consistent'])
            self.assertTrue(any('fetch/push URLs' in issue for issue in found['issues']))
            self.assertEqual(self.preserved(world, repo), before)

    def test_config_and_remote_mutation_between_enrollment_entry_exit_are_fresh(self):
        for mutation in ('config', 'remote'):
            with self.subTest(mutation=mutation), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo = world.repository()
                url = 'https://github.com/example/project.git'
                world.git('remote', 'add', 'origin', url, cwd=repo)
                _, identity = scope_fixtures.ScopeBindingContract().bank(
                    world, repo, remote=project_identity.normalize(url, repo))
                bind(world, repo); self.commit(world, repo)
                self.assertEqual(scope_binding.private_enrollment_context(repo).project_id, identity)
                require = scope_binding.require
                changed = []
                body_entered = False
                canary = world.paths['state'] / 'unexpected-enrollment-body'
                def mutate_after_entry(*args, **kwargs):
                    result = require(*args, **kwargs)
                    if not changed:
                        if mutation == 'config':
                            world.git('config', 'agent-memory.bankRoot', 'different-bank', cwd=repo)
                        else:
                            world.git('remote', 'set-url', 'origin', 'https://github.com/example/changed.git', cwd=repo)
                        changed.append(self.preserved(world, repo))
                    return result
                with patch.object(scope_binding, 'require', side_effect=mutate_after_entry):
                    with self.assertRaises(ContractError):
                        with scope_binding.private_enrollment_context(repo):
                            body_entered = True
                            canary.write_bytes(b'Unexpected enrollment body write.\n')
                self.assertFalse(body_entered)
                self.assertFalse(canary.exists())
                self.assertTrue(changed)
                self.assertEqual(self.preserved(world, repo), changed[0])
                with self.assertRaises(ContractError):
                    scope_binding.private_enrollment_context(repo)
                self.assertEqual(self.preserved(world, repo), changed[0])

    def test_real_config_validation_time_is_charged_to_unresolved_inventory_child(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository(); self.bank(world, repo, custom=True); self.commit(world, repo)
            before = self.preserved(world, repo)
            children = Children(self, ('receive()',))
            spawn = identity_git._spawn
            validate = repository_config.validate_root
            now = [0.0]
            def launch(root, *args, **kwargs):
                if args[0] == 'ls-files':
                    return children.spawn(root, '0')
                return spawn(root, *args, **kwargs)
            def parse_and_validate(value):
                result = validate(value)
                now[0] = 11
                return result
            try:
                with patch.object(identity_git, '_spawn', side_effect=launch), \
                        patch.object(identity_git.time, 'monotonic', side_effect=lambda: now[0]), \
                        patch.object(repository_config, 'validate_root', side_effect=parse_and_validate):
                    with self.assertRaisesRegex(ContractError, 'inventory is unavailable'):
                        repository_config.settings(repo)
                children.assert_closed()
                self.assertLess(children.processes[0].returncode, 0)
                self.assertEqual(self.preserved(world, repo), before)
            finally:
                children.close()

    def test_malformed_name_children_refuse_after_valid_real_config(self):
        for command, raw in (
                ('ls-files', b'H memory-bank/MEMORY.md'),
                ('ls-files', b'X memory-bank/MEMORY.md\0'),
                ('ls-tree', b'/outside/MEMORY.md\0')):
            with self.subTest(command=command, raw=raw), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo = world.repository(); self.bank(world, repo); self.commit(world, repo)
                before = self.preserved(world, repo)
                children = Children(self, ('os.write(1, ' + repr(raw) + ')',))
                spawn = identity_git._spawn
                def launch(root, *args, **kwargs):
                    if args[0] == command:
                        return children.spawn(root, '0')
                    return spawn(root, *args, **kwargs)
                try:
                    with patch.object(identity_git, '_spawn', side_effect=launch):
                        with self.assertRaises(ContractError):
                            repository_config.settings(repo)
                    children.assert_closed()
                    self.assertEqual(self.preserved(world, repo), before)
                finally:
                    children.close()

    def test_parent_traversal_name_never_adopts_outside_router(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository(); bank, _ = self.bank(world, repo); self.commit(world, repo)
            canary = repo.parent / 'outside' / 'MEMORY.md'
            canary.parent.mkdir()
            router = (bank / 'MEMORY.md').read_bytes()
            canary.write_bytes(router)
            self.assertTrue(canary.is_file())
            self.assertFalse(canary.is_symlink())
            before = self.preserved(world, repo)
            baseline = repository_config.settings(repo)
            raw_baseline = repository_config.settings(repo, raw_locations=True)
            self.assertEqual(baseline['bank_root'], 'memory-bank')
            self.assertEqual(raw_baseline['bank_root'], 'memory-bank')
            raw = b'../outside/MEMORY.md\0'
            children = Children(self, ('os.write(1, ' + repr(raw) + ')',))
            spawn = identity_git._spawn
            def launch(root, *args, **kwargs):
                if args[0] == 'ls-tree':
                    return children.spawn(root, '0')
                return spawn(root, *args, **kwargs)
            try:
                with patch.object(identity_git, '_spawn', side_effect=launch):
                    self.assertEqual(repository_config.settings(repo), baseline)
                    self.assertEqual(repository_config.settings(repo, raw_locations=True), raw_baseline)
                children.assert_closed()
                self.assertTrue(canary.is_file())
                self.assertFalse(canary.is_symlink())
                self.assertEqual(canary.read_bytes(), router)
                self.assertEqual(self.preserved(world, repo), before)
            finally:
                children.close()

    def test_interrupt_in_real_config_validation_preserves_traceback_and_cancels_siblings(self):
        for exception_type in (KeyboardInterrupt, SystemExit):
            with self.subTest(exception=exception_type), World() as world, patch.dict(os.environ, world.env, clear=True):
                repo = world.repository(); self.bank(world, repo, custom=True); self.commit(world, repo)
                before = self.preserved(world, repo)
                children = Children(self, ('receive()', 'receive()'))
                spawn = identity_git._spawn
                validate = repository_config.validate_root
                primary = exception_type('original config parser interruption')
                def launch(root, *args, **kwargs):
                    if args[0] in ('ls-files', 'ls-tree'):
                        return children.spawn(root, '0' if args[0] == 'ls-files' else '1')
                    return spawn(root, *args, **kwargs)
                def interrupted_validation(value):
                    validate(value)
                    raise primary
                names = []
                try:
                    with patch.object(identity_git, '_spawn', side_effect=launch), \
                            patch.object(repository_config, 'validate_root', side_effect=interrupted_validation):
                        with self.assertRaises(exception_type) as caught:
                            try:
                                repository_config.settings(repo)
                            except BaseException as exc:
                                traceback = exc.__traceback__
                                while traceback is not None:
                                    names.append(traceback.tb_frame.f_code.co_name)
                                    traceback = traceback.tb_next
                                raise
                    self.assertIs(caught.exception, primary)
                    self.assertIn('interrupted_validation', names)
                    children.assert_closed()
                    self.assertEqual(self.preserved(world, repo), before)
                finally:
                    children.close()

    def test_tracked_body_and_enrollment_boundaries_have_no_live_query_children(self):
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            repo = world.repository(); bank, identity = self.bank(world, repo); self.commit(world, repo)
            before = self.preserved(world, repo)
            spawn = identity_git._spawn
            require = scope_binding.require
            queries = []
            def assert_finished():
                for process in queries:
                    self.assertIsNotNone(process.returncode, 'query escaped its observation boundary')
                    self.assertTrue(process.stdout.closed and process.stderr.closed)
                    with self.assertRaises(ChildProcessError):
                        os.waitpid(process.pid, os.WNOHANG)
            def launch(root, *args, **kwargs):
                grouped = (args[0] in ('ls-files', 'ls-tree')
                           or args[0] == 'config' and ('--get-regexp' in args or 'agent-memory.authorityRemote' in args)
                           or args == ('remote',))
                if not grouped:
                    # Includes immutable OID/size/body reads and the next fresh
                    # root/HEAD sample, not a synthetic post-processing hook.
                    assert_finished()
                process = spawn(root, *args, **kwargs)
                if grouped:
                    queries.append(process)
                return process
            def guarded(*args, **kwargs):
                assert_finished()
                result = require(*args, **kwargs)
                assert_finished()
                return result
            with patch.object(identity_git, '_spawn', side_effect=launch), \
                    patch.object(scope_binding, 'require', side_effect=guarded):
                self.assertEqual(repository_config.settings(repo)['bank_root'], bank.name)
                self.assertEqual(scope_binding.private_enrollment_context(repo).project_id, identity)
            self.assertTrue(queries)
            assert_finished()
            self.assertEqual(self.preserved(world, repo), before)

    def test_real_verified_hold_remains_readonly_through_grouped_settings_and_enrollment(self):
        from contracts.test_held_source_enrollment import HeldSourceEnrollment
        with World() as world, patch.dict(os.environ, world.env, clear=True):
            _, repo, bank, _, directory, original, body = HeldSourceEnrollment().fixture(world)
            before = self.preserved(world, repo)
            spawn = identity_git._spawn
            queries = []
            def launch(root, *args, **kwargs):
                process = spawn(root, *args, **kwargs)
                queries.append(process)
                return process
            with patch.object(identity_git, '_spawn', side_effect=launch):
                self.assertEqual(repository_config.settings(repo)['bank_root'], bank.name)
                self.assertEqual(repository_config.recovery_settings(repo)['bank_root'], bank.name)
                context = scope_binding.private_enrollment_context(repo)
                self.assertTrue(context.held)
                self.assertEqual(context.root, repo.resolve())
            self.assertTrue(queries)
            for process in queries:
                self.assertIsNotNone(process.returncode)
                self.assertTrue(process.stdout.closed and process.stderr.closed)
                with self.assertRaises(ChildProcessError):
                    os.waitpid(process.pid, os.WNOHANG)
            self.assertEqual((directory / original['blob']).read_bytes(), body)
            self.assertEqual(self.preserved(world, repo), before)

    def test_remote_query_cleanup_failure_cannot_become_successful_discovery(self):
        for close_fault in (False, True):
            with self.subTest(close_fault=close_fault), World() as world, \
                    patch.dict(os.environ, world.env, clear=True):
                repo = world.repository()
                self.bank(world, repo); self.commit(world, repo)
                world.git('remote', 'add', 'origin', 'https://github.com/example/project.git', cwd=repo)
                before = self.preserved(world, repo)
                spawn, communicate = identity_git._spawn, subprocess.Popen.communicate
                targets = []
                def launch(root, *args, **kwargs):
                    process = spawn(root, *args, **kwargs)
                    if args[:2] == ('remote', 'get-url'):
                        targets.append(process)
                        if close_fault:
                            process.stdout = ClosingPipe(process.stdout)
                    return process
                def timeout(process, *args, **kwargs):
                    if process in targets:
                        raise subprocess.TimeoutExpired(process.args, 10)
                    return communicate(process, *args, **kwargs)
                with patch.object(identity_git, '_spawn', side_effect=launch), \
                        patch.object(subprocess.Popen, 'communicate', timeout):
                    if close_fault:
                        with self.assertRaises(identity_git._GitInfrastructureError) as caught:
                            project_identity.discover(repo)
                        self.assertIsInstance(caught.exception.__cause__, subprocess.TimeoutExpired)
                    else:
                        found = project_identity.discover(repo)
                        self.assertIsNone(found['canonical_remote'])
                        self.assertFalse(found['remotes']['origin']['consistent'])
                self.assertTrue(targets)
                for process in targets:
                    self.assertIsNotNone(process.returncode)
                    self.assertTrue(process.stdout.closed and process.stderr.closed)
                    with self.assertRaises(ChildProcessError):
                        os.waitpid(process.pid, os.WNOHANG)
                self.assertEqual(self.preserved(world, repo), before)


if __name__ == '__main__':
    unittest.main()
