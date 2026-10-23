"""Actual lost-response, competing-head and offline publication workflows."""
import json
from pathlib import Path
import shutil
import sys
import unittest

from world import World
from publication_fixtures import PublicationFixture


class PublicationRecoveryContract(PublicationFixture):
    def test_lost_response_inventory_does_not_duplicate_or_reverse_a_later_correction(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w); self.github(w, repo, bank, peer)
            (bank/'activeContext.md').write_text('# Current focus\n\nA reviewed discovery independent of unfinished code.\n')
            before = self.state(w, repo, bank); request = self.prepared(w, repo)['request']
            path = self.save(w, request, 'lost-response-request.json')
            failed = w.run([sys.executable, '-I', self.pack/'scripts/publish_memory.py', 'land', '--repo', repo, '--input', path],
                           env={'AGENT_MEMORY_FAULT_STAGE': 'publication-after-push', 'AGENT_MEMORY_FAULT_MODE': 'exit'},
                           check=False, timeout=120)
            self.assertEqual(failed.returncode, 86)
            self.assertEqual(w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout.split()[0], request['candidate'])
            shutil.rmtree(w.env['AGENT_MEMORY_STATE_HOME'])
            first = self.land(w, repo, request)
            self.assertEqual(first['sharing_state'], 'already-landed'); self.assertTrue(first['publication_verified'])
            w.git('pull', '--ff-only', cwd=peer)
            (peer/'memory-bank/activeContext.md').write_text('# Current focus\n\nA later evidence-backed correction stays authoritative.\n')
            w.git('add', 'memory-bank/activeContext.md', cwd=peer); w.git('commit', '-m', 'Later actual correction', cwd=peer)
            w.git('push', 'origin', 'HEAD:main', cwd=peer)
            actual = w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout
            again = self.land(w, repo, request)
            self.assertTrue(again['publication_verified']); self.assertFalse(again['intended_delta_current'])
            self.assertEqual(again['witnessed_commit'], request['candidate'])
            self.assertEqual(actual, w.git('ls-remote', 'origin', 'refs/heads/main', cwd=repo).stdout)
            self.assertEqual(before, self.state(w, repo, bank))

    def test_competing_main_during_push_is_rejected_then_fresh_reconciliation_preserves_authority_code(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w); policy = self.github(w, repo, bank, peer)
            (bank/'activeContext.md').write_text('# Current focus\n\nIndependent local learning is ready.\n')
            (peer/'memory-bank/projectbrief.md').write_text('# Project\n\nCompatible independently landed project learning.\n')
            (peer/'code.py').write_text('VALUE = 900  # newly landed authority code\n')
            w.git('add', 'memory-bank/projectbrief.md', 'code.py', cwd=peer); w.git('commit', '-m', 'Competing actual authority', cwd=peer)
            request = self.prepared(w, repo)['request']; before = self.state(w, repo, bank)
            hook = repo/'.git/hooks/pre-push'; flag = w.inside('competing-main-ran')
            real_git = json.loads(policy.read_text())['real_git']
            hook.write_text('#!'+sys.executable+'\n'+
                'import os,subprocess,sys\nfrom pathlib import Path\n'+
                'flag=Path('+repr(str(flag))+')\n'+
                'for line in sys.stdin:\n'+
                ' if line.split()[2]=="refs/heads/main" and not flag.exists():\n'+
                '  flag.touch()\n'+
                '  env={k:v for k,v in os.environ.items() if not k.startswith("GIT_") or k in {"GIT_ALLOW_PROTOCOL","GIT_CONFIG_NOSYSTEM","GIT_CONFIG_SYSTEM","GIT_CONFIG_GLOBAL"}}\n'+
                '  subprocess.run('+repr([real_git,'-C',str(peer),'push','origin','HEAD:main'])+',env=env,check=True,stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)\n')
            hook.chmod(0o755)
            pending = self.land(w, repo, request)
            self.assertTrue(flag.exists()); self.assertFalse(pending['publication_verified'])
            self.assertEqual(pending['sharing_state'], 'pending-reconciliation'); self.assertEqual(before, self.state(w, repo, bank))
            fresh = self.prepared(w, repo)['request']; self.assertNotEqual(fresh['base'], request['base'])
            landed = self.land(w, repo, fresh); self.assertTrue(landed['publication_verified'])
            actual = landed['witnessed_commit']
            self.assertIn('VALUE = 900', w.git('show', actual+':code.py', cwd=repo).stdout)
            self.assertIn('Independent local learning', w.git('show', actual+':memory-bank/activeContext.md', cwd=repo).stdout)
            self.assertEqual(before, self.state(w, repo, bank))

