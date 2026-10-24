"""Verified channel updates through the installed command in disposable homes."""
import json
from pathlib import Path
import shutil
import sys
import time
import unittest

from world import World
from runtime_update_fixtures import ROOT, fixture, advance, call, snapshot, owned, receipt_path, version, bank


class RuntimeUpdateContract(unittest.TestCase):
    def test_dangling_lifecycle_records_are_preserved_and_refuse_update(self):
        for name in ('owner.json', 'journal.json'):
            with self.subTest(record=name), World() as world:
                channel, pack, receipt = fixture(world)
                advance(world, channel, pack)
                memory = bank(world, world.paths['repo']); bank_before = snapshot(memory)
                path = receipt_path(world).parent / name
                path.unlink(); path.symlink_to(world.inside('unrelated missing record'))
                before = snapshot(path); targets = owned(receipt)
                raw = receipt_path(world).read_bytes()
                if name == 'journal.json':
                    all_before = snapshot(world.root)
                    call(world, receipt, '--check', expected=1)
                    self.assertEqual(snapshot(world.root), all_before)
                _, result = call(world, receipt, expected=1)
                self.assertFalse(result['verified'])
                self.assertEqual(snapshot(path), before)
                self.assertEqual(owned(receipt), targets)
                self.assertEqual(receipt_path(world).read_bytes(), raw)
                self.assertEqual(snapshot(memory), bank_before)

    def test_read_only_check_valid_update_and_repeat_preserve_user_text_and_source_channel(self):
        for mode in ('link','copy'):
            with self.subTest(mode=mode), World() as world:
                channel, pack, receipt = fixture(world,mode)
                target = world.paths['codex']/'AGENTS.md'
                target.write_text('Outside prefix\n'+target.read_text()+'Outside suffix\n')
                user = target.read_bytes()
                revision = advance(world,channel,pack,shim=True)
                before = snapshot(world.root)
                _, check = call(world,receipt,'--check')
                self.assertTrue(check['check_complete']); self.assertFalse(check['verified'])
                self.assertTrue(check['update_available']); self.assertEqual(snapshot(world.root),before)
                channel_before = snapshot(channel)
                _, result = call(world,receipt)
                self.assertTrue(result['verified']); self.assertEqual(result['receipt']['source']['revision'],revision)
                self.assertEqual(result['receipt']['source']['version'],'1.0.1')
                self.assertEqual(snapshot(channel),channel_before)
                self.assertIn('Outside prefix',target.read_text()); self.assertIn('Outside suffix',target.read_text())
                self.assertIn('For this project, when',target.read_text())
                self.assertEqual(target.read_text().count('<!-- agent-memory:activation:begin -->'),1)
                self.assertTrue(result['restart_clients'])
                old_receipt = receipt_path(world).read_bytes(); installed = owned(result['receipt'])
                _, repeated = call(world,result['receipt'])
                self.assertFalse(repeated['changed']); self.assertEqual(repeated['restart_clients'],[])
                self.assertEqual(receipt_path(world).read_bytes(),old_receipt)
                self.assertEqual(owned(result['receipt']),installed)
                self.assertNotEqual(user,target.read_bytes())  # Only the owned block changed.

    def test_dirty_remote_ref_and_unowned_target_changes_refuse_without_activation(self):
        for case in ('dirty','remote','branch','copy-edit','block-edit'):
            with self.subTest(case=case), World() as world:
                channel,pack,receipt = fixture(world)
                advance(world,channel,pack)
                code = Path(receipt['source']['path'])
                if case=='dirty': (code/'user-note.txt').write_text('Keep this local edit.\n')
                elif case=='remote': world.git('remote','set-url','origin',world.paths['repo'],cwd=code)
                elif case=='branch': world.git('checkout','-b','other-branch',cwd=code)
                elif case=='copy-edit': (world.paths['codex']/'skills/agent-memory/user-note.txt').write_text('Keep my copy edit.\n')
                else:
                    path = world.paths['codex']/'AGENTS.md'; path.write_text(path.read_text().replace('load the agent-memory skill','keep this custom block'))
                before = owned(receipt); raw = receipt_path(world).read_bytes()
                _, denied = call(world,receipt,expected=1)
                self.assertFalse(denied['verified']); self.assertEqual(owned(receipt),before)
                self.assertEqual(receipt_path(world).read_bytes(),raw)
                check_before = snapshot(world.root)
                call(world,receipt,'--check',expected=1)
                self.assertEqual(snapshot(world.root),check_before)

    def test_downgrade_non_fast_forward_and_invalid_candidate_keep_the_previous_runtime(self):
        for case in ('downgrade','non-ff','invalid','python','surface','uninstall-surface','purge-surface'):
            with self.subTest(case=case), World() as world:
                channel,pack,receipt = fixture(world)
                if case=='non-ff':
                    world.git('checkout','--orphan','replacement',cwd=channel)
                    version(pack,'2.0.0'); world.git('add','.',cwd=channel)
                    world.git('commit','-m','Unrelated runtime history',cwd=channel)
                    world.git('branch','-f','main','HEAD',cwd=channel)
                    world.git('checkout','main',cwd=channel)
                else:
                    advance(world,channel,pack,'0.9.9' if case=='downgrade' else '1.0.1')
                    if case=='invalid': (pack/'schemas/runtime-compatibility.json').unlink()
                    elif case=='python':
                        path=pack/'schemas/runtime-compatibility.json'; spec=json.loads(path.read_text()); spec['python_minimum']='99.0'; path.write_text(json.dumps(spec))
                    elif case in ('surface','uninstall-surface','purge-surface'):
                        command={'surface':'update','uninstall-surface':'uninstall','purge-surface':'purge-data'}[case]
                        path=pack/'scripts/agent_memory/installation.py'; path.write_text(path.read_text().replace("add_parser('"+command+"'", "add_parser('removed-"+command+"'"))
                    if case in ('invalid','python','surface','uninstall-surface','purge-surface'):
                        world.git('add','.',cwd=channel); world.git('commit','-m','Candidate refusal fixture',cwd=channel)
                before = owned(receipt); raw = receipt_path(world).read_bytes()
                _, denied = call(world,receipt,expected=1)
                self.assertFalse(denied['verified']); self.assertEqual(owned(receipt),before)
                self.assertEqual(receipt_path(world).read_bytes(),raw)

    def test_development_sources_are_never_mutated_by_update(self):
        with World() as world:
            source=world.inside('explicit development source'); shutil.copytree(ROOT/'skills/agent-memory',source)
            installed=world.run([sys.executable,'-I',ROOT/'scripts/installer.py','install','--development-source',source,'--presets','none'])
            receipt=json.loads(installed.stdout)['receipt']; before=snapshot(source)
            call(world,receipt,expected=1); self.assertEqual(snapshot(source),before)
            before_check=snapshot(world.root); call(world,receipt,'--check',expected=1)
            self.assertEqual(snapshot(world.root),before_check)

    def test_changed_owned_copy_update_retains_prior_payload_and_receipt_backups(self):
        with World() as world:
            channel,pack,receipt=fixture(world); before=owned(receipt)
            advance(world,channel,pack)
            _,result=call(world,receipt)
            backups=result['receipt']['backups']
            for target in receipt['targets']:
                if target['kind'] in ('copy','source'):
                    matches=[b for b in backups if b['original_path']==target['path']]
                    self.assertTrue(matches)
                    self.assertTrue(any(snapshot(Path(b['path']))==before[target['path']] for b in matches))
            self.assertTrue(any(b['original_path']==str(receipt_path(world)) for b in backups))

    def test_lost_response_and_partial_activation_recover_then_retry_without_duplicates(self):
        for stage in ('update-after-activate-0','update-after-receipt'):
            with self.subTest(stage=stage), World() as world:
                channel,pack,receipt=fixture(world); before=owned(receipt); revision=advance(world,channel,pack)
                call(world,receipt,expected=86,env={'AGENT_MEMORY_FAULT_STAGE':stage,'AGENT_MEMORY_FAULT_MODE':'exit'})
                recovered=world.run([sys.executable,'-I',ROOT/'scripts/installer.py','recover'])
                result=json.loads(recovered.stdout); self.assertTrue(result['verified'])
                if stage.endswith('0'): self.assertEqual(owned(receipt),before)
                _,completed=call(world,receipt)
                self.assertTrue(completed['verified']); self.assertEqual(completed['receipt']['source']['revision'],revision)
                self.assertEqual((world.paths['codex']/'AGENTS.md').read_text().count('<!-- agent-memory:activation:begin -->'),1)
                self.assertEqual(call(world,completed['receipt'])[1]['restart_clients'],[])

    def test_discovered_and_explicit_banks_remain_unchanged_and_unsupported_schema_refuses(self):
        for registered in (False,True):
            with self.subTest(registered=registered), World() as world:
                channel,pack,receipt=fixture(world)
                repo=world.repository('another project')
                memory=bank(world,repo)
                if registered:
                    from identity_fixtures import bind
                    bind(world,repo)
                advance(world,channel,pack)
                arguments=[] if registered else ['--repo',str(repo)]
                before=snapshot(memory)
                _,planned=call(world,receipt,'--check',*arguments)
                self.assertTrue(any(b['bank']==str(memory) for b in planned['compatibility']['banks']))
                self.assertEqual(snapshot(memory),before)
                router=memory/'MEMORY.md'; router.write_text(router.read_text().replace('agent-memory/v1','agent-memory/v999'))
                bad_before=snapshot(memory); active=owned(receipt); receipt_bytes=receipt_path(world).read_bytes()
                call(world,receipt,*arguments,expected=1)
                self.assertEqual(snapshot(memory),bad_before); self.assertEqual(owned(receipt),active)
                self.assertEqual(receipt_path(world).read_bytes(),receipt_bytes)

    def test_fast_forward_outside_the_pack_needs_no_client_restart(self):
        with World() as world:
            channel,pack,receipt=fixture(world)
            (channel/'release-note.txt').write_text('Only checkout documentation changed.\n')
            world.git('add','.',cwd=channel); world.git('commit','-m','Outside-pack change',cwd=channel)
            _,result=call(world,receipt)
            self.assertTrue(result['verified']); self.assertTrue(result['changed'])
            self.assertFalse(result['loaded_content_changes']); self.assertEqual(result['restart_clients'],[])
            self.assertEqual(result['receipt']['source']['artifact_digest'],receipt['source']['artifact_digest'])

    def test_trusted_source_helper_updates_when_the_installed_command_does_not_yet_dispatch_update(self):
        with World() as world:
            channel,pack,receipt=fixture(world,bootstrap=True)
            unavailable=world.run([Path(receipt['bin_dir'])/'agent-memory','update','--check'],check=False)
            self.assertEqual(unavailable.returncode,2)
            # The new trusted package is separate from the older managed runtime.
            module=pack/'scripts/agent_memory/installation.py'
            module.write_bytes((ROOT/'skills/agent-memory/scripts/agent_memory/installation.py').read_bytes())
            revision=advance(world,channel,pack)
            _,result=call(world,receipt,source=True)
            self.assertTrue(result['verified']); self.assertEqual(result['receipt']['source']['revision'],revision)
            self.assertTrue(call(world,result['receipt'],'--check')[1]['check_complete'])

    def test_competing_update_observes_the_live_lifecycle_lock(self):
        with World() as world:
            channel,pack,receipt=fixture(world); advance(world,channel,pack)
            rendezvous=world.inside('update pause'); rendezvous.mkdir()
            stage='update-write-0-before'
            world.env.update({'AGENT_MEMORY_FAULT_STAGE':stage,'AGENT_MEMORY_FAULT_MODE':'pause',
                              'AGENT_MEMORY_FAULT_DIR':str(rendezvous)})
            child=world.spawn([Path(receipt['bin_dir'])/'agent-memory','update'])
            for key in ('AGENT_MEMORY_FAULT_STAGE','AGENT_MEMORY_FAULT_MODE','AGENT_MEMORY_FAULT_DIR'):
                del world.env[key]
            deadline=time.monotonic()+8
            while not (rendezvous/(stage+'.ready')).exists():
                self.assertIsNone(child.poll())
                self.assertLess(time.monotonic(),deadline)
                time.sleep(0.01)
            before=owned(receipt); raw=receipt_path(world).read_bytes()
            _,denied=call(world,receipt,expected=1)
            self.assertIn('holds the lock',denied['error']); self.assertEqual(owned(receipt),before)
            self.assertEqual(receipt_path(world).read_bytes(),raw)
            (rendezvous/(stage+'.release')).touch()
            stdout,stderr=child.communicate(timeout=60)
            self.assertEqual(child.returncode,0,stdout+stderr)
            self.assertTrue(json.loads(stdout)['verified'])

    def test_unchanged_revision_refreshes_allowed_outside_text_without_a_code_switch(self):
        with World() as world:
            _,_,receipt=fixture(world)
            code=Path(receipt['source']['path']); before_code=snapshot(code)
            target=world.paths['codex']/'AGENTS.md'
            target.write_text('New outside user text.\n'+target.read_text())
            before=target.read_bytes()
            _,result=call(world,receipt)
            self.assertTrue(result['verified']); self.assertFalse(result['update_available'])
            self.assertEqual(result['restart_clients'],[]); self.assertEqual(target.read_bytes(),before)
            self.assertEqual(snapshot(code),before_code)
            recovered=world.run([sys.executable,'-I',ROOT/'scripts/installer.py','recover'])
            self.assertTrue(json.loads(recovered.stdout)['verified'])
            self.assertFalse(call(world,result['receipt'])[1]['changed'])

    def test_unchanged_channel_does_not_derive_a_new_artifact_from_a_different_umask(self):
        with World() as world:
            _,_,receipt=fixture(world); before=owned(receipt); raw=receipt_path(world).read_bytes()
            launcher=world.inside('private-umask-launch.py')
            launcher.write_text("import os,runpy,sys\nos.umask(0o077)\nentry=sys.argv.pop(1)\nrunpy.run_path(entry,run_name='__main__')\n")
            command=[sys.executable,'-I',launcher,Path(receipt['bin_dir'])/'agent-memory','update']
            result=world.run(command)
            value=json.loads(result.stdout)
            self.assertTrue(value['verified']); self.assertFalse(value['changed'])
            self.assertFalse(value['loaded_content_changes'])
            self.assertEqual(owned(receipt),before); self.assertEqual(receipt_path(world).read_bytes(),raw)
