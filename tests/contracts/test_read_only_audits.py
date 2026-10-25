"""Public audits preserve full runtime inventories without ambient -B guards."""
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from runtime_update_fixtures import ROOT, fixture, bank, snapshot
from identity_fixtures import bind


class ReadOnlyAudits(unittest.TestCase):
    def test_bytecode_prefix_keeps_isolation_dispatch_and_shell_restrictions(self):
        for command in ('python3 -B "$SKILL_DIR/scripts/checkpoint.py" inspect',
                        'python3 -B -I "$SKILL_DIR/scripts/checkpoint.py" nonexistent',
                        'python3 -B -I "$SKILL_DIR/scripts/checkpoint.py" inspect; touch /tmp/never-execute'):
            with self.subTest(command=command), World() as w:
                pack = w.inside('candidate')
                shutil.copytree(ROOT / 'skills/agent-memory', pack)
                guide = pack / 'checkpoint/GUIDE.md'
                guide.write_text(guide.read_text() + '\n```sh\n' + command + '\n```\n')
                before = snapshot(w.root)
                r = w.run([sys.executable, '-B', '-I', ROOT / 'scripts/validate_pack.py',
                           '--pack', pack], check=False)
                self.assertNotEqual(r.returncode, 0)
                self.assertFalse(json.loads(r.stdout)['valid'])
                self.assertEqual(snapshot(w.root), before)

    def test_repeated_installed_and_documented_source_audits_preserve_every_byte_and_mode(self):
        for mode in ('copy', 'link'):
            with self.subTest(mode=mode), World() as w:
                _, _, receipt = fixture(w, mode)
                repo = w.repository()
                bank(w, repo)
                bind(w, repo)
                cli = Path(receipt['bin_dir']) / 'am'
                pack = Path(receipt['source']['pack'])
                # These installed invocations get neither -B nor an ambient
                # suppression variable. Their entrypoint must provide the guard.
                w.env.pop('PYTHONDONTWRITEBYTECODE', None)
                self.assertNotIn('PYTHONPYCACHEPREFIX', w.env)
                before = snapshot(w.root)
                commands = [([cli, '--version'], False),
                    ([cli, 'version', '--json'], True),
                    ([cli, 'status', '--repo', repo], False),
                    ([cli, 'status', '--repo', repo, '--json'], True),
                    ([cli, 'doctor', '--repo', repo], False),
                    ([cli, 'doctor', '--repo', repo, '--json'], True),
                    ([sys.executable, '-I', pack / 'scripts/diagnose.py',
                      'doctor', '--repo', repo, '--json'], True),
                    ([sys.executable, '-B', '-I', pack / 'scripts/diagnose.py',
                      'doctor', '--repo', repo, '--json'], True),
                    ([sys.executable, '-B', '-I', pack / 'scripts/artifact.py',
                      'inspect', '--pack', pack], True),
                    ([sys.executable, '-B', '-I', pack / 'scripts/validate_pack.py',
                      '--pack', pack], True)]
                for _ in range(2):
                    for args, structured in commands:
                        r = w.run(args, cwd=repo, check=False, timeout=120)
                        self.assertEqual(r.returncode, 0, (args, r.stdout, r.stderr))
                        if structured:
                            value = json.loads(r.stdout)
                            self.assertFalse(value['writes_performed'])
                        self.assertEqual(snapshot(w.root), before)
                self.assertFalse(list(pack.rglob('*.pyc')))
                self.assertFalse(list(pack.rglob('__pycache__')))
                for target in receipt['targets']:
                    if target['kind'] == 'copy':
                        self.assertFalse(list(Path(target['path']).rglob('*.pyc')))

    def test_outside_receipt_aware_audit_refuses_and_preserves_inert_cache_drift(self):
        with World() as w:
            _, _, receipt = fixture(w, 'copy')
            repo = w.repository()
            bank(w, repo)
            bind(w, repo)
            copy = next(Path(t['path']) for t in receipt['targets'] if t['kind'] == 'copy')
            cache = copy / 'scripts/agent_memory/__pycache__/frontmatter.unexpected.pyc'
            cache.parent.mkdir()
            cache.write_bytes(b'INERT_UNEXPECTED_CACHE_BYTES_DO_NOT_EXECUTE\n')
            w.env.pop('PYTHONDONTWRITEBYTECODE', None)
            before = snapshot(w.root)
            # Run trusted source outside the affected copy, never import cache.
            r = w.run([sys.executable, '-B', '-I', ROOT / 'scripts/diagnose.py',
                       'doctor', '--repo', repo, '--json'], cwd=repo, check=False, timeout=120)
            self.assertEqual(r.returncode, 1, r.stdout + r.stderr)
            value = json.loads(r.stdout)
            self.assertFalse(value['writes_performed'])
            self.assertTrue(any(c['component'] == 'runtime.target.copy' for c in value['failures']))
            self.assertEqual(snapshot(w.root), before)
            self.assertEqual(cache.read_bytes(), b'INERT_UNEXPECTED_CACHE_BYTES_DO_NOT_EXECUTE\n')


if __name__ == '__main__':
    unittest.main()
