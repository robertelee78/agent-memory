"""Public snapshots must refuse excessive inputs before unbounded reads."""
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
SCRIPTS = ROOT / 'skills/agent-memory/scripts'
sys.path.insert(0, str(SCRIPTS))
from agent_memory import bank_observation, bank_transactions, bank_writes
from agent_memory.frontmatter import ContractError


class BoundedBankObservation(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.repo = Path(self.temp.name)
        self.bank = self.repo / 'memory-bank'
        shutil.copytree(ROOT / 'memory-bank', self.bank)

    def tearDown(self):
        self.temp.cleanup()

    def inspect(self):
        return subprocess.run([sys.executable, '-B', str(SCRIPTS / 'bank_writes.py'),
                               'inspect', '--repo', str(self.repo)], capture_output=True, text=True)

    def test_public_snapshot_refuses_oversized_source_without_modification(self):
        source = self.bank / 'projectbrief.md'
        original = b'x' * (20 * 1024 * 1024)
        source.write_bytes(original)
        result = self.inspect()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn('exceeds 16KiB', json.loads(result.stdout)['error'])
        self.assertEqual(source.stat().st_size, len(original))
        self.assertEqual(source.read_bytes(), original)

    def test_public_snapshot_refuses_real_aggregate_excess(self):
        directory = self.bank / 'topics'
        directory.mkdir()
        for index in range(4097):
            (directory / f"{index}.md").write_bytes(b"x" * 16384)
        result = self.inspect()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("exceeds 64MiB", json.loads(result.stdout)["error"])
        self.assertEqual(len(list(directory.iterdir())), 4097)

    def test_supported_limit_and_transaction_hashes(self):
        source = self.bank / 'projectbrief.md'
        source.write_bytes(b'x' * 16384)
        checked, hashes = bank_writes.snapshot(self.repo)
        self.assertTrue(checked['valid'])
        self.assertEqual(hashes, bank_transactions.file_hashes(bank_transactions.files_at(self.bank)))

    def test_aggregate_bound_refuses_before_loading_more(self):
        with patch.object(bank_observation, 'MAX_BYTES', 1024):
            with self.assertRaisesRegex(ContractError, '64MiB'):
                bank_writes.snapshot(self.repo)

    def test_symlink_and_nonregular_markdown_refused(self):
        (self.bank / 'outside.md').symlink_to(self.bank / 'projectbrief.md')
        with self.assertRaisesRegex(ContractError, 'symlink'):
            bank_writes.snapshot(self.repo)
        (self.bank / 'outside.md').unlink()
        os.mkfifo(self.bank / 'outside.md')
        with self.assertRaisesRegex(ContractError, 'ordinary'):
            bank_writes.snapshot(self.repo)

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
            with self.assertRaisesRegex(ContractError, 'changed'):
                bank_writes.snapshot(self.repo)


if __name__ == '__main__':
    unittest.main()
