"""Replay partitions 0 and 4 of all observed lifecycle interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class InstallCopyBeforeContract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_0(self):
        self.replay('install', 'copy', 'before', partition=0)

    def test_actual_logical_writes_in_partition_4(self):
        self.replay('install', 'copy', 'before', partition=4)
