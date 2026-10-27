"""Disposable real update mutations between the first and final compatibility scans."""
import json, os, pathlib, subprocess, time, unittest
from world import World
from runtime_update_fixtures import ROOT, fixture, advance, bank, owned, receipt_path
import sys

class MutableUpdateCompatibility(unittest.TestCase):
    def inspect_repo(self,w,repo):
        git=repo/'.git'
        return {'head':w.git('rev-parse','HEAD',cwd=repo).stdout,
                'index':(git/'index').read_bytes(),
                'config':(git/'config').read_bytes(),
                'router':(repo/'memory-bank/MEMORY.md').read_bytes(),
                'lock':(repo/'memory-bank/.agent-memory.lock').read_bytes() if (repo/'memory-bank/.agent-memory.lock').exists() else None}

    def exercise(self, kind):
        with World() as world:
            channel,pack,receipt=fixture(world);repo=world.repository('mutable repository')
            memory=bank(world,repo);world.git('add','memory-bank',cwd=repo);world.git('commit','-m','Bank baseline',cwd=repo)
            original_head=world.git('rev-parse','HEAD',cwd=repo).stdout.strip()
            blob=world.git('hash-object','-w',str(memory/'MEMORY.md'),cwd=repo).stdout.strip()
            # Prepare the alternate commit with a private index; no mutation of
            # the target HEAD, normal index, bank, or config until the rendezvous.
            if kind=='head':
                tmpindex=world.inside('private-index')
                world.env['GIT_INDEX_FILE']=str(tmpindex)
                try:
                    world.git('read-tree','HEAD',cwd=repo)
                    world.git('update-index','--add','--cacheinfo',f'100644,{blob},other-bank/MEMORY.md',cwd=repo)
                    tree=world.git('write-tree',cwd=repo).stdout.strip()
                    alt_head=world.git('commit-tree',tree,'-p',original_head,'-m','Competing bank',cwd=repo).stdout.strip()
                finally: world.env.pop('GIT_INDEX_FILE')
            new_revision=advance(world,channel,pack)
            before_targets=owned(receipt);before_receipt=receipt_path(world).read_bytes()
            original_project=self.inspect_repo(world,repo)
            rendezvous=world.inside('update-compatibility-rendezvous');rendezvous.mkdir()
            stage='update-after-activate-0'
            world.env.update({'AGENT_MEMORY_FAULT_STAGE':stage,'AGENT_MEMORY_FAULT_MODE':'pause','AGENT_MEMORY_FAULT_DIR':str(rendezvous)})
            child=world.spawn([pathlib.Path(receipt['bin_dir'])/'agent-memory','--json','update','--repo',str(repo)])
            for key in ('AGENT_MEMORY_FAULT_STAGE','AGENT_MEMORY_FAULT_MODE','AGENT_MEMORY_FAULT_DIR'):world.env.pop(key)
            deadline=time.monotonic()+120
            while not (rendezvous/(stage+'.ready')).exists():
                self.assertIsNone(child.poll(),'update exited before activation rendezvous')
                self.assertLess(time.monotonic(),deadline,'activation rendezvous did not arrive')
                time.sleep(0.01)
            if kind=='head': world.git('update-ref','refs/heads/main',alt_head,original_head,cwd=repo)
            elif kind=='index': world.git('update-index','--add','--cacheinfo',f'100644,{blob},other-bank/MEMORY.md',cwd=repo)
            elif kind=='config': world.git('config','--local','agent-memory.bankroot','other-bank',cwd=repo)
            elif kind=='bank':
                router=memory/'MEMORY.md';raw=router.read_text();assert 'agent-memory/v1' in raw
                router.write_text(raw.replace('agent-memory/v1','agent-memory/v999'))
            elif kind=='lock': (memory/'.agent-memory.lock').write_text('other session owns this lock')
            elif kind=='transient':
                marker=memory/'.agent-memory.lock'; marker.write_text('brief concurrent session'); marker.unlink()
            else: raise AssertionError(kind)
            observed=self.inspect_repo(world,repo)
            (rendezvous/(stage+'.release')).touch()
            stdout,stderr=child.communicate(timeout=120)
            result=json.loads(stdout)
            if kind=='transient':
                self.assertEqual(child.returncode,0,stdout+stderr); self.assertTrue(result['verified'])
                self.assertEqual(result['receipt']['source']['revision'],new_revision)
                self.assertEqual(self.inspect_repo(world,repo),observed)
                print(json.dumps({'case':kind,'exit':child.returncode,'verified':True,'project_state_preserved':True}),flush=True)
                return
            self.assertEqual(child.returncode,1,stdout+stderr)
            self.assertFalse(result['verified'])
            self.assertTrue(result.get('error'),result)
            self.assertTrue(result.get('repair'),result)
            self.assertEqual(self.inspect_repo(world,repo),observed,'update changed independently modified project state')
            self.assertEqual(receipt_path(world).read_bytes(),before_receipt,'failed activation changed receipt')
            # A rollback to an older runtime is correctly refused while the
            # newly observed bank/locator cannot be read. Verify the journal
            # and receipt survive, then explicitly restore the fixture's own
            # mutation and use the trusted recovery helper.
            journal_path=world.paths['state']/'agent-memory/install/journal.json'
            journal=json.loads(journal_path.read_text())
            self.assertIn(journal['status'],('activating','rolled-back'))
            for action in journal['actions']:
                if action.get('before') is not None and action.get('after') != action.get('before'):
                    self.assertTrue(pathlib.Path(action['stage']).exists() or pathlib.Path(action['stage']).is_symlink())
            if kind=='head': world.git('update-ref','refs/heads/main',original_head,alt_head,cwd=repo)
            elif kind=='index': (repo/'.git/index').write_bytes(original_project['index'])
            elif kind=='config': (repo/'.git/config').write_bytes(original_project['config'])
            elif kind=='bank': (memory/'MEMORY.md').write_bytes(original_project['router'])
            elif kind=='lock': (memory/'.agent-memory.lock').unlink()
            self.assertEqual(self.inspect_repo(world,repo),original_project)
            recovery=world.run([sys.executable,'-I',ROOT/'scripts/installer.py','recover'],check=False,timeout=120)
            self.assertEqual(recovery.returncode,0,recovery.stdout+recovery.stderr)
            self.assertTrue(json.loads(recovery.stdout)['verified'])
            self.assertEqual(receipt_path(world).read_bytes(),before_receipt)
            self.assertEqual(owned(receipt),before_targets,'trusted recovery failed to restore owned targets')
            if kind=='lock': self.assertIn('idle bank',result['error'])
            print(json.dumps({'case':kind,'exit':child.returncode,'error':result['error'],
                'repair':result['repair'],'changed_project_state_preserved':True,'trusted_recovery_restored_receipt_and_targets':True}),flush=True)

    def test_changed_head_is_reobserved(self):self.exercise('head')
    def test_changed_index_is_reobserved(self):self.exercise('index')
    def test_changed_local_config_is_reobserved(self):self.exercise('config')
    def test_changed_bank_is_reobserved(self):self.exercise('bank')
    def test_new_idle_lock_is_reobserved(self):self.exercise('lock')
    def test_transient_lock_cleared_before_recheck_succeeds(self):self.exercise('transient')

if __name__=='__main__':unittest.main()
