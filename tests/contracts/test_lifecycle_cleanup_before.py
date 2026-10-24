"""Interrupt before cleanup of an unpublished record after a real injected error."""
import unittest
from lifecycle_cleanup_fixtures import CleanupWriteCases


class CleanupBeforeContract(CleanupWriteCases, unittest.TestCase):
    def test_before_error_cleanup_write(self):
        self.replay_cleanup('before')
