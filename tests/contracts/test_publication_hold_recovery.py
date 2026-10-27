"""Publication exposes typed known-hold failure without mutating feature state."""
import json
from world import World
from contracts import test_quarantined_sharing_v2 as quarantined
from publication_fixtures import PublicationFixture
from contracts import test_semantic_sharing as sem

class PublicationHoldRecoveryContract(PublicationFixture):
    private = quarantined.QuarantinedSharingVersionTwoContract.private
    inspection = quarantined.QuarantinedSharingVersionTwoContract.inspection
    versioned_review = quarantined.QuarantinedSharingVersionTwoContract.versioned_review
    def test_missing_private_hold_state_reports_specific_recovery_and_keeps_pending_work(self):
        self.marker='sk-'+('X'*24)
        with World() as w:
            repo,bank,peer,base=self.fixture(w); self.github(w,repo,bank,peer)
            helper=sem.SemanticSharingContract()
            helper.add(bank,'isolated-local','# Isolated evidence\n\n'+self.marker+'\n')
            (bank/'activeContext.md').write_text('# Current focus\n\nIndependent local learning.\n')
            (peer/'memory-bank/projectbrief.md').write_text('# Project\n\nIndependent peer learning.\n'); helper.publish(w,peer)
            reviewed=self.versioned_review(w,repo)
            prepared=self.good(w,'publish_memory.py','prepare','--v2','--repo',repo,'--input',self.private(w,'review-proof',reviewed['proof']))
            registry=next(w.paths['state'].rglob('semantic-holds/*/registry.json')); registry.unlink()
            before=self.state(w,repo,bank)
            command,value=self.call(w,'publish_memory.py','land','--v2','--repo',repo,'--input',self.private(w,'prepared',prepared['request']))
            self.assertEqual(command.returncode,1,value)
            self.assertEqual(value['failure_component'],'known-dispute-state')
            self.assertTrue(value['failure_code'].startswith('known-holds-'))
            self.assertIn('recover-holds',value['repair'])
            self.assertFalse(value['publication_verified'])
            self.assertEqual(before,self.state(w,repo,bank))
