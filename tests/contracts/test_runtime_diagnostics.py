"""Exercise public diagnostics in isolated homes and actual installations."""
import fcntl
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from runtime_update_fixtures import ROOT, PACK, fixture, bank, snapshot, receipt_path


def call(world, operation, *options, receipt=None, expected=0):
    command = ([Path(receipt['bin_dir']) / 'agent-memory'] if receipt else
               [sys.executable, '-I', ROOT / 'scripts/diagnose.py'])
    result = world.run([*command, operation, '--json', *options], check=False, timeout=120)
    if result.returncode != expected:
        raise AssertionError((result.returncode, result.stdout, result.stderr))
    value = json.loads(result.stdout)
    assert not value['writes_performed'] and not value['repairs_performed'] and not value['network_performed']
    return value


def component(value, name):
    return [v for v in value['checks'] if v['component'] == name]


class RuntimeDiagnosticsContract(unittest.TestCase):
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
            copy = world.paths['codex'] / 'skills/agent-memory/templates/activation-markdown.example.txt'
            copy.write_text('User replacement.\n')
            before = snapshot(world.root)
            value = call(world, 'doctor', '--repo', world.paths['repo'], expected=1)
            failed = {v['component'] for v in value['failures']}
            self.assertIn('runtime.target.wrapper', failed); self.assertIn('runtime.target.copy', failed)
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
