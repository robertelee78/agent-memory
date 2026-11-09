"""Replay partition 0/4 of all observed uninstall-write-link-before interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class UninstallLinkBeforePart0Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_0(self):
        self.replay('uninstall', 'link', 'before', partition=0)
