"""Replay partition 3/4 of observed uninstall-write-link-after interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class UninstallLinkAfterPart3Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_3(self):
        self.replay('uninstall', 'link', 'after', partition=3)
