"""Installed diagnostics distinguish authentication from mechanical lifecycle proof."""
import fcntl
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from identity_fixtures import bind
from runtime_update_fixtures import ROOT, fixture, bank, snapshot, receipt_path


class LifecycleSecurityContract(unittest.TestCase):
    def diagnostic(self, w, cli, operation, repo=None, expected=0):
        args = [cli, '--json', operation]
        if repo is not None and operation != 'version': args += ['--repo', repo]
        result = w.run(args, check=False, timeout=120)
        value = json.loads(result.stdout)
        self.assertEqual(result.returncode, expected, value)
        self.assertFalse(value['writes_performed']); self.assertFalse(value['network_performed'])
        self.assertFalse(value['repairs_performed'])
        security = value['security']
        self.assertEqual(security['publisher_authentication'], 'not_established')
        self.assertFalse(security['signature_verification_performed'])
        self.assertFalse(security['executing_verifier_authenticated'])
        self.assertFalse(security['platform_acls_audited'])
        return value

    def test_managed_and_explicit_source_have_distinct_recorded_selection_without_signature_or_write_claim(self):
        for kind in ('managed', 'development'):
            with self.subTest(kind=kind), World() as w:
                if kind == 'managed':
                    _, _, receipt = fixture(w)
                else:
                    source = w.inside('explicit operator source')
                    shutil.copytree(ROOT/'skills/agent-memory', source)
                    result = w.run([sys.executable, '-I', ROOT/'scripts/installer.py', 'install',
                                    '--development-source', source, '--presets', 'none'])
                    receipt = json.loads(result.stdout)['receipt']
                cli = Path(receipt['bin_dir'])/'am'; repo = w.repository(); bank(w, repo); bind(w, repo)
                (repo/'feature.py').write_text('unfinished = True\n'); w.git('add', 'feature.py', cwd=repo)
                w.env.pop('PYTHONDONTWRITEBYTECODE', None)
                before = snapshot(w.root)
                for operation in ('version', 'status', 'doctor'):
                    value = self.diagnostic(w, cli, operation, repo)
                    selection = value['security']['recorded_source_selection']
                    self.assertEqual(selection, 'unsigned_managed_git' if kind == 'managed' else 'operator_selected_development')
                    if operation == 'doctor':
                        warning = next(c for c in value['checks'] if c['component'] == 'distribution.trust')
                        self.assertEqual(warning['state'], 'warning')
                        self.assertIn('unsigned managed Git' if kind == 'managed' else 'operator-selected development/custom', warning['detail'])
                    self.assertEqual(snapshot(w.root), before)
                human = w.run([cli, '--version'], check=False)
                self.assertEqual(human.returncode, 0, human.stdout+human.stderr)
                self.assertNotIn('publisher_authentication', human.stdout)
                self.assertNotIn('platform_acls_audited', human.stdout)
                self.assertEqual(snapshot(w.root), before)

    def test_corrupt_or_spoofed_receipt_stays_unauthenticated_and_preserved_on_failures(self):
        for case in ('malformed', 'spoofed-authentication'):
            with self.subTest(case=case), World() as w:
                _, _, receipt = fixture(w); cli = Path(receipt['bin_dir'])/'am'; path = receipt_path(w)
                if case == 'malformed':
                    path.write_text('{"schema":"invalid-receipt","publisher_authenticated":true}\n')
                else:
                    value = json.loads(path.read_text()); value['source']['publisher_authenticated'] = True
                    value['source']['signature_verified'] = True; path.write_text(json.dumps(value))
                before = snapshot(w.root)
                for operation in ('version', 'status', 'doctor'):
                    value = self.diagnostic(w, cli, operation, w.paths['repo'], expected=1)
                    self.assertEqual(value['security']['recorded_source_selection'], 'unavailable')
                    self.assertTrue(value['failures']); self.assertEqual(snapshot(w.root), before)

    def test_real_busy_writer_defers_ownership_checks_without_authenticating_or_stealing_lock(self):
        with World() as w:
            _, _, receipt = fixture(w); cli = Path(receipt['bin_dir'])/'am'
            lock = receipt_path(w).parent/'lock'
            with lock.open('rb') as stream:
                fcntl.flock(stream, fcntl.LOCK_EX | fcntl.LOCK_NB)
                before = snapshot(w.root)
                value = self.diagnostic(w, cli, 'doctor', w.paths['repo'])
                self.assertEqual(value['security']['recorded_source_selection'], 'unsigned_managed_git')
                self.assertTrue(any(c['component'] == 'runtime.lock' and c['state'] == 'warning' for c in value['checks']))
                self.assertFalse(any(c['component'].startswith('runtime.target.') for c in value['checks']))
                self.assertEqual(snapshot(w.root), before)

    def test_outside_helper_without_receipt_or_with_invalid_repository_keeps_negative_qualifications(self):
        with World() as w:
            before = snapshot(w.root)
            for operation, extra, expected in [('version', [], 0), ('status', ['--repo', w.inside('missing repository')], 1)]:
                result = w.run([sys.executable, '-I', ROOT/'scripts/diagnose.py', operation, *extra], check=False)
                value = json.loads(result.stdout); self.assertEqual(result.returncode, expected, value)
                self.assertEqual(value['security']['recorded_source_selection'], 'unavailable')
                self.assertEqual(value['security']['publisher_authentication'], 'not_established')
                self.assertFalse(value['security']['signature_verification_performed'])
                self.assertFalse(value['security']['executing_verifier_authenticated'])
                self.assertFalse(value['security']['platform_acls_audited'])
                self.assertEqual(snapshot(w.root), before)


if __name__ == '__main__':
    unittest.main()
