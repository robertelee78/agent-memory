"""Replay partition 1/4 of all observed uninstall-write-link-before interruptions."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class UninstallLinkBeforePart1Contract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_logical_writes_in_partition_1(self):
        self.replay('uninstall', 'link', 'before', partition=1)
