"""Installed explicit migration/recovery with production and test-only codecs."""
import hashlib
import json
from pathlib import Path
import sys
import unittest

from world import World
from migration_fixtures import (ROOT, LEGACY, NATIVE, installed, memory, snapshot,
                                save, call, preview, unlock, recover)


class ExplicitMigrationContract(unittest.TestCase):
    def test_production_native_noop_and_unsupported_refusal_do_not_create_backup_or_change_bank(self):
        with World() as w:
            command, source = installed(w, codecs=False)
            repo, bank, _ = memory(w, legacy=False); before = snapshot(bank)
            value = preview(w, command, repo)
            self.assertEqual(value['state'], 'already-current'); self.assertFalse(value['backup_created'])
            self.assertFalse(value['canonical_writes_performed']); self.assertEqual(snapshot(bank), before)
            self.assertEqual(json.loads((source/'schemas/migrations.json').read_text())['steps'], [])
            self.assertEqual(json.loads((source/'schemas/runtime-compatibility.json').read_text())['migrator_source_schemas'], [])
            path = bank/'MEMORY.md'; path.write_text(path.read_text().replace(NATIVE, LEGACY))
            before = snapshot(bank)
            denied = call(w, command, 'preview', '--repo', repo, '--to', NATIVE, expected=1)
            self.assertFalse(denied['verified']); self.assertEqual(snapshot(bank), before)
            self.assertFalse(list(w.paths['state'].rglob('backup.json')))

    def test_complete_multistep_conversion_has_private_complete_backup_and_historical_replay_preserves_new_edits(self):
        with World() as w:
            command, source = installed(w); repo, bank, _ = memory(w)
            before = snapshot(bank); package = snapshot(source)
            p = preview(w, command, repo); self.assertEqual(snapshot(bank), before)
            self.assertEqual(p['proposal']['chain'], ['test-only-first', 'test-only-second'])
            self.assertFalse(p['backup_created'])
            done = call(w, command, 'apply', '--proposal', save(w, p))
            self.assertTrue(done['verified']); self.assertTrue(done['receipt']['outcome_directly_readable'])
            self.assertTrue(done['receipt']['outcome_format_revalidated'])
            self.assertEqual((bank/'projectbrief.md').stat().st_mode & 0o777, 0o640)
            self.assertIn(NATIVE, (bank/'MEMORY.md').read_text()); self.assertIn('# Project', (bank/'projectbrief.md').read_text())
            backup = Path(done['backup_path']); self.assertEqual(backup.stat().st_mode & 0o777, 0o600)
            saved = json.loads(backup.read_text()); self.assertEqual(saved['files']['MEMORY.md'].count(LEGACY), 1)
            self.assertEqual(snapshot(source), package)
            original_receipt = Path(done['receipt_path']).read_bytes()
            replay = call(w, command, 'apply', '--proposal', save(w, p))
            self.assertTrue(replay['receipt_is_historical']); self.assertFalse(replay['canonical_writes_performed'])
            path = bank/'projectbrief.md'; path.write_text(path.read_text()+'\nNew later learning.\n')
            newer = snapshot(bank)
            replay = call(w, command, 'apply', '--proposal', save(w, p))
            self.assertTrue(replay['receipt_is_historical']); self.assertFalse(replay['still_current'])
            self.assertEqual(snapshot(bank), newer); self.assertEqual(Path(done['receipt_path']).read_bytes(), original_receipt)

    def test_completed_restore_is_new_transaction_and_retained_helper_works_without_global_install(self):
        with World() as w:
            command, source = installed(w); repo, bank, _ = memory(w); before = snapshot(bank)
            done = call(w, command, 'apply', '--proposal', save(w, preview(w, command, repo)))
            receipt = Path(done['receipt_path']); original_receipt = receipt.read_bytes()
            operation = done['receipt']['operation_id']
            p = call(w, command, 'restore', '--repo', repo, '--operation', operation)
            self.assertNotEqual(p['proposal']['operation_id'], operation)
            retained = Path(done['retained_helper']); retained_package = retained.parents[1]
            retained_before = snapshot(retained_package)
            # Remove the only installed wrapper; invoke the retained trusted helper
            # explicitly while the global command is unavailable.
            command.unlink()
            pfile = save(w, p, 'restore.json')
            result = w.run([sys.executable, '-I', retained, 'restore', '--repo', repo, '--proposal', pfile], check=False)
            restored = json.loads(result.stdout); self.assertEqual(result.returncode, 0, restored)
            self.assertTrue(restored['verified']); self.assertEqual(snapshot(bank), before)
            self.assertEqual(receipt.read_bytes(), original_receipt)
            self.assertEqual(snapshot(retained_package), retained_before)
            self.assertNotEqual(restored['receipt_path'], str(receipt))

    def test_stale_proposal_mode_edit_and_new_learning_after_completion_refuse_without_overwrite(self):
        with World() as w:
            command, _ = installed(w); repo, bank, _ = memory(w)
            p = preview(w, command, repo); path = bank/'projectbrief.md'; path.chmod(0o600)
            before = snapshot(bank)
            denied = call(w, command, 'apply', '--proposal', save(w, p), expected=1)
            self.assertFalse(denied['verified']); self.assertEqual(snapshot(bank), before)
            done = call(w, command, 'apply', '--proposal', save(w, preview(w, command, repo)))
            path.write_text(path.read_text()+'\nPreserve newer correction.\n'); before = snapshot(bank)
            call(w, command, 'restore', '--repo', repo, '--operation', done['receipt']['operation_id'], expected=1)
            self.assertEqual(snapshot(bank), before)

    def test_interrupted_conversion_recovers_finish_or_original_rollback_and_lost_response_is_safe(self):
        stages = ('migration-before-backup', 'migration-after-backup', 'migration-after-marker',
                  'migration-after-write:projectbrief.md', 'migration-before-receipt',
                  'migration-after-receipt', 'migration-after-marker-removal')
        for mode in ('finish', 'rollback'):
            for stage in stages:
                if mode == 'rollback' and stage in ('migration-after-receipt', 'migration-after-marker-removal'):
                    continue  # A finished conversion needs a NEW restoration transaction.
                with self.subTest(mode=mode, stage=stage), World() as w:
                    command, _ = installed(w); repo, bank, _ = memory(w); before = snapshot(bank)
                    p = preview(w, command, repo); operation = p['proposal']['operation_id']
                    call(w, command, 'apply', '--proposal', save(w, p), expected=86,
                         env={'AGENT_MEMORY_FAULT_STAGE': stage, 'AGENT_MEMORY_FAULT_MODE': 'exit'})
                    unlock(w, command, repo)
                    if (bank/'.agent-memory-transaction.md').exists():
                        normal = w.run([command, '--json', 'bank', 'validate', '--repo', repo], check=False)
                        self.assertNotEqual(normal.returncode, 0)
                    done = recover(w, command, repo, operation, mode)
                    self.assertTrue(done['verified']); self.assertFalse((bank/'.agent-memory-transaction.md').exists())
                    if mode == 'rollback':
                        self.assertEqual(snapshot(bank), before)
                    else:
                        self.assertIn(NATIVE, (bank/'MEMORY.md').read_text())

    def test_receipt_backup_pin_corruption_refuses_historical_verification_and_completed_restore(self):
        for bad in (None, {'pretend': 'digest'}, 'not-a-digest', '0'*64):
            with self.subTest(pin=bad), World() as w:
                command, _ = installed(w); repo, bank, _ = memory(w)
                p = preview(w, command, repo)
                done = call(w, command, 'apply', '--proposal', save(w, p))
                receipt = Path(done['receipt_path']); value = json.loads(receipt.read_text())
                value['backup_sha256'] = bad; receipt.write_text(json.dumps(value)); before = snapshot(bank)
                call(w, command, 'apply', '--proposal', save(w, p), expected=1)
                call(w, command, 'restore', '--repo', repo, '--operation', done['receipt']['operation_id'], expected=1)
                self.assertEqual(snapshot(bank), before)

    def test_interrupted_unknown_new_edits_and_corrupt_backup_never_get_overwritten(self):
        for change in ('bytes', 'mode', 'extra', 'backup'):
            with self.subTest(change=change), World() as w:
                command, _ = installed(w); repo, bank, _ = memory(w)
                p = preview(w, command, repo)
                call(w, command, 'apply', '--proposal', save(w, p), expected=86,
                     env={'AGENT_MEMORY_FAULT_STAGE': 'migration-after-write:projectbrief.md', 'AGENT_MEMORY_FAULT_MODE': 'exit'})
                unlock(w, command, repo)
                if change == 'bytes':
                    path = bank/'projectbrief.md'; path.write_text(path.read_text()+'\nPreserve a consequential correction.\n')
                elif change == 'mode':
                    (bank/'projectbrief.md').chmod(0o600)
                elif change == 'extra':
                    (bank/'new-learning.md').write_text('# Preserve newly captured learning\n')
                else:
                    backup = Path(p['backup_path']); value = json.loads(backup.read_text())
                    value['files']['projectbrief.md'] += '\nCorrupt backup participant.\n'
                    backup.write_text(json.dumps(value))
                before = snapshot(bank)
                call(w, command, 'recover', '--repo', repo, '--operation', p['proposal']['operation_id'],
                     '--mode', 'rollback', expected=1)
                self.assertEqual(snapshot(bank), before)

    def test_restoration_interruption_has_its_own_recovery_and_preserves_original_finished_receipt(self):
        for mode in ('finish', 'rollback'):
            with self.subTest(mode=mode), World() as w:
                command, _ = installed(w); repo, bank, _ = memory(w); original = snapshot(bank)
                done = call(w, command, 'apply', '--proposal', save(w, preview(w, command, repo)))
                converted = snapshot(bank); receipt = Path(done['receipt_path']); receipt_bytes = receipt.read_bytes()
                p = call(w, command, 'restore', '--repo', repo, '--operation', done['receipt']['operation_id'])
                call(w, command, 'restore', '--repo', repo, '--proposal', save(w, p), expected=86,
                     env={'AGENT_MEMORY_FAULT_STAGE': 'migration-after-write:projectbrief.md', 'AGENT_MEMORY_FAULT_MODE': 'exit'})
                unlock(w, command, repo)
                restored = recover(w, command, repo, p['proposal']['operation_id'], mode)
                self.assertTrue(restored['verified']); self.assertEqual(snapshot(bank), original if mode == 'finish' else converted)
                self.assertEqual(receipt.read_bytes(), receipt_bytes)

    def test_changed_state_root_inside_runtime_refuses_before_any_runtime_or_bank_write(self):
        for operation in ('preview', 'apply', 'historical', 'recover'):
            with self.subTest(operation=operation), World() as w:
                command, source = installed(w); repo, bank, _ = memory(w)
                p = preview(w, command, repo)
                if operation == 'historical':
                    call(w, command, 'apply', '--proposal', save(w, p))
                before_bank, before_code = snapshot(bank), snapshot(source)
                w.env['AGENT_MEMORY_STATE_HOME'] = str(source/'unsafe-state-inside-runtime')
                if operation == 'preview':
                    call(w, command, 'preview', '--repo', repo, '--to', NATIVE, expected=1)
                elif operation == 'recover':
                    call(w, command, 'recover', '--repo', repo, '--operation', p['proposal']['operation_id'],
                         '--mode', 'finish', expected=1)
                else:
                    call(w, command, 'apply', '--proposal', save(w, p), expected=1)
                self.assertEqual(snapshot(source), before_code); self.assertEqual(snapshot(bank), before_bank)

    def test_fresh_and_existing_install_check_and_apply_refuse_unsupported_bank_without_activation(self):
        import shutil
        for existing in (False, True):
            for check in (False, True):
                with self.subTest(existing=existing, check=check), World() as w:
                    if existing:
                        command, source = installed(w, codecs=False)
                    else:
                        source = w.inside('plain-native-source'); shutil.copytree(ROOT/'skills/agent-memory', source)
                    repo, bank, _ = memory(w)
                    before = snapshot(bank); code_before = snapshot(source)
                    wrappers = snapshot(w.paths['home']/'.local/bin')
                    args = [sys.executable, '-I', ROOT/'scripts/installer.py', 'install',
                            '--development-source', source, '--presets', 'none']
                    if check: args.append('--check')
                    result = w.run(args, check=False); value = json.loads(result.stdout)
                    self.assertEqual(result.returncode, 1, value); self.assertFalse(value['verified'])
                    self.assertEqual(snapshot(bank), before); self.assertEqual(snapshot(source), code_before)
                    self.assertEqual(snapshot(w.paths['home']/'.local/bin'), wrappers)

    def test_migration_preserves_uncommitted_feature_code_index_head_and_rejects_generic_recovery(self):
        with World() as w:
            command, _ = installed(w); repo, bank, _ = memory(w)
            code = repo/'feature.py'; code.write_text('initial = True\n')
            w.git('add', 'feature.py', cwd=repo); w.git('commit', '-m', 'Disposable feature base', cwd=repo)
            code.write_text('staged_change = True\n'); w.git('add', 'feature.py', cwd=repo)
            code.write_text('staged_change = True\nworking_change = True\n')
            before_code = code.read_bytes(); before_index = (repo/'.git/index').read_bytes()
            before_head = w.git('rev-parse', 'HEAD', cwd=repo).stdout
            p = preview(w, command, repo)
            call(w, command, 'apply', '--proposal', save(w, p), expected=86,
                 env={'AGENT_MEMORY_FAULT_STAGE':'migration-after-marker','AGENT_MEMORY_FAULT_MODE':'exit'})
            unlock(w, command, repo); paused = snapshot(bank)
            generic = w.run([command, '--json', 'transaction', 'recover', '--repo', repo], check=False)
            self.assertEqual(generic.returncode, 1); self.assertEqual(snapshot(bank), paused)
            self.assertTrue(recover(w, command, repo, p['proposal']['operation_id'], 'finish')['verified'])
            self.assertEqual(code.read_bytes(), before_code); self.assertEqual((repo/'.git/index').read_bytes(), before_index)
            self.assertEqual(w.git('rev-parse', 'HEAD', cwd=repo).stdout, before_head)

    def test_live_lock_and_immediate_pre_replace_mode_change_are_detected_without_overwrite(self):
        import time
        with World() as w:
            command, _ = installed(w); repo, bank, _ = memory(w)
            p = preview(w, command, repo); rendezvous = w.inside('migration-rendezvous')
            stage = 'transaction-before-recheck:projectbrief.md'
            w.env.update(AGENT_MEMORY_FAULT_STAGE=stage, AGENT_MEMORY_FAULT_MODE='pause', AGENT_MEMORY_FAULT_DIR=str(rendezvous))
            child = w.spawn([command, '--json', 'migrate', 'apply', '--proposal', save(w, p)])
            ready = rendezvous/(stage+'.ready'); release = rendezvous/(stage+'.release')
            deadline = time.monotonic()+8
            while not ready.exists() and time.monotonic() < deadline:
                if child.poll() is not None: break
                time.sleep(0.01)
            self.assertTrue(ready.exists(), 'Writer did not reach the owned pre-replacement rendezvous')
            call(w, command, 'preview', '--repo', repo, '--to', NATIVE, expected=1)
            path = bank/'projectbrief.md'; original_bytes = path.read_bytes(); path.chmod(0o600)
            release.touch(); out, err = child.communicate(timeout=20)
            self.assertEqual(child.returncode, 1, out+err); value = json.loads(out)
            self.assertIsNone(value['canonical_writes_performed']); self.assertTrue(value['canonical_state_requires_inspection'])
            self.assertEqual(path.read_bytes(), original_bytes); self.assertEqual(path.stat().st_mode & 0o777, 0o600)
            self.assertTrue((bank/'.agent-memory-transaction.md').exists())


if __name__ == '__main__':
    unittest.main()
