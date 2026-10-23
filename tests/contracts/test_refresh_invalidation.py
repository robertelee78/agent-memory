"""Landed erasure defeats stale local edits; unknown conditions never become current."""
from datetime import datetime, timezone
from pathlib import Path
import unittest
import uuid

from world import World
from refresh_fixtures import RefreshFixture
from contracts.test_retrieval import RetrievalContract


class RefreshInvalidationContract(RefreshFixture):
    def test_landed_tombstone_removes_a_locally_edited_old_record_without_staging_or_resurrection(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            record, path = self.imported(w, peer, 'old', '# Old lesson\n\nThis guidance will be explicitly erased.\n')
            first = self.apply_refresh(w, self.prepared_refresh(w, repo)); self.assertTrue(first['remote_freshness_verified'])
            w.git('add','memory-bank',cwd=repo)
            w.git('commit','--only','-m','Ordinary local memory checkpoint','--','memory-bank',cwd=repo)
            (bank/path).write_text((bank/path).read_text()+'\nUnpublished old-body edit must not restore erased guidance.\n')
            tomb = self.good(w,'tombstones.py','render','--input',self.save(w,{
                'id':record['id'],'scope':record['project-id'],'deleted-at':datetime.now(timezone.utc).isoformat(),
                'deletion-authority':str(uuid.uuid4()),'reason-class':'user-request'}))
            shared = peer/'memory-bank'; (shared/path).unlink(); target = shared/tomb['path']; target.parent.mkdir(parents=True)
            target.write_text(tomb['content']); router = shared/'MEMORY.md'
            text = ''.join(line for line in router.read_text().splitlines(keepends=True) if '['+path+']' not in line)
            row = '| ['+tomb['path']+'](<'+tomb['path']+'>) | Terminal deletion guard | repository | deleted | Before any reuse |\n'
            router.write_text(text.replace('\n## Omitted contexts\n',row+'\n## Omitted contexts\n'))
            w.git('add','--all','--','memory-bank',cwd=peer); w.git('commit','-m','Explicit shared erasure',cwd=peer); w.git('push','origin','HEAD:main',cwd=peer)
            before = self.state(w,repo,bank)
            recall = RetrievalContract().request(record['project-id'],ids=[record['id']],query='')
            proposal = self.prepared_refresh(w,repo,recall=recall); done = self.apply_refresh(w,proposal)
            self.assertTrue(done['remote_freshness_verified']); self.assertFalse((bank/path).exists())
            self.assertEqual((bank/tomb['path']).read_text(),tomb['content']); self.assertEqual(done['recall']['results'],[])
            self.assertEqual(before[1:],self.state(w,repo,bank)[1:])
            repeated = self.apply_refresh(w,proposal)
            self.assertTrue(repeated['receipt']['already_applied']); self.assertFalse((bank/path).exists())
            self.assertEqual(repeated['recall']['results'],[])

    def test_imported_unknown_or_other_branch_conditions_stay_out_of_current_recall(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w)
            unknown, one = self.imported(w,peer,'unknown','# Unknown applicability\n\nIts component conditions still need clarification.\n',component=None)
            other, two = self.imported(w,peer,'other-branch','# Other branch\n\nThis applies only to a different branch.\n',branch='another-feature')
            before = self.state(w,repo,bank)
            recall = RetrievalContract().request(unknown['project-id'],ids=[unknown['id'],other['id']],query='')
            done = self.apply_refresh(w,self.prepared_refresh(w,repo,recall=recall))
            self.assertTrue(done['remote_freshness_verified']); self.assertEqual(done['recall']['results'],[])
            self.assertEqual(done['recalled_import_paths'],[]); self.assertTrue((bank/one).exists()); self.assertTrue((bank/two).exists())
            self.assertEqual(before[1:],self.state(w,repo,bank)[1:])
