"""Public recovery identity and private-discovery boundary contracts."""
import os
from pathlib import Path
import shutil
import subprocess
import sys
import unittest

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
