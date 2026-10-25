"""The commands a person types, without a hidden machine-output default."""
import json
from pathlib import Path
import sys
import unittest

from world import World
from cli_alias_fixtures import channel, install, alias_tmpdir, advance_to_current, ROOT, PACK
from runtime_update_fixtures import fixture, advance
from contracts.test_installer import snapshot


def pipe(world, source, *args):
    return world.run(['/bin/bash', '-c',
                      'set -o pipefail; cat "$1" | /bin/bash -s -- "${@:2}"',
                      'public-install', ROOT/'install.sh', '--remote', source.as_uri(), '--ref', 'main', *args],
                     check=False, timeout=120)


class PublicCLIContract(unittest.TestCase):
    def test_unsupported_runtime_python_refuses_before_loading_operations(self):
        with World() as w:
            before = snapshot(w.root)
            for entry in ('installer.py', 'am_console.py'):
                result = w.run([sys.executable, '-I', '-c',
                    'import runpy,sys; sys.version_info=(3,9); '
                    'runpy.run_path(sys.argv[1],run_name="__main__")', PACK/'scripts'/entry], check=False)
                self.assertEqual(result.returncode, 1)
                self.assertIn('Python 3.11+', result.stderr)
                self.assertNotIn('Traceback', result.stderr)
                self.assertEqual(result.stdout, '')
            self.assertEqual(snapshot(w.root), before)

    def test_default_help_version_flags_and_public_parser_names_are_read_only(self):
        with World() as w:
            _, _, receipt = fixture(w); cli = Path(receipt['bin_dir'])/'am'
            before = snapshot(w.root)
            for args in ([], ['help'], ['--help'], ['-h']):
                result = w.run([cli, *args])
                self.assertIn('am update [--check]', result.stdout)
                self.assertNotIn('installer.py', result.stdout + result.stderr)
            versions = [w.run([cli, *args]).stdout for args in (['version'], ['--version'], ['-V'])]
            self.assertEqual(len(set(versions)), 1)
            self.assertTrue(versions[0].startswith('am 1.0.0\nrevision '))
            self.assertIn('(clean)\nsource ', versions[0])
            for args in (['install', '--help'], ['update', '--help'], ['uninstall', '--help'],
                         ['help', 'identity'], ['identity', 'status', '--help'],
                         ['capture', '--help'], ['inspect', '--help']):
                result = w.run([cli, *args]); self.assertIn('usage: am ', result.stdout)
                self.assertNotIn('installer.py', result.stdout + result.stderr)
                self.assertNotIn('am capture capture', result.stdout)
                self.assertNotIn('am inspect inspect', result.stdout)
            invalid = w.run([cli, '--not-an-option'], check=False)
            self.assertEqual(invalid.returncode, 2); self.assertIn('am:', invalid.stderr)
            self.assertNotIn('installer.py', invalid.stderr)
            self.assertEqual(snapshot(w.root), before)

    def test_json_is_explicit_and_source_helpers_and_memory_results_remain_structured(self):
        with World() as w:
            _, _, receipt = fixture(w); directory = Path(receipt['bin_dir'])
            for name in ('am', 'agent-memory'):
                for args in (['version', '--json'], ['--json', '--version'], ['-V', '--json']):
                    result = json.loads(w.run([directory/name, *args]).stdout)
                    self.assertEqual(result['operation'], 'version')
                    self.assertEqual(result['executing_source']['version'], '1.0.0')
                    self.assertFalse(result['writes_performed']); self.assertFalse(result['network_performed'])
            direct = json.loads(w.run([sys.executable, '-I', PACK/'scripts/installer.py', 'version']).stdout)
            self.assertEqual(direct['schema'], 'agent-memory/diagnostic-result-v1')
            result = w.run([directory/'am', 'identity', 'status', '--repo', w.paths['repo']], check=False)
            self.assertIsInstance(json.loads(result.stdout), dict)
            self.assertIn('artifact_digest', w.run([directory/'am', 'version', '--verbose']).stdout)

    def test_human_bootstrap_preview_repeat_update_and_uninstall_journey_under_temp_alias(self):
        with World() as w:
            source, pack = channel(w); alias_tmpdir(w)
            before = snapshot(w.root)
            planned = pipe(w, source, '--presets', 'none', '--check')
            self.assertEqual(planned.returncode, 0, planned.stderr)
            self.assertIn('Installation preview; no changes made', planned.stdout)
            self.assertNotIn('"verified"', planned.stdout); self.assertEqual(snapshot(w.root), before)
            first = pipe(w, source, '--presets', 'none')
            self.assertEqual(first.returncode, 0, first.stderr); self.assertIn('Installed am 0.0.2', first.stdout)
            receipt_path = w.paths['state']/'agent-memory/install/receipt.json'
            receipt = json.loads(receipt_path.read_text()); cli = Path(receipt['bin_dir'])/'am'
            old = receipt_path.read_bytes()
            checked = w.run([cli, 'update', '--check']); self.assertIn('is current', checked.stdout)
            self.assertIn('Preview only', checked.stdout); self.assertEqual(receipt_path.read_bytes(), old)
            (pack/'templates/public-update.txt').write_text('Updated artifact.\n')
            w.git('add', '.', cwd=source); w.git('commit', '-m', 'Public update fixture', cwd=source)
            # The bare repeated one-liner follows the guarded updater, as AWA does.
            updated = pipe(w, source)
            self.assertEqual(updated.returncode, 0, updated.stderr); self.assertIn('Updated am', updated.stdout)
            self.assertEqual(json.loads(receipt_path.read_text())['source']['revision'],
                             w.git('rev-parse', 'HEAD', cwd=source).stdout.strip())
            repeated = pipe(w, source); self.assertEqual(repeated.returncode, 0, repeated.stderr)
            self.assertIn('is current', repeated.stdout)
            bank = w.paths['repo']/'memory-bank'; bank.mkdir(); (bank/'keep.md').write_text('Keep memory.\n')
            preview = w.run([cli, 'uninstall', '--check']); self.assertIn('Uninstall preview', preview.stdout)
            self.assertTrue(cli.exists())
            removed = w.run([cli, 'uninstall']); self.assertIn('Uninstalled owned runtime', removed.stdout)
            self.assertIn('memory and user data preserved', removed.stdout)
            self.assertFalse(cli.exists()); self.assertEqual((bank/'keep.md').read_text(), 'Keep memory.\n')

    def test_actual_legacy_wrapper_gains_flags_and_human_output_without_wrapper_replacement(self):
        with World() as w:
            source, _ = channel(w, legacy=True); first = install(w, source, legacy=True)
            directory = Path(first['receipt']['bin_dir']); legacy = directory/'agent-memory'
            old_bytes = legacy.read_bytes(); advance_to_current(w, source); alias_tmpdir(w)
            # Actually execute the frozen old updater, not a fabricated receipt.
            old_update = w.run([legacy, 'update'])
            self.assertTrue(json.loads(old_update.stdout)['verified'])
            self.assertEqual(legacy.read_bytes(), old_bytes)
            version = w.run([legacy, '--version']); self.assertTrue(version.stdout.startswith('agent-memory 0.0.2\n'))
            refreshed = w.run([legacy, 'install']); self.assertIn('Installed agent-memory 0.0.2', refreshed.stdout)
            short = directory/'am'
            self.assertTrue(w.run([short, '--version']).stdout.startswith('am 0.0.2\n'))
            self.assertEqual(legacy.read_bytes(), short.read_bytes())

    def test_refusals_and_preserved_uninstall_are_readable_and_keep_failure_codes(self):
        with World() as w:
            _, _, receipt = fixture(w); directory = Path(receipt['bin_dir']); cli = directory/'am'
            preview = w.run([cli, 'status']); self.assertIn('memory not_located', preview.stdout)
            self.assertNotIn('"schema"', preview.stdout)
            cli.write_text('User changed this wrapper.\n')
            result = w.run([directory/'agent-memory', 'uninstall'])
            self.assertIn('preserved targets; removal is not complete', result.stdout)
            self.assertEqual(cli.read_text(), 'User changed this wrapper.\n')
        with World() as w:
            source, _ = channel(w); directory = w.root/'unsafe-\x1b[31m'; directory.mkdir()
            (directory/'am').write_text('Unrelated command.\n')
            result = w.run([sys.executable, '-I', PACK/'scripts/am_console.py', 'install',
                            '--remote', source.as_uri(), '--presets', 'none', '--bin-dir', directory], check=False)
            self.assertEqual(result.returncode, 1); self.assertIn('am:', result.stderr)
            self.assertNotIn('\x1b', result.stderr); self.assertIn('\\x1b', result.stderr)
            self.assertEqual((directory/'am').read_text(), 'Unrelated command.\n')

    def test_human_recovery_after_partial_activation_and_no_pending_transaction(self):
        with World() as w:
            source, _ = channel(w)
            result = w.run([sys.executable, '-I', PACK/'scripts/installer.py', 'install',
                            '--remote', source.as_uri(), '--presets', 'none'], check=False,
                           env={'AGENT_MEMORY_FAULT_STAGE': 'install-after-activate-1',
                                'AGENT_MEMORY_FAULT_MODE': 'exit'})
            self.assertEqual(result.returncode, 86)
            cli = w.paths['home']/'.local/bin/am'
            recovered = w.run([cli, 'recover']); self.assertIn('Rolled back interrupted runtime', recovered.stdout)
            self.assertFalse(cli.exists())
        with World() as w:
            result = w.run([sys.executable, '-I', PACK/'scripts/am_console.py', 'recover'])
            self.assertEqual(result.stdout, 'No pending runtime transaction.\n')
