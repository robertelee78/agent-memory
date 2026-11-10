"""Intended-bank capacity and owned transaction marker regressions."""
import copy
import errno
import json
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch
import uuid
import stat

from world import World
from contracts import test_bank_transactions, test_bank_writes

MARKER = test_bank_transactions.MARKER

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'skills/agent-memory/scripts'))
from agent_memory import bank_capacity, bank_observation, bank_transactions, bank_writes, recovery_frontier, sharing_limits
from agent_memory.frontmatter import ContractError


class BankEligibilityWritesContract(unittest.TestCase):
    def setUp(self):
        self.single = test_bank_writes.BankWritesContract()
        self.transaction = test_bank_transactions.BankTransactionsContract()

    def protected(self, world, repo, bank):
        state = Path(world.env['AGENT_MEMORY_STATE_HOME'])
        evidence = {p.relative_to(state).as_posix(): p.read_bytes()
                    for p in state.rglob('*') if p.is_file()
                    and ('transactions' in p.parts or 'operations' in p.parts)}
        return ({p.relative_to(bank).as_posix(): p.read_bytes()
                 for p in bank.rglob('*') if p.is_file()},
                (repo / '.git/HEAD').read_bytes(), (repo / '.git/index').read_bytes(),
                (repo / 'feature.txt').read_bytes(), evidence)

    def feature(self, world, repo):
        (repo / 'feature.txt').write_text('Unfinished feature must remain intact.\n')
        world.git('add', 'feature.txt', cwd=repo)

    def capacity(self, result, data, path, observed, code='bank-source-byte-capacity', limit=16384):
        self.assertEqual(result.returncode, 1, data)
        self.assertEqual(data['failure_code'], code)
        self.assertEqual(data['failure_phase'], 'bank-eligibility')
        self.assertEqual(data['failure_component'], 'canonical-bank')
        self.assertEqual(data['failure_path'], path)
        self.assertEqual(data['capacity_limit'], limit)
        self.assertEqual(data['capacity_observed'], observed)
        self.assertEqual(data['capacity_measurement'], 'exact')
        self.assertEqual(data['capacity_unit'], 'bytes')
        self.assertNotIn('BODY_SECRET_CANARY', result.stdout + result.stderr)

    def oversized(self):
        # The character count is below the bound; complete UTF-8 bytes exceed it.
        return '# BODY_SECRET_CANARY\n\n' + 'é' * 8190 + '\n'

    def test_transaction_preflight_refuses_incomplete_inventory_and_failed_target_stat(self):
        with World() as w:
            repo, bank, _, _, proposal = self.transaction.fixture(w)
            self.feature(w, repo)
            hidden = bank / 'topics'
            hidden.mkdir()
            occupant = hidden / 'unobserved.bin'
            occupant.write_bytes(b'preserve foreign occupant')
            before = self.protected(w, repo, bank)
            real_scandir = os.scandir

            def inaccessible(path):
                if os.fspath(path) == os.fspath(hidden):
                    raise PermissionError(errno.EACCES, 'inaccessible directory', path)
                return real_scandir(path)

            with patch.object(os, 'scandir', side_effect=inaccessible):
                with self.assertRaises(ContractError):
                    bank_transactions.preflight(bank, proposal['proposal'])
            self.assertEqual(self.protected(w, repo, bank), before)

            class UnclassifiableEntry:
                def __init__(self, entry):
                    self.name = entry.name

                def is_dir(self):
                    raise OSError(errno.EIO, 'unavailable entry type')

            class IncompleteScan:
                def __init__(self, scan):
                    self.scan = scan

                def __enter__(self):
                    self.scan.__enter__()
                    return self

                def __exit__(self, *args):
                    return self.scan.__exit__(*args)

                def __iter__(self):
                    return self

                def __next__(self):
                    entry = next(self.scan)
                    return UnclassifiableEntry(entry) if entry.name == hidden.name else entry

            def unclassifiable(path):
                scan = real_scandir(path)
                return IncompleteScan(scan) if os.fspath(path) == os.fspath(bank) else scan

            with patch.object(os, 'scandir', side_effect=unclassifiable):
                with self.assertRaises(ContractError):
                    bank_transactions.preflight(bank, proposal['proposal'])
            self.assertEqual(self.protected(w, repo, bank), before)

            real_stat = os.stat
            target = bank / 'activeContext.md'

            def failed_stat(path, *args, **kwargs):
                if os.fspath(path) == os.fspath(target):
                    raise OSError(errno.EIO, 'unavailable metadata', path)
                return real_stat(path, *args, **kwargs)

            with patch.object(os, 'stat', side_effect=failed_stat):
                with self.assertRaises(ContractError):
                    bank_transactions.preflight(bank, proposal['proposal'])
            self.assertEqual(self.protected(w, repo, bank), before)

    def test_transaction_preflight_refuses_symlink_inside_runtime_directory(self):
        with World() as w:
            repo, bank, _, _, proposal = self.transaction.fixture(w)
            self.feature(w, repo)
            runtime = bank / '.agent-memory-private'
            runtime.mkdir()
            outside = w.inside('outside')
            outside.write_bytes(b'preserve outside')
            (runtime / 'link').symlink_to(outside)
            before = self.protected(w, repo, bank)
            with self.assertRaises(ContractError):
                bank_transactions.preflight(bank, proposal['proposal'])
            self.assertEqual(self.protected(w, repo, bank), before)
            self.assertTrue((runtime / 'link').is_symlink())
            self.assertEqual(outside.read_bytes(), b'preserve outside')


    def test_render_and_single_preview_reject_complete_utf8_source(self):
        with World() as w:
            repo = w.repository()
            bank = self.single.bank(w, repo)
            self.feature(w, repo)
            before = self.protected(w, repo, bank)
            body = self.oversized()
            amount = len(body.encode('utf-8'))
            self.assertLess(len(body), 16384)
            self.assertGreater(amount, 16384)
            source = w.inside('large-content.md')
            source.write_text(body, encoding='utf-8')
            r, result = self.single.command(w, 'preview', '--repo', repo, '--path',
                                             'activeContext.md', '--content', source)
            self.capacity(r, result, 'activeContext.md', amount)
            self.assertEqual(self.protected(w, repo, bank), before)
            template = json.loads((ROOT / 'skills/agent-memory/templates/router-proposal.example.json').read_bytes())
            template['project-id'] = str(uuid.uuid4())
            template['contexts']['projectbrief.md']['body'] = body
            r, result = self.transaction.command(w, 'render', '--input',
                self.transaction.save(w, template, 'large-render.json'), helper=ROOT / 'scripts/bank_contract.py')
            self.capacity(r, result, 'projectbrief.md', amount)
            self.assertEqual(self.protected(w, repo, bank), before)

    def test_single_apply_revalidates_a_schema_valid_changed_request(self):
        with World() as w:
            bank = self.single.bank(w, w.repository())
            repo = bank.parent
            self.feature(w, repo)
            proposal = self.single.preview(w, bank)
            proposal['proposal']['content'] = self.oversized()
            before = self.protected(w, repo, bank)
            r, result = self.single.apply(w, proposal)
            self.capacity(r, result, 'activeContext.md', len(self.oversized().encode('utf-8')))
            self.assertEqual(self.protected(w, repo, bank), before)

    def test_multifile_preview_and_apply_reject_complete_oversized_map(self):
        with World() as w:
            repo, bank, _, after, proposal = self.transaction.fixture(w)
            self.feature(w, repo)
            before = self.protected(w, repo, bank)
            body = self.oversized()
            desired = dict(after, **{'activeContext.md': body})
            r, result = self.transaction.command(w, 'preview', '--repo', repo, '--files',
                self.transaction.save(w, desired, 'oversized-map.json'), '--operation', 'compact')
            self.capacity(r, result, 'activeContext.md', len(body.encode('utf-8')))
            self.assertEqual(self.protected(w, repo, bank), before)
            changed = copy.deepcopy(proposal)
            changed['proposal']['files'] = desired
            r, result = self.transaction.apply(w, changed)
            self.capacity(r, result, 'activeContext.md', len(body.encode('utf-8')))
            self.assertEqual(self.protected(w, repo, bank), before)

    def test_multifile_capacity_includes_record_metadata(self):
        with World() as w:
            repo, bank, _, after, _ = self.transaction.fixture(w)
            self.feature(w, repo)
            desired = self.transaction.optional_files(
                after, {'topics/metadata.md': '# Short body\n\nUseful evidence.\n'})
            original = desired['topics/metadata.md']
            desired['topics/metadata.md'] = original.replace(
                'Disposable structural fixture; not real admission evidence',
                'BODY_SECRET_CANARY' + 'é' * 8200)
            amount = len(desired['topics/metadata.md'].encode('utf-8'))
            self.assertGreater(amount, 16384)
            before = self.protected(w, repo, bank)
            r, result = self.transaction.command(w, 'preview', '--repo', repo, '--files',
                self.transaction.save(w, desired, 'metadata-map.json'), '--operation', 'compact')
            self.capacity(r, result, 'topics/metadata.md', amount)
            self.assertEqual(self.protected(w, repo, bank), before)

    def test_aggregate_replacement_counts_the_complete_result_not_both_versions(self):
        with World() as w:
            bank = self.single.bank(w, w.repository())
            repo = bank.parent
            self.feature(w, repo)
            old = (bank / 'activeContext.md').read_text()
            replacement = old + '\n' + 'é' * 300
            total = sum(p.stat().st_size for p in bank.iterdir() if p.is_file())
            intended = total - len(old.encode()) + len(replacement.encode())
            desired = {p.name: p.read_text() for p in bank.iterdir() if p.is_file()}
            desired['activeContext.md'] = replacement
            with patch.dict(os.environ, w.env, clear=True):
                single_proposal = bank_writes.preview(repo, 'activeContext.md', replacement)
                multi_proposal = bank_transactions.preview(repo, desired)
            before = self.protected(w, repo, bank)
            # Patch the actual common budget, and the observer's imported copy,
            # so the original observation is legal but the replacement is not.
            with patch.dict(os.environ, w.env, clear=True), \
                 patch.object(sharing_limits, 'MAX_BANK', intended - 1), \
                 patch.object(bank_observation, 'MAX_BYTES', intended - 1):
                for operation in (
                    lambda: bank_writes.preview(repo, 'activeContext.md', replacement),
                    lambda: bank_writes.apply(single_proposal),
                    lambda: bank_transactions.preview(repo, desired),
                    lambda: bank_transactions.apply(multi_proposal),
                ):
                    with self.assertRaises(bank_capacity.BankCapacityError) as raised:
                        operation()
                    error = raised.exception
                    self.assertEqual((error.code, error.path, error.limit, error.observed, error.measurement),
                        ('bank-aggregate-byte-capacity', '.', intended - 1, intended, 'exact'))
            self.assertEqual(self.protected(w, repo, bank), before)
            # At the exact resulting size, replacing the old version is legal.
            with patch.dict(os.environ, w.env, clear=True), \
                 patch.object(sharing_limits, 'MAX_BANK', intended), \
                 patch.object(bank_observation, 'MAX_BYTES', intended):
                prepared = bank_writes.preview(repo, 'activeContext.md', replacement)
                self.assertEqual(prepared['proposal']['content'], replacement)
            self.assertEqual(self.protected(w, repo, bank), before)

    def large_transaction(self, w):
        repo, bank, before, after, _ = self.transaction.fixture(w)
        additions = {f'topics/inventory-evidence-{n:03d}.md': '# Fixture\n\nA bounded record.\n'
                     for n in range(48)}
        before = self.transaction.optional_files(before, additions)
        after = self.transaction.optional_files(after, additions)
        for path, content in before.items():
            target = bank / path
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content)
        r, proposal = self.transaction.command(w, 'preview', '--repo', repo, '--files',
            self.transaction.save(w, after, 'large-inventory.json'), '--operation', 'compact')
        self.assertEqual(r.returncode, 0, proposal)
        self.feature(w, repo)
        return repo, bank, before, after, proposal

    def test_large_inventory_transaction_and_exact_retry(self):
        with World() as w:
            repo, bank, before, after, proposal = self.large_transaction(w)
            # The real marker is inspected at an interrupted write boundary.
            self.transaction.crash(w, proposal, 'transaction-after-marker')
            marker = (bank / MARKER).read_bytes()
            self.assertGreater(len(marker), 4096)
            self.assertEqual(len(json.loads(marker)['base_hashes']), len(before))
            self.transaction.unlock(w, repo)
            recovery = self.transaction.recovery(w, repo, 'finish')
            r, done = self.transaction.recover_apply(w, repo, recovery)
            self.assertEqual(r.returncode, 0, done)
            self.assertTrue(done['verified'])
            self.assertEqual(self.transaction.canonical(bank), after)
            r, replay = self.transaction.apply(w, proposal)
            self.assertEqual(r.returncode, 0, replay)
            self.assertFalse(replay['writes_performed'])
            self.assertEqual(self.transaction.canonical(bank), after)
            self.assertFalse((bank / MARKER).exists())

    def test_large_inventory_ordinary_apply_and_retry(self):
        with World() as w:
            repo, bank, _, after, proposal = self.large_transaction(w)
            index, head = (repo / '.git/index').read_bytes(), (repo / '.git/HEAD').read_bytes()
            r, done = self.transaction.apply(w, proposal)
            self.assertEqual(r.returncode, 0, done)
            self.assertTrue(done['verified'])
            self.assertEqual(self.transaction.canonical(bank), after)
            r, replay = self.transaction.apply(w, proposal)
            self.assertEqual(r.returncode, 0, replay)
            self.assertTrue(replay['already_applied'])
            self.assertFalse(replay['writes_performed'])
            self.assertEqual(self.transaction.canonical(bank), after)
            self.assertEqual((repo / '.git/index').read_bytes(), index)
            self.assertEqual((repo / '.git/HEAD').read_bytes(), head)
            self.assertEqual((repo / 'feature.txt').read_text(), 'Unfinished feature must remain intact.\n')

    def test_large_owned_marker_keeps_frontier_but_unknown_or_changed_bytes_refuse(self):
        with World() as w:
            repo, bank, _, _, proposal = self.large_transaction(w)
            with patch.dict(os.environ, w.env, clear=True):
                before_frontier = recovery_frontier.observe(repo)
            self.transaction.crash(w, proposal, 'transaction-after-marker')
            self.transaction.unlock(w, repo)
            marker = (bank / MARKER).read_bytes()
            before = self.protected(w, repo, bank)
            with patch.dict(os.environ, w.env, clear=True):
                self.assertEqual(recovery_frontier.observe(repo, transaction=marker), before_frontier)
                for witness in (None, marker[:-1], marker[:-1] + b' '):
                    with self.subTest(witness_bytes=len(witness) if witness is not None else None):
                        with self.assertRaises(ContractError):
                            recovery_frontier.observe(repo, transaction=witness)
                sentinel = bank / '.agent-memory-recovery.md'
                sentinel.write_bytes(marker)
                try:
                    with self.assertRaises(ContractError):
                        recovery_frontier.observe(repo, transaction=marker)
                finally:
                    sentinel.unlink()
            self.assertEqual(self.protected(w, repo, bank), before)

    def test_large_marker_rollback_and_unknown_marker_refusal(self):
        with World() as w:
            repo, bank, before, _, proposal = self.large_transaction(w)
            self.transaction.crash(w, proposal, 'transaction-after-write:activeContext.md')
            marker = (bank / MARKER).read_bytes()
            self.assertGreater(len(marker), 4096)
            self.assertEqual(len(json.loads(marker)['base_hashes']), len(before))
            self.transaction.unlock(w, repo)
            saved = self.protected(w, repo, bank)
            r, refused = self.transaction.command(w, 'validate', '--repo', repo,
                                                   helper=ROOT / 'scripts/bank_contract.py')
            self.assertEqual(r.returncode, 1, refused)
            self.assertNotIn('failure_code', refused)
            self.assertEqual(self.protected(w, repo, bank), saved)
            recovery = self.transaction.recovery(w, repo, 'rollback')
            r, done = self.transaction.recover_apply(w, repo, recovery)
            self.assertEqual(r.returncode, 0, done)
            self.assertTrue(done['verified'])
            self.assertEqual(self.transaction.canonical(bank), before)
            r, retry = self.transaction.apply(w, proposal)
            self.assertEqual(r.returncode, 0, retry)
            self.assertEqual(retry['outcome'], 'rolled-back')
            self.assertFalse(retry['writes_performed'])
            self.assertFalse((bank / MARKER).exists())
            # Even a valid-looking but unowned large marker is never adopted.
            (bank / MARKER).write_bytes(marker)
            saved = self.protected(w, repo, bank)
            r, refused = self.transaction.command(w, 'validate', '--repo', repo,
                                                   helper=ROOT / 'scripts/bank_contract.py')
            self.assertEqual(r.returncode, 1, refused)
            self.assertNotIn('failure_code', refused)
            self.assertEqual(self.protected(w, repo, bank), saved)

    @unittest.skipUnless(hasattr(os, 'mkfifo'), 'FIFO requires POSIX support')
    def test_unknown_marker_is_refused_without_opening_its_body(self):
        with World() as w:
            repo, bank, _, _, _ = self.transaction.fixture(w)
            self.feature(w, repo)
            before = self.protected(w, repo, bank)
            marker = bank / MARKER
            os.mkfifo(marker)
            r = w.run([sys.executable, '-I', ROOT / 'scripts/bank_contract.py',
                       'validate', '--repo', repo], check=False, timeout=5)
            result = json.loads(r.stdout)
            self.assertEqual(r.returncode, 1, result)
            self.assertNotIn('failure_code', result)
            self.assertTrue(stat.S_ISFIFO(marker.lstat().st_mode))
            marker.unlink()
            self.assertEqual(self.protected(w, repo, bank), before)


if __name__ == '__main__':
    unittest.main()
