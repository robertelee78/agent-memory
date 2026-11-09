"""Replay selected registered install/copy/before writes, or all directly."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class InstallCopyBeforeContract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_selected_logical_writes(self):
        self.replay('install', 'copy', 'before')
