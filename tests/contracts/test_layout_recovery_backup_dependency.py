""Public recovery refuses backup-only preservation and stale child restores."""
import copy
import json
import unittest

import layout_recovery_fixtures as f
from world import World


def files(directory):
    return {p.relative_to(directory).as_posix(): p.read_bytes()
            for p in directory.rglob('*') if p.is_file()}


class RecoveryBackupDependencyContract(unittest.TestCase):
    def test_private_backup_does_not_replace_available_canonical_guidance(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, native = f.memory(w, oversized=True)
            _, inspected = f.inspect(w, command, repo)
            operation = inspected['operation_id']
            directory = f.private_directory(w, bank, operation)
            request = f.review_request(w, command, source, repo, inspected, native)
            history = next(path for path in native if path.startswith('history/sources/'))
            original = next(row for row in inspected['manifest']['sources'] if row['path'] == history)
            blob = directory / original['blob']
            self.assertEqual(blob.read_bytes(), (bank / history).read_bytes())
            bad = copy.deepcopy(request)
            unit = next(unit for unit in bad['unit_map']['units']
                        if any(target['path'] == history for target in unit['targets']))
            unit['targets'] = []
            bad['review']['unit_map_sha256'] = f.sha(f.encoded(bad['unit_map']))
            before_bank, before_private = files(bank), files(directory)
            index = (repo / '.git/index').read_bytes()
            output = w.inside('backup-only.json')
            refusal = f.call(w, command, 'prepare', '--repo', repo, '--operation', operation,
                             '--input', f.save(w, bad), '--output', output, expected=1)
            self.assertFalse(refusal['canonical_writes_performed'])
            self.assertFalse(output.exists())
            self.assertEqual(files(bank), before_bank)
            self.assertEqual(files(directory), before_private)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual(blob.read_bytes(), before_bank[history])
            proposal = w.inside('preserved.json')
            f.call(w, command, 'prepare', '--repo', repo, '--operation', operation,
                   '--input', f.save(w, request), '--output', proposal)
            completed = f.call(w, command, 'apply', '--repo', repo, '--proposal', proposal)
            self.assertTrue(completed['semantic_recovery_complete'])
            self.assertEqual((bank / history).read_text(), native[history])
            self.assertEqual(blob.read_bytes(), before_bank[history])

    def test_later_admission_refuses_completed_repair_restore(self):
        with World() as w:
            command, source, repo, bank, _, inspected, applied = f.small_pending(w)
            operation = inspected['operation_id']
            unit_id, = applied['pending_unit_ids']
            directory = f.private_directory(w, bank, operation)
            restore = w.inside('before-admission-restore.json')
            preview = f.call(w, command, 'rollback', '--repo', repo, '--operation', operation,
                             '--output', restore)
            self.assertNotEqual(preview['operation_id'], operation)
            admission, transaction, target = f.admission_proposal(
                w, command, source, repo, bank, inspected, full=True, unit_id=unit_id)
            f.public(w, command, 'admission', 'apply', '--proposal', admission)
            status = f.call(w, command, 'status', '--repo', repo, '--operation', operation)
            self.assertTrue(status['semantic_recovery_complete'])
            self.assertEqual(status['pending_unit_ids'], [])
            self.assertEqual((bank / target).read_bytes(), transaction['files'][target].encode())
            head = json.loads((directory / 'ledger-head.json').read_text())
            self.assertGreater(head['generation'], 0)
            ledger = json.loads((directory / 'ledger' / (str(head['generation']).zfill(8) + '.json')).read_text())
            unit = next(row for row in ledger['units'] if row['unit_id'] == unit_id)
            self.assertEqual(unit['state'], 'admitted')
            link, = unit['terminal_receipts']
            receipt = json.loads((directory / 'resolutions' / (link['resolution_id'] + '.json')).read_text())
            self.assertEqual(receipt['canonical_operation_id'], transaction['operation_id'])
            before_bank, before_private = files(bank), files(directory)
            child = f.private_directory(w, bank, preview['operation_id'])
            before_child = files(child)
            index = (repo / '.git/index').read_bytes()
            refused = f.call(w, command, 'rollback', '--repo', repo, '--proposal', restore, expected=1)
            self.assertFalse(refused['canonical_writes_performed'])
            self.assertEqual(files(bank), before_bank)
            self.assertEqual(files(directory), before_private)
            self.assertEqual(files(child), before_child)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            fresh = w.inside('after-admission-restore.json')
            refused = f.call(w, command, 'rollback', '--repo', repo, '--operation', operation,
                             '--output', fresh, expected=1)
            self.assertFalse(refused['canonical_writes_performed'])
            self.assertFalse(fresh.exists())
            self.assertEqual(files(bank), before_bank)
            self.assertEqual(files(directory), before_private)
            self.assertEqual(files(child), before_child)
            self.assertEqual((repo / '.git/index').read_bytes(), index)

    def test_completed_repair_without_later_admission_can_restore(self):
        with World() as w:
            command, source = f.installed(w)
            repo, bank, native = f.memory(w, oversized=True)
            original = files(bank)
            _, inspected = f.inspect(w, command, repo)
            proposal, _ = f.prepare(w, command, source, repo, inspected, native)
            f.call(w, command, 'apply', '--repo', repo, '--proposal', proposal)
            directory = f.private_directory(w, bank, inspected['operation_id'])
            receipt = (directory / 'receipt.json').read_bytes()
            restore = w.inside('control-restore.json')
            child = f.call(w, command, 'rollback', '--repo', repo,
                           '--operation', inspected['operation_id'], '--output', restore)
            self.assertNotEqual(child['operation_id'], inspected['operation_id'])
            f.call(w, command, 'rollback', '--repo', repo, '--proposal', restore)
            self.assertEqual(files(bank), original)
            self.assertEqual((directory / 'receipt.json').read_bytes(), receipt)
