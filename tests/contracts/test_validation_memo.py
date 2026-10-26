"""Exact-content reuse must preserve refusal, fresh authority and caller isolation."""
import json
from pathlib import Path
import sys
import unittest

from world import World
from publication_fixtures import PublicationFixture
from record_fixtures import record_text, project_id

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT/'skills/agent-memory/scripts'))
from agent_memory import bank_contract as bank, validation_memo as memo, security, recall_catalog
from agent_memory.frontmatter import ContractError


class ValidationMemoContract(PublicationFixture):
    def files(self, w):
        repo, directory, peer, base = self.fixture(w)
        return {p.relative_to(directory).as_posix():p.read_text() for p in directory.rglob('*.md')}

    def test_changed_child_bytes_paths_and_returned_values_do_not_reuse_old_truth(self):
        with World() as w:
            files = self.files(w)
            with memo.operation():
                first = bank.validate_files(files)
                first['entries'].clear()
                second = bank.validate_files(files)
                self.assertTrue(second['entries'])
                second['entries'][0]['scope_chain'].append('component:unobserved')
                self.assertNotIn('component:unobserved', bank.validate_files(files)['entries'][0]['scope_chain'])
                empty = {**files, 'projectbrief.md':''}
                with self.assertRaises(ContractError):bank.validate_files(empty)
                renamed = dict(files);renamed['different.md'] = renamed.pop('projectbrief.md')
                with self.assertRaises(ContractError):bank.validate_files(renamed)
                self.assertTrue(bank.validate_files(files)['valid'])
            self.assertIsNone(memo.current())

    def test_identical_file_authority_text_rechecks_changed_filesystem_topology(self):
        with World() as w:
            files = self.files(w);meta,routes,omitted = bank.parse_router(files['MEMORY.md'])
            target=w.inside('authority-location');other=w.inside('another-authority');other.mkdir()
            meta['canonical-remote']=target.as_uri();files['MEMORY.md']=bank.router_text(meta,routes,omitted)
            with memo.operation():
                self.assertTrue(bank.validate_files(files)['valid'])
                target.symlink_to(other,target_is_directory=True)
                with self.assertRaises(ContractError):bank.validate_files(files)

    def test_security_budget_redaction_results_and_failed_checks_remain_distinct(self):
        with memo.operation():
            large='Reviewed source prose. '*900
            self.assertFalse(security.scan(large,maximum=65536)['reasons'])
            with self.assertRaises(ContractError):security.scan(large)
            secret='token = fixturecredential123456'
            result=security.scan(secret);self.assertTrue(result['redaction_required'])
            result['reasons'].clear();result['redacted_text']='caller replacement'
            self.assertTrue(security.scan(secret)['reasons'])
            self.assertNotEqual(security.scan(secret)['redacted_text'],'caller replacement')
            with self.assertRaises(ContractError):security.scan('')
            self.assertFalse(security.scan('Useful independent source.')['reasons'])

    def test_parsed_record_output_is_produced_even_after_a_format_hit(self):
        with World() as w:
            files=self.files(w);meta,routes,omitted=bank.parse_router(files['MEMORY.md'])
            path='topics/native-record.md';files[path]=record_text('# Native record\n\nKeep distinct source evidence.\n',project_id(files['MEMORY.md']))
            routes.append({'path':path,'purpose':'Native record','scope':'repository','lifecycle':'active','read-when':'For source evidence'})
            files['MEMORY.md']=bank.router_text(meta,routes,omitted)
            with memo.operation():
                bank.validate_files(files)
                for _ in range(2):
                    parsed={};bank.validate_files(files,parsed_records=parsed)
                    self.assertIn(path,parsed);self.assertIn('Keep distinct source evidence.',parsed[path][2])
                    parsed.clear()

    def test_exceptions_nesting_and_budget_fallback_do_not_leak_validation_state(self):
        with World() as w:
            files=self.files(w);old=memo.MAX_BYTES
            try:
                with self.assertRaises(RuntimeError):
                    with memo.operation() as outer:
                        with memo.operation() as inner:self.assertIs(inner,outer)
                        memo.MAX_BYTES=0
                        self.assertTrue(bank.validate_files(files)['valid'])
                        with self.assertRaises(ContractError):bank.validate_files({**files,'projectbrief.md':''})
                        raise RuntimeError('Fixture exception unwinds operation')
                self.assertIsNone(memo.current())
            finally:memo.MAX_BYTES=old
            with memo.operation() as fresh:self.assertFalse(fresh.values)

    def test_catalog_generation_remains_available_outside_operation_and_refuses_active_memo(self):
        cold=recall_catalog.generation()
        self.assertIn('agent_memory.validation_memo',cold['modules'])
        with memo.operation():
            with self.assertRaises(recall_catalog.CacheMiss):recall_catalog.generation()
        self.assertEqual(cold,recall_catalog.generation())

    def test_public_land_rechecks_source_authority_index_and_policy_after_first_check(self):
        program='''import json,subprocess,sys
from pathlib import Path
sys.path.insert(0,sys.argv[1]);from agent_memory import memory_publication as publication
repo=Path(sys.argv[2]);request=Path(sys.argv[3]);action=sys.argv[4];policy=Path(sys.argv[5]);peer=Path(sys.argv[6])
original=publication.writes.fault
def fault(stage):
 if stage=='publication-before-final-review':
  if action=='source':
   p=repo/'memory-bank/activeContext.md';p.write_text(p.read_text()+'\\nLater local evidence stays preserved.\\n')
  elif action=='authority':
   for args in (['commit','--allow-empty','-m','Later authority observation'],['push','origin','HEAD:main']):subprocess.run(['git','-C',str(peer),*args],check=True,capture_output=True)
  elif action=='index':
   p=repo/'independent-feature.txt';p.write_text('Later staged feature work.\\n');subprocess.run(['git','-C',str(repo),'add',p.name],check=True,capture_output=True)
  else:
   value=json.loads(policy.read_text());value['push']=False;policy.write_text(json.dumps(value))
 return original(stage)
publication.writes.fault=fault
raise SystemExit(publication.main(['land','--repo',str(repo),'--input',str(request)]))
'''
        for action in ('source','authority','index','policy'):
            with self.subTest(action=action),World() as w:
                repo,directory,peer,base=self.fixture(w);policy=self.github(w,repo,directory,peer)
                (directory/'activeContext.md').write_text('# Current focus\n\nRetain the authored local learning.\n')
                request=self.prepared(w,repo)['request'];before=self.state(w,repo,directory)
                head=w.git('ls-remote','origin','refs/heads/main',cwd=repo).stdout
                output=w.run([sys.executable,'-I','-c',program,self.pack/'scripts',repo,self.save(w,request),action,policy,peer],check=False,timeout=120)
                self.assertEqual(output.returncode,1,output.stdout+output.stderr)
                self.assertFalse(json.loads(output.stdout)['publication_verified'])
                after=self.state(w,repo,directory)
                self.assertEqual(before[1],after[1]);self.assertEqual(before[3:],after[3:])
                if action!='index':self.assertEqual(before[2],after[2])
                else:self.assertNotEqual(before[2],after[2])
                if action=='source':self.assertIn('Later local evidence',(directory/'activeContext.md').read_text())
                else:self.assertEqual(before[0],after[0])
                actual=w.git('ls-remote','origin','refs/heads/main',cwd=repo).stdout
                if action=='authority':
                    self.assertNotEqual(actual,head);self.assertTrue(actual.startswith(w.git('rev-parse','HEAD',cwd=peer).stdout.strip()))
                else:self.assertEqual(actual,head)


if __name__=='__main__':unittest.main()
