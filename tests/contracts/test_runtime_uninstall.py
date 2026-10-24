"""Actual uninstall/reinstall commands in disposable runtime and project roots."""
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from runtime_update_fixtures import ROOT, PACK, fixture, bank, snapshot, receipt_path


def uninstall(world, receipt, *args, source=False, expected=0, env=None):
    command = ([sys.executable, '-I', ROOT/'scripts/uninstall_runtime.py', 'uninstall']
               if source else [Path(receipt['bin_dir'])/'agent-memory', 'uninstall'])
    result = world.run([*command, *args], check=False, env=env, timeout=120)
    if result.returncode != expected:
        raise AssertionError((result.returncode, result.stdout, result.stderr))
    return json.loads(result.stdout)


class RuntimeUninstallContract(unittest.TestCase):
    def test_preview_remove_repeat_and_reinstall_preserve_bank_original_settings_and_backups(self):
        for mode in ('link', 'copy'):
            with self.subTest(mode=mode), World() as world:
                channel, _, receipt = fixture(world, mode)
                memory = bank(world, world.paths['repo']); before_bank = snapshot(memory)
                backups = {b['path']:snapshot(Path(b['path'])) for b in receipt['backups']}
                before = snapshot(world.root)
                check = uninstall(world, receipt, '--check')
                self.assertTrue(check['check_complete']); self.assertFalse(check['verified'])
                self.assertEqual(snapshot(world.root), before)
                result = uninstall(world, receipt)
                self.assertTrue(result['verified']); self.assertTrue(result['uninstalled'])
                self.assertFalse(result['preserved'])
                for target in receipt['targets']:
                    path = Path(target['path'])
                    if target['kind'] == 'activation':
                        self.assertEqual(path.read_bytes(), b'User instructions stay here.\n')
                    else:
                        self.assertFalse(path.exists() or path.is_symlink(), str(path))
                self.assertEqual(snapshot(memory), before_bank)
                self.assertEqual({p:snapshot(Path(p)) for p in backups}, backups)
                self.assertTrue(json.loads(receipt_path(world).read_text())['uninstalled'])
                repeated = uninstall(world, receipt, source=True)
                self.assertTrue(repeated['verified']); self.assertFalse(repeated['changed'])
                # Lock owner diagnostics may refresh; actual targets/data/receipt do not.
                self.assertEqual(snapshot(memory), before_bank)
                restored = world.run([sys.executable, '-I', ROOT/'scripts/installer.py', 'install',
                                      '--remote', channel, '--ref', 'main', '--mode', mode,
                                      '--presets', 'codex', '--target-dir', world.inside('custom discovery with spaces')])
                self.assertTrue(json.loads(restored.stdout)['verified'])
                self.assertEqual(snapshot(memory), before_bank)

    def test_modified_copy_and_block_are_preserved_and_source_is_retained(self):
        with World() as world:
            _, _, receipt = fixture(world)
            path = world.paths['codex']/'skills/agent-memory/user.txt'; path.write_text('Keep this user edit.\n')
            settings = world.paths['codex']/'AGENTS.md'
            settings.write_text(settings.read_text().replace('load the agent-memory skill', 'load my custom instructions'))
            edited = settings.read_bytes(); code = Path(receipt['source']['path']); before_code = snapshot(code)
            result = uninstall(world, receipt)
            self.assertTrue(result['verified']); self.assertTrue(result['preserved'])
            self.assertEqual(path.read_text(), 'Keep this user edit.\n')
            self.assertEqual(settings.read_bytes(), edited); self.assertEqual(snapshot(code), before_code)

    def test_allowed_outside_edits_survive_uninstall_even_after_update_captures_them(self):
        with World() as world:
            _, _, receipt = fixture(world)
            settings = world.paths['codex']/'AGENTS.md'
            settings.write_text('Additional prefix.\n'+settings.read_text()+'Additional suffix.\n')
            updated = world.run([Path(receipt['bin_dir'])/'agent-memory', 'update'])
            self.assertTrue(json.loads(updated.stdout)['verified'])
            result = uninstall(world, receipt)
            self.assertTrue(result['verified'])
            text = settings.read_text()
            self.assertIn('Additional prefix.\nUser instructions stay here.\n', text)
            self.assertTrue(text.endswith('Additional suffix.\n'))
            self.assertNotIn('agent-memory:activation', text)

    def test_missing_managed_source_is_removed_from_ownership_using_trusted_helper(self):
        with World() as world:
            _, _, receipt = fixture(world)
            shutil.rmtree(receipt['source']['path'])
            result = uninstall(world, receipt, source=True)
            self.assertTrue(result['verified']); self.assertTrue(result['uninstalled'])
            self.assertFalse((Path(receipt['bin_dir'])/'agent-memory').exists())

    def test_interrupted_removal_recovers_and_retry_converges(self):
        with World() as world:
            _, _, receipt = fixture(world)
            memory = bank(world, world.paths['repo']); before_bank = snapshot(memory)
            before = {t['path']:snapshot(Path(t['path'])) for t in receipt['targets']}
            command = [Path(receipt['bin_dir'])/'agent-memory', 'uninstall']
            killed = world.run(command, check=False, env={'AGENT_MEMORY_FAULT_STAGE':'uninstall-after-activate-0',
                                                        'AGENT_MEMORY_FAULT_MODE':'exit'})
            self.assertEqual(killed.returncode, 86)
            recovered = world.run([sys.executable, '-I', ROOT/'scripts/installer.py', 'recover'])
            self.assertTrue(json.loads(recovered.stdout)['verified'])
            self.assertEqual({p:snapshot(Path(p)) for p in before}, before)
            result = uninstall(world, receipt, source=True)
            self.assertTrue(result['verified']); self.assertEqual(snapshot(memory), before_bank)

    def test_development_source_is_preserved(self):
        with World() as world:
            development = world.repository('developer source')
            shutil.copytree(PACK, development/'skills/agent-memory')
            world.git('add', '.', cwd=development)
            world.git('commit', '-m', 'Developer runtime source', cwd=development)
            (development/'developer-notes.md').write_text('Preserve this uncommitted developer edit.\n')
            installed = world.run([sys.executable, '-I', ROOT/'scripts/installer.py', 'install',
                                   '--development-source', development, '--presets', 'codex'])
            receipt = json.loads(installed.stdout)['receipt']; before = snapshot(development)
            result = uninstall(world, receipt, source=True)
            self.assertTrue(result['verified']); self.assertEqual(snapshot(development), before)
            self.assertIn('--development-source', result['reinstall']['argv'])

    def test_missing_receipt_refuses_without_guessing_ownership(self):
        with World() as world:
            _, _, receipt = fixture(world)
            receipt_path(world).unlink()
            targets = {t['path']:snapshot(Path(t['path'])) for t in receipt['targets']}
            check_before = snapshot(world.root)
            uninstall(world, receipt, '--check', source=True, expected=1)
            self.assertEqual(snapshot(world.root), check_before)
            result = uninstall(world, receipt, source=True, expected=1)
            self.assertFalse(result['verified'])
            self.assertEqual({p:snapshot(Path(p)) for p in targets}, targets)
