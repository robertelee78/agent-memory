"""Explicit confirmed local-bank deletion through real isolated public commands."""
import json
import os
from pathlib import Path
import shutil
import sys
import time
import unittest

from world import World
from runtime_update_fixtures import ROOT, fixture, bank, snapshot, receipt_path


def purge(world, memory, *args, expected=0, env=None, repo=None, installed=None):
    command = ([Path(installed['bin_dir'])/'agent-memory', 'purge-data'] if installed else
               [sys.executable, '-I', ROOT/'scripts/uninstall_runtime.py', 'purge-data'])
    result = world.run([*command, '--repo', repo or world.paths['repo'], '--path', memory, *args],
                       check=False, env=env, timeout=120)
    if result.returncode != expected:
        raise AssertionError((result.returncode,result.stdout,result.stderr))
    return json.loads(result.stdout) if result.stdout else None


def journal(world, confirmation):
    return world.paths['state']/'agent-memory/install/purges'/(confirmation+'.json')


class RuntimePurgeContract(unittest.TestCase):
    def test_same_inode_and_content_after_file_metadata_change_requires_fresh_consent(self):
        with World() as world:
            memory=bank(world,world.paths['repo']); path=memory/'projectbrief.md'
            raw=path.read_bytes(); original=path.stat()
            token=purge(world,memory,'--check')['preview']['confirmation']
            os.utime(path,ns=(original.st_atime_ns,original.st_mtime_ns+1_000_000_000))
            self.assertEqual(path.stat().st_ino,original.st_ino); self.assertEqual(path.read_bytes(),raw)
            self.assertNotEqual(path.stat().st_ctime_ns,original.st_ctime_ns)
            before=snapshot(memory)
            denied=purge(world,memory,'--confirm',token,expected=1)
            self.assertFalse(denied['verified']); self.assertEqual(snapshot(memory),before)

    def test_exact_preview_confirmation_and_repeat_preserve_runtime_backups_code_and_git(self):
        with World() as world:
            _,_,receipt = fixture(world)
            repo = world.repository(); code = repo/'worker.py'; code.write_text('print("original")\n')
            world.git('add','.',cwd=repo); world.git('commit','-m','Initial project',cwd=repo)
            code.write_text('print("staged worker edit")\n'); world.git('add','worker.py',cwd=repo)
            memory = bank(world,repo); (memory/'.agent-memory-cache').mkdir()
            (memory/'.agent-memory-cache/index.json').write_text('{"derived":true}\n')
            other = world.repository('another project'); other_bank = bank(world,other)
            retained = {str(p):snapshot(p) for p in [Path(receipt['source']['path']),code,repo/'.git',other_bank,
                                                    world.paths['backups'],receipt_path(world)]}
            before = snapshot(world.root); result = purge(world,memory,'--check',installed=receipt)
            self.assertTrue(result['check_complete']); self.assertEqual(snapshot(world.root),before)
            approved = result['preview']; self.assertEqual(approved['path'],str(memory))
            self.assertGreater(approved['counts']['durable_files'],0)
            self.assertEqual(approved['counts']['generated_runtime_files'],1)
            purge(world,memory,expected=2,installed=receipt)
            self.assertEqual(snapshot(world.root),before)
            removed = purge(world,memory,'--confirm',approved['confirmation'],installed=receipt)
            self.assertTrue(removed['verified']); self.assertFalse(memory.exists())
            self.assertTrue(removed['receipt']['deletion_readback_verified'])
            self.assertEqual({p:snapshot(Path(p)) for p in retained},retained)
            repeated = purge(world,memory,'--confirm',approved['confirmation'])
            self.assertTrue(repeated['verified']); self.assertFalse(repeated['changed'])

    def test_changed_content_and_recreated_identical_bank_require_new_confirmation(self):
        for change in ('content','new-inode'):
            with self.subTest(change=change), World() as world:
                memory = bank(world,world.paths['repo'])
                token = purge(world,memory,'--check')['preview']['confirmation']
                if change=='content':
                    (memory/'projectbrief.md').write_text((memory/'projectbrief.md').read_text()+'\nNew evidence.\n')
                else:
                    memory.rename(world.inside('retained old bank')); bank(world,world.paths['repo'])
                before = snapshot(memory)
                denied = purge(world,memory,'--confirm',token,expected=1)
                self.assertFalse(denied['verified']); self.assertEqual(snapshot(memory),before)

    def test_broad_home_root_and_symlink_inputs_are_refused_without_writes(self):
        for case in ('broad','home','root','alias','inside-link'):
            with self.subTest(case=case), World() as world:
                memory = bank(world,world.paths['repo']); path=memory; repo=world.paths['repo']
                if case=='broad': path=repo
                elif case=='home': repo=world.paths['home']; path=repo
                elif case=='root': repo=Path('/'); path=repo
                elif case=='alias':
                    path=world.inside('bank alias'); path.symlink_to(memory,target_is_directory=True)
                else:
                    target=world.inside('unrelated.txt'); target.write_text('Keep me.\n')
                    (memory/'linked-data').symlink_to(target)
                before=snapshot(world.root)
                denied=purge(world,path,'--check',repo=repo,expected=1)
                self.assertFalse(denied['verified']); self.assertEqual(snapshot(world.root),before)

    def test_interruptions_before_journal_detach_partial_delete_and_lost_response_converge(self):
        for point in ('purge-bank-lock-ready','purge-after-detach','purge-after-entry-0',
                      'purge-after-root-remove','purge-after-receipt'):
            with self.subTest(point=point), World() as world:
                memory=bank(world,world.paths['repo'])
                token=purge(world,memory,'--check')['preview']['confirmation']
                purge(world,memory,'--confirm',token,expected=86,env={
                    'AGENT_MEMORY_FAULT_STAGE':point,'AGENT_MEMORY_FAULT_MODE':'exit'})
                completed=purge(world,memory,'--confirm',token)
                self.assertTrue(completed['verified']); self.assertFalse(memory.exists())
                record=json.loads(journal(world,token).read_text())
                self.assertEqual(record['status'],'complete'); self.assertTrue(record['deletion_readback_verified'])

    def test_retry_preserves_a_recreated_canonical_bank_and_purges_only_the_confirmed_quarantine(self):
        with World() as world:
            memory=bank(world,world.paths['repo']); token=purge(world,memory,'--check')['preview']['confirmation']
            purge(world,memory,'--confirm',token,expected=86,env={
                'AGENT_MEMORY_FAULT_STAGE':'purge-after-detach','AGENT_MEMORY_FAULT_MODE':'exit'})
            bank(world,world.paths['repo']); (memory/'projectbrief.md').write_text('New bank must survive.\n')
            before=snapshot(memory)
            completed=purge(world,memory,'--confirm',token)
            self.assertTrue(completed['verified']); self.assertTrue(completed['replacement_bank_present'])
            self.assertEqual(snapshot(memory),before)
            self.assertFalse(Path(completed['receipt']['stage']).exists())

    def test_unapproved_quarantine_data_is_preserved_on_retry(self):
        with World() as world:
            memory=bank(world,world.paths['repo']); token=purge(world,memory,'--check')['preview']['confirmation']
            purge(world,memory,'--confirm',token,expected=86,env={
                'AGENT_MEMORY_FAULT_STAGE':'purge-after-detach','AGENT_MEMORY_FAULT_MODE':'exit'})
            stage=Path(json.loads(journal(world,token).read_text())['stage'])
            (stage/'unapproved.txt').write_text('Do not delete this later addition.\n'); before=snapshot(stage)
            denied=purge(world,memory,'--confirm',token,expected=1)
            self.assertFalse(denied['verified']); self.assertEqual(snapshot(stage),before)

    def test_foreign_bank_lock_record_is_preserved(self):
        with World() as world:
            memory=bank(world,world.paths['repo']); token=purge(world,memory,'--check')['preview']['confirmation']
            # An existing ordinary writer's lock is not purge's authority to recover.
            lock=memory/'.agent-memory.lock'
            lock.write_text(json.dumps({'operation':'bank-checkpoint','host':'other-writer','pid':1}))
            before=snapshot(memory)
            purge(world,memory,'--confirm',token,expected=1)
            self.assertEqual(snapshot(memory),before)

    def test_pending_memory_transaction_marker_is_preserved(self):
        with World() as world:
            memory=bank(world,world.paths['repo'])
            (memory/'.agent-memory-transaction.md').write_text('Incomplete transaction: preserve and recover.\n')
            before=snapshot(world.root)
            purge(world,memory,'--check',expected=1)
            self.assertEqual(snapshot(world.root),before)

    def test_live_bank_writer_blocks_purge_until_the_process_releases_its_lock(self):
        with World() as world:
            memory=bank(world,world.paths['repo']); token=purge(world,memory,'--check')['preview']['confirmation']
            ready=world.inside('writer-ready'); release=world.inside('writer-release')
            script=world.inside('held-writer.py')
            script.write_text("import fcntl,json,os,socket,sys,time\nfrom pathlib import Path\n"
                              "lock,ready,release=map(Path,sys.argv[1:])\n"
                              "fd=os.open(lock,os.O_CREAT|os.O_EXCL|os.O_RDWR,0o600)\n"
                              "try:\n"
                              " os.write(fd,json.dumps({'operation':'bank-checkpoint','pid':os.getpid(),'host':socket.gethostname()}).encode());os.fsync(fd)\n"
                              " fcntl.flock(fd,fcntl.LOCK_EX|fcntl.LOCK_NB);ready.write_text('held')\n"
                              " deadline=time.monotonic()+15\n"
                              " while not release.exists() and time.monotonic()<deadline:time.sleep(0.01)\n"
                              " if not release.exists():raise RuntimeError('writer release timed out')\n"
                              "finally:\n lock.unlink();os.close(fd)\n")
            child=world.spawn([sys.executable,'-I',script,memory/'.agent-memory.lock',ready,release])
            deadline=time.monotonic()+10
            while not ready.exists() and time.monotonic()<deadline:
                self.assertIsNone(child.poll()); time.sleep(0.01)
            self.assertTrue(ready.exists()); before=snapshot(memory)
            purge(world,memory,'--confirm',token,expected=1)
            self.assertEqual(snapshot(memory),before); self.assertIsNone(child.poll())
            release.write_text('release'); stdout,stderr=child.communicate(timeout=5)
            self.assertEqual(child.returncode,0,stdout+stderr)
            self.assertTrue(purge(world,memory,'--confirm',token)['verified'])
