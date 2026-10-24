"""Replay partition 2/4 of all observed install-write-link-before interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWritePart2Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_2(self):
        self.replay('install', 'link', 'before', partition=2)
