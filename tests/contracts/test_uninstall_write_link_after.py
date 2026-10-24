"""Interrupt after every observed uninstall logical write (link)."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWriteContract(LifecycleWriteCases, unittest.TestCase):
    def test_after_every_uninstall_link_write(self):
        self.replay('uninstall', 'link', 'after')
