"""Replay partition 0/4 of all observed uninstall-write-copy-before interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class UninstallCopyBeforePart0Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_0(self):
        self.replay('uninstall', 'copy', 'before', partition=0)
