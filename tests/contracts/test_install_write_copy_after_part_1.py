"""Replay partitions 1 and 5 of all observed lifecycle interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWritePart1Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_1(self):
        self.replay('install', 'copy', 'after', partition=1)

    def test_actual_logical_writes_in_partition_5(self):
        self.replay('install', 'copy', 'after', partition=5)
