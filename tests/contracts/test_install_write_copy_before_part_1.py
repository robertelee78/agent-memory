"""Replay partition 1/4 of all observed install-write-copy-before interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class InstallCopyBeforePart1Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_1(self):
        self.replay('install', 'copy', 'before', partition=1)
