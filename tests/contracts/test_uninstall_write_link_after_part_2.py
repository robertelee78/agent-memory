"""Replay partition 2/4 of observed uninstall-write-link-after interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class UninstallLinkAfterPart2Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_2(self):
        self.replay('uninstall', 'link', 'after', partition=2)
