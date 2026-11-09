"""Replay partitions 2 and 6 of all observed lifecycle interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWritePart2Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_2(self):
        self.replay('install', 'link', 'after', partition=2)

    def test_actual_logical_writes_in_partition_6(self):
        self.replay('install', 'link', 'after', partition=6)
