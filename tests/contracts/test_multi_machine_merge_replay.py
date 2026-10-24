"""Actual later code integration and old refresh replay must honor landed erasure."""
from datetime import datetime, timezone
import json
import sys
import unittest
import uuid

import installed_cli_fixtures as installed
from contracts import test_retrieval as retrieval
from multi_machine_fixtures import MultiMachineFixture
from world import World


class MultiMachineMergeReplay(MultiMachineFixture):
    def test_erasure_survives_an_ordinary_code_merge_and_pre_erasure_proposal_replay(self):
        with World() as w:
            self.cli = installed.install(w)
            repo,bank,peer,_base = self.fixture(w)
            record,path = self.imported(w,peer,'erased-after-import',
                                        '# Old lesson\n\nThis explicitly erased lesson must not return.\n')
            recall = retrieval.RetrievalContract().request(record['project-id'],ids=[record['id']],query='')
            old_proposal = self.prepared_refresh(w,repo,recall=recall)
            imported = self.apply_refresh(w,old_proposal)
            self.assertTrue(imported['remote_freshness_verified'])
            self.assertEqual(len(imported['recall']['results']),1)
            w.git('add','memory-bank',cwd=repo)
            w.git('commit','--only','-m','Ordinary memory checkpoint before erasure','--','memory-bank',cwd=repo)
            (bank/path).write_text((bank/path).read_text()+'\nAn unpublished old-body edit cannot reverse erasure.\n')
            # Construct existing authority-side deletion fixture data through
            # the portable public renderer. Sharing/recall under test use the
            # installed CLI throughout; no unknown route falls back to source.
            deletion = {'id':record['id'],'scope':record['project-id'],
                        'deleted-at':datetime.now(timezone.utc).isoformat(),
                        'deletion-authority':str(uuid.uuid4()),'reason-class':'user-request'}
            rendered = w.run([sys.executable,'-I',self.pack/'scripts/tombstones.py','render',
                              '--input',self.save(w,deletion)])
            tomb = json.loads(rendered.stdout)
            shared = peer/'memory-bank'; (shared/path).unlink()
            target = shared/tomb['path']; target.parent.mkdir(parents=True)
            target.write_text(tomb['content'])
            router = shared/'MEMORY.md'
            text = ''.join(line for line in router.read_text().splitlines(keepends=True) if '['+path+']' not in line)
            row = '| ['+tomb['path']+'](<'+tomb['path']+'>) | Terminal deletion guard | repository | deleted | Before any reuse |\n'
            router.write_text(text.replace('\n## Omitted contexts\n',row+'\n## Omitted contexts\n'))
            (peer/'code.py').write_text('VALUE = 900  # authority code awaits ordinary integration\n')
            w.git('add','--all','--','memory-bank','code.py',cwd=peer)
            w.git('commit','-m','Explicit shared erasure and independent authority code',cwd=peer)
            w.git('push','origin','HEAD:main',cwd=peer)
            before = self.protected(w,repo)
            erased = self.apply_refresh(w,self.prepared_refresh(w,repo,recall=recall))
            self.assertTrue(erased['remote_freshness_verified'])
            self.assertEqual(erased['recall']['results'],[])
            self.assertFalse((bank/path).exists())
            self.assertEqual((bank/tomb['path']).read_text(),tomb['content'])
            self.assertEqual(self.protected(w,repo),before)
            # Only this explicit ordinary Git operation integrates feature code.
            w.git('add','code.py','memory-bank',cwd=repo)
            w.git('commit','-m','Ordinary feature checkpoint after memory-only refresh',cwd=repo)
            w.git('fetch','origin',cwd=repo)
            merged = w.git('merge','--no-ff','origin/main','-m','Ordinary code integration after erasure',cwd=repo,check=False)
            if merged.returncode:
                self.assertEqual(merged.returncode,1)
                self.assertEqual(w.git('diff','--name-only','--diff-filter=U',cwd=repo).stdout.strip(),'code.py')
                (repo/'code.py').write_text('VALUE = 900  # explicit ordinary feature-code resolution\n')
                w.git('add','code.py',cwd=repo)
                w.git('commit','-m','Resolve only ordinary feature-code conflict',cwd=repo)
            self.assertEqual(len(w.git('rev-list','--parents','-n','1','HEAD',cwd=repo).stdout.split()),3)
            self.good(w,'bank_contract.py','validate','--repo',repo)
            after_merge = self.state(w,repo,bank)
            process,replayed = self.call(w,'refresh_memory.py','apply',
                                         '--input',self.save(w,old_proposal,'pre-erasure-proposal.json'))
            if process.returncode==0:self.assertFalse(replayed['canonical_writes_performed'])
            else:self.assertEqual(process.returncode,1,(replayed,process.stderr))
            self.assertEqual(self.state(w,repo,bank),after_merge)
            fresh = self.apply_refresh(w,self.prepared_refresh(w,repo,recall=recall))
            self.assertTrue(fresh['remote_freshness_verified'])
            self.assertFalse(fresh['feature_code_freshness_verified'])
            self.assertEqual(fresh['recall']['results'],[])
            self.assertFalse((bank/path).exists())
            self.assertEqual((bank/tomb['path']).read_text(),tomb['content'])
            self.assertEqual(self.state(w,repo,bank)[1:],after_merge[1:])
            self.assertIn('VALUE = 900',(repo/'code.py').read_text())
            self.event('erasure-after-ordinary-code-merge',actual_two_parent_merge=True,
                       pre_erasure_proposal_replayed=True,old_body_absent=True,terminal_guard_retained=True,
                       fresh_boundary_verified=True,deleted_lesson_recalled=False)


if __name__=='__main__':unittest.main()
