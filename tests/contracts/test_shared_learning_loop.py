"""Public capture/publish/refresh across isolated actor profiles and stale clones.

Profiles model independent machine registry/state roots; harness labels are
fixture attribution, and no live client or credential service is involved.
"""
from contextlib import contextmanager
import hashlib
from pathlib import Path
import unittest

from world import World
from identity_fixtures import bind
from refresh_fixtures import RefreshFixture
from contracts import test_admission as admission
from contracts.test_retrieval import RetrievalContract


class SharedLearningLoopContract(RefreshFixture):
    @contextmanager
    def actor_profile(self, w, name):
        original = w.env.copy(); root = w.inside(name)
        config, state, backup = root/'config', root/'state', root/'backup'
        for path in (config, state, backup): path.mkdir(parents=True, exist_ok=True, mode=0o700)
        w.env.update(AGENT_MEMORY_CONFIG_HOME=str(config), AGENT_MEMORY_STATE_HOME=str(state), AGENT_MEMORY_BACKUP_HOME=str(backup))
        try:
            yield
        finally:
            w.env.clear(); w.env.update(original)

    def test_automatic_durable_capture_lands_and_another_actor_recalls_it_without_feature_code(self):
        with World() as w:
            repo, bank, seed, base = self.fixture(w); policy = self.github(w, repo, bank, seed)
            import json
            remote = Path(json.loads(policy.read_text())['remote'])
            receiver = w.clone(remote, 'already-running other harness'); receiver_bank = receiver/'memory-bank'
            w.git('remote','set-url','origin','https://github.com/fixture/memory.git',cwd=receiver)
            w.git('checkout','-b','receiver-feature',cwd=receiver)
            (receiver/'code.py').write_text('VALUE = 456  # receiver staged work\n'); w.git('add','code.py',cwd=receiver)
            with self.actor_profile(w, 'other-machine'):
                actor_b = bind(w, receiver)['developer_id']
            actor_a = self.good(w,'scope_binding.py','status','--repo',repo)['developer_id']
            self.assertNotEqual(actor_a,actor_b)
            # Refresh known shared sources before admitting the turn's new learning.
            self.apply_refresh(w,self.prepared_refresh(w,repo))
            teacher = admission.AdmissionContract(); teacher.pack = self.pack
            scope = self.good(w,'scope_binding.py','status','--repo',repo)
            teacher.pid = scope['project_id']; teacher.actor = scope['developer_id']
            evidence = repo/'evidence.md'
            evidence.write_text('# Confirmed fixture operator correction\n\nFor the named component use the recorded retry cap.\n')
            capture = teacher.request()
            capture['repository_evidence'] = {'path':'evidence.md','sha256':hashlib.sha256(evidence.read_bytes()).hexdigest()}
            capture['candidate']['metadata']['conditions'] = {'component':'code.py'}
            capture['candidate']['metadata']['origin'].update({'harness':'claude-fixture','session':'teacher-feature-session'})
            proposal = teacher.prepared(w,repo,capture); p, admitted = teacher.apply(w,proposal)
            self.assertEqual(p.returncode,0,admitted); self.assertTrue(admitted['admission_verified'])
            before_teacher = self.state(w,repo,bank); feature_head = w.git('rev-parse','HEAD',cwd=repo).stdout.strip()
            publication = self.prepared(w,repo)['request']; landed = self.land(w,repo,publication)
            self.assertTrue(landed['publication_verified']); self.assertEqual(before_teacher,self.state(w,repo,bank))
            self.assertNotIn(feature_head,w.git('rev-list',publication['candidate'],cwd=repo).stdout.splitlines())
            self.assertEqual(w.git('show',publication['candidate']+':code.py',cwd=repo).stdout,'VALUE = 1\n')
            identity = capture['candidate']['metadata']['id']; path = 'topics/'+identity+'.md'
            self.assertFalse((receiver_bank/path).exists()); before_receiver = self.state(w,receiver,receiver_bank)
            with self.actor_profile(w,'other-machine'):
                status = self.good(w,'scope_binding.py','status','--repo',receiver)
                self.assertEqual(status['developer_id'],actor_b); self.assertEqual(status['project_id'],teacher.pid)
                recall = RetrievalContract().request(teacher.pid,ids=[identity],query='',context={'component':'code.py'})
                received = self.apply_refresh(w,self.prepared_refresh(w,receiver,recall=recall))
            self.assertTrue(received['remote_freshness_verified']); self.assertFalse(received['feature_code_freshness_verified'])
            self.assertEqual(received['recall']['scope_context']['developer_id'],actor_b)
            record = received['recall']['results'][0]['record']
            self.assertEqual(record['origin']['developer-id'],actor_a); self.assertEqual(record['origin']['harness'],'claude-fixture')
            self.assertEqual(record['conditions'],{'component':'code.py'})
            self.assertEqual((bank/path).read_bytes(),(receiver_bank/path).read_bytes())
            self.assertEqual(before_receiver[1:],self.state(w,receiver,receiver_bank)[1:])
            self.assertEqual(before_teacher,self.state(w,repo,bank))
