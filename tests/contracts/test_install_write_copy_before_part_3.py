"""Replay partitions 3 and 7 of all observed lifecycle interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class InstallCopyBeforePart3Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_3(self):
        self.replay('install', 'copy', 'before', partition=3)

    def test_actual_logical_writes_in_partition_7(self):
        self.replay('install', 'copy', 'before', partition=7)
