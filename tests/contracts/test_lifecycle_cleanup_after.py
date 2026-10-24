"""Interrupt after cleanup of an unpublished record after a real injected error."""
import unittest
from lifecycle_cleanup_fixtures import CleanupWriteCases


class CleanupAfterContract(CleanupWriteCases, unittest.TestCase):
    def test_after_error_cleanup_write(self):
        self.replay_cleanup('after')
