import concurrent.futures
import hashlib
import json
import importlib.util
import os
from pathlib import Path
import tempfile
import unittest
from unittest import mock

SPEC = importlib.util.spec_from_file_location('boundary_ledger', Path(__file__).resolve().parents[2] / 'skills/agent-memory/hooks/boundary_ledger.py')
ledger = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(ledger)

class BoundaryLedgerTest(unittest.TestCase):
    def test_durable_reservation_and_restart_refusal(self):
        with tempfile.TemporaryDirectory() as root:
            first = ledger.reserve(os.path.realpath(root), 'session', 'turn')
            self.assertEqual(first['status'], 'reserved')
            directory = Path(root) / 'experimental-boundary'
            self.assertEqual(len(list(directory.iterdir())), 2)
            self.assertEqual(ledger.reserve(os.path.realpath(root), 'session', 'turn')['reason'], 'existing-intent')
            self.assertEqual(ledger.reserve(os.path.realpath(root), 'session', 'other')['status'], 'reserved')

    def test_intent_without_attempt_is_uncertain(self):
        with tempfile.TemporaryDirectory() as root:
            directory = Path(root) / 'experimental-boundary'
            directory.mkdir(mode=0o700)
            key = hashlib.sha256(json.dumps(['session', 'turn'], separators=(',', ':')).encode()).hexdigest()
            (directory / (key + '.intent')).write_text('interrupted')
            self.assertEqual(ledger.reserve(os.path.realpath(root), 'session', 'turn')['status'], 'pending')
            self.assertFalse((directory / (key + '.attempt')).exists())

    def test_concurrent_single_reservation(self):
        with tempfile.TemporaryDirectory() as root:
            with concurrent.futures.ThreadPoolExecutor(max_workers=8) as pool:
                results = list(pool.map(lambda _: ledger.reserve(os.path.realpath(root), 'session', 'turn'), range(8)))
            self.assertEqual(sum(x['status'] == 'reserved' for x in results), 1)

    def test_ids_cannot_collide_on_separator(self):
        with tempfile.TemporaryDirectory() as root:
            root = os.path.realpath(root)
            self.assertEqual(ledger.reserve(root, 'a\0b', 'c')['status'], 'reserved')
            self.assertEqual(ledger.reserve(root, 'a', 'b\0c')['status'], 'reserved')

    def test_supported_no_learning_assessment_compacts_and_deduplicates(self):
        supported_path = Path(__file__).resolve().parents[2] / 'skills/agent-memory/hooks/boundary_ledger.py'
        spec = importlib.util.spec_from_file_location('supported_ack_ledger', supported_path)
        supported = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(supported)
        with tempfile.TemporaryDirectory() as root:
            root = os.path.realpath(root)
            self.assertEqual(supported.reserve(root, 's', 'turn')['status'], 'reserved')
            self.assertEqual(supported.reserve(root, 's', 'turn', 'assessed-no-learning')['status'], 'assessed-no-learning')
            self.assertEqual(supported.reserve(root, 's', 'turn')['status'], 'assessed-no-learning')
            self.assertEqual(len(list((Path(root) / 'experimental-boundary' / hashlib.sha256(b's').hexdigest()).iterdir())), 1)
            with self.assertRaises(FileNotFoundError):
                supported.reserve(root, 's', 'missing', 'assessed-no-learning')

    def test_supported_capacity_has_non_destructive_recovery(self):
        from unittest.mock import patch
        supported_path = Path(__file__).resolve().parents[2] / 'skills/agent-memory/hooks/boundary_ledger.py'
        spec = importlib.util.spec_from_file_location('supported_ledger', supported_path)
        supported = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(supported)
        with tempfile.TemporaryDirectory() as root:
            root = os.path.realpath(root)
            with patch.dict(os.environ, {'AGENT_MEMORY_BOUNDARY_LEDGER_LIMIT': '2'}):
                self.assertEqual(supported.reserve(root, 's', 'one')['status'], 'reserved')
                self.assertEqual(supported.reserve(root, 's', 'two')['reason'], 'ledger-capacity')
            with patch.dict(os.environ, {'AGENT_MEMORY_BOUNDARY_LEDGER_LIMIT': '4'}):
                self.assertEqual(supported.reserve(root, 's', 'two')['status'], 'reserved')
                self.assertEqual(len(list((Path(root) / 'experimental-boundary' / hashlib.sha256(b's').hexdigest()).iterdir())), 4)

    def test_rebinding_before_return_fails_pending(self):
        with tempfile.TemporaryDirectory() as root:
            root = os.path.realpath(root)
            real_fsync = os.fsync
            calls = 0
            def swap(fd):
                nonlocal calls
                calls += 1
                real_fsync(fd)
                if calls == 5:
                    path = Path(root) / 'experimental-boundary'
                    path.rename(Path(root) / 'old-boundary')
                    path.mkdir(mode=0o700)
            with mock.patch.object(ledger.os, 'fsync', side_effect=swap):
                with self.assertRaisesRegex(ValueError, 'namespace changed'):
                    ledger.reserve(root, 'session', 'turn')
            self.assertEqual(len(list((Path(root) / 'old-boundary').iterdir())), 2)

    def test_rejects_symlink_and_untrusted_root(self):
        with tempfile.TemporaryDirectory() as root, tempfile.TemporaryDirectory() as other:
            (Path(root) / 'experimental-boundary').symlink_to(other, target_is_directory=True)
            with self.assertRaises(OSError): ledger.reserve(os.path.realpath(root), 'session', 'turn')
            self.assertEqual(list(Path(other).iterdir()), [])
        with tempfile.TemporaryDirectory() as root:
            os.chmod(root, 0o777)
            with self.assertRaises(ValueError): ledger.reserve(os.path.realpath(root), 'session', 'turn')

if __name__ == '__main__': unittest.main()

class PublishedAcknowledgmentContract(unittest.TestCase):
    def test_requires_real_witnessed_publication(self):
        import subprocess
        import sys
        from world import World
        from publication_fixtures import PublicationFixture
        helper = Path(__file__).resolve().parents[2] / 'skills/agent-memory/hooks/boundary_ledger.py'
        with World() as w:
            fixture = PublicationFixture()
            repo, bank, _ = fixture.bootstrap(w)
            fixture.github(w, repo, bank)
            request = fixture.prepared(w, repo)['request']
            root = str(w.root.resolve())
            def invoke(action, repository=repo):
                payload = {'root': root, 'session': 'fixture-session', 'source': 'fixture-source',
                           'action': action, 'repo': str(repository), 'publication_request': request}
                return subprocess.run([sys.executable, '-I', str(helper)], input=json.dumps(payload), text=True,
                                      capture_output=True, env=w.env, timeout=120)
            self.assertEqual(json.loads(invoke('reserve').stdout)['status'], 'reserved')
            before = invoke('published')
            self.assertNotEqual(before.returncode, 0)
            self.assertEqual(json.loads(before.stdout)['status'], 'pending')
            landed = fixture.land(w, repo, request)
            self.assertTrue(landed['publication_verified'])
            after = invoke('published')
            self.assertEqual(after.returncode, 0, after.stderr + after.stdout)
            proof = json.loads(after.stdout)
            self.assertTrue(proof['publication_verified'])
            self.assertFalse(proof['source_publication_link_verified'])
            self.assertEqual(proof['witnessed_commit'], landed['witnessed_commit'])
            retry = invoke('published')
            self.assertEqual(retry.returncode, 0, retry.stdout + retry.stderr)
            self.assertTrue(json.loads(retry.stdout)['already_completed'])
            self.assertTrue(json.loads(retry.stdout)['publication_verified'])
            other = w.repository("other-repository")
            self.assertNotEqual(invoke('published', other).returncode, 0)

class UninitializedBoundaryContract(unittest.TestCase):
    def test_absent_context_skips_and_malformed_bank_remains_pending(self):
        import subprocess
        import sys
        from world import World
        helper = Path(__file__).resolve().parents[2] / 'skills/agent-memory/hooks/boundary_ledger.py'
        with World() as w:
            repo = w.repository()
            outside = w.inside('outside'); outside.mkdir()
            def invoke(location):
                value = {'root': str(w.root.resolve()), 'session': 's', 'source': 'p', 'repo': str(location)}
                result = subprocess.run([sys.executable, '-I', str(helper)], input=json.dumps(value), text=True,
                                        capture_output=True, env=w.env, timeout=30)
                return result.returncode, json.loads(result.stdout)
            for location in (repo, outside):
                rc, result = invoke(location)
                self.assertEqual(rc, 0)
                self.assertEqual(result['status'], 'not-applicable')
            self.assertFalse((w.root/'experimental-boundary').exists())
            (repo/'memory-bank').mkdir()
            (repo/'memory-bank'/'MEMORY.md').write_text('malformed bank\n')
            rc, result = invoke(repo)
            self.assertNotEqual(rc, 0)
            self.assertEqual(result['status'], 'pending')
