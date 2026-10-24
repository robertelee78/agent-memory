"""Real installer commands in disposable homes; no live clients or credentials."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import sys
import unittest

from world import World

ROOT = Path(__file__).resolve().parents[2]
PACK = ROOT/'skills/agent-memory'
HELPER = PACK/'scripts/installer.py'
BEGIN = '<!-- agent-memory:activation:begin -->'


def snapshot(root):
    result = {}
    for path in root.rglob('*'):
        if path.is_symlink():
            result[str(path.relative_to(root))] = ('link', os.readlink(path))
        elif path.is_dir():
            result[str(path.relative_to(root))] = ('directory', path.stat().st_mode & 0o777)
        elif path.is_file():
            result[str(path.relative_to(root))] = (hashlib.sha256(path.read_bytes()).hexdigest(),
                                                  path.stat().st_mode & 0o777)
    return result


class InstallerContract(unittest.TestCase):
    def source(self, world):
        target = world.inside('developer source with spaces')
        shutil.copytree(PACK, target)
        return target

    def run_install(self, world, source=None, *args, env=None, expected=0):
        result = world.run([sys.executable, '-I', HELPER, 'install',
                            *(['--development-source', source] if source else []), *args],
                           check=False, env=env)
        self.assertEqual(result.returncode, expected, result.stdout + result.stderr)
        return json.loads(result.stdout) if result.stdout else None

    def state(self, world):
        return world.paths['state']/'agent-memory/install'

    def owned(self, receipt):
        result = {}
        for target in receipt['targets']:
            path = Path(target['path'])
            if path.is_symlink():
                result[str(path)] = ('link', os.readlink(path))
            elif path.is_dir():
                result[str(path)] = snapshot(path)
            else:
                result[str(path)] = (path.read_bytes(), path.stat().st_mode & 0o777)
        return result

    def test_fresh_plan_link_install_repeat_and_installed_command_preserve_source_and_user_config(self):
        with World() as world:
            source = self.source(world); source_before = snapshot(source)
            config = world.paths['codex']/'AGENTS.md'; config.write_text('My existing instructions\n')
            before = snapshot(world.root)
            plan = self.run_install(world, source, '--check')
            self.assertFalse(plan['installation_writes_performed']); self.assertEqual(snapshot(world.root), before)
            result = self.run_install(world, source); receipt = result['receipt']
            self.assertTrue(result['verified']); self.assertEqual(receipt['source']['revision'], 'unversioned')
            self.assertEqual(len(receipt['enabled_adapters']), 4)
            self.assertTrue(config.read_text().startswith('My existing instructions\n'))
            self.assertEqual(config.read_text().count(BEGIN), 1)
            self.assertEqual(snapshot(source), source_before)
            for target in receipt['targets']:
                if target['kind'] == 'link':
                    path = Path(target['path']); self.assertTrue(path.is_symlink())
                    self.assertEqual(path.resolve(), source)
                    check = world.run([sys.executable, '-I', path/'scripts/artifact.py', 'inspect'])
                    self.assertEqual(json.loads(check.stdout)['artifact_digest'], receipt['source']['artifact_digest'])
            owned = self.owned(receipt); records = snapshot(self.state(world))
            repeat = self.run_install(world, None)
            self.assertFalse(repeat['changed']); self.assertEqual(repeat['restart_clients'], [])
            self.assertEqual(self.owned(receipt), owned)
            self.assertEqual((self.state(world)/'receipt.json').read_bytes(),
                             (json.dumps(receipt, sort_keys=True, indent=2)+'\n').encode())
            self.assertEqual({k:v for k,v in snapshot(self.state(world)).items() if k not in ('lock','owner.json')},
                             {k:v for k,v in records.items() if k not in ('lock','owner.json')})
            cli = Path(receipt['bin_dir'])/'agent-memory'
            self.assertEqual(world.run([cli, '--help']).returncode, 0)
            command_plan = json.loads(world.run([cli, 'install', '--check']).stdout)
            self.assertFalse(any(t['changed'] for t in command_plan['targets']))

    def test_all_targets_preflight_before_activation_and_unrelated_types_are_preserved(self):
        for kind in ('file','directory','symlink'):
            with self.subTest(kind=kind), World() as world:
                source = self.source(world)
                collision = world.paths['claude']/'skills/agent-memory'; collision.parent.mkdir()
                if kind == 'file': collision.write_text('keep this')
                elif kind == 'directory': collision.mkdir(); (collision/'mine').write_text('keep this')
                else: collision.symlink_to(world.paths['repo'], target_is_directory=True)
                before = snapshot(world.root)
                result = self.run_install(world, source, expected=1)
                self.assertFalse(result['verified']); self.assertIn('preserve', result['error'])
                after = snapshot(world.root)
                self.assertEqual({k:v for k,v in after.items() if not k.startswith('state/')},
                                 {k:v for k,v in before.items() if not k.startswith('state/')})
                self.assertFalse((self.state(world)/'receipt.json').exists())

    def test_copy_replacement_backup_and_conversion_have_identical_artifact_identity(self):
        with World() as world:
            source = self.source(world)
            first = self.run_install(world, source, '--mode', 'copy', '--presets', 'none',
                                     '--target-dir', world.inside('custom clients with spaces'))
            receipt = first['receipt']; client = next(t for t in receipt['targets'] if t['kind']=='copy')
            client_path = Path(client['path']); self.assertFalse(client_path.is_symlink())
            old_files = snapshot(client_path)
            (source/'templates/new-evidence.txt').write_text('A valid additional artifact asset.\n')
            second = self.run_install(world, source)
            self.assertNotEqual(second['receipt']['source']['artifact_digest'], receipt['source']['artifact_digest'])
            backup = next(b for b in second['receipt']['backups'] if b['original_path']==str(client_path))
            self.assertEqual(snapshot(Path(backup['path'])), old_files)
            as_link = self.run_install(world, source, '--mode', 'link')
            self.assertTrue(client_path.is_symlink()); self.assertEqual(client_path.resolve(), source)
            self.assertEqual(as_link['restart_clients'], [])
            self.assertEqual(as_link['receipt']['source']['artifact_digest'], second['receipt']['source']['artifact_digest'])
            self.assertTrue(Path(backup['path']).is_dir())
            self.assertEqual(self.run_install(world, source)['restart_clients'], [])

    def test_activation_outside_edits_survive_and_inside_edits_refuse(self):
        with World() as world:
            source = self.source(world)
            first = self.run_install(world, source, '--presets', 'codex')
            config = world.paths['codex']/'AGENTS.md'
            config.write_text('Before user rules\n' + config.read_text() + 'After user rules\n')
            outside = config.read_bytes()
            refreshed = self.run_install(world, source)
            self.assertTrue(refreshed['verified']); self.assertEqual(refreshed['restart_clients'], [])
            self.assertEqual(config.read_bytes(), outside)
            config.write_text(config.read_text().replace('load the agent-memory skill', 'my custom changed block'))
            before = self.owned(refreshed['receipt'])
            denied = self.run_install(world, source, expected=1)
            self.assertIn('modified', denied['error']); self.assertEqual(self.owned(refreshed['receipt']), before)

    def test_partial_activation_failure_rolls_back_and_retry_converges(self):
        with World() as world:
            source = self.source(world)
            config = world.paths['codex']/'AGENTS.md'; config.write_text('Keep existing config\n')
            result = self.run_install(world, source, '--presets', 'codex', expected=1,
                                      env={'AGENT_MEMORY_FAULT_STAGE':'install-after-activate-1'})
            self.assertIn('Injected write failure', result['error'])
            self.assertEqual(config.read_text(), 'Keep existing config\n')
            self.assertFalse((world.paths['codex']/'skills/agent-memory').exists())
            self.assertFalse((self.state(world)/'receipt.json').exists())
            self.assertEqual(json.loads((self.state(world)/'journal.json').read_text())['status'], 'rolled-back')
            retry = self.run_install(world, source, '--presets', 'codex')
            self.assertTrue(retry['verified']); self.assertEqual(config.read_text().count(BEGIN), 1)

    def test_bootstrap_recovers_process_exit_before_wrapper_and_lost_commit_response(self):
        for stage in ('install-after-activate-0','install-after-receipt'):
            with self.subTest(stage=stage), World() as world:
                source = self.source(world)
                self.run_install(world, source, '--presets', 'codex', expected=86,
                                 env={'AGENT_MEMORY_FAULT_STAGE':stage,'AGENT_MEMORY_FAULT_MODE':'exit'})
                recovery = world.run([sys.executable, '-I', HELPER, 'recover'])
                value = json.loads(recovery.stdout); self.assertTrue(value['verified'])
                self.assertEqual(value['status'], 'complete' if stage.endswith('receipt') else 'rolled-back')
                retry = self.run_install(world, source, '--presets', 'codex')
                self.assertTrue(retry['verified'])
                self.assertEqual((world.paths['codex']/'AGENTS.md').read_text().count(BEGIN), 1)
                self.assertEqual(self.run_install(world, source)['restart_clients'], [])

    def test_managed_source_local_channel_and_external_receipt(self):
        with World() as world:
            channel = world.repository('channel')
            shutil.copytree(PACK, channel/'skills/agent-memory')
            world.git('add', '.', cwd=channel); world.git('commit', '-m', 'Disposable reviewed source', cwd=channel)
            revision = world.git('rev-parse', 'HEAD', cwd=channel).stdout.strip()
            before = snapshot(channel)
            result = self.run_install(world, None, '--remote', channel, '--ref', 'main', '--presets', 'none')
            receipt = result['receipt']; source = receipt['source']
            self.assertEqual(source['revision'], revision); self.assertFalse(source['dirty'])
            self.assertEqual(source['kind'], 'managed')
            self.assertEqual(Path(source['path']), world.paths['data']/'agent-memory/source')
            self.assertFalse(self.state(world).is_relative_to(source['path']))
            self.assertEqual(snapshot(channel), before)
            self.assertFalse(self.run_install(world, None)['changed'])
            (Path(source['path'])/'unexpected-user-note').write_text('Keep local edits\n')
            refused = self.run_install(world, None, expected=1)
            self.assertIn('clean', refused['error'])

    def test_failed_replacement_restores_existing_copies_and_the_exact_prior_receipt(self):
        with World() as world:
            source = self.source(world)
            initial = self.run_install(world, source, '--mode', 'copy', '--presets', 'codex',
                                       '--target-dir', world.inside('another client'))
            original = self.owned(initial['receipt'])
            receipt_path = self.state(world)/'receipt.json'; receipt_bytes = receipt_path.read_bytes()
            (source/'templates/another-useful-asset.txt').write_text('Changed source content.\n')
            denied = self.run_install(world, source, expected=1,
                                      env={'AGENT_MEMORY_FAULT_STAGE':'install-after-activate-1'})
            self.assertIn('Injected write failure', denied['error'])
            self.assertEqual(self.owned(initial['receipt']), original)
            self.assertEqual(receipt_path.read_bytes(), receipt_bytes)
            retry = self.run_install(world, source)
            self.assertTrue(retry['verified'])
            self.assertNotEqual(retry['receipt']['source']['artifact_digest'],
                                initial['receipt']['source']['artifact_digest'])

    def test_recovery_preserves_edits_made_after_an_interruption(self):
        with World() as world:
            source = self.source(world)
            self.run_install(world, source, '--mode', 'copy', '--presets', 'codex', expected=86,
                             env={'AGENT_MEMORY_FAULT_STAGE':'install-after-activate-0',
                                  'AGENT_MEMORY_FAULT_MODE':'exit'})
            client = world.paths['codex']/'skills/agent-memory'
            edited = client/'user-note.txt'; edited.write_text('Preserve my post-interruption edit.\n')
            result = world.run([sys.executable, '-I', HELPER, 'recover'], check=False)
            self.assertEqual(result.returncode, 1)
            self.assertFalse(json.loads(result.stdout)['verified'])
            self.assertEqual(edited.read_text(), 'Preserve my post-interruption edit.\n')
            self.assertEqual(json.loads((self.state(world)/'journal.json').read_text())['status'],
                             'rollback-conflict')

    def test_unrelated_lock_and_linked_state_are_never_adopted(self):
        with World() as world:
            source = self.source(world); state = self.state(world); state.mkdir(parents=True)
            lock = state/'lock'; lock.write_text('Unrelated file I own.\n')
            self.run_install(world, source, expected=1)
            self.assertEqual(lock.read_text(), 'Unrelated file I own.\n')
            lock.unlink(); state.rmdir(); state.symlink_to(world.paths['bank'], target_is_directory=True)
            before = snapshot(world.paths['bank'])
            self.run_install(world, source, expected=1)
            self.assertEqual(snapshot(world.paths['bank']), before)

    def test_completed_recovery_checks_the_actual_receipt_and_owned_targets(self):
        for case in ('missing-receipt', 'edited-copy', 'missing-copy', 'healthy'):
            with self.subTest(case=case), World() as world:
                source = self.source(world)
                installed = self.run_install(world, source, '--mode', 'copy', '--presets', 'codex')
                client = world.paths['codex']/'skills/agent-memory'
                if case == 'missing-receipt':
                    (self.state(world)/'receipt.json').unlink()
                elif case == 'edited-copy':
                    (client/'user-note.txt').write_text('Preserve my edited installed copy.\n')
                elif case == 'missing-copy':
                    shutil.rmtree(client)
                before = snapshot(world.root)
                result = world.run([sys.executable, '-I', HELPER, 'recover'], check=False)
                value = json.loads(result.stdout)
                self.assertEqual(result.returncode, 0 if case=='healthy' else 1, value)
                self.assertEqual(value['verified'], case=='healthy')
                after = snapshot(world.root)
                self.assertEqual({k:v for k,v in before.items() if k not in ('state/agent-memory/install/lock','state/agent-memory/install/owner.json')},
                                 {k:v for k,v in after.items() if k not in ('state/agent-memory/install/lock','state/agent-memory/install/owner.json')})

    def test_bank_state_overlap_is_refused_before_any_bank_or_runtime_write(self):
        with World() as world:
            source = self.source(world)
            proposal = {
                'project-id':'8e0b66aa-2be5-4fa3-b47f-1245474aa7dd',
                'router-updated':'2026-10-22T17:00:00Z',
                'contexts':{'projectbrief.md':{'body':'# Project\n\nPreserve shared project learning.\n',
                    'purpose':'Orient the agent','scope':'repository','lifecycle':'active',
                    'read-when':'When resuming work'}},
                'omitted':{name:'No relevant evidence yet' for name in
                           ('productContext.md','activeContext.md','systemPatterns.md','techContext.md','progress.md')},
            }
            path = world.inside('bank-fixture.json'); path.write_text(json.dumps(proposal))
            rendered = json.loads(world.run([sys.executable, '-I', ROOT/'scripts/bank_contract.py',
                                            'render', '--input', path]).stdout)
            bank = world.paths['repo']/'memory-bank'; bank.mkdir()
            for name, body in rendered['files'].items():
                (bank/name).write_text(body)
            validated = world.run([sys.executable, '-I', ROOT/'scripts/bank_contract.py',
                                   'validate', '--repo', world.paths['repo']])
            self.assertEqual(validated.returncode, 0)
            world.env['XDG_STATE_HOME'] = str(bank)
            before = snapshot(world.root)
            for options in ((), ('--check',)):
                refused = self.run_install(world, source, *options, expected=1)
                self.assertIn('Memory Bank', refused['error'])
                self.assertEqual(snapshot(world.root), before)
            recovery = world.run([sys.executable, '-I', HELPER, 'recover'], check=False)
            self.assertEqual(recovery.returncode, 1)
            self.assertEqual(snapshot(world.root), before)

    def test_state_inside_development_source_refuses_before_creating_a_lock(self):
        with World() as world:
            source = self.source(world)
            world.env['XDG_STATE_HOME'] = str(source/'runtime state')
            before = snapshot(world.root)
            for options in ((), ('--check',)):
                refused = self.run_install(world, source, '--presets', 'none', *options, expected=1)
                self.assertEqual(snapshot(world.root), before)
                self.assertIn('development source', refused['error'])
                self.assertIn('Do not run recovery', refused['repair'])
            recovered = world.run([sys.executable, '-I', source/'scripts/installer.py', 'recover'], check=False)
            self.assertEqual(recovered.returncode, 1)
            self.assertFalse(json.loads(recovered.stdout)['verified'])
            self.assertEqual(snapshot(world.root), before)

    def test_explicit_development_link_refresh_still_revalidates_new_content(self):
        with World() as world:
            source = self.source(world)
            first = self.run_install(world, source, '--mode', 'link', '--presets', 'codex')
            (source/'templates/new-local-source-asset.txt').write_text('Explicit new development artifact.\n')
            result = self.run_install(world, source)
            self.assertTrue(result['verified'])
            self.assertNotEqual(result['receipt']['source']['artifact_digest'],
                                first['receipt']['source']['artifact_digest'])
            self.assertEqual(result['restart_clients'], ['codex'])
            self.assertEqual(self.run_install(world, source)['restart_clients'], [])

    def test_recover_with_no_pending_transaction_is_read_only(self):
        with World() as world:
            source = self.source(world); before = snapshot(world.root)
            result = world.run([sys.executable, '-I', source/'scripts/installer.py', 'recover'])
            value = json.loads(result.stdout)
            self.assertTrue(value['verified']); self.assertEqual(value['status'], 'no-pending-transaction')
            self.assertEqual(snapshot(world.root), before); self.assertFalse(value['writes_performed'])
