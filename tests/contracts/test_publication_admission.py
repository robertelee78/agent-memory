"""Governed real capture/admission publishes unmerged evidence without widening it."""
import hashlib
import json
import unittest

from world import World
from publication_fixtures import PublicationFixture
from contracts import test_admission as admission
from contracts.test_comparison import unpack


class PublicationAdmissionContract(PublicationFixture):
    def test_admitted_unmerged_discovery_keeps_exact_conditions_evidence_origin_and_history(self):
        with World() as w:
            repo, bank, peer, base = self.fixture(w); self.github(w, repo, bank, peer)
            w.git('checkout', '-b', 'unfinished-experiment', cwd=repo)
            w.git('commit', '-m', 'Unfinished feature ancestor', cwd=repo)
            (repo/'code.py').write_text('VALUE = 3  # additional staged unfinished code\n'); w.git('add', 'code.py', cwd=repo)
            evidence = repo/'evidence.md'
            evidence.write_text('# Isolated observed experiment\n\nUnder the named component conditions the measured retry cap is three.\n')
            helper = admission.AdmissionContract(); helper.pack = self.pack
            scope = self.good(w, 'scope_binding.py', 'status', '--repo', repo)
            helper.pid = scope['project_id']; helper.actor = scope['developer_id']
            request = helper.request('verified-repository', 'durable-discovery')
            request['candidate']['metadata']['conditions'] = {'component':'code.py','branch':'unfinished-experiment'}
            request['candidate']['metadata']['origin'].update({'code-state':'unmerged','harness':'custom-fixture','session':'unfinished-experiment'})
            request['repository_evidence'] = {'path':'evidence.md','sha256':hashlib.sha256(evidence.read_bytes()).hexdigest()}
            request['assessment']['review'] = 'Reviewed the actual fixture experiment and its component applicability; feature code remains unmerged.'
            proposal = helper.prepared(w, repo, request); p, receipt = helper.apply(w, proposal)
            self.assertEqual(p.returncode, 0, receipt); self.assertTrue(receipt['admission_verified'])
            before = self.state(w, repo, bank); feature = w.git('rev-parse', 'HEAD', cwd=repo).stdout.strip()
            prepared = self.prepared(w, repo)['request']; landed = self.land(w, repo, prepared)
            self.assertTrue(landed['publication_verified']); self.assertFalse(landed['feature_code_freshness_verified'])
            for path, content in before[0].items():
                actual = w.git('show', prepared['candidate']+':memory-bank/'+path, cwd=repo).stdout.encode()
                if path == 'MEMORY.md':
                    # Structured merges may normalize header/route ordering.
                    # The typed lesson and history below must remain verbatim.
                    self.assertEqual(unpack(actual.decode())[0], unpack(content.decode())[0])
                else:
                    self.assertEqual(actual, content)
            self.assertIn('unmerged', (bank/('topics/'+request['candidate']['metadata']['id']+'.md')).read_text())
            self.assertNotIn(feature, w.git('rev-list', prepared['candidate'], cwd=repo).stdout.splitlines())
            self.assertEqual(w.git('show', prepared['candidate']+':code.py', cwd=repo).stdout, 'VALUE = 1\n')
            self.assertEqual(before, self.state(w, repo, bank))
