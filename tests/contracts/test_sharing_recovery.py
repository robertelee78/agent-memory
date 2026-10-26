"""Actual local-remote retry/readback, cursor loss and deletion-aware Git merges."""
import hashlib
import json
import shutil
from pathlib import Path
import shutil
import sys
import time
import unittest
import uuid

from world import World
from identity_fixtures import bind
from record_fixtures import record_text, project_id
from contracts import test_sharing_boundary as boundary

ROOT = Path(__file__).resolve().parents[2]


class SharingRecoveryContract(unittest.TestCase):
    def save(self, w, value, name='recovery-input.json'):
        path=w.inside(name);path.write_text(json.dumps(value));return path

    def call(self,w,*args):
        p=w.run([sys.executable,'-I',self.pack/'scripts/sharing_recovery.py',*args],check=False)
        return p,json.loads(p.stdout)

    def good(self,w,*args):
        p,v=self.call(w,*args);self.assertEqual(p.returncode,0,(v,p.stderr));return v

    def fixture(self,w,root='memory-bank'):
        helper=boundary.SharingBoundaryContract();repo,bank=helper.fixture(w,root);self.pack=helper.pack
        w.git('reset','--','code.py',cwd=repo)
        (bank/'activeContext.md').write_text('# Current focus\n\nA useful isolated session focus.\n')
        router=bank/'MEMORY.md';text=router.read_text()
        text=''.join(line for line in text.splitlines(keepends=True) if not line.startswith('- `activeContext.md`:'))
        text=text.replace('\n## Omitted contexts\n','| [activeContext.md](<activeContext.md>) | Session focus | repository | active | At a boundary |\n\n## Omitted contexts\n')
        router.write_text(text);w.git('add',root,cwd=repo);w.git('commit','-m','Useful fixture context',cwd=repo)
        w.git('add','code.py',cwd=repo)
        w.git('push','origin','HEAD:main',cwd=repo)
        remote=w.git('remote','get-url','origin',cwd=repo).stdout.strip()
        peer=w.clone(remote,'peer')
        base=w.git('rev-parse','HEAD',cwd=repo).stdout.strip()
        return repo,bank,peer,base

    def state(self,w,repo,bank):
        return ({p.relative_to(bank).as_posix():p.read_bytes() for p in bank.rglob('*') if p.is_file()},
                (repo/'code.py').read_bytes(),(repo/'.git/index').read_bytes(),w.git('rev-parse','HEAD',cwd=repo).stdout,
                w.git('symbolic-ref','HEAD',cwd=repo).stdout)

    def publish(self,w,peer,path,text):
        target=peer/path;target.parent.mkdir(parents=True,exist_ok=True);target.write_text(text)
        w.git('add','--',path,cwd=peer);w.git('commit','-m','Reviewed isolated memory change',cwd=peer)
        candidate=w.git('rev-parse','HEAD',cwd=peer).stdout.strip();w.git('push','origin','HEAD:main',cwd=peer)
        return candidate

    def test_offline_capture_stays_readable_and_online_observation_preserves_feature_state(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);before=self.state(w,repo,bank)
            initial=self.good(w,'inspect','--repo',repo)
            self.assertFalse(initial['remote_freshness_verified']);self.assertEqual(initial['sharing_state'],'pending-stale')
            remote=Path(w.git('remote','get-url','origin',cwd=repo).stdout.strip());away=remote.with_name('offline.git');remote.rename(away)
            pending=self.good(w,'inspect','--repo',repo,'--fetch');self.assertFalse(pending['remote_freshness_verified'])
            self.assertTrue(pending['local_memory_available']);self.assertFalse(pending['publication_verified'])
            failed=w.run([sys.executable,'-I',self.pack/'scripts/refresh_memory.py','inspect','--v2','--repo',repo,'--output',w.inside('offline-inspection.json')],check=False)
            diagnostic=json.loads(failed.stdout)
            self.assertEqual(failed.returncode,1,diagnostic)
            self.assertEqual(diagnostic['failure_component'],'authority-advertisement')
            self.assertEqual(diagnostic['failure_code'],'authority-advertisement-unavailable')
            self.assertIn('configured authority branch',diagnostic['reason'])
            self.assertIn('Preserve the bank',diagnostic['repair'])
            self.assertNotIn(str(remote),failed.stdout+failed.stderr)
            self.assertEqual(before,self.state(w,repo,bank))
            away.rename(remote)
            online=self.good(w,'inspect','--repo',repo,'--fetch');self.assertEqual(online['observation']['authority_head'],base)
            self.assertTrue(online['remote_freshness_verified']);self.assertEqual(before,self.state(w,repo,bank))
            self.assertEqual(w.git('for-each-ref','--format=%(refname)','refs/agent-memory',cwd=repo).stdout.strip(),'')

    def test_lost_success_response_and_squash_readback_do_not_require_a_duplicate_push(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);before=self.state(w,repo,bank)
            content=(peer/'memory-bank/projectbrief.md').read_text()+'\nReviewed branch-independent learning.\n'
            candidate=self.publish(w,peer,'memory-bank/projectbrief.md',content)
            # The caller loses the successful push response and all optional cursors.
            state=Path(w.env['AGENT_MEMORY_STATE_HOME']);shutil.rmtree(state,ignore_errors=True)
            request=self.save(w,{'base':base,'candidate':candidate})
            first=self.good(w,'replay','--repo',repo,'--request',request)
            self.assertTrue(first['publication_verified']);self.assertEqual(first['retry_state'],'already-landed')
            self.assertTrue(first['intended_delta_current']);self.assertFalse(first['canonical_writes_performed']);self.assertTrue(first['git_metadata_writes_performed'])
            again=self.good(w,'replay','--repo',repo,'--request',request)
            self.assertEqual(again['witnessed_commit'],candidate);self.assertEqual(before,self.state(w,repo,bank))
            # Distinct commit identity, same reviewed bytes (squash/recreated commit).
            w.git('checkout','-b','recreated',base,cwd=peer)
            (peer/'memory-bank/projectbrief.md').write_text(content);w.git('add','memory-bank/projectbrief.md',cwd=peer)
            w.git('commit','-m','Different candidate identity with same memory-only delta',cwd=peer)
            alternate=w.git('rev-parse','HEAD',cwd=peer).stdout.strip();w.git('push','origin','HEAD:refs/heads/proposed',cwd=peer)
            # Fetch only the local candidate object; it has not landed as that commit.
            w.git('fetch','origin','refs/heads/proposed',cwd=repo)
            readback=self.good(w,'replay','--repo',repo,'--request',self.save(w,{'base':base,'candidate':alternate}))
            self.assertTrue(readback['intended_delta_landed']);self.assertEqual(readback['witnessed_commit'],candidate)

    def merged_publication(self, w, retain):
        repo,bank,peer,base=self.fixture(w);before=self.state(w,repo,bank)
        original=(bank/'projectbrief.md').read_text()
        intended='# Project\n\nThe intended proposed memory value.\n'
        retained='# Project\n\nThe authority retained a different reviewed value.\n'
        # Build a genuine memory-only candidate without the original staged code.
        w.env['GIT_INDEX_FILE']=str(w.inside('candidate-private-index'))
        try:
            w.git('read-tree',base,cwd=repo);(bank/'projectbrief.md').write_text(intended)
            w.git('add','memory-bank/projectbrief.md',cwd=repo)
            tree=w.git('write-tree',cwd=repo).stdout.strip()
            candidate=w.git('commit-tree',tree,'-p',base,'-m','Proposed memory-only value',cwd=repo).stdout.strip()
        finally:
            w.env.pop('GIT_INDEX_FILE');(bank/'projectbrief.md').write_text(original)
        self.assertEqual(before,self.state(w,repo,bank))
        self.publish(w,peer,'memory-bank/projectbrief.md',retained)
        w.git('fetch',str(repo),candidate,cwd=peer)
        merge=w.git('merge','--no-ff',candidate,'-m','Integrate proposed memory with explicit resolution',cwd=peer,check=False)
        self.assertEqual(merge.returncode,1);self.assertEqual(w.git('diff','--name-only','--diff-filter=U',cwd=peer).stdout.strip(),'memory-bank/projectbrief.md')
        (peer/'memory-bank/projectbrief.md').write_text(intended if retain else retained)
        w.git('add','memory-bank/projectbrief.md',cwd=peer);w.git('commit','-m','Resolved actual authority memory',cwd=peer)
        landed=w.git('rev-parse','HEAD',cwd=peer).stdout.strip();w.git('push','origin','HEAD:main',cwd=peer)
        lineage=w.git('rev-list','--first-parent',landed,'^'+base,cwd=peer).stdout.splitlines()
        self.assertNotIn(candidate,lineage)
        self.assertIn(candidate,w.git('rev-list',landed,'^'+base,cwd=peer).stdout.splitlines())
        actual=[w.git('show',revision+':memory-bank/projectbrief.md',cwd=peer).stdout for revision in lineage]
        self.assertEqual(any(text==intended for text in actual),retain)
        return repo,bank,peer,base,candidate,landed,before

    def test_reachable_side_parent_discarded_by_main_is_not_a_publication_witness(self):
        with World() as w:
            repo,bank,peer,base,candidate,landed,before=self.merged_publication(w,False)
            value=self.good(w,'replay','--repo',repo,'--request',self.save(w,{'base':base,'candidate':candidate}))
            self.assertFalse(value['publication_verified']);self.assertFalse(value['intended_delta_landed'])
            self.assertIsNone(value['witnessed_commit']);self.assertEqual(value['retry_state'],'reconciliation-required')
            self.assertFalse(value['safe_to_retry_without_reconciliation']);self.assertEqual(before,self.state(w,repo,bank))

    def test_actual_merge_tree_retaining_delta_is_a_witness_even_after_later_correction(self):
        with World() as w:
            repo,bank,peer,base,candidate,landed,before=self.merged_publication(w,True)
            request=self.save(w,{'base':base,'candidate':candidate})
            value=self.good(w,'replay','--repo',repo,'--request',request)
            self.assertTrue(value['publication_verified']);self.assertTrue(value['intended_delta_current'])
            self.assertEqual(value['witnessed_commit'],landed);self.assertNotEqual(landed,candidate)
            self.publish(w,peer,'memory-bank/projectbrief.md','# Project\n\nA later confirmed correction remains authoritative.\n')
            historical=self.good(w,'replay','--repo',repo,'--request',request)
            self.assertTrue(historical['intended_delta_landed']);self.assertFalse(historical['intended_delta_current'])
            self.assertEqual(historical['witnessed_commit'],landed);self.assertEqual(before,self.state(w,repo,bank))

    def test_later_correction_keeps_historical_publication_receipt_without_claiming_current_truth(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            candidate=self.publish(w,peer,'memory-bank/projectbrief.md','# Project\n\nInitial actual learning.\n')
            self.publish(w,peer,'memory-bank/projectbrief.md','# Project\n\nLater confirmed correction.\n')
            value=self.good(w,'replay','--repo',repo,'--request',self.save(w,{'base':base,'candidate':candidate}))
            self.assertTrue(value['intended_delta_landed']);self.assertFalse(value['intended_delta_current'])
            self.assertEqual(value['witnessed_commit'],candidate)
            self.assertIn('Later confirmed correction',(peer/'memory-bank/projectbrief.md').read_text())

    def test_changed_default_remote_history_and_unrelated_candidate_code_require_review(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);before=self.state(w,repo,bank)
            (peer/'code.py').write_text('FEATURE = 99\n');w.git('add','code.py',cwd=peer);w.git('commit','-m','Code not eligible as memory publication',cwd=peer)
            candidate=w.git('rev-parse','HEAD',cwd=peer).stdout.strip();w.git('push','origin','HEAD:refs/heads/proposed',cwd=peer)
            w.git('fetch','origin','refs/heads/proposed',cwd=repo)
            p,v=self.call(w,'replay','--repo',repo,'--request',self.save(w,{'base':base,'candidate':candidate}));self.assertEqual(p.returncode,1)
            remote=Path(w.git('remote','get-url','origin',cwd=repo).stdout.strip())
            w.git('symbolic-ref','HEAD','refs/heads/other',cwd=remote)
            p,v=self.call(w,'inspect','--repo',repo,'--fetch');self.assertEqual(p.returncode,0);self.assertFalse(v['remote_freshness_verified'])
            self.assertEqual(before,self.state(w,repo,bank))

    def test_cursor_loss_combines_independent_edits_and_repeated_normal_merge_without_reversing_memory(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            # A separate feature branch keeps its own staged code and local memory.
            w.git('checkout','-b','feature',cwd=repo)
            (bank/'activeContext.md').write_text('# Current focus\n\nUseful unpublished feature observation.\n')
            landed='# Project\n\nA source-linked landed shared goal.\n'
            candidate=self.publish(w,peer,'memory-bank/projectbrief.md',landed)
            before=self.state(w,repo,bank)
            value=self.good(w,'reconcile','--repo',repo)
            self.assertEqual(value['base'],base);self.assertTrue(value['semantic_review_required'])
            self.assertEqual(value['candidate_files']['activeContext.md'],(bank/'activeContext.md').read_text())
            self.assertEqual(value['candidate_files']['projectbrief.md'],landed)
            self.assertFalse(value['publication_verified']);self.assertEqual(before,self.state(w,repo,bank))
            # The explicit reviewed mechanical plan uses the existing recoverable
            # writer; automatic semantic review and boundary refresh are later stories.
            files=self.save(w,value['candidate_files'],'files.json')
            r=w.run([sys.executable,'-I',self.pack/'scripts/bank_transactions.py','preview','--repo',repo,'--files',files,'--operation','refresh'])
            proposal=self.save(w,json.loads(r.stdout),'apply.json')
            w.run([sys.executable,'-I',self.pack/'scripts/bank_transactions.py','apply','--proposal',proposal])
            w.git('add','memory-bank',cwd=repo);w.git('commit','-m','Reviewed imported memory with feature work',cwd=repo)
            # A normal code merge includes the already imported landed memory.
            w.git('fetch','origin',cwd=repo);w.git('merge','--no-edit','origin/main',cwd=repo)
            repeated=self.good(w,'reconcile','--repo',repo)
            self.assertEqual(repeated['candidate_files']['projectbrief.md'],landed)
            self.assertEqual(repeated['changed_memory_paths'],[])
            self.assertIn('unpublished feature observation',repeated['candidate_files']['activeContext.md'])
            self.assertFalse(repeated['feature_code_freshness_verified'])

    def add_record(self,w,repo,bank):
        body=record_text('A branch lesson that will be explicitly erased.\n',project_id((bank/'MEMORY.md').read_text()))
        identity=json.loads(next(x.split(': ',1)[1] for x in body.splitlines() if x.startswith('id: ')))
        path='topics/lesson.md';(bank/'topics').mkdir(exist_ok=True);(bank/path).write_text(body)
        row='| ['+path+'](<'+path+'>) | Conditional lesson | repository | active | Before retry work |\n'
        p=bank/'MEMORY.md';p.write_text(p.read_text().replace('\n## Omitted contexts\n',row+'\n## Omitted contexts\n'))
        w.git('add','memory-bank',cwd=repo);w.git('commit','-m','Typed isolated admitted fixture',cwd=repo)
        w.git('push','origin','HEAD:main',cwd=repo)
        return identity

    def test_older_branch_edit_cannot_restore_tombstoned_learning_after_later_code_merge_or_cursor_loss(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);identity=self.add_record(w,repo,bank)
            base=w.git('rev-parse','HEAD',cwd=repo).stdout.strip();w.git('pull','--ff-only',cwd=peer)
            w.git('checkout','-b','old-feature',cwd=repo)
            (bank/'topics/lesson.md').write_text((bank/'topics/lesson.md').read_text().replace('will be explicitly erased','was locally edited before erasure'))
            peer_bank=peer/'memory-bank';(peer_bank/'topics/lesson.md').unlink()
            path='history/tombstones/'+identity+'.md';target=peer_bank/path;target.parent.mkdir(parents=True)
            tomb={'id':identity,'scope':project_id((peer_bank/'MEMORY.md').read_text()),'deleted-at':'2026-10-23T01:00:00Z','deletion-authority':str(uuid.uuid4()),'reason-class':'user-request'}
            target.write_text('---\n'+''.join(k+': '+json.dumps(v)+'\n' for k,v in tomb.items())+'---\n')
            router=peer_bank/'MEMORY.md';rows=router.read_text().splitlines(keepends=True)
            text=''.join(x for x in rows if '[topics/lesson.md]' not in x)
            text=text.replace('\n## Omitted contexts\n','| ['+path+'](<'+path+'>) | Terminal deletion guard | repository | deleted | Before recall |\n\n## Omitted contexts\n')
            router.write_text(text)
            # Stage all tombstone/deletion files as one actual remote commit.
            w.git('add','-A','memory-bank',cwd=peer);w.git('commit','-m','Complete explicit erasure',cwd=peer);w.git('push','origin','HEAD:main',cwd=peer)
            before=self.state(w,repo,bank);value=self.good(w,'reconcile','--repo',repo)
            self.assertNotIn('topics/lesson.md',value['candidate_files']);self.assertIn(path,value['candidate_files'])
            self.assertNotIn('locally edited',json.dumps(value));self.assertEqual(before,self.state(w,repo,bank))
            # No remembered private cursor is necessary to derive this plan.
            shutil.rmtree(Path(w.env['AGENT_MEMORY_STATE_HOME']),ignore_errors=True)
            again=self.good(w,'reconcile','--repo',repo);self.assertEqual(value['candidate_files'],again['candidate_files'])

    def test_last_observed_memory_base_does_not_require_importing_authority_code_ancestry(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);w.git('checkout','-b','stale-feature',cwd=repo)
            (peer/'memory-bank/projectbrief.md').write_text('# Project\n\nFirst landed shared goal.\n')
            (peer/'code.py').write_text('VALUE = 99  # authority code must stay outside feature refresh\n')
            w.git('add','memory-bank/projectbrief.md','code.py',cwd=peer);w.git('commit','-m','Authority code and independently useful memory',cwd=peer)
            observed=w.git('rev-parse','HEAD',cwd=peer).stdout.strip();w.git('push','origin','HEAD:main',cwd=peer)
            first=self.good(w,'reconcile','--repo',repo)
            self.assertEqual(first['observation']['authority_head'],observed)
            files=self.save(w,first['candidate_files'],'first-import.json')
            r=w.run([sys.executable,'-I',self.pack/'scripts/bank_transactions.py','preview','--repo',repo,'--files',files,'--operation','refresh'])
            w.run([sys.executable,'-I',self.pack/'scripts/bank_transactions.py','apply','--proposal',self.save(w,json.loads(r.stdout),'first-apply.json')])
            self.assertEqual(w.git('merge-base','--is-ancestor',observed,'HEAD',cwd=repo,check=False).returncode,1)
            self.assertEqual(w.git('rev-parse','HEAD',cwd=repo).stdout.strip(),base)
            (bank/'activeContext.md').write_text('# Current focus\n\nLocal unpublished work must be preserved.\n')
            latest='# Project\n\nA later landed shared goal.\n';self.publish(w,peer,'memory-bank/projectbrief.md',latest)
            before=self.state(w,repo,bank)
            second=self.good(w,'reconcile','--repo',repo,'--base',observed)
            self.assertEqual(second['candidate_files']['projectbrief.md'],latest)
            self.assertIn('Local unpublished work',second['candidate_files']['activeContext.md'])
            self.assertFalse(second['feature_code_freshness_verified']);self.assertEqual(before,self.state(w,repo,bank))
            self.assertIn('VALUE = 2',(repo/'code.py').read_text());self.assertNotIn('VALUE = 99',(repo/'code.py').read_text())

    def test_both_changed_same_prose_requires_meaning_review_without_choosing_landing_order(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            (bank/'projectbrief.md').write_text('# Project\n\nThe wall should be yellow.\n')
            self.publish(w,peer,'memory-bank/projectbrief.md','# Project\n\nThe wall should be blue.\n')
            before=self.state(w,repo,bank);p,v=self.call(w,'reconcile','--repo',repo)
            self.assertEqual(p.returncode,1);self.assertEqual(v['sharing_state'],'pending-stale')
            self.assertNotIn('candidate_files',v);self.assertEqual(before,self.state(w,repo,bank))

    def test_normal_rebase_keeps_previously_imported_memory_once_and_local_feature_code(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);w.git('checkout','-b','feature-rebase',cwd=repo)
            landed='# Project\n\nA verified shared goal imported before code rebase.\n'
            self.publish(w,peer,'memory-bank/projectbrief.md',landed)
            value=self.good(w,'reconcile','--repo',repo)
            files=self.save(w,value['candidate_files'],'rebase-files.json')
            r=w.run([sys.executable,'-I',self.pack/'scripts/bank_transactions.py','preview','--repo',repo,'--files',files,'--operation','refresh'])
            proposal=self.save(w,json.loads(r.stdout),'rebase-apply.json')
            w.run([sys.executable,'-I',self.pack/'scripts/bank_transactions.py','apply','--proposal',proposal])
            w.git('add','memory-bank',cwd=repo);w.git('commit','-m','Feature plus reviewed imported memory',cwd=repo)
            w.git('fetch','origin',cwd=repo);w.git('rebase','origin/main',cwd=repo)
            before=self.state(w,repo,bank);repeated=self.good(w,'reconcile','--repo',repo)
            self.assertEqual(repeated['candidate_files']['projectbrief.md'],landed)
            self.assertEqual(repeated['changed_memory_paths'],[]);self.assertEqual(before,self.state(w,repo,bank))
            self.assertIn('VALUE = 2',(repo/'code.py').read_text())

    def test_current_untracked_admitted_record_does_not_block_fresh_remote_inventory(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            body=record_text('Useful local admitted learning not committed yet.\n',project_id((bank/'MEMORY.md').read_text()))
            (bank/'topics').mkdir();(bank/'topics/new.md').write_text(body)
            router=bank/'MEMORY.md';router.write_text(router.read_text().replace('\n## Omitted contexts\n',
                '| [topics/new.md](<topics/new.md>) | Useful local lesson | repository | active | Before matching work |\n\n## Omitted contexts\n'))
            before=self.state(w,repo,bank);value=self.good(w,'inspect','--repo',repo,'--fetch')
            self.assertTrue(value['remote_freshness_verified']);self.assertFalse(value['publication_verified'])
            self.assertEqual(before,self.state(w,repo,bank))

    def test_inactive_stable_version_cannot_be_reactivated_by_an_older_working_copy(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);identity=self.add_record(w,repo,bank)
            path=bank/'topics/lesson.md';original=path.read_text()
            revoked=original.replace('status: "active"','status: "revoked"')
            revoked=revoked.replace('\n---\n','\nstate-reason: "Actual confirmed fixture revocation"\n---\n',1)
            path.write_text(revoked);router=bank/'MEMORY.md'
            original_router=router.read_text();router.write_text(original_router.replace('| repository | active | Before retry work |','| repository | revoked | Before retry work |'))
            w.git('reset','--','code.py',cwd=repo);w.git('add','memory-bank',cwd=repo);w.git('commit','-m','Confirmed inactive fixture baseline',cwd=repo)
            w.git('push','origin','HEAD:main',cwd=repo);w.git('add','code.py',cwd=repo)
            # Losing a cursor must not let an older body override that baseline.
            path.write_text(original);router.write_text(original_router)
            before=self.state(w,repo,bank);p,value=self.call(w,'reconcile','--repo',repo)
            self.assertEqual(p.returncode,1);self.assertNotIn('candidate_files',value)
            self.assertEqual(before,self.state(w,repo,bank))

    def test_remote_advance_during_observation_requires_new_inventory(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);before=self.state(w,repo,bank)
            barrier=w.inside('fetch barrier')
            w.env.update(AGENT_MEMORY_FAULT_STAGE='sharing-after-fetch',AGENT_MEMORY_FAULT_MODE='pause',AGENT_MEMORY_FAULT_DIR=str(barrier))
            child=w.spawn([sys.executable,'-I',self.pack/'scripts/sharing_recovery.py','inspect','--repo',repo,'--fetch'])
            ready=barrier/'sharing-after-fetch.ready';deadline=time.monotonic()+15
            while not ready.exists() and time.monotonic()<deadline:time.sleep(.02)
            self.assertTrue(ready.exists(),'actual fetch did not reach barrier')
            candidate=self.publish(w,peer,'memory-bank/projectbrief.md','# Project\n\nConcurrent actual learning.\n')
            (barrier/'sharing-after-fetch.release').touch()
            out,err=child.communicate(timeout=30);self.assertEqual(child.returncode,0,err)
            self.assertFalse(json.loads(out)['remote_freshness_verified'])
            for key in ['AGENT_MEMORY_FAULT_STAGE','AGENT_MEMORY_FAULT_MODE','AGENT_MEMORY_FAULT_DIR']:w.env.pop(key)
            retried=self.good(w,'inspect','--repo',repo,'--fetch')
            self.assertEqual(retried['observation']['authority_head'],candidate)
            self.assertEqual(before,self.state(w,repo,bank))

    def test_nested_bank_observation_reconciliation_and_replay_use_same_configured_prefix(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w,'docs/shared memory')
            candidate=self.publish(w,peer,'docs/shared memory/projectbrief.md','# Project\n\nA nested-bank shared lesson.\n')
            before=self.state(w,repo,bank);value=self.good(w,'reconcile','--repo',repo)
            self.assertEqual(value['observation']['bank_root'],'docs/shared memory')
            self.assertTrue(all(p.startswith('docs/shared memory/') for p in value['changed_memory_paths']))
            replay=self.good(w,'replay','--repo',repo,'--request',self.save(w,{'base':base,'candidate':candidate}))
            self.assertTrue(replay['publication_verified']);self.assertFalse((repo/'memory-bank').exists())
            self.assertEqual(before,self.state(w,repo,bank))



    def test_parallel_worktree_observers_leave_remote_tracking_refs_unchanged(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            other=w.inside('parallel observer')
            w.git('worktree','add','-b','parallel-observer',other,'HEAD',cwd=repo)
            bind(w,other)
            (other/'code.py').write_text('VALUE = 3  # separate staged observer work\n')
            w.git('add','code.py',cwd=other)
            def state(checkout):
                index=Path(w.git('rev-parse','--path-format=absolute','--git-path','index',cwd=checkout).stdout.strip())
                return ((checkout/'code.py').read_bytes(),index.read_bytes(),
                        w.git('rev-parse','HEAD',cwd=checkout).stdout,
                        {p.relative_to(checkout/'memory-bank').as_posix():p.read_bytes() for p in (checkout/'memory-bank').rglob('*') if p.is_file()})
            before=[state(checkout) for checkout in (repo,other)]
            tracking=w.git('for-each-ref','--format=%(refname) %(objectname)','refs/remotes',cwd=repo).stdout
            content=(peer/'memory-bank/projectbrief.md').read_text()+'\nIndependent landed peer policy.\n'
            landed=self.publish(w,peer,'memory-bank/projectbrief.md',content)
            # Preserve actual Git stderr only inside this disposable fixture;
            # production still discloses no remote/body/credential stderr.
            log=w.inside('private-git-observation-errors.txt');log.write_text('');log.chmod(0o600)
            binary=w.inside('observation-bin');binary.mkdir()
            real=shutil.which('git',path=w.env['PATH'])
            wrapper=binary/'git'
            wrapper.write_text('#!'+sys.executable+'\nimport os,sys\nfd=os.open('+repr(str(log))+',os.O_WRONLY|os.O_APPEND)\nos.dup2(fd,2);os.close(fd)\nos.execv('+repr(real)+',['+repr(real)+',*sys.argv[1:]])\n')
            wrapper.chmod(0o755);w.env['PATH']=str(binary)+':'+w.env['PATH']
            children=[w.spawn([sys.executable,'-I',self.pack/'scripts/sharing_recovery.py','inspect','--fetch','--repo',checkout]) for checkout in (repo,other)]
            observations=[]
            for process in children:
                stdout,stderr=process.communicate(timeout=90)
                observations.append((process.returncode,stdout,stderr))
            self.assertTrue(all(row[0]==0 for row in observations),(observations,log.read_text()))
            for _,stdout,_ in observations:
                value=json.loads(stdout)
                self.assertTrue(value['remote_freshness_verified'],value)
                self.assertEqual(value['observation']['authority_head'],landed)
            self.assertEqual(w.git('for-each-ref','--format=%(refname) %(objectname)','refs/remotes',cwd=repo).stdout,tracking,log.read_text())
            self.assertEqual([state(checkout) for checkout in (repo,other)],before)
            self.assertEqual(w.git('for-each-ref','--format=%(refname)','refs/agent-memory/observations',cwd=repo).stdout,'')



    def test_fetch_failure_after_real_advertisement_has_specific_safe_repair(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w);before=self.state(w,repo,bank)
            binary=w.inside('fetch failure adapter');binary.mkdir()
            real=shutil.which('git',path=w.env['PATH']);wrapper=binary/'git'
            wrapper.write_text('#!'+sys.executable+'\nimport os,sys\nif "fetch" in sys.argv[1:]:\n sys.stderr.write("PRIVATE_FETCH_FAILURE\\n");sys.exit(1)\nos.execv('+repr(real)+',['+repr(real)+',*sys.argv[1:]])\n')
            wrapper.chmod(0o755);w.env['PATH']=str(binary)+':'+w.env['PATH']
            # The adapter leaves actual identity and ls-remote intact, then
            # fails the public refresh's transport only at the fetch stage.
            failed=w.run([sys.executable,'-I',self.pack/'scripts/refresh_memory.py','inspect','--v2','--repo',repo,'--output',w.inside('fetch-inspection.json')],check=False)
            diagnostic=json.loads(failed.stdout)
            self.assertEqual(failed.returncode,1,diagnostic)
            self.assertEqual(diagnostic['failure_component'],'authority-fetch')
            self.assertEqual(diagnostic['failure_code'],'authority-fetch-unavailable')
            self.assertIn('configured authority branch',diagnostic['reason'])
            self.assertIn('Preserve the bank',diagnostic['repair'])
            self.assertNotIn('PRIVATE_FETCH_FAILURE',failed.stdout+failed.stderr)
            self.assertEqual(before,self.state(w,repo,bank))


if __name__=='__main__':unittest.main()
