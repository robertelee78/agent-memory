"""Replay partition 3/4 of observed uninstall-write-copy-before interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class UninstallCopyBeforePart3Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_3(self):
        self.replay('uninstall', 'copy', 'before', partition=3)
