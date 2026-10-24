"""Replay partition 3/4 of all observed install-write-copy-before interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class InstallCopyBeforePart3Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_3(self):
        self.replay('install', 'copy', 'before', partition=3)
