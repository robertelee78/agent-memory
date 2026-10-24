"""Interrupt before every observed uninstall logical write (copy)."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWriteContract(LifecycleWriteCases, unittest.TestCase):
    def test_before_every_uninstall_copy_write(self):
        self.replay('uninstall', 'copy', 'before')
