"""Disposable cache loss/corruption must heal to useful canonical recall."""
import hashlib
import fcntl
import json
import mmap
import os
from pathlib import Path
import shutil
import sys
import unittest
import time

from world import World
from contracts import test_discovery as discovery_fixtures
from contracts import test_governance as governance_fixtures


class RecallCatalogContract(unittest.TestCase):
    fixture=discovery_fixtures.DiscoveryContract.fixture
    call=discovery_fixtures.DiscoveryContract.call
    good=discovery_fixtures.DiscoveryContract.good
    save=discovery_fixtures.DiscoveryContract.save
    trace=discovery_fixtures.DiscoveryContract.trace
    recall=discovery_fixtures.DiscoveryContract.recall

    def ready(self,w):
        repo,bank=self.fixture(w)
        meta,path=self.h.add(bank,'attempt-policy',body='# Attempt policy\n\nRepeat transient requests at most three times.\n')
        trace=self.trace(w,repo,bank,[path]);request=self.recall(trace,path)
        before=self.h.state(w,repo,bank)
        first=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
        self.assertEqual([r['record']['id'] for r in first['results']],[meta['id']])
        self.assertFalse(first['optional_indexes_used']);self.assertTrue(first['metrics']['catalog_updated'])
        self.assertEqual(before,self.h.state(w,repo,bank))
        pointer=next(w.paths['state'].rglob('recall-catalog-v1/current.json'))
        return repo,bank,meta,request,pointer,before

    def test_warm_semantic_selection_matches_cold_sources_without_storing_learning_bodies(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            warm=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertTrue(warm['optional_indexes_used']);self.assertEqual(warm['results'][0]['record'],meta)
            self.assertEqual(warm['results'][0]['body'],'# Attempt policy\n\nRepeat transient requests at most three times.\n')
            generation=pointer.parent/json.loads(pointer.read_text())['file']
            self.assertNotIn(b'Repeat transient requests at most three times.',generation.read_bytes())
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_missing_cache_automatically_rebuilds_and_returns_the_same_useful_memory(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            shutil.rmtree(pointer.parent)
            rebuilt=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertFalse(rebuilt['optional_indexes_used']);self.assertTrue(rebuilt['metrics']['catalog_updated'])
            self.assertEqual(rebuilt['results'][0]['record']['id'],meta['id'])
            self.assertTrue(self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))['optional_indexes_used'])
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_ordinary_recall_preserves_optional_navigation_catalog_without_republishing_it(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            directory=pointer.parent
            snapshot={p.name:(p.read_bytes(),p.stat().st_mtime_ns) for p in directory.iterdir() if p.is_file()}
            ordinary={key:value for key,value in request.items() if key!='discovery'}
            result=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,ordinary))
            self.assertEqual([row['record']['id'] for row in result['results']],[meta['id']])
            self.assertFalse(result['optional_indexes_used']);self.assertFalse(result['metrics']['catalog_updated'])
            self.assertFalse(result['derived_cache_writes_performed'])
            self.assertEqual(snapshot,{p.name:(p.read_bytes(),p.stat().st_mtime_ns) for p in directory.iterdir() if p.is_file()})
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_caller_modified_metadata_cannot_forge_a_successful_empty_selection(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            generation=pointer.parent/json.loads(pointer.read_text())['file']
            header,raw=generation.read_bytes().split(b'\n',1);header=json.loads(header);payload=json.loads(raw)
            payload['checked']['entries']=[]
            raw=json.dumps(payload,sort_keys=True,separators=(',',':'),ensure_ascii=False).encode()
            header.update(payload_bytes=len(raw),payload_sha256=hashlib.sha256(raw).hexdigest())
            generation.write_bytes(json.dumps(header,separators=(',',':')).encode()+b'\n'+raw)
            rebuilt=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertFalse(rebuilt['optional_indexes_used']);self.assertTrue(rebuilt['metrics']['catalog_updated'])
            self.assertEqual(rebuilt['results'][0]['record']['id'],meta['id'])
            self.assertTrue(self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))['optional_indexes_used'])
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_common_installed_cli_reuses_the_same_validated_catalog(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            installed=self.good(w,'installer.py','install','--development-source',self.pack,'--presets','none')
            cli=Path(installed['receipt']['bin_dir'])/'am'
            for _ in range(2):
                result=w.run([cli,'search','--repo',repo,'--request',self.save(w,request)])
                recalled=json.loads(result.stdout)
                self.assertTrue(recalled['optional_indexes_used'])
                self.assertEqual(recalled['results'][0]['record']['id'],meta['id'])
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_warm_policy_matches_full_reader_for_hidden_unsafe_and_conditional_corrections(self):
        with World() as w:
            repo,bank=self.fixture(w)
            cases=[]
            variants=[('hidden',{'visibility':'hidden'},False),
                ('other-kind',{'kind':'project-decision'},False),
                ('unsafe',{},False),('expired',{'valid-until':'2020-01-01T00:00:00Z'},True),
                ('untrusted',{'trust':'untrusted'},True),
                ('unknown',{'conditions':{'prerequisite':None}},True),
                ('other-branch',{'conditions':{'branch':'feature/different'}},True)]
            for name,updates,old_remains in variants:
                old,path=self.h.add(bank,'old-'+name)
                body='# Correction\n\npassword=WITHHELD_CORRECTION_BODY\n' if name=='unsafe' else '# Correction\n\nAn independently qualified replacement.\n'
                self.h.add(bank,'new-'+name,body=body,supersedes=[{'id':old['id'],'version':1}],**updates)
                cases.append((name,old,path,old_remains))
            for name,old,path,old_remains in cases:
                with self.subTest(name=name):
                    request=self.recall(self.trace(w,repo,bank,[path]),path)
                    w.env['AGENT_MEMORY_NO_RECALL_CATALOG']='1'
                    cold=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
                    w.env.pop('AGENT_MEMORY_NO_RECALL_CATALOG')
                    self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
                    warm=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
                    self.assertTrue(warm['optional_indexes_used'])
                    self.assertEqual(warm['results'],cold['results'])
                    self.assertEqual(bool(warm['results']),old_remains)
                    self.assertNotIn('WITHHELD_CORRECTION_BODY',json.dumps(warm))

    def test_late_mmap_change_with_restored_mtime_cannot_pass_the_final_whole_content_check(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            stage='catalog-final-source-recheck';rendezvous=w.inside('rendezvous')
            w.env.update(AGENT_MEMORY_FAULT_STAGE=stage,AGENT_MEMORY_FAULT_MODE='pause',AGENT_MEMORY_FAULT_DIR=str(rendezvous))
            child=w.spawn([sys.executable,'-I',self.pack/'scripts/retrieve.py','--repo',repo,'--request',self.save(w,request)])
            ready=rendezvous/(stage+'.ready');deadline=time.monotonic()+8
            while not ready.exists() and child.poll() is None and time.monotonic()<deadline:time.sleep(.01)
            self.assertTrue(ready.exists(),'The public warm reader did not reach its final source check')
            target=bank/'projectbrief.md';stamp=target.stat();original=target.read_bytes()
            with target.open('r+b') as stream,mmap.mmap(stream.fileno(),0) as mapped:
                position=original.index(b'Project');mapped[position:position+7]=b'Changed';mapped.flush()
            os.utime(target,ns=(stamp.st_atime_ns,stamp.st_mtime_ns))
            (rendezvous/(stage+'.release')).touch()
            out,err=child.communicate(timeout=30);failed=json.loads(out)
            self.assertEqual(child.returncode,1,(out,err));self.assertNotIn('results',failed)
            self.assertFalse(pointer.exists())
            self.assertEqual(before[1:],self.h.state(w,repo,bank)[1:])

    def test_actual_forget_removes_owned_generations_before_any_later_recall(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            path=request['paths'][0];directory=pointer.parent
            pointer_bytes=pointer.read_bytes();old_name=json.loads(pointer_bytes)['file'];old_generation=(directory/old_name).read_bytes()
            g=governance_fixtures.GovernanceContract();g.pack=self.pack;g.pid=self.h.pid;g.actor=self.h.actor
            deletion=g.request(bank,meta,path,'delete')
            deletion['authority'].update(basis='explicit-deletion',reason_class='user-request');deletion['delete_ids']=[meta['id']]
            result,plan=g.preview(w,repo,deletion);self.assertEqual(result.returncode,0,plan)
            result,done=g.apply(w,plan['proposal']);self.assertEqual(result.returncode,0,done)
            self.assertFalse((bank/path).exists());self.assertFalse(pointer.exists())
            self.assertEqual(list(directory.glob('g-*.catalog')),[])
            # Restoring a previously read immutable witness cannot resurrect the
            # erased source; the actual bank is requalified on the next call.
            (directory/old_name).write_bytes(old_generation);(directory/old_name).chmod(0o600)
            pointer.write_bytes(pointer_bytes);pointer.chmod(0o600)
            fresh=self.trace(w,repo,bank,[]);query=self.recall(fresh,path);query['paths']=[]
            recalled=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,query))
            self.assertEqual(recalled['results'],[])
            query.pop('discovery');query['ids']=[meta['id']]
            recalled=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,query))
            self.assertEqual(recalled['results'],[])
            self.assertEqual(before[1:],self.h.state(w,repo,bank)[1:])

    def test_corrupt_oversized_cleanup_header_defers_without_decoding_it_or_failing_actual_forget(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            generation=pointer.parent/json.loads(pointer.read_text())['file']
            # Valid but over-bound JSON: parsing this is unnecessary work. A
            # narrow spy makes the pre-decode limit a deterministic assertion.
            raw=json.dumps({'padding':'x'*17000}).encode()+b'\n{}';generation.write_bytes(raw)
            program='''import sys,json
sys.path.insert(0,sys.argv[1])
from agent_memory import recall_catalog as c
original=c.json.loads;oversized=[]
def bounded(value,*args,**kwargs):
 if len(value)>c.MAX_HEADER:oversized.append(len(value))
 return original(value,*args,**kwargs)
c.json.loads=bounded
assert c.invalidate_after_change(sys.argv[2],{})=='deferred-to-canonical-fallback'
assert not oversized,'Oversized header was decoded before its bound was checked'
print('bounded cleanup fallback')
'''
            result=w.run([sys.executable,'-I','-c',program,self.pack/'scripts',repo],check=False)
            self.assertEqual(result.returncode,0,result.stderr);self.assertIn('bounded cleanup fallback',result.stdout)
            g=governance_fixtures.GovernanceContract();g.pack=self.pack;g.pid=self.h.pid;g.actor=self.h.actor
            deletion=g.request(bank,meta,request['paths'][0],'delete')
            deletion['authority'].update(basis='explicit-deletion',reason_class='user-request');deletion['delete_ids']=[meta['id']]
            result,plan=g.preview(w,repo,deletion);self.assertEqual(result.returncode,0,plan)
            result,done=g.apply(w,plan['proposal']);self.assertEqual(result.returncode,0,done)
            self.assertTrue(done['verified']);self.assertEqual(done['derived_cache_state'],'deferred-to-canonical-fallback')
            self.assertFalse((bank/request['paths'][0]).exists());self.assertEqual(generation.read_bytes(),raw)
            result,replayed=g.apply(w,plan['proposal']);self.assertEqual(result.returncode,0,replayed)
            self.assertTrue(replayed['already_applied']);self.assertEqual(replayed['derived_cache_state'],'deferred-to-canonical-fallback')
            self.assertEqual(before[1:],self.h.state(w,repo,bank)[1:])

    def test_full_namespace_preserves_unknown_evidence_and_stops_adding_cache_files(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            generation=pointer.parent/json.loads(pointer.read_text())['file'];generation.write_bytes(b'corrupt disposable generation')
            while len(list(pointer.parent.iterdir()))<256:
                evidence=pointer.parent/('unknown-'+str(len(list(pointer.parent.iterdir()))))
                evidence.write_bytes(b'Preserve this unknown evidence.');evidence.chmod(0o600)
            preserved={p.name:p.read_bytes() for p in pointer.parent.iterdir()}
            for _ in range(2):
                rebuilt=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
                self.assertFalse(rebuilt['optional_indexes_used']);self.assertFalse(rebuilt['metrics']['catalog_updated'])
                self.assertEqual(rebuilt['results'][0]['record']['id'],meta['id'])
                self.assertEqual(preserved,{p.name:p.read_bytes() for p in pointer.parent.iterdir()})
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_changed_executing_package_rebuilds_instead_of_reusing_an_old_engine_witness(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            source=self.pack/'scripts/agent_memory/records.py'
            source.write_bytes(source.read_bytes()+b'\n# An actual new executing package generation.\n')
            rebuilt=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertFalse(rebuilt['optional_indexes_used']);self.assertTrue(rebuilt['metrics']['catalog_updated'])
            self.assertEqual(rebuilt['results'][0]['record']['id'],meta['id'])
            warm=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertTrue(warm['optional_indexes_used']);self.assertEqual(warm['results'],rebuilt['results'])
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_unavailable_bounded_boot_manifest_disables_only_the_optional_accelerator(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            extra=self.pack/'optional-generation.json'
            for variant in ('fifo','link','oversized','non-code-entries'):
                with self.subTest(variant=variant):
                    if variant=='fifo':os.mkfifo(extra,0o600)
                    elif variant=='link':extra.symlink_to(w.inside('outside-source'))
                    elif variant=='oversized':
                        with extra.open('wb') as stream:stream.truncate(2*1024*1024+1)
                    else:
                        extra.mkdir()
                        for n in range(4097):(extra/str(n)).touch()
                    recalled=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
                    self.assertFalse(recalled['optional_indexes_used']);self.assertFalse(recalled['metrics']['catalog_updated'])
                    self.assertEqual(recalled['results'][0]['record']['id'],meta['id'])
                    if extra.is_dir():shutil.rmtree(extra)
                    else:extra.unlink()
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_cached_global_oversize_guard_cannot_disappear_behind_record_conditions(self):
        with World() as w:
            repo,bank=self.fixture(w);_,path=self.h.add(bank,'bounded-claim')
            self.h.add(bank,'large-other-claim',body='# Large claim\n\n'+'x'*17000+'\n',
                scope='component:src/api',conditions={'prerequisite':'unobserved-required-condition'})
            request=self.recall(self.trace(w,repo,bank,[path]),path)
            cold=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertTrue(cold['metrics']['catalog_updated'])
            context={'component':'src/api'}
            started=self.good(w,'discover_memory.py','start','--repo',repo,'--intent','recall',
                '--task','Inspect the actual bounded claim with API applicability.', '--context',self.save(w,context,'context.json'))
            self.assertTrue(started['optional_indexes_used'])
            trace=started['discovery'];trace.update(complete=True,rationale='Read the actual plausible bounded source under this component.')
            trace['sources']=[{'path':path,'sha256':hashlib.sha256((bank/path).read_bytes()).hexdigest()}]
            request=self.recall(trace,path);request['context']=context
            for disabled in (False,True):
                if disabled:w.env['AGENT_MEMORY_NO_RECALL_CATALOG']='1'
                result,refused=self.call(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
                self.assertEqual(result.returncode,1,refused);self.assertNotIn('results',refused)

    def test_actual_loaded_policy_code_namespace_and_dependency_changes_invalidate_the_witness(self):
        with World() as w:
            repo,bank=self.fixture(w)
            program='''import sys,types,json
sys.path.insert(0,sys.argv[1])
from agent_memory import recall_catalog as c,records,retrieval
engine=c.generation();c.coherent(engine);checked=[]
def refused(label):
 try:c.coherent(engine)
 except c.CacheMiss:checked.append(label)
 else:raise AssertionError(label+' incorrectly retained engine identity')
original=records.MODEL['properties']['kind']['enum']
records.MODEL['properties']['kind']['enum']=[*original,'changed-loaded-policy']
refused('loaded-policy');records.MODEL['properties']['kind']['enum']=original;c.coherent(engine)
original=records.record.__code__;records.record.__code__=(lambda *args,**kwargs:None).__code__
refused('loaded-code');records.record.__code__=original;c.coherent(engine)
original=retrieval.bank_contract;retrieval.bank_contract=types.ModuleType('agent_memory.bank_contract')
refused('substituted-namespace');retrieval.bank_contract=original;c.coherent(engine)
import agent_memory.projections
refused('unexpected-native-dependency')
print(json.dumps(checked))
'''
            result=w.run([sys.executable,'-I','-c',program,self.pack/'scripts'],check=False)
            self.assertEqual(result.returncode,0,result.stderr)
            self.assertEqual(json.loads(result.stdout),['loaded-policy','loaded-code','substituted-namespace','unexpected-native-dependency'])

    def test_hint_inventory_charges_excluded_runtime_entries_against_its_physical_budget(self):
        with World() as w:
            repo,bank=self.fixture(w);isolated=w.inside('hint-fixture');isolated.mkdir()
            (isolated/'MEMORY.md').write_text('not consumed as a body by the hint inventory')
            hidden=isolated/'.agent-memory-derived';hidden.mkdir()
            for n in range(4):(hidden/str(n)).touch()
            program='''import sys
from pathlib import Path
sys.path.insert(0,sys.argv[1])
from agent_memory import recall_catalog as c
c.bank_observation.MAX_FILES=2
try:c.physical_inventory(Path(sys.argv[2]))
except c.CacheMiss:print('bounded runtime traversal')
else:raise AssertionError('Excluded runtime entries evaded the finite hint traversal budget')
'''
            result=w.run([sys.executable,'-I','-c',program,self.pack/'scripts',isolated],check=False)
            self.assertEqual(result.returncode,0,result.stderr);self.assertEqual(result.stdout.strip(),'bounded runtime traversal')

    def test_busy_optional_publisher_leaves_useful_recall_available_and_rebuilds_after_unlock(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w);pointer.unlink()
            with (pointer.parent/'publication.lock').open('r+b') as lock:
                fcntl.flock(lock.fileno(),fcntl.LOCK_EX|fcntl.LOCK_NB)
                result=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
                self.assertFalse(result['optional_indexes_used']);self.assertFalse(result['metrics']['catalog_updated'])
                self.assertEqual(result['results'][0]['record']['id'],meta['id'])
            healed=self.good(w,'retrieve.py','--repo',repo,'--request',self.save(w,request))
            self.assertTrue(healed['metrics']['catalog_updated']);self.assertEqual(healed['results'],result['results'])
            self.assertEqual(before,self.h.state(w,repo,bank))

    def test_loaded_immutable_reader_survives_concurrent_generation_replacement(self):
        with World() as w:
            repo,bank,meta,request,pointer,before=self.ready(w)
            old_generation=pointer.parent/json.loads(pointer.read_text())['file']
            stage='catalog-final-source-recheck';rendezvous=w.inside('reader-rendezvous')
            w.env.update(AGENT_MEMORY_FAULT_STAGE=stage,AGENT_MEMORY_FAULT_MODE='pause',AGENT_MEMORY_FAULT_DIR=str(rendezvous))
            path=self.save(w,request)
            child=w.spawn([sys.executable,'-I',self.pack/'scripts/retrieve.py','--repo',repo,'--request',path])
            ready=rendezvous/(stage+'.ready');deadline=time.monotonic()+8
            while not ready.exists() and child.poll() is None and time.monotonic()<deadline:time.sleep(.01)
            self.assertTrue(ready.exists(),'The immutable reader did not reach the final whole-content check')
            pointer.unlink()
            for key in ('AGENT_MEMORY_FAULT_STAGE','AGENT_MEMORY_FAULT_MODE','AGENT_MEMORY_FAULT_DIR'):w.env.pop(key)
            new=self.good(w,'retrieve.py','--repo',repo,'--request',path)
            self.assertTrue(new['metrics']['catalog_updated']);self.assertFalse(old_generation.exists())
            (rendezvous/(stage+'.release')).touch()
            out,err=child.communicate(timeout=30);old=json.loads(out)
            self.assertEqual(child.returncode,0,(out,err));self.assertTrue(old['optional_indexes_used'])
            self.assertEqual(old['results'],new['results']);self.assertEqual(before,self.h.state(w,repo,bank))
