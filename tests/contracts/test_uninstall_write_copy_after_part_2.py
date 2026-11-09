"""Replay partition 2/4 of all observed uninstall-write-copy-after interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class UninstallCopyAfterPart2Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_2(self):
        self.replay('uninstall', 'copy', 'after', partition=2)
