"""Interrupt before every observed explicit purge logical write."""
import unittest
from lifecycle_fault_fixtures import PurgeWriteCases


class PurgeWriteContract(PurgeWriteCases, unittest.TestCase):
    def test_before_every_purge_write(self):
        self.replay_purge('before')
