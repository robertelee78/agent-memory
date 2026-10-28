"""A completed import remains quickly observable without staging feature memory."""
import copy
import hashlib
import json
from pathlib import Path
import sys
import unittest
from world import World
from refresh_fixtures import RefreshFixture
from contracts import test_semantic_sharing as semantic_helpers


class ReceiptSnapshotCurrent(RefreshFixture):
    def current(self, w, repo):
        response=w.run([sys.executable,'-I',self.pack/'scripts/installer.py','refresh','current','--repo',repo],check=False)
        return response,json.loads(response.stdout)

    def imported_fixture(self,w):
        repo,bank,peer,base=self.fixture(w)
        (peer/'memory-bank/activeContext.md').write_text('# Active context\n\nReviewed imported context edit.\n')
        record,path=self.imported(w,peer,'receipt-covered','# Receipt-covered lesson\n\nA durable same-project fact.\n')
        done=self.apply_refresh(w,self.prepared_refresh(w,repo))
        self.assertEqual(done['sharing_state'],'refreshed')
        return repo,bank,peer,path,done

    def test_import_then_current_then_full_noop_then_current_preserves_feature_state(self):
        with World() as w:
            repo,bank,peer,path,done=self.imported_fixture(w)
            before=self.state(w,repo,bank)
            for after_noop in (False,True):
                if after_noop:
                    again=self.apply_refresh(w,self.prepared_refresh(w,repo))
                    self.assertFalse(again['canonical_writes_performed'])
                response,value=self.current(w,repo)
                self.assertEqual(response.returncode,0,value)
                self.assertEqual(value['sharing_state'],'bank-synchronized')
                self.assertTrue(value['remote_freshness_verified'])
                self.assertFalse(value['canonical_writes_performed'])
                self.assertFalse(value['original_index_writes_performed'])
                self.assertEqual(before,self.state(w,repo,bank))
            self.assertIn('Reviewed imported context edit',(bank/'activeContext.md').read_text())

    def test_imported_remote_deletion_is_part_of_complete_snapshot(self):
        with World() as w:
            repo,bank,peer,path,done=self.imported_fixture(w)
            # Disposable setup: commit only memory so the next remote removal
            # leaves an older tracked index entry; staged feature code stays staged.
            w.git('add','memory-bank',cwd=repo)
            w.git('commit','--only','-m','Fixture tracked imported memory','--','memory-bank',cwd=repo)
            remote_bank=peer/'memory-bank'
            (remote_bank/path).unlink()
            router=remote_bank/'MEMORY.md'
            router.write_text(''.join(line for line in router.read_text().splitlines(keepends=True) if path not in line))
            w.git('add','memory-bank',cwd=peer);w.git('commit','-m','Reviewed remote lesson removal',cwd=peer);w.git('push','origin','HEAD:main',cwd=peer)
            deleted=self.apply_refresh(w,self.prepared_refresh(w,repo,base=done['observed_authority_head']))
            self.assertFalse((bank/path).exists())
            before=self.state(w,repo,bank)
            response,value=self.current(w,repo)
            self.assertEqual(response.returncode,0,value)
            self.assertEqual(value['sharing_state'],'bank-synchronized')
            self.assertEqual(before,self.state(w,repo,bank))

    def test_local_mutations_staged_memory_and_new_authority_remain_review_required(self):
        with World() as w:
            repo,bank,peer,path,done=self.imported_fixture(w)
            baseline=self.state(w,repo,bank)
            target=bank/path; original=target.read_bytes()
            for mutation in ('edit','delete','add','mode','stage'):
                if mutation=='edit': target.write_bytes(original+b'\nUnreviewed local change.\n')
                elif mutation=='delete':target.unlink()
                elif mutation=='add':semantic_helpers.SemanticSharingContract().add(bank,'private-local','# Unpublished local lesson\n\nPreserve local evidence.\n')
                elif mutation=='mode':target.chmod(0o755)
                else:w.git('add','memory-bank',cwd=repo)
                before=self.state(w,repo,bank)
                response,value=self.current(w,repo)
                self.assertNotEqual(value['sharing_state'],'bank-synchronized',(mutation,value))
                self.assertFalse(value['canonical_writes_performed'])
                self.assertEqual(before,self.state(w,repo,bank))
                if mutation=='add':
                    for relative in set(before[0])-set(baseline[0]):(bank/relative).unlink()
                    for relative,body in baseline[0].items():(bank/relative).write_bytes(body)
                elif mutation=='mode':target.chmod(0o644)
                elif mutation=='stage':
                    # Restore only disposable fixture index bytes, never a user's index.
                    index=Path(w.git('rev-parse','--path-format=absolute','--git-path','index',cwd=repo).stdout.strip())
                    index.write_bytes(baseline[2])
                else:target.write_bytes(original)
            self.imported(w,peer,'later-authority','# Later remote lesson\n\nRequires a new semantic review.\n')
            response,value=self.current(w,repo)
            self.assertEqual(value['sharing_state'],'review-required',value)
            self.assertEqual(baseline,self.state(w,repo,bank))

    def test_cursor_receipt_and_branch_pins_cannot_be_mixed_or_missing(self):
        with World() as w:
            repo,bank,peer,path,done=self.imported_fixture(w)
            cursor=next(w.paths['state'].rglob('refresh-cursor.json'))
            raw=cursor.read_bytes();value=json.loads(raw)
            receipt=Path(done['receipt']['receipt_path']);receipt_raw=receipt.read_bytes()
            for mutation in ('missing','corrupt','receipt','mixed','branch'):
                if mutation=='missing':cursor.unlink()
                elif mutation=='corrupt':cursor.write_text('{}')
                elif mutation in ('receipt','mixed'):
                    changed=json.loads(receipt_raw);changed['operation_id']='11111111-1111-4111-8111-111111111111'
                    receipt.write_text(json.dumps(changed))
                    if mutation=='mixed':
                        changed_cursor=copy.deepcopy(value)
                        changed_cursor['receipt_sha256']=hashlib.sha256(receipt.read_bytes()).hexdigest()
                        changed_cursor.pop('checksum')
                        changed_cursor['checksum']=hashlib.sha256((json.dumps(changed_cursor,sort_keys=True,ensure_ascii=False,indent=2,allow_nan=False)+'\n').encode()).hexdigest()
                        cursor.write_text(json.dumps(changed_cursor))
                else:w.git('symbolic-ref','HEAD','refs/heads/same-revision-other-branch',cwd=repo);w.git('update-ref','HEAD',value['git_context']['revision'],cwd=repo)
                before=self.state(w,repo,bank)
                response,observed=self.current(w,repo)
                self.assertNotEqual(observed['sharing_state'],'bank-synchronized',(mutation,observed))
                self.assertEqual(before,self.state(w,repo,bank))
                cursor.write_bytes(raw);receipt.write_bytes(receipt_raw)
                if mutation=='branch':w.git('symbolic-ref','HEAD','refs/heads/feature-session',cwd=repo)
