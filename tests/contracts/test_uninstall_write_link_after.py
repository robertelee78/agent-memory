"""Replay partition 0/4 of observed uninstall-write-link-after interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWriteContract(LifecycleWriteCases, unittest.TestCase):
    def test_after_every_uninstall_link_write(self):
        self.replay('uninstall', 'link', 'after', partition=0)
