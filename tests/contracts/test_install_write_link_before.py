"""Replay partition 0/4 of all observed install-write-link-before interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWriteContract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_0(self):
        self.replay('install', 'link', 'before', partition=0)
