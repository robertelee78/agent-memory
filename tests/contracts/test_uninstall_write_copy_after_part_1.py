"""Replay partition 1/4 of all observed uninstall-write-copy-after interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class UninstallCopyAfterPart1Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_1(self):
        self.replay('uninstall', 'copy', 'after', partition=1)
