"""Public recovery identity and private-discovery boundary contracts."""
import json
import os
from pathlib import Path
import shutil
import socket
import subprocess
import sys
import unittest
import uuid

from world import World
import layout_recovery_fixtures as f
from contracts import test_layout_recovery_frontier as frontier_tests
from contracts import test_layout_recovery_publication as publication_tests


def bank_bytes(bank):
    return {p.relative_to(bank).as_posix(): p.read_bytes()
            for p in bank.rglob('*') if p.is_file()}


class RecoveryIdentityClosureContract(unittest.TestCase):
    def test_duplicate_uuid_text_does_not_establish_raw_router_identity(self):
        with World() as w:
            command, _ = f.installed(w)
            repo, bank, _ = f.memory(w)
            w.git('add', 'memory-bank', cwd=repo)
            _, observed = f.inspect(w, command, repo)
            identity = observed['manifest']['project_id']
            raw = ('Malformed identity prose: ' + identity + '\nRepeated: ' + identity + '\n').encode()
            (bank / 'MEMORY.md').write_bytes(raw)
            before = bank_bytes(bank)
            index = (repo / '.git/index').read_bytes()
            result = f.call(w, command, 'inspect', '--repo', repo,
                            '--output', w.inside('ambiguous-router.json'), expected=1)
            self.assertEqual(result['failure_code'], 'pending-identity-review')
            self.assertFalse(result['canonical_writes_performed'])
            self.assertEqual(bank_bytes(bank), before)
            self.assertEqual((repo / '.git/index').read_bytes(), index)

    def test_live_bank_lock_refuses_inspection_without_stealing_lock(self):
        with World() as w:
            command, _ = f.installed(w)
            repo, bank, _ = f.memory(w)
            lock = bank / '.agent-memory.lock'
            lock.write_text(json.dumps({'operation_id': str(uuid.uuid4()), 'operation': 'bank-capture',
                                        'host': socket.gethostname(), 'pid': os.getpid(),
                                        'start': '2026-10-22T18:00:00Z'}))
            lock.chmod(0o600)
            before = bank_bytes(bank)
            result = f.call(w, command, 'inspect', '--repo', repo,
                            '--output', w.inside('live-lock-inspect.json'), expected=1)
            self.assertFalse(result['canonical_writes_performed'])
            self.assertEqual(bank_bytes(bank), before)

    def test_authority_fetch_push_disagreement_refuses_recovery_inspection(self):
        with World() as w:
            command, _ = f.installed(w)
            repo, bank, _ = f.memory(w)
            w.git('remote', 'add', 'origin', 'https://github.com/owner/project.git', cwd=repo)
            w.git('add', 'memory-bank', cwd=repo)
            w.git('remote', 'set-url', '--push', 'origin', 'https://github.com/other/project.git', cwd=repo)
            before = bank_bytes(bank)
            index = (repo / '.git/index').read_bytes()
            result = f.call(w, command, 'inspect', '--repo', repo,
                            '--output', w.inside('authority-refused.json'), expected=1)
            self.assertFalse(result['canonical_writes_performed'])
            self.assertEqual(bank_bytes(bank), before)
            self.assertEqual((repo / '.git/index').read_bytes(), index)

    def test_unsupported_extended_attributes_refuse_without_modification(self):
        with World() as w:
            command, _ = f.installed(w)
            repo, bank, _ = f.memory(w)
            target = bank / 'projectbrief.md'
            name = 'com.agent-memory.test' if sys.platform == 'darwin' else 'user.agent-memory-test'
            if sys.platform == 'darwin':
                w.run(['xattr', '-w', name, 'preserve', target])
                attribute = lambda: w.run(['xattr', '-p', name, target]).stdout
                self.assertEqual(attribute(), 'preserve\n')
            else:
                os.setxattr(target, name, b'preserve')
                attribute = lambda: os.getxattr(target, name)
                self.assertEqual(attribute(), b'preserve')
            original_attribute = attribute()
            before = bank_bytes(bank)
            result = f.call(w, command, 'inspect', '--repo', repo,
                            '--output', w.inside('xattr-refused.json'), expected=1)
            self.assertEqual(result['failure_code'], 'pending-metadata-preservation')
            self.assertFalse(result['canonical_writes_performed'])
            self.assertEqual(attribute(), original_attribute)
            self.assertEqual(bank_bytes(bank), before)

    def test_darwin_acl_refuses_without_modification(self):
        with World() as w:
            command, _ = f.installed(w)
            repo, bank, _ = f.memory(w)
            target = bank / 'projectbrief.md'
            if sys.platform == 'darwin':
                w.run(['chmod', '+a', 'everyone deny delete', target])
                acl = lambda: w.run(['ls', '-le', target]).stdout
                cleanup = ['chmod', '-N', target]
            elif sys.platform.startswith('linux'):
                setfacl = shutil.which('setfacl')
                if setfacl is None:
                    self.fail('POSIX ACL refusal requires setfacl on this fixture host')
                added = w.run([setfacl, '-m', 'u:' + str(os.geteuid()) + ':r--', target], check=False)
                if added.returncode:
                    self.fail('POSIX ACL fixture setup failed: ' + added.stderr)
                acl = lambda: os.getxattr(target, 'system.posix_acl_access')
                cleanup = [setfacl, '-b', target]
            else:
                self.fail('ACL fixture unsupported on platform ' + sys.platform)
            try:
                before = bank_bytes(bank)
                original_acl = acl()
                result = f.call(w, command, 'inspect', '--repo', repo,
                                '--output', w.inside('acl-refused.json'), expected=1)
                self.assertEqual(result['failure_code'], 'pending-metadata-preservation')
                self.assertFalse(result['canonical_writes_performed'])
                self.assertEqual(acl(), original_acl)
                self.assertEqual(bank_bytes(bank), before)
            finally:
                w.run(cleanup)

    def test_settings_or_bank_parent_change_after_preview_refuses_apply(self):
        for change in ('settings', 'parent'):
            with self.subTest(change=change), World() as w:
                command, source = f.installed(w)
                repo, bank, native = f.memory(w, oversized=True)
                w.git('add', 'memory-bank', cwd=repo)
                _, observed = f.inspect(w, command, repo)
                proposal, _ = f.prepare(w, command, source, repo, observed, native)
                if change == 'settings':
                    w.git('config', '--local', 'agent-memory.authorityBranch', 'another-branch', cwd=repo)
                else:
                    bank.chmod((bank.stat().st_mode & 0o777) ^ 0o020)
                before = bank_bytes(bank)
                index = (repo / '.git/index').read_bytes()
                result = f.call(w, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1)
                self.assertFalse(result['canonical_writes_performed'])
                self.assertEqual(bank_bytes(bank), before)
                self.assertEqual((repo / '.git/index').read_bytes(), index)
                if change == 'parent':
                    self.assertEqual(bank.stat().st_mode & 0o777,
                                     (observed['manifest']['bank_binding']['mode'] & 0o777) ^ 0o020)

    def test_apply_rejects_changed_reviewed_quiescence_attestation(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, native = f.memory(w)
            (bank / 'handoff-prompt.md').write_text(f.policy_body(0))
            _, observed = f.inspect(w, command, repo)
            for omission in ('attestation', 'routes'):
                request = f.review_request(w, command, source, repo, observed, native)
                if omission == 'attestation':
                    request['quiescence_attestation'] = None
                else:
                    request['quiescence_attestation']['declared_invocation_routes'] = []
                before = bank_bytes(bank)
                refused = f.call(w, command, 'prepare', '--repo', repo,
                                 '--operation', observed['operation_id'],
                                 '--input', f.save(w, request),
                                 '--output', w.inside('refused-' + omission + '.json'), expected=1)
                self.assertFalse(refused['canonical_writes_performed'])
                self.assertEqual(bank_bytes(bank), before)
            _, prepared = f.prepare(w, command, source, repo, observed, native)
            prepared['proposal']['quiescence_attestation']['declared_invocation_routes'] = []
            altered = f.save(w, prepared)
            before = bank_bytes(bank)
            index = repo / '.git/index'
            before_index = index.read_bytes() if index.exists() else None
            result = f.call(w, command, 'apply', '--repo', repo, '--proposal', altered, expected=1)
            self.assertFalse(result['canonical_writes_performed'])
            self.assertEqual(bank_bytes(bank), before)
            self.assertEqual(index.read_bytes() if index.exists() else None, before_index)

    def test_head_or_index_change_after_preview_refuses_before_bank_mutation(self):
        for change in ('head', 'index'):
            with self.subTest(change=change), World() as w:
                command, source = f.installed(w)
                repo, bank, native = f.memory(w, oversized=True)
                feature = repo / 'feature.txt'
                feature.write_text('Original independent feature.\n')
                w.git('add', 'feature.txt', cwd=repo)
                w.git('commit', '-m', 'Feature baseline', cwd=repo)
                _, observed = f.inspect(w, command, repo)
                proposal, _ = f.prepare(w, command, source, repo, observed, native)
                feature.write_text('Changed independent feature.\n')
                w.git('add', 'feature.txt', cwd=repo)
                if change == 'head':
                    w.git('commit', '-m', 'New feature history', cwd=repo)
                before = bank_bytes(bank), feature.read_bytes(), (repo / '.git/index').read_bytes()
                head = w.git('rev-parse', 'HEAD', cwd=repo).stdout
                result = f.call(w, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1)
                self.assertFalse(result['canonical_writes_performed'])
                self.assertEqual((bank_bytes(bank), feature.read_bytes(),
                                  (repo / '.git/index').read_bytes()), before)
                self.assertEqual(w.git('rev-parse', 'HEAD', cwd=repo).stdout, head)

    def test_registry_change_after_preview_refuses_apply(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, native = f.memory(w, oversized=True)
            _, observed = f.inspect(w, command, repo)
            proposal, _ = f.prepare(w, command, source, repo, observed, native)
            registry = Path(w.env['AGENT_MEMORY_CONFIG_HOME']) / 'projects.json'
            value = json.loads(registry.read_bytes())
            key = observed['manifest']['identity']['scope_context']['checkout_key']
            value['checkouts'][key]['locations'].append(str(w.inside('additional-checkout-location')))
            registry.write_bytes(f.encoded(value))
            before = bank_bytes(bank), registry.read_bytes()
            result = f.call(w, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1)
            self.assertFalse(result['canonical_writes_performed'])
            self.assertEqual((bank_bytes(bank), registry.read_bytes()), before)

    def test_conflicting_historical_checkout_decisions_refuse_inspection(self):
        with World() as w:
            command, _ = f.installed(w)
            repo, bank, _ = f.memory(w)
            registry = Path(w.env['AGENT_MEMORY_CONFIG_HOME']) / 'projects.json'
            data = json.loads(registry.read_bytes())
            info = (repo / '.git').stat()
            key = str(info.st_dev) + ':' + str(info.st_ino)
            entry = data['checkouts'].pop(key)
            other = json.loads(json.dumps(entry))
            other['authority_name'] = 'different-authority'
            data['checkouts'][str(info.st_dev + 1) + ':' + str(info.st_ino)] = entry
            data['checkouts'][str(info.st_dev + 2) + ':' + str(info.st_ino)] = other
            registry.write_bytes(f.encoded(data))
            before = bank_bytes(bank), registry.read_bytes()
            result = f.call(w, command, 'inspect', '--repo', repo,
                            '--output', w.inside('historical-conflict.json'), expected=1)
            self.assertFalse(result['canonical_writes_performed'])
            self.assertEqual((bank_bytes(bank), registry.read_bytes()), before)

    def test_unreadable_source_after_preview_remains_unmodified_and_pending(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, native = f.memory(w, oversized=True)
            _, observed = f.inspect(w, command, repo)
            proposal, _ = f.prepare(w, command, source, repo, observed, native)
            before = bank_bytes(bank)
            target = bank / 'projectbrief.md'
            mode = target.stat().st_mode & 0o7777
            target.chmod(0)
            try:
                if os.access(target, os.R_OK):
                    self.fail('Permission-inability fixture requires an unprivileged principal')
                result = f.call(w, command, 'apply', '--repo', repo, '--proposal', proposal, expected=1)
                self.assertFalse(result['canonical_writes_performed'])
                self.assertEqual(target.stat().st_mode & 0o7777, 0)
            finally:
                target.chmod(mode)
            self.assertEqual(bank_bytes(bank), before)

    def test_setgid_collaborative_bank_mode_and_group_survive_apply(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, native = f.memory(w, oversized=True)
            bank.chmod(0o2770)
            original = bank.stat()
            self.assertEqual(original.st_mode & 0o7777, 0o2770)
            _, observed = f.inspect(w, command, repo)
            proposal, _ = f.prepare(w, command, source, repo, observed, native)
            result = f.call(w, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertTrue(result['canonical_eligibility'])
            self.assertEqual(bank.stat().st_mode & 0o7777, 0o2770)
            self.assertEqual(bank.stat().st_gid, original.st_gid)

    def test_changed_state_and_xdg_home_cannot_hide_public_apply_hold(self):
        helper = frontier_tests.RecoveryFrontierContract('runTest')
        for change in ('state', 'xdg'):
            with self.subTest(change=change), World() as w:
                command, _, repo, bank, _, inspected, applied = f.small_pending(w)
                self.assertFalse(applied['semantic_recovery_complete'])
                before = bank_bytes(bank)
                if change == 'state':
                    w.env['AGENT_MEMORY_STATE_HOME'] = str(w.inside('other-state'))
                else:
                    w.env.pop('AGENT_MEMORY_STATE_HOME')
                    w.env['XDG_STATE_HOME'] = str(w.inside('other-xdg-state'))
                result, value = helper.recall(w, command, repo, inspected['manifest']['project_id'])
                self.assertNotEqual(result.returncode, 0)
                self.assertNotIn('results', value)
                self.assertEqual(bank_bytes(bank), before)

    def test_copied_checkout_cannot_treat_public_hold_as_complete(self):
        helper = frontier_tests.RecoveryFrontierContract('runTest')
        with World() as w:
            command, _, repo, bank, _, inspected, applied = f.small_pending(w)
            self.assertFalse(applied['semantic_recovery_complete'])
            copied = w.inside('copied-checkout')
            shutil.copytree(repo, copied, symlinks=True)
            copied_bank = copied / bank.relative_to(repo)
            before = bank_bytes(bank), bank_bytes(copied_bank)
            result, value = helper.recall(w, command, copied, inspected['manifest']['project_id'])
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('results', value)
            self.assertEqual((bank_bytes(bank), bank_bytes(copied_bank)), before)

    def test_moved_checkout_cannot_treat_public_hold_as_complete(self):
        helper = frontier_tests.RecoveryFrontierContract('runTest')
        with World() as w:
            command, _, repo, bank, _, inspected, applied = f.small_pending(w)
            self.assertFalse(applied['semantic_recovery_complete'])
            before = bank_bytes(bank)
            moved = w.inside('moved-checkout')
            relative_bank = bank.relative_to(repo)
            shutil.move(str(repo), str(moved))
            w.paths['repo'] = moved
            result, value = helper.recall(w, command, moved, inspected['manifest']['project_id'])
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('results', value)
            self.assertEqual(bank_bytes(moved / relative_bank), before)

    def test_orphaned_public_hold_cannot_be_read_as_empty_frontier(self):
        helper = frontier_tests.RecoveryFrontierContract('runTest')
        with World() as w:
            command, _, repo, bank, _, inspected, applied = f.small_pending(w)
            self.assertFalse(applied['semantic_recovery_complete'])
            directory = f.private_directory(w, bank, inspected['operation_id'])
            index = directory.parent / 'index.json'
            self.assertTrue(index.is_file())
            index.unlink()
            before = bank_bytes(bank), bank_bytes(directory)
            result, value = helper.recall(w, command, repo, inspected['manifest']['project_id'])
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('results', value)
            self.assertEqual((bank_bytes(bank), bank_bytes(directory)), before)

    def test_changed_config_home_cannot_make_held_reader_report_complete(self):
        helper = frontier_tests.RecoveryFrontierContract('runTest')
        with World() as w:
            command, repo, bank, inspected, _ = helper.pending(w)
            before = bank_bytes(bank)
            w.env['AGENT_MEMORY_CONFIG_HOME'] = str(w.inside('different-config-home'))
            result, value = helper.recall(w, command, repo, inspected['manifest']['project_id'])
            self.assertNotEqual(result.returncode, 0)
            self.assertNotIn('results', value)
            self.assertEqual(bank_bytes(bank), before)

    def test_deleting_every_discovery_record_loses_the_pending_obligation(self):
        # This is a documented limitation, not a claim that deleted private
        # state can be discovered without any surviving marker or binding.
        helper = frontier_tests.RecoveryFrontierContract('runTest')
        with World() as w:
            command, repo, bank, inspected, _ = helper.pending(w)
            operation = inspected['manifest']['operation_id']
            base = f.private_directory(w, bank, operation).parent
            bindings = Path(w.env['AGENT_MEMORY_CONFIG_HOME']) / 'layout-recovery-bindings.json'
            self.assertTrue(bindings.exists())
            for name in ('.agent-memory-transaction.md', '.agent-memory-recovery.md'):
                (bank / name).unlink()
            bindings.unlink()
            shutil.rmtree(base)
            result, value = helper.recall(w, command, repo, inspected['manifest']['project_id'])
            self.assertEqual(result.returncode, 0, value)
            self.assertNotEqual(value.get('semantic_coverage'), 'incomplete-recovery')
            self.assertFalse(value.get('pending_recovery_units', 0))

    def test_publication_refuses_unicode_and_case_aliased_authority_sources(self):
        fixture = publication_tests.LayoutPublicationContract('runTest')
        for names in (('alias.md', 'ALIAS.md'), ('caf\u00e9.md', 'cafe\u0301.md')):
            with self.subTest(names=names), World() as w:
                command, _, repo, bank, remote, operation, _ = fixture.public_repair(w)
                peer = w.clone(str(remote), 'alias-peer')
                body = w.inside('alias-blob'); body.write_text('Distinct raw authority source.\n')
                blob = w.git('hash-object', '-w', body, cwd=peer).stdout.strip()
                # Construct Git trees directly: neither the filesystem nor
                # Git's platform-dependent index normalization may fold names.
                def make_tree(entries):
                    return subprocess.run(
                        ['git', 'mktree', '-z'], cwd=peer, env=w.env,
                        input=''.join(entry + '\0' for entry in entries),
                        text=True, capture_output=True, check=True, timeout=60,
                    ).stdout.strip()

                bank_entries = w.git('ls-tree', '-z', 'HEAD:memory-bank', cwd=peer).stdout.split('\0')[:-1]
                bank_entries.extend('100644 blob ' + blob + '\t' + name for name in names)
                bank_tree = make_tree(bank_entries)
                raw_entries = w.git('ls-tree', '-z', bank_tree, cwd=peer).stdout.split('\0')[:-1]
                self.assertTrue(all('100644 blob ' + blob + '\t' + name in raw_entries for name in names))
                root_entries = w.git('ls-tree', '-z', 'HEAD', cwd=peer).stdout.split('\0')[:-1]
                root_entries = [entry for entry in root_entries if entry.partition('\t')[2] != 'memory-bank']
                root_entries.append('040000 tree ' + bank_tree + '\tmemory-bank')
                tree = make_tree(root_entries)
                parent = w.git('rev-parse', 'HEAD', cwd=peer).stdout.strip()
                commit = w.git('commit-tree', tree, '-p', parent, '-m',
                               'Aliased authority sources', cwd=peer).stdout.strip()
                w.git('push', 'origin', commit + ':refs/heads/main', cwd=peer)
                before = bank_bytes(bank)
                index = (repo / '.git/index').read_bytes()
                result = f.call(w, command, 'publish', 'inspect', '--repo', repo,
                                '--operation', operation, '--output', w.inside('alias-refused.json'), expected=1)
                self.assertFalse(result['canonical_writes_performed'])
                self.assertEqual(bank_bytes(bank), before)
                self.assertEqual((repo / '.git/index').read_bytes(), index)
