"""Complete bank observation enforces bounded, stable, read-only eligibility."""
import json
import os
from pathlib import Path
import shutil
import sys
import unittest
from unittest.mock import patch

from world import World

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / 'skills/agent-memory/scripts'
sys.path.insert(0, str(SCRIPTS))
from agent_memory import bank_capacity, bank_contract, bank_observation, bank_transactions, bank_writes
from agent_memory.frontmatter import ContractError


class BoundedBankObservation(unittest.TestCase):
    def setUp(self):
        self.world = World()
        self.addCleanup(self.world.close)
        self.environment = patch.dict(os.environ, self.world.env, clear=True)
        self.environment.start()
        self.addCleanup(self.environment.stop)
        self.repo = self.world.paths['repo']
        self.bank = self.repo / 'memory-bank'
        shutil.copytree(ROOT / 'memory-bank', self.bank)

    def files(self):
        return {p.relative_to(self.bank).as_posix(): p.read_bytes()
                for p in self.bank.rglob('*') if p.is_file() and not p.is_symlink()}

    def inspect(self):
        return self.world.run([sys.executable, '-B', SCRIPTS / 'bank_writes.py',
                               'inspect', '--repo', self.repo], check=False)

    def capacity(self, error, code, path, limit, observed, measurement, unit):
        self.assertIsInstance(error, bank_capacity.BankCapacityError)
        self.assertEqual((error.code, error.path, error.limit, error.observed, error.measurement),
                         (code, path, limit, observed, measurement))
        details = error.details()
        self.assertEqual({key: details[key] for key in (
            'failure_code', 'failure_phase', 'failure_component', 'failure_path',
            'capacity_limit', 'capacity_observed', 'capacity_measurement', 'capacity_unit')},
            {'failure_code': code, 'failure_phase': 'bank-eligibility',
             'failure_component': 'canonical-bank', 'failure_path': path,
             'capacity_limit': limit, 'capacity_observed': observed,
             'capacity_measurement': measurement, 'capacity_unit': unit})
        self.assertNotIn('PRIVATE_BODY_CANARY', json.dumps(details))
        self.assertNotIn('PRIVATE_BODY_CANARY', str(error))

    def test_complete_utf8_source_boundary_and_file_only_validator(self):
        source = self.bank / 'projectbrief.md'
        prefix = '# Fixture context\n\nPRIVATE_BODY_CANARY café 🧠\n'
        body = prefix + 'x' * (16384 - len(prefix.encode('utf-8')) - 1) + '\n'
        source.write_bytes(body.encode('utf-8'))
        before = self.files()
        self.assertFalse((self.repo / '.git').exists())
        checked = bank_contract.validate(self.repo)
        self.assertTrue(checked['valid'])
        self.assertEqual(checked['validation_scope'], 'canonical-bank-eligibility')
        self.assertTrue(checked['source_stability_verified'])
        self.assertEqual(self.files(), before)
        checked, hashes = bank_writes.snapshot(self.repo)
        self.assertTrue(checked['valid'])
        self.assertEqual(hashes, bank_transactions.file_hashes(bank_transactions.files_at(self.bank)))
        self.assertEqual(self.files(), before)
        source.write_bytes((body + 'x').encode('utf-8'))
        before = self.files()
        with self.assertRaises(bank_capacity.BankCapacityError) as caught:
            bank_contract.validate(self.repo)
        self.capacity(caught.exception, 'bank-source-byte-capacity', 'projectbrief.md',
                      16384, 16385, 'exact', 'bytes')
        result = self.inspect()
        self.assertNotEqual(result.returncode, 0)
        value = json.loads(result.stdout)
        self.assertEqual(value['failure_code'], caught.exception.code)
        self.assertEqual(value['failure_path'], 'projectbrief.md')
        self.assertEqual(value['capacity_observed'], 16385)
        self.assertEqual(value['capacity_measurement'], 'exact')
        self.assertNotIn('PRIVATE_BODY_CANARY', result.stdout + result.stderr)
        self.assertEqual(self.files(), before)
        self.assertFalse((self.world.paths['state'] / 'agent-memory').exists())

    def test_stable_reported_size_descriptor_is_exact_and_preserved(self):
        source = self.bank / 'projectbrief.md'
        canary = b'PRIVATE_BODY_CANARY'
        original = canary + b'x' * (79054 - len(canary))
        source.write_bytes(original)
        before = self.files()
        with self.assertRaises(bank_capacity.BankCapacityError) as caught:
            bank_writes.snapshot(self.repo)
        self.capacity(caught.exception, 'bank-source-byte-capacity', 'projectbrief.md',
                      16384, 79054, 'exact', 'bytes')
        self.assertEqual(self.files(), before)

    def test_public_snapshot_refuses_real_aggregate_excess(self):
        other = self.files()
        directory = self.bank / 'bulk'
        directory.mkdir()
        body = b'PRIVATE_BODY_CANARY' + b'x' * (16384 - len(b'PRIVATE_BODY_CANARY'))
        for index in range(4097):
            (directory / f'{index}.md').write_bytes(body)
        result = self.inspect()
        self.assertNotEqual(result.returncode, 0)
        value = json.loads(result.stdout)
        self.assertEqual(value['failure_code'], 'bank-aggregate-byte-capacity')
        self.assertEqual(value['failure_phase'], 'bank-eligibility')
        self.assertEqual(value['capacity_limit'], 64 * 1024 * 1024)
        self.assertEqual(value['capacity_observed'], 64 * 1024 * 1024 + 1)
        self.assertEqual(value['capacity_measurement'], 'at-least')
        self.assertEqual(value['capacity_unit'], 'bytes')
        self.assertTrue(value['failure_path'].startswith('bulk/'))
        self.assertNotIn('PRIVATE_BODY_CANARY', result.stdout + result.stderr)
        paths = list(directory.iterdir())
        self.assertEqual(len(paths), 4097)
        self.assertTrue(all(path.read_bytes() == body for path in paths))
        self.assertEqual({path: data for path, data in self.files().items()
                          if not path.startswith('bulk/')}, other)

    def test_complete_map_count_is_exact_and_partial_inventory_is_at_least(self):
        files = {f'topics/{i}.md': 'x' for i in range(65537)}
        with self.assertRaises(bank_capacity.BankCapacityError) as caught:
            bank_contract.validate_files(files)
        self.capacity(caught.exception, 'bank-source-count-capacity', '.',
                      65536, 65537, 'exact', 'sources')
        inventory = self.repo / 'count-inventory'
        inventory.mkdir()
        for name in ('one.md', 'two.md', 'three.md'):
            (inventory / name).write_bytes(b'x')
        with patch.object(bank_observation, 'MAX_FILES', 1):
            with self.assertRaises(bank_capacity.BankCapacityError) as caught:
                bank_observation.scan(inventory, read_sources=False, require_router=False)
        error = caught.exception
        self.capacity(error, 'bank-source-count-capacity', error.path, 1, 2, 'at-least', 'sources')
        self.assertTrue((inventory / error.path).is_file())
        self.assertEqual({p.name: p.read_bytes() for p in inventory.iterdir()},
                         {'one.md': b'x', 'two.md': b'x', 'three.md': b'x'})

    def test_excess_count_source_redirect_stays_an_unsafe_refusal(self):
        inventory = self.repo / 'count-race'
        inventory.mkdir()
        for name in ('one.md', 'two.md'):
            (inventory / name).write_bytes(b'original')
        outside = self.repo / 'outside-source'
        outside.write_bytes(b'PRIVATE_BODY_CANARY')
        original_read = bank_observation.read_source
        reads = 0
        def redirect(bank, relative, limit):
            nonlocal reads
            reads += 1
            if reads == 2:
                path = bank / relative
                path.unlink()
                path.symlink_to(outside)
            return original_read(bank, relative, limit)
        with patch.object(bank_observation, 'MAX_FILES', 1), \
             patch.object(bank_observation, 'read_source', side_effect=redirect):
            with self.assertRaises(ContractError) as caught:
                bank_observation.scan(inventory, read_sources=False, require_router=False)
        self.assertEqual(reads, 2)
        self.assertNotIsInstance(caught.exception, bank_capacity.BankCapacityError)
        self.assertEqual(outside.read_bytes(), b'PRIVATE_BODY_CANARY')

    def test_symlink_and_nonregular_markdown_are_noncapacity_refusals(self):
        path = self.bank / 'outside.md'
        path.symlink_to(self.bank / 'projectbrief.md')
        with self.assertRaises(ContractError) as caught:
            bank_writes.snapshot(self.repo)
        self.assertNotIsInstance(caught.exception, bank_capacity.BankCapacityError)
        self.assertTrue(path.is_symlink())
        path.unlink()
        os.mkfifo(path)
        with self.assertRaises(ContractError) as caught:
            bank_writes.snapshot(self.repo)
        self.assertNotIsInstance(caught.exception, bank_capacity.BankCapacityError)
        self.assertTrue(path.exists())

    def test_parent_replaced_by_symlink_is_not_followed(self):
        nested = self.bank / 'race-dir'
        nested.mkdir()
        (nested / 'source.md').write_text('safe')
        outside = self.repo / 'outside'
        outside.mkdir()
        (outside / 'source.md').write_text('outside')
        moved = self.bank / 'old-topics'
        nested.rename(moved)
        nested.symlink_to(outside, target_is_directory=True)
        with self.assertRaises(ContractError) as caught:
            bank_observation.read_source(self.bank, 'race-dir/source.md', 16384)
        self.assertNotIsInstance(caught.exception, bank_capacity.BankCapacityError)
        self.assertEqual((outside / 'source.md').read_text(), 'outside')
        self.assertEqual((moved / 'source.md').read_text(), 'safe')

    def test_oversized_source_or_parent_replaced_before_binding_recheck(self):
        for change in ('source', 'parent'):
            with self.subTest(change=change):
                nested = self.bank / 'race-dir'
                nested.mkdir()
                source = nested / 'source.md'
                original = b'PRIVATE_BODY_CANARY' + b'x' * 79035
                source.write_bytes(original)
                real_stat = os.stat
                changed = False
                def swap(path, *args, **kwargs):
                    nonlocal changed
                    if not changed and kwargs.get('dir_fd') is not None and path == (
                            'source.md' if change == 'source' else 'race-dir'):
                        changed = True
                        if change == 'source':
                            source.rename(nested / 'old.txt')
                            source.write_bytes(b'replacement')
                        else:
                            nested.rename(self.bank / 'old-topics')
                            nested.mkdir()
                            (nested / 'source.md').write_bytes(b'replacement')
                    return real_stat(path, *args, **kwargs)
                with patch.object(bank_observation.os, 'stat', side_effect=swap):
                    with self.assertRaises(ContractError) as caught:
                        bank_observation.read_source(self.bank, 'race-dir/source.md', 16384)
                self.assertTrue(changed)
                self.assertNotIsInstance(caught.exception, bank_capacity.BankCapacityError)
                saved = nested / 'old.txt' if change == 'source' else self.bank / 'old-topics/source.md'
                self.assertEqual(saved.read_bytes(), original)
                self.assertEqual(source.read_bytes(), b'replacement')
                shutil.rmtree(nested)
                if change == 'parent':
                    shutil.rmtree(self.bank / 'old-topics')

    def test_unknown_markers_are_refused_without_opening(self):
        for name in ('.agent-memory.lock', '.agent-memory-transaction.md'):
            with self.subTest(marker=name):
                marker = self.bank / name
                original = b'PRIVATE_BODY_CANARY' + b'x' * 5000
                marker.write_bytes(original)
                real_open = os.open
                def guarded(path, *args, **kwargs):
                    if Path(path).name == name:
                        self.fail('unknown marker body was opened')
                    return real_open(path, *args, **kwargs)
                with patch.object(bank_observation.os, 'open', side_effect=guarded):
                    with self.assertRaises(ContractError) as caught:
                        bank_contract.validate(self.repo)
                self.assertNotIsInstance(caught.exception, bank_capacity.BankCapacityError)
                self.assertNotIn('PRIVATE_BODY_CANARY', str(caught.exception))
                self.assertEqual(marker.read_bytes(), original)
                marker.unlink()

    def test_exact_owned_large_transaction_marker_and_mismatch(self):
        marker = self.bank / '.agent-memory-transaction.md'
        files = bank_transactions.files_at(self.bank)
        original = bank_transactions.marker_bytes({
            'operation_id': '00000000-0000-4000-8000-000000000000',
            'operation': 'replace',
            'observed_hashes': bank_transactions.file_hashes(files),
            'files': {**files, **{
                f'topics/PRIVATE_BODY_CANARY-{index:03}.md': '# Proposed context\n'
                for index in range(128)}},
        })
        self.assertGreater(len(original), 4096)
        marker.write_bytes(original)
        before = self.files()
        checked = bank_contract.validate(self.repo, owned_transaction=original)
        self.assertTrue(checked['valid'])
        self.assertEqual(self.files(), before)
        with self.assertRaises(ContractError) as caught:
            bank_contract.validate(self.repo, owned_transaction=original[:-1])
        self.assertNotIsInstance(caught.exception, bank_capacity.BankCapacityError)
        self.assertNotIn('PRIVATE_BODY_CANARY', str(caught.exception))
        self.assertEqual(self.files(), before)

    def test_owned_marker_replacement_during_read_is_refused(self):
        for name, argument in (('.agent-memory.lock', 'owned_lock'),
                               ('.agent-memory-transaction.md', 'owned_transaction')):
            with self.subTest(marker=name):
                marker = self.bank / name
                original = b'PRIVATE_BODY_CANARY' + b'x' * 5000
                marker.write_bytes(original)
                inode = marker.stat().st_ino
                saved = self.repo / 'old-marker'
                real_fstat = os.fstat
                changed = False
                def swap(fd):
                    nonlocal changed
                    info = real_fstat(fd)
                    if not changed and info.st_ino == inode:
                        changed = True
                        marker.rename(saved)
                        marker.write_bytes(b'foreign marker')
                    return info
                with patch.object(bank_observation.os, 'fstat', side_effect=swap):
                    with self.assertRaises(ContractError) as caught:
                        bank_contract.validate(self.repo, **{argument: original})
                self.assertTrue(changed)
                self.assertNotIsInstance(caught.exception, bank_capacity.BankCapacityError)
                self.assertEqual(saved.read_bytes(), original)
                self.assertEqual(marker.read_bytes(), b'foreign marker')
                marker.unlink()
                saved.unlink()

    def test_changed_source_refused_at_recheck(self):
        original_scan = bank_observation.scan
        calls = 0
        def changing(*args, **kwargs):
            nonlocal calls
            value = original_scan(*args, **kwargs)
            calls += 1
            if calls == 1:
                (self.bank / 'projectbrief.md').write_text('changed after observation')
            return value
        with patch.object(bank_observation, 'scan', side_effect=changing):
            with self.assertRaises(ContractError) as caught:
                bank_writes.snapshot(self.repo)
        self.assertNotIsInstance(caught.exception, bank_capacity.BankCapacityError)
        self.assertEqual((self.bank / 'projectbrief.md').read_text(), 'changed after observation')


if __name__ == '__main__':
    unittest.main()
