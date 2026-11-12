"""Replay selected registered install/link/before writes, or all directly."""
import unittest
from lifecycle_fault_fixtures import LifecycleWriteCases


class LifecycleWriteContract(LifecycleWriteCases, unittest.TestCase):
    def test_actual_selected_logical_writes(self):
        self.replay('install', 'link', 'before')
