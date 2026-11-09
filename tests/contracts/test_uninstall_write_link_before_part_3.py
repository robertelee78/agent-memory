"""Replay partition 3/4 of observed uninstall-write-link-before interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class UninstallLinkBeforePart3Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_3(self):
        self.replay('uninstall', 'link', 'before', partition=3)
