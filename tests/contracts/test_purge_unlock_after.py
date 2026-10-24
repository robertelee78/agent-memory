"""Interrupted post-unlock refusal must retain useful memory without purge consent."""
import unittest
from lifecycle_unlock_fixtures import PurgeUnlockCases


class PurgeUnlockAfterContract(PurgeUnlockCases, unittest.TestCase):
    def test_after_refused_purge_unlock(self):
        self.replay_unlock('after')
