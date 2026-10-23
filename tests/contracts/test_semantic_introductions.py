"""Meaningful router/index prose participates in the same public semantic review."""
import json
from pathlib import Path
import sys
import unittest
import uuid
from world import World
from record_fixtures import project_id
from contracts import test_semantic_sharing as fixtures


class SemanticIntroductionsContract(unittest.TestCase):
    save=fixtures.SemanticSharingContract.save
    state=fixtures.SemanticSharingContract.state
    fixture=fixtures.SemanticSharingContract.fixture
    call=fixtures.SemanticSharingContract.call
    good=fixtures.SemanticSharingContract.good
    judgment=fixtures.SemanticSharingContract.judgment
    review=fixtures.SemanticSharingContract.review
    add=fixtures.SemanticSharingContract.add
    publish=fixtures.SemanticSharingContract.publish
    apply=fixtures.SemanticSharingContract.apply

    def test_router_and_custom_index_claims_are_disclosed_and_isolated_without_losing_independent_routes(self):
        for container in ('router','index'):
            with self.subTest(container=container),World() as w:
                repo,bank,peer,base=self.fixture(w)
                if container=='router':path='MEMORY.md'
                else:
                    _,detail=self.add(bank,'detail','# Runtime detail\n\nUse the isolated build directory.\n')
                    request={'path':'topics/index.md','project-id':project_id((bank/'MEMORY.md').read_text()),
                        'updated':'2026-10-23T00:00:00Z','routes':[{'path':detail,'purpose':'Runtime detail','scope':'repository',
                        'lifecycle':'active','read-when':'Before isolated builds'}]}
                    index=json.loads(w.run([sys.executable,'-I',self.pack/'scripts/bank_layout.py','render-index','--input',self.save(w,request)]).stdout)['content']
                    path='topics/index.md';(bank/path).write_text(index)
                    router=bank/'MEMORY.md';text=router.read_text()
                    lines=text.splitlines(keepends=True);row=next(line for line in lines if '['+detail+']' in line)
                    text=text.replace(row,'| [topics/index.md](<topics/index.md>) | Runtime topic map | repository | active | Before isolated builds |\n');router.write_text(text)
                source=bank/path;source.write_text(source.read_text().replace('\n## Routes\n','\nThe project wall should be yellow throughout this repository.\n\n## Routes\n'))
                other=str(uuid.uuid4());_,blue=self.add(peer/'memory-bank','blue','# Wall\n\nThe project wall should be blue throughout this repository.\n',developer=other)
                (peer/'memory-bank/projectbrief.md').write_text('# Project\n\nIndependent landed orientation must continue.\n');self.publish(w,peer)
                (bank/'activeContext.md').write_text('# Focus\n\nIndependent unpublished local learning.\n')
                before=self.state(w,repo,bank);view=self.good(w,'inspect','--repo',repo)
                yellow=[row for row in view['sources'] if row['path']==path and 'wall should be yellow' in row['body']]
                self.assertEqual(len(yellow),1);self.assertEqual(yellow[0]['component'],'introduction')
                self.assertIn('routes',yellow[0]['container'])
                value=self.judgment(view,lambda a,b:'contradiction' if {a['path'],b['path']}=={path,blue} and any('wall should be yellow' in row['body'] for row in (a,b)) else 'related')
                r,plan=self.review(w,repo,value);self.assertEqual(r.returncode,0,plan)
                self.assertEqual(plan['blocked_introduction_paths'],[path]);self.assertIn(other,plan['other_developer_ids'])
                self.assertIn('wall should be yellow',plan['refresh_files'][path]);self.assertNotIn(blue,plan['refresh_files'])
                self.assertNotIn('wall should be yellow',plan['publish_files'][path]);self.assertIn(blue,plan['publish_files'])
                self.assertIn('Independent landed orientation',plan['refresh_files']['projectbrief.md'])
                self.assertIn('Independent unpublished',plan['publish_files']['activeContext.md'])
                if container=='index':
                    self.assertIn('topics/detail.md',plan['publish_files'])
                    self.assertIn('topics/index.md',plan['publish_files']['MEMORY.md'])
                self.assertEqual(before,self.state(w,repo,bank))
                done=self.apply(w,repo,plan['refresh_files']);self.assertTrue(done['verified'])
                self.assertEqual(before[1:],self.state(w,repo,bank)[1:])

    def test_compatible_router_introduction_changes_preserve_both_meanings_and_routes(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            router=bank/'MEMORY.md';text=router.read_text().replace('\n## Routes\n','\n## Build note\nUse the isolated directory.\n\n## Review note\nRead actual sources.\n\n## Routes\n');router.write_text(text)
            w.git('reset','--','code.py',cwd=repo);w.git('add','memory-bank',cwd=repo);w.git('commit','-m','Shared meaningful introduction',cwd=repo);w.git('push','origin','HEAD:main',cwd=repo);w.git('add','code.py',cwd=repo);w.git('pull','--ff-only',cwd=peer)
            router.write_text(text.replace('Use the isolated directory.','Use the verified isolated directory.'))
            peer_router=peer/'memory-bank/MEMORY.md';peer_router.write_text(text.replace('Read actual sources.','Read actual sources and evidence.'));self.publish(w,peer)
            before=self.state(w,repo,bank);view=self.good(w,'inspect','--repo',repo)
            value=self.judgment(view,lambda a,b:'compatible' if a['path']==b['path']=='MEMORY.md' else 'related')
            r,plan=self.review(w,repo,value);self.assertEqual(r.returncode,0,plan)
            for files in (plan['refresh_files'],plan['publish_files']):
                self.assertIn('verified isolated directory',files['MEMORY.md']);self.assertIn('sources and evidence',files['MEMORY.md'])
            self.assertEqual(before,self.state(w,repo,bank))

    def test_disputed_route_purpose_read_condition_and_omission_cannot_leak_into_independent_publication(self):
        for location in ('purpose','read-when','omission','new-index-route'):
            with self.subTest(location=location),World() as w:
                repo,bank,peer,base=self.fixture(w)
                _,independent=self.add(bank,'independent','# Build lesson\n\nUse the isolated build directory.\n')
                if location=='new-index-route':
                    _,target=self.add(bank,'dependent','# Scoped lesson\n\nInspect the dependent subsystem.\n')
                    request={'path':'topics/index.md','project-id':project_id((bank/'MEMORY.md').read_text()),
                        'updated':'2026-10-23T00:00:00Z','routes':[{'path':target,
                        'purpose':'The project wall should be yellow throughout this repository.',
                        'scope':'repository','lifecycle':'active','read-when':'Before the dependent work'}]}
                    content=json.loads(w.run([sys.executable,'-I',self.pack/'scripts/bank_layout.py','render-index','--input',self.save(w,request)]).stdout)['content']
                    content=content.replace('\n## Routes\n','\nA custom dependent index introduction remains local.\n\n## Routes\n')
                    (bank/'topics/index.md').write_text(content);router=bank/'MEMORY.md';lines=router.read_text().splitlines(keepends=True)
                    router.write_text(''.join('| [topics/index.md](<topics/index.md>) | Dependent topic map | repository | active | Before dependent work |\n' if '['+target+']' in line else line for line in lines))
                    source_path='topics/index.md'
                else:
                    source_path='MEMORY.md';router=bank/source_path;text=router.read_text()
                    if location=='omission':
                        target='productContext.md';lines=text.splitlines(keepends=True)
                        text=''.join('- `productContext.md`: The project wall should be yellow throughout this repository.\n' if line.startswith('- `productContext.md`:') else line for line in lines)
                    else:
                        target='projectbrief.md';lines=text.splitlines(keepends=True);changed=[]
                        for line in lines:
                            if '[projectbrief.md]' in line:
                                cells=line.split('|');cells[2 if location=='purpose' else 5]=' The project wall should be yellow throughout this repository ';line='|'.join(cells)
                            changed.append(line)
                        text=''.join(changed)
                    router.write_text(text)
                _,blue=self.add(peer/'memory-bank','wall-blue','# Wall\n\nThe project wall should be blue throughout this repository.\n');self.publish(w,peer)
                before=self.state(w,repo,bank);view=self.good(w,'inspect','--repo',repo)
                yellow=[row for row in view['sources'] if row['path']==source_path and 'wall should be yellow' in row['body']]
                self.assertEqual(len(yellow),1);self.assertEqual(yellow[0]['component'],'omission' if location=='omission' else 'route-prose')
                self.assertEqual(yellow[0]['target_path'],target)
                value=self.judgment(view,lambda a,b:'contradiction' if {a['key'],b['key']}=={yellow[0]['key'],next(row['key'] for row in view['sources'] if row['path']==blue)} else 'related')
                r,plan=self.review(w,repo,value);self.assertEqual(r.returncode,0,plan)
                self.assertIn(source_path,plan['blocked_paths']);self.assertIn(target,plan['blocked_paths'])
                self.assertIn(independent,plan['publish_files']);self.assertIn(blue,plan['publish_files'])
                self.assertNotIn('wall should be yellow',json.dumps(plan['publish_files']))
                self.assertNotIn(blue,plan['refresh_files'])
                if location=='new-index-route':
                    self.assertNotIn(target,plan['publish_files']);self.assertNotIn(source_path,plan['publish_files'])
                    self.assertIn('custom dependent index',plan['refresh_files'][source_path])
                self.assertEqual(before,self.state(w,repo,bank))
                proof=self.good(w,'check','--repo',repo,'--input',self.save(w,plan['proof']))
                self.assertTrue(proof['source_observation_current'])

    def test_disputed_invalidation_caption_keeps_erasure_and_revocation_guards(self):
        from contracts import test_governance as governance
        for operation in ('delete','revoke'):
            with self.subTest(operation=operation),World() as w:
                repo,bank,peer,base=self.fixture(w)
                old,path=self.add(bank,'obsolete','# Obsolete note\n\nAn old observation explicitly invalidated by the current operator.\n')
                w.git('reset','--','code.py',cwd=repo);self.publish(w,repo);w.git('add','code.py',cwd=repo);w.git('pull','--ff-only',cwd=peer)
                status=json.loads(w.run([sys.executable,'-I',self.pack/'scripts/scope_binding.py','status','--repo',repo]).stdout)
                gov=governance.GovernanceContract();gov.pack=self.pack;gov.pid=status['project_id'];gov.actor=status['developer_id']
                cached=gov.projection(w,repo,old);request=gov.request(bank,old,path,operation)
                if operation=='delete':
                    request['authority'].update(basis='explicit-deletion',reason_class='user-request');request['delete_ids']=[old['id']]
                r,proposal=gov.preview(w,repo,request);self.assertEqual(r.returncode,0,proposal)
                r,done=gov.apply(w,proposal['proposal']);self.assertEqual(r.returncode,0,done)
                # The fixture stages its memory removal before boundary use;
                # unrelated staged feature code remains part of preservation proof.
                w.git('add','-u','memory-bank',cwd=repo)
                guard_path='history/tombstones/'+old['id']+'.md' if operation=='delete' else path
                guard=(bank/guard_path).read_text();router=bank/'MEMORY.md';changed=[]
                for line in router.read_text().splitlines(keepends=True):
                    if '['+guard_path+']' in line:
                        cells=line.split('|');cells[2]=' The project wall should be yellow throughout this repository ';line='|'.join(cells)
                    changed.append(line)
                router.write_text(''.join(changed))
                _,blue=self.add(peer/'memory-bank','blue','# Wall\n\nThe project wall should be blue throughout this repository.\n');self.publish(w,peer)
                before=self.state(w,repo,bank);view=self.good(w,'inspect','--repo',repo)
                yellow=next(row for row in view['sources'] if row['component']=='route-prose' and row['target_path']==guard_path)
                bluekey=next(row['key'] for row in view['sources'] if row['path']==blue)
                value=self.judgment(view,lambda a,b:'contradiction' if {a['key'],b['key']}=={yellow['key'],bluekey} else 'related')
                r,plan=self.review(w,repo,value);self.assertEqual(r.returncode,0,plan)
                for files in (plan['publish_files'],plan['refresh_files']):
                    self.assertIn(guard_path,files);self.assertEqual(files[guard_path],guard)
                    if operation=='delete':self.assertNotIn(path,files)
                    else:self.assertIn('status: "revoked"',files[path])
                self.assertNotIn('wall should be yellow',plan['publish_files']['MEMORY.md'])
                self.assertEqual(before,self.state(w,repo,bank))
                done=self.apply(w,repo,plan['refresh_files']);self.assertTrue(done['verified'])
                r=w.run([sys.executable,'-I',self.pack/'scripts/projection.py','check','--repo',repo,'--input',self.save(w,cached)],check=False)
                self.assertEqual(r.returncode,1);self.assertEqual((bank/guard_path).read_text(),guard)

    def test_identical_prose_routed_under_multiple_conditions_cannot_claim_only_one_disjoint_variant(self):
        with World() as w:
            repo,bank,peer,base=self.fixture(w)
            body='# Wall\n\nThe project wall should be yellow.\n'
            (bank/'projectbrief.md').write_text(body)
            w.git('reset','--','code.py',cwd=repo);w.git('add','memory-bank',cwd=repo);w.git('commit','-m','Known shared prose',cwd=repo);w.git('push','origin','HEAD:main',cwd=repo);w.git('add','code.py',cwd=repo);w.git('pull','--ff-only',cwd=peer)
            for target,branch in ((bank,'older'),(peer/'memory-bank','main')):
                router=target/'MEMORY.md';lines=router.read_text().splitlines(keepends=True)
                router.write_text(''.join(line.replace('| repository | active |','| branch:'+branch+' | active |') if '[projectbrief.md]' in line else line for line in lines))
            _,blue=self.add(peer/'memory-bank','blue','# Wall\n\nThe project wall should be blue.\n',conditions={'branch':'main'})
            self.publish(w,peer);before=self.state(w,repo,bank);view=self.good(w,'inspect','--repo',repo)
            yellow=next(row for row in view['sources'] if row['path']=='projectbrief.md')
            self.assertEqual(set(yellow['sides']),{'local','landed'});self.assertNotEqual(yellow['scope_chains']['local'],yellow['scope_chains']['landed'])
            value=self.judgment(view,lambda a,b:'disjoint-conditions' if {a['path'],b['path']}=={'projectbrief.md',blue} else 'related')
            r,refused=self.review(w,repo,value);self.assertEqual(r.returncode,1,refused);self.assertEqual(before,self.state(w,repo,bank))


if __name__=='__main__':unittest.main()
