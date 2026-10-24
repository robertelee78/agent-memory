"""Interrupt after every observed recover-commit logical write (copy)."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWriteContract(LifecycleWriteCases, unittest.TestCase):
    def test_after_every_recover_commit_copy_write(self):
        self.replay('recover-commit', 'copy', 'after')
