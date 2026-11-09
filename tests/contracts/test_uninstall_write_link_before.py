"""Replay partition 0/4 of observed uninstall-write-link-before interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWriteContract(LifecycleWriteCases, unittest.TestCase):
    def test_before_every_uninstall_link_write(self):
        self.replay('uninstall', 'link', 'before', partition=0)
