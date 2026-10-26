"""Actual public recall source/topology and marker races in disposable worlds."""
import json
import os
from pathlib import Path
import stat
import sys
import time
import unittest

from world import World
from contracts import test_retrieval as recall
from contracts import test_consolidation as maintenance


class RecallObservationContract(unittest.TestCase):
    def test_portable_alias_guard_preserves_component_unicode_and_root_anchor_collisions(self):
        with World() as w:
            helper,repo,bank,project,path=self.fixture(w)
            program='''import sys
sys.path.insert(0,sys.argv[1])
from agent_memory.bank_contract import portable_aliases
from agent_memory.frontmatter import ContractError
for values in (['topics/Name/a.md','topics/name/b.md'],['topics/caf\\u00e9.md','topics/cafe\\u0301.md'],
               ['/Topic/a','/topic/b'],['//Topic/a','//topic/b'],['topics/Stra\\u00dfe/a','topics/STRASSE/b']):
 try:portable_aliases(values)
 except ContractError:pass
 else:raise AssertionError('A portable component alias was accepted')
for values in (['topics/shared/a.md','topics/shared/b.md','topics/shared/a.md'],
               ['/topics/a','/topics/b'],['//topics/a','//topics/b'],['a//b','a/./b'],['.','/','//']):
 portable_aliases(values)
print('portable alias and root-anchor controls passed')
'''
            result=w.run([sys.executable,'-I','-c',program,helper.pack/'scripts'],check=False)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertIn('portable alias and root-anchor controls passed',result.stdout)

    def fixture(self,w):
        helper=recall.RetrievalContract();repo,bank,project=helper.fixture(w)
        metadata,path=helper.add(bank,project,'observed',body='# Authored count\n\nThe authored count is three.\n')
        return helper,repo,bank,project,path

    def paused(self,w,helper,repo,project,stage):
        directory=w.inside('rendezvous');directory.mkdir()
        w.env.update({'AGENT_MEMORY_FAULT_STAGE':stage,'AGENT_MEMORY_FAULT_MODE':'pause','AGENT_MEMORY_FAULT_DIR':str(directory)})
        request=helper.save(w,helper.request(project,query='authored count'))
        child=w.spawn([sys.executable,'-I',helper.pack/'scripts/retrieve.py','--repo',repo,'--request',request])
        deadline=time.monotonic()+5
        while not (directory/(stage+'.ready')).exists():
            if child.poll() is not None or time.monotonic()>deadline:
                self.fail('Public reader did not reach its owned source-recheck rendezvous')
            time.sleep(.02)
        return child,directory

    def refused(self,child,directory,stage):
        (directory/(stage+'.release')).touch()
        stdout,stderr=child.communicate(timeout=5)
        self.assertEqual(child.returncode,1,(stdout,stderr))
        value=json.loads(stdout);self.assertFalse(value['verified']);self.assertNotIn('results',value)

    def test_omitted_context_occupied_by_directory_or_fifo_is_not_absence(self):
        for kind in ('directory','fifo'):
            with self.subTest(kind=kind),World() as w:
                helper,repo,bank,project,path=self.fixture(w);code=helper.code(w,repo)
                target=bank/'productContext.md'
                if kind=='directory':target.mkdir()
                else:os.mkfifo(target)
                result,value=helper.retrieve(w,repo,project,query='authored count')
                self.assertEqual(result.returncode,1,value);self.assertNotIn('results',value)
                self.assertTrue(target.exists());self.assertEqual(helper.code(w,repo),code)
                if kind=='directory':target.rmdir()
                else:target.unlink()
                self.assertEqual(helper.retrieve(w,repo,project,query='authored count')[0].returncode,0)

    def test_changed_source_after_read_or_at_final_recheck_is_unavailable(self):
        for stage in ('recall-after-source-read','recall-final-source-recheck'):
            with self.subTest(stage=stage),World() as w:
                helper,repo,bank,project,path=self.fixture(w);code=helper.code(w,repo)
                child,directory=self.paused(w,helper,repo,project,stage)
                target=bank/path;target.write_text(target.read_text().replace('count is three.','count is four.'))
                self.refused(child,directory,stage)
                self.assertIn('count is four.',target.read_text());self.assertEqual(helper.code(w,repo),code)

    def test_fifo_marker_arriving_at_final_recheck_refuses_without_blocking_or_removal(self):
        stage='recall-final-source-recheck'
        for name in ('.agent-memory.lock','.agent-memory-transaction.md'):
            with self.subTest(marker=name),World() as w:
                helper,repo,bank,project,path=self.fixture(w);code=helper.code(w,repo)
                child,directory=self.paused(w,helper,repo,project,stage)
                target=bank/name;os.mkfifo(target)
                self.refused(child,directory,stage)
                self.assertTrue(stat.S_ISFIFO(target.lstat().st_mode));self.assertEqual(helper.code(w,repo),code)
                target.unlink()  # Explicit owner of this disposable injected FIFO.

    def test_owned_lock_disappearance_or_replacement_refuses_public_maintenance_before_write(self):
        stage='transaction-locked-before-inventory'
        for change in ('disappeared','changed'):
            with self.subTest(change=change),World() as w:
                helper=maintenance.ConsolidationContract();repo,bank=helper.fixture(w)
                first,p1=helper.add(bank,'first',body='# Retry policy\n\nUse three attempts for the authored case.\n')
                _,p2=helper.add(bank,'second',body='# Attempt cap\n\nThe authored case permits three attempts.\n')
                value=helper.compared(w,repo,first,'# Retry policy\n\nUse three attempts.\n',[p1,p2])
                result,prepared=helper.preview(w,repo,value);self.assertEqual(result.returncode,0,prepared)
                self.assertIsNotNone(prepared['proposal']);before=helper.state(w,repo,bank)
                proposal=helper.save(w,prepared['proposal'],'maintenance.json')
                directory=w.inside('owned-rendezvous');directory.mkdir()
                w.env.update({'AGENT_MEMORY_FAULT_STAGE':stage,'AGENT_MEMORY_FAULT_MODE':'pause','AGENT_MEMORY_FAULT_DIR':str(directory)})
                child=w.spawn([sys.executable,'-I',helper.pack/'scripts/consolidate_learning.py','apply','--proposal',proposal])
                deadline=time.monotonic()+8
                while not (directory/(stage+'.ready')).exists():
                    if child.poll() is not None or time.monotonic()>deadline:
                        self.fail('Public maintenance did not reach its actual owned transaction preflight')
                    time.sleep(.02)
                lock=bank/'.agent-memory.lock';self.assertTrue(lock.is_file())
                if change=='disappeared':lock.unlink()
                else:lock.write_text('Foreign replacement injected by the disposable fixture owner.\n')
                (directory/(stage+'.release')).touch();stdout,stderr=child.communicate(timeout=5)
                self.assertEqual(child.returncode,1,(stdout,stderr));self.assertFalse(json.loads(stdout)['verified'])
                if change=='changed':
                    self.assertEqual(lock.read_text(),'Foreign replacement injected by the disposable fixture owner.\n')
                    lock.unlink()  # Explicit owner of this injected foreign record.
                self.assertEqual(helper.state(w,repo,bank),before)
