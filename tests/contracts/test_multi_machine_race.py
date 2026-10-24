"""Two actual installed publication clients reach the same remote push together."""
from concurrent.futures import ThreadPoolExecutor
import copy
import json
from pathlib import Path
import sys
import unittest

import installed_cli_fixtures as installed
from contracts.test_comparison import unpack
from multi_machine_fixtures import MultiMachineFixture
from world import World


class MultiMachineRace(MultiMachineFixture):
    def test_simultaneous_public_publishers_consolidate_duplicate_learning_before_retry_lands(self):
        with World() as w:
            a,b,c,sibling,dev_a,dev_b,_baseline = self.setup_actors(w)
            body = '# Retry cap\n\nRetry at most three times.\n'
            first,p1 = self.fact(a/'memory-bank','race-A',body,
                                 dev_a,'claude','race-A-session')
            second,p2 = self.fact(b/'memory-bank','race-B','# Attempt limit\n\nUse no more than three retries.\n',
                                  dev_b,'codex','race-B-session')
            before = {str(repo):self.protected(w,repo) for repo in (a,b,c,sibling)}
            candidate_a = self.publication(w,a)
            with self.actor(w,'profile-B'):
                candidate_b = self.publication(w,b)
                # Borrow the same disposable filesystem with an independent,
                # immutable actor environment; the outer World alone owns it.
                other = copy.copy(w); other.env = w.env.copy()
            self.assertEqual(candidate_a['base'],candidate_b['base'])
            inputs = [self.save(w,candidate_a,'race-A-request.json'),
                      self.save(w,candidate_b,'race-B-request.json')]
            barrier = w.inside('real push rendezvous'); barrier.mkdir()
            hooks = []
            for index,repo in enumerate((a,b)):
                hook = Path(w.git('rev-parse','--path-format=absolute','--git-path','hooks/pre-push',cwd=repo).stdout.strip())
                hook.write_text('#!'+sys.executable+'\n'+
                    'import json,sys,time\nfrom pathlib import Path\n'+
                    'root=Path('+repr(str(barrier))+')\n'+
                    'lines=[line.split() for line in sys.stdin]\n'+
                    '(root/'+repr(str(index)+'.ready')+').write_text(json.dumps(lines))\n'+
                    'deadline=time.monotonic()+20\n'+
                    'while len(list(root.glob("*.ready")))!=2:\n'+
                    ' if time.monotonic()>deadline:raise RuntimeError("Other actual publisher did not reach pre-push")\n'+
                    ' time.sleep(0.01)\n')
                hook.chmod(0o755); hooks.append(hook)
            def publish(actor,repo,path):
                return installed.invoke(actor,self.cli,'publish_memory.py','land','--repo',repo,'--input',path)
            with ThreadPoolExecutor(max_workers=2) as executor:
                futures = [executor.submit(publish,actor,repo,path)
                           for actor,repo,path in ((w,a,inputs[0]),(other,b,inputs[1]))]
                results = [future.result() for future in futures]
            for hook in hooks:hook.unlink()
            for index,candidate in enumerate((candidate_a,candidate_b)):
                observed = json.loads((barrier/(str(index)+'.ready')).read_text())
                self.assertEqual(len(observed),1)
                self.assertEqual(observed[0][1],candidate['candidate'])
                self.assertEqual(observed[0][2:],[ 'refs/heads/main', candidate['base'] ])
            for process,receipt in results:self.assertEqual(process.returncode,0,(receipt,process.stderr))
            for index,(_,receipt) in enumerate(results):
                self.event('simultaneous-initial-result',actor='A' if index==0 else 'B',
                           publication_verified=receipt['publication_verified'],sharing_state=receipt['sharing_state'],
                           witnessed_commit=receipt.get('witnessed_commit'))
            winners = [i for i,(_,receipt) in enumerate(results) if receipt['publication_verified']]
            self.assertEqual(len(winners),1,results)
            loser = 1-winners[0]; pending = results[loser][1]
            self.assertEqual(pending['sharing_state'],'pending-reconciliation')
            loser_world,loser_repo = (w,a) if loser==0 else (other,b)
            def equivalent(left,right):
                return 'equivalent' if {left['path'],right['path']}=={p1,p2} else 'related'
            def consolidate(view):
                sources = [row for row in view['sources'] if row['path'] in {p1,p2}]
                self.assertEqual(len(sources),2)
                primary = next(row for row in sources if row['path']==p1)
                return [{'sources':[row['key'] for row in sources],'primary':primary['key'],
                         'body':body,'reason':'Both complete source claims establish the same cap under identical conditions.'}]
            fresh = self.publication(loser_world,loser_repo,equivalent,consolidate=consolidate)
            self.assertNotEqual(fresh['base'],candidate_a['base'])
            landed = self.land(loser_world,loser_repo,fresh)
            self.assertTrue(landed['publication_verified'])
            actual = landed['witnessed_commit']
            self.assertEqual(w.git('ls-remote','origin','refs/heads/main',cwd=loser_repo).stdout.split()[0],actual)
            paths = w.git('ls-tree','-r','--name-only',actual,'--','memory-bank/topics',cwd=loser_repo).stdout.splitlines()
            self.assertEqual(set(paths),{'memory-bank/'+p1,'memory-bank/'+p2})
            current,_ = unpack(w.git('show',actual+':memory-bank/'+p1,cwd=loser_repo).stdout)
            superseded,_ = unpack(w.git('show',actual+':memory-bank/'+p2,cwd=loser_repo).stdout)
            self.assertEqual(current['id'],first['id']);self.assertEqual(current['version'],2)
            self.assertEqual(current['status'],'active');self.assertEqual(current['origin'],first['origin'])
            self.assertIn({'id':second['id'],'version':1},current['derived-from'])
            self.assertEqual(superseded['status'],'superseded');self.assertEqual(superseded['origin'],second['origin'])
            original,_ = unpack(w.git('show',actual+':memory-bank/history/versions/'+first['id']+'/v1.md',cwd=loser_repo).stdout)
            self.assertEqual(original['origin'],first['origin'])
            self.assertEqual(w.git('show',actual+':code.py',cwd=loser_repo).stdout,'VALUE = 1\n')
            ancestors = set(w.git('rev-list',actual,cwd=loser_repo).stdout.splitlines())
            for repo in (a,b,c,sibling):self.assertNotIn(before[str(repo)][0][2].strip(),ancestors)
            for repo in (a,b,c,sibling):self.assertEqual(self.protected(w,repo),before[str(repo)])
            self.event('simultaneous-publication',actual_publishers=2,pre_push_rendezvous=True,
                       shared_base=candidate_a['base'],successful_initial_publications=1,
                       fresh_reconciliation_verified=True,active_lessons=1,retained_source_origins=2,
                       deduplicated_before_retry_lands=True,actual_authority=actual)


if __name__=='__main__':unittest.main()
