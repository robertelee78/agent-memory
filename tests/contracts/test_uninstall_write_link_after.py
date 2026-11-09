"""Replay partition 0/4 of all observed uninstall-write-link-after interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class UninstallLinkAfterPart0Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_0(self):
        self.replay('uninstall', 'link', 'after', partition=0)
